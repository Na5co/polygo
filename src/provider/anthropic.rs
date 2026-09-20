//! Anthropic Messages API.

use super::{Ctx, HttpError, Provider, Reply, Usage, fatal, post_json};
use anyhow::Result;

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

impl Anthropic {
    fn key_fix(&self) -> String {
        format!(
            "export ANTHROPIC_API_KEY=… (or POLYGO_API_KEY), or `polygo use anthropic/{} --api-key …` to store one",
            self.model
        )
    }

    fn explain(&self, e: HttpError) -> anyhow::Error {
        let said = e
            .server_says()
            .map(|m| format!(" ({m})"))
            .unwrap_or_default();
        match &e {
            HttpError::Unreachable(_) => fatal(
                format!("nothing is answering at {}", self.base_url),
                "check the network and base_url in polygo.toml · `polygo doctor` checks everything",
            ),
            HttpError::Status {
                code: 401 | 403, ..
            } => fatal(
                format!("{} rejected the API key{said}", self.base_url),
                self.key_fix(),
            ),
            HttpError::Status { code: 404, .. } => fatal(
                format!("{} has no model `{}`{said}", self.base_url, self.model),
                "check the model name (`polygo models` lists common ones)",
            ),
            _ => anyhow::Error::new(e).context("anthropic messages API"),
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
            return Err(fatal(
                "anthropic needs an API key and none is set",
                self.key_fix(),
            ));
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
        .map_err(|e| self.explain(e))?;
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
