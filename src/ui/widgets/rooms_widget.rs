use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexMap;
use rat_focus::{FocusBuilder, FocusFlag, HasFocus};
use ratatui::symbols::border;
use ratatui::{prelude::*, widgets::*};
use ratatui_macros::line;

use crate::app::app_event::AppEvent;
use crate::app::app_main::App;
use crate::app::models::SyncRoom;
use crate::server::types::RoomId;
use crate::ui::theme::Theme;
use crate::ui::utils::{
    BlockDefault, BlockExt, CollapsedBorder, CombinedWidgetState, ScrollbarStateExt, Shortcut,
    StringExt,
};

type SyncRooms = IndexMap<RoomId, SyncRoom>;
#[derive(Default)]
pub struct RoomListWidgetState {
    pub area: Rect, // Should get updated when it renders
    pub focus: FocusFlag,
    pub list_state: ListState,
    pub scrollbar_state: ScrollbarState,
    pub rooms: SyncRooms,
}
impl RoomListWidgetState {
    pub fn get_selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }
    pub fn get_selected_id(&self) -> Option<&RoomId> {
        if let Some(i) = self.get_selected_index() {
            let keys: Vec<&RoomId> = self.rooms.keys().collect();
            Some(keys[i])
        } else {
            None
        }
    }
    pub fn get_selected(&self) -> Option<&SyncRoom> {
        if let Some(i) = self.get_selected_index() {
            let keys: Vec<&RoomId> = self.rooms.keys().collect();
            self.rooms.get(keys[i])
        } else {
            None
        }
    }
}
impl HasFocus for RoomListWidgetState {
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
impl CombinedWidgetState for RoomListWidgetState {
    fn get_shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut {
                description: "First".to_string(),
                button: "g".to_string(),
            },
            Shortcut {
                description: "Last".to_string(),
                button: "G".to_string(),
            },
            Shortcut {
                description: "None".to_string(),
                button: "h".to_string(),
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
                    self.list_state.select_first();
                    self.scrollbar_state.match_list_state(&self.list_state);
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.list_state.select_last();
                    self.scrollbar_state.match_list_state(&self.list_state);
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.list_state.select(None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.list_state.select_next();
                    self.scrollbar_state.match_list_state(&self.list_state);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.list_state.select_previous();
                    self.scrollbar_state.match_list_state(&self.list_state);
                }
                _ => {}
            }
        }

        Ok(result)
    }
}

// Rebuild it on the fly for simplicity
struct RoomListWidget<'a> {
    theme: &'a Theme,
    title: Option<String>,
    borders: Borders,
    border_set: symbols::border::Set,
}
impl<'a> RoomListWidget<'a> {
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
impl<'a> StatefulWidget for RoomListWidget<'a> {
    type State = RoomListWidgetState;

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

        let items: Vec<ListItem> = state
            .rooms
            .iter()
            .map(|(room_id, _room)| ListItem::from(line!(room_id.clone())))
            .collect();

        let list = List::new(items)
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);

        // Render
        let inner = block.inner_with_margin(area, 0, 1);
        block.render(area, buf); // Render first because otherwise colors get discarded
        state
            .scrollbar_state
            .render_list(list, &mut state.list_state, inner, buf);
    }
}

pub fn rooms_widget(app: &mut App, area: Rect, buf: &mut Buffer, builder: &mut FocusBuilder) {
    let room_list = RoomListWidget::new(
        &app.theme,
        Some("List of rooms".to_string()),
        CollapsedBorder::all(),
        border::PLAIN,
    );

    // Render
    room_list.render(area, buf, &mut app.room_list_widget_state);

    // Build focus
    app.room_list_widget_state.build(builder);
}
