# FileOrganizer 0.4.0

FileOrganizer now offers six interface languages and a persistent dark theme while preserving the review-before-apply organization workflow.

## Added

- Choose English, Russian, French, German, Spanish or Italian from Settings using native language names. Navigation, review, rule editing, AI preferences, the interactive walkthrough, folder chooser titles, update actions and standard status/recovery messages use the chosen language.
- Switch between Light and Dark above the settings tabs. The coherent palette covers the main interface, rule editor, walkthrough, inputs, selections, popups and CPU-renderer background.

## Improved

- Language and theme apply without restarting and save automatically. Changing appearance preserves the current reviewed plan, selected operations, monitoring state and unsaved rule edits.
- Existing settings migrate with English/light defaults while keeping folders, rules, recovery history and the startup-update opt-out. Unknown future language/theme values fall back safely.
- Translated messages preserve file paths, version numbers, model names and formatting placeholders. User content and detailed platform/network diagnostics remain intact.
- Native window decorations follow the chosen theme rather than retaining fixed light caption colors.

## Verification and platform

Windows x64 standalone asset: `FileOrganizer-v0.4.0.exe`, with SHA-256 checksums. Settings and the undo journal remain in the existing LocalAppData directory. The update check/startup opt-out and explicit in-app installation flow are preserved; no updater installation was run during development.

Automated catalog, configuration and isolated file-workflow checks are recorded in `docs/VALIDATION.md`. The application was not launched for this change: updated localization/dark-theme screenshots, native input and idle/hidden runtime measurements remain pending. README screenshots are labeled real version 0.3.1 English/light evidence. Compilation and fixture tests do not establish those native runtime behaviors.
