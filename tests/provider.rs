//! G2.1: provider trait — deterministic mock, response parsing, live Ollama smoke (feature `live`).
use polygo::provider::{Ctx, Provider, Request, mock::Mock, parse_translations_json};

fn ctx(target: &str) -> Ctx {
    Ctx {
        source_locale: "en".into(),
        target_locale: target.into(),
        glossary: vec![],
        do_not_translate: vec![],
        format_hint: None,
    }
}

fn req(key: &str, source: &str) -> Request {
    Request {
        key: key.into(),
        source: source.into(),
        comment: None,
        context: None,
        examples: vec![],
    }
}

#[test]
fn provider_mock_is_deterministic_and_reversible() {
    let p = Mock::default();
    let batch = vec![req("save", "Save"), req("hello", "Hello, %@!")];
    let a = p.translate(&batch, &ctx("de")).unwrap();
    let b = p.translate(&batch, &ctx("de")).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len(), 2);
    assert_eq!(a[0].key, "save");
    assert_eq!(
        Mock::reverse(&a[0].text),
        Some(("de".to_string(), "Save".to_string()))
    );
    assert_eq!(
        Mock::reverse(&a[1].text),
        Some(("de".to_string(), "Hello, %@!".to_string()))
    );
    assert_ne!(p.translate(&batch, &ctx("fr")).unwrap()[0].text, a[0].text);
    assert_eq!(p.name(), "mock");
}

#[test]
fn provider_mock_handles_empty_batch_and_preserves_order() {
    let p = Mock::default();
    assert!(p.translate(&[], &ctx("de")).unwrap().is_empty());
    let batch: Vec<Request> = (0..50)
        .map(|i| req(&format!("k{i}"), &format!("Text {i}")))
        .collect();
    let out = p.translate(&batch, &ctx("ja")).unwrap();
    assert_eq!(
        out.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
        batch.iter().map(|r| r.key.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn provider_mock_can_fail_on_demand_for_resume_tests() {
    let p = Mock {
        fail_on_key: Some("boom".into()),
    };
    let batch = vec![req("ok", "Fine"), req("boom", "Explode")];
    assert!(p.translate(&batch, &ctx("de")).is_err());
    assert!(p.translate(&batch[..1], &ctx("de")).is_ok());
}

#[test]
fn provider_response_parsing_is_forgiving() {
    let wanted = ["save", "cancel"];
    // Plain JSON.
    let t = parse_translations_json(r#"{"translations":[{"key":"save","text":"Speichern"},{"key":"cancel","text":"Abbrechen"}]}"#, &wanted).unwrap();
    assert_eq!(t.len(), 2);
    // Fenced and chatty.
    let t = parse_translations_json("Sure! ```json\n{\"translations\":[{\"key\":\"save\",\"text\":\"Speichern\"},{\"key\":\"cancel\",\"text\":\"Abbrechen\"}]}\n```", &wanted).unwrap();
    assert_eq!(t[1].text, "Abbrechen");
    // Object form keyed by key.
    let t =
        parse_translations_json(r#"{"save":"Speichern","cancel":"Abbrechen"}"#, &wanted).unwrap();
    assert_eq!(t[0].key, "save");
    // Missing key is an error naming the key.
    let err = parse_translations_json(
        r#"{"translations":[{"key":"save","text":"Speichern"}]}"#,
        &wanted,
    )
    .unwrap_err();
    assert!(err.to_string().contains("cancel"), "{err}");
    // Garbage is an error, not a panic.
    assert!(parse_translations_json("no json here", &wanted).is_err());
}

#[cfg(feature = "live")]
#[test]
fn provider_ollama_smoke() {
    use polygo::provider::ollama::Ollama;
    let p = Ollama::new(None, "qwen3:8b");
    let batch = vec![
        req("greeting", "Hello, %@! You have %lld new messages."),
        req("save", "Save"),
    ];
    let out = p.translate(&batch, &ctx("de")).unwrap();
    assert_eq!(out.len(), 2);
    assert!(
        out[0].text.contains("%@") && out[0].text.contains("%lld"),
        "{}",
        out[0].text
    );
    assert!(!out[1].text.is_empty());
}
