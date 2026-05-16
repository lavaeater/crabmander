use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{prelude::*, widgets::Clear};
use ratatui_textarea::TextArea;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::{
    action::{Action, BranchInfo, Side},
    components::{
        context_menu::{builtin_providers, ContextMenuProvider, MenuCtx},
        dialog::{self, DialogState, MenuAction, MenuItem},
        func_bar,
        git_view::GitView,
        panel::{Panel, copy_recursive, delete_recursive, extract_archive_sync, file_name_of},
    },
    config::{Config, get_data_dir},
    ops::{DeferredOp, OpCtx},
    palette::Palette,
    recent_dirs,
    tui::{Event, Tui},
};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Normal,
    Dialog,
    Git,
    GitCommit,
    GitBranch,
}

pub struct App {
    config: Config,
    palette: Palette,
    tick_rate: f64,
    frame_rate: f64,
    left: Panel,
    right: Panel,
    active: Side,
    dialog: Option<DialogState>,
    mode: Mode,
    last_error: Option<String>,
    /// Session-wide cache of recursively-computed directory sizes.
    /// Invalidated on any mutating file operation.
    dir_size_cache: std::collections::HashMap<std::path::PathBuf, u64>,
    /// Command to run after suspending the TUI (set by Execute action).
    /// Tuple: (cmd, args, sides_to_reload_after).
    pending_command: Option<(String, Vec<String>, Vec<Side>)>,
    git_view: Option<GitView>,
    commit_textarea: Option<TextArea<'static>>,
    branch_list: Vec<BranchInfo>,
    branch_cursor: usize,
    git_watcher: Option<tokio_util::sync::CancellationToken>,
    providers: Vec<Box<dyn ContextMenuProvider>>,
    recent_dirs: Vec<std::path::PathBuf>,
    /// Active long-running operations: (id, label, done, total).
    ops: Vec<(u64, String, u64, u64)>,
    next_op_id: u64,
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

        let config = Config::new()?;
        let opaline_theme = opaline::builtins::load_by_name(&config.theme)
            .unwrap_or_else(|| {
                tracing::warn!("unknown theme {:?}, falling back to catppuccin-mocha", config.theme);
                opaline::builtins::load_by_name("catppuccin-mocha").expect("builtin must exist")
            });
        let palette = Palette::from(&opaline_theme);

        Ok(Self {
            config,
            palette,
            tick_rate,
            frame_rate,
            left,
            right,
            active: Side::Left,
            dialog: None,
            mode: Mode::Normal,
            last_error: None,
            dir_size_cache: std::collections::HashMap::new(),
            pending_command: None::<(String, Vec<String>, Vec<Side>)>,
            git_view: None,
            commit_textarea: None,
            branch_list: Vec::new(),
            branch_cursor: 0,
            git_watcher: None,
            providers: builtin_providers(),
            recent_dirs: recent_dirs::load(&get_data_dir()),
            ops: Vec::new(),
            next_op_id: 0,
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
            if let Some((cmd, args, reload)) = self.pending_command.take() {
                tui.exit()?;
                println!("\nRunning: {} {}\n", cmd, args.join(" "));
                std::process::Command::new(&cmd).args(&args).status().ok();
                println!("\nPress Enter to continue...");
                let mut _s = String::new();
                std::io::stdin().read_line(&mut _s).ok();
                tui.enter()?;
                action_tx.send(Action::ClearScreen)?;
                for side in reload {
                    self.get_panel(side).reload();
                }
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

        if self.mode == Mode::GitCommit {
            match key.code {
                KeyCode::Esc => tx.send(Action::GitCommitCancel)?,
                KeyCode::Enter
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        || key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    tx.send(Action::GitCommitSubmit)?;
                }
                _ => {
                    if let Some(ta) = &mut self.commit_textarea {
                        ta.input(key);
                    }
                }
            }
            return Ok(());
        }

        if self.mode == Mode::GitBranch {
            match key.code {
                KeyCode::Esc => tx.send(Action::ExitGitMode)?,
                KeyCode::Up | KeyCode::Char('k') => tx.send(Action::GitBranchNavUp)?,
                KeyCode::Down | KeyCode::Char('j') => tx.send(Action::GitBranchNavDown)?,
                KeyCode::Enter => tx.send(Action::GitBranchConfirm)?,
                KeyCode::Char('n') => tx.send(Action::GitNewBranch)?,
                _ => {}
            }
            return Ok(());
        }

        if self.mode == Mode::Git {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => tx.send(Action::ExitGitMode)?,
                KeyCode::Up | KeyCode::Char('k') => tx.send(Action::GitNavUp)?,
                KeyCode::Down | KeyCode::Char('j') => tx.send(Action::GitNavDown)?,
                KeyCode::Tab => tx.send(Action::GitSwitchPane)?,
                KeyCode::Char(' ') | KeyCode::Insert => tx.send(Action::GitToggleMark)?,
                KeyCode::F(1) | KeyCode::Char('a') => tx.send(Action::GitStage)?,
                KeyCode::F(2) | KeyCode::Char('u') => tx.send(Action::GitUnstage)?,
                KeyCode::F(3) | KeyCode::Char('c') => tx.send(Action::GitCommit)?,
                KeyCode::F(4) | KeyCode::Char('p') => tx.send(Action::GitPush)?,
                KeyCode::F(5) | KeyCode::Char('P') => tx.send(Action::GitPull)?,
                KeyCode::F(6) | KeyCode::Char('b') => tx.send(Action::GitListBranches)?,
                KeyCode::F(7) | KeyCode::Char('A') => tx.send(Action::GitAddAllAndCommit)?,
                KeyCode::Char('n') => tx.send(Action::GitNewBranch)?,
                KeyCode::Char('r') => tx.send(Action::GitReload)?,
                _ => {}
            }
            return Ok(());
        }

        // Ctrl+G enters git mode when the active panel is inside a git repo.
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('g') {
            if self.active_panel().git_branch.is_some() {
                tx.send(Action::EnterGitMode)?;
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
                Action::OpErrors(errors) => {
                    let title = format!("{} error(s)", errors.len());
                    self.open_dialog(DialogState::error_list(title, errors));
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
                Action::CalcSizes => {
                    // Collect relevant cache entries (direct children of this panel's path)
                    // into a temporary map to avoid a simultaneous borrow on self.
                    let panel_path = self.active_panel().path.clone();
                    let cache: std::collections::HashMap<_, _> = self
                        .dir_size_cache
                        .iter()
                        .filter(|(p, _)| p.parent() == Some(panel_path.as_path()))
                        .map(|(p, &s)| (p.clone(), s))
                        .collect();
                    self.active_panel_mut().start_size_calc(&cache);
                }
                Action::CycleSortMode => self.active_panel_mut().cycle_sort_mode(),
                Action::InvertSort => self.active_panel_mut().invert_sort(),
                Action::ContextMenu => self.open_context_menu(),

                // Dir size results (from F4) — write to session cache then update panel.
                Action::DirSizeResult {
                    side,
                    panel_path,
                    name,
                    size,
                } => {
                    self.dir_size_cache.insert(panel_path.join(&name), size);
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
                Action::GitInfoLoaded {
                    side,
                    path,
                    branch,
                    is_dirty,
                } => {
                    self.get_panel_mut(side).on_git_info_loaded(&path, branch, is_dirty);
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
                Action::ExecuteFile { cmd, args, reload } => {
                    self.pending_command = Some((cmd, args, reload));
                }
                Action::OpCompleted(sides) => {
                    self.dir_size_cache.clear(); // sizes are stale after any mutation
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

                Action::SelectTheme => self.open_theme_selector(),
                Action::RecentDirs => self.open_recent_dirs_menu(),

                // Git mode
                Action::EnterGitMode => self.enter_git_mode(),
                Action::ExitGitMode => self.exit_git_mode(),
                Action::GitNavUp => {
                    if let Some(gv) = &mut self.git_view { gv.nav_up(); }
                }
                Action::GitNavDown => {
                    if let Some(gv) = &mut self.git_view { gv.nav_down(); }
                }
                Action::GitSwitchPane => {
                    if let Some(gv) = &mut self.git_view { gv.switch_pane(); }
                }
                Action::GitToggleMark => {
                    if let Some(gv) = &mut self.git_view { gv.toggle_mark(); }
                }
                Action::GitAddAllAndCommit => self.do_git_add_all_and_commit(),
                Action::GitAddAllDone => self.open_commit_textarea(),
                Action::GitStage => self.do_git_stage(),
                Action::GitUnstage => self.do_git_unstage(),
                Action::GitCommit => self.do_git_commit(),
                Action::GitCommitSubmit => self.submit_git_commit(),
                Action::GitCommitCancel => self.cancel_git_commit(),
                Action::GitPush => self.do_git_push(),
                Action::GitPull => self.do_git_pull(),
                Action::GitListBranches => self.do_git_list_branches(),
                Action::GitNewBranch => self.open_new_branch_dialog(),
                Action::GitBranchNavUp => {
                    self.branch_cursor = self.branch_cursor.saturating_sub(1);
                }
                Action::GitBranchNavDown => {
                    let max = self.branch_list.len().saturating_sub(1);
                    self.branch_cursor = (self.branch_cursor + 1).min(max);
                }
                Action::GitBranchConfirm => self.do_git_checkout_branch(),
                Action::GitBranchesLoaded { branches } => {
                    self.branch_cursor = branches
                        .iter()
                        .position(|b| b.is_current)
                        .unwrap_or(0);
                    self.branch_list = branches;
                    self.mode = Mode::GitBranch;
                }
                Action::Progress { id, label, done, total } => {
                    if let Some(op) = self.ops.iter_mut().find(|o| o.0 == id) {
                        op.2 = done;
                        op.3 = total;
                    } else {
                        self.ops.push((id, label, done, total));
                    }
                }
                Action::ProgressDone(id) => {
                    self.ops.retain(|o| o.0 != id);
                }

                Action::GitReload | Action::GitOpCompleted => {
                    if let Some(gv) = &self.git_view {
                        GitView::load_status(gv.git_root.clone(), self.action_tx.clone());
                    }
                }
                Action::GitStatusLoaded { git_root: _, branch, entries } => {
                    if let Some(gv) = &mut self.git_view {
                        gv.on_status_loaded(branch, entries);
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
        let git_view = &mut self.git_view;
        let git_mode = matches!(self.mode, Mode::Git | Mode::GitCommit | Mode::GitBranch);
        let commit_textarea = &mut self.commit_textarea;
        let branch_list = &self.branch_list;
        let branch_cursor = self.branch_cursor;
        let show_branches = self.mode == Mode::GitBranch;
        let palette = &self.palette;
        let ops = &self.ops;

        tui.draw(|frame| {
            let area = frame.area();
            let [panels_area, status_area, func_area] = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(area);

            if git_mode {
                if let Some(gv) = git_view.as_mut() {
                    draw_git_status_bar(frame, status_area, gv, last_error.as_deref(), palette);
                    gv.draw(frame, panels_area, palette);
                }
                if let Some(ta) = commit_textarea {
                    draw_commit_textarea(frame, panels_area, ta);
                }
                if show_branches {
                    draw_branch_popup(frame, panels_area, branch_list, branch_cursor, palette);
                }
            } else {
                let [left_area, right_area] =
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .areas(panels_area);

                left.draw(frame, left_area, palette);
                right.draw(frame, right_area, palette);

                let active_panel = if active == Side::Left { &*left } else { &*right };
                draw_status_bar(frame, status_area, active_panel, last_error.as_deref(), palette);
            }

            func_bar::draw(frame, func_area, git_mode, palette);

            if let Some(d) = dialog {
                dialog::draw(frame, d, area, palette);
            }

            draw_ops_overlay(frame, area, ops, palette);
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
        let active = self.active;
        self.open_dialog(DialogState::input(
            "Copy",
            format!("Copy {} to:", label),
            dest,
            Box::new(OpCopy { sources, active }),
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
        let active = self.active;
        self.open_dialog(DialogState::input(
            "Move",
            format!("Move {} to:", label),
            dest,
            Box::new(OpMove { sources, active }),
        ));
    }

    fn open_mkdir_dialog(&mut self) {
        let base = self.active_panel().path.clone();
        let active = self.active;
        self.open_dialog(DialogState::input(
            "Make Directory",
            "New directory name:",
            "",
            Box::new(OpMkdir { base, active }),
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
        let active = self.active;
        self.open_dialog(DialogState::confirm(
            "Delete",
            msg,
            Box::new(OpDelete { paths: sources, active }),
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
            Box::new(OpOpenInNano { base }),
        ));
    }

    fn open_recent_dirs_menu(&mut self) {
        if self.recent_dirs.is_empty() {
            self.last_error = Some("No recent directories — use F1 to navigate first".into());
            return;
        }
        let items: Vec<MenuItem> = self
            .recent_dirs
            .iter()
            .map(|p| {
                MenuItem::new(
                    p.to_string_lossy().into_owned(),
                    MenuAction::CdTo(p.clone()),
                )
            })
            .collect();
        self.open_dialog(DialogState::context_menu("Recent Directories", items));
    }

    fn open_theme_selector(&mut self) {
        let current = self.config.theme.clone();
        let mut names: Vec<(&'static str, &'static str)> =
            opaline::builtins::builtin_names().to_vec();
        names.sort_by_key(|(_, display)| *display);

        let items: Vec<MenuItem> = names
            .into_iter()
            .map(|(id, display)| {
                let label = if id == current.as_str() {
                    format!("{} ✓", display)
                } else {
                    display.to_string()
                };
                MenuItem::new(label, MenuAction::SetTheme(id.to_string()))
            })
            .collect();

        self.open_dialog(DialogState::context_menu("Select Theme", items));
    }

    fn open_context_menu(&mut self) {
        let Some(entry) = self.active_panel().current_entry() else { return };
        if entry.name == ".." { return; }

        let ctx = MenuCtx {
            entry_path: self.active_panel().path.join(&entry.name),
            panel_dir: self.active_panel().path.clone(),
            other_dir: self.get_panel(self.active.other()).path.clone(),
            effective_targets: self.active_panel().effective_targets(),
            active_side: self.active,
            entry: entry.clone(),
        };
        let title = format!("Actions: {}", ctx.entry.name);

        let items: Vec<MenuItem> = self.providers
            .iter()
            .flat_map(|p| p.items(&ctx))
            .collect();

        self.open_dialog(DialogState::context_menu(title, items));
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
                self.execute_op(op, String::new());
            }
            DialogState::Input { value, op, .. } => {
                self.last_error = None;
                self.mode = if op.stays_in_git_mode() { Mode::Git } else { Mode::Normal };
                self.execute_op(op, value);
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
                recent_dirs::push(&mut self.recent_dirs, dest.clone());
                recent_dirs::save(&get_data_dir(), &self.recent_dirs);
                let side = self.active;
                Panel::load_dir(dest, side, self.action_tx.clone());
            }
            DialogState::ErrorList { .. } => {
                self.mode = Mode::Normal;
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
                    Box::new(OpExecute { path }),
                ));
            }
            MenuAction::ExtractHere { archives, dest } => {
                let active = self.active;
                let tx = self.action_tx.clone();
                let id = self.next_op_id;
                self.next_op_id += 1;
                let total = archives.len() as u64;
                tokio::spawn(async move {
                    let _ = tx.send(Action::Progress {
                        id,
                        label: "Extracting…".into(),
                        done: 0,
                        total,
                    });
                    let mut errors: Vec<String> = Vec::new();
                    let mut succeeded = 0usize;
                    for (i, archive) in archives.into_iter().enumerate() {
                        let dest2 = dest.clone();
                        match tokio::task::spawn_blocking(move || {
                            extract_archive_sync(&archive, &dest2)
                        })
                        .await
                        {
                            Ok(Ok(())) => succeeded += 1,
                            Ok(Err(e)) => errors.push(e.to_string()),
                            Err(e) => errors.push(e.to_string()),
                        }
                        let _ = tx.send(Action::Progress {
                            id,
                            label: "Extracting…".into(),
                            done: (i + 1) as u64,
                            total,
                        });
                    }
                    let _ = tx.send(Action::ProgressDone(id));
                    if succeeded > 0 {
                        let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
                    }
                    if !errors.is_empty() {
                        let _ = tx.send(Action::OpErrors(errors));
                    }
                });
            }
            MenuAction::MountDevice { device } => {
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::process::Command::new("udisksctl")
                        .args(["mount", "-b", &format!("/dev/{}", device)])
                        .output()
                        .await;
                    match result {
                        Ok(out) if out.status.success() => {}
                        Ok(out) => {
                            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                            let _ = tx.send(Action::OpError(msg));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::OpError(e.to_string()));
                        }
                    }
                });
            }
            MenuAction::CdTo(path) => {
                let side = self.active;
                Panel::load_dir(path, side, self.action_tx.clone());
            }
            MenuAction::SetTheme(id) => {
                if let Some(theme) = opaline::builtins::load_by_name(&id) {
                    self.palette = Palette::from(&theme);
                    self.config.theme = id;
                }
            }
            MenuAction::UnmountDevice { device } => {
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::process::Command::new("udisksctl")
                        .args(["unmount", "-b", &format!("/dev/{}", device)])
                        .output()
                        .await;
                    match result {
                        Ok(out) if out.status.success() => {}
                        Ok(out) => {
                            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                            let _ = tx.send(Action::OpError(msg));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::OpError(e.to_string()));
                        }
                    }
                });
            }

            MenuAction::Chown { paths, current_owner, reload_sides } => {
                self.open_dialog(DialogState::input(
                    "Change Owner",
                    format!(
                        "New owner for {} item(s)\n(current: {})\nFormat: user  or  user:group",
                        paths.len(),
                        current_owner
                    ),
                    current_owner,
                    Box::new(OpChown { paths, reload_sides }),
                ));
            }
        }
    }

    fn make_op_ctx(&mut self, input: String) -> OpCtx {
        let op_id = self.next_op_id;
        self.next_op_id += 1;
        OpCtx { tx: self.action_tx.clone(), op_id, input }
    }

    fn execute_op(&mut self, op: Box<dyn DeferredOp>, input: String) {
        let ctx = self.make_op_ctx(input);
        op.execute(ctx);
    }

    // --- Async file ops ---

    // Called from ExecuteDelete/ExecuteCopy/ExecuteMove/ExecuteMkdir actions (legacy path).
    fn do_delete(&mut self, paths: Vec<std::path::PathBuf>) {
        let tx = self.action_tx.clone();
        let active = self.active;
        let id = self.next_op_id;
        self.next_op_id += 1;
        let total = paths.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(Action::Progress { id, label: "Deleting…".into(), done: 0, total });
            for (i, path) in paths.iter().enumerate() {
                if let Err(e) = delete_recursive(path).await {
                    let _ = tx.send(Action::ProgressDone(id));
                    let _ = tx.send(Action::OpError(e.to_string()));
                    return;
                }
                let _ = tx.send(Action::Progress { id, label: "Deleting…".into(), done: (i + 1) as u64, total });
            }
            let _ = tx.send(Action::ProgressDone(id));
            let _ = tx.send(Action::OpCompleted(vec![active]));
        });
    }

    fn do_copy(&mut self, sources: Vec<std::path::PathBuf>, dest: std::path::PathBuf, active: Side) {
        let tx = self.action_tx.clone();
        let id = self.next_op_id;
        self.next_op_id += 1;
        let total = sources.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(Action::Progress { id, label: "Copying…".into(), done: 0, total });
            let mut errors: Vec<String> = Vec::new();
            let mut succeeded = 0usize;
            for (i, src) in sources.iter().enumerate() {
                let name = match file_name_of(src) {
                    Ok(n) => n,
                    Err(e) => {
                        errors.push(format!("{}: {}", src.display(), e));
                        let _ = tx.send(Action::Progress { id, label: "Copying…".into(), done: (i + 1) as u64, total });
                        continue;
                    }
                };
                if let Err(e) = copy_recursive(src, &dest.join(&name)).await {
                    errors.push(format!("{}: {}", src.display(), e));
                } else {
                    succeeded += 1;
                }
                let _ = tx.send(Action::Progress { id, label: "Copying…".into(), done: (i + 1) as u64, total });
            }
            let _ = tx.send(Action::ProgressDone(id));
            if succeeded > 0 {
                let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
            }
            if !errors.is_empty() {
                let _ = tx.send(Action::OpErrors(errors));
            }
        });
    }

    fn do_move(&mut self, sources: Vec<std::path::PathBuf>, dest: std::path::PathBuf, active: Side) {
        let tx = self.action_tx.clone();
        let id = self.next_op_id;
        self.next_op_id += 1;
        let total = sources.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(Action::Progress { id, label: "Moving…".into(), done: 0, total });
            let mut errors: Vec<String> = Vec::new();
            let mut succeeded = 0usize;
            for (i, src) in sources.iter().enumerate() {
                let name = match file_name_of(src) {
                    Ok(n) => n,
                    Err(e) => {
                        errors.push(format!("{}: {}", src.display(), e));
                        let _ = tx.send(Action::Progress { id, label: "Moving…".into(), done: (i + 1) as u64, total });
                        continue;
                    }
                };
                let dest_full_path = dest.join(&name);
                match tokio::fs::rename(src, &dest_full_path).await {
                    Ok(()) => succeeded += 1,
                    Err(e) => {
                        let detail = if e.raw_os_error() == Some(libc::EXDEV) {
                            format!("{}: cannot move across filesystems (use Copy then Delete)", src.display())
                        } else {
                            format!("{}: {}", src.display(), e)
                        };
                        errors.push(detail);
                    }
                }
                let _ = tx.send(Action::Progress { id, label: "Moving…".into(), done: (i + 1) as u64, total });
            }
            let _ = tx.send(Action::ProgressDone(id));
            if succeeded > 0 {
                let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
            }
            if !errors.is_empty() {
                let _ = tx.send(Action::OpErrors(errors));
            }
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

    // --- Git mode ---

    fn enter_git_mode(&mut self) {
        let panel = self.active_panel();
        let branch = panel.git_branch.clone().unwrap_or_default();
        let Some(git_root) = find_git_root(&panel.path) else { return };
        let gv = GitView::new(git_root.clone(), branch);
        GitView::load_status(git_root.clone(), self.action_tx.clone());
        self.git_view = Some(gv);
        self.mode = Mode::Git;
        self.last_error = None;
        self.git_watcher = Some(start_git_watcher(git_root, self.action_tx.clone()));
    }

    fn exit_git_mode(&mut self) {
        if let Some(token) = self.git_watcher.take() {
            token.cancel();
        }
        self.mode = Mode::Normal;
        self.git_view = None;
        // Reload both panels in case git ops changed the filesystem.
        self.left.reload();
        self.right.reload();
    }

    fn do_git_stage(&mut self) {
        let Some(gv) = &self.git_view else { return };
        let targets = gv.stage_targets();
        if targets.is_empty() { return; }
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let mut index = repo.index()?;
                for path in &targets {
                    index.add_path(std::path::Path::new(path))?;
                }
                index.write()?;
                Ok::<_, git2::Error>(())
            })
            .await;
            git_op_result(result, &tx);
        });
    }

    fn do_git_unstage(&mut self) {
        let Some(gv) = &self.git_view else { return };
        let targets = gv.unstage_targets();
        if targets.is_empty() { return; }
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let head_obj = repo
                    .head()
                    .ok()
                    .and_then(|h| h.peel(git2::ObjectType::Commit).ok());
                repo.reset_default(head_obj.as_ref(), targets.iter())?;
                Ok::<_, git2::Error>(())
            })
            .await;
            git_op_result(result, &tx);
        });
    }

    fn do_git_commit(&mut self) {
        let Some(gv) = &self.git_view else { return };
        if !gv.has_staged() {
            self.last_error = Some("Nothing staged to commit".into());
            return;
        }
        self.open_commit_textarea();
    }

    fn open_commit_textarea(&mut self) {
        let mut ta = TextArea::default();
        ta.set_block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(" Commit message — Ctrl+Enter or Alt+Enter to commit, Esc to cancel "),
        );
        ta.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        ta.set_placeholder_text("Summary line\n\nOptional body…");
        self.commit_textarea = Some(ta);
        self.mode = Mode::GitCommit;
    }

    fn do_git_add_all_and_commit(&mut self) {
        let Some(gv) = &self.git_view else { return };
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let mut index = repo.index()?;
                // Equivalent to `git add -A`: stage new/modified files and
                // remove index entries for files deleted from the working tree.
                index.add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)?;
                index.update_all(["."].iter(), None)?;
                index.write()?;
                Ok::<_, git2::Error>(())
            })
            .await;
            match result {
                Ok(Ok(())) => { let _ = tx.send(Action::GitAddAllDone); }
                Ok(Err(e)) => { let _ = tx.send(Action::OpError(e.to_string())); }
                Err(e)     => { let _ = tx.send(Action::OpError(e.to_string())); }
            }
        });
    }

    fn submit_git_commit(&mut self) {
        let Some(ta) = self.commit_textarea.take() else { return };
        self.mode = Mode::Git;
        let msg = ta.lines().join("\n");
        if msg.trim().is_empty() {
            self.last_error = Some("Commit message is empty".into());
            return;
        }
        let Some(gv) = &self.git_view else { return };
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let sig = repo.signature()?;
                let mut index = repo.index()?;
                let tree_id = index.write_tree()?;
                let tree = repo.find_tree(tree_id)?;
                let parent: Option<git2::Commit> =
                    repo.head().ok().and_then(|h| h.peel_to_commit().ok());
                let parents: Vec<&git2::Commit> = parent.iter().collect();
                repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parents)?;
                Ok::<_, git2::Error>(())
            })
            .await;
            git_op_result(result, &tx);
        });
    }

    fn cancel_git_commit(&mut self) {
        self.commit_textarea = None;
        self.mode = Mode::Git;
    }

    fn do_git_push(&mut self) {
        let Some(gv) = &self.git_view else { return };
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        let id = self.next_op_id;
        self.next_op_id += 1;
        let _ = tx.send(Action::Progress { id, label: "Pushing…".into(), done: 0, total: 0 });
        tokio::spawn(async move {
            let tx_inner = tx.clone();
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let branch = repo
                    .head()?
                    .shorthand()
                    .map(str::to_owned)
                    .unwrap_or_else(|| "main".into());
                let mut remote = repo.find_remote("origin")?;
                let mut callbacks = git2::RemoteCallbacks::new();
                callbacks.credentials(git2_cred_callback);
                let tx_cb = tx_inner.clone();
                callbacks.push_transfer_progress(move |current, total, _bytes| {
                    let _ = tx_cb.send(Action::Progress {
                        id,
                        label: "Pushing…".into(),
                        done: current as u64,
                        total: total as u64,
                    });
                });
                let mut push_opts = git2::PushOptions::new();
                push_opts.remote_callbacks(callbacks);
                let refspec = format!("refs/heads/{0}:refs/heads/{0}", branch);
                remote.push(&[refspec.as_str()], Some(&mut push_opts))?;
                Ok::<_, git2::Error>(())
            })
            .await;
            let _ = tx.send(Action::ProgressDone(id));
            git_op_result(result, &tx);
        });
    }

    fn do_git_pull(&mut self) {
        let Some(gv) = &self.git_view else { return };
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        let id = self.next_op_id;
        self.next_op_id += 1;
        let _ = tx.send(Action::Progress { id, label: "Pulling…".into(), done: 0, total: 0 });
        tokio::spawn(async move {
            let tx_inner = tx.clone();
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let branch = repo
                    .head()?
                    .shorthand()
                    .map(str::to_owned)
                    .unwrap_or_else(|| "main".into());
                let mut remote = repo.find_remote("origin")?;
                let mut callbacks = git2::RemoteCallbacks::new();
                callbacks.credentials(git2_cred_callback);
                let tx_cb = tx_inner.clone();
                callbacks.transfer_progress(move |stats| {
                    let _ = tx_cb.send(Action::Progress {
                        id,
                        label: "Pulling…".into(),
                        done: stats.received_objects() as u64,
                        total: stats.total_objects() as u64,
                    });
                    true
                });
                let mut fetch_opts = git2::FetchOptions::new();
                fetch_opts.remote_callbacks(callbacks);
                remote.fetch(&[] as &[&str], Some(&mut fetch_opts), None)?;

                let fetch_head = repo.find_reference("FETCH_HEAD")?;
                let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
                let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

                if analysis.is_up_to_date() {
                    return Ok(());
                }
                if analysis.is_fast_forward() {
                    let refname = format!("refs/heads/{}", branch);
                    let mut reference = repo.find_reference(&refname)?;
                    reference.set_target(fetch_commit.id(), "pull: Fast-forward")?;
                    repo.set_head(&refname)?;
                    repo.checkout_head(Some(
                        git2::build::CheckoutBuilder::default().force(),
                    ))?;
                } else {
                    return Err(git2::Error::from_str(
                        "pull requires a merge commit — not supported here",
                    ));
                }
                Ok::<_, git2::Error>(())
            })
            .await;
            let _ = tx.send(Action::ProgressDone(id));
            git_op_result(result, &tx);
        });
    }
    fn open_new_branch_dialog(&mut self) {
        let Some(gv) = &self.git_view else { return };
        let git_root = gv.git_root.clone();
        // Close the branch picker if open, then show the input dialog.
        self.branch_list.clear();
        self.open_dialog(DialogState::input(
            "New Branch",
            "Branch name:",
            String::new(),
            Box::new(OpGitCreateBranch { git_root }),
        ));
    }

    fn do_git_list_branches(&mut self) {
        let Some(gv) = &self.git_view else { return };
        GitView::load_branches(gv.git_root.clone(), self.action_tx.clone());
    }

    fn do_git_checkout_branch(&mut self) {
        let Some(info) = self.branch_list.get(self.branch_cursor).cloned() else { return };
        let Some(gv) = &self.git_view else { return };
        let git_root = gv.git_root.clone();
        let tx = self.action_tx.clone();
        self.mode = Mode::Git;
        self.branch_list.clear();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                if info.is_local {
                    // Checkout existing local branch.
                    let obj =
                        repo.revparse_single(&format!("refs/heads/{}", info.name))?;
                    repo.checkout_tree(&obj, None)?;
                    repo.set_head(&format!("refs/heads/{}", info.name))?;
                } else {
                    // Remote-only: create a local tracking branch then checkout.
                    let remote_ref_owned = info
                        .remote_ref
                        .clone()
                        .unwrap_or_else(|| format!("origin/{}", info.name));
                    let remote_branch =
                        repo.find_branch(&remote_ref_owned, git2::BranchType::Remote)?;
                    let commit = remote_branch.get().peel_to_commit()?;
                    let mut local = repo.branch(&info.name, &commit, false)?;
                    local.set_upstream(Some(&remote_ref_owned))?;
                    let obj =
                        repo.revparse_single(&format!("refs/heads/{}", info.name))?;
                    repo.checkout_tree(&obj, None)?;
                    repo.set_head(&format!("refs/heads/{}", info.name))?;
                }
                Ok::<_, git2::Error>(())
            })
            .await;
            git_op_result(result, &tx);
        });
    }
}

// --- DeferredOp implementations ---

#[derive(Debug)]
struct OpDelete {
    paths: Vec<std::path::PathBuf>,
    active: Side,
}
impl DeferredOp for OpDelete {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let tx = ctx.tx;
        let active = self.active;
        let id = ctx.op_id;
        let total = self.paths.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(Action::Progress { id, label: "Deleting…".into(), done: 0, total });
            for (i, path) in self.paths.iter().enumerate() {
                if let Err(e) = delete_recursive(path).await {
                    let _ = tx.send(Action::ProgressDone(id));
                    let _ = tx.send(Action::OpError(e.to_string()));
                    return;
                }
                let _ = tx.send(Action::Progress { id, label: "Deleting…".into(), done: (i + 1) as u64, total });
            }
            let _ = tx.send(Action::ProgressDone(id));
            let _ = tx.send(Action::OpCompleted(vec![active]));
        });
    }
}

#[derive(Debug)]
struct OpCopy {
    sources: Vec<std::path::PathBuf>,
    active: Side,
}
impl DeferredOp for OpCopy {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let dest: std::path::PathBuf = ctx.input.into();
        let tx = ctx.tx;
        let active = self.active;
        let id = ctx.op_id;
        let total = self.sources.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(Action::Progress { id, label: "Copying…".into(), done: 0, total });
            let mut errors: Vec<String> = Vec::new();
            let mut succeeded = 0usize;
            for (i, src) in self.sources.iter().enumerate() {
                let name = match file_name_of(src) {
                    Ok(n) => n,
                    Err(e) => {
                        errors.push(format!("{}: {}", src.display(), e));
                        let _ = tx.send(Action::Progress { id, label: "Copying…".into(), done: (i + 1) as u64, total });
                        continue;
                    }
                };
                if let Err(e) = copy_recursive(src, &dest.join(&name)).await {
                    errors.push(format!("{}: {}", src.display(), e));
                } else {
                    succeeded += 1;
                }
                let _ = tx.send(Action::Progress { id, label: "Copying…".into(), done: (i + 1) as u64, total });
            }
            let _ = tx.send(Action::ProgressDone(id));
            if succeeded > 0 {
                let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
            }
            if !errors.is_empty() {
                let _ = tx.send(Action::OpErrors(errors));
            }
        });
    }
}

#[derive(Debug)]
struct OpMove {
    sources: Vec<std::path::PathBuf>,
    active: Side,
}
impl DeferredOp for OpMove {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let dest: std::path::PathBuf = ctx.input.into();
        let tx = ctx.tx;
        let active = self.active;
        let id = ctx.op_id;
        let total = self.sources.len() as u64;
        tokio::spawn(async move {
            let _ = tx.send(Action::Progress { id, label: "Moving…".into(), done: 0, total });
            let mut errors: Vec<String> = Vec::new();
            let mut succeeded = 0usize;
            for (i, src) in self.sources.iter().enumerate() {
                let name = match file_name_of(src) {
                    Ok(n) => n,
                    Err(e) => {
                        errors.push(format!("{}: {}", src.display(), e));
                        let _ = tx.send(Action::Progress { id, label: "Moving…".into(), done: (i + 1) as u64, total });
                        continue;
                    }
                };
                let dest_full = dest.join(&name);
                match tokio::fs::rename(src, &dest_full).await {
                    Ok(()) => succeeded += 1,
                    Err(e) => {
                        let detail = if e.raw_os_error() == Some(libc::EXDEV) {
                            format!("{}: cannot move across filesystems (use Copy then Delete)", src.display())
                        } else {
                            format!("{}: {}", src.display(), e)
                        };
                        errors.push(detail);
                    }
                }
                let _ = tx.send(Action::Progress { id, label: "Moving…".into(), done: (i + 1) as u64, total });
            }
            let _ = tx.send(Action::ProgressDone(id));
            if succeeded > 0 {
                let _ = tx.send(Action::OpCompleted(vec![active, active.other()]));
            }
            if !errors.is_empty() {
                let _ = tx.send(Action::OpErrors(errors));
            }
        });
    }
}

#[derive(Debug)]
struct OpMkdir {
    base: std::path::PathBuf,
    active: Side,
}
impl DeferredOp for OpMkdir {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let path = self.base.join(ctx.input.trim());
        let tx = ctx.tx;
        let active = self.active;
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::create_dir(&path).await {
                let _ = tx.send(Action::OpError(e.to_string()));
                return;
            }
            let _ = tx.send(Action::OpCompleted(vec![active]));
        });
    }
}

#[derive(Debug)]
struct OpOpenInNano {
    base: std::path::PathBuf,
}
impl DeferredOp for OpOpenInNano {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let filename = ctx.input;
        if !filename.is_empty() {
            let path = self.base.join(&filename).to_string_lossy().into_owned();
            let _ = ctx.tx.send(Action::ExecuteFile {
                cmd: "nano".into(),
                args: vec![path],
                reload: vec![],
            });
        }
    }
}

#[derive(Debug)]
struct OpExecute {
    path: std::path::PathBuf,
}
impl DeferredOp for OpExecute {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let cmd = self.path.to_string_lossy().into_owned();
        let args: Vec<String> = ctx.input.split_whitespace().map(String::from).collect();
        let _ = ctx.tx.send(Action::ExecuteFile { cmd, args, reload: vec![] });
    }
}

#[derive(Debug)]
struct OpChown {
    paths: Vec<std::path::PathBuf>,
    reload_sides: Vec<Side>,
}
impl DeferredOp for OpChown {
    fn execute(self: Box<Self>, ctx: OpCtx) {
        let new_owner = ctx.input;
        if !new_owner.is_empty() {
            let recursive = self.paths.iter().any(|p| p.is_dir());
            let mut args: Vec<String> = vec!["chown".into()];
            if recursive {
                args.push("-R".into());
            }
            args.push(new_owner);
            args.extend(self.paths.iter().map(|p| p.to_string_lossy().into_owned()));
            let _ = ctx.tx.send(Action::ExecuteFile {
                cmd: "sudo".into(),
                args,
                reload: self.reload_sides,
            });
        }
    }
}

#[derive(Debug)]
struct OpGitCreateBranch {
    git_root: std::path::PathBuf,
}
impl DeferredOp for OpGitCreateBranch {
    fn stays_in_git_mode(&self) -> bool { true }

    fn execute(self: Box<Self>, ctx: OpCtx) {
        let name = ctx.input.trim().to_owned();
        if name.is_empty() { return; }
        let tx = ctx.tx;
        let git_root = self.git_root;
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;
                let head_commit = repo.head()?.peel_to_commit()?;
                let mut branch = repo.branch(&name, &head_commit, false)?;
                if repo.find_remote("origin").is_ok() {
                    let _ = branch.set_upstream(Some(&format!("origin/{}", name)));
                }
                let obj = repo.revparse_single(&format!("refs/heads/{}", name))?;
                repo.checkout_tree(&obj, None)?;
                repo.set_head(&format!("refs/heads/{}", name))?;
                Ok::<_, git2::Error>(())
            })
            .await;
            git_op_result(result, &tx);
        });
    }
}

// --- Status bar ---

fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    panel: &Panel,
    error: Option<&str>,
    palette: &Palette,
) {
    use ratatui::widgets::Paragraph;

    if let Some(err) = error {
        frame.render_widget(Paragraph::new(err).style(palette.status_error), area);
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

    frame.render_widget(
        Paragraph::new(format!("{}{}", left_text, filter_text)).style(palette.status_normal),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(right_text)
            .alignment(Alignment::Right)
            .style(palette.status_size),
        right_area,
    );
}

fn draw_git_status_bar(
    frame: &mut Frame,
    area: Rect,
    gv: &GitView,
    error: Option<&str>,
    palette: &Palette,
) {
    use ratatui::widgets::Paragraph;

    if let Some(err) = error {
        frame.render_widget(Paragraph::new(err).style(palette.status_error), area);
        return;
    }

    let left = format!(" Git: {}  [{}]", gv.git_root.display(), gv.branch);
    let hint = " ^G exit  F1 stage  F2 unstage  F3 commit  p push  P pull  F6 branches  n new-branch  r reload ";
    let hint_w = hint.len() as u16;
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(hint_w)]).areas(area);
    frame.render_widget(Paragraph::new(left).style(palette.git_status_bar), left_area);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Right)
            .style(Style::default().add_modifier(Modifier::DIM)),
        right_area,
    );
}

fn find_git_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    git2::Repository::discover(start)
        .ok()
        .and_then(|r| r.workdir().map(|p| p.to_path_buf()))
}

fn git2_cred_callback(
    _url: &str,
    username: Option<&str>,
    allowed: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    if allowed.contains(git2::CredentialType::SSH_KEY) {
        let user = username.unwrap_or("git");
        if std::env::var_os("SSH_AUTH_SOCK").is_some() {
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(user) {
                return Ok(cred);
            }
        }
        // Fall back to key files in ~/.ssh
        let home = std::env::var("HOME").unwrap_or_default();
        for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
            let privkey = std::path::Path::new(&home).join(".ssh").join(key_name);
            if privkey.exists() {
                return git2::Cred::ssh_key(user, None, &privkey, None);
            }
        }
        Err(git2::Error::from_str("no SSH key found"))
    } else if allowed.contains(git2::CredentialType::DEFAULT) {
        git2::Cred::default()
    } else {
        Err(git2::Error::from_str("no supported credential type"))
    }
}

fn git_op_result(
    result: Result<Result<(), git2::Error>, tokio::task::JoinError>,
    tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    match result {
        Ok(Ok(())) => { let _ = tx.send(Action::GitOpCompleted); }
        Ok(Err(e)) => { let _ = tx.send(Action::OpError(e.to_string())); }
        Err(e) => { let _ = tx.send(Action::OpError(e.to_string())); }
    }
}

fn draw_commit_textarea(frame: &mut Frame, area: Rect, ta: &mut TextArea<'static>) {
    let popup = centered_rect(70, 50, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(&*ta, popup);
}

fn draw_branch_popup(
    frame: &mut Frame,
    area: Rect,
    branches: &[BranchInfo],
    cursor: usize,
    palette: &Palette,
) {
    use ratatui::widgets::{Block, Borders, List, ListItem};

    let popup = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(palette.border_active)
        .title(" Switch Branch — Enter checkout  n new  Esc cancel ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // Leave one line at the bottom for a local/remote legend.
    let list_h = inner.height.saturating_sub(1) as usize;
    let offset = if cursor < list_h { 0 } else { cursor + 1 - list_h };

    let items: Vec<ListItem> = branches
        .iter()
        .enumerate()
        .skip(offset)
        .take(list_h)
        .map(|(i, b)| {
            let marker = if b.is_current { ">" } else { " " };
            let locality = if b.is_local { "local " } else { "remote" };
            let label = format!(" {} [{locality}] {}", marker, b.name);
            let style = if i == cursor {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if b.is_current {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let legend_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    use ratatui::widgets::Paragraph;
    frame.render_widget(
        Paragraph::new("  > current  [local] local branch  [remote] remote-only")
            .style(Style::default().add_modifier(Modifier::DIM)),
        legend_area,
    );

    let list_area = Rect { height: list_h as u16, ..inner };
    frame.render_widget(List::new(items), list_area);
}

fn draw_ops_overlay(
    frame: &mut Frame,
    area: Rect,
    ops: &[(u64, String, u64, u64)],
    palette: &Palette,
) {
    use ratatui::widgets::{Block, Borders, Paragraph};

    let n = ops.len() as u16;
    if n == 0 {
        return;
    }

    let width = 44u16.min(area.width);
    let height = n + 2;
    let x = area.x + area.width.saturating_sub(width);
    let y = area.y + area.height.saturating_sub(height);
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(palette.border_active)
        .title(" Working… ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, (_, label, done, total)) in ops.iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let text = if *total == 0 {
            format!("{label}")
        } else {
            let t = (*total).max(1);
            let d = *done;
            let bar_w = inner.width.saturating_sub(label.len() as u16 + 7) as usize;
            let filled = (bar_w as u64 * d / t) as usize;
            let empty = bar_w.saturating_sub(filled);
            format!(
                "{label} {:>3}% {}{}",
                d * 100 / t,
                "█".repeat(filled),
                "░".repeat(empty)
            )
        };
        frame.render_widget(Paragraph::new(text).style(palette.status_normal), row);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = area.width * percent_x / 100;
    let h = area.height * percent_y / 100;
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Rect::new(x, y, w, h)
}

// --- Git file watcher ---

fn start_git_watcher(
    git_root: std::path::PathBuf,
    tx: mpsc::UnboundedSender<Action>,
) -> tokio_util::sync::CancellationToken {
    use notify::{RecursiveMode, Watcher, recommended_watcher};
    use std::sync::mpsc as std_mpsc;
    use std::time::{Duration, Instant};

    let token = tokio_util::sync::CancellationToken::new();
    let token_clone = token.clone();

    std::thread::spawn(move || {
        let (std_tx, std_rx) = std_mpsc::channel();
        let Ok(mut watcher) = recommended_watcher(std_tx) else { return };
        if watcher.watch(&git_root, RecursiveMode::Recursive).is_err() { return };

        // Debounce: wait until 200 ms of silence before firing GitReload.
        const DEBOUNCE: Duration = Duration::from_millis(200);
        let mut pending: Option<Instant> = None;

        loop {
            if token_clone.is_cancelled() { break; }

            // Non-blocking drain of all available events.
            loop {
                match std_rx.try_recv() {
                    Ok(_) => { pending = Some(Instant::now()); }
                    Err(std_mpsc::TryRecvError::Empty) => break,
                    Err(std_mpsc::TryRecvError::Disconnected) => return,
                }
            }

            if let Some(t) = pending
                && t.elapsed() >= DEBOUNCE
            {
                pending = None;
                let _ = tx.send(Action::GitReload);
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    });

    token
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
