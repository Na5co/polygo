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
    for word in stripped.split(|c: char| !c.is_alphanumeric() && c != '\'') {
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
