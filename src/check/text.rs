//! Plain-text sanity checks: empty, identical to source, runaway length.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

/// Absolute slack added to the ratio bound so short strings ("OK" → "Akkoord") pass.
const SLACK: usize = 8;

pub fn check_text(
    source: &str,
    translation: &str,
    source_locale: &str,
    target_locale: &str,
    ratio: f64,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let src = source.trim();
    let tr = translation.trim();
    if tr.is_empty() {
        if !src.is_empty() {
            out.push(Finding {
                code: "empty",
                severity: Severity::Error,
                message: "translation is empty".into(),
            });
        }
        return out;
    }
    let same_language = lang(source_locale) == lang(target_locale);
    let prose = strip_placeholders(src);
    let has_letters = prose.chars().any(|c| c.is_alphabetic());
    let is_markup_only = !prose.chars().any(|c| c.is_alphanumeric());
    // Acronyms and very short tokens ("OK", "URL", "ID") are the same everywhere.
    let letters: Vec<char> = prose.chars().filter(|c| c.is_alphabetic()).collect();
    let acronym_like = letters.len() <= 3 || letters.iter().all(|c| c.is_uppercase());
    if !same_language && has_letters && !is_markup_only && !acronym_like && src == tr {
        out.push(Finding {
            code: "identical",
            severity: Severity::Warning,
            message: "identical to the source text".into(),
        });
    }
    if let Some(words) = untranslated_fragment(src, tr, &[]) {
        out.push(Finding {
            code: "fragment",
            severity: Severity::Warning,
            message: format!(
                "left in the source language: {}",
                words
                    .iter()
                    .map(|w| format!("`{w}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    if let Some(m) = markup_mismatch(src, tr) {
        out.push(Finding {
            code: "markup",
            severity: Severity::Error,
            message: m,
        });
    }
    if let Some(m) = edge_whitespace_mismatch(source, translation) {
        out.push(Finding {
            code: "whitespace",
            severity: Severity::Warning,
            message: m,
        });
    }
    if let Some(m) = punctuation_dropped(src, tr) {
        out.push(Finding {
            code: "punctuation",
            severity: Severity::Warning,
            message: m,
        });
    }
    let s = src.chars().count() as f64;
    let t = tr.chars().count() as f64;
    let slack = SLACK as f64;
    if t > s * ratio + slack {
        out.push(Finding {
            code: "length",
            severity: Severity::Warning,
            message: format!("{t:.0} chars vs {s:.0} in the source (limit {ratio}× + {SLACK})"),
        });
    } else if s > t * ratio + slack {
        out.push(Finding {
            code: "length",
            severity: Severity::Warning,
            message: format!("only {t:.0} chars vs {s:.0} in the source"),
        });
    }
    out
}

fn lang(locale: &str) -> String {
    locale
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Remove placeholder-looking runs (`%…d`, `{…}`, `$t(…)`, `$name`) and tags so "has letters"
/// reflects human prose, not format specifiers.
fn strip_placeholders(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if b.get(i + 1) == Some(&b'#') && b.get(i + 2) == Some(&b'@') => {
                // stringsdict variable `%#@name@`.
                let j = text[i + 3..]
                    .find('@')
                    .map(|k| i + 3 + k + 1)
                    .unwrap_or(b.len());
                i = j;
            }
            b'%' => {
                let mut j = i + 1;
                while j < b.len() && !(b[j].is_ascii_alphabetic() || b[j] == b'@' || b[j] == b'%') {
                    j += 1;
                }
                // Length modifiers (`%lld`, `%zu`, `%hhd`) precede the conversion char.
                while j < b.len() && matches!(b[j], b'h' | b'l' | b'q' | b'L' | b'z' | b'j' | b't')
                {
                    j += 1;
                }
                i = (j + 1).min(b.len());
            }
            b'{' => {
                let mut depth = 0;
                let mut j = i;
                while j < b.len() {
                    match b[j] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                i = (j + 1).min(b.len());
            }
            b'<' => {
                // Inline markup / HTML tags are not prose either.
                let j = text[i..].find('>').map(|k| i + k + 1).unwrap_or(i + 1);
                i = j;
            }
            b'$' => {
                let mut j = i + 1;
                if b.get(j) == Some(&b't') && b.get(j + 1) == Some(&b'(') {
                    while j < b.len() && b[j] != b')' {
                        j += 1;
                    }
                    j += 1;
                } else {
                    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                        j += 1;
                    }
                }
                i = j.min(b.len());
            }
            _ if text[i..].starts_with("www.") || starts_url_scheme(&text[i..]) => {
                // URLs are not prose.
                let j = text[i..]
                    .find(|c: char| c.is_whitespace() || c == ')' || c == '"' || c == '\'')
                    .map(|k| i + k)
                    .unwrap_or(b.len());
                i = j;
            }
            _ => {
                let c = text[i..].chars().next().unwrap();
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}

/// Latin lowercase words copied from the source into a translation whose script is
/// not Latin (`ようこそ back!`, `Удалить photos`). Only fires when most letters of the
/// translation are non-Latin, so Latin-script targets are never judged. Words that are
/// short (≤ 2), mixed-case (`iOS`), contain digits, appear in `allowed`, or are common
/// loanwords are ignored.
pub fn untranslated_fragment(
    source: &str,
    translation: &str,
    allowed: &[String],
) -> Option<Vec<String>> {
    let stripped = strip_placeholders(translation);
    let letters: Vec<char> = stripped.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return None;
    }
    let latin = letters.iter().filter(|c| is_latin(**c)).count();
    if latin * 2 > letters.len() {
        return None;
    }
    let prose = strip_placeholders(source).to_lowercase();
    let source_words: std::collections::BTreeSet<&str> =
        prose.split(|c: char| !c.is_alphanumeric()).collect();
    let allowed_lower: Vec<String> = allowed.iter().map(|a| a.to_lowercase()).collect();
    let mut hits = Vec::new();
    // Whitespace-delimited tokens first: `form-name`, `@slack`, `a/b`, `&mdash;` and
    // `user@x.com` are code, handles, paths and entities, not words to translate.
    let tokens = stripped.split_whitespace().filter(|t| {
        let core = t.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ','
                    | '!'
                    | '?'
                    | ':'
                    | ';'
                    | ')'
                    | '('
                    | '"'
                    | '\u{201c}'
                    | '\u{201d}'
                    | '\u{00ab}'
                    | '\u{00bb}'
            )
        });
        !core.is_empty()
            && core
                .chars()
                .all(|c| c.is_alphanumeric() || c == '\'' || c == '\u{2019}')
    });
    for word in tokens.flat_map(|t| t.split(|c: char| !c.is_alphanumeric() && c != '\'')) {
        let w = word.trim_matches('\'');
        if w.chars().count() <= 2
            || !w.chars().all(|c| is_latin(c) && c.is_lowercase())
            || LOANWORDS.contains(&w)
            || !source_words.contains(w)
            || allowed_lower
                .iter()
                .any(|a| a.split_whitespace().any(|part| part == w))
        {
            continue;
        }
        if !hits.iter().any(|h: &String| h == w) {
            hits.push(w.to_string());
        }
    }
    (!hits.is_empty()).then_some(hits)
}

/// `scheme://…` at the start of `s` (http, duck, coccoc, …).
fn starts_url_scheme(s: &str) -> bool {
    let n = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'-' || *b == b'.')
        .count();
    n > 0 && s[n..].starts_with("://")
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '\u{00C0}'..='\u{024F}')
}

/// Lowercase English words routinely kept as-is in non-Latin-script UIs.
const LOANWORDS: &[&str] = &[
    "email",
    "mail",
    "wifi",
    "http",
    "https",
    "www",
    "com",
    "app",
    "web",
    "url",
    "pdf",
    "png",
    "jpg",
    "gif",
    "svg",
    "css",
    "html",
    "json",
    "xml",
    "api",
    "sdk",
    "ios",
    "android",
    "macos",
    "windows",
    "linux",
    "beta",
    "pro",
    "plus",
    "lite",
    "max",
    "mini",
    "ok",
    "px",
    "sms",
    "gps",
    "nfc",
    "usb",
    "hdmi",
    "bluetooth",
    "vpn",
    "dns",
    "ssl",
    "tls",
    "ssh",
    "ftp",
    "cpu",
    "gpu",
    "ram",
    "ssd",
    "hdd",
    "qr",
    "cookie",
    "cookies",
    "push",
    "emoji",
    "wiki",
    "blog",
    "chat",
    "bot",
    "online",
    "offline",
    "premium",
    "csv",
    "zip",
    "dark",
    "light",
];

// ---- markup ---------------------------------------------------------------------------------

/// Tags in order: `b`, `/b`, `br` (self-closing collapses to its name). Attributes are
/// ignored: `<a href="…">` and `<a href="…" target="_blank">` are the same tag. Only
/// `<name …>` with a letter first counts, so `a < b` is not a tag.
fn tags(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let (closing, body) = match rest.strip_prefix('/') {
            Some(b) => (true, b),
            None => (false, rest),
        };
        let name: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-' || *c == '_')
            .collect();
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let Some(end) = body.find('>') else { break };
        if body[..end].contains('<') {
            continue;
        }
        out.push(if closing {
            format!("/{}", name.to_ascii_lowercase())
        } else {
            name.to_ascii_lowercase()
        });
        rest = &body[end + 1..];
    }
    out
}

/// Every tag of the source must appear in the translation the same number of times.
/// A dropped `</b>` or `<a href>` breaks rendering in every UI framework.
pub fn markup_mismatch(source: &str, translation: &str) -> Option<String> {
    let (s, t) = (tags(source), tags(translation));
    if s.is_empty() && t.is_empty() {
        return None;
    }
    fn count(v: &[String]) -> std::collections::BTreeMap<&str, i32> {
        let mut m = std::collections::BTreeMap::new();
        for x in v {
            *m.entry(x.as_str()).or_default() += 1;
        }
        m
    }
    let (sc, tc) = (count(&s), count(&t));
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, n) in &sc {
        let d = n - tc.get(k).copied().unwrap_or(0);
        if d > 0 {
            missing.push(format!("<{k}>"));
        }
    }
    for (k, n) in &tc {
        let d = n - sc.get(k).copied().unwrap_or(0);
        if d > 0 {
            extra.push(format!("<{k}>"));
        }
    }
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("missing {}", missing.join(", ")));
    }
    if !extra.is_empty() {
        parts.push(format!("unexpected {}", extra.join(", ")));
    }
    Some(parts.join("; "))
}

// ---- edges ----------------------------------------------------------------------------------

fn edge(text: &str) -> (String, String) {
    let lead: String = text.chars().take_while(|c| c.is_whitespace()).collect();
    let trail: String = text
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (lead, trail)
}

fn show_ws(s: &str) -> String {
    s.replace(' ', "␠")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// Leading/trailing whitespace that the source has and the translation lacks, or the
/// other way round: strings glued together in the UI lose their space, or a trailing
/// newline goes missing.
pub fn edge_whitespace_mismatch(source: &str, translation: &str) -> Option<String> {
    if source.trim().is_empty() || translation.trim().is_empty() {
        return None;
    }
    let (sl, st) = edge(source);
    let (tl, tt) = edge(translation);
    let mut parts = Vec::new();
    if sl != tl {
        parts.push(format!(
            "leading \"{}\" vs \"{}\" in the source",
            show_ws(&tl),
            show_ws(&sl)
        ));
    }
    if st != tt {
        parts.push(format!(
            "trailing \"{}\" vs \"{}\" in the source",
            show_ws(&tt),
            show_ws(&st)
        ));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The source ends with sentence punctuation and the translation ends with none at all.
/// Any script's terminal mark counts, so `。`, `؟`, `।` and a closing quote/bracket pass.
pub fn punctuation_dropped(source: &str, translation: &str) -> Option<String> {
    const SOURCE_MARKS: &[char] = &[':', '.', '!', '?', '…'];
    const ANY_MARK: &[char] = &[
        ':', '.', '!', '?', '…', ';', ',', '。', '！', '？', '：', '、', '،', '؟', '।', '॥',
        '\u{FF0C}', '"', '\'', '”', '’', '»', ')', ']', '}', '>', '*', '_', '~',
    ];
    let last_src = source.chars().rev().find(|c| !c.is_whitespace())?;
    if !SOURCE_MARKS.contains(&last_src) {
        return None;
    }
    // A placeholder at the end (`Total: %d`) is a value, not a sentence.
    if source.trim_end().ends_with(['}', ')', '@', 'd', 's', 'f']) {
        return None;
    }
    // Short labels like "OK." are not sentences worth policing.
    if source.chars().filter(|c| c.is_alphabetic()).count() < 4 {
        return None;
    }
    let last_tr = translation.chars().rev().find(|c| !c.is_whitespace())?;
    if ANY_MARK.contains(&last_tr) || !last_tr.is_alphanumeric() {
        return None;
    }
    Some(format!(
        "source ends with `{last_src}`, translation with `{last_tr}`"
    ))
}
