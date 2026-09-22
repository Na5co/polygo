//! `polygo check --sarif`: SARIF 2.1.0 for GitHub code scanning (and any other SARIF
//! consumer). Uploaded with `github/codeql-action/upload-sarif`, findings show in the
//! Security tab and on pull requests with history: new, fixed, still open.

use crate::check::code::{Code, Severity};
use crate::check::run::Report;
use serde_json::{Value, json};

pub fn render(report: &Report, strict: bool) -> Value {
    let rules: Vec<Value> = Code::ALL
        .iter()
        .map(|c| {
            json!({
                "id": format!("polygo/{c}"),
                "name": c.title().replace(' ', ""),
                "shortDescription": { "text": c.title() },
                "fullDescription": { "text": c.explain() },
                "defaultConfiguration": { "level": c.severity().as_str() },
                "helpUri": "https://github.com/Na5co/polygo#polygo-check",
                "help": { "text": c.explain() },
            })
        })
        .collect();
    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            let level = if f.severity == Severity::Error || strict {
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
    use crate::check::code::Code;

    #[test]
    fn every_code_has_words() {
        for c in Code::ALL {
            assert!(!c.title().is_empty() && c.explain().len() > 40, "{c}");
            assert_eq!(Code::parse(c.as_str()), Some(c));
        }
    }
}
