//! Where a finding lives: the file that holds the translation (the locale file for
//! per-locale formats, the catalog for `.xcstrings`) and the line of the key in it, so
//! `polygo check` can say `locales/de.json:12` and a GitHub annotation lands on the line.
//!
//! Lines come from a text search for the key in the file's own syntax, not from the
//! parsers, so this costs one read per file and cannot disagree with what the user sees.

use crate::config::{FileSpec, Format};
use crate::project;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Locator<'a> {
    root: &'a Path,
    cache: HashMap<PathBuf, Option<Index>>,
}

/// One pass over a file: every line that starts with a quoted JSON key or an XML/PO key,
/// so lookups are a map probe instead of a text scan (a String Catalog has 50k of them).
struct Index {
    /// JSON-family: (key, indent, line), in file order.
    keys: Vec<(String, usize, usize)>,
    /// XML `name="…"` and PO `msgid "…"`: key → first line.
    named: HashMap<String, usize>,
}

impl Index {
    fn build(format: Format, text: &str) -> Index {
        let mut keys = Vec::new();
        let mut named = HashMap::new();
        for (i, line) in text.lines().enumerate() {
            let n = i + 1;
            match format {
                Format::Xcstrings | Format::Json | Format::Arb => {
                    let indent = line.len() - line.trim_start().len();
                    let t = line.trim_start();
                    if let Some(rest) = t.strip_prefix('"')
                        && let Some(end) = json_string_end(rest)
                        && rest[end + 1..].trim_start().starts_with(':')
                        && let Ok(k) = serde_json::from_str::<String>(&t[..end + 2])
                    {
                        keys.push((k, indent, n));
                    }
                }
                Format::Strings => {
                    // `"key" = "…";` or `key = "…";`, possibly after a comment.
                    let t = line.trim_start();
                    let key = if let Some(rest) = t.strip_prefix('"') {
                        json_string_end(rest).and_then(|end| {
                            let raw = &rest[..end];
                            rest[end + 1..]
                                .trim_start()
                                .starts_with('=')
                                .then(|| raw.replace("\\\"", "\"").replace("\\\\", "\\"))
                        })
                    } else {
                        t.split_once('=').and_then(|(k, _)| {
                            let k = k.trim();
                            (!k.is_empty()
                                && k.chars()
                                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)))
                            .then(|| k.to_string())
                        })
                    };
                    if let Some(k) = key {
                        named.entry(k).or_insert(n);
                    }
                }
                Format::Android | Format::Resx => {
                    let mut rest = line;
                    while let Some(i) = rest.find("name=\"") {
                        let after = &rest[i + 6..];
                        if let Some(e) = after.find('"') {
                            let k = after[..e].replace("&quot;", "\"").replace("&amp;", "&");
                            named.entry(k).or_insert(n);
                            rest = &after[e + 1..];
                        } else {
                            break;
                        }
                    }
                }
                Format::Po => {
                    if let Some(rest) = line.strip_prefix("msgid ")
                        && let Ok(k) = serde_json::from_str::<String>(rest.trim())
                    {
                        named.entry(k).or_insert(n);
                    }
                }
            }
        }
        Index { keys, named }
    }
}

/// Byte index of the closing quote of a JSON string whose opening quote was just consumed.
fn json_string_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

impl<'a> Locator<'a> {
    pub fn new(root: &'a Path) -> Self {
        Locator {
            root,
            cache: HashMap::new(),
        }
    }

    fn index(&mut self, rel: &str, format: Format) -> Option<&Index> {
        let path = self.root.join(rel);
        self.cache
            .entry(path.clone())
            .or_insert_with(|| {
                crate::formats::read_text(&path)
                    .ok()
                    .map(|t| Index::build(format, &t))
            })
            .as_ref()
    }

    /// `(file, line)` for `key` (the key within `spec`, plural/array suffix allowed) in
    /// `locale`. The file is the one a fix would be made in; the line is `None` when the
    /// key cannot be found there (e.g. the translation is missing).
    pub fn locate(&mut self, spec: &FileSpec, key: &str, locale: &str) -> (String, Option<usize>) {
        let source = spec.path.display().to_string().replace('\\', "/");
        let file = match (&spec.format, &spec.locale_path) {
            (Format::Xcstrings, _) | (_, None) => source.clone(),
            (_, Some(template)) => {
                let lf = project::locale_file(template, locale);
                if self.root.join(&lf).exists() {
                    lf
                } else {
                    source.clone()
                }
            }
        };
        let base = crate::core::base_key(key);
        let base = base.split('#').next().unwrap_or(base);
        let format = spec.format;
        let line = self
            .index(&file, format)
            .and_then(|ix| line_of(format, ix, base, locale));
        (file, line)
    }
}

/// The key's line. JSON-family keys are looked up by the dotted key when it is a literal
/// key in the file, else by the last path segment.
fn line_of(format: Format, ix: &Index, key: &str, locale: &str) -> Option<usize> {
    match format {
        Format::Xcstrings => {
            // Keys sit one level under `"strings"`; the locale is nested under the key.
            let strings = ix.keys.iter().position(|(k, _, _)| k == "strings")?;
            let key_indent = ix.keys.get(strings + 1)?.1;
            let pos = strings
                + 1
                + ix.keys[strings + 1..]
                    .iter()
                    .position(|(k, ind, _)| *ind == key_indent && k == key)?;
            let key_line = ix.keys[pos].2;
            let end = ix.keys[pos + 1..]
                .iter()
                .find(|(_, ind, _)| *ind == key_indent)
                .map_or(usize::MAX, |k| k.2);
            let loc = ix.keys[pos + 1..]
                .iter()
                .take_while(|(_, _, l)| *l < end)
                .find(|(k, ind, _)| k == locale && *ind == key_indent + 4)
                .map(|k| k.2);
            Some(loc.unwrap_or(key_line))
        }
        Format::Json | Format::Arb => {
            let last = key.rsplit('.').next().unwrap_or(key);
            ix.keys
                .iter()
                .find(|(k, _, _)| k == key)
                .or_else(|| ix.keys.iter().find(|(k, _, _)| k == last))
                .map(|k| k.2)
        }
        Format::Android | Format::Resx | Format::Strings => ix.named.get(key).copied(),
        Format::Po => {
            let msgid = key.rsplit('\u{4}').next().unwrap_or(key);
            ix.named.get(msgid).copied()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_in_each_syntax() {
        let at = |f: Format, text: &str, key: &str, loc: &str| {
            line_of(f, &Index::build(f, text), key, loc)
        };
        let json = "{\n  \"a\": \"A\",\n  \"menu\": {\n    \"open\": \"Open\"\n  }\n}\n";
        assert_eq!(at(Format::Json, json, "a", "de"), Some(2));
        assert_eq!(at(Format::Json, json, "menu.open", "de"), Some(4));
        assert_eq!(at(Format::Json, json, "nope", "de"), None);
        let xml = "<resources>\n  <string name=\"a\">A</string>\n  <plurals name=\"n\">\n  </plurals>\n</resources>\n";
        assert_eq!(at(Format::Android, xml, "n", "de"), Some(3));
        let xc = "{\n  \"strings\" : {\n    \"Open\" : {\n      \"localizations\" : {\n        \"de\" : {\n          \"stringUnit\" : {}\n        },\n        \"fr\" : {}\n      }\n    },\n    \"Save\" : {\n      \"localizations\" : {\n        \"de\" : {}\n      }\n    }\n  }\n}\n";
        assert_eq!(at(Format::Xcstrings, xc, "Open", "de"), Some(5));
        assert_eq!(at(Format::Xcstrings, xc, "Open", "fr"), Some(8));
        assert_eq!(at(Format::Xcstrings, xc, "Save", "de"), Some(13));
        // A locale the key lacks: the key's own line, not the next key's locale.
        assert_eq!(at(Format::Xcstrings, xc, "Open", "pl"), Some(3));
        let po = "msgid \"\"\nmsgstr \"\"\n\nmsgid \"Hello\"\nmsgstr \"Hallo\"\n";
        assert_eq!(at(Format::Po, po, "Hello", "de"), Some(4));
    }
}
