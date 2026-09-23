//! What arrives from GitHub: the signature that proves it did, and the little that a
//! pull-request event has to say. Everything else is ignored on purpose — the bot holds no
//! state and answers one question, "is this pull request worth a review?".

use anyhow::{Result, anyhow};

/// `X-Hub-Signature-256: sha256=…` over the raw body, with the App's webhook secret.
/// Compared in constant time: a comparison that returns early leaks the digest.
pub fn verify(secret: &str, body: &[u8], header: Option<&str>) -> Result<()> {
    let header = header.ok_or_else(|| anyhow!("no X-Hub-Signature-256 header"))?;
    let hex = header
        .strip_prefix("sha256=")
        .ok_or_else(|| anyhow!("signature is not sha256=…"))?;
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    let got = decode_hex(hex).ok_or_else(|| anyhow!("signature is not hex"))?;
    ring::hmac::verify(&key, body, &got)
        .map_err(|_| anyhow!("signature does not match the webhook secret"))
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// What a Marketplace event says, for the log: someone subscribed, changed plan or
/// cancelled. GitHub requires a listed app to receive these; the bot has one free plan, so
/// there is nothing to do about them beyond knowing they happened.
pub fn marketplace(kind: &str, payload: &serde_json::Value) -> Option<String> {
    if kind != "marketplace_purchase" {
        return None;
    }
    let action = payload["action"].as_str().unwrap_or("?");
    let account = payload["marketplace_purchase"]["account"]["login"]
        .as_str()
        .unwrap_or("?");
    let plan = payload["marketplace_purchase"]["plan"]["name"]
        .as_str()
        .unwrap_or("?");
    Some(format!("marketplace: {account} {action} the {plan} plan"))
}

/// A pull request the bot should look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub installation: u64,
    /// `owner/repo` of the *base*: a fork's pull request is fetched from here too.
    pub repo: String,
    pub number: u64,
    pub head_sha: String,
}

/// The event, or a reason it is not one to review. Errors are for malformed payloads;
/// `Ok(None)` is a healthy "nothing to do here".
pub fn pull_request(kind: &str, payload: &serde_json::Value) -> Result<Option<Event>> {
    if kind == "ping" || kind == "installation" || kind == "installation_repositories" {
        return Ok(None);
    }
    if kind == "marketplace_purchase" {
        return Ok(None);
    }
    if kind != "pull_request" {
        return Ok(None);
    }
    let action = payload["action"].as_str().unwrap_or("");
    if !matches!(
        action,
        "opened" | "synchronize" | "reopened" | "ready_for_review"
    ) {
        return Ok(None);
    }
    let pr = &payload["pull_request"];
    // A draft is still being written; the review would be noise.
    if pr["draft"].as_bool().unwrap_or(false) {
        return Ok(None);
    }
    let installation = payload["installation"]["id"]
        .as_u64()
        .ok_or_else(|| anyhow!("pull_request event without an installation"))?;
    let repo = payload["repository"]["full_name"]
        .as_str()
        .ok_or_else(|| anyhow!("event without a repository"))?
        .to_string();
    let number = pr["number"]
        .as_u64()
        .ok_or_else(|| anyhow!("event without a pull request number"))?;
    let head_sha = pr["head"]["sha"]
        .as_str()
        .ok_or_else(|| anyhow!("event without a head sha"))?
        .to_string();
    Ok(Some(Event {
        installation,
        repo,
        number,
        head_sha,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sign(secret: &str, body: &[u8]) -> String {
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        let tag = ring::hmac::sign(&key, body);
        let hex: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256={hex}")
    }

    #[test]
    fn signature_must_match_the_secret() {
        let body = br#"{"action":"opened"}"#;
        assert!(verify("s3cret", body, Some(&sign("s3cret", body))).is_ok());
        assert!(verify("s3cret", body, Some(&sign("other", body))).is_err());
        assert!(verify("s3cret", b"tampered", Some(&sign("s3cret", body))).is_err());
        assert!(verify("s3cret", body, None).is_err());
        assert!(verify("s3cret", body, Some("sha1=abcd")).is_err());
        assert!(verify("s3cret", body, Some("sha256=zz")).is_err());
    }

    fn event(action: &str) -> serde_json::Value {
        json!({
            "action": action,
            "installation": { "id": 42 },
            "repository": { "full_name": "acme/app" },
            "pull_request": { "number": 7, "draft": false, "head": { "sha": "abc123" } },
        })
    }

    #[test]
    fn marketplace_events_are_noted_and_nothing_else() {
        let ev = json!({
            "action": "purchased",
            "marketplace_purchase": {
                "account": { "login": "acme" },
                "plan": { "name": "Free" }
            }
        });
        assert_eq!(
            marketplace("marketplace_purchase", &ev).as_deref(),
            Some("marketplace: acme purchased the Free plan")
        );
        assert_eq!(marketplace("pull_request", &ev), None);
        // A Marketplace event is not a pull request, and must not be mistaken for a
        // malformed one.
        assert!(pull_request("marketplace_purchase", &ev).unwrap().is_none());
    }

    #[test]
    fn only_pull_requests_worth_reviewing() {
        let e = pull_request("pull_request", &event("opened"))
            .unwrap()
            .unwrap();
        assert_eq!(
            e,
            Event {
                installation: 42,
                repo: "acme/app".into(),
                number: 7,
                head_sha: "abc123".into()
            }
        );
        assert!(
            pull_request("pull_request", &event("synchronize"))
                .unwrap()
                .is_some()
        );
        // Labels, assignments, comments, closes: nothing to say.
        assert!(
            pull_request("pull_request", &event("labeled"))
                .unwrap()
                .is_none()
        );
        assert!(pull_request("push", &event("opened")).unwrap().is_none());
        assert!(pull_request("ping", &json!({})).unwrap().is_none());
        let mut draft = event("opened");
        draft["pull_request"]["draft"] = json!(true);
        assert!(pull_request("pull_request", &draft).unwrap().is_none());
        // A payload that claims to be a pull request but is not usable is an error.
        let mut broken = event("opened");
        broken["installation"] = json!(null);
        assert!(pull_request("pull_request", &broken).is_err());
    }
}
