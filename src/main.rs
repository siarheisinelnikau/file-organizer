#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod branding;
mod design;
mod host;
mod renderer;
mod ui;

use std::path::PathBuf;

fn data_directory() -> anyhow::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA is unavailable"))?;
    Ok(PathBuf::from(base).join("FileOrganizer"))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--capture") {
        return ui::capture_screens(&args[2..]);
    }
    if args.get(1).is_some_and(|s| s == "--apply-update") {
        if let Err(e) = file_organizer::updater::apply_helper(&args) {
            // A visible UI is intentionally not opened by the updater helper.
            if let Ok(directory) = data_directory() {
                let _ = std::fs::write(directory.join("update-error.txt"), format!("{e:#}"));
            }
            return Err(e);
        }
        return Ok(());
    }
    let directory = data_directory()?;
    let cleanup = args
        .get(1)
        .is_some_and(|s| s == "--cleanup-update")
        .then(|| args.clone());
    let render_log = if args.get(1).is_some_and(|s| s == "--render-log") && args.len() == 3 {
        Some(PathBuf::from(&args[2]))
    } else {
        anyhow::ensure!(
            args.len() == 1 || args.get(1).is_some_and(|s| s == "--cleanup-update"),
            "Unknown arguments"
        );
        None
    };
    host::run(directory, render_log, cleanup)
}
