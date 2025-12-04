use color_eyre::eyre::Context;
use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{
        app_event::{AppEvent, AppEventClient, DebugDataChannel},
        app_main::App,
        encrypt::try_decrypt_claims,
        file_manager::{FileProgressReport, InputFile, OutputFile, SpeedReport},
        handlers::app_handler::AppHandler,
    },
    cli::{Commands, SignalingSolutions},
    client::{
        message::Message,
        payload,
        rtc_base::WebConnection,
        signaling::{negotiator::HandshakeState, signaling_solution::SignalingMessage},
    },
};

/// Struct for handling client app events
pub struct ClientHandler;
impl AppHandler for ClientHandler {
    fn handle_key_events(key_event: &KeyEvent) -> color_eyre::Result<AppEvent> {
        let mut result: AppEvent = AppEvent::None;

        if key_event.is_release() {
            result = match key_event.code {
                KeyCode::Char('q') => AppEventClient::Quit.into(),
                _ => AppEvent::None,
            }
        }

        Ok(result)
    }

    fn handle_app_events(app: &mut App, event: AppEvent) -> color_eyre::Result<()> {
        if let AppEvent::Client(app_event) = event {
            match app_event {
                AppEventClient::Quit => on_quit(app),
                AppEventClient::UpdateHandshakeState(state) => {
                    on_update_handshake_state(app, state)
                }
                AppEventClient::ManualSignalingInit(polite) => {
                    on_manual_signaling_init(app, polite)
                }
                AppEventClient::ManualSignalingInput(text) => on_manual_signaling_input(app, text)?,
                AppEventClient::ManualSignalingOutput(msg) => on_manual_signaling_output(app, msg),
                AppEventClient::InitConnection(wc) => on_init_connection(app, wc),
                AppEventClient::ChannelOpened(ddc) => on_channel_opened(app, ddc),
                AppEventClient::Connected => on_connected(app),
                AppEventClient::Disconnected => on_disconnected(app),
                AppEventClient::MessageReceived(message) => on_message_received(app, message),
                AppEventClient::ReportFileSpeed(report) => on_report_file_speed(app, report),
                AppEventClient::OutputFileProgress(progress) => {
                    on_file_progress(app, progress, true)
                }
                AppEventClient::InputFileProgress(progress) => {
                    on_file_progress(app, progress, false)
                }
                AppEventClient::OutputFileFinished(ddc) => on_file_finished(app, ddc),
                AppEventClient::InputFileNew(input_file) => on_input_file_new(app, input_file),
                AppEventClient::MetaSent(ddc) => on_meta_sent(app, ddc),
            }
        }

        Ok(())
    }
}

fn on_quit(app: &mut App) {
    app.exit = true;
}
fn on_update_handshake_state(app: &mut App, state: HandshakeState) {
    app.handshake_state = state;
}
fn on_manual_signaling_init(app: &mut App, polite: bool) {
    app.handshake_widget_state.polite = Some(polite);
}
// Manual signaling part, should be pretty foolproof
fn on_manual_signaling_input(app: &mut App, text: String) -> color_eyre::Result<()> {
    // If signaling is manual and the handshake wasn't provided yet
    if let Commands::Client(client_args) = &app.args.app_mode
        && let SignalingSolutions::Manual(args) = &client_args.signaling_mode
        && app.handshake_widget_state.input_text.is_empty()
        && let Some(tx) = &mut app.client_state.handshake_tx
    {
        // Validate it and send it to the signaling side
        let text =
            try_decrypt_claims(&text, &args.secret).wrap_err("Incorrect manual handshake")?;
        let result: Result<SignalingMessage, serde_json::Error> = serde_json::from_str(&text);

        // We can ignore errors here methinks, but maybe a toast notification would be nice
        if let Ok(msg) = result {
            let mut send_flag = false;
            if args.polite {
                if let SignalingMessage::Offer(_) = msg {
                    send_flag = true;
                }
            }
            // If polite it should be an offer
            else if let SignalingMessage::Answer(_) = msg {
                send_flag = true;
            } // If impolite it should be an answer

            if send_flag {
                app.handshake_widget_state.input_text = text;
                tx.send(msg).ok();
            }
        }
    }

    Ok(())
}
fn on_manual_signaling_output(app: &mut App, msg: String) {
    app.handshake_widget_state.output_text = msg;
}
fn on_init_connection(app: &mut App, wc: WebConnection) {
    app.client_state.wc = Some(wc);
}
fn on_channel_opened(app: &mut App, ddc: DebugDataChannel) {
    send_all_meta(app, ddc);
}
fn on_connected(app: &mut App) {
    log::info!("Connection established");
    app.client_state.connected = true;
}
fn on_disconnected(app: &mut App) {
    log::info!("Disconnected");
    app.client_state.connected = false;
}
fn on_message_received(app: &mut App, message: Message) {
    match message {
        Message::TextMessage(_) => {} // TODO: implement
        Message::FilePacketReceived(report) => {
            app.file_manager.add_output_report(report);
        }
        Message::FileReceived(id) => {
            app.file_manager.set_output_finished(id);
        }
    }
}
fn on_report_file_speed(app: &mut App, report: SpeedReport) {
    app.file_manager.add_input_report(report);
}
fn on_file_progress(app: &mut App, progress_report: FileProgressReport, output: bool) {
    if output {
        let output_file = app
            .file_manager
            .output_map
            .get_mut(&progress_report.file_id);
        if let Some(output_file) = output_file {
            output_file.progress = progress_report.progress;
        }
    } else {
        let input_file = app.file_manager.input_map.get_mut(&progress_report.file_id);
        if let Some(input_file) = input_file {
            input_file.progress = progress_report.progress;
        }
    }
}
fn on_file_finished(app: &mut App, ddc: DebugDataChannel) {
    send_next_file(app, ddc);
}
fn on_input_file_new(app: &mut App, input_file: InputFile) {
    app.file_manager.input_map.insert(input_file.id, input_file);
}
fn on_meta_sent(app: &mut App, ddc: DebugDataChannel) {
    send_next_file(app, ddc);
}

fn send_next_file(app: &mut App, ddc: DebugDataChannel) {
    let mut exit: bool = false;
    while !exit {
        if let Some(of) = app.file_manager.get_next_output_file() {
            if !of.meta.is_dir && of.meta.size > 0 {
                send_file_data(app, &ddc, &of);
                exit = true;
            }
        } else {
            exit = true;
        }
    }
}
fn send_file_data(app: &mut App, ddc: &DebugDataChannel, output_file: &OutputFile) {
    if let Commands::Client(client_args) = &app.args.app_mode
        && let Some(wc) = &app.client_state.wc
    {
        let maid = app.get_maid();
        let dc = ddc.dc.clone();

        let mut buffer_watch_rx = wc.buffer_watch_tx.subscribe();
        let output_file = output_file.clone();
        let chunk_size = client_args.chunk_size;

        tokio::spawn(async move {
            let token = maid.token.child_token();
            tokio::select! {
                _ = token.cancelled() => {},
                result = payload::send_file_data(
                    dc, &output_file, chunk_size, &mut buffer_watch_rx, Some(&maid.event_tx)
                ) => {
                    if let Err(err) = result { maid.error_tx.send_error(err); }
                }
            }
        });
    }
}
fn send_all_meta(app: &mut App, ddc: DebugDataChannel) {
    if let Commands::Client(client_args) = &app.args.app_mode
        && let Some(wc) = &app.client_state.wc
    {
        let maid = app.get_maid();
        let dc = ddc.dc.clone();

        let mut buffer_watch_rx = wc.buffer_watch_tx.subscribe();
        let output_files = app.file_manager.output_queue.clone();
        let chunk_size = client_args.chunk_size;

        tokio::spawn(async move {
            let token = maid.token.child_token();
            tokio::select! {
                _ = token.cancelled() => {},
                result = payload::send_all_meta(
                    dc, &output_files, chunk_size, &mut buffer_watch_rx, Some(&maid.event_tx)
                ) => {
                    if let Err(err) = result { maid.error_tx.send_error(err); }
                },
            }
        });
    }
}
