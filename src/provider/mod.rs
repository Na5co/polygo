//! Translation providers. Every backend implements [`Provider`]; the engine only
//! ever talks to the trait. Responses are requested as JSON and parsed leniently.

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai;

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

pub trait Provider {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn translate(&self, batch: &[Request], ctx: &Ctx) -> Result<Vec<Translation>>;
}

/// Build a provider from config. API keys come from the environment:
/// `POLYGO_API_KEY` first, then `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`.
pub fn from_config(cfg: &crate::config::Provider) -> Result<Box<dyn Provider + Sync>> {
    let model = cfg.model.clone();
    Ok(match cfg.kind.as_str() {
        "mock" => Box::new(mock::Mock::default()),
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
}

// ---- shared prompt + parsing -------------------------------------------------------

pub const SYSTEM_PROMPT: &str = "You are a professional software localizer translating user-interface strings \
from {src} into {dst}. Every \"translation\" value MUST be written in {dst}; copying the source text is a failure. \
Rules: keep every placeholder exactly as written ({hint}); keep inline markup and HTML tags; match the tone and \
terminology of the examples when given; be as short and natural as a native {dst} app would; never add explanations.\n\
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
            "### {} key: {}\nsource: {}\n",
            i + 1,
            r.key,
            r.source
        ));
        if let Some(c) = &r.comment {
            s.push_str(&format!("developer comment: {c}\n"));
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

/// Shared HTTP POST-JSON helper with a generous timeout for local models.
pub(crate) fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .build()
        .new_agent();
    let mut req = agent.post(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut resp = req.send_json(body).with_context(|| format!("POST {url}"))?;
    resp.body_mut()
        .read_json::<serde_json::Value>()
        .context("reading response JSON")
}
