use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use color_eyre::eyre::eyre;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Row, Table},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::action::{Action, EntryInfo, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode { Name, Size, Modified }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder { Asc, Desc }

pub struct Panel {
    pub side: Side,
    pub path: PathBuf,
    pub entries: Vec<EntryInfo>,
    /// Indices into `entries` that pass the current filter (all entries when filter is empty).
    pub view_indices: Vec<usize>,
    /// Cursor position within `view_indices`.
    pub cursor: usize,
    /// Scroll offset within `view_indices`.
    pub offset: usize,
    pub marked: HashSet<String>,
    pub is_active: bool,
    pub loading: bool,
    pub filter: String,
    /// Computed recursive sizes for directories, keyed by entry name.
    pub dir_sizes: HashMap<String, u64>,
    pub sizes_calculating: bool,
    sizes_pending: usize,
    pub sort_mode: SortMode,
    pub sort_order: SortOrder,
    action_tx: UnboundedSender<Action>,
}

impl Panel {
    pub fn new(side: Side, path: PathBuf, action_tx: UnboundedSender<Action>) -> Self {
        Self {
            side,
            path,
            entries: Vec::new(),
            view_indices: Vec::new(),
            cursor: 0,
            offset: 0,
            marked: HashSet::new(),
            is_active: false,
            loading: false,
            filter: String::new(),
            dir_sizes: HashMap::new(),
            sizes_calculating: false,
            sizes_pending: 0,
            sort_mode: SortMode::Name,
            sort_order: SortOrder::Asc,
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
                    let _ = tx.send(Action::DirLoaded {
                        side,
                        path,
                        entries,
                    });
                }
                Err(e) => {
                    let _ = tx.send(Action::Error(e.to_string()));
                }
            }
        });
    }

    pub fn on_dir_loaded(&mut self, path: PathBuf, entries: Vec<EntryInfo>) {
        let prev_name = self.current_entry().map(|e| e.name.clone());
        let path_changed = path != self.path;
        self.path = path;
        self.entries = entries;
        if path_changed {
            self.filter.clear();
            self.dir_sizes.clear();
            self.sizes_calculating = false;
            self.sizes_pending = 0;
        }
        self.rebuild_view();

        if let Some(name) = prev_name {
            self.cursor = self
                .view_indices
                .iter()
                .position(|&i| self.entries[i].name == name)
                .unwrap_or(0);
        } else {
            self.cursor = 0;
        }
        self.loading = false;
    }

    fn rebuild_view(&mut self) {
        let f = self.filter.to_lowercase();

        let mut dotdot: Vec<usize> = Vec::new();
        let mut dirs: Vec<usize> = Vec::new();
        let mut files: Vec<usize> = Vec::new();

        for (i, e) in self.entries.iter().enumerate() {
            if !self.filter.is_empty() && e.name != ".." && !e.name.to_lowercase().contains(&f) {
                continue;
            }
            if e.name == ".." {
                dotdot.push(i);
            } else if e.is_dir {
                dirs.push(i);
            } else {
                files.push(i);
            }
        }

        let mode = self.sort_mode;
        let order = self.sort_order;
        let entries = &self.entries;
        let dir_sizes = &self.dir_sizes;

        let mut sort = |group: &mut Vec<usize>| {
            group.sort_by(|&a, &b| sort_cmp(entries, dir_sizes, a, b, mode, order));
        };
        sort(&mut dirs);
        sort(&mut files);

        self.view_indices = dotdot.into_iter().chain(dirs).chain(files).collect();
        let max = self.view_indices.len().saturating_sub(1);
        self.cursor = self.cursor.min(max);
    }

    // --- Filter ---

    pub fn push_filter_char(&mut self, c: char) {
        self.filter.push(c);
        let prev_name = self.current_entry().map(|e| e.name.clone());
        self.rebuild_view();
        if let Some(name) = prev_name {
            self.cursor = self
                .view_indices
                .iter()
                .position(|&i| self.entries[i].name == name)
                .unwrap_or(0);
        }
    }

    pub fn pop_filter_char(&mut self) {
        self.filter.pop();
        self.rebuild_view();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.rebuild_view();
    }

    // --- Navigation (all operate on view_indices) ---

    pub fn nav_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn nav_down(&mut self) {
        if self.cursor + 1 < self.view_indices.len() {
            self.cursor += 1;
        }
    }

    pub fn nav_page_up(&mut self, page: usize) {
        self.cursor = self.cursor.saturating_sub(page);
    }

    pub fn nav_page_down(&mut self, page: usize) {
        let max = self.view_indices.len().saturating_sub(1);
        self.cursor = (self.cursor + page).min(max);
    }

    pub fn nav_top(&mut self) {
        self.cursor = 0;
    }

    pub fn nav_bottom(&mut self) {
        self.cursor = self.view_indices.len().saturating_sub(1);
    }

    pub fn nav_enter(&self) {
        if let Some(entry) = self.current_entry() {
            if entry.name == ".." {
                self.nav_parent_internal();
            } else if entry.is_dir {
                Self::load_dir(
                    self.path.join(&entry.name),
                    self.side,
                    self.action_tx.clone(),
                );
            } else {
                let path = self.path.join(&entry.name);
                tokio::spawn(async move {
                    tokio::process::Command::new("xdg-open")
                        .arg(path)
                        .spawn()
                        .ok();
                });
            }
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

    // --- Marking ---

    pub fn toggle_mark(&mut self) {
        let Some(entry) = self.current_entry() else {
            return;
        };
        if entry.name == ".." {
            self.nav_down();
            return;
        }
        let name = entry.name.clone();
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

    // --- Queries ---

    pub fn current_entry(&self) -> Option<&EntryInfo> {
        self.view_indices
            .get(self.cursor)
            .map(|&i| &self.entries[i])
    }

    /// Alias for compatibility.
    pub fn cursor_entry(&self) -> Option<&EntryInfo> {
        self.current_entry()
    }

    pub fn effective_targets(&self) -> Vec<PathBuf> {
        if !self.marked.is_empty() {
            self.marked
                .iter()
                .map(|name| self.path.join(name))
                .collect()
        } else if let Some(entry) = self.current_entry() {
            if entry.name != ".." {
                vec![self.path.join(&entry.name)]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    pub fn marked_summary(&self) -> Option<(usize, u64)> {
        if self.marked.is_empty() {
            return None;
        }
        let size: u64 = self
            .entries
            .iter()
            .filter(|e| self.marked.contains(&e.name))
            .map(|e| e.size)
            .sum();
        Some((self.marked.len(), size))
    }

    // --- Sorting ---

    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Name => SortMode::Size,
            SortMode::Size => SortMode::Modified,
            SortMode::Modified => SortMode::Name,
        };
        self.rebuild_view();
    }

    pub fn invert_sort(&mut self) {
        self.sort_order = match self.sort_order {
            SortOrder::Asc => SortOrder::Desc,
            SortOrder::Desc => SortOrder::Asc,
        };
        self.rebuild_view();
    }

    // --- Size calculation ---

    pub fn start_size_calc(&mut self) {
        self.dir_sizes.clear();
        self.sizes_calculating = true;

        let dirs: Vec<(String, PathBuf)> = self
            .entries
            .iter()
            .filter(|e| e.is_dir && e.name != "..")
            .map(|e| (e.name.clone(), self.path.join(&e.name)))
            .collect();

        self.sizes_pending = dirs.len();
        // Switch to size-descending immediately so known sizes bubble up as they arrive.
        self.sort_mode = SortMode::Size;
        self.sort_order = SortOrder::Desc;
        self.rebuild_view();

        if dirs.is_empty() {
            self.sizes_calculating = false;
            return;
        }

        let tx = self.action_tx.clone();
        let side = self.side;
        let panel_path = self.path.clone();

        for (name, path) in dirs {
            let tx = tx.clone();
            let panel_path = panel_path.clone();
            tokio::spawn(async move {
                let size = recursive_size(&path).await;
                let _ = tx.send(Action::DirSizeResult { side, panel_path, name, size });
            });
        }
    }

    pub fn on_dir_size_result(&mut self, panel_path: &Path, name: String, size: u64) {
        // Ignore stale results from a previous directory.
        if panel_path != self.path {
            return;
        }
        self.dir_sizes.insert(name, size);
        self.sizes_pending = self.sizes_pending.saturating_sub(1);
        if self.sizes_pending == 0 {
            self.sizes_calculating = false;
        }
        self.rebuild_view(); // re-sort as each size arrives
    }

    /// Total known size: all files + all computed dir sizes. Returns (total, is_approximate).
    pub fn size_summary(&self) -> Option<(u64, bool)> {
        if !self.sizes_calculating && self.dir_sizes.is_empty() {
            return None;
        }
        let file_total: u64 = self.entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
        let dir_total: u64 = self.dir_sizes.values().sum();
        Some((file_total + dir_total, self.sizes_calculating))
    }

    // --- Draw ---

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

        // Layout: header | [filter bar] | list
        let has_filter = !self.filter.is_empty();
        let constraints: Vec<Constraint> = if has_filter {
            vec![
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ]
        } else {
            vec![Constraint::Length(1), Constraint::Min(0)]
        };

        let areas = Layout::vertical(constraints).split(inner);
        let (header_area, filter_area, list_area) = if has_filter {
            (areas[0], Some(areas[1]), areas[2])
        } else {
            (areas[0], None, areas[1])
        };

        // Header — highlight the active sort column with an arrow.
        let widths = [
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(10),
        ];
        let arrow = if self.sort_order == SortOrder::Asc { "↑" } else { "↓" };
        let h_name = if self.sort_mode == SortMode::Name { format!("Name{}", arrow) } else { "Name".into() };
        let h_size = if self.sort_mode == SortMode::Size { format!("Size{}", arrow) } else { "Size".into() };
        let h_mod  = if self.sort_mode == SortMode::Modified { format!("Date{}", arrow) } else { "Date".into() };
        let header = Row::new(vec![h_name, h_size, h_mod])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));
        frame.render_widget(Table::new(std::iter::once(header), widths), header_area);

        // Filter bar
        if let Some(farea) = filter_area {
            let filter_text = format!(" Filter: {}_", self.filter);
            frame.render_widget(
                Paragraph::new(filter_text)
                    .style(Style::default().fg(Color::Black).bg(Color::Yellow)),
                farea,
            );
        }

        // List
        let visible_height = list_area.height as usize;
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + visible_height && visible_height > 0 {
            self.offset = self.cursor + 1 - visible_height;
        }

        let rows: Vec<Row> = self
            .view_indices
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(visible_height)
            .map(|(view_pos, &entry_idx)| {
                let e = &self.entries[entry_idx];
                let is_cursor = view_pos == self.cursor;
                let is_marked = self.marked.contains(&e.name);

                let mark = if is_marked { "*" } else { " " };
                let display_name = if e.is_dir && e.name != ".." {
                    format!("{}{}/", mark, e.name)
                } else {
                    format!("{}{}", mark, e.name)
                };
                let size_str = if e.is_dir && e.name != ".." {
                    match self.dir_sizes.get(&e.name) {
                        Some(&s) => format_size(s),
                        None if self.sizes_calculating => "  ···  ".to_string(),
                        None => "<DIR>  ".to_string(),
                    }
                } else if e.is_dir {
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

        frame.render_widget(Table::new(rows, widths), list_area);
    }
}

// --- Async directory loading ---

async fn read_dir_entries(path: &Path) -> color_eyre::Result<Vec<EntryInfo>> {
    let mut entries = Vec::new();

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

// --- File operation helpers ---

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

pub async fn extract_archive(archive: &Path, dest: &Path) -> color_eyre::Result<()> {
    use tokio::process::Command;
    tokio::fs::create_dir_all(dest).await?;

    let name = archive
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let a = archive.to_string_lossy();
    let d = dest.to_string_lossy();

    let status = if name.ends_with(".zip") {
        Command::new("unzip")
            .args([a.as_ref(), "-d", d.as_ref()])
            .status()
            .await?
    } else if name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
        || name.ends_with(".tar.xz")
        || name.ends_with(".txz")
        || name.ends_with(".tar.zst")
        || name.ends_with(".tar")
    {
        Command::new("tar")
            .args(["-xf", a.as_ref(), "-C", d.as_ref()])
            .status()
            .await?
    } else if name.ends_with(".7z") {
        Command::new("7z")
            .args(["x", a.as_ref(), &format!("-o{}", d)])
            .status()
            .await?
    } else if name.ends_with(".rar") {
        Command::new("unrar")
            .args(["x", a.as_ref(), d.as_ref()])
            .status()
            .await?
    } else {
        return Err(eyre!("Unsupported archive type: {}", name));
    };

    if !status.success() {
        return Err(eyre!(
            "Extraction failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

pub fn file_name_of(p: &Path) -> color_eyre::Result<String> {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| eyre!("path has no file name: {}", p.display()))
}

pub fn is_archive(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".txz", ".tar.zst",
        ".7z", ".rar", ".gz", ".bz2", ".xz", ".zst",
    ]
    .iter()
    .any(|ext| n.ends_with(ext))
}

#[cfg(unix)]
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn is_executable(_path: &Path) -> bool {
    false
}

// --- Sorting ---

fn sort_cmp(
    entries: &[EntryInfo],
    dir_sizes: &HashMap<String, u64>,
    a: usize,
    b: usize,
    mode: SortMode,
    order: SortOrder,
) -> std::cmp::Ordering {
    let ea = &entries[a];
    let eb = &entries[b];

    let ord = match mode {
        SortMode::Name => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),

        SortMode::Size => {
            let sa = if ea.is_dir { dir_sizes.get(&ea.name).copied() } else { Some(ea.size) };
            let sb = if eb.is_dir { dir_sizes.get(&eb.name).copied() } else { Some(eb.size) };
            match (sa, sb) {
                (Some(a), Some(b)) => {
                    // Apply order only to known-vs-known comparisons.
                    let c = a.cmp(&b);
                    if order == SortOrder::Desc { c.reverse() } else { c }
                }
                // Unknown sizes always sink to the bottom regardless of direction.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
            }
        }

        SortMode::Modified => {
            let c = ea.modified.cmp(&eb.modified);
            if order == SortOrder::Desc { c.reverse() } else { c }
        }
    };

    // For Name and Modified the order flag is applied here uniformly.
    if mode == SortMode::Name && order == SortOrder::Desc { ord.reverse() } else { ord }
}

// --- Recursive size ---

async fn recursive_size(path: &Path) -> u64 {
    let Ok(mut dir) = tokio::fs::read_dir(path).await else {
        return 0;
    };
    let mut total = 0u64;
    while let Ok(Some(entry)) = dir.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if meta.is_dir() {
                total += Box::pin(recursive_size(&entry.path())).await;
            } else {
                total += meta.len();
            }
        }
    }
    total
}

// --- Formatting helpers ---

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
