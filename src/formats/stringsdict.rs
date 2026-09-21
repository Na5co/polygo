//! Apple `.stringsdict`: the plural rules that go with `.strings`. A plist of
//! `key → { NSStringLocalizedFormatKey, <variable> → { NSStringFormatSpecTypeKey,
//! NSStringFormatValueTypeKey, zero|one|two|few|many|other → format } }`.
//! Read-only for now: `check` validates it, `translate` does not write it yet.

use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variable {
    /// `d`, `u`, `f`, `ld`… from NSStringFormatValueTypeKey.
    pub value_type: String,
    /// CLDR category → format string, in file order.
    pub forms: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// `NSStringLocalizedFormatKey`, e.g. `%#@files@`.
    pub format: String,
    pub variables: BTreeMap<String, Variable>,
    /// Line of the top-level `<key>`.
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub entries: BTreeMap<String, Entry>,
}

/// A plist node, just enough of it.
#[derive(Debug, Clone)]
enum Node {
    Dict(Vec<(String, Node, usize)>),
    Text(String),
    Other,
}

fn parse_plist(text: &str) -> Result<Node> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    // Stack of open dicts: (pairs so far, pending key, line of pending key).
    let mut stack: Vec<(Vec<(String, Node, usize)>, Option<String>, usize)> = Vec::new();
    let mut root: Option<Node> = None;
    let mut in_key = false;
    let mut in_string = false;
    let mut text_buf = String::new();
    let line_at = |pos: usize| text[..pos.min(text.len())].matches('\n').count() + 1;
    loop {
        let pos = reader.buffer_position() as usize;
        match reader.read_event().context("plist XML")? {
            Event::Eof => break,
            Event::Start(e) => match e.name().as_ref() {
                b"dict" => stack.push((Vec::new(), None, 0)),
                b"key" => {
                    in_key = true;
                    text_buf.clear();
                }
                b"string" => {
                    in_string = true;
                    text_buf.clear();
                }
                b"array" | b"data" | b"date" | b"integer" | b"real" | b"true" | b"false" => {
                    // Not part of a stringsdict we care about; attach as Other when it ends.
                }
                _ => {}
            },
            Event::Text(t) => {
                if in_key || in_string {
                    text_buf.push_str(&t.unescape().context("plist text")?);
                }
            }
            Event::Empty(e) => {
                let node = match e.name().as_ref() {
                    b"string" | b"key" => Node::Text(String::new()),
                    _ => Node::Other,
                };
                if let Some((pairs, pending, l)) = stack.last_mut()
                    && let Some(k) = pending.take()
                {
                    pairs.push((k, node, *l));
                }
            }
            Event::End(e) => match e.name().as_ref() {
                b"key" => {
                    in_key = false;
                    if let Some((_, pending, l)) = stack.last_mut() {
                        *pending = Some(std::mem::take(&mut text_buf));
                        *l = line_at(pos);
                    }
                }
                b"string" => {
                    in_string = false;
                    let node = Node::Text(std::mem::take(&mut text_buf));
                    if let Some((pairs, pending, l)) = stack.last_mut()
                        && let Some(k) = pending.take()
                    {
                        pairs.push((k, node, *l));
                    }
                }
                b"dict" => {
                    let (pairs, _, _) = stack.pop().context("unbalanced <dict>")?;
                    let node = Node::Dict(pairs);
                    match stack.last_mut() {
                        Some((pairs, pending, l)) => {
                            if let Some(k) = pending.take() {
                                pairs.push((k, node, *l));
                            }
                        }
                        None => root = Some(node),
                    }
                }
                b"array" | b"integer" | b"real" | b"date" | b"data" => {
                    if let Some((pairs, pending, l)) = stack.last_mut()
                        && let Some(k) = pending.take()
                    {
                        pairs.push((k, Node::Other, *l));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    root.context("no <dict> in plist")
}

pub fn parse(text: &str) -> Result<Document> {
    let Node::Dict(top) = parse_plist(text)? else {
        anyhow::bail!("stringsdict root is not a dict");
    };
    let mut entries = BTreeMap::new();
    for (key, node, line) in top {
        let Node::Dict(pairs) = node else { continue };
        let mut entry = Entry {
            line,
            ..Default::default()
        };
        for (k, v, _) in pairs {
            match (k.as_str(), v) {
                ("NSStringLocalizedFormatKey", Node::Text(t)) => entry.format = t,
                (name, Node::Dict(var)) => {
                    let mut variable = Variable::default();
                    let mut is_plural = false;
                    for (vk, vv, _) in var {
                        match (vk.as_str(), vv) {
                            ("NSStringFormatSpecTypeKey", Node::Text(t)) => {
                                is_plural = t == "NSStringPluralRuleType";
                            }
                            ("NSStringFormatValueTypeKey", Node::Text(t)) => {
                                variable.value_type = t;
                            }
                            (cat, Node::Text(t))
                                if matches!(
                                    cat,
                                    "zero" | "one" | "two" | "few" | "many" | "other"
                                ) =>
                            {
                                variable.forms.push((cat.to_string(), t));
                            }
                            _ => {}
                        }
                    }
                    if is_plural {
                        entry.variables.insert(name.to_string(), variable);
                    }
                }
                _ => {}
            }
        }
        entries.insert(key, entry);
    }
    Ok(Document { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variables_and_forms() {
        let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>%d files</key>
	<dict>
		<key>NSStringLocalizedFormatKey</key>
		<string>%#@files@</string>
		<key>files</key>
		<dict>
			<key>NSStringFormatSpecTypeKey</key>
			<string>NSStringPluralRuleType</string>
			<key>NSStringFormatValueTypeKey</key>
			<string>d</string>
			<key>one</key>
			<string>%d file</string>
			<key>other</key>
			<string>%d files</string>
		</dict>
	</dict>
</dict>
</plist>
"#;
        let doc = parse(src).unwrap();
        let e = &doc.entries["%d files"];
        assert_eq!(e.format, "%#@files@");
        assert_eq!(e.line, 5);
        let v = &e.variables["files"];
        assert_eq!(v.value_type, "d");
        assert_eq!(
            v.forms,
            vec![
                ("one".into(), "%d file".into()),
                ("other".into(), "%d files".into())
            ]
        );
    }
}
