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

#[test]
fn rewrite_replaces_strings_generates_helper_and_honours_ignores() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src/site")).unwrap();
    fs::create_dir_all(root.join("src/admin")).unwrap();
    fs::write(
        root.join("src/site/page.ts"),
        "import { escapeHtml } from \"../util\";\n\nexport const page = (email: string, n: number) => `<section>\n  <h2>Your forms</h2>\n  <p>Signed in as ${escapeHtml(email)}.</p>\n  <p>Welcome to Leafslip.</p>\n  <p>You have ${n} <em>new</em> answers.</p>\n  <a class=\"cta\" href=\"/new\">Write a form</a>\n  <input placeholder=\"Search forms\">\n</section>`;\n",
    )
    .unwrap();
    fs::write(
        root.join("src/App.tsx"),
        "export function App({ n }: { n: number }) {\n  return (\n    <div>\n      <h1>Welcome back</h1>\n      <p>You have {n} new messages.</p>\n      <input placeholder=\"Search forms\" />\n    </div>\n  );\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/admin/page.ts"),
        "export const a = `<p>Admin only text</p>`;\n",
    )
    .unwrap();
    fs::write(
        root.join("src/doors.ts"),
        "type Door = { id: string; label: string };\nexport const doors: Door[] = [\n  { id: \"typed\", label: \"typed\" },\n  { id: \"drawn\", label: \"Drawn by hand\", title: 'Open the editor' },\n];\nconst cfg = { label: \"btn-primary\", description: \"\" };\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = []\n\n[extract]\nignore = [\"leafslip\"]\nignore_paths = [\"src/admin/**\"]\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["extract", "--rewrite"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !text.contains("Leafslip") && !text.contains("Admin only"),
        "{text}"
    );

    let page = fs::read_to_string(root.join("src/site/page.ts")).unwrap();
    assert!(
        page.starts_with(
            "import { escapeHtml } from \"../util\";\nimport { t } from \"../i18n\";\n"
        ),
        "{page}"
    );
    assert!(page.contains("<h2>${t(\"Your forms\")}</h2>"), "{page}");
    assert!(
        page.contains("<p>${t(\"Signed in as {{0}}.\", { 0: escapeHtml(email) })}</p>"),
        "{page}"
    );
    assert!(
        page.contains("<p>Welcome to Leafslip.</p>"),
        "ignored word must stay:\n{page}"
    );
    assert!(
        page.contains("<p>${t(\"You have {{0}} <em>new</em> answers.\", { 0: n })}</p>"),
        "sentence with inline markup is one string:\n{page}"
    );
    assert!(
        page.contains("placeholder=\"${t(\"Search forms\")}\""),
        "{page}"
    );

    assert!(
        page.contains("<a class=\"cta\" href=\"/new\">${t(\"Write a form\")}</a>"),
        "whole inline element is not a fragment:\n{page}"
    );

    let app = fs::read_to_string(root.join("src/App.tsx")).unwrap();
    assert!(app.starts_with("import { t } from \"./i18n\";\n"), "{app}");
    assert!(app.contains("<h1>{t(\"Welcome back\")}</h1>"), "{app}");
    assert!(
        app.contains("<p>{t(\"You have {{0}} new messages.\", { 0: n })}</p>"),
        "{app}"
    );
    assert!(app.contains("placeholder={t(\"Search forms\")}"), "{app}");

    let doors = fs::read_to_string(root.join("src/doors.ts")).unwrap();
    assert!(doors.contains("label: t(\"typed\")"), "{doors}");
    assert!(
        doors.contains("label: t(\"Drawn by hand\"), title: t(\"Open the editor\")"),
        "{doors}"
    );
    assert!(
        doors.contains("label: \"btn-primary\""),
        "identifier-like values stay:\n{doors}"
    );
    assert!(
        doors.contains("label: string"),
        "type annotations stay:\n{doors}"
    );
    assert!(
        doors.starts_with("import { t } from \"./i18n\";\n"),
        "{doors}"
    );

    let helper = fs::read_to_string(root.join("src/i18n.ts")).unwrap();
    assert!(
        helper.contains("../locales/de.json") && !helper.contains("import en"),
        "{helper}"
    );
    assert!(
        helper.contains("export function t(") && helper.contains("setLocale"),
        "{helper}"
    );
    let catalog: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("locales/en.json")).unwrap()).unwrap();
    assert_eq!(catalog["Signed in as {{0}}."], "Signed in as {{0}}.");
    assert!(catalog.get("Welcome to Leafslip.").is_none());
    assert!(catalog.get("Admin only text").is_none());
    assert!(
        fs::read_to_string(root.join("src/admin/page.ts"))
            .unwrap()
            .contains("<p>Admin only text</p>")
    );

    // Running it again changes nothing (no double import, no re-wrapping).
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["extract", "--rewrite"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(root.join("src/App.tsx")).unwrap(), app);
    assert_eq!(
        fs::read_to_string(root.join("src/site/page.ts")).unwrap(),
        page
    );
    assert_eq!(
        fs::read_to_string(root.join("src/doors.ts")).unwrap(),
        doors
    );
}
