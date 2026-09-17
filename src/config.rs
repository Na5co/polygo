//! `polygo.toml` — project configuration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "polygo.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub source_locale: String,
    pub target_locales: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileSpec>,
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub glossary: Option<PathBuf>,
    /// Strings per provider call.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Parallel provider calls.
    #[serde(default = "default_jobs")]
    pub jobs: usize,
    /// `check` warns when a translation is longer than this multiple of the source.
    #[serde(default = "default_length_ratio")]
    pub length_ratio: f64,
    /// Attach code-usage context and similar translations to prompts.
    #[serde(default = "default_true")]
    pub context: bool,
    /// Approximate token budget for context per string.
    #[serde(default = "default_context_tokens")]
    pub context_tokens: usize,
}

fn default_true() -> bool {
    true
}

fn default_context_tokens() -> usize {
    600
}

fn default_length_ratio() -> f64 {
    2.5
}

fn default_batch_size() -> usize {
    20
}

fn default_jobs() -> usize {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Xcstrings,
    Android,
    Json,
    Arb,
    Po,
    Resx,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSpec {
    pub format: Format,
    /// Source-language file (for `xcstrings` this holds every locale).
    pub path: PathBuf,
    /// For per-locale formats: path template with `{locale}`, e.g.
    /// `res/values-{locale}/strings.xml` or `locales/{locale}.json`.
    #[serde(default)]
    pub locale_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    /// `mock`, `ollama`, `openai`, `anthropic`.
    pub kind: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    /// Per-request HTTP timeout in seconds (local models can be slow on big batches).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    300
}

impl Default for Provider {
    fn default() -> Self {
        Provider {
            kind: "ollama".into(),
            model: Some("qwen3:8b".into()),
            base_url: None,
            timeout_secs: default_timeout(),
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Config> {
        let path = root.join(FILE_NAME);
        let text = std::fs::read_to_string(&path).with_context(|| {
            format!("no {} in {} (run `polygo init`)", FILE_NAME, root.display())
        })?;
        toml::from_str(&text).context("invalid polygo.toml")
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("config is serializable")
    }

    pub fn model_name(&self) -> String {
        self.provider
            .model
            .clone()
            .unwrap_or_else(|| "default".into())
    }
}
