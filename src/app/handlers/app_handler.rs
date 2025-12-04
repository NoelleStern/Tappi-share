use async_trait::async_trait;
use crossterm::event::KeyEvent;

use crate::app::{app_event::AppEvent, app_main::App};

/// A trait that contains an app behavior
#[async_trait]
pub trait AppHandler {
    /// Handle key events here
    fn handle_key_events(key_event: &KeyEvent) -> color_eyre::Result<AppEvent>;
    /// Handle app events here
    fn handle_app_events(app: &mut App, event: AppEvent) -> color_eyre::Result<()>;
}
