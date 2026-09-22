//! A string file that does not parse. Without this pass a locale file someone broke — a
//! comma dropped from a JSON catalog, an unclosed `<string>` — is simply not there: the
//! detector cannot read it, so the locale disappears from the run and `check` says the
//! project is fine. That is the one answer `check` must never give.

use crate::check::code::{Code, Severity};
use crate::config::{Config, FileSpec, Format};
use crate::formats;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// A file polygo cannot read, and why.
pub struct Broken {
    /// Index of the `[[files]]` spec it belongs to.
    pub idx: usize,
    /// Path relative to the checked root, as findings name files.
    pub file: String,
    pub line: Option<usize>,
    /// The locale the path stands for, or the source locale for a source file.
    pub locale: String,
    pub message: String,
}

/// Every string file of the project that does not parse: the source, and each file the
/// `locale_path` template matches — including locales the configuration never mentions,
/// which is exactly where a broken file hides.
pub fn scan(root: &Path, cfg: &Config) -> Result<Vec<Broken>> {
    let mut out = Vec::new();
    for (idx, spec) in cfg.files.iter().enumerate() {
        let source = root.join(&spec.path);
        if source.is_file()
            && let Some((line, message)) = parse_error(&source, spec.format)
        {
            out.push(Broken {
                idx,
                file: spec.path.to_string_lossy().replace('\\', "/"),
                line,
                locale: cfg.source_locale.clone(),
                message,
            });
        }
        for (locale, path) in locale_files(root, spec) {
            if let Some((line, message)) = parse_error(&path, spec.format) {
                out.push(Broken {
                    idx,
                    file: path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    line,
                    locale,
                    message,
                });
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

/// Report the broken files and say whether the rest of the check can still run: a file the
/// project means to read (a configured target locale, or the source) stops everything,
/// since loading the units would fail on it anyway.
pub fn check(cx: &mut crate::check::report::Cx, broken: &[Broken]) -> bool {
    let mut fatal = false;
    for b in broken {
        let is_source = b.locale == cx.cfg.source_locale;
        fatal |= is_source || cx.cfg.target_locales.contains(&b.locale);
        cx.emit_at(
            b.idx,
            b.file.clone(),
            b.line,
            &b.file,
            &b.locale,
            Code::Syntax,
            Severity::Error,
            format!("cannot be parsed: {}", b.message),
        );
    }
    fatal
}

/// `(line, message)` when the file does not parse, `None` when it does.
fn parse_error(path: &Path, format: Format) -> Option<(Option<usize>, String)> {
    let text = match formats::read_text(path) {
        Ok(t) => t,
        Err(e) => return Some((None, one_line(&format!("{e:#}")))),
    };
    let parsed = match format {
        Format::Xcstrings => formats::xcstrings::parse(&text).map(|_| ()),
        Format::Android => formats::android::parse(&text).map(|_| ()),
        Format::Json => formats::json::parse(&text).map(|_| ()),
        Format::Arb => formats::arb::parse(&text).map(|_| ()),
        Format::Po => formats::po::parse(&text).map(|_| ()),
        Format::Resx => formats::resx::parse(&text).map(|_| ()),
        Format::Strings => formats::strings::parse(&text).map(|_| ()),
    };
    let e = parsed.err()?;
    let message = one_line(&format!("{e:#}"));
    Some((line_of(&message, &text), message))
}

/// Where the fix goes. Parsers say either "at line 584 column 2" or "at byte 34886"; a
/// byte offset is turned into the line that holds it.
fn line_of(message: &str, text: &str) -> Option<usize> {
    let number = |after: &str| -> Option<usize> {
        let rest = message.split(after).nth(1)?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    if let Some(line) = number("line ").filter(|n| *n > 0) {
        return Some(line);
    }
    let byte = number("byte ")?;
    let before = text.get(..byte.min(text.len()))?;
    let line = before.matches('\n').count() + 1;
    Some(line.min(text.lines().count().max(1)))
}

fn one_line(message: &str) -> String {
    // anyhow chains the context onto the error, and a parser that already says what went
    // wrong then says it twice.
    let flat = message.replace('\n', " ");
    let mut parts: Vec<&str> = flat.split(": ").collect();
    parts.dedup();
    let m = parts.join(": ");
    if m.chars().count() > 200 {
        format!("{}…", m.chars().take(200).collect::<String>())
    } else {
        m
    }
}

/// Existing files the spec's `locale_path` template matches, with the locale each stands
/// for. `{locale}` and `{android_locale}` never span a path separator, so only the
/// directory the template's static part points at is walked.
fn locale_files(root: &Path, spec: &FileSpec) -> Vec<(String, PathBuf)> {
    let Some(template) = &spec.locale_path else {
        return Vec::new();
    };
    let token = ["{locale}", "{android_locale}"]
        .into_iter()
        .find(|t| template.contains(t));
    let Some((pre, post)) = token.and_then(|t| template.split_once(t)) else {
        return Vec::new();
    };
    // The deepest directory that does not depend on the locale.
    let (dir, name_pre) = match pre.rsplit_once('/') {
        Some((d, rest)) => (root.join(d), rest.to_string()),
        None => (root.to_path_buf(), pre.to_string()),
    };
    // What follows the locale: the rest of that path segment, then any deeper path.
    let (name_post, tail) = match post.split_once('/') {
        Some((p, t)) => (p.to_string(), Some(t.to_string())),
        None => (post.to_string(), None),
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(locale) = name
            .strip_prefix(&name_pre)
            .and_then(|r| r.strip_suffix(&name_post))
            .filter(|l| !l.is_empty())
        else {
            continue;
        };
        let path = match &tail {
            Some(t) => entry.path().join(t),
            None => entry.path(),
        };
        if path.is_file() {
            out.push((android_to_bcp47(locale), path));
        }
    }
    out.sort();
    out
}

/// `values-pt-rBR` names `pt-BR`; every other template already holds the locale as
/// configured.
fn android_to_bcp47(dir_locale: &str) -> String {
    match crate::init::android_dir_locale(&format!("values-{dir_locale}")) {
        Some(l) => l,
        None => dir_locale.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_comes_from_whatever_the_parser_says() {
        let text = "a\nbb\nccc\n";
        assert_eq!(line_of("expected `,` at line 2 column 3", text), Some(2));
        // Byte 5 is in the third line ("ccc" starts at byte 5).
        assert_eq!(line_of("expected ',' or '}' at byte 5", text), Some(3));
        assert_eq!(line_of("expected ',' at byte 0", text), Some(1));
        assert_eq!(line_of("beyond the end at byte 9999", text), Some(3));
        assert_eq!(line_of("no numbers here", text), None);
    }
}
