use std::{collections::HashSet, path::PathBuf};

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::action::{Action, BranchInfo, GitEntry, GitIndexStatus, GitWorktreeStatus};

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
        }
    }

    pub fn on_status_loaded(&mut self, branch: String, entries: Vec<GitEntry>) {
        info!("on_status_loaded: {} total entries", entries.len());
        self.branch = branch;
        let wt_len = entries.iter().filter(|e| e.worktree.is_some()).count();
        let st_len = entries.iter().filter(|e| e.index.is_some()).count();
        self.entries = entries;
        self.left_cursor = self.left_cursor.min(wt_len.saturating_sub(1));
        self.right_cursor = self.right_cursor.min(st_len.saturating_sub(1));
        // Drop marks that no longer exist.
        let wt: HashSet<_> = self.worktree_entries().into_iter().map(|e| e.path.clone()).collect();
        let st: HashSet<_> = self.staging_entries().into_iter().map(|e| e.path.clone()).collect();
        self.left_marked.retain(|p| wt.contains(p));
        self.right_marked.retain(|p| st.contains(p));
    }

    fn worktree_entries(&self) -> Vec<&GitEntry> {
        self.entries.iter().filter(|e| e.worktree.is_some()).collect()
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
                        .and_then(|h| h.shorthand().map(str::to_owned))
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
                                Some(GitEntry { path, index, worktree })
                            } else {
                                None
                            }
                        })
                        .collect();

                    info!(
                        "git2 status: {} entries on branch {:?}",
                        entries.len(),
                        branch
                    );
                    Ok::<_, git2::Error>((branch, entries))
                }
            })
            .await;

            match result {
                Ok(Ok((branch, entries))) => {
                    let _ = tx.send(Action::GitStatusLoaded { git_root, branch, entries });
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
                    .and_then(|h| h.shorthand().map(str::to_owned));

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
                branches.sort_by(|a, b| {
                    b.is_current.cmp(&a.is_current).then(a.name.cmp(&b.name))
                });

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
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(area);
        self.draw_worktree(frame, left_area, palette);
        self.draw_staging(frame, right_area, palette);
    }

    fn draw_worktree(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let is_active = self.active_pane == GitPane::Worktree;
        let border_style = if is_active { palette.border_active } else { palette.border_inactive };
        let wt_count = self.entries.iter().filter(|e| e.worktree.is_some()).count();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" Working Tree — {} ({}) ", self.branch, wt_count));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Collect entries first so we can then mutably update the offset.
        let entries: Vec<(String, Option<GitIndexStatus>, Option<GitWorktreeStatus>)> =
            self.worktree_entries()
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
        info!("draw_worktree: {} entries, inner_h={}, offset={}", entries.len(), visible, self.left_offset);
        if self.left_cursor < self.left_offset {
            self.left_offset = self.left_cursor;
        } else if visible > 0 && self.left_cursor >= self.left_offset + visible {
            self.left_offset = self.left_cursor + 1 - visible;
        }

        let items: Vec<ListItem> = entries
            .iter()
            .enumerate()
            .skip(self.left_offset)
            .take(visible)
            .map(|(i, (path, _idx, wt))| {
                let is_cursor = i == self.left_cursor;
                let is_marked = self.left_marked.contains(path);
                let (ch, base_style) = worktree_status_display(wt.as_ref().unwrap(), palette);
                let mark = if is_marked { "*" } else { " " };
                let label = format!(" {} {} {}", mark, ch, path);
                let base = if is_marked { palette.entry_marked } else { base_style };
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
        let border_style = if is_active { palette.border_active } else { palette.border_inactive };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Staging Area ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let entries: Vec<(String, Option<GitIndexStatus>, Option<GitWorktreeStatus>)> =
            self.staging_entries()
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
                let base = if is_marked { palette.entry_marked } else { base_style };
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

// --- Display helpers ---

fn worktree_status_display(
    s: &GitWorktreeStatus,
    p: &crate::palette::Palette,
) -> (&'static str, Style) {
    match s {
        GitWorktreeStatus::Modified  => ("M", p.git_wt_modified),
        GitWorktreeStatus::Deleted   => ("D", p.git_wt_deleted),
        GitWorktreeStatus::Untracked => ("?", p.git_wt_untracked),
    }
}

fn index_status_display(
    s: &GitIndexStatus,
    p: &crate::palette::Palette,
) -> (&'static str, Style) {
    match s {
        GitIndexStatus::Added    => ("A", p.git_idx_added),
        GitIndexStatus::Modified => ("M", p.git_idx_modified),
        GitIndexStatus::Deleted  => ("D", p.git_idx_deleted),
        GitIndexStatus::Renamed  => ("R", p.git_idx_renamed),
        GitIndexStatus::Copied   => ("C", p.git_idx_renamed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{GitEntry, GitIndexStatus, GitWorktreeStatus};

    fn wt_entry(path: &str) -> GitEntry {
        GitEntry { path: path.to_owned(), index: None, worktree: Some(GitWorktreeStatus::Modified) }
    }

    fn idx_entry(path: &str) -> GitEntry {
        GitEntry { path: path.to_owned(), index: Some(GitIndexStatus::Added), worktree: None }
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
        v.on_status_loaded("main".into(), entries);
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
        v.on_status_loaded("feature/foo".into(), vec![]);
        assert_eq!(v.branch, "feature/foo");
    }

    #[test]
    fn on_status_loaded_clamps_cursor_to_new_list_length() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs"), wt_entry("c.rs")]);
        v.left_cursor = 2;
        v.on_status_loaded("main".into(), vec![wt_entry("a.rs")]);
        assert_eq!(v.left_cursor, 0);
    }

    #[test]
    fn on_status_loaded_drops_stale_marks() {
        let mut v = view_with(vec![wt_entry("a.rs"), wt_entry("b.rs")]);
        v.left_marked.insert("b.rs".into());
        v.on_status_loaded("main".into(), vec![wt_entry("a.rs")]);
        assert!(!v.left_marked.contains("b.rs"));
    }
}

