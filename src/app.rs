use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::{
    action::{Action, Side},
    components::{
        dialog::{self, DeferredOp, DialogState},
        func_bar,
        panel::{copy_recursive, delete_recursive, file_name_of, Panel},
    },
    config::Config,
    tui::{Event, Tui},
};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Normal,
    Dialog,
}

pub struct App {
    config: Config,
    tick_rate: f64,
    frame_rate: f64,
    left: Panel,
    right: Panel,
    active: Side,
    dialog: Option<DialogState>,
    mode: Mode,
    last_error: Option<String>,
    should_quit: bool,
    should_suspend: bool,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
}

impl App {
    pub fn new(tick_rate: f64, frame_rate: f64) -> color_eyre::Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());

        let mut left = Panel::new(Side::Left, cwd.clone(), action_tx.clone());
        let right = Panel::new(Side::Right, cwd, action_tx.clone());
        left.is_active = true;

        Ok(Self {
            config: Config::new()?,
            tick_rate,
            frame_rate,
            left,
            right,
            active: Side::Left,
            dialog: None,
            mode: Mode::Normal,
            last_error: None,
            should_quit: false,
            should_suspend: false,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
        })
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let mut tui = Tui::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        tui.enter()?;

        // Kick off async directory loads for both panels.
        self.left.reload();
        self.right.reload();

        let action_tx = self.action_tx.clone();
        loop {
            self.handle_events(&mut tui).await?;
            self.handle_actions(&mut tui)?;
            if self.should_suspend {
                tui.suspend()?;
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                tui.enter()?;
            } else if self.should_quit {
                tui.stop()?;
                break;
            }
        }
        tui.exit()?;
        Ok(())
    }

    async fn handle_events(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        let Some(event) = tui.next_event().await else {
            return Ok(());
        };
        let action_tx = self.action_tx.clone();
        match event {
            Event::Quit => action_tx.send(Action::Quit)?,
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
            Event::Key(key) => self.handle_key_event(key)?,
            _ => {}
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        let tx = self.action_tx.clone();

        if self.mode == Mode::Dialog {
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    tx.send(Action::DialogConfirm)?;
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    tx.send(Action::DialogCancel)?;
                }
                KeyCode::Backspace => {
                    tx.send(Action::DialogInputBackspace)?;
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    tx.send(Action::DialogInputChar(c))?;
                }
                _ => {}
            }
            return Ok(());
        }

        // Normal mode: look up in keybindings.
        let Some(keymap) = self.config.keybindings.0.get(&self.mode) else {
            return Ok(());
        };
        match keymap.get(&vec![key]) {
            Some(action) => {
                info!("Got action: {action:?}");
                tx.send(action.clone())?;
            }
            _ => {
                self.last_tick_key_events.push(key);
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                    info!("Got action: {action:?}");
                    tx.send(action.clone())?;
                }
            }
        }
        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                debug!("{action:?}");
            }
            match action {
                Action::Tick => {
                    self.last_tick_key_events.drain(..);
                }
                Action::Quit => self.should_quit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => tui.terminal.clear()?,
                Action::Resize(w, h) => {
                    tui.resize(Rect::new(0, 0, w, h))?;
                    self.render(tui)?;
                }
                Action::Render => self.render(tui)?,

                Action::Error(msg) => self.last_error = Some(msg),
                Action::OpError(msg) => self.last_error = Some(msg),

                // Navigation
                Action::NavUp => self.active_panel_mut().nav_up(),
                Action::NavDown => self.active_panel_mut().nav_down(),
                Action::NavPageUp => {
                    let h = self.panel_page_height(tui);
                    self.active_panel_mut().nav_page_up(h);
                }
                Action::NavPageDown => {
                    let h = self.panel_page_height(tui);
                    self.active_panel_mut().nav_page_down(h);
                }
                Action::NavTop => self.active_panel_mut().nav_top(),
                Action::NavBottom => self.active_panel_mut().nav_bottom(),
                Action::NavEnter => self.active_panel().nav_enter(),
                Action::NavParent => self.active_panel().nav_parent(),
                Action::SwitchPanel => self.switch_panel(),

                // Marking
                Action::ToggleMark => self.active_panel_mut().toggle_mark(),
                Action::ToggleMarkAll => self.active_panel_mut().toggle_mark_all(),

                // F-key operations
                Action::Copy => self.open_copy_dialog(),
                Action::Move => self.open_move_dialog(),
                Action::Mkdir => self.open_mkdir_dialog(),
                Action::Delete => self.open_delete_dialog(),

                // Async dir load completed
                Action::DirLoaded { side, path, entries } => {
                    self.get_panel_mut(side).on_dir_loaded(path, entries);
                }

                // Execute ops (dispatched from dialog confirm)
                Action::ExecuteDelete(paths) => self.spawn_delete(paths),
                Action::ExecuteCopy { sources, dest } => {
                    let active = self.active;
                    self.spawn_copy(sources, dest, active);
                }
                Action::ExecuteMove { sources, dest } => {
                    let active = self.active;
                    self.spawn_move(sources, dest, active);
                }
                Action::ExecuteMkdir { base, name } => {
                    let active = self.active;
                    self.spawn_mkdir(base, name, active);
                }
                Action::OpCompleted(sides) => {
                    for side in sides {
                        self.get_panel(side).reload();
                    }
                }

                // Dialog
                Action::DialogConfirm => self.dialog_confirm(),
                Action::DialogCancel => self.dialog_cancel(),
                Action::DialogInputChar(c) => {
                    if let Some(d) = &mut self.dialog {
                        d.push_char(c);
                    }
                }
                Action::DialogInputBackspace => {
                    if let Some(d) = &mut self.dialog {
                        d.pop_char();
                    }
                }

                _ => {}
            }
        }
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> color_eyre::Result<()> {
        let left = &mut self.left;
        let right = &mut self.right;
        let active = self.active;
        let dialog = &self.dialog;
        let last_error = &self.last_error;

        tui.draw(|frame| {
            let area = frame.area();
            let [panels_area, status_area, func_area] = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(area);

            let [left_area, right_area] = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .areas(panels_area);

            left.draw(frame, left_area);
            right.draw(frame, right_area);

            // Status bar
            let active_panel = if active == Side::Left { &*left } else { &*right };
            draw_status_bar(frame, status_area, active_panel, last_error.as_deref());

            // Function key bar
            func_bar::draw(frame, func_area);

            // Dialog overlay (drawn last so it's on top)
            if let Some(d) = dialog {
                dialog::draw(frame, d, area);
            }
        })?;
        Ok(())
    }

    // --- Panel helpers ---

    fn active_panel(&self) -> &Panel {
        self.get_panel(self.active)
    }

    fn active_panel_mut(&mut self) -> &mut Panel {
        self.get_panel_mut(self.active)
    }

    fn get_panel(&self, side: Side) -> &Panel {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn get_panel_mut(&mut self, side: Side) -> &mut Panel {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    fn switch_panel(&mut self) {
        self.active = self.active.other();
        self.left.is_active = self.active == Side::Left;
        self.right.is_active = self.active == Side::Right;
    }

    fn panel_page_height(&self, tui: &Tui) -> usize {
        // Panel area height minus borders and header.
        let total = tui.terminal.size().map(|s| s.height).unwrap_or(24);
        (total.saturating_sub(4) as usize).max(1) // 2 borders + 1 header + 1 status + 1 func
    }

    // --- Dialog openers ---

    fn open_copy_dialog(&mut self) {
        let sources = self.active_panel().effective_targets();
        if sources.is_empty() {
            return;
        }
        let dest = self.get_panel(self.active.other()).path.to_string_lossy().into_owned();
        let label = if sources.len() == 1 {
            sources[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            format!("{} files", sources.len())
        };
        self.open_dialog(DialogState::input(
            "Copy",
            format!("Copy {} to:", label),
            dest,
            DeferredOp::Copy { sources },
        ));
    }

    fn open_move_dialog(&mut self) {
        let sources = self.active_panel().effective_targets();
        if sources.is_empty() {
            return;
        }
        let dest = self.get_panel(self.active.other()).path.to_string_lossy().into_owned();
        let label = if sources.len() == 1 {
            sources[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            format!("{} files", sources.len())
        };
        self.open_dialog(DialogState::input(
            "Move",
            format!("Move {} to:", label),
            dest,
            DeferredOp::Move { sources },
        ));
    }

    fn open_mkdir_dialog(&mut self) {
        let base = self.active_panel().path.clone();
        self.open_dialog(DialogState::input(
            "Make Directory",
            "New directory name:",
            "",
            DeferredOp::Mkdir { base },
        ));
    }

    fn open_delete_dialog(&mut self) {
        let sources = self.active_panel().effective_targets();
        if sources.is_empty() {
            return;
        }
        let msg = if sources.len() == 1 {
            format!(
                "Delete '{}'?",
                sources[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            )
        } else {
            format!("Delete {} items?", sources.len())
        };
        self.open_dialog(DialogState::confirm("Delete", msg, DeferredOp::Delete(sources)));
    }

    fn open_dialog(&mut self, state: DialogState) {
        self.dialog = Some(state);
        self.mode = Mode::Dialog;
    }

    fn dialog_confirm(&mut self) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        self.mode = Mode::Normal;
        self.last_error = None;
        let tx = self.action_tx.clone();

        match dialog {
            DialogState::Confirm { op, .. } => self.execute_op(op, None, tx),
            DialogState::Input { value, op, .. } => self.execute_op(op, Some(value), tx),
        }
    }

    fn dialog_cancel(&mut self) {
        self.dialog = None;
        self.mode = Mode::Normal;
    }

    fn execute_op(
        &self,
        op: DeferredOp,
        value: Option<String>,
        tx: mpsc::UnboundedSender<Action>,
    ) {
        let active = self.active;
        match op {
            DeferredOp::Delete(paths) => {
                tokio::spawn(async move {
                    for path in &paths {
                        if let Err(e) = delete_recursive(path).await {
                            let _ = tx.send(Action::OpError(e.to_string()));
                            return;
                        }
                    }
                    let _ = tx.send(Action::OpCompleted(vec![active]));
                });
            }
            DeferredOp::Copy { sources } => {
                let dest: std::path::PathBuf = value.unwrap_or_default().into();
                tokio::spawn(async move {
                    for src in &sources {
                        let name = match file_name_of(src) {
                            Ok(n) => n,
                            Err(e) => {
                                let _ = tx.send(Action::OpError(e.to_string()));
                                return;
                            }
                        };
                        if let Err(e) = copy_recursive(src, &dest.join(name)).await {
                            let _ = tx.send(Action::OpError(e.to_string()));
                            return;
                        }
                    }
                    let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
                });
            }
            DeferredOp::Move { sources } => {
                let dest: std::path::PathBuf = value.unwrap_or_default().into();
                tokio::spawn(async move {
                    for src in &sources {
                        let name = match file_name_of(src) {
                            Ok(n) => n,
                            Err(e) => {
                                let _ = tx.send(Action::OpError(e.to_string()));
                                return;
                            }
                        };
                        let dst = dest.join(name);
                        // Try rename first; fall back to copy+delete for cross-device.
                        if tokio::fs::rename(src, &dst).await.is_err() {
                            if let Err(e) = copy_recursive(src, &dst).await {
                                let _ = tx.send(Action::OpError(e.to_string()));
                                return;
                            }
                            if let Err(e) = delete_recursive(src).await {
                                let _ = tx.send(Action::OpError(e.to_string()));
                                return;
                            }
                        }
                    }
                    let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
                });
            }
            DeferredOp::Mkdir { base } => {
                let name = value.unwrap_or_default();
                tokio::spawn(async move {
                    if let Err(e) = tokio::fs::create_dir(base.join(&name)).await {
                        let _ = tx.send(Action::OpError(e.to_string()));
                        return;
                    }
                    let _ = tx.send(Action::OpCompleted(vec![active]));
                });
            }
        }
    }

    // --- Async spawners (legacy path, now inlined into execute_op) ---

    fn spawn_delete(&self, paths: Vec<std::path::PathBuf>) {
        let tx = self.action_tx.clone();
        let active = self.active;
        tokio::spawn(async move {
            for path in &paths {
                if let Err(e) = delete_recursive(path).await {
                    let _ = tx.send(Action::OpError(e.to_string()));
                    return;
                }
            }
            let _ = tx.send(Action::OpCompleted(vec![active]));
        });
    }

    fn spawn_copy(
        &self,
        sources: Vec<std::path::PathBuf>,
        dest: std::path::PathBuf,
        active: Side,
    ) {
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            for src in &sources {
                let name = match file_name_of(src) {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = tx.send(Action::OpError(e.to_string()));
                        return;
                    }
                };
                if let Err(e) = copy_recursive(src, &dest.join(name)).await {
                    let _ = tx.send(Action::OpError(e.to_string()));
                    return;
                }
            }
            let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
        });
    }

    fn spawn_move(
        &self,
        sources: Vec<std::path::PathBuf>,
        dest: std::path::PathBuf,
        active: Side,
    ) {
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            for src in &sources {
                let name = match file_name_of(src) {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = tx.send(Action::OpError(e.to_string()));
                        return;
                    }
                };
                let dst = dest.join(name);
                if tokio::fs::rename(src, &dst).await.is_err() {
                    if let Err(e) = copy_recursive(src, &dst).await {
                        let _ = tx.send(Action::OpError(e.to_string()));
                        return;
                    }
                    if let Err(e) = delete_recursive(src).await {
                        let _ = tx.send(Action::OpError(e.to_string()));
                        return;
                    }
                }
            }
            let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
        });
    }

    fn spawn_mkdir(&self, base: std::path::PathBuf, name: String, active: Side) {
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::create_dir(base.join(&name)).await {
                let _ = tx.send(Action::OpError(e.to_string()));
                return;
            }
            let _ = tx.send(Action::OpCompleted(vec![active]));
        });
    }
}

fn draw_status_bar(frame: &mut Frame, area: Rect, panel: &Panel, error: Option<&str>) {
    use ratatui::widgets::Paragraph;

    if let Some(err) = error {
        let para = Paragraph::new(err)
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        frame.render_widget(para, area);
        return;
    }

    let left_text = if let Some(entry) = panel.cursor_entry() {
        if entry.is_dir {
            format!(" {} <DIR>", entry.name)
        } else {
            format!(" {}  {} bytes", entry.name, entry.size)
        }
    } else {
        String::new()
    };

    let right_text = if let Some((count, size)) = panel.marked_summary() {
        format!("{} marked ({} bytes) ", count, size)
    } else {
        String::new()
    };

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_text.len() as u16)])
            .areas(area);

    frame.render_widget(
        Paragraph::new(left_text).style(Style::default().add_modifier(Modifier::BOLD)),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(right_text)
            .alignment(Alignment::Right)
            .style(Style::default().fg(Color::Yellow)),
        right_area,
    );
}
