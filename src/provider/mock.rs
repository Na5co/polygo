//! Deterministic, offline provider for tests: `⟦<locale>⟧ <source>`.

use super::{Ctx, Provider, Request, Translation};
use anyhow::{Result, bail};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);

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
        // Test hooks: POLYGO_MOCK_PANIC_AFTER=N aborts the process on the (N+1)th request
        // (simulates a crash mid-run); POLYGO_MOCK_LOG=path appends every translated key.
        let panic_after: Option<usize> = std::env::var("POLYGO_MOCK_PANIC_AFTER")
            .ok()
            .and_then(|v| v.parse().ok());
        let log = std::env::var("POLYGO_MOCK_LOG").ok();
        let mut out = Vec::with_capacity(batch.len());
        for r in batch {
            let n = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
            if panic_after.is_some_and(|limit| n > limit) {
                eprintln!("mock provider: simulated crash after {} requests", n - 1);
                std::process::exit(70);
            }
            if let Some(path) = &log {
                use std::io::Write as _;
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                writeln!(f, "{}", r.key)?;
            }
            out.push(Translation {
                key: r.key.clone(),
                text: format!("⟦{}⟧ {}", ctx.target_locale, r.source),
            });
        }
        Ok(out)
    }
}
