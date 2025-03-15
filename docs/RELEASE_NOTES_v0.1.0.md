# FileOrganizer 0.1.0 — release draft

Initial Windows x64 implementation of rule-based file organization with review, explicit automation and recoverable undo. This draft describes local source capabilities, not a published or interactively validated release.

## Added

- Ordered visual rules for filename patterns, extensions, source subfolders, file size and modification age, with Downloads/archives, Photos and Documents templates.
- Move, copy and rename actions constrained to approved folders, with dry-run plans and per-file selection.
- Skip/keep-both collision handling and a no-overwrite publication boundary.
- Paused-by-default native monitoring, event debouncing, repeated stability observations and opt-in automatic application.
- Durable SQLite operation journal with restart-aware rollback recovery, content checks and per-operation undo.
- Persistent settings and single-data-directory instance locking.
- Future public-release update checks with a startup opt-out and user-initiated verified portable installation/restart.

## Distribution

- Planned standalone executable: `FileOrganizer-v0.1.0.exe`, Windows x64 MSVC. Keep `SHA256SUMS.txt` beside the release asset.
- No account, AI service, model, driver, service or browser runtime is required.
- MIT-licensed source. Runtime dependencies remain subject to their own compatible licenses.

## Known limitations before publication

Local verification: 22 isolated-fixture tests, formatting, Clippy and optimized Windows x64 packaging passed. Static PE/import and SHA-256 inspection passed without launching the executable. See `docs/VALIDATION.md` for the precise artifact digest and test scope.

- GUI interaction, genuine application/settings screenshots and visible/minimized performance measurements remain pending.
- Live public-release network checks and installation/restart/rollback have not been exercised. The canonical repository/release may not exist yet.
- Local-drive regular files only. No links/reparse traversal or network folders. File metadata/alternate streams are not preserved.
- Large folder scans are bounded. No service, tray/autostart registration, deletion actions, content/EXIF conditions or journal-retention controls.
- Complete the checklist in `docs/VALIDATION.md` before describing this as a verified public release.
