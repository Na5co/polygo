//! Any OpenAI-compatible `/v1/chat/completions` endpoint (OpenAI, llama.cpp server,
//! vLLM, LM Studio, OpenRouter, ...).

use super::{
    Ctx, Provider, Request, Translation, parse_translations_json, post_json, response_schema,
    system_prompt, user_prompt,
};
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

    fn translate(&self, batch: &[Request], ctx: &Ctx) -> Result<Vec<Translation>> {
        if batch.is_empty() {
            return Ok(vec![]);
        }
        let body = serde_json::json!({
            "model": self.model,
            "temperature": 0.2,
            "response_format": { "type": "json_schema", "json_schema": { "name": "translations", "schema": response_schema() } },
            "messages": [
                { "role": "system", "content": system_prompt(ctx) },
                { "role": "user", "content": user_prompt(batch, ctx) }
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
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");
        let wanted: Vec<&str> = batch.iter().map(|r| r.key.as_str()).collect();
        parse_translations_json(content, &wanted)
    }
}
