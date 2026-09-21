//! CLDR plural-category completeness.
//!
//! Every locale needs a specific set of cardinal categories. Extra categories
//! (`zero`, `=0`, an unnecessary `one` in Japanese) are harmless; missing ones make
//! the UI show the wrong form or fall through. The table below covers the
//! locales apps actually ship; unknown locales default to `one`/`other`.
//! The CLDR 42+ `many` for fr/es/it/ca/pt (millions) is accepted but not required,
//! because neither Xcode nor Android tooling requires it in practice.

use std::collections::BTreeMap;

/// Required cardinal categories for a locale (BCP-47 tag; region/script ignored).
pub fn required(locale: &str) -> &'static [&'static str] {
    let lang = locale
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match lang.as_str() {
        // No plural distinction.
        "ja" | "ko" | "zh" | "yue" | "vi" | "th" | "id" | "in" | "ms" | "lo" | "km" | "my"
        | "jv" | "su" | "ig" | "yo" | "wo" | "kea" | "to" => &["other"],
        // East Slavic + Polish + Baltic/Czech/Slovak: one, few, many, other.
        "ru" | "uk" | "be" | "pl" | "lt" | "cs" | "sk" => &["one", "few", "many", "other"],
        // South Slavic (Serbo-Croatian family): one, few, other.
        "sr" | "hr" | "bs" | "sh" => &["one", "few", "other"],
        "sl" => &["one", "two", "few", "other"],
        "ro" | "mo" => &["one", "few", "other"],
        "ar" | "ars" => &["zero", "one", "two", "few", "many", "other"],
        "he" | "iw" => &["one", "two", "other"],
        "cy" => &["zero", "one", "two", "few", "many", "other"],
        "ga" => &["one", "two", "few", "many", "other"],
        "mt" => &["one", "two", "few", "many", "other"],
        "lv" | "prg" => &["zero", "one", "other"],
        "gd" => &["one", "two", "few", "other"],
        "br" => &["one", "two", "few", "many", "other"],
        "gv" => &["one", "two", "few", "many", "other"],
        "kw" => &["zero", "one", "two", "few", "many", "other"],
        "dsb" | "hsb" => &["one", "two", "few", "other"],
        "iu" | "naq" | "se" | "sma" | "smi" | "smj" | "smn" | "sms" => &["one", "two", "other"],
        // Everything else (Germanic, Romance, Greek, Turkic, Indic, Finnic, ...).
        _ => &["one", "other"],
    }
}

/// Required categories not present in `present` (exact `=N` cases are ignored).
pub fn missing<S: AsRef<str>>(locale: &str, present: &[S]) -> Vec<&'static str> {
    // A plural that only has `other` (plus optional exact `=N` cases) is a deliberate
    // opt-out: ICU allows it and authors use it just for `#` formatting.
    let categories: Vec<&str> = present
        .iter()
        .map(AsRef::as_ref)
        .filter(|c| !c.starts_with('='))
        .collect();
    if categories == ["other"] {
        return vec![];
    }
    required(locale)
        .iter()
        .copied()
        .filter(|req| !present.iter().any(|p| p.as_ref() == *req))
        .collect()
}

/// `{arg, plural, one {..} other {..}}` arguments and their case keys (top level and nested).
pub fn icu_cases(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{'
            && let Some((arg, kind, body_start, close)) = icu_arg(text, i)
        {
            if kind == "plural" {
                out.push((arg, case_keys(&text[body_start..close])));
            }
            // Recurse into the argument body for nested plurals.
            out.extend(icu_cases(&text[body_start..close]));
            i = close + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// Parse `{name, kind, rest}` at `start`; returns (name, kind, index where `rest` begins, index of the closing brace).
fn icu_arg(text: &str, start: usize) -> Option<(String, String, usize, usize)> {
    let b = text.as_bytes();
    let mut i = start + 1;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    let ns = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    if i == ns {
        return None;
    }
    let name = text[ns..i].to_string();
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if b.get(i) != Some(&b',') {
        return None;
    }
    i += 1;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    let ks = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    let kind = text[ks..i].to_string();
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if b.get(i) == Some(&b',') {
        i += 1;
    }
    let body_start = i;
    let mut depth = 1;
    let mut j = start + 1;
    while j < b.len() {
        match b[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((name, kind, body_start, j));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// Case keys of an ICU plural body: `=0 {..} one {..} other {..}` → `["=0","one","other"]`.
fn case_keys(body: &str) -> Vec<String> {
    let b = body.as_bytes();
    let mut i = 0;
    let mut keys = Vec::new();
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let ks = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'{' {
            i += 1;
        }
        if i == ks {
            break;
        }
        keys.push(body[ks..i].to_string());
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) != Some(&b'{') {
            break;
        }
        let mut depth = 0;
        while i < b.len() {
            match b[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    keys
}

pub const I18NEXT_SUFFIXES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// `item_few` → `("item", "few")` when the suffix is an i18next plural category.
pub fn i18next_split(key: &str) -> Option<(&str, &str)> {
    let (base, suffix) = key.rsplit_once('_')?;
    (!base.is_empty() && I18NEXT_SUFFIXES.contains(&suffix)).then_some((base, suffix))
}

/// Group i18next plural keys: `item_one`, `item_other` → `item: [one, other]`.
pub fn i18next_groups(keys: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for k in keys {
        if let Some((base, suffix)) = i18next_split(k) {
            groups
                .entry(base.to_string())
                .or_default()
                .push(suffix.to_string());
        }
    }
    groups
}

/// The groups in a *source* file that are real plurals: `_other` plus at least one
/// more category. `step_one` without `step_other` is an ordinary key, and a lone
/// `items_other` is i18next's way of opting out of plural forms (used for every count).
pub fn i18next_plural_groups(keys: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut groups = i18next_groups(keys);
    groups.retain(|_, cats| cats.len() > 1 && cats.iter().any(|c| c == "other"));
    groups
}

/// All plural category sets in a String Catalog: `(key, locale, categories)` for
/// both `variations.plural` and `substitutions.*.variations.plural`.
pub fn xcstrings_plurals(
    doc: &crate::formats::xcstrings::Document,
) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    let Some(strings) = doc.root.get("strings").and_then(|v| v.as_object()) else {
        return out;
    };
    for (key, entry) in strings {
        let Some(locs) = entry.get("localizations").and_then(|v| v.as_object()) else {
            continue;
        };
        for (locale, l) in locs {
            if let Some(p) = l.pointer("/variations/plural").and_then(|v| v.as_object()) {
                out.push((key.clone(), locale.clone(), p.keys().cloned().collect()));
            }
            if let Some(subs) = l.get("substitutions").and_then(|v| v.as_object()) {
                for (name, sub) in subs {
                    if let Some(p) = sub
                        .pointer("/variations/plural")
                        .and_then(|v| v.as_object())
                    {
                        out.push((
                            format!("{key}#{name}"),
                            locale.clone(),
                            p.keys().cloned().collect(),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// `(name, quantities)` for every `<plurals>` in an Android resources file.
pub fn android_plurals(doc: &crate::formats::android::Document) -> Vec<(String, Vec<String>)> {
    doc.entries
        .iter()
        .filter(|e| e.kind == crate::formats::android::Kind::Plurals)
        .map(|e| {
            (
                e.name.clone(),
                e.values.iter().filter_map(|v| v.quantity.clone()).collect(),
            )
        })
        .collect()
}

/// In the `zero`, `one` and `two` forms the count is a known number, so "Ein Mitglied"
/// for `%d member` is a translation choice, not a bug: unless the language's `one` also
/// covers 21, 31, 101 (East Slavic, Serbo-Croatian, Baltic), where the number must show.
pub fn count_optional(locale: &str, category: &str) -> bool {
    if !matches!(category, "zero" | "one" | "two") {
        return false;
    }
    let lang = locale
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    !matches!(
        lang.as_str(),
        "ru" | "uk" | "be" | "hr" | "sr" | "bs" | "sh" | "lt" | "lv" | "prg"
    )
}

/// The text without its numeric printf placeholders (`%d`, `%1$lld`, `%u`…), so two
/// plural forms can be compared on everything but the count.
pub fn strip_count(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if b.get(i + 1) == Some(&b'%') {
                out.push_str("%%");
                i += 2;
                continue;
            }
            // `%arg`: the substituted count in a String Catalog substitution form.
            if b[i..].starts_with(b"%arg") {
                i += 4;
                continue;
            }
            // %[N$][flags][width][.prec][length]conv
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b'$' {
                j += 1;
            } else {
                j = i + 1;
            }
            while j < b.len() && matches!(b[j], b'-' | b'+' | b' ' | b'#' | b'0' | b'\'') {
                j += 1;
            }
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            while j < b.len() && matches!(b[j], b'l' | b'h' | b'z' | b'q' | b'j' | b't' | b'L') {
                j += 1;
            }
            if j < b.len() && matches!(b[j], b'd' | b'i' | b'u' | b'x' | b'X' | b'o') {
                i = j + 1;
                continue;
            }
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Argument position of `var` in a `.stringsdict` format key: `%2$#@seconds@` says 2,
/// otherwise the variable's order among the `%#@…@` references.
pub fn stringsdict_position(format: &str, var: &str) -> usize {
    let needle = format!("#@{var}@");
    let Some(at) = format.find(&needle) else {
        return 1;
    };
    // Explicit `%N$` right before `#@`.
    let before = &format[..at];
    if let Some(p) = before.rfind('%') {
        let mid = &before[p + 1..];
        if let Some(n) = mid.strip_suffix('$').and_then(|d| d.parse::<usize>().ok()) {
            return n;
        }
    }
    before.matches("#@").count() + 1
}

/// Renumber unnumbered printf specs in a `.stringsdict` form to the variable's own
/// argument: inside a variable, `%d` means that variable's value.
pub fn renumber(form: &str, pos: usize) -> String {
    let b = form.as_bytes();
    let mut out = String::with_capacity(form.len() + 4);
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if b.get(i + 1) == Some(&b'%') {
                out.push_str("%%");
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let numbered = j > i + 1 && b.get(j) == Some(&b'$');
            out.push('%');
            if !numbered && j < b.len() && b[j] != b'#' {
                out.push_str(&format!("{pos}$"));
            }
            i += 1;
            continue;
        }
        let c = form[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Compare two plural forms' placeholders, knowing which category they are. In `zero`,
/// `one`, `two`: an exact-count language may leave the number out or put it in
/// (`count_optional`); a language whose `one` also covers 21, 31 must keep the number, so
/// only an added count is fine there (English "one member" → Russian "%d участник").
pub fn compare_forms(
    locale: &str,
    category: &str,
    source: &str,
    translation: &str,
) -> Option<crate::check::placeholders::Mismatch> {
    use crate::check::placeholders::compare;
    if count_optional(locale, category) {
        return compare(&strip_count(source), &strip_count(translation));
    }
    if matches!(category, "zero" | "one" | "two") && strip_count(source) == source {
        // The source form has no count; the translation adding one is right.
        return compare(source, &strip_count(translation));
    }
    compare(source, translation)
}
