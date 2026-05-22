use std::path::PathBuf;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;

pub struct GlobalGitView {
    pub repos: Vec<PathBuf>,
    /// Per-repo metadata loaded async: (branch, is_dirty)
    pub repo_meta: std::collections::HashMap<PathBuf, (Option<String>, bool)>,
    pub cursor: usize,
    offset: usize,
}

impl GlobalGitView {
    pub fn new(repos: Vec<PathBuf>) -> Self {
        Self {
            repos,
            repo_meta: std::collections::HashMap::new(),
            cursor: 0,
            offset: 0,
        }
    }

    pub fn nav_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn nav_down(&mut self) {
        self.cursor = (self.cursor + 1).min(self.repos.len().saturating_sub(1));
    }

    pub fn selected_repo(&self) -> Option<&PathBuf> {
        self.repos.get(self.cursor)
    }

    pub fn remove_selected(&mut self) -> Option<PathBuf> {
        if self.repos.is_empty() {
            return None;
        }
        let removed = self.repos.remove(self.cursor);
        self.repo_meta.remove(&removed);
        self.cursor = self.cursor.min(self.repos.len().saturating_sub(1));
        Some(removed)
    }

    pub fn on_repo_info_loaded(&mut self, path: PathBuf, branch: Option<String>, is_dirty: bool) {
        self.repo_meta.insert(path, (branch, is_dirty));
    }

    /// Kick off async metadata loads for all repos.
    pub fn load_all_meta(repos: Vec<PathBuf>, tx: UnboundedSender<Action>) {
        for path in repos {
            Self::load_repo_meta(path, tx.clone());
        }
    }

    pub fn load_repo_meta(path: PathBuf, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking({
                let path = path.clone();
                move || {
                    let repo = git2::Repository::open(&path)?;
                    let branch = repo
                        .head()
                        .ok()
                        .and_then(|h| h.shorthand().ok().map(str::to_owned));
                    let is_dirty = repo
                        .statuses(Some(
                            git2::StatusOptions::new()
                                .include_untracked(false)
                                .include_ignored(false),
                        ))
                        .map(|s| s.iter().any(|e| !e.status().is_empty()))
                        .unwrap_or(false);
                    Ok::<_, git2::Error>((branch, is_dirty))
                }
            })
            .await;
            let (branch, is_dirty) = match result {
                Ok(Ok(v)) => v,
                _ => (None, false),
            };
            let _ = tx.send(Action::GitRepoInfoLoaded { path, branch, is_dirty });
        });
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(palette.border_active)
            .title(" Git Repositories ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.repos.is_empty() {
            frame.render_widget(
                Paragraph::new(" No git repositories bookmarked yet.\n Press Ctrl+G in any git repo to add it here.")
                    .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let visible = inner.height as usize;
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if visible > 0 && self.cursor >= self.offset + visible {
            self.offset = self.cursor + 1 - visible;
        }

        let items: Vec<ListItem> = self
            .repos
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible)
            .map(|(i, path)| {
                let is_cursor = i == self.cursor;
                let display_path = path.to_string_lossy();

                let (branch_str, dirty_marker) =
                    match self.repo_meta.get(path) {
                        Some((branch, dirty)) => {
                            let b = branch.as_deref().unwrap_or("?");
                            let d = if *dirty { "*" } else { "" };
                            (format!("[{}{}]", b, d), true)
                        }
                        None => ("…".into(), false),
                    };

                let path_style = if is_cursor {
                    palette.border_active.add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                let meta_style = if dirty_marker {
                    palette.git_wt_modified
                } else {
                    palette.git_idx_added
                };
                let meta_style = if is_cursor {
                    meta_style.add_modifier(Modifier::REVERSED)
                } else {
                    meta_style
                };

                let line = Line::from(vec![
                    Span::styled(format!(" {:<60}", display_path), path_style),
                    Span::styled(format!(" {}", branch_str), meta_style),
                ]);
                ListItem::new(line)
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }
}
