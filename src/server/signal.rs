use futures::{SinkExt, StreamExt, stream::SplitSink};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use warp::Filter;
use warp::filters::ws;
use warp::filters::ws::{Message, WebSocket};

use crate::app::app_event::AppEventServer;
use crate::app::event::{BasicEvent, BasicEventSenderExt};
use crate::app::models::Maid;
use crate::cli::ServerArgs;
use crate::server::types::{History, Room, RoomId, RoomUser, Rooms, UserId, UserMessage};

// Custom rejection for forbidden access
#[derive(Debug)]
struct Forbidden;
impl warp::reject::Reject for Forbidden {}

pub async fn main(maid: Maid, args: ServerArgs) -> color_eyre::Result<()> {
    let rooms: Rooms = Rooms::default();

    let maid = warp::any().map(move || maid.clone());
    let rooms = warp::any().map(move || rooms.clone());

    let room_route = warp::path("room".to_string())
        .and(warp::ws())
        .and(warp::query::<HashMap<String, String>>())
        .and(maid)
        .and(rooms)
        .and_then(
            |ws: ws::Ws, query: HashMap<String, String>, maid: Maid, rooms: Rooms| async move {
                if let Some(room_id) = query.get("room") {
                    let room_id: String = room_id.clone();
                    let reply = ws.on_upgrade(move |socket| {
                        connect(socket, maid.clone(), rooms.clone(), room_id.clone())
                    });

                    Ok(reply)
                } else {
                    Err(warp::reject::custom(Forbidden))
                }
            },
        );

    warp::serve(room_route).run(args.address).await;
    log::info!("Server started at ws://{}/room", args.address);

    Ok(())
}

#[allow(unused_assignments)]
async fn connect(ws: WebSocket, maid: Maid, rooms: Rooms, room_id: RoomId) {
    // Bookkeeping
    let mut user: Option<Arc<RoomUser>> = None;

    // Establishing a connection; tx is outgoing and rx is incoming from the server
    // user_tx sends to user, user_rx receives from user, tx sends to server, rx receives from server
    let (mut user_tx, mut user_rx) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>(); // Multi-tx, single-rx
    {
        // Try joining room
        user = join_room(maid.event_tx.clone(), rooms.clone(), &room_id, tx).await;
        if let Some(user) = user.clone() {
            // Send all of the chat history directly to the new user
            {
                let room_lock = rooms.lock().await;
                let room = room_lock.get(&room_id);

                if let Some(room) = room {
                    send_history(room.history.clone(), &mut user_tx).await;
                }
            }

            // Redirect-messages-to-the-new-user task
            let token = maid.token.child_token();
            tokio::spawn(async move {
                let token = token.clone();
                loop {
                    tokio::select! {
                        _ = token.cancelled() => {},
                        msg = rx.recv() => {  // When server receives a message
                            if let Some(msg) = msg {
                                if user_tx.send(msg.clone()).await.is_err() { // Try sending the message to a user
                                    break; // End the task on disconnect and other unexpected stuff
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            });

            // Report back user
            maid.event_tx
                .send_event(AppEventServer::AddRoomUser((*user).clone()))
                .await; // Should be fine
        }
    }

    if let Some(user) = user {
        // Reading and broadcasting the messages
        while let Some(result) = user_rx.next().await {
            // When we receive a message from user
            if let Ok(result) = result {
                broadcast_msg(maid.event_tx.clone(), rooms.clone(), user.clone(), result).await; // Redirect it to server
            }
        }

        // Handle disconnect
        disconnect(maid.event_tx.clone(), rooms.clone(), user.clone()).await;
    }
}

async fn get_room(rooms: Rooms, room_id: &RoomId) -> Option<Arc<Room>> {
    rooms.lock().await.get(room_id).cloned()
}

async fn join_room(
    sender: UnboundedSender<BasicEvent>,
    rooms: Rooms,
    room_id: &RoomId,
    tx: UnboundedSender<Message>,
) -> Option<Arc<RoomUser>> {
    let mut result: Option<Arc<RoomUser>> = None;
    let mut create_flag = false;

    let mut room_lock = rooms.lock().await;
    let room = room_lock.entry(room_id.clone()).or_insert_with(|| {
        create_flag = true;
        Arc::new(Room::new(room_id))
    });

    let mut users_lock = room.users.lock().await;
    if users_lock.len() < room.capacity {
        let user = Arc::new(RoomUser::new(
            petname::petname(2, "-")?,
            room_id.clone(),
            tx,
        ));

        users_lock.insert(user.id, user.clone());
        result = Some(user);
    }

    // Report back room
    if create_flag {
        sender
            .send_event(AppEventServer::AddRoom(room_id.clone()))
            .await; // Should be fine
    }

    result
}

async fn send_history(history: History, user_tx: &mut SplitSink<WebSocket, Message>) {
    // Bypasses the redirect and therefore getting in the history
    let history_guard = history.lock().await;
    for user_msg in history_guard.iter() {
        if user_tx
            .send(Message::text(user_msg.msg.clone()))
            .await
            .is_err()
        {
            return;
        }
    }
}

async fn append_to_history(
    sender: UnboundedSender<BasicEvent>,
    room_id: &RoomId,
    user_id: &UserId,
    msg: Message,
    history: History,
) {
    if msg.is_text()
        && let Ok(msg_text) = msg.to_str()
    {
        let msg_text = msg_text.to_string();
        let user_msg = UserMessage::new(room_id.clone(), *user_id, msg_text);

        // Append to history RAII
        {
            let mut history_guard = history.lock().await;
            history_guard.push(user_msg.clone());
        }

        // Report the message back
        sender
            .send_event(AppEventServer::AddMessage(user_msg))
            .await; // Should be fine
    }
}

// If my_id is provided, doesn't send there
async fn broadcast_msg(
    sender: UnboundedSender<BasicEvent>,
    rooms: Rooms,
    user: Arc<RoomUser>,
    msg: Message,
) {
    if msg.is_text() {
        // Send to all of the other users
        let room = get_room(rooms, &user.room_id).await;
        if let Some(room) = room {
            for (uid, ru) in room.users.lock().await.iter() {
                if user.id != *uid {
                    let tx = &ru.tx;
                    tx.send(msg.clone()).ok(); // TODO: review it
                }
            }

            // Append text message to the history
            append_to_history(
                sender.clone(),
                &room.id,
                &user.id,
                msg.clone(),
                room.history.clone(),
            )
            .await;
        }
    }
}

// Remove user as well as room if empty
async fn disconnect(sender: UnboundedSender<BasicEvent>, rooms: Rooms, user: Arc<RoomUser>) {
    // println!("Bye-bye user {my_id}");
    let room = get_room(rooms.clone(), &user.room_id).await;
    if let Some(room) = room {
        room.users.lock().await.remove(&user.id);

        //Report back user change
        sender
            .send_event(AppEventServer::RemoveRoomUser((*user).clone()))
            .await; // Should be fine

        if room.users.lock().await.is_empty() {
            rooms.lock().await.remove(&room.id);

            // Report back room change
            sender
                .send_event(AppEventServer::RemoveRoom(room.id.clone()))
                .await; // Should be fine
        }
    }
}
