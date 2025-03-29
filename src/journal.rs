//! Durable intent journal. Every mutation has a persisted recovery record first.
use crate::{filesystem as disk, metadata::PreservedMetadata, model::*};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub operation: Operation,
    pub sha256: String,
    /// Older journals did not record metadata; their entries remain recoverable.
    #[serde(default)]
    pub metadata: Option<PreservedMetadata>,
    pub source_roots: Vec<String>,
    pub destination_roots: Vec<String>,
    pub stage: PathBuf,
    pub phase: String,
    pub error: String,
    pub created: String,
}

pub struct Journal {
    db: Connection,
}

fn fingerprint(path: &Path, stamp: &Stamp) -> String {
    format!("{}|{}|{}", path_key(path), stamp.bytes, stamp.modified_ns)
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        disk::check_path(path)?;
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.as_os_str().to_owned();
            sidecar.push(suffix);
            disk::check_path(Path::new(&sidecar))?;
        }
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS config (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS operations (
                id TEXT PRIMARY KEY, json TEXT NOT NULL, phase TEXT NOT NULL, error TEXT NOT NULL DEFAULT '',
                created TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S UTC','now')));
            CREATE TABLE IF NOT EXISTS processed (fingerprint TEXT PRIMARY KEY);
            CREATE INDEX IF NOT EXISTS operations_pending ON operations(phase)
                WHERE phase NOT IN ('Done','Undone','Recovered');")?;
        Ok(Self { db })
    }

    pub fn config(&self) -> Result<Config> {
        let mut statement = self.db.prepare("SELECT json FROM config WHERE id=1")?;
        let mut rows = statement.query([])?;
        match rows.next()? {
            Some(row) => Ok(serde_json::from_str(&row.get::<_, String>(0)?)?),
            None => Ok(Config::default()),
        }
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        self.db.execute("INSERT INTO config(id,json) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            [serde_json::to_string(config)?])?;
        Ok(())
    }

    pub fn entries(&self, pending_only: bool) -> Result<Vec<Entry>> {
        let sql = if pending_only {
            "SELECT id,json,phase,error,created FROM operations WHERE phase NOT IN ('Done','Undone','Recovered') ORDER BY rowid DESC"
        } else {
            "SELECT id,json,phase,error,created FROM operations ORDER BY rowid DESC LIMIT 500"
        };
        let mut statement = self.db.prepare(sql)?;
        let mut rows = statement.query([])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            let mut entry: Entry = serde_json::from_str(&row.get::<_, String>(1)?)?;
            entry.id = row.get(0)?;
            entry.phase = row.get(2)?;
            entry.error = row.get(3)?;
            entry.created = row.get(4)?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub fn suppressed(&self) -> Result<HashSet<String>> {
        let mut statement = self.db.prepare("SELECT fingerprint FROM processed")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn is_suppressed(fact: &FileFact, keys: &HashSet<String>) -> bool {
        keys.contains(&fingerprint(&fact.path, &fact.stamp))
    }

    pub fn unsuppressed(&self, files: Vec<FileFact>) -> Result<Vec<FileFact>> {
        // Query only the bounded scan's candidates rather than loading all lifetime history.
        let mut statement = self
            .db
            .prepare("SELECT 1 FROM processed WHERE fingerprint=?1")?;
        let mut eligible = Vec::new();
        for fact in files {
            if !statement.exists([fingerprint(&fact.path, &fact.stamp)])? {
                eligible.push(fact);
            }
        }
        Ok(eligible)
    }

    fn remember(&self, path: &Path, stamp: &Stamp) -> Result<()> {
        self.db.execute(
            "INSERT OR IGNORE INTO processed(fingerprint) VALUES(?1)",
            [fingerprint(path, stamp)],
        )?;
        Ok(())
    }

    fn phase(&self, id: &str, phase: &str) -> Result<()> {
        self.db.execute(
            "UPDATE operations SET phase=?2,error='' WHERE id=?1",
            params![id, phase],
        )?;
        Ok(())
    }

    fn error(&self, id: &str, error: &anyhow::Error) -> Result<()> {
        self.db.execute(
            "UPDATE operations SET error=?2 WHERE id=?1",
            params![id, format!("{error:#}")],
        )?;
        Ok(())
    }

    pub fn execute(&self, operation: &Operation, config: &Config) -> Result<()> {
        ensure!(
            self.entries(true)?.is_empty(),
            "Recover unfinished operations before applying another plan"
        );
        let source = disk::inside(&operation.source, &config.source_roots)?;
        let destination_roots = if operation.action == Action::Rename {
            &config.source_roots
        } else {
            &config.destination_roots
        };
        let destination = disk::inside(&operation.destination, destination_roots)?;
        ensure!(
            path_key(&source) != path_key(&destination),
            "Source and destination are identical"
        );
        ensure!(
            !destination.try_exists()?,
            "Destination appeared after preview; generate a fresh plan"
        );
        let mut input =
            disk::LockedFile::open_preserving(&source, operation.action != Action::Copy)?;
        ensure!(
            input.stamp()? == operation.stamp,
            "Source changed after preview; generate a fresh plan"
        );
        let metadata = input.metadata()?;
        metadata.validate()?;
        let hash = input.digest()?;
        let id = uuid::Uuid::new_v4().to_string();
        let stage = destination
            .parent()
            .context("No destination parent")?
            .join(format!(".fo-{id}.tmp"));
        let entry = Entry {
            id: id.clone(),
            operation: Operation {
                source: source.clone(),
                destination: destination.clone(),
                ..operation.clone()
            },
            sha256: hash.clone(),
            metadata: Some(metadata.clone()),
            source_roots: config.source_roots.clone(),
            destination_roots: destination_roots.clone(),
            stage,
            phase: "Prepared".into(),
            error: String::new(),
            created: String::new(),
        };
        self.db.execute(
            "INSERT INTO operations(id,json,phase) VALUES(?1,?2,'Prepared')",
            params![id, serde_json::to_string(&entry)?],
        )?;
        let outcome = (|| -> Result<()> {
            disk::ensure_parent(&destination, destination_roots)?;
            input.copy_to(&entry.stage)?;
            ensure!(
                disk::digest(&entry.stage)? == hash,
                "Staged data verification failed"
            );
            ensure!(
                input.stamp()? == operation.stamp && input.digest()? == hash,
                "Source changed while copying"
            );
            disk::set_metadata(&entry.stage, &metadata)?;
            self.phase(&id, "Staged")?;
            disk::inside(&destination, destination_roots)?;
            disk::publish(&entry.stage, &destination)?;
            self.phase(&id, "Published")?;
            // Reapply after rename to defeat Windows creation-time tunneling at the final name.
            disk::set_metadata(&destination, &metadata)?;
            if operation.action != Action::Copy {
                input.delete()?;
            } else {
                #[cfg(not(windows))]
                input.set_metadata(&metadata)?;
            }
            self.remember(&source, &operation.stamp)?;
            self.remember(&destination, &disk::stamp(&fs::metadata(&destination)?)?)?;
            self.phase(&id, "Done")?;
            Ok(())
        })();
        if let Err(ref e) = outcome {
            self.error(&id, e)?;
        }
        outcome
    }

    /// Roll back conservatively. Changed files and occupied restore paths are never overwritten.
    fn rollback(&self, entry: &Entry) -> Result<()> {
        let op = &entry.operation;
        disk::inside(&op.source, &entry.source_roots)?;
        disk::inside(&op.destination, &entry.destination_roots)?;
        disk::inside(&entry.stage, &entry.destination_roots)?;
        if op.destination.try_exists()? {
            let mut output = disk::LockedFile::open_preserving(&op.destination, true)?;
            // Legacy records can only retain the metadata still available on the output.
            let metadata = entry.metadata.clone().unwrap_or(output.metadata()?);
            ensure!(
                output.digest()? == entry.sha256,
                "Destination changed; preserving it: {}",
                op.destination.display()
            );
            if op.action != Action::Copy {
                if op.source.try_exists()? {
                    let mut original = disk::LockedFile::open_preserving(&op.source, false)?;
                    ensure!(
                        original.digest()? == entry.sha256,
                        "Restore path is occupied by different content"
                    );
                    // During normal undo an existing path is a conflict, even with identical bytes.
                    ensure!(
                        entry.phase != "Done",
                        "Restore path is already occupied; move it aside before undo"
                    );
                    if let Some(metadata) = &entry.metadata {
                        original.set_metadata(metadata)?;
                    }
                } else {
                    disk::ensure_parent(&op.source, &entry.source_roots)?;
                    let restore = op
                        .source
                        .parent()
                        .unwrap()
                        .join(format!(".fo-{}-undo.tmp", entry.id));
                    if restore.try_exists()? {
                        ensure!(
                            disk::digest(&restore)? == entry.sha256,
                            "Partial restore file requires manual inspection: {}",
                            restore.display()
                        );
                    } else {
                        output.copy_to(&restore)?;
                        ensure!(
                            disk::digest(&restore)? == entry.sha256,
                            "Restore verification failed"
                        );
                    }
                    disk::set_metadata(&restore, &metadata)?;
                    disk::inside(&op.source, &entry.source_roots)?;
                    disk::publish(&restore, &op.source)?;
                    disk::set_metadata(&op.source, &metadata)?;
                }
                self.remember(&op.source, &disk::stamp(&fs::metadata(&op.source)?)?)?;
            }
            output.delete()?;
        } else if op.action != Action::Copy && !op.source.try_exists()? {
            anyhow::bail!("Both paths are missing; manual recovery is required");
        } else if op.action != Action::Copy {
            let mut original = disk::LockedFile::open_preserving(&op.source, false)?;
            ensure!(
                original.digest()? == entry.sha256,
                "Original file changed; inspect this recovery record"
            );
            if let Some(metadata) = &entry.metadata {
                original.set_metadata(metadata)?;
            }
        }
        if entry.stage.try_exists()? {
            let mut stage = disk::LockedFile::open_preserving(&entry.stage, true)?;
            ensure!(
                stage.digest()? == entry.sha256,
                "Partial staging file preserved for manual inspection: {}",
                entry.stage.display()
            );
            stage.delete()?;
        }
        self.remember(&op.source, &op.stamp)?;
        Ok(())
    }

    pub fn undo(&self, id: &str) -> Result<()> {
        ensure!(
            self.entries(true)?.is_empty(),
            "Recover unfinished operations first"
        );
        let entry = self
            .entries(false)?
            .into_iter()
            .find(|e| e.id == id)
            .context("Journal entry not found in recent history")?;
        ensure!(
            entry.phase == "Done",
            "Only completed operations can be undone"
        );
        // Preflight before persisting UndoPending: normal undo must never accept an occupied source.
        if entry.operation.action != Action::Copy {
            ensure!(
                !entry.operation.source.try_exists()?,
                "Restore path is occupied"
            );
        }
        self.phase(id, "UndoPending")?;
        let result = self.rollback(&entry);
        match &result {
            Ok(()) => self.phase(id, "Undone")?,
            Err(e) => self.error(id, e)?,
        }
        result
    }

    pub fn recover(&self) -> Vec<String> {
        let entries = match self.entries(true) {
            Ok(entries) => entries,
            Err(e) => return vec![format!("{e:#}")],
        };
        let mut results = Vec::new();
        for entry in entries {
            let outcome = self
                .rollback(&entry)
                .and_then(|()| self.phase(&entry.id, "Recovered"));
            match outcome {
                Ok(()) => results.push(format!("Recovered {}", entry.operation.source.display())),
                Err(e) => {
                    let _ = self.error(&entry.id, &e);
                    results.push(format!("Recovery needs attention: {e:#}"));
                }
            }
        }
        results
    }
}
