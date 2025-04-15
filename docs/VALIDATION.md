# Validation record

Development date: 2026-10-31 (Europe/Minsk).

Computer use, desktop capture, normal GUI interaction, real user-folder watches, updater download/installation, external AI APIs, system changes and remote Git operations have not been performed. The design follow-up authorizes isolated application-generated visual review: the executable's finite headless branch renders only fixture UI and exits, without creating a native window, opening the user database or starting file/update workers. Compilation and these images do not certify native interaction or idle performance.

## Automated checks

Verified with Rust 1.96.1 / Cargo 1.96.1 on Windows x64 using cached dependencies. Commands run from this repository with the local target directory, at most two Cargo jobs and two test threads:

```powershell
cargo fmt --all --check
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --offline --locked --all-targets -- --test-threads=2
.\scripts\package.ps1
```

Results:

- `cargo fmt --all --check`: passed.
- `cargo clippy --offline --locked --all-targets -- -D warnings`: passed, no code diagnostics. Cargo reported environment-level incremental-cache hard-link warnings and used copies instead.
- `cargo test --offline --locked --all-targets -- --test-threads=2`: **33 passed, 0 failed, 0 ignored**: 30 filesystem/planning/update regressions and 3 rendering/workflow regressions. No test creates a native window.
- `git diff --check`: passed.
- `scripts/package.ps1` with `CARGO_NET_OFFLINE=true`: passed. Optimized `x86_64-pc-windows-msvc` build completed with two jobs and cached dependencies.
- `python scripts/inspect_executable.py dist/FileOrganizer-v0.1.0.exe`: passed. AMD64, PE32+, Windows GUI subsystem; import table contains Windows system DLLs and no external MSVC runtime DLL. This checks binary structure, not actual graphics/desktop behavior.

Packaged artifact after the metadata-preservation change: `dist/FileOrganizer-v0.1.0.exe`, **11,587,584 bytes**. `dist/SHA256SUMS.txt` matches the independently calculated inspection digest:

```text
fed6c74bdc8a2bddc965fcf56929b5699bb3b3e75c80f6f6d91e959580565c9d  FileOrganizer-v0.1.0.exe
```

The normal GUI entry point was not launched. The release EXE was invoked only through its bounded `--capture` branch; all captures exited successfully. Distribution output is ignored by Git and remains available locally. Dependency/font notice audit remains required before publication.

Coverage includes compound/ordered conditions, invalid Windows device-name templates, in-plan and late collisions, move/copy/rename with restart/undo, changed-source/output refusal, occupied restore paths, approved roots, rejection of network/alternate-stream paths before access, pending publication/staging/undo recovery, conservative handling of partial stages, nested-output exclusion, atomic no-overwrite publication, stability resets, actual native watcher delivery after fixture creation, repeated-event suppression, orderly shutdown with queued notifications, persistent settings and pure update metadata/checksum/PE-architecture validation. Update tests do not make HTTP requests or run the helper.

Metadata regressions verify all three Windows file timestamps using independently set fixture values and compare actual filesystem metadata. Copy/move/rename and undo are exercised with Normal, Read-only, Hidden, System, Archive, Temporary and Not content indexed combinations, including cleared Archive. Tests reopen the journal, edit output metadata before undo, preserve later source edits on copy undo, replay interrupted moves with an existing/missing original, clean read-only staging, read an old record with no metadata field, retain edited read-only output on refused undo, retain the only complete copy after an injected metadata-restoration error, and reuse a recently deleted destination name with a different creation time. All fixtures are local; no volume compression, filesystem policy or system setting is changed.

Fixture location matters in this managed sandbox: the redirected system Temp folder permits creating files but rejects even ordinary renames and hard links with `Access denied`. The initial run exposed that environment restriction. Tests therefore create uniquely named temporary folders under this repository's ignored `target/test-fixtures/`, where no-overwrite publication and normal cleanup work. All 30 filesystem integration tests passed there; no permissions/elevation were requested and no real user files were touched.

## Design and renderer verification

- Reviewed real executable-generated Review, Rules, Folders, Automation, Updates, Activity, Welcome and Recovery client areas.
- Captures used 1160 × 820 at 100%, 960 × 680 at 100%, and 1200 × 850 at 125% (960 × 680 logical units), with fixed demonstration paths/data. Each invocation uses 24 finite frames, a 20-second internal deadline and a 30-second external subprocess timeout.
- Final release runs returned exit code 0. PNGs were decoded and inspected. The standard capture is embedded in README; `docs/screenshots/capture.json` identifies the backend, dimensions, frame count, complete-capture timings and absence of windows/workers/network.
- Corrected horizontal-layout inheritance in the rule editor, inactive theme styles, preview width, header action space, narrow layout and overflow. Long form sections scroll at the minimum size; long file/path labels truncate with full-value tooltips.
- Added regressions for hidden dirty-state retention, ordered partial texture updates/geometry clipping, and the rename-only workflow without an output root. An initial synthetic raw renderer fixture aborted; the final regression uses an initialized egui frame and a 128 × 128 surface, within the guarded renderer size limits.
- Registry fetching failed with a crates.io initialization/DNS-thread error; no escalation or authentication was attempted. The compatible cached CPU rasterizer plus an explicit version adapter allows fully offline builds.
- Static imports contain no OpenGL/Direct3D or external MSVC runtime DLLs. The CPU raster path executed during captures; the GDI presentation path was inspected in softbuffer's pinned Win32 source. **Native presentation, visible/hidden frame counts, idle CPU/GPU load and restore behavior have not been measured. The full runtime rendering gate is incomplete.** `--render-log` provides opt-in counters for that future diagnostic; no normal window was opened for this task.

## Deferred manual checks

- Run the packaged executable on a Windows x64 desktop; check all four screens, scrolling, editing, selection, resize, 100/150/200% DPI, close during a large copy, and second-instance behavior.
- Native OS chrome/dialogs, keyboard traversal, clipboard, IME and actual window activation still need interactive validation. Real client-area screenshots are now in `docs/screenshots/` and README.
- Measure release idle/active/minimized CPU, frame/input responsiveness and memory with representative folder sizes. Verify that idle rendering sleeps. No performance measurements are claimed yet.
- Exercise move/copy/rename, collision choices, open downloads/writers, watcher bursts, age thresholds, pause/resume and manual undo on disposable NTFS volumes, including different source/output drives.
- Check metadata preservation across different volumes and FAT/exFAT timestamp resolutions, attribute-write ACL denials, real metadata/delete failures, and access-time policies. The automated Windows fixtures verify the current local filesystem; they do not simulate every storage driver or power loss.
- Verify real symlink/junction/reparse fixtures and attempts to swap ancestors, read-only/denied-access paths, long Unicode paths, volume removal, disk-full behavior and storage interruption. No reparse traversal is intended, but these OS scenarios need additional dedicated fixtures/manual checks.
- Abruptly end a disposable application instance at each journal phase and inspect recovery. Automated tests inject equivalent persisted states; they do not simulate power loss or filesystem flush failures.
- After the public repository/release exists, check GitHub 404/offline/rate limits/TLS failures, startup opt-out, numeric stable selection, progress, corrupt/truncated assets, unwritable install directories, pending-file shutdown, helper timeout, replacement, restart, backup rollback and settings preservation. Never point update tests at a real working installation.
- Audit dependency license notices and add appropriate notices to the first public distribution. Decide on code signing before publication.

The CI workflow checks/builds and uploads a workflow artifact when the repository is later published; it does not create a GitHub release. No publication is performed in this session.
