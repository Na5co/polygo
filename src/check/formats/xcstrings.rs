//! String Catalogs: Xcode's own state bookkeeping, and plural variations per locale.

use crate::check::code::{Code, Severity};
use crate::check::plurals;
use crate::check::report::Cx;
use crate::formats;
use anyhow::Result;

pub fn check(cx: &mut Cx, idx: usize) -> Result<()> {
    let path = cx.source_path(idx);
    let doc = formats::xcstrings::parse(&crate::formats::read_text(&path)?)?;
    let source_locale = cx.cfg.source_locale.clone();
    // A unit marked needs_review / stale / new, shipped as is; a key whose
    // extractionState is stale (Xcode no longer finds it in the code).
    for (key, locale, state) in states(&doc, &source_locale) {
        if !locale.is_empty() && !cx.locales.contains(&locale) {
            continue;
        }
        if locale.is_empty() {
            // The key's own line, since no locale entry is involved.
            let (file, line) = cx.locate(idx, &key, "");
            cx.emit_at(
                idx,
                file,
                line,
                &key,
                &source_locale,
                Code::State,
                Severity::Warning,
                "extractionState is stale: Xcode no longer finds this key in the code",
            );
        } else {
            cx.emit(
                idx,
                &key,
                &locale,
                Code::State,
                Severity::Warning,
                format!("marked `{state}` in Xcode, shipped as is"),
            );
        }
    }
    for (key, locale, cats) in plurals::xcstrings_plurals(&doc) {
        if !cx.locales.contains(&locale) {
            continue;
        }
        let missing = plurals::missing(&locale, &cats);
        if !missing.is_empty() {
            cx.emit(
                idx,
                &key,
                &locale,
                Code::Plural,
                Severity::Error,
                format!("missing plural form(s) {}", missing.join(", ")),
            );
        }
    }
    Ok(())
}

/// `(key, locale, state)` for every string unit in a target locale whose state is not
/// `translated`, and `(key, "", "stale")` for keys with `extractionState = stale`.
fn states(
    doc: &formats::xcstrings::Document,
    source_locale: &str,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Some(strings) = doc.root.get("strings").and_then(|v| v.as_object()) else {
        return out;
    };
    for (key, entry) in strings {
        if entry.get("extractionState").and_then(|v| v.as_str()) == Some("stale") {
            out.push((key.clone(), String::new(), "stale".into()));
        }
        let Some(locs) = entry.get("localizations").and_then(|v| v.as_object()) else {
            continue;
        };
        for (locale, l) in locs {
            if locale == source_locale {
                continue;
            }
            let mut states = Vec::new();
            if let Some(s) = l.pointer("/stringUnit/state").and_then(|v| v.as_str()) {
                states.push(s);
            }
            if let Some(p) = l.pointer("/variations/plural").and_then(|v| v.as_object()) {
                states.extend(
                    p.values()
                        .filter_map(|c| c.pointer("/stringUnit/state").and_then(|v| v.as_str())),
                );
            }
            if let Some(s) = states
                .into_iter()
                .find(|s| matches!(*s, "needs_review" | "stale" | "new"))
            {
                out.push((key.clone(), locale.clone(), s.to_string()));
            }
        }
    }
    out
}
