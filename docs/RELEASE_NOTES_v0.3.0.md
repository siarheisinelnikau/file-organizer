# FileOrganizer 0.3.0 — local release draft

Adds an interactive visual walkthrough and configures the native title bar for readable contrast. Changes are relative to local 0.2.1; no public release is published here.

## Added

- **How it works** entry points on Review and in the sidebar.
- A finite animated walkthrough of source files, settings, review, organization and undo, with Play/Pause, Reset, chapter jumps and a draggable timeline.
- Separate rule and AI examples, Move/Copy comparison, selectable file types and an adjustable example confidence threshold. Plans reuse the pure production planners; no real files, settings, journal or AI service are changed.
- Brief result holds between motion and undo so the before/after state can be read. Unselected proposals remain for review instead of being labeled as undone.

## Fixed

- The native window now explicitly requests the light theme used by the client area. Windows 11 caption background/text colors use the shared near-white canvas and dark text palette; older Windows versions retain the light-theme request. This addresses the reported dark-title-bar/dark-text mismatch at the window configuration boundary.

## Verification and distribution

- 54 Rust tests passed; the existing localhost HTTP test remains ignored because the development sandbox denies client sockets. Clippy, formatting, PE icon/version inspection and Windows x64 packaging passed.
- Real CPU-rendered client-area fixtures cover rule/AI review, transfer, copy and undo states at standard/minimum sizes and 125% scaling. These images do not capture the native title bar.
- Native caption contrast, OS dialogs, keyboard/accessibility, animation pacing and visible/hidden performance still need desktop observation. No normal GUI or computer-use tool was launched.
- Standalone Windows x64 asset: `FileOrganizer-v0.3.0.exe`, with `SHA256SUMS.txt`.
