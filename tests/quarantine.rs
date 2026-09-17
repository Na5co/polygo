//! G2.4: malformed or rule-breaking model output is repaired once, then quarantined —
//! never written to the locale files.
use std::fs;
use std::path::Path;
use std::process::Command;

fn polygo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
}

fn project(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"k1\": \"First string\",\n  \"k2\": \"Second string\",\n  \"k3\": \"Third string\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
}

#[test]
fn malformed_output_is_quarantined() {
    // 1. Garbage on the first call, valid on the repair call → everything recovered.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_MALFORMED_ONCE", "1")
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    for k in ["k1", "k2", "k3"] {
        assert!(de.contains(&format!("\"{k}\"")), "{de}");
    }

    // 2. One key is always missing from the output → that key is quarantined, the rest written.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_DROP_KEY", "k2")
        .arg("translate")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "quarantine exit code: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("k2") && err.contains("review"), "{err}");
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(de.contains("\"k1\"") && de.contains("\"k3\""), "{de}");
    assert!(
        !de.contains("\"k2\""),
        "quarantined key must not be written:\n{de}"
    );
    let lock = fs::read_to_string(dir.path().join("polygo.lock")).unwrap();
    assert!(lock.contains("[keys.k2.review.de]"), "{lock}");

    let status = polygo()
        .current_dir(dir.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(v["locales"]["de"]["needs_review"], 1);
    assert_eq!(v["locales"]["de"]["up_to_date"], 2);

    // A plain re-run leaves quarantined keys alone (no provider call for them)...
    let log = dir.path().join("mock.log");
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_LOG", &log)
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!log.exists(), "must not retry quarantined keys by default");
    // ...and --retry-review tries again; with a now-healthy provider it succeeds.
    let out = polygo()
        .current_dir(dir.path())
        .args(["translate", "--retry-review"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(de.contains("\"k2\""), "{de}");
    let lock = fs::read_to_string(dir.path().join("polygo.lock")).unwrap();
    assert!(
        !lock.contains("[keys.k2.review.de]"),
        "review note cleared:\n{lock}"
    );

    // 3. Permanently broken provider → all quarantined, nothing written, no crash.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_MALFORMED", "1")
        .arg("translate")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(!dir.path().join("locales/de.json").exists());
}

#[test]
fn identical_to_source_is_confirmed_by_a_second_ask_not_quarantined() {
    // "OK" is legitimately the same in German: the mock echoes it twice → accepted.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(
        dir.path().join("locales/en.json"),
        "{\n  \"ok\": \"OK\",\n  \"hello\": \"Hello\"\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let log = dir.path().join("mock.log");
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_ECHO_KEY", "ok")
        .env("POLYGO_MOCK_LOG", &log)
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(de.contains("\"ok\": \"OK\""), "{de}");
    assert!(de.contains("⟦de⟧ Hello"), "{de}");
    // The echo key was asked twice (initial + confirmation), hello only once.
    let calls: Vec<String> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(calls.iter().filter(|k| *k == "ok").count(), 2);
    assert_eq!(calls.iter().filter(|k| *k == "hello").count(), 1);
}

#[test]
fn key_echo_is_repaired_not_written() {
    // Keys like "Icon Name: Summer" (source "Summer"): a reply equal to the key is rejected.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("locales")).unwrap();
    fs::write(
        dir.path().join("locales/en.json"),
        "{\n  \"icon.summer\": \"Summer\"\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_KEY_ECHO_ONCE", "1")
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(
        de.contains("⟦de⟧ Summer"),
        "repair must replace the key echo:\n{de}"
    );
}

#[test]
fn untranslated_fragment_is_repaired_then_quarantined() {
    // Repaired on the second ask → written normally.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_FRAGMENT_ONCE", "k2")
        .env("POLYGO_NO_BACKOFF", "1")
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(de.contains("\"k2\": \"⟦de⟧ Second string\""), "{de}");

    // Still leaving `string` untranslated after the repair round → quarantined, exit 3.
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let out = polygo()
        .current_dir(dir.path())
        .env("POLYGO_MOCK_FRAGMENT", "k2")
        .env("POLYGO_NO_BACKOFF", "1")
        .arg("translate")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("k2") && err.contains("left untranslated: `string`"),
        "{err}"
    );
    let de = fs::read_to_string(dir.path().join("locales/de.json")).unwrap();
    assert!(
        !de.contains("こんにちは"),
        "fragment must not be written:\n{de}"
    );
    assert!(de.contains("\"k1\"") && de.contains("\"k3\""), "{de}");
}
