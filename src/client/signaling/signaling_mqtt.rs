use async_trait::async_trait;
use rumqttc::{AsyncClient, EventLoop, LastWill, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time;
use tokio_util::sync::CancellationToken;

use crate::app::encrypt::{Secret, try_decrypt_claims, try_encrypt_claims};
use crate::app::models::ErrorTX;
use crate::cli::SignalingSolutionMqttArgs;
use crate::client::signaling::signaling_solution::{SignalingInterface, SignalingMessage};

pub struct SignalingMqtt {
    client: AsyncClient,
    event_loop: Arc<Mutex<EventLoop>>,

    // Tunnels incoming messages further
    rx: UnboundedReceiver<String>, // Use on receive_message
    tx: UnboundedSender<String>,   // Put messages here

    args: SignalingSolutionMqttArgs,

    // Error sender
    error_tx: ErrorTX,
    // Cancellation token
    token: CancellationToken,

    // Tasks
    receive_task: Option<tokio::task::JoinHandle<()>>,

    // First message should be retained
    retain_flag: bool,
}
impl SignalingMqtt {
    pub fn new(
        args: SignalingSolutionMqttArgs,
        error_tx: ErrorTX,
        token: CancellationToken,
    ) -> Self {
        let mut mqtt_options =
            MqttOptions::new(args.local_name.clone(), args.broker.clone(), args.port);
        mqtt_options
            .set_last_will(LastWill::new(
                args.local_topic(),
                "",
                QoS::ExactlyOnce,
                true,
            ))
            .set_keep_alive(Duration::from_secs(args.keep_alive as u64))
            .set_clean_session(true);

        let (client, event_loop) = AsyncClient::new(mqtt_options, 10);
        let (tx, rx) = unbounded_channel::<String>();
        let event_loop = Arc::new(Mutex::new(event_loop));

        Self {
            client,
            event_loop,
            tx,
            rx,
            args,
            error_tx,
            token,
            receive_task: None,
            retain_flag: true,
        }
    }

    pub async fn init(&mut self) -> color_eyre::Result<()> {
        self.client
            .subscribe(self.args.remote_topic(), QoS::ExactlyOnce)
            .await?; // Subscribe
        self.receive_task = Some(self.spawn_receive_task()?);
        Ok(())
    }

    pub async fn close(&mut self) -> color_eyre::Result<()> {
        time::sleep(Duration::from_secs(5)).await; // TODO: this is a hack, but otherwise the last message might get lost

        self.client
            .publish(self.args.local_topic(), QoS::ExactlyOnce, true, "")
            .await?; // Emulate last will
        self.client.disconnect().await?; // Disconnect gracefully

        if let Some(spawn_loop) = &self.receive_task {
            spawn_loop.abort();
        }

        Ok(())
    }

    pub async fn send(&self, text: String, retain: bool) -> color_eyre::Result<()> {
        let msg = try_encrypt_claims(text, &self.args.secret)?;
        self.client
            .publish(self.args.local_topic(), QoS::ExactlyOnce, retain, msg)
            .await?;
        Ok(())
    }

    fn spawn_receive_task(&self) -> color_eyre::Result<tokio::task::JoinHandle<()>> {
        let event_loop = self.event_loop.clone();
        let secret = self.args.secret.clone();
        let mut tx = self.tx.clone();
        let error_tx = self.error_tx.clone();
        let token = self.token.child_token();

        let task = tokio::spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {},
                _ = Self::receive_loop(&event_loop, &mut tx, &secret, error_tx) => {}
            }
        });

        Ok(task)
    }

    async fn receive_loop(
        event_loop: &Arc<Mutex<EventLoop>>,
        tx: &mut UnboundedSender<String>,
        secret: &Option<Secret>,
        error_tx: ErrorTX,
    ) {
        loop {
            if let Err(err) = Self::receive(event_loop, tx, secret).await {
                error_tx.send_error(err);
                break;
            }
        }
    }

    async fn receive(
        event_loop: &Arc<Mutex<EventLoop>>,
        tx: &mut UnboundedSender<String>,
        secret: &Option<Secret>,
    ) -> color_eyre::Result<()> {
        let mut event_loop_lock = event_loop.lock().await;
        let event = event_loop_lock.poll().await?;
        if let rumqttc::Event::Incoming(packet) = event
            && let Packet::Publish(publish) = packet
        {
            let payload_str = std::str::from_utf8(&publish.payload)?;

            if !payload_str.is_empty() {
                let text = try_decrypt_claims(payload_str, secret)?;
                tx.send(text)?;
            }
        }

        Ok(())
    }
}
#[async_trait]
impl SignalingInterface for SignalingMqtt {
    async fn connect(&mut self) -> color_eyre::Result<()> {
        self.init().await?;
        Ok(())
    }
    async fn disconnect(&mut self) -> color_eyre::Result<()> {
        self.close().await?;
        Ok(())
    }
    async fn send_message(&mut self, message: SignalingMessage) -> color_eyre::Result<()> {
        let json = serde_json::to_string(&message)?;
        self.send(json, self.retain_flag).await?;
        self.retain_flag = false;
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
