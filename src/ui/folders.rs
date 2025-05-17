use super::*;
use anyhow::{Result, ensure};
use file_organizer::filesystem as disk;
use std::{
    path::Path,
    sync::{Arc, mpsc},
    thread,
};

#[derive(Clone, Debug)]
pub(super) enum Target {
    Source(Option<usize>),
    Allowed(Option<usize>),
    Ai,
    Rule(String),
}
impl Target {
    fn title(&self) -> &'static str {
        match self {
            Self::Source(_) => "Choose a folder to organize",
            Self::Allowed(_) => "Choose an allowed destination",
            Self::Ai => "Choose the AI organized folder",
            Self::Rule(_) => "Choose the rule destination",
        }
    }
}

type Selection = Result<Option<String>>;
type Picker = dyn Fn(&str, &str, Option<&winit::window::Window>) -> Selection + Send + Sync;
struct Pending {
    target: Target,
    before: Config,
    result: mpsc::Receiver<Selection>,
}

pub(super) struct FolderPicker {
    enabled: bool,
    parent: Option<Arc<winit::window::Window>>,
    pending: Option<Pending>,
    picker: Arc<Picker>,
}
impl FolderPicker {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            parent: None,
            pending: None,
            picker: Arc::new(native_folder),
        }
    }
    pub fn set_parent(&mut self, window: Arc<winit::window::Window>) {
        self.parent = Some(window);
    }
    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }
    fn start(
        &mut self,
        target: Target,
        before: Config,
        initial: String,
        ctx: egui::Context,
    ) -> Result<()> {
        ensure!(
            self.enabled,
            "Folder selection is disabled in isolated capture mode"
        );
        ensure!(self.pending.is_none(), "A folder chooser is already open");
        let (sender, result) = mpsc::sync_channel(1);
        let parent = self.parent.clone();
        let picker = self.picker.clone();
        let title = target.title();
        thread::Builder::new()
            .name("folder-chooser".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    picker(title, &initial, parent.as_deref())
                }))
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "The folder chooser stopped unexpectedly. You can still paste a path."
                    ))
                });
                let _ = sender.send(result);
                ctx.request_repaint();
            })?;
        self.pending = Some(Pending {
            target,
            before,
            result,
        });
        Ok(())
    }
    fn poll(&mut self) -> Option<(Target, Config, Selection)> {
        let result = match self.pending.as_ref()?.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("The folder chooser closed unexpectedly"))
            }
        };
        let pending = self.pending.take().unwrap();
        Some((pending.target, pending.before, result))
    }
}

#[cfg(windows)]
fn native_folder(title: &str, initial: &str, parent: Option<&winit::window::Window>) -> Selection {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    // Filesystem inspection and native COM UI run only on this explicit-action thread.
    if let Some(directory) = initial_directory(initial) {
        dialog = dialog.set_directory(directory);
    }
    if let Some(parent) = parent {
        dialog = dialog.set_parent(parent);
    }
    dialog
        .pick_folder()
        .map(|path| checked_folder(&path))
        .transpose()
}
#[cfg(not(windows))]
fn native_folder(_: &str, _: &str, _: Option<&winit::window::Window>) -> Selection {
    anyhow::bail!("Native folder selection is currently supported on Windows")
}
fn initial_directory(value: &str) -> Option<PathBuf> {
    let path = Path::new(value.trim());
    disk::check_path(path).ok()?;
    path.ancestors().find(|p| p.is_dir()).map(Path::to_owned)
}
fn checked_folder(path: &Path) -> Result<String> {
    let root = disk::root(
        path.to_str()
            .ok_or_else(|| anyhow::anyhow!("The selected folder name is not valid Unicode"))?,
    )?;
    let value = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("The selected folder name is not valid Unicode"))?;
    Ok(value.strip_prefix(r"\\?\").unwrap_or(value).to_owned())
}
fn key(value: &str) -> String {
    path_key(Path::new(value.trim().trim_start_matches(r"\\?\")))
        .trim_end_matches('/')
        .to_owned()
}
fn contains(parent: &str, child: &str) -> bool {
    let parent = key(parent);
    let child = key(child);
    !parent.is_empty() && (child == parent || child.starts_with(&(parent + "/")))
}
fn add_unique(roots: &mut Vec<String>, value: &str) {
    if roots.iter().any(|p| key(p) == key(value)) {
        return;
    }
    if let Some(blank) = roots.iter_mut().find(|p| p.trim().is_empty()) {
        *blank = value.into();
    } else {
        roots.push(value.into());
    }
}
fn allow_output(config: &mut Config, value: &str) {
    if !config.destination_roots.iter().any(|p| contains(p, value)) {
        add_unique(&mut config.destination_roots, value);
    }
}
fn adopt(config: &mut Config, target: Target, value: &str) -> Result<()> {
    match target {
        Target::Source(index) => {
            let outputs: Vec<&str> = if config.mode == PlanningMode::AiHub {
                vec![config.ai.destination.as_str()]
            } else {
                config
                    .rules
                    .iter()
                    .filter(|r| r.enabled && r.action != Action::Rename)
                    .map(|r| r.destination.as_str())
                    .collect()
            };
            ensure!(
                !outputs
                    .iter()
                    .any(|p| !p.trim().is_empty() && contains(p, value)),
                "This source is inside an output folder. Choose a separate source folder."
            );
            if let Some(index) = index {
                ensure!(
                    !config
                        .source_roots
                        .iter()
                        .enumerate()
                        .any(|(i, p)| i != index && key(p) == key(value)),
                    "This source folder is already listed"
                );
                *config
                    .source_roots
                    .get_mut(index)
                    .ok_or_else(|| anyhow::anyhow!("The source row was removed"))? = value.into();
            } else {
                add_unique(&mut config.source_roots, value);
            }
        }
        Target::Allowed(index) => {
            if config.mode == PlanningMode::AiHub && config.ai.destination.trim().is_empty() {
                ensure!(
                    !config.source_roots.iter().any(|p| contains(value, p)),
                    "The organized folder would contain a source folder. Choose a subfolder or a separate destination."
                );
            }
            if let Some(index) = index {
                ensure!(
                    !config
                        .destination_roots
                        .iter()
                        .enumerate()
                        .any(|(i, p)| i != index && key(p) == key(value)),
                    "This destination is already listed"
                );
                *config
                    .destination_roots
                    .get_mut(index)
                    .ok_or_else(|| anyhow::anyhow!("The destination row was removed"))? =
                    value.into();
            } else {
                add_unique(&mut config.destination_roots, value);
            }
            if config.mode == PlanningMode::AiHub && config.ai.destination.trim().is_empty() {
                config.ai.destination = value.into();
            }
        }
        Target::Ai => {
            ensure!(
                !config.source_roots.iter().any(|p| contains(value, p)),
                "The organized folder would contain a source folder. Choose a subfolder or a separate destination."
            );
            config.ai.destination = value.into();
            allow_output(config, value);
        }
        Target::Rule(id) => {
            let rule = config
                .rules
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or_else(|| anyhow::anyhow!("The rule was removed"))?;
            ensure!(
                !config.source_roots.iter().any(|p| contains(value, p)),
                "The output would contain a source folder. Choose a subfolder or a separate destination."
            );
            rule.destination = value.into();
            allow_output(config, value);
        }
    }
    Ok(())
}

impl Organizer {
    pub fn set_dialog_parent(&mut self, window: Arc<winit::window::Window>) {
        self.folders.set_parent(window);
    }
    pub(super) fn choose_folder(&mut self, ctx: &egui::Context, target: Target, initial: String) {
        if self.watching || self.ai_busy {
            self.send(Command::Pause);
            self.message(false, "Monitoring paused while you choose a folder.".into());
        }
        if let Err(error) = self
            .folders
            .start(target, self.config.clone(), initial, ctx.clone())
        {
            self.message(true, error.to_string());
        }
    }
    pub(super) fn poll_folders(&mut self) {
        let Some((target, before, result)) = self.folders.poll() else {
            return;
        };
        if self.shutting_down {
            return;
        }
        match result {
            Ok(None) => {}
            Err(error) => self.message(true, error.to_string()),
            Ok(Some(value)) => {
                if before != self.config {
                    self.message(
                        true,
                        "Settings changed while the chooser was open. Choose the folder again."
                            .into(),
                    );
                    return;
                }
                let mut config = self.config.clone();
                match adopt(&mut config, target, &value) {
                    Err(error) => self.message(true, error.to_string()),
                    Ok(()) if config != self.config => {
                        self.config = config;
                        self.configuration_changed();
                    }
                    Ok(()) => {}
                }
            }
        }
    }
}

pub(super) fn folder_field(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    value: &mut String,
    saved: &[String],
) -> bool {
    ui.strong(label);
    let browse = ui
        .horizontal(|ui| {
            let width = (ui.available_width() - 108.0).max(100.0);
            edit_path(ui, value, width);
            ui.button("Choose…")
                .on_hover_text("Open the Windows folder chooser")
                .clicked()
        })
        .inner;
    if saved.iter().any(|p| !p.trim().is_empty()) {
        egui::ComboBox::from_id_salt(id)
            .selected_text("Use a saved destination")
            .width(210.0)
            .show_ui(ui, |ui| {
                for path in saved.iter().filter(|p| !p.trim().is_empty()) {
                    if ui
                        .add(
                            egui::Button::selectable(
                                key(path) == key(value),
                                egui::RichText::new(path),
                            )
                            .wrap(),
                        )
                        .clicked()
                    {
                        *value = path.clone();
                    }
                }
            });
    }
    browse
}

pub(super) fn edit_path(ui: &mut egui::Ui, value: &mut String, width: f32) {
    if ui
        .add(
            egui::TextEdit::singleline(value)
                .hint_text("Choose a folder or paste a path")
                .desired_width(width),
        )
        .lost_focus()
    {
        let trimmed = value.trim();
        *value = trimmed
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(trimmed)
            .to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_updates_preview_but_cancel_and_stale_results_leave_settings_intact() {
        let ctx = egui::Context::default();
        let mut app = Organizer::new(&ctx, PathBuf::from(r"C:\Fixture\App"), false);
        app.plan.scanned = 7;
        let finish = |app: &mut Organizer, before: Config, selection: Selection| {
            let (sender, result) = mpsc::sync_channel(1);
            sender.send(selection).unwrap();
            app.folders.pending = Some(Pending {
                target: Target::Ai,
                before,
                result,
            });
            app.poll_folders();
        };
        let before = app.config.clone();
        finish(&mut app, before.clone(), Ok(None));
        assert_eq!(app.config, before);
        assert!(!app.dirty);
        assert_eq!(app.plan.scanned, 7);
        app.config.ai.goal = "Changed while choosing".into();
        finish(&mut app, before, Ok(Some(r"C:\Sorted".into())));
        assert!(app.config.ai.destination.is_empty());
        assert!(app.messages.last().unwrap().0);
        let before = app.config.clone();
        finish(&mut app, before, Ok(Some(r"C:\Sorted".into())));
        assert_eq!(app.config.ai.destination, r"C:\Sorted");
        assert_eq!(app.config.destination_roots, vec![r"C:\Sorted"]);
        assert!(app.dirty);
        assert_eq!(app.plan.scanned, 0);
    }
    #[test]
    fn picking_outputs_grants_only_the_chosen_scope_and_seeds_ai_setup() {
        let mut config = Config {
            mode: PlanningMode::AiHub,
            source_roots: vec![r"C:\Inbox".into()],
            ..Config::default()
        };
        adopt(&mut config, Target::Allowed(None), r"C:\Sorted").unwrap();
        assert_eq!(config.ai.destination, r"C:\Sorted");
        adopt(&mut config, Target::Ai, r"C:\Sorted\Projects").unwrap();
        assert_eq!(config.destination_roots, vec![r"C:\Sorted"]);
        adopt(&mut config, Target::Ai, r"D:\Filed").unwrap();
        assert_eq!(config.destination_roots, vec![r"C:\Sorted", r"D:\Filed"]);
        assert!(adopt(&mut config, Target::Ai, r"C:\").is_err());
    }
    #[test]
    fn selections_deduplicate_and_do_not_misidentify_neighboring_paths() {
        let mut config = Config::default();
        adopt(&mut config, Target::Source(None), r"C:\Inbox").unwrap();
        adopt(&mut config, Target::Source(None), r"\\?\c:\inbox\").unwrap();
        assert_eq!(config.source_roots.len(), 1);
        assert!(!contains(r"C:\Sort", r"C:\Sorted"));
        let rule = Rule::default();
        let id = rule.id.clone();
        config.rules.push(rule);
        adopt(&mut config, Target::Rule(id), r"C:\Sorted").unwrap();
        assert_eq!(config.rules[0].destination, r"C:\Sorted");
        assert_eq!(config.destination_roots, vec![r"C:\Sorted"]);
        assert!(adopt(&mut config, Target::Rule("missing".into()), r"C:\Elsewhere").is_err());
    }
    #[test]
    fn injected_cancel_does_not_change_config_and_picker_is_single_flight() {
        let ctx = egui::Context::default();
        let mut picker = FolderPicker::new(true);
        let (release, wait) = mpsc::sync_channel(1);
        let wait = std::sync::Mutex::new(wait);
        picker.picker = Arc::new(move |_, _, _| {
            wait.lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))?;
            Ok(None)
        });
        let config = Config::default();
        picker
            .start(Target::Ai, config.clone(), String::new(), ctx.clone())
            .unwrap();
        assert!(
            picker
                .start(Target::Ai, config.clone(), String::new(), ctx)
                .is_err()
        );
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (_, before, result) = loop {
            if let Some(result) = picker.poll() {
                break result;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(before, config);
        assert!(result.unwrap().is_none());
        assert!(!picker.is_open());
    }
    #[test]
    fn checked_selection_and_starting_directory_use_artificial_local_folders() {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
        std::fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::tempdir_in(parent).unwrap();
        let value = checked_folder(temp.path()).unwrap();
        assert_eq!(
            disk::root(&value).unwrap(),
            std::fs::canonicalize(temp.path()).unwrap()
        );
        assert_eq!(
            initial_directory(&temp.path().join("not-created/child").to_string_lossy()),
            Some(temp.path().to_owned())
        );
        assert!(checked_folder(Path::new(r"\\server\share")).is_err());
        assert!(checked_folder(&temp.path().join("missing")).is_err());
    }
}
