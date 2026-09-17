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
    if !same_language && has_letters && !is_markup_only && src == tr {
        out.push(Finding {
            code: "identical",
            severity: Severity::Warning,
            message: "identical to the source text".into(),
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
            b'%' => {
                let mut j = i + 1;
                while j < b.len() && !(b[j].is_ascii_alphabetic() || b[j] == b'@' || b[j] == b'%') {
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
            _ => {
                let c = text[i..].chars().next().unwrap();
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}
