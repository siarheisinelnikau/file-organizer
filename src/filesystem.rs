//! Filesystem boundary. No traversal through symbolic links or Windows reparse points.
use crate::model::{Config, FileFact, Stamp, path_key};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const FILE_LIMIT: usize = 10_000;
pub const DIRECTORY_LIMIT: usize = 2_000;

pub fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn stamp(meta: &Metadata) -> Result<Stamp> {
    Ok(Stamp {
        bytes: meta.len(),
        modified_ns: meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    })
}

pub fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// Check each existing component before canonicalizing, including root ancestors.
pub fn check_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "Path must be absolute: {}",
        path.display()
    );
    let mut current = PathBuf::new();
    for part in path.components() {
        ensure!(
            !matches!(part, Component::ParentDir | Component::CurDir),
            "Relative path components are forbidden"
        );
        current.push(part);
        #[cfg(windows)]
        if let Component::Prefix(prefix) = part {
            ensure!(
                matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ),
                "Only absolute local drive paths are supported"
            );
            // A prefix such as \\?\C: is not itself an inspectable directory until RootDir is appended.
            continue;
        }
        #[cfg(windows)]
        if let Component::Normal(name) = part {
            ensure!(
                !name.to_string_lossy().contains(':'),
                "Alternate data stream paths are forbidden"
            );
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) => ensure!(
                !is_link(&meta),
                "Symlink/reparse point refused: {}",
                current.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("Inspect {}", current.display())),
        }
    }
    Ok(())
}

pub fn root(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    check_path(path)?;
    ensure!(
        path.is_dir(),
        "Approved root must already be a directory: {}",
        path.display()
    );
    Ok(fs::canonicalize(path)?)
}

pub fn inside(path: &Path, approved: &[String]) -> Result<PathBuf> {
    check_path(path)?;
    // Resolve the deepest existing ancestor, then append the missing suffix.
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.try_exists()? {
        suffix.push(
            ancestor
                .file_name()
                .context("No existing ancestor")?
                .to_owned(),
        );
        ancestor = ancestor.parent().context("No parent")?;
    }
    let mut resolved = fs::canonicalize(ancestor)?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    for candidate in approved {
        let approved = root(candidate)?;
        if resolved.starts_with(&approved) && resolved != approved {
            return Ok(resolved);
        }
    }
    bail!("Path is outside approved roots: {}", path.display())
}

pub fn ensure_parent(path: &Path, approved: &[String]) -> Result<()> {
    let resolved = inside(path, approved)?;
    let parent = resolved.parent().context("Missing parent")?;
    let mut missing = Vec::new();
    let mut cursor = parent;
    while !cursor.try_exists()? {
        missing.push(cursor.to_owned());
        cursor = cursor.parent().context("Missing directory ancestor")?;
    }
    for dir in missing.into_iter().rev() {
        check_path(dir.parent().unwrap())?;
        fs::create_dir(&dir).with_context(|| format!("Create {}", dir.display()))?;
    }
    check_path(parent)?;
    Ok(())
}

pub struct Scan {
    pub files: Vec<FileFact>,
    pub occupied: HashSet<String>,
    pub notices: Vec<String>,
}

pub fn scan(config: &Config) -> Result<Scan> {
    ensure!(
        !config.source_roots.is_empty(),
        "Add at least one source folder"
    );
    let sources: Vec<_> = config
        .source_roots
        .iter()
        .map(|p| root(p))
        .collect::<Result<_>>()?;
    for destination in &config.destination_roots {
        root(destination)?;
    }
    let mut excluded: Vec<_> = config
        .rules
        .iter()
        .filter(|r| r.enabled && r.action != crate::model::Action::Rename)
        .filter_map(|r| {
            let p = Path::new(&r.destination).join(".validation");
            inside(&p, &config.destination_roots)
                .ok()
                .and_then(|p| p.parent().map(Path::to_owned))
        })
        .collect();
    excluded.sort();
    excluded.dedup();
    let mut out = Scan {
        files: Vec::new(),
        occupied: HashSet::new(),
        notices: Vec::new(),
    };
    let mut seen = HashSet::new();
    let mut dirs = 0;
    let mut seen_directories = HashSet::new();
    let mut total_entries = 0;
    for source in sources {
        ensure!(
            !excluded.iter().any(|d| source.starts_with(d)),
            "A source is inside an output folder; separate source and destination folders"
        );
        let mut stack = vec![source.clone()];
        while let Some(dir) = stack.pop() {
            if !seen_directories.insert(path_key(&dir)) {
                continue;
            }
            dirs += 1;
            ensure!(
                dirs <= DIRECTORY_LIMIT,
                "Scan exceeds {DIRECTORY_LIMIT} folders; choose a narrower source"
            );
            check_path(&dir)?;
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    out.notices
                        .push(format!("Cannot read {}: {e}", dir.display()));
                    continue;
                }
            };
            for entry in entries {
                total_entries += 1;
                ensure!(
                    total_entries <= 30_000,
                    "Scan exceeds 30000 directory entries; choose a narrower source"
                );
                let entry = entry?;
                let path = entry.path();
                let meta = fs::symlink_metadata(&path)?;
                out.occupied.insert(path_key(&path));
                if is_link(&meta) {
                    out.notices
                        .push(format!("Link skipped: {}", path.display()));
                    continue;
                }
                if meta.is_dir() {
                    if config.recursive && !excluded.iter().any(|d| path.starts_with(d)) {
                        stack.push(path);
                    }
                } else if meta.is_file() && seen.insert(path_key(&path)) {
                    ensure!(
                        out.files.len() < FILE_LIMIT,
                        "Scan exceeds {FILE_LIMIT} files; choose a narrower source"
                    );
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with(".fo-")
                        || [".part", ".crdownload", ".tmp", ".download"]
                            .iter()
                            .any(|s| name.ends_with(s))
                    {
                        continue;
                    }
                    out.files.push(FileFact {
                        path,
                        source_root: source.clone(),
                        stamp: stamp(&meta)?,
                    });
                }
            }
        }
    }
    let mut output_entries = 0;
    for dir in excluded {
        check_path(&dir)?;
        if !dir.try_exists()? {
            continue;
        }
        for entry in fs::read_dir(dir)? {
            output_entries += 1;
            ensure!(
                output_entries <= 30_000,
                "Output folders exceed 30000 entries; narrow the rules"
            );
            out.occupied.insert(path_key(&entry?.path()));
        }
    }
    out.files.sort_by_key(|f| path_key(&f.path));
    out.notices.truncate(200);
    Ok(out)
}

/// On Windows, deny concurrent write/delete sharing and open reparse points themselves.
pub struct LockedFile {
    file: File,
    path: PathBuf,
}

impl LockedFile {
    pub fn open(path: &Path, deletable: bool) -> Result<Self> {
        check_path(path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x0020_0000);
            if deletable {
                options.access_mode(0x8000_0000 | 0x0001_0000);
            }
        }
        #[cfg(not(windows))]
        let _ = deletable;
        let file = options.open(path).with_context(|| {
            format!(
                "Open stable file {} (it may still be in use)",
                path.display()
            )
        })?;
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && !is_link(&meta),
            "Only regular, non-reparse files are supported"
        );
        Ok(Self {
            file,
            path: path.to_owned(),
        })
    }

    pub fn stamp(&self) -> Result<Stamp> {
        stamp(&self.file.metadata()?)
    }

    pub fn digest(&mut self) -> Result<String> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = self.file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    pub fn copy_to(&mut self, path: &Path) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut target = OpenOptions::new().write(true).create_new(true).open(path)?;
        std::io::copy(&mut self.file, &mut target)?;
        target.flush()?;
        target.sync_all()?;
        Ok(())
    }

    pub fn delete(self) -> Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
            };
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            // The handle owns DELETE access and prevents concurrent writers/path replacement.
            let ok = unsafe {
                SetFileInformationByHandle(
                    self.file.as_raw_handle(),
                    FileDispositionInfo,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                )
            };
            ensure!(
                ok != 0,
                "Delete {}: {}",
                self.path.display(),
                std::io::Error::last_os_error()
            );
            Ok(())
        }
        #[cfg(not(windows))]
        {
            check_path(&self.path)?;
            fs::remove_file(&self.path)?;
            Ok(())
        }
    }
}

pub fn digest(path: &Path) -> Result<String> {
    LockedFile::open(path, false)?.digest()
}

/// Publish within the destination directory without replacing an existing file.
pub fn publish(stage: &Path, destination: &Path) -> Result<()> {
    check_path(destination)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let from: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // Omitting MOVEFILE_REPLACE_EXISTING is essential: a collision must fail atomically.
        let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) };
        ensure!(
            ok != 0,
            "Publish without overwriting: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(stage, destination).context("Publish without overwriting")?;
        fs::remove_file(stage)?;
    }
    Ok(())
}
