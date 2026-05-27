//! L3 differential + downstream gate for cobrust-requests.
//!
//! Per ADR-0022 §2 (sync surface) + the brief's "in-process wiremock
//! server for deterministic tests" requirement. We spin a tiny TCP
//! server on a random localhost port that speaks HTTP/1.1, dispatch
//! the cobrust-requests free verbs + Session methods at it, and
//! assert the Response observers match what the wiremock returned.
//!
//! When `httpbin.org` is reachable, we additionally drive the
//! `requests.get('https://httpbin.org/get').json()` smoke path —
//! this is the canonical real-world differential. When offline, the
//! suite logs a clean skip and the in-process gate alone proves L3.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use cobrust_requests::{HttpErrorKind, Session, get as cobrust_get, post as cobrust_post};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Tiny, single-shot HTTP/1.1 wiremock — accepts one request, parses
/// the request line + headers, runs the supplied handler, and writes
/// the response. Returns the bound port so the caller can dispatch.
fn spawn_wiremock<F>(handler: F) -> u16
where
    F: FnOnce(&str, Vec<u8>) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("local_addr").port();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        // Read until we see CRLFCRLF — minimal HTTP/1.1 request parser.
        let mut buf = [0u8; 4096];
        let mut total = Vec::with_capacity(1024);
        loop {
            let n = match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            total.extend_from_slice(&buf[..n]);
            if total.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let raw = String::from_utf8_lossy(&total).into_owned();
        let mut lines = raw.split("\r\n");
        let request_line = lines.next().unwrap_or("").to_string();
        // Find Content-Length to read the body. Default to 0.
        let mut content_length: usize = 0;
        for line in &mut lines {
            if line.is_empty() {
                break;
            }
            if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = rest.trim().parse().unwrap_or(0);
            }
        }
        // Body bytes already in `total` after \r\n\r\n.
        let header_end = total
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|p| p + 4)
            .unwrap_or(total.len());
        let mut body: Vec<u8> = total[header_end.min(total.len())..].to_vec();
        while body.len() < content_length {
            let n = match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            body.extend_from_slice(&buf[..n]);
        }
        body.truncate(content_length);
        let (status, headers, resp_body) = handler(&request_line, body);
        let mut response = format!("HTTP/1.1 {status} OK\r\n");
        response.push_str(&format!("Content-Length: {}\r\n", resp_body.len()));
        for (k, v) in &headers {
            response.push_str(&format!("{k}: {v}\r\n"));
        }
        response.push_str("\r\n");
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(&resp_body);
        let _ = stream.flush();
    });
    port
}

#[test]
fn l3_get_against_in_process_wiremock_returns_body() {
    let request_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let log_clone = request_log.clone();
    let port = spawn_wiremock(move |request_line, _body| {
        log_clone.lock().unwrap().push(request_line.to_string());
        (
            200,
            vec![("Content-Type".into(), "application/json".into())],
            br#"{"hello":"world"}"#.to_vec(),
        )
    });
    let url = format!("http://127.0.0.1:{port}/sentinel");
    let resp = cobrust_get(&url).expect("get succeeds");
    assert_eq!(resp.status_code(), 200);
    assert!(resp.ok());
    let json = resp.json().expect("json");
    assert_eq!(json.get("hello").and_then(|v| v.as_str()), Some("world"));
    let log = request_log.lock().unwrap();
    assert!(log[0].starts_with("GET "), "request line: {}", log[0]);
    assert!(
        log[0].contains("/sentinel"),
        "expected /sentinel: {}",
        log[0]
    );
}

#[test]
fn l3_post_against_in_process_wiremock_echoes_body() {
    let port = spawn_wiremock(|request_line, body| {
        // Echo the request body back as the response body.
        assert!(request_line.starts_with("POST "));
        (201, vec![], body)
    });
    let url = format!("http://127.0.0.1:{port}/echo");
    let resp = cobrust_post(&url, b"the body").expect("post succeeds");
    assert_eq!(resp.status_code(), 201);
    assert_eq!(resp.text().expect("text"), "the body");
}

#[test]
fn l3_session_reuses_client_across_calls() {
    // Two sequential requests to two wiremock instances — both must
    // succeed via the same Session (which means the inner reqwest
    // pool persisted; a second invocation that errors would mean the
    // pool was torn down between calls).
    let session = Session::new();
    let port_a = spawn_wiremock(|_req, _body| (200, vec![], b"a".to_vec()));
    let port_b = spawn_wiremock(|_req, _body| (200, vec![], b"b".to_vec()));
    let resp_a = session
        .get(&format!("http://127.0.0.1:{port_a}/"))
        .expect("first");
    let resp_b = session
        .get(&format!("http://127.0.0.1:{port_b}/"))
        .expect("second");
    assert_eq!(resp_a.text().expect("a"), "a");
    assert_eq!(resp_b.text().expect("b"), "b");
}

#[test]
fn l3_session_methods_dispatch_correct_verbs() {
    let session = Session::new();
    for verb in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] {
        let want_verb = verb.to_owned();
        let port = spawn_wiremock(move |request_line, _body| {
            let observed = request_line.split(' ').next().unwrap_or("").to_owned();
            let body = if observed == want_verb {
                b"ok".to_vec()
            } else {
                format!("expected {want_verb}, got {observed}").into_bytes()
            };
            (200, vec![], body)
        });
        let url = format!("http://127.0.0.1:{port}/v");
        let resp = match verb {
            "GET" => session.get(&url),
            "POST" => session.post(&url, b""),
            "PUT" => session.put(&url, b""),
            "PATCH" => session.patch(&url, b""),
            "DELETE" => session.delete(&url),
            "HEAD" => session.head(&url),
            _ => unreachable!(),
        }
        .expect(verb);
        if verb != "HEAD" {
            // HEAD bodies are spec-empty; assert non-HEAD only.
            assert_eq!(resp.text().expect("text"), "ok", "{verb} mismatch");
        }
    }
}

#[test]
fn l3_invalid_url_routes_to_invalid_url_kind() {
    let err = cobrust_get("not://a real url space").expect_err("must error");
    assert_eq!(err.kind, HttpErrorKind::InvalidUrl);
}

#[test]
fn l3_pyo3_wrapper_directory_layout() {
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(crate_dir.join("python/requests_init.py").exists());
    assert!(crate_dir.join("python/setup.py").exists());
    assert!(crate_dir.join("PROVENANCE.toml").exists());
}

#[test]
#[ignore = "F59: depends on external httpbin.org, which is flaky/rate-limited \
            under load (returns 503/degraded even when reachable) — an external \
            service must not gate CI (F37/F44 deterministic-CI discipline). \
            Opt-in via `cargo test -- --ignored`. Surfaced 2026-05-27 when the \
            probe succeeded but the real GET returned non-200 on a GH runner."]
fn l3_optional_httpbin_smoke() {
    // Opt-in smoke against the real httpbin.org. If httpbin is reachable AND
    // healthy (200 + well-formed body), the JSON must echo the URL we asked
    // for. ANY degradation (unreachable, non-200, malformed body) is a clean
    // skip — the M-batch ADR-0022 "skip cleanly if offline" contract, widened
    // to cover the up-but-degraded case that flaked CI (F59).
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client");
    if client.get("https://httpbin.org/get").send().is_err() {
        eprintln!("L3 httpbin smoke: skipping — httpbin.org unreachable");
        return;
    }
    let resp = match cobrust_get("https://httpbin.org/get") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("L3 httpbin smoke: skipping — request failed: {e:?}");
            return;
        }
    };
    if resp.status_code() != 200 {
        eprintln!(
            "L3 httpbin smoke: skipping — httpbin returned {} (degraded)",
            resp.status_code()
        );
        return;
    }
    let Ok(json) = resp.json() else {
        eprintln!("L3 httpbin smoke: skipping — malformed body");
        return;
    };
    let url = json.get("url").and_then(|v| v.as_str()).unwrap_or_default();
    assert!(url.contains("httpbin.org"), "url field: {url}");
}
