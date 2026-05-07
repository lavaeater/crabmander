use clap::Parser;
use cli::Cli;

use crate::app::App;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod logging;
mod tui;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let args = Cli::parse();

    if args.install_desktop_entry {
        install_desktop_entry()?;
        return Ok(());
    }

    let mut app = App::new(args.tick_rate, args.frame_rate)?;
    app.run().await?;
    Ok(())
}

fn install_desktop_entry() -> color_eyre::Result<()> {
    use std::io::Write;

    // Resolve the path to the running binary so the .desktop Exec points at it exactly.
    let exe = std::env::current_exe()?;
    let exe_path = exe.display();

    let desktop = format!(
        "[Desktop Entry]\n\
         Name=Crabmander\n\
         Comment=Twin-pane TUI file manager\n\
         Exec={exe_path}\n\
         Icon=utilities-file-manager\n\
         Terminal=true\n\
         Type=Application\n\
         Categories=Utility;FileManager;\n\
         Keywords=files;manager;tui;\n"
    );

    let dir = dirs_next()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("crabmander.desktop");
    std::fs::File::create(&path)?.write_all(desktop.as_bytes())?;

    // Best-effort refresh of the desktop database.
    std::process::Command::new("update-desktop-database")
        .arg(&dir)
        .status()
        .ok();

    println!("Desktop entry installed to {}", path.display());
    println!("You may need to log out and back in for it to appear in your menu.");
    Ok(())
}

fn dirs_next() -> color_eyre::Result<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    Ok(home.join(".local/share/applications"))
}
