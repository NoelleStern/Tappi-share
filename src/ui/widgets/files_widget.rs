use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexMap;
use rat_focus::{FocusBuilder, FocusFlag, HasFocus};
use ratatui::{prelude::*, widgets::*};
use ratatui::{style::Style, symbols::border};
use ratatui_macros::horizontal;
use ratatui_macros::line;
use tui_widget_list::{ListBuilder, ListState as WidgetListState, ListView};

use crate::app::app_event::AppEvent;
use crate::app::app_main::App;
use crate::app::file_manager::{FileId, FileManager, ProgressFile};
use crate::ui::theme::Theme;
use crate::ui::utils::{
    BlockDefault, CollapsedBorder, CombinedWidgetState, ScrollbarStateExt, Shortcut, StringExt,
    WidgetListStateExt,
};

const CHECK_MARK: &str = "[✓]";

#[derive(Default)]
pub struct FileListWidgetState {
    pub area: Rect, // Should get updated when it renders
    pub focus: FocusFlag,
    pub list_state: WidgetListState,
    pub scrollbar_state: ScrollbarState,
}
impl HasFocus for FileListWidgetState {
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
impl CombinedWidgetState for FileListWidgetState {
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
                    self.list_state.first();
                    self.scrollbar_state
                        .match_widget_list_state(&self.list_state);
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.list_state.last();
                    self.scrollbar_state
                        .match_widget_list_state(&self.list_state);
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.list_state.select(None);
                    self.scrollbar_state
                        .match_widget_list_state(&self.list_state);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.list_state.next();
                    self.scrollbar_state
                        .match_widget_list_state(&self.list_state);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.list_state.previous();
                    self.scrollbar_state
                        .match_widget_list_state(&self.list_state);
                }
                _ => {}
            }
        }

        Ok(result)
    }
}

// Rebuild it on the fly for simplicity
struct FileListWidget<'a, V: ProgressFile> {
    theme: &'a Theme,
    title: Option<String>,
    borders: Borders,
    border_set: symbols::border::Set,
    files: &'a IndexMap<&'a FileId, &'a V>,
    speed: f64,
    estimate: f64,
    completed: bool,
}
impl<'a, V: ProgressFile> FileListWidget<'a, V> {
    #[allow(clippy::too_many_arguments)] // TODO: investigate
    fn new(
        theme: &'a Theme,
        title: Option<String>,
        borders: Borders,
        border_set: symbols::border::Set,
        files: &'a IndexMap<&'a FileId, &V>,
        speed: f64,
        estimate: f64,
        completed: bool,
    ) -> Self {
        Self {
            theme,
            title,
            borders,
            border_set,
            files,
            speed,
            estimate,
            completed,
        }
    }
}
impl<'a, V: ProgressFile> StatefulWidget for FileListWidget<'a, V> {
    type State = FileListWidgetState;

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

        // Add speed estimate
        if self.speed > 0.0 {
            block = block
                .title_bottom(line!(format_speed_estimate(self.speed, self.estimate)).centered());
        }

        // Add check mark
        if self.completed {
            block = block.title_bottom(line!(CHECK_MARK).right_aligned());
        }

        // Render
        let selected = if state.is_focused() {
            state.list_state.selected
        } else {
            None
        };
        let file_list_view = file_list_widget(self.theme, self.files, selected, None);

        let size = self.files.len();
        let length = (size as u16) * 3;
        let inner = block.inner(area);

        block.render(area, buf);
        state.scrollbar_state.render_widget_list(
            file_list_view,
            &mut state.list_state,
            size,
            length,
            inner,
            buf,
        );
    }
}

pub fn files_widget(app: &mut App, area: Rect, buf: &mut Buffer, builder: &mut FocusBuilder) {
    // Compose layout
    let containing_block = BlockDefault::window(&app.theme, None, false);
    let layout = horizontal![==50%, ==50%];
    let a: [Rect; 2] = layout.areas(containing_block.inner(area));

    // File lists init
    let input_speed = FileManager::get_average_speed(&app.file_manager.input_map);
    let input_estimate = FileManager::get_estimate(&app.file_manager.input_map);
    let input_completed = FileManager::get_completion(&app.file_manager.input_map);

    let output_speed = FileManager::get_average_speed(&app.file_manager.output_map);
    let output_estimate = FileManager::get_estimate(&app.file_manager.output_map);
    let output_completed = FileManager::get_completion(&app.file_manager.output_map);

    let input_files = app.file_manager.get_input_map();
    let input_list = FileListWidget::new(
        &app.theme,
        Some("Incoming files".to_string()),
        CollapsedBorder::all(),
        border::PLAIN,
        &input_files,
        input_speed,
        input_estimate,
        input_completed,
    );
    let output_files = app.file_manager.get_output_map_no_dir();
    let output_list = FileListWidget::new(
        &app.theme,
        Some("Outgoing files".to_string()),
        CollapsedBorder::all(),
        border::PLAIN,
        &output_files,
        output_speed,
        output_estimate,
        output_completed,
    );

    // Render
    containing_block.render(area, buf); // Render first because otherwise colors get discarded
    input_list.render(a[0], buf, &mut app.input_list_widget_state);
    output_list.render(a[1], buf, &mut app.output_list_widget_state);

    // Build focus
    app.input_list_widget_state.build(builder);
    app.output_list_widget_state.build(builder);
}

fn file_list_widget<'a, K, V>(
    theme: &'a Theme,
    files: &'a IndexMap<&K, &V>,
    selected: Option<usize>,
    bg_color: Option<Color>,
) -> ListView<'a, Gauge<'a>>
where
    K: std::hash::Hash + Eq,
    V: ProgressFile,
{
    // Dang, this crate is clean
    let keys = files.keys();
    let builder = ListBuilder::new(move |lbc| {
        let selected = if let Some(s) = selected {
            lbc.index == s
        } else {
            false
        };

        let fg_color = if selected {
            theme.info.clone().into()
        } else {
            Color::White
        };

        let key = keys[lbc.index];
        let file = files[key]; // Should be fine
        let gauge = progress_gauge(theme, file, fg_color, bg_color);

        (gauge, 3)
    });

    ListView::new(builder, files.len())
}

fn progress_gauge<'a, F: ProgressFile>(
    theme: &Theme,
    file: &'a F,
    fg_color: Color,
    bg_color: Option<Color>,
) -> Gauge<'a> {
    let mut block = Block::bordered()
        .border_set(border::PLAIN)
        .bg(bg_color.unwrap_or(theme.surface1.clone().into())) // Hack to bypass the black background bug
        .fg(fg_color);

    // Add name
    if let Some(name) = file.get_name() {
        block = block.title(format!("[{name}]"));
    }

    // Add check mark
    block = if !file.get_finished() {
        block
    } else {
        block.title(line!(CHECK_MARK).right_aligned())
    };

    // Add speed
    if file.get_progress() > 0.0 {
        block = if file.get_finished() {
            block
        } else {
            block.title_bottom(line!(format_speed(file.get_speed())).right_aligned())
        };
    }

    // Set gauge style
    let gauge_style = if file.get_progress() >= 1.0 {
        Style::default()
            .fg(theme.success.clone().into())
            .add_modifier(Modifier::BOLD) // BG doesn't matter
    } else {
        Style::default()
            .bg(theme.surface2.clone().into())
            .fg(theme.warning.clone().into()) // BG matters
    };

    // Assemble
    Gauge::default()
        .gauge_style(gauge_style)
        .ratio(file.get_progress())
        .block(block)
        .fg(theme.text.clone())
}

fn format_speed(speed: f64) -> String {
    format!("[{:.1} Mbps]", speed)
}
fn format_speed_estimate(speed: f64, estimate: f64) -> String {
    format!(
        "[{:.1} Mbps, ETA: {}]",
        speed,
        seconds_to_hms(estimate as u64)
    )
}
fn seconds_to_hms(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}
