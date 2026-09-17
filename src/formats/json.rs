//! i18next-style JSON locale files (flat or nested objects of strings).
//!
//! Byte-stability strategy: a small position-tracking JSON parser records the
//! span of every string *value*; serialization replays the original text and
//! splices in edited values only. This survives every formatting quirk in the
//! wild (indent width, key order, mixed `\uXXXX` and raw Unicode, trailing
//! newline) because we never re-emit anything we did not change.

use anyhow::{Context, Result, bail};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Key path from the root; array elements appear as their index.
    pub path: Vec<String>,
    /// Span of the string literal (including quotes) in the original text.
    span: Range<usize>,
    decoded: String,
    edited: Option<String>,
}

impl Entry {
    /// Dotted key, e.g. `menu.edit.undo`. Segments themselves may contain dots
    /// (flat i18next files); use `path` when you need the exact segments.
    pub fn key(&self) -> String {
        self.path.join(".")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    text: String,
    pub entries: Vec<Entry>,
}

impl Document {
    pub fn set_text(&mut self, i: usize, text: &str) {
        let e = &mut self.entries[i];
        e.edited = Some(text.to_string());
        e.decoded = text.to_string();
    }
}

impl Entry {
    /// Decoded text of this entry (original literal, or the edit).
    pub fn text(&self) -> String {
        self.decoded.clone()
    }
}

pub fn parse(text: &str) -> Result<Document> {
    let mut p = Parser {
        s: text.as_bytes(),
        text,
        i: 0,
        entries: Vec::new(),
        path: Vec::new(),
    };
    if text.starts_with('\u{FEFF}') {
        p.i = '\u{FEFF}'.len_utf8();
    }
    p.ws();
    if p.peek() != Some(b'{') {
        bail!("i18n JSON root must be an object");
    }
    p.value()?;
    p.ws();
    if p.i != p.s.len() {
        bail!("trailing characters after JSON document at byte {}", p.i);
    }
    let entries = p
        .entries
        .into_iter()
        .map(|(path, span)| Entry {
            decoded: decode_literal(&text[span.clone()]),
            path,
            span,
            edited: None,
        })
        .collect();
    Ok(Document {
        text: text.to_string(),
        entries,
    })
}

pub fn serialize(doc: &Document) -> String {
    let mut edits: Vec<(Range<usize>, String)> = doc
        .entries
        .iter()
        .filter_map(|e| {
            e.edited
                .as_ref()
                .map(|t| (e.span.clone(), encode_literal(t)))
        })
        .collect();
    if edits.is_empty() {
        return doc.text.clone();
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = doc.text.clone();
    for (range, lit) in edits {
        out.replace_range(range, &lit);
    }
    out
}

/// Decode a JSON string literal (with quotes) to text.
pub fn decode_literal(lit: &str) -> String {
    serde_json::from_str::<String>(lit).unwrap_or_else(|_| lit.trim_matches('"').to_string())
}

/// Encode text as a JSON string literal: escapes `"`, `\` and control characters,
/// leaves Unicode raw and never escapes `/` (matches what prettier/i18next emit).
pub fn encode_literal(text: &str) -> String {
    serde_json::to_string(text).expect("string is always serializable")
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
    entries: Vec<(Vec<String>, Range<usize>)>,
    path: Vec<String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn expect(&mut self, b: u8) -> Result<()> {
        if self.peek() == Some(b) {
            self.i += 1;
            Ok(())
        } else {
            bail!("expected '{}' at byte {}", b as char, self.i)
        }
    }

    fn value(&mut self) -> Result<()> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => {
                let span = self.string()?;
                self.entries.push((self.path.clone(), span));
                Ok(())
            }
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(c) => bail!("unexpected '{}' at byte {}", c as char, self.i),
            None => bail!("unexpected end of input"),
        }
    }

    fn object(&mut self) -> Result<()> {
        self.expect(b'{')?;
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.ws();
            let key_span = self.string().context("object key")?;
            let key = decode_literal(&self.text[key_span]);
            self.ws();
            self.expect(b':')?;
            self.ws();
            self.path.push(key);
            self.value()?;
            self.path.pop();
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => bail!("expected ',' or '}}' at byte {}", self.i),
            }
        }
    }

    fn array(&mut self) -> Result<()> {
        self.expect(b'[')?;
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(());
        }
        let mut idx = 0usize;
        loop {
            self.ws();
            self.path.push(idx.to_string());
            self.value()?;
            self.path.pop();
            idx += 1;
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => bail!("expected ',' or ']' at byte {}", self.i),
            }
        }
    }

    /// Consume a string literal and return its span including quotes.
    fn string(&mut self) -> Result<Range<usize>> {
        let start = self.i;
        self.expect(b'"')?;
        loop {
            match self.peek() {
                None => bail!("unterminated string starting at byte {start}"),
                Some(b'\\') => self.i += 2,
                Some(b'"') => {
                    self.i += 1;
                    return Ok(start..self.i);
                }
                Some(_) => self.i += 1,
            }
        }
    }

    fn literal(&mut self, word: &str) -> Result<()> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(())
        } else {
            bail!("invalid literal at byte {}", self.i)
        }
    }

    fn number(&mut self) -> Result<()> {
        let start = self.i;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.i += 1;
        }
        if self.i == start {
            bail!("invalid number at byte {start}");
        }
        Ok(())
    }
}

/// Formatting facts detected from an existing file, used when a locale file has to be
/// (re)built rather than spliced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub indent: String,
    pub trailing_newline: bool,
}

impl Style {
    pub fn detect(text: &str) -> Style {
        let indent = text
            .lines()
            .skip(1)
            .find(|l| !l.trim().is_empty())
            .map(|l| {
                l.chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "  ".to_string());
        Style {
            indent,
            trailing_newline: text.ends_with('\n'),
        }
    }
}

/// Pretty-print a tree in the common i18next/prettier style: `"key": value`,
/// nested indentation, raw Unicode, no escaped slashes.
pub fn render(tree: &serde_json::Value, style: &Style) -> String {
    let mut out = String::new();
    render_value(&mut out, tree, &style.indent, 0);
    if style.trailing_newline {
        out.push('\n');
    }
    out
}

fn render_value(out: &mut String, v: &serde_json::Value, indent: &str, depth: usize) {
    use serde_json::Value;
    let pad = |out: &mut String, d: usize| {
        for _ in 0..d {
            out.push_str(indent);
        }
    };
    match v {
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            let n = map.len();
            for (i, (k, item)) in map.iter().enumerate() {
                pad(out, depth + 1);
                out.push_str(&encode_literal(k));
                out.push_str(": ");
                render_value(out, item, indent, depth + 1);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, depth);
            out.push('}');
        }
        Value::Object(_) => out.push_str("{}"),
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            let n = items.len();
            for (i, item) in items.iter().enumerate() {
                pad(out, depth + 1);
                render_value(out, item, indent, depth + 1);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, depth);
            out.push(']');
        }
        Value::Array(_) => out.push_str("[]"),
        Value::String(s) => out.push_str(&encode_literal(s)),
        other => out.push_str(&other.to_string()),
    }
}

/// Build a locale tree shaped like `source`, taking string leaves from `values`
/// (dotted-path → text) and falling back to `existing` for leaves not in `values`.
/// Leaves with no value in either are omitted so i18next falls back to the source.
pub fn build_locale_tree(
    source: &serde_json::Value,
    existing: Option<&serde_json::Value>,
    values: &std::collections::BTreeMap<String, String>,
) -> serde_json::Value {
    fn go(
        node: &serde_json::Value,
        existing: Option<&serde_json::Value>,
        path: &mut Vec<String>,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Option<serde_json::Value> {
        use serde_json::Value;
        match node {
            Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (k, v) in map {
                    path.push(k.clone());
                    let ex = existing.and_then(|e| e.get(k));
                    if let Some(built) = go(v, ex, path, values) {
                        out.insert(k.clone(), built);
                    }
                    path.pop();
                }
                // Keep keys that only exist in the target file (e.g. locale-specific extras).
                if let Some(Value::Object(ex)) = existing {
                    for (k, v) in ex {
                        if !out.contains_key(k) && !map.contains_key(k) {
                            out.insert(k.clone(), v.clone());
                        }
                    }
                }
                if out.is_empty() {
                    None
                } else {
                    Some(Value::Object(out))
                }
            }
            Value::Array(items) => {
                let mut out = Vec::new();
                for (i, v) in items.iter().enumerate() {
                    path.push(i.to_string());
                    let ex = existing.and_then(|e| e.get(i));
                    if let Some(built) = go(v, ex, path, values) {
                        out.push(built);
                    }
                    path.pop();
                }
                if out.is_empty() {
                    None
                } else {
                    Some(Value::Array(out))
                }
            }
            Value::String(_) => {
                let key = path.join(".");
                values
                    .get(&key)
                    .map(|t| Value::String(t.clone()))
                    .or_else(|| existing.filter(|e| e.is_string()).cloned())
            }
            other => Some(other.clone()),
        }
    }
    go(source, existing, &mut Vec::new(), values)
        .unwrap_or(serde_json::Value::Object(Default::default()))
}
