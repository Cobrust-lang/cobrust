//! L3 downstream gate for cobrust-pit.
//!
//! Per ADR-0022 §2 (sync surface) + the constitution §4.2 / §6
//! closed-loop spirit. We spin the real axum-backed `App` on an
//! ephemeral port (`127.0.0.1:0`) via `serve_in_background`, then drive
//! it with a real in-process HTTP client (`reqwest::blocking`) and
//! assert: routing, path params, GET/POST verbs, JSON request→response
//! round-trip, 404 for unknown routes, and status-code propagation.
//!
//! This is the ultimate oracle for this increment — the same code path
//! a Flask user's `app.run()` takes, exercised over a real socket.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]

use std::time::Duration;

use pit::{App, Request, Response};
use serde_json::json;

/// Build the canonical test app: a handful of routes covering every
/// surface the downstream gate exercises.
fn build_app() -> App {
    let mut app = App::new();
    app.get("/", |_req: Request| Response::text("root"))
        .expect("register /");
    app.get("/users/<id>", |req: Request| {
        let id = req.path_param("id").unwrap_or("?").to_owned();
        Response::json(&json!({ "id": id }))
    })
    .expect("register /users/<id>");
    app.get("/search", |req: Request| {
        let q = req.query("q").unwrap_or("").to_owned();
        Response::text(format!("q={q}"))
    })
    .expect("register /search");
    app.post("/echo", |req: Request| {
        let body = req.text().unwrap_or_default();
        Response::text(body).with_status(201)
    })
    .expect("register /echo");
    app.post("/sum", |req: Request| {
        let Ok(v) = req.json() else {
            return Response::text("bad json").with_status(400);
        };
        let a = v.get("a").and_then(serde_json::Value::as_i64).unwrap_or(0);
        let b = v.get("b").and_then(serde_json::Value::as_i64).unwrap_or(0);
        Response::json(&json!({ "sum": a + b }))
    })
    .expect("register /sum");
    app
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client")
}

#[test]
fn serves_root_and_path_params_and_query_and_post_and_json_and_404() {
    let handle = build_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind ephemeral");
    let addr = handle.local_addr();
    let base = format!("http://{addr}");
    let c = client();

    // 1. Root GET — plain text.
    let resp = c.get(format!("{base}/")).send().expect("GET /");
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(resp.text().expect("body"), "root");

    // 2. Path param capture.
    let resp = c
        .get(format!("{base}/users/42"))
        .send()
        .expect("GET /users/42");
    assert_eq!(resp.status().as_u16(), 200);
    let v: serde_json::Value = resp.json().expect("json");
    assert_eq!(v.get("id").and_then(serde_json::Value::as_str), Some("42"));

    // 3. Query string.
    let resp = c
        .get(format!("{base}/search?q=cobra+pit"))
        .send()
        .expect("GET /search");
    assert_eq!(resp.text().expect("body"), "q=cobra pit");

    // 4. POST echo + custom status code.
    let resp = c
        .post(format!("{base}/echo"))
        .body("payload-bytes")
        .send()
        .expect("POST /echo");
    assert_eq!(resp.status().as_u16(), 201);
    assert_eq!(resp.text().expect("body"), "payload-bytes");

    // 5. JSON request -> JSON response round-trip.
    let resp = c
        .post(format!("{base}/sum"))
        .json(&json!({ "a": 3, "b": 4 }))
        .send()
        .expect("POST /sum");
    assert_eq!(resp.status().as_u16(), 200);
    let v: serde_json::Value = resp.json().expect("json");
    assert_eq!(v.get("sum").and_then(serde_json::Value::as_i64), Some(7));

    // 6. Unknown route -> 404.
    let resp = c
        .get(format!("{base}/does-not-exist"))
        .send()
        .expect("GET /does-not-exist");
    assert_eq!(resp.status().as_u16(), 404);

    // 7. Wrong method on a known path -> 404 (no POST handler on /).
    let resp = c.post(format!("{base}/")).send().expect("POST /");
    assert_eq!(resp.status().as_u16(), 404);
}

#[test]
fn malformed_json_body_routes_to_400() {
    let handle = build_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind ephemeral");
    let base = format!("http://{}", handle.local_addr());
    let resp = client()
        .post(format!("{base}/sum"))
        .body("not json at all")
        .send()
        .expect("POST /sum");
    assert_eq!(resp.status().as_u16(), 400);
    assert_eq!(resp.text().expect("body"), "bad json");
}

#[test]
fn json_response_carries_application_json_content_type() {
    let handle = build_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind ephemeral");
    let base = format!("http://{}", handle.local_addr());
    let resp = client()
        .get(format!("{base}/users/7"))
        .send()
        .expect("GET /users/7");
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("application/json"), "content-type: {ct}");
}

#[test]
fn pyo3_wrapper_directory_layout() {
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(crate_dir.join("python/pit_init.py").exists());
    assert!(crate_dir.join("python/setup.py").exists());
    assert!(crate_dir.join("PROVENANCE.toml").exists());
}
