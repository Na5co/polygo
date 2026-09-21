//! Translation providers. Every backend implements [`Provider`]; the engine only
//! ever talks to the trait. Responses are requested as JSON and parsed leniently.

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai;

use crate::trace::{Kind, Span};
use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

/// One string to translate, with everything the model may use to do it well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub key: String,
    pub source: String,
    pub comment: Option<String>,
    /// Where the string is used (file, enclosing identifier, code snippet).
    pub context: Option<String>,
    /// Similar existing translations `(source, translation)` in the target locale.
    pub examples: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctx {
    pub source_locale: String,
    pub target_locale: String,
    /// Term → required translation in the target locale.
    pub glossary: Vec<(String, String)>,
    /// Terms that must appear untranslated.
    pub do_not_translate: Vec<String>,
    /// Placeholder conventions to preserve, e.g. "Apple .xcstrings (%@, %lld, %1$@)".
    pub format_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    pub key: String,
    pub text: String,
}

/// A key the model could not translate acceptably even after one repair round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub key: String,
    pub reason: String,
    /// The model's last attempt, if it produced anything for this key.
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub translations: Vec<Translation>,
    pub review: Vec<Review>,
}

/// Token counts as the backend reported them, when it did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// One raw model reply: the text, plus usage for the trace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub usage: Option<Usage>,
}

impl From<String> for Reply {
    fn from(text: String) -> Self {
        Reply { text, usage: None }
    }
}

impl From<&str> for Reply {
    fn from(text: &str) -> Self {
        Reply {
            text: text.to_string(),
            usage: None,
        }
    }
}

pub trait Provider: Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    /// One raw model call. Backends only implement this; prompting, parsing, repair
    /// and quarantine live in [`run`] so every backend behaves identically.
    fn complete(&self, system: &str, user: &str, ctx: &Ctx) -> Result<Reply>;

    /// Like [`complete`](Self::complete) but asking for a different JSON shape (used by
    /// `polygo audit`). Backends that enforce a schema override this; the default just
    /// relies on the prompt.
    fn complete_json(
        &self,
        system: &str,
        user: &str,
        ctx: &Ctx,
        _schema: &serde_json::Value,
    ) -> Result<Reply> {
        self.complete(system, user, ctx)
    }

    /// [`complete`](Self::complete) recorded as an `LLM` span under `parent` (a no-op
    /// span when tracing is off). Every call the engine makes goes through here.
    fn complete_traced(
        &self,
        parent: &Span,
        name: &str,
        system: &str,
        user: &str,
        ctx: &Ctx,
        schema: Option<&serde_json::Value>,
    ) -> Result<String> {
        let mut span = parent.child(name, Kind::Llm);
        span.set("llm.provider", self.name())
            .set("llm.system", self.name())
            .set("llm.model_name", self.model())
            .set_prompt(system, user);
        let result = match schema {
            Some(s) => self.complete_json(system, user, ctx, s),
            None => self.complete(system, user, ctx),
        };
        match &result {
            Ok(reply) => span.set_reply(reply),
            Err(e) => span.set_error(e),
        };
        span.end();
        result.map(|r| r.text)
    }

    fn translate(&self, batch: &[Request], ctx: &Ctx) -> Result<Outcome> {
        run(self, batch, ctx)
    }
}

/// Prompt → parse → validate → one repair round → quarantine what is still wrong.
/// One trace per batch when tracing is on (see [`crate::trace`]).
pub fn run<P: Provider + ?Sized>(provider: &P, batch: &[Request], ctx: &Ctx) -> Result<Outcome> {
    if batch.is_empty() {
        return Ok(Outcome::default());
    }
    let mut span = Span::root("translate batch", Kind::Chain);
    span.set("polygo.source_locale", ctx.source_locale.as_str())
        .set("polygo.target_locale", ctx.target_locale.as_str())
        .set_int("polygo.batch_size", batch.len() as i64)
        .set("input.mime_type", "application/json")
        .set(
            "input.value",
            serde_json::json!(
                batch
                    .iter()
                    .map(|r| serde_json::json!({ "key": r.key, "source": r.source }))
                    .collect::<Vec<_>>()
            )
            .to_string(),
        );
    let result = run_traced(provider, batch, ctx, &span);
    match &result {
        Ok(outcome) => {
            span.set_int("polygo.translated", outcome.translations.len() as i64)
                .set_int("polygo.review", outcome.review.len() as i64)
                .set("output.mime_type", "application/json")
                .set(
                    "output.value",
                    serde_json::json!({
                        "translations": outcome.translations.iter()
                            .map(|t| serde_json::json!({ "key": t.key, "text": t.text }))
                            .collect::<Vec<_>>(),
                        "review": outcome.review.iter()
                            .map(|r| serde_json::json!({ "key": r.key, "reason": r.reason, "suggestion": r.suggestion }))
                            .collect::<Vec<_>>(),
                    })
                    .to_string(),
                );
        }
        Err(e) => {
            span.set_error(e);
        }
    }
    span.end();
    result
}

fn run_traced<P: Provider + ?Sized>(
    provider: &P,
    batch: &[Request],
    ctx: &Ctx,
    span: &Span,
) -> Result<Outcome> {
    let system = system_prompt(ctx);
    let raw = provider.complete_traced(
        span,
        "translate",
        &system,
        &user_prompt(batch, ctx),
        ctx,
        None,
    )?;
    let wanted: Vec<&str> = batch.iter().map(|r| r.key.as_str()).collect();
    let mut found = parse_translations_lenient(&raw, &wanted);
    let mut problems = validate(batch, ctx, &found, &[]);

    if !problems.is_empty() {
        // Repair round: only the problem keys, with what went wrong spelled out.
        let retry: Vec<Request> = batch
            .iter()
            .filter(|r| problems.iter().any(|(k, _)| k == &r.key))
            .cloned()
            .collect();
        let mut note = String::from("Your previous reply had problems:\n");
        for (k, why) in &problems {
            note.push_str(&format!("- {k}: {why}\n"));
        }
        note.push_str(
            "If a string is correctly left identical to the source in the target language, return it again unchanged. \
Otherwise fix the listed problems and reply with the JSON only.\n\n",
        );
        let user = format!("{note}{}", user_prompt(&retry, ctx));
        let raw2 = provider.complete_traced(span, "repair", &system, &user, ctx, None)?;
        let retry_wanted: Vec<&str> = retry.iter().map(|r| r.key.as_str()).collect();
        let repaired = parse_translations_lenient(&raw2, &retry_wanted);
        // Identical-to-source is accepted once the model confirms it on the second ask.
        let confirmed: Vec<String> = problems
            .iter()
            .filter(|(_, why)| why.starts_with("identical"))
            .map(|(k, _)| k.clone())
            .collect();
        for t in repaired {
            found.retain(|f| f.key != t.key);
            found.push(t);
        }
        problems = validate(&retry, ctx, &found, &confirmed);
    }

    // Strings that still echo the source after the repair round *and* were shown code
    // context get one plain ask: code often contains the English string as an identifier,
    // which small models read as "keep it in English". Without the snippet they translate.
    let echoes: Vec<Request> = batch
        .iter()
        .filter(|r| {
            (r.context.is_some() || !r.examples.is_empty())
                && found
                    .iter()
                    .any(|t| t.key == r.key && t.text.trim() == r.source.trim())
        })
        .map(|r| Request {
            context: None,
            examples: vec![],
            ..r.clone()
        })
        .collect();
    if !echoes.is_empty() {
        let raw3 = provider.complete_traced(
            span,
            "echo retry",
            &system,
            &user_prompt(&echoes, ctx),
            ctx,
            None,
        )?;
        let wanted3: Vec<&str> = echoes.iter().map(|r| r.key.as_str()).collect();
        for t in parse_translations_lenient(&raw3, &wanted3) {
            let src = &echoes.iter().find(|r| r.key == t.key).unwrap().source;
            if t.text.trim() != src.trim() && !t.text.trim().is_empty() {
                found.retain(|f| f.key != t.key);
                found.push(t);
            }
        }
        problems = validate(
            batch,
            ctx,
            &found,
            &batch.iter().map(|r| r.key.clone()).collect::<Vec<_>>(),
        );
    }

    let mut outcome = Outcome::default();
    for r in batch {
        match problems.iter().find(|(k, _)| k == &r.key) {
            Some((_, why)) => outcome.review.push(Review {
                key: r.key.clone(),
                reason: why.clone(),
                suggestion: found
                    .iter()
                    .find(|t| t.key == r.key)
                    .map(|t| t.text.clone()),
            }),
            None => {
                if let Some(t) = found.iter().find(|t| t.key == r.key) {
                    outcome.translations.push(t.clone());
                }
            }
        }
    }
    Ok(outcome)
}

/// Problems with the current `found` set for `batch`: missing keys, empty output,
/// glossary/do-not-translate violations, and (unless in `confirmed`) source echoes.
fn validate(
    batch: &[Request],
    ctx: &Ctx,
    found: &[Translation],
    confirmed: &[String],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in batch {
        let Some(t) = found.iter().find(|t| t.key == r.key) else {
            out.push((r.key.clone(), "missing from the reply".into()));
            continue;
        };
        if t.text.trim().is_empty() {
            out.push((r.key.clone(), "empty translation".into()));
            continue;
        }
        let src_lower = r.source.to_lowercase();
        let tr_lower = t.text.to_lowercase();
        for term in &ctx.do_not_translate {
            if src_lower.contains(&term.to_lowercase()) && !t.text.contains(term.as_str()) {
                out.push((
                    r.key.clone(),
                    format!("glossary: `{term}` must stay untranslated"),
                ));
            }
        }
        for (term, required) in &ctx.glossary {
            if src_lower.contains(&term.to_lowercase())
                && !tr_lower.contains(&required.to_lowercase())
            {
                out.push((
                    r.key.clone(),
                    format!("glossary: `{term}` must be translated as `{required}`"),
                ));
            }
        }
        if out.iter().any(|(k, _)| k == &r.key) {
            continue;
        }
        if let Some(max) = crate::core::directives(r.comment.as_deref()).max_chars
            && t.text.chars().count() > max
        {
            out.push((
                r.key.clone(),
                format!(
                    "{} characters, but this string must fit in {max}: shorten it (abbreviate if you must)",
                    t.text.chars().count()
                ),
            ));
            continue;
        }
        if !crate::core::plural_form_may_omit_count(&r.key)
            && let Some(m) = crate::check::placeholders::compare(&r.source, &t.text)
        {
            out.push((
                r.key.clone(),
                format!(
                    "placeholders changed ({m}): keep every placeholder exactly as in the source, in the same form (do not add `1$` numbering unless the source has it)"
                ),
            ));
            continue;
        }
        if r.key != r.source && t.text.trim() == r.key.trim() {
            out.push((
                r.key.clone(),
                "returned the key instead of a translation of the source string".into(),
            ));
            continue;
        }
        let (sl, tl) = (r.source.chars().count(), t.text.chars().count());
        if tl > sl * 3 + 20 {
            out.push((
                r.key.clone(),
                "far longer than the source: translate only the source string, not its context"
                    .into(),
            ));
            continue;
        }
        let mut allowed: Vec<String> = ctx.do_not_translate.clone();
        allowed.extend(ctx.glossary.iter().map(|(_, v)| v.clone()));
        // One stray word in a long sentence is a `check` warning, not a reason to
        // bounce the whole string; two words, or one in a short string, is.
        let word_count = t.text.split_whitespace().count();
        if let Some(words) = crate::check::text::untranslated_fragment(&r.source, &t.text, &allowed)
            && (words.len() >= 2 || word_count <= 6)
        {
            out.push((
                r.key.clone(),
                format!(
                    "left untranslated: {}: translate every word into the target language",
                    words
                        .iter()
                        .map(|w| format!("`{w}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
            continue;
        }
        let same_language = ctx.source_locale.split(['-', '_']).next()
            == ctx.target_locale.split(['-', '_']).next();
        let has_letters = r.source.chars().any(|c| c.is_alphabetic());
        if !same_language
            && has_letters
            && t.text.trim() == r.source.trim()
            && !confirmed.contains(&r.key)
        {
            out.push((
                r.key.clone(),
                "identical to the source: translate it or confirm it must stay".into(),
            ));
        }
    }
    out
}

/// Like [`parse_translations_json`] but never fails: returns whatever keys were found.
pub fn parse_translations_lenient(raw: &str, wanted: &[&str]) -> Vec<Translation> {
    let Some(json) = extract_json_object(raw) else {
        return vec![];
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    let mut found: Vec<(String, String)> = Vec::new();
    if let Some(items) = value.get("translations").and_then(|v| v.as_array()) {
        for it in items {
            if let Ok(item) = serde_json::from_value::<Item>(it.clone()) {
                found.push((item.key, item.translation));
            }
        }
    } else if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(t) = v.as_str() {
                found.push((k.clone(), t.to_string()));
            }
        }
    }
    wanted
        .iter()
        .filter_map(|key| {
            found
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, text)| Translation {
                    key: (*key).to_string(),
                    text: text.clone(),
                })
        })
        .collect()
}

/// Build a provider from config. API keys come from the environment:
/// `POLYGO_API_KEY` first, then `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`.
pub fn from_config(cfg: &crate::config::Provider) -> Result<Box<dyn Provider>> {
    let model = cfg.model.clone();
    Ok(match cfg.kind.as_str() {
        "mock" => Box::new(mock::Mock),
        "ollama" => Box::new(ollama::Ollama::new(
            cfg.base_url.clone(),
            model.as_deref().unwrap_or("qwen3:8b"),
        )),
        "openai" | "openai-compatible" => Box::new(openai::OpenAiCompatible::new(
            cfg.base_url.clone(),
            model.as_deref().unwrap_or("gpt-4o-mini"),
            api_key(&["POLYGO_API_KEY", "OPENAI_API_KEY"]),
        )),
        "anthropic" => Box::new(anthropic::Anthropic::new(
            cfg.base_url.clone(),
            model.as_deref().unwrap_or("claude-sonnet-5"),
            api_key(&["POLYGO_API_KEY", "ANTHROPIC_API_KEY"]),
        )),
        other => bail!("unknown provider kind `{other}` (mock, ollama, openai, anthropic)"),
    })
}

fn api_key(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| std::env::var(n).ok())
        .filter(|k| !k.is_empty())
        .or_else(|| {
            // `polygo use … --api-key` stores it under the provider kind.
            let kind = if names.contains(&"OPENAI_API_KEY") {
                "openai"
            } else {
                "anthropic"
            };
            crate::models::stored_api_key(kind)
        })
}

// ---- shared prompt + parsing -------------------------------------------------------

pub const SYSTEM_PROMPT: &str = "You are a professional software localizer translating user-interface strings \
from {src} into {dst}. Every \"translation\" value MUST be written in {dst}; copying the source text is a failure. \
Rules: keep every placeholder exactly as written ({hint}); keep inline markup and HTML tags; match the tone and \
terminology of the examples when given; be as short and natural as a native {dst} app would; never add explanations. \
Any \"used in\" code shown is reference only, to tell a button from a heading: translate ONLY the source string, \
never the code or the other strings around it. Code identifiers often equal the English string; that is not a reason \
to keep it in English: user-facing text is translated unless it is a brand or proper noun.\n\
Respond with JSON only, one item per input key, in the same order:\n\
{\"translations\":[{\"key\":\"<key>\",\"translation\":\"<{dst} text>\"}]}";

pub fn locale_name(tag: &str) -> String {
    let lower = tag.to_lowercase();
    let mut parts = lower.split(['-', '_']);
    let lang = parts.next().unwrap_or("");
    let region = parts.next_back().filter(|r| r.len() == 2 || r.len() == 4);
    let name = match lang {
        "en" => "English",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "da" => "Danish",
        "nb" | "no" => "Norwegian",
        "fi" => "Finnish",
        "pl" => "Polish",
        "cs" => "Czech",
        "sk" => "Slovak",
        "hu" => "Hungarian",
        "ro" => "Romanian",
        "bg" => "Bulgarian",
        "el" => "Greek",
        "tr" => "Turkish",
        "ru" => "Russian",
        "uk" => "Ukrainian",
        "he" | "iw" => "Hebrew",
        "ar" => "Arabic",
        "fa" => "Persian",
        "hi" => "Hindi",
        "bn" => "Bengali",
        "th" => "Thai",
        "vi" => "Vietnamese",
        "id" | "in" => "Indonesian",
        "ms" => "Malay",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        "ca" => "Catalan",
        "hr" => "Croatian",
        "sr" => "Serbian",
        "sl" => "Slovenian",
        "lt" => "Lithuanian",
        "lv" => "Latvian",
        "et" => "Estonian",
        "ta" => "Tamil",
        "te" => "Telugu",
        "ur" => "Urdu",
        "sw" => "Swahili",
        "af" => "Afrikaans",
        "fil" | "tl" => "Filipino",
        "ga" => "Irish",
        "eu" => "Basque",
        "gl" => "Galician",
        "is" => "Icelandic",
        _ => return tag.to_string(),
    };
    let qualifier = match (lang, region) {
        ("zh", Some("hans")) | ("zh", Some("cn")) => Some("Simplified"),
        ("zh", Some("hant")) | ("zh", Some("tw")) | ("zh", Some("hk")) => Some("Traditional"),
        ("pt", Some("br")) => Some("Brazil"),
        ("pt", Some("pt")) => Some("Portugal"),
        ("en", Some("gb")) => Some("UK"),
        ("en", Some("us")) => Some("US"),
        ("es", Some("mx")) => Some("Mexico"),
        ("es", Some("419")) => Some("Latin America"),
        ("fr", Some("ca")) => Some("Canada"),
        ("nl", Some("be")) => Some("Belgium"),
        _ => None,
    };
    match qualifier {
        Some(q) => format!("{name} ({q}) ({tag})"),
        None => format!("{name} ({tag})"),
    }
}

pub fn system_prompt(ctx: &Ctx) -> String {
    let mut s = SYSTEM_PROMPT
        .replace("{src}", &locale_name(&ctx.source_locale))
        .replace("{dst}", &locale_name(&ctx.target_locale))
        .replace(
            "{hint}",
            ctx.format_hint
                .as_deref()
                .unwrap_or("e.g. %@, %d, %1$s, {name}, {{name}}"),
        );
    if !ctx.glossary.is_empty() {
        s.push_str("\nGlossary (always use these translations):");
        for (term, tr) in &ctx.glossary {
            s.push_str(&format!("\n- {term} → {tr}"));
        }
    }
    if !ctx.do_not_translate.is_empty() {
        s.push_str("\nNever translate these terms: ");
        s.push_str(&ctx.do_not_translate.join(", "));
    }
    s
}

pub fn user_prompt(batch: &[Request], ctx: &Ctx) -> String {
    let src = locale_name(&ctx.source_locale);
    let dst = locale_name(&ctx.target_locale);
    let mut s = format!(
        "Translate the following {} string(s) from {src} into {dst}.\n\n",
        batch.len()
    );
    for (i, r) in batch.iter().enumerate() {
        s.push_str(&format!(
            "### {} key: {}\nsource ({src}): {}\n",
            i + 1,
            r.key,
            r.source
        ));
        if let Some(c) = &r.comment {
            s.push_str(&format!("developer comment: {c}\n"));
            if let Some(max) = crate::core::directives(Some(c)).max_chars {
                s.push_str(&format!(
                    "length limit: at most {max} characters (UI space is fixed)\n"
                ));
            }
        }
        if let Some(c) = &r.context {
            s.push_str(&format!("used in:\n{c}\n"));
        }
        if !r.examples.is_empty() {
            s.push_str("similar existing translations:\n");
            for (src, tr) in &r.examples {
                s.push_str(&format!("- {src} → {tr}\n"));
            }
        }
        s.push('\n');
    }
    s.push_str(&format!(
        "Now return the JSON with every \"translation\" written in {dst}."
    ));
    s
}

/// JSON schema for providers that support constrained output (Ollama, OpenAI-compatible).
pub fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "translations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "key": { "type": "string" }, "translation": { "type": "string" } },
                    "required": ["key", "translation"]
                }
            }
        },
        "required": ["translations"]
    })
}

#[derive(Deserialize)]
struct Item {
    key: String,
    #[serde(alias = "text", alias = "value", alias = "target")]
    translation: String,
}

/// Parse a model response into translations for exactly `wanted` keys (in that order).
/// Accepts a bare JSON object, a fenced block, extra prose around it, and the
/// `{"key": "text"}` shorthand. Missing keys are an error naming them.
pub fn parse_translations_json(raw: &str, wanted: &[&str]) -> Result<Vec<Translation>> {
    let json = extract_json_object(raw).context("no JSON object in model response")?;
    let value: serde_json::Value =
        serde_json::from_str(json).context("model response is not valid JSON")?;
    let mut found: Vec<(String, String)> = Vec::new();
    if let Some(items) = value.get("translations").and_then(|v| v.as_array()) {
        for it in items {
            if let Ok(item) = serde_json::from_value::<Item>(it.clone()) {
                found.push((item.key, item.translation));
            }
        }
    } else if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(t) = v.as_str() {
                found.push((k.clone(), t.to_string()));
            }
        }
    }
    let mut out = Vec::with_capacity(wanted.len());
    let mut missing = Vec::new();
    for key in wanted {
        match found.iter().find(|(k, _)| k == key) {
            Some((_, text)) => out.push(Translation {
                key: (*key).to_string(),
                text: text.clone(),
            }),
            None => missing.push(*key),
        }
    }
    if !missing.is_empty() {
        bail!(
            "model response missing translations for: {}",
            missing.join(", ")
        );
    }
    Ok(out)
}

/// Find the outermost `{ ... }` in a response, skipping prose and code fences.
fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in raw[start..].char_indices() {
        if in_str {
            match c {
                '\\' if !esc => esc = true,
                '"' if !esc => in_str = false,
                _ => esc = false,
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

/// A failure that retrying will not fix: the server is not there, the key is wrong,
/// the model does not exist. Printed as the problem plus the fix, like `doctor`.
#[derive(Debug)]
pub struct Fatal {
    pub what: String,
    pub fix: String,
}

impl std::fmt::Display for Fatal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\n  fix: {}", self.what, self.fix)
    }
}

impl std::error::Error for Fatal {}

pub fn fatal(what: impl Into<String>, fix: impl Into<String>) -> anyhow::Error {
    Fatal {
        what: what.into(),
        fix: fix.into(),
    }
    .into()
}

/// Anywhere in the chain: the engine does not retry these and prints them as they are.
pub fn is_fatal(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<Fatal>())
}

/// What went wrong with an HTTP call, for providers to turn into a `Fatal` or a retry.
#[derive(Debug)]
pub enum HttpError {
    /// TCP/DNS level: nothing is listening, or the host does not resolve.
    Unreachable(String),
    /// Non-2xx reply; the body is what the server said (truncated).
    Status { code: u16, body: String },
    /// Timeout, bad JSON, anything else worth one more try.
    Other(anyhow::Error),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Unreachable(e) => write!(f, "{e}"),
            HttpError::Status { code, body } if body.is_empty() => write!(f, "http status {code}"),
            HttpError::Status { code, body } => write!(f, "http status {code}: {body}"),
            HttpError::Other(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for HttpError {}

impl HttpError {
    /// A short quote of the server's own error text, if any.
    pub fn server_says(&self) -> Option<String> {
        let HttpError::Status { body, .. } = self else {
            return None;
        };
        let msg = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or_else(|| v.get("error"))
                    .or_else(|| v.get("message"))
                    .and_then(|m| m.as_str().map(str::to_string))
            })
            .unwrap_or_else(|| body.clone());
        // First sentence, at most 120 chars: enough to see why, not the whole essay.
        let msg = msg.trim();
        let msg = msg.split_once(". ").map_or(msg, |(first, _)| first);
        (!msg.is_empty()).then(|| msg.chars().take(120).collect())
    }
}

/// Shared HTTP POST-JSON helper with a generous timeout for local models.
pub(crate) fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &serde_json::Value,
) -> std::result::Result<serde_json::Value, HttpError> {
    let secs = std::env::var("POLYGO_HTTP_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(secs)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut req = agent.post(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut resp = req.send_json(body).map_err(|e| match &e {
        ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => {
            HttpError::Unreachable(format!("cannot reach {url}: {e}"))
        }
        ureq::Error::Io(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::NotFound
            ) =>
        {
            HttpError::Unreachable(format!("cannot reach {url}: {io}"))
        }
        _ => HttpError::Other(anyhow::Error::new(e).context(format!("POST {url}"))),
    })?;
    let code = resp.status().as_u16();
    if !(200..300).contains(&code) {
        let body = resp
            .body_mut()
            .read_to_string()
            .unwrap_or_default()
            .chars()
            .take(2000)
            .collect();
        return Err(HttpError::Status { code, body });
    }
    resp.body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|e| HttpError::Other(anyhow::Error::new(e).context("reading response JSON")))
}
