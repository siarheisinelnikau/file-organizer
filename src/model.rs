use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Move,
    Copy,
    Rename,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conflict {
    Skip,
    KeepBoth,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanningMode {
    #[default]
    Rules,
    AiHub,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub base_url: String,
    pub destination: String,
    pub goal: String,
    pub include_text: bool,
    pub allow_rename: bool,
    pub allow_http_lan: bool,
    pub minimum_confidence: u8,
    pub action: Action,
    pub conflict: Conflict,
}
impl Default for AiSettings {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080/v1".into(),
            destination: String::new(),
            goal:
                "Organize files by subject and purpose using a small, consistent folder hierarchy."
                    .into(),
            include_text: false,
            allow_rename: false,
            allow_http_lan: false,
            minimum_confidence: 80,
            action: Action::Move,
            conflict: Conflict::KeepBoth,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiDecision {
    pub category: Option<String>,
    pub name: Option<String>,
    pub confidence: u8,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub pattern: String,
    pub extensions: String,
    /// Optional path relative to a source root, matched as a glob.
    pub folder_pattern: String,
    pub min_bytes: u64,
    /// Zero means no maximum.
    pub max_bytes: u64,
    pub min_age_minutes: u64,
    pub action: Action,
    pub destination: String,
    pub name_template: String,
    pub conflict: Conflict,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "New rule".into(),
            enabled: true,
            pattern: "*".into(),
            extensions: String::new(),
            folder_pattern: "*".into(),
            min_bytes: 0,
            max_bytes: 0,
            min_age_minutes: 1,
            action: Action::Move,
            destination: String::new(),
            name_template: "{name}".into(),
            conflict: Conflict::Skip,
        }
    }
}

impl Rule {
    pub fn template(kind: &str) -> Self {
        let (name, extensions) = match kind {
            "Photos" => ("Photos", "jpg,jpeg,png,heic,webp,tiff,raw"),
            "Documents" => ("Documents", "pdf,doc,docx,txt,odt,xlsx,csv,pptx"),
            _ => ("Downloads: archives", "zip,7z,rar,tar,gz"),
        };
        Self {
            name: name.into(),
            extensions: extensions.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mode: PlanningMode,
    pub ai: AiSettings,
    pub source_roots: Vec<String>,
    pub destination_roots: Vec<String>,
    pub rules: Vec<Rule>,
    pub recursive: bool,
    pub startup_updates: bool,
    pub settle_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: PlanningMode::Rules,
            ai: AiSettings::default(),
            source_roots: Vec::new(),
            destination_roots: Vec::new(),
            rules: Vec::new(),
            recursive: true,
            startup_updates: true,
            settle_seconds: 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub bytes: u64,
    pub modified_ns: u128,
}

#[derive(Clone, Debug)]
pub struct FileFact {
    pub path: PathBuf,
    pub source_root: PathBuf,
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    #[serde(default)]
    pub ai: Option<AiDecision>,
    pub rule_id: String,
    pub rule_name: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub stamp: Stamp,
    pub action: Action,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub operations: Vec<Operation>,
    pub notices: Vec<String>,
    pub scanned: usize,
    pub waiting: usize,
}

pub fn path_key(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}
