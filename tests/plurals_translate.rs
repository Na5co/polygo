//! Plural forms are translated, one unit per CLDR category the target locale needs,
//! for .xcstrings variations (top-level and substitutions), Android <plurals> and
//! gettext msgid_plural: written into the file's native structure.
use std::fs;
use std::path::Path;
use std::process::Command;

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .args(args)
        .env("POLYGO_NO_BACKOFF", "1")
        .output()
        .unwrap()
}

fn ok(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn xcstrings_plural_variations_and_substitutions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("App")).unwrap();
    let catalog = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "%lld photos" : {
      "comment" : "Album subtitle",
      "localizations" : {
        "en" : {
          "variations" : {
            "plural" : {
              "one" : {
                "stringUnit" : {
                  "state" : "translated",
                  "value" : "%lld photo"
                }
              },
              "other" : {
                "stringUnit" : {
                  "state" : "translated",
                  "value" : "%lld photos"
                }
              }
            }
          }
        }
      }
    },
    "Open" : {
      "localizations" : {
        "de" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Öffnen"
          }
        }
      }
    },
    "followers %lld %@" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "%#@followers@"
          },
          "substitutions" : {
            "followers" : {
              "formatSpecifier" : "lld",
              "variations" : {
                "plural" : {
                  "one" : {
                    "stringUnit" : {
                      "state" : "translated",
                      "value" : "%2$@ follower"
                    }
                  },
                  "other" : {
                    "stringUnit" : {
                      "state" : "translated",
                      "value" : "%2$@ followers"
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
    fs::write(root.join("App/Localizable.xcstrings"), catalog).unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\", \"pl\", \"ja\"]\n\n[[files]]\nformat = \"xcstrings\"\npath = \"App/Localizable.xcstrings\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();

    // Plan: German needs one+other, Polish one/few/many/other, Japanese only other.
    let out = run(root, &["translate", "--dry-run"]);
    ok(&out);
    let plan = String::from_utf8_lossy(&out.stdout);
    assert!(plan.contains("%lld photos#plural.one"), "{plan}");
    assert!(plan.contains("%lld photos#plural.few"), "{plan}");
    assert!(
        plan.contains("followers %lld %@#followers#plural.many"),
        "{plan}"
    );
    let de: Vec<&str> = plan
        .lines()
        .skip_while(|l| !l.starts_with("de "))
        .take_while(|l| l.starts_with("de ") || l.starts_with("  "))
        .collect();
    assert!(!de.iter().any(|l| l.contains("#plural.few")), "{de:?}");
    let ja: Vec<&str> = plan
        .lines()
        .skip_while(|l| !l.starts_with("ja "))
        .take_while(|l| l.starts_with("ja ") || l.starts_with("  "))
        .collect();
    assert!(
        ja.iter().any(|l| l.contains("#plural.other"))
            && !ja.iter().any(|l| l.contains("#plural.one")),
        "{ja:?}"
    );

    ok(&run(root, &["translate"]));
    let text = fs::read_to_string(root.join("App/Localizable.xcstrings")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let pl = &doc["strings"]["%lld photos"]["localizations"]["pl"]["variations"]["plural"];
    for cat in ["one", "few", "many", "other"] {
        let v = pl[cat]["stringUnit"]["value"].as_str().unwrap();
        assert!(v.starts_with("⟦pl⟧ %lld photo"), "{cat}: {v}");
    }
    assert_eq!(pl["few"]["stringUnit"]["state"], "translated");
    let de = &doc["strings"]["%lld photos"]["localizations"]["de"]["variations"]["plural"];
    assert!(de.get("few").is_none());
    assert_eq!(de["one"]["stringUnit"]["value"], "⟦de⟧ %lld photo");
    let ja = &doc["strings"]["%lld photos"]["localizations"]["ja"]["variations"]["plural"];
    assert!(ja.get("one").is_none() && ja.get("other").is_some());
    // Substitution: metadata mirrored, outer stringUnit translated as an ordinary string.
    let sub =
        &doc["strings"]["followers %lld %@"]["localizations"]["pl"]["substitutions"]["followers"];
    assert_eq!(sub["formatSpecifier"], "lld");
    assert_eq!(
        sub["variations"]["plural"]["many"]["stringUnit"]["value"],
        "⟦pl⟧ %2$@ followers"
    );
    assert_eq!(
        doc["strings"]["followers %lld %@"]["localizations"]["pl"]["stringUnit"]["value"],
        "⟦pl⟧ %#@followers@"
    );
    // Xcode style survives: 2-space indent, `"key" : value`, sorted plural keys.
    assert!(text.contains("\"few\" : {\n"), "{text}");
    let few = text.find("\"few\"").unwrap();
    let many = text.find("\"many\"").unwrap();
    assert!(few < many);
    // Round-trips byte-stable and check passes (all categories present, placeholders kept).
    let reparsed = polygo::formats::xcstrings::parse(&text).unwrap();
    assert_eq!(polygo::formats::xcstrings::serialize(&reparsed), text);
    ok(&run(root, &["check"]));
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
    // Status counts plural forms only for the locales that need them.
    let out = run(root, &["status", "--json"]);
    let st: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // %lld photos×2 + followers outer + followers×2; "Open" pre-existed → edited.
    assert_eq!(st["locales"]["de"]["up_to_date"], 5);
    assert_eq!(st["locales"]["de"]["edited"], 1);
    assert_eq!(st["locales"]["ja"]["up_to_date"], 4);
    assert_eq!(st["locales"]["pl"]["up_to_date"], 10);
}

#[test]
fn android_plurals_written_per_quantity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("res/values")).unwrap();
    fs::create_dir_all(root.join("res/values-ru")).unwrap();
    fs::write(
        root.join("res/values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"app_name\" translatable=\"false\">Demo</string>\n    <!-- Toast after import -->\n    <plurals name=\"imported\">\n        <item quantity=\"one\">Imported %d file</item>\n        <item quantity=\"other\">Imported %d files</item>\n    </plurals>\n    <string name=\"open\">Open</string>\n</resources>\n",
    )
    .unwrap();
    // Russian already has `one` from a human; the rest must be filled in.
    fs::write(
        root.join("res/values-ru/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <plurals name=\"imported\">\n        <item quantity=\"one\">Импортирован %d файл</item>\n    </plurals>\n</resources>\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"ru\", \"de\"]\n\n[[files]]\nformat = \"android\"\npath = \"res/values/strings.xml\"\nlocale_path = \"res/values-{android_locale}/strings.xml\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    ok(&run(root, &["translate"]));
    let ru = fs::read_to_string(root.join("res/values-ru/strings.xml")).unwrap();
    assert!(
        ru.contains("<item quantity=\"one\">Импортирован %d файл</item>"),
        "human kept:\n{ru}"
    );
    for q in ["few", "many", "other"] {
        assert!(
            ru.contains(&format!(
                "        <item quantity=\"{q}\">⟦ru⟧ Imported %d files</item>"
            )),
            "{q}:\n{ru}"
        );
    }
    assert!(
        ru.contains("<string name=\"open\">⟦ru⟧ Open</string>"),
        "{ru}"
    );
    let de = fs::read_to_string(root.join("res/values-de/strings.xml")).unwrap();
    assert!(
        de.contains("    <plurals name=\"imported\">\n        <item quantity=\"one\">⟦de⟧ Imported %d file</item>\n        <item quantity=\"other\">⟦de⟧ Imported %d files</item>\n    </plurals>\n"),
        "{de}"
    );
    assert!(!de.contains("quantity=\"few\""));
    let parsed = polygo::formats::android::parse(&ru).unwrap();
    assert_eq!(polygo::formats::android::serialize(&parsed), ru);
    ok(&run(root, &["check"]));
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
}

#[test]
fn po_msgid_plural_written_into_msgstr_slots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locale/en/LC_MESSAGES")).unwrap();
    fs::create_dir_all(root.join("locale/ru/LC_MESSAGES")).unwrap();
    fs::write(
        root.join("locale/en/LC_MESSAGES/app.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: en\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\n#: views.py:1\nmsgid \"Save\"\nmsgstr \"\"\n\n#. Count of files\nmsgid \"%(n)d file\"\nmsgid_plural \"%(n)d files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n",
    )
    .unwrap();
    // Existing Russian file with 3 slots, first already translated by a human.
    fs::write(
        root.join("locale/ru/LC_MESSAGES/app.po"),
        "msgid \"\"\nmsgstr \"\"\n\"Language: ru\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Plural-Forms: nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);\\n\"\n\n#. Count of files\nmsgid \"%(n)d file\"\nmsgid_plural \"%(n)d files\"\nmsgstr[0] \"%(n)d файл\"\nmsgstr[1] \"\"\nmsgstr[2] \"\"\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"ru\", \"de\"]\n\n[[files]]\nformat = \"po\"\npath = \"locale/en/LC_MESSAGES/app.po\"\nlocale_path = \"locale/{locale}/LC_MESSAGES/app.po\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = run(root, &["translate", "--dry-run"]);
    ok(&out);
    let plan = String::from_utf8_lossy(&out.stdout);
    assert!(
        plan.contains("%(n)d file#plural.few") && plan.contains("%(n)d file#plural.many"),
        "{plan}"
    );
    let ru: Vec<&str> = plan
        .lines()
        .skip_while(|l| !l.starts_with("ru "))
        .take_while(|l| l.starts_with("ru ") || l.starts_with("  "))
        .collect();
    // gettext Russian has three slots (one/few/many): no `other`, and `one` is human-made.
    assert!(!ru.iter().any(|l| l.contains("#plural.other")), "{ru:?}");
    assert!(!ru.iter().any(|l| l.contains("#plural.one")), "{ru:?}");

    ok(&run(root, &["translate"]));
    let ru = fs::read_to_string(root.join("locale/ru/LC_MESSAGES/app.po")).unwrap();
    assert!(ru.contains("msgstr[0] \"%(n)d файл\"\nmsgstr[1] \"⟦ru⟧ %(n)d files\"\nmsgstr[2] \"⟦ru⟧ %(n)d files\"\n"), "{ru}");
    assert!(
        ru.contains("msgid \"Save\"\nmsgstr \"⟦ru⟧ Save\"\n"),
        "{ru}"
    );
    let de = fs::read_to_string(root.join("locale/de/LC_MESSAGES/app.po")).unwrap();
    assert!(
        de.contains("\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\""),
        "{de}"
    );
    assert!(
        de.contains("#. Count of files\nmsgid \"%(n)d file\"\nmsgid_plural \"%(n)d files\"\nmsgstr[0] \"⟦de⟧ %(n)d file\"\nmsgstr[1] \"⟦de⟧ %(n)d files\"\n"),
        "{de}"
    );
    let parsed = polygo::formats::po::parse(&ru).unwrap();
    assert_eq!(polygo::formats::po::serialize(&parsed), ru);
    ok(&run(root, &["check"]));
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
}

#[test]
fn i18next_plural_suffixes_written_per_cldr_category() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"title\": \"Photos\",\n  \"album\": {\n    \"photos_one\": \"{{count}} photo\",\n    \"photos_other\": \"{{count}} photos\",\n    \"subtitle\": \"Shared album\"\n  },\n  \"step_one\": \"First step\",\n  \"items_other\": \"{{count}} item(s)\"\n}\n",
    )
    .unwrap();
    // Polish already has `one` from a human; few/many/other must be filled in.
    fs::write(
        root.join("locales/pl.json"),
        "{\n  \"album\": {\n    \"photos_one\": \"{{count}} zdjęcie\"\n  }\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"pl\", \"de\", \"ja\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = run(root, &["translate", "--dry-run"]);
    ok(&out);
    let plan = String::from_utf8_lossy(&out.stdout);
    assert!(plan.contains("album.photos#plural.few"), "{plan}");
    assert!(!plan.contains("album.photos_one"), "{plan}");
    // `step_one` has no `_other` sibling: an ordinary key. `items_other` alone is an
    // opt-out (i18next uses it for every count): also ordinary.
    assert!(plan.contains("  step_one\n"), "{plan}");
    assert!(plan.contains("  items_other\n"), "{plan}");

    ok(&run(root, &["translate"]));
    let pl = fs::read_to_string(root.join("locales/pl.json")).unwrap();
    assert_eq!(
        pl,
        "{\n  \"title\": \"⟦pl⟧ Photos\",\n  \"album\": {\n    \"photos_one\": \"{{count}} zdjęcie\",\n    \"photos_few\": \"⟦pl⟧ {{count}} photos\",\n    \"photos_many\": \"⟦pl⟧ {{count}} photos\",\n    \"photos_other\": \"⟦pl⟧ {{count}} photos\",\n    \"subtitle\": \"⟦pl⟧ Shared album\"\n  },\n  \"step_one\": \"⟦pl⟧ First step\",\n  \"items_other\": \"⟦pl⟧ {{count}} item(s)\"\n}\n"
    );
    let de = fs::read_to_string(root.join("locales/de.json")).unwrap();
    assert!(
        de.contains("\"photos_one\": \"⟦de⟧ {{count}} photo\""),
        "{de}"
    );
    assert!(
        de.contains("\"photos_other\": \"⟦de⟧ {{count}} photos\""),
        "{de}"
    );
    assert!(!de.contains("photos_few"), "{de}");
    // Japanese has no plural distinction: only `_other`.
    let ja = fs::read_to_string(root.join("locales/ja.json")).unwrap();
    assert!(
        ja.contains("\"photos_other\": \"⟦ja⟧ {{count}} photos\""),
        "{ja}"
    );
    assert!(!ja.contains("photos_one"), "{ja}");

    ok(&run(root, &["check"]));
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
    let out = run(root, &["status"]);
    let status = String::from_utf8_lossy(&out.stdout);
    assert!(
        status.contains("pl       new: 0  stale: 0  untranslated: 0  edited: 1  up-to-date: 7"),
        "{status}"
    );
}

#[test]
fn android_string_arrays_translated_per_item() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("res/values")).unwrap();
    fs::create_dir_all(root.join("res/values-ru")).unwrap();
    fs::create_dir_all(root.join("res/values-fr")).unwrap();
    fs::write(
        root.join("res/values/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"open\">Open</string>\n    <!-- Sort menu, same order as the spinner -->\n    <string-array name=\"sort_modes\">\n        <item>Newest first</item>\n        <item>Oldest first</item>\n        <item>By name</item>\n    </string-array>\n    <string-array name=\"country_codes\" translatable=\"false\">\n        <item>DE</item>\n        <item>FR</item>\n    </string-array>\n</resources>\n",
    )
    .unwrap();
    // Russian: a human already translated the whole array.
    fs::write(
        root.join("res/values-ru/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string-array name=\"sort_modes\">\n        <item>Сначала новые</item>\n        <item>Сначала старые</item>\n        <item>По имени</item>\n    </string-array>\n</resources>\n",
    )
    .unwrap();
    // French: only the first two items exist; the third must be appended, not lost.
    fs::write(
        root.join("res/values-fr/strings.xml"),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string-array name=\"sort_modes\">\n        <item>Plus récents d\\'abord</item>\n        <item>Plus anciens d\\'abord</item>\n    </string-array>\n</resources>\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"ru\", \"fr\", \"de\"]\n\n[[files]]\nformat = \"android\"\npath = \"res/values/strings.xml\"\nlocale_path = \"res/values-{android_locale}/strings.xml\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
    let out = run(root, &["translate", "--dry-run"]);
    ok(&out);
    let plan = String::from_utf8_lossy(&out.stdout);
    assert!(plan.contains("sort_modes#array.0"), "{plan}");
    assert!(!plan.contains("country_codes"), "{plan}");
    // Russian is fully human-translated: only `open` is planned.
    assert!(plan.contains("ru       1 to translate"), "{plan}");
    assert!(plan.contains("fr       2 to translate"), "{plan}");
    assert!(plan.contains("de       4 to translate"), "{plan}");

    ok(&run(root, &["translate"]));
    let ru = fs::read_to_string(root.join("res/values-ru/strings.xml")).unwrap();
    assert!(ru.contains("<item>Сначала новые</item>"), "{ru}");
    assert!(!ru.contains("⟦ru⟧ Newest"), "{ru}");
    let fr = fs::read_to_string(root.join("res/values-fr/strings.xml")).unwrap();
    assert!(
        fr.contains("        <item>Plus anciens d\\'abord</item>\n        <item>⟦fr⟧ By name</item>\n    </string-array>\n"),
        "{fr}"
    );
    let de = fs::read_to_string(root.join("res/values-de/strings.xml")).unwrap();
    assert!(
        de.contains("    <string-array name=\"sort_modes\">\n        <item>⟦de⟧ Newest first</item>\n        <item>⟦de⟧ Oldest first</item>\n        <item>⟦de⟧ By name</item>\n    </string-array>\n"),
        "{de}"
    );
    assert!(!de.contains("country_codes"), "{de}");
    for f in [&ru, &fr, &de] {
        let parsed = polygo::formats::android::parse(f).unwrap();
        assert_eq!(polygo::formats::android::serialize(&parsed), **f);
    }
    ok(&run(root, &["check"]));
    let out = run(root, &["translate"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to translate"));
    let out = run(root, &["status"]);
    let status = String::from_utf8_lossy(&out.stdout);
    assert!(status.contains("4 units"), "{status}");
    assert!(
        status.contains("ru       new: 0  stale: 0  untranslated: 0  edited: 3  up-to-date: 1"),
        "{status}"
    );
}
