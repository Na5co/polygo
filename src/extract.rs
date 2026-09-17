//! `polygo extract`: pull user-facing text out of web source (JSX/TSX, HTML in
//! template literals, .html/.vue/.svelte) into an i18next `locales/en.json`, with a
//! report of where every string came from. The code rewrite to `t("key")` is left to
//! the developer; this gives them the catalog and the list.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hit {
    pub file: String,
    pub line: usize,
    pub text: String,
    pub key: String,
    /// `text`, or the attribute name (`placeholder`, `aria-label`, ...).
    pub kind: String,
}

const EXT: &[&str] = &[
    "tsx", "jsx", "ts", "js", "mjs", "vue", "svelte", "astro", "html",
];
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "out",
    "coverage",
    "vendor",
    ".git",
    "target",
    ".claude",
    "public",
    "static",
    "assets",
];
const TEXT_ATTRS: &[&str] = &[
    "placeholder",
    "title",
    "alt",
    "aria-label",
    "aria-description",
    "aria-placeholder",
    "label",
    "aria-roledescription",
];
/// Elements whose text is never UI copy.
const SKIP_ELEMENTS: &[&str] = &[
    "script", "style", "svg", "code", "pre", "kbd", "samp", "math", "noscript", "template",
    "textarea",
];

pub fn scan(root: &Path) -> Result<Vec<Hit>> {
    let mut hits = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_some_and(|t| t.is_dir()) && SKIP_DIRS.contains(&name.as_ref()))
        })
        .build();
    let mut files: Vec<PathBuf> = walker
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| EXT.contains(&x.to_str().unwrap_or("")))
        })
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            !(n.contains(".test.")
                || n.contains(".spec.")
                || n.contains(".stories.")
                || n.ends_with(".d.ts")
                || n.contains(".generated."))
        })
        .collect();
    files.sort();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        scan_text(&rel, &text, &mut hits);
    }
    // Same text in several places is one key.
    hits.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok(hits)
}

/// Scan markup-ish text: anything between `>` and `<` that reads like a sentence, plus
/// text attributes. Works on JSX, HTML inside template literals, and plain HTML alike,
/// because it only looks at the markup, not the language around it.
pub fn scan_text(file: &str, text: &str, out: &mut Vec<Hit>) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut skip_depth: Vec<String> = Vec::new(); // open skip elements
    let mut line = 1;
    while i < b.len() {
        if b[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        // A tag? Find its `>` while skipping quoted values and `{...}` expressions
        // (`onClick={() => x}` contains a `>`).
        let Some(end) = tag_end(text, i) else {
            i += 1;
            continue;
        };
        let tag = &text[i + 1..end];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_ascii_lowercase();
        let closing = tag.starts_with('/');
        if name.is_empty() || tag.starts_with('!') || tag.starts_with('?') || !is_tag(tag) {
            // A `<` in code (`a < b`), not markup.
            i += 1;
            continue;
        }
        if SKIP_ELEMENTS.contains(&name.as_str()) {
            if closing {
                skip_depth.retain(|n| *n != name);
            } else if !tag.ends_with('/') {
                skip_depth.push(name.clone());
            }
        }
        if !closing && skip_depth.is_empty() {
            for (attr, value) in attributes(tag) {
                if TEXT_ATTRS.contains(&attr.as_str()) && looks_like_copy(&value) {
                    out.push(hit(file, line, &value, &attr));
                }
            }
        }
        line += tag.matches('\n').count();
        i = end + 1;
        if skip_depth.is_empty() {
            // Text run up to the next tag.
            let run_end = text[i..].find('<').map(|k| i + k).unwrap_or(text.len());
            let run = &text[i..run_end];
            let cleaned = clean(run);
            if looks_like_copy(&cleaned) {
                out.push(hit(file, line, &cleaned, "text"));
            }
            line += run.matches('\n').count();
            i = run_end;
        }
    }
}

fn hit(file: &str, line: usize, text: &str, kind: &str) -> Hit {
    Hit {
        file: file.to_string(),
        line,
        text: text.to_string(),
        key: key_for(text),
        kind: kind.to_string(),
    }
}

/// `name="value"` / `name='value'` pairs (JSX `name={...}` are skipped: expressions).
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = tag.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && !(b[i].is_ascii_alphabetic() || b[i] == b'@' || b[i] == b':') {
            i += 1;
        }
        let start = i;
        while i < b.len()
            && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'-' | b'_' | b':' | b'@' | b'.'))
        {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let name = tag[start..i].to_ascii_lowercase();
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if b.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        match b.get(j) {
            Some(&q) if q == b'"' || q == b'\'' => {
                let vstart = j + 1;
                let Some(vend) = tag[vstart..].find(q as char).map(|k| vstart + k) else {
                    break;
                };
                out.push((name, tag[vstart..vend].to_string()));
                i = vend + 1;
            }
            _ => {
                i = j;
            }
        }
    }
    out
}

fn tag_end(text: &str, start: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = start + 1;
    let mut depth = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' if depth > 0 || i > start + 1 => quote = Some(c),
                b'{' => depth += 1,
                b'}' => depth = (depth - 1).max(0),
                b'>' if depth == 0 => return Some(i),
                b'<' if depth == 0 => return None, // not a tag after all
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Strict-ish tag syntax: `/?name (attr(=value)?)* /?` where value is quoted, `{...}`
/// or a bare token. Rejects code that merely contains `<` and `>`.
fn is_tag(tag: &str) -> bool {
    let t = tag.trim_end_matches('/').trim();
    let t = t.strip_prefix('/').unwrap_or(t);
    let b = t.as_bytes();
    let mut i = 0;
    // name
    if !b.first().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'-' | b'.' | b':')) {
        i += 1;
    }
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return true;
        }
        // Attribute name (JSX spread `{...props}` allowed).
        if b[i] == b'{' {
            let Some(close) = t[i..].find('}') else {
                return false;
            };
            i += close + 1;
            continue;
        }
        if !(b[i].is_ascii_alphabetic() || matches!(b[i], b'@' | b':' | b'#' | b'_' | b'$')) {
            return false;
        }
        while i < b.len()
            && (b[i].is_ascii_alphanumeric()
                || matches!(b[i], b'-' | b'_' | b':' | b'@' | b'.' | b'$'))
        {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            match b.get(i) {
                Some(&q) if q == b'"' || q == b'\'' => {
                    let Some(close) = t[i + 1..].find(q as char) else {
                        return false;
                    };
                    i += close + 2;
                }
                Some(b'{') => {
                    let mut depth = 0;
                    let mut j = i;
                    let mut closed = false;
                    while j < b.len() {
                        match b[j] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    closed = true;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    if !closed {
                        return false;
                    }
                    i = j + 1;
                }
                Some(b'`') => {
                    let Some(close) = t[i + 1..].find('`') else {
                        return false;
                    };
                    i += close + 2;
                }
                Some(_) => {
                    while i < b.len() && !b[i].is_ascii_whitespace() {
                        i += 1;
                    }
                }
                None => return false,
            }
        }
    }
}

/// Collapse whitespace; drop runs that are only interpolation.
fn clean(run: &str) -> String {
    let s: String = run.split_whitespace().collect::<Vec<_>>().join(" ");
    s.trim().to_string()
}

/// Does this read like something a user sees?
pub fn looks_like_copy(s: &str) -> bool {
    let s = s.trim();
    if s.chars().count() < 2 || !s.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    // Pure or leading interpolation (`${x}`, `{x}`, `{{ x }}`): not translatable copy.
    let stripped = strip_interpolations(s);
    if !stripped.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    // HTML entities only, URLs, and code-like tokens.
    if s.starts_with('&') && s.ends_with(';') && !s.contains(' ') {
        return false;
    }
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with('/')
        || s.starts_with('#')
    {
        return false;
    }
    // Code between template literals (`...` : cond ? `...`), not copy.
    if s.contains('`')
        || s.contains("=>")
        || s.contains(");")
        || s.contains("};")
        || s.contains(" ? ")
        || s.contains("/*")
        || s.contains("*/")
    {
        return false;
    }
    let single = !s.contains(' ');
    if single {
        // A lone word is copy only if it looks like a word ("Save", "Sections"), not an
        // identifier, a class list, a number with a unit or a file name.
        let w = s.trim_matches(|c: char| !c.is_alphanumeric());
        let has_upper_inside = w.chars().skip(1).any(|c| c.is_uppercase());
        return w.chars().all(|c| c.is_alphabetic() || c == '\'')
            && !has_upper_inside
            && !w.chars().all(|c| c.is_uppercase() && w.len() > 4);
    }
    // Multi-word: reject template noise like `${a} ${b}` handled above; accept the rest.
    true
}

fn strip_interpolations(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if (b[i] == b'$' && b.get(i + 1) == Some(&b'{')) || b[i] == b'{' {
            depth += 1;
            i += if b[i] == b'$' { 2 } else { 1 };
            continue;
        }
        if b[i] == b'}' && depth > 0 {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 {
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        } else {
            i += 1;
        }
    }
    out
}

/// i18next-style natural key: the text itself, with `${expr}` / `{expr}` turned into
/// numbered `{{0}}` placeholders and long texts cut to a slug.
pub fn key_for(text: &str) -> String {
    let with_ph = number_interpolations(text);
    if with_ph.chars().count() <= 60 {
        return with_ph;
    }
    let words: Vec<String> = with_ph
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(|w| w.to_lowercase())
        .collect();
    words.join("_")
}

/// The catalog value: same as the key transformation but keeps full length.
pub fn value_for(text: &str) -> String {
    number_interpolations(text)
}

fn number_interpolations(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    let mut n = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if depth == 0 && ((b[i] == b'$' && b.get(i + 1) == Some(&b'{')) || b[i] == b'{') {
            depth = 1;
            out.push_str(&format!("{{{{{n}}}}}"));
            n += 1;
            i += if b[i] == b'$' { 2 } else { 1 };
            continue;
        }
        if depth > 0 {
            if b[i] == b'{' {
                depth += 1;
            } else if b[i] == b'}' {
                depth -= 1;
            }
            i += 1;
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Merge hits into an i18next JSON file (existing keys are kept). Returns (added, total).
pub fn write_catalog(path: &Path, hits: &[Hit]) -> Result<(usize, usize)> {
    let mut map: BTreeMap<String, String> = if path.exists() {
        serde_json::from_str(&std::fs::read_to_string(path)?).context("existing catalog JSON")?
    } else {
        BTreeMap::new()
    };
    let before = map.len();
    for h in hits {
        map.entry(h.key.clone())
            .or_insert_with(|| value_for(&h.text));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(&map)?;
    text.push('\n');
    std::fs::write(path, text)?;
    Ok((map.len() - before, map.len()))
}
