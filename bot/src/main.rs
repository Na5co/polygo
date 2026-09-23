//! `polygo-bot`: a GitHub App that reviews pull requests with `polygo check`.
//!
//! One endpoint, no database, no queue, nothing kept between events: a webhook arrives,
//! the signature is verified, the pull request is fetched, checked and commented on, and
//! the working copy is deleted. Configure it with the environment:
//!
//!   POLYGO_BOT_APP_ID          the App's id (Settings → Developer settings → GitHub Apps)
//!   POLYGO_BOT_PRIVATE_KEY     the App's private key (PEM), or
//!   POLYGO_BOT_PRIVATE_KEY_FILE  a file holding it
//!   POLYGO_BOT_WEBHOOK_SECRET  the webhook secret, the same string GitHub signs with
//!   PORT                       default 8080
//!   POLYGO_BOT_SYNC=1          review before answering GitHub, instead of on a thread
//!   POLYGO_BOT_MAX_REPO_MB     skip repositories bigger than this (default 500)
//!   POLYGO_BOT_MAX_REVIEWS     reviews one installation may ask for per 10 min (default 20)
//!
//! `POLYGO_BOT_SYNC=1` is for platforms that stop the CPU as soon as a request is answered
//! (Cloud Run's default, and anything else serverless): a background thread would be
//! frozen mid-review there. The cost is that a check taking longer than GitHub's ten
//! seconds makes the delivery show as timed out — the review is still posted, since the
//! platform lets the handler finish after GitHub hangs up.

mod github;
mod handle;
mod limits;
mod webhook;

use anyhow::{Context, Result, bail};
use std::sync::Arc;

fn main() -> Result<()> {
    let app_id = env("POLYGO_BOT_APP_ID")?;
    let secret = env("POLYGO_BOT_WEBHOOK_SECRET")?;
    let key = match std::env::var("POLYGO_BOT_PRIVATE_KEY_FILE") {
        Ok(p) if !p.is_empty() => {
            std::fs::read_to_string(&p).with_context(|| format!("reading {p}"))?
        }
        _ => env("POLYGO_BOT_PRIVATE_KEY")?,
    };
    let app = Arc::new(github::App::new(&app_id, &key)?);
    let limits = Arc::new(limits::Limits::from_env());
    let rate = Arc::new(limits::Rate::default());
    let sync = std::env::var("POLYGO_BOT_SYNC").is_ok_and(|v| v == "1");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let server = tiny_http::Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow::anyhow!("listening on port {port}: {e}"))?;
    // The bot has no version of its own; the one that matters is the check's.
    log(&format!(
        "polygo-bot (polygo {}) listening on :{port} as app {app_id}{}",
        polygo::VERSION,
        if sync { " (sync)" } else { "" }
    ));
    for mut req in server.incoming_requests() {
        let path = req.url().split('?').next().unwrap_or("/").to_string();
        let method = req.method().as_str().to_string();
        if method == "GET" && (path == "/" || path == "/health") {
            let _ = req.respond(text(200, "polygo-bot ok"));
            continue;
        }
        if method != "POST" || (path != "/" && path != "/webhook") {
            let _ = req.respond(text(404, "not found"));
            continue;
        }
        let (signature, kind) = (
            header(&req, "X-Hub-Signature-256"),
            header(&req, "X-GitHub-Event").unwrap_or_default(),
        );
        let mut body = Vec::new();
        if req.as_reader().read_to_end(&mut body).is_err() {
            let _ = req.respond(text(400, "could not read the body"));
            continue;
        }
        if let Err(e) = webhook::verify(&secret, &body, signature.as_deref()) {
            log(&format!("rejected a delivery: {e}"));
            let _ = req.respond(text(401, "bad signature"));
            continue;
        }
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body)
            && let Some(what) = webhook::marketplace(&kind, &v)
        {
            log(&what);
            let _ = req.respond(text(202, "accepted"));
            continue;
        }
        let event = match serde_json::from_slice(&body)
            .map_err(anyhow::Error::from)
            .and_then(|v: serde_json::Value| webhook::pull_request(&kind, &v))
        {
            Ok(e) => e,
            Err(e) => {
                log(&format!("ignored a {kind} delivery: {e}"));
                let _ = req.respond(text(400, "unusable payload"));
                continue;
            }
        };
        let Some(ev) = event else {
            let _ = req.respond(text(202, "accepted"));
            continue;
        };
        if sync {
            // The platform stops this container's CPU when the response goes out, so the
            // review has to happen first.
            let what = run(&app, &ev, &limits, &rate);
            log(&what);
            let _ = req.respond(text(202, what));
            continue;
        }
        // Answer now and work after: GitHub expects a delivery to be acknowledged within
        // ten seconds, and a check on a large catalog takes longer than that.
        let _ = req.respond(text(202, "accepted"));
        let (app, limits, rate) = (Arc::clone(&app), Arc::clone(&limits), Arc::clone(&rate));
        std::thread::spawn(move || log(&run(&app, &ev, &limits, &rate)));
    }
    Ok(())
}

/// One review, and one line about it either way.
fn run(
    app: &github::App,
    ev: &webhook::Event,
    limits: &limits::Limits,
    rate: &limits::Rate,
) -> String {
    match handle::review(app, ev, limits, rate) {
        Ok(what) => what,
        Err(e) => format!("{}#{}: {e:#}", ev.repo, ev.number),
    }
}

fn header(req: &tiny_http::Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

fn env(name: &str) -> Result<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => bail!("{name} is not set (see the comment at the top of bot/src/main.rs)"),
    }
}

fn text(status: u16, body: impl AsRef<str>) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(body.as_ref()).with_status_code(status)
}

/// One line per event, to stdout: whatever runs the container collects it.
fn log(what: &str) {
    println!("{what}");
}
