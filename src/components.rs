use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{action::Action, config::Config, tui::Event};

pub mod dialog;
pub mod fps;
pub mod func_bar;
pub mod home;
pub mod panel;

/// `Component` is a trait that represents a visual and interactive element of the user interface.
pub trait Component {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> color_eyre::Result<()> {
        let _ = tx;
        Ok(())
    }
    fn register_config_handler(&mut self, config: Config) -> color_eyre::Result<()> {
        let _ = config;
        Ok(())
    }
    fn init(&mut self, area: Size) -> color_eyre::Result<()> {
        let _ = area;
        Ok(())
    }
    fn handle_events(&mut self, event: Option<Event>) -> color_eyre::Result<Option<Action>> {
        let action = match event {
            Some(Event::Key(key_event)) => self.handle_key_event(key_event)?,
            Some(Event::Mouse(mouse_event)) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }
    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        let _ = key;
        Ok(None)
    }
    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> color_eyre::Result<Option<Action>> {
        let _ = mouse;
        Ok(None)
    }
    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        let _ = action;
        Ok(None)
    }
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()>;
}
