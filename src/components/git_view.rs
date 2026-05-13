use std::{collections::HashSet, path::PathBuf};

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::action::{Action, GitEntry, GitIndexStatus, GitWorktreeStatus};

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

