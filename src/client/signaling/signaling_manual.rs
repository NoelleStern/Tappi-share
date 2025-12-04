use async_trait::async_trait;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use crate::{
    app::{
        app_event::AppEventClient,
        encrypt::try_encrypt_claims,
        event::{BasicEvent, BasicEventSenderExt},
    },
    cli::SignalingSolutionManualArgs,
    client::signaling::{
        negotiator::UuidExt,
        signaling_solution::{SignalingInterface, SignalingMessage},
    },
};

pub struct SignalingManual {
    /// Outgoing messages tx
    sender: UnboundedSender<BasicEvent>,
    /// Incoming messages tx
    /// Required to initialize correctly
    itx: UnboundedSender<SignalingMessage>,
    /// Incoming messages rx
    irx: UnboundedReceiver<SignalingMessage>,
    /// Manual signaling arguments
    args: SignalingSolutionManualArgs,
}
impl SignalingManual {
    pub fn new(sender: UnboundedSender<BasicEvent>, args: SignalingSolutionManualArgs) -> Self {
        let (itx, irx) = unbounded_channel::<SignalingMessage>();
        Self {
            sender,
            itx,
            irx,
            args,
        }
    }

    pub async fn init(&self) {
        SignalingManual::set_politeness(&self.itx, self.args.polite);
        self.sender
            .send_event(AppEventClient::ManualSignalingInit(self.args.polite))
            .await;
    }

    pub fn sender(&self) -> UnboundedSender<SignalingMessage> {
        self.itx.clone()
    }

    /// This emulates the other side sending its UUID
    pub fn set_politeness(itx: &UnboundedSender<SignalingMessage>, polite: bool) {
        let uuid = if polite { Uuid::full() } // Full of Fs
        else { Uuid::nil() }; // Full of 0s

        itx.send(SignalingMessage::Uuid(uuid)).ok();
    }
}
#[async_trait]
impl SignalingInterface for SignalingManual {
    async fn connect(&mut self) -> color_eyre::Result<()> {
        self.init().await;
        Ok(())
    }
    async fn disconnect(&mut self) -> color_eyre::Result<()> {
        Ok(())
    }
    async fn send_message(&mut self, message: SignalingMessage) -> color_eyre::Result<()> {
        if let SignalingMessage::Uuid(_) = message {
        }
        // Skip any UUID messages
        else {
            let json = serde_json::to_string(&message)?;
            let text = try_encrypt_claims(json, &self.args.secret)?;
            self.sender
                .send_event(AppEventClient::ManualSignalingOutput(text))
                .await;
        }
        Ok(())
    }
    async fn receive_message(&mut self) -> color_eyre::Result<Option<SignalingMessage>> {
        Ok(self.irx.recv().await)
    }
}
