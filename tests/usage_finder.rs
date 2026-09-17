//! G4.1: find where a key is used in source code, with the enclosing identifier and a snippet.
use polygo::context::usage::Index;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn fixture(root: &Path) {
    write(
        root,
        "App/Views/SettingsView.swift",
        r#"import SwiftUI

struct SettingsView: View {
    @State private var name = ""

    var body: some View {
        Form {
            TextField("Display name", text: $name)
            Button("Save changes") { save() }
            Text(String(localized: "%lld files selected"))
            Text("Delete account?")
                .font(.headline)
        }
    }

    func save() {
        let msg = NSLocalizedString("Saved!", comment: "toast")
        print(msg)
    }
}
"#,
    );
    write(
        root,
        "app/src/main/java/com/example/MainActivity.kt",
        r#"package com.example

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = getString(R.string.app_title)
        binding.button.text = getString(R.string.sign_in)
    }

    @Composable
    fun Greeting(name: String) {
        Text(text = stringResource(R.string.greeting_format, name))
    }
}
"#,
    );
    write(
        root,
        "app/src/main/res/layout/activity_main.xml",
        r#"<LinearLayout>
    <TextView android:text="@string/welcome_subtitle" />
</LinearLayout>
"#,
    );
    write(
        root,
        "lib/screens/home_screen.dart",
        r#"import 'package:flutter/material.dart';

class HomeScreen extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Scaffold(
      appBar: AppBar(title: Text(l10n.homeTitle)),
      body: Center(child: Text(context.l10n.emptyState)),
    );
  }
}
"#,
    );
    write(
        root,
        "src/components/Checkout.tsx",
        r#"import { useTranslation, Trans } from 'react-i18next';

export default function Checkout({ items }: Props) {
  const { t } = useTranslation();
  return (
    <div>
      <h1>{t('checkout.title')}</h1>
      <p>{t("checkout.summary", { count: items.length })}</p>
      <Trans i18nKey="checkout.terms">By continuing you agree</Trans>
      <button>{t(`checkout.pay`)}</button>
    </div>
  );
}

export const Footer = () => <span>{t('footer.copyright')}</span>;
"#,
    );
    // Noise that must be ignored.
    write(root, "node_modules/lib/index.js", "t('checkout.title')");
    write(root, "build/generated/Strings.swift", "\"Save changes\"");
}

#[test]
fn usage_finder_resolves_keys_across_languages() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let index = Index::build(dir.path()).unwrap();
    assert!(
        index.files() >= 5 && index.files() <= 6,
        "files indexed: {}",
        index.files()
    );

    let keys = [
        "Display name",
        "Save changes",
        "%lld files selected",
        "Delete account?",
        "Saved!",
        "app_title",
        "sign_in",
        "greeting_format",
        "welcome_subtitle",
        "homeTitle",
        "emptyState",
        "checkout.title",
        "checkout.summary",
        "checkout.terms",
        "checkout.pay",
        "footer.copyright",
    ];
    let found = index.find_all(&keys);
    let resolved = keys.iter().filter(|k| found.contains_key(**k)).count();
    assert!(
        resolved * 10 >= keys.len() * 9,
        "resolved {resolved}/{}: {:?}",
        keys.len(),
        found.keys()
    );

    let u = &found["Save changes"];
    assert!(u.path.ends_with("SettingsView.swift"), "{}", u.path);
    assert_eq!(u.ident.as_deref(), Some("SettingsView"), "{u:?}");
    assert!(
        u.snippet.contains("Button(\"Save changes\")"),
        "{}",
        u.snippet
    );
    assert!(u.snippet.lines().count() <= 13);
    assert!(!u.path.contains("build/"), "generated dirs must be skipped");

    assert_eq!(found["Saved!"].ident.as_deref(), Some("save"));
    assert_eq!(found["sign_in"].ident.as_deref(), Some("onCreate"));
    assert_eq!(found["greeting_format"].ident.as_deref(), Some("Greeting"));
    assert!(
        found["welcome_subtitle"]
            .path
            .ends_with("activity_main.xml")
    );
    assert_eq!(found["homeTitle"].ident.as_deref(), Some("build"));
    assert_eq!(found["checkout.title"].ident.as_deref(), Some("Checkout"));
    assert!(
        found["checkout.title"].path.ends_with("Checkout.tsx"),
        "node_modules must be skipped"
    );
    assert_eq!(found["footer.copyright"].ident.as_deref(), Some("Footer"));
}

#[test]
fn usage_finder_respects_identifier_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "A.kt",
        "val a = getString(R.string.title_long)\n",
    );
    write(dir.path(), "B.kt", "val b = getString(R.string.title)\n");
    let index = Index::build(dir.path()).unwrap();
    let found = index.find_all(&["title", "title_long"]);
    assert!(
        found["title"].path.ends_with("B.kt"),
        "{:?}",
        found["title"]
    );
    assert!(found["title_long"].path.ends_with("A.kt"));
}

#[test]
fn usage_finder_is_fast_on_a_large_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let n_files = 5000;
    for i in 0..n_files {
        let body = format!(
            "import Foundation\n\nstruct View{i}: View {{\n    var body: some View {{\n        Text(\"key_{i}\")\n        Text(\"Shared label\")\n    }}\n}}\n"
        );
        write(root, &format!("Sources/Mod{}/View{i}.swift", i % 50), &body);
    }
    let keys: Vec<String> = (0..400).map(|i| format!("key_{}", i * 12)).collect();
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    // Best of three: the test suite runs in parallel and a debug build under load
    // can double; the budget is about the algorithm, not the scheduler.
    let mut elapsed = std::time::Duration::MAX;
    let mut found = std::collections::HashMap::new();
    for _ in 0..3 {
        let t0 = Instant::now();
        let index = Index::build(root).unwrap();
        found = index.find_all(&key_refs);
        elapsed = elapsed.min(t0.elapsed());
    }
    assert_eq!(found.len(), 400);
    // 200 ms on a laptop; the Windows CI runner's disk reads 5000 small files at a
    // fraction of that speed (measured 350 ms), so it gets a looser budget.
    let budget = if cfg!(windows) { 1000 } else { 200 };
    assert!(
        elapsed.as_millis() < budget,
        "took {elapsed:?} for {n_files} files"
    );
}
