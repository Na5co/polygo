//! Deterministic, offline provider for tests: `⟦<locale>⟧ <source>`.

use super::{Ctx, Provider, Request, Translation};
use anyhow::{Result, bail};

#[derive(Debug, Default, Clone)]
pub struct Mock {
    /// If set, translating a batch containing this key fails (for resume/quarantine tests).
    pub fail_on_key: Option<String>,
}

impl Mock {
    /// Invert a mock translation back into `(locale, source)`.
    pub fn reverse(text: &str) -> Option<(String, String)> {
        let rest = text.strip_prefix('⟦')?;
        let (locale, src) = rest.split_once("⟧ ")?;
        Some((locale.to_string(), src.to_string()))
    }
}

impl Provider for Mock {
    fn name(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "mock-1"
    }

    fn translate(&self, batch: &[Request], ctx: &Ctx) -> Result<Vec<Translation>> {
        if let Some(bad) = &self.fail_on_key
            && batch.iter().any(|r| &r.key == bad)
        {
            bail!("mock provider: injected failure on key `{bad}`");
        }
        Ok(batch
            .iter()
            .map(|r| Translation {
                key: r.key.clone(),
                text: format!("⟦{}⟧ {}", ctx.target_locale, r.source),
            })
            .collect())
    }
}
