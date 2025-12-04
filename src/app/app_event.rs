use std::{
    fmt::{self, Debug},
    sync::Arc,
};
use webrtc::data_channel::RTCDataChannel;

use crate::{
    app::{
        event::BasicEvent,
        file_manager::{FileProgressReport, InputFile, SpeedReport},
    },
    client::{message::Message, rtc_base::WebConnection, signaling::negotiator::HandshakeState},
    server::types::{RoomId, RoomUser, UserMessage},
};

/// Application events.
///
/// You can extend this enum with your own custom events.
#[derive(Clone, Debug)]
pub enum AppEvent {
    None,
    FocusNext,
    FocusPrev,
    Client(AppEventClient),
    Server(AppEventServer),
}
impl From<AppEvent> for BasicEvent {
    fn from(ev: AppEvent) -> Self {
        Self::App(ev)
    }
}

/// Holds possible client app events
#[derive(Clone, Debug)]
pub enum AppEventClient {
    /// Quit the application.
    Quit,
    /// Connection was initialized
    InitConnection(WebConnection),
    /// A WebRTC channel was open
    ChannelOpened(DebugDataChannel),
    /// Connection was established
    Connected,
    /// Connection has broken event
    Disconnected,
    /// Updates server signaling UI
    UpdateHandshakeState(HandshakeState),
    /// Manual signaling initialization event
    ManualSignalingInit(bool),
    /// Manual signaling input event
    ManualSignalingInput(String),
    /// Manual signaling output event
    ManualSignalingOutput(String),
    /// A message got sent back
    MessageReceived(Message),
    /// Output file progress update
    OutputFileProgress(FileProgressReport),
    /// Report incoming file speed (outgoing reports come via webrtc channel)
    ReportFileSpeed(SpeedReport),
    /// Input file progress update
    InputFileProgress(FileProgressReport),
    /// Output file was successfully sent
    OutputFileFinished(DebugDataChannel),
    /// New incoming file was added
    InputFileNew(InputFile),
    /// Metadata was successfully sent
    MetaSent(DebugDataChannel),
}
impl From<AppEventClient> for AppEvent {
    fn from(ev: AppEventClient) -> Self {
        Self::Client(ev)
    }
}
impl From<AppEventClient> for BasicEvent {
    fn from(ev: AppEventClient) -> Self {
        Self::App(AppEvent::from(ev))
    }
}

/// Holds possible server app events
#[derive(Clone, Debug)]
pub enum AppEventServer {
    /// Quit the application.
    Quit,
    AddRoom(RoomId),
    RemoveRoom(RoomId),
    AddRoomUser(RoomUser),
    RemoveRoomUser(RoomUser),
    AddMessage(UserMessage),
}
impl From<AppEventServer> for AppEvent {
    fn from(ev: AppEventServer) -> Self {
        Self::Server(ev)
    }
}
impl From<AppEventServer> for BasicEvent {
    fn from(ev: AppEventServer) -> Self {
        Self::App(AppEvent::from(ev))
    }
}

#[derive(Clone)]
pub struct DebugDataChannel {
    pub dc: Arc<RTCDataChannel>,
}
impl DebugDataChannel {
    pub fn new(dc: Arc<RTCDataChannel>) -> Self {
        Self { dc }
    }
}
impl fmt::Debug for DebugDataChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let channel = &self.dc;

        let mut debug_struct = f.debug_struct("RTCDataChannel");

        debug_struct
            .field("id", &channel.id())
            .field("label", &channel.label())
            .field("protocol", &channel.protocol())
            .field("ordered", &channel.ordered())
            .field("max_retransmits", &channel.max_retransmits())
            .field("max_packet_lifetime", &channel.max_packet_lifetime())
            .field("ready_state", &channel.ready_state());

        debug_struct.finish()
    }
}
