use async_trait::async_trait;
use color_eyre::eyre::OptionExt;
use crossterm::{self, event::Event as CrosstermEvent};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::app::app_event::AppEvent;

/// The frequency at which tick events are emitted.
const TICK_FPS: f64 = 30.0;

/// Representation of all possible events.
#[derive(Clone, Debug)]
pub enum BasicEvent {
    /// An event that is emitted on a regular schedule.
    ///
    /// Use this event to run any code which has to run outside of being a direct response to a user
    /// event. e.g. polling external systems, updating animations, or rendering the UI based on a
    /// fixed frame rate.
    Tick,
    /// Crossterm events.
    ///
    /// These events are emitted by the terminal.
    Crossterm(CrosstermEvent),
    /// Application events.
    ///
    /// Use this event to emit custom events that are specific to your application.
    App(AppEvent),
}

/// Terminal event handler.
#[derive(Debug)]
pub struct EventHandler {
    /// Event sender channel.
    sender: UnboundedSender<BasicEvent>,
    /// Event receiver channel.
    receiver: UnboundedReceiver<BasicEvent>,
}

// Allows to send events with ease
#[async_trait]
pub trait BasicEventSenderExt {
    async fn send_event<T: Into<BasicEvent> + Send>(&self, msg: T);
}

#[async_trait]
impl BasicEventSenderExt for UnboundedSender<BasicEvent> {
    async fn send_event<T: Into<BasicEvent> + Send>(&self, msg: T) {
        self.send(msg.into()).ok();
    }
}

impl EventHandler {
    /// Constructs a new instance of [`EventHandler`] and spawns a new thread to handle events.
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<BasicEvent>();
        let actor = EventTask::new(sender.clone());
        tokio::spawn(async { actor.run().await }); // I don't have to kill it specifically
        Self { sender, receiver }
    }

    /// Receives an event from the sender.
    ///
    /// This function blocks until an event is received.
    ///
    /// # Errors
    ///
    /// This function returns an error if the sender channel is disconnected. This can happen if an
    /// error occurs in the event thread. In practice, this should not happen unless there is a
    /// problem with the underlying terminal.
    pub async fn next(&mut self) -> color_eyre::Result<BasicEvent> {
        self.receiver
            .recv()
            .await
            .ok_or_eyre("Failed to receive event")
    }

    /// Queue an app event to be sent to the event receiver.
    ///
    /// This is useful for sending events to the event handler which will be processed by the next
    /// iteration of the application's event loop.
    pub fn send(&mut self, app_event: impl Into<BasicEvent>) {
        // Ignore the result as the receiver cannot be dropped while this struct still has a
        // reference to it
        self.sender.send(app_event.into()).ok();
    }

    pub fn send_app_event(&mut self, app_event: AppEvent) {
        // Ignore AppEvent::None
        match app_event {
            AppEvent::None => {}
            _ => {
                self.sender.send(app_event.into()).ok();
            }
        }
    }

    pub fn sender(&self) -> UnboundedSender<BasicEvent> {
        self.sender.clone()
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// A thread that handles reading crossterm events and emitting tick events on a regular schedule.
struct EventTask {
    /// Event sender channel.
    sender: UnboundedSender<BasicEvent>,
}

impl EventTask {
    /// Constructs a new instance of [`EventThread`].
    fn new(sender: UnboundedSender<BasicEvent>) -> Self {
        Self { sender }
    }

    /// Runs the event thread.
    ///
    /// This function emits tick events at a fixed rate and polls for crossterm events in between.
    async fn run(mut self) -> color_eyre::Result<()> {
        let tick_rate = Duration::from_secs_f64(1.0 / TICK_FPS);
        let mut tick = tokio::time::interval(tick_rate);
        let mut reader = crossterm::event::EventStream::new();

        loop {
            let tick_delay = tick.tick();
            let crossterm_event = reader.next();

            tokio::select! {
                _ = self.sender.closed() => {
                    break;
                }
                _ = tick_delay => {
                    self.send(BasicEvent::Tick);
                },
                val = crossterm_event => {
                    if let Some(val) = val {
                        self.send(BasicEvent::Crossterm(val?));
                    }
                },
            };
        }

        Ok(())
    }

    /// Sends an event to the receiver.
    fn send(&mut self, event: BasicEvent) {
        // Ignores the result because shutting down the app drops the receiver, which causes the send
        // operation to fail. This is expected behavior and should not panic.
        self.sender.send(event).ok();
    }
}
