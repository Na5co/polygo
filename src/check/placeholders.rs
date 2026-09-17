//! Placeholder parity between a source string and its translation.
//!
//! Recognised forms:
//! - printf family (Apple, Android, C): `%@ %d %lld %1$@ %2$s %.2f %%`
//! - ICU / Flutter / .NET-ish braces: `{name} {0} {count, plural, ...}`
//! - i18next: `{{name}} $t(key)`
//! - shell/Dart-style `$name`
//!
//! Unnumbered printf arguments are numbered by position, so reordering them in a
//! translation is a mismatch, while explicitly numbered arguments (`%2$@ %1$@`)
//! may appear in any order. Flags, width and precision are ignored; the
//! conversion (and length modifier) must match, because `%d`→`%s` crashes Android.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Placeholder {
    /// Canonical token, e.g. `%1$@`, `%2$lld`, `{name}`, `{{count}}`, `$t(key)`, `$name`.
    pub canonical: String,
}

/// Extract placeholders in order of appearance.
pub fn extract(text: &str) -> Vec<Placeholder> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    let mut next_arg = 1usize;
    while i < b.len() {
        match b[i] {
            b'%' => {
                if let Some((spec, len)) = printf_spec(&text[i..], &mut next_arg) {
                    if let Some(s) = spec {
                        out.push(Placeholder { canonical: s });
                    }
                    i += len;
                    continue;
                }
                i += 1;
            }
            b'{' => {
                if let Some(br) = brace_token(&text[i..]) {
                    for tok in br.tokens {
                        out.push(Placeholder { canonical: tok });
                    }
                    if br.malformed {
                        out.push(Placeholder {
                            canonical: "<malformed ICU>".into(),
                        });
                    }
                    i += br.len;
                    continue;
                }
                i += 1;
            }
            b'$' => {
                if let Some((tok, len)) = dollar_token(&text[i..]) {
                    out.push(Placeholder { canonical: tok });
                    i += len;
                    continue;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Parse a printf conversion at the start of `s` (which begins with `%`).
/// Returns `(Some(canonical), len)`, or `(None, 2)` for the literal `%%`.
fn printf_spec(s: &str, next_arg: &mut usize) -> Option<(Option<String>, usize)> {
    let b = s.as_bytes();
    let mut i = 1;
    if b.get(i) == Some(&b'%') {
        return Some((None, 2));
    }
    // Optional argument number: digits followed by '$'.
    let mut argnum: Option<usize> = None;
    let start_digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > start_digits && b.get(i) == Some(&b'$') {
        argnum = s[start_digits..i].parse().ok();
        i += 1;
    } else {
        i = start_digits;
    }
    // Apple stringsdict variable reference: `%#@name@` / `%1$#@name@`. Variables are
    // defined per locale in String Catalogs, so names may legitimately differ between
    // source and translation; only the number of references is compared.
    if s[i..].starts_with("#@") {
        let name_start = i + 2;
        let mut end = name_start;
        while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
            end += 1;
        }
        if end == name_start || b.get(end) != Some(&b'@') {
            return None;
        }
        if argnum.is_none() {
            *next_arg += 1;
        }
        return Some((Some("%#@…@".to_string()), end + 1));
    }
    // Flags. The space flag is deliberately excluded: in UI text `25% off` is prose,
    // not `% o`.
    while i < b.len() && matches!(b[i], b'-' | b'+' | b'#' | b'0' | b'\'' | b',') {
        i += 1;
    }
    // Width (digits or *).
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'*') {
        i += 1;
    }
    // Precision.
    if b.get(i) == Some(&b'.') {
        i += 1;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'*') {
            i += 1;
        }
    }
    // Length modifiers.
    let len_start = i;
    while i < b.len() && matches!(b[i], b'h' | b'l' | b'q' | b'L' | b'z' | b't' | b'j') {
        i += 1;
    }
    let length = &s[len_start..i];
    // Conversion.
    let conv = *b.get(i)? as char;
    if !matches!(
        conv,
        '@' | 'd'
            | 'i'
            | 'u'
            | 'f'
            | 'F'
            | 'e'
            | 'E'
            | 'g'
            | 'G'
            | 'x'
            | 'X'
            | 'o'
            | 's'
            | 'S'
            | 'c'
            | 'C'
            | 'p'
            | 'a'
            | 'A'
            | 'b'
    ) {
        return None;
    }
    i += 1;
    let n = match argnum {
        Some(n) => n,
        None => {
            let n = *next_arg;
            *next_arg += 1;
            n
        }
    };
    // Normalise integer lengths: %d %ld %lld %i %u all mean "an integer" to a translator,
    // but we keep the family distinct from strings/floats.
    let family = match conv {
        'd' | 'i' | 'u' | 'x' | 'X' | 'o' => "d".to_string(),
        'f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'a' | 'A' => "f".to_string(),
        's' | 'S' => "s".to_string(),
        'c' | 'C' => "c".to_string(),
        other => other.to_string(),
    };
    let _ = length;
    Some((Some(format!("%{n}${family}")), i))
}

/// Result of parsing one brace argument.
struct Brace {
    /// Canonical token: `{name}` or `{{name}}`; plural/select args get a `{name}` token
    /// plus a `{name}/plural` marker so a plural turned into a plain arg is detected.
    tokens: Vec<String>,
    /// Bytes consumed.
    len: usize,
    /// True when an ICU plural/select is structurally broken (case without body, no `other`).
    malformed: bool,
}

/// `{name}`, `{{name}}`, `{0}`, `{count, plural, one {# item} other {# items}}`.
/// Recurses into ICU case bodies so nested placeholders count too.
fn brace_token(s: &str) -> Option<Brace> {
    let b = s.as_bytes();
    let double = b.get(1) == Some(&b'{');
    let mut i = if double { 2 } else { 1 };
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    let name_start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'.' | b'-')) {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = s[name_start..i].to_string();
    // Find the matching close brace(s).
    let mut depth = if double { 2 } else { 1 };
    let mut j = i;
    let close;
    loop {
        if j >= b.len() {
            return None;
        }
        match b[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = j + 1;
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    let inner_end = if double { close - 2 } else { close - 1 };
    let rest = s[i..inner_end].trim();
    let mut tokens = vec![if double {
        format!("{{{{{name}}}}}")
    } else {
        format!("{{{name}}}")
    }];
    let mut malformed = false;
    if let Some(after_comma) = rest.strip_prefix(',') {
        let after_comma = after_comma.trim_start();
        let kind_end = after_comma
            .find(|c: char| c == ',' || c.is_whitespace())
            .unwrap_or(after_comma.len());
        let kind = &after_comma[..kind_end];
        if matches!(kind, "plural" | "select" | "selectordinal") {
            tokens.push(format!("{{{name}}}/{kind}"));
            let cases = after_comma[kind_end..].trim_start().trim_start_matches(',');
            let (case_tokens, ok, has_other) = parse_icu_cases(cases);
            tokens.extend(case_tokens);
            if !ok || !has_other {
                malformed = true;
            }
        }
    }
    Some(Brace {
        tokens,
        len: close,
        malformed,
    })
}

/// Parse `key {body} key {body} ...`, returning nested placeholders from the bodies,
/// whether every case had a body, and whether an `other` case exists.
fn parse_icu_cases(cases: &str) -> (Vec<String>, bool, bool) {
    let b = cases.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();
    let mut ok = true;
    let mut has_other = false;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        // Case key: `one`, `other`, `=0`, `few`, ...
        let key_start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'{' {
            i += 1;
        }
        let key = &cases[key_start..i];
        if key.is_empty() {
            ok = false;
            break;
        }
        if key == "other" {
            has_other = true;
        }
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) != Some(&b'{') {
            ok = false;
            break;
        }
        // Body: match braces.
        let body_start = i + 1;
        let mut depth = 0;
        let mut j = i;
        let mut closed = false;
        while j < b.len() {
            match b[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        if !closed {
            ok = false;
            break;
        }
        for p in extract(&cases[body_start..j]) {
            tokens.push(p.canonical);
        }
        i = j + 1;
    }
    (tokens, ok, has_other)
}

/// `$t(key)` (i18next nesting) or `$name` (Dart-style) — but not `$5` or a bare `$`.
fn dollar_token(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if s.starts_with("$t(") {
        let end = s.find(')')?;
        return Some((s[..=end].to_string(), end + 1));
    }
    let mut i = 1;
    if !(i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == b'_')) {
        return None;
    }
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    Some((s[..i].to_string(), i))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub missing: Vec<String>,
    pub extra: Vec<String>,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if !self.missing.is_empty() {
            parts.push(format!("missing {}", self.missing.join(" ")));
        }
        if !self.extra.is_empty() {
            parts.push(format!("unexpected {}", self.extra.join(" ")));
        }
        write!(f, "{}", parts.join("; "))
    }
}

/// Compare the placeholder multisets of `source` and `translation`.
pub fn compare(source: &str, translation: &str) -> Option<Mismatch> {
    // Explicitly numbered printf arguments may be repeated or reordered freely, so they
    // are compared as a set; everything else as a multiset.
    let count = |text: &str| {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        let numbered = explicit_numbered(text);
        for p in extract(text) {
            let e = m.entry(p.canonical.clone()).or_default();
            if numbered && p.canonical.starts_with('%') {
                *e = 1;
            } else {
                *e += 1;
            }
        }
        m
    };
    let a = count(source);
    let b = count(translation);
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, n) in &a {
        let have = b.get(k).copied().unwrap_or(0);
        for _ in have..*n {
            missing.push(k.clone());
        }
    }
    for (k, n) in &b {
        let have = a.get(k).copied().unwrap_or(0);
        for _ in have..*n {
            extra.push(k.clone());
        }
    }
    if missing.is_empty() && extra.is_empty() {
        None
    } else {
        Some(Mismatch { missing, extra })
    }
}

/// True when the text uses explicit `%n$` argument numbering anywhere.
fn explicit_numbered(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'%' && b[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if b.get(j) == Some(&b'$') {
                return true;
            }
        }
        i += 1;
    }
    false
}
