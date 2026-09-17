//! Translation memory: every translation polygo writes or a human confirms, shared
//! across projects at `~/.config/polygo/memory.toml` (or `$POLYGO_CONFIG_DIR`).
//!
//! - Human-made entries (hand-edited files, `polygo review` approvals) are reused
//!   verbatim for identical source strings: no model call.
//! - Model-made entries are never reused blindly; they become few-shot examples.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    /// `human` or the model name.
    pub by: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Memory {
    /// locale → source text → entry
    #[serde(default)]
    pub locales: BTreeMap<String, BTreeMap<String, Entry>>,
    #[serde(skip)]
    dirty: bool,
}

pub fn path() -> PathBuf {
    crate::models::config_dir().join("memory.toml")
}

impl Memory {
    pub fn load() -> Memory {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let p = path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, toml::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", p.display()))?;
        Ok(())
    }

    /// Remember a translation. Human entries overwrite model ones, never the reverse.
    pub fn learn(&mut self, locale: &str, source: &str, text: &str, by: &str) {
        let source = source.trim();
        let text = text.trim();
        if source.is_empty() || text.is_empty() || source == text {
            return;
        }
        let map = self.locales.entry(locale.to_string()).or_default();
        let replace = match map.get(source) {
            None => true,
            Some(e) => by == "human" || (e.by != "human" && e.text != text),
        };
        if replace {
            map.insert(
                source.to_string(),
                Entry {
                    text: text.to_string(),
                    by: by.to_string(),
                },
            );
            self.dirty = true;
        }
    }

    /// A human-confirmed translation for exactly this source, if any.
    pub fn recall(&self, locale: &str, source: &str) -> Option<&str> {
        self.locales
            .get(locale)?
            .get(source.trim())
            .filter(|e| e.by == "human")
            .map(|e| e.text.as_str())
    }

    /// Every remembered pair for a locale, as few-shot candidates.
    pub fn examples(&self, locale: &str) -> Vec<(String, String)> {
        self.locales
            .get(locale)
            .map(|m| m.iter().map(|(s, e)| (s.clone(), e.text.clone())).collect())
            .unwrap_or_default()
    }

    pub fn counts(&self) -> Vec<(String, usize, usize)> {
        self.locales
            .iter()
            .map(|(l, m)| {
                let human = m.values().filter(|e| e.by == "human").count();
                (l.clone(), human, m.len() - human)
            })
            .collect()
    }

    pub fn forget(&mut self, locale: Option<&str>) -> usize {
        let n = match locale {
            Some(l) => self.locales.remove(l).map(|m| m.len()).unwrap_or(0),
            None => {
                let n = self.locales.values().map(BTreeMap::len).sum();
                self.locales.clear();
                n
            }
        };
        self.dirty = n > 0;
        n
    }
}
