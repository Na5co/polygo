//! Flutter Application Resource Bundle (`.arb`): flat JSON of `key: text` plus
//! `@key` metadata objects (description, placeholders) and `@@locale`.
//!
//! Parsing reuses the span-based JSON parser, so round-trips are byte-stable. Locale
//! files are rebuilt in source key order; metadata lives only in the template file.

use crate::core::Unit;
use crate::formats::json;
use anyhow::Result;
use std::collections::BTreeMap;

pub type Document = json::Document;

pub fn parse(text: &str) -> Result<Document> {
    json::parse(text)
}

pub fn serialize(doc: &Document) -> String {
    json::serialize(doc)
}

/// Translatable units: top-level string values whose key does not start with `@`.
/// `@key.description` becomes the unit comment; placeholder names are appended.
pub fn units(doc: &Document) -> Vec<Unit> {
    let mut descriptions: BTreeMap<String, String> = BTreeMap::new();
    let mut placeholders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in &doc.entries {
        if e.path.len() == 2
            && e.path[0].starts_with('@')
            && !e.path[0].starts_with("@@")
            && e.path[1] == "description"
        {
            descriptions.insert(e.path[0][1..].to_string(), e.text());
        }
        if e.path.len() >= 3 && e.path[0].starts_with('@') && e.path[1] == "placeholders" {
            let key = e.path[0][1..].to_string();
            let name = e.path[2].clone();
            let v = placeholders.entry(key).or_default();
            if !v.contains(&name) {
                v.push(name);
            }
        }
    }
    doc.entries
        .iter()
        .filter(|e| e.path.len() == 1 && !e.path[0].starts_with('@'))
        .map(|e| {
            let key = e.path[0].clone();
            let mut comment = descriptions.get(&key).cloned();
            if let Some(ph) = placeholders.get(&key)
                && !ph.is_empty()
            {
                let note = format!("placeholders: {}", ph.join(", "));
                comment = Some(match comment {
                    Some(c) => format!("{c} ({note})"),
                    None => note,
                });
            }
            Unit {
                key,
                source: e.text(),
                comment,
                translations: BTreeMap::new(),
            }
        })
        .collect()
}

/// Values of a locale file: `key → text` for non-`@` keys.
pub fn values(doc: &Document) -> BTreeMap<String, String> {
    doc.entries
        .iter()
        .filter(|e| e.path.len() == 1 && !e.path[0].starts_with('@'))
        .map(|e| (e.path[0].clone(), e.text()))
        .collect()
}

/// Build a locale file: `@@locale` first, then every source key in source order using
/// `values` (new translations) or the existing target value, then target-only keys.
/// `@key` metadata is not copied (Flutter reads it from the template only).
pub fn build_locale_file(
    source_text: &str,
    existing_text: Option<&str>,
    locale: &str,
    values: &BTreeMap<String, String>,
) -> String {
    let source: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(source_text).unwrap_or_default();
    let existing: serde_json::Map<String, serde_json::Value> = existing_text
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();
    let mut out = serde_json::Map::new();
    out.insert(
        "@@locale".into(),
        serde_json::Value::String(locale.to_string()),
    );
    for (k, v) in &existing {
        if k.starts_with("@@") && k != "@@locale" {
            out.insert(k.clone(), v.clone());
        }
    }
    for k in source.keys() {
        if k.starts_with('@') {
            continue;
        }
        if let Some(t) = values.get(k) {
            out.insert(k.clone(), serde_json::Value::String(t.clone()));
        } else if let Some(v) = existing.get(k).filter(|v| v.is_string()) {
            out.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &existing {
        if !out.contains_key(k) && !k.starts_with('@') {
            out.insert(k.clone(), v.clone());
        }
    }
    let style = existing_text
        .map(json::Style::detect)
        .unwrap_or_else(|| json::Style::detect(source_text));
    json::render(&serde_json::Value::Object(out), &style)
}
