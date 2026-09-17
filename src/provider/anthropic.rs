//! Anthropic Messages API.

use super::{Ctx, Provider, Reply, Usage, post_json};
use anyhow::{Context as _, Result, bail};

pub struct Anthropic {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl Anthropic {
    pub fn new(base_url: Option<String>, model: &str, api_key: Option<String>) -> Self {
        Anthropic {
            base_url: base_url
                .unwrap_or_else(|| "https://api.anthropic.com".into())
                .trim_end_matches('/')
                .to_string(),
            model: model.to_string(),
            api_key,
        }
    }
}

impl Provider for Anthropic {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, system: &str, user: &str, _ctx: &Ctx) -> Result<Reply> {
        let Some(key) = &self.api_key else {
            bail!("anthropic provider needs ANTHROPIC_API_KEY (or POLYGO_API_KEY)");
        };
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "temperature": 0.2,
            "system": system,
            "messages": [ { "role": "user", "content": user } ]
        });
        let resp = post_json(
            &format!("{}/v1/messages", self.base_url),
            &[
                ("x-api-key", key.as_str()),
                ("anthropic-version", "2023-06-01"),
            ],
            &body,
        )
        .context("anthropic messages API")?;
        let text = resp["content"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        let usage = resp["usage"]["input_tokens"]
            .as_u64()
            .map(|input_tokens| Usage {
                input_tokens,
                output_tokens: resp["usage"]["output_tokens"].as_u64().unwrap_or(0),
            });
        Ok(Reply { text, usage })
    }
}
