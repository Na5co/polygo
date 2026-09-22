//! i18next JSON: `key_one` / `key_other` groups need every suffix the locale requires.

use crate::check::code::{Code, Severity};
use crate::check::plurals;
use crate::check::report::Cx;
use crate::formats;
use anyhow::{Context, Result};

pub fn check(cx: &mut Cx, idx: usize) -> Result<()> {
    let path = cx.source_path(idx);
    let source = formats::json::parse(&crate::formats::read_text(&path)?)?;
    let source_groups =
        plurals::i18next_plural_groups(&source.entries.iter().map(|e| e.key()).collect::<Vec<_>>());
    let locales = cx.locales.clone();
    for locale in &locales {
        let Some(lp) = cx.locale_path(idx, locale) else {
            break;
        };
        if !lp.exists() {
            continue;
        }
        let doc = formats::json::parse(&crate::formats::read_text(&lp)?)
            .with_context(|| format!("parsing {}", lp.display()))?;
        let groups =
            plurals::i18next_groups(&doc.entries.iter().map(|e| e.key()).collect::<Vec<_>>());
        for base in source_groups.keys() {
            let cats = groups.get(base).cloned().unwrap_or_default();
            let missing = plurals::missing(locale, &cats);
            if !missing.is_empty() {
                // Located at `base_other`, which the locale has when anything of the group does.
                let (file, line) = cx.locate(idx, &format!("{base}_other"), locale);
                cx.emit_at(
                    idx,
                    file,
                    line,
                    base,
                    locale,
                    Code::Plural,
                    Severity::Error,
                    format!(
                        "i18next plural keys missing: {}",
                        missing
                            .iter()
                            .map(|c| format!("{base}_{c}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
    }
    Ok(())
}
