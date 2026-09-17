//! .NET `.resx` / `.resw` resource files: `<data name="Key"><value>Text</value></data>`.
//! Span-based: only edited `<value>` contents are spliced; new entries are appended
//! before `</root>` in the file's indentation style.

use crate::core::Unit;
use anyhow::{Context, Result, bail};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// Raw `<value>` content (entities encoded).
    pub raw: String,
    pub comment: Option<String>,
    span: Option<Range<usize>>,
    edited: Option<String>,
}

impl Entry {
    pub fn text(&self) -> String {
        match &self.edited {
            Some(t) => t.clone(),
            None => decode(&self.raw),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    text: String,
    pub entries: Vec<Entry>,
}

pub fn parse(text: &str) -> Result<Document> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;
    let mut entries = Vec::new();
    loop {
        match reader.read_event().context("XML syntax error")? {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == b"data" => {
                let mut name = None;
                let mut binary = false;
                for a in e.attributes() {
                    let a = a.context("bad attribute")?;
                    match a.key.as_ref() {
                        b"name" => name = Some(a.unescape_value()?.into_owned()),
                        b"type" | b"mimetype" => binary = true,
                        _ => {}
                    }
                }
                let Some(name) = name else {
                    bail!("<data> without name")
                };
                // Walk the children until </data>.
                let mut raw = String::new();
                let mut span = None;
                let mut comment = None;
                loop {
                    match reader.read_event().context("XML syntax error")? {
                        Event::Eof => bail!("unexpected EOF inside <data name=\"{name}\">"),
                        Event::End(e) if e.name().as_ref() == b"data" => break,
                        Event::Start(e) if e.name().as_ref() == b"value" => {
                            let start = reader.buffer_position() as usize;
                            let end = skip_to_end(&mut reader, text, b"value")?;
                            raw = text[start..end].to_string();
                            span = Some(start..end);
                        }
                        Event::Empty(e) if e.name().as_ref() == b"value" => {
                            raw = String::new();
                        }
                        Event::Start(e) if e.name().as_ref() == b"comment" => {
                            let start = reader.buffer_position() as usize;
                            let end = skip_to_end(&mut reader, text, b"comment")?;
                            comment = Some(decode(&text[start..end]));
                        }
                        _ => {}
                    }
                }
                if !binary {
                    entries.push(Entry {
                        name,
                        raw,
                        comment,
                        span,
                        edited: None,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(Document {
        text: text.to_string(),
        entries,
    })
}

fn skip_to_end(reader: &mut Reader<&[u8]>, text: &str, tag: &[u8]) -> Result<usize> {
    loop {
        match reader.read_event().context("XML syntax error")? {
            Event::Eof => bail!("unexpected EOF"),
            Event::End(e) if e.name().as_ref() == tag => {
                let after = reader.buffer_position() as usize;
                return text[..after].rfind("</").context("end tag");
            }
            _ => {}
        }
    }
}

pub fn decode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn encode(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Document {
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    pub fn set_text(&mut self, i: usize, text: &str) {
        self.entries[i].edited = Some(text.to_string());
    }

    /// Append `<data name="..." xml:space="preserve"><value>..</value></data>` before `</root>`.
    pub fn insert(&mut self, name: &str, text: &str) {
        let indent = self.detect_indent();
        let close = self.text.rfind("</root>").unwrap_or(self.text.len());
        let line_start = self.text[..close].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let at = if self.text[line_start..close].trim().is_empty() {
            line_start
        } else {
            close
        };
        let nl = if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let block = format!(
            "{indent}<data name=\"{}\" xml:space=\"preserve\">{nl}{indent}{indent}<value>{}</value>{nl}{indent}</data>{nl}",
            name.replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;"),
            encode(text)
        );
        let shift = block.len();
        for e in &mut self.entries {
            if let Some(s) = &mut e.span
                && s.start >= at
            {
                s.start += shift;
                s.end += shift;
            }
        }
        self.text.insert_str(at, &block);
        self.entries.push(Entry {
            name: name.to_string(),
            raw: encode(text),
            comment: None,
            span: None,
            edited: None,
        });
    }

    fn detect_indent(&self) -> String {
        self.text
            .lines()
            .find(|l| {
                l.trim_start().starts_with("<data ") || l.trim_start().starts_with("<resheader ")
            })
            .map(|l| l.chars().take_while(|c| c.is_whitespace()).collect())
            .unwrap_or_else(|| "  ".to_string())
    }
}

pub fn serialize(doc: &Document) -> String {
    let mut edits: Vec<(Range<usize>, String)> = doc
        .entries
        .iter()
        .filter_map(|e| match (&e.edited, &e.span) {
            (Some(t), Some(s)) => Some((s.clone(), encode(t))),
            _ => None,
        })
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
            key: e.name.clone(),
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
        .map(|e| (e.name.clone(), e.text()))
        .collect()
}

/// A new locale file: the source file with every `<data>` element removed (keeps the
/// XML declaration, schema and resheaders Visual Studio expects).
pub fn new_locale_file(source_text: &str) -> String {
    let mut out = String::with_capacity(source_text.len());
    let mut rest = source_text;
    while let Some(i) = rest.find("<data ") {
        // Back up to the start of the line so the indentation goes too.
        let line_start = rest[..i].rfind('\n').map(|p| p + 1).unwrap_or(i);
        out.push_str(&rest[..line_start]);
        let Some(end) = rest[i..].find("</data>") else {
            out.push_str(&rest[line_start..]);
            return out;
        };
        let mut after = i + end + "</data>".len();
        if rest[after..].starts_with("\r\n") {
            after += 2;
        } else if rest[after..].starts_with('\n') {
            after += 1;
        }
        rest = &rest[after..];
    }
    out.push_str(rest);
    out
}
