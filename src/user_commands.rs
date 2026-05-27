use std::path::Path;

const FILE: &str = "user_commands.json";

pub fn load(data_dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(data_dir.join(FILE)) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&text).unwrap_or_default()
}

pub fn save(data_dir: &Path, cmds: &[String]) {
    if let Ok(json) = serde_json::to_string(cmds) {
        let _ = std::fs::write(data_dir.join(FILE), json);
    }
}

pub fn add(cmds: &mut Vec<String>, template: String) {
    if !cmds.contains(&template) {
        cmds.push(template);
    }
}

pub fn remove(cmds: &mut Vec<String>, template: &str) {
    cmds.retain(|c| c != template);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_new_appends() {
        let mut cmds = vec!["a".to_owned()];
        add(&mut cmds, "b".to_owned());
        assert_eq!(cmds, vec!["a", "b"]);
    }

    #[test]
    fn add_duplicate_is_noop() {
        let mut cmds = vec!["a".to_owned()];
        add(&mut cmds, "a".to_owned());
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn remove_existing() {
        let mut cmds = vec!["a".to_owned(), "b".to_owned()];
        remove(&mut cmds, "a");
        assert_eq!(cmds, vec!["b"]);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut cmds = vec!["a".to_owned()];
        remove(&mut cmds, "z");
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir()
            .join(format!("crabmander-test-user-cmds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let expected = vec!["ffmpeg -i {file} out.mp3".to_owned(), "cp {file} {other}".to_owned()];
        save(&dir, &expected);
        let loaded = load(&dir);
        assert_eq!(loaded, expected);
        std::fs::remove_dir_all(&dir).ok();
    }
}
