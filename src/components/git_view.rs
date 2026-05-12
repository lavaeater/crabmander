use std::{collections::HashSet, path::{Path, PathBuf}};

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
            let output = tokio::process::Command::new("git")
                .arg("-C")
                .arg(&git_root)
                // --untracked-files=normal overrides any repo/global showUntrackedFiles=no.
                // --porcelain is the stable short-format alias for --porcelain=v1.
                .args(["status", "--porcelain", "--untracked-files=normal"])
                .output()
                .await;

            let Ok(out) = output else {
                info!("git status command failed to run");
                return;
            };
            let text = String::from_utf8_lossy(&out.stdout);
            info!("git status raw output ({} bytes, exit {:?}):\n{}", text.len(), out.status.code(), text);
            let entries = parse_porcelain(&text);
            info!("parsed {} entries: {:?}", entries.len(), entries.iter().map(|e| (&e.path, e.worktree.as_ref().map(|w| format!("{:?}", w)))).collect::<Vec<_>>());

            let branch = read_branch(&git_root).await;
            let _ = tx.send(Action::GitStatusLoaded { git_root, branch, entries });
        });
    }

    // --- Drawing ---

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(area);
        self.draw_worktree(frame, left_area);
        self.draw_staging(frame, right_area);
    }

    fn draw_worktree(&mut self, frame: &mut Frame, area: Rect) {
        let is_active = self.active_pane == GitPane::Worktree;
        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };
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
                let (ch, col) = worktree_status_display(wt.as_ref().unwrap());
                let mark = if is_marked { "*" } else { " " };
                let label = format!(" {} {} {}", mark, ch, path);
                let base = if is_marked { Style::default().fg(Color::Yellow) } else { Style::default().fg(col) };
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

    fn draw_staging(&mut self, frame: &mut Frame, area: Rect) {
        let is_active = self.active_pane == GitPane::Staging;
        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };
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
                let (ch, col) = index_status_display(idx.as_ref().unwrap());
                let mark = if is_marked { "*" } else { " " };
                let label = format!(" {} {} {}", mark, ch, path);
                let base = if is_marked { Style::default().fg(Color::Yellow) } else { Style::default().fg(col) };
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

fn worktree_status_display(s: &GitWorktreeStatus) -> (&'static str, Color) {
    match s {
        GitWorktreeStatus::Modified  => ("M", Color::Yellow),
        GitWorktreeStatus::Deleted   => ("D", Color::Red),
        GitWorktreeStatus::Untracked => ("?", Color::White),  // Gray is invisible on dark terminals
    }
}

fn index_status_display(s: &GitIndexStatus) -> (&'static str, Color) {
    match s {
        GitIndexStatus::Added => ("A", Color::Green),
        GitIndexStatus::Modified => ("M", Color::Cyan),
        GitIndexStatus::Deleted => ("D", Color::Red),
        GitIndexStatus::Renamed => ("R", Color::Cyan),
        GitIndexStatus::Copied => ("C", Color::Cyan),
    }
}

// --- Async helpers ---

async fn read_branch(git_root: &Path) -> String {
    let head = tokio::fs::read_to_string(git_root.join(".git/HEAD"))
        .await
        .unwrap_or_default();
    if let Some(b) = head.trim().strip_prefix("ref: refs/heads/") {
        b.to_owned()
    } else {
        head.trim().chars().take(7).collect()
    }
}

// --- Status parsing ---

fn parse_porcelain(output: &str) -> Vec<GitEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        // Every porcelain line is: X Y <space> path  (minimum 4 bytes)
        let b = line.as_bytes();
        if b.len() < 4 {
            continue;
        }
        let x = b[0];
        let y = b[1];
        // b[2] is always an ASCII space
        let rest = &line[3..];

        // Untracked: both status chars are '?'
        if x == b'?' && y == b'?' {
            let path = rest.to_string();
            entries.push(GitEntry { path, index: None, worktree: Some(GitWorktreeStatus::Untracked) });
            continue;
        }

        // Ignored lines ('!') — skip
        if x == b'!' {
            continue;
        }

        // Rename/copy: "orig -> dest" — keep only dest.
        let path = rest
            .split_once(" -> ")
            .map(|(_, dst)| dst)
            .unwrap_or(rest)
            .to_string();

        let index = match x {
            b'A' => Some(GitIndexStatus::Added),
            b'M' => Some(GitIndexStatus::Modified),
            b'D' => Some(GitIndexStatus::Deleted),
            b'R' => Some(GitIndexStatus::Renamed),
            b'C' => Some(GitIndexStatus::Copied),
            _ => None,
        };
        let worktree = match y {
            b'M' => Some(GitWorktreeStatus::Modified),
            b'D' => Some(GitWorktreeStatus::Deleted),
            _ => None,
        };

        if index.is_some() || worktree.is_some() {
            entries.push(GitEntry { path, index, worktree });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_untracked_and_modified() {
        let input = " M src/app.rs\n?? src/components/git_view.rs\n";
        let entries = parse_porcelain(input);
        assert_eq!(
            entries.len(),
            2,
            "expected 2 entries, got {}: {:?}",
            entries.len(),
            entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert_eq!(entries[0].path, "src/app.rs");
        assert!(matches!(entries[0].worktree, Some(GitWorktreeStatus::Modified)));
        assert_eq!(entries[1].path, "src/components/git_view.rs");
        assert!(matches!(entries[1].worktree, Some(GitWorktreeStatus::Untracked)));
        assert!(entries[1].index.is_none());
    }

    #[test]
    fn parse_deleted_in_worktree() {
        let input = " D deleted.txt\n";
        let entries = parse_porcelain(input);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].worktree, Some(GitWorktreeStatus::Deleted)));
    }

    #[test]
    fn parse_staged_new_file() {
        let input = "A  new.txt\n";
        let entries = parse_porcelain(input);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].index, Some(GitIndexStatus::Added)));
        assert!(entries[0].worktree.is_none());
    }
}
