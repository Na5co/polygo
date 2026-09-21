//! `glossary.toml`: brand terms and required translations.
//!
//! ```toml
//! do_not_translate = ["Polygo", "GitHub"]
//!
//! [terms.de]
//! "Sign in" = "Anmelden"
//! ```

use crate::config::Config;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
pub struct Glossary {
    #[serde(default)]
    pub do_not_translate: Vec<String>,
    /// locale → source term → translation
    #[serde(default)]
    pub terms: BTreeMap<String, BTreeMap<String, String>>,
}

impl Glossary {
    pub fn terms_for(&self, locale: &str) -> Vec<(String, String)> {
        self.terms
            .get(locale)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    /// One message per glossary rule the translation breaks.
    pub fn violations(&self, locale: &str, source: &str, translation: &str) -> Vec<String> {
        let src = source.to_lowercase();
        let mut out = Vec::new();
        for term in &self.do_not_translate {
            if src.contains(&term.to_lowercase()) && !translation.contains(term.as_str()) {
                out.push(format!("`{term}` must stay untranslated"));
            }
        }
        if let Some(map) = self.terms.get(locale) {
            for (term, required) in map {
                if src.contains(&term.to_lowercase())
                    && !translation
                        .to_lowercase()
                        .contains(&required.to_lowercase())
                {
                    out.push(format!("`{term}` must be rendered as `{required}`"));
                }
            }
        }
        out
    }

    /// True when every glossary term present in `source` has its required translation
    /// in `translation`, and every do-not-translate term survived verbatim.
    pub fn satisfied(&self, locale: &str, source: &str, translation: &str) -> bool {
        let src = source.to_lowercase();
        for term in &self.do_not_translate {
            if src.contains(&term.to_lowercase()) && !translation.contains(term.as_str()) {
                return false;
            }
        }
        if let Some(map) = self.terms.get(locale) {
            for (term, required) in map {
                if src.contains(&term.to_lowercase())
                    && !translation
                        .to_lowercase()
                        .contains(&required.to_lowercase())
                {
                    return false;
                }
            }
        }
        true
    }
}

pub fn load(root: &Path, cfg: &Config) -> Result<Glossary> {
    let path = match &cfg.glossary {
        Some(p) => root.join(p),
        None => {
            let default = root.join("glossary.toml");
            if !default.exists() {
                return Ok(Glossary::default());
            }
            default
        }
    };
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("invalid glossary {}", path.display()))
}
