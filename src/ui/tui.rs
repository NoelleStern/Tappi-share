use ratatui::prelude::*;

use crate::app::app_main::App;
use crate::cli::Commands;
use crate::ui::views::client_view::render as render_client;
use crate::ui::views::error_view::render as render_error;
use crate::ui::views::server_view::render as render_server;

// A MESSAGE TO THAT SILLY PERSON CALLED "ME": ALWAYS RENDER FROM OUTER TO INNER!

impl Widget for &mut App {
    /// Renders the user interface widgets.
    ///
    // This is where you add new widgets.
    // See the following resources:
    // - https://docs.rs/ratatui/latest/ratatui/widgets/index.html
    // - https://github.com/ratatui/ratatui/tree/master/examples
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.exit {
            match self.args.app_mode {
                Commands::Client(_) => {
                    render_client(self, area, buf);
                }
                Commands::Server(_) => {
                    render_server(self, area, buf);
                }
            }
        } else {
            render_error(self, area, buf);
        }

        self.redraw = false;
    }
}
