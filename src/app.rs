use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::{
    action::{Action, Side},
    components::{
        dialog::{self, DeferredOp, DialogState, MenuAction, MenuItem},
        func_bar,
        panel::{
            Panel, copy_recursive, delete_recursive, extract_archive, file_name_of, is_archive,
            is_executable,
        },
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
    /// Command to run after suspending the TUI (set by Execute action).
    pending_command: Option<(String, Vec<String>)>,
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
            pending_command: None,
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

        self.left.reload();
        self.right.reload();

        let action_tx = self.action_tx.clone();
        loop {
            self.handle_events(&mut tui).await?;
            self.handle_actions(&mut tui)?;

            // Run a pending shell command: suspend TUI, exec, resume.
            if let Some((cmd, args)) = self.pending_command.take() {
                tui.exit()?;
                println!("\nRunning: {} {}\n", cmd, args.join(" "));
                std::process::Command::new(&cmd).args(&args).status().ok();
                println!("\nPress Enter to continue...");
                let mut _s = String::new();
                std::io::stdin().read_line(&mut _s).ok();
                tui.enter()?;
                action_tx.send(Action::ClearScreen)?;
            }

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
        let tx = self.action_tx.clone();
        match event {
            Event::Quit => tx.send(Action::Quit)?,
            Event::Tick => tx.send(Action::Tick)?,
            Event::Render => tx.send(Action::Render)?,
            Event::Resize(x, y) => tx.send(Action::Resize(x, y))?,
            Event::Key(key) => self.handle_key_event(key)?,
            _ => {}
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<()> {
        let tx = self.action_tx.clone();

        if self.mode == Mode::Dialog {
            let is_qcd = self
                .dialog
                .as_ref()
                .map(|d| d.is_quick_cd())
                .unwrap_or(false);
            let is_confirm = matches!(&self.dialog, Some(DialogState::Confirm { .. }));

            match key.code {
                KeyCode::Enter => tx.send(Action::DialogConfirm)?,
                KeyCode::Esc => tx.send(Action::DialogCancel)?,
                KeyCode::Up => tx.send(Action::DialogNavUp)?,
                KeyCode::Down => tx.send(Action::DialogNavDown)?,

                // QuickCd-specific
                KeyCode::Tab if is_qcd => tx.send(Action::QuickCdComplete)?,
                KeyCode::Backspace if is_qcd => tx.send(Action::QuickCdBackspace)?,
                KeyCode::Char(c)
                    if is_qcd
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    tx.send(Action::QuickCdChar(c))?;
                }

                // Confirm-dialog y/n shortcuts
                KeyCode::Char('y' | 'Y') if is_confirm => {
                    tx.send(Action::DialogConfirm)?;
                }
                KeyCode::Char('n' | 'N') if is_confirm => {
                    tx.send(Action::DialogCancel)?;
                }

                // Input dialogs
                KeyCode::Backspace => tx.send(Action::DialogInputBackspace)?,
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    tx.send(Action::DialogInputChar(c))?;
                }
                _ => {}
            }
            return Ok(());
        }

        // Normal mode: keybinding lookup first.
        let Some(keymap) = self.config.keybindings.0.get(&self.mode) else {
            return Ok(());
        };
        match keymap.get(&vec![key]) {
            Some(action) => {
                info!("Got action: {action:?}");
                self.last_tick_key_events.clear();
                tx.send(action.clone())?;
            }
            _ => {
                self.last_tick_key_events.push(key);
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                    info!("Got action: {action:?}");
                    self.last_tick_key_events.clear();
                    tx.send(action.clone())?;
                } else if self.last_tick_key_events.len() == 1 {
                    // Unbound single key → filter input or Esc to clear filter.
                    match key.code {
                        KeyCode::Char(c)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                        {
                            tx.send(Action::FilterChar(c))?;
                        }
                        KeyCode::Backspace => tx.send(Action::FilterBackspace)?,
                        KeyCode::Esc => tx.send(Action::FilterClear)?,
                        _ => {}
                    }
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

                Action::Error(msg) | Action::OpError(msg) => {
                    self.last_error = Some(msg);
                }

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
                Action::SyncPanelDir => self.sync_panel_dir(),

                // Filter
                Action::FilterChar(c) => {
                    self.last_error = None;
                    self.active_panel_mut().push_filter_char(c);
                }
                Action::FilterBackspace => self.active_panel_mut().pop_filter_char(),
                Action::FilterClear => self.active_panel_mut().clear_filter(),

                // Marking
                Action::ToggleMark => self.active_panel_mut().toggle_mark(),
                Action::ToggleMarkAll => self.active_panel_mut().toggle_mark_all(),

                // Quick CD
                Action::QuickCd => self.open_quick_cd_dialog(),
                Action::QuickCdChar(c) => {
                    if let Some(DialogState::QuickCd { input, .. }) = &mut self.dialog {
                        if input.is_empty() && c == '~' {
                            let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
                            *input = format!("{}/", home);
                        } else {
                            input.push(c);
                        }
                    }
                    self.refresh_quick_cd();
                }
                Action::QuickCdBackspace => {
                    if let Some(DialogState::QuickCd { input, .. }) = &mut self.dialog {
                        input.pop();
                    }
                    self.refresh_quick_cd();
                }
                Action::QuickCdComplete => {
                    if let Some(new_input) = self.quick_cd_complete_input() {
                        if let Some(DialogState::QuickCd { input, .. }) = &mut self.dialog {
                            *input = new_input;
                        }
                        self.refresh_quick_cd();
                    }
                }

                // F-key operations
                Action::Copy => self.open_copy_dialog(),
                Action::Move => self.open_move_dialog(),
                Action::Mkdir => self.open_mkdir_dialog(),
                Action::Delete => self.open_delete_dialog(),
                Action::View => self.open_nano_dialog(),
                Action::CalcSizes => self.active_panel_mut().start_size_calc(),
                Action::CycleSortMode => self.active_panel_mut().cycle_sort_mode(),
                Action::InvertSort => self.active_panel_mut().invert_sort(),
                Action::ContextMenu => self.open_context_menu(),

                // Dir size results (from F4)
                Action::DirSizeResult {
                    side,
                    panel_path,
                    name,
                    size,
                } => {
                    self.get_panel_mut(side)
                        .on_dir_size_result(&panel_path, name, size);
                }

                // Async dir load
                Action::DirLoaded {
                    side,
                    path,
                    entries,
                } => {
                    self.get_panel_mut(side).on_dir_loaded(path, entries);
                }

                // Execute ops (from ExecuteCopy/Move/Delete/Mkdir actions — legacy path)
                Action::ExecuteDelete(paths) => self.do_delete(paths),
                Action::ExecuteCopy { sources, dest } => {
                    let active = self.active;
                    self.do_copy(sources, dest, active);
                }
                Action::ExecuteMove { sources, dest } => {
                    let active = self.active;
                    self.do_move(sources, dest, active);
                }
                Action::ExecuteMkdir { base, name } => {
                    let active = self.active;
                    self.do_mkdir(base.join(name), active);
                }
                Action::ExecuteFile { cmd, args } => {
                    self.pending_command = Some((cmd, args));
                }
                Action::OpCompleted(sides) => {
                    for side in sides {
                        self.get_panel(side).reload();
                    }
                }

                // Dialog
                Action::DialogConfirm => self.dialog_confirm(),
                Action::DialogCancel => self.dialog_cancel(),
                Action::DialogNavUp => {
                    if let Some(d) = &mut self.dialog {
                        d.nav_up();
                    }
                }
                Action::DialogNavDown => {
                    if let Some(d) = &mut self.dialog {
                        d.nav_down();
                    }
                }
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

            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(panels_area);

            left.draw(frame, left_area);
            right.draw(frame, right_area);

            let active_panel = if active == Side::Left {
                &*left
            } else {
                &*right
            };
            draw_status_bar(frame, status_area, active_panel, last_error.as_deref());

            func_bar::draw(frame, func_area);

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

    fn sync_panel_dir(&mut self) {
        let path = self.active_panel().path.clone();
        let other = self.active.other();
        Panel::load_dir(path, other, self.action_tx.clone());
    }

    fn panel_page_height(&self, tui: &Tui) -> usize {
        let total = tui.terminal.size().map(|s| s.height).unwrap_or(24);
        (total.saturating_sub(5) as usize).max(1)
    }

    // --- Dialog openers ---

    fn open_copy_dialog(&mut self) {
        let sources = self.active_panel().effective_targets();
        if sources.is_empty() {
            return;
        }
        let dest = self
            .get_panel(self.active.other())
            .path
            .to_string_lossy()
            .into_owned();
        let label = target_label(&sources);
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
        let dest = self
            .get_panel(self.active.other())
            .path
            .to_string_lossy()
            .into_owned();
        let label = target_label(&sources);
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
                sources[0]
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        } else {
            format!("Delete {} items?", sources.len())
        };
        self.open_dialog(DialogState::confirm(
            "Delete",
            msg,
            DeferredOp::Delete(sources),
        ));
    }

    fn open_nano_dialog(&mut self) {
        let base = self.active_panel().path.clone();
        let prefill = self
            .active_panel()
            .current_entry()
            .filter(|e| !e.is_dir && e.name != "..")
            .map(|e| e.name.clone())
            .unwrap_or_default();
        self.open_dialog(DialogState::input(
            "Open in Nano",
            format!("File in {}:", base.display()),
            prefill,
            DeferredOp::OpenInNano { base },
        ));
    }

    fn open_context_menu(&mut self) {
        let Some(entry) = self.active_panel().current_entry() else {
            return;
        };
        if entry.name == ".." {
            return;
        }

        let path = self.active_panel().path.join(&entry.name);
        let panel_dir = self.active_panel().path.clone();
        let other_dir = self.get_panel(self.active.other()).path.clone();
        let is_dir = entry.is_dir;
        let name = entry.name.clone();

        let mut items: Vec<MenuItem> = Vec::new();

        // Always available
        items.push(MenuItem::new(
            "Open with OS (xdg-open)",
            MenuAction::OpenWithOs(path.clone()),
        ));
        let code_dir = if is_dir {
            path.clone()
        } else {
            panel_dir.clone()
        };
        items.push(MenuItem::new(
            "Run VS Code here",
            MenuAction::RunCodeHere(code_dir),
        ));

        // Archive extraction
        if !is_dir && is_archive(&name) {
            items.push(MenuItem::new(
                "Extract here",
                MenuAction::ExtractHere {
                    archive: path.clone(),
                    dest: panel_dir,
                },
            ));
            items.push(MenuItem::new(
                format!("Extract to → {}", other_dir.display()),
                MenuAction::ExtractHere {
                    archive: path.clone(),
                    dest: other_dir,
                },
            ));
        }

        // Executable
        if !is_dir && is_executable(&path) {
            items.push(MenuItem::new("Execute…", MenuAction::RequestExecute(path)));
        }

        self.open_dialog(DialogState::context_menu(
            format!("Actions: {}", name),
            items,
        ));
    }

    fn open_dialog(&mut self, state: DialogState) {
        self.dialog = Some(state);
        self.mode = Mode::Dialog;
    }

    // --- Dialog confirmation ---

    fn dialog_confirm(&mut self) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };

        match dialog {
            DialogState::Confirm { op, .. } => {
                self.mode = Mode::Normal;
                self.last_error = None;
                self.execute_op(op, None);
            }
            DialogState::Input { value, op, .. } => {
                self.mode = Mode::Normal;
                self.last_error = None;
                self.execute_op(op, Some(value));
            }
            DialogState::ContextMenu {
                items, selected, ..
            } => {
                self.mode = Mode::Normal;
                if let Some(item) = items.into_iter().nth(selected) {
                    self.execute_menu_action(item.action);
                }
            }
            DialogState::QuickCd {
                input,
                matches,
                selected,
                base_path,
            } => {
                self.mode = Mode::Normal;
                let dest = quick_cd_destination(&input, &matches, selected, &base_path);
                let side = self.active;
                Panel::load_dir(dest, side, self.action_tx.clone());
            }
        }
    }

    fn dialog_cancel(&mut self) {
        self.dialog = None;
        self.mode = Mode::Normal;
    }

    fn execute_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::OpenWithOs(path) => {
                tokio::spawn(async move {
                    tokio::process::Command::new("xdg-open")
                        .arg(path)
                        .spawn()
                        .ok();
                });
            }
            MenuAction::RunCodeHere(dir) => {
                tokio::spawn(async move {
                    tokio::process::Command::new("code").arg(dir).spawn().ok();
                });
            }
            MenuAction::RequestExecute(path) => {
                // Open an args dialog; on confirm → ExecuteFile
                self.open_dialog(DialogState::input(
                    "Execute",
                    format!(
                        "Arguments for {}:",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    "",
                    DeferredOp::Execute { path },
                ));
            }
            MenuAction::ExtractHere { archive, dest } => {
                let active = self.active;
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match extract_archive(&archive, &dest).await {
                        Ok(()) => {
                            let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::OpError(e.to_string()));
                        }
                    }
                });
            }
        }
    }

    fn execute_op(&self, op: DeferredOp, value: Option<String>) {
        let tx = self.action_tx.clone();
        let active = self.active;

        match op {
            DeferredOp::Delete(paths) => self.do_delete(paths),

            DeferredOp::Copy { sources } => {
                let dest: std::path::PathBuf = value.unwrap_or_default().into();
                self.do_copy(sources, dest, active);
            }

            DeferredOp::Move { sources } => {
                let dest: std::path::PathBuf = value.unwrap_or_default().into();
                self.do_move(sources, dest, active);
            }

            DeferredOp::Mkdir { base } => {
                let name = value.unwrap_or_default();
                self.do_mkdir(base.join(name), active);
            }

            DeferredOp::OpenInNano { base } => {
                let filename = value.unwrap_or_default();
                if !filename.is_empty() {
                    let path = base.join(&filename).to_string_lossy().into_owned();
                    let _ = tx.send(Action::ExecuteFile {
                        cmd: "nano".into(),
                        args: vec![path],
                    });
                }
            }

            DeferredOp::Execute { path } => {
                let cmd = path.to_string_lossy().into_owned();
                let args_str = value.unwrap_or_default();
                let args: Vec<String> = args_str.split_whitespace().map(String::from).collect();
                let _ = tx.send(Action::ExecuteFile { cmd, args });
            }
        }
    }

    // --- Async file ops ---

    fn do_delete(&self, paths: Vec<std::path::PathBuf>) {
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

    fn do_copy(&self, sources: Vec<std::path::PathBuf>, dest: std::path::PathBuf, active: Side) {
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

    fn do_move(&self, sources: Vec<std::path::PathBuf>, dest: std::path::PathBuf, active: Side) {
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
                let dest_full_path = dest.join(name);
                if tokio::fs::rename(src, &dest_full_path).await.is_err() {
                    if let Err(e) = copy_recursive(src, &dest_full_path).await {
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

    fn do_mkdir(&self, path: std::path::PathBuf, active: Side) {
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::create_dir(&path).await {
                let _ = tx.send(Action::OpError(e.to_string()));
                return;
            }
            let _ = tx.send(Action::OpCompleted(vec![active]));
        });
    }
}

// --- Status bar ---

fn draw_status_bar(frame: &mut Frame, area: Rect, panel: &Panel, error: Option<&str>) {
    use ratatui::widgets::Paragraph;

    if let Some(err) = error {
        frame.render_widget(
            Paragraph::new(err).style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            area,
        );
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

    let filter_text = if !panel.filter.is_empty() {
        format!(" [filter: {}]", panel.filter)
    } else {
        String::new()
    };

    let right_text = if let Some((count, size)) = panel.marked_summary() {
        format!("{} marked ({}) ", count, format_status_size(size))
    } else if let Some((total, approx)) = panel.size_summary() {
        let prefix = if approx { "~" } else { "" };
        format!("{}total {} ", prefix, format_status_size(total))
    } else {
        String::new()
    };

    let right_w = right_text.len() as u16;
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w)]).areas(area);

    let left_full = format!("{}{}", left_text, filter_text);
    frame.render_widget(
        Paragraph::new(left_full).style(Style::default().add_modifier(Modifier::BOLD)),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(right_text)
            .alignment(Alignment::Right)
            .style(Style::default().fg(Color::Yellow)),
        right_area,
    );
}

// --- QuickCd helpers ---

impl App {
    fn open_quick_cd_dialog(&mut self) {
        let base_path = self.active_panel().path.clone();
        let matches = quick_cd_list_dirs(&base_path, "");
        self.open_dialog(DialogState::QuickCd {
            input: String::new(),
            matches,
            selected: 0,
            base_path,
        });
    }

    fn refresh_quick_cd(&mut self) {
        if let Some(DialogState::QuickCd {
            input,
            matches,
            selected,
            base_path,
        }) = &mut self.dialog
        {
            let (parent, filter) = quick_cd_parse_input(input, base_path);
            *matches = quick_cd_list_dirs(&parent, &filter);
            *selected = 0;
        }
    }

    fn quick_cd_complete_input(&self) -> Option<String> {
        let DialogState::QuickCd {
            input,
            matches,
            selected,
            ..
        } = self.dialog.as_ref()?
        else {
            return None;
        };
        let name = matches.get(*selected)?;
        // Replace the filter part (after last `/`) with `name/`
        let prefix_len = input.rfind('/').map(|i| i + 1).unwrap_or(0);
        Some(format!("{}{}/", &input[..prefix_len], name))
    }
}

fn quick_cd_parse_input(input: &str, base: &std::path::Path) -> (std::path::PathBuf, String) {
    let expanded = quick_cd_expand(input);
    if let Some(last_slash) = expanded.rfind('/') {
        let parent_str = &expanded[..=last_slash];
        let filter = expanded[last_slash + 1..].to_string();
        let parent = if parent_str.starts_with('/') {
            std::path::PathBuf::from(parent_str)
        } else {
            base.join(parent_str)
        };
        (parent, filter)
    } else {
        (base.to_path_buf(), expanded)
    }
}

fn quick_cd_list_dirs(parent: &std::path::Path, filter: &str) -> Vec<String> {
    let filter_lower = filter.to_lowercase();
    let mut dirs: Vec<String> = std::fs::read_dir(parent)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| filter.is_empty() || name.to_lowercase().contains(&filter_lower))
        .collect();
    dirs.sort_by_key(|a| a.to_lowercase());
    dirs
}

fn quick_cd_destination(
    input: &str,
    matches: &[String],
    selected: usize,
    base: &std::path::Path,
) -> std::path::PathBuf {
    if !matches.is_empty() {
        let (parent, _) = quick_cd_parse_input(input, base);
        parent.join(&matches[selected.min(matches.len() - 1)])
    } else {
        // No matches — try to navigate to the raw input.
        let expanded = quick_cd_expand(input);
        if expanded.starts_with('/') {
            std::path::PathBuf::from(expanded)
        } else {
            base.join(expanded)
        }
    }
}

fn quick_cd_expand(s: &str) -> String {
    if let Some(stripped) = s.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        format!("{}{}", home, stripped)
    } else {
        s.to_string()
    }
}

// --- General helpers ---

fn format_status_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut val = bytes as f64;
    let mut unit = 0;
    while val >= 1000.0 && unit + 1 < UNITS.len() {
        val /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", val, UNITS[unit])
    }
}

fn target_label(sources: &[std::path::PathBuf]) -> String {
    if sources.len() == 1 {
        sources[0]
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        format!("{} files", sources.len())
    }
}
