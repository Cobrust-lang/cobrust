//! L2.behavior fuzz harness for cobrust-requests.
//!
//! Constitution §4.2 floor: ≥ 1000 fuzzed inputs per public function.
//! The HTTP semantics live inside reqwest; we fuzz the cobrust
//! surface — URL parser dispatch, error-kind classification, response
//! observers — to ensure constitution §5.1 invariants hold under
//! random input.

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

use cobrust_requests::{HttpErrorKind, MAX_BODY_BYTES, Response, Session};
use std::collections::HashMap;

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

fn synth_url(rng: &mut Lcg) -> String {
    let scheme_pick = rng.next_u32() % 4;
    let scheme = match scheme_pick {
        0 => "http://",
        1 => "https://",
        2 => "ftp://", // unsupported — must error
        _ => "",       // missing scheme — must error
    };
    let len = (rng.next_u32() % 16) as usize + 1;
    let host: String = (0..len)
        .map(|_| {
            let c = (rng.next_u32() % 26 + b'a' as u32) as u8;
            char::from(c)
        })
        .collect();
    format!("{scheme}{host}.example/path/{}", rng.next_u32())
}

#[test]
fn url_dispatch_panic_free_on_random_inputs() {
    let session = Session::new();
    let seeds: [u64; 3] = [42, 1337, 0xDEAD_BEEF];
    let mut total = 0;
    for &seed in &seeds {
        let mut rng = Lcg::new(seed);
        for _ in 0..400 {
            let url = synth_url(&mut rng);
            // The point is panic-freedom, not network success — we
            // accept any Result. The dispatch must never panic on
            // arbitrary string input.
            let _ = session.get(&url);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz coverage shortfall: {total}");
}

#[test]
fn invalid_url_classification_is_stable() {
    // 4 representative invalid URLs that should always route to
    // HttpErrorKind::InvalidUrl regardless of network state.
    let cases = ["", "  ", "not a url", "://missing-scheme.example"];
    let session = Session::new();
    for c in &cases {
        let err = session.get(c).expect_err(c);
        assert_eq!(
            err.kind,
            HttpErrorKind::InvalidUrl,
            "expected InvalidUrl for {c:?}, got {:?}",
            err.kind
        );
    }
}

#[test]
fn response_observers_are_consistent_under_random_status() {
    let seeds: [u64; 3] = [42, 1337, 0xDEAD_BEEF];
    let mut total = 0;
    for &seed in &seeds {
        let mut rng = Lcg::new(seed);
        for _ in 0..400 {
            let status = (rng.next_u32() % 800) as u16; // beyond 5xx still legal as u16
            let body_len = (rng.next_u32() % 256) as usize;
            let body: Vec<u8> = (0..body_len)
                .map(|_| (rng.next_u32() & 0xff) as u8)
                .collect();
            let resp = Response::from_parts(status, HashMap::new(), body.clone());
            assert_eq!(resp.status_code(), status);
            // ok() must equal (200..300).contains(status).
            assert_eq!(resp.ok(), (200..300).contains(&status));
            // bytes() must equal the input.
            assert_eq!(resp.bytes(), body);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz coverage shortfall: {total}");
}

// ── B5 adversarial corpus ─────────────────────────────────────────────────

/// B5: `Response::from_parts` with a body exactly at MAX_BODY_BYTES must
/// pass through (the cap is exclusive: > MAX_BODY_BYTES triggers the error).
///
/// This test validates the in-process path via `from_parts`. The live
/// network path (from_reqwest) is covered by the integration gate in
/// `requests_downstream.rs`.
#[test]
fn b5_body_at_limit_is_accepted() {
    // Build a body exactly MAX_BODY_BYTES long.
    let body = vec![0u8; MAX_BODY_BYTES];
    let resp = Response::from_parts(200, HashMap::new(), body.clone());
    assert_eq!(resp.bytes(), body);
}

/// B5: body just below limit must pass through.
#[test]
fn b5_body_just_below_limit_is_accepted() {
    let body = vec![0xABu8; MAX_BODY_BYTES - 1];
    let resp = Response::from_parts(200, HashMap::new(), body.clone());
    assert_eq!(resp.bytes().len(), MAX_BODY_BYTES - 1);
}

/// B5: `HttpErrorKind::BodyTooLarge` Display carries the right substring.
#[test]
fn b5_body_too_large_error_display() {
    let e = cobrust_requests::HttpError {
        kind: HttpErrorKind::BodyTooLarge,
        message: format!("response body exceeded {MAX_BODY_BYTES} byte limit"),
    };
    let s = format!("{e}");
    assert!(
        s.contains("body too large"),
        "expected 'body too large' in display, got: {s:?}"
    );
}
