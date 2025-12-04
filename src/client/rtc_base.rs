use color_eyre::eyre::eyre;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{Mutex, watch};
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::app::app_event::{AppEventClient, DebugDataChannel};
use crate::app::event::BasicEvent;
use crate::app::event::BasicEventSenderExt;
use crate::app::file_manager::MetaData;
use crate::app::models::{ErrorTX, Maid};
use crate::cli::ClientArgs;
use crate::client::message::handle_message;

/// File output KiB threshold
// I'm fighting the urge to make it 640K
const THRESHOLD: usize = 128 * 1024; // 128KB sounds reasonable enough

/// Manages WebRTC and signaling
#[derive(Clone, Debug)]
pub struct WebConnection {
    pub pc: Arc<RTCPeerConnection>,
    pub buffer_watch_tx: watch::Sender<bool>,
}
impl WebConnection {
    pub async fn init(maid: Maid, args: ClientArgs) -> color_eyre::Result<()> {
        let wc = WebConnection::new(maid.clone(), &args).await?;
        maid.event_tx
            .send_event(AppEventClient::InitConnection(wc))
            .await;
        Ok(())
    }

    pub async fn new(maid: Maid, args: &ClientArgs) -> color_eyre::Result<Self> {
        let config = Self::conf(
            args.username.clone(),
            args.credential.clone(),
            &mut args.additional_servers.clone(),
        );

        let dc_init = RTCDataChannelInit {
            negotiated: Some(0),
            ordered: Some(true),
            ..Default::default()
        };

        // Basic peer connection setup
        let api = APIBuilder::new().build(); // Create the API object
        let pc = api.new_peer_connection(config).await?;
        let pc = Arc::new(pc);

        // Create a data and message channel, ordered by default
        // Let's use pre-negotiated channels since the clients are simplistic and completely symmetrical
        let dc = pc.create_data_channel("data", Some(dc_init)).await?;
        dc.set_buffered_amount_low_threshold(THRESHOLD).await;

        // Attach handlers
        let buffer_watch_tx = watch::channel(true).0;
        attach_buffer_handler(dc.clone(), buffer_watch_tx.clone()).await;
        attach_connection_handler(pc.clone(), maid.event_tx.clone(), maid.error_tx.clone());
        attach_channel_open_handler(dc.clone(), maid.event_tx.clone());

        // Attach on message method
        on_message(
            dc.clone(),
            maid.error_tx.clone(),
            buffer_watch_tx.subscribe(),
            maid.event_tx.clone(),
        );

        Ok(Self {
            pc,
            buffer_watch_tx,
        })
    }

    fn conf(
        username: Option<String>,
        credential: Option<String>,
        additional_servers: &mut Option<Vec<String>>,
    ) -> RTCConfiguration {
        let mut servers: Vec<String> = vec![];

        if let Some(additional_servers) = additional_servers {
            servers.append(additional_servers);
        }

        RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: servers,
                username: username.unwrap_or_default(),
                credential: credential.unwrap_or_default(),
            }],
            ..Default::default()
        }
    }
}

fn attach_connection_handler(
    pc: Arc<RTCPeerConnection>,
    sender: UnboundedSender<BasicEvent>,
    error_tx: ErrorTX,
) {
    let etx = error_tx.clone();
    pc.on_ice_connection_state_change(Box::new(move |state| {
        let etx = etx.clone();

        Box::pin(async move {
            if state == RTCIceConnectionState::Failed {
                etx.send_error(eyre!(state).wrap_err("ICE connection Failed"));
            }
        })
    }));

    pc.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
        let sender = sender.clone();
        let error_tx = error_tx.clone();

        Box::pin(async move {
            match state {
                RTCPeerConnectionState::Connected => {
                    sender.send_event(AppEventClient::Connected).await;
                }
                RTCPeerConnectionState::Disconnected => {
                    sender.send_event(AppEventClient::Disconnected).await;
                }
                RTCPeerConnectionState::Failed => {
                    error_tx.send_error(eyre!(state).wrap_err("RTC connection failed"));
                }
                _ => {}
            }
        })
    }));
}

fn attach_channel_open_handler(dc: Arc<RTCDataChannel>, sender: UnboundedSender<BasicEvent>) {
    dc.on_open(Box::new({
        let dc = dc.clone();

        move || {
            Box::pin(async move {
                sender
                    .send_event(AppEventClient::ChannelOpened(DebugDataChannel::new(
                        dc.clone(),
                    )))
                    .await;
            })
        }
    }));
}

async fn attach_buffer_handler(dc: Arc<RTCDataChannel>, buffer_watch_tx: watch::Sender<bool>) {
    dc.on_buffered_amount_low(Box::new(move || {
        let buffer_watch_tx = buffer_watch_tx.clone();

        Box::pin(async move {
            buffer_watch_tx.send(true).ok();
        })
    }))
    .await;
}

// You're supposed to call it only once at a time
pub async fn wait_for_ice_completion(pc: Arc<RTCPeerConnection>) {
    let (tx, mut rx) = watch::channel(false);

    pc.on_ice_gathering_state_change(Box::new(move |state| {
        let tx = tx.clone();
        Box::pin(async move {
            if state == RTCIceGathererState::Complete {
                tx.send(true).ok();
            }
        })
    }));

    // Wait for ICE gathering to complete
    while !*rx.borrow() {
        rx.changed().await.ok();
    }
}

fn on_message(
    dc: Arc<RTCDataChannel>,
    error_tx: ErrorTX,
    buffer_watch_rx: watch::Receiver<bool>,
    sender: UnboundedSender<BasicEvent>,
) {
    let channel = dc.clone();
    let metadata_map = Arc::new(Mutex::new(HashMap::<usize, MetaData>::new()));
    let metadata_bytes_map = Arc::new(Mutex::new(HashMap::<usize, Vec<u8>>::new()));

    dc.on_message(Box::new(move |msg| {
        let channel = channel.clone();
        let buffer_watch_rx = buffer_watch_rx.clone();
        let sender = sender.clone();
        let metadata_map = metadata_map.clone();
        let metadata_bytes_map = metadata_bytes_map.clone();
        let error_tx = error_tx.clone();

        Box::pin(async move {
            let buffer_watch_rx = &mut buffer_watch_rx.clone();
            if let Err(err) = handle_message(
                msg,
                channel,
                buffer_watch_rx,
                sender,
                metadata_map,
                metadata_bytes_map,
            )
            .await
            {
                error_tx.send_error(err);
            }
        })
    }));
}
