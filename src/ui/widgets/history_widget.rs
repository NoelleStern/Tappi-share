use crossterm::event::{KeyCode, KeyEvent};
use rat_focus::{FocusBuilder, FocusFlag, HasFocus};
use ratatui::symbols::border;
use ratatui::{prelude::*, widgets::*};
use ratatui_macros::line;
use tui_scrollview::{ScrollView, ScrollViewState};

use crate::app::app_event::AppEvent;
use crate::app::app_main::App;
use crate::app::models::SyncRoom;
use crate::server::types::UserMessage;
use crate::ui::theme::Theme;
use crate::ui::utils::{
    BlockDefault, BlockExt, CollapsedBorder, CombinedWidgetState, Shortcut, StringExt,
};

#[derive(Default)]
pub struct HistoryWidgetState {
    pub area: Rect, // Should get updated when it renders
    pub focus: FocusFlag,
    pub scroll_view_state: ScrollViewState,
}
impl HasFocus for HistoryWidgetState {
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
impl CombinedWidgetState for HistoryWidgetState {
    fn get_shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut {
                description: "Top".to_string(),
                button: "g".to_string(),
            },
            Shortcut {
                description: "Bottom".to_string(),
                button: "G".to_string(),
            },
            Shortcut {
                description: "Down".to_string(),
                button: "j".to_string(),
            },
            Shortcut {
                description: "Up".to_string(),
                button: "k".to_string(),
            },
        ]
    }
    fn handle_key_events(&mut self, key_event: &KeyEvent) -> color_eyre::Result<AppEvent> {
        let result: AppEvent = AppEvent::None;

        if key_event.is_release() {
            match key_event.code {
                KeyCode::Char('g') | KeyCode::Home => {
                    self.scroll_view_state.scroll_to_top();
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.scroll_view_state.scroll_to_bottom();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.scroll_view_state.scroll_down();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll_view_state.scroll_up();
                }
                _ => {}
            }
        }

        Ok(result)
    }
}

// Rebuild it on the fly for simplicity
struct HistoryWidget<'a> {
    theme: &'a Theme,
    title: Option<String>,
    borders: Borders,
    border_set: symbols::border::Set,
    history: Option<&'a Vec<UserMessage>>,
}
impl<'a> HistoryWidget<'a> {
    fn new(
        theme: &'a Theme,
        title: Option<String>,
        borders: Borders,
        border_set: symbols::border::Set,
        history: Option<&'a Vec<UserMessage>>,
    ) -> Self {
        Self {
            theme,
            title,
            borders,
            border_set,
            history,
        }
    }
}
impl<'a> StatefulWidget for HistoryWidget<'a> {
    type State = HistoryWidgetState;

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
        let mut total_height: usize = 0;
        let inner = block.inner_with_margin(area, 0, 1);
        block.render(area, buf); // Render first because otherwise colors get discarded

        if let Some(users) = self.history {
            let width: u16 = inner.width - 2; // 1 for scrollbar + 1 for margin
            let mut layout_constraints: Vec<Constraint> = vec![];
            let items: Vec<Paragraph> = users
                .iter()
                .enumerate()
                .map(|(i, msg)| {
                    let text = format!("({}): {}", msg.user_id, msg.msg);
                    let wrapped_text = textwrap::wrap(&text, width as usize);
                    let height = wrapped_text.len();

                    let mut item = Paragraph::new(
                        wrapped_text
                            .iter()
                            .map(|f| line!(f.to_string()))
                            .collect::<Vec<Line>>(),
                    );

                    if i % 2 == 0 {
                        item = item.bg(self.theme.surface2.clone());
                    } else {
                        item = item.bg(self.theme.surface1.clone());
                    }

                    layout_constraints.push(Constraint::Length(height as u16));
                    total_height += height;
                    item
                })
                .collect();

            let mut scroll_view = ScrollView::new(Size::new(width, total_height as u16));
            let layout_vertical =
                Layout::vertical(layout_constraints).split(scroll_view.buf().area);
            for (i, item) in items.iter().enumerate() {
                item.render(layout_vertical[i], scroll_view.buf_mut());
            }
            scroll_view.render(inner, buf, &mut state.scroll_view_state);
        }
    }
}

pub fn history_widget(app: &mut App, area: Rect, buf: &mut Buffer, builder: &mut FocusBuilder) {
    let room: Option<&SyncRoom> = app.room_list_widget_state.get_selected();
    let mut history: Option<&Vec<UserMessage>> = None;

    if let Some(room) = room {
        history = Some(&room.history);
    }

    let history_widget = HistoryWidget::new(
        &app.theme,
        Some("Room message history".to_string()),
        CollapsedBorder::all(),
        border::PLAIN,
        history,
    );

    // Render
    history_widget.render(area, buf, &mut app.history_widget_state);

    // Build focus
    app.history_widget_state.build(builder);
}
