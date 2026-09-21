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
    /// `[keys]`: which keys polygo leaves alone.
    #[serde(default, skip_serializing_if = "Keys::is_default")]
    pub keys: Keys,
}

/// `[keys]` in polygo.toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Keys {
    /// Globs over the key: `["debug.*", "internal_*", "legal.terms"]`. Matching keys are
    /// never translated, counted or checked, like `polygo:skip` in a comment, for formats
    /// that have no comment field (i18next JSON) or when there are many. `*` also
    /// crosses dots; with several `[[files]]`, `path/to/file.json:key` targets one file.
    #[serde(default)]
    pub skip: Vec<String>,
}

impl Keys {
    fn is_default(&self) -> bool {
        *self == Keys::default()
    }
}

/// Compiled `[keys] skip`.
#[derive(Debug, Clone)]
pub struct KeySkip {
    set: globset::GlobSet,
}

impl KeySkip {
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// `key` is the key within `file` (plural suffixes `#plural.few` / `#var` ignored).
    pub fn matches(&self, file: &Path, key: &str) -> bool {
        if self.set.is_empty() {
            return false;
        }
        let base = key.split('#').next().unwrap_or(key);
        self.set.is_match(base)
            || self.set.is_match(format!(
                "{}:{base}",
                file.display().to_string().replace('\\', "/")
            ))
    }
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
    /// Apple `.strings` (legacy iOS/macOS, `en.lproj/Localizable.strings`).
    Strings,
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
        let cfg: Config = match toml::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                // `target_locale = [...]` fails as "missing field target_locales"; say why.
                let mut msg = e.to_string();
                if let Some(missing) = msg
                    .split("missing field `")
                    .nth(1)
                    .and_then(|r| r.split('`').next())
                    && let Ok(table) = text.parse::<toml::Table>()
                    && let Some(near) = table.keys().find(|k| close(k, missing))
                {
                    msg = format!("{msg}\n  (found `{near}`: did you mean `{missing}`?)");
                }
                return Err(anyhow::anyhow!("{msg}").context("invalid polygo.toml"));
            }
        };
        for w in unknown_keys(&text) {
            eprintln!("polygo.toml: {w}");
        }
        Ok(cfg)
    }

    pub fn to_toml(&self) -> String {
        let pretty = toml::to_string_pretty(self).expect("config is serializable");
        // `target_locales = ["de", "fr"]` on one line, not one line per locale.
        let mut doc: toml_edit::DocumentMut = pretty.parse().expect("own output parses");
        Self::set_target_locales(&mut doc, &self.target_locales);
        doc.to_string()
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

    pub fn key_skip(&self) -> Result<KeySkip> {
        let mut b = globset::GlobSetBuilder::new();
        for g in &self.keys.skip {
            b.add(
                globset::Glob::new(g)
                    .with_context(|| format!("[keys] skip: bad glob {g:?} in polygo.toml"))?,
            );
        }
        Ok(KeySkip {
            set: b.build().context("[keys] skip")?,
        })
    }

    pub fn model_name(&self) -> String {
        self.provider
            .model
            .clone()
            .unwrap_or_else(|| "default".into())
    }
}

// ---- unknown keys ------------------------------------------------------------------------
//
// serde ignores keys it does not know, which is right for forward compatibility and wrong
// for `batch_szie = 5`: the setting silently does nothing. So: warn, with a suggestion.

const ROOT_KEYS: &[&str] = &[
    "source_locale",
    "target_locales",
    "files",
    "provider",
    "glossary",
    "batch_size",
    "jobs",
    "length_ratio",
    "context",
    "context_tokens",
    "memory",
    "extract",
    "keys",
];
const FILE_KEYS: &[&str] = &["format", "path", "locale_path"];
const PROVIDER_KEYS: &[&str] = &["kind", "model", "base_url", "timeout_secs"];
const EXTRACT_KEYS: &[&str] = &["ignore_paths", "ignore", "ignore_exact"];
const KEYS_KEYS: &[&str] = &["skip"];

/// One warning per key polygo does not understand, e.g.
/// "unknown key `batch_szie` (did you mean `batch_size`?)".
pub fn unknown_keys(text: &str) -> Vec<String> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut check = |prefix: &str, t: &toml::Table, known: &[&str]| {
        for k in t.keys() {
            if known.contains(&k.as_str()) {
                continue;
            }
            let hint = known
                .iter()
                .find(|c| close(k, c))
                .map(|c| format!(" (did you mean `{c}`?)"))
                .unwrap_or_default();
            out.push(format!("unknown key `{prefix}{k}`{hint}"));
        }
    };
    check("", &table, ROOT_KEYS);
    if let Some(t) = table.get("provider").and_then(|v| v.as_table()) {
        check("provider.", t, PROVIDER_KEYS);
    }
    if let Some(t) = table.get("extract").and_then(|v| v.as_table()) {
        check("extract.", t, EXTRACT_KEYS);
    }
    if let Some(t) = table.get("keys").and_then(|v| v.as_table()) {
        check("keys.", t, KEYS_KEYS);
    }
    if let Some(files) = table.get("files").and_then(|v| v.as_array()) {
        for (i, f) in files.iter().enumerate() {
            if let Some(t) = f.as_table() {
                check(&format!("files[{i}]."), t, FILE_KEYS);
            }
        }
    }
    out
}

/// Typo distance: a couple of edits, or one being the other plus/minus a short suffix
/// (`target_locale` / `target_locales`, `context_token` / `context_tokens`).
fn close(a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    if (a.starts_with(&b) || b.starts_with(&a)) && a.len().abs_diff(b.len()) <= 2 {
        return true;
    }
    levenshtein(&a, &b) <= 2
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
