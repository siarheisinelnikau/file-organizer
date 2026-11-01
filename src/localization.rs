//! Explicit UI catalogs. File names, paths, model responses and diagnostic details are never translated.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    Russian,
    French,
    German,
    Spanish,
    Italian,
}
impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }
    fn index(self) -> Option<usize> {
        match self {
            Self::English => None,
            Self::Russian => Some(0),
            Self::French => Some(1),
            Self::German => Some(2),
            Self::Spanish => Some(3),
            Self::Italian => Some(4),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

type Catalog = BTreeMap<String, [String; 5]>;
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        include_str!("../locales/messages.tsv")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let columns: Vec<_> = line.split('|').map(str::to_owned).collect();
                assert_eq!(columns.len(), 6, "translation columns: {line}");
                (
                    columns[0].clone(),
                    columns[1..].to_vec().try_into().expect("five translations"),
                )
            })
            .collect()
    })
}
pub fn text(language: Language, english: &str) -> &str {
    language
        .index()
        .and_then(|index| catalog().get(english).map(|row| row[index].as_str()))
        .unwrap_or(english)
}
pub fn set_language(ctx: &egui::Context, language: Language) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("ui-language"), language));
}
pub fn language(ui: &egui::Ui) -> Language {
    ui.ctx()
        .data(|data| data.get_temp(egui::Id::unique("ui-language")))
        .unwrap_or_default()
}
pub fn tr<'a>(ui: &egui::Ui, english: &'a str) -> &'a str {
    text(language(ui), english)
}

/// Translate a known template while preserving the values already formatted by Rust.
/// Only explicitly marked UI templates call this function; arbitrary data is not scanned.
pub fn formatted(ui: &egui::Ui, template: &str, rendered: &str) -> String {
    format_for(language(ui), template, rendered)
}
pub fn format_for(language: Language, template: &str, rendered: &str) -> String {
    let translated = text(language, template);
    if translated == template {
        return rendered.to_owned();
    }
    let mut rest = template;
    let mut values = Vec::new();
    let mut input = rendered;
    while let Some(open) = rest.find('{') {
        let prefix = &rest[..open];
        let Some(after_prefix) = input.strip_prefix(prefix) else {
            return rendered.to_owned();
        };
        let Some(close) = rest[open..].find('}').map(|i| open + i) else {
            return rendered.to_owned();
        };
        let token = &rest[open..=close];
        rest = &rest[close + 1..];
        let following = rest.split('{').next().unwrap_or_default();
        let end = if following.is_empty() {
            after_prefix.len()
        } else {
            let found = if rest.contains('{') {
                after_prefix.find(following)
            } else {
                after_prefix.strip_suffix(following).map(str::len)
            };
            let Some(end) = found else {
                return rendered.to_owned();
            };
            end
        };
        values.push((token, &after_prefix[..end]));
        input = &after_prefix[end..];
    }
    if input != rest {
        return rendered.to_owned();
    }
    let mut result = String::new();
    let mut translated_rest = translated;
    let mut used = vec![false; values.len()];
    while let Some(open) = translated_rest.find('{') {
        result.push_str(&translated_rest[..open]);
        let Some(close) = translated_rest[open..].find('}').map(|i| open + i) else {
            return rendered.to_owned();
        };
        let token = &translated_rest[open..=close];
        let Some((index, (_, value))) = values
            .iter()
            .enumerate()
            .find(|(index, (key, _))| !used[*index] && *key == token)
        else {
            return rendered.to_owned();
        };
        used[index] = true;
        result.push_str(value);
        translated_rest = &translated_rest[close + 1..];
    }
    result.push_str(translated_rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(text: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let close = rest[open..].find('}').expect("closed placeholder") + open;
            result.push(rest[open..=close].into());
            rest = &rest[close + 1..];
        }
        result.sort();
        result
    }
    #[test]
    fn every_catalog_entry_has_five_complete_translations_and_preserves_placeholders() {
        assert!(catalog().len() > 200);
        for (key, row) in catalog() {
            for translated in row {
                assert!(!translated.trim().is_empty(), "{key}");
                assert_eq!(
                    placeholders(key),
                    placeholders(translated),
                    "{key}: {translated}"
                );
            }
        }
    }
    #[test]
    fn formatted_messages_preserve_version_paths_and_multiple_values() {
        assert_eq!(
            format_for(Language::Russian, "Version {}", "Version 0.4.0"),
            "Версия 0.4.0"
        );
        assert_eq!(
            format_for(Language::French, "AI · {} / {} files", "AI · 7 / 11 files"),
            "IA · 7 / 11 fichiers"
        );
        assert_eq!(
            text(Language::German, r"C:\Review\Documents.txt"),
            r"C:\Review\Documents.txt"
        );
        assert_eq!(text(Language::English, "Review"), "Review");
    }
}
