#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

use std::path::PathBuf;

fn data_directory() -> anyhow::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA is unavailable"))?;
    Ok(PathBuf::from(base).join("FileOrganizer"))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
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
    if args.get(1).is_some_and(|s| s == "--cleanup-update") {
        let _ = file_organizer::updater::clean_after_update(&args, &directory.join("updates"));
    } else {
        anyhow::ensure!(args.len() == 1, "Unknown arguments");
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([900.0, 620.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "FileOrganizer",
        options,
        Box::new(move |cc| Ok(Box::new(ui::Organizer::new(cc, directory)))),
    )
    .map_err(|e| anyhow::anyhow!("Window initialization failed: {e}"))
}
