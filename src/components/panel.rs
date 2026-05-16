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
pub enum SortMode {
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

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
    /// Current branch name; `None` when not inside a git repository.
    pub git_branch: Option<String>,
    pub git_dirty: bool,
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
            git_branch: None,
            git_dirty: false,
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
            self.marked.clear();
            self.dir_sizes.clear();
            self.sizes_calculating = false;
            self.sizes_pending = 0;
            self.git_branch = None;
            self.git_dirty = false;
        } else {
            // Prune marks for files that no longer exist after a reload.
            let existing: HashSet<&str> = self.entries.iter().map(|e| e.name.as_str()).collect();
            self.marked.retain(|name| existing.contains(name.as_str()));
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

        Self::load_git_info(self.path.clone(), self.side, self.action_tx.clone());
    }

    pub fn on_git_info_loaded(&mut self, path: &Path, branch: Option<String>, is_dirty: bool) {
        if path != self.path {
            return; // stale result from a previous directory
        }
        self.git_branch = branch;
        self.git_dirty = is_dirty;
    }

    pub fn load_git_info(path: PathBuf, side: Side, tx: UnboundedSender<Action>) {
        tokio::spawn(async move {
            let (branch, is_dirty) = detect_git_info(&path).await;
            let _ = tx.send(Action::GitInfoLoaded { side, path, branch, is_dirty });
        });
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

        let sort = |group: &mut Vec<usize>| {
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

    /// `cache` maps absolute directory paths to pre-computed recursive sizes.
    /// Cache hits are applied immediately; misses are computed asynchronously.
    pub fn start_size_calc(&mut self, cache: &HashMap<PathBuf, u64>) {
        self.dir_sizes.clear();
        self.sizes_calculating = true;
        self.sort_mode = SortMode::Size;
        self.sort_order = SortOrder::Desc;

        let dirs: Vec<(String, PathBuf)> = self
            .entries
            .iter()
            .filter(|e| e.is_dir && e.name != "..")
            .map(|e| (e.name.clone(), self.path.join(&e.name)))
            .collect();

        // Apply cache hits immediately; queue misses for async computation.
        let mut to_compute: Vec<(String, PathBuf)> = Vec::new();
        for (name, path) in dirs {
            if let Some(&cached) = cache.get(&path) {
                self.dir_sizes.insert(name, cached);
            } else {
                to_compute.push((name, path));
            }
        }

        self.sizes_pending = to_compute.len();
        self.rebuild_view();

        if to_compute.is_empty() {
            self.sizes_calculating = false;
            return;
        }

        let tx = self.action_tx.clone();
        let side = self.side;
        let panel_path = self.path.clone();

        for (name, path) in to_compute {
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
        let file_total: u64 = self
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.size)
            .sum();
        let dir_total: u64 = self.dir_sizes.values().sum();
        Some((file_total + dir_total, self.sizes_calculating))
    }

    // --- Draw ---

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, palette: &crate::palette::Palette) {
        let border_style = if self.is_active {
            palette.border_active
        } else {
            palette.border_inactive
        };

        let git_suffix = match &self.git_branch {
            Some(branch) if self.git_dirty => format!(" [{}*]", branch),
            Some(branch) => format!(" [{}]", branch),
            None => String::new(),
        };
        // 2 border chars + 2 padding spaces + git suffix
        let path_budget = (area.width as usize).saturating_sub(4 + git_suffix.len());
        let condensed = condense_path(&self.path.to_string_lossy(), path_budget);
        let title = format!(" {}{} ", condensed, git_suffix);
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
            Constraint::Min(14),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(9),
        ];
        let arrow = if self.sort_order == SortOrder::Asc { "↑" } else { "↓" };
        let h_name = if self.sort_mode == SortMode::Name { format!("Name{}", arrow) } else { "Name".into() };
        let h_size = if self.sort_mode == SortMode::Size { format!("Size{}", arrow) } else { "Size".into() };
        let h_mod  = if self.sort_mode == SortMode::Modified { format!("Date{}", arrow) } else { "Date".into() };
        let header = Row::new(vec![h_name, h_size, h_mod, "Owner".into()])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));
        frame.render_widget(Table::new(std::iter::once(header), widths), header_area);

        // Filter bar
        if let Some(farea) = filter_area {
            let filter_text = format!(" Filter: {}_", self.filter);
            frame.render_widget(
                Paragraph::new(filter_text).style(palette.filter_bar),
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
                } else if e.is_symlink {
                    format!("{}{}@", mark, e.name) // @ suffix: classic ls -F convention
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

                let owner_str = e.owner.clone();
                let row = Row::new(vec![display_name, size_str, date_str, owner_str]);

                let base = if is_marked {
                    palette.entry_marked
                } else if e.is_dir {
                    palette.entry_dir
                } else if e.is_symlink {
                    palette.entry_symlink
                } else if e.nlink > 1 {
                    palette.entry_hardlink
                } else {
                    Style::default()
                };

                if is_cursor && self.is_active {
                    row.style(base.add_modifier(Modifier::REVERSED))
                } else if is_cursor {
                    row.style(base.add_modifier(Modifier::REVERSED | Modifier::DIM))
                } else {
                    row.style(base)
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
            is_symlink: false,
            size: 0,
            modified: 0,
            nlink: 1,
            owner: String::new(),
        });
    }

    let mut dir = tokio::fs::read_dir(path).await?;
    while let Some(entry) = dir.next_entry().await? {
        // file_type() does NOT follow symlinks — tells us if the entry itself is a symlink.
        let is_symlink = entry.file_type().await?.is_symlink();
        // metadata() follows symlinks — gives us size, is_dir, modified of the target.
        let meta = entry.metadata().await?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        #[cfg(unix)]
        let (nlink, owner) = {
            use std::os::unix::fs::MetadataExt;
            (meta.nlink() as u32, uid_to_name(meta.uid()))
        };
        #[cfg(not(unix))]
        let (nlink, owner) = (1u32, String::new());

        entries.push(EntryInfo {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            is_symlink,
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            nlink,
            owner,
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

/// Extract a single archive to `dest` using pure-Rust (or bundled-C) crates.
/// Runs synchronously; call from `tokio::task::spawn_blocking`.
pub fn extract_archive_sync(archive: &Path, dest: &Path) -> color_eyre::Result<()> {
    use std::fs::File;

    std::fs::create_dir_all(dest)?;

    let name = archive
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let file = File::open(archive)?;

    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(file)?;
        zip.extract(dest)?;
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let gz = flate2::read::GzDecoder::new(file);
        tar::Archive::new(gz).unpack(dest)?;
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        let bz = bzip2::read::BzDecoder::new(file);
        tar::Archive::new(bz).unpack(dest)?;
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        let xz = xz2::read::XzDecoder::new(file);
        tar::Archive::new(xz).unpack(dest)?;
    } else if name.ends_with(".tar.zst") {
        let zst = zstd::Decoder::new(file)?;
        tar::Archive::new(zst).unpack(dest)?;
    } else if name.ends_with(".tar") {
        tar::Archive::new(file).unpack(dest)?;
    } else if name.ends_with(".gz") {
        let stem = name.strip_suffix(".gz").unwrap();
        let mut gz = flate2::read::GzDecoder::new(file);
        let mut out = std::fs::File::create(dest.join(stem))?;
        std::io::copy(&mut gz, &mut out)?;
    } else if name.ends_with(".bz2") {
        let stem = name.strip_suffix(".bz2").unwrap();
        let mut bz = bzip2::read::BzDecoder::new(file);
        let mut out = std::fs::File::create(dest.join(stem))?;
        std::io::copy(&mut bz, &mut out)?;
    } else if name.ends_with(".xz") {
        let stem = name.strip_suffix(".xz").unwrap();
        let mut xz = xz2::read::XzDecoder::new(file);
        let mut out = std::fs::File::create(dest.join(stem))?;
        std::io::copy(&mut xz, &mut out)?;
    } else if name.ends_with(".zst") {
        let stem = name.strip_suffix(".zst").unwrap();
        let mut zst = zstd::Decoder::new(file)?;
        let mut out = std::fs::File::create(dest.join(stem))?;
        std::io::copy(&mut zst, &mut out)?;
    } else {
        return Err(eyre!("Unsupported archive type: {}", name));
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

// --- Owner lookup ---

#[cfg(unix)]
fn uid_to_name(uid: u32) -> String {
    use std::ffi::CStr;
    use std::mem;
    let mut pwd: libc::passwd = unsafe { mem::zeroed() };
    let mut buf = vec![0u8; 512];
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return uid.to_string();
    }
    unsafe { CStr::from_ptr((*result).pw_name).to_string_lossy().into_owned() }
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
            let sa = if ea.is_dir {
                dir_sizes.get(&ea.name).copied()
            } else {
                Some(ea.size)
            };
            let sb = if eb.is_dir {
                dir_sizes.get(&eb.name).copied()
            } else {
                Some(eb.size)
            };
            match (sa, sb) {
                (Some(a), Some(b)) => {
                    // Apply order only to known-vs-known comparisons.
                    let c = a.cmp(&b);
                    if order == SortOrder::Desc {
                        c.reverse()
                    } else {
                        c
                    }
                }
                // Unknown sizes always sink to the bottom regardless of direction.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
            }
        }

        SortMode::Modified => {
            let c = ea.modified.cmp(&eb.modified);
            if order == SortOrder::Desc {
                c.reverse()
            } else {
                c
            }
        }
    };

    // For Name and Modified the order flag is applied here uniformly.
    if mode == SortMode::Name && order == SortOrder::Desc {
        ord.reverse()
    } else {
        ord
    }
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

pub(crate) fn format_size(bytes: u64) -> String {
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

pub(crate) fn format_age(unix_secs: u64) -> String {
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

// --- Path condensing ---

/// Shorten leftmost path components to their first character until the string
/// fits within `max_chars`.  `/home/tommie/projects` → `/h/t/projects` etc.
pub(crate) fn condense_path(path: &str, max_chars: usize) -> String {
    if path.len() <= max_chars {
        return path.to_string();
    }
    let has_leading = path.starts_with('/');
    let stripped = if has_leading { &path[1..] } else { path };
    let mut parts: Vec<String> = stripped.split('/').map(str::to_owned).collect();
    // Shorten from left, but never shorten the last component.
    for i in 0..parts.len().saturating_sub(1) {
        if parts[i].len() > 1 {
            let first = parts[i].chars().next().unwrap_or('_');
            parts[i] = first.to_string();
            let candidate = format!("{}{}", if has_leading { "/" } else { "" }, parts.join("/"));
            if candidate.len() <= max_chars {
                return candidate;
            }
        }
    }
    format!("{}{}", if has_leading { "/" } else { "" }, parts.join("/"))
}

// --- Git info ---

async fn detect_git_info(start: &Path) -> (Option<String>, bool) {
    let start = start.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let repo = match git2::Repository::discover(&start) {
            Ok(r) => r,
            Err(_) => return (None, false),
        };
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(str::to_owned))
            .unwrap_or_else(|| "HEAD".into());
        let is_dirty = repo
            .statuses(Some(
                git2::StatusOptions::new()
                    .include_untracked(false)
                    .include_ignored(false),
            ))
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        (Some(branch), is_dirty)
    })
    .await
    .unwrap_or((None, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Side;

    // --- Helpers ---

    fn entry(name: &str, is_dir: bool, size: u64, modified: u64) -> EntryInfo {
        EntryInfo {
            name: name.to_string(),
            is_dir,
            is_symlink: false,
            size,
            modified,
            nlink: 1,
            owner: "user".to_string(),
        }
    }

    fn file(name: &str) -> EntryInfo {
        entry(name, false, 100, 1_000_000)
    }

    fn dir(name: &str) -> EntryInfo {
        entry(name, true, 0, 1_000_000)
    }

    /// Build a Panel with `entries` already loaded (no async, no tokio runtime needed).
    fn panel_with(entries: Vec<EntryInfo>) -> Panel {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut p = Panel::new(Side::Left, "/tmp/test".into(), tx);
        p.entries = entries;
        p.rebuild_view();
        p
    }

    // --- condense_path ---

    #[test]
    fn condense_path_fits_unchanged() {
        assert_eq!(condense_path("/home/user", 20), "/home/user");
    }

    #[test]
    fn condense_path_shortens_prefix_components() {
        let long = "/home/tommie/projects/my-project";
        let result = condense_path(long, 20);
        assert!(result.len() <= 20, "path should be condensed; got {result:?}");
        assert!(result.ends_with("my-project"), "last component must be intact");
    }

    #[test]
    fn condense_path_never_shortens_last_component() {
        let result = condense_path("/home/user/very-long-final-name", 5);
        assert!(result.ends_with("very-long-final-name"));
    }

    #[test]
    fn condense_path_relative_no_leading_slash() {
        let result = condense_path("projects/src/main", 10);
        assert!(result.ends_with("main"));
    }

    // --- format_size ---

    #[test]
    fn format_size_zero_bytes() {
        let s = format_size(0);
        assert!(s.contains('B') && !s.contains('K'), "got {s:?}");
    }

    #[test]
    fn format_size_kilobytes() {
        let s = format_size(2048);
        assert!(s.contains('K'), "expected K unit; got {s:?}");
    }

    #[test]
    fn format_size_megabytes() {
        let s = format_size(2 * 1024 * 1024);
        assert!(s.contains('M'), "expected M unit; got {s:?}");
    }

    #[test]
    fn format_size_gigabytes() {
        let s = format_size(2 * 1024 * 1024 * 1024);
        assert!(s.contains('G'), "expected G unit; got {s:?}");
    }

    // --- format_age ---

    #[test]
    fn format_age_zero_returns_spaces() {
        let s = format_age(0);
        assert!(s.chars().all(|c| c == ' '), "expected all spaces; got {s:?}");
    }

    #[test]
    fn format_age_very_old_timestamp_shows_years() {
        // unix timestamp 1000 is 1970-01-01T00:16:40 — always many years ago
        let s = format_age(1000);
        assert!(s.contains('y'), "expected 'y' for years; got {s:?}");
    }

    #[test]
    fn format_age_recent_shows_minutes() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = format_age(now - 120); // 2 minutes ago
        assert!(s.contains('m'), "expected 'm' for minutes; got {s:?}");
    }

    // --- is_archive ---

    #[test]
    fn is_archive_recognises_common_extensions() {
        for name in &["a.zip", "a.tar.gz", "a.tgz", "a.tar.bz2", "a.tbz2",
                       "a.tar.xz", "a.txz", "a.7z", "a.rar", "a.tar", "a.gz"] {
            assert!(is_archive(name), "{name} should be recognised as archive");
        }
    }

    #[test]
    fn is_archive_rejects_non_archives() {
        for name in &["file.txt", "image.png", "video.mp4", "Makefile", "archive"] {
            assert!(!is_archive(name), "{name} should not be an archive");
        }
    }

    #[test]
    fn is_archive_case_insensitive() {
        assert!(is_archive("BACKUP.ZIP"));
        assert!(is_archive("Data.TAR.GZ"));
    }

    // --- Panel navigation ---

    #[test]
    fn nav_up_at_top_stays_at_zero() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt")]);
        p.cursor = 0;
        p.nav_up();
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn nav_up_from_middle_decrements() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.cursor = 2;
        p.nav_up();
        assert_eq!(p.cursor, 1);
    }

    #[test]
    fn nav_down_at_last_stays() {
        let mut p = panel_with(vec![file("a.txt")]);
        p.cursor = 0;
        p.nav_down();
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn nav_down_from_middle_increments() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.cursor = 1;
        p.nav_down();
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn nav_top_jumps_to_first() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.cursor = 2;
        p.nav_top();
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn nav_bottom_jumps_to_last() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.nav_bottom();
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn nav_page_up_clamps_to_zero() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.cursor = 2;
        p.nav_page_up(100);
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn nav_page_down_clamps_to_last() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.nav_page_down(100);
        assert_eq!(p.cursor, 2);
    }

    // --- Marking ---

    #[test]
    fn toggle_mark_on_dotdot_skips_mark_and_advances() {
        let mut p = panel_with(vec![dir(".."), file("a.txt")]);
        p.cursor = 0;
        p.toggle_mark();
        assert!(!p.marked.contains(".."), "'..' must never be markable");
        assert_eq!(p.cursor, 1, "cursor should advance past '..'");
    }

    #[test]
    fn toggle_mark_marks_file_and_advances_cursor() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt")]);
        p.cursor = 0;
        p.toggle_mark();
        assert!(p.marked.contains("a.txt"));
        assert_eq!(p.cursor, 1);
    }

    #[test]
    fn toggle_mark_unmarks_already_marked_file() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt")]);
        p.marked.insert("a.txt".into());
        p.cursor = 0;
        p.toggle_mark();
        assert!(!p.marked.contains("a.txt"), "second toggle should unmark");
    }

    #[test]
    fn toggle_mark_all_marks_everything_except_dotdot() {
        let mut p = panel_with(vec![dir(".."), dir("subdir"), file("a.txt"), file("b.txt")]);
        p.toggle_mark_all();
        assert!(!p.marked.contains(".."));
        assert!(p.marked.contains("subdir"));
        assert!(p.marked.contains("a.txt"));
        assert!(p.marked.contains("b.txt"));
    }

    #[test]
    fn toggle_mark_all_when_all_marked_clears_all() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt")]);
        p.toggle_mark_all(); // mark all
        assert_eq!(p.marked.len(), 2);
        p.toggle_mark_all(); // clear all
        assert!(p.marked.is_empty());
    }

    // --- effective_targets ---

    #[test]
    fn effective_targets_returns_cursor_entry_when_no_marks() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt")]);
        p.cursor = 1;
        let targets = p.effective_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].file_name().unwrap(), "b.txt");
    }

    #[test]
    fn effective_targets_empty_when_cursor_on_dotdot() {
        let mut p = panel_with(vec![dir(".."), file("a.txt")]);
        p.cursor = 0; // cursor on ".."
        assert!(p.effective_targets().is_empty());
    }

    #[test]
    fn effective_targets_returns_all_marked_ignoring_cursor() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.marked.insert("a.txt".into());
        p.marked.insert("c.txt".into());
        p.cursor = 1; // cursor on b.txt — should be ignored
        let targets = p.effective_targets();
        assert_eq!(targets.len(), 2);
        let names: Vec<&str> = targets.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"c.txt"));
    }

    // --- marked_summary ---

    #[test]
    fn marked_summary_none_when_no_marks() {
        let p = panel_with(vec![file("a.txt")]);
        assert!(p.marked_summary().is_none());
    }

    #[test]
    fn marked_summary_counts_and_sums_marked_sizes() {
        let mut p = panel_with(vec![
            entry("a.txt", false, 100, 0),
            entry("b.txt", false, 200, 0),
            entry("c.txt", false, 400, 0),
        ]);
        p.marked.insert("a.txt".into());
        p.marked.insert("c.txt".into());
        let (count, size) = p.marked_summary().unwrap();
        assert_eq!(count, 2);
        assert_eq!(size, 500);
    }

    // --- Sort ---

    #[test]
    fn cycle_sort_mode_cycles_name_size_modified() {
        let mut p = panel_with(vec![file("a.txt")]);
        assert_eq!(p.sort_mode, SortMode::Name);
        p.cycle_sort_mode();
        assert_eq!(p.sort_mode, SortMode::Size);
        p.cycle_sort_mode();
        assert_eq!(p.sort_mode, SortMode::Modified);
        p.cycle_sort_mode();
        assert_eq!(p.sort_mode, SortMode::Name);
    }

    #[test]
    fn invert_sort_toggles_asc_desc() {
        let mut p = panel_with(vec![file("a.txt")]);
        assert_eq!(p.sort_order, SortOrder::Asc);
        p.invert_sort();
        assert_eq!(p.sort_order, SortOrder::Desc);
        p.invert_sort();
        assert_eq!(p.sort_order, SortOrder::Asc);
    }

    #[test]
    fn rebuild_view_sorts_dirs_before_files() {
        let p = panel_with(vec![file("z_file.txt"), dir("a_dir")]);
        let first = &p.entries[p.view_indices[0]];
        assert!(first.is_dir, "directories must appear before files");
    }

    #[test]
    fn rebuild_view_sorts_alphabetically_by_default() {
        let p = panel_with(vec![file("c.txt"), file("a.txt"), file("b.txt")]);
        let names: Vec<&str> = p
            .view_indices
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn rebuild_view_sort_desc_reverses_order() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.invert_sort();
        let names: Vec<&str> = p
            .view_indices
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["c.txt", "b.txt", "a.txt"]);
    }

    #[test]
    fn rebuild_view_sort_by_size_desc() {
        let mut p = panel_with(vec![
            entry("small.txt", false, 10, 0),
            entry("big.txt",   false, 999, 0),
            entry("mid.txt",   false, 500, 0),
        ]);
        p.cycle_sort_mode(); // Size
        p.invert_sort();     // Desc
        let first = &p.entries[p.view_indices[0]];
        assert_eq!(first.name, "big.txt");
    }

    // --- Filter ---

    #[test]
    fn push_filter_char_narrows_view() {
        let mut p = panel_with(vec![file("alpha.txt"), file("beta.txt"), file("gamma.txt")]);
        p.push_filter_char('b');
        p.push_filter_char('e');
        assert_eq!(p.view_indices.len(), 1);
        assert_eq!(p.entries[p.view_indices[0]].name, "beta.txt");
    }

    #[test]
    fn push_filter_char_is_case_insensitive() {
        let mut p = panel_with(vec![file("Upper.txt"), file("lower.txt")]);
        p.push_filter_char('u');
        p.push_filter_char('p');
        assert_eq!(p.view_indices.len(), 1);
        assert_eq!(p.entries[p.view_indices[0]].name, "Upper.txt");
    }

    #[test]
    fn pop_filter_char_widens_view() {
        let mut p = panel_with(vec![file("alpha.txt"), file("zeta.txt")]);
        // "alph" matches only alpha.txt; "zeta.txt" does not contain "alph"
        for c in "alph".chars() {
            p.push_filter_char(c);
        }
        assert_eq!(p.view_indices.len(), 1, "filter 'alph' should match only alpha.txt");
        p.pop_filter_char(); // filter back to "alp"
        assert_eq!(p.view_indices.len(), 1, "filter 'alp' still matches only alpha.txt");
        p.pop_filter_char();
        p.pop_filter_char();
        p.pop_filter_char(); // filter cleared
        assert_eq!(p.view_indices.len(), 2, "cleared filter should show all entries");
    }

    #[test]
    fn clear_filter_restores_full_view() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.push_filter_char('z'); // matches nothing
        assert_eq!(p.view_indices.len(), 0);
        p.clear_filter();
        assert_eq!(p.view_indices.len(), 3);
    }

    #[test]
    fn filter_never_hides_dotdot() {
        let mut p = panel_with(vec![dir(".."), file("a.txt")]);
        p.push_filter_char('z'); // matches nothing except ".."
        let visible_names: Vec<&str> = p
            .view_indices
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert!(visible_names.contains(&".."), "'..' must always remain visible");
    }

    // --- current_entry / cursor_entry ---

    #[test]
    fn current_entry_none_on_empty_panel() {
        let p = panel_with(vec![]);
        assert!(p.current_entry().is_none());
    }

    #[test]
    fn current_entry_returns_entry_at_cursor() {
        let mut p = panel_with(vec![file("a.txt"), file("b.txt"), file("c.txt")]);
        p.cursor = 1;
        assert_eq!(p.current_entry().unwrap().name, "b.txt");
    }

    #[test]
    fn cursor_entry_is_alias_for_current_entry() {
        let mut p = panel_with(vec![file("x.txt")]);
        p.cursor = 0;
        assert_eq!(p.cursor_entry().unwrap().name, p.current_entry().unwrap().name);
    }
}
