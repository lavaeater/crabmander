use std::path::{Path, PathBuf};

const FILE: &str = "git_repos.json";

pub fn load(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(data_dir.join(FILE)) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

pub fn save(data_dir: &Path, repos: &[PathBuf]) {
    let strings: Vec<&str> = repos.iter().filter_map(|p| p.to_str()).collect();
    if let Ok(json) = serde_json::to_string(&strings) {
        let _ = std::fs::write(data_dir.join(FILE), json);
    }
}

/// Add `git_root` to the global repo list (dedup, most-recently-used first).
pub fn add(repos: &mut Vec<PathBuf>, git_root: PathBuf) {
    repos.retain(|p| p != &git_root);
    repos.insert(0, git_root);
}

/// Remove `git_root` from the list.
pub fn remove(repos: &mut Vec<PathBuf>, git_root: &Path) {
    repos.retain(|p| p != git_root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_new_repo_prepends() {
        let mut repos = vec![PathBuf::from("/a")];
        add(&mut repos, PathBuf::from("/b"));
        assert_eq!(repos[0], PathBuf::from("/b"));
        assert_eq!(repos[1], PathBuf::from("/a"));
    }

    #[test]
    fn add_existing_moves_to_front() {
        let mut repos = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        add(&mut repos, PathBuf::from("/b"));
        assert_eq!(repos, vec![PathBuf::from("/b"), PathBuf::from("/a")]);
    }

    #[test]
    fn remove_existing() {
        let mut repos = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        remove(&mut repos, Path::new("/a"));
        assert_eq!(repos, vec![PathBuf::from("/b")]);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut repos = vec![PathBuf::from("/a")];
        remove(&mut repos, Path::new("/z"));
        assert_eq!(repos.len(), 1);
    }
}
