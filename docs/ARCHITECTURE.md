# Architecture and recovery

## Boundaries

- `model.rs`: serializable rules, settings, facts and operations.
- `engine.rs`: pure first-match planning and collision reservation. No filesystem access.
- `filesystem.rs`: bounded scans, canonical approved paths, link rejection, Windows file locking, hashing and no-overwrite publication.
- `metadata.rs`: serializable original timestamps/basic attributes, handle-based restoration and suppression of reader-induced access-time changes.
- `journal.rs`: SQLite settings, durable operation phases, suppression fingerprints, execution and rollback.
- `worker.rs`: one serial filesystem owner, bounded command/result channels, native notifications, settling/debouncing and reconciliation deadlines.
- `updater.rs`: release parsing, integrity checks and the portable helper protocol.
- `ui.rs` and `ui/views.rs`: application state, the Review-first interface, rule editor, activity and grouped settings. No directory scans/network requests in frame callbacks.
- `design.rs`: shared light palette, typography, spacing and primary actions.
- `renderer.rs`: the CPU raster adapter shared by the desktop host and headless captures.
- `host.rs`: winit events, native input/clipboard/IME, software GDI presentation, repaint deadlines and shutdown.
- `ui/capture.rs`: bounded isolated screen fixtures; parsed before workers or user-data startup.

The former eframe/OpenGL host has been replaced by `egui 0.37 → mesh/texture adapter → egui_software_backend 0.0.2 CPU → softbuffer 0.4.8 Win32 GDI`. The available rasterizer consumes egui 0.33 structures, so the explicit adapter copies only geometry, premultiplied colors, ordered texture updates and clip coordinates. UI construction remains egui 0.37. GPU callbacks are rejected. The adapter has a partial-texture-update regression test. Viewports below 64 pixels or above 16 million pixels are refused; normal UI minimum size is 960 × 680 logical units.

The host uses `Wait`/`WaitUntil`, drops stale repaint requests, and guards frames with dirty/drawable state. Hidden/minimized/occluded windows retain dirty data without layout/raster/present. Worker events still drain and orderly shutdown remains available while hidden. Renderer failures stop further painting before orderly close. Finite offscreen captures execute this same UI and CPU rasterizer without constructing any window. Source inspection and captures do not constitute native idle/hidden/performance verification; the runtime gate remains incomplete pending an authorized native diagnostic run.

When paused, the filesystem worker blocks on a command receiver. While monitoring it sleeps until an event/debounce, settling retry or 60-second reconciliation deadline. Native watcher callbacks use nonblocking bounded enqueue; a full queue coalesces notifications and the reconciliation scan covers missed callbacks. Monitoring is process-lifetime only and starts paused on every launch. A separate serial update worker keeps HTTP and installation preparation off the GUI thread.

## Execution protocol

1. Validate source/destination inside approved local roots and reject links/reparse ancestors. Open the source with read/attribute-write access and, for moves, DELETE access. On Windows sharing permits readers but rejects concurrent data writers and deleters. Disable access-time updates on this reader handle, capture original timestamps/attributes, reject unsupported storage/cloud flags, recheck size/mtime and calculate SHA-256.
2. Insert `Prepared` into SQLite with operation, original root approvals, metadata snapshot, staging path and digest. SQLite uses WAL and `synchronous=FULL`. The optional JSON metadata field permits reading older records without a schema rewrite.
3. Create missing destination subdirectories within approved output roots. Copy bytes to a random `.fo-UUID.tmp` file with `create_new`, flush/fsync, verify its digest and revalidate the source. Restore the snapshot on the staging file and persist `Staged`.
4. Publish beside the destination using Windows `MoveFileExW` with write-through and without replace-existing. A destination appearing since preview is an error, never an overwrite. Persist `Published`, then reapply metadata at the final name to avoid Windows creation-time tunneling.
5. For moves/renames, remove the original through its locked file handle. Record source/output stamps as processed, then persist `Done`.

The copy-then-remove implementation supports different source/output volumes. It temporarily needs room for a full copy. It preserves bytes plus the Windows creation/access/write times and settable basic attributes. Unsupported storage/cloud state is rejected at source validation. Created directories are retained after rollback/undo so unrelated data in them is never removed.

A failure leaves its last persisted phase and error. Batches stop on the first error; earlier completed operations remain individually undoable. Pending phases block further application. Startup reads the journal and displays unfinished records; recovery never runs automatically.

## Undo and interrupted-operation recovery

Normal undo preflights an empty restore path for moves/renames, writes `UndoPending`, then validates the output hash. It restores through a new staging file if needed, publishes without overwrite, remembers the restored stamp to prevent immediate reorganization, removes the matching output, and persists `Undone`. Copy undo removes only the matching output. If an error happens after `UndoPending`, recovery can finish the same rollback idempotently.

Restored files receive the journaled original timestamps and attributes before staging publication and again at the final name. This is independent of later metadata edits to an output. Recovery also reapplies the snapshot when the original already exists with the expected bytes (including interruption after clearing Read-only but before deleting the original). Read-only is cleared through the locked file handle only at deletion; a failed delete attempts to restore the previous metadata. The output is not deleted if copying or metadata restoration fails. Legacy records without a snapshot preserve the available output metadata when recreating a missing original and leave an existing original's metadata unchanged.

Timestamps use exact Windows FILETIME integer values, not floating-point dates. `SetFileTime` with the documented all-ones access-time value disables read-induced access updates for organizer handles. `SetFileInformationByHandle(FileBasicInfo)` restores creation/access/write times and basic flags; NTFS change time remains managed by the filesystem. See Microsoft's [SetFileTime documentation](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfiletime) and [FILE_BASIC_INFO reference](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_basic_info). Filesystem timestamp precision still applies; sources with unset/out-of-range timestamps are refused. Windows metadata behavior is covered by fixture tests; other operating systems are not release targets.

**Recover unfinished operations** uses original recorded approvals, so editing the current rules does not destroy the recovery context. It revalidates all paths and hashes. If the original exists with the expected bytes, it can remove the matching duplicate output; if the original is absent, it first restores bytes from the output. It then removes a complete matching staging file and marks `Recovered`.

Ambiguous state is deliberately preserved. For example, a power interruption during copying can leave a partial `.fo-UUID.tmp` or `.fo-UUID-undo.tmp`. A hash mismatch keeps that file and the journal pending. With the application closed, inspect the original/output/staging paths shown by the record/error, back up the data folder, and preserve any unique bytes. After manually moving an ambiguous partial stage to a safe inspection folder, retry recovery. A missing original and output or a changed output requires restoring expected bytes from a backup before recovery can finish. Do not discard the journal to bypass a conflict. There is no promise of recovery from drive loss or storage that violates flush guarantees.

## Updater protocol

The installed app checks the canonical public GitHub stable release API without authentication. Semantic versions, official repository asset URLs, platform naming, maximum size and SHA-256 are validated. The helper is a copy of the current executable in the app's update staging directory; the downloaded binary is not executed before verification. After the user initiates installation, the filesystem worker closes the database and releases its instance lock. The helper waits for the old PID, copies the verified asset beside the executable, renames the old executable to a unique rollback backup, installs and restarts. On a rename/restart failure it attempts to restore the backup. A successful restart cleans only validated backup/staging paths. Failure details are retained in `update-error.txt`. No updater function, network request or replacement is invoked by automated tests.

## Limits

Approved roots and path checks prevent accidental out-of-scope operations, not deliberate directory-swap attacks by another local process. Network/UNC paths are unsupported. NTFS alternate streams, ACLs, ownership, extended attributes and storage/cloud features are not copied. Supported basic flags are Read-only, Hidden, System, Archive, Temporary, Not content indexed and Normal (no flags). Metadata writes require attribute-write permission, and timestamp resolution depends on the filesystem. Snapshot suppression uses normalized path, byte size and modification timestamp, not file identity: an external tool replacing bytes while preserving both size and timestamp may require an explicit timestamp change to become eligible. A prepared plan is not an immutable byte-content snapshot; execution reads the current exclusively locked bytes after size/mtime checks. Unsupported/locked files stop the current batch. The app has no service/tray/autostart integration and no journal-retention UI in 0.1.0.
