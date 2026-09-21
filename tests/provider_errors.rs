//! Failures retrying cannot fix (nothing listening, bad key, unknown model) stop the
//! run at once and print the problem and the fix, instead of three backoff rounds
//! and a chain of socket errors.
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn project(root: &Path, provider: &str) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(root.join("locales/en.json"), "{\n  \"a\": \"Hello\"\n}\n").unwrap();
    fs::write(
        root.join("polygo.toml"),
        format!(
            "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{{locale}}.json\"\n\n[provider]\n{provider}\n"
        ),
    )
    .unwrap();
}

/// `polygo translate` with no API key in the environment and a private config dir, so
/// the developer's own credentials and translation memory stay out of the test.
fn translate_with(root: &Path, env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_polygo"));
    cmd.current_dir(root)
        .arg("translate")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("POLYGO_API_KEY")
        .env("POLYGO_CONFIG_DIR", root.join("cfg"));
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

fn translate(root: &Path) -> std::process::Output {
    translate_with(root, &[])
}

/// One-shot HTTP server that answers every request with `status` and `body`, counting them.
fn serve(status: u16, body: &'static str) -> (String, Arc<AtomicUsize>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}", server.server_addr());
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            h.fetch_add(1, Ordering::SeqCst);
            let resp = tiny_http::Response::from_string(body).with_status_code(status);
            let _ = req.respond(resp);
        }
    });
    (url, hits)
}

#[test]
fn ollama_down_fails_fast_with_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        "kind = \"ollama\"\nmodel = \"qwen3:8b\"\nbase_url = \"http://127.0.0.1:1\"",
    );
    let t = std::time::Instant::now();
    let out = translate(dir.path());
    assert!(
        t.elapsed().as_secs() < 3,
        "no backoff rounds: {:?}",
        t.elapsed()
    );
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ollama is not running at http://127.0.0.1:1"),
        "{err}"
    );
    assert!(
        err.contains("fix: ") && err.contains("ollama serve"),
        "{err}"
    );
    assert!(
        !err.contains("gave up") && !err.contains("batch 1/1"),
        "{err}"
    );
}

#[test]
fn ollama_unknown_model_says_polygo_use() {
    let (url, hits) = serve(404, r#"{"error":"model 'nope:1b' not found"}"#);
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        &format!("kind = \"ollama\"\nmodel = \"nope:1b\"\nbase_url = \"{url}\""),
    );
    let out = translate(dir.path());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ollama has no model `nope:1b` (model 'nope:1b' not found)"),
        "{err}"
    );
    assert!(err.contains("fix: `polygo use nope:1b` pulls it"), "{err}");
    // One /api/tags probe (404 here, so the preflight steps aside) and one chat call.
    assert_eq!(hits.load(Ordering::SeqCst), 2, "no retries");
}

#[test]
fn openai_without_key_names_the_env_var() {
    let (url, hits) = serve(
        401,
        r#"{"error":{"message":"You didn't provide an API key. You need to provide your API key in an Authorization header.","type":"invalid_request_error"}}"#,
    );
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        &format!("kind = \"openai\"\nmodel = \"gpt-4o-mini\"\nbase_url = \"{url}\""),
    );
    let out = translate(dir.path());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("wants an API key and none is set"), "{err}");
    assert!(err.contains("export OPENAI_API_KEY"), "{err}");
    assert!(
        err.contains("polygo use openai/gpt-4o-mini --api-key"),
        "{err}"
    );
    assert!(
        !err.contains("Authorization header"),
        "server essay not quoted: {err}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn openai_rejected_key_quotes_the_first_sentence() {
    let (url, hits) = serve(
        401,
        r#"{"error":{"message":"Incorrect API key provided: sk-abc. You can find your API key at https://platform.openai.com.","type":"invalid_request_error"}}"#,
    );
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        &format!("kind = \"openai\"\nmodel = \"gpt-4o-mini\"\nbase_url = \"{url}\""),
    );
    let out = translate_with(dir.path(), &[("OPENAI_API_KEY", "sk-abc")]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("rejected the API key (Incorrect API key provided: sk-abc)"),
        "{err}"
    );
    assert!(!err.contains("platform.openai.com"), "{err}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn server_errors_are_still_retried() {
    let (url, hits) = serve(503, "overloaded");
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        &format!("kind = \"openai\"\nmodel = \"m\"\nbase_url = \"{url}\""),
    );
    let out = translate_with(dir.path(), &[("POLYGO_NO_BACKOFF", "1")]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("gave up after 3 attempts"), "{err}");
    assert!(err.contains("http status 503: overloaded"), "{err}");
    assert_eq!(hits.load(Ordering::SeqCst), 3);
}

#[test]
fn check_says_when_there_is_nothing_to_check_yet() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "kind = \"mock\"");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_polygo"))
            .current_dir(dir.path())
            .args(args)
            .env("POLYGO_CONFIG_DIR", dir.path().join("cfg"))
            .output()
            .unwrap()
    };
    let out = run(&["check"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("nothing to check yet (no translations in de)"),
        "{text}"
    );
    assert!(text.contains("`polygo translate` first"), "{text}");
    assert!(run(&["translate"]).status.success());
    let out = run(&["check"]);
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("check: ok (1 translation(s) in 1 locale(s))")
    );
}

#[test]
fn missing_ollama_model_without_a_terminal_names_the_pull_options() {
    // Ollama "up" with no models: the preflight cannot ask (no tty) and says how to pull.
    let (url, hits) = serve(200, r#"{"models":[]}"#);
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        &format!("kind = \"ollama\"\nmodel = \"qwen3:8b\"\nbase_url = \"{url}\""),
    );
    let out = translate(dir.path());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ollama has no model `qwen3:8b` (~5.2 GB)"),
        "{err}"
    );
    assert!(
        err.contains("`polygo use qwen3:8b` pulls it (or `polygo translate --yes`)"),
        "{err}"
    );
    // Only /api/tags was asked (reachability, then the model list); no translation call.
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}
