use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use color_eyre::eyre::eyre;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Row, Table},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::{Action, EntryInfo, Side};

pub struct Panel {
    pub side: Side,
    pub path: PathBuf,
    pub entries: Vec<EntryInfo>,
    pub cursor: usize,
    pub offset: usize,
    pub marked: HashSet<String>,
    pub is_active: bool,
    pub loading: bool,
    action_tx: UnboundedSender<Action>,
}

impl Panel {
    pub fn new(side: Side, path: PathBuf, action_tx: UnboundedSender<Action>) -> Self {
        Self {
            side,
            path,
            entries: Vec::new(),
            cursor: 0,
            offset: 0,
            marked: HashSet::new(),
            is_active: false,
            loading: false,
            action_tx,
        }
    }

    pub fn reload(&self) {
        Self::load_dir(self.path.clone(), self.side, self.action_tx.clone());
    }

    pub fn load_dir(path: PathBuf, side: Side, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            match read_dir_entries(&path).await {
                Ok(entries) => {
                    let _ = tx.send(Action::DirLoaded { side, path, entries });
                }
                Err(e) => {
                    let _ = tx.send(Action::Error(e.to_string()));
                }
            }
        });
    }

    pub fn on_dir_loaded(&mut self, path: PathBuf, entries: Vec<EntryInfo>) {
        let prev_name = self.entries.get(self.cursor).map(|e| e.name.clone());
        self.path = path;
        self.entries = entries;
        self.marked.clear();
        self.loading = false;

        // Try to restore cursor to the same entry name; otherwise clamp.
        if let Some(name) = prev_name {
            self.cursor = self
                .entries
                .iter()
                .position(|e| e.name == name)
                .unwrap_or(0);
        } else {
            self.cursor = 0;
        }
        self.clamp_cursor();
    }

    pub fn nav_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn nav_down(&mut self) {
        if !self.entries.is_empty() && self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }

    pub fn nav_page_up(&mut self, page: usize) {
        self.cursor = self.cursor.saturating_sub(page);
    }

    pub fn nav_page_down(&mut self, page: usize) {
        let max = self.entries.len().saturating_sub(1);
        self.cursor = (self.cursor + page).min(max);
    }

    pub fn nav_top(&mut self) {
        self.cursor = 0;
    }

    pub fn nav_bottom(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
    }

    pub fn nav_enter(&self) {
        if self.entries.is_empty() {
            return;
        }
        let entry = &self.entries[self.cursor];
        if entry.name == ".." {
            self.nav_parent_internal();
        } else if entry.is_dir {
            let new_path = self.path.join(&entry.name);
            Self::load_dir(new_path, self.side, self.action_tx.clone());
        }
    }

    pub fn nav_parent(&self) {
        self.nav_parent_internal();
    }

    fn nav_parent_internal(&self) {
        if let Some(parent) = self.path.parent() {
            Self::load_dir(parent.to_path_buf(), self.side, self.action_tx.clone());
        }
    }

    pub fn toggle_mark(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let name = self.entries[self.cursor].name.clone();
        if name == ".." {
            self.nav_down();
            return;
        }
        if self.marked.contains(&name) {
            self.marked.remove(&name);
        } else {
            self.marked.insert(name);
        }
        self.nav_down();
    }

    pub fn toggle_mark_all(&mut self) {
        let all: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.name != "..")
            .map(|e| e.name.clone())
            .collect();
        if self.marked.len() == all.len() {
            self.marked.clear();
        } else {
            self.marked = all.into_iter().collect();
        }
    }

    /// Returns the paths to operate on: marked files, or cursor file if nothing marked.
    pub fn effective_targets(&self) -> Vec<PathBuf> {
        if !self.marked.is_empty() {
            self.marked
                .iter()
                .map(|name| self.path.join(name))
                .collect()
        } else if let Some(entry) = self.entries.get(self.cursor) {
            if entry.name != ".." {
                vec![self.path.join(&entry.name)]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    pub fn cursor_entry(&self) -> Option<&EntryInfo> {
        self.entries.get(self.cursor)
    }

    pub fn marked_summary(&self) -> Option<(usize, u64)> {
        if self.marked.is_empty() {
            return None;
        }
        let count = self.marked.len();
        let size: u64 = self
            .entries
            .iter()
            .filter(|e| self.marked.contains(&e.name))
            .map(|e| e.size)
            .sum();
        Some((count, size))
    }

    fn clamp_cursor(&mut self) {
        if self.entries.is_empty() {
            self.cursor = 0;
            self.offset = 0;
        } else {
            self.cursor = self.cursor.min(self.entries.len() - 1);
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let border_style = if self.is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };

        let title = format!(" {} ", self.path.display());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 2 {
            return;
        }

        let [header_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);

        // Header row
        let header = Row::new(vec!["Name", "Size", "Modified"])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));
        let widths = [
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(10),
        ];
        let header_table = Table::new(std::iter::once(header), widths);
        frame.render_widget(header_table, header_area);

        // Adjust scroll offset to keep cursor visible.
        let visible_height = list_area.height as usize;
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + visible_height {
            self.offset = self.cursor + 1 - visible_height;
        }

        let rows: Vec<Row> = self
            .entries
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible_height)
            .map(|(i, e)| {
                let is_cursor = i == self.cursor;
                let is_marked = self.marked.contains(&e.name);

                let mark = if is_marked { "*" } else { " " };
                let display_name = if e.is_dir && e.name != ".." {
                    format!("{}{}/", mark, e.name)
                } else {
                    format!("{}{}", mark, e.name)
                };

                let size_str = if e.is_dir {
                    "<DIR>  ".to_string()
                } else {
                    format_size(e.size)
                };
                let date_str = format_age(e.modified);

                let row = Row::new(vec![display_name, size_str, date_str]);

                if is_cursor && self.is_active {
                    row.style(Style::default().add_modifier(Modifier::REVERSED))
                } else if is_cursor {
                    row.style(Style::default().add_modifier(Modifier::REVERSED | Modifier::DIM))
                } else if is_marked {
                    row.style(Style::default().fg(Color::Yellow))
                } else if e.is_dir {
                    row.style(Style::default().fg(Color::Cyan))
                } else {
                    row.style(Style::default())
                }
            })
            .collect();

        let table = Table::new(rows, widths);
        frame.render_widget(table, list_area);
    }
}

async fn read_dir_entries(path: &Path) -> color_eyre::Result<Vec<EntryInfo>> {
    let mut entries = Vec::new();

    // Always add ".." unless at filesystem root.
    if path.parent().is_some() {
        entries.push(EntryInfo {
            name: "..".into(),
            is_dir: true,
            size: 0,
            modified: 0,
        });
    }

    let mut dir = tokio::fs::read_dir(path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let meta = entry.metadata().await?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        entries.push(EntryInfo {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
        });
    }

    entries.sort_by(|a, b| {
        if a.name == ".." {
            return std::cmp::Ordering::Less;
        }
        if b.name == ".." {
            return std::cmp::Ordering::Greater;
        }
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut val = bytes as f64;
    let mut unit = 0;
    while val >= 1000.0 && unit + 1 < UNITS.len() {
        val /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{:>7}B", bytes)
    } else {
        format!("{:>6.1}{}", val, UNITS[unit])
    }
}

fn format_age(unix_secs: u64) -> String {
    if unix_secs == 0 {
        return "         ".into();
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(unix_secs);
    let elapsed = now.saturating_sub(unix_secs);
    if elapsed < 3600 {
        format!("{:>5}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{:>5}h ago", elapsed / 3600)
    } else if elapsed < 86400 * 365 {
        format!("{:>6}d ago", elapsed / 86400)
    } else {
        format!("{:>6}y ago", elapsed / (86400 * 365))
    }
}

pub async fn copy_recursive(src: &Path, dst: &Path) -> color_eyre::Result<()> {
    if src.is_dir() {
        tokio::fs::create_dir_all(dst).await?;
        let mut dir = tokio::fs::read_dir(src).await?;
        while let Some(entry) = dir.next_entry().await? {
            let src_child = entry.path();
            let dst_child = dst.join(entry.file_name());
            Box::pin(copy_recursive(&src_child, &dst_child)).await?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(src, dst).await?;
    }
    Ok(())
}

pub async fn delete_recursive(path: &Path) -> color_eyre::Result<()> {
    if path.is_dir() {
        tokio::fs::remove_dir_all(path).await?;
    } else {
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

pub fn file_name_of(p: &Path) -> color_eyre::Result<String> {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| eyre!("path has no file name: {}", p.display()))
}
