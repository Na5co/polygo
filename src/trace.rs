//! Opt-in tracing of model calls to [Arize Phoenix](https://github.com/Arize-ai/phoenix)
//! (or any OTLP/HTTP collector). Off unless `PHOENIX_COLLECTOR_ENDPOINT` is set; then
//! every batch becomes a trace: one `CHAIN` span for the batch and one `LLM` span per
//! model call, with the prompts, the reply, token counts, latency and errors, using the
//! [OpenInference](https://github.com/Arize-ai/openinference) attribute names Phoenix
//! renders natively.
//!
//! No OpenTelemetry crates: the OTLP protobuf for spans is a few dozen fields, and
//! Phoenix only accepts protobuf, so this module writes it by hand and POSTs with the
//! same `ureq` the providers use. Export failures are reported once and never fail a run.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where spans go, or `None` when tracing is off. Read once per process.
fn config() -> Option<&'static Config> {
    static CONFIG: OnceLock<Option<Config>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let endpoint = std::env::var("PHOENIX_COLLECTOR_ENDPOINT")
                .ok()
                .filter(|v| !v.is_empty())?;
            let endpoint = endpoint.trim_end_matches('/').to_string();
            let url = if endpoint.ends_with("/v1/traces") {
                endpoint
            } else {
                format!("{endpoint}/v1/traces")
            };
            Some(Config {
                url,
                api_key: std::env::var("PHOENIX_API_KEY")
                    .ok()
                    .filter(|v| !v.is_empty()),
                project: std::env::var("PHOENIX_PROJECT_NAME")
                    .ok()
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "polygo".into()),
            })
        })
        .as_ref()
}

struct Config {
    url: String,
    api_key: Option<String>,
    project: String,
}

pub fn enabled() -> bool {
    config().is_some()
}

/// Set once an export has failed; the hint below is then withheld.
static EXPORT_FAILED: AtomicBool = AtomicBool::new(false);

/// Where to look at the traces, for the end-of-run hint. `None` when tracing is off
/// or the collector could not be reached.
pub fn ui_url() -> Option<String> {
    let c = config()?;
    if EXPORT_FAILED.load(Ordering::Relaxed) {
        return None;
    }
    let base = c.url.trim_end_matches("/v1/traces");
    Some(format!("{base}/projects (project `{}`)", c.project))
}

/// One id per process so Phoenix groups every batch of a run under one session.
fn session_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| hex(&fresh_id()[..8]))
}

// ---- spans ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Kind {
    Chain,
    Llm,
}

impl Kind {
    fn openinference(self) -> &'static str {
        match self {
            Kind::Chain => "CHAIN",
            Kind::Llm => "LLM",
        }
    }
}

enum Value {
    Str(String),
    Int(i64),
}

/// A live span. Ends on [`Span::end`] or on drop (then with whatever was recorded so
/// far), so early `?` returns still export. A no-op when tracing is off.
pub struct Span {
    inner: Option<Live>,
}

struct Live {
    trace_id: [u8; 16],
    span_id: [u8; 8],
    parent_id: Option<[u8; 8]>,
    name: String,
    start_ns: u64,
    attrs: Vec<(&'static str, Value)>,
    error: Option<String>,
}

impl Span {
    /// A root span: the start of a new trace.
    pub fn root(name: &str, kind: Kind) -> Span {
        Self::new(name, kind, None)
    }

    /// A child of `parent`, in the same trace.
    pub fn child(&self, name: &str, kind: Kind) -> Span {
        match &self.inner {
            Some(p) => Self::new(name, kind, Some((p.trace_id, p.span_id))),
            None => Span { inner: None },
        }
    }

    fn new(name: &str, kind: Kind, parent: Option<([u8; 16], [u8; 8])>) -> Span {
        if !enabled() {
            return Span { inner: None };
        }
        let id = fresh_id();
        let (trace_id, parent_id) = match parent {
            Some((t, p)) => (t, Some(p)),
            None => (id[..16].try_into().unwrap(), None),
        };
        let mut span = Span {
            inner: Some(Live {
                trace_id,
                span_id: id[16..24].try_into().unwrap(),
                parent_id,
                name: name.to_string(),
                start_ns: now_ns(),
                attrs: Vec::new(),
                error: None,
            }),
        };
        span.set("openinference.span.kind", kind.openinference());
        span.set("session.id", session_id());
        span
    }

    pub fn set(&mut self, key: &'static str, value: impl Into<String>) -> &mut Self {
        if let Some(l) = &mut self.inner {
            l.attrs.push((key, Value::Str(value.into())));
        }
        self
    }

    pub fn set_int(&mut self, key: &'static str, value: i64) -> &mut Self {
        if let Some(l) = &mut self.inner {
            l.attrs.push((key, Value::Int(value)));
        }
        self
    }

    /// The prompt as OpenInference input messages plus `input.value` for the trace view.
    pub fn set_prompt(&mut self, system: &str, user: &str) -> &mut Self {
        self.set("llm.input_messages.0.message.role", "system")
            .set("llm.input_messages.0.message.content", system)
            .set("llm.input_messages.1.message.role", "user")
            .set("llm.input_messages.1.message.content", user)
            .set("input.value", user)
            .set("input.mime_type", "text/plain")
    }

    pub fn set_reply(&mut self, reply: &crate::provider::Reply) -> &mut Self {
        self.set("llm.output_messages.0.message.role", "assistant")
            .set("llm.output_messages.0.message.content", reply.text.as_str())
            .set("output.value", reply.text.as_str());
        if let Some(u) = reply.usage {
            self.set_int("llm.token_count.prompt", u.input_tokens as i64)
                .set_int("llm.token_count.completion", u.output_tokens as i64)
                .set_int(
                    "llm.token_count.total",
                    (u.input_tokens + u.output_tokens) as i64,
                );
        }
        self
    }

    pub fn set_error(&mut self, err: &anyhow::Error) -> &mut Self {
        if let Some(l) = &mut self.inner {
            l.error = Some(format!("{err:#}"));
        }
        self
    }

    /// Ends the span now. Root spans flush the buffer: everything under them is done.
    pub fn end(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        let Some(live) = self.inner.take() else {
            return;
        };
        let Some(cfg) = config() else { return };
        let is_root = live.parent_id.is_none();
        let bytes = encode_span(&live, now_ns());
        buffer().lock().unwrap().push(bytes);
        if is_root {
            flush(cfg);
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        self.finish();
    }
}

// ---- export --------------------------------------------------------------------------

fn buffer() -> &'static Mutex<Vec<Vec<u8>>> {
    static BUF: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();
    BUF.get_or_init(|| Mutex::new(Vec::new()))
}

fn flush(cfg: &Config) {
    let spans: Vec<Vec<u8>> = std::mem::take(&mut *buffer().lock().unwrap());
    if spans.is_empty() {
        return;
    }
    let body = encode_request(cfg, &spans);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();
    let mut req = agent
        .post(&cfg.url)
        .header("content-type", "application/x-protobuf");
    if let Some(key) = &cfg.api_key {
        req = req.header("authorization", &format!("Bearer {key}"));
    }
    if let Err(e) = req.send(&body[..])
        && !EXPORT_FAILED.swap(true, Ordering::Relaxed)
    {
        eprintln!(
            "polygo: trace export to {} failed: {e} (run continues)",
            cfg.url
        );
    }
}

// ---- OTLP protobuf -------------------------------------------------------------------
// opentelemetry/proto/{collector/trace/v1/trace_service,trace/v1/trace,common/v1/common,
// resource/v1/resource}.proto, field numbers as of protocol v1.x.

const WIRE_VARINT: u64 = 0;
const WIRE_FIXED64: u64 = 1;
const WIRE_LEN: u64 = 2;

fn put_varint(buf: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

fn put_tag(buf: &mut Vec<u8>, field: u64, wire: u64) {
    put_varint(buf, (field << 3) | wire);
}

fn put_bytes(buf: &mut Vec<u8>, field: u64, data: &[u8]) {
    put_tag(buf, field, WIRE_LEN);
    put_varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

fn put_uint(buf: &mut Vec<u8>, field: u64, v: u64) {
    put_tag(buf, field, WIRE_VARINT);
    put_varint(buf, v);
}

fn put_fixed64(buf: &mut Vec<u8>, field: u64, v: u64) {
    put_tag(buf, field, WIRE_FIXED64);
    buf.extend_from_slice(&v.to_le_bytes());
}

/// `KeyValue { key = 1, value = 2: AnyValue { string_value = 1, int_value = 3 } }`
fn encode_attr(key: &str, value: &Value) -> Vec<u8> {
    let mut any = Vec::new();
    match value {
        Value::Str(s) => put_bytes(&mut any, 1, s.as_bytes()),
        Value::Int(i) => put_uint(&mut any, 3, *i as u64),
    }
    let mut kv = Vec::new();
    put_bytes(&mut kv, 1, key.as_bytes());
    put_bytes(&mut kv, 2, &any);
    kv
}

/// `Span`: trace_id = 1, span_id = 2, parent_span_id = 4, name = 5, kind = 6,
/// start = 7, end = 8 (fixed64 ns), attributes = 9, status = 15 { message = 2, code = 3 }.
fn encode_span(live: &Live, end_ns: u64) -> Vec<u8> {
    let mut s = Vec::new();
    put_bytes(&mut s, 1, &live.trace_id);
    put_bytes(&mut s, 2, &live.span_id);
    if let Some(p) = &live.parent_id {
        put_bytes(&mut s, 4, p);
    }
    put_bytes(&mut s, 5, live.name.as_bytes());
    put_uint(&mut s, 6, 1); // SPAN_KIND_INTERNAL
    put_fixed64(&mut s, 7, live.start_ns);
    put_fixed64(&mut s, 8, end_ns);
    for (k, v) in &live.attrs {
        put_bytes(&mut s, 9, &encode_attr(k, v));
    }
    let mut status = Vec::new();
    match &live.error {
        Some(msg) => {
            put_bytes(&mut status, 2, msg.as_bytes());
            put_uint(&mut status, 3, 2); // STATUS_CODE_ERROR
        }
        None => put_uint(&mut status, 3, 1), // STATUS_CODE_OK
    }
    put_bytes(&mut s, 15, &status);
    s
}

/// `ExportTraceServiceRequest { resource_spans = 1: ResourceSpans { resource = 1,
/// scope_spans = 2: ScopeSpans { scope = 1 { name = 1, version = 2 }, spans = 2 } } }`
fn encode_request(cfg: &Config, spans: &[Vec<u8>]) -> Vec<u8> {
    let mut resource = Vec::new();
    for (k, v) in [
        ("service.name", "polygo"),
        ("service.version", env!("CARGO_PKG_VERSION")),
        ("openinference.project.name", cfg.project.as_str()),
    ] {
        put_bytes(&mut resource, 1, &encode_attr(k, &Value::Str(v.into())));
    }
    let mut scope = Vec::new();
    put_bytes(&mut scope, 1, b"polygo");
    put_bytes(&mut scope, 2, env!("CARGO_PKG_VERSION").as_bytes());
    let mut scope_spans = Vec::new();
    put_bytes(&mut scope_spans, 1, &scope);
    for s in spans {
        put_bytes(&mut scope_spans, 2, s);
    }
    let mut resource_spans = Vec::new();
    put_bytes(&mut resource_spans, 1, &resource);
    put_bytes(&mut resource_spans, 2, &scope_spans);
    let mut req = Vec::new();
    put_bytes(&mut req, 1, &resource_spans);
    req
}

// ---- ids and time --------------------------------------------------------------------

/// 32 unique bytes: a hash of the clock, the pid and a counter. Ids need to be unique,
/// not unguessable, so no RNG dependency.
fn fresh_id() -> [u8; 32] {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut seed = Vec::with_capacity(24);
    seed.extend_from_slice(&now_ns().to_le_bytes());
    seed.extend_from_slice(&(std::process::id() as u64).to_le_bytes());
    seed.extend_from_slice(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    *blake3::hash(&seed).as_bytes()
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_matches_protobuf_examples() {
        let mut b = Vec::new();
        put_varint(&mut b, 1);
        put_varint(&mut b, 150);
        put_varint(&mut b, 300);
        assert_eq!(b, [0x01, 0x96, 0x01, 0xac, 0x02]);
    }

    #[test]
    fn attr_encodes_like_protoc() {
        // KeyValue{key:"a", value:{string_value:"b"}} == 0a 01 61 12 03 0a 01 62
        assert_eq!(
            encode_attr("a", &Value::Str("b".into())),
            [0x0a, 0x01, 0x61, 0x12, 0x03, 0x0a, 0x01, 0x62]
        );
        // KeyValue{key:"n", value:{int_value:7}} == 0a 01 6e 12 02 18 07
        assert_eq!(
            encode_attr("n", &Value::Int(7)),
            [0x0a, 0x01, 0x6e, 0x12, 0x02, 0x18, 0x07]
        );
    }

    #[test]
    fn span_has_ids_name_times_and_status() {
        let live = Live {
            trace_id: [1; 16],
            span_id: [2; 8],
            parent_id: Some([3; 8]),
            name: "x".into(),
            start_ns: 5,
            attrs: vec![],
            error: Some("boom".into()),
        };
        let s = encode_span(&live, 6);
        let mut want = vec![0x0a, 16];
        want.extend([1; 16]);
        want.extend([0x12, 8]);
        want.extend([2; 8]);
        want.extend([0x22, 8]);
        want.extend([3; 8]);
        want.extend([0x2a, 1, b'x', 0x30, 1]);
        want.extend([0x39]);
        want.extend(5u64.to_le_bytes());
        want.extend([0x41]);
        want.extend(6u64.to_le_bytes());
        // status: 7a len { 12 04 "boom" 18 02 }
        want.extend([0x7a, 8, 0x12, 4, b'b', b'o', b'o', b'm', 0x18, 2]);
        assert_eq!(s, want);
    }

    #[test]
    fn ids_are_unique() {
        let a = fresh_id();
        let b = fresh_id();
        assert_ne!(a, b);
    }
}
