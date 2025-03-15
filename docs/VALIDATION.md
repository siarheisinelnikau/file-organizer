# Validation record

Development date: 2026-10-31 (Europe/Minsk).

This session is code-only and unattended. No GUI application, computer-use tool, screenshot capture, real user-folder watch, updater download/installation, external AI API, driver/service installation or remote Git operation is authorized. All functional filesystem checks use temporary synthetic fixtures. Compilation is not a GUI smoke test.

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
- `cargo test --offline --locked --all-targets -- --test-threads=2`: **22 passed, 0 failed, 0 ignored**. No test invokes the GUI entry point.
- `git diff --check`: passed.
- `scripts/package.ps1` with `CARGO_NET_OFFLINE=true`: passed. Optimized `x86_64-pc-windows-msvc` build completed with two jobs and cached dependencies.
- `python scripts/inspect_executable.py dist/FileOrganizer-v0.1.0.exe`: passed. AMD64, PE32+, Windows GUI subsystem; import table contains Windows system DLLs and no external MSVC runtime DLL. This checks binary structure, not actual graphics/desktop behavior.

Packaged artifact: `dist/FileOrganizer-v0.1.0.exe`, **11,385,856 bytes**. `dist/SHA256SUMS.txt` matches the independently calculated inspection digest:

```text
75ad4355ee77c26c8427376b32c062844b2342abc5de252aea6056db3784d9e2  FileOrganizer-v0.1.0.exe
```

The EXE was never launched. Distribution output is ignored by Git and remains available locally. A read-only Cargo metadata inventory resolved 156 packages for the Windows graph (including development dependencies); all declare licenses. The source uses MIT; dependency/font notices still need the publication audit below.

Coverage includes compound/ordered conditions, invalid Windows device-name templates, in-plan and late collisions, move/copy/rename with restart/undo, changed-source/output refusal, occupied restore paths, approved roots, rejection of network/alternate-stream paths before access, pending publication/staging/undo recovery, conservative handling of partial stages, nested-output exclusion, atomic no-overwrite publication, stability resets, actual native watcher delivery after fixture creation, repeated-event suppression, orderly shutdown with queued notifications, persistent settings and pure update metadata/checksum/PE-architecture validation. Update tests do not make HTTP requests or run the helper.

Fixture location matters in this managed sandbox: the redirected system Temp folder permits creating files but rejects even ordinary renames and hard links with `Access denied`. The initial run exposed that environment restriction. Tests therefore create uniquely named temporary folders under this repository's ignored `target/test-fixtures/`, where no-overwrite publication and normal cleanup work. All 22 final tests passed there; no permissions/elevation were requested and no real user files were touched.

## Deferred manual checks

- Run the packaged executable on a Windows x64 desktop; check all four screens, scrolling, editing, selection, resize, 100/150/200% DPI, close during a large copy, and second-instance behavior.
- Capture real Rules/Review and Folders & settings screenshots, put them under `docs/screenshots/`, and embed accurate repository-relative images in README.
- Measure release idle/active/minimized CPU, frame/input responsiveness and memory with representative folder sizes. Verify that idle rendering sleeps. No performance measurements are claimed yet.
- Exercise move/copy/rename, collision choices, open downloads/writers, watcher bursts, age thresholds, pause/resume and manual undo on disposable NTFS volumes, including different source/output drives.
- Verify real symlink/junction/reparse fixtures and attempts to swap ancestors, read-only/denied-access paths, long Unicode paths, volume removal, disk-full behavior and storage interruption. No reparse traversal is intended, but these OS scenarios need additional dedicated fixtures/manual checks.
- Abruptly end a disposable application instance at each journal phase and inspect recovery. Automated tests inject equivalent persisted states; they do not simulate power loss or filesystem flush failures.
- After the public repository/release exists, check GitHub 404/offline/rate limits/TLS failures, startup opt-out, numeric stable selection, progress, corrupt/truncated assets, unwritable install directories, pending-file shutdown, helper timeout, replacement, restart, backup rollback and settings preservation. Never point update tests at a real working installation.
- Audit dependency license notices and add appropriate notices to the first public distribution. Decide on code signing before publication.

The CI workflow checks/builds and uploads a workflow artifact when the repository is later published; it does not create a GitHub release. No publication is performed in this session.
