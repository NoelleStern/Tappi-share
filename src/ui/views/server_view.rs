use rat_focus::FocusBuilder;
use ratatui::prelude::*;
use ratatui_macros::{horizontal, vertical};

use crate::app::app_main::App;
use crate::ui::utils::{BlockDefault, MainFrame, Shortcut, ShortcutStyle};
use crate::ui::widgets::history_widget::history_widget;
use crate::ui::widgets::rooms_widget::rooms_widget;
use crate::ui::widgets::users_widget::users_widget;

const TITLE: &str = "tappi-share server";

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let instructions = ShortcutStyle::new(&app.theme)
        .shortcut_line(vec![Shortcut::new("Quit".to_string(), "q".to_string())])
        .left_aligned();

    // Main frame
    let mut main_frame = MainFrame::create(&app.theme, area, TITLE);
    main_frame.block = main_frame.block.title_bottom(instructions);
    main_frame.block = Shortcut::add_shortcut_bottom_title(
        &app.theme,
        app.widget_shortcuts.clone(),
        main_frame.block,
    );

    // Main layout
    let block = BlockDefault::window(&app.theme, None, false);
    let horizontal_layout = horizontal![*=1, *=4];
    let areas: [Rect; 2] = horizontal_layout.areas(block.inner(main_frame.inner));

    // Render
    let mut builder = FocusBuilder::default(); // Init focus builder
    main_frame.render(area, buf);
    block.render(main_frame.inner, buf);
    rooms_widget(app, areas[0], buf, &mut builder);
    render_room_info(app, areas[1], buf, &mut builder);
    app.focus = builder.build(); // Build
}

pub fn render_room_info(app: &mut App, area: Rect, buf: &mut Buffer, builder: &mut FocusBuilder) {
    let vertical_layout = vertical![*=1, *=5];
    let areas: [Rect; 2] = vertical_layout.areas(area);

    users_widget(app, areas[0], buf, builder);
    history_widget(app, areas[1], buf, builder);
}
