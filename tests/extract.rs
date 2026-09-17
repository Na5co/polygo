//! `polygo extract`: user-facing text out of JSX / HTML-in-template-literals / .html.
use std::fs;
use std::process::Command;

#[test]
fn extract_finds_copy_and_skips_code() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("node_modules/x")).unwrap();
    fs::write(
        root.join("src/App.tsx"),
        r#"import React from "react";
export function App({ n }: { n: number }) {
  if (n < 3 && n > 1) return null;
  const cls = "btn primary";
  return (
    <div className={cls}>
      <h1>Welcome back</h1>
      <p>You have {n} new messages.</p>
      <input placeholder="Search forms" aria-label="Search" />
      <button onClick={() => save()}>Save changes</button>
      <a href="/docs">Read the docs</a>
      <code>npm install</code>
      <svg><path d="M4 8h16"/></svg>
      <span>{n}</span>
      <img alt="Company logo" src="/logo.png" />
    </div>
  );
}
"#,
    )
    .unwrap();
    fs::write(
        root.join("src/page.ts"),
        "export const page = (email: string) => `<section><h2>Your forms</h2><p>Signed in as ${email}.</p>${email ? `<button type=\"submit\">Sign out</button>` : \"\"}</section>`;\nconst x = a < b ? c : d;\n",
    )
    .unwrap();
    fs::write(root.join("src/App.test.tsx"), "<p>Not this</p>").unwrap();
    fs::write(root.join("node_modules/x/i.tsx"), "<p>Nor this</p>").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["extract", "--dry-run"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for want in [
        "src/App.tsx:7  Welcome back",
        "You have {n} new messages.",
        "Search forms",
        "src/App.tsx:9  Search",
        "Save changes",
        "Read the docs",
        "Company logo",
        "Your forms",
        "Signed in as ${email}.",
        "Sign out",
    ] {
        assert!(text.contains(want), "missing {want:?}:\n{text}");
    }
    for nope in [
        "npm install",
        "M4 8h16",
        "btn primary",
        "Not this",
        "Nor this",
        "a < b",
    ] {
        assert!(!text.contains(nope), "should not extract {nope:?}:\n{text}");
    }
    assert!(text.contains("10 string(s), 10 unique"), "{text}");
    assert!(!root.join("locales/en.json").exists());

    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("extract")
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("locales/en.json")).unwrap()).unwrap();
    assert_eq!(json["Welcome back"], "Welcome back");
    assert_eq!(
        json["You have {{0}} new messages."],
        "You have {{0}} new messages."
    );
    assert_eq!(json["Signed in as {{0}}."], "Signed in as {{0}}.");
    // The catalog is now a project polygo can init.
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("init")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        fs::read_to_string(root.join("polygo.toml"))
            .unwrap()
            .contains("locales/en.json")
    );
}
