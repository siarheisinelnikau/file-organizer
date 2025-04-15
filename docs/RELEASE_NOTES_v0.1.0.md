# FileOrganizer 0.1.0 — release draft

Initial Windows x64 implementation of rule-based file organization with review, explicit automation and recoverable undo. This draft describes local source capabilities, not a published or interactively validated release.

## Added

- Ordered visual rules for filename patterns, extensions, source subfolders, file size and modification age, with Downloads/archives, Photos and Documents templates.
- Move, copy and rename actions constrained to approved folders, with dry-run plans and per-file selection.
- Skip/keep-both collision handling and a no-overwrite publication boundary.
- Paused-by-default native monitoring, event debouncing, repeated stability observations and opt-in automatic application.
- Durable SQLite operation journal with restart-aware rollback recovery, content checks and per-operation undo.
- Preservation of original creation/access/modification timestamps and basic Windows file attributes, including read-only files, during copy/move/rename and undo/recovery. Older journal records remain readable.
- Persistent settings and single-data-directory instance locking.
- Future public-release update checks with a startup opt-out and user-initiated verified portable installation/restart.

## Improved

- Review-first navigation, guided setup, a focused rule editor, grouped folder/automation/update settings and clear save/apply actions.
- Shared light visual system, readable type sizes, consistent spacing and path truncation for narrow layouts.
- CPU rasterization with reactive Win32 GDI presentation replaces the OpenGL host. Hidden frames are gated at source; native idle/restore measurements remain pending.
- Finite isolated `--capture` mode for real application-generated PNGs without computer use, user data, file watches or update requests.

## Distribution

- Planned standalone executable: `FileOrganizer-v0.1.0.exe`, Windows x64 MSVC. Keep `SHA256SUMS.txt` beside the release asset.
- No account, AI service, model, driver, service or browser runtime is required.
- MIT-licensed source. Runtime dependencies remain subject to their own compatible licenses.

## Known limitations before publication

Local verification: 33 isolated-fixture/rendering tests, formatting, Clippy and optimized Windows x64 packaging passed. Static PE/import and SHA-256 inspection passed without entering normal GUI operation. See `docs/VALIDATION.md` for the precise artifact digest and test scope.

- Native GUI interaction and visible/minimized performance measurements remain pending. Real CPU-rendered client-area application/settings screenshots are included.
- Live public-release network checks and installation/restart/rollback have not been exercised. The canonical repository/release may not exist yet.
- Local-drive regular files only. No links/reparse traversal or network folders. ACLs, ownership, alternate streams and extended attributes are not preserved; storage/cloud flags requiring special copy mechanisms are rejected. Older journal entries cannot reconstruct original metadata that was never recorded.
- Large folder scans are bounded. No service, tray/autostart registration, deletion actions, content/EXIF conditions or journal-retention controls.
- Complete the checklist in `docs/VALIDATION.md` before describing this as a verified public release.
