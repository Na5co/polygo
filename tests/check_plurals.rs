//! G3.2: CLDR plural completeness per locale, across xcstrings variations, Android
//! <plurals>, ICU inline plurals and i18next `_one/_other` key suffixes.
use polygo::check::plurals::{
    android_plurals, i18next_groups, i18next_plural_groups, icu_cases, missing, required,
    xcstrings_plurals,
};
use std::fs;
use std::path::PathBuf;

fn corpus(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(sub)
}

#[test]
fn check_plurals_required_table() {
    assert_eq!(required("en"), ["one", "other"]);
    assert_eq!(required("en-GB"), ["one", "other"]);
    assert_eq!(required("ja"), ["other"]);
    assert_eq!(required("zh-Hans"), ["other"]);
    assert_eq!(required("ru"), ["one", "few", "many", "other"]);
    assert_eq!(required("pl"), ["one", "few", "many", "other"]);
    assert_eq!(required("cs"), ["one", "few", "many", "other"]);
    assert_eq!(required("hr"), ["one", "few", "other"]);
    assert_eq!(
        required("ar"),
        ["zero", "one", "two", "few", "many", "other"]
    );
    assert_eq!(required("sl"), ["one", "two", "few", "other"]);
    assert_eq!(required("lv"), ["zero", "one", "other"]);
    assert_eq!(required("he"), ["one", "two", "other"]);
    // French/Spanish/Italian/Portuguese: `many` (millions) is optional, not required.
    assert_eq!(required("fr"), ["one", "other"]);
    assert_eq!(required("pt-BR"), ["one", "other"]);
    // Unknown locales fall back to the most common shape.
    assert_eq!(required("xx"), ["one", "other"]);
}

#[test]
fn check_plurals_missing_and_extras() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert!(missing("ru", &s(&["one", "few", "many", "other"])).is_empty());
    assert_eq!(missing("ru", &s(&["one", "other"])), ["few", "many"]);
    assert_eq!(missing("be", &s(&["few", "one", "other"])), ["many"]);
    // Extras (`zero`, `=0`, an unneeded `one` in Japanese) are fine.
    assert!(missing("en", &s(&["zero", "one", "other"])).is_empty());
    assert!(missing("ja", &s(&["one", "other"])).is_empty());
    assert!(missing("fr", &s(&["one", "many", "other"])).is_empty());
    assert_eq!(
        missing("ar", &s(&["one", "other"])),
        ["zero", "two", "few", "many"]
    );
}

#[test]
fn check_plurals_icu_and_i18next_shapes() {
    let cases =
        icu_cases("You have {count, plural, =0 {no items} one {# item} other {# items}} in {cart}");
    assert_eq!(
        cases,
        vec![(
            "count".to_string(),
            vec!["=0".to_string(), "one".to_string(), "other".to_string()]
        )]
    );
    // selectordinal has different categories and is left alone; select is not a plural.
    assert!(
        icu_cases("{n, selectordinal, one {#st} other {#th}} {g, select, m {he} other {they}}")
            .is_empty()
    );

    let keys: Vec<String> = [
        "item_one",
        "item_other",
        "day_one",
        "day_other",
        "day_few",
        "plain",
        "x_zero",
    ]
    .iter()
    .map(|k| k.to_string())
    .collect();
    let groups = i18next_groups(&keys);
    assert_eq!(groups["item"], ["one", "other"]);
    assert_eq!(groups["day"], ["one", "other", "few"]);
    assert_eq!(groups["x"], ["zero"]);
    assert!(!groups.contains_key("plain"));
    // Source side: only groups with `_other` and a second form are plurals. `step_one` +
    // `step_two` are wizard steps, `x_zero` alone is nothing, `items_other` alone is an
    // opt-out.
    let strict = i18next_plural_groups(
        &[
            "item_one",
            "item_other",
            "step_one",
            "step_two",
            "x_zero",
            "items_other",
        ]
        .map(String::from),
    );
    assert_eq!(strict.keys().collect::<Vec<_>>(), ["item"]);
}

#[test]
fn check_plurals() {
    // Loop: every locale's plural variations are complete.
    let loop_doc = polygo::formats::xcstrings::parse(
        &fs::read_to_string(corpus("xcstrings/loop.xcstrings")).unwrap(),
    )
    .unwrap();
    let mut problems = Vec::new();
    for (key, locale, cats) in xcstrings_plurals(&loop_doc) {
        let m = missing(&locale, &cats);
        if !m.is_empty() {
            problems.push(format!("{key} [{locale}] missing {m:?}"));
        }
    }
    assert!(problems.is_empty(), "{problems:?}");

    // IceCubes: the only incomplete sets are the known Belarusian ones missing `many`.
    let ice = polygo::formats::xcstrings::parse(
        &fs::read_to_string(corpus("xcstrings/icecubes.xcstrings")).unwrap(),
    )
    .unwrap();
    let forms = xcstrings_plurals(&ice);
    assert!(
        forms.len() > 300,
        "expected variations + substitutions, got {}",
        forms.len()
    );
    let bad: Vec<(String, String, Vec<&str>)> = forms
        .iter()
        .filter_map(|(k, l, c)| {
            let m = missing(l, c);
            (!m.is_empty()).then(|| (k.clone(), l.clone(), m))
        })
        .collect();
    // IceCubes ships be/pl/uk plurals with only one/other: 44 genuine gaps (see KNOWN_BUGS.md).
    assert!(
        bad.len() >= 40,
        "expected the known Slavic gaps, got {}: {bad:?}",
        bad.len()
    );
    assert!(
        bad.iter()
            .all(|(_, l, m)| ["be", "pl", "uk"].contains(&l.as_str())
                && m.iter().all(|c| ["few", "many"].contains(c))),
        "unexpected plural problems: {bad:?}"
    );

    // Android: English source plurals all have one+other.
    for f in [
        "antennapod.xml",
        "newpipe.xml",
        "fdroid.xml",
        "k9mail-legacy.xml",
        "duckduckgo.xml",
    ] {
        let doc = polygo::formats::android::parse(
            &fs::read_to_string(corpus("android").join(f)).unwrap(),
        )
        .unwrap();
        let forms = android_plurals(&doc);
        assert!(!forms.is_empty(), "{f}: no plurals found");
        for (name, cats) in forms {
            assert!(missing("en", &cats).is_empty(), "{f}: {name} {cats:?}");
        }
    }

    // i18next: grafana's English `_one/_other` groups are complete; immich's ICU plurals too.
    let grafana =
        polygo::formats::json::parse(&fs::read_to_string(corpus("json/grafana.json")).unwrap())
            .unwrap();
    let keys: Vec<String> = grafana.entries.iter().map(|e| e.key()).collect();
    let groups = i18next_groups(&keys);
    assert!(groups.len() >= 20, "{}", groups.len());
    for (base, cats) in &groups {
        assert!(missing("en", cats).is_empty(), "{base}: {cats:?}");
    }
    let immich =
        polygo::formats::json::parse(&fs::read_to_string(corpus("json/immich.json")).unwrap())
            .unwrap();
    let mut icu_count = 0;
    for e in &immich.entries {
        for (arg, cats) in icu_cases(&e.text()) {
            icu_count += 1;
            assert!(
                missing("en", &cats).is_empty(),
                "{}: {arg} {cats:?}",
                e.key()
            );
        }
    }
    assert!(icu_count >= 50, "{icu_count}");
}

#[test]
fn zero_one_two_forms_may_omit_the_count() {
    // Arabic dual "يومان" has no number in it; Apple and Android both allow that.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("App")).unwrap();
    std::fs::write(
        root.join("App/Localizable.xcstrings"),
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "%lld days" : {
      "localizations" : {
        "ar" : {
          "variations" : {
            "plural" : {
              "one" : { "stringUnit" : { "state" : "translated", "value" : "يوم واحد" } },
              "two" : { "stringUnit" : { "state" : "translated", "value" : "يومان" } },
              "few" : { "stringUnit" : { "state" : "translated", "value" : "%lld أيام" } },
              "many" : { "stringUnit" : { "state" : "translated", "value" : "%lld يومًا" } },
              "other" : { "stringUnit" : { "state" : "translated", "value" : "يوم" } },
              "zero" : { "stringUnit" : { "state" : "translated", "value" : "لا أيام" } }
            }
          }
        },
        "en" : {
          "variations" : {
            "plural" : {
              "one" : { "stringUnit" : { "state" : "translated", "value" : "%lld day" } },
              "other" : { "stringUnit" : { "state" : "translated", "value" : "%lld days" } }
            }
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"ar\"]\n\n[[files]]\nformat = \"xcstrings\"\npath = \"App/Localizable.xcstrings\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(["check", "--json"])
        .output()
        .unwrap();
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let errs: Vec<String> = r["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["severity"] == "error")
        .map(|f| f["key"].as_str().unwrap().to_string())
        .collect();
    // Only `other` (which really did drop the count) is an error.
    assert_eq!(errs, vec!["%lld days#plural.other".to_string()], "{errs:?}");
}
