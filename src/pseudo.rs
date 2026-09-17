//! Pseudo-localization: `[Šéţţíñĝš ~~~]`: accented, ~40 % longer, bracketed, with
//! every placeholder and markup tag left exactly as it was. Run the app in this locale
//! and hardcoded strings stay plain English, tight layouts truncate, and a broken
//! placeholder shows up as literal text.

/// Android's official pseudolocale; on iOS launch with `-AppleLanguages (en-XA)`.
pub const DEFAULT_LOCALE: &str = "en-XA";

pub fn transform(source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    let mut letters = 0usize;
    let b = source.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let rest = &source[i..];
        if let Some(n) = placeholder_len(rest) {
            out.push_str(&rest[..n]);
            i += n;
            continue;
        }
        let c = rest.chars().next().unwrap();
        let mapped = accent(c);
        if mapped != c || c.is_alphabetic() {
            letters += 1;
        }
        out.push(mapped);
        i += c.len_utf8();
    }
    // Expansion: German/Finnish routinely run 30–40 % longer than English.
    let pad = letters.div_ceil(3);
    if pad > 0 {
        out.push(' ');
        out.extend(std::iter::repeat_n('~', pad));
    }
    format!("[{out}]")
}

/// Length of a placeholder / markup token at the start of `s`, or `None`.
fn placeholder_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    match b[0] {
        b'%' => {
            if b.get(1) == Some(&b'%') {
                return Some(2);
            }
            if b.get(1) == Some(&b'#') && b.get(2) == Some(&b'@') {
                return s[3..].find('@').map(|k| 3 + k + 1);
            }
            let mut j = 1;
            while j < b.len() && !(b[j].is_ascii_alphabetic() || b[j] == b'@') {
                if b[j].is_ascii_whitespace() {
                    return None;
                }
                j += 1;
            }
            while j < b.len() && matches!(b[j], b'h' | b'l' | b'q' | b'L' | b'z' | b'j' | b't') {
                j += 1;
            }
            (j < b.len()).then_some(j + 1)
        }
        b'{' => {
            let mut depth = 0;
            for (k, c) in s.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(k + 1);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        b'<' => s.find('>').map(|k| k + 1),
        b'$' => {
            if s.starts_with("$t(") {
                return s.find(')').map(|k| k + 1);
            }
            let n = s[1..]
                .bytes()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
                .count();
            (n > 0).then_some(n + 1)
        }
        b'&' => {
            // XML entity (&amp;): keep whole.
            s.find(';')
                .filter(|k| {
                    *k < 8
                        && s[1..*k]
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'#')
                })
                .map(|k| k + 1)
        }
        b'\\' => (b.len() > 1).then_some(2),
        _ => None,
    }
}

fn accent(c: char) -> char {
    match c {
        'a' => 'á',
        'b' => 'ƀ',
        'c' => 'ç',
        'd' => 'ð',
        'e' => 'é',
        'f' => 'ƒ',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'í',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ļ',
        'm' => 'ɱ',
        'n' => 'ñ',
        'o' => 'ö',
        'p' => 'þ',
        'q' => 'ǫ',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'ü',
        'v' => 'ṽ',
        'w' => 'ŵ',
        'x' => 'ẋ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'Á',
        'B' => 'Ɓ',
        'C' => 'Ç',
        'D' => 'Ð',
        'E' => 'É',
        'F' => 'Ƒ',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Í',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ļ',
        'M' => 'Ṁ',
        'N' => 'Ñ',
        'O' => 'Ö',
        'P' => 'Þ',
        'Q' => 'Ǫ',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Ü',
        'V' => 'Ṽ',
        'W' => 'Ŵ',
        'X' => 'Ẋ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

/// Write the pseudo locale for every unit. Plural units are written for the categories
/// the source itself has (`one`/`other`), whatever the real targets need.
pub fn write(
    root: &std::path::Path,
    cfg: &crate::config::Config,
    locale: &str,
) -> anyhow::Result<usize> {
    let units = crate::project::load_units(root, cfg)?;
    let items: Vec<(String, String)> = units
        .iter()
        .filter(|u| match crate::core::split_plural(&u.key) {
            Some((_, cat)) => matches!(cat, "one" | "other" | "zero"),
            None => true,
        })
        .map(|u| (u.key.clone(), transform(&u.source)))
        .collect();
    let refs: Vec<(&str, &str, &str)> = items
        .iter()
        .map(|(k, t)| (k.as_str(), locale, t.as_str()))
        .collect();
    crate::engine::write_translations(root, cfg, &refs)?;
    Ok(refs.len())
}
