//! `polygo doctor`: every failure names its fix; exit 1 when anything fails.
use std::fs;
use std::process::Command;

fn run(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(args)
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("POLYGO_API_KEY")
        .output()
        .unwrap()
}

fn project(root: &std::path::Path, provider: &str) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(root.join("locales/en.json"), "{\n  \"hi\": \"Hello\"\n}\n").unwrap();
    fs::write(
        root.join("polygo.toml"),
        format!(
            "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{{locale}}.json\"\n\n[provider]\n{provider}\n"
        ),
    )
    .unwrap();
}

#[test]
fn doctor_passes_with_mock_and_reports_ready() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "kind = \"mock\"");
    let out = run(dir.path(), &["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("ok   config") && text.contains("1 unit(s)"),
        "{text}"
    );
    assert!(
        text.trim_end().ends_with("ready — run `polygo translate`"),
        "{text}"
    );
    let out = run(dir.path(), &["doctor", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.as_array().unwrap().iter().all(|c| c["ok"] == true));
}

#[test]
fn doctor_names_the_fix_for_missing_config_key_and_unreachable_ollama() {
    // No polygo.toml at all.
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), &["doctor"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("FAIL config") && text.contains("fix:        polygo init"),
        "{text}"
    );

    // API provider without a key.
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        "kind = \"anthropic\"\nmodel = \"claude-sonnet-5\"",
    );
    let out = run(dir.path(), &["doctor"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("export ANTHROPIC_API_KEY"), "{text}");

    // Ollama pointed at a closed local port: refused immediately (loopback only, no egress).
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        "kind = \"ollama\"\nmodel = \"qwen3:8b\"\nbase_url = \"http://127.0.0.1:1\"",
    );
    let out = run(dir.path(), &["doctor"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("FAIL ollama") && text.contains("ollama serve"),
        "{text}"
    );
    assert!(text.contains("not ready"), "{text}");
}
