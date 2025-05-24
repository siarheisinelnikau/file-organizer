# How it works walkthrough

Open **How it works** beside the mode selector on Review or from the persistent sidebar link. The walkthrough uses three fictional files and stays separate from your folders, saved settings, monitoring and history. It never submits an AI request or executes a filesystem operation. AI categories/confidence are illustrated suggestions, not outputs from a live model.

- **Play / Pause** runs or freezes a finite 16-second illustration.
- Drag **Timeline** to compare any instant; scrubbing pauses playback. Numbered chapter buttons jump directly to Files, Settings, Review, Organize or Undo.
- **Reset** returns to the beginning. Playback stops at the end; it does not loop automatically.
- Switch between **Manual rules** and **AI Hub**. Rule examples use the production pure matching planner; AI examples use the production category/path planner and confidence recommendation filter with deterministic decisions.
- Change **Move / Copy**, the rule's file type or the AI recommendation threshold. These controls change the example only. The predicted selection and destination folders update without changing your real configuration.

The source side shows original files. Review shows proposed destinations. Organize moves/copies only the example selection, then holds its completed state briefly. Unmatched or unrecommended files stay at the source; they are never shown as completed or undone actions. Move undo illustrates restoration; copy undo illustrates removal of a verified copy while retaining the original.

Four automated regressions cover example matching/confidence, virtual-time playback and completion, selected-file transfer/undo states, and configuration isolation/navigation pause. The guide caches its example plan until its settings change. Repaint requests occur only during explicit playback (33 ms animation cadence); leaving the page, closing or making the host non-drawable pauses the guide. Static headless screenshots verify its client-area layout, not native input or desktop performance.
