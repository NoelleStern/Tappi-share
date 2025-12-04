use std::{
    collections::HashMap,
    sync::{Arc, atomic},
};
use tokio::sync::{Mutex, mpsc::UnboundedSender};
use warp::filters::ws::Message;

// User definitions
pub type UserId = usize;
pub static NEXT_USERID: atomic::AtomicUsize = atomic::AtomicUsize::new(1);
pub fn get_new_user_id() -> usize {
    NEXT_USERID.fetch_add(1, atomic::Ordering::Relaxed) // Get and increment
}
#[derive(Clone, Debug)]
pub struct RoomUser {
    pub id: usize,
    pub name: String,
    pub room_id: RoomId,
    pub tx: UnboundedSender<Message>,
}
impl RoomUser {
    pub fn new(name: String, room_id: RoomId, tx: UnboundedSender<Message>) -> Self {
        Self {
            id: get_new_user_id(),
            name,
            room_id,
            tx,
        }
    }

    pub fn name_with_id(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}

// Room definitions
pub type RoomId = String;
pub type RoomUsers = Arc<Mutex<HashMap<UserId, Arc<RoomUser>>>>;
pub struct Room {
    pub id: RoomId,
    pub users: RoomUsers,
    pub history: History,
    pub capacity: usize,
}
impl Room {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            users: RoomUsers::default(),
            history: History::default(),
            capacity: 2,
        }
    }
}
pub type Rooms = Arc<Mutex<HashMap<RoomId, Arc<Room>>>>;

/// Message history
pub type History = Arc<Mutex<Vec<UserMessage>>>;
#[derive(Debug, Clone)]
pub struct UserMessage {
    pub user_id: UserId,
    pub room_id: RoomId,
    pub msg: String,
}
impl UserMessage {
    pub fn new(room_id: RoomId, user_id: UserId, msg: String) -> Self {
        Self {
            user_id,
            room_id,
            msg,
        }
    }
}
