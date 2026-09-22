//! Checks that look at one translation next to its source: placeholders, text sanity,
//! ICU plurals, glossary, content. One `Cx::emit` per finding, located once per unit.

use crate::check::code::{Code, Severity};
use crate::check::report::Cx;
use crate::check::{content, placeholders, plurals, text};
use crate::core::Unit;
use crate::glossary::Glossary;
use crate::lockfile::Lock;
use crate::project;

pub fn check(cx: &mut Cx, units: &[Unit], lock: &Lock, glossary: &Glossary, length_ratio: f64) {
    let locales = cx.locales.clone();
    for u in units {
        let (idx, local_key) = project::split_key(cx.cfg, &u.key);
        for locale in &locales {
            let Some(t) = u.translations.get(locale) else {
                continue;
            };
            cx.report.checked += 1;
            for (code, severity, message) in one(cx, u, lock, glossary, length_ratio, locale, t) {
                cx.emit(idx, local_key, locale, code, severity, message);
            }
        }
    }
}

/// Every finding for one translation, in a fixed order.
fn one(
    cx: &Cx,
    u: &Unit,
    lock: &Lock,
    glossary: &Glossary,
    length_ratio: f64,
    locale: &str,
    t: &str,
) -> Vec<(Code, Severity, String)> {
    let mut found = Vec::new();
    if let Some(max) = crate::core::directives(u.comment.as_deref()).max_chars
        && t.chars().count() > max
    {
        found.push((
            Code::Length,
            Severity::Error,
            format!(
                "{} chars, but the key allows at most {max} (polygo:max)",
                t.chars().count()
            ),
        ));
    }
    // Plural forms: an exact-count form may leave the number out, a Russian `one`
    // (also 21, 31…) may not.
    let mismatch = match crate::core::split_plural(&u.key) {
        Some((_, cat)) => plurals::compare_forms(locale, cat, &u.source, t),
        None => placeholders::compare(&u.source, t),
    };
    if let Some(m) = mismatch {
        found.push((Code::Placeholders, Severity::Error, m.to_string()));
    }
    // A translation polygo wrote and confirmed (recorded in the lockfile) is not
    // re-flagged as identical: the model was asked twice and kept it.
    let confirmed = lock
        .keys
        .get(&u.key)
        .and_then(|r| r.locales.get(locale))
        .is_some_and(|l| l.hash == crate::core::hash(t));
    for f in text::check_text(&u.source, t, &cx.cfg.source_locale, locale, length_ratio) {
        if f.code == Code::Identical && confirmed {
            continue;
        }
        found.push((f.code, f.severity, f.message));
    }
    for (arg, cats) in plurals::icu_cases(t) {
        let missing = plurals::missing(locale, &cats);
        if !missing.is_empty() {
            found.push((
                Code::Plural,
                Severity::Error,
                format!("ICU plural `{arg}` is missing {}", missing.join(", ")),
            ));
        }
    }
    for v in glossary.violations(locale, &u.source, t) {
        found.push((Code::Glossary, Severity::Error, v));
    }
    let warn = |code: Code, m: Option<String>| m.map(|m| (code, Severity::Warning, m));
    found.extend(warn(Code::Encoding, content::mojibake(t)));
    found.extend(warn(Code::Invisible, content::invisible(t)));
    found.extend(warn(Code::Link, content::link_mismatch(&u.source, t)));
    found.extend(warn(Code::Brackets, content::unbalanced(&u.source, t)));
    found.extend(warn(Code::Entities, content::double_escaped(t)));
    found
}
