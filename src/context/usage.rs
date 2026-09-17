//! Find where localization keys are used in source code.
//!
//! One pass: every key becomes a handful of literal needles (`"key"`, `'key'`,
//! `R.string.key`, `@string/key`, `.key`), all keys go into a single Aho-Corasick
//! automaton, and every source file is scanned once. Matches are validated for
//! identifier boundaries and enriched with the enclosing declaration and a
//! ±6-line snippet.

use aho_corasick::{AhoCorasick, MatchKind};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SOURCE_EXT: &[&str] = &[
    "swift", "m", "mm", "kt", "kts", "java", "dart", "ts", "tsx", "js", "jsx", "mjs", "cjs", "vue",
    "svelte", "xml",
];
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "Pods",
    "build",
    ".build",
    "DerivedData",
    "target",
    "dist",
    ".next",
    "vendor",
    ".dart_tool",
    "Carthage",
    ".gradle",
    "out",
    "coverage",
    "__pycache__",
];
const MAX_FILE_BYTES: u64 = 1_000_000;
const CONTEXT_LINES: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// Path relative to the indexed root.
    pub path: String,
    /// 1-based line of the match.
    pub line: usize,
    /// Enclosing function/type/component name, if one could be found.
    pub ident: Option<String>,
    /// ±6 lines around the match, dedented.
    pub snippet: String,
}

pub struct Index {
    root: PathBuf,
    files: Vec<(String, String)>, // (relative path, contents)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Needle {
    /// Quoted literal: no boundary check needed beyond the quotes.
    Quoted,
    /// Identifier-ish reference (`R.string.key`, `.key`): the next char must not continue an identifier.
    Ident,
}

impl Index {
    pub fn build(root: &Path) -> Result<Index> {
        let mut files = Vec::new();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !(e.file_type().is_some_and(|t| t.is_dir()) && SKIP_DIRS.contains(&name.as_ref()))
            })
            .build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !SOURCE_EXT.contains(&ext) {
                continue;
            }
            if entry
                .metadata()
                .map(|m| m.len() > MAX_FILE_BYTES)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            files.push((rel, text));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Index {
            root: root.to_path_buf(),
            files,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn files(&self) -> usize {
        self.files.len()
    }

    /// First usage of each key that has one.
    pub fn find_all(&self, keys: &[&str]) -> HashMap<String, Usage> {
        let mut patterns: Vec<String> = Vec::new();
        let mut owners: Vec<(usize, Needle)> = Vec::new(); // pattern → (key index, kind)
        for (ki, key) in keys.iter().enumerate() {
            if key.is_empty() {
                continue;
            }
            let mut add = |p: String, kind: Needle| {
                patterns.push(p);
                owners.push((ki, kind));
            };
            let esc_dq = key.replace('\\', "\\\\").replace('"', "\\\"");
            let esc_sq = key.replace('\\', "\\\\").replace('\'', "\\'");
            add(format!("\"{esc_dq}\""), Needle::Quoted);
            add(format!("'{esc_sq}'"), Needle::Quoted);
            add(format!("`{key}`"), Needle::Quoted);
            if is_identifier(key) {
                add(format!("R.string.{key}"), Needle::Ident);
                add(format!("@string/{key}"), Needle::Ident);
                add(format!(".{key}"), Needle::Ident);
            }
        }
        let mut out: HashMap<String, Usage> = HashMap::new();
        if patterns.is_empty() {
            return out;
        }
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&patterns)
            .expect("patterns build");
        // Lower-confidence `.key` hits are kept only if nothing better turns up.
        let mut weak: HashMap<usize, Usage> = HashMap::new();
        let mut strong: HashMap<usize, Usage> = HashMap::new();
        for (rel, text) in &self.files {
            for m in ac.find_iter(text) {
                let (ki, kind) = owners[m.pattern().as_usize()];
                if strong.contains_key(&ki) {
                    continue;
                }
                let bytes = text.as_bytes();
                let after = bytes.get(m.end()).copied();
                if kind == Needle::Ident
                    && after.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    continue;
                }
                let is_dot_ref = patterns[m.pattern().as_usize()].starts_with('.');
                if is_dot_ref {
                    // `.key` must not be preceded by another identifier char run like `..key` or a digit.
                    let before = m.start().checked_sub(1).and_then(|i| bytes.get(i)).copied();
                    if before.is_some_and(|c| c == b'.' || c.is_ascii_digit()) {
                        continue;
                    }
                }
                let usage = make_usage(rel, text, m.start());
                if is_dot_ref {
                    weak.entry(ki).or_insert(usage);
                } else {
                    strong.insert(ki, usage);
                }
            }
        }
        for (ki, key) in keys.iter().enumerate() {
            if let Some(u) = strong.remove(&ki).or_else(|| weak.remove(&ki)) {
                out.insert((*key).to_string(), u);
            }
        }
        out
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn make_usage(rel: &str, text: &str, offset: usize) -> Usage {
    let line_idx = text[..offset].matches('\n').count();
    let lines: Vec<&str> = text.lines().collect();
    let lo = line_idx.saturating_sub(CONTEXT_LINES);
    let hi = (line_idx + CONTEXT_LINES + 1).min(lines.len());
    let window: Vec<&str> = lines[lo..hi].iter().map(|l| truncate(l, 200)).collect();
    let indent = window
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let snippet = window
        .iter()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Usage {
        path: rel.to_string(),
        line: line_idx + 1,
        ident: enclosing_ident(&lines, line_idx),
        snippet,
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Walk upwards from `line` to the nearest declaration. Property-level declarations
/// such as SwiftUI's `var body` or a React `const { t }` are skipped in favour of the
/// enclosing function/type.
fn enclosing_ident(lines: &[&str], line: usize) -> Option<String> {
    let mut fallback: Option<String> = None;
    for i in (0..=line).rev() {
        if let Some((name, strong)) = declaration_name(lines[i]) {
            if strong {
                return Some(name);
            }
            fallback.get_or_insert(name);
        }
    }
    fallback
}

/// `(identifier, is_strong)` if the line starts a declaration. Strong = function/type/component.
fn declaration_name(line: &str) -> Option<(String, bool)> {
    let t = line.trim_start();
    let t = t
        .trim_start_matches("export default ")
        .trim_start_matches("export ")
        .trim_start_matches("public ")
        .trim_start_matches("private ")
        .trim_start_matches("internal ")
        .trim_start_matches("fileprivate ")
        .trim_start_matches("override ")
        .trim_start_matches("static ")
        .trim_start_matches("final ")
        .trim_start_matches("open ")
        .trim_start_matches("async ")
        .trim_start_matches("@objc ");
    const STRONG: &[&str] = &[
        "func ",
        "fun ",
        "function ",
        "def ",
        "struct ",
        "class ",
        "enum ",
        "extension ",
        "protocol ",
        "interface ",
        "object ",
        "actor ",
        "widget ",
    ];
    for kw in STRONG {
        if let Some(rest) = t.strip_prefix(kw) {
            return ident_at(rest).map(|n| (n, true));
        }
    }
    // Dart/Java/Kotlin method: `Widget build(BuildContext context) {` / `void onCreate(...) {`
    if let Some(open) = t.find('(')
        && t.ends_with('{')
    {
        let head = &t[..open];
        let mut parts = head.split_whitespace().rev();
        if let Some(name) = parts.next()
            && parts.next().is_some()
            && is_identifier(name)
            && !matches!(name, "if" | "for" | "while" | "switch" | "return" | "catch")
        {
            return Some((name.to_string(), true));
        }
    }
    // Arrow-function components and weak property declarations.
    for kw in ["const ", "let ", "var ", "val ", "lateinit var "] {
        if let Some(rest) = t.strip_prefix(kw) {
            let name = ident_at(rest)?;
            let strong = rest.contains("=>") || rest.contains("function");
            return Some((name, strong));
        }
    }
    None
}

fn ident_at(s: &str) -> Option<String> {
    let s = s.trim_start();
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some(s[..end].to_string())
}
