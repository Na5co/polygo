//! Content checks that catch what a file diff hides: text saved in the wrong encoding,
//! characters nobody can see, a link that changed, a bracket left open, an entity
//! escaped twice. Each returns a message when the translation has the problem.

/// UTF-8 bytes read as Latin-1/CP1252 and saved again: `Ã©` for `é`, `â€™` for `’`,
/// `Â ` for a non-breaking space. Nobody types these; a file was re-encoded on the way.
pub fn mojibake(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    for w in chars.windows(2) {
        let (a, b) = (w[0], w[1]);
        let cp1252_tail =
            matches!(b as u32, 0x80..=0xBF) || "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".contains(b);
        if (a == 'Ã' || a == 'Â' || a == 'â' || a == 'Ð' || a == 'Ñ') && cp1252_tail {
            return Some(format!(
                "looks like text saved in the wrong encoding (`{a}{b}`)"
            ));
        }
    }
    None
}

/// Characters that do not show but change behaviour: a zero-width space inside a word,
/// a BOM in the middle of a string, line/paragraph separators, bidi embedding controls
/// left behind by an editor, and C0 controls. ZWNJ/ZWJ and LRM/RLM are left alone:
/// Persian, Arabic and Indic text need them.
pub fn invisible(text: &str) -> Option<String> {
    for (i, c) in text.char_indices() {
        let name = match c {
            '\u{200B}' => "zero-width space",
            '\u{FEFF}' if i > 0 => "byte-order mark",
            '\u{2028}' => "line separator U+2028",
            '\u{2029}' => "paragraph separator U+2029",
            '\u{202A}'..='\u{202E}' => "bidi embedding control",
            '\u{0000}'..='\u{0008}' | '\u{000B}' | '\u{000C}' | '\u{000E}'..='\u{001F}' => {
                "control character"
            }
            _ => continue,
        };
        return Some(format!(
            "{name} (U+{:04X}) at character {}",
            c as u32,
            i + 1
        ));
    }
    None
}

/// URLs and email addresses, found by scanning rather than splitting on spaces (CJK text
/// has none). ASCII only, which is what a link in UI text is.
fn links(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let url_char = |c: u8| c.is_ascii_graphic() && !matches!(c, b'<' | b'>' | b'"' | b'\'');
    let mail_char =
        |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'%' | b'+' | b'-');
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"http://") || b[i..].starts_with(b"https://") {
            let mut j = i;
            while j < b.len() && url_char(b[j]) {
                j += 1;
            }
            // All ASCII, so the slice is on char boundaries.
            let w = text[i..j].trim_end_matches(['.', ',', ';', ':', ')', ']', '!', '?']);
            if w.len() > 8 {
                out.push(w.to_string());
            }
            i = j.max(i + 1);
            continue;
        }
        if b[i] == b'@' && i > 0 && mail_char(b[i - 1]) && b[i - 1] != b'%' {
            let mut s = i;
            while s > 0 && mail_char(b[s - 1]) {
                s -= 1;
            }
            let mut e = i + 1;
            while e < b.len() && (b[e].is_ascii_alphanumeric() || matches!(b[e], b'.' | b'-')) {
                e += 1;
            }
            let domain = text[i + 1..e].trim_end_matches(['.', '-']);
            let local = &text[s..i];
            // `%#@sites@` is an Xcode substitution, not an address.
            let substitution = s > 0 && matches!(b[s - 1], b'@' | b'#');
            if !local.is_empty()
                && !substitution
                && !local.contains('%')
                && !domain.starts_with('.')
                && domain.contains('.')
                && domain.rsplit('.').next().is_some_and(|tld| {
                    tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
                })
            {
                out.push(format!("{local}@{domain}"));
            }
            i = e.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

/// A source that is an identifier rather than text (`modals.create-webhook.url`: Penpot
/// keeps English in `en.po`, the msgid is a key) has nothing to compare links against.
fn source_is_key(source: &str) -> bool {
    !source.contains(char::is_whitespace) && source.contains('.') && !source.contains("://")
}

/// Every URL and email address in the source must appear unchanged in the translation,
/// and the translation must not point somewhere the source does not.
pub fn link_mismatch(source: &str, translation: &str) -> Option<String> {
    if source_is_key(source) {
        return None;
    }
    let (s, t) = (links(source), links(translation));
    if s.is_empty() && t.is_empty() {
        return None;
    }
    let missing: Vec<&String> = s.iter().filter(|l| !t.contains(l)).collect();
    let extra: Vec<&String> = t.iter().filter(|l| !s.contains(l)).collect();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!(
            "missing {}",
            missing
                .iter()
                .map(|l| format!("`{l}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !extra.is_empty() {
        parts.push(format!(
            "unexpected {}",
            extra
                .iter()
                .map(|l| format!("`{l}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some(parts.join("; "))
}

// Guillemets are not here: German and Danish write »…«, French «…», the reverse.
const PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('「', '」'),
    ('『', '』'),
    ('（', '）'),
    ('【', '】'),
];

fn balance(text: &str, open: char, close: char) -> (usize, usize) {
    (
        text.chars().filter(|c| *c == open).count(),
        text.chars().filter(|c| *c == close).count(),
    )
}

/// A bracket pair the source keeps balanced and the translation does not.
pub fn unbalanced(source: &str, translation: &str) -> Option<String> {
    if source_is_key(source) {
        return None;
    }
    for (open, close) in PAIRS {
        let (so, sc) = balance(source, *open, *close);
        let (to, tc) = balance(translation, *open, *close);
        if so == sc && to != tc {
            return Some(format!(
                "`{open}` opened {to} time(s), `{close}` closed {tc}"
            ));
        }
    }
    None
}

/// `&amp;amp;`, `&amp;lt;` …: an entity escaped twice shows the escape on screen.
pub fn double_escaped(text: &str) -> Option<String> {
    for needle in [
        "&amp;amp;",
        "&amp;lt;",
        "&amp;gt;",
        "&amp;quot;",
        "&amp;#",
        "&amp;nbsp;",
    ] {
        if text.contains(needle) {
            return Some(format!("`{needle}` is an entity escaped twice"));
        }
    }
    None
}

/// Android format strings: two or more unnumbered `%s`/`%d` in one string are an aapt
/// error ("Multiple substitutions specified in non-positional format"), unless the
/// element says `formatted="false"`.
pub fn android_unnumbered_args(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut unnumbered = 0;
    while i + 1 < b.len() {
        if b[i] == b'%' {
            match b[i + 1] {
                b'%' => {
                    i += 2;
                    continue;
                }
                c if c.is_ascii_digit() => {
                    // `%1$s`: numbered. `%5.2f` would be a width, but Android strings
                    // number their arguments, so treat digits as a position.
                    i += 2;
                    continue;
                }
                c if c.is_ascii_alphabetic() || c == b'.' || c == b'-' => unnumbered += 1,
                _ => {}
            }
        }
        i += 1;
    }
    (unnumbered >= 2).then(|| {
        format!(
            "{unnumbered} unnumbered format arguments: Android needs %1$s, %2$s… (or formatted=\"false\")"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mojibake_and_invisible() {
        assert!(mojibake("Ã©tÃ©").is_some());
        assert!(mojibake("donâ€™t").is_some());
        assert!(mojibake("été").is_none());
        assert!(mojibake("Ãngela").is_none()); // a real name: Ã + n is not a mojibake pair
        assert!(invisible("zero\u{200B}width").is_some());
        assert!(invisible("\u{FEFF}leading bom is the file's").is_none());
        assert!(invisible("mid\u{FEFF}dle").is_some());
        assert!(invisible("نیم\u{200C}فاصله").is_none()); // ZWNJ is Persian orthography
        assert!(invisible("tab\tand newline\n").is_none());
    }

    #[test]
    fn links_brackets_entities_args() {
        assert_eq!(
            link_mismatch("See https://a.io/help.", "Voir https://a.io/help."),
            None
        );
        assert!(
            link_mismatch("See https://a.io/help.", "Voir https://a.io/aide.")
                .unwrap()
                .contains("missing `https://a.io/help`")
        );
        assert!(link_mismatch("Mail support@a.io", "Mail soporte@a.io").is_some());
        assert_eq!(link_mismatch("Mail %@", "Mail %@"), None);
        // CJK text has no spaces; the address is still found. Xcode substitutions and
        // markdown links are not addresses.
        assert_eq!(
            link_mismatch(
                "Write to support@signal.org.",
                "support@signal.orgに連絡してください。"
            ),
            None
        );
        assert_eq!(
            link_mismatch("%#@sites@.Puede", "%#@sites@.Operación"),
            None
        );
        assert_eq!(
            link_mismatch("Clear %#@sites@. Ok", "Borrar %#@sites@.Puede"),
            None
        );
        assert_eq!(
            link_mismatch("[support@a.io](%s)", "[support@a.io](%s)"),
            None
        );
        assert_eq!(
            link_mismatch("modals.webhook.url", "https://example.com/x"),
            None
        );
        assert_eq!(unbalanced("Save (all)", "Alles speichern (alle)"), None);
        assert!(unbalanced("Save (all)", "Alles speichern (alle").is_some());
        assert_eq!(unbalanced("Smile :)", "Sourire :)"), None); // source unbalanced too
        assert!(double_escaped("Terms &amp;amp; conditions").is_some());
        assert!(double_escaped("Terms &amp; conditions").is_none());
        assert!(android_unnumbered_args("%s of %s").is_some());
        assert!(android_unnumbered_args("%1$s of %2$s").is_none());
        assert!(android_unnumbered_args("100%% of %s").is_none());
        assert!(android_unnumbered_args("%d%%").is_none());
    }
}
