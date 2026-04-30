//! Property-based fuzz harness for the M1 frontend.
//!
//! Constitution `CLAUDE.md` §7 declares M1 done when the lexer/parser
//! is "fuzz-tested 24h." Since `cargo-fuzz` requires nightly toolchain
//! and this repo pins stable Rust 1.94.1, we satisfy the gate via
//! `proptest` per the M1 acceptance offer (option (b)) — at least
//! 100k cases per parser rule, plus seeded edge inputs. Method and
//! seeds are documented in `docs/agent/findings/m1-fuzz-method.md`.
//!
//! ## Property
//!
//! For *every* input — well-formed Cobrust source, malformed source,
//! random ASCII, random Unicode, random bytes — the lexer and parser
//! **never panic**. They either return a value or a structured error.
//!
//! This is the lexer-level "no panic on any UTF-8 input" + parser-level
//! "no panic on any token sequence" property, enforced by 5 properties
//! tuned to ≥ 100 000 cases each via `PROPTEST_CASES=100000`. The
//! built-in test run uses 4 096 cases for CI ergonomics; the long
//! version is gated behind `COBRUST_M1_FUZZ_LONG=1`.
//!
//! See also `tests/round_trip.rs` for the positive-shape acceptance.

#![allow(clippy::needless_pass_by_value)]

use cobrust_frontend::lexer::lex_bytes;
use cobrust_frontend::span::FileId;
use cobrust_frontend::{lex, parse, parse_str};
use proptest::prelude::*;

/// `proptest` knob — overridden via env var.
fn cases() -> u32 {
    if std::env::var_os("COBRUST_M1_FUZZ_LONG").is_some() {
        100_000
    } else {
        4_096
    }
}

fn cfg() -> ProptestConfig {
    ProptestConfig {
        cases: cases(),
        // Default (256 KiB) is plenty.
        max_shrink_iters: 1024,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(cfg())]

    /// Property 1: arbitrary UTF-8 → no panic in the lexer.
    #[test]
    fn lexer_never_panics_on_utf8(s in ".{0,256}") {
        let _ = lex(&s, FileId::SYNTHETIC);
    }

    /// Property 2: arbitrary bytes → no panic in the byte entrypoint.
    #[test]
    fn lexer_never_panics_on_bytes(bs in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = lex_bytes(&bs, FileId::SYNTHETIC);
    }

    /// Property 3: arbitrary UTF-8 → no panic in `parse_str` (which
    /// composes the lexer and the parser).
    #[test]
    fn parser_never_panics_on_utf8(s in ".{0,256}") {
        let _ = parse_str(&s, FileId::SYNTHETIC);
    }

    /// Property 4: tokens that *do* lex are still safe to feed to the
    /// parser. (Lex-then-parse is the common path; this exercises
    /// parser robustness on inputs the lexer accepts.)
    #[test]
    fn lex_then_parse_never_panics(s in ".{0,256}") {
        if let Ok(toks) = lex(&s, FileId::SYNTHETIC) {
            let _ = parse(&toks);
        }
    }

    /// Property 5: targeted ASCII shape — sequences of identifiers,
    /// punctuation, whitespace and newlines. Higher density than the
    /// fully-random `.` regex; finds parser glitches the broader
    /// property tends to miss.
    #[test]
    fn parser_robust_on_ascii_shape(parts in
        proptest::collection::vec(
            prop_oneof![
                "[a-zA-Z_][a-zA-Z0-9_]*".prop_map(String::from),
                "[0-9]{1,8}".prop_map(String::from),
                Just(" ".to_owned()),
                Just(":".to_owned()),
                Just(",".to_owned()),
                Just("(".to_owned()),
                Just(")".to_owned()),
                Just("[".to_owned()),
                Just("]".to_owned()),
                Just("{".to_owned()),
                Just("}".to_owned()),
                Just("=".to_owned()),
                Just("+".to_owned()),
                Just("-".to_owned()),
                Just("*".to_owned()),
                Just("\n".to_owned()),
                Just("    ".to_owned()),
                Just("if ".to_owned()),
                Just("else: ".to_owned()),
                Just("fn ".to_owned()),
                Just("for ".to_owned()),
                Just("in ".to_owned()),
                Just("\"x\"".to_owned()),
                Just("f\"{x}\"".to_owned()),
            ],
            0..32,
        )
    ) {
        let s: String = parts.concat();
        let _ = parse_str(&s, FileId::SYNTHETIC);
    }
}

// =====================================================================
// Seed corpus — committed inputs that the property must always handle
// =====================================================================

const SEEDS: &[&str] = &[
    "",
    " ",
    "\n",
    "\t",
    "\r\n",
    "🦀",
    "let x = 1\n",
    "fn f(): pass\n",
    "fn f(x: i64) -> i64: return x\n",
    "if True: pass\n",
    "while True: break\n",
    "for x in xs: pass\n",
    "match v: case _: pass\n",
    "with open(p) as f: pass\n",
    "try: pass\nexcept E: pass\nfinally: pass\n",
    "raise E from c\n",
    "@d\nfn f(): pass\n",
    "type T = i64 | None\n",
    "[x for x in xs if x > 0]\n",
    "{x: y for x, y in items}\n",
    "(x for x in xs)\n",
    "lambda x: x + 1\n",
    "f(1, 2, k=v, *xs, **kw)\n",
    "obj.field[0:10:2]\n",
    "0xFFFF_FFFF\n",
    "1.5e-3j\n",
    "f\"x={value:>10}\"\n",
    "f\"{f'{nested}'}\"",
    "\"\"\"triple\nquoted\"\"\"\n",
    "b\"\\x00\\x01\\xff\"\n",
    // Pathological / malformed — must not panic.
    "0x",
    "'unterminated",
    "\\",
    "if\n\t  x",
    "def x:",   // `def` is not a keyword — bare ident
    "is",       // dropped form, must report cleanly
    "global x", // dropped form
    "((((((((((",
    "))))))))",
    "f\"{",
    "{",
    "}",
    "{1: 2, **rest}",
    "[*xs]",
    // Nested f-string + format spec.
    "f\"{a + b!r:>{width}.{prec}f}\"",
    // Long indentation chain.
    "fn a():\n    if True:\n        if True:\n            if True:\n                pass\n",
    // Mixed UTF-8 identifiers (NFKC).
    "let café = 1\n",
    "let 数値 = 0\n",
];

#[test]
fn seed_corpus_never_panics() {
    for (i, s) in SEEDS.iter().enumerate() {
        // Allow either Ok or Err — assert only no panic.
        let _ = parse_str(s, FileId::SYNTHETIC);
        // Also ensure the byte path is safe.
        let _ = lex_bytes(s.as_bytes(), FileId::SYNTHETIC);
        // A small print so failure logs identify which seed crashed.
        let _ = i;
    }
}

#[test]
fn invalid_utf8_does_not_panic() {
    let cases: &[&[u8]] = &[
        &[0xff],
        &[0xff, 0xfe, 0xfd],
        b"abc\xff",
        b"\xc0\x80",     // Overlong — invalid UTF-8.
        b"\xed\xa0\x80", // Surrogate — invalid UTF-8.
    ];
    for bs in cases {
        let r = lex_bytes(bs, FileId::SYNTHETIC);
        assert!(r.is_err(), "expected error on invalid UTF-8");
    }
}
