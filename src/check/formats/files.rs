//! What every file-based format shares: the same key twice in one file (the last one
//! wins silently), keys a locale file has and the source does not, and gettext's fuzzy
//! flag, which ships as untranslated.

use crate::check::code::{Code, Severity};
use crate::check::plurals;
use crate::check::report::Cx;
use crate::config::Format;
use crate::formats;
use anyhow::Result;
use std::collections::BTreeSet;

pub fn check(cx: &mut Cx, idx: usize) -> Result<()> {
    let format = cx.spec(idx).format;
    let source_locale = cx.cfg.source_locale.clone();
    let text = crate::formats::read_text(&cx.source_path(idx))?;
    for key in duplicate_keys(format, &text)? {
        cx.emit(
            idx,
            &key,
            &source_locale,
            Code::Duplicate,
            Severity::Error,
            "key appears more than once in this file",
        );
    }
    if cx.spec(idx).locale_path.is_none() {
        return Ok(());
    }
    let source_keys = keys_of(format, &text)?;
    let locales = cx.locales.clone();
    for locale in &locales {
        let Some(lp) = cx.locale_path(idx, locale) else {
            break;
        };
        if !lp.exists() {
            continue;
        }
        let ltext = crate::formats::read_text(&lp)?;
        for key in duplicate_keys(format, &ltext)? {
            cx.emit(
                idx,
                &key,
                locale,
                Code::Duplicate,
                Severity::Error,
                "key appears more than once in this file",
            );
        }
        for key in keys_of(format, &ltext)? {
            if source_keys.contains(&key) {
                continue;
            }
            // `photos_few` next to a source `photos_one`/`photos_other` is a plural form
            // the locale needs, not an orphan.
            if format == Format::Json
                && let Some((base, _)) = plurals::i18next_split(&key)
                && source_keys.contains(&format!("{base}_other"))
            {
                continue;
            }
            cx.emit(
                idx,
                &key,
                locale,
                Code::Orphan,
                Severity::Warning,
                "not in the source file: delete it, or restore the source key",
            );
        }
        if format == Format::Po {
            let doc = formats::po::parse(&ltext)?;
            for e in doc
                .entries
                .iter()
                .filter(|e| e.fuzzy && !e.msgid.is_empty())
            {
                cx.emit(
                    idx,
                    &e.key(),
                    locale,
                    Code::Fuzzy,
                    Severity::Warning,
                    "marked fuzzy: gettext shows the source text instead; review it and drop the flag",
                );
            }
        }
    }
    Ok(())
}

/// Every key a file defines, for the orphan check.
fn keys_of(format: Format, text: &str) -> Result<BTreeSet<String>> {
    Ok(all_keys(format, text)?.into_iter().collect())
}

/// Keys in file order, duplicates included.
fn all_keys(format: Format, text: &str) -> Result<Vec<String>> {
    Ok(match format {
        Format::Json => formats::json::parse(text)?
            .entries
            .iter()
            .map(|e| e.key())
            .collect(),
        Format::Arb => formats::arb::values(&formats::arb::parse(text)?)
            .into_keys()
            .collect(),
        Format::Android => formats::android::parse(text)?
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect(),
        Format::Po => formats::po::parse(text)?
            .entries
            .iter()
            .filter(|e| !e.msgid.is_empty())
            .map(|e| e.key())
            .collect(),
        Format::Resx => formats::resx::parse(text)?
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect(),
        Format::Strings => formats::strings::parse(text)?
            .entries
            .iter()
            .map(|e| e.key.clone())
            .collect(),
        Format::Xcstrings => vec![],
    })
}

/// Keys defined more than once in one file, in first-seen order. Android compares the
/// pair (resource type, name): a string and a plurals may share a name. ARB `values`
/// (non-`@` keys) and JSON both come from the same parser; a String Catalog written by
/// Xcode never has duplicates (serde keeps the last).
fn duplicate_keys(format: Format, text: &str) -> Result<Vec<String>> {
    let all: Vec<String> = match format {
        Format::Android => formats::android::parse(text)?
            .entries
            .iter()
            .map(|e| format!("{:?}\u{0}{}", e.kind, e.name))
            .collect(),
        Format::Arb => formats::json::parse(text)?
            .entries
            .iter()
            .map(|e| e.key())
            .collect(),
        _ => all_keys(format, text)?,
    };
    let mut seen = BTreeSet::new();
    let mut dups = Vec::new();
    for k in all {
        if !seen.insert(k.clone()) && !dups.contains(&k) {
            dups.push(k);
        }
    }
    Ok(dups
        .into_iter()
        .map(|k| k.rsplit('\u{0}').next().unwrap_or(&k).to_string())
        .collect())
}
