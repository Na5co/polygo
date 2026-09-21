//! `polygo check --sarif`: SARIF 2.1.0 for GitHub code scanning (and any other SARIF
//! consumer). Uploaded with `github/codeql-action/upload-sarif`, findings show in the
//! Security tab and on pull requests with history: new, fixed, still open.

use crate::check::run::Report;
use serde_json::{Value, json};

/// What each code means, once, so the rule table reads well in the Security tab.
pub const RULES: &[(&str, &str, &str)] = &[
    (
        "placeholders",
        "Placeholder mismatch",
        "A format placeholder (%1$@, {{name}}, {count, plural…}) is missing, added, retyped or reordered in the translation.",
    ),
    (
        "plural",
        "Missing plural form",
        "The locale needs a CLDR plural category the translation does not provide (few/many for Polish, six for Arabic).",
    ),
    (
        "markup",
        "Markup mismatch",
        "A tag the source has is missing from the translation, or the translation adds one.",
    ),
    (
        "escape",
        "Android escape",
        "An unescaped apostrophe or quote, or a leading @/? that aapt reads as a resource reference.",
    ),
    (
        "array",
        "Array length",
        "An Android <string-array> has a different number of items than the source.",
    ),
    (
        "duplicate",
        "Duplicate key",
        "The same key appears more than once in one file; the last one wins silently.",
    ),
    (
        "glossary",
        "Glossary",
        "A glossary term was translated, or not rendered as required.",
    ),
    ("empty", "Empty translation", "The translation is blank."),
    (
        "identical",
        "Identical to source",
        "The translation is the source text.",
    ),
    (
        "fragment",
        "Half translated",
        "Words of the source language remain in a non-Latin-script translation.",
    ),
    (
        "length",
        "Length",
        "The translation is far longer or shorter than the source, or exceeds a polygo:max limit.",
    ),
    (
        "whitespace",
        "Edge whitespace",
        "Leading or trailing whitespace differs from the source.",
    ),
    (
        "punctuation",
        "Punctuation dropped",
        "The source ends with punctuation and the translation ends with a letter.",
    ),
    (
        "orphan",
        "Orphan key",
        "The locale file has a key the source file no longer has.",
    ),
    (
        "fuzzy",
        "Fuzzy",
        "A gettext entry marked fuzzy: shown untranslated at runtime.",
    ),
    (
        "state",
        "Xcode state",
        "A String Catalog unit marked needs_review, stale or new, or a key whose extraction is stale.",
    ),
    (
        "encoding",
        "Wrong encoding",
        "Text that looks like UTF-8 read as Latin-1 (Ã©, â€™).",
    ),
    (
        "invisible",
        "Invisible character",
        "A zero-width space, mid-string BOM, line separator, bidi control or control character.",
    ),
    (
        "link",
        "Link changed",
        "A URL or email address in the source is missing or different in the translation.",
    ),
    (
        "brackets",
        "Unbalanced brackets",
        "A bracket pair the source keeps balanced is unbalanced in the translation.",
    ),
    (
        "entities",
        "Double-escaped entity",
        "An HTML entity escaped twice (&amp;amp;).",
    ),
    (
        "inconsistent",
        "Inconsistent term",
        "The same short source term is translated differently across keys in one locale.",
    ),
];

pub fn render(report: &Report, strict: bool) -> Value {
    let rules: Vec<Value> = RULES
        .iter()
        .map(|(id, short, full)| {
            json!({
                "id": format!("polygo/{id}"),
                "name": short.replace(' ', ""),
                "shortDescription": { "text": short },
                "fullDescription": { "text": full },
                "helpUri": "https://github.com/Na5co/polygo#polygo-check",
                "help": { "text": full },
            })
        })
        .collect();
    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            let level = if f.severity == "error" || strict {
                "error"
            } else {
                "warning"
            };
            let mut location = json!({
                "physicalLocation": {
                    "artifactLocation": { "uri": f.file, "uriBaseId": "%SRCROOT%" }
                }
            });
            if let Some(l) = f.line {
                location["physicalLocation"]["region"] = json!({ "startLine": l });
            }
            json!({
                "ruleId": format!("polygo/{}", f.code),
                "level": level,
                "message": { "text": format!("{} [{}]: {}", f.key, f.locale, f.message) },
                "locations": [location],
                "partialFingerprints": {
                    "polygo/v1": format!("{}:{}:{}:{}", f.file, f.key, f.locale, f.code)
                }
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "polygo",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/Na5co/polygo",
                    "rules": rules
                }
            },
            "results": results
        }]
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_rule() {
        // Codes that run.rs / text.rs / content.rs emit must be described.
        let src = concat!(
            include_str!("run.rs"),
            include_str!("text.rs"),
            include_str!("content.rs")
        );
        for (id, _, _) in super::RULES {
            assert!(
                src.contains(&format!("\"{id}\"")),
                "{id} not emitted anywhere"
            );
        }
        for code in [
            "placeholders",
            "plural",
            "markup",
            "escape",
            "array",
            "duplicate",
            "glossary",
            "empty",
            "identical",
            "fragment",
            "length",
            "whitespace",
            "punctuation",
            "orphan",
            "fuzzy",
            "state",
            "encoding",
            "invisible",
            "link",
            "brackets",
            "entities",
            "inconsistent",
        ] {
            assert!(
                super::RULES.iter().any(|(id, _, _)| *id == code),
                "{code} has no rule"
            );
        }
    }
}
