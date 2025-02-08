use file_organizer::{
    engine, filesystem as disk,
    journal::{Entry, Journal},
    model::*,
    updater::{self, Asset, Release},
    worker::{Command, Event, Stability, Worker},
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    source: PathBuf,
    output: PathBuf,
    database: PathBuf,
    config: Config,
}

impl Fixture {
    fn new() -> Self {
        // The managed Windows sandbox grants rename rights inside the repository;
        // its redirected system Temp directory does not grant equivalent rights.
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-fixtures");
        fs::create_dir_all(&fixtures).unwrap();
        let temp = tempfile::tempdir_in(fixtures).unwrap();
        let source = temp.path().join("inbox");
        let output = temp.path().join("filed");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&output).unwrap();
        let source = fs::canonicalize(source).unwrap();
        let output = fs::canonicalize(output).unwrap();
        let rule = Rule {
            name: "Documents".into(),
            min_age_minutes: 0,
            destination: output.to_string_lossy().into_owned(),
            ..Rule::default()
        };
        let config = Config {
            source_roots: vec![source.to_string_lossy().into_owned()],
            destination_roots: vec![output.to_string_lossy().into_owned()],
            rules: vec![rule],
            settle_seconds: 2,
            startup_updates: false,
            ..Config::default()
        };
        Self {
            database: temp.path().join("test.db"),
            _temp: temp,
            source,
            output,
            config,
        }
    }

    fn operation(&self, action: Action) -> Operation {
        let source = self.source.join("report.txt");
        fs::write(&source, b"original fixture bytes").unwrap();
        Operation {
            rule_id: "fixture".into(),
            rule_name: "Fixture".into(),
            stamp: disk::stamp(&fs::metadata(&source).unwrap()).unwrap(),
            source,
            destination: self.output.join("report.txt"),
            action,
        }
    }

    fn pending(&self, op: Operation, phase: &str, stage: Option<&[u8]>) -> Entry {
        let entry = Entry {
            id: "crash-fixture".into(),
            sha256: format!("{:x}", sha2::Sha256::digest(b"original fixture bytes")),
            source_roots: self.config.source_roots.clone(),
            destination_roots: self.config.destination_roots.clone(),
            stage: self.output.join(".fo-crash-fixture.tmp"),
            operation: op,
            phase: phase.into(),
            error: String::new(),
            created: String::new(),
        };
        if let Some(bytes) = stage {
            fs::write(&entry.stage, bytes).unwrap();
        }
        let connection = rusqlite::Connection::open(&self.database).unwrap();
        connection
            .execute(
                "INSERT INTO operations(id,json,phase) VALUES(?1,?2,?3)",
                rusqlite::params![entry.id, serde_json::to_string(&entry).unwrap(), phase],
            )
            .unwrap();
        entry
    }
}

use sha2::Digest;

fn fact(root: &Path, name: &str, size: u64, age_minutes: u64) -> FileFact {
    FileFact {
        path: root.join(name),
        source_root: root.to_owned(),
        stamp: Stamp {
            bytes: size,
            modified_ns: disk::now_ns() - age_minutes as u128 * 60_000_000_000,
        },
    }
}

#[test]
fn conditions_are_combined_and_first_enabled_rule_wins() {
    let f = Fixture::new();
    let mut config = f.config.clone();
    config.rules[0].pattern = "invoice-*.?df".into();
    config.rules[0].extensions = "pdf, DOCX".into();
    config.rules[0].min_bytes = 10;
    config.rules[0].max_bytes = 30;
    config.rules[0].min_age_minutes = 5;
    config.rules[0].folder_pattern = "bills/*".into();
    let mut second = config.rules[0].clone();
    second.id = "second".into();
    second.name = "Second".into();
    config.rules.push(second);
    let facts = vec![
        fact(&f.source, "bills/2026/INVOICE-one.PDF", 20, 10),
        fact(&f.source, "other/invoice-two.pdf", 20, 10),
        fact(&f.source, "bills/2026/invoice-big.pdf", 31, 10),
        fact(&f.source, "bills/2026/invoice-new.pdf", 20, 0),
        fact(&f.source, "bills/2026/note.pdf", 20, 10),
    ];
    let plan = engine::plan(&config, &facts, &HashSet::new(), disk::now_ns());
    assert_eq!(plan.operations.len(), 1);
    assert_eq!(plan.operations[0].rule_name, "Documents");
    config.rules[0].enabled = false;
    assert_eq!(
        engine::plan(&config, &facts, &HashSet::new(), disk::now_ns()).operations[0].rule_name,
        "Second"
    );
}

#[test]
fn collisions_reserve_names_across_the_whole_plan() {
    let f = Fixture::new();
    let mut config = f.config.clone();
    config.rules[0].conflict = Conflict::KeepBoth;
    let facts = vec![
        fact(&f.source, "one/report.pdf", 8, 10),
        fact(&f.source, "two/report.pdf", 8, 10),
    ];
    let occupied = HashSet::from([path_key(&f.output.join("report.pdf"))]);
    let plan = engine::plan(&config, &facts, &occupied, disk::now_ns());
    assert_eq!(
        plan.operations[0].destination,
        f.output.join("report (1).pdf")
    );
    assert_eq!(
        plan.operations[1].destination,
        f.output.join("report (2).pdf")
    );
    config.rules[0].conflict = Conflict::Skip;
    assert!(
        engine::plan(&config, &facts, &occupied, disk::now_ns())
            .operations
            .is_empty()
    );
    assert_eq!(fs::read_dir(&f.output).unwrap().count(), 0);
}

#[test]
fn templates_cannot_escape_or_create_windows_device_names() {
    let f = Fixture::new();
    let facts = vec![fact(&f.source, "safe.pdf", 10, 5)];
    for template in [
        "../escape.pdf",
        "CON.txt",
        "NUL .txt",
        "COM¹.txt",
        "a:b.pdf",
        "trailing.",
        "{unknown}",
        "a\\b",
    ] {
        let mut config = f.config.clone();
        config.rules[0].name_template = template.into();
        assert!(
            engine::plan(&config, &facts, &HashSet::new(), disk::now_ns())
                .operations
                .is_empty(),
            "{template}"
        );
    }
    let mut config = f.config.clone();
    config.rules[0].name_template = "filed-{stem}.{ext}".into();
    assert_eq!(
        engine::plan(&config, &facts, &HashSet::new(), disk::now_ns()).operations[0].destination,
        f.output.join("filed-safe.pdf")
    );
    assert!(engine::matches("a?*Z", "AbcZ"));
    assert!(!engine::matches("*.pdf", "file.pdf.exe"));
}

#[test]
fn move_copy_and_rename_round_trip_with_restart() {
    for action in [Action::Move, Action::Copy, Action::Rename] {
        let f = Fixture::new();
        let mut op = f.operation(action);
        if action == Action::Rename {
            op.destination = f.source.join("renamed.txt");
        }
        let journal = Journal::open(&f.database).unwrap();
        journal.execute(&op, &f.config).unwrap();
        assert_eq!(op.source.exists(), action == Action::Copy);
        assert_eq!(
            fs::read(&op.destination).unwrap(),
            b"original fixture bytes"
        );
        let id = journal.entries(false).unwrap()[0].id.clone();
        drop(journal);
        let reopened = Journal::open(&f.database).unwrap();
        reopened.undo(&id).unwrap();
        assert_eq!(fs::read(&op.source).unwrap(), b"original fixture bytes");
        assert!(!op.destination.exists());
        assert_eq!(reopened.entries(false).unwrap()[0].phase, "Undone");
        let scanned = disk::scan(&f.config).unwrap();
        let keys = reopened.suppressed().unwrap();
        assert!(
            scanned
                .files
                .iter()
                .all(|file| Journal::is_suppressed(file, &keys))
        );
    }
}

#[test]
fn changed_source_and_late_collision_are_rejected_without_mutation() {
    let f = Fixture::new();
    let op = f.operation(Action::Move);
    let journal = Journal::open(&f.database).unwrap();
    fs::write(&op.source, b"changed after preview").unwrap();
    assert!(journal.execute(&op, &f.config).is_err());
    assert!(!op.destination.exists());
    let op = f.operation(Action::Move);
    fs::write(&op.destination, b"late arrival").unwrap();
    assert!(journal.execute(&op, &f.config).is_err());
    assert_eq!(fs::read(&op.destination).unwrap(), b"late arrival");
    assert!(op.source.exists());
    assert!(journal.entries(false).unwrap().is_empty());
}

#[test]
fn undo_preserves_modified_outputs() {
    let f = Fixture::new();
    let op = f.operation(Action::Move);
    let journal = Journal::open(&f.database).unwrap();
    journal.execute(&op, &f.config).unwrap();
    fs::write(&op.destination, b"user edited this").unwrap();
    let id = journal.entries(false).unwrap()[0].id.clone();
    assert!(journal.undo(&id).is_err());
    assert!(!op.source.exists());
    assert_eq!(fs::read(&op.destination).unwrap(), b"user edited this");
    assert_eq!(journal.entries(true).unwrap().len(), 1);
}

#[test]
fn undo_never_overwrites_an_occupied_restore_path() {
    let f = Fixture::new();
    let op = f.operation(Action::Move);
    let journal = Journal::open(&f.database).unwrap();
    journal.execute(&op, &f.config).unwrap();
    fs::write(&op.source, b"another file").unwrap();
    let id = journal.entries(false).unwrap()[0].id.clone();
    assert!(journal.undo(&id).is_err());
    assert_eq!(fs::read(&op.source).unwrap(), b"another file");
    assert!(op.destination.exists());
    assert!(journal.entries(true).unwrap().is_empty());
}

#[test]
fn approved_roots_and_parent_components_are_enforced() {
    let f = Fixture::new();
    let mut op = f.operation(Action::Move);
    let journal = Journal::open(&f.database).unwrap();
    op.destination = f._temp.path().join("outside.txt");
    assert!(journal.execute(&op, &f.config).is_err());
    op.destination = f.output.join("..\\escape.txt");
    assert!(journal.execute(&op, &f.config).is_err());
    assert!(op.source.exists());
    assert!(journal.entries(false).unwrap().is_empty());
}

#[test]
fn pending_publication_recovers_with_or_without_original() {
    for original_exists in [true, false] {
        let f = Fixture::new();
        let op = f.operation(Action::Move);
        let journal = Journal::open(&f.database).unwrap();
        fs::copy(&op.source, &op.destination).unwrap();
        if !original_exists {
            fs::remove_file(&op.source).unwrap();
        }
        f.pending(op.clone(), "Published", None);
        assert!(journal.execute(&op, &f.config).is_err());
        let messages = journal.recover();
        assert!(messages[0].starts_with("Recovered"), "{messages:?}");
        assert_eq!(fs::read(&op.source).unwrap(), b"original fixture bytes");
        assert!(!op.destination.exists());
        assert!(journal.entries(true).unwrap().is_empty());
    }
}

#[test]
fn staging_and_partial_staging_have_conservative_recovery() {
    for complete in [true, false] {
        let f = Fixture::new();
        let op = f.operation(Action::Move);
        let journal = Journal::open(&f.database).unwrap();
        let entry = f.pending(
            op.clone(),
            "Prepared",
            Some(if complete {
                b"original fixture bytes"
            } else {
                b"partial"
            }),
        );
        let messages = journal.recover();
        assert_eq!(fs::read(&op.source).unwrap(), b"original fixture bytes");
        assert!(!op.destination.exists());
        assert_eq!(entry.stage.exists(), !complete);
        assert_eq!(
            journal.entries(true).unwrap().is_empty(),
            complete,
            "{messages:?}"
        );
    }
}

#[test]
fn interrupted_undo_can_be_recovered_idempotently() {
    let f = Fixture::new();
    let op = f.operation(Action::Move);
    let journal = Journal::open(&f.database).unwrap();
    fs::copy(&op.source, &op.destination).unwrap();
    f.pending(op.clone(), "UndoPending", None);
    assert!(journal.recover()[0].starts_with("Recovered"));
    assert!(journal.recover().is_empty());
    assert!(op.source.exists());
    assert!(!op.destination.exists());
}

#[test]
fn nested_output_is_excluded_and_temporary_downloads_are_ignored() {
    let mut f = Fixture::new();
    let nested = f.source.join("organized");
    fs::create_dir(&nested).unwrap();
    f.config.destination_roots = vec![f.source.to_string_lossy().into_owned()];
    f.config.rules[0].destination = nested.to_string_lossy().into_owned();
    fs::write(nested.join("done.pdf"), b"output").unwrap();
    fs::write(f.source.join("file.part"), b"in progress").unwrap();
    fs::write(f.source.join("ready.pdf"), b"ready").unwrap();
    let scan = disk::scan(&f.config).unwrap();
    assert_eq!(scan.files.len(), 1);
    assert_eq!(scan.files[0].path.file_name().unwrap(), "ready.pdf");
}

#[test]
fn publication_has_an_atomic_no_overwrite_boundary() {
    let f = Fixture::new();
    let stage = f.output.join(".fo-test.tmp");
    let target = f.output.join("existing.txt");
    fs::write(&stage, b"new").unwrap();
    fs::write(&target, b"existing").unwrap();
    assert!(disk::publish(&stage, &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"existing");
    assert_eq!(fs::read(&stage).unwrap(), b"new");
}

#[test]
fn stability_resets_after_changes_and_repeated_events_do_not_duplicate_files() {
    let f = Fixture::new();
    let file = fact(&f.source, "stable.txt", 10, 5);
    let start = Instant::now();
    let mut stability = Stability::new();
    let interval = Duration::from_secs(3);
    assert_eq!(stability.ready(vec![file.clone()], start, interval).1, 1);
    assert_eq!(
        stability
            .ready(vec![file.clone()], start + interval, interval)
            .0
            .len(),
        1
    );
    let mut changed = file;
    changed.stamp.bytes += 1;
    assert_eq!(
        stability
            .ready(vec![changed.clone()], start + interval, interval)
            .1,
        1
    );
    assert_eq!(
        stability
            .ready(vec![changed], start + interval * 2, interval)
            .0
            .len(),
        1
    );
    stability.ready(Vec::new(), start + interval * 3, interval);
}

#[test]
fn configuration_persists_opt_out_and_order() {
    let f = Fixture::new();
    let mut config = f.config.clone();
    config.rules.push(Rule::template("Photos"));
    let journal = Journal::open(&f.database).unwrap();
    journal.save_config(&config).unwrap();
    drop(journal);
    let loaded = Journal::open(&f.database).unwrap().config().unwrap();
    assert!(!loaded.startup_updates);
    assert_eq!(loaded.rules.len(), 2);
    assert_eq!(loaded.rules[1].name, "Photos");
}

fn release(tag: &str) -> Release {
    let name = format!("FileOrganizer-{}.exe", tag);
    Release {
        tag_name: tag.into(),
        draft: false,
        prerelease: false,
        assets: vec![Asset {
            browser_download_url: format!("{}/releases/download/{tag}/{name}", updater::REPOSITORY),
            name,
            size: 1024,
            digest: None,
        }],
    }
}

#[test]
fn updater_selects_stable_versions_numerically_and_rejects_foreign_assets() {
    assert!(
        updater::select(release("v0.10.0"), "0.9.0")
            .unwrap()
            .is_some()
    );
    assert!(
        updater::select(release("v0.1.0"), "0.1.0")
            .unwrap()
            .is_none()
    );
    let mut draft = release("v1.0.0");
    draft.draft = true;
    assert!(updater::select(draft, "0.1.0").unwrap().is_none());
    let mut pre = release("v1.0.0-beta.1");
    pre.prerelease = true;
    assert!(updater::select(pre, "0.1.0").unwrap().is_none());
    let mut foreign = release("v1.0.0");
    foreign.assets[0].browser_download_url = "https://example.com/app.exe".into();
    assert!(updater::select(foreign, "0.1.0").is_err());
    let mut wrong = release("v1.0.0");
    wrong.assets[0].name = "FileOrganizer-v1.0.0-arm64.exe".into();
    assert!(updater::select(wrong, "0.1.0").is_err());
}

#[test]
fn checksum_parser_requires_an_exact_unambiguous_entry() {
    let hash = "a".repeat(64);
    let name = "FileOrganizer-v1.0.0.exe";
    assert_eq!(
        updater::checksum(&format!("{hash}  {name}\n"), name).unwrap(),
        hash
    );
    assert!(updater::checksum(&format!("{hash}  {name}.other\n"), name).is_err());
    assert!(updater::checksum(&format!("{hash}  {name}\n{hash}  {name}"), name).is_err());
}

#[test]
fn updater_rejects_truncated_and_wrong_architecture_executables() {
    let f = Fixture::new();
    let path = f.output.join("header-fixture.exe");
    fs::write(&path, b"MZ").unwrap();
    assert!(updater::verify_executable(&path).is_err());
    let mut bytes = vec![0; 128];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(updater::verify_executable(&path).is_ok());
    bytes[68..70].copy_from_slice(&0xaa64u16.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(updater::verify_executable(&path).is_err());
}

#[cfg(windows)]
#[test]
fn network_and_alternate_stream_paths_are_rejected_before_access() {
    let f = Fixture::new();
    assert!(disk::check_path(Path::new(r"\\localhost\share\fixture.txt")).is_err());
    assert!(disk::check_path(&f.source.join("file.txt:stream")).is_err());
}

fn until(worker: &Worker, predicate: impl Fn(&Event) -> bool, seconds: u64) -> Event {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        let event = worker
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("Worker deadline exceeded");
        if let Event::Error(ref error) = event {
            panic!("Worker failed: {error}");
        }
        if predicate(&event) {
            return event;
        }
    }
}

#[test]
fn background_review_and_automatic_modes_operate_only_on_fixture_folders() {
    for automatic in [false, true] {
        let f = Fixture::new();
        let op = f.operation(Action::Copy);
        fs::remove_file(&op.source).unwrap();
        let mut config = f.config.clone();
        config.rules[0].action = Action::Copy;
        #[cfg(windows)]
        {
            // Exercise UI-style paths as well as canonical \\?\ paths returned by scanning.
            config.rules[0].destination = config.rules[0]
                .destination
                .trim_start_matches(r"\\?\")
                .to_owned();
        }
        let worker = Worker::start(f._temp.path().join("app-data"), || {});
        until(&worker, |e| matches!(e, Event::Loaded(_)), 5);
        worker
            .commands
            .send(Command::Watch { config, automatic })
            .unwrap();
        until(
            &worker,
            |e| matches!(e, Event::Plan { plan, .. } if plan.scanned == 0),
            5,
        );
        // This write must arrive through the real native watcher rather than a manual scan request.
        fs::write(&op.source, b"original fixture bytes").unwrap();
        let event = until(
            &worker,
            |e| matches!(e,Event::Plan { plan,.. } if !plan.operations.is_empty()),
            10,
        );
        if !automatic {
            assert!(!op.destination.exists());
            let Event::Plan { generation, .. } = event else {
                unreachable!()
            };
            worker.commands.send(Command::Pause).unwrap();
            until(&worker, |e| matches!(e, Event::Watching(false, false)), 5);
            worker
                .commands
                .send(Command::Apply {
                    generation,
                    indices: vec![0],
                })
                .unwrap();
        }
        until(
            &worker,
            |e| matches!(e,Event::History(entries,_) if entries.iter().any(|e|e.phase=="Done")),
            10,
        );
        assert!(op.source.exists());
        assert_eq!(
            fs::read(&op.destination).unwrap(),
            b"original fixture bytes"
        );
        if automatic {
            for _ in 0..3 {
                worker.commands.send(Command::Changed).unwrap();
            }
            until(
                &worker,
                |e| matches!(e, Event::Plan { plan, .. } if plan.operations.is_empty()),
                5,
            );
        }
        worker.commands.send(Command::Shutdown).unwrap();
        until(&worker, |e| matches!(e, Event::Stopped), 5);
        let journal = Journal::open(&f._temp.path().join("app-data").join("organizer.db")).unwrap();
        assert_eq!(journal.entries(false).unwrap().len(), 1);
    }
}

#[test]
fn orderly_shutdown_saves_settings_even_with_queued_notifications() {
    let f = Fixture::new();
    let data = f._temp.path().join("app-data");
    let worker = Worker::start(data.clone(), || {});
    until(&worker, |e| matches!(e, Event::Loaded(_)), 5);
    for _ in 0..8 {
        let _ = worker.commands.try_send(Command::Changed);
    }
    worker.shutdown(Some(f.config.clone()));
    until(&worker, |e| matches!(e, Event::Stopped), 5);
    let loaded = Journal::open(&data.join("organizer.db"))
        .unwrap()
        .config()
        .unwrap();
    assert_eq!(loaded.source_roots, f.config.source_roots);
    assert!(!loaded.startup_updates);
}

#[cfg(windows)]
#[test]
fn open_windows_writer_prevents_organization() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    let op = f.operation(Action::Move);
    let journal = Journal::open(&f.database).unwrap();
    let writer = fs::OpenOptions::new()
        .write(true)
        .share_mode(1)
        .open(&op.source)
        .unwrap();
    assert!(journal.execute(&op, &f.config).is_err());
    assert!(op.source.exists());
    assert!(!op.destination.exists());
    drop(writer);
    journal.execute(&op, &f.config).unwrap();
}
