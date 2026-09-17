//! gettext `.po` files.
//!
//! Span-based like the Android parser: the original text is kept and only the
//! `msgstr` value regions that were edited get spliced. Entries are keyed by
//! `msgctxt\u{4}msgid` (the gettext convention). Plural entries (`msgid_plural`)
//! are preserved untouched and not exposed as units for now.

use crate::core::Unit;
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub ctxt: Option<String>,
    pub msgid: String,
    pub plural: Option<String>,
    /// Decoded `msgstr` (singular) or `msgstr[0]`.
    pub msgstr: String,
    /// `#.` extracted comments and `#:` references, joined.
    pub comment: Option<String>,
    /// Span of the singular msgstr value region (from its first quote to the end of its last line).
    msgstr_span: Range<usize>,
    edited: Option<String>,
    is_plural: bool,
}

impl Entry {
    pub fn key(&self) -> String {
        match &self.ctxt {
            Some(c) => format!("{c}\u{4}{}", self.msgid),
            None => self.msgid.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    text: String,
    pub entries: Vec<Entry>,
    /// Decoded header (the `msgid ""` entry's msgstr), e.g. `Language: de\n...`.
    pub header: String,
}

pub fn parse(text: &str) -> Result<Document> {
    let mut entries = Vec::new();
    let mut header = String::new();
    let mut i = 0usize;
    let lines: Vec<(usize, &str)> = line_offsets(text);
    let n = lines.len();
    while i < n {
        // Skip blank lines.
        if lines[i].1.trim().is_empty() {
            i += 1;
            continue;
        }
        // Collect comments.
        let mut comments: Vec<String> = Vec::new();
        let mut obsolete = false;
        while i < n && lines[i].1.starts_with('#') {
            let l = lines[i].1;
            if l.starts_with("#~") {
                obsolete = true;
            } else if let Some(c) = l.strip_prefix("#.") {
                comments.push(c.trim().to_string());
            } else if let Some(r) = l.strip_prefix("#:") {
                comments.push(format!("refs: {}", r.trim()));
            }
            i += 1;
        }
        if obsolete {
            while i < n && !lines[i].1.trim().is_empty() {
                i += 1;
            }
            continue;
        }
        if i >= n || lines[i].1.trim().is_empty() {
            continue;
        }
        let mut ctxt = None;
        let mut msgid: Option<String> = None;
        let mut plural = None;
        let mut msgstr: Option<(String, Range<usize>)> = None;
        let mut msgstr_plural_first: Option<(String, Range<usize>)> = None;
        while i < n && !lines[i].1.trim().is_empty() {
            let (off, l) = lines[i];
            let (kw, rest) = match l.find(' ') {
                Some(p) if l.starts_with("msg") => (&l[..p], &l[p + 1..]),
                _ => {
                    if l.starts_with('"') {
                        bail!("stray continuation line at byte {off}");
                    }
                    bail!("unexpected line at byte {off}: {l}");
                }
            };
            // Value region: from the first quote on this line through all continuation lines.
            let q = off + (l.len() - rest.len()) + rest.find('"').unwrap_or(0);
            let mut end = off + l.len();
            let mut value = decode_quoted(rest.trim());
            let mut j = i + 1;
            while j < n && lines[j].1.trim_start().starts_with('"') {
                value.push_str(&decode_quoted(lines[j].1.trim()));
                end = lines[j].0 + lines[j].1.len();
                j += 1;
            }
            match kw {
                "msgctxt" => ctxt = Some(value),
                "msgid" => msgid = Some(value),
                "msgid_plural" => plural = Some(value),
                "msgstr" => msgstr = Some((value, q..end)),
                _ if kw.starts_with("msgstr[") => {
                    if kw == "msgstr[0]" {
                        msgstr_plural_first = Some((value, q..end));
                    }
                }
                _ => bail!("unknown keyword {kw} at byte {off}"),
            }
            i = j;
        }
        let Some(msgid) = msgid else { continue };
        let is_plural = plural.is_some();
        let (msgstr_text, span) = match (msgstr, msgstr_plural_first) {
            (Some(s), _) => s,
            (None, Some(s)) => s,
            (None, None) => bail!("entry without msgstr: {msgid:?}"),
        };
        if msgid.is_empty() && ctxt.is_none() && header.is_empty() {
            header = msgstr_text.clone();
        }
        entries.push(Entry {
            ctxt,
            msgid,
            plural,
            msgstr: msgstr_text,
            comment: if comments.is_empty() {
                None
            } else {
                Some(comments.join(" · "))
            },
            msgstr_span: span,
            edited: None,
            is_plural,
        });
    }
    Ok(Document {
        text: text.to_string(),
        entries,
        header,
    })
}

fn line_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut off = 0;
    for l in text.split_inclusive('\n') {
        let trimmed = l.strip_suffix('\n').unwrap_or(l);
        let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
        out.push((off, trimmed));
        off += l.len();
    }
    out
}

/// Decode a single PO quoted string (`"..."`) into text.
pub fn decode_quoted(s: &str) -> String {
    let inner = s.trim().trim_start_matches('"').trim_end_matches('"');
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Encode text in gettext style: one line, or `""` + one quoted line per source line.
pub fn encode(text: &str) -> String {
    let esc = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\t', "\\t")
    };
    if !text.contains('\n') {
        return format!("\"{}\"", esc(text));
    }
    let mut out = String::from("\"\"");
    let mut rest = text;
    while let Some(i) = rest.find('\n') {
        out.push_str(&format!("\n\"{}\\n\"", esc(&rest[..i])));
        rest = &rest[i + 1..];
    }
    if !rest.is_empty() {
        out.push_str(&format!("\n\"{}\"", esc(rest)));
    }
    out
}

impl Document {
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.key() == key)
    }

    pub fn set_msgstr(&mut self, i: usize, text: &str) {
        let e = &mut self.entries[i];
        e.edited = Some(text.to_string());
        e.msgstr = text.to_string();
    }

    /// Append a new singular entry at the end of the file.
    pub fn insert(&mut self, ctxt: Option<&str>, msgid: &str, msgstr: &str, comment: Option<&str>) {
        if !self.text.ends_with('\n') && !self.text.is_empty() {
            self.text.push('\n');
        }
        let mut block = String::from("\n");
        if let Some(c) = comment {
            for line in c.split(" · ") {
                if let Some(r) = line.strip_prefix("refs: ") {
                    block.push_str(&format!("#: {r}\n"));
                } else {
                    block.push_str(&format!("#. {line}\n"));
                }
            }
        }
        if let Some(c) = ctxt {
            block.push_str(&format!("msgctxt {}\n", encode(c)));
        }
        block.push_str(&format!("msgid {}\n", encode(msgid)));
        let value_start = self.text.len() + block.len() + "msgstr ".len();
        let enc = encode(msgstr);
        block.push_str(&format!("msgstr {enc}\n"));
        let value_end = value_start + enc.len();
        self.text.push_str(&block);
        self.entries.push(Entry {
            ctxt: ctxt.map(str::to_string),
            msgid: msgid.to_string(),
            plural: None,
            msgstr: msgstr.to_string(),
            comment: comment.map(str::to_string),
            msgstr_span: value_start..value_end,
            edited: None,
            is_plural: false,
        });
    }
}

pub fn serialize(doc: &Document) -> String {
    let mut edits: Vec<(Range<usize>, String)> = doc
        .entries
        .iter()
        .filter_map(|e| {
            e.edited
                .as_ref()
                .map(|t| (e.msgstr_span.clone(), encode(t)))
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

/// Units from a template/source file: singular entries with a non-empty msgid.
pub fn units(doc: &Document) -> Vec<Unit> {
    doc.entries
        .iter()
        .filter(|e| !e.msgid.is_empty() && !e.is_plural)
        .map(|e| Unit {
            key: e.key(),
            source: e.msgid.clone(),
            comment: e.comment.clone(),
            translations: BTreeMap::new(),
        })
        .collect()
}

/// `key → msgstr` for translated singular entries of a locale file.
pub fn values(doc: &Document) -> BTreeMap<String, String> {
    doc.entries
        .iter()
        .filter(|e| !e.msgid.is_empty() && !e.is_plural && !e.msgstr.is_empty())
        .map(|e| (e.key(), e.msgstr.clone()))
        .collect()
}

/// A new locale file: the source header with `Language:` and `Plural-Forms:` set, no entries.
pub fn new_locale_file(source: &Document, locale: &str) -> String {
    let mut header_lines: Vec<String> = Vec::new();
    let mut saw_lang = false;
    let mut saw_plural = false;
    for line in source.header.lines() {
        if line.starts_with("Language:") {
            header_lines.push(format!("Language: {locale}"));
            saw_lang = true;
        } else if line.starts_with("Plural-Forms:") {
            header_lines.push(format!("Plural-Forms: {}", plural_forms(locale)));
            saw_plural = true;
        } else {
            header_lines.push(line.to_string());
        }
    }
    if !saw_lang {
        header_lines.push(format!("Language: {locale}"));
    }
    if !saw_plural {
        header_lines.push(format!("Plural-Forms: {}", plural_forms(locale)));
    }
    let mut out = String::from("msgid \"\"\nmsgstr \"\"\n");
    for l in header_lines {
        out.push_str(&format!("\"{}\\n\"\n", l.replace('"', "\\\"")));
    }
    out
}

/// gettext plural rule for a locale.
pub fn plural_forms(locale: &str) -> &'static str {
    let lang = locale.split(['-', '_']).next().unwrap_or("");
    match lang {
        "ja" | "ko" | "zh" | "vi" | "th" | "id" | "ms" | "tr" => "nplurals=1; plural=0;",
        "fr" | "pt" => "nplurals=2; plural=(n > 1);",
        "ru" | "uk" | "be" => {
            "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);"
        }
        "pl" => {
            "nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);"
        }
        "cs" | "sk" => "nplurals=3; plural=(n==1) ? 0 : (n>=2 && n<=4) ? 1 : 2;",
        "sl" => "nplurals=4; plural=(n%100==1 ? 0 : n%100==2 ? 1 : n%100==3 || n%100==4 ? 2 : 3);",
        "ar" => {
            "nplurals=6; plural=(n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5);"
        }
        "ro" => "nplurals=3; plural=(n==1 ? 0 : (n==0 || (n%100 > 0 && n%100 < 20)) ? 1 : 2);",
        "lt" => {
            "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && (n%100<10 || n%100>=20) ? 1 : 2);"
        }
        "lv" => "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n != 0 ? 1 : 2);",
        "he" => "nplurals=2; plural=(n != 1);",
        "hr" | "sr" | "bs" => {
            "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);"
        }
        _ => "nplurals=2; plural=(n != 1);",
    }
}
