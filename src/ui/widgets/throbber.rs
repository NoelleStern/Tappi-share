use ratatui::text::Line;
use ratatui_macros::line;
use throbber_widgets_tui::{Throbber, ThrobberState};

pub fn label_throbber<'a>(
    throbber: Throbber<'a>,
    state: &mut ThrobberState,
    text: String,
    label_left: Option<bool>,
) -> Line<'a> {
    let label_left = label_left.unwrap_or(false);

    if label_left {
        left_label(throbber, text, state)
    } else {
        throbber.label(text).to_line(state)
    }
}
pub fn custom_throbber<'a>() -> Throbber<'a> {
    Throbber::default().throbber_set(throbber_widgets_tui::symbols::throbber::Set {
        full: "...",
        empty: "   ",
        symbols: &["   ", ".  ", ".. ", "..."],
    })
}
fn left_label<'a>(throbber: Throbber<'a>, text: String, state: &mut ThrobberState) -> Line<'a> {
    line!(text, throbber.to_symbol_span(state),)
}

pub struct ThrobberStateCounter {
    pub state: ThrobberState,
    counter: ThrobberCounter,
}
impl ThrobberStateCounter {
    pub fn new(interval: u8) -> Self {
        Self {
            state: ThrobberState::default(),
            counter: ThrobberCounter::new(interval),
        }
    }

    pub fn update(&mut self) {
        if self.counter.update() {
            self.state.calc_next();
        }
    }
}

pub struct ThrobberCounter {
    interval: u8,
    counter: u8,
}
impl ThrobberCounter {
    pub fn new(interval: u8) -> Self {
        Self {
            interval,
            counter: 0,
        }
    }

    pub fn update(&mut self) -> bool {
        let mut result = false;

        if self.counter >= self.interval {
            self.counter = 0;
            result = true;
        }
        self.counter += 1;

        result
    }
}
