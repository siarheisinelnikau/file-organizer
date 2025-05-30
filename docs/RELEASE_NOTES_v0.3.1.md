# FileOrganizer 0.3.1 — local changes

- Delegate portable update discovery, download verification and installation to the shared Rust Auto Updator core, preserving startup opt-out and the user-initiated flow.
- Preserve cleanup compatibility with helpers from existing releases.
- Reuse the last CPU bitmap on OS expose requests instead of rebuilding unchanged UI; report raster frames and presentations separately.
- Validate executable icon resources without opening the application.

No publication or online installation was performed. Native interactive responsiveness and visible/hidden runtime measurements remain separate acceptance checks. There is no tray surface in this application.
