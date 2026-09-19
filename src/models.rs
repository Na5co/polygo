//! `polygo models` / `polygo use`: pick a model, pull it, write the provider config.
//!
//! User-level state lives in `$POLYGO_CONFIG_DIR` or `~/.config/polygo/`:
//! - `defaults.toml`   : the `[provider]` that `polygo init` writes into new projects
//! - `credentials.toml`: API keys by provider kind (mode 0600); environment wins

use crate::config::{Config, Provider};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// An Ollama model with a short note on what it is good for.
pub struct Catalog {
    pub name: &'static str,
    pub size: &'static str,
    pub note: &'static str,
}

/// Sizes are the Ollama library's download sizes, rounded.
pub const CATALOG: &[Catalog] = &[
    Catalog {
        name: "qwen3:8b",
        size: "5.2 GB",
        note: "default · fast on any laptop · good for major languages, weak on small ones",
    },
    Catalog {
        name: "gemma4",
        size: "9.6 GB",
        note: "stronger multilingual output · recommended for Slavic, Baltic, Balkan and Asian targets",
    },
    Catalog {
        name: "qwen3:14b",
        size: "9.3 GB",
        note: "qwen3:8b's bigger sibling · noticeably better plurals and idiom",
    },
    Catalog {
        name: "gemma3:12b",
        size: "8.1 GB",
        note: "solid multilingual alternative to gemma4 if that one is not available",
    },
    Catalog {
        name: "qwen3:30b-a3b",
        size: "19 GB",
        note: "mixture-of-experts: 30B quality at ~3B speed · needs 24 GB+ RAM",
    },
    Catalog {
        name: "gemma3:27b",
        size: "17 GB",
        note: "best local quality here · slow without a big GPU / 32 GB Mac",
    },
];

pub fn config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("POLYGO_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME")
        && !x.is_empty()
    {
        return PathBuf::from(x).join("polygo");
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("polygo")
}

// ---- defaults for `init` -------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Defaults {
    #[serde(default)]
    provider: Option<Provider>,
}

/// The provider `polygo init` should write, if the user set one with `use --global`.
pub fn default_provider() -> Option<Provider> {
    let text = std::fs::read_to_string(config_dir().join("defaults.toml")).ok()?;
    toml::from_str::<Defaults>(&text).ok()?.provider
}

fn save_default_provider(p: &Provider) -> Result<PathBuf> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("defaults.toml");
    let d = Defaults {
        provider: Some(p.clone()),
    };
    std::fs::write(&path, toml::to_string_pretty(&d)?)?;
    Ok(path)
}

// ---- credentials ----------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Credentials {
    #[serde(default)]
    keys: std::collections::BTreeMap<String, String>,
}

/// Stored API key for a provider kind (`openai`, `anthropic`), if any.
pub fn stored_api_key(kind: &str) -> Option<String> {
    let text = std::fs::read_to_string(config_dir().join("credentials.toml")).ok()?;
    toml::from_str::<Credentials>(&text)
        .ok()?
        .keys
        .remove(kind)
        .filter(|k| !k.is_empty())
}

fn store_api_key(kind: &str, key: &str) -> Result<PathBuf> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("credentials.toml");
    let mut creds: Credentials = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default();
    creds.keys.insert(kind.to_string(), key.to_string());
    let text = toml::to_string_pretty(&creds)?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&path)?;
    f.write_all(text.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

// ---- `polygo use` ---------------------------------------------------------------------

pub struct UseArgs {
    /// `gemma4`, `ollama/gemma4`, `openai/gpt-4o-mini`, `anthropic/claude-sonnet-5`;
    /// a bare name is always an Ollama model.
    pub spec: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    /// Also make this the default for future `polygo init`.
    pub global: bool,
    /// Don't pull a missing Ollama model.
    pub no_pull: bool,
}

/// Parse `spec` into a provider config: `<kind>/<model>` or a bare Ollama model.
pub fn parse_spec(spec: &str, base_url: Option<String>) -> Result<Provider> {
    let (kind, model) = match spec.split_once('/') {
        Some((k, m)) if matches!(k, "ollama" | "openai" | "anthropic" | "mock") => {
            (k.to_string(), m.to_string())
        }
        Some(_) => bail!(
            "unknown provider in `{spec}`: use `ollama/<model>`, `openai/<model>`, `anthropic/<model>`, or a bare Ollama model name"
        ),
        None => ("ollama".to_string(), spec.to_string()),
    };
    if model.is_empty() {
        bail!("missing model name in `{spec}`");
    }
    Ok(Provider {
        kind,
        model: Some(model),
        base_url,
        timeout_secs: Provider::default().timeout_secs,
    })
}

/// Apply `polygo use`: pull if needed, store the key, write polygo.toml (when present)
/// and/or the global default. Returns human-readable lines describing what happened.
pub fn use_model(root: &Path, args: &UseArgs, out: &mut dyn Write) -> Result<Provider> {
    let provider = parse_spec(&args.spec, args.base_url.clone())?;
    let model = provider.model.clone().unwrap_or_default();

    match provider.kind.as_str() {
        "ollama" => {
            let base =
                crate::provider::ollama::Ollama::new(provider.base_url.clone(), &model).base_url();
            if !ollama_reachable(&base) {
                bail!(
                    "Ollama is not running at {base}.\n  install: https://ollama.com/download  (macOS: `brew install ollama`)\n  then:    `ollama serve` (the desktop app does this for you)\n  or pick an API model instead: `polygo use openai/gpt-4o-mini --api-key …`"
                );
            }
            if ollama_has(&base, &model) {
                writeln!(out, "{model} is already pulled")?;
            } else if args.no_pull {
                bail!("{model} is not pulled (run without --no-pull, or `ollama pull {model}`)");
            } else {
                let size = CATALOG
                    .iter()
                    .find(|c| c.name == model)
                    .map(|c| format!(" (~{})", c.size))
                    .unwrap_or_default();
                writeln!(out, "pulling {model}{size} …")?;
                pull(&base, &model, out)?;
                if !ollama_has(&base, &model) {
                    bail!("pull finished but {model} is still not listed by Ollama");
                }
                writeln!(out, "pulled {model}")?;
            }
        }
        "openai" | "anthropic" => {
            if let Some(key) = &args.api_key {
                let path = store_api_key(&provider.kind, key)?;
                writeln!(
                    out,
                    "stored API key for {} in {}",
                    provider.kind,
                    path.display()
                )?;
            } else {
                let env_var = if provider.kind == "openai" {
                    "OPENAI_API_KEY"
                } else {
                    "ANTHROPIC_API_KEY"
                };
                let have = std::env::var("POLYGO_API_KEY").is_ok()
                    || std::env::var(env_var).is_ok()
                    || stored_api_key(&provider.kind).is_some();
                let own_endpoint = provider.kind == "openai" && provider.base_url.is_some();
                if !(have || own_endpoint) {
                    writeln!(
                        out,
                        "note: no API key found: pass --api-key, or export {env_var}"
                    )?;
                }
            }
        }
        "mock" => {}
        other => bail!("unknown provider kind {other}"),
    }

    let path = root.join(crate::config::FILE_NAME);
    let mut wrote_project = false;
    if path.exists() {
        Config::load(root)?; // fail early on an unreadable file
        Config::edit(root, |doc| {
            if !doc.contains_table("provider") {
                doc["provider"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            let t = &mut doc["provider"];
            t["kind"] = toml_edit::value(provider.kind.as_str());
            match &provider.model {
                Some(m) => t["model"] = toml_edit::value(m.as_str()),
                None => {
                    t.as_table_mut().map(|t| t.remove("model"));
                }
            }
            match &provider.base_url {
                Some(b) => t["base_url"] = toml_edit::value(b.as_str()),
                None => {
                    t.as_table_mut().map(|t| t.remove("base_url"));
                }
            }
            t["timeout_secs"] = toml_edit::value(provider.timeout_secs as i64);
        })?;
        writeln!(
            out,
            "{}: provider = {} · model = {}{}",
            path.display(),
            provider.kind,
            model,
            provider
                .base_url
                .as_deref()
                .map(|b| format!(" · base_url = {b}"))
                .unwrap_or_default()
        )?;
        wrote_project = true;
    }
    if args.global || !wrote_project {
        let p = save_default_provider(&provider)?;
        writeln!(
            out,
            "{}: default for new projects = {}/{}",
            p.display(),
            provider.kind,
            model
        )?;
    }
    Ok(provider)
}

// ---- Ollama HTTP ----------------------------------------------------------------------

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(secs)))
        .build()
        .new_agent()
}

fn ollama_reachable(base: &str) -> bool {
    agent(3).get(format!("{base}/api/tags")).call().is_ok()
}

/// Names Ollama has pulled (`gemma4:latest`, `qwen3:8b`, …).
pub fn ollama_models(base: &str) -> Vec<String> {
    agent(3)
        .get(format!("{base}/api/tags"))
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_json::<serde_json::Value>().ok())
        .and_then(|v| {
            v["models"].as_array().map(|a| {
                a.iter()
                    .filter_map(|m| m["name"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

pub fn ollama_has(base: &str, model: &str) -> bool {
    let want = if model.contains(':') {
        model.to_string()
    } else {
        format!("{model}:latest")
    };
    ollama_models(base).iter().any(|n| *n == want || n == model)
}

/// `POST /api/pull` with streaming progress lines rendered as a single updating bar.
fn pull(base: &str, model: &str, out: &mut dyn Write) -> Result<()> {
    let resp = agent(6 * 3600)
        .post(format!("{base}/api/pull"))
        .send_json(serde_json::json!({ "model": model, "stream": true }))
        .with_context(|| format!("POST {base}/api/pull"))?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut last_status = String::new();
    let mut last_draw = std::time::Instant::now() - std::time::Duration::from_secs(1);
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line) else {
                continue;
            };
            if let Some(err) = v["error"].as_str() {
                bail!("ollama: {err}");
            }
            let status = v["status"].as_str().unwrap_or("").to_string();
            let (total, done) = (v["total"].as_f64(), v["completed"].as_f64());
            let finished = total.is_some_and(|t| done.is_some_and(|d| d >= t));
            if !finished
                && status == last_status
                && last_draw.elapsed() < std::time::Duration::from_millis(250)
            {
                continue;
            }
            last_draw = std::time::Instant::now();
            match (total, done) {
                (Some(t), Some(d)) if t > 0.0 => {
                    let pct = (d / t * 100.0).min(100.0);
                    let filled = (pct / 4.0) as usize;
                    write!(
                        out,
                        "\r  [{}{}] {pct:5.1}%  {:.1} / {:.1} GB  {status:<24}",
                        "#".repeat(filled),
                        " ".repeat(25 - filled),
                        d / 1e9,
                        t / 1e9
                    )?;
                }
                _ if status != last_status => {
                    write!(out, "\r  {status:<70}")?;
                }
                _ => {}
            }
            out.flush()?;
            last_status = status;
        }
    }
    writeln!(out)?;
    Ok(())
}

/// Lines for `polygo models`.
pub fn list(root: &Path, out: &mut dyn Write) -> Result<()> {
    let current = Config::load(root).ok().map(|c| c.provider);
    let base =
        crate::provider::ollama::Ollama::new(current.as_ref().and_then(|p| p.base_url.clone()), "")
            .base_url();
    let reachable = ollama_reachable(&base);
    let pulled = if reachable {
        ollama_models(&base)
    } else {
        vec![]
    };
    let is_pulled = |m: &str| {
        let want = if m.contains(':') {
            m.to_string()
        } else {
            format!("{m}:latest")
        };
        pulled.iter().any(|n| *n == want || n == m)
    };
    let active = |kind: &str, m: &str| {
        current
            .as_ref()
            .is_some_and(|p| p.kind == kind && p.model.as_deref() == Some(m))
    };
    writeln!(
        out,
        "Local (Ollama{}):",
        if reachable { "" } else { ": not running" }
    )?;
    for c in CATALOG {
        let mark = if active("ollama", c.name) {
            "*"
        } else if is_pulled(c.name) {
            "+"
        } else {
            " "
        };
        writeln!(out, "  {mark} {:<15} {:>7}   {}", c.name, c.size, c.note)?;
    }
    for p in pulled.iter().filter(|p| {
        !CATALOG
            .iter()
            .any(|c| **p == c.name || **p == format!("{}:latest", c.name))
    }) {
        let name = p.trim_end_matches(":latest");
        let mark = if active("ollama", name) || active("ollama", p) {
            "*"
        } else {
            "+"
        };
        writeln!(
            out,
            "  {mark} {name:<15}           (pulled, not in the catalog)"
        )?;
    }
    writeln!(out, "\nAPI (bring your own key):")?;
    for (spec, note) in [
        (
            "openai/gpt-4o-mini",
            "OPENAI_API_KEY · cheap, strong on every language",
        ),
        (
            "anthropic/claude-sonnet-5",
            "ANTHROPIC_API_KEY · best quality",
        ),
        (
            "openai/<model> --base-url URL",
            "any OpenAI-compatible server: llama.cpp, vLLM, LM Studio, OpenRouter, Groq",
        ),
    ] {
        let (kind, m) = spec.split_once('/').unwrap_or(("openai", spec));
        let mark = if active(kind, m) { "*" } else { " " };
        writeln!(out, "  {mark} {spec:<28} {note}")?;
    }
    writeln!(
        out,
        "\n* active   + pulled\n`polygo use <model>` pulls it if needed and writes polygo.toml; add --global for new projects."
    )?;
    Ok(())
}
