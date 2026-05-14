use std::path::{Path, PathBuf};

const MAX: usize = 10;
const FILE: &str = "recent_dirs.json";

pub fn load(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(data_dir.join(FILE)) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

pub fn save(data_dir: &Path, dirs: &[PathBuf]) {
    let strings: Vec<&str> = dirs.iter()
        .filter_map(|p| p.to_str())
        .collect();
    if let Ok(json) = serde_json::to_string(&strings) {
        let _ = std::fs::write(data_dir.join(FILE), json);
    }
}

/// Insert `dest` at the front, deduplicate, trim to MAX.
pub fn push(dirs: &mut Vec<PathBuf>, dest: PathBuf) {
    dirs.retain(|p| p != &dest);
    dirs.insert(0, dest);
    dirs.truncate(MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- push ---

    #[test]
    fn push_new_entry_goes_to_front() {
        let mut dirs = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        push(&mut dirs, PathBuf::from("/new"));
        assert_eq!(dirs[0], PathBuf::from("/new"));
        assert_eq!(dirs.len(), 3);
    }

    #[test]
    fn push_existing_entry_moves_to_front() {
        let mut dirs = vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")];
        push(&mut dirs, PathBuf::from("/b"));
        assert_eq!(dirs[0], PathBuf::from("/b"));
        assert_eq!(dirs.len(), 3, "no duplicates after moving to front");
    }

    #[test]
    fn push_deduplicates_identical_paths() {
        let mut dirs = vec![PathBuf::from("/a"), PathBuf::from("/a")];
        push(&mut dirs, PathBuf::from("/a"));
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0], PathBuf::from("/a"));
    }

    #[test]
    fn push_truncates_to_max() {
        let mut dirs: Vec<PathBuf> = (0..MAX).map(|i| PathBuf::from(format!("/{i}"))).collect();
        push(&mut dirs, PathBuf::from("/extra"));
        assert_eq!(dirs.len(), MAX);
        assert_eq!(dirs[0], PathBuf::from("/extra"));
    }

    #[test]
    fn push_truncates_oldest_entry() {
        let mut dirs: Vec<PathBuf> = (0..MAX).map(|i| PathBuf::from(format!("/{i}"))).collect();
        let last_before = dirs.last().unwrap().clone();
        push(&mut dirs, PathBuf::from("/new"));
        assert!(!dirs.contains(&last_before), "oldest entry should be dropped");
    }

    // --- load ---

    #[test]
    fn load_nonexistent_dir_returns_empty() {
        let missing = PathBuf::from("/tmp/crabmander-test-nonexistent-xyzzy-99999");
        assert_eq!(load(&missing), Vec::<PathBuf>::new());
    }

    // --- save + load ---

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir()
            .join(format!("crabmander-test-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let expected = vec![PathBuf::from("/foo"), PathBuf::from("/bar/baz")];
        save(&dir, &expected);
        let loaded = load(&dir);
        assert_eq!(loaded, expected);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_empty_list_then_load_returns_empty() {
        let dir = std::env::temp_dir()
            .join(format!("crabmander-test-recent-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        save(&dir, &[]);
        let loaded = load(&dir);
        assert!(loaded.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
