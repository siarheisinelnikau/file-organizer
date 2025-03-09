# FileOrganizer

A Windows desktop organizer that turns folder rules into a reviewable plan, then records every applied action for recovery and undo.

**Status:** first local implementation, version 0.1.0. Windows x64 compilation and fixture tests are the validation target. Interactive desktop validation and real application/settings screenshots are pending because this development session explicitly forbids launching the GUI. No screenshot placeholders or fabricated images are included. The planned [public repository](https://github.com/siarheisinelnikau/file-organizer) and releases have not been published by this session.

## What it does

- Visual, ordered rules with an enable switch and first-match behavior.
- Conditions for case-insensitive filename globs, extensions, relative source subfolders, byte size and age since last modification.
- Move, copy or rename, with filename tokens `{name}`, `{stem}` and `{ext}`.
- Downloads/archives, photos and documents templates, with destinations chosen by you.
- Dry-run plans with individual selection. Conflicts either skip the name or keep both using numbered names. Nothing silently overwrites an existing file.
- Folder monitoring with debouncing and two unchanged observations before a file becomes eligible. Automatic application requires an explicit opt-in; every application launch starts paused.
- Persistent SQLite activity journal, SHA-256 checks, conservative crash recovery and per-operation undo.
- Installed version, manual update checks, optional startup checks enabled by default, and a user-initiated download/verify/install/restart flow for future compatible public releases.

FileOrganizer is an independent MIT-licensed implementation. [File Juggler](https://www.filejuggler.com/) is a reference for the general user problem; no competitor code, assets or interface have been copied. There is no affiliation.

## Build and package

Use Rust 1.95 or newer with the Windows MSVC toolchain and existing Visual C++ build tools. No service, driver, browser runtime, AI model, account or API key is required. Dependencies are locked in `Cargo.lock`; SQLite and TLS support are bundled into the executable. The Windows x64 target uses the static CRT.

```powershell
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets -- --test-threads=2
.\scripts\package.ps1
```

Cargo is limited to two jobs and uses this repository's `target` directory. Add `--offline` to Cargo commands when the dependencies are already cached. Packaging builds without launching the application and produces `dist/FileOrganizer-v0.1.0.exe` and `dist/SHA256SUMS.txt`. The executable name has no architecture suffix because the initial distribution targets one architecture, Windows x64. Source builds for other platforms are not a supported release target.

For a first manual run, launch the packaged executable yourself. Start with a disposable folder and keep independent backups before using important files.

## First workflow

1. Open **Folders & settings**. Add existing absolute source folders and existing output roots. No real folders are preselected. Local drive paths are supported; symbolic links, junctions/reparse points and network paths are refused.
2. Open **Rules**, add a template, and enter a destination inside an approved output root. Missing subfolders can be created during application. For Rename, the destination stays in the original source folder.
3. Adjust conditions and the filename template. Conditions are ANDed. Extensions are comma separated, without a leading dot. `*` matches any characters, including separators in a subfolder condition; `?` matches one character. An empty subfolder pattern matches only the source root. Size is in bytes, and age uses modification time in minutes. Types are detected by extension, not content or EXIF.
4. Choose **Preview now**. The first observation waits for the settling interval (default 3 seconds); a background second observation refreshes the plan. Preview does not change source/output files. Saving settings and scanning can write only application metadata.
5. Select actions, then choose **Apply selected actions**. Each file is revalidated before execution. A late collision or changed source stops the batch and preserves the conflicting file. Generate a fresh preview to reconsider conflict names.
6. Use **Activity → Undo** to restore a completed move/rename or remove an unchanged copy. Undo does not overwrite an occupied original path. If an unfinished operation exists, **Recover unfinished operations** attempts a conservative rollback first.

**Watch for review** keeps the plan updated without applying it. Pause monitoring to freeze the plan before selecting actions. For unattended filing while the application is open, review the rules, tick the explicit automatic-organization acknowledgement in settings, then choose **Start automatic organization**. Edits pause monitoring and invalidate the displayed plan. Settings are saved explicitly or during orderly close. Closing waits for the current file to reach a journaled boundary and cancels remaining batch items; it does not leave a background service running.

## Safety and scope

Source and destination roots bound all operations. Output subtrees are excluded from input scanning; source roots wholly inside an output directory are rejected. Rename results and already processed file versions are recorded to prevent self-trigger loops. Undo does not immediately reapply the same file version. To intentionally reprocess an unchanged file, change its modification time/content or create a new source file; there is no destructive history reset button.

On Windows, source/output handles deny concurrent write/delete sharing while copying, hashing or removing a file. Open writers cause execution to fail safely. Data is copied to a uniquely named staging file beside the destination, flushed, verified, and published with Windows `MoveFileExW` **without** `REPLACE_EXISTING`. A move removes the original only after publication and a persisted journal phase. This also works across source/destination volumes. Undo uses the same staged publication rule. Filesystem ancestor components are checked for reparse points before traversal and again at execution boundaries.

This is a desktop tool for trusted, user-owned local folders. It is not a security sandbox against a malicious process swapping directory ancestors during an operation. Filesystem/hardware flush guarantees still apply. Only regular file contents are copied: timestamps, ACLs, alternate streams, extended attributes and other metadata are not preserved. Undo restores bytes, paths and names, not the original metadata. There is no automatic deletion rule, content analysis, cloud organization, tray mode, autostart registration or service installation.

Scans are bounded to 10,000 eligible files, 2,000 directories and 30,000 directory entries; each plan has at most 2,000 actions. Output-name inspection is bounded to 30,000 entries. Activity displays the latest 500 records; the durable journal and processed-file fingerprints remain on disk. Choose narrower folders for larger collections. Native watcher errors pause organization; a 60-second reconciliation scan while monitoring covers age conditions and coalesced notifications. A missing/inaccessible folder stops or reports the scan rather than silently following another location.

## Recovery and local data

Settings and journal data live under `%LOCALAPPDATA%\FileOrganizer\organizer.db`, with SQLite WAL/SHM files and an instance lock. A second instance using the same data directory is refused. There is no telemetry. The database contains paths, rules, timestamps and file hashes; protect it as local personal data. File contents stay local. Public update checks contact GitHub.

Unfinished records block new application until resolved. Recovery inspects recorded paths and hashes, restores a missing original if possible, then removes only matching output/staging files. Changed outputs, occupied restore paths with different content, missing files or partially written staging files are preserved and require manual inspection. See [recovery details](docs/ARCHITECTURE.md). Back up the entire data directory while the application is closed before making any manual database repair. There is intentionally no one-click way to discard an ambiguous journal record.

## Updates and future distribution

**Folders & settings → Updates** shows the installed version, **Check for updates**, and the persistent **Check for updates on startup** toggle. Startup checks run in a separate worker and show no no-update or transient-error dialog. Disabling the toggle and saving prevents startup requests on the next launch; manual checking remains available. A missing/unpublished repository, offline connection or rate limit is reported as an unavailable check, not as “up to date.”

The stable updater compares semantic versions, rejects drafts/prereleases and selects the canonical `FileOrganizer-vVERSION.exe` asset. **Download and install (restarts app)** requires explicit user action and a paused organizer. It displays progress, checks published size, SHA-256 (GitHub asset digest or `SHA256SUMS.txt`) and Windows x64 PE headers, finishes file work, exits, replaces the executable through a copied headless helper, then restarts. Configuration/journal data remain in LocalAppData. The install directory must be writable without elevation. A failed download leaves the installation unchanged; replacement has a rollback backup. Helper failures are saved in `update-error.txt` for the next launch. Interrupted helpers may leave staging/backup files for inspection.

No release/network download or updater installation has been exercised in this development session. Before publishing, complete the [manual validation checklist](docs/VALIDATION.md) and use the [release notes draft](docs/RELEASE_NOTES_v0.1.0.md). Builds are not code-signed. SHA-256 verifies integrity against release metadata; it is not a publisher signature.

## Development

The pure planner is separated from filesystem inspection, journal execution, watcher scheduling and GUI code. One bounded background worker serializes filesystem work; a separate worker serializes update requests. The GUI wakes for input or results and does not scan folders or hash files in paint callbacks. See [architecture](docs/ARCHITECTURE.md) and [validation record](docs/VALIDATION.md).

License: [MIT](LICENSE).
