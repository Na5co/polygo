//! G3.1: placeholder parity: 100% of corpus mutations caught, 0 false positives on originals.
use polygo::check::placeholders::{compare, extract};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

fn canon(text: &str) -> Vec<String> {
    extract(text).into_iter().map(|p| p.canonical).collect()
}

#[test]
fn check_placeholders_extraction() {
    assert_eq!(canon("Hello, %@! %lld new"), ["%1$@", "%2$d"]);
    assert_eq!(canon("%2$@ before %1$@"), ["%2$@", "%1$@"]);
    assert_eq!(canon("100%% done, %.2f MB"), ["%1$f"]);
    assert_eq!(canon("%1$s of %2$s (%3$d%%)"), ["%1$s", "%2$s", "%3$d"]);
    assert_eq!(
        canon("Hi {{ name }}, see $t(footer) {count, plural, one {# item} other {# items}}"),
        ["{{name}}", "$t(footer)", "{count}", "{count}/plural"]
    );
    assert_eq!(
        canon("Costs $5 and ${amount} or $amount"),
        ["{amount}", "$amount"]
    );
    assert_eq!(canon("50% off"), Vec::<String>::new());
    assert_eq!(canon("25% anonymous, 30% better"), Vec::<String>::new());
    assert_eq!(canon("%#@count@ starred, %1$#@items@"), ["%#@…@", "%#@…@"]);
    assert_eq!(canon("{0} of {1}"), ["{0}", "{1}"]);
    // ICU bodies are recursed into; `#` is not a placeholder.
    assert_eq!(
        canon("{count, plural, one {# item for {name}} other {# items}}"),
        ["{count}", "{count}/plural", "{name}"]
    );
    assert_eq!(
        canon("{sel, select, a {{x}} other {{y}}}"),
        ["{sel}", "{sel}/select", "{x}", "{y}"]
    );
    // Python named arguments; a translated name in any script is still a token.
    assert_eq!(canon("%(count)d of %(total)s"), ["%(count)d", "%(total)s"]);
    assert_eq!(canon("50%(approx.) done"), Vec::<String>::new());
    assert_eq!(
        canon("{{ модели }} и {имя} у $nombre"),
        ["{{модели}}", "{имя}", "$nombre"]
    );
    let p = extract("Hi {{ name }}!");
    assert_eq!(
        (p[0].raw.as_str(), p[0].name.as_deref()),
        ("{{ name }}", Some("name"))
    );
    let p = extract("{0} and {see below} and %1$@ and {n, plural, one {#} other {#}}");
    assert_eq!(
        p.iter().map(|p| p.name.as_deref()).collect::<Vec<_>>(),
        [None, None, None, Some("n"), Some("n")]
    );
}

#[test]
fn check_placeholders_renamed() {
    // The most common placeholder bug of all: the translator translated the name. It is
    // named as such and comes with the repair.
    let m = compare("{{ models }}", "{{ modelli }}").unwrap();
    assert_eq!(
        m.to_string(),
        "placeholder name translated: {{models}} → {{modelli}} (names must stay as in the source)"
    );
    assert!(m.missing.is_empty() && m.extra.is_empty());
    assert_eq!(m.fix("{{ modelli }}").as_deref(), Some("{{ models }}"));
    // Spacing is the translation's; the name is the source's. Every occurrence.
    let m = compare("{{user}} and {{user}}", "{{ utente }} e {{ utente }}").unwrap();
    assert_eq!(
        m.fix("{{ utente }} e {{ utente }}").as_deref(),
        Some("{{ user }} e {{ user }}")
    );
    // Two names, paired in order of appearance.
    let m = compare(
        "Model {{name}} is now {{status}}",
        "Модел {{наме}} је сада {{статус}}",
    )
    .unwrap();
    assert_eq!(
        m.to_string(),
        "placeholder names translated: {{name}} → {{наме}}, {{status}} → {{статус}} (names must stay as in the source)"
    );
    assert_eq!(
        m.fix("Модел {{наме}} је сада {{статус}}").as_deref(),
        Some("Модел {{name}} је сада {{status}}")
    );
    // Python, Dart, ICU: the name is put back and the plural body is kept.
    let m = compare("%(count)d files", "%(anzahl)d Dateien").unwrap();
    assert_eq!(
        m.fix("%(anzahl)d Dateien").as_deref(),
        Some("%(count)d Dateien")
    );
    let m = compare("Hello $name", "Hola $nombre").unwrap();
    assert_eq!(m.fix("Hola $nombre").as_deref(), Some("Hola $name"));
    let m = compare(
        "{count, plural, one {# item} other {# items}}",
        "{nombre, plural, one {# élément} other {# éléments}}",
    )
    .unwrap();
    assert_eq!(
        m.fix("{nombre, plural, one {# élément} other {# éléments}}")
            .as_deref(),
        Some("{count, plural, one {# élément} other {# éléments}}")
    );
    // Not a rename: a different shape, a dropped placeholder next to a renamed one, a
    // positional argument. Those stay missing/unexpected, and there is no fix.
    let m = compare("Hi {{name}}", "Hallo {name}").unwrap();
    assert!(m.renamed.is_empty());
    assert_eq!(m.to_string(), "missing {{name}}; unexpected {name}");
    let m = compare("{{a}} and {{b}}", "{{x}}").unwrap();
    assert!(m.renamed.is_empty() && m.fix("{{x}}").is_none());
    let m = compare("{{a}} and {{b}}", "{{x}} y {{b}} y {{c}}").unwrap();
    assert_eq!(m.to_string(), "missing {{a}}; unexpected {{c}} {{x}}");
    assert!(compare("{0} of {1}", "{1} von {0}").is_none());
    // Python named arguments may repeat or drop a repetition, like numbered ones.
    assert!(compare("%(n)s, %(n)s", "%(n)s").is_none());
}

#[test]
fn check_placeholders_icu_structure() {
    // A case without a body, or no `other`, is malformed.
    assert!(
        compare(
            "{n, plural, one {x} other {y}}",
            "{n, plural, one  other {y}}"
        )
        .is_some()
    );
    assert!(compare("{n, plural, one {x} other {y}}", "{n, plural, one {x}}").is_some());
    // Different case sets are fine (Russian needs few/many), as long as well-formed.
    assert!(
        compare(
            "{n, plural, one {x} other {y}}",
            "{n, plural, one {a} few {b} many {c} other {d}}"
        )
        .is_none()
    );
    // A plural arg turned into a plain arg loses information.
    assert!(compare("{n, plural, one {x} other {y}}", "{n} things").is_some());
}

#[test]
fn check_placeholders_compare() {
    assert!(compare("Hello, %@!", "Hallo, %@!").is_none());
    // Numbered reordering is fine.
    assert!(compare("%@ has %lld items", "%2$lld Einträge hat %1$@").is_none());
    // Unnumbered reordering is not.
    let m = compare("%@ has %d", "%d hat %@").unwrap();
    assert_eq!(m.missing, ["%1$@", "%2$d"]);
    // Type change is caught.
    assert!(compare("%d files", "%s Dateien").is_some());
    // Dropped / added placeholder.
    assert!(compare("Hi {{name}}", "Hallo").is_some());
    assert!(compare("Hi", "Hallo {{name}}").is_some());
    // Brace style change is caught.
    assert!(compare("Hi {{name}}", "Hallo {name}").is_some());
    // Literal percent is not a placeholder.
    assert!(compare("100%% done", "100%% fertig").is_none());
    // Integer length modifiers are interchangeable for translators.
    assert!(compare("%lld items", "%d Einträge").is_none());
    // Numbered args may be repeated or dropped-as-repeat; a lost distinct arg is still caught.
    assert!(compare("%1$@ and %1$@ on %2$@", "%1$@ su %2$@ e %2$@").is_none());
    assert!(compare("%1$@ on %2$@", "%1$@ only").is_some());
}

#[derive(Deserialize)]
struct Mutation {
    key: String,
    locale: String,
    source: String,
    translation: String,
    kind: String,
}

fn corpus_dirs() -> Vec<PathBuf> {
    ["xcstrings", "android", "json"]
        .iter()
        .map(|d| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/corpus")
                .join(d)
        })
        .collect()
}

#[test]
fn check_placeholders() {
    // 1. Every mutation in every *.mutations.json must be caught.
    let mut total = 0;
    let mut missed = Vec::new();
    for dir in corpus_dirs() {
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if !path.to_string_lossy().ends_with(".mutations.json") {
                continue;
            }
            let muts: Vec<Mutation> =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            for m in muts {
                total += 1;
                if compare(&m.source, &m.translation).is_none() {
                    missed.push(format!(
                        "{}: {} [{}] {} → {:?} ⇒ {:?}",
                        path.file_name().unwrap().to_string_lossy(),
                        m.key,
                        m.locale,
                        m.kind,
                        m.source,
                        m.translation
                    ));
                }
            }
        }
    }
    assert!(total >= 100, "need a real mutation corpus, found {total}");
    assert!(
        missed.is_empty(),
        "missed {}/{} mutations:\n{}",
        missed.len(),
        total,
        missed.join("\n")
    );

    // 2. Zero false positives on the real translations in the xcstrings corpus,
    //    except those listed in known_bugs.json (genuine upstream bugs we verified).
    let known: Vec<String> = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/corpus/xcstrings/known_bugs.json"),
        )
        .unwrap_or_else(|_| "[]".into()),
    )
    .unwrap();
    let mut pairs = 0;
    let mut flagged = Vec::new();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/xcstrings");
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "xcstrings") {
            continue;
        }
        let doc = polygo::formats::xcstrings::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        let src_lang = doc.root["sourceLanguage"].as_str().unwrap_or("en");
        for u in polygo::formats::xcstrings::units(&doc, src_lang) {
            for (locale, t) in &u.translations {
                pairs += 1;
                if let Some(m) = compare(&u.source, t) {
                    let id = format!(
                        "{}:{}:{}",
                        path.file_name().unwrap().to_string_lossy(),
                        u.key,
                        locale
                    );
                    if !known.contains(&id) {
                        flagged.push(format!("{id}: {m}\n    {:?}\n    {:?}", u.source, t));
                    }
                }
            }
        }
    }
    assert!(
        pairs >= 5000,
        "expected thousands of real pairs, got {pairs}"
    );
    assert!(
        flagged.is_empty(),
        "false positives on real translations ({}/{pairs}):\n{}",
        flagged.len(),
        flagged.join("\n")
    );
}
