use std::path::{Path, PathBuf};

const FILE: &str = "last_paths.json";

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct LastPaths {
    pub left: Option<PathBuf>,
    pub right: Option<PathBuf>,
}

pub fn load(data_dir: &Path) -> LastPaths {
    let Ok(text) = std::fs::read_to_string(data_dir.join(FILE)) else {
        return LastPaths::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(data_dir: &Path, left: &Path, right: &Path) {
    let lp = LastPaths { left: Some(left.to_owned()), right: Some(right.to_owned()) };
    if let Ok(json) = serde_json::to_string(&lp) {
        let _ = std::fs::write(data_dir.join(FILE), json);
    }
}
