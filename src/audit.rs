//! `polygo audit`: a second model grades existing translations: meaning, grammar,
//! register, plural form: and says why. Structural checks (`polygo check`) cannot see
//! "Достъп за достъп"; a judge can.

use crate::config::Config;
use crate::core::Unit;
use crate::provider::{Ctx, Provider, locale_name};
use crate::trace::{Kind, Span};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub locale: String,
    pub key: String,
    pub source: String,
    pub translation: String,
    /// 1 = wrong, 3 = awkward, 5 = native quality.
    pub score: u8,
    pub issue: String,
}

pub struct Options {
    pub locales: Option<Vec<String>>,
    /// Flag verdicts with `score <= threshold`.
    pub threshold: u8,
    pub batch_size: usize,
    /// Only the first N strings per locale (0 = all).
    pub limit: usize,
    pub context: bool,
}

const SYSTEM: &str = "\
You are a senior {dst} localization reviewer auditing UI strings translated from {src}.
For each item judge the {dst} translation against the source and its context:
- 5: native quality, correct meaning, natural for a UI
- 4: correct, minor style issue
- 3: understandable but awkward, wrong register, or an unnatural calque
- 2: grammar or agreement error, or a meaning that drifts
- 1: wrong meaning, untranslated, nonsense, or a placeholder used wrongly
Judge the translation only. A source that is itself just a placeholder or a code-like \
token is fine when left as is. Reply with JSON only: \
{\"verdicts\":[{\"key\":\"<the key exactly as given after 'key:'>\",\"score\":<1-5>,\"issue\":\"<why, one sentence; empty for 4 and 5>\"}]}";

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "verdicts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "score": { "type": "integer", "minimum": 1, "maximum": 5 },
                        "issue": { "type": "string" }
                    },
                    "required": ["key", "score", "issue"]
                }
            }
        },
        "required": ["verdicts"]
    })
}

fn user_prompt(
    items: &[(&Unit, &str)],
    ctx: &Ctx,
    usages: &std::collections::HashMap<String, crate::context::usage::Usage>,
) -> String {
    let (src, dst) = (
        locale_name(&ctx.source_locale),
        locale_name(&ctx.target_locale),
    );
    let mut s = format!(
        "Audit the following {} {dst} translation(s) of {src} UI strings.\n\n",
        items.len()
    );
    for (i, (u, t)) in items.iter().enumerate() {
        s.push_str(&format!("### {} key: {}\n", i + 1, u.key));
        s.push_str(&format!("source ({src}): {}\n", u.source));
        s.push_str(&format!("translation ({dst}): {t}\n"));
        if let Some(c) = &u.comment {
            s.push_str(&format!("developer comment: {c}\n"));
        }
        if let Some(us) = usages.get(&u.key) {
            let head = match &us.ident {
                Some(id) => format!("{}:{} inside `{id}`", us.path, us.line),
                None => format!("{}:{}", us.path, us.line),
            };
            s.push_str(&format!("used in: {head}\n"));
        }
        s.push('\n');
    }
    s
}

pub fn run(
    root: &Path,
    cfg: &Config,
    judge: &dyn Provider,
    opts: &Options,
    mut progress: impl FnMut(&str, usize, usize),
) -> Result<Vec<Verdict>> {
    let units = crate::project::load_units(root, cfg)?;
    let locales = opts
        .locales
        .clone()
        .unwrap_or_else(|| cfg.target_locales.clone());
    let index = if opts.context {
        Some(crate::context::usage::Index::build(root)?)
    } else {
        None
    };
    let all_keys: Vec<&str> = units
        .iter()
        .map(|u| crate::core::base_key(&u.key))
        .collect();
    let usages: std::collections::HashMap<String, crate::context::usage::Usage> = index
        .map(|ix| {
            let found = ix.find_all(&all_keys);
            units
                .iter()
                .filter_map(|u| {
                    found
                        .get(crate::core::base_key(&u.key))
                        .map(|us| (u.key.clone(), us.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    let glossary = crate::glossary::load(root, cfg)?;

    let mut out = Vec::new();
    for locale in &locales {
        let mut items: Vec<(&Unit, &str)> = units
            .iter()
            .filter_map(|u| u.translations.get(locale).map(|t| (u, t.as_str())))
            .filter(|(u, t)| u.source.trim() != t.trim())
            .collect();
        if opts.limit > 0 {
            items.truncate(opts.limit);
        }
        let ctx = Ctx {
            source_locale: cfg.source_locale.clone(),
            target_locale: locale.clone(),
            glossary: glossary.terms_for(locale),
            do_not_translate: glossary.do_not_translate.clone(),
            format_hint: None,
        };
        let system = SYSTEM
            .replace("{src}", &locale_name(&ctx.source_locale))
            .replace("{dst}", &locale_name(&ctx.target_locale));
        let total = items.len().div_ceil(opts.batch_size.max(1));
        for (i, batch) in items.chunks(opts.batch_size.max(1)).enumerate() {
            progress(locale, i + 1, total);
            let mut span = Span::root("audit batch", Kind::Chain);
            span.set("polygo.target_locale", locale.as_str())
                .set_int("polygo.batch_size", batch.len() as i64);
            let raw = judge
                .complete_traced(
                    &span,
                    "audit",
                    &system,
                    &user_prompt(batch, &ctx, &usages),
                    &ctx,
                    Some(&schema()),
                )
                .inspect_err(|e| {
                    span.set_error(e);
                })
                .with_context(|| format!("{locale}: audit batch {}/{total}", i + 1))?;
            span.set("output.value", raw.as_str());
            span.end();
            let verdicts = parse(&raw);
            for (n, (u, t)) in batch.iter().enumerate() {
                let (score, issue) = lookup(&verdicts, &u.key, &u.source, n + 1)
                    .unwrap_or((0, "no verdict from the judge".to_string()));
                if score == 0 || score <= opts.threshold {
                    out.push(Verdict {
                        locale: locale.clone(),
                        key: u.key.clone(),
                        source: u.source.clone(),
                        translation: (*t).to_string(),
                        score,
                        issue,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Judges paraphrase keys: match exactly, then by the item's number, then by a
/// normalized (lowercase, alphanumeric-only) form, then by unique prefix.
fn lookup(
    verdicts: &BTreeMap<String, (u8, String)>,
    key: &str,
    source: &str,
    n: usize,
) -> Option<(u8, String)> {
    // Judges often answer with the source text instead of the key.
    if let Some(v) = verdicts.get(key).or_else(|| verdicts.get(source)) {
        return Some(v.clone());
    }
    if let Some(v) = verdicts.get(&n.to_string()) {
        return Some(v.clone());
    }
    let norm = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let wants = [norm(key), norm(source)];
    let mut candidates = verdicts.iter().filter(|(k, _)| {
        let kn = norm(k);
        !kn.is_empty()
            && wants.iter().any(|want| {
                !want.is_empty() && (kn == *want || want.starts_with(&kn) || kn.starts_with(want))
            })
    });
    let first = candidates.next()?;
    if candidates.next().is_some() {
        return None; // ambiguous
    }
    Some(first.1.clone())
}

fn parse(raw: &str) -> BTreeMap<String, (u8, String)> {
    let mut out = BTreeMap::new();
    let start = raw.find('{').unwrap_or(0);
    let end = raw.rfind('}').map(|e| e + 1).unwrap_or(raw.len());
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[start..end]) else {
        return out;
    };
    if let Some(items) = v["verdicts"].as_array() {
        for it in items {
            let key = it["key"]
                .as_str()
                .map(str::to_string)
                .or_else(|| it["key"].as_u64().map(|n| n.to_string()));
            if let (Some(k), Some(score)) = (key, it["score"].as_u64()) {
                out.insert(
                    k,
                    (
                        score.clamp(1, 5) as u8,
                        it["issue"]
                            .as_str()
                            .unwrap_or("")
                            .trim()
                            .trim_matches(|c| c == '<' || c == '>')
                            .to_string(),
                    ),
                );
            }
        }
    }
    out
}

/// Keys to re-translate per locale (for `--fix`).
pub fn keys_by_locale(verdicts: &[Verdict]) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for v in verdicts {
        m.entry(v.locale.clone()).or_default().push(v.key.clone());
    }
    m
}
