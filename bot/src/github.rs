//! Talking to GitHub as the App: a JWT signed with the App's key buys an installation
//! token, and the installation token does the rest. Only the five calls the bot makes.

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::{Value, json};

const API: &str = "https://api.github.com";
const UA: &str = concat!("polygo-bot/", env!("CARGO_PKG_VERSION"));

pub struct App {
    app_id: String,
    key: RsaKeyPair,
    rng: ring::rand::SystemRandom,
}

impl App {
    /// `app_id` and the App's private key, as GitHub hands it over (`BEGIN RSA PRIVATE
    /// KEY`, PKCS#1) or converted to PKCS#8.
    pub fn new(app_id: &str, private_key_pem: &str) -> Result<App> {
        let der = pem_body(private_key_pem)?;
        let key = RsaKeyPair::from_der(&der)
            .or_else(|_| RsaKeyPair::from_pkcs8(&der))
            .map_err(|e| anyhow!("the App private key is not a usable RSA key: {e}"))?;
        Ok(App {
            app_id: app_id.to_string(),
            key,
            rng: ring::rand::SystemRandom::new(),
        })
    }

    /// A ten-minute JWT, which is what GitHub accepts to mint installation tokens.
    fn jwt(&self) -> Result<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        // 60 seconds back: GitHub rejects a token whose iat is in its future.
        let claims = json!({ "iat": now - 60, "exp": now + 540, "iss": self.app_id });
        let signing_input = format!(
            "{}.{}",
            B64.encode(br#"{"alg":"RS256","typ":"JWT"}"#),
            B64.encode(serde_json::to_vec(&claims)?)
        );
        let mut sig = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &self.rng,
                signing_input.as_bytes(),
                &mut sig,
            )
            .map_err(|e| anyhow!("signing the App JWT failed: {e}"))?;
        Ok(format!("{signing_input}.{}", B64.encode(&sig)))
    }

    /// A token that acts as the installation, good for an hour; the bot asks per event and
    /// keeps none of them.
    pub fn installation_token(&self, installation: u64) -> Result<String> {
        let v = post(
            &format!("{API}/app/installations/{installation}/access_tokens"),
            &self.jwt()?,
            None,
        )
        .context("minting an installation token")?;
        v["token"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("no token in the installation response"))
    }
}

/// The base64 body of a PEM block, whatever its label.
fn pem_body(pem: &str) -> Result<Vec<u8>> {
    let body: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----") && !l.trim().is_empty())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .context("the private key is not PEM")
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_defaults()
}

fn get(url: &str, token: &str) -> Result<Value> {
    let mut res = agent()
        .get(url)
        .header("authorization", &format!("Bearer {token}"))
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .header("user-agent", UA)
        .call()
        .with_context(|| format!("GET {url}"))?;
    Ok(res.body_mut().read_json()?)
}

fn post(url: &str, token: &str, body: Option<&Value>) -> Result<Value> {
    let req = agent()
        .post(url)
        .header("authorization", &format!("Bearer {token}"))
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .header("user-agent", UA);
    let mut res = match body {
        Some(b) => req.send_json(b),
        None => req.send_empty(),
    }
    .with_context(|| format!("POST {url}"))?;
    Ok(res.body_mut().read_json().unwrap_or(Value::Null))
}

/// Every page of `GET /repos/{repo}/pulls/{n}/{what}`.
fn paginate(repo: &str, number: u64, what: &str, token: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for page in 1..=10 {
        let url = format!("{API}/repos/{repo}/pulls/{number}/{what}?per_page=100&page={page}");
        let v = get(&url, token)?;
        let Some(items) = v.as_array() else {
            bail!("{what}: expected a list");
        };
        let n = items.len();
        out.extend(items.iter().cloned());
        if n < 100 {
            break;
        }
    }
    Ok(out)
}

/// The pull request's diff, as one unified diff: the `patch` GitHub returns per file, with
/// the headers polygo reads paths from. Binary and truncated files have no patch.
pub fn diff(repo: &str, number: u64, token: &str) -> Result<String> {
    let mut s = String::new();
    for f in paginate(repo, number, "files", token)? {
        let (Some(name), Some(patch)) = (f["filename"].as_str(), f["patch"].as_str()) else {
            continue;
        };
        s.push_str(&format!("--- a/{name}\n+++ b/{name}\n{patch}\n"));
    }
    Ok(s)
}

/// `path:line` of every review comment the bot already left, so it does not say a thing
/// twice when the branch is pushed again.
pub fn said_already(repo: &str, number: u64, token: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for c in paginate(repo, number, "comments", token)? {
        let body = c["body"].as_str().unwrap_or("");
        if !body.contains("<!-- polygo -->") {
            continue;
        }
        if let Some(path) = c["path"].as_str() {
            let line = c["line"].as_u64().or_else(|| c["original_line"].as_u64());
            if let Some(line) = line {
                out.push(format!("{path}:{line}"));
            }
        }
    }
    Ok(out)
}

pub fn post_review(repo: &str, number: u64, token: &str, review: &Value) -> Result<()> {
    post(
        &format!("{API}/repos/{repo}/pulls/{number}/reviews"),
        token,
        Some(review),
    )?;
    Ok(())
}

/// Fetch the pull request's head into `dir`, shallow. `refs/pull/{n}/head` exists on the
/// base repository even when the branch lives in a fork, so one token is enough and the
/// bot never needs write access to anyone's fork.
pub fn fetch_head(repo: &str, number: u64, token: &str, dir: &std::path::Path) -> Result<()> {
    let url = format!("https://x-access-token:{token}@github.com/{repo}.git");
    let git = |args: &[&str]| -> Result<()> {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .context("running git (is it installed in the image?)")?;
        if !out.status.success() {
            // The token is in the remote URL; keep it out of the logs.
            let err = String::from_utf8_lossy(&out.stderr).replace(token, "***");
            bail!("git {}: {}", args[0], err.trim());
        }
        Ok(())
    };
    git(&["init", "-q"])?;
    git(&["remote", "add", "origin", &url])?;
    git(&[
        "fetch",
        "-q",
        "--depth",
        "1",
        "origin",
        &format!("refs/pull/{number}/head"),
    ])?;
    git(&["checkout", "-q", "FETCH_HEAD"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_body_ignores_labels_and_wrapping() {
        let der =
            pem_body("-----BEGIN RSA PRIVATE KEY-----\nAQID\n-----END RSA PRIVATE KEY-----\n")
                .unwrap();
        assert_eq!(der, [1, 2, 3]);
        assert_eq!(pem_body("AQ ID").ok(), None);
        assert!(pem_body("not base64 at all!!").is_err());
    }

    #[test]
    fn a_bad_key_is_refused_with_words() {
        let e = match App::new("1", "-----BEGIN RSA PRIVATE KEY-----\nAQID\n-----END-----") {
            Ok(_) => panic!("three bytes are not an RSA key"),
            Err(e) => e.to_string(),
        };
        assert!(e.contains("not a usable RSA key"), "{e}");
    }
}
