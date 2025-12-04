use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexMap;
use rat_focus::{FocusBuilder, FocusFlag, HasFocus};
use ratatui::symbols::border;
use ratatui::{prelude::*, widgets::*};
use ratatui_macros::line;

use crate::app::app_event::AppEvent;
use crate::app::app_main::App;
use crate::app::models::SyncRoom;
use crate::server::types::{RoomUser, UserId};
use crate::ui::theme::Theme;
use crate::ui::utils::{
    BlockDefault, BlockExt, CollapsedBorder, CombinedWidgetState, Shortcut, StringExt,
};

#[derive(Default)]
pub struct UserListWidgetState {
    pub area: Rect, // Should get updated when it renders
    pub focus: FocusFlag,
    pub list_state: ListState,
}
impl UserListWidgetState {
    pub fn get_selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }
}
impl HasFocus for UserListWidgetState {
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
impl CombinedWidgetState for UserListWidgetState {
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
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.list_state.select_last();
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.list_state.select(None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.list_state.select_next();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.list_state.select_previous();
                }
                _ => {}
            }
        }

        Ok(result)
    }
}

// Rebuild it on the fly for simplicity
struct UserListWidget<'a> {
    theme: &'a Theme,
    title: Option<String>,
    borders: Borders,
    border_set: symbols::border::Set,
    users: Option<&'a IndexMap<UserId, RoomUser>>,
}
impl<'a> UserListWidget<'a> {
    fn new(
        theme: &'a Theme,
        title: Option<String>,
        borders: Borders,
        border_set: symbols::border::Set,
        users: Option<&'a IndexMap<UserId, RoomUser>>,
    ) -> Self {
        Self {
            theme,
            title,
            borders,
            border_set,
            users,
        }
    }
}
impl<'a> StatefulWidget for UserListWidget<'a> {
    type State = UserListWidgetState;

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
        let inner = block.inner_with_margin(area, 0, 1);
        block.render(area, buf); // Render first because otherwise colors get discarded
        if let Some(users) = self.users {
            let items: Vec<ListItem> = users
                .iter()
                .enumerate()
                .map(|(i, (_user_id, user))| {
                    let mut item =
                        ListItem::from(line!(format!("{}: {}", i + 1, user.name_with_id())));

                    if let Some(selected) = state.list_state.selected()
                        && state.is_focused()
                        && i == selected
                    {
                        item = item.fg(self.theme.info.clone());
                    }

                    item
                })
                .collect();

            let list = List::new(items);

            StatefulWidget::render(list, inner, buf, &mut state.list_state);
        }
    }
}

pub fn users_widget(app: &mut App, area: Rect, buf: &mut Buffer, builder: &mut FocusBuilder) {
    let room: Option<&SyncRoom> = app.room_list_widget_state.get_selected();
    let mut users: Option<&IndexMap<UserId, RoomUser>> = None;

    if let Some(room) = room {
        users = Some(&room.users);
    }

    let user_list = UserListWidget::new(
        &app.theme,
        Some("List of users".to_string()),
        CollapsedBorder::all(),
        border::PLAIN,
        users,
    );

    // Render
    user_list.render(area, buf, &mut app.user_list_widget_state);

    // Build focus
    app.user_list_widget_state.build(builder);
}
