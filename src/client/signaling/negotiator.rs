use color_eyre::eyre::eyre;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;
use webrtc::peer_connection::{RTCPeerConnection, sdp::session_description::RTCSessionDescription};

use crate::{
    app::{
        app_event::AppEventClient,
        event::{BasicEvent, BasicEventSenderExt},
        models::Maid,
    },
    cli::{ClientArgs, SignalingSolutions},
    client::{
        rtc_base::wait_for_ice_completion,
        signaling::{
            signaling_manual::SignalingManual,
            signaling_mqtt::SignalingMqtt,
            signaling_solution::{SignalingInterface, SignalingMessage},
            signaling_websocket::SignalingWebsocket,
        },
    },
};

// Connecting to server -> connected to server -> uuid sent ->
// uuid received -> offer sent -> answer received -> connection established
//               -> offer received -> answer sent ->
#[derive(Clone, Debug, Default)]
pub enum HandshakeState {
    #[default]
    Initial,
    ConnectingToServer,
    ConnectedToServer,
    UUIDSent,
    UUIDReceived,
    OfferSent,
    OfferReceived,
    AnswerSent,
    AnswerReceived,
    ExchangeFinished,
}

/// Negotiator struct
///
/// Handles the signaling negotiation process and resolves different signaling solutions
pub struct Negotiator<S: SignalingInterface> {
    sender: UnboundedSender<BasicEvent>,
    pc: Arc<RTCPeerConnection>,
    signaling: S,
    uuid: Uuid,
    handle_same_uuid: bool,
}
impl<S: SignalingInterface> Negotiator<S> {
    pub fn new(
        sender: UnboundedSender<BasicEvent>,
        pc: Arc<RTCPeerConnection>,
        signaling: S,
        handle_same_uuid: bool,
    ) -> Self {
        Self {
            sender,
            pc,
            signaling,
            uuid: Uuid::exclude_edge_cases(),
            handle_same_uuid,
        }
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        log::info!("Negotiation started");

        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::ConnectingToServer,
            ))
            .await;

        self.signaling.connect().await?;
        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::ConnectedToServer,
            ))
            .await;

        self.signaling
            .send_message(SignalingMessage::Uuid(self.uuid))
            .await?;
        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::UUIDSent,
            ))
            .await;

        loop {
            if let Some(signaling_message) = self.signaling.receive_message().await? {
                match signaling_message {
                    SignalingMessage::Uuid(uuid) => self.handle_uuid(uuid).await?,
                    SignalingMessage::Offer(sdp) => {
                        self.handle_offer(sdp).await?;
                        break;
                    } // TODO: fix, this is a hack
                    SignalingMessage::Answer(sdp) => {
                        self.handle_answer(sdp).await?;
                        break;
                    } // TODO: fix, this is a hack
                }
            }
        }

        log::info!("Negotiation finished");
        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::ExchangeFinished,
            ))
            .await;
        self.signaling.disconnect().await?;

        Ok(())
    }

    async fn handle_uuid(&mut self, uuid: Uuid) -> color_eyre::Result<()> {
        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::UUIDReceived,
            ))
            .await;

        // WOW, that's a rarity! But it could theoretically happen, right?
        // I mean, it could happen on manual signaling simply by mistake, to be honest
        // Upd.: i don't actually think it could happen on manual signaling
        if self.uuid == uuid {
            if self.handle_same_uuid {
                self.uuid = Uuid::exclude_edge_cases(); // Assign a new UUID
                self.signaling
                    .send_message(SignalingMessage::Uuid(self.uuid))
                    .await?; // Report it
            } else {
                return Err(eyre!("UUID clash"));
            }
        } else {
            let polite: bool = self.uuid < uuid; // Determine politeness

            // If impolite - make an offer
            if !polite {
                // Create an offer, confirm it and wait for all of the ice candidates
                let offer = self.pc.create_offer(None).await?;
                self.pc.set_local_description(offer.clone()).await?;
                wait_for_ice_completion(self.pc.clone()).await;

                if let Some(local_desc) = self.pc.local_description().await {
                    self.signaling
                        .send_message(SignalingMessage::Offer(local_desc.sdp))
                        .await?;

                    self.sender
                        .send_event(AppEventClient::UpdateHandshakeState(
                            HandshakeState::OfferSent,
                        ))
                        .await;
                }
            }
        }

        Ok(())
    }

    async fn handle_offer(&mut self, sdp: String) -> color_eyre::Result<()> {
        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::OfferReceived,
            ))
            .await;

        // Accept remote offer
        let remote_offer = RTCSessionDescription::offer(sdp)?;
        self.pc.set_remote_description(remote_offer).await?;

        // Create an answer
        let answer = self.pc.create_answer(None).await?;
        self.pc.set_local_description(answer.clone()).await?;
        wait_for_ice_completion(self.pc.clone()).await;

        // Send the answer
        self.signaling
            .send_message(SignalingMessage::Answer(answer.sdp))
            .await?;

        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::AnswerSent,
            ))
            .await;

        Ok(())
    }

    async fn handle_answer(&mut self, sdp: String) -> color_eyre::Result<()> {
        self.sender
            .send_event(AppEventClient::UpdateHandshakeState(
                HandshakeState::AnswerReceived,
            ))
            .await;

        let remote_answer = RTCSessionDescription::answer(sdp)?;
        self.pc.set_remote_description(remote_answer).await?;
        Ok(())
    }
}

pub async fn negotiate(
    pc: Arc<RTCPeerConnection>,
    args: ClientArgs,
    maid: Maid,
    signaling_manual: Option<SignalingManual>,
) -> color_eyre::Result<()> {
    match &args.signaling_mode {
        SignalingSolutions::Manual(_signaling_args) => {
            if let Some(signaling_manual) = signaling_manual {
                let mut negotiator =
                    Negotiator::new(maid.event_tx.clone(), pc.clone(), signaling_manual, false);
                negotiator.run().await?;
            }
        }
        SignalingSolutions::Socket(signaling_args) => {
            let url = SignalingWebsocket::build_url(&signaling_args.address, &signaling_args.room)?;
            let sc =
                SignalingWebsocket::from_url(&url, maid.error_tx.clone(), maid.token.child_token())
                    .await?;
            let mut negotiator = Negotiator::new(maid.event_tx.clone(), pc.clone(), sc, true);
            negotiator.run().await?;
        }
        SignalingSolutions::Mqtt(signaling_args) => {
            let sc = SignalingMqtt::new(
                signaling_args.clone(),
                maid.error_tx.clone(),
                maid.token.child_token(),
            );
            let mut negotiator = Negotiator::new(maid.event_tx.clone(), pc.clone(), sc, true);
            negotiator.run().await?;
        }
    }
    Ok(())
}

pub trait UuidExt {
    /// Creates an all Fs UUID
    fn full() -> Uuid;
    /// Excludes all 0s and all Fs UUIDs
    /// which manual signaling makes use of
    fn exclude_edge_cases() -> Uuid;
}
impl UuidExt for Uuid {
    fn full() -> Uuid {
        Uuid::from_bytes([0xFFu8; 16])
    }

    fn exclude_edge_cases() -> Uuid {
        loop {
            let id = Uuid::new_v4();
            if id != Uuid::nil() && id != Uuid::full() {
                return id;
            }
        }
    }
}
