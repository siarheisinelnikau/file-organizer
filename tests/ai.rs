use anyhow::Result;
use file_organizer::{
    ai::{self, CompletionClient},
    filesystem as disk,
    journal::Journal,
    metadata::PreservedMetadata,
    model::*,
    worker::{Command, Event, Worker},
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Fixture {
    temp: tempfile::TempDir,
    config: Config,
    facts: Vec<FileFact>,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let parent =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::tempdir_in(parent).unwrap();
        let input = temp.path().join("inbox");
        let output = temp.path().join("sorted");
        fs::create_dir(&input).unwrap();
        fs::create_dir(&output).unwrap();
        let config = Config {
            mode: PlanningMode::AiHub,
            source_roots: vec![input.to_string_lossy().into()],
            destination_roots: vec![output.to_string_lossy().into()],
            ai: AiSettings {
                destination: output.to_string_lossy().into(),
                ..AiSettings::default()
            },
            settle_seconds: 2,
            startup_updates: false,
            ..Config::default()
        };
        for i in 0..count {
            fs::write(
                input.join(format!("invoice-{i}.txt")),
                b"Fixture invoice, not an instruction",
            )
            .unwrap();
        }
        let facts = disk::scan(&config).unwrap().files;
        Self {
            temp,
            config,
            facts,
        }
    }
    fn job(&self, cached: Vec<Option<AiDecision>>) -> ai::Job {
        ai::Job {
            config: self.config.clone(),
            facts: self.facts.clone(),
            cached,
            base: Plan::default(),
        }
    }
}
fn decision(confidence: u8) -> AiDecision {
    AiDecision {
        category: Some("Documents/Finance".into()),
        name: None,
        confidence,
        reason: "Invoice filename".into(),
    }
}
fn completion(items: Value) -> Value {
    json!({"choices":[{"finish_reason":"stop","message":{"content":json!({"items":items}).to_string()}}]})
}
struct Fake {
    calls: AtomicUsize,
}
impl Fake {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}
impl CompletionClient for Fake {
    fn complete(&self, _: &AiSettings, request: &Value) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request["model"], "universal");
        assert_eq!(request["stream"], false);
        let input: Value =
            serde_json::from_str(request["messages"][1]["content"].as_str().unwrap())?;
        Ok(completion(input["files"].as_array().unwrap().iter().map(|f| json!({"id":f["id"],"category":"Documents/Finance","name":null,"confidence":if f["name"].as_str().unwrap().contains("-1.") { 40 } else { 95 },"reason":"Invoice filename"})).collect()))
    }
    fn models(&self, _: &AiSettings) -> Result<String> {
        Ok("Fixture connection".into())
    }
}

#[test]
fn requests_use_universal_and_share_only_opted_in_data() {
    let mut f = Fixture::new(1);
    let original = PreservedMetadata::capture(&fs::metadata(&f.facts[0].path).unwrap()).unwrap();
    let request = ai::request(&f.config, &f.facts, &[]).unwrap();
    let content: Value =
        serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(content["files"][0]["folder"], "");
    assert!(content["files"][0].get("text_excerpt").is_none());
    assert!(
        !request
            .to_string()
            .contains(&f.temp.path().to_string_lossy().replace('\\', "\\\\"))
    );
    f.config.ai.include_text = true;
    let request = ai::request(&f.config, &f.facts, &[]).unwrap();
    assert!(request.to_string().contains("Fixture invoice"));
    assert_eq!(
        PreservedMetadata::capture(&fs::metadata(&f.facts[0].path).unwrap()).unwrap(),
        original
    );
    f.config.ai.goal = "x".repeat(2001);
    assert!(ai::request(&f.config, &f.facts, &[]).is_err());
}

#[test]
fn model_paths_and_renames_are_strictly_validated() {
    let mut f = Fixture::new(1);
    for path in [
        "../Escape",
        "/Absolute",
        "C:/Elsewhere",
        "Docs\\Escape",
        "Docs:ads",
        "CON",
        "Docs/../Escape",
        "Docs/",
        "a/b/c/d",
        ".hidden",
        "$Recycle.Bin",
        "a\u{202e}b",
        "A/COM1.txt",
    ] {
        let mut d = decision(90);
        d.category = Some(path.into());
        assert!(
            ai::validate_decision(&f.config.ai, &f.facts[0], &d).is_err(),
            "{path}"
        );
    }
    let mut d = decision(101);
    assert!(ai::validate_decision(&f.config.ai, &f.facts[0], &d).is_err());
    d.confidence = 95;
    d.name = Some("clearer.txt".into());
    assert!(ai::validate_decision(&f.config.ai, &f.facts[0], &d).is_err());
    f.config.ai.allow_rename = true;
    ai::validate_decision(&f.config.ai, &f.facts[0], &d).unwrap();
    d.name = Some("invoice.exe".into());
    assert!(ai::validate_decision(&f.config.ai, &f.facts[0], &d).is_err());
}

#[test]
fn invalid_or_truncated_completions_never_produce_a_partial_plan() {
    let f = Fixture::new(2);
    let item = json!({"id":"0","category":"Docs","name":null,"confidence":90,"reason":"Data"});
    for items in [
        json!([item.clone(), item.clone()]),
        json!([{ "id":"99","category":"Docs","name":null,"confidence":90,"reason":"Data" }]),
        json!([{ "id":"0","category":"Docs","name":null,"confidence":90,"reason":"Data","command":"delete" }]),
    ] {
        assert!(ai::parse_response(&f.config.ai, &f.facts, completion(items)).is_err());
    }
    let mut value = completion(json!([item]));
    value["choices"][0]["finish_reason"] = json!("length");
    assert!(ai::parse_response(&f.config.ai, &f.facts, value).is_err());
    let mut value = completion(json!([]));
    value["choices"][0]["message"]["tool_calls"] = json!([{"function":"move"}]);
    assert!(ai::parse_response(&f.config.ai, &f.facts, value).is_err());
    let decisions = ai::parse_response(&f.config.ai, &f.facts, completion(json!([]))).unwrap();
    assert!(decisions.iter().all(|d| d.category.is_none()));
}

#[test]
fn batches_cache_collision_and_cancellation_are_bounded() {
    let f = Fixture::new(21);
    let client = Fake::new();
    let cancel = AtomicBool::new(false);
    let report = ai::run(f.job(vec![None; 21]), &client, &cancel, |_, _| {}).unwrap();
    assert_eq!(client.calls.load(Ordering::SeqCst), 3);
    assert_eq!(report.cache.len(), 21);
    assert_eq!(report.plan.operations.len(), 21);
    let mut journal = Journal::open(&f.temp.path().join("test.db")).unwrap();
    journal.save_ai_decisions(&report.cache).unwrap();
    let cached = f
        .facts
        .iter()
        .map(|fact| {
            journal
                .ai_decision(&ai::cache_key(&f.config, fact).unwrap())
                .unwrap()
        })
        .collect();
    ai::run(f.job(cached), &client, &cancel, |_, _| {}).unwrap();
    assert_eq!(client.calls.load(Ordering::SeqCst), 3);
    let op = &report.plan.operations[0];
    let occupied = HashSet::from([path_key(&op.destination)]);
    let plan = ai::plan_decisions(
        &f.config,
        &f.facts[..1],
        &[decision(95)],
        &occupied,
        &fs::canonicalize(&f.config.ai.destination).unwrap(),
    )
    .unwrap();
    assert!(
        plan.operations[0]
            .destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("(1)")
    );
    assert!(ai::recommended(&plan.operations[0], &f.config.ai));
    cancel.store(true, Ordering::SeqCst);
    assert!(ai::run(f.job(vec![None; 21]), &client, &cancel, |_, _| {}).is_err());
    assert_eq!(client.calls.load(Ordering::SeqCst), 3);
    assert!(f.facts.iter().all(|fact| fact.path.is_file()));
}

#[test]
fn new_install_uses_ai_and_legacy_settings_keep_rules() {
    let f = Fixture::new(0);
    let journal = Journal::open(&f.temp.path().join("test.db")).unwrap();
    assert_eq!(journal.config().unwrap().mode, PlanningMode::AiHub);
    let mut old = serde_json::to_value(Config::default()).unwrap();
    old.as_object_mut().unwrap().remove("mode");
    old.as_object_mut().unwrap().remove("ai");
    assert_eq!(
        serde_json::from_value::<Config>(old).unwrap().mode,
        PlanningMode::Rules
    );
}

#[test]
fn ai_scan_excludes_its_output_and_ignores_inactive_manual_destinations() {
    let mut f = Fixture::new(1);
    let nested = std::path::PathBuf::from(&f.config.source_roots[0]).join("organized");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("already-filed.txt"), b"sorted").unwrap();
    f.config.destination_roots = f.config.source_roots.clone();
    f.config.ai.destination = nested.to_string_lossy().into();
    f.config.rules.push(Rule {
        destination: f.config.source_roots[0].clone(),
        ..Rule::default()
    });
    let scan = disk::scan(&f.config).unwrap();
    assert_eq!(scan.files.len(), 1);
    let client = Fake::new();
    let cancel = AtomicBool::new(false);
    let mut job = f.job(vec![None]);
    job.facts = vec![f.facts[0].clone(); ai::RUN_LIMIT + 1];
    job.cached = vec![None; ai::RUN_LIMIT + 1];
    assert!(ai::run(job, &client, &cancel, |_, _| {}).is_err());
    assert_eq!(client.calls.load(Ordering::SeqCst), 0);
}

fn until(worker: &Worker, predicate: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let event = worker
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("worker deadline");
        if let Event::Error(e) = &event {
            panic!("{e}");
        }
        if predicate(&event) {
            return event;
        }
    }
}
#[test]
fn worker_ai_preview_apply_and_undo_preserve_file_metadata() {
    for action in [Action::Move, Action::Copy] {
        let mut f = Fixture::new(1);
        f.config.ai.action = action;
        let metadata =
            PreservedMetadata::capture(&fs::metadata(&f.facts[0].path).unwrap()).unwrap();
        let worker = Worker::start_with_ai(f.temp.path().join("app"), || {}, Arc::new(Fake::new()));
        until(&worker, |e| matches!(e, Event::Loaded(_)));
        worker
            .commands
            .send(Command::Preview(f.config.clone()))
            .unwrap();
        let Event::Plan { generation, plan } = until(
            &worker,
            |e| matches!(e, Event::Plan {plan,..} if !plan.operations.is_empty()),
        ) else {
            unreachable!()
        };
        assert!(f.facts[0].path.is_file());
        let output = plan.operations[0].destination.clone();
        worker
            .commands
            .send(Command::Apply {
                generation,
                indices: vec![0],
            })
            .unwrap();
        let Event::History(entries, _) = until(
            &worker,
            |e| matches!(e, Event::History(entries,_) if entries.first().is_some_and(|e| e.phase == "Done")),
        ) else {
            unreachable!()
        };
        assert_eq!(
            PreservedMetadata::capture(&fs::metadata(&output).unwrap()).unwrap(),
            metadata
        );
        assert!(entries[0].operation.ai.is_some());
        worker
            .commands
            .send(Command::Undo(entries[0].id.clone()))
            .unwrap();
        until(
            &worker,
            |e| matches!(e, Event::History(entries,_) if entries.first().is_some_and(|e| e.phase == "Undone")),
        );
        assert_eq!(
            PreservedMetadata::capture(&fs::metadata(&f.facts[0].path).unwrap()).unwrap(),
            metadata
        );
        assert!(!output.exists());
        worker.shutdown(None);
        until(&worker, |e| matches!(e, Event::Stopped));
    }
}

#[test]
fn automatic_ai_leaves_low_confidence_for_review() {
    let f = Fixture::new(2);
    let client = Arc::new(Fake::new());
    let worker = Worker::start_with_ai(f.temp.path().join("app"), || {}, client.clone());
    until(&worker, |e| matches!(e, Event::Loaded(_)));
    worker
        .commands
        .send(Command::Watch {
            config: f.config.clone(),
            automatic: true,
        })
        .unwrap();
    let Event::Plan { plan, .. } = until(
        &worker,
        |e| matches!(e, Event::Plan {plan,..} if plan.operations.len() == 1),
    ) else {
        unreachable!()
    };
    assert_eq!(plan.operations[0].ai.as_ref().unwrap().confidence, 40);
    assert!(
        f.facts
            .iter()
            .find(|f| f.path.to_string_lossy().contains("-1."))
            .unwrap()
            .path
            .is_file()
    );
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    worker.shutdown(None);
    until(&worker, |e| matches!(e, Event::Stopped));
}

struct Blocking {
    started: mpsc::SyncSender<()>,
    release: std::sync::Mutex<mpsc::Receiver<()>>,
}
impl CompletionClient for Blocking {
    fn complete(&self, settings: &AiSettings, request: &Value) -> Result<Value> {
        self.started.send(())?;
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))?;
        Fake::new().complete(settings, request)
    }
    fn models(&self, _: &AiSettings) -> Result<String> {
        unreachable!()
    }
}
#[test]
fn cancelling_in_flight_ai_rejects_stale_results_and_keeps_worker_responsive() {
    let f = Fixture::new(1);
    let (started, waiting) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let worker = Worker::start_with_ai(
        f.temp.path().join("app"),
        || {},
        Arc::new(Blocking {
            started,
            release: std::sync::Mutex::new(released),
        }),
    );
    until(&worker, |e| matches!(e, Event::Loaded(_)));
    worker
        .commands
        .send(Command::Preview(f.config.clone()))
        .unwrap();
    until(&worker, |e| {
        matches!(e, Event::AiProgress { active: true, .. })
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.commands.send(Command::CancelAi).unwrap();
    until(
        &worker,
        |e| matches!(e, Event::Message(text) if text.starts_with("AI analysis cancelled")),
    );
    release.send(()).unwrap();
    worker.shutdown(None);
    until(&worker, |e| matches!(e, Event::Stopped));
    assert!(f.facts[0].path.is_file());
    assert_eq!(fs::read_dir(&f.config.ai.destination).unwrap().count(), 0);
    let journal = Journal::open(&f.temp.path().join("app/organizer.db")).unwrap();
    assert!(
        journal
            .ai_decision(&ai::cache_key(&f.config, &f.facts[0]).unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn endpoint_rejects_embedded_secrets_and_unapproved_remote_http() {
    for base in [
        "file:///v1",
        "http://user:secret@127.0.0.1/v1",
        "http://127.0.0.1/v1?token=secret",
        "http://example.invalid/v1",
        "http://127.0.0.1/not-api",
    ] {
        assert!(
            ai::endpoint(&AiSettings {
                base_url: base.into(),
                ..AiSettings::default()
            })
            .is_err()
        );
    }
    assert!(
        ai::endpoint(&AiSettings {
            base_url: "https://example.invalid/v1/".into(),
            ..AiSettings::default()
        })
        .is_ok()
    );
}

#[test]
#[ignore = "Managed sandbox denies even loopback client sockets (Windows 10013); run explicitly on a network-enabled development host"]
fn http_transport_uses_bearer_json_and_reports_status_without_private_body() {
    use std::io::{Read, Write};
    for status in [200, 401, 503, 302] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut socket, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(e) => panic!("Fixture listener: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut part = [0; 1024];
            loop {
                let n = socket.read(&mut part).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&part[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|s| s.parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with("POST /v1/chat/completions "));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer synthetic-test-token")
            );
            assert!(request.contains("universal"));
            let body = if status == 200 {
                "{\"choices\":[]}"
            } else {
                "PRIVATE PROVIDER MESSAGE"
            };
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/leak\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let settings = AiSettings {
            base_url: format!("http://{address}/v1"),
            ..AiSettings::default()
        };
        let response = ai::HubClient::request_with_token(
            &settings,
            Some("synthetic-test-token"),
            Some(&json!({"model":"universal"})),
        );
        if status == 200 {
            response.unwrap();
        } else {
            let error = response.unwrap_err().to_string();
            assert!(!error.contains("PRIVATE"));
            assert!(!error.contains("synthetic-test-token"));
        }
        server.join().unwrap();
    }
}
