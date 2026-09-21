//! G5.3: `polygo review` serves one embedded page on localhost; approving through the API
//! writes back through the same serializers (byte-stable) and records the lockfile.
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn http(port: u16, req: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

fn wait_for(port: u16) {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10) {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("review server did not start");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn project(root: &Path) {
    fs::create_dir_all(root.join("locales")).unwrap();
    fs::write(
        root.join("locales/en.json"),
        "{\n  \"hello\": \"Hello\",\n  \"bye\": \"Goodbye\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("polygo.toml"),
        "source_locale = \"en\"\ntarget_locales = [\"de\"]\n\n[[files]]\nformat = \"json\"\npath = \"locales/en.json\"\nlocale_path = \"locales/{locale}.json\"\n\n[provider]\nkind = \"mock\"\n",
    )
    .unwrap();
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

#[test]
fn review_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project(root);
    // Produce machine translations, then quarantine one key so both kinds of items exist.
    let out = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .env("POLYGO_CONFIG_DIR", root.join("cfg"))
        .arg("translate")
        .output()
        .unwrap();
    assert!(out.status.success());
    let before = fs::read_to_string(root.join("locales/de.json")).unwrap();

    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_polygo"))
        .current_dir(root)
        .env("POLYGO_CONFIG_DIR", root.join("cfg"))
        .args(["review", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _server = Server(child);
    wait_for(port);

    // The page and the items API.
    let page = http(
        port,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(
        page.contains("<html") && page.contains("polygo"),
        "{}",
        &page[..200.min(page.len())]
    );
    let items = http(
        port,
        "GET /api/items HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let body = items.split("\r\n\r\n").nth(1).unwrap();
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    let list = v["items"].as_array().unwrap();
    assert_eq!(list.len(), 2, "{v}");
    assert!(
        list.iter().any(|i| i["key"] == "hello"
            && i["locale"] == "de"
            && i["translation"] == "⟦de⟧ Hello")
    );

    // Approve an edit: the file changes only where it should, the lockfile records a human.
    let payload = r#"{"locale":"de","key":"hello","text":"Hallo"}"#;
    let resp = http(
        port,
        &format!(
            "POST /api/approve HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        ),
    );
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    let after = fs::read_to_string(root.join("locales/de.json")).unwrap();
    assert_eq!(after, before.replace("⟦de⟧ Hello", "Hallo"));
    let lock = fs::read_to_string(root.join("polygo.lock")).unwrap();
    assert!(lock.contains("provider = \"human\""), "{lock}");

    // Reject quarantines the key for a human.
    let payload = r#"{"locale":"de","key":"bye"}"#;
    let resp = http(
        port,
        &format!(
            "POST /api/reject HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        ),
    );
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    let lock = fs::read_to_string(root.join("polygo.lock")).unwrap();
    assert!(lock.contains("[keys.bye.review.de]"), "{lock}");

    // Non-localhost Host header is refused (DNS rebinding guard).
    let evil = http(
        port,
        "GET /api/items HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n",
    );
    assert!(evil.starts_with("HTTP/1.1 403"), "{evil}");
}
