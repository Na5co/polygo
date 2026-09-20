//! Format-independent model: a translatable unit is a key, its source-language
//! text, and whatever translations currently exist.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Unit {
    pub key: String,
    pub source: String,
    /// Developer comment / context attached to the key, if the format has one.
    pub comment: Option<String>,
    /// locale → translated text (only locales that actually have a value).
    pub translations: BTreeMap<String, String>,
    /// Target locales this unit applies to; `None` means every target locale.
    /// Plural forms use it: German needs `one`/`other`, Polish also `few`/`many`.
    pub locales: Option<Vec<String>>,
}

impl Unit {
    pub fn applies_to(&self, locale: &str) -> bool {
        self.locales
            .as_ref()
            .is_none_or(|l| l.iter().any(|x| x == locale))
    }
}

/// BLAKE3 hash of a string, hex-encoded, truncated to 16 bytes (32 hex chars):
/// plenty for change detection and short enough to keep the lockfile readable.
pub fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..32].to_string()
}

// ---- plural forms ------------------------------------------------------------------
//
// A plural string is translated as one unit per CLDR category the *target* locale
// needs. The unit key is `<key>#plural.<category>`; the source text of a category the
// source language lacks (`few` for English) is the source's `other` form.

pub const PLURAL_SEP: &str = "#plural.";

pub fn plural_key(key: &str, category: &str) -> String {
    format!("{key}{PLURAL_SEP}{category}")
}

/// `("key", "few")` for `key#plural.few`.
pub fn split_plural(key: &str) -> Option<(&str, &str)> {
    let (base, cat) = key.rsplit_once(PLURAL_SEP)?;
    if cat.is_empty() || cat.contains('#') {
        return None;
    }
    Some((base, cat))
}

// ---- string-array items ----------------------------------------------------------------
//
// An Android `<string-array>` is one unit per item, keyed `<name>#array.<index>`, so
// each item is translated, checked and tracked on its own while the array keeps its order.

pub const ARRAY_SEP: &str = "#array.";

pub fn array_key(name: &str, index: usize) -> String {
    format!("{name}{ARRAY_SEP}{index}")
}

/// `("name", 2)` for `name#array.2`.
pub fn split_array(key: &str) -> Option<(&str, usize)> {
    let (base, idx) = key.rsplit_once(ARRAY_SEP)?;
    Some((base, idx.parse().ok()?))
}

/// One unit per array item. `targets`: locale → that locale's existing items.
pub fn array_units(
    name: &str,
    comment: Option<&str>,
    items: &[String],
    targets: &BTreeMap<String, Vec<String>>,
) -> Vec<Unit> {
    let list: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {t:?}", i + 1))
        .collect();
    items
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let mut note = format!(
                "Item {} of {} in the list {name:?}; the items are shown together in this order, so keep them parallel in form. Full list: {}.",
                i + 1,
                items.len(),
                list.join(", ")
            );
            if let Some(c) = comment {
                note = format!("{c} · {note}");
            }
            Unit {
                key: array_key(name, i),
                source: text.clone(),
                comment: Some(note),
                translations: targets
                    .iter()
                    .filter_map(|(l, have)| have.get(i).map(|v| (l.clone(), v.clone())))
                    .filter(|(_, v)| !v.is_empty())
                    .collect(),
                locales: None,
            }
        })
        .collect()
}

/// The key without a plural or array suffix (for code-usage lookup and display).
pub fn base_key(key: &str) -> &str {
    split_plural(key)
        .map(|(b, _)| b)
        .or_else(|| split_array(key).map(|(b, _)| b))
        .unwrap_or(key)
}

/// What a category means, for the model: with a concrete count, because "few" vs
/// "many" is exactly what a model gets wrong without one.
pub fn plural_hint(category: &str) -> &'static str {
    match category {
        "zero" => "the form used when the count is 0",
        "one" => "the form used when the count is 1 (in some languages also 21, 31, …)",
        "two" => "the form used when the count is 2",
        "few" => "the form used when the count is 3 (typically 2–4, e.g. 3 or 23)",
        "many" => "the form used when the count is 11 (typically 5–20 and 25+, e.g. 11 or 45)",
        "other" => "the general form, used when the count is e.g. 100 or 1.5",
        _ => "the form used for that exact count",
    }
}

/// Order categories the way CLDR (and Xcode/Android) list them.
pub fn category_order(c: &str) -> u8 {
    match c {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "few" => 3,
        "many" => 4,
        "other" => 5,
        _ => 6,
    }
}

/// Build one unit per category from a source plural's forms.
///
/// `targets`: for every target locale, the categories it needs and the forms it already
/// has. Categories the source has but no target needs (an explicit `zero`) are kept for
/// every locale.
pub fn plural_units(
    key: &str,
    comment: Option<&str>,
    source_forms: &BTreeMap<String, String>,
    targets: &BTreeMap<String, (Vec<String>, BTreeMap<String, String>)>,
) -> Vec<Unit> {
    let Some(other) = source_forms
        .get("other")
        .or_else(|| source_forms.values().next_back())
    else {
        return vec![];
    };
    let mut forms_list: Vec<String> = source_forms
        .iter()
        .map(|(c, v)| format!("{c} = {v:?}"))
        .collect();
    forms_list.sort();
    // An explicit `zero` (or an exact `=N`) in the source is a stylistic choice every
    // locale should mirror; `one`/`few`/… are only produced where the locale needs them.
    let stylistic = |c: &str| matches!(category_order(c), 0 | 6);
    let mut categories: Vec<String> = targets.values().flat_map(|(c, _)| c.clone()).collect();
    categories.extend(source_forms.keys().filter(|c| stylistic(c)).cloned());
    categories.sort_by_key(|c| (category_order(c), c.clone()));
    categories.dedup();
    categories
        .into_iter()
        .map(|cat| {
            let locales: Vec<String> = targets
                .iter()
                .filter(|(_, (cats, _))| {
                    cats.contains(&cat) || (stylistic(&cat) && source_forms.contains_key(&cat))
                })
                .map(|(l, _)| l.clone())
                .collect();
            let translations = targets
                .iter()
                .filter_map(|(l, (_, have))| have.get(&cat).map(|v| (l.clone(), v.clone())))
                .filter(|(_, v)| !v.is_empty())
                .collect();
            let mut note = format!(
                "Plural form `{cat}` of {key:?}: write {}. Use the correct noun/verb inflection for that count in the target language. Source forms: {}.",
                plural_hint(&cat),
                forms_list.join(", ")
            );
            if let Some(c) = comment {
                note = format!("{c} · {note}");
            }
            Unit {
                key: plural_key(key, &cat),
                source: source_forms.get(&cat).unwrap_or(other).clone(),
                comment: Some(note),
                translations,
                locales: Some(locales),
            }
        })
        .collect()
}

// ---- per-key directives in developer comments ------------------------------------------
//
// `polygo:skip`          never translate this key (kept out of status and checks)
// `polygo:max=20`        translations may be at most 20 characters (check error, repair)
// `polygo:context=…`     free text for the model: the whole comment is sent anyway;
//                        this just makes the intent explicit in the file

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directives {
    pub skip: bool,
    pub max_chars: Option<usize>,
}

pub fn directives(comment: Option<&str>) -> Directives {
    let mut d = Directives::default();
    let Some(c) = comment else {
        return d;
    };
    for tok in c.split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';') {
        let Some(rest) = tok.strip_prefix("polygo:") else {
            continue;
        };
        if rest == "skip" {
            d.skip = true;
        } else if let Some(n) = rest.strip_prefix("max=") {
            d.max_chars = n
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok();
        }
    }
    d
}
