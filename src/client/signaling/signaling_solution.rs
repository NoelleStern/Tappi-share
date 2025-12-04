use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SignalingMessage {
    Uuid(Uuid),
    Offer(String),
    Answer(String),
}

// Simple enough!
#[async_trait]
pub trait SignalingInterface {
    async fn connect(&mut self) -> color_eyre::Result<()>;
    async fn disconnect(&mut self) -> color_eyre::Result<()>;
    async fn send_message(&mut self, message: SignalingMessage) -> color_eyre::Result<()>;
    async fn receive_message(&mut self) -> color_eyre::Result<Option<SignalingMessage>>;
}
