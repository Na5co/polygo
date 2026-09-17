//! `polygo doctor` — is this project ready to translate? Config, files, locales,
//! provider reachability and model availability, each with the fix spelled out.

use crate::config::{Config, Format};
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    /// What to run or change when `ok` is false.
    pub fix: Option<String>,
}

fn pass(name: &str, detail: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        ok: true,
        detail: detail.into(),
        fix: None,
    }
}

fn fail(name: &str, detail: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        ok: false,
        detail: detail.into(),
        fix: Some(fix.into()),
    }
}

/// Run every check. Never panics; a missing config is itself a finding.
pub fn run(root: &Path) -> Vec<Check> {
    let mut out = Vec::new();
    let cfg = match Config::load(root) {
        Ok(c) => c,
        Err(e) => {
            out.push(fail("config", format!("{e:#}"), "polygo init"));
            return out;
        }
    };
    out.push(pass(
        "config",
        format!(
            "{} · source {} · targets [{}]",
            crate::config::FILE_NAME,
            cfg.source_locale,
            cfg.target_locales.join(", ")
        ),
    ));
    if cfg.target_locales.is_empty() {
        out.push(fail(
            "locales",
            "no target locales",
            "set target_locales = [\"de\", \"fr\"] in polygo.toml",
        ));
    }

    // Files parse and contain something to translate.
    match crate::project::load_units(root, &cfg) {
        Ok(units) => {
            let plural = units
                .iter()
                .filter(|u| crate::core::split_plural(&u.key).is_some())
                .count();
            out.push(pass(
                "files",
                format!(
                    "{} file(s) · {} unit(s){}",
                    cfg.files.len(),
                    units.len(),
                    if plural > 0 {
                        format!(" ({plural} plural forms)")
                    } else {
                        String::new()
                    }
                ),
            ));
            if units.is_empty() {
                out.push(fail(
                    "units",
                    "nothing translatable was found in the configured files",
                    "check `path` under [[files]] in polygo.toml",
                ));
            }
        }
        Err(e) => out.push(fail(
            "files",
            format!("{e:#}"),
            "fix the path/format under [[files]] in polygo.toml",
        )),
    }
    for f in &cfg.files {
        if f.format != Format::Xcstrings && f.locale_path.is_none() {
            out.push(fail(
                "locale_path",
                format!(
                    "{}: per-locale format without locale_path",
                    f.path.display()
                ),
                "add locale_path = \"…/{locale}/…\" to that [[files]] entry",
            ));
        }
    }
    if let Some(g) = &cfg.glossary {
        match crate::glossary::load(root, &cfg) {
            Ok(_) => out.push(pass("glossary", g.display().to_string())),
            Err(e) => out.push(fail("glossary", format!("{e:#}"), "fix glossary.toml")),
        }
    }

    // Provider.
    let model = cfg.model_name();
    match cfg.provider.kind.as_str() {
        "mock" => out.push(pass("provider", "mock (deterministic, offline)")),
        "ollama" => out.extend(check_ollama(&cfg, &model)),
        "openai" | "anthropic" => {
            let kind = cfg.provider.kind.as_str();
            let key_var = if kind == "openai" {
                "OPENAI_API_KEY"
            } else {
                "ANTHROPIC_API_KEY"
            };
            let has_key = std::env::var("POLYGO_API_KEY").is_ok() || std::env::var(key_var).is_ok();
            if has_key || (kind == "openai" && cfg.provider.base_url.is_some()) {
                out.push(pass(
                    "provider",
                    format!(
                        "{kind} · {model} · {}",
                        cfg.provider
                            .base_url
                            .as_deref()
                            .unwrap_or("default endpoint")
                    ),
                ));
            } else {
                out.push(fail(
                    "provider",
                    format!("{kind}: no API key in the environment"),
                    format!("export {key_var}=… (or POLYGO_API_KEY)"),
                ));
            }
        }
        other => out.push(fail(
            "provider",
            format!("unknown provider kind `{other}`"),
            "use kind = \"ollama\" | \"openai\" | \"anthropic\"",
        )),
    }
    out
}

fn check_ollama(cfg: &Config, model: &str) -> Vec<Check> {
    let base =
        crate::provider::ollama::Ollama::new(cfg.provider.base_url.clone(), model).base_url();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .new_agent();
    let tags = agent
        .get(format!("{base}/api/tags"))
        .call()
        .and_then(|mut r| r.body_mut().read_json::<serde_json::Value>());
    match tags {
        Err(e) => vec![fail(
            "ollama",
            format!("not reachable at {base} ({e})"),
            "install from https://ollama.com and run `ollama serve` (or set provider.base_url)",
        )],
        Ok(v) => {
            let names: Vec<String> = v["models"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m["name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let want = if model.contains(':') {
                model.to_string()
            } else {
                format!("{model}:latest")
            };
            let mut out = vec![pass("ollama", format!("reachable at {base}"))];
            if names.iter().any(|n| n == &want || n == model) {
                out.push(pass("model", format!("{model} is pulled")));
            } else {
                out.push(fail(
                    "model",
                    format!(
                        "{model} is not pulled (have: {})",
                        if names.is_empty() {
                            "nothing".to_string()
                        } else {
                            names.join(", ")
                        }
                    ),
                    format!("ollama pull {model}"),
                ));
            }
            out
        }
    }
}

/// Print the report; returns whether everything passed.
pub fn print(checks: &[Check], json: bool) -> Result<bool> {
    let ok = checks.iter().all(|c| c.ok);
    if json {
        println!("{}", serde_json::to_string_pretty(checks)?);
        return Ok(ok);
    }
    for c in checks {
        println!(
            "{} {:<11} {}",
            if c.ok { "ok  " } else { "FAIL" },
            c.name,
            c.detail
        );
        if let Some(fix) = &c.fix {
            println!("     fix:        {fix}");
        }
    }
    println!(
        "{}",
        if ok {
            "ready — run `polygo translate`"
        } else {
            "not ready — fix the items above"
        }
    );
    Ok(ok)
}
