//! Legacy Apple `.strings` projects keep their plurals in `.stringsdict`: every dict in
//! the source `.lproj` (any name: Signal ships `PluralAware.stringsdict`) is checked
//! against each locale's copy. One `.lproj` may hold several `.strings` specs; its
//! dicts are checked once.

use crate::check::code::{Code, Severity};
use crate::check::plurals;
use crate::check::report::Cx;
use crate::formats;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub fn check(cx: &mut Cx, idx: usize, seen: &mut BTreeSet<PathBuf>) -> Result<()> {
    let path = cx.source_path(idx);
    let Some(src_dir) = path.parent() else {
        return Ok(());
    };
    if cx.spec(idx).locale_path.is_none() {
        return Ok(());
    }
    let mut dicts: Vec<PathBuf> = std::fs::read_dir(src_dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "stringsdict"))
                .collect()
        })
        .unwrap_or_default();
    dicts.sort();
    let locales = cx.locales.clone();
    for src_dict in dicts {
        if !seen.insert(src_dict.clone()) {
            continue;
        }
        let source = formats::stringsdict::parse(&crate::formats::read_text(&src_dict)?)
            .with_context(|| format!("parsing {}", src_dict.display()))?;
        let name = src_dict.file_name().unwrap().to_owned();
        for locale in &locales {
            let Some(lp) = cx.locale_path(idx, locale) else {
                break;
            };
            let lp = lp.with_file_name(&name);
            if !lp.exists() {
                continue;
            }
            let rel = lp
                .strip_prefix(cx.root)
                .unwrap_or(&lp)
                .display()
                .to_string()
                .replace('\\', "/");
            let doc = formats::stringsdict::parse(&crate::formats::read_text(&lp)?)
                .with_context(|| format!("parsing {}", lp.display()))?;
            for (key, se) in &source.entries {
                let Some(te) = doc.entries.get(key) else {
                    cx.emit_at(
                        idx,
                        rel.clone(),
                        None,
                        key,
                        locale,
                        Code::Plural,
                        Severity::Warning,
                        "not in this locale's .stringsdict: iOS falls back to the source language",
                    );
                    continue;
                };
                // A variable is referenced from the format key or from another
                // variable's forms (`%d %2$#@total@`).
                let refs: String = std::iter::once(se.format.as_str())
                    .chain(
                        se.variables
                            .values()
                            .flat_map(|v| v.forms.iter().map(|(_, f)| f.as_str())),
                    )
                    .collect::<Vec<_>>()
                    .join(" ");
                for (var, sv) in &se.variables {
                    let Some(tv) = te.variables.get(var) else {
                        cx.emit_at(
                            idx,
                            rel.clone(),
                            Some(te.line),
                            &format!("{key}#{var}"),
                            locale,
                            Code::Plural,
                            Severity::Error,
                            format!("variable `{var}` is missing"),
                        );
                        continue;
                    };
                    let cats: Vec<String> = tv.forms.iter().map(|(c, _)| c.clone()).collect();
                    let missing = plurals::missing(locale, &cats);
                    if !missing.is_empty() {
                        cx.emit_at(
                            idx,
                            rel.clone(),
                            Some(te.line),
                            &format!("{key}#{var}"),
                            locale,
                            Code::Plural,
                            Severity::Error,
                            format!("missing plural form(s) {}", missing.join(", ")),
                        );
                    }
                    let src_other = sv
                        .forms
                        .iter()
                        .find(|(c, _)| c == "other")
                        .or(sv.forms.last())
                        .map(|(_, t)| t.as_str())
                        .unwrap_or("");
                    let pos = plurals::stringsdict_position(&refs, var);
                    for (cat, form) in &tv.forms {
                        let src_form = sv
                            .forms
                            .iter()
                            .find(|(c, _)| c == cat)
                            .map_or(src_other, |(_, t)| t.as_str());
                        // Inside a variable `%d` is that variable's argument; in an
                        // exact-count form the number may be left out.
                        let (s, t) = (
                            plurals::renumber(src_form, pos),
                            plurals::renumber(form, pos),
                        );
                        if let Some(m) = plurals::compare_forms(locale, cat, &s, &t) {
                            cx.emit_at(
                                idx,
                                rel.clone(),
                                Some(te.line),
                                &format!("{key}#{var}.{cat}"),
                                locale,
                                Code::Placeholders,
                                Severity::Error,
                                m.to_string(),
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
