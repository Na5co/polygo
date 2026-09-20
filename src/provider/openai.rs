//! Any OpenAI-compatible `/v1/chat/completions` endpoint (OpenAI, llama.cpp server,
//! vLLM, LM Studio, OpenRouter, ...).

use super::{Ctx, HttpError, Provider, Reply, Usage, fatal, post_json, response_schema};
use anyhow::Result;

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

impl OpenAiCompatible {
    fn explain(&self, e: HttpError) -> anyhow::Error {
        let said = e
            .server_says()
            .map(|m| format!(" ({m})"))
            .unwrap_or_default();
        let key_fix = format!(
            "export OPENAI_API_KEY=… (or POLYGO_API_KEY), or `polygo use openai/{} --api-key …` to store one",
            self.model
        );
        match &e {
            HttpError::Unreachable(_) => fatal(
                format!("nothing is answering at {}", self.base_url),
                if self.base_url.contains("127.0.0.1") || self.base_url.contains("localhost") {
                    "start the local server (llama.cpp, vLLM, LM Studio…) or fix base_url in polygo.toml · `polygo doctor` checks everything".to_string()
                } else {
                    "check the network and base_url in polygo.toml · `polygo doctor` checks everything".to_string()
                },
            ),
            HttpError::Status {
                code: 401 | 403, ..
            } if self.api_key.is_none() => fatal(
                format!("{} wants an API key and none is set", self.base_url),
                key_fix,
            ),
            HttpError::Status {
                code: 401 | 403, ..
            } => fatal(
                format!("{} rejected the API key{said}", self.base_url),
                key_fix,
            ),
            HttpError::Status { code: 404, .. } => fatal(
                format!("{} has no model `{}`{said}", self.base_url, self.model),
                "check the model name (`polygo models` lists common ones) or base_url in polygo.toml",
            ),
            _ => anyhow::Error::new(e)
                .context(format!("openai-compatible endpoint {}", self.base_url)),
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
        .map_err(|e| self.explain(e))?;
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
