# Versioned builds

Local commits and application versions serve different purposes. Commit each completed change, but advance the application version whenever changed executable contents are handed to the user. Use a patch increment for fixes and a minor increment for new capabilities. Documentation-only or intermediate implementation commits do not each require a new version. Repackaging identical bytes may keep its existing version.

`Cargo.toml` is the version source for the UI, updater, Windows FileVersion/ProductVersion resources and `FileOrganizer-vVERSION.exe` asset name. Update the package and its lockfile entry together:

```powershell
python scripts/bump_version.py patch
# For a new capability instead:
python scripts/bump_version.py minor
```

Then write `docs/RELEASE_NOTES_vVERSION.md`, update current README/validation references, run the checks and package. The helper changes only this package's manifest version and lockfile entry; dependency versions remain intact. It does not commit or publish anything. Stable numeric components must fit Windows' 16-bit version fields.

`scripts/package.ps1` inspects the built PE before copying it: x64 architecture, GUI subsystem, no external MSVC runtime, matching numeric file/product versions, and embedded icon frames identical to the checked-in ICO. It rejects different bytes under an existing versioned asset path and rejects versions older than another packaged local build. Previous local executables are retained. `SHA256SUMS.txt` describes the current packaged asset.

The 0.2.0 transition corrects the earlier practice of delivering different local binaries as 0.1.0. It also adds an Explorer executable icon, explicitly assigned native window/taskbar icons, and Windows file properties. Older locally saved 0.1.0 copies still contain their original resources; use the new versioned executable. No published release was modified.

Artwork is original project code in `scripts/generate_icon.py`, distributed under the repository's MIT license. The checked-in `assets/FileOrganizer.ico` has 16, 20, 24, 32, 40, 48, 64, 128 and 256-pixel frames. `assets/icon-256.png` supplies the native window/taskbar image. Regeneration requires Pillow; ordinary application use does not require Python or Pillow. Packaging checks require Python 3.11+ and the existing Windows SDK/MSVC tools.
