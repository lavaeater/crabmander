#![allow(dead_code)] // Remove this once you start using the code

use std::{collections::HashMap, env, path::PathBuf, sync::Arc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::ProjectDirs;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, de::Deserializer};
use std::sync::LazyLock;
use tracing::error;

use crate::{
    app::Mode,
    keymap::{BindingProblem, Keymap, RawBindings},
};

const CONFIG: &str = include_str!("../.config/config.json5");

#[derive(Clone, Debug, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub config_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default, flatten)]
    pub app: AppConfig,
    /// Raw keybinding entries from the user's config; resolved into `Config::keymap`.
    #[serde(default)]
    pub keybindings: RawBindings,
    /// Defaults merged with the user's keybindings. Built in `Config::new`.
    #[serde(skip)]
    pub keymap: Arc<Keymap>,
    #[serde(default)]
    pub styles: Styles,
    /// Opaline theme name (kebab-case). Run `crabmander --list-themes` to see options.
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app: AppConfig::default(),
            keybindings: RawBindings::default(),
            keymap: Arc::default(),
            styles: Styles::default(),
            theme: default_theme(),
        }
    }
}

fn default_theme() -> String {
    "catppuccin-mocha".into()
}

/// The keybindings shipped in the binary (`.config/config.json5`).
pub fn default_keybindings() -> RawBindings {
    json5::from_str::<Config>(CONFIG).map_or_else(|_| RawBindings::new(), |c| c.keybindings)
}

pub static PROJECT_NAME: LazyLock<String> =
    LazyLock::new(|| env!("CARGO_CRATE_NAME").to_uppercase());
pub static DATA_FOLDER: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    env::var(format!("{}_DATA", *PROJECT_NAME))
        .ok()
        .map(PathBuf::from)
});
pub static CONFIG_FOLDER: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    env::var(format!("{}_CONFIG", *PROJECT_NAME))
        .ok()
        .map(PathBuf::from)
});

impl Config {
    pub fn new() -> color_eyre::Result<Self, config::ConfigError> {
        let default_config: Config = json5::from_str(CONFIG).unwrap();
        let data_dir = get_data_dir();
        let config_dir = get_config_dir();
        let mut builder = config::Config::builder()
            .set_default("data_dir", data_dir.to_str().unwrap())?
            .set_default("config_dir", config_dir.to_str().unwrap())?;

        let config_files = [
            ("config.json5", config::FileFormat::Json5),
            ("config.json", config::FileFormat::Json),
            ("config.yaml", config::FileFormat::Yaml),
            ("config.toml", config::FileFormat::Toml),
            ("config.ini", config::FileFormat::Ini),
        ];
        let mut found_config = false;
        for (file, format) in &config_files {
            let source = config::File::from(config_dir.join(file))
                .format(*format)
                .required(false);
            builder = builder.add_source(source);
            if config_dir.join(file).exists() {
                found_config = true;
            }
        }
        if !found_config {
            error!("No configuration file found. Application may not behave as expected");
        }

        // A broken user config must not stop startup (FR-009): use the defaults and report it.
        let (mut cfg, unreadable) = match builder.build().and_then(config::Config::try_deserialize)
        {
            Ok(cfg) => (cfg, None),
            Err(err) => {
                error!("could not read the config file, using defaults: {err}");
                let cfg = Self {
                    app: AppConfig {
                        data_dir,
                        config_dir,
                    },
                    keybindings: RawBindings::new(),
                    ..default_config.clone()
                };
                (cfg, Some(BindingProblem::config_unreadable(err)))
            }
        };

        let mut keymap = Keymap::build(&default_config.keybindings, &cfg.keybindings);
        keymap.problems.splice(0..0, unreadable);
        cfg.keymap = Arc::new(keymap);
        for (mode, default_styles) in &default_config.styles.0 {
            let user_styles = cfg.styles.0.entry(*mode).or_default();
            for (style_key, style) in default_styles {
                user_styles.entry(style_key.clone()).or_insert(*style);
            }
        }

        Ok(cfg)
    }
}

pub fn get_data_dir() -> PathBuf {
    if let Some(s) = DATA_FOLDER.clone() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.data_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".data")
    }
}

pub fn get_config_dir() -> PathBuf {
    if let Some(s) = CONFIG_FOLDER.clone() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.config_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".config")
    }
}

fn project_directory() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "kdheepak", env!("CARGO_PKG_NAME"))
}

/// Parses one key (`ctrl-a`, `F5`, `<`-less). Modifier and key names are case-insensitive; a
/// single character is taken as written, so `Y` means Shift+Y.
fn parse_key_event(raw: &str) -> color_eyre::Result<KeyEvent, String> {
    let (remaining, modifiers) = extract_modifiers(raw);
    if remaining.chars().count() == 1 {
        return parse_key_code_with_modifiers(remaining, modifiers);
    }
    parse_key_code_with_modifiers(&remaining.to_ascii_lowercase(), modifiers)
}

fn extract_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        let mut matched = false;
        for (prefix, modifier) in [
            ("ctrl-", KeyModifiers::CONTROL),
            ("alt-", KeyModifiers::ALT),
            ("shift-", KeyModifiers::SHIFT),
        ] {
            // A lone "-" after a modifier is the minus key, not another prefix.
            if current.len() > prefix.len()
                && current.is_char_boundary(prefix.len())
                && current[..prefix.len()].eq_ignore_ascii_case(prefix)
            {
                modifiers.insert(modifier);
                current = &current[prefix.len()..];
                matched = true;
            }
        }
        if !matched {
            break;
        }
    }

    (current, modifiers)
}

fn parse_key_code_with_modifiers(
    raw: &str,
    mut modifiers: KeyModifiers,
) -> color_eyre::Result<KeyEvent, String> {
    let c = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" => KeyCode::Char(' '),
        "hyphen" | "minus" => KeyCode::Char('-'),
        "equals" => KeyCode::Char('='),
        "tab" => KeyCode::Tab,
        c if c.chars().count() == 1 => {
            let mut c = c.chars().next().unwrap_or_default();
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse {raw}")),
    };
    Ok(KeyEvent::new(c, modifiers))
}

pub fn key_event_to_string(key_event: &KeyEvent) -> String {
    let char;
    let key_code = match key_event.code {
        KeyCode::Backspace => "backspace",
        KeyCode::Enter => "enter",
        KeyCode::Left => "left",
        KeyCode::Right => "right",
        KeyCode::Up => "up",
        KeyCode::Down => "down",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pageup",
        KeyCode::PageDown => "pagedown",
        KeyCode::Tab => "tab",
        KeyCode::BackTab => "backtab",
        KeyCode::Delete => "delete",
        KeyCode::Insert => "insert",
        KeyCode::F(c) => {
            char = format!("f({c})");
            &char
        }
        KeyCode::Char(' ') => "space",
        KeyCode::Char(c) => {
            char = c.to_string();
            &char
        }
        KeyCode::Esc => "esc",
        KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::Menu
        | KeyCode::ScrollLock
        | KeyCode::Media(_)
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::KeypadBegin
        | KeyCode::Modifier(_) => "",
    };

    let mut modifiers = Vec::with_capacity(3);

    if key_event.modifiers.intersects(KeyModifiers::CONTROL) {
        modifiers.push("ctrl");
    }

    if key_event.modifiers.intersects(KeyModifiers::SHIFT) {
        modifiers.push("shift");
    }

    if key_event.modifiers.intersects(KeyModifiers::ALT) {
        modifiers.push("alt");
    }

    let mut key = modifiers.join("-");

    if !key.is_empty() {
        key.push('-');
    }
    key.push_str(key_code);

    key
}

pub fn parse_key_sequence(raw: &str) -> color_eyre::Result<Vec<KeyEvent>, String> {
    if raw.chars().filter(|c| *c == '>').count() != raw.chars().filter(|c| *c == '<').count() {
        return Err(format!("Unable to parse `{raw}`"));
    }
    let raw = if raw.contains("><") {
        raw
    } else {
        let raw = raw.strip_prefix('<').unwrap_or(raw);

        raw.strip_prefix('>').unwrap_or(raw)
    };
    let sequences = raw
        .split("><")
        .map(|seq| {
            if let Some(s) = seq.strip_prefix('<') {
                s
            } else if let Some(s) = seq.strip_suffix('>') {
                s
            } else {
                seq
            }
        })
        .collect::<Vec<_>>();

    sequences.into_iter().map(parse_key_event).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Styles(pub HashMap<Mode, HashMap<String, Style>>);

impl<'de> Deserialize<'de> for Styles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<Mode, HashMap<String, String>>::deserialize(deserializer)?;

        let styles = parsed_map
            .into_iter()
            .map(|(mode, inner_map)| {
                let converted_inner_map = inner_map
                    .into_iter()
                    .map(|(str, style)| (str, parse_style(&style)))
                    .collect();
                (mode, converted_inner_map)
            })
            .collect();

        Ok(Styles(styles))
    }
}

pub fn parse_style(line: &str) -> Style {
    let (foreground, background) =
        line.split_at(line.to_lowercase().find("on ").unwrap_or(line.len()));
    let foreground = process_color_string(foreground);
    let background = process_color_string(&background.replace("on ", ""));

    let mut style = Style::default();
    if let Some(fg) = parse_color(&foreground.0) {
        style = style.fg(fg);
    }
    if let Some(bg) = parse_color(&background.0) {
        style = style.bg(bg);
    }
    style = style.add_modifier(foreground.1 | background.1);
    style
}

fn process_color_string(color_str: &str) -> (String, Modifier) {
    let color = color_str
        .replace("grey", "gray")
        .replace("bright ", "")
        .replace("bold ", "")
        .replace("underline ", "")
        .replace("inverse ", "");

    let mut modifiers = Modifier::empty();
    if color_str.contains("underline") {
        modifiers |= Modifier::UNDERLINED;
    }
    if color_str.contains("bold") {
        modifiers |= Modifier::BOLD;
    }
    if color_str.contains("inverse") {
        modifiers |= Modifier::REVERSED;
    }

    (color, modifiers)
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim_start();
    let s = s.trim_end();
    if s.contains("bright color") {
        let s = s.trim_start_matches("bright ");
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c.wrapping_shl(8)))
    } else if s.contains("color") {
        let c = s
            .trim_start_matches("color")
            .parse::<u8>()
            .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("gray") {
        let c = 232
            + s.trim_start_matches("gray")
                .parse::<u8>()
                .unwrap_or_default();
        Some(Color::Indexed(c))
    } else if s.contains("rgb") {
        let red = (s.as_bytes()[3] as char).to_digit(10).unwrap_or_default() as u8;
        let green = (s.as_bytes()[4] as char).to_digit(10).unwrap_or_default() as u8;
        let blue = (s.as_bytes()[5] as char).to_digit(10).unwrap_or_default() as u8;
        let c = 16 + red * 36 + green * 6 + blue;
        Some(Color::Indexed(c))
    } else if s == "bold black" {
        Some(Color::Indexed(8))
    } else if s == "bold red" {
        Some(Color::Indexed(9))
    } else if s == "bold green" {
        Some(Color::Indexed(10))
    } else if s == "bold yellow" {
        Some(Color::Indexed(11))
    } else if s == "bold blue" {
        Some(Color::Indexed(12))
    } else if s == "bold magenta" {
        Some(Color::Indexed(13))
    } else if s == "bold cyan" {
        Some(Color::Indexed(14))
    } else if s == "bold white" {
        Some(Color::Indexed(15))
    } else if s == "black" {
        Some(Color::Indexed(0))
    } else if s == "red" {
        Some(Color::Indexed(1))
    } else if s == "green" {
        Some(Color::Indexed(2))
    } else if s == "yellow" {
        Some(Color::Indexed(3))
    } else if s == "blue" {
        Some(Color::Indexed(4))
    } else if s == "magenta" {
        Some(Color::Indexed(5))
    } else if s == "cyan" {
        Some(Color::Indexed(6))
    } else if s == "white" {
        Some(Color::Indexed(7))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        action::Action,
        keymap::{KeyContext, Lookup, parse_seq},
    };

    fn defaults() -> RawBindings {
        default_keybindings()
    }

    /// Loads a user config string the way `Config::new` loads files.
    fn user_config(text: &str, format: config::FileFormat) -> RawBindings {
        config::Config::builder()
            .add_source(config::File::from_str(text, format))
            .build()
            .unwrap()
            .try_deserialize::<Config>()
            .unwrap()
            .keybindings
    }

    #[test]
    fn default_config_builds_a_keymap() {
        let km = Keymap::build(&defaults(), &RawBindings::new());
        assert_eq!(km.problems, vec![]);
        let normal = [KeyContext::Normal];
        assert_eq!(
            km.resolve(&normal, &parse_seq("<f5>").unwrap()),
            Lookup::Action(Action::Copy)
        );
        let git = [KeyContext::Git];
        assert_eq!(
            km.resolve(&git, &parse_seq("<a>").unwrap()),
            Lookup::Action(Action::GitStage)
        );
    }

    #[test]
    fn old_style_user_config_still_loads() {
        let user = user_config(
            r#"{ "keybindings": { "Normal": { "<ctrl-y>": "Copy", "<Ctrl-q>": "Quit" } } }"#,
            config::FileFormat::Json5,
        );
        let km = Keymap::build(&defaults(), &user);
        assert_eq!(km.problems, vec![]);
        let normal = [KeyContext::Normal];
        assert_eq!(
            km.resolve(&normal, &parse_seq("<ctrl-y>").unwrap()),
            Lookup::Action(Action::Copy)
        );
        assert_eq!(
            km.resolve(&normal, &parse_seq("<ctrl-q>").unwrap()),
            Lookup::Action(Action::Quit)
        );
        assert_eq!(
            km.resolve(&normal, &parse_seq("<f5>").unwrap()),
            Lookup::Action(Action::Copy)
        );
    }

    #[test]
    fn removal_works_in_every_config_format() {
        let cases = [
            (
                "keybindings:\n  Normal:\n    \"<q>\": ~\n    \"<ctrl-y>\": Copy\n",
                config::FileFormat::Yaml,
            ),
            (
                "[keybindings.Normal]\n\"<q>\" = \"Unbind\"\n\"<ctrl-y>\" = \"Copy\"\n",
                config::FileFormat::Toml,
            ),
            (
                r#"{ "keybindings": { "Normal": { "<q>": null, "<ctrl-y>": "Copy" } } }"#,
                config::FileFormat::Json,
            ),
        ];
        for (text, format) in cases {
            let km = Keymap::build(&defaults(), &user_config(text, format));
            assert_eq!(km.problems, vec![], "{format:?}");
            let normal = [KeyContext::Normal];
            assert_eq!(
                km.resolve(&normal, &parse_seq("<q>").unwrap()),
                Lookup::None,
                "{format:?}"
            );
            assert_eq!(
                km.resolve(&normal, &parse_seq("<ctrl-y>").unwrap()),
                Lookup::Action(Action::Copy),
                "{format:?}"
            );
        }
    }

    #[test]
    fn equals_key_name() {
        assert_eq!(
            parse_key_event("alt-equals").unwrap(),
            KeyEvent::new(KeyCode::Char('='), KeyModifiers::ALT)
        );
        assert_eq!(
            parse_key_event("alt-=").unwrap(),
            parse_key_event("alt-equals").unwrap()
        );
        assert_eq!(
            parse_key_event("alt--").unwrap(),
            KeyEvent::new(KeyCode::Char('-'), KeyModifiers::ALT)
        );
    }

    #[test]
    fn single_characters_keep_their_case() {
        assert_eq!(
            parse_key_event("Y").unwrap(),
            KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::empty())
        );
        assert_eq!(
            parse_key_event("Ctrl-q").unwrap(),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)
        );
    }

    #[test]
    fn test_parse_style_default() {
        let style = parse_style("");
        assert_eq!(style, Style::default());
    }

    #[test]
    fn test_parse_style_foreground() {
        let style = parse_style("red");
        assert_eq!(style.fg, Some(Color::Indexed(1)));
    }

    #[test]
    fn test_parse_style_background() {
        let style = parse_style("on blue");
        assert_eq!(style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn test_parse_style_modifiers() {
        let style = parse_style("underline red on blue");
        assert_eq!(style.fg, Some(Color::Indexed(1)));
        assert_eq!(style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn test_process_color_string() {
        let (color, modifiers) = process_color_string("underline bold inverse gray");
        assert_eq!(color, "gray");
        assert!(modifiers.contains(Modifier::UNDERLINED));
        assert!(modifiers.contains(Modifier::BOLD));
        assert!(modifiers.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_parse_color_rgb() {
        let color = parse_color("rgb123");
        let expected = 16 + 36 + 2 * 6 + 3;
        assert_eq!(color, Some(Color::Indexed(expected)));
    }

    #[test]
    fn test_parse_color_unknown() {
        let color = parse_color("unknown");
        assert_eq!(color, None);
    }

    #[test]
    fn test_simple_keys() {
        assert_eq!(
            parse_key_event("a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())
        );
    }

    #[test]
    fn test_with_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("alt-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );

        assert_eq!(
            parse_key_event("shift-esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_multiple_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-alt-a").unwrap(),
            KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )
        );

        assert_eq!(
            parse_key_event("ctrl-shift-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_reverse_multiple_modifiers() {
        assert_eq!(
            key_event_to_string(&KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            "ctrl-alt-a".to_string()
        );
    }

    #[test]
    fn test_invalid_keys() {
        assert!(parse_key_event("invalid-key").is_err());
        assert!(parse_key_event("ctrl-invalid-key").is_err());
    }

    #[test]
    fn test_case_insensitivity() {
        assert_eq!(
            parse_key_event("CTRL-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("AlT-eNtEr").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );
    }
}
