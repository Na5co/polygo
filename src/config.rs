//! `polygo.toml`: project configuration.

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
    /// Use the cross-project translation memory (~/.config/polygo/memory.toml).
    #[serde(default = "default_true")]
    pub memory: bool,
    /// `polygo extract` settings.
    #[serde(default, skip_serializing_if = "Extract::is_default")]
    pub extract: Extract,
}

/// `[extract]` in polygo.toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Extract {
    /// Paths to leave alone (gitignore-style globs, relative to the project root):
    /// `["src/admin*", "packages/internal/**"]`.
    #[serde(default)]
    pub ignore_paths: Vec<String>,
    /// Strings containing any of these (case-insensitive) are not extracted:
    /// brand names, internal jargon, `["Leafslip", "API key"]`.
    #[serde(default)]
    pub ignore: Vec<String>,
    /// Exact strings to skip.
    #[serde(default)]
    pub ignore_exact: Vec<String>,
}

impl Extract {
    fn is_default(&self) -> bool {
        *self == Extract::default()
    }
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

    /// Edit `polygo.toml` in place: comments, key order and formatting the user put in
    /// the file survive, only what `f` touches changes.
    pub fn edit(root: &Path, f: impl FnOnce(&mut toml_edit::DocumentMut)) -> Result<()> {
        let path = root.join(FILE_NAME);
        let text = std::fs::read_to_string(&path).with_context(|| {
            format!("no {} in {} (run `polygo init`)", FILE_NAME, root.display())
        })?;
        let mut doc: toml_edit::DocumentMut = text.parse().context("invalid polygo.toml")?;
        f(&mut doc);
        std::fs::write(&path, doc.to_string())
            .with_context(|| format!("writing {}", path.display()))
    }

    /// Set `target_locales` in a parsed document: one line, the way `init` writes it,
    /// keeping any comment the user left on that line.
    pub fn set_target_locales(doc: &mut toml_edit::DocumentMut, locales: &[String]) {
        let arr = match doc.get_mut("target_locales").and_then(|i| i.as_array_mut()) {
            Some(existing) => {
                existing.clear();
                existing
            }
            None => {
                doc["target_locales"] = toml_edit::value(toml_edit::Array::new());
                doc["target_locales"].as_array_mut().expect("just set")
            }
        };
        for l in locales {
            arr.push(l.as_str());
        }
        arr.fmt();
    }

    pub fn model_name(&self) -> String {
        self.provider
            .model
            .clone()
            .unwrap_or_else(|| "default".into())
    }
}
