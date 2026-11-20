use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use ratatui::{
    prelude::*,
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::action::{
    Action, BranchInfo, DiffLine, DiffSegment, GitBlameLine, GitCommit, GitEntry, GitIndexStatus,
    GitReflogEntry, GitWorktreeStatus, SubmoduleInfo,
};

// ---------------------------------------------------------------------------
// Tab model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitViewTab {
    #[default]
    Status,
    Log,
    Graph,
    Reflog,
    Diff,
    Blame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitPane {
    Worktree,
    Staging,
}

pub struct GitView {
    pub git_root: PathBuf,
    pub branch: String,
    entries: Vec<GitEntry>,
    pub active_pane: GitPane,
    left_cursor: usize,
    right_cursor: usize,
    left_offset: usize,
    right_offset: usize,
    left_marked: HashSet<String>,
    right_marked: HashSet<String>,
    pub submodule_statuses: HashMap<String, SubmoduleInfo>,

    // --- Tab state ---
    pub tab: GitViewTab,
    pub log_commits: Vec<GitCommit>,
    pub log_cursor: usize,
    log_offset: usize,
    pub graph_commits: Vec<GitCommit>,
    pub graph_rows: Vec<GraphRow>,
    graph_cursor: usize,
    graph_offset: usize,
    pub reflog_entries: Vec<GitReflogEntry>,
    reflog_cursor: usize,
    reflog_offset: usize,
    pub diff_path: Option<String>,
    pub diff_lines: Vec<DiffLine>,
    diff_offset: usize,
    pub blame_path: Option<String>,
    pub blame_lines: Vec<GitBlameLine>,
    blame_cursor: usize,
    blame_offset: usize,
}

impl GitView {
    pub fn new(git_root: PathBuf, branch: String) -> Self {
        Self {
            git_root,
            branch,
            entries: Vec::new(),
            active_pane: GitPane::Worktree,
            left_cursor: 0,
            right_cursor: 0,
            left_offset: 0,
            right_offset: 0,
            left_marked: HashSet::new(),
            right_marked: HashSet::new(),
            submodule_statuses: HashMap::new(),
            tab: GitViewTab::Status,
            log_commits: Vec::new(),
            log_cursor: 0,
            log_offset: 0,
            graph_commits: Vec::new(),
            graph_rows: Vec::new(),
            graph_cursor: 0,
            graph_offset: 0,
            reflog_entries: Vec::new(),
            reflog_cursor: 0,
            reflog_offset: 0,
            diff_path: None,
            diff_lines: Vec::new(),
            diff_offset: 0,
            blame_path: None,
            blame_lines: Vec::new(),
            blame_cursor: 0,
            blame_offset: 0,
        }
    }

    // --- Tab switching ---

    pub fn switch_tab(&mut self, tab: GitViewTab) {
        self.tab = tab;
    }

    pub fn tab_nav_up(&mut self) {
        match self.tab {
            GitViewTab::Status => self.nav_up(),
            GitViewTab::Log => self.log_cursor = self.log_cursor.saturating_sub(1),
            GitViewTab::Graph => self.graph_cursor = self.graph_cursor.saturating_sub(1),
            GitViewTab::Reflog => self.reflog_cursor = self.reflog_cursor.saturating_sub(1),
            GitViewTab::Diff => self.diff_offset = self.diff_offset.saturating_sub(1),
            GitViewTab::Blame => self.blame_cursor = self.blame_cursor.saturating_sub(1),
        }
    }

    pub fn tab_nav_down(&mut self) {
        match self.tab {
            GitViewTab::Status => self.nav_down(),
            GitViewTab::Log => {
                self.log_cursor =
                    (self.log_cursor + 1).min(self.log_commits.len().saturating_sub(1));
            }
            GitViewTab::Graph => {
                self.graph_cursor =
                    (self.graph_cursor + 1).min(self.graph_rows.len().saturating_sub(1));
            }
            GitViewTab::Reflog => {
                self.reflog_cursor =
                    (self.reflog_cursor + 1).min(self.reflog_entries.len().saturating_sub(1));
            }
            GitViewTab::Diff => {
                self.diff_offset =
                    (self.diff_offset + 1).min(self.diff_lines.len().saturating_sub(1));
            }
            GitViewTab::Blame => {
                self.blame_cursor =
                    (self.blame_cursor + 1).min(self.blame_lines.len().saturating_sub(1));
            }
        }
    }

    pub fn tab_page_up(&mut self, page: usize) {
        match self.tab {
            GitViewTab::Status => {}
            GitViewTab::Log => self.log_cursor = self.log_cursor.saturating_sub(page),
            GitViewTab::Graph => self.graph_cursor = self.graph_cursor.saturating_sub(page),
            GitViewTab::Reflog => self.reflog_cursor = self.reflog_cursor.saturating_sub(page),
            GitViewTab::Diff => self.diff_offset = self.diff_offset.saturating_sub(page),
            GitViewTab::Blame => self.blame_cursor = self.blame_cursor.saturating_sub(page),
        }
    }

    pub fn tab_page_down(&mut self, page: usize) {
        match self.tab {
            GitViewTab::Status => {}
            GitViewTab::Log => {
                self.log_cursor =
                    (self.log_cursor + page).min(self.log_commits.len().saturating_sub(1));
            }
            GitViewTab::Graph => {
                self.graph_cursor =
                    (self.graph_cursor + page).min(self.graph_rows.len().saturating_sub(1));
            }
            GitViewTab::Reflog => {
                self.reflog_cursor =
                    (self.reflog_cursor + page).min(self.reflog_entries.len().saturating_sub(1));
            }
            GitViewTab::Diff => {
                self.diff_offset =
                    (self.diff_offset + page).min(self.diff_lines.len().saturating_sub(1));
            }
            GitViewTab::Blame => {
                self.blame_cursor =
                    (self.blame_cursor + page).min(self.blame_lines.len().saturating_sub(1));
            }
        }
    }

    /// Return the path of the file under the cursor in the Status tab (for diff/blame).
    pub fn status_cursor_path(&self) -> Option<String> {
        match self.active_pane {
            GitPane::Worktree => self
                .worktree_entries()
                .get(self.left_cursor)
                .map(|e| e.path.clone()),
            GitPane::Staging => self
                .staging_entries()
                .get(self.right_cursor)
                .map(|e| e.path.clone()),
        }
    }

    // --- Async loaders for new tabs ---

    pub fn load_log(git_root: PathBuf, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking({
                let git_root = git_root.clone();
                move || load_commits_blocking(&git_root, 500)
            })
            .await;
            match result {
                Ok(Ok(commits)) => {
                    let _ = tx.send(Action::GitCommitsLoaded { git_root, commits });
                }
                Ok(Err(e)) => info!("git log error: {}", e),
                Err(e) => info!("spawn_blocking error: {}", e),
            }
        });
    }

    pub fn load_reflog(git_root: PathBuf, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking({
                let git_root = git_root.clone();
                move || {
                    let repo = git2::Repository::open(&git_root)?;
                    let reflog = repo.reflog("HEAD")?;
                    let entries: Vec<GitReflogEntry> = reflog
                        .iter()
                        .map(|e| {
                            let committer = e.committer();
                            GitReflogEntry {
                                new_id: format!("{:.7}", e.id_new()),
                                message: e.message().ok().flatten().unwrap_or("").to_owned(),
                                timestamp: committer.when().seconds(),
                                committer: committer.name().unwrap_or("").to_owned(),
                            }
                        })
                        .collect();
                    Ok::<_, git2::Error>(entries)
                }
            })
            .await;
            match result {
                Ok(Ok(entries)) => {
                    let _ = tx.send(Action::GitReflogLoaded { git_root, entries });
                }
                Ok(Err(e)) => info!("git reflog error: {}", e),
                Err(e) => info!("spawn_blocking error: {}", e),
            }
        });
    }

    pub fn load_blame(git_root: PathBuf, path: String, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking({
                let git_root = git_root.clone();
                let path = path.clone();
                move || {
                    let repo = git2::Repository::open(&git_root)?;
                    let abs_path = git_root.join(&path);
                    let content = std::fs::read_to_string(&abs_path).unwrap_or_default();
                    let file_lines: Vec<&str> = content.lines().collect();

                    let mut opts = git2::BlameOptions::new();
                    opts.track_copies_same_file(true)
                        .track_copies_same_commit_moves(true)
                        .track_copies_any_commit_copies(true);

                    let blame = repo.blame_file(std::path::Path::new(&path), Some(&mut opts))?;

                    let mut lines: Vec<GitBlameLine> = Vec::new();
                    for hunk in blame.iter() {
                        let commit_id = hunk.orig_commit_id();
                        let commit_short = format!("{:.7}", commit_id);
                        let sig = hunk.orig_signature();
                        let author_name = sig
                            .as_ref()
                            .and_then(|s| s.name().ok())
                            .unwrap_or("?")
                            .to_owned();
                        let timestamp = sig.as_ref().map(|s| s.when().seconds()).unwrap_or(0);
                        let start = hunk.final_start_line().saturating_sub(1);
                        let count = hunk.lines_in_hunk();
                        for i in 0..count {
                            let line_no = start + i + 1;
                            let content = file_lines
                                .get(start + i)
                                .map(|s| s.to_string())
                                .unwrap_or_default();
                            lines.push(GitBlameLine {
                                line_no,
                                content,
                                commit_id: commit_id.to_string(),
                                commit_short: commit_short.clone(),
                                author: author_name.clone(),
                                timestamp,
                            });
                        }
                    }
                    lines.sort_by_key(|l| l.line_no);
                    Ok::<_, git2::Error>(lines)
                }
            })
            .await;
            match result {
                Ok(Ok(lines)) => {
                    let _ = tx.send(Action::GitBlameLoaded {
                        git_root,
                        path,
                        lines,
                    });
                }
                Ok(Err(e)) => info!("git blame error: {}", e),
                Err(e) => info!("spawn_blocking error: {}", e),
            }
        });
    }

    pub fn load_diff(git_root: PathBuf, path: String, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking({
                let git_root = git_root.clone();
                let path = path.clone();
                move || {
                    let repo = git2::Repository::open(&git_root)?;
                    let mut opts = git2::DiffOptions::new();
                    opts.pathspec(&path);
                    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
                    let mut diff_lines: Vec<DiffLine> = Vec::new();
                    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                        let content = std::str::from_utf8(line.content())
                            .unwrap_or("")
                            .trim_end_matches('\n')
                            .to_owned();
                        let seg = match line.origin() {
                            '+' => DiffSegment::Added(content),
                            '-' => DiffSegment::Removed(content),
                            _ => DiffSegment::Context(content),
                        };
                        diff_lines.push(DiffLine {
                            segments: vec![seg],
                        });
                        true
                    })?;
                    // Apply word-level diff to adjacent +/- lines
                    let diff_lines = apply_word_diff(diff_lines);
                    Ok::<_, git2::Error>(diff_lines)
                }
            })
            .await;
            match result {
                Ok(Ok(lines)) => {
                    let _ = tx.send(Action::GitDiffLoaded {
                        git_root,
                        path,
                        lines,
                    });
                }
                Ok(Err(e)) => info!("git diff error: {}", e),
                Err(e) => info!("spawn_blocking error: {}", e),
            }
        });
    }

    // --- Tab data updates ---

    pub fn on_commits_loaded(&mut self, commits: Vec<GitCommit>) {
        self.graph_rows = build_graph(&commits);
        self.graph_commits = commits.clone();
        self.log_commits = commits;
        self.log_cursor = 0;
        self.log_offset = 0;
        self.graph_cursor = 0;
        self.graph_offset = 0;
    }

    pub fn on_reflog_loaded(&mut self, entries: Vec<GitReflogEntry>) {
        self.reflog_entries = entries;
        self.reflog_cursor = 0;
        self.reflog_offset = 0;
    }

    pub fn on_blame_loaded(&mut self, path: String, lines: Vec<GitBlameLine>) {
        self.blame_path = Some(path);
        self.blame_lines = lines;
        self.blame_cursor = 0;
        self.blame_offset = 0;
    }

    pub fn on_diff_loaded(&mut self, path: String, lines: Vec<DiffLine>) {
        self.diff_path = Some(path);
        self.diff_lines = lines;
        self.diff_offset = 0;
    }

    pub fn on_status_loaded(
        &mut self,
        branch: String,
        entries: Vec<GitEntry>,
        submodules: Vec<SubmoduleInfo>,
    ) {
        info!(
            "on_status_loaded: {} total entries, {} submodules",
            entries.len(),
            submodules.len()
        );
        self.branch = branch;
        let wt_len = entries.iter().filter(|e| e.worktree.is_some()).count();
        let st_len = entries.iter().filter(|e| e.index.is_some()).count();
        self.entries = entries;
        self.left_cursor = self.left_cursor.min(wt_len.saturating_sub(1));
        self.right_cursor = self.right_cursor.min(st_len.saturating_sub(1));
        // Drop marks that no longer exist.
        let wt: HashSet<_> = self
            .worktree_entries()
            .into_iter()
            .map(|e| e.path.clone())
            .collect();
        let st: HashSet<_> = self
            .staging_entries()
            .into_iter()
            .map(|e| e.path.clone())
            .collect();
        self.left_marked.retain(|p| wt.contains(p));
        self.right_marked.retain(|p| st.contains(p));
        self.submodule_statuses = submodules
            .into_iter()
            .map(|s| (s.path.clone(), s))
            .collect();
    }

    pub fn dirty_submodules(&self) -> Vec<&SubmoduleInfo> {
        self.submodule_statuses
            .values()
            .filter(|s| s.is_dirty())
            .collect()
    }

    pub fn all_submodule_paths(&self) -> Vec<PathBuf> {
        self.submodule_statuses
            .values()
            .map(|s| s.abs_path.clone())
            .collect()
    }

    fn worktree_entries(&self) -> Vec<&GitEntry> {
        self.entries
            .iter()
            .filter(|e| e.worktree.is_some())
            .collect()
    }

    fn staging_entries(&self) -> Vec<&GitEntry> {
        self.entries.iter().filter(|e| e.index.is_some()).collect()
    }

    // --- Navigation ---

    pub fn nav_up(&mut self) {
        match self.active_pane {
            GitPane::Worktree => self.left_cursor = self.left_cursor.saturating_sub(1),
            GitPane::Staging => self.right_cursor = self.right_cursor.saturating_sub(1),
        }
    }

    pub fn nav_down(&mut self) {
        match self.active_pane {
            GitPane::Worktree => {
                let max = self.worktree_entries().len().saturating_sub(1);
                self.left_cursor = (self.left_cursor + 1).min(max);
            }
            GitPane::Staging => {
                let max = self.staging_entries().len().saturating_sub(1);
                self.right_cursor = (self.right_cursor + 1).min(max);
            }
        }
    }

    pub fn switch_pane(&mut self) {
        self.active_pane = match self.active_pane {
            GitPane::Worktree => GitPane::Staging,
            GitPane::Staging => GitPane::Worktree,
        };
    }

    pub fn toggle_mark(&mut self) {
        match self.active_pane {
            GitPane::Worktree => {
                let entries = self.worktree_entries();
                if let Some(e) = entries.get(self.left_cursor) {
                    let path = e.path.clone();
                    let max = entries.len().saturating_sub(1);
                    if self.left_marked.remove(&path) {
                    } else {
                        self.left_marked.insert(path);
                    }
                    self.left_cursor = (self.left_cursor + 1).min(max);
                }
            }
            GitPane::Staging => {
                let entries = self.staging_entries();
                if let Some(e) = entries.get(self.right_cursor) {
                    let path = e.path.clone();
                    let max = entries.len().saturating_sub(1);
                    if self.right_marked.remove(&path) {
                    } else {
                        self.right_marked.insert(path);
                    }
                    self.right_cursor = (self.right_cursor + 1).min(max);
                }
            }
        }
    }

    // --- Targets for operations ---

    /// Paths to stage: marks if any, else cursor entry.
    pub fn stage_targets(&self) -> Vec<String> {
        if !self.left_marked.is_empty() {
            return self.left_marked.iter().cloned().collect();
        }
        self.worktree_entries()
            .get(self.left_cursor)
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    /// Paths to unstage: marks if any, else cursor entry.
    pub fn unstage_targets(&self) -> Vec<String> {
        if !self.right_marked.is_empty() {
            return self.right_marked.iter().cloned().collect();
        }
        self.staging_entries()
            .get(self.right_cursor)
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    pub fn has_staged(&self) -> bool {
        !self.staging_entries().is_empty()
    }

    // --- Async status loading ---

    pub fn load_status(git_root: PathBuf, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking({
                let git_root = git_root.clone();
                move || {
                    let repo = git2::Repository::open(&git_root)?;

                    let branch = repo
                        .head()
                        .ok()
                        .and_then(|h| h.shorthand().ok().map(str::to_owned))
                        .unwrap_or_else(|| "HEAD".into());

                    let mut opts = git2::StatusOptions::new();
                    opts.include_untracked(true)
                        .recurse_untracked_dirs(true)
                        .include_ignored(false);
                    let statuses = repo.statuses(Some(&mut opts))?;

                    let entries: Vec<GitEntry> = statuses
                        .iter()
                        .filter_map(|e| {
                            let path = e.path().unwrap_or("").to_string();
                            let s = e.status();
                            let index = if s.contains(git2::Status::INDEX_NEW) {
                                Some(GitIndexStatus::Added)
                            } else if s.contains(git2::Status::INDEX_MODIFIED)
                                || s.contains(git2::Status::INDEX_TYPECHANGE)
                            {
                                Some(GitIndexStatus::Modified)
                            } else if s.contains(git2::Status::INDEX_DELETED) {
                                Some(GitIndexStatus::Deleted)
                            } else if s.contains(git2::Status::INDEX_RENAMED) {
                                Some(GitIndexStatus::Renamed)
                            } else {
                                None
                            };
                            let worktree = if s.contains(git2::Status::WT_NEW) {
                                Some(GitWorktreeStatus::Untracked)
                            } else if s.contains(git2::Status::WT_MODIFIED)
                                || s.contains(git2::Status::WT_TYPECHANGE)
                            {
                                Some(GitWorktreeStatus::Modified)
                            } else if s.contains(git2::Status::WT_DELETED) {
                                Some(GitWorktreeStatus::Deleted)
                            } else {
                                None
                            };
                            if index.is_some() || worktree.is_some() {
                                Some(GitEntry {
                                    path,
                                    index,
                                    worktree,
                                })
                            } else {
                                None
                            }
                        })
                        .collect();

                    let submodules = collect_submodule_statuses(&repo, &git_root);

                    info!(
                        "git2 status: {} entries, {} submodules on branch {:?}",
                        entries.len(),
                        submodules.len(),
                        branch
                    );
                    Ok::<_, git2::Error>((branch, entries, submodules))
                }
            })
            .await;

            match result {
                Ok(Ok((branch, entries, submodules))) => {
                    let _ = tx.send(Action::GitStatusLoaded {
                        git_root,
                        branch,
                        entries,
                        submodules,
                    });
                }
                Ok(Err(e)) => {
                    info!("git2 status error: {}", e);
                }
                Err(e) => {
                    info!("spawn_blocking error: {}", e);
                }
            }
        });
    }

    // --- Branch listing ---

    pub fn load_branches(git_root: PathBuf, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let repo = git2::Repository::open(&git_root)?;

                let current = repo
                    .head()
                    .ok()
                    .and_then(|h| h.shorthand().ok().map(str::to_owned));

                // Collect local branch names.
                let mut local_names: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut branches: Vec<BranchInfo> = Vec::new();

                for item in repo.branches(Some(git2::BranchType::Local))? {
                    let (branch, _) = item?;
                    let Some(name) = branch.name()?.map(str::to_owned) else {
                        continue;
                    };
                    let is_current = current.as_deref() == Some(&name);
                    local_names.insert(name.clone());
                    branches.push(BranchInfo {
                        name,
                        is_local: true,
                        is_current,
                        remote_ref: None,
                    });
                }

                // Sort local: current first, then alphabetical.
                branches.sort_by(|a, b| b.is_current.cmp(&a.is_current).then(a.name.cmp(&b.name)));

                // Remote branches that have no local counterpart.
                let mut remote_branches: Vec<BranchInfo> = Vec::new();
                for item in repo.branches(Some(git2::BranchType::Remote))? {
                    let (branch, _) = item?;
                    let Some(full_name) = branch.name()?.map(str::to_owned) else {
                        continue;
                    };
                    if full_name.ends_with("/HEAD") {
                        continue;
                    }
                    // Strip the remote prefix (e.g. "origin/") to get the short name.
                    let short = full_name.split_once('/').map(|x| x.1).unwrap_or(&full_name);
                    if local_names.contains(short) {
                        continue;
                    }
                    remote_branches.push(BranchInfo {
                        name: short.to_owned(),
                        is_local: false,
                        is_current: false,
                        remote_ref: Some(full_name),
                    });
                }
                remote_branches.sort_by(|a, b| a.name.cmp(&b.name));
                branches.extend(remote_branches);

                Ok::<_, git2::Error>(branches)
            })
            .await;

            match result {
                Ok(Ok(branches)) => {
                    let _ = tx.send(Action::GitBranchesLoaded { branches });
                }
                Ok(Err(e)) => {
                    let _ = tx.send(Action::OpError(e.to_string()));
                }
                Err(e) => {
                    let _ = tx.send(Action::OpError(e.to_string()));
                }
            }
        });
    }

    // --- Drawing ---

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let [tab_bar_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

        self.draw_tab_bar(frame, tab_bar_area, palette);

        match self.tab.clone() {
            GitViewTab::Status => {
                let [left_area, right_area] =
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .areas(content_area);
                self.draw_worktree(frame, left_area, palette);
                self.draw_staging(frame, right_area, palette);
            }
            GitViewTab::Log => self.draw_log(frame, content_area, palette),
            GitViewTab::Graph => self.draw_graph(frame, content_area, palette),
            GitViewTab::Reflog => self.draw_reflog(frame, content_area, palette),
            GitViewTab::Diff => self.draw_diff(frame, content_area, palette),
            GitViewTab::Blame => self.draw_blame(frame, content_area, palette),
        }
    }

    fn draw_tab_bar(&self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let tabs = [
            ("s:Status", GitViewTab::Status),
            ("l:Log", GitViewTab::Log),
            ("g:Graph", GitViewTab::Graph),
            ("R:Reflog", GitViewTab::Reflog),
            ("d:Diff", GitViewTab::Diff),
            ("B:Blame", GitViewTab::Blame),
        ];
        let mut spans = Vec::new();
        for (label, t) in &tabs {
            let is_active = *t == self.tab;
            let style = if is_active {
                palette
                    .border_active
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };
            spans.push(Span::styled(format!(" {} ", label), style));
            spans.push(Span::raw(" "));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_log(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.border_active)
            .title(format!(" Log — {} ", self.branch));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.log_commits.is_empty() {
            frame.render_widget(
                Paragraph::new(" Loading…").style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        if self.log_cursor < self.log_offset {
            self.log_offset = self.log_cursor;
        } else if visible > 0 && self.log_cursor >= self.log_offset + visible {
            self.log_offset = self.log_cursor + 1 - visible;
        }

        let items: Vec<ListItem> = self
            .log_commits
            .iter()
            .enumerate()
            .skip(self.log_offset)
            .take(visible)
            .map(|(i, c)| {
                let is_cursor = i == self.log_cursor;
                let refs_str = if c.refs.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", c.refs.join(", "))
                };
                let line = Line::from(vec![
                    Span::styled(format!("{} ", c.id_short), palette.git_idx_added),
                    Span::styled(refs_str, palette.git_wt_modified),
                    Span::raw(&c.summary),
                ]);
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    fn draw_graph(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.border_active)
            .title(format!(" Graph — {} ", self.branch));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.graph_rows.is_empty() {
            frame.render_widget(
                Paragraph::new(" Loading…").style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        if self.graph_cursor < self.graph_offset {
            self.graph_offset = self.graph_cursor;
        } else if visible > 0 && self.graph_cursor >= self.graph_offset + visible {
            self.graph_offset = self.graph_cursor + 1 - visible;
        }

        let items: Vec<ListItem> = self
            .graph_rows
            .iter()
            .enumerate()
            .skip(self.graph_offset)
            .take(visible)
            .map(|(i, row)| {
                let is_cursor = i == self.graph_cursor;
                let mut spans = Vec::new();
                // Graph prefix
                for ch in &row.prefix {
                    let style = match ch {
                        GraphChar::Node => palette.git_idx_added.add_modifier(Modifier::BOLD),
                        GraphChar::Vertical => palette.git_wt_modified,
                        GraphChar::BranchRight | GraphChar::BranchLeft => palette.git_wt_untracked,
                        GraphChar::Space => Style::default(),
                    };
                    spans.push(Span::styled(ch.to_str(), style));
                }
                // Commit info (only rows with a commit node)
                if let Some(commit) = &row.commit {
                    let refs_str = if commit.refs.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", commit.refs.join(", "))
                    };
                    spans.push(Span::styled(
                        format!(" {} ", commit.id_short),
                        palette.git_idx_added,
                    ));
                    spans.push(Span::styled(refs_str, palette.git_wt_modified));
                    spans.push(Span::raw(&commit.summary));
                }
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(spans)).style(style)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    fn draw_reflog(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.border_active)
            .title(" Reflog — HEAD ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.reflog_entries.is_empty() {
            frame.render_widget(
                Paragraph::new(" Loading…").style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        if self.reflog_cursor < self.reflog_offset {
            self.reflog_offset = self.reflog_cursor;
        } else if visible > 0 && self.reflog_cursor >= self.reflog_offset + visible {
            self.reflog_offset = self.reflog_cursor + 1 - visible;
        }

        let items: Vec<ListItem> = self
            .reflog_entries
            .iter()
            .enumerate()
            .skip(self.reflog_offset)
            .take(visible)
            .map(|(idx, e)| {
                let is_cursor = idx == self.reflog_cursor;
                let ts = format_timestamp(e.timestamp);
                let line = Line::from(vec![
                    Span::styled(format!("{} ", e.new_id), palette.git_idx_added),
                    Span::styled(format!("HEAD@{{{:3}}} ", idx), palette.git_wt_untracked),
                    Span::styled(format!("{} ", ts), palette.git_wt_modified),
                    Span::raw(&e.message),
                ]);
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    fn draw_diff(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let title = match &self.diff_path {
            Some(p) => format!(" Diff — {} ", p),
            None => " Diff ".into(),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.border_active)
            .title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.diff_lines.is_empty() {
            frame.render_widget(
                Paragraph::new(if self.diff_path.is_some() {
                    " No changes"
                } else {
                    " Select a file in the Status tab, then press 'd'"
                })
                .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        self.diff_offset = self
            .diff_offset
            .min(self.diff_lines.len().saturating_sub(1));

        let total = self.diff_lines.len();
        let items: Vec<ListItem> = self
            .diff_lines
            .iter()
            .skip(self.diff_offset)
            .take(visible)
            .map(|dl| {
                let spans: Vec<Span> = dl
                    .segments
                    .iter()
                    .map(|seg| match seg {
                        DiffSegment::Added(s) => {
                            Span::styled(s.clone(), Style::default().fg(Color::Green))
                        }
                        DiffSegment::Removed(s) => {
                            Span::styled(s.clone(), Style::default().fg(Color::Red))
                        }
                        DiffSegment::Context(s) => Span::raw(s.clone()),
                    })
                    .collect();
                ListItem::new(Line::from(spans))
            })
            .collect();

        let mut sb_state = ScrollbarState::new(total).position(self.diff_offset);
        frame.render_widget(List::new(items), inner);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area,
            &mut sb_state,
        );
    }

    fn draw_blame(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let title = match &self.blame_path {
            Some(p) => format!(" Blame — {} ", p),
            None => " Blame ".into(),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.border_active)
            .title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.blame_lines.is_empty() {
            frame.render_widget(
                Paragraph::new(if self.blame_path.is_some() {
                    " Loading…"
                } else {
                    " Select a file in the Status tab, then press 'B'"
                })
                .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        if self.blame_cursor < self.blame_offset {
            self.blame_offset = self.blame_cursor;
        } else if visible > 0 && self.blame_cursor >= self.blame_offset + visible {
            self.blame_offset = self.blame_cursor + 1 - visible;
        }

        let items: Vec<ListItem> = self
            .blame_lines
            .iter()
            .enumerate()
            .skip(self.blame_offset)
            .take(visible)
            .map(|(i, bl)| {
                let is_cursor = i == self.blame_cursor;
                let date = format_date(bl.timestamp);
                let line = Line::from(vec![
                    Span::styled(format!("{} ", bl.commit_short), palette.git_idx_added),
                    Span::styled(
                        format!("{:<14} ", truncate(&bl.author, 14)),
                        palette.git_wt_modified,
                    ),
                    Span::styled(format!("{} ", date), palette.git_wt_untracked),
                    Span::styled(
                        format!("{:>5} ", bl.line_no),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::raw(&bl.content),
                ]);
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    fn draw_worktree(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let is_active = self.active_pane == GitPane::Worktree;
        let border_style = if is_active {
            palette.border_active
        } else {
            palette.border_inactive
        };
        let wt_count = self.entries.iter().filter(|e| e.worktree.is_some()).count();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" Working Tree — {} ({}) ", self.branch, wt_count));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Collect entries first so we can then mutably update the offset.
        let entries: Vec<(String, Option<GitIndexStatus>, Option<GitWorktreeStatus>)> = self
            .worktree_entries()
            .into_iter()
            .map(|e| (e.path.clone(), e.index.clone(), e.worktree.clone()))
            .collect();

        if entries.is_empty() {
            frame.render_widget(
                Paragraph::new(" Nothing to stage — working tree clean")
                    .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        info!(
            "draw_worktree: {} entries, inner_h={}, offset={}",
            entries.len(),
            visible,
            self.left_offset
        );
        if self.left_cursor < self.left_offset {
            self.left_offset = self.left_cursor;
        } else if visible > 0 && self.left_cursor >= self.left_offset + visible {
            self.left_offset = self.left_cursor + 1 - visible;
        }

        let left_cursor = self.left_cursor;
        let left_offset = self.left_offset;
        let left_marked = &self.left_marked;
        let submodule_statuses = &self.submodule_statuses;

        let items: Vec<ListItem> = entries
            .iter()
            .enumerate()
            .skip(left_offset)
            .take(visible)
            .map(|(i, (path, _idx, wt))| {
                let is_cursor = i == left_cursor;
                let is_marked = left_marked.contains(path);
                let mark = if is_marked { "*" } else { " " };

                let (label, base_style) = if let Some(sub) = submodule_statuses.get(path.as_str()) {
                    let badge = submodule_badge(sub);
                    let branch_hint = sub
                        .branch
                        .as_deref()
                        .map(|b| format!("  ({})", b))
                        .unwrap_or_default();
                    (
                        format!(" {} ◆ {}  {}{}", mark, path, badge, branch_hint),
                        palette.git_wt_modified,
                    )
                } else {
                    let (ch, style) = worktree_status_display(wt.as_ref().unwrap(), palette);
                    (format!(" {} {} {}", mark, ch, path), style)
                };

                let base = if is_marked {
                    palette.entry_marked
                } else {
                    base_style
                };
                let style = if is_cursor && is_active {
                    base.add_modifier(Modifier::REVERSED)
                } else if is_cursor {
                    base.add_modifier(Modifier::REVERSED | Modifier::DIM)
                } else {
                    base
                };
                ListItem::new(label).style(style)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    fn draw_staging(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let is_active = self.active_pane == GitPane::Staging;
        let border_style = if is_active {
            palette.border_active
        } else {
            palette.border_inactive
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Staging Area ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let entries: Vec<(String, Option<GitIndexStatus>, Option<GitWorktreeStatus>)> = self
            .staging_entries()
            .into_iter()
            .map(|e| (e.path.clone(), e.index.clone(), e.worktree.clone()))
            .collect();

        if entries.is_empty() {
            frame.render_widget(
                Paragraph::new(" Nothing staged")
                    .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        if self.right_cursor < self.right_offset {
            self.right_offset = self.right_cursor;
        } else if visible > 0 && self.right_cursor >= self.right_offset + visible {
            self.right_offset = self.right_cursor + 1 - visible;
        }

        let items: Vec<ListItem> = entries
            .iter()
            .enumerate()
            .skip(self.right_offset)
            .take(visible)
            .map(|(i, (path, idx, _wt))| {
                let is_cursor = i == self.right_cursor;
                let is_marked = self.right_marked.contains(path);
                let (ch, base_style) = index_status_display(idx.as_ref().unwrap(), palette);
                let mark = if is_marked { "*" } else { " " };
                let label = format!(" {} {} {}", mark, ch, path);
                let base = if is_marked {
                    palette.entry_marked
                } else {
                    base_style
                };
                let style = if is_cursor && is_active {
                    base.add_modifier(Modifier::REVERSED)
                } else if is_cursor {
                    base.add_modifier(Modifier::REVERSED | Modifier::DIM)
                } else {
                    base
                };
                ListItem::new(label).style(style)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }
}

// ---------------------------------------------------------------------------
// Git log / graph helpers
// ---------------------------------------------------------------------------

/// Load up to `limit` commits from HEAD via revwalk.
fn load_commits_blocking(
    git_root: &std::path::Path,
    limit: usize,
) -> Result<Vec<GitCommit>, git2::Error> {
    let repo = git2::Repository::open(git_root)?;

    // Build a map from OID → refs for decoration.
    let mut ref_map: std::collections::HashMap<git2::Oid, Vec<String>> =
        std::collections::HashMap::new();
    let head_id = repo.head().ok().and_then(|h| h.target());
    let head_branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(str::to_owned));

    for reference in repo.references()?.flatten() {
        let Some(oid) = reference.target() else {
            continue;
        };
        let name = reference.shorthand().unwrap_or("").to_owned();
        if name.is_empty() {
            continue;
        }

        // For the HEAD ref build "HEAD -> branch" decoration.
        let decoration = if reference.is_branch() {
            let is_head = head_id == Some(oid) && head_branch.as_deref() == Some(&name);
            if is_head {
                format!("HEAD -> {}", name)
            } else {
                name
            }
        } else if reference.is_remote() {
            name
        } else if reference.is_tag() {
            format!("tag: {}", name)
        } else {
            name
        };
        ref_map.entry(oid).or_default().push(decoration);
    }

    // Peel annotated tags to their commit target.
    let mut peeled_map: std::collections::HashMap<git2::Oid, Vec<String>> =
        std::collections::HashMap::new();
    for reference in repo.references()?.flatten() {
        if reference.is_tag()
            && let Ok(obj) = reference.peel(git2::ObjectType::Commit)
        {
            let commit_oid = obj.id();
            let name = reference.shorthand().unwrap_or("").to_owned();
            if !name.is_empty() {
                peeled_map
                    .entry(commit_oid)
                    .or_default()
                    .push(format!("tag: {}", name));
            }
        }
    }
    for (oid, decs) in peeled_map {
        let entry = ref_map.entry(oid).or_default();
        // avoid duplicates
        for d in decs {
            if !entry.contains(&d) {
                entry.push(d);
            }
        }
    }

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    let mut commits = Vec::with_capacity(limit.min(256));
    for oid in revwalk.flatten().take(limit) {
        let commit = repo.find_commit(oid)?;
        let id_short = format!("{:.7}", oid);
        let summary = commit.summary().ok().flatten().unwrap_or("").to_owned();
        let author = commit.author().name().unwrap_or("?").to_owned();
        let timestamp = commit.time().seconds();
        let refs = ref_map.remove(&oid).unwrap_or_default();
        let parent_ids = commit.parent_ids().map(|p| p.to_string()).collect();
        commits.push(GitCommit {
            id: oid.to_string(),
            id_short,
            summary,
            author,
            timestamp,
            refs,
            parent_ids,
        });
    }
    Ok(commits)
}

// ---------------------------------------------------------------------------
// ASCII graph renderer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphChar {
    Node,        // *
    Vertical,    // |
    BranchRight, // \  (going right = older lane)
    BranchLeft,  // /  (going left = merge)
    Space,       //
}

impl GraphChar {
    fn to_str(&self) -> &'static str {
        match self {
            GraphChar::Node => "*",
            GraphChar::Vertical => "│",
            GraphChar::BranchRight => "\\",
            GraphChar::BranchLeft => "/",
            GraphChar::Space => " ",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphRow {
    pub prefix: Vec<GraphChar>,
    pub commit: Option<GitCommit>,
}

/// Build graph rows for all commits.
pub fn build_graph(commits: &[GitCommit]) -> Vec<GraphRow> {
    // lanes[i] = Some(oid_string) meaning lane i is tracking that commit as its next expected node.
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows: Vec<GraphRow> = Vec::new();

    for commit in commits {
        // Find this commit's column.
        let col = lanes
            .iter()
            .position(|l| l.as_deref() == Some(&commit.id))
            .unwrap_or_else(|| {
                // New head — find an empty lane or push one.
                lanes.iter().position(|l| l.is_none()).unwrap_or_else(|| {
                    lanes.push(None);
                    lanes.len() - 1
                })
            });

        // Set up parents: first parent inherits the lane, extras get new lanes.
        let mut new_lanes = lanes.clone();
        new_lanes[col] = commit.parent_ids.first().cloned();
        let mut extra_cols: Vec<usize> = Vec::new();
        for parent_id in commit.parent_ids.iter().skip(1) {
            // Check if it's already tracked in a lane.
            if let Some(existing) = new_lanes
                .iter()
                .position(|l| l.as_deref() == Some(parent_id))
            {
                extra_cols.push(existing);
            } else {
                let slot = new_lanes
                    .iter()
                    .position(|l| l.is_none())
                    .unwrap_or_else(|| {
                        new_lanes.push(None);
                        new_lanes.len() - 1
                    });
                new_lanes[slot] = Some(parent_id.clone());
                extra_cols.push(slot);
            }
        }

        // Build the prefix for this commit row.
        let width = lanes.len().max(new_lanes.len()).max(col + 1);
        let mut prefix: Vec<GraphChar> = Vec::with_capacity(width * 2);
        for i in 0..width {
            if i == col {
                prefix.push(GraphChar::Node);
            } else if lanes.get(i).and_then(|l| l.as_ref()).is_some()
                || new_lanes.get(i).and_then(|l| l.as_ref()).is_some()
            {
                prefix.push(GraphChar::Vertical);
            } else {
                prefix.push(GraphChar::Space);
            }
            prefix.push(GraphChar::Space);
        }

        rows.push(GraphRow {
            prefix,
            commit: Some(commit.clone()),
        });

        // Emit connector rows for merges (when extra parents go to different lanes).
        if !extra_cols.is_empty() {
            // One connector row showing lines diverging to extra parent lanes.
            let mut conn: Vec<GraphChar> = Vec::with_capacity(width * 2);
            for i in 0..width {
                let in_new = new_lanes.get(i).and_then(|l| l.as_ref()).is_some();
                if i == col {
                    // Draw vertical down from node.
                    if new_lanes.get(col).and_then(|l| l.as_ref()).is_some() {
                        conn.push(GraphChar::Vertical);
                    } else {
                        conn.push(GraphChar::Space);
                    }
                } else if extra_cols.contains(&i) {
                    // This lane is a new parent branching out.
                    if i > col {
                        conn.push(GraphChar::BranchRight);
                    } else {
                        conn.push(GraphChar::BranchLeft);
                    }
                } else if in_new {
                    conn.push(GraphChar::Vertical);
                } else {
                    conn.push(GraphChar::Space);
                }
                conn.push(GraphChar::Space);
            }
            rows.push(GraphRow {
                prefix: conn,
                commit: None,
            });
        }

        lanes = new_lanes;
        // Trim trailing Nones.
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
    }
    rows
}

// ---------------------------------------------------------------------------
// Word-diff helper
// ---------------------------------------------------------------------------

/// Split adjacent removed/added line pairs into word-level segments.
fn apply_word_diff(lines: Vec<DiffLine>) -> Vec<DiffLine> {
    let mut result: Vec<DiffLine> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        // Find a run of removed lines followed immediately by added lines.
        let rem_start = i;
        while i < lines.len() && matches!(lines[i].segments.first(), Some(DiffSegment::Removed(_)))
        {
            i += 1;
        }
        let add_start = i;
        while i < lines.len() && matches!(lines[i].segments.first(), Some(DiffSegment::Added(_))) {
            i += 1;
        }
        let add_end = i;

        if rem_start == add_start || add_start == add_end {
            // Not a paired hunk; emit as-is.
            for line in &lines[rem_start..add_end] {
                result.push(line.clone());
            }
            continue;
        }

        // Pair up removed / added lines and apply word diff within each pair.
        let rem_lines: Vec<&str> = (rem_start..add_start)
            .map(|j| match lines[j].segments.first() {
                Some(DiffSegment::Removed(s)) => s.as_str(),
                _ => "",
            })
            .collect();
        let add_lines: Vec<&str> = (add_start..add_end)
            .map(|j| match lines[j].segments.first() {
                Some(DiffSegment::Added(s)) => s.as_str(),
                _ => "",
            })
            .collect();

        // For each removed line, try to find a corresponding added line to diff against.
        for (ri, &rem_text) in rem_lines.iter().enumerate() {
            let add_text = add_lines.get(ri).copied().unwrap_or("");
            result.push(word_diff_line(rem_text, add_text, DiffSegment::Removed));
        }
        for (ai, &add_text) in add_lines.iter().enumerate() {
            let rem_text = rem_lines.get(ai).copied().unwrap_or("");
            result.push(word_diff_line(add_text, rem_text, DiffSegment::Added));
        }
    }
    result
}

fn word_diff_line(text: &str, other: &str, line_kind: fn(String) -> DiffSegment) -> DiffLine {
    let words_a: Vec<&str> = tokenize_words(text);
    let words_b: Vec<&str> = tokenize_words(other);
    let common = longest_common_subsequence(&words_a, &words_b);

    let mut segments: Vec<DiffSegment> = Vec::new();
    let mut ai = 0;
    let mut ci = 0;
    while ci < common.len() {
        let (ca, _cb) = common[ci];
        // Words before the common match are "changed".
        let unchanged_a = &words_a[ai..ca];
        if !unchanged_a.is_empty() {
            segments.push(line_kind(unchanged_a.join("")));
        }
        segments.push(DiffSegment::Context(words_a[ca].to_owned()));
        ai = ca + 1;
        ci += 1;
    }
    // Remaining tail words.
    if ai < words_a.len() {
        segments.push(line_kind(words_a[ai..].join("")));
    }
    if segments.is_empty() {
        segments.push(DiffSegment::Context(String::new()));
    }
    DiffLine { segments }
}

fn tokenize_words(s: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_word = false;
    for (i, c) in s.char_indices() {
        let is_word = c.is_alphanumeric() || c == '_';
        if is_word != in_word {
            if i > start {
                tokens.push(&s[start..i]);
            }
            start = i;
            in_word = is_word;
        }
    }
    if start < s.len() {
        tokens.push(&s[start..]);
    }
    tokens
}

fn longest_common_subsequence<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(usize, usize)> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut result = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            result.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_timestamp(ts: i64) -> String {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64;
    let delta = now - ts;
    if delta < 60 {
        return "just now".into();
    }
    if delta < 3600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 86400 {
        return format!("{}h ago", delta / 3600);
    }
    if delta < 86400 * 30 {
        return format!("{}d ago", delta / 86400);
    }
    format_date(ts)
}

fn format_date(ts: i64) -> String {
    // Simple ISO-like date without chrono: YYYY-MM-DD
    let secs = ts as u64;
    // Days since Unix epoch
    let days = secs / 86400;
    let mut y = 1970u32;
    let mut d = days as u32;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }
    let month_days: [u32; 12] = [
        31,
        if is_leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 1u32;
    for &md in &month_days {
        if d < md {
            break;
        }
        d -= md;
        mo += 1;
    }
    format!("{:04}-{:02}-{:02}", y, mo, d + 1)
}

fn is_leap(y: u32) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

// --- Submodule helpers ---

fn collect_submodule_statuses(
    repo: &git2::Repository,
    git_root: &std::path::Path,
) -> Vec<SubmoduleInfo> {
    let subs = match repo.submodules() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    subs.into_iter()
        .filter_map(|sub| {
            let rel_path = sub.path().to_string_lossy().into_owned();
            let abs_path = git_root.join(sub.path());
            let sub_repo = git2::Repository::open(&abs_path).ok()?;
            let branch = sub_repo
                .head()
                .ok()
                .and_then(|h| h.shorthand().ok().map(str::to_owned));
            let mut opts = git2::StatusOptions::new();
            opts.include_untracked(true)
                .recurse_untracked_dirs(true)
                .include_ignored(false);
            let statuses = sub_repo.statuses(Some(&mut opts)).ok()?;
            let mut staged = 0usize;
            let mut wt_modified = 0usize;
            let mut wt_untracked = 0usize;
            for e in statuses.iter() {
                let s = e.status();
                if s.intersects(
                    git2::Status::INDEX_NEW
                        | git2::Status::INDEX_MODIFIED
                        | git2::Status::INDEX_DELETED
                        | git2::Status::INDEX_RENAMED,
                ) {
                    staged += 1;
                }
                if s.contains(git2::Status::WT_NEW) {
                    wt_untracked += 1;
                } else if s.intersects(git2::Status::WT_MODIFIED | git2::Status::WT_DELETED) {
                    wt_modified += 1;
                }
            }
            Some(SubmoduleInfo {
                path: rel_path,
                abs_path,
                branch,
                staged,
                wt_modified,
                wt_untracked,
            })
        })
        .collect()
}

fn submodule_badge(sub: &SubmoduleInfo) -> String {
    let mut parts = Vec::new();
    if sub.staged > 0 {
        parts.push(format!("+{}", sub.staged));
    }
    if sub.wt_modified > 0 {
        parts.push(format!("~{}", sub.wt_modified));
    }
    if sub.wt_untracked > 0 {
        parts.push(format!("?{}", sub.wt_untracked));
    }
    if parts.is_empty() {
        "[clean]".into()
    } else {
        format!("[{}]", parts.join(" "))
    }
}

// --- Display helpers ---

fn worktree_status_display(
    s: &GitWorktreeStatus,
    p: &crate::palette::Palette,
) -> (&'static str, Style) {
    match s {
        GitWorktreeStatus::Modified => ("M", p.git_wt_modified),
        GitWorktreeStatus::Deleted => ("D", p.git_wt_deleted),
        GitWorktreeStatus::Untracked => ("?", p.git_wt_untracked),
    }
}

fn index_status_display(s: &GitIndexStatus, p: &crate::palette::Palette) -> (&'static str, Style) {
    match s {
        GitIndexStatus::Added => ("A", p.git_idx_added),
        GitIndexStatus::Modified => ("M", p.git_idx_modified),
        GitIndexStatus::Deleted => ("D", p.git_idx_deleted),
        GitIndexStatus::Renamed => ("R", p.git_idx_renamed),
        GitIndexStatus::Copied => ("C", p.git_idx_renamed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{GitEntry, GitIndexStatus, GitWorktreeStatus};

    fn wt_entry(path: &str) -> GitEntry {
        GitEntry {
            path: path.to_owned(),
            index: None,
            worktree: Some(GitWorktreeStatus::Modified),
        }
    }

    fn idx_entry(path: &str) -> GitEntry {
        GitEntry {
            path: path.to_owned(),
            index: Some(GitIndexStatus::Added),
            worktree: None,
        }
    }

    fn both_entry(path: &str) -> GitEntry {
        GitEntry {
            path: path.to_owned(),
            index: Some(GitIndexStatus::Modified),
            worktree: Some(GitWorktreeStatus::Modified),
        }
    }

    fn view_with(entries: Vec<GitEntry>) -> GitView {
        let mut v = GitView::new(PathBuf::from("/repo"), "main".into());
        v.on_status_loaded("main".into(), entries, vec![]);
        v
    }

    #[test]
    fn new_view_starts_on_worktree_pane() {
        let v = GitView::new(PathBuf::from("/repo"), "main".into());
        assert_eq!(v.active_pane, GitPane::Worktree);
    }

    #[test]
    fn nav_up_does_not_underflow() {
        let mut v = view_with(vec![wt_entry("a.rs")]);
        v.nav_up();
        assert_eq!(v.left_cursor, 0);
    }

    #[test]
    fn nav_down_clamps_to_last_entry() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.nav_down();
        v.nav_down();
        v.nav_down();
        assert_eq!(v.left_cursor, 1);
    }

    #[test]
    fn nav_down_empty_list_does_not_panic() {
        let mut v = view_with(vec![]);
        v.nav_down();
        assert_eq!(v.left_cursor, 0);
    }

    #[test]
    fn switch_pane_toggles_between_worktree_and_staging() {
        let mut v = GitView::new(PathBuf::from("/repo"), "main".into());
        assert_eq!(v.active_pane, GitPane::Worktree);
        v.switch_pane();
        assert_eq!(v.active_pane, GitPane::Staging);
        v.switch_pane();
        assert_eq!(v.active_pane, GitPane::Worktree);
    }

    #[test]
    fn toggle_mark_marks_worktree_entry_and_advances_cursor() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.toggle_mark();
        assert!(v.left_marked.contains("a.rs"));
        assert_eq!(v.left_cursor, 1);
    }

    #[test]
    fn toggle_mark_unmarks_already_marked_entry() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.toggle_mark();
        v.left_cursor = 0;
        v.toggle_mark();
        assert!(!v.left_marked.contains("a.rs"));
    }

    #[test]
    fn stage_targets_returns_cursor_when_no_marks() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.left_cursor = 1;
        assert_eq!(v.stage_targets(), vec!["b.rs".to_owned()]);
    }

    #[test]
    fn stage_targets_returns_marks_when_marks_exist() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.left_marked.insert("a.rs".into());
        let targets = v.stage_targets();
        assert_eq!(targets, vec!["a.rs".to_owned()]);
    }

    #[test]
    fn unstage_targets_returns_cursor_staging_entry() {
        let mut v = view_with(vec![idx_entry("a.rs"), idx_entry("b.rs")]);
        v.switch_pane();
        v.right_cursor = 1;
        assert_eq!(v.unstage_targets(), vec!["b.rs".to_owned()]);
    }

    #[test]
    fn has_staged_false_when_no_index_entries() {
        let v = view_with(vec![wt_entry("a.rs")]);
        assert!(!v.has_staged());
    }

    #[test]
    fn has_staged_true_when_index_entries_present() {
        let v = view_with(vec![idx_entry("a.rs")]);
        assert!(v.has_staged());
    }

    #[test]
    fn both_entry_appears_in_both_panes() {
        let v = view_with(vec![both_entry("a.rs")]);
        assert_eq!(v.worktree_entries().len(), 1);
        assert_eq!(v.staging_entries().len(), 1);
    }

    #[test]
    fn on_status_loaded_updates_branch_name() {
        let mut v = GitView::new(PathBuf::from("/repo"), "main".into());
        v.on_status_loaded("feature/foo".into(), vec![], vec![]);
        assert_eq!(v.branch, "feature/foo");
    }

    #[test]
    fn on_status_loaded_clamps_cursor_to_new_list_length() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs"), wt_entry("c.rs")]);
        v.left_cursor = 2;
        v.on_status_loaded("main".into(), vec![wt_entry("a.rs")], vec![]);
        assert_eq!(v.left_cursor, 0);
    }

    #[test]
    fn on_status_loaded_drops_stale_marks() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.left_marked.insert("b.rs".into());
        v.on_status_loaded("main".into(), vec![wt_entry("a.rs")], vec![]);
        assert!(!v.left_marked.contains("b.rs"));
    }
}
