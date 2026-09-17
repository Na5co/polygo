//! G1.6: `polygo init` detects the project type and writes a sensible polygo.toml.
use polygo::config::{Config, Format};
use polygo::init::detect;
use std::fs;
use std::path::Path;
use std::process::Command;

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

const XCSTRINGS: &str = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "hi" : {
      "localizations" : {
        "de" : { "stringUnit" : { "state" : "translated", "value" : "Hallo" } },
        "en" : { "stringUnit" : { "state" : "translated", "value" : "Hi" } },
        "fr" : { "stringUnit" : { "state" : "translated", "value" : "Salut" } }
      }
    }
  },
  "version" : "1.0"
}"#;

#[test]
fn init_detects_ios_string_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "MyApp.xcodeproj/project.pbxproj", "");
    write(root, "MyApp/Localizable.xcstrings", XCSTRINGS);
    write(root, "MyApp/InfoPlist.xcstrings", XCSTRINGS);
    write(root, "Pods/Some/Localizable.xcstrings", XCSTRINGS); // must be ignored
    let cfg = detect(root).unwrap();
    assert_eq!(cfg.source_locale, "en");
    assert_eq!(cfg.target_locales, vec!["de", "fr"]);
    let paths: Vec<String> = cfg
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        vec!["MyApp/InfoPlist.xcstrings", "MyApp/Localizable.xcstrings"]
    );
    assert!(
        cfg.files
            .iter()
            .all(|f| f.format == Format::Xcstrings && f.locale_path.is_none())
    );
}

#[test]
fn init_detects_android_values_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "build.gradle.kts", "");
    write(
        root,
        "app/src/main/res/values/strings.xml",
        "<resources><string name=\"a\">A</string></resources>\n",
    );
    write(
        root,
        "app/src/main/res/values-de/strings.xml",
        "<resources><string name=\"a\">A</string></resources>\n",
    );
    write(
        root,
        "app/src/main/res/values-pt-rBR/strings.xml",
        "<resources><string name=\"a\">A</string></resources>\n",
    );
    write(
        root,
        "app/src/main/res/values-zh-rCN/strings.xml",
        "<resources><string name=\"a\">A</string></resources>\n",
    );
    write(
        root,
        "app/src/main/res/values-night/colors.xml",
        "<resources/>\n",
    ); // not a locale
    write(
        root,
        "app/build/intermediates/res/values/strings.xml",
        "<resources/>\n",
    ); // ignored
    let cfg = detect(root).unwrap();
    assert_eq!(cfg.source_locale, "en");
    assert_eq!(cfg.target_locales, vec!["de", "pt-BR", "zh-CN"]);
    assert_eq!(cfg.files.len(), 1);
    let f = &cfg.files[0];
    assert_eq!(f.format, Format::Android);
    assert_eq!(
        f.path.to_string_lossy(),
        "app/src/main/res/values/strings.xml"
    );
    assert_eq!(
        f.locale_path.as_deref(),
        Some("app/src/main/res/values-{android_locale}/strings.xml")
    );
}

#[test]
fn init_detects_flutter_arb() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "pubspec.yaml", "name: app\n");
    write(
        root,
        "l10n.yaml",
        "arb-dir: lib/l10n\ntemplate-arb-file: app_en.arb\n",
    );
    write(
        root,
        "lib/l10n/app_en.arb",
        "{\"@@locale\": \"en\", \"hello\": \"Hello\"}\n",
    );
    write(
        root,
        "lib/l10n/app_es.arb",
        "{\"@@locale\": \"es\", \"hello\": \"Hola\"}\n",
    );
    let cfg = detect(root).unwrap();
    assert_eq!(cfg.source_locale, "en");
    assert_eq!(cfg.target_locales, vec!["es"]);
    assert_eq!(cfg.files.len(), 1);
    assert_eq!(cfg.files[0].format, Format::Arb);
    assert_eq!(cfg.files[0].path.to_string_lossy(), "lib/l10n/app_en.arb");
    assert_eq!(
        cfg.files[0].locale_path.as_deref(),
        Some("lib/l10n/app_{locale}.arb")
    );
}

#[test]
fn init_detects_nextjs_i18next_namespaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "package.json",
        "{\"dependencies\":{\"next\":\"15\",\"i18next\":\"25\"}}",
    );
    write(
        root,
        "public/locales/en/common.json",
        "{\"hello\":\"Hello\"}\n",
    );
    write(
        root,
        "public/locales/en/auth.json",
        "{\"login\":\"Log in\"}\n",
    );
    write(
        root,
        "public/locales/de/common.json",
        "{\"hello\":\"Hallo\"}\n",
    );
    write(root, "node_modules/x/locales/en/common.json", "{}"); // ignored
    let cfg = detect(root).unwrap();
    assert_eq!(cfg.source_locale, "en");
    assert_eq!(cfg.target_locales, vec!["de"]);
    let mut paths: Vec<(String, String)> = cfg
        .files
        .iter()
        .map(|f| {
            (
                f.path.to_string_lossy().into_owned(),
                f.locale_path.clone().unwrap(),
            )
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            (
                "public/locales/en/auth.json".to_string(),
                "public/locales/{locale}/auth.json".to_string()
            ),
            (
                "public/locales/en/common.json".to_string(),
                "public/locales/{locale}/common.json".to_string()
            ),
        ]
    );
    assert!(cfg.files.iter().all(|f| f.format == Format::Json));
}

#[test]
fn init_detects_flat_locale_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "package.json", "{}");
    write(root, "src/locales/en.json", "{\"a\":\"A\"}\n");
    write(root, "src/locales/fr.json", "{\"a\":\"A\"}\n");
    let cfg = detect(root).unwrap();
    assert_eq!(cfg.target_locales, vec!["fr"]);
    assert_eq!(cfg.files[0].path.to_string_lossy(), "src/locales/en.json");
    assert_eq!(
        cfg.files[0].locale_path.as_deref(),
        Some("src/locales/{locale}.json")
    );
}

#[test]
fn init_detects_single_source_file_in_locales_dir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "locales/en.json", "{\"a\":\"A\"}\n");
    write(root, "config/en.json", "{\"not\":\"a locale file\"}\n");
    let cfg = detect(root).unwrap();
    assert_eq!(cfg.files.len(), 1);
    assert_eq!(cfg.files[0].path.to_string_lossy(), "locales/en.json");
    assert!(cfg.target_locales.is_empty());
}

#[test]
fn init_cli_writes_config_and_refuses_to_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "App/Localizable.xcstrings", XCSTRINGS);
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
    let cfg = Config::load(root).unwrap();
    assert_eq!(cfg.target_locales, vec!["de", "fr"]);
    let again = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .arg("init")
        .output()
        .unwrap();
    assert!(
        !again.status.success(),
        "second init must refuse without --force"
    );
    let forced = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["init", "--force"])
        .output()
        .unwrap();
    assert!(forced.status.success());
}

#[test]
fn init_errors_when_nothing_found() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "README.md", "nothing here");
    assert!(detect(dir.path()).is_err());
}
