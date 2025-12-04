use async_trait::async_trait;
use color_eyre::eyre::Context;
use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use std::sync::Arc;
use tokio::{
    net::TcpStream,
    sync::{
        Mutex,
        mpsc::{UnboundedReceiver, UnboundedSender},
    },
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    app::models::ErrorTX,
    client::signaling::signaling_solution::{SignalingInterface, SignalingMessage},
};

pub struct SignalingWebsocket {
    // Socket interface
    socket_rx: Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    socket_tx: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,

    // Tunnels incoming messages further
    rx: UnboundedReceiver<String>, // Use on receive_message
    tx: UnboundedSender<String>,   // Put messages here

    // Error sender
    error_tx: ErrorTX,
    // Cancellation token
    token: CancellationToken,

    // Tasks
    receive_task: Option<tokio::task::JoinHandle<()>>,
}
impl SignalingWebsocket {
    pub fn new(
        socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
        error_tx: ErrorTX,
        token: CancellationToken,
    ) -> Self {
        let (socket_tx, socket_rx) = socket.split();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let socket_rx = Arc::new(Mutex::new(socket_rx));

        Self {
            socket_rx,
            socket_tx,
            rx,
            tx,
            error_tx,
            token,
            receive_task: None,
        }
    }

    pub async fn from_url(
        url: &Url,
        error_tx: ErrorTX,
        token: CancellationToken,
    ) -> color_eyre::Result<Self> {
        let socket = SignalingWebsocket::create_ws_connection(url).await?;
        Ok(SignalingWebsocket::new(socket, error_tx, token))
    }

    // Create a WebSocket connection
    pub async fn create_ws_connection(
        url: &Url,
    ) -> color_eyre::Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
        let (socket, _) = connect_async(url.as_str())
            .await
            .wrap_err("Failed to establish a WebSocket connection")?;

        log::info!("WebSocket connection was established");

        Ok(socket)
    }

    // Build a request url
    pub fn build_url(address: &str, room_id: &str) -> color_eyre::Result<Url> {
        let base_address = format!("ws://{}/room", address);
        let mut url = Url::parse(&base_address)?;
        url.query_pairs_mut().append_pair("room", room_id);
        Ok(url)
    }

    pub fn init(&mut self) {
        self.receive_task = Some(self.spawn_receive_task());
    }

    pub async fn close(&mut self) -> color_eyre::Result<()> {
        if let Some(spawn_loop) = &self.receive_task {
            spawn_loop.abort();
        }

        self.socket_tx.close().await?;

        Ok(())
    }

    pub async fn send(&mut self, text: String) -> color_eyre::Result<()> {
        self.socket_tx.send(Message::Text(text.into())).await?;
        Ok(())
    }

    fn spawn_receive_task(&self) -> tokio::task::JoinHandle<()> {
        let socket_rx = self.socket_rx.clone();
        let mut tx = self.tx.clone();
        let error_tx = self.error_tx.clone();
        let token = self.token.child_token();

        tokio::spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {},
                _ = Self::receive_loop(socket_rx, &mut tx, error_tx) => {}
            }
        })
    }

    async fn receive_loop(
        socket_rx: Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
        tx: &mut UnboundedSender<String>,
        error_tx: ErrorTX,
    ) {
        loop {
            match Self::receive(&socket_rx, tx).await {
                Ok(result) => {
                    if result.is_some() {
                        break;
                    }
                }
                Err(err) => {
                    error_tx.send_error(err);
                    break;
                }
            }
        }
    }

    async fn receive(
        socket_rx: &Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
        tx: &mut UnboundedSender<String>,
    ) -> color_eyre::Result<Option<bool>> {
        let mut socket_rx_lock = socket_rx.lock().await;
        match socket_rx_lock.next().await {
            Some(result) => {
                let msg = result.wrap_err("WebSocket message error")?;
                let msg_text = msg.to_text()?.to_string();
                tx.send(msg_text)?;
                Ok(None)
            }
            None => Ok(Some(true)),
        }
    }
}
#[async_trait]
impl SignalingInterface for SignalingWebsocket {
    async fn connect(&mut self) -> color_eyre::Result<()> {
        self.init();
        Ok(())
    }
    async fn disconnect(&mut self) -> color_eyre::Result<()> {
        self.close().await?;
        Ok(())
    }

    async fn send_message(&mut self, message: SignalingMessage) -> color_eyre::Result<()> {
        let json = serde_json::to_string(&message)?;
        self.send(json).await?;
        Ok(())
    }
    async fn receive_message(&mut self) -> color_eyre::Result<Option<SignalingMessage>> {
        let mut result: Option<SignalingMessage> = None;
        let message = self.rx.recv().await;

        if let Some(message) = message
            && let Ok(signaling_message) = serde_json::from_str(&message)
        {
            result = Some(signaling_message);
        }

        Ok(result)
    }
}
