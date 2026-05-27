//! L2.behavior fuzz harness for cobrust-sqlite3.
//!
//! Constitution §4.2 floor: ≥ 1000 fuzzed inputs per public function
//! (the project fuzz convention used by `cobrust-requests` etc.). The
//! SQLite engine itself is exercised by rusqlite's own suite; here we
//! fuzz the cobrust surface — qmark binding + value round-trip +
//! fetchone/fetchall consistency — to ensure constitution §5.1
//! invariants (no panic, Err never lost) hold under random input
//! across multiple seeds.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]

use cobrust_sqlite3::{MEMORY, Value, connect};

/// SplitMix64 — small, seedable, reproducible PRNG (same construction
/// the `cobrust-requests` fuzz harness uses).
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Synthesize one of the five storage-class values from the RNG.
fn synth_value(rng: &mut Rng) -> Value {
    match rng.next_u64() % 5 {
        0 => Value::Null,
        1 => Value::Integer(rng.next_u64() as i64),
        2 => {
            // A finite f64 (avoid NaN/inf so equality round-trips).
            let bits = rng.next_u64();
            let f = f64::from_bits(bits);
            if f.is_finite() {
                Value::Real(f)
            } else {
                Value::Real((rng.next_u64() % 1_000_000) as f64 / 7.0)
            }
        }
        3 => {
            let len = (rng.next_u64() % 16) as usize;
            let s: String = (0..len)
                .map(|_| char::from(b'a' + (rng.next_u64() % 26) as u8))
                .collect();
            Value::Text(s)
        }
        _ => {
            let len = (rng.next_u64() % 16) as usize;
            let b: Vec<u8> = (0..len).map(|_| (rng.next_u64() & 0xff) as u8).collect();
            Value::Blob(b)
        }
    }
}

#[test]
fn random_insert_select_round_trips_are_lossless() {
    let seeds: [u64; 3] = [42, 1337, 0xDEAD_BEEF];
    let mut total = 0usize;
    for &seed in &seeds {
        let mut rng = Rng::new(seed);
        let conn = connect(MEMORY).expect("open");
        let mut cur = conn.cursor();
        cur.execute("CREATE TABLE fz (rowid INTEGER PRIMARY KEY, v)", &[])
            .expect("create");

        // Insert a few hundred random cells per seed, reading each one
        // back immediately and asserting the round-trip is lossless.
        for _ in 0..400 {
            let v = synth_value(&mut rng);
            cur.execute("INSERT INTO fz (v) VALUES (?)", std::slice::from_ref(&v))
                .expect("insert");
            let rowid = cur.lastrowid().expect("insert sets lastrowid");

            cur.execute("SELECT v FROM fz WHERE rowid = ?", &[Value::Integer(rowid)])
                .expect("select");
            let row = cur.fetchone().expect("inserted row exists");
            assert_eq!(
                row.get(0),
                Some(&v),
                "round-trip lossy for seed {seed} rowid {rowid}"
            );
            assert!(cur.fetchone().is_none(), "exactly one row expected");
            total += 1;
        }

        // fetchall over the whole table must return exactly the count
        // inserted, and fetchone-then-fetchall must be consistent.
        cur.execute("SELECT v FROM fz ORDER BY rowid", &[])
            .expect("select all");
        let first = cur.fetchone();
        let rest = cur.fetchall();
        assert_eq!(
            usize::from(first.is_some()) + rest.len(),
            400,
            "fetchone + fetchall must cover all rows exactly once"
        );
    }
    assert!(total >= 1000, "fuzz coverage shortfall: {total}");
}

#[test]
fn random_qmark_binding_never_panics() {
    // Drive arbitrary param sequences (including count mismatches) at a
    // fixed statement; the surface must always return Ok/Err, never
    // panic (constitution §5.1).
    let conn = connect(MEMORY).expect("open");
    let mut cur = conn.cursor();
    cur.execute("CREATE TABLE p (a, b)", &[]).expect("create");

    let mut rng = Rng::new(0x5EED);
    let mut total = 0usize;
    for _ in 0..1000 {
        let n = (rng.next_u64() % 4) as usize; // 0..=3 params for a 2-placeholder stmt
        let params: Vec<Value> = (0..n).map(|_| synth_value(&mut rng)).collect();
        // n == 2 should Ok; everything else should be a Parameter Err.
        let result = cur.execute("INSERT INTO p (a, b) VALUES (?, ?)", &params);
        // A 2-placeholder statement binds iff exactly 2 params are
        // supplied; any other arity must be a clean Err (never a panic).
        let bound_ok = result.is_ok();
        assert_eq!(
            bound_ok,
            n == 2,
            "{n} params against a 2-placeholder stmt: expected bind={}, got bind={bound_ok}",
            n == 2
        );
        total += 1;
    }
    assert!(total >= 1000, "fuzz coverage shortfall: {total}");
}
