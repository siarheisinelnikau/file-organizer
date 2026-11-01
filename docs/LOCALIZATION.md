# Interface languages and themes

FileOrganizer 0.4.1 supports English, Russian, French, German, Spanish and Italian. Settings always exposes the language selector with native names and Light / Dark buttons above its tabs. Both preferences save automatically and update without restarting.

`Config.language` and `Config.theme` live in the existing SQLite configuration JSON. Serde defaults preserve older configurations, including folders, ordered rules and startup-update opt-out. Unknown future language/theme values fall back to English/light instead of making the database unreadable. Appearance-only saves update those two fields in the worker's saved configuration; they do not commit unsaved rule edits, replace the current plan, stop a watcher or cancel AI work. Orderly shutdown saves the latest loaded UI snapshot even when older preference saves or acknowledgements are queued. Closing before settings load never writes startup defaults over the existing configuration.

## Catalog

`locales/messages.psv` is a UTF-8, pipe-delimited catalog. Each row contains the complete English message key, followed by Russian, French, German, Spanish and Italian translations. Empty values and embedded `|` characters are not permitted. Keep Rust placeholder tokens exactly unchanged, including names and format specifications. The catalog is embedded in the executable and parsed once; changing language does not read the disk or load network resources.

Look up only authored UI strings through `tr`. Explicitly marked formatted messages use their English template plus Rust-formatted values, so paths, version numbers and model names remain intact. Saved user rule names, folder names, filenames, model-generated explanations and detailed backend diagnostics are data and must not be translated. Native file-dialog titles use the selected app language; Windows supplies the dialog's own system labels.

When adding UI text, supply all five translations and use the existing lookup at the display boundary. Standard worker/update results remain in canonical English in memory and are translated when displayed so a language change also updates an existing result. Technical diagnostics can retain their original language.

## Palette and rendering

Light and dark palettes share the same typography, spacing and semantic roles: canvas, surface, primary/muted text, accent, selection, border, warning and error. Custom widgets, the walkthrough painter, input fields, standard popups and the CPU renderer use the active roles. Native window decorations follow the selected egui theme through the existing viewport-command integration, rather than retaining forced light caption colors.

Appearance updates request one useful repaint. Normal rendering still uses the existing CPU rasterizer, softbuffer's Windows GDI presentation, retained bitmap, dirty flag and `Wait` / `WaitUntil` scheduling. This change adds no periodic render timer. Walkthrough playback keeps its existing finite, visible-only animation behavior.

## Verification limits

Catalog completeness and placeholder preservation, old-settings migration, future-value fallback, persistence, rapid preference changes on shutdown, early-close preservation and monitoring/plan preservation have automated fixture checks. Normal GUI launches and screenshot capture are prohibited by the repository instructions for this session, so version 0.4.1 visual layout, native titlebar behavior, language switching through actual input, and visible/hidden rendering counters remain unverified. Existing README images are real version 0.3.1 English/light evidence and are labeled accordingly.
