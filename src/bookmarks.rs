use std::path::{Path, PathBuf};

const FILE: &str = "bookmarks.json";

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
    let strings: Vec<&str> = dirs.iter().filter_map(|p| p.to_str()).collect();
    if let Ok(json) = serde_json::to_string(&strings) {
        let _ = std::fs::write(data_dir.join(FILE), json);
    }
}

/// Add `path` to bookmarks (dedup, no truncation).
pub fn add(dirs: &mut Vec<PathBuf>, path: PathBuf) {
    if !dirs.contains(&path) {
        dirs.push(path);
    }
}

/// Remove `path` from bookmarks.
pub fn remove(dirs: &mut Vec<PathBuf>, path: &Path) {
    dirs.retain(|p| p != path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_new_path_appends() {
        let mut dirs = vec![PathBuf::from("/a")];
        add(&mut dirs, PathBuf::from("/b"));
        assert_eq!(dirs, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn add_existing_path_is_noop() {
        let mut dirs = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        add(&mut dirs, PathBuf::from("/a"));
        assert_eq!(dirs.len(), 2);
    }

    #[test]
    fn remove_existing_path() {
        let mut dirs = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        remove(&mut dirs, Path::new("/a"));
        assert_eq!(dirs, vec![PathBuf::from("/b")]);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut dirs = vec![PathBuf::from("/a")];
        remove(&mut dirs, Path::new("/z"));
        assert_eq!(dirs.len(), 1);
    }

    #[test]
    fn load_nonexistent_dir_returns_empty() {
        let missing = PathBuf::from("/tmp/crabmander-test-bookmarks-nonexistent-xyzzy-99999");
        assert_eq!(load(&missing), Vec::<PathBuf>::new());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("crabmander-test-bookmarks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let expected = vec![PathBuf::from("/foo"), PathBuf::from("/bar/baz")];
        save(&dir, &expected);
        let loaded = load(&dir);
        assert_eq!(loaded, expected);
        std::fs::remove_dir_all(&dir).ok();
    }
}
