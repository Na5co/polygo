//! Choose existing translations that resemble the string being translated, to show the
//! model the project's tone and terminology. Plain token Jaccard with stopwords removed
//! is enough here: candidates are short UI strings from the same app.

use std::collections::HashSet;

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "to", "of", "and", "or", "in", "on", "at", "for", "is", "are", "be", "this",
    "that", "these", "those", "it", "its", "you", "your", "we", "our", "with", "from", "by", "as",
    "do", "does", "not", "no", "yes", "can", "will", "want", "sure",
];

fn tokens(s: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut HashSet<String>| {
        if cur.len() >= 2
            && !STOPWORDS.contains(&cur.as_str())
            && cur.chars().any(|c| c.is_alphabetic())
        {
            out.insert(cur.clone());
        }
        cur.clear();
    };
    for c in s.chars() {
        if c.is_alphanumeric() {
            cur.extend(c.to_lowercase());
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Up to `k` `(source, translation)` pairs most similar to `source`, best first.
/// Pairs whose source is identical to `source` and pairs with no token overlap are skipped.
pub fn select(source: &str, candidates: &[(String, String)], k: usize) -> Vec<(String, String)> {
    let target = tokens(source);
    if target.is_empty() || k == 0 {
        return vec![];
    }
    let mut scored: Vec<(f64, usize)> = candidates
        .iter()
        .enumerate()
        // Skip the string itself, empty translations, untranslated entries (a translation
        // identical to its source would teach the model to leave things in English) and
        // pairs with broken placeholders.
        .filter(|(_, (s, t))| {
            s != source
                && !t.trim().is_empty()
                && !s.trim().eq_ignore_ascii_case(t.trim())
                && crate::check::placeholders::compare(s, t).is_none()
        })
        .filter_map(|(i, (s, _))| {
            let ts = tokens(s);
            let inter = target.intersection(&ts).count();
            if inter == 0 {
                return None;
            }
            let union = target.union(&ts).count();
            let jaccard = inter as f64 / union as f64;
            // Mild preference for candidates of similar length (same kind of UI string).
            let len_pen = ((s.len() as f64) - (source.len() as f64)).abs() / 200.0;
            Some((jaccard - len_pen, i))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    scored
        .into_iter()
        .take(k)
        .map(|(_, i)| candidates[i].clone())
        .collect()
}
