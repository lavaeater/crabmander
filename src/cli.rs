use clap::Parser;

use crate::config::{get_config_dir, get_data_dir};

#[derive(Parser, Debug)]
#[command(author, version = version(), about)]
pub struct Cli {
    /// Tick rate, i.e. number of ticks per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 4.0)]
    pub tick_rate: f64,

    /// Frame rate, i.e. number of frames per second
    #[arg(short, long, value_name = "FLOAT", default_value_t = 60.0)]
    pub frame_rate: f64,

    /// Install a .desktop entry for the current user and exit
    #[arg(long)]
    pub install_desktop_entry: bool,
}

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("VERGEN_GIT_DESCRIBE"),
    " (",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

pub fn version() -> String {
    let author = clap::crate_authors!();

    // let current_exe_path = PathBuf::from(clap::crate_name!()).display().to_string();
    let config_dir_path = get_config_dir().display().to_string();
    let data_dir_path = get_data_dir().display().to_string();

    format!(
        "\
{VERSION_MESSAGE}

Authors: {author}

Config directory: {config_dir_path}
Data directory: {data_dir_path}"
    )
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn default_flags_are_applied() {
        let cli = Cli::try_parse_from(["crabmander"]).unwrap();
        assert!((cli.tick_rate - 4.0).abs() < f64::EPSILON);
        assert!((cli.frame_rate - 60.0).abs() < f64::EPSILON);
        assert!(!cli.install_desktop_entry);
    }

    #[test]
    fn tick_rate_flag_is_parsed() {
        let cli = Cli::try_parse_from(["crabmander", "--tick-rate", "10"]).unwrap();
        assert!((cli.tick_rate - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn frame_rate_flag_is_parsed() {
        let cli = Cli::try_parse_from(["crabmander", "--frame-rate", "30"]).unwrap();
        assert!((cli.frame_rate - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn install_desktop_entry_flag_is_parsed() {
        let cli = Cli::try_parse_from(["crabmander", "--install-desktop-entry"]).unwrap();
        assert!(cli.install_desktop_entry);
    }

    #[test]
    fn version_string_contains_config_dir() {
        let v = super::version();
        assert!(v.contains("Config directory:"));
        assert!(v.contains("Data directory:"));
    }
}
