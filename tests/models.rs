//! `polygo models` / `polygo use`: spec parsing, project + global config writes,
//! stored API keys (0600) picked up by the provider, clear errors without Ollama.
use std::fs;
use std::process::Command;

fn polygo(root: &std::path::Path, cfg_dir: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_polygo"));
    c.current_dir(root)
        .env("POLYGO_CONFIG_DIR", cfg_dir)
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("POLYGO_API_KEY");
    c
}

fn project(root: &std::path::Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(root.join("locales/en.json"), "{\n  \"hi\": \"Hello\"\n}\n").unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
}

#[test]
fn spec_parsing() {
    use polygo::models::parse_spec;
    let p = parse_spec("gemma4", None).unwrap();
    assert_eq!(
        (p.kind.as_str(), p.model.as_deref()),
        ("ollama", Some("gemma4"))
    );
    let p = parse_spec("openai/gpt-4o-mini", None).unwrap();
    assert_eq!(
        (p.kind.as_str(), p.model.as_deref()),
        ("openai", Some("gpt-4o-mini"))
    );
    let p = parse_spec(
        "openai/llama-3.3-70b",
        Some("https://api.groq.com/openai/v1".into()),
    )
    .unwrap();
    assert_eq!(p.kind, "openai");
    assert_eq!(
        p.base_url.as_deref(),
        Some("https://api.groq.com/openai/v1")
    );
    // A bare name is always Ollama, even with --base-url (a remote Ollama box).
    assert_eq!(
        parse_spec("qwen3:8b", Some("http://gpu-box:11434".into()))
            .unwrap()
            .kind,
        "ollama"
    );
    assert!(parse_spec("huggingface/x", None).is_err());
    assert!(parse_spec("ollama/", None).is_err());
}

#[test]
fn use_api_model_stores_key_and_provider_and_doctor_finds_it() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    project(dir.path());
    let out = polygo(dir.path(), cfg.path())
        .args([
            "use",
            "anthropic/claude-sonnet-5",
            "--api-key",
            "sk-test-not-real",
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("stored API key for anthropic"), "{text}");
    let toml = fs::read_to_string(dir.path().join("polygo.toml")).unwrap();
    assert!(
        toml.contains("kind = \"anthropic\"") && toml.contains("model = \"claude-sonnet-5\""),
        "{toml}"
    );
    let creds = cfg.path().join("credentials.toml");
    assert!(
        fs::read_to_string(&creds)
            .unwrap()
            .contains("anthropic = \"sk-test-not-real\"")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&creds).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    // doctor sees the stored key without any environment variable.
    let out = polygo(dir.path(), cfg.path())
        .arg("doctor")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("ok   provider    anthropic · claude-sonnet-5"),
        "{text}"
    );
    // No key and nothing stored → doctor names `polygo use … --api-key`.
    let cfg2 = tempfile::tempdir().unwrap();
    let out = polygo(dir.path(), cfg2.path())
        .arg("doctor")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text.contains("polygo use anthropic/claude-sonnet-5 --api-key"),
        "{text}"
    );
}

#[test]
fn use_global_sets_the_default_that_init_writes() {
    let cfg = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    // No polygo.toml here: `use` writes only the global default (mock needs no pull).
    let out = polygo(empty.path(), cfg.path())
        .args(["use", "mock/fake"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("default for new projects = mock/fake"),
        "{text}"
    );
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(
        dir.path().join("locales/en.json"),
        "{\n  \"hi\": \"Hello\"\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("locales/de.json"),
        "{\n  \"hi\": \"Hallo\"\n}\n",
    )
    .unwrap();
    let out = polygo(dir.path(), cfg.path()).arg("init").output().unwrap();
    assert!(out.status.success());
    let toml = fs::read_to_string(dir.path().join("polygo.toml")).unwrap();
    assert!(
        toml.contains("kind = \"mock\"") && toml.contains("model = \"fake\""),
        "{toml}"
    );
}

#[test]
fn use_without_ollama_explains_how_to_install_and_models_still_lists() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    project(dir.path());
    // Closed loopback port: refused instantly, no egress.
    let out = polygo(dir.path(), cfg.path())
        .args(["use", "gemma4", "--base-url", "http://127.0.0.1:1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Ollama is not running") && err.contains("brew install ollama"),
        "{err}"
    );
    assert!(err.contains("polygo use openai/"), "{err}");
    // polygo.toml untouched on failure.
    assert!(
        fs::read_to_string(dir.path().join("polygo.toml"))
            .unwrap()
            .contains("kind = \"mock\"")
    );

    fs::write(
        dir.path().join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[provider]\nkind = \"ollama\"\nmodel = \"gemma4\"\nbase_url = \"http://127.0.0.1:1\"\n",
    )
    .unwrap();
    let out = polygo(dir.path(), cfg.path())
        .arg("models")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("not running"), "{text}");
    assert!(
        text.contains("* gemma4") && text.contains("qwen3:8b") && text.contains("anthropic/"),
        "{text}"
    );
}
