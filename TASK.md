# FileOrganizer

Repository: `file-organizer`
Priority: 3 of 15 (user-requested launch order)
Product: FileOrganizer
Purpose: Automatic file organization with review and undo
Inspiration: https://www.filejuggler.com/

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Build an understandable rule-based organizer for selected folders. Conditions should cover filename/pattern, extension/type, size, age and source folder; actions should support move/copy/rename into configurable folders. Include ready-to-use Downloads/photos/documents templates, a visual rule editor, rule ordering, a dry-run preview, conflict decisions, activity log, enable/pause controls and a recoverable operation journal.

Separate a pure planning engine from execution and filesystem watchers. Debounce events, wait for files still being written, avoid self-trigger loops and unsafe symlink/reparse traversal, constrain actions to approved roots and never silently overwrite. Automatic destructive deletion is not a default. Test plans, collisions, repeated events, partial failures and undo entirely on temporary fixtures. Do not watch, move or rename the user's real files during development.

## Packaging and publication target

Windows standalone asset: `FileOrganizer-vVERSION.exe` (for the initial release, `FileOrganizer-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/file-organizer (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

