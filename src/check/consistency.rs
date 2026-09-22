//! The same short source text translated two ways in one locale ("Cancel" → "Abbrechen"
//! here, "Abbruch" there): the minority gets the warning, with the counts.

use crate::check::code::Code;
use crate::check::report::Cx;
use crate::core::Unit;
use crate::project;
use std::collections::BTreeMap;

pub fn check(cx: &mut Cx, units: &[Unit]) {
    let locales = cx.locales.clone();
    for locale in &locales {
        let mut by_source: BTreeMap<&str, Vec<(&Unit, &str)>> = BTreeMap::new();
        for u in units {
            if crate::core::split_plural(&u.key).is_some() || !u.applies_to(locale) {
                continue;
            }
            let Some(t) = u.translations.get(locale) else {
                continue;
            };
            let src = u.source.trim();
            // UI terms only: a few words, no placeholders or markup, not a sentence.
            let words = src.split_whitespace().count();
            if words == 0
                || words > 3
                || src.contains(['%', '{', '<', '$'])
                || src.chars().filter(|c| c.is_alphabetic()).count() < 3
            {
                continue;
            }
            // An untranslated copy is the `identical` warning's business, not a vote.
            if t.trim() == src {
                continue;
            }
            by_source.entry(src).or_default().push((u, t.trim()));
        }
        for (src, group) in by_source {
            if group.len() < 2 {
                continue;
            }
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut shown: BTreeMap<String, &str> = BTreeMap::new();
            for (_, t) in &group {
                let l = t.to_lowercase();
                *counts.entry(l.clone()).or_default() += 1;
                shown.entry(l).or_insert(t);
            }
            if counts.len() < 2 {
                continue;
            }
            let mut ranked: Vec<(&String, &usize)> = counts.iter().collect();
            ranked.sort_by_key(|(t, n)| (std::cmp::Reverse(**n), (*t).clone()));
            let (majority, n) = (ranked[0].0.clone(), *ranked[0].1);
            // A tie is a choice, not a mistake.
            if ranked.get(1).is_some_and(|(_, m)| **m == n) {
                continue;
            }
            let majority_shown = shown
                .get(&majority)
                .copied()
                .unwrap_or(majority.as_str())
                .to_string();
            for (u, t) in &group {
                let tl = t.to_lowercase();
                if tl == majority || inflection_of(&tl, &majority) {
                    continue;
                }
                let (idx, local_key) = project::split_key(cx.cfg, &u.key);
                cx.warn_or_err(
                    idx,
                    local_key,
                    locale,
                    Code::Inconsistent,
                    format!("`{src}` is `{majority_shown}` in {n} other key(s), here `{t}`"),
                );
            }
        }
    }
}

/// `aucune` vs `aucun`, `attiva` vs `attivo`, `essayer` vs `essayez`: the same word
/// agreeing with a different noun or mood, not a different translation.
fn inflection_of(a: &str, b: &str) -> bool {
    let common = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    let shorter = a.chars().count().min(b.chars().count());
    common >= 3 && common + 2 >= shorter && a.chars().count().abs_diff(b.chars().count()) <= 2
}
