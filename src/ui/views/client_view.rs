use rat_focus::FocusBuilder;
use ratatui::prelude::*;
use ratatui_macros::{line, vertical};

use crate::app::app_main::App;
use crate::cli::{Commands, SignalingSolutions};
use crate::ui::utils::{MainFrame, Shortcut, ShortcutStyle};
use crate::ui::widgets::files_widget::files_widget;
use crate::ui::widgets::manual_handshake_widget::manual_handshake_widget;
use crate::ui::widgets::server_handshake_widget::server_handshake_widget;

const TITLE: &str = "tappi-share client";

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let mut manual_flag = false;
    if let Commands::Client(client_args) = &app.args.app_mode
        && let SignalingSolutions::Manual(_args) = &client_args.signaling_mode
    {
        manual_flag = true
    }

    let instructions = ShortcutStyle::new(&app.theme)
        .shortcut_line(vec![Shortcut::new("Quit".to_string(), "q".to_string())])
        .left_aligned();

    // Main frame
    let mut main_frame = MainFrame::create(&app.theme, area, TITLE);
    main_frame.block = main_frame.block.title_bottom(instructions);
    main_frame.block = main_frame.block.title(status_line(app).right_aligned());
    main_frame.block = Shortcut::add_shortcut_bottom_title(
        &app.theme,
        app.widget_shortcuts.clone(),
        main_frame.block,
    );

    // Render
    let mut builder = FocusBuilder::default(); // Init focus builder
    main_frame.render(area, buf);

    let vertical_layout = vertical![==4, *=1].spacing(1);
    let inner_areas: [Rect; 2] = vertical_layout.areas(main_frame.inner);

    if manual_flag {
        manual_handshake_widget(app, inner_areas[0], buf, &mut builder);
        files_widget(app, inner_areas[1], buf, &mut builder);
    } else {
        server_handshake_widget(app, inner_areas[0], buf);
        files_widget(app, inner_areas[1], buf, &mut builder);
    }

    app.focus = builder.build(); // Build
}

fn status_line<'a>(app: &mut App) -> Line<'a> {
    line!(
        " ",
        "connected: ".fg(app.theme.text.clone()),
        format!("{:5}", app.client_state.connected).fg(if app.client_state.connected {
            app.theme.success.clone()
        } else {
            app.theme.error.clone()
        }),
        " ",
    )
}
