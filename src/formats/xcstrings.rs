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
        });
    }
    out
}
