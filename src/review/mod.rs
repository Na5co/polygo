//! `polygo review` — a tiny local web page for approving, editing or rejecting
//! translations. One embedded HTML file, no build step, served on 127.0.0.1 only;
//! requests whose Host header is not localhost are refused (DNS-rebinding guard).

use crate::config::Config;
use crate::lockfile::{Lock, State};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Response, Server};

const PAGE: &str = include_str!("page.html");

#[derive(Serialize)]
struct Item {
    locale: String,
    key: String,
    source: String,
    translation: Option<String>,
    suggestion: Option<String>,
    reason: Option<String>,
    comment: Option<String>,
    context: Option<String>,
    /// `needs-review` or `machine`.
    state: &'static str,
}

#[derive(Deserialize)]
struct Action {
    locale: String,
    key: String,
    #[serde(default)]
    text: String,
}

/// Everything a human may want to look at: quarantined keys and machine translations.
fn items(root: &Path, cfg: &Config) -> Result<Vec<Item>> {
    let units = crate::project::load_units(root, cfg)?;
    let lock = Lock::load(&root.join(crate::lockfile::FILE_NAME))?;
    let locales: Vec<&str> = cfg.target_locales.iter().map(String::as_str).collect();
    let status = lock.status(&units, &locales);
    let index = crate::context::usage::Index::build(root).ok();
    let keys: Vec<&str> = units
        .iter()
        .map(|u| crate::project::split_key(cfg, &u.key).1)
        .collect();
    let usages = index.map(|i| i.find_all(&keys)).unwrap_or_default();
    let mut out = Vec::new();
    for u in &units {
        for l in &locales {
            let st = status
                .per_locale
                .get(*l)
                .and_then(|m| m.get(&u.key))
                .copied();
            let rec = lock.keys.get(&u.key);
            let local_key = crate::project::split_key(cfg, &u.key).1;
            let context = usages.get(local_key).map(|us| match &us.ident {
                Some(id) => format!("{}:{} in `{id}`", us.path, us.line),
                None => format!("{}:{}", us.path, us.line),
            });
            match st {
                Some(State::NeedsReview) => {
                    let note = rec.and_then(|r| r.review.get(*l));
                    out.push(Item {
                        locale: (*l).to_string(),
                        key: u.key.clone(),
                        source: u.source.clone(),
                        translation: u.translations.get(*l).cloned(),
                        suggestion: note.and_then(|n| n.suggestion.clone()),
                        reason: note.map(|n| n.reason.clone()),
                        comment: u.comment.clone(),
                        context,
                        state: "needs-review",
                    });
                }
                Some(State::UpToDate) => {
                    let machine = rec
                        .and_then(|r| r.locales.get(*l))
                        .is_some_and(|rec| rec.provider != "human");
                    if machine {
                        out.push(Item {
                            locale: (*l).to_string(),
                            key: u.key.clone(),
                            source: u.source.clone(),
                            translation: u.translations.get(*l).cloned(),
                            suggestion: None,
                            reason: None,
                            comment: u.comment.clone(),
                            context,
                            state: "machine",
                        });
                    }
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn host_is_local(req: &tiny_http::Request) -> bool {
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| {
            let v = h.value.as_str();
            let host = v.rsplit_once(':').map(|(h, _)| h).unwrap_or(v);
            matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
        })
        .unwrap_or(false)
}

fn json_response(status: u16, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
}

pub fn serve(root: &Path, port: u16, open: bool) -> Result<()> {
    let cfg = Config::load(root)?;
    let root: PathBuf = root.to_path_buf();
    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("bind 127.0.0.1:{port}: {e}"))?;
    let bound = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .unwrap_or(port);
    let url = format!("http://127.0.0.1:{bound}");
    println!("polygo review: {url}  (Ctrl-C to stop)");
    if open {
        let _ = std::process::Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        })
        .arg(&url)
        .spawn();
    }
    for mut req in server.incoming_requests() {
        if !host_is_local(&req) {
            let _ = req
                .respond(Response::from_string("forbidden: non-local Host").with_status_code(403));
            continue;
        }
        let url = req.url().to_string();
        let response = match (req.method(), url.as_str()) {
            (Method::Get, "/") | (Method::Get, "/index.html") => Response::from_string(PAGE)
                .with_header(
                    Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap(),
                ),
            (Method::Get, "/api/items") => match items(&root, &cfg) {
                Ok(list) => json_response(200, serde_json::json!({ "items": list }).to_string()),
                Err(e) => json_response(
                    500,
                    serde_json::json!({ "error": e.to_string() }).to_string(),
                ),
            },
            (Method::Post, "/api/approve") | (Method::Post, "/api/reject") => {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let approve = url == "/api/approve";
                match serde_json::from_str::<Action>(&body)
                    .context("invalid JSON body")
                    .and_then(|a| apply(&root, &cfg, &a, approve))
                {
                    Ok(()) => json_response(200, "{\"ok\":true}".into()),
                    Err(e) => json_response(
                        400,
                        serde_json::json!({ "error": e.to_string() }).to_string(),
                    ),
                }
            }
            _ => Response::from_string("not found").with_status_code(404),
        };
        let _ = req.respond(response);
    }
    Ok(())
}

fn apply(root: &Path, cfg: &Config, a: &Action, approve: bool) -> Result<()> {
    let units = crate::project::load_units(root, cfg)?;
    let unit = units
        .iter()
        .find(|u| u.key == a.key)
        .with_context(|| format!("unknown key {}", a.key))?;
    if !cfg.target_locales.contains(&a.locale) {
        anyhow::bail!("unknown locale {}", a.locale);
    }
    let lock_path = root.join(crate::lockfile::FILE_NAME);
    let mut lock = Lock::load(&lock_path)?;
    if approve {
        if a.text.trim().is_empty() {
            anyhow::bail!("translation is empty");
        }
        crate::engine::write_translation(root, cfg, &a.key, &a.locale, &a.text)?;
        lock.record(unit, &a.locale, &a.text, "human", "review");
    } else {
        lock.quarantine(
            unit,
            &a.locale,
            "rejected in review",
            unit.translations.get(&a.locale).map(String::as_str),
        );
    }
    lock.save(&lock_path)
}
