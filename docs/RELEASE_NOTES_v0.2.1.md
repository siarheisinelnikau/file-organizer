# FileOrganizer 0.2.1 — local release draft

Folder setup now uses Windows folder choosers throughout the application. This patch also removes repeated destination entry and the separate pause step before applying a reviewed selection. Changes are relative to local 0.2.0; no public release is published here.

## Fixed

- Added Choose buttons to source folders, allowed destinations, the AI organized folder and manual-rule destinations. Add folder opens the chooser directly; Cancel leaves settings and previews intact.
- Selecting an AI/rule destination also approves that exact folder when it is outside an already allowed root. The interface explains the permission; manually pasted paths still use the existing approved roots.
- Added saved-destination menus and prefilled template destinations under the first saved root. In AI mode, the first chosen allowed destination initializes an empty organized-folder setting.
- Added bytes/KB/MB/GB size controls and common source-subfolder presets. Active advanced conditions expand in the rule editor.
- Kept optional pasted paths, including removal of surrounding quotes from Windows Copy as path when the input loses focus.
- Applying a reviewed selection pauses monitoring in the worker, checks the plan generation and applies only the selected operations. The editing UI remains busy until file work finishes.

## Verification and distribution

- 50 Rust tests passed with temporary files or injected chooser responses; one existing localhost HTTP test remains ignored in the restricted environment. Clippy, version/package checks and Windows x64 packaging passed.
- Real client-area renders cover populated/empty folder setup, AI settings, rule destinations and advanced size controls at standard/minimum widths. Capture mode disables native dialogs, workers, user data and network access.
- Native folder-dialog interaction, keyboard/accessibility and live window performance still need desktop validation. No normal GUI or computer-use tool was launched during this change.
- Standalone asset: `FileOrganizer-v0.2.1.exe`, with `SHA256SUMS.txt`. Settings and journal format remain compatible with preceding local builds.
