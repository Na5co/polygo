//! Deterministic, offline provider for tests. Implements `complete` like a real
//! backend: it reads the keys and sources out of the prompt and replies with JSON
//! (`⟦<locale>⟧ <source>`), so the shared prompt/parse/repair path is exercised.
//!
//! Test hooks (environment variables):
//! - `POLYGO_MOCK_PANIC_AFTER=N`   exit(70) on the (N+1)th string: simulates a crash
//! - `POLYGO_MOCK_LOG=path`        append every string key handled
//! - `POLYGO_MOCK_IGNORE_GLOSSARY` break glossary terms instead of honouring them
//! - `POLYGO_MOCK_MALFORMED`       always reply with garbage
//! - `POLYGO_MOCK_MALFORMED_ONCE`  garbage on the first call only
//! - `POLYGO_MOCK_DROP_KEY=k`      never include key `k` in replies
//! - `POLYGO_MOCK_ECHO_KEY=k`      reply with the source text for key `k`
//! - `POLYGO_MOCK_DUMP=path`       append every user prompt received (for prompt tests)
//! - `POLYGO_MOCK_KEY_ECHO_ONCE`   on the first call, reply with the key instead of a translation
//! - `POLYGO_MOCK_FRAGMENT=k`      reply in Japanese script but leave the source's last word for key `k`
//! - `POLYGO_MOCK_FRAGMENT_ONCE`   like FRAGMENT but only on the first call
//! - `POLYGO_MOCK_BREAK_PLACEHOLDER=k`      drop the first `%` placeholder for key `k`
//! - `POLYGO_MOCK_BREAK_PLACEHOLDER_ONCE`   like BREAK_PLACEHOLDER but only on the first call

use super::{Ctx, Provider, Reply};
use anyhow::Result;
use std::sync::atomic::{AtomicUsize, Ordering};

static STRINGS: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Default, Clone)]
pub struct Mock;

impl Mock {
    /// Invert a mock translation back into `(locale, source)`.
    pub fn reverse(text: &str) -> Option<(String, String)> {
        let rest = text.strip_prefix('⟦')?;
        let (locale, src) = rest.split_once("⟧ ")?;
        Some((locale.to_string(), src.to_string()))
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl Provider for Mock {
    fn name(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "mock-1"
    }

    fn complete_json(
        &self,
        _system: &str,
        user: &str,
        _ctx: &Ctx,
        _schema: &serde_json::Value,
    ) -> Result<Reply> {
        // Audit prompts use the same `### n key: <key>` / `source …: <text>` layout.
        let bad: Vec<String> = env("POLYGO_MOCK_AUDIT_BAD")
            .map(|v| v.split(',').map(str::to_string).collect())
            .unwrap_or_default();
        let verdicts: Vec<serde_json::Value> = parse_prompt(user)
            .into_iter()
            .map(|(key, _)| {
                let is_bad = bad.contains(&key);
                serde_json::json!({
                    "key": key,
                    "score": if is_bad { 1 } else { 5 },
                    "issue": if is_bad { "wrong meaning (mock)" } else { "" }
                })
            })
            .collect();
        Ok(serde_json::json!({ "verdicts": verdicts })
            .to_string()
            .into())
    }

    fn complete(&self, _system: &str, user: &str, ctx: &Ctx) -> Result<Reply> {
        let call = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(path) = env("POLYGO_MOCK_DUMP") {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            writeln!(f, "{user}\n-----")?;
        }
        if env("POLYGO_MOCK_MALFORMED").is_some()
            || (call == 1 && env("POLYGO_MOCK_MALFORMED_ONCE").is_some())
        {
            return Ok("I'm sorry, I cannot help with that request.".into());
        }
        let panic_after: Option<usize> =
            env("POLYGO_MOCK_PANIC_AFTER").and_then(|v| v.parse().ok());
        let log = env("POLYGO_MOCK_LOG");
        let drop_key = env("POLYGO_MOCK_DROP_KEY");
        let echo_key = env("POLYGO_MOCK_ECHO_KEY");
        let break_key = env("POLYGO_MOCK_BREAK_PLACEHOLDER").or_else(|| {
            (call == 1)
                .then(|| env("POLYGO_MOCK_BREAK_PLACEHOLDER_ONCE"))
                .flatten()
        });
        let fragment_key = env("POLYGO_MOCK_FRAGMENT").or_else(|| {
            (call == 1)
                .then(|| env("POLYGO_MOCK_FRAGMENT_ONCE"))
                .flatten()
        });
        let ignore_glossary = env("POLYGO_MOCK_IGNORE_GLOSSARY").is_some();

        let mut items = Vec::new();
        for (key, source) in parse_prompt(user) {
            let n = STRINGS.fetch_add(1, Ordering::SeqCst) + 1;
            if panic_after.is_some_and(|limit| n > limit) {
                eprintln!("mock provider: simulated crash after {} strings", n - 1);
                std::process::exit(70);
            }
            if let Some(path) = &log {
                use std::io::Write as _;
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                writeln!(f, "{key}")?;
            }
            if drop_key.as_deref() == Some(key.as_str()) {
                continue;
            }
            let text = if echo_key.as_deref() == Some(key.as_str()) {
                source.clone()
            } else if break_key.as_deref() == Some(key.as_str()) {
                let broken = match source.find('%') {
                    Some(i) => {
                        let end = source[i + 1..]
                            .find(|c: char| !c.is_ascii_alphanumeric() && c != '@' && c != '$')
                            .map(|k| i + 1 + k)
                            .unwrap_or(source.len());
                        format!("{}{}", &source[..i], &source[end..])
                    }
                    None => source.clone(),
                };
                format!("⟦{}⟧ {broken}", ctx.target_locale)
            } else if fragment_key.as_deref() == Some(key.as_str()) {
                let last = source.split_whitespace().last().unwrap_or("");
                format!("ようこそ、こんにちは {last}")
            } else if call == 1 && env("POLYGO_MOCK_KEY_ECHO_ONCE").is_some() {
                key.clone()
            } else {
                let mut t = source.clone();
                for (term, tr) in &ctx.glossary {
                    t = t.replace(term.as_str(), if ignore_glossary { "???" } else { tr });
                }
                format!("⟦{}⟧ {t}", ctx.target_locale)
            };
            items.push(serde_json::json!({ "key": key, "translation": text }));
        }
        Ok(serde_json::json!({ "translations": items })
            .to_string()
            .into())
    }
}

/// Pull `(key, source)` pairs back out of the user prompt built by `user_prompt`.
fn parse_prompt(user: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut key: Option<String> = None;
    for line in user.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            key = rest.split_once("key: ").map(|(_, k)| k.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("source") {
            // `source (English (en)): text` or `source: text`
            let src = rest
                .split_once("): ")
                .or_else(|| rest.split_once(": "))
                .map(|(_, v)| v.to_string());
            if let (Some(k), Some(src)) = (key.take(), src) {
                out.push((k, src));
            }
        }
    }
    out
}
