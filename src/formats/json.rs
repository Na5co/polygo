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
