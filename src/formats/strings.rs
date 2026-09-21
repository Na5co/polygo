//! Apple `.strings` files (`en.lproj/Localizable.strings`): `"key" = "value";` with
//! `/* comments */`, the format every iOS app used before String Catalogs and most still
//! ship. Span-based: only edited values are spliced; new entries are appended. Files are
//! often UTF-16 with a BOM; the encoding is kept and written back the same way.

use crate::core::Unit;
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

/// Decode a `.strings` file's bytes, honouring a UTF-16 BOM.
pub fn decode_file(bytes: &[u8]) -> Result<(String, Encoding)> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Ok((String::from_utf16(&units)?, Encoding::Utf16Le));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return Ok((String::from_utf16(&units)?, Encoding::Utf16Be));
    }
    Ok((String::from_utf8(bytes.to_vec())?, Encoding::Utf8))
}

pub fn encode_file(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            out
        }
        Encoding::Utf16Be => {
            let mut out = vec![0xFE, 0xFF];
            out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
            out
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    value: String,
    /// The `/* … */` or `//` comment right before the entry.
    pub comment: Option<String>,
    /// Span of the value between its quotes.
    span: Range<usize>,
    /// Where the entry starts in the text: its comment if it has one, else its key.
    start: usize,
    edited: Option<String>,
}

/// Where `insert` puts a new entry.
#[derive(Debug, Clone, Copy)]
pub enum Place {
    After(usize),
    Before(usize),
    End,
}

impl Entry {
    pub fn text(&self) -> String {
        self.edited.clone().unwrap_or_else(|| self.value.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    text: String,
    pub encoding: Encoding,
    pub entries: Vec<Entry>,
}

pub fn parse(text: &str) -> Result<Document> {
    parse_with(text, Encoding::Utf8)
}

pub fn parse_with(text: &str, encoding: Encoding) -> Result<Document> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut entries = Vec::new();
    let mut comment: Option<String> = None;
    let mut comment_start: Option<usize> = None;
    let line_of = |at: usize| text[..at].matches('\n').count() + 1;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map(|e| i + 2 + e).ok_or_else(|| {
                    anyhow::anyhow!("unterminated comment at line {}", line_of(i))
                })?;
                comment = Some(text[i + 2..end].trim().to_string());
                comment_start.get_or_insert(i);
                i = end + 2;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = text[i..].find('\n').map_or(b.len(), |e| i + e);
                comment = Some(text[i + 2..end].trim().to_string());
                comment_start.get_or_insert(i);
                i = end;
            }
            b'"' | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' => {
                let start = comment_start.take().unwrap_or(i);
                let (key, next) = if b[i] == b'"' {
                    let (k, end) = quoted(text, i)?;
                    (k, end)
                } else {
                    let end = i + text[i..]
                        .find(|c: char| !(c.is_ascii_alphanumeric() || "_.-".contains(c)))
                        .unwrap_or(text.len() - i);
                    (text[i..end].to_string(), end)
                };
                i = skip_ws(text, next);
                if b.get(i) != Some(&b'=') {
                    bail!("expected `=` after key {key:?} at line {}", line_of(i));
                }
                i = skip_ws(text, i + 1);
                if b.get(i) != Some(&b'"') {
                    bail!("expected a quoted value for {key:?} at line {}", line_of(i));
                }
                let (value, end) = quoted(text, i)?;
                let span = i + 1..end - 1;
                i = skip_ws(text, end);
                if b.get(i) != Some(&b';') {
                    bail!("expected `;` after {key:?} at line {}", line_of(i));
                }
                i += 1;
                entries.push(Entry {
                    key,
                    value,
                    comment: comment.take(),
                    span,
                    start,
                    edited: None,
                });
            }
            other => bail!("unexpected `{}` at line {}", other as char, line_of(i)),
        }
    }
    Ok(Document {
        text: text.to_string(),
        encoding,
        entries,
    })
}

fn skip_ws(text: &str, mut i: usize) -> usize {
    let b = text.as_bytes();
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Decode a quoted string starting at the opening quote; returns the value and the
/// index just past the closing quote.
fn quoted(text: &str, start: usize) -> Result<(String, usize)> {
    let mut out = String::new();
    let mut chars = text[start + 1..].char_indices();
    while let Some((off, c)) = chars.next() {
        match c {
            '"' => return Ok((out, start + 1 + off + 1)),
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some('0') => out.push('\0'),
                Some('U') | Some('u') => {
                    let hex: String = (0..4)
                        .filter_map(|_| chars.next().map(|(_, c)| c))
                        .collect();
                    match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        Some(ch) => out.push(ch),
                        None => bail!("bad \\U escape `{hex}`"),
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => bail!("unterminated string"),
            },
            c => out.push(c),
        }
    }
    bail!("unterminated string starting at byte {start}")
}

pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn encode_key(key: &str) -> String {
    format!("\"{}\"", encode(key))
}

impl Document {
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.key == key)
    }

    pub fn set_text(&mut self, i: usize, text: &str) {
        self.entries[i].edited = Some(text.to_string());
    }

    /// Insert `"key" = "value";` (with the source's comment) at `place`, so a locale
    /// file can keep the source file's order.
    pub fn insert(&mut self, key: &str, text: &str, comment: Option<&str>, place: Place) {
        let nl = if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let at = match place {
            Place::After(i) => {
                let semi = self.text[self.entries[i].span.end..]
                    .find(';')
                    .map_or(self.text.len(), |p| self.entries[i].span.end + p + 1);
                self.text[semi..]
                    .find('\n')
                    .map_or(self.text.len(), |p| semi + p + 1)
            }
            Place::Before(j) => {
                let s = self.entries[j].start;
                self.text[..s].rfind('\n').map_or(0, |p| p + 1)
            }
            Place::End => self.text.len(),
        };
        let mut block = String::new();
        if at == self.text.len() && !self.text.is_empty() && !self.text.ends_with('\n') {
            block.push_str(nl);
        }
        let entry_start = at + block.len();
        if let Some(c) = comment {
            block.push_str(&format!("/* {c} */{nl}"));
        }
        let value = encode(text);
        let prefix = format!("{} = \"", encode_key(key));
        let vstart = at + block.len() + prefix.len();
        block.push_str(&prefix);
        block.push_str(&value);
        block.push_str(&format!("\";{nl}"));
        let shift = block.len();
        for e in &mut self.entries {
            if e.start >= at {
                e.start += shift;
                e.span.start += shift;
                e.span.end += shift;
            }
        }
        self.text.insert_str(at, &block);
        let entry = Entry {
            key: key.to_string(),
            value: text.to_string(),
            comment: comment.map(str::to_string),
            span: vstart..vstart + value.len(),
            start: entry_start,
            edited: None,
        };
        match place {
            Place::After(i) => self.entries.insert(i + 1, entry),
            Place::Before(j) => self.entries.insert(j, entry),
            Place::End => self.entries.push(entry),
        }
    }
}

pub fn serialize(doc: &Document) -> String {
    let mut edits: Vec<(Range<usize>, String)> = doc
        .entries
        .iter()
        .filter_map(|e| e.edited.as_ref().map(|t| (e.span.clone(), encode(t))))
        .collect();
    if edits.is_empty() {
        return doc.text.clone();
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = doc.text.clone();
    for (r, s) in edits {
        out.replace_range(r, &s);
    }
    out
}

pub fn units(doc: &Document) -> Vec<Unit> {
    doc.entries
        .iter()
        .map(|e| Unit {
            key: e.key.clone(),
            source: e.text(),
            comment: e.comment.clone(),
            translations: BTreeMap::new(),
            locales: None,
        })
        .collect()
}

pub fn values(doc: &Document) -> BTreeMap<String, String> {
    doc.entries
        .iter()
        .filter(|e| !e.text().is_empty())
        .map(|e| (e.key.clone(), e.text()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_escapes_and_bare_keys() {
        let src = "/* Title */\n\"hello\" = \"Hello, \\\"world\\\"\\n\";\n\n// menu\nopen.item = \"Open \\U00e9\";\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.entries.len(), 2);
        assert_eq!(doc.entries[0].key, "hello");
        assert_eq!(doc.entries[0].text(), "Hello, \"world\"\n");
        assert_eq!(doc.entries[0].comment.as_deref(), Some("Title"));
        assert_eq!(doc.entries[1].key, "open.item");
        assert_eq!(doc.entries[1].text(), "Open é");
        assert_eq!(doc.entries[1].comment.as_deref(), Some("menu"));
        assert_eq!(serialize(&doc), src);
    }

    #[test]
    fn edits_splice_and_inserts_append() {
        let src = "\"a\" = \"A\";\n\"b\" = \"B\";\n";
        let mut doc = parse(src).unwrap();
        doc.set_text(0, "Ä \"q\"");
        doc.insert("c", "C\nD", Some("third"), Place::End);
        doc.insert("a2", "after a", None, Place::After(0));
        let out = serialize(&doc);
        assert_eq!(
            out,
            "\"a\" = \"Ä \\\"q\\\"\";\n\"a2\" = \"after a\";\n\"b\" = \"B\";\n/* third */\n\"c\" = \"C\\nD\";\n"
        );
        let again = parse(&out).unwrap();
        assert_eq!(again.entries[3].text(), "C\nD");
        assert_eq!(serialize(&again), out);
        // Edits recorded before an insert still land on the right value.
        doc.set_text(2, "Bee");
        assert!(serialize(&doc).contains("\"b\" = \"Bee\";"));
        // Before an entry that has a comment: the new one goes above the comment.
        doc.insert("b0", "pre", None, Place::Before(3));
        let out = serialize(&doc);
        assert!(
            out.contains("\"b\" = \"Bee\";\n\"b0\" = \"pre\";\n/* third */\n\"c\""),
            "{out}"
        );
        assert_eq!(serialize(&parse(&out).unwrap()), out);
    }

    #[test]
    fn utf16_round_trips_with_bom() {
        let text = "\"a\" = \"Ä\";\n";
        for enc in [Encoding::Utf16Le, Encoding::Utf16Be] {
            let bytes = encode_file(text, enc);
            let (decoded, e) = decode_file(&bytes).unwrap();
            assert_eq!((decoded.as_str(), e), (text, enc));
            assert_eq!(encode_file(&decoded, e), bytes);
        }
    }
}
