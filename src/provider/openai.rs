//! Any OpenAI-compatible `/v1/chat/completions` endpoint (OpenAI, llama.cpp server,
//! vLLM, LM Studio, OpenRouter, ...).

use super::{Ctx, Provider, Reply, Usage, post_json, response_schema};
use anyhow::{Context as _, Result};

pub struct OpenAiCompatible {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiCompatible {
    pub fn new(base_url: Option<String>, model: &str, api_key: Option<String>) -> Self {
        OpenAiCompatible {
            base_url: base_url
                .unwrap_or_else(|| "https://api.openai.com/v1".into())
                .trim_end_matches('/')
                .to_string(),
            model: model.to_string(),
            api_key,
        }
    }
}

impl Provider for OpenAiCompatible {
    fn name(&self) -> &str {
        "openai"
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
            "temperature": 0.2,
            "response_format": { "type": "json_schema", "json_schema": { "name": "reply", "schema": schema } },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ]
        });
        let auth = self.api_key.as_ref().map(|k| format!("Bearer {k}"));
        let mut headers: Vec<(&str, &str)> = vec![];
        if let Some(a) = &auth {
            headers.push(("Authorization", a.as_str()));
        }
        let resp = post_json(
            &format!("{}/chat/completions", self.base_url),
            &headers,
            &body,
        )
        .with_context(|| format!("openai-compatible endpoint {}", self.base_url))?;
        let text = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let usage = resp["usage"]["prompt_tokens"]
            .as_u64()
            .map(|input_tokens| Usage {
                input_tokens,
                output_tokens: resp["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            });
        Ok(Reply { text, usage })
    }
}
