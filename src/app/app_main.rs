use crossterm::event::{KeyCode, KeyEvent};
use rat_focus::Focus;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

use crate::{
    app::{
        app_event::AppEvent,
        event::{BasicEvent, EventHandler},
        file_manager::FileManager,
        handlers::{
            app_handler::AppHandler, client_handler::ClientHandler, server_handler::ServerHandler,
        },
        models::{ClientState, ErrorTX, Maid},
    },
    cli::{Cli, Commands, SignalingSolutions},
    client::{
        client_init::init,
        signaling::{negotiator::HandshakeState, signaling_manual::SignalingManual},
    },
    server,
    ui::{
        theme::Theme,
        utils::{CombinedWidgetState, Shortcut},
        widgets::{
            files_widget::FileListWidgetState, history_widget::HistoryWidgetState,
            manual_handshake_widget::ManualHandshakeWidgetState, rooms_widget::RoomListWidgetState,
            throbber::ThrobberStateCounter, users_widget::UserListWidgetState,
        },
    },
};

/// The main data structure
pub struct App {
    // Base app stuff
    /// Should the application exit?
    pub exit: bool,
    /// Should the application redraw?
    /// Tied directly to the tick event
    pub redraw: bool,
    /// User-provided arguments
    pub args: Cli,
    /// General event handler
    pub events: EventHandler,
    /// Error report
    pub error: Option<color_eyre::Report>,
    /// Error tx
    pub error_tx: ErrorTX,
    /// Error rx
    pub error_rx: UnboundedReceiver<color_eyre::Report>,
    /// Tokio task cancellation token
    pub cancellation_token: CancellationToken,
    /// Theme colors
    pub theme: Theme,

    // App handlers and app states
    /// File handler, allows to operate on files with ease
    pub file_manager: FileManager,
    /// File-sharing client state
    pub client_state: ClientState,
    /// Signaling handshake state
    pub handshake_state: HandshakeState,

    // Base widget stuff
    /// Focus handler, simplifies focus management (updates after each re-render)
    pub focus: Focus,
    /// Throbber state counter, spins all throbbers (updates on tick)
    pub throbber_sc: ThrobberStateCounter,
    /// Shortcuts of a focused widget
    pub widget_shortcuts: Vec<Shortcut>,

    // Client widget states
    pub handshake_widget_state: ManualHandshakeWidgetState,
    pub input_list_widget_state: FileListWidgetState,
    pub output_list_widget_state: FileListWidgetState,

    // Server widget states
    pub room_list_widget_state: RoomListWidgetState,
    pub user_list_widget_state: UserListWidgetState,
    pub history_widget_state: HistoryWidgetState,
}
impl App {
    pub fn new(args: Cli) -> color_eyre::Result<Self> {
        let (error_tx, error_rx) = tokio::sync::mpsc::unbounded_channel::<color_eyre::Report>();
        let ignore_empty: bool = if let Commands::Client(client_args) = &args.app_mode {
            client_args.ignore_empty
        } else {
            false
        };

        Ok(Self {
            // App
            exit: false,
            redraw: true,
            args,
            events: EventHandler::new(),
            error: None,
            error_tx: ErrorTX(error_tx),
            error_rx,
            theme: Theme::load_default()?,
            file_manager: FileManager::new(ignore_empty),
            client_state: ClientState::default(),
            handshake_state: HandshakeState::default(),
            cancellation_token: CancellationToken::new(),
            // UI
            focus: Focus::default(),
            throbber_sc: ThrobberStateCounter::new(3),
            widget_shortcuts: vec![],
            handshake_widget_state: ManualHandshakeWidgetState::default(),
            input_list_widget_state: FileListWidgetState::default(),
            output_list_widget_state: FileListWidgetState::default(),
            room_list_widget_state: RoomListWidgetState::default(),
            user_list_widget_state: UserListWidgetState::default(),
            history_widget_state: HistoryWidgetState::default(),
        })
    }

    pub fn get_maid(&self) -> Maid {
        Maid::new(
            self.error_tx.clone(),
            self.events.sender(),
            self.cancellation_token.child_token(),
        )
    }

    /// Main entry point
    pub async fn run(
        mut self,
        args: &Cli,
        terminal: &mut DefaultTerminal,
    ) -> color_eyre::Result<()> {
        startup(&mut self, args)?; // Start up the side process

        self.main_loop(terminal).await?; // Run the main loop
        self.cancellation_token.cancel(); // Cancel all tasks
        self.error_loop(terminal).await?; // Show an error screen if something went wrong

        if let Some(error) = self.error {
            Err(error)
        } else {
            Ok(())
        }
    }

    async fn main_loop(&mut self, terminal: &mut DefaultTerminal) -> color_eyre::Result<()> {
        while !self.exit {
            // Redraw
            if self.redraw {
                self.draw(terminal)?;
            }

            // Event loop
            let error = tokio::select! {
                event = self.events.next() => { // Event loop
                    let result = self.process_event(event?).await;
                    result.err()
                }
                err = self.error_rx.recv() => { // Error catcher
                    err
                }
            };

            if let Some(err) = error {
                log::error!("{}", err);
                self.error = Some(err);
                self.exit = true;
            }
        }

        Ok(())
    }

    async fn error_loop(&mut self, terminal: &mut DefaultTerminal) -> color_eyre::Result<()> {
        // Simple error loop
        if self.error.is_some() {
            loop {
                // Redraw
                if self.redraw {
                    self.draw(terminal)?;
                }

                // Event loop
                let event = self.events.next().await?;
                match event {
                    BasicEvent::Tick => {
                        self.on_tick();
                    }
                    BasicEvent::Crossterm(crossterm::event::Event::Key(key_event)) => {
                        if key_event.is_release()
                            && let KeyCode::Char('q') = key_event.code
                        {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Cool and sexy event processor!
    async fn process_event(&mut self, event: BasicEvent) -> color_eyre::Result<()> {
        // Handle key and tick events
        self.handle_tick_and_crossterm(&event)?;

        // Handle app events
        if let BasicEvent::App(app_event) = event {
            match self.args.app_mode {
                Commands::Client(_) => ClientHandler::handle_app_events(self, app_event)?,
                Commands::Server(_) => ServerHandler::handle_app_events(self, app_event)?,
            }
        }

        // Set shortcuts
        let mut shortcuts: Vec<Shortcut> = vec![];
        for cws in self.get_focusable_widgets() {
            if cws.is_focused() {
                shortcuts = cws.get_shortcuts();
            }
        }
        self.widget_shortcuts = shortcuts;

        Ok(())
    }

    /// Tick/crossterm event handler
    fn handle_tick_and_crossterm(&mut self, event: &BasicEvent) -> color_eyre::Result<()> {
        match event {
            BasicEvent::Tick => self.on_tick(),
            BasicEvent::Crossterm(crossterm::event::Event::Key(key_event)) => {
                let mut app_events: Vec<AppEvent> = vec![];

                // Handle focus key events
                self.handle_focus_key_events(key_event);

                // Handle global key events
                let handler_event = match self.args.app_mode {
                    Commands::Client(_) => ClientHandler::handle_key_events(key_event)?,
                    Commands::Server(_) => ServerHandler::handle_key_events(key_event)?,
                };
                app_events.push(handler_event);

                // Handle per-widget key events
                for cws in self.get_focusable_widgets() {
                    if cws.is_focused() {
                        let widget_event = cws.handle_key_events(key_event)?;
                        app_events.push(widget_event);
                    }
                }

                // Send resulting events
                for ev in app_events {
                    self.events.send_app_event(ev);
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Focus event handler
    fn handle_focus_key_events(&mut self, key_event: &KeyEvent) {
        if key_event.is_release() {
            match key_event.code {
                KeyCode::Esc => {
                    self.focus.none();
                }
                KeyCode::Tab => {
                    self.focus.next();
                }
                KeyCode::BackTab => {
                    self.focus.prev();
                }
                _ => {}
            };
        }
    }

    /// Draws TUI
    fn draw(&mut self, terminal: &mut DefaultTerminal) -> color_eyre::Result<()> {
        terminal.draw(|frame| frame.render_widget(self, frame.area()))?; // Redraw
        Ok(())
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    pub fn on_tick(&mut self) {
        self.throbber_sc.update();
        self.redraw = true;
    }

    pub fn focusable_widgets_client(&mut self) -> Vec<Box<&mut dyn CombinedWidgetState>> {
        vec![
            Box::new(&mut self.handshake_widget_state),
            Box::new(&mut self.input_list_widget_state),
            Box::new(&mut self.output_list_widget_state),
        ]
    }
    pub fn focusable_widgets_server(&mut self) -> Vec<Box<&mut dyn CombinedWidgetState>> {
        vec![
            Box::new(&mut self.room_list_widget_state),
            Box::new(&mut self.user_list_widget_state),
            Box::new(&mut self.history_widget_state),
        ]
    }
    pub fn get_focusable_widgets(&mut self) -> Vec<Box<&mut dyn CombinedWidgetState>> {
        match self.args.app_mode {
            Commands::Client(_) => self.focusable_widgets_client(),
            Commands::Server(_) => self.focusable_widgets_server(),
        }
    }
}

/// Startup process
fn startup(app: &mut App, args: &Cli) -> color_eyre::Result<()> {
    if let Commands::Client(client_args) = &app.args.app_mode {
        log::info!("Client started in {:?} mode", client_args.signaling_mode);
    }

    match &args.app_mode {
        Commands::Client(args) => {
            // Clone stuff
            let maid = app.get_maid();
            let args_client = args.clone();

            // Add files to the file handler
            if let Some(files) = args.files.clone() {
                app.file_manager.add_output_files(&files)?;
            }

            // Prepare manual signaling
            let mut signaling_manual: Option<SignalingManual> = None;
            if let SignalingSolutions::Manual(args) = &args.signaling_mode {
                let sm = SignalingManual::new(app.events.sender(), args.clone());
                app.client_state.handshake_tx = Some(sm.sender());
                signaling_manual = Some(sm);
            }

            // Run main task
            tokio::spawn(async move {
                let token: CancellationToken = maid.token.child_token();
                let error_tx = maid.error_tx.clone();
                tokio::select! {
                    _ = token.cancelled() => {},
                    result = init(maid, signaling_manual, args_client) => {
                        if let Err(err) = result { error_tx.send_error(err); }
                    },
                }
            });
        }
        Commands::Server(args) => {
            // Clone stuff
            let maid = app.get_maid();
            let args = args.clone();

            // Run main task
            tokio::spawn(async move {
                let token = maid.token.child_token();
                let error_tx = maid.error_tx.clone();
                tokio::select! {
                    _ = token.cancelled() => {},
                    result = server::signal::main(maid, args) => {
                        if let Err(err) = result { error_tx.send_error(err); }
                    },
                }
            });
        }
    }

    Ok(())
}
