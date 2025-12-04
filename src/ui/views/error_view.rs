use ansi_to_tui::IntoText;
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::app_main::App;
use crate::ui::utils::{Ansi, BlockDefault, BlockExt, MainFrame, Shortcut, ShortcutStyle};

const TITLE: &str = "tappi ERROR";

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
    let block = BlockDefault::window(&app.theme, None, true);

    let mut full_report: String = "".to_string();
    if let Some(error) = &app.error {
        full_report = format!("Error: {:?}", error);
    }
    let mut paragraph = Paragraph::new("");
    if !full_report.is_empty() {
        let full_report = Ansi::replace_colors(&app.theme, &full_report);
        let colored = full_report.as_bytes().into_text();

        if let Ok(colored) = colored {
            paragraph = Paragraph::new(colored).wrap(Wrap::default());
        }
    }

    // Render
    let inner = block.inner_with_margin(main_frame.inner, 0, 1);
    main_frame.render(area, buf);
    block.render(main_frame.inner, buf);
    paragraph.render(inner, buf);
}
