# FileOrganizer 0.2.0 — local release draft

This build gives the AI-assisted organizer a distinct version and fixes its missing native application icon. Earlier development builds were repeatedly delivered under 0.1.0; no public release is published or changed here.

## Fixed

- Embedded the original FileOrganizer icon in the executable at nine sizes, from 16 to 256 pixels, for Windows Explorer and DPI scaling.
- Assigned the matching icon explicitly to the native window and Windows taskbar.
- Added Windows file/product version information and the correct versioned original filename. Settings and the updater report 0.2.0 from the same Cargo package version.
- Packaging now rejects a changed executable under an existing versioned asset name and checks the embedded icon and file/product versions before copying the build.

## Development

- Added a version increment helper and documented patch/minor increments for finished user-facing builds.
- Retained AI Hub Universal classification, confidence-based review/automation, metadata preservation and journal-based undo from the preceding local implementation.
- Verified 44 Rust tests, two version-helper regressions, fixture-based packaging-policy checks, Clippy, PE resources and a bounded headless screen capture. The localhost HTTP transport test remains explicitly ignored because this managed environment denies client sockets.

## Distribution and remaining checks

- Standalone Windows x64 asset: `FileOrganizer-v0.2.0.exe`, with `SHA256SUMS.txt`.
- Windows resource data and actual icon images were inspected without opening a native window. Explorer/taskbar appearance in a live desktop session, native input/performance, live AI Hub/model requests and update installation remain unverified. See [validation](VALIDATION.md).
