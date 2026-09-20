//! Ollama `/api/chat` with structured (JSON-schema) output and thinking disabled.

use super::{Ctx, HttpError, Provider, Reply, Usage, fatal, post_json, response_schema};
use anyhow::Result;

pub struct Ollama {
    base_url: String,
    model: String,
}

impl Ollama {
    pub fn new(base_url: Option<String>, model: &str) -> Self {
        let base = base_url
            .or_else(|| {
                std::env::var("OLLAMA_HOST").ok().map(|h| {
                    if h.starts_with("http") {
                        h
                    } else {
                        format!("http://{h}")
                    }
                })
            })
            .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
        Ollama {
            base_url: base.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }
}

impl Ollama {
    pub fn base_url(&self) -> String {
        self.base_url.clone()
    }

    /// Say what to do, not what the socket said.
    fn explain(&self, e: HttpError) -> anyhow::Error {
        match &e {
            HttpError::Unreachable(_) => fatal(
                format!("ollama is not running at {}", self.base_url),
                if self.base_url.starts_with("http://127.0.0.1")
                    || self.base_url.contains("localhost")
                {
                    "start it with `ollama serve` (or open the Ollama app); not installed? https://ollama.com or `brew install ollama` · `polygo doctor` checks everything".to_string()
                } else {
                    format!(
                        "check that Ollama is up at {} (base_url in polygo.toml / OLLAMA_HOST) · `polygo doctor` checks everything",
                        self.base_url
                    )
                },
            ),
            HttpError::Status { code: 404, .. } => fatal(
                format!(
                    "ollama has no model `{}`{}",
                    self.model,
                    e.server_says()
                        .map(|m| format!(" ({m})"))
                        .unwrap_or_default()
                ),
                format!(
                    "`polygo use {}` pulls it · `polygo models` lists the options",
                    self.model
                ),
            ),
            _ => {
                anyhow::Error::new(e).context(format!("ollama {} ({})", self.model, self.base_url))
            }
        }
    }
}

impl Provider for Ollama {
    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, system: &str, user: &str, ctx: &Ctx) -> Result<Reply> {
        self.complete_json(system, user, ctx, &response_schema())
    }

    fn complete_json(
        &self,
        system: &str,
        user: &str,
        _ctx: &Ctx,
        schema: &serde_json::Value,
    ) -> Result<Reply> {
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "think": false,
            "format": schema,
            // Ollama's default context is 4k; batches with code context need more.
            "options": { "temperature": 0.2, "num_ctx": 16384 },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ]
        });
        if std::env::var("POLYGO_DEBUG_PROMPT").is_ok() {
            eprintln!("--- system ---\n{system}\n--- user ---\n{user}");
        }
        let resp = post_json(&format!("{}/api/chat", self.base_url), &[], &body)
            .map_err(|e| self.explain(e))?;
        let content = resp["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if std::env::var("POLYGO_DEBUG_PROMPT").is_ok() {
            eprintln!("--- response ---\n{content}");
        }
        let usage = resp["prompt_eval_count"]
            .as_u64()
            .map(|input_tokens| Usage {
                input_tokens,
                output_tokens: resp["eval_count"].as_u64().unwrap_or(0),
            });
        Ok(Reply {
            text: content,
            usage,
        })
    }
}
