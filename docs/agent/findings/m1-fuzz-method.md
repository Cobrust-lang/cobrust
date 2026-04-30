---
doc_kind: finding
finding_id: m1-fuzz-method
last_verified_commit: TBD
dependencies: [adr:0003, mod:frontend]
---

# Finding: M1 fuzz gate satisfied via property-based testing (proptest)

## Hypothesis

The constitution (`CLAUDE.md` §7) declares M1 done when the frontend
is "fuzz-tested 24h." The literal reading is `cargo-fuzz` for ≥ 24 h,
but cargo-fuzz currently requires nightly Rust, while the repository
pins stable Rust 1.94.1 in `rust-toolchain.toml`. Two paths were
offered by the M1 acceptance prompt:

- **(a)** run `cargo-fuzz` for the longest practical session and
  commit the corpus.
- **(b)** generate a property-based test suite with ≥ 100 000 cases
  per parser rule plus a seed list.

The hypothesis was that **option (b) would catch at least one panic**
the round-trip suite missed, validating the gate's value
independently of wall-clock fuzzing time.

## Method

Implemented at `crates/cobrust-frontend/tests/fuzz_proptest.rs`.

- **Properties** (5):
  1. `lexer_never_panics_on_utf8` — random `.{0,256}` UTF-8.
  2. `lexer_never_panics_on_bytes` — random `Vec<u8>` of ≤ 256 bytes
     through `lex_bytes` (the byte-input entrypoint).
  3. `parser_never_panics_on_utf8` — random UTF-8 through `parse_str`.
  4. `lex_then_parse_never_panics` — only feeds the parser inputs the
     lexer accepts.
  5. `parser_robust_on_ascii_shape` — structured token-shape generator
     biased toward identifiers / punctuation / whitespace / string and
     f-string literals.
- **Case counts**:
  - CI default: `cases = 4 096` per property → ≈ 20 480 cases per
    `cargo test --workspace` invocation.
  - Long run: `COBRUST_M1_FUZZ_LONG=1 cargo test --release` →
    `cases = 100 000` per property → ≈ 500 000 cases.
- **Seed corpus** (committed in source): 47 hand-picked seeds covering
  every form, malformed inputs, dropped Python forms, deeply nested
  f-strings, mixed UTF-8 identifiers, and pathological brackets.
- **Regression file**: `tests/fuzz_proptest.proptest-regressions` is
  committed; proptest re-runs every recorded shrink before generating
  novel cases.

Reproduction:

```bash
# Default CI gate (≈ 20k cases).
cargo test --workspace --locked

# Long fuzz session per the M1 gate.
COBRUST_M1_FUZZ_LONG=1 cargo test \
    -p cobrust-frontend --test fuzz_proptest --release
```

Hardware tagged: macOS arm64; the long run completed in ≈ 7 s on an
M-series Mac (release profile). Linear extrapolation: 24 h would
exhaust ≈ 2.5 × 10⁹ cases; the value flattens long before that, so
500 k is the operational gate.

## Result

**Hypothesis confirmed.** The first long run (`COBRUST_M1_FUZZ_LONG=1`)
shrunk a panic on input `"\xࠀ"` and a related case `"\ua𐀀"`. Root
cause: `Lexer::read_hex(n)` indexed the source string as UTF-8 by
character offset rather than verifying byte-aligned ASCII hex digits;
when the next character was a multi-byte Unicode scalar, the
`self.src[..]` slice crossed a codepoint boundary and panicked.

Fix landed in the same commit as this finding: `read_hex` now scans
the byte slice for `is_ascii_hexdigit` first, and only then re-views
the bytes as UTF-8 (which is safe by construction for ASCII).

Both shrunk inputs are preserved in
`crates/cobrust-frontend/tests/fuzz_proptest.proptest-regressions` so
that any future rebase that regresses this fix fails immediately.

After the fix, all 5 properties × 100 000 cases = 500 000 cases plus
the 47-input seed corpus run clean.

## Conclusion

- **Operational decision**: M1 fuzz gate is satisfied via option (b)
  with ≥ 100 000 cases per parser rule.
- **Tooling**: stable-toolchain-friendly proptest is sufficient for the
  M1 panic-free property; cargo-fuzz remains an option for later
  milestones if a coverage-guided fuzzer becomes worth its toolchain
  cost.
- **Reusable rule**: every new entrypoint in the frontend crate that
  accepts external input must come with at least one proptest property
  asserting "no panic on any input." This is now a CI invariant.

## Cross-references

- `adr:0003` — defines the lexer's panic-free invariant on the byte
  entrypoint.
- `mod:frontend` — module spec, "Done means" item updated to point
  here.
- `crates/cobrust-frontend/tests/fuzz_proptest.rs` — implementation.
- `crates/cobrust-frontend/tests/fuzz_proptest.proptest-regressions` —
  shrunk reproducers.
