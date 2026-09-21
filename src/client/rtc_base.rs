use std::sync::Arc;
use color_eyre::eyre::eyre;
use webrtc::api::APIBuilder;
use std::collections::HashMap;
use tokio::sync::{Mutex, watch};
use tokio::sync::mpsc::UnboundedSender;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::cli::ClientArgs;
use crate::app::event::BasicEvent;
use crate::app::file_manager::MetaData;
use crate::app::models::{ErrorTX, Maid};
use crate::app::event::BasicEventSenderExt;
use crate::app::app_event::{AppEventClient, DebugDataChannel};
use crate::client::message::{MessageHandlerState, handle_message};


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
        maid.event_tx.send_event(AppEventClient::InitConnection(wc)).await;
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

        Ok(Self { pc, buffer_watch_tx })
    }

    fn conf(
        username: Option<String>,
        credential: Option<String>,
        additional_servers: &mut Option<Vec<String>>,
    ) -> RTCConfiguration {
        // No credentials
        let mut ice_servers = vec![
            RTCIceServer {
                urls: vec![
                    "stun:stun.l.google.com:19302".to_owned(),
                    "stun:stun1.l.google.com:19302".to_owned(),
                    "stun:stun2.l.google.com:19302".to_owned(),
                    "stun:stun3.l.google.com:19302".to_owned(),
                    "stun:stun4.l.google.com:19302".to_owned(),
                ],
                ..Default::default()
            },
        ];

        // All credentials
        if let Some(turn_urls) = additional_servers {
            if !turn_urls.is_empty() {
                ice_servers.push(RTCIceServer {
                    urls: turn_urls.to_vec(),
                    username: username.unwrap_or_default().to_string(),
                    credential: credential.unwrap_or_default().to_string(),
                });
            }
        }

        // Combine
        RTCConfiguration { ice_servers, ..Default::default() }
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
            match state {
                RTCIceConnectionState::Unspecified  => log::info!("RTCIceConnectionState::Unspecified"),
                RTCIceConnectionState::New          => log::info!("RTCIceConnectionState::New"),
                RTCIceConnectionState::Checking     => log::info!("RTCIceConnectionState::Checking"),
                RTCIceConnectionState::Connected    => log::info!("RTCIceConnectionState::Connected"),
                RTCIceConnectionState::Completed    => log::info!("RTCIceConnectionState::Completed"),
                RTCIceConnectionState::Disconnected => log::info!("RTCIceConnectionState::Disconnected"),
                RTCIceConnectionState::Closed       => log::info!("RTCIceConnectionState::Closed"),
                RTCIceConnectionState::Failed       => {
                    log::info!("RTCIceConnectionStateFailedUnspecified");
                    etx.send_error(
                        eyre!("ICE connection state: {:?}", state).wrap_err("ICE connection failed")
                    );
                }
            }
        })
    }));

    pc.on_ice_candidate(Box::new(move |oc: Option<webrtc::ice_transport::ice_candidate::RTCIceCandidate>| {
        Box::pin(async move {
            if let Some(c) = oc { log::info!("{:#?}", c); }
        })
    }));
    
    pc.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
        let sender = sender.clone();
        let error_tx = error_tx.clone();

        Box::pin(async move {
            match state {
                RTCPeerConnectionState::Unspecified => log::info!("RTCPeerConnectionState::Unspecified"),
                RTCPeerConnectionState::New         => log::info!("RTCPeerConnectionState::New"),
                RTCPeerConnectionState::Connecting  => log::info!("RTCPeerConnectionState::Connecting"),
                RTCPeerConnectionState::Closed      => log::info!("RTCPeerConnectionState::Closed"),
                RTCPeerConnectionState::Connected => {
                    log::info!("RTCPeerConnectionState::Connected");
                    sender.send_event(AppEventClient::Connected).await;
                }
                RTCPeerConnectionState::Disconnected => {
                    log::info!("RTCPeerConnectionState::Disconnected");
                    sender.send_event(AppEventClient::Disconnected).await;
                }
                RTCPeerConnectionState::Failed => {
                    log::info!("RTCPeerConnectionState::Failed");
                    error_tx.send_error(eyre!(state).wrap_err("RTC connection failed"));
                }
            }
        })
    }));
}

fn attach_channel_open_handler(dc: Arc<RTCDataChannel>, sender: UnboundedSender<BasicEvent>) {
    dc.on_open(Box::new({
        let dc = dc.clone();

        move || {
            Box::pin(async move {
                sender.send_event(AppEventClient::ChannelOpened(DebugDataChannel::new(dc.clone()))).await;
            })
        }
    }));
}

async fn attach_buffer_handler(dc: Arc<RTCDataChannel>, buffer_watch_tx: watch::Sender<bool>) {
    dc.on_buffered_amount_low(Box::new(move || {
        let buffer_watch_tx = buffer_watch_tx.clone();
        Box::pin(async move { buffer_watch_tx.send(true).ok(); })
    }))
    .await;
}

// You're supposed to call it only once at a time
pub async fn wait_for_ice_completion(pc: Arc<RTCPeerConnection>) {
    let (tx, mut rx) = watch::channel(false);

    pc.on_ice_gathering_state_change(Box::new(move |state| {
        let tx = tx.clone();
        Box::pin(async move {
            match state {
                RTCIceGathererState::Unspecified    => log::info!("RTCIceGathererState::Unspecified"),
                RTCIceGathererState::New            => log::info!("RTCIceGathererState::New"),
                RTCIceGathererState::Gathering      => log::info!("RTCIceGathererState::Gathering"),
                RTCIceGathererState::Closed         => log::info!("RTCIceGathererState::Closed"),
                RTCIceGathererState::Complete       => {
                    log::info!("RTCIceGathererState::Complete");
                    tx.send(true).ok();
                },
            }
        })
    }));

    // Wait for ICE gathering to complete
    while !*rx.borrow() { rx.changed().await.ok(); }
}

fn on_message(
    dc: Arc<RTCDataChannel>,
    error_tx: ErrorTX,
    buffer_watch_rx: watch::Receiver<bool>,
    sender: UnboundedSender<BasicEvent>,
) {
    let channel = dc.clone();
    let state = Arc::new(Mutex::new(MessageHandlerState::default()));
    let metadata_map = Arc::new(Mutex::new(HashMap::<usize, MetaData>::new()));
    let metadata_bytes_map = Arc::new(Mutex::new(HashMap::<usize, Vec<u8>>::new()));

    dc.on_message(Box::new(move |msg| {
        let channel = channel.clone();
        let buffer_watch_rx = buffer_watch_rx.clone();
        let sender = sender.clone();
        let state = state.clone();
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
                state,
                metadata_map,
                metadata_bytes_map,
            )
            .await { error_tx.send_error(err); }
        })
    }));
}
