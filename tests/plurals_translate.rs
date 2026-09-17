//! Plural forms are translated, one unit per CLDR category the target locale needs,
//! for .xcstrings variations (top-level and substitutions), Android <plurals> and
//! gettext msgid_plural — written into the file's native structure.
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
