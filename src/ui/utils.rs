use crossterm::event::KeyEvent;
use rat_focus::HasFocus;
use ratatui::prelude::*;
use ratatui::symbols::border;
use ratatui::text::Line;
use ratatui::widgets::*;
use ratatui_macros::horizontal;
use regex::Regex;
use tachyonfx::ToRgbComponents;
use tui_widget_list::{ListState as WidgetListState, ListView};

use crate::app::app_event::AppEvent;
use crate::ui::theme::Theme;

pub struct MainFrame<'a> {
    pub block: Block<'a>,
    pub inner: Rect,
}
impl<'a> MainFrame<'a> {
    pub fn new(block: Block<'a>, inner: Rect) -> Self {
        Self { block, inner }
    }
    pub fn create(theme: &Theme, area: Rect, frame_title: &str) -> Self {
        let block = Block::bordered()
            .title(frame_title.spaced())
            .border_set(border::PLAIN)
            .bg(theme.surface0.clone())
            .fg(theme.accent.clone());

        let inner = block.inner_with_margin(area, 1, 2);
        MainFrame::new(block, inner)
    }
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.block.clone().render(area, buf);
    }
}

pub struct BlockDefault;
impl BlockDefault {
    pub fn plain<'a>(theme: &'a Theme) -> Block<'a> {
        Block::default()
            .fg(theme.text.clone())
            .border_style(Style::default().fg(theme.primary.clone().into()))
    }
    pub fn bordered<'a>(theme: &'a Theme) -> Block<'a> {
        Block::bordered()
            .fg(theme.text.clone())
            .border_style(Style::default().fg(theme.primary.clone().into()))
    }
    pub fn window<'a>(theme: &'a Theme, title: Option<&str>, bordered: bool) -> Block<'a> {
        let mut block = Self::plain(theme).bg(theme.surface1.clone());

        if let Some(window_title) = title {
            block = block.title(window_title.spaced());
        }

        if bordered {
            block = block.border_set(border::PLAIN).borders(Borders::ALL)
        }

        block
    }
    pub fn focus_style_block<'a>(block: &Block<'a>) -> Block<'a> {
        block.clone().border_set(border::THICK)
    }
}

#[derive(Clone)]
pub struct Shortcut {
    pub description: String,
    pub button: String,
}
impl Shortcut {
    pub fn new(description: String, button: String) -> Self {
        Self {
            description,
            button,
        }
    }
    pub fn shortcut_line<'a>(shortcuts: Vec<Shortcut>, shortcut_style: ShortcutStyle) -> Line<'a> {
        let mut spans: Vec<Span> = vec![];
        let bold_modifier: Modifier = if shortcut_style.bold {
            Modifier::BOLD
        } else {
            Modifier::default()
        };

        // Beginning separator
        spans.push(" ".into());
        for s in shortcuts.iter() {
            // Shortcut description
            spans.push(s.description.to_string().fg(shortcut_style.base_color));

            // Middle separator
            spans.push(" ".into());

            // Shortcut key
            spans.push("(".fg(shortcut_style.base_color));
            spans.push(
                s.button
                    .to_string()
                    .fg(shortcut_style.button_color)
                    .add_modifier(bold_modifier),
            );
            spans.push(")".fg(shortcut_style.base_color));
            spans.push(" ".into());
            // End separator
        }

        Line::from(spans) // line! macros doesn't work here for some reason
    }
    pub fn add_shortcut_bottom_title<'a>(
        theme: &'a Theme,
        widget_shortcuts: Vec<Shortcut>,
        block: Block<'a>,
    ) -> Block<'a> {
        if !widget_shortcuts.is_empty() {
            block.title_bottom(
                ShortcutStyle::new(theme)
                    .shortcut_line(widget_shortcuts.clone())
                    .right_aligned(),
            )
        } else {
            block
        }
    }
}
pub struct ShortcutStyle {
    base_color: Color,
    button_color: Color,
    bold: bool,
}
impl ShortcutStyle {
    pub fn new(theme: &Theme) -> Self {
        Self {
            base_color: Color::White,
            button_color: theme.info.clone().into(),
            bold: false,
        }
    }
    pub fn shortcut_line<'a>(self, shortcuts: Vec<Shortcut>) -> Line<'a> {
        Shortcut::shortcut_line(shortcuts, self)
    }
}

pub trait CombinedWidgetState: HasFocus {
    fn get_shortcuts(&self) -> Vec<Shortcut> {
        vec![]
    }
    fn handle_key_events(&mut self, _key_event: &KeyEvent) -> color_eyre::Result<AppEvent> {
        Ok(AppEvent::None)
    }
}

pub trait StringExt {
    fn spaced(&self) -> String;
}
impl StringExt for &String {
    fn spaced(&self) -> String {
        format!(" {self} ")
    }
}
impl StringExt for &str {
    fn spaced(&self) -> String {
        format!(" {self} ")
    }
}

pub struct Ansi;
impl Ansi {
    pub fn replace_colors(theme: &Theme, string: &str) -> String {
        let string = Self::ansi_replace_color(string, r"\x1b\[0m", "\x1b[97m"); // Replace reset with white
        let string = Self::ansi_replace_color_rgb(
            &string,
            r"\x1b\[91m",
            Into::<Color>::into(theme.error.clone()).to_rgb(),
        ); // Replace red with red
        Self::ansi_replace_color_rgb(
            &string,
            r"\x1b\[35m",
            Into::<Color>::into(theme.primary.clone()).to_rgb(),
        )
    }
    pub fn ansi_replace_color(string: &str, regex: &str, replace: &str) -> String {
        let re = Regex::new(regex);
        if let Ok(re) = re {
            re.replace_all(string, replace).to_string()
        } else {
            string.to_string()
        }
    }
    pub fn ansi_replace_color_rgb(string: &str, regex: &str, rgb: (u8, u8, u8)) -> String {
        let rgb_string = format!("\x1b[38;2;{};{};{}m", rgb.0, rgb.1, rgb.2);
        let re = Regex::new(regex);
        if let Ok(re) = re {
            re.replace_all(string, rgb_string).to_string()
        } else {
            string.to_string()
        }
    }
}

pub trait BlockExt {
    fn inner_with_margin(&self, area: Rect, vertical: u16, horizontal: u16) -> Rect;
}
impl BlockExt for Block<'_> {
    fn inner_with_margin(&self, area: Rect, vertical: u16, horizontal: u16) -> Rect {
        self.inner(area).inner(Margin {
            vertical,
            horizontal,
        })
    }
}

pub trait ScrollbarStateExt {
    fn match_list_state(&mut self, list_state: &ListState);
    fn match_widget_list_state(&mut self, list_state: &WidgetListState);
    fn render_list(&mut self, list: List, list_state: &mut ListState, area: Rect, buf: &mut Buffer);
    fn render_widget_list(
        &mut self,
        list_view: ListView<'_, Gauge<'_>>,
        list_state: &mut WidgetListState,
        size: usize,
        length: u16,
        area: Rect,
        buf: &mut Buffer,
    );
}
impl ScrollbarStateExt for ScrollbarState {
    fn match_list_state(&mut self, list_state: &ListState) {
        if let Some(s) = list_state.selected() {
            *self = self.position(s);
        }
    }
    fn match_widget_list_state(&mut self, list_state: &WidgetListState) {
        if let Some(s) = list_state.selected {
            *self = self.position(s);
        }
    }
    fn render_list(
        &mut self,
        list: List,
        list_state: &mut ListState,
        area: Rect,
        buf: &mut Buffer,
    ) {
        *self = self.content_length(list.len());

        let horizontal_layout = horizontal![*=1, ==2];
        let areas: [Rect; 2] = horizontal_layout.areas(area);

        if area.height < list.len() as u16 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            StatefulWidget::render(scrollbar, areas[1], buf, self);
        }
        StatefulWidget::render(list, areas[0], buf, list_state);
    }
    fn render_widget_list(
        &mut self,
        list_view: ListView<'_, Gauge<'_>>,
        list_state: &mut WidgetListState,
        size: usize,
        length: u16,
        area: Rect,
        buf: &mut Buffer,
    ) {
        *self = self.content_length(size);

        let mut list_view_area: Rect = area;
        if area.height < length {
            let horizontal_layout = horizontal![*=1, ==1];
            let areas: [Rect; 2] = horizontal_layout.areas(area);
            list_view_area = areas[0];

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            StatefulWidget::render(scrollbar, areas[1], buf, self);
        }

        list_view.render(list_view_area, buf, list_state);
    }
}

// TODO: rework it if possible, this is silly
pub trait WidgetListStateExt {
    fn first(&mut self);
    fn last(&mut self);
}
impl WidgetListStateExt for WidgetListState {
    fn first(&mut self) {
        self.select(None);
        self.next();
    }
    fn last(&mut self) {
        self.select(None);
        self.next();
        self.previous();
    }
}

/// Advanced Rect operations
pub trait RectExt {
    fn min_width(self, min: u16) -> Self;
    fn min_height(self, min: u16) -> Self;
    fn max_width(self, max: u16) -> Self;
    fn max_height(self, max: u16) -> Self;
    fn with_width(self, height: u16) -> Self;
    fn with_height(self, height: u16) -> Self;
    fn clamp_width(self, min: u16, max: u16) -> Self;
    fn clamp_height(self, min: u16, max: u16) -> Self;
}
impl RectExt for Rect {
    fn min_width(self, min: u16) -> Self {
        Self {
            width: self.width.max(min),
            ..self
        }
    }
    fn min_height(self, min: u16) -> Self {
        Self {
            height: self.height.max(min),
            ..self
        }
    }
    fn max_width(self, max: u16) -> Self {
        Self {
            width: self.width.min(max),
            ..self
        }
    }
    fn max_height(self, max: u16) -> Self {
        Self {
            height: self.height.min(max),
            ..self
        }
    }
    fn clamp_width(self, min: u16, max: u16) -> Self {
        Self {
            height: self.height.clamp(min, max),
            ..self
        }
    }
    fn clamp_height(self, min: u16, max: u16) -> Self {
        Self {
            width: self.width.clamp(min, max),
            ..self
        }
    }
    fn with_width(self, width: u16) -> Self {
        Self { width, ..self }
    }
    fn with_height(self, height: u16) -> Self {
        Self { height, ..self }
    }
}

// Usually it's CollapsedBorder::bottom() + CollapsedSet::top_collapsed()
// and CollapsedBorder::right() + CollapsedSet::left_collapsed()
pub struct CollapsedBorder;
impl CollapsedBorder {
    pub fn top() -> Borders {
        Borders::TOP | Borders::LEFT | Borders::RIGHT
    }
    pub fn left() -> Borders {
        Borders::TOP | Borders::BOTTOM | Borders::LEFT
    }
    pub fn all() -> Borders {
        Borders::ALL
    }
}

// Those are named according to position
pub struct CollapsedSet;
impl CollapsedSet {
    // bl+br
    pub fn top_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            bottom_left: symbols::line::NORMAL.bottom_left,
            bottom_right: symbols::line::NORMAL.bottom_right,
            ..set
        }
    }
    // tl+tr
    pub fn bottom_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_left: symbols::line::NORMAL.vertical_right,
            top_right: symbols::line::NORMAL.vertical_left,
            ..set
        }
    }
    // tr+br
    pub fn left_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_right: symbols::line::NORMAL.top_right,
            bottom_right: symbols::line::NORMAL.bottom_right,
            ..set
        }
    }
    // tl+bl
    pub fn right_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_left: symbols::line::NORMAL.horizontal_down,
            bottom_left: symbols::line::NORMAL.horizontal_up,
            ..set
        }
    }
    // tr+bl+br
    pub fn top_left_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_right: symbols::line::NORMAL.top_right,
            bottom_left: symbols::line::NORMAL.vertical_right,
            bottom_right: symbols::line::NORMAL.vertical_left,
            ..set
        }
    }
    // tl+bl+br
    pub fn top_right_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_left: symbols::line::NORMAL.horizontal_down,
            bottom_left: symbols::line::NORMAL.vertical_right,
            bottom_right: symbols::line::NORMAL.vertical_left,
            ..set
        }
    }
    // tl+tr+br
    pub fn bottom_left_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_left: symbols::line::NORMAL.vertical_right,
            top_right: symbols::line::NORMAL.vertical_left,
            bottom_right: symbols::line::NORMAL.bottom_right,
            ..set
        }
    }
    // tl+tr+bl
    pub fn bottom_right_collapsed(set: symbols::border::Set) -> symbols::border::Set {
        symbols::border::Set {
            top_left: symbols::line::NORMAL.vertical_right,
            top_right: symbols::line::NORMAL.vertical_left,
            bottom_left: symbols::line::NORMAL.vertical_right,
            ..set
        }
    }
}
