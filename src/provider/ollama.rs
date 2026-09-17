//! Ollama `/api/chat` with structured (JSON-schema) output and thinking disabled.

use super::{Ctx, Provider, post_json, response_schema};
use anyhow::{Context as _, Result};

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
}

impl Provider for Ollama {
    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, system: &str, user: &str, _ctx: &Ctx) -> Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "think": false,
            "format": response_schema(),
            "options": { "temperature": 0.2 },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ]
        });
        if std::env::var("POLYGO_DEBUG_PROMPT").is_ok() {
            eprintln!("--- system ---\n{system}\n--- user ---\n{user}");
        }
        let resp =
            post_json(&format!("{}/api/chat", self.base_url), &[], &body).with_context(|| {
                format!(
                    "ollama ({}) — is `ollama serve` running and `{}` pulled?",
                    self.base_url, self.model
                )
            })?;
        let content = resp["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if std::env::var("POLYGO_DEBUG_PROMPT").is_ok() {
            eprintln!("--- response ---\n{content}");
        }
        Ok(content)
    }
}
