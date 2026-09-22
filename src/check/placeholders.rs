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
    /// The token as written (`{{ name }}`, spaces included); for an ICU plural the whole
    /// argument. Always a substring of the text it was extracted from.
    pub raw: String,
    /// The name a translator can get wrong (`models` in `{{ models }}`, `%(models)s`,
    /// `$models`), when the token has one. Positional `{0}` and `%1$@` have none.
    pub name: Option<String>,
}

impl Placeholder {
    fn simple(canonical: impl Into<String>, raw: &str) -> Placeholder {
        Placeholder {
            canonical: canonical.into(),
            raw: raw.to_string(),
            name: None,
        }
    }

    /// The token with its name blanked: `{{}}`, `{}`, `{}/plural`, `%()s`, `$`. Two tokens
    /// of one shape are the same placeholder under different names.
    fn shape(&self) -> Option<String> {
        let name = self.name.as_deref()?;
        Some(self.canonical.replacen(name, "", 1))
    }

    /// `raw` with the name replaced, keeping spacing and any ICU body.
    fn renamed(&self, name: &str) -> String {
        match &self.name {
            Some(n) => self.raw.replacen(n.as_str(), name, 1),
            None => self.raw.clone(),
        }
    }
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
                // `%arg` is Xcode's token for the value a substitution stands for.
                if text[i..].starts_with("%arg") {
                    out.push(Placeholder::simple("%arg", "%arg"));
                    i += 4;
                    continue;
                }
                if let Some((spec, len)) = printf_spec(&text[i..], &mut next_arg) {
                    if let Some((canonical, name)) = spec {
                        out.push(Placeholder {
                            canonical,
                            raw: text[i..i + len].to_string(),
                            name,
                        });
                    }
                    i += len;
                    continue;
                }
                i += 1;
            }
            b'{' => {
                if let Some(br) = brace_token(&text[i..]) {
                    let raw = &text[i..i + br.len];
                    out.extend(br.tokens);
                    if br.malformed {
                        out.push(Placeholder::simple("<malformed ICU>", raw));
                    }
                    i += br.len;
                    continue;
                }
                i += 1;
            }
            b'$' => {
                if let Some((tok, len)) = dollar_token(&text[i..]) {
                    let name = (!tok.starts_with("$t(")).then(|| tok[1..].to_string());
                    out.push(Placeholder {
                        raw: tok.clone(),
                        canonical: tok,
                        name,
                    });
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
/// Returns `(Some((canonical, name)), len)`, or `(None, 2)` for the literal `%%`.
type Spec = (String, Option<String>);
fn printf_spec(s: &str, next_arg: &mut usize) -> Option<(Option<Spec>, usize)> {
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
        return Some((Some(("%#@…@".to_string(), None)), end + 1));
    }
    // Python named argument: `%(name)s`, `%(count)d`. Named arguments do not take a
    // position, and may be repeated or reordered freely.
    let mut named: Option<String> = None;
    if b.get(i) == Some(&b'(') {
        let end = s[i..].find(')')? + i;
        let name = &s[i + 1..end];
        if name.is_empty() || name.contains(|c: char| c.is_whitespace() || c == '%') {
            return None;
        }
        named = Some(name.to_string());
        i = end + 1;
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
    let n = match (argnum, &named) {
        (_, Some(_)) => 0,
        (Some(n), None) => n,
        (None, None) => {
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
    match named {
        Some(name) => Some((Some((format!("%({name}){family}"), Some(name))), i)),
        None => Some((Some((format!("%{n}${family}"), None)), i)),
    }
}

/// Result of parsing one brace argument.
struct Brace {
    /// `{name}` or `{{name}}`; plural/select args get a `{name}` token plus a
    /// `{name}/plural` marker so a plural turned into a plain arg is detected.
    tokens: Vec<Placeholder>,
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
    // Any script: a translated name (`{{ modèles }}`, `{{ модели }}`) is still a token,
    // and reported as one rather than as prose.
    for c in s[i..].chars() {
        if c.is_alphanumeric() || matches!(c, '_' | '.' | '-') {
            i += c.len_utf8();
        } else {
            break;
        }
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
    let raw = &s[..close];
    // A name a translator could have translated: a word, not `{0}`, and the token is
    // just that name (or an ICU argument), not `{see below}` prose.
    let named =
        (rest.is_empty() || rest.starts_with(',')) && !name.bytes().all(|c| c.is_ascii_digit());
    let token = |canonical: String| Placeholder {
        canonical,
        raw: raw.to_string(),
        name: named.then(|| name.clone()),
    };
    let mut tokens = vec![token(if double {
        format!("{{{{{name}}}}}")
    } else {
        format!("{{{name}}}")
    })];
    let mut malformed = false;
    if let Some(after_comma) = rest.strip_prefix(',') {
        let after_comma = after_comma.trim_start();
        let kind_end = after_comma
            .find(|c: char| c == ',' || c.is_whitespace())
            .unwrap_or(after_comma.len());
        let kind = &after_comma[..kind_end];
        if matches!(kind, "plural" | "select" | "selectordinal") {
            tokens.push(token(format!("{{{name}}}/{kind}")));
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
fn parse_icu_cases(cases: &str) -> (Vec<Placeholder>, bool, bool) {
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
        tokens.extend(extract(&cases[body_start..j]));
        i = j + 1;
    }
    (tokens, ok, has_other)
}

/// `$t(key)` (i18next nesting) or `$name` (Dart-style): but not `$5` or a bare `$`.
fn dollar_token(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if s.starts_with("$t(") {
        let end = s.find(')')?;
        return Some((s[..=end].to_string(), end + 1));
    }
    let mut i = 1;
    let first = s[1..].chars().next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    for c in s[1..].chars() {
        if c.is_alphanumeric() || c == '_' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    let _ = b;
    Some((s[..i].to_string(), i))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    /// Placeholders whose name the translator translated: (source token, translation
    /// token), same shape, different name. `{{ models }}` → `{{ modelli }}` is the most
    /// common placeholder bug there is, and the one bug with a mechanical repair.
    pub renamed: Vec<(Placeholder, Placeholder)>,
}

impl Mismatch {
    /// `translation` with every translated placeholder name put back to the source's.
    /// Only when that is the whole problem: a rename next to a dropped placeholder is left
    /// to a person or the model. The caller re-checks the result.
    pub fn fix(&self, translation: &str) -> Option<String> {
        if self.renamed.is_empty() || !self.missing.is_empty() || !self.extra.is_empty() {
            return None;
        }
        let mut out = translation.to_string();
        for (from, to) in &self.renamed {
            let name = from.name.as_deref()?;
            out = out.replace(&to.raw, &to.renamed(name));
        }
        (out != translation).then_some(out)
    }
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if !self.renamed.is_empty() {
            let mut pairs: Vec<String> = self
                .renamed
                .iter()
                .map(|(a, b)| format!("{} → {}", a.canonical, b.canonical))
                .collect();
            pairs.dedup();
            parts.push(format!(
                "placeholder name{} translated: {} (names must stay as in the source)",
                if pairs.len() == 1 { "" } else { "s" },
                pairs.join(", ")
            ));
        }
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
            if (numbered && p.canonical.starts_with('%')) || p.canonical.starts_with("%(") {
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
        return None;
    }
    let renamed = renamed_pairs(source, translation, &mut missing, &mut extra);
    Some(Mismatch {
        missing,
        extra,
        renamed,
    })
}

/// Pair a missing named token with an unexpected one of the same shape: the same
/// placeholder under a translated name. Pairs go by order of appearance, and only when a
/// shape has as many missing as unexpected; anything else stays missing/unexpected.
fn renamed_pairs(
    source: &str,
    translation: &str,
    missing: &mut Vec<String>,
    extra: &mut Vec<String>,
) -> Vec<(Placeholder, Placeholder)> {
    // The tokens behind the canonical names, in text order, one per reported occurrence.
    let occurrences = |text: &str, names: &[String]| -> Vec<Placeholder> {
        let mut left = names.to_vec();
        let mut out = Vec::new();
        for p in extract(text) {
            if let Some(pos) = left.iter().position(|n| *n == p.canonical) {
                left.remove(pos);
                if p.name.is_some() {
                    out.push(p);
                }
            }
        }
        out
    };
    let from = occurrences(source, missing);
    let to = occurrences(translation, extra);
    let mut pairs = Vec::new();
    let mut shapes: Vec<String> = from.iter().filter_map(Placeholder::shape).collect();
    shapes.dedup();
    for shape in shapes {
        let a: Vec<&Placeholder> = from
            .iter()
            .filter(|p| p.shape().as_ref() == Some(&shape))
            .collect();
        let b: Vec<&Placeholder> = to
            .iter()
            .filter(|p| p.shape().as_ref() == Some(&shape))
            .collect();
        if a.is_empty() || a.len() != b.len() {
            continue;
        }
        for (x, y) in a.into_iter().zip(b) {
            pairs.push((x.clone(), y.clone()));
        }
    }
    for (x, y) in &pairs {
        if let Some(i) = missing.iter().position(|m| *m == x.canonical) {
            missing.remove(i);
        }
        if let Some(i) = extra.iter().position(|e| *e == y.canonical) {
            extra.remove(i);
        }
    }
    pairs
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
