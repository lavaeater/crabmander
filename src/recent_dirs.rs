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
