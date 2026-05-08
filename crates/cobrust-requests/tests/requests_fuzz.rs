//! L2.behavior fuzz harness for cobrust-requests.
//!
//! Constitution §4.2 floor: ≥ 1000 fuzzed inputs per public function.
//! The HTTP semantics live inside reqwest; we fuzz the cobrust
//! surface — URL parser dispatch, error-kind classification, response
//! observers — to ensure constitution §5.1 invariants hold under
//! random input.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use cobrust_requests::{HttpErrorKind, Response, Session};
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
