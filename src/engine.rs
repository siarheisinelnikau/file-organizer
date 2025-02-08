//! Pure, deterministic planning. This module never accesses the filesystem.
use crate::model::*;
use std::{collections::HashSet, path::Path};

/// Case-insensitive '*' / '?' glob. '*' can include directory separators.
pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(s) = star {
            mark += 1;
            j = mark;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

pub fn safe_filename(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_uppercase();
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with([' ', '.'])
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(['1', '2', '3', '4', '5', '6', '7', '8', '9']))
        && name.encode_utf16().count() <= 240
}

fn output_name(rule: &Rule, file: &Path) -> Option<String> {
    let name = file.file_name()?.to_str()?;
    let stem = file.file_stem()?.to_str()?;
    let ext = file.extension().and_then(|s| s.to_str()).unwrap_or("");
    let result = rule
        .name_template
        .replace("{name}", name)
        .replace("{stem}", stem)
        .replace("{ext}", ext);
    (safe_filename(&result) && !result.contains(['{', '}'])).then_some(result)
}

pub fn plan(config: &Config, facts: &[FileFact], occupied: &HashSet<String>, now_ns: u128) -> Plan {
    let mut result = Plan {
        scanned: facts.len(),
        ..Plan::default()
    };
    let mut reserved = occupied.clone();
    for file in facts {
        let name = file.path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let ext = file.path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let relative = file
            .path
            .parent()
            .unwrap_or(Path::new(""))
            .strip_prefix(&file.source_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let rule = config.rules.iter().find(|r| {
            r.enabled
                && matches(&r.pattern, name)
                && (r.extensions.trim().is_empty()
                    || r.extensions
                        .split(',')
                        .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(ext)))
                && matches(&r.folder_pattern, &relative)
                && file.stamp.bytes >= r.min_bytes
                && (r.max_bytes == 0 || file.stamp.bytes <= r.max_bytes)
                && now_ns.saturating_sub(file.stamp.modified_ns)
                    >= u128::from(r.min_age_minutes) * 60_000_000_000
        });
        let Some(rule) = rule else { continue };
        let Some(name) = output_name(rule, &file.path) else {
            result
                .notices
                .push(format!("{}: invalid filename template", rule.name));
            continue;
        };
        let parent = if rule.action == Action::Rename {
            file.path.parent().unwrap().to_owned()
        } else {
            rule.destination.clone().into()
        };
        if !parent.is_absolute() {
            result
                .notices
                .push(format!("{}: destination must be absolute", rule.name));
            continue;
        }
        let mut destination = parent.join(&name);
        if path_key(&destination) == path_key(&file.path) {
            continue;
        }
        if reserved.contains(&path_key(&destination)) {
            if rule.conflict == Conflict::Skip {
                result
                    .notices
                    .push(format!("Collision skipped: {}", destination.display()));
                continue;
            }
            let stem = Path::new(&name).file_stem().unwrap().to_string_lossy();
            let ext = Path::new(&name)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let Some(unique) = (1..=1000)
                .map(|n| parent.join(format!("{stem} ({n}){ext}")))
                .find(|p| !reserved.contains(&path_key(p)))
            else {
                result.notices.push(format!("No available name for {name}"));
                continue;
            };
            destination = unique;
        }
        reserved.insert(path_key(&destination));
        result.operations.push(Operation {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            source: file.path.clone(),
            destination,
            stamp: file.stamp.clone(),
            action: rule.action,
        });
    }
    result
}
