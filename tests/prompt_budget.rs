//! G4.3: context is attached to requests within a hard token budget; --no-context turns it off.
use polygo::context::assemble::{Budget, attach};
use polygo::context::usage::Usage;
use polygo::provider::Request;
use std::fs;
use std::path::Path;
use std::process::Command;

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
fn prompt_budget_respected() {
    let huge_snippet: String = (0..400)
        .map(|i| format!("    let line{i} = compute({i}) // padding padding padding\n"))
        .collect();
    let usage = Usage {
        path: "App/Views/BigView.swift".into(),
        line: 200,
        ident: Some("BigView".into()),
        snippet: huge_snippet.clone(),
    };
    let examples: Vec<(String, String)> = (0..20)
        .map(|i| {
            (
                format!("Example source number {i} with some words"),
                format!("Beispielquelle Nummer {i} mit ein paar Wörtern"),
            )
        })
        .collect();
    let budget = Budget { tokens: 300 };
    let mut r = req("k", "Save changes");
    attach(&mut r, Some(&usage), &examples, &budget);

    let ctx = r.context.clone().unwrap();
    assert!(ctx.contains("App/Views/BigView.swift"), "{ctx}");
    assert!(ctx.contains("BigView"), "{ctx}");
    assert!(ctx.len() < huge_snippet.len(), "snippet must be truncated");
    let total_chars = ctx.len()
        + r.examples
            .iter()
            .map(|(s, t)| s.len() + t.len() + 4)
            .sum::<usize>();
    assert!(
        total_chars <= budget.tokens * 4,
        "{total_chars} chars exceeds {} tokens",
        budget.tokens
    );
    assert!(r.examples.len() <= 3, "{}", r.examples.len());
    assert!(
        !r.examples.is_empty(),
        "examples should survive a 300-token budget"
    );

    // A tiny budget drops the snippet before it drops the file/ident line, and drops examples last-to-first.
    let mut r = req("k", "Save changes");
    attach(&mut r, Some(&usage), &examples, &Budget { tokens: 30 });
    let ctx = r.context.clone().unwrap_or_default();
    assert!(ctx.contains("BigView.swift"), "{ctx}");
    assert!(
        ctx.len()
            + r.examples
                .iter()
                .map(|(s, t)| s.len() + t.len() + 4)
                .sum::<usize>()
            <= 120
    );

    // No usage and no examples → nothing attached.
    let mut r = req("k", "Save changes");
    attach(&mut r, None, &[], &budget);
    assert!(r.context.is_none() && r.examples.is_empty());
}

fn project(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"cta\": \"Save\",\n  \"other\": \"Save changes\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("locales/de.json"),
        "{\n  \"other\": \"Änderungen speichern\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/Form.tsx"),
        "export function Form() {\n  const { t } = useTranslation();\n  return <button type=\"submit\">{t('cta')}</button>;\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
}

#[test]
fn prompt_budget_respected_end_to_end_and_no_context_flag() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let dump = dir.path().join("prompts.txt");
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(dir.path())
        .env("POLYGO_MOCK_DUMP", &dump)
        .arg("translate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let prompts = fs::read_to_string(&dump).unwrap();
    assert!(
        prompts.contains("src/Form.tsx"),
        "usage context missing:\n{prompts}"
    );
    assert!(prompts.contains("Form"), "{prompts}");
    assert!(
        prompts.contains("Save changes → Änderungen speichern"),
        "few-shot missing:\n{prompts}"
    );

    let dir = tempfile::tempdir().unwrap();
    project(dir.path());
    let dump = dir.path().join("prompts.txt");
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(dir.path())
        .env("POLYGO_MOCK_DUMP", &dump)
        .args(["translate", "--no-context"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let prompts = fs::read_to_string(&dump).unwrap();
    assert!(
        !prompts.contains("src/Form.tsx") && !prompts.contains("used in"),
        "{prompts}"
    );
    assert!(!prompts.contains("similar existing"), "{prompts}");
}
