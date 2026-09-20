//! Android resource strings (`res/values/strings.xml`).
//!
//! Byte-stability strategy: keep the original text verbatim and record the byte
//! span of every translatable value (`<string>`, each `<plurals><item>`, each
//! `<string-array><item>`). Serialization replays the original text and splices
//! in only the values that were edited, so untouched files come back identical
//! and edits produce minimal diffs.

use anyhow::{Context, Result, bail};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::fmt::Write as _;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Plurals,
    StringArray,
}

/// One translatable value inside an entry (a string, a plural case, or an array item).
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    /// Raw content exactly as it appears between the tags (escapes, entities, markup, CDATA).
    pub raw: String,
    /// `quantity="..."` for plural items.
    pub quantity: Option<String>,
    /// Byte range of `raw` in the original text, or `None` for an empty element (`<string name="x"/>`).
    span: Option<Range<usize>>,
    /// Byte range of the whole element, used to rewrite empty elements.
    element: Range<usize>,
    /// New raw content if edited.
    edited: Option<String>,
}

impl Value {
    /// Current raw content (edited if set, otherwise original).
    pub fn current_raw(&self) -> &str {
        self.edited.as_deref().unwrap_or(&self.raw)
    }

    /// Human/LLM-facing text: CDATA unwrapped, entities and Android escapes decoded.
    pub fn text(&self) -> String {
        decode(strip_cdata(self.current_raw()))
    }

    fn uses_cdata(&self) -> bool {
        let r = self.raw.trim();
        r.starts_with("<![CDATA[") && r.ends_with("]]>")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    pub name: String,
    /// `translatable="false"` marks strings that must never be translated.
    pub translatable: bool,
    /// The `<!-- comment -->` immediately preceding the element, trimmed.
    pub comment: Option<String>,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    text: String,
    pub entries: Vec<Entry>,
}

impl Document {
    /// Replace the text of `entries[entry].values[idx]`, preserving CDATA wrapping if the
    /// original used it, and encoding Android escapes otherwise.
    pub fn set_text(&mut self, entry: usize, idx: usize, text: &str) {
        let v = &mut self.entries[entry].values[idx];
        let raw = if v.uses_cdata() {
            format!("<![CDATA[{text}]]>")
        } else {
            encode(text)
        };
        v.edited = Some(raw);
    }
}

pub fn parse(text: &str) -> Result<Document> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;
    let mut entries = Vec::new();
    let mut pending_comment: Option<String> = None;
    let mut only_ws_since_comment = true;

    loop {
        let before = reader.buffer_position() as usize;
        match reader.read_event().context("XML syntax error")? {
            Event::Eof => break,
            Event::Comment(c) => {
                let s = String::from_utf8_lossy(&c).trim().to_string();
                pending_comment = Some(s);
                only_ws_since_comment = true;
            }
            Event::Text(t) => {
                if !t.iter().all(|b| b.is_ascii_whitespace()) {
                    only_ws_since_comment = false;
                }
            }
            Event::Start(e) => {
                let comment = if only_ws_since_comment {
                    pending_comment.take()
                } else {
                    None
                };
                pending_comment = None;
                match e.name().as_ref() {
                    b"string" => {
                        let (name, translatable) = name_and_translatable(&e)?;
                        let content_start = reader.buffer_position() as usize;
                        let content_end = skip_to_matching_end(&mut reader, text, b"string")?;
                        entries.push(Entry {
                            kind: Kind::String,
                            name,
                            translatable,
                            comment,
                            values: vec![Value {
                                raw: text[content_start..content_end].to_string(),
                                quantity: None,
                                span: Some(content_start..content_end),
                                element: before..reader.buffer_position() as usize,
                                edited: None,
                            }],
                        });
                    }
                    b"plurals" | b"string-array" => {
                        let kind = if e.name().as_ref() == b"plurals" {
                            Kind::Plurals
                        } else {
                            Kind::StringArray
                        };
                        let (name, translatable) = name_and_translatable(&e)?;
                        let values = read_items(&mut reader, text, e.name().as_ref())?;
                        entries.push(Entry {
                            kind,
                            name,
                            translatable,
                            comment,
                            values,
                        });
                    }
                    _ => {}
                }
                only_ws_since_comment = false;
            }
            Event::Empty(e) => {
                let comment = if only_ws_since_comment {
                    pending_comment.take()
                } else {
                    None
                };
                pending_comment = None;
                if e.name().as_ref() == b"string" {
                    let (name, translatable) = name_and_translatable(&e)?;
                    entries.push(Entry {
                        kind: Kind::String,
                        name,
                        translatable,
                        comment,
                        values: vec![Value {
                            raw: String::new(),
                            quantity: None,
                            span: None,
                            element: before..reader.buffer_position() as usize,
                            edited: None,
                        }],
                    });
                }
                only_ws_since_comment = false;
            }
            _ => {}
        }
    }
    Ok(Document {
        text: text.to_string(),
        entries,
    })
}

/// Read `<item>` children until the parent's end tag.
fn read_items(reader: &mut Reader<&[u8]>, text: &str, parent: &[u8]) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    loop {
        let before = reader.buffer_position() as usize;
        match reader.read_event().context("XML syntax error")? {
            Event::Eof => bail!(
                "unexpected EOF inside <{}>",
                String::from_utf8_lossy(parent)
            ),
            Event::End(e) if e.name().as_ref() == parent => return Ok(values),
            Event::Start(e) if e.name().as_ref() == b"item" => {
                let quantity = attr(&e, b"quantity")?;
                let content_start = reader.buffer_position() as usize;
                let content_end = skip_to_matching_end(reader, text, b"item")?;
                values.push(Value {
                    raw: text[content_start..content_end].to_string(),
                    quantity,
                    span: Some(content_start..content_end),
                    element: before..reader.buffer_position() as usize,
                    edited: None,
                });
            }
            Event::Empty(e) if e.name().as_ref() == b"item" => {
                values.push(Value {
                    raw: String::new(),
                    quantity: attr(&e, b"quantity")?,
                    span: None,
                    element: before..reader.buffer_position() as usize,
                    edited: None,
                });
            }
            _ => {}
        }
    }
}

/// Consume events until the end tag that closes the current element (which may contain
/// nested markup such as `<b>` or `<xliff:g>`), returning the byte offset where that end
/// tag begins, i.e. the end of the element's raw content.
fn skip_to_matching_end(reader: &mut Reader<&[u8]>, text: &str, tag: &[u8]) -> Result<usize> {
    let mut depth = 0usize;
    loop {
        match reader.read_event().context("XML syntax error")? {
            Event::Eof => bail!("unexpected EOF inside <{}>", String::from_utf8_lossy(tag)),
            Event::Start(e) if e.name().as_ref() == tag => depth += 1,
            Event::End(e) if e.name().as_ref() == tag => {
                if depth == 0 {
                    let after = reader.buffer_position() as usize;
                    let start = text[..after].rfind("</").context("end tag position")?;
                    return Ok(start);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
}

fn attr(e: &BytesStart, key: &[u8]) -> Result<Option<String>> {
    for a in e.attributes() {
        let a = a.context("bad attribute")?;
        if a.key.as_ref() == key {
            return Ok(Some(
                a.unescape_value()
                    .context("bad attribute value")?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn name_and_translatable(e: &BytesStart) -> Result<(String, bool)> {
    let name = attr(e, b"name")?.context("resource element without name=")?;
    let translatable = attr(e, b"translatable")?.as_deref() != Some("false");
    Ok((name, translatable))
}

pub fn serialize(doc: &Document) -> String {
    // Collect edits as (range, replacement), apply back-to-front so offsets stay valid.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for entry in &doc.entries {
        for v in &entry.values {
            let Some(new) = &v.edited else { continue };
            match &v.span {
                Some(span) => edits.push((span.clone(), new.clone())),
                None => {
                    // `<string name="x"/>` → `<string name="x">new</string>`
                    let element = &doc.text[v.element.clone()];
                    let tag_name = if entry.kind == Kind::String {
                        "string"
                    } else {
                        "item"
                    };
                    let open = element.trim_end_matches("/>").trim_end();
                    edits.push((v.element.clone(), format!("{open}>{new}</{tag_name}>")));
                }
            }
        }
    }
    if edits.is_empty() {
        return doc.text.clone();
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = doc.text.clone();
    for (range, new) in edits {
        out.replace_range(range, &new);
    }
    out
}

fn strip_cdata(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(inner) = t
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
    {
        inner
    } else {
        raw
    }
}

/// Decode XML entities and Android string escapes into plain text. Inline markup is left as-is.
pub fn decode(raw: &str) -> String {
    let unescaped = decode_entities(raw);
    let mut out = String::with_capacity(unescaped.len());
    let mut chars = unescaped.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => {
                        out.push_str("\\u");
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => out.push(other), // \' \" \\ \@ \?
            None => out.push('\\'),
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let ent = &rest[1..end];
        let decoded = match ent {
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
        match decoded {
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

/// Encode plain text as Android raw string content: `&` → `&amp;` (unless already an
/// entity), quotes and apostrophes backslash-escaped, newlines/tabs as `\n`/`\t`, and a
/// leading `@` or `?` escaped. `<`/`>` are left alone so inline markup keeps working.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        match c {
            '&' => {
                let looks_like_entity = text[i + 1..].find(';').is_some_and(|j| {
                    j > 0
                        && j <= 8
                        && text[i + 1..i + 1 + j]
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'#')
                });
                out.push_str(if looks_like_entity { "&" } else { "&amp;" });
            }
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '@' | '?' if i == 0 => {
                let _ = write!(out, "\\{c}");
            }
            _ => out.push(c),
        }
        i += c.len_utf8();
    }
    out
}

impl Document {
    /// Index of the entry named `name`, if any.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    /// Append a new `<string>` before `</resources>`, matching the file's indentation.
    pub fn insert_string(&mut self, name: &str, text: &str) {
        let indent = self.detect_indent();
        let element = format!(
            "{indent}<string name=\"{}\">{}</string>\n",
            escape_attr(name),
            encode(text)
        );
        let at = self.before_resources_end();
        self.splice_in(at, &element);
        self.entries.push(Entry {
            kind: Kind::String,
            name: name.to_string(),
            translatable: true,
            comment: None,
            values: vec![Value {
                raw: encode(text),
                quantity: None,
                span: None,
                element: at..at + element.len(),
                edited: None,
            }],
        });
    }

    /// Set one quantity of a `<plurals>`: replaces the existing `<item>`, appends a new
    /// `<item>` to an existing block, or creates the block before `</resources>`.
    pub fn set_plural_item(&mut self, name: &str, quantity: &str, text: &str) {
        let indent = self.detect_indent();
        match self
            .entries
            .iter()
            .position(|e| e.kind == Kind::Plurals && e.name == name)
        {
            Some(i) => {
                if let Some(idx) = self.entries[i]
                    .values
                    .iter()
                    .position(|v| v.quantity.as_deref() == Some(quantity))
                {
                    self.set_text(i, idx, text);
                    return;
                }
                // Append after the last item, copying its line's indentation.
                let last_end = self.entries[i]
                    .values
                    .last()
                    .map(|v| v.element.end)
                    .unwrap_or_else(|| self.before_resources_end());
                let line_start = self.text[..last_end]
                    .rfind('\n')
                    .map(|p| p + 1)
                    .unwrap_or(0);
                let item_indent: String = self.text[line_start..]
                    .chars()
                    .take_while(|c| c.is_whitespace() && *c != '\n')
                    .collect();
                let element = format!(
                    "\n{item_indent}<item quantity=\"{quantity}\">{}</item>",
                    encode(text)
                );
                self.splice_in(last_end, &element);
                let content_start = last_end + element.len() - encode(text).len() - "</item>".len();
                self.entries[i].values.push(Value {
                    raw: encode(text),
                    quantity: Some(quantity.to_string()),
                    span: Some(content_start..content_start + encode(text).len()),
                    element: last_end..last_end + element.len(),
                    edited: None,
                });
            }
            None => {
                let item_indent = format!("{indent}{indent}");
                let block = format!(
                    "{indent}<plurals name=\"{}\">\n{item_indent}<item quantity=\"{quantity}\">{}</item>\n{indent}</plurals>\n",
                    escape_attr(name),
                    encode(text)
                );
                let at = self.before_resources_end();
                self.splice_in(at, &block);
                let item_start =
                    at + indent.len() + "<plurals name=\"\">\n".len() + escape_attr(name).len();
                let item_len = item_indent.len()
                    + "<item quantity=\"\">".len()
                    + quantity.len()
                    + encode(text).len()
                    + "</item>".len();
                let content_start =
                    item_start + item_indent.len() + "<item quantity=\"\">".len() + quantity.len();
                self.entries.push(Entry {
                    kind: Kind::Plurals,
                    name: name.to_string(),
                    translatable: true,
                    comment: None,
                    values: vec![Value {
                        raw: encode(text),
                        quantity: Some(quantity.to_string()),
                        span: Some(content_start..content_start + encode(text).len()),
                        element: item_start..item_start + item_len,
                        edited: None,
                    }],
                });
            }
        }
    }

    /// Set item `idx` of a `<string-array>`. A missing block is created, and a block
    /// shorter than `source_items` is padded from the source first, so the array is
    /// never left shorter than the original (Android arrays are positional).
    pub fn set_array_item(&mut self, name: &str, idx: usize, text: &str, source_items: &[String]) {
        let indent = self.detect_indent();
        let i = match self
            .entries
            .iter()
            .position(|e| e.kind == Kind::StringArray && e.name == name)
        {
            Some(i) => i,
            None => {
                let item_indent = format!("{indent}{indent}");
                let mut block = format!("{indent}<string-array name=\"{}\">\n", escape_attr(name));
                let mut values = Vec::new();
                for item in source_items {
                    let enc = encode(item);
                    let line = format!("{item_indent}<item>{enc}</item>\n");
                    let start = block.len();
                    let content_start = start + item_indent.len() + "<item>".len();
                    values.push(Value {
                        raw: enc.clone(),
                        quantity: None,
                        span: Some(content_start..content_start + enc.len()),
                        element: start..start + line.len() - 1,
                        edited: None,
                    });
                    block.push_str(&line);
                }
                block.push_str(&format!("{indent}</string-array>\n"));
                let at = self.before_resources_end();
                self.splice_in(at, &block);
                for v in &mut values {
                    if let Some(sp) = &mut v.span {
                        sp.start += at;
                        sp.end += at;
                    }
                    v.element.start += at;
                    v.element.end += at;
                }
                self.entries.push(Entry {
                    kind: Kind::StringArray,
                    name: name.to_string(),
                    translatable: true,
                    comment: None,
                    values,
                });
                self.entries.len() - 1
            }
        };
        // Pad a short block from the source, copying the last item's indentation.
        while self.entries[i].values.len() <= idx {
            let n = self.entries[i].values.len();
            let src = source_items.get(n).cloned().unwrap_or_default();
            let last_end = self.entries[i]
                .values
                .last()
                .map(|v| v.element.end)
                .unwrap_or_else(|| self.before_resources_end());
            let line_start = self.text[..last_end]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let item_indent: String = self.text[line_start..]
                .chars()
                .take_while(|c| c.is_whitespace() && *c != '\n')
                .collect();
            let enc = encode(&src);
            let element = format!("\n{item_indent}<item>{enc}</item>");
            self.splice_in(last_end, &element);
            let content_start = last_end + element.len() - enc.len() - "</item>".len();
            self.entries[i].values.push(Value {
                raw: enc.clone(),
                quantity: None,
                span: Some(content_start..content_start + enc.len()),
                element: last_end..last_end + element.len(),
                edited: None,
            });
        }
        self.set_text(i, idx, text);
    }

    /// Byte offset at the start of the `</resources>` line (or end of text).
    fn before_resources_end(&self) -> usize {
        let close = self.text.rfind("</resources>").unwrap_or(self.text.len());
        let line_start = self.text[..close].rfind('\n').map(|i| i + 1).unwrap_or(0);
        if self.text[line_start..close].trim().is_empty() {
            line_start
        } else {
            close
        }
    }

    /// Insert raw text at `at`, shifting every recorded span after it.
    fn splice_in(&mut self, at: usize, s: &str) {
        self.text.insert_str(at, s);
        let shift = s.len();
        for e in &mut self.entries {
            for v in &mut e.values {
                if let Some(sp) = &mut v.span
                    && sp.start >= at
                {
                    sp.start += shift;
                    sp.end += shift;
                }
                if v.element.start >= at {
                    v.element.start += shift;
                    v.element.end += shift;
                }
            }
        }
    }

    fn detect_indent(&self) -> String {
        for line in self.text.lines() {
            if line.trim_start().starts_with("<string") || line.trim_start().starts_with("<plurals")
            {
                let ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                return ws;
            }
        }
        "    ".to_string()
    }
}

/// Decoded items of the `<string-array>` named `name` (empty if absent).
pub fn array_items(doc: &Document, name: &str) -> Vec<String> {
    doc.entries
        .iter()
        .find(|e| e.kind == Kind::StringArray && e.name == name)
        .map(|e| e.values.iter().map(Value::text).collect())
        .unwrap_or_default()
}

/// A fresh, empty resources file in the conventional Android style.
pub fn empty_file() -> String {
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n</resources>\n".to_string()
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}
