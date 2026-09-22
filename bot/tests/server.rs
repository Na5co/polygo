//! The App's front door: health, signature, and an answer fast enough for GitHub. The
//! private key is generated here and thrown away — no key belongs in a repository.
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SECRET: &str = "shhh";

struct Bot(Child);
impl Drop for Bot {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_for(port: u16) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(20) {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A throwaway 2048-bit key, from the openssl every CI image has. Without openssl the
/// test has nothing to sign with and says so rather than failing.
fn test_key(path: &std::path::Path) -> bool {
    Command::new("openssl")
        .args(["genrsa", "-out", path.to_str().unwrap(), "2048"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn sign(body: &[u8]) -> String {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, SECRET.as_bytes());
    let tag = ring::hmac::sign(&key, body);
    let hex: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256={hex}")
}

fn request(port: u16, head: &str, body: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(format!("{head}Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
        .unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out
}

#[test]
fn the_app_answers_health_and_refuses_an_unsigned_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("app.pem");
    if !test_key(&key) {
        eprintln!("openssl not available; skipping the polygo-bot server test");
        return;
    }
    let port = free_port();
    let bot = Bot(Command::new(env!("CARGO_BIN_EXE_polygo-bot"))
        .env("POLYGO_BOT_APP_ID", "12345")
        .env("POLYGO_BOT_PRIVATE_KEY_FILE", &key)
        .env("POLYGO_BOT_WEBHOOK_SECRET", SECRET)
        .env("PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap());
    assert!(wait_for(port), "polygo-bot did not start");

    let health = request(
        port,
        &format!("GET /health HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n"),
        "",
    );
    assert!(
        health.contains("200 OK") && health.contains("polygo-bot ok"),
        "{health}"
    );

    // A delivery nobody signed is not from GitHub.
    let body = r#"{"zen":"hi"}"#;
    let head = format!(
        "POST /webhook HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nX-GitHub-Event: ping\r\n"
    );
    let res = request(port, &head, body);
    assert!(res.contains("401"), "{res}");

    // A signed ping is accepted and does nothing, promptly.
    let signed = format!("{head}X-Hub-Signature-256: {}\r\n", sign(body.as_bytes()));
    let t0 = Instant::now();
    let res = request(port, &signed, body);
    assert!(res.contains("202"), "{res}");
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "the answer must be prompt"
    );

    // A signature over a different body is refused too.
    let wrong = format!("{head}X-Hub-Signature-256: {}\r\n", sign(b"something else"));
    let res = request(port, &wrong, body);
    assert!(res.contains("401"), "{res}");
    drop(bot);
}

#[test]
fn the_app_refuses_to_start_without_its_secrets() {
    let out = Command::new(env!("CARGO_BIN_EXE_polygo-bot"))
        .env_remove("POLYGO_BOT_APP_ID")
        .env_remove("POLYGO_BOT_WEBHOOK_SECRET")
        .env_remove("POLYGO_BOT_PRIVATE_KEY")
        .env_remove("POLYGO_BOT_PRIVATE_KEY_FILE")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("POLYGO_BOT_APP_ID is not set"), "{err}");
}
