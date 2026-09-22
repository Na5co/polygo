//! Android resources: what aapt rejects at build time, arrays that changed length, and
//! `<plurals>` per locale.

use crate::check::code::{Code, Severity};
use crate::check::report::Cx;
use crate::check::{content, plurals};
use crate::formats;
use anyhow::{Context, Result};
use std::collections::BTreeMap;

pub fn check(cx: &mut Cx, idx: usize) -> Result<()> {
    let path = cx.source_path(idx);
    let source = formats::android::parse(&crate::formats::read_text(&path)?)
        .with_context(|| format!("parsing {}", path.display()))?;
    let source_locale = cx.cfg.source_locale.clone();
    // The source file breaks the build exactly like a translation does.
    escapes(cx, idx, &source, &source_locale);
    let source_arrays = array_len(&source);
    let locales = cx.locales.clone();
    for locale in &locales {
        let Some(lp) = cx.locale_path(idx, locale) else {
            break;
        };
        if !lp.exists() {
            continue;
        }
        let doc = formats::android::parse(&crate::formats::read_text(&lp)?)
            .with_context(|| format!("parsing {}", lp.display()))?;
        escapes(cx, idx, &doc, locale);
        // Arrays are positional: a shorter one is an IndexOutOfBounds at runtime, a
        // longer one shows items the source never had.
        for (name, n) in array_len(&doc) {
            if let Some(&src_n) = source_arrays.get(&name)
                && src_n != n
            {
                cx.emit(
                    idx,
                    &name,
                    locale,
                    Code::Array,
                    Severity::Error,
                    format!("{n} item(s) vs {src_n} in the source"),
                );
            }
        }
        for (name, cats) in plurals::android_plurals(&doc) {
            let missing = plurals::missing(locale, &cats);
            if !missing.is_empty() {
                cx.emit(
                    idx,
                    &name,
                    locale,
                    Code::Plural,
                    Severity::Error,
                    format!("missing plural form(s) {}", missing.join(", ")),
                );
            }
        }
    }
    Ok(())
}

/// An apostrophe or a double quote that is not escaped, a leading @ or ? (a resource
/// reference), and two or more unnumbered format arguments.
fn escapes(cx: &mut Cx, idx: usize, doc: &formats::android::Document, locale: &str) {
    for e in &doc.entries {
        if !e.translatable {
            continue;
        }
        for v in &e.values {
            if let Some((sev, m)) = escape_problem(&v.raw) {
                cx.emit(idx, &e.name, locale, Code::Escape, sev, m);
            }
            if e.formatted
                && let Some(m) = content::android_unnumbered_args(&v.text())
            {
                cx.emit(idx, &e.name, locale, Code::Placeholders, Severity::Error, m);
            }
        }
    }
}

fn array_len(doc: &formats::android::Document) -> BTreeMap<String, usize> {
    doc.entries
        .iter()
        .filter(|e| e.kind == formats::android::Kind::StringArray)
        .map(|e| (e.name.clone(), e.values.len()))
        .collect()
}

/// Android resource strings that `aapt` refuses: an unescaped `'` (must be `\'` or the
/// whole value wrapped in `"…"`), or a value starting with `@` / `?` that is not a
/// resource reference. A stray unbalanced `"` is a warning: Android drops it from the
/// text. Tags, CDATA and entities are left alone.
pub fn escape_problem(raw: &str) -> Option<(Severity, String)> {
    let t = raw.trim();
    if t.starts_with("<![CDATA[") {
        return None;
    }
    if (t.starts_with('@') || t.starts_with('?')) && !is_reference(t) {
        return Some((
            Severity::Error,
            format!(
                "starts with `{}`: Android reads that as a resource reference; write `\\{}`",
                &t[..1],
                &t[..1]
            ),
        ));
    }
    let quoted = t.len() >= 2 && t.starts_with('"') && t.ends_with('"');
    if quoted {
        return None;
    }
    let b = t.as_bytes();
    let mut i = 0;
    let mut in_tag = false;
    let mut dq = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'<' => in_tag = true,
            b'>' => in_tag = false,
            b'\'' if !in_tag => {
                return Some((
                    Severity::Error,
                    "unescaped apostrophe: Android needs `\\'` (or wrap the whole string in double quotes)"
                        .into(),
                ));
            }
            b'"' if !in_tag => dq += 1,
            _ => {}
        }
        i += 1;
    }
    // An unescaped " toggles whitespace mode and is dropped from the text; a stray one
    // silently disappears from the UI (DuckDuckGo and NewPipe both ship one).
    if dq % 2 == 1 {
        return Some((
            Severity::Warning,
            "stray double quote: Android drops it from the text; escape it as `\\\"`".into(),
        ));
    }
    None
}

fn is_reference(t: &str) -> bool {
    let body = &t[1..];
    let body = body.strip_prefix('+').unwrap_or(body);
    let (pkg_type, name) = match body.split_once('/') {
        Some(x) => x,
        None => return false,
    };
    let ty = pkg_type.rsplit(':').next().unwrap_or(pkg_type);
    !name.is_empty()
        && ty.chars().all(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}
