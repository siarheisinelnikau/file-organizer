//! FileOrganizer product configuration; update transport and installation are shared.
use anyhow::{Result, ensure};
pub use rust_auto_updator::{Event, Prepared as Downloaded, Release as Update, Service};
use std::{fs, path::Path};

pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/file-organizer";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn config() -> Result<rust_auto_updator::Config, String> {
    let mut config = rust_auto_updator::Config::new(
        "siarheisinelnikau",
        "file-organizer",
        VERSION,
        "FileOrganizer",
    )?;
    config.max_executable_size = 128 * 1024 * 1024;
    config.allow_asset_digest = true;
    config.install.stage_prefix = ".FileOrganizer-update-".into();
    config.install.helper_name = "FileOrganizer-helper.exe".into();
    Ok(config)
}

pub fn apply_helper(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 2,
        "Legacy helpers must finish using their original executable"
    );
    rust_auto_updator::apply_update(&config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)
}

pub fn launch_helper(prepared: &mut Downloaded) -> Result<(), String> {
    rust_auto_updator::handoff(prepared)
}

/// Accept cleanup from both the shared protocol and already released old helpers.
pub fn clean_after_update(args: &[String], directory: &Path) -> Result<()> {
    if args.len() == 3 {
        let config = config().map_err(anyhow::Error::msg)?;
        rust_auto_updator::cleanup_after_update(args[2].clone().into(), config.install);
        return Ok(());
    }
    clean_legacy(args, directory)
}

fn clean_legacy(args: &[String], directory: &Path) -> Result<()> {
    ensure!(args.len() == 5, "Invalid cleanup arguments");
    let current = std::env::current_exe()?;
    let backup = Path::new(&args[2]);
    let helper = Path::new(&args[3]);
    let staged = Path::new(&args[4]);
    ensure!(
        backup.parent() == current.parent()
            && backup
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with(".FileOrganizer-"),
        "Invalid backup path"
    );
    for (path, prefix) in [(helper, "helper-"), (staged, "update-")] {
        crate::filesystem::inside(path, &[directory.to_string_lossy().into_owned()])?;
        ensure!(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with(prefix),
            "Invalid staging path"
        );
    }
    // A helper may still be exiting. Retaining it is harmless if Windows refuses deletion.
    for path in [backup, helper, staged] {
        let _ = fs::remove_file(path);
    }
    if let Some(parent) = directory.parent() {
        let error = parent.join("update-error.txt");
        if crate::filesystem::check_path(&error).is_ok() {
            let _ = fs::remove_file(error);
        }
    }
    Ok(())
}
