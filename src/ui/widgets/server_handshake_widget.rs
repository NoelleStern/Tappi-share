use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};
use ratatui_macros::line;

use crate::{
    app::app_main::App,
    cli::{Commands, SignalingSolutions},
    ui::utils::{BlockDefault, BlockExt, StringExt},
};

pub fn server_handshake_widget(app: &mut App, area: Rect, buf: &mut Buffer) {
    if let Commands::Client(client_args) = &app.args.app_mode {
        let line = match &client_args.signaling_mode {
            SignalingSolutions::Socket(args) => {
                line!(format!("{} ({}:{})", args.room, args.address, args.port))
            }
            SignalingSolutions::Mqtt(args) => {
                line!(format!(
                    "Local: {} Remote: {}",
                    args.local_name, args.remote_name
                ))
            }
            _ => {
                line!("")
            }
        };

        let window_block = BlockDefault::window(&app.theme, None, false);
        let block = BlockDefault::bordered(&app.theme).title("Signaling status".spaced());

        let paragraph = Paragraph::new(vec![
            line,
            line!(format!("Status: {:?}", app.handshake_state)),
        ]);

        let block_area = window_block.inner(area);
        let paragraph_area: Rect = block.inner_with_margin(block_area, 0, 1);
        window_block.render(area, buf);
        block.render(block_area, buf);
        paragraph.render(paragraph_area, buf);
    }
}
