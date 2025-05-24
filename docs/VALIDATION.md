# Validation record

Latest verification: 2026-11-01 (Europe/Minsk).

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
- `cargo test --offline --locked --all-targets -- --test-threads=2`: **54 passed, 0 failed, 1 ignored**: 30 filesystem/planning/update regressions, 10 AI integration regressions and 14 branding/folder-selection/guide/rendering/workflow regressions. The ignored HTTP transport fixture is described below. No test creates a native window.
- `git diff --check`: passed.
- `scripts/package.ps1` with `CARGO_NET_OFFLINE=true`: passed. Optimized `x86_64-pc-windows-msvc` build completed with two jobs and cached dependencies.
- `python scripts/inspect_executable.py dist/FileOrganizer-v0.3.0.exe`: passed. AMD64, PE32+, Windows GUI subsystem; import table contains Windows system DLLs and no external MSVC runtime DLL. This checks binary structure, not actual graphics/desktop behavior.

Packaged artifact after the walkthrough and native caption configuration changes: `dist/FileOrganizer-v0.3.0.exe`, **12,267,008 bytes**. `dist/SHA256SUMS.txt` matches the independently calculated inspection digest:

```text
21db593ef307b40a62fc7f7a0f2a95935afcb00c9bec6737013832fe19cedaf8  FileOrganizer-v0.3.0.exe
```

The normal GUI entry point was not launched. The release EXE was invoked only through its bounded `--capture` branch; all captures exited successfully. Distribution output is ignored by Git and remains available locally. Dependency/font notice audit remains required before publication.

Coverage includes compound/ordered conditions, invalid Windows device-name templates, in-plan and late collisions, move/copy/rename with restart/undo, changed-source/output refusal, occupied restore paths, approved roots, rejection of network/alternate-stream paths before access, pending publication/staging/undo recovery, conservative handling of partial stages, nested-output exclusion, atomic no-overwrite publication, stability resets, actual native watcher delivery after fixture creation, repeated-event suppression, orderly shutdown with queued notifications, persistent settings and pure update metadata/checksum/PE-architecture validation. Update tests do not make HTTP requests or run the helper.

Metadata regressions verify all three Windows file timestamps using independently set fixture values and compare actual filesystem metadata. Copy/move/rename and undo are exercised with Normal, Read-only, Hidden, System, Archive, Temporary and Not content indexed combinations, including cleared Archive. Tests reopen the journal, edit output metadata before undo, preserve later source edits on copy undo, replay interrupted moves with an existing/missing original, clean read-only staging, read an old record with no metadata field, retain edited read-only output on refused undo, retain the only complete copy after an injected metadata-restoration error, and reuse a recently deleted destination name with a different creation time. All fixtures are local; no volume compression, filesystem policy or system setting is changed.

Fixture location matters in this managed sandbox: the redirected system Temp folder permits creating files but rejects even ordinary renames and hard links with `Access denied`. The initial run exposed that environment restriction. Tests therefore create uniquely named temporary folders under this repository's ignored `target/test-fixtures/`, where no-overwrite publication and normal cleanup work. All 30 filesystem integration tests passed there; no permissions/elevation were requested and no real user files were touched.

## Version and native icon verification

- Windows resource compilation embeds FileVersion/ProductVersion 0.3.0.0 and OriginalFilename `FileOrganizer-v0.3.0.exe`. Independent Windows FileVersionInfo reading reports the expected product/name/version strings.
- `inspect_executable.py --expect-version 0.3.0 --icon assets/FileOrganizer.ico` verified every embedded icon payload against the checked-in ICO: 16, 20, 24, 32, 40, 48, 64, 128 and 256 pixels. Actual decoded frames were inspected on light/dark backgrounds. The native icon constructor passed without creating a window.
- `python scripts/test_versioning.py`: two isolated regressions passed, exercising patch/minor/major increments, dependency preservation and invalid-lockfile refusal before manifest writes.
- `scripts/test_package_policy.ps1`: artificial-byte fixtures confirmed identical repeat packaging succeeds, changed same-version bytes and version regression fail, a new patch is accepted, and the existing artifact remains intact.
- No Explorer icon-cache reset, shortcut/pinning change or native GUI launch was performed. Windows taskbar/window icon appearance still requires a future desktop observation; the missing resources and missing assignments are repaired and checked at the code/resource boundaries.

## Walkthrough and caption verification

Four guide regressions passed: rule/AI example plans and recommendation thresholds, virtual-time progression/automatic completion and pause, selected-file transfer/undo state including unapplied proposals, and isolated UI rendering without Config edits plus navigation/hide pause hooks. Example plans are cached until settings change; playback alone schedules a 33 ms repaint, and the host's drawable guard pauses it when hidden/minimized. Rendering remains the same CPU/GDI path. Normal native idle/hidden/animation measurements remain unverified.

The real executable's finite capture branch renders nineteen fixtures, including guide AI/rule review, move, move undo, copy and copy undo. Standard, minimum and 125% captures were decoded/inspected. Layout fixes widened the scrub track, kept the compact walkthrough within the minimum viewport, and removed an unsupported arrow glyph from a caption. This establishes static client-area evidence; the guide's virtual-clock tests separately cover progression and stopping.

The supplied user image showed nearly black caption text on a dark native title bar. Source inspection found a system-themed native frame paired with the explicitly light client area. Window creation now requests Light and sets Windows caption background/text from CANVAS/INK through the pinned winit API. Windows 11 supports those explicit color attributes; older systems retain the light-theme request. These are per-window settings, not a system theme change. **The native caption fix has not been visually verified**, because no native window was launched and headless captures omit non-client chrome.

## Folder-setup usability verification

Five folder-selection regressions passed without opening a dialog: injected cancellation and single-flight behavior, configuration/preview invalidation after acceptance, stale-result rejection, exact output scope and first-destination seeding, duplicate normalization and neighboring-path boundaries, local selected-root/starting-directory checks and unsupported/missing path refusal. A separate template regression checks derived destination subfolders. All filesystem inspection used repository-local fixtures.

The existing native-watch fixture now applies its reviewed selection directly while monitoring. It verifies that the actor reports monitoring paused and produces exactly one journaled copy. Generation checks and revalidation still gate execution. Ordinary Pause also emits its Busy completion event so the editing UI does not remain disabled.

The release executable rendered populated and empty folder settings, AI destination selection, rule destinations and advanced byte-unit controls without starting a window or the native picker. Standard/minimum/125% images were inspected. The picker is disabled in capture mode; native dialog/parent-window behavior, navigation and keyboard interaction remain explicitly unverified. No real user folder was selected or changed.

## AI integration verification

Ten enabled AI tests passed with deterministic completion clients and repository-local files: Universal request/data minimization, opt-in text reads without metadata changes, unsafe categories and extension changes, malformed/truncated/tool-call/unknown-ID responses, missing classifications, batching/run limits, persistent cache reuse, collision reservation, output exclusion, legacy settings, real move/copy/undo metadata comparisons, confidence-gated automatic execution and cancellation of an in-flight job without applying/caching stale results. No real model, hub credential store, user files or external AI API was used.

An attempted bounded localhost transport fixture failed before connection with Windows socket error 10013 (PermissionDenied), including after disabling environment proxy use. The retained transport test is explicitly ignored locally; run `cargo test --locked --test ai http_transport -- --ignored` on a network-enabled host. CI includes that explicit command for a future Windows runner. HTTP wire behavior, live Hub authentication and actual model quality are not claimed as verified. No elevation or network-policy change was requested. The token storage code compiled, but development did not read/write Windows Credential Manager.

## Design and renderer verification

- Reviewed real executable-generated Review, Rules, Folders, Automation, Updates, Activity, Welcome, Recovery, AI Review, AI Settings, AI Working, empty Folder Setup, expanded Rule Details and six guide client-area states.
- Captures used 1160 × 820 at 100%, 960 × 680 at 100%, and 1200 × 850 at 125% (960 × 680 logical units), with fixed demonstration paths/data. Each invocation uses 57 finite frames, a 20-second internal deadline and a 30-second external subprocess timeout.
- Final release runs returned exit code 0. PNGs were decoded and inspected. The standard capture is embedded in README; `docs/screenshots/capture.json` identifies the backend, dimensions, frame count, complete-capture timings and absence of windows/workers/network.
- Corrected horizontal-layout inheritance in the rule editor, inactive theme styles, preview width, header action space, narrow layout and overflow. Long form sections scroll at the minimum size; long file/path labels truncate with full-value tooltips.
- Added regressions for hidden dirty-state retention, ordered partial texture updates/geometry clipping, and the rename-only workflow without an output root. An initial synthetic raw renderer fixture aborted; the final regression uses an initialized egui frame and a 128 × 128 surface, within the guarded renderer size limits.
- Registry fetching failed with a crates.io initialization/DNS-thread error; no escalation or authentication was attempted. The compatible cached CPU rasterizer plus an explicit version adapter allows fully offline builds.
- Static imports contain no OpenGL/Direct3D or external MSVC runtime DLLs. The CPU raster path executed during captures; the GDI presentation path was inspected in softbuffer's pinned Win32 source. **Native presentation, visible/hidden frame counts, idle CPU/GPU load and restore behavior have not been measured. The full runtime rendering gate is incomplete.** `--render-log` provides opt-in counters for that future diagnostic; no normal window was opened for this task.

## Deferred manual checks

- On disposable files, connect to AI Hub with a user-supplied token and a running local model. Verify `/v1/models`, Universal routing, JSON compatibility, semantic category quality, model switching with Reclassify, HTTP status/timeout/redirect handling and credential save/remove/address scoping. Test connection is not proof that a model is ready.

- Run the packaged executable on a Windows x64 desktop; check Review, Rules, Activity, Settings and How it works, scrolling, editing, selection, resize, 100/150/200% DPI, close during a large copy, and second-instance behavior.
- Native OS chrome/dialogs, keyboard traversal, clipboard, IME and actual window activation still need interactive validation. Real client-area screenshots are now in `docs/screenshots/` and README.
- Measure release idle/active/minimized CPU, frame/input responsiveness and memory with representative folder sizes. Verify that idle rendering sleeps. No performance measurements are claimed yet.
- Exercise move/copy/rename, collision choices, open downloads/writers, watcher bursts, age thresholds, pause/resume and manual undo on disposable NTFS volumes, including different source/output drives.
- Check metadata preservation across different volumes and FAT/exFAT timestamp resolutions, attribute-write ACL denials, real metadata/delete failures, and access-time policies. The automated Windows fixtures verify the current local filesystem; they do not simulate every storage driver or power loss.
- Verify real symlink/junction/reparse fixtures and attempts to swap ancestors, read-only/denied-access paths, long Unicode paths, volume removal, disk-full behavior and storage interruption. No reparse traversal is intended, but these OS scenarios need additional dedicated fixtures/manual checks.
- Abruptly end a disposable application instance at each journal phase and inspect recovery. Automated tests inject equivalent persisted states; they do not simulate power loss or filesystem flush failures.
- After the public repository/release exists, check GitHub 404/offline/rate limits/TLS failures, startup opt-out, numeric stable selection, progress, corrupt/truncated assets, unwritable install directories, pending-file shutdown, helper timeout, replacement, restart, backup rollback and settings preservation. Never point update tests at a real working installation.
- Audit dependency license notices and add appropriate notices to the first public distribution. Decide on code signing before publication.

The CI workflow checks/builds and uploads a workflow artifact when the repository is later published; it does not create a GitHub release. No publication is performed in this session.
