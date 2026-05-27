//! L2.behavior fuzz harness for cobrust-pit.
//!
//! Constitution §4.2 floor: seeded, randomized round-trips per public
//! function. The HTTP semantics live inside axum/hyper; we fuzz the
//! cobrust surface — route compilation, segment matching, path-param
//! capture, 404 classification, and a live ephemeral-server round-trip
//! — to ensure constitution §5.1 invariants hold under random input.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::cast_possible_truncation)]

use std::time::Duration;

use pit::{App, Request, Response};

/// Deterministic SplitMix64-style PRNG (seeded; no external dep).
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) as u32) ^ ((z >> 32) as u32)
    }
}

/// A random URL-safe segment of 1..=8 lowercase letters / digits.
fn synth_segment(rng: &mut Lcg) -> String {
    let len = (rng.next_u32() % 8) as usize + 1;
    (0..len)
        .map(|_| {
            let pick = rng.next_u32() % 36;
            if pick < 26 {
                (b'a' + pick as u8) as char
            } else {
                (b'0' + (pick - 26) as u8) as char
            }
        })
        .collect()
}

/// Build an app with a single `/item/<id>` route that echoes the id, plus
/// a literal `/health`. The fuzz exercises path-param capture + 404.
fn build_fuzz_app() -> App {
    let mut app = App::new();
    app.get("/item/<id>", |req: Request| {
        Response::text(req.path_param("id").unwrap_or("?").to_owned())
    })
    .expect("register");
    app.get("/health", |_req: Request| Response::text("ok"))
        .expect("register");
    app
}

#[test]
fn live_path_param_round_trip_is_lossless() {
    let handle = build_fuzz_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind");
    let base = format!("http://{}", handle.local_addr());
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");

    let seeds: [u64; 3] = [42, 1337, 0xDEAD_BEEF];
    let mut total = 0;
    for &seed in &seeds {
        let mut rng = Lcg::new(seed);
        for _ in 0..150 {
            let id = synth_segment(&mut rng);
            let resp = client
                .get(format!("{base}/item/{id}"))
                .send()
                .expect("send");
            assert_eq!(resp.status().as_u16(), 200, "id={id}");
            // The captured path param must round-trip byte-for-byte.
            assert_eq!(resp.text().expect("body"), id);
            total += 1;
        }
    }
    assert!(total >= 400, "fuzz coverage shortfall: {total}");
}

#[test]
fn unknown_routes_classify_as_404_under_random_paths() {
    let handle = build_fuzz_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind");
    let base = format!("http://{}", handle.local_addr());
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");

    let seeds: [u64; 3] = [7, 99, 0xC0FF_EE00];
    let mut total = 0;
    for &seed in &seeds {
        let mut rng = Lcg::new(seed);
        for _ in 0..150 {
            // A random 2-segment path under a prefix the app never
            // registered ("/x/...") must always 404 — never match the
            // single-segment /health or the /item/<id> shape.
            let a = synth_segment(&mut rng);
            let b = synth_segment(&mut rng);
            let resp = client
                .get(format!("{base}/x/{a}/{b}"))
                .send()
                .expect("send");
            assert_eq!(resp.status().as_u16(), 404, "path=/x/{a}/{b}");
            total += 1;
        }
    }
    assert!(total >= 400, "fuzz coverage shortfall: {total}");
}

#[test]
fn route_registration_dispatch_is_panic_free_on_random_paths() {
    // Pure in-process (no socket): register a parametric route, then
    // throw random concrete paths at the dispatcher. The point is
    // panic-freedom + match-stability, not network success.
    let seeds: [u64; 3] = [1, 2, 3];
    let mut total = 0;
    for &seed in &seeds {
        let mut rng = Lcg::new(seed);
        for _ in 0..200 {
            let mut app = App::new();
            let seg = synth_segment(&mut rng);
            // Register /<seg>/<id>; vary the literal each iteration.
            let route = format!("/{seg}/<id>");
            app.get(&route, |req: Request| {
                Response::text(req.path_param("id").unwrap_or("").to_owned())
            })
            .expect("register");

            // A matching path captures; a mismatching prefix 404s.
            let id = synth_segment(&mut rng);
            let hit = app.dispatch_for_test("GET", &format!("/{seg}/{id}"));
            assert!(hit.is_some(), "expected match for /{seg}/{id}");
            let miss = app.dispatch_for_test("GET", &format!("/nope/{id}/extra"));
            assert!(miss.is_none());
            total += 1;
        }
    }
    assert!(total >= 400, "fuzz coverage shortfall: {total}");
}
