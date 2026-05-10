---
doc_kind: finding
finding_id: msgpack-fuzz-190gib-allocation
last_verified_commit: 25dd034
discovered_by: m9-cross-arch sub-agent (af85fad72157d2dcf, sonnet) on 2026-05-09 — promoted to independent finding by review-claude handoff §A.5; independently reproduced by P7 sonnet sprint 2026-05-09
severity: P1 (denial-of-service via attacker-controlled msgpack input; not silent miscompile)
related: [m9-cross-arch-linux-x86_64-validation, m9-cross-arch-9ff481c-regression]
status: open
---

# Finding: cobrust-msgpack fuzz harness allocates 190 GiB on adversarial input

## Hypothesis

The `unpack_panic_free_on_random_garbage` fuzz test in `cobrust-msgpack` violates
its own stated contract — "no input panics; every input either succeeds or returns
`Err(MsgError)`" — because one of the deterministic fuzz seeds produces a byte
sequence that begins with the MAP_32 format code (`0xdf`) followed by a 4-byte
big-endian length field containing a multi-billion element count. The parser calls
`Vec::with_capacity(length)` unconditionally before attempting to read any data,
causing the OS allocator to abort the process with SIGABRT.

## Method

- **SSH target**: &lt;internal validator host — Ubuntu 22.04 x86_64, 40 cores / 62 GiB RAM&gt;.
- **Toolchain**: rustc 1.94.1 / cargo 1.94.1 (matches `rust-toolchain.toml`).
- **Sync**: `rsync` of HEAD `25dd034` to `~/cobrust-msgpack-orphan/`.
- **Command run**:
  ```
  RUST_BACKTRACE=full cargo test --package cobrust-msgpack --test msgpack_fuzz \
      -- unpack_panic_free_on_random_garbage
  ```
- **Triggering input reconstructed** via LCG simulation (`Lcg::new(42)`, iteration 233):
  - Random byte-slice length: 6 bytes (determined by `rng.next_u32() % 16 + 1`)
  - Byte sequence: `[0xdf, 0xcb, 0x99, 0xda, 0xe0, 0x8e]`
  - `0xdf` = MAP_32 format code; next 4 bytes = big-endian length `0xcb99dae0`.

## Result

### Gate outcome

```
running 1 test
memory allocation of 191288041728 bytes failed
stack backtrace:
  ...
  cobrust_msgpack::parser::unpack_map
      at ./crates/cobrust-msgpack/src/parser.rs:493:44
  cobrust_msgpack::parser::unpack_one
      at ./crates/cobrust-msgpack/src/parser.rs:634:28
  cobrust_msgpack::parser::unpack
      at ./crates/cobrust-msgpack/src/parser.rs:383:24
  msgpack_fuzz::unpack_panic_free_on_random_garbage
      at ./tests/msgpack_fuzz.rs:191:21
...
error: test failed (signal: 6, SIGABRT)
```

### Allocation site

- **File**: `crates/cobrust-msgpack/src/parser.rs`
- **Line**: 493 — `Vec::with_capacity(length)` inside `unpack_map`
- **Function signature**: `pub fn unpack_map(data: &[u8], pos: usize, length: usize)`

### Attempted allocation size

- `length` = `0xcb99dae0` = 3,415,857,888 entries
- `sizeof::<(String, MsgValue)>` on x86_64 = 56 bytes
- Attempted heap request: `3,415,857,888 × 56` = **191,288,041,728 bytes** (≈ 178.2 GiB)
- System RAM: 62 GiB — allocation fails → `handle_alloc_error` → SIGABRT

### Triggering input (concrete)

| Field | Value |
|---|---|
| LCG seed | `42` |
| Fuzz loop iteration | 233 |
| Byte sequence | `[0xdf, 0xcb, 0x99, 0xda, 0xe0, 0x8e]` |
| Parser interpretation | MAP_32 marker + 4-byte length = 3,415,857,888 |
| `Vec::with_capacity` argument | 3,415,857,888 |

### Failing test name

`msgpack_fuzz::unpack_panic_free_on_random_garbage` (in
`crates/cobrust-msgpack/tests/msgpack_fuzz.rs`)

### Passing tests

- `pack_unpack_round_trips_panic_free`: not affected. Uses the bounded
  `synth_value` generator (array/map depth ≤ 2, array/map size ≤ 3) — the
  generated `MsgValue` trees never produce MSG-32 entries with giant lengths.
- `pack_uint_smallest_encoding_picked`: not affected. Uses hard-coded value pairs.

## Root-cause analysis

### Why the crash happens

`unpack_map` at `parser.rs:493` calls `Vec::<(String, MsgValue)>::with_capacity(length)`
immediately on receiving the declared element count from the msgpack header — **before
reading any actual data bytes**. The msgpack MAP_32 header is a 5-byte sequence: one
format-code byte (`0xdf`) plus four big-endian length bytes. Any byte sequence of length
≥ 5 that starts with `0xdf` will cause `unpack_map` to be called with a length that could
be up to `u32::MAX` (4,294,967,295). Attempting `Vec::with_capacity(u32::MAX)` for a
type with sizeof ≥ 4 bytes will exhaust any practical system's RAM and trigger SIGABRT.

The same structural pattern exists in `unpack_array` at `parser.rs:401`:
```rust
let mut out: Vec<MsgValue> = Vec::with_capacity(length);
```
`sizeof::<MsgValue>` on x86_64 is 40 bytes; `u32::MAX × 40` ≈ 160 GiB. This site is
equally vulnerable. Neither fuzz seed happened to trigger it in the current 1200-iteration
corpus, but it will fail on any input starting with `0xdd` (ARRAY_32) + large 4-byte length.

### Why the contract is violated

The test's docstring states: "no input panics (every input either succeeds or returns
`Err(MsgError)`)". A SIGABRT from an out-of-memory allocation abort is NOT a Rust panic
and cannot be caught by `std::panic::catch_unwind`. The `let _ = unpack(&raw)` line
intends to silently discard both `Ok` and `Err` results — but `alloc_error_handler`
bypasses the Result machinery entirely and aborts the process.

### Why macOS arm64 does not trigger this

The test harness runs all 3 seeds × 400 iterations sequentially in a single thread.
On macOS arm64 (Apple Silicon MacBooks typically have 16–64 GiB unified memory), the
system overcommits memory: `Vec::with_capacity(3_415_857_888)` on macOS succeeds (the
virtual address space is allocated without immediate physical backing), the loop then
attempts to read `data[cursor]` for 3.4 billion iterations, hits the slice bounds check
`cursor >= data.len()` on the first or second iteration and returns `Err(MsgError::unpack("EOF
before value"))`, which the harness discards. No crash.

On Linux x86_64, the default kernel overcommit policy (`/proc/sys/vm/overcommit_memory`
= 0 = heuristic) refuses to commit a single 178 GiB allocation when only 62 GiB RAM is
available. The allocator invokes `handle_alloc_error`, which aborts the process.

This is a **Linux-only observable failure** for the current allocation size and RAM;
on a Linux system with ≥ 300 GiB RAM it would silently pass (slowly iterating through
3.4 billion empty-read round trips).

### Vulnerable sites summary

| File | Line | Function | Allocation | Max size | Safe? |
|---|---|---|---|---|---|
| `parser.rs` | 401 | `unpack_array` | `Vec::<MsgValue>::with_capacity(length)` | `u32::MAX × 40` ≈ 160 GiB | **NO** |
| `parser.rs` | 493 | `unpack_map` | `Vec::<(String, MsgValue)>::with_capacity(length)` | `u32::MAX × 56` ≈ 214 GiB | **NO** — confirmed fatal |
| `parser.rs` | 648 | `unpack_str` | `data[pos..pos+length].to_vec()` | bounded by `data.len()` | YES (bounds-checked) |
| `parser.rs` | 421 | `unpack_bin` | `data[pos..pos+length].to_vec()` | bounded by `data.len()` | YES (bounds-checked) |

`unpack_str` and `unpack_bin` are safe because they check `pos + length > data.len()`
before allocating. `unpack_array` and `unpack_map` make no such check.

## Conclusion

**P1 — denial-of-service surface in the msgpack parser.**

Any byte stream that begins with `0xdf <4 bytes of large length>` or
`0xdd <4 bytes of large length>` will cause `cobrust-msgpack::unpack` to crash the
calling process on Linux x86_64 with systems that enforce overcommit limits (most
production Linux deployments). This includes any Cobrust application that unpacks
attacker-controlled msgpack data.

The immediate impact on CI/CD is that the `msgpack_fuzz` test fails on the
<self-hosted-runner> (the project's only Linux x86_64 gate), preventing that test
suite from being green on Linux. The fuzz test's panic-freedom contract is violated.

Not blocking M-batch / current milestone closure since `cobrust-msgpack` is
feature-flagged and not yet in the hot path of any shipped artifact. However, this
must be resolved before any downstream library that ingests msgpack data from
untrusted sources is delivered.

## Recommended fix direction

**Primary (targeted — covers both confirmed vulnerable sites):**

Add an input-relative size cap before each `Vec::with_capacity` in `unpack_array`
and `unpack_map`. The canonical guard pattern is:

```rust
// In unpack_map (parser.rs ~L490):
const MAX_PREALLOC: usize = 65_536;   // 64 KiB elements — safe on any platform
let prealloc = length.min(MAX_PREALLOC);
let mut out: Vec<(String, MsgValue)> = Vec::with_capacity(prealloc);
```

(Same pattern for `unpack_array`.)

The cap of 64 KiB elements is conservative. Legitimate msgpack payloads rarely
have flat arrays/maps with > 65 K entries in a single level; if they do, the
`Vec` will still grow correctly (just without the pre-allocation hint matching
the declared element count). The performance penalty for capping at 64 K is
negligible for realistic inputs and eliminates the DoS surface entirely.

**Alternative (strict — validates against input length):**

Before calling `Vec::with_capacity(length)`, verify that `length` is achievable
given the remaining input bytes:

```rust
// Each element in a map requires at least 2 bytes (1 fixstr key + 1 nil value).
if pos + length * 2 > data.len() {
    return Err(MsgError::unpack("declared map length exceeds remaining input"));
}
```

This is tighter but adds a multiply; for the adversarial-input fuzz harness it also
changes the failure mode from SIGABRT to `Err(MsgError)`, which is the correct
contract-respecting behavior.

The cap approach is recommended first because it is mechanical, has no false
positives, and does not require reasoning about per-element minimum sizes.

## Cross-references

- finding `m9-cross-arch-linux-x86_64-validation.md` §L176–179 (original mention,
  dismissed as "separate, pre-existing fuzz-knob issue unrelated to ADR-0033")
- finding `m9-cross-arch-9ff481c-regression.md` §"Resolution addendum" last paragraph
  (second mention, "still no independent finding; queued P7 sonnet per review-claude
  handoff §A.5")
- `crates/cobrust-msgpack/src/parser.rs` lines 401 (`unpack_array`) and 493 (`unpack_map`)
  — the two vulnerable `Vec::with_capacity` sites
- `crates/cobrust-msgpack/tests/msgpack_fuzz.rs` line 191 (`unpack_panic_free_on_random_garbage`)
  — the failing test
- ADR-0010 (`docs/agent/adr/0010-native-ext-translation.md`) §"Decision" —
  native-ext translation methodology; its L2.behavior gate requires panic-freedom;
  DoS-resistance should be an explicit item in the gate criteria
