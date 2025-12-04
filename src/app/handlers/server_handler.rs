use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{
        app_event::{AppEvent, AppEventServer},
        app_main::App,
        handlers::app_handler::AppHandler,
        models::SyncRoom,
    },
    server::types::{RoomUser, UserMessage},
};

/// Struct for handling server app events
pub struct ServerHandler;
impl AppHandler for ServerHandler {
    fn handle_key_events(key_event: &KeyEvent) -> color_eyre::Result<AppEvent> {
        let mut result: AppEvent = AppEvent::None;

        if key_event.is_release() {
            result = match key_event.code {
                KeyCode::Char('q') => AppEventServer::Quit.into(),
                _ => AppEvent::None,
            }
        }

        Ok(result)
    }

    fn handle_app_events(app: &mut App, event: AppEvent) -> color_eyre::Result<()> {
        if let AppEvent::Server(app_event) = event {
            match app_event {
                AppEventServer::Quit => on_quit(app),
                AppEventServer::AddRoom(room_id) => on_add_room(app, room_id),
                AppEventServer::RemoveRoom(room_id) => on_remove_room(app, room_id),
                AppEventServer::AddRoomUser(user) => on_add_room_user(app, user),
                AppEventServer::RemoveRoomUser(user) => on_remove_room_user(app, user),
                AppEventServer::AddMessage(user_msg) => on_add_message(app, user_msg),
            }
        }

        Ok(())
    }
}

fn on_quit(app: &mut App) {
    app.exit = true;
}
fn on_add_room(app: &mut App, room_id: String) {
    app.room_list_widget_state
        .rooms
        .insert(room_id, SyncRoom::default());
}
fn on_remove_room(app: &mut App, room_id: String) {
    app.room_list_widget_state.rooms.shift_remove(&room_id);
}
fn on_add_room_user(app: &mut App, user: RoomUser) {
    let room = app.room_list_widget_state.rooms.get_mut(&user.room_id);
    if let Some(room) = room {
        room.users.insert(user.id, user);
    }
}
fn on_remove_room_user(app: &mut App, user: RoomUser) {
    let room = app.room_list_widget_state.rooms.get_mut(&user.room_id);
    if let Some(room) = room {
        room.users.shift_remove(&user.id);
    }
}
fn on_add_message(app: &mut App, user_msg: UserMessage) {
    let room = app.room_list_widget_state.rooms.get_mut(&user_msg.room_id);
    if let Some(room) = room {
        room.history.push(user_msg);
    }
}
