//! Integration tests for `cobrust_pkg::registry_client` (ADR-0065 §3.4).
//!
//! These tests spin up a tiny in-process HTTP server using a raw
//! `std::net::TcpListener` (no `mockito` / `wiremock` dev-dep added).
//! The server serves a fixed JSON index and a tarball payload for one
//! happy-path test, a corrupted payload for the SHA-mismatch test, and
//! a 404 for the not-found test.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use cobrust_pkg::registry_client::{RegistryClient, RegistryClientError, WheelMeta};
use cobrust_pkg::wheel_select::COBRUST_ABI_VERSION;
use sha2::{Digest, Sha256};

/// Spawn a single-request HTTP server on a random port. `responses` is a list
/// of `(path_prefix, status, body)` rules; the first matching prefix wins.
/// Returns the bound URL like `http://127.0.0.1:PORT` and a thread join handle.
fn spawn_mock_server(
    responses: Vec<(String, u16, Vec<u8>)>,
    iterations: usize,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = listener.local_addr().expect("addr");
    let url = format!("http://127.0.0.1:{}", addr.port());
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    let handle = thread::spawn(move || {
        let _ = ready_tx.send(());
        for _ in 0..iterations {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            handle_one(&mut stream, &responses);
        }
    });
    let _ = ready_rx.recv();
    (url, handle)
}

fn handle_one(stream: &mut TcpStream, responses: &[(String, u16, Vec<u8>)]) {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]).to_string();
    // First line: `GET /path HTTP/1.1`
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();

    for (prefix, status, body) in responses {
        if path.starts_with(prefix) {
            let status_line = match *status {
                200 => "HTTP/1.1 200 OK\r\n",
                404 => "HTTP/1.1 404 Not Found\r\n",
                _ => "HTTP/1.1 500 Internal Server Error\r\n",
            };
            let _ = stream.write_all(status_line.as_bytes());
            let _ = stream.write_all(
                format!(
                    "Content-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            );
            let _ = stream.write_all(body);
            return;
        }
    }
    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().fold(String::new(), |mut acc, b| {
        use std::fmt::Write as _;
        let _ = write!(&mut acc, "{b:02x}");
        acc
    })
}

fn make_meta(filename: &str, sha256: &str) -> WheelMeta {
    WheelMeta {
        filename: filename.to_owned(),
        triple: "x86_64-unknown-linux-gnu".to_owned(),
        cpu_level: "v1".to_owned(),
        sha256: sha256.to_owned(),
        cobrust_abi: "0.1".to_owned(),
        cobrust_abi_version: COBRUST_ABI_VERSION,
        experimental: false,
        size_bytes: 1024,
        download_url: String::new(),
    }
}

#[test]
fn fetch_index_parses_valid_json() {
    let wheels = vec![WheelMeta {
        filename: "cobrust-hello-0.1.0-x86_64-unknown-linux-gnu-v1.tar.gz".to_owned(),
        triple: "x86_64-unknown-linux-gnu".to_owned(),
        cpu_level: "v1".to_owned(),
        sha256: "0".repeat(64),
        cobrust_abi: "0.1".to_owned(),
        cobrust_abi_version: COBRUST_ABI_VERSION,
        experimental: false,
        size_bytes: 1024,
        download_url: "https://example.com/wheel.tar.gz".to_owned(),
    }];
    let body = serde_json::to_vec(&wheels).expect("encode");

    let (base, handle) = spawn_mock_server(
        vec![("/index/hello-cb/0.1.0/wheels.json".to_owned(), 200, body)],
        1,
    );

    let client = RegistryClient::new(&base).expect("client");
    let got = client.fetch_index("hello-cb", "0.1.0").expect("fetch");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].cpu_level, "v1");
    let _ = handle.join();
}

#[test]
fn download_wheel_verifies_sha256_on_match() {
    let payload = b"fake wheel bytes".to_vec();
    let sha = sha256_hex(&payload);
    let mut meta = make_meta("test.tar.gz", &sha);
    meta.size_bytes = payload.len() as u64;

    let (base, handle) =
        spawn_mock_server(vec![("/wheel/test.tar.gz".to_owned(), 200, payload)], 1);

    meta.download_url = format!("{base}/wheel/test.tar.gz");

    let client = RegistryClient::new(&base).expect("client");
    let dest = tempfile::tempdir().expect("tmpdir");
    let path: PathBuf = client.download_wheel(&meta, dest.path()).expect("download");
    assert!(path.exists());
    let _ = handle.join();
}

#[test]
fn download_wheel_rejects_sha256_mismatch() {
    let payload = b"fake wheel bytes".to_vec();
    // Advertise a wrong SHA.
    let mut meta = make_meta("test.tar.gz", &"deadbeef".repeat(8)); // 64 hex chars, but not actual hash
    meta.size_bytes = payload.len() as u64;

    let (base, handle) =
        spawn_mock_server(vec![("/wheel/test.tar.gz".to_owned(), 200, payload)], 1);
    meta.download_url = format!("{base}/wheel/test.tar.gz");

    let client = RegistryClient::new(&base).expect("client");
    let dest = tempfile::tempdir().expect("tmpdir");
    let err = client
        .download_wheel(&meta, dest.path())
        .expect_err("must reject mismatch");
    assert!(matches!(err, RegistryClientError::Sha256Mismatch { .. }));
    let msg = err.to_string();
    assert!(msg.contains("SHA-256 mismatch"));
    assert!(msg.contains("suggestion"));
    let _ = handle.join();
}
