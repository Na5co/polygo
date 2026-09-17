//! Xcode String Catalog (`.xcstrings`) — JSON written by Xcode with a fixed style:
//! two-space indent, `"key" : value` (space before the colon), keys in Xcode's own
//! order (not code-point sorted), empty objects as `{\n\n<indent>}`, raw UTF-8,
//! forward slashes unescaped, and usually no trailing newline.
//!
//! We keep the parsed tree order-preserving and re-emit it in exactly that style,
//! so an untouched file serializes back to the identical bytes.

use anyhow::{Context, Result};
use serde_json::Value;
use std::fmt::Write as _;

/// A parsed catalog plus the formatting facts needed to reproduce the original bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The JSON tree, key order preserved.
    pub root: Value,
    /// Whether the original file ended with a newline.
    pub trailing_newline: bool,
    /// Line ending used by the original file (`"\n"` or `"\r\n"`).
    pub newline: &'static str,
}

/// Parse a `.xcstrings` file's contents.
pub fn parse(text: &str) -> Result<Document> {
    let root: Value = serde_json::from_str(text).context("invalid .xcstrings JSON")?;
    if !root.is_object() {
        anyhow::bail!("top level of .xcstrings must be an object");
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    Ok(Document {
        root,
        trailing_newline: text.ends_with('\n'),
        newline,
    })
}

/// Serialize back in Xcode's exact style.
pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    write_value(&mut out, &doc.root, 0);
    if doc.trailing_newline {
        out.push('\n');
    }
    if doc.newline == "\r\n" {
        out = out.replace('\n', "\r\n");
    }
    out
}

const INDENT: &str = "  ";

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

fn write_value(out: &mut String, v: &Value, depth: usize) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let _ = write!(out, "{n}");
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[\n\n");
                push_indent(out, depth);
                out.push(']');
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                push_indent(out, depth + 1);
                write_value(out, item, depth + 1);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, depth);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{\n\n");
                push_indent(out, depth);
                out.push('}');
                return;
            }
            out.push_str("{\n");
            let n = map.len();
            for (i, (k, item)) in map.iter().enumerate() {
                push_indent(out, depth + 1);
                write_string(out, k);
                out.push_str(" : ");
                write_value(out, item, depth + 1);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, depth);
            out.push('}');
        }
    }
}

/// Foundation-style JSON string escaping: quotes, backslashes and control
/// characters only. Slashes and non-ASCII are written raw.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

use crate::core::Unit;
use std::collections::BTreeMap;

/// Extract translatable units. Keys with plural/device `variations` in the source
/// language are skipped for now (handled by a later gate); everything with a plain
/// `stringUnit` — or no source localization at all, where the key is the text — is a unit.
pub fn units(doc: &Document, source_locale: &str) -> Vec<Unit> {
    let mut out = Vec::new();
    let Some(strings) = doc.root.get("strings").and_then(Value::as_object) else {
        return out;
    };
    for (key, entry) in strings {
        let locs = entry.get("localizations").and_then(Value::as_object);
        let src_loc = locs.and_then(|l| l.get(source_locale));
        if src_loc.is_some_and(|l| l.get("variations").is_some()) {
            continue;
        }
        if entry.get("shouldTranslate").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let source = src_loc
            .and_then(|l| l.get("stringUnit"))
            .and_then(|u| u.get("value"))
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string();
        let mut translations = BTreeMap::new();
        if let Some(locs) = locs {
            for (locale, l) in locs {
                if locale == source_locale {
                    continue;
                }
                let value = l
                    .get("stringUnit")
                    .and_then(|u| u.get("value"))
                    .and_then(Value::as_str);
                if let Some(v) = value
                    && !v.is_empty()
                {
                    translations.insert(locale.clone(), v.to_string());
                }
            }
        }
        out.push(Unit {
            key: key.clone(),
            source,
            comment: entry
                .get("comment")
                .and_then(Value::as_str)
                .map(str::to_string),
            translations,
            locales: None,
        });
    }
    out
}

/// Set (or add) the translation of `key` in `locale`. Locale keys are kept in
/// Xcode's sorted order; `stringUnit` is written as `state` then `value`.
pub fn set_translation(doc: &mut Document, key: &str, locale: &str, text: &str) -> bool {
    let Some(entry) = doc
        .root
        .get_mut("strings")
        .and_then(Value::as_object_mut)
        .and_then(|s| s.get_mut(key))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    if !entry.contains_key("localizations") {
        // Xcode orders entry fields alphabetically: comment, extractionState, localizations, ...
        let mut rebuilt = serde_json::Map::new();
        let mut inserted = false;
        for (k, v) in std::mem::take(entry) {
            if !inserted && k.as_str() > "localizations" {
                rebuilt.insert("localizations".into(), Value::Object(Default::default()));
                inserted = true;
            }
            rebuilt.insert(k, v);
        }
        if !inserted {
            rebuilt.insert("localizations".into(), Value::Object(Default::default()));
        }
        *entry = rebuilt;
    }
    let locs = entry
        .get_mut("localizations")
        .and_then(Value::as_object_mut)
        .expect("just ensured");
    let mut unit = serde_json::Map::new();
    unit.insert("state".into(), Value::String("translated".into()));
    unit.insert("value".into(), Value::String(text.to_string()));
    let mut loc_obj = serde_json::Map::new();
    loc_obj.insert("stringUnit".into(), Value::Object(unit));
    if let Some(existing) = locs.get_mut(locale) {
        *existing = Value::Object(loc_obj);
        return true;
    }
    let mut rebuilt = serde_json::Map::new();
    let mut inserted = false;
    for (k, v) in std::mem::take(locs) {
        if !inserted && k.as_str() > locale {
            rebuilt.insert(locale.to_string(), Value::Object(loc_obj.clone()));
            inserted = true;
        }
        rebuilt.insert(k, v);
    }
    if !inserted {
        rebuilt.insert(locale.to_string(), Value::Object(loc_obj));
    }
    *locs = rebuilt;
    true
}

// ---- plural variations ----------------------------------------------------------------

/// `(plural key, source forms)` for every plural in the catalog: top-level
/// `variations.plural` keyed as `key`, substitution plurals keyed as `key#name`.
fn plural_sources(doc: &Document, source_locale: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let mut out = Vec::new();
    let Some(strings) = doc.root.get("strings").and_then(Value::as_object) else {
        return out;
    };
    for (key, entry) in strings {
        if entry.get("shouldTranslate").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let Some(src) = entry.pointer(&format!("/localizations/{source_locale}")) else {
            continue;
        };
        if let Some(forms) = plural_forms_of(src) {
            out.push((key.clone(), forms));
        }
        if let Some(subs) = src.get("substitutions").and_then(Value::as_object) {
            for (name, sub) in subs {
                if let Some(forms) = plural_forms_of(sub) {
                    out.push((format!("{key}#{name}"), forms));
                }
            }
        }
    }
    out
}

fn plural_forms_of(node: &Value) -> Option<BTreeMap<String, String>> {
    let p = node.pointer("/variations/plural")?.as_object()?;
    let forms: BTreeMap<String, String> = p
        .iter()
        .filter_map(|(cat, v)| {
            v.pointer("/stringUnit/value")
                .and_then(Value::as_str)
                .map(|s| (cat.clone(), s.to_string()))
        })
        .collect();
    (!forms.is_empty()).then_some(forms)
}

/// Existing plural forms of `plural_key` (`key` or `key#substitution`) in `locale`.
fn plural_forms_in(doc: &Document, plural_key: &str, locale: &str) -> BTreeMap<String, String> {
    let (key, sub) = match plural_key.rsplit_once('#') {
        Some((k, s)) if doc.root.pointer(&format!("/strings/{}", ptr(k))).is_some() => (k, Some(s)),
        _ => (plural_key, None),
    };
    let mut path = format!("/strings/{}/localizations/{}", ptr(key), ptr(locale));
    if let Some(s) = sub {
        path.push_str(&format!("/substitutions/{}", ptr(s)));
    }
    doc.root
        .pointer(&path)
        .and_then(plural_forms_of)
        .unwrap_or_default()
}

fn ptr(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

/// One unit per plural category every target locale needs (see `core::plural_units`).
pub fn plural_units(doc: &Document, source_locale: &str, targets: &[String]) -> Vec<Unit> {
    let mut out = Vec::new();
    for (pkey, forms) in plural_sources(doc, source_locale) {
        let comment = doc
            .root
            .pointer(&format!(
                "/strings/{}/comment",
                ptr(pkey.split('#').next().unwrap())
            ))
            .and_then(Value::as_str)
            .map(str::to_string);
        let per_locale: BTreeMap<String, (Vec<String>, BTreeMap<String, String>)> = targets
            .iter()
            .map(|l| {
                let need = crate::check::plurals::required(l)
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect();
                (l.clone(), (need, plural_forms_in(doc, &pkey, l)))
            })
            .collect();
        out.extend(crate::core::plural_units(
            &pkey,
            comment.as_deref(),
            &forms,
            &per_locale,
        ));
    }
    out
}

/// Write one plural form. For a substitution plural (`key#name`) the substitution's
/// `argNumber`/`formatSpecifier` are copied from the source locale, and the outer
/// `stringUnit` is left to the ordinary translation of `key`.
pub fn set_plural_form(
    doc: &mut Document,
    source_locale: &str,
    plural_key: &str,
    locale: &str,
    category: &str,
    text: &str,
) -> bool {
    let (key, sub) = match plural_key.rsplit_once('#') {
        Some((k, s)) if doc.root.pointer(&format!("/strings/{}", ptr(k))).is_some() => {
            (k.to_string(), Some(s.to_string()))
        }
        _ => (plural_key.to_string(), None),
    };
    // Substitution metadata to mirror (source is read before the mutable borrow).
    let meta: Vec<(String, Value)> = match &sub {
        Some(s) => doc
            .root
            .pointer(&format!(
                "/strings/{}/localizations/{}/substitutions/{}",
                ptr(&key),
                ptr(source_locale),
                ptr(s)
            ))
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter(|(k, _)| k.as_str() != "variations")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        None => vec![],
    };
    let Some(entry) = doc
        .root
        .get_mut("strings")
        .and_then(Value::as_object_mut)
        .and_then(|s| s.get_mut(&key))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let locs = sorted_entry(entry, "localizations");
    let loc = sorted_entry(locs, locale);
    let holder = match &sub {
        Some(s) => {
            let subs = sorted_entry(loc, "substitutions");
            let node = sorted_entry(subs, s);
            for (k, v) in meta {
                if !node.contains_key(&k) {
                    node.insert(k, v);
                }
            }
            sort_keys(node);
            node
        }
        None => loc,
    };
    let variations = sorted_entry(holder, "variations");
    let plural = sorted_entry(variations, "plural");
    let mut unit = serde_json::Map::new();
    unit.insert("state".into(), Value::String("translated".into()));
    unit.insert("value".into(), Value::String(text.to_string()));
    let mut cat_obj = serde_json::Map::new();
    cat_obj.insert("stringUnit".into(), Value::Object(unit));
    plural.insert(category.to_string(), Value::Object(cat_obj));
    sort_keys(plural);
    true
}

/// Get-or-insert an object-valued child, keeping keys in Xcode's sorted order.
fn sorted_entry<'a>(
    obj: &'a mut serde_json::Map<String, Value>,
    key: &str,
) -> &'a mut serde_json::Map<String, Value> {
    if !obj.get(key).is_some_and(Value::is_object) {
        obj.insert(key.to_string(), Value::Object(Default::default()));
        sort_keys(obj);
    }
    obj.get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("just inserted")
}

fn sort_keys(obj: &mut serde_json::Map<String, Value>) {
    let mut items: Vec<(String, Value)> = std::mem::take(obj).into_iter().collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in items {
        obj.insert(k, v);
    }
}
