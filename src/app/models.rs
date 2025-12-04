use indexmap::IndexMap;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::app::event::BasicEvent;
use crate::client::rtc_base::WebConnection;
use crate::client::signaling::signaling_solution::SignalingMessage;
use crate::server::types::{RoomUser, UserId, UserMessage};

/// Client-related data structure
///
/// Gets modified by app events
#[derive(Default)]
pub struct ClientState {
    pub wc: Option<WebConnection>,
    pub connected: bool,
    pub handshake_tx: Option<UnboundedSender<SignalingMessage>>,
}

// I probably should rename it, but it's too cute and i love it
pub struct Maid {
    pub error_tx: ErrorTX,
    pub event_tx: UnboundedSender<BasicEvent>,
    pub token: CancellationToken,
}
impl Maid {
    pub fn new(
        error_tx: ErrorTX,
        event_tx: UnboundedSender<BasicEvent>,
        token: CancellationToken,
    ) -> Self {
        Self {
            error_tx,
            event_tx,
            token,
        }
    }
}
impl Clone for Maid {
    fn clone(&self) -> Self {
        Self {
            error_tx: self.error_tx.clone(),
            event_tx: self.event_tx.clone(),
            token: self.token.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ErrorTX(pub UnboundedSender<color_eyre::Report>);
impl ErrorTX {
    pub fn send_error(&self, error: color_eyre::Report) {
        self.0.send(error).ok();
    }
}

#[derive(Debug, Default)]
pub struct SyncRoom {
    pub users: IndexMap<UserId, RoomUser>,
    pub history: Vec<UserMessage>,
}
