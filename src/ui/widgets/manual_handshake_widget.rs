use arboard::Clipboard;
use crossterm::event::{KeyCode, KeyEvent};
use rat_focus::{FocusBuilder, FocusFlag, HasFocus};
use ratatui::symbols::border;
use ratatui::{prelude::*, widgets::*};
use ratatui_macros::line;

use crate::app::app_event::{AppEvent, AppEventClient};
use crate::app::app_main::App;
use crate::ui::theme::Theme;
use crate::ui::utils::{
    BlockDefault, BlockExt, CollapsedBorder, CombinedWidgetState, Shortcut, StringExt,
};

#[derive(Default)]
pub struct ManualHandshakeWidgetState {
    pub area: Rect, // Should get updated when it renders
    pub focus: FocusFlag,
    pub input_text: String,
    pub output_text: String,
    pub polite: Option<bool>,
}
impl ManualHandshakeWidgetState {
    fn copy(&self) -> color_eyre::Result<()> {
        let output = &self.output_text;
        let mut clipboard = Clipboard::new()?;
        if !output.is_empty() {
            clipboard.set_text(output)?;
        }
        Ok(())
    }
    fn get_clipboard_text(&self) -> color_eyre::Result<String> {
        let mut clipboard = Clipboard::new()?;
        let text = clipboard.get_text()?;
        Ok(text)
    }
}
impl HasFocus for ManualHandshakeWidgetState {
    fn area(&self) -> Rect {
        self.area
    }
    fn build(&self, builder: &mut FocusBuilder) {
        builder.leaf_widget(self);
    }
    fn focus(&self) -> FocusFlag {
        self.focus.clone()
    }
}
impl CombinedWidgetState for ManualHandshakeWidgetState {
    fn get_shortcuts(&self) -> Vec<Shortcut> {
        let mut result = vec![];

        if let Some(polite) = self.polite {
            if polite {
                if !self.output_text.is_empty() {
                    result.push(Shortcut {
                        description: "Copy".to_string(),
                        button: "c".to_string(),
                    });
                } else if self.input_text.is_empty() {
                    result.push(Shortcut {
                        description: "Paste".to_string(),
                        button: "v".to_string(),
                    });
                }
            } else {
                result.push(Shortcut {
                    description: "Copy".to_string(),
                    button: "c".to_string(),
                });
                if self.input_text.is_empty() {
                    result.push(Shortcut {
                        description: "Paste".to_string(),
                        button: "v".to_string(),
                    })
                };
            }
        }

        result
    }
    fn handle_key_events(&mut self, key_event: &KeyEvent) -> color_eyre::Result<AppEvent> {
        let mut result: AppEvent = AppEvent::None;

        if key_event.is_release() {
            result = match key_event.code {
                KeyCode::Char('c') => {
                    self.copy()?;
                    AppEvent::None
                }
                KeyCode::Char('v') => {
                    AppEventClient::ManualSignalingInput(self.get_clipboard_text()?).into()
                }
                _ => AppEvent::None,
            }
        }

        Ok(result)
    }
}

// Rebuild it on the fly for simplicity
struct ManualHandshakeWidget<'a> {
    theme: &'a Theme,
    title: Option<String>,
    borders: Borders,
    border_set: symbols::border::Set,
}
impl<'a> ManualHandshakeWidget<'a> {
    fn new(
        theme: &'a Theme,
        title: Option<String>,
        borders: Borders,
        border_set: symbols::border::Set,
    ) -> Self {
        Self {
            theme,
            title,
            borders,
            border_set,
        }
    }
}
impl<'a> StatefulWidget for ManualHandshakeWidget<'a> {
    type State = ManualHandshakeWidgetState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        state.area = area; // Set the area

        // Create a block
        let mut block = BlockDefault::plain(self.theme)
            .borders(self.borders)
            .border_set(self.border_set);

        // Add title
        if let Some(widget_title) = &self.title {
            block = block.title(widget_title.spaced());
        }

        // Set focus style
        if state.is_focused() {
            block = BlockDefault::focus_style_block(&block);
        }

        // Render
        let input_text = character_of_size('*', state.input_text.len());
        let output_text = character_of_size('*', state.output_text.len());

        let inner = block.inner_with_margin(area, 0, 1);
        Paragraph::new(vec![
            line!(format!("Input: {}", input_text)),
            line!(format!("Output: {}", output_text)),
        ])
        .fg(self.theme.text.clone())
        .render(inner, buf);

        block.render(area, buf);
    }
}

fn character_of_size(character: char, len: usize) -> String {
    std::iter::repeat_n(character, len).collect::<String>()
}

pub fn manual_handshake_widget(
    app: &mut App,
    area: Rect,
    buf: &mut Buffer,
    builder: &mut FocusBuilder,
) {
    let block = BlockDefault::window(&app.theme, None, false);

    let handshake_widget = ManualHandshakeWidget::new(
        &app.theme,
        Some("Handshake".to_string()),
        CollapsedBorder::all(),
        border::PLAIN,
    );

    // Render
    let inner = block.inner(area);
    block.render(area, buf);
    handshake_widget.render(inner, buf, &mut app.handshake_widget_state);

    // Build focus
    app.handshake_widget_state.build(builder);
}
