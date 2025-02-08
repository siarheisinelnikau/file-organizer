//! Public, unauthenticated stable-release updates. Never executed by tests.
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/file-organizer";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const API: &str = "https://api.github.com/repos/siarheisinelnikau/file-organizer/releases/latest";
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub version: Version,
    pub asset: Asset,
    pub checksum: Option<Asset>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(concat!("FileOrganizer/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn official_url(url: &str, tag: &str, name: &str) -> bool {
    // Exact canonical URL, not merely a trusted-looking host prefix.
    url == format!("{REPOSITORY}/releases/download/{tag}/{name}")
}

pub fn select(release: Release, installed: &str) -> Result<Option<Update>> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(release.tag_name.trim_start_matches('v'))?;
    if !version.pre.is_empty() || version <= Version::parse(installed)? {
        return Ok(None);
    }
    let expected = format!("FileOrganizer-v{version}.exe");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected)
        .context("New release has no compatible standalone executable")?
        .clone();
    ensure!(
        asset.size > 0 && asset.size <= MAX_DOWNLOAD,
        "Release asset has an invalid size"
    );
    ensure!(
        official_url(&asset.browser_download_url, &release.tag_name, &asset.name),
        "Executable is not a canonical repository asset"
    );
    let checksum = release
        .assets
        .into_iter()
        .find(|a| a.name == "SHA256SUMS.txt");
    if let Some(ref a) = checksum {
        ensure!(
            official_url(&a.browser_download_url, &release.tag_name, &a.name),
            "Checksum is not a canonical repository asset"
        );
    }
    Ok(Some(Update {
        version,
        asset,
        checksum,
    }))
}

pub fn check() -> Result<Option<Update>> {
    let mut response = agent().get(API).header("Accept", "application/vnd.github+json").call()
        .context("Release check unavailable (repository/release may not be published, offline, or rate limited)")?;
    let json = response
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()?;
    select(serde_json::from_str(&json)?, VERSION)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn checksum(text: &str, name: &str) -> Result<String> {
    let mut found = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(file)) = (parts.next(), parts.next()) else {
            continue;
        };
        if file.trim_start_matches('*') == name {
            ensure!(
                valid_hash(hash) && parts.next().is_none(),
                "Malformed SHA256SUMS entry"
            );
            ensure!(found.is_none(), "Duplicate SHA256SUMS entry");
            found = Some(hash.to_lowercase());
        }
    }
    found.context("Executable checksum missing from SHA256SUMS.txt")
}

#[derive(Clone, Debug)]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
    pub version: Version,
}

/// Validate the release's PE platform before replacing a working installation.
pub fn verify_executable(path: &Path) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .context("Update is not a complete Windows executable")?;
    ensure!(
        &dos[..2] == b"MZ",
        "Update has no Windows executable header"
    );
    let offset = u32::from_le_bytes(dos[60..64].try_into()?) as u64;
    ensure!(
        (64..=1024 * 1024).contains(&offset),
        "Invalid PE header offset"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut pe = [0; 26];
    file.read_exact(&mut pe).context("Incomplete PE header")?;
    ensure!(
        &pe[..4] == b"PE\0\0"
            && u16::from_le_bytes([pe[4], pe[5]]) == 0x8664
            && u16::from_le_bytes([pe[24], pe[25]]) == 0x20b,
        "Update must be a Windows x64 PE32+ executable"
    );
    Ok(())
}

pub fn download(
    update: &Update,
    directory: &Path,
    progress: impl Fn(u64, u64),
) -> Result<Downloaded> {
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "In-app installation currently supports Windows x64 only"
    );
    let client = agent();
    let expected = if let Some(hash) = update
        .asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
    {
        ensure!(valid_hash(hash), "Invalid release asset digest");
        hash.to_lowercase()
    } else {
        let asset = update
            .checksum
            .as_ref()
            .context("Release must supply a SHA-256 digest or SHA256SUMS.txt")?;
        let text = client
            .get(&asset.browser_download_url)
            .call()?
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()?;
        checksum(&text, &update.asset.name)?
    };
    fs::create_dir_all(directory)?;
    crate::filesystem::check_path(directory)?;
    let path = directory.join(format!("update-{}.exe", uuid::Uuid::new_v4()));
    let result = (|| -> Result<Downloaded> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut response = client.get(&update.asset.browser_download_url).call()?;
        let mut reader = response
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD)
            .reader();
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0; 64 * 1024];
        let mut last_percent = 101;
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            ensure!(
                bytes <= update.asset.size && bytes <= MAX_DOWNLOAD,
                "Download is larger than its published size"
            );
            output.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
            let percent = bytes * 100 / update.asset.size;
            if percent != last_percent {
                progress(bytes, update.asset.size);
                last_percent = percent;
            }
        }
        ensure!(bytes == update.asset.size, "Incomplete download");
        let actual = format!("{:x}", hash.finalize());
        ensure!(
            actual == expected,
            "SHA-256 verification failed; installed executable was preserved"
        );
        output.sync_all()?;
        drop(output);
        verify_executable(&path)?;
        Ok(Downloaded {
            path: path.clone(),
            sha256: actual,
            version: update.version.clone(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result
}

/// Prepare a copied helper so the installed executable can be replaced after exit.
pub fn launch_helper(download: &Downloaded) -> Result<()> {
    let current = std::env::current_exe()?;
    crate::filesystem::check_path(&current)?;
    ensure!(
        crate::filesystem::digest(&download.path)? == download.sha256,
        "Staged update changed"
    );
    let helper = download
        .path
        .with_file_name(format!("helper-{}.exe", uuid::Uuid::new_v4()));
    fs::copy(&current, &helper)?;
    let mut command = Command::new(&helper);
    command
        .arg("--apply-update")
        .arg(std::process::id().to_string())
        .arg(&current)
        .arg(&download.path)
        .arg(&download.sha256)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    if let Err(e) = command.spawn() {
        let _ = fs::remove_file(helper);
        return Err(e.into());
    }
    Ok(())
}

/// Headless helper entry point. Never call this during development verification.
pub fn apply_helper(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 6 && args[1] == "--apply-update",
        "Invalid updater arguments"
    );
    let pid: u32 = args[2].parse()?;
    let target = Path::new(&args[3]);
    let staged = Path::new(&args[4]);
    ensure!(valid_hash(&args[5]), "Invalid checksum");
    crate::filesystem::check_path(target)?;
    crate::filesystem::check_path(staged)?;
    ensure!(
        crate::filesystem::digest(staged)? == args[5],
        "Staged update verification failed"
    );
    verify_executable(staged)?;
    wait_for_parent(pid)?;
    let backup = target.with_file_name(format!(".FileOrganizer-{}.backup", uuid::Uuid::new_v4()));
    // Copy beside the target first: app data and the executable may live on different volumes.
    let replacement = target.with_file_name(format!(".FileOrganizer-{}.new", uuid::Uuid::new_v4()));
    let mut input = crate::filesystem::LockedFile::open(staged, false)?;
    input.copy_to(&replacement)?;
    ensure!(
        crate::filesystem::digest(&replacement)? == args[5],
        "Replacement verification failed"
    );
    drop(input);
    fs::rename(target, &backup)
        .context("Cannot replace the executable; check folder permissions and running instances")?;
    if let Err(e) = fs::rename(&replacement, target) {
        if let Err(restore) = fs::rename(&backup, target) {
            anyhow::bail!(
                "Install failed: {e}. Backup restoration also failed: {restore}. Original executable is at {}",
                backup.display()
            );
        }
        return Err(e).context("Install failed; previous executable was restored");
    }
    let mut restart = Command::new(target);
    restart
        .arg("--cleanup-update")
        .arg(&backup)
        .arg(std::env::current_exe()?)
        .arg(staged);
    if let Err(e) = restart.spawn() {
        fs::remove_file(target).with_context(|| format!("Restart failed: {e}; replacement could not be removed. Original executable is at {}", backup.display()))?;
        fs::rename(&backup, target).with_context(|| {
            format!(
                "Restart failed: {e}; backup restoration failed. Original executable is at {}",
                backup.display()
            )
        })?;
        return Err(e).context("Restart failed; previous executable was restored");
    }
    Ok(())
}

#[cfg(windows)]
fn wait_for_parent(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    // No elevation. The helper only waits for its own application process.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(87) {
            return Ok(());
        } // PID has already exited.
        return Err(error).context(
            "Cannot confirm that the old application exited; installation was not attempted",
        );
    }
    let status = unsafe { WaitForSingleObject(handle, 120_000) };
    unsafe {
        CloseHandle(handle);
    }
    ensure!(
        status == WAIT_OBJECT_0,
        "Application did not exit in time; update was not installed"
    );
    Ok(())
}

#[cfg(not(windows))]
fn wait_for_parent(_pid: u32) -> Result<()> {
    anyhow::bail!("Installation requires Windows")
}

pub fn clean_after_update(args: &[String], directory: &Path) -> Result<()> {
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
