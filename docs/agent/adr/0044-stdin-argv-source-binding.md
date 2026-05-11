---
doc_kind: adr
adr_id: 0044
title: "Source-level stdin + argv binding for Cobrust user programs (W2 LeetCode wedge)"
status: accepted
date: 2026-05-11
last_verified_commit: TBD
supersedes: []
superseded_by: []
relates_to: [adr:0019, adr:0024, adr:0025, adr:0027, adr:0030, adr:0034, adr:0038]
discovered_by_review: review-claude Option C §B W2 leetcode wedge sprint
---

# ADR-0044: Source-level stdin + argv binding for Cobrust user programs (W2 LeetCode wedge)

## Context

### Strategic motivation — W2 user wedge

Cobrust at HEAD `769a5d8` (v0.1.1 shipped 2026-05-11) has all 14 Phase E
milestones merged, all 8 H1-H8 Python semantics drifts closed, and 5/5
tomli functions real-LLM translated. **But the project owner cannot
write a LeetCode solution in Cobrust** because:

- `examples/cat.cb`, `wc.cb`, `echo.cb`, `sort.cb`, `unique_lines.cb`,
  `regex_grep.cb`, `csv_sum.cb` are all **stubs** — the M12.x docstring
  comments say "Source-level intent (full program once stdin reading
  is wired end-to-end through codegen — Phase F)". They print
  hardcoded literals because **source-level `stdin` is not callable
  from `.cb`**.
- `std.env.args()` is implemented at the Rust stdlib tier
  (`crates/cobrust-stdlib/src/env.rs:21`) and the runtime shim
  (`crates/cobrust-stdlib/src/runtime.rs:172` —
  `__cobrust_capture_argv`) is linked into every executable via
  `crates/cobrust-cli/runtime/cobrust_main.c:25`. But **no Cobrust
  source can call `std.env.args()`** because the module-path
  resolution machinery for stdlib calls is not wired through MIR.

User self-diagnosis 2026-05-11: "刷不了 leetcode, 很多东西都没做完".
review-claude Option C §B confirmed this is the
**milestone-vs-user-traction gap** signal — Cobrust shipped milestones
M0..M14 + 0.1.0-beta + v0.1.1 without a single program reading stdin.

### Constitution alignment

- §1.1 "syntactically familiar to Python users" → user expects
  `s = input()` to work (Python `input()`).
- §1.2 dual mandate puts the language half on equal footing with the
  translator half. Translator is "production-validated for single
  library"; language is "can compile fizzbuzz but not Two Sum".
- §2.2 drop-list bound: "exceptions as default error path" replaced by
  `Result<T, E>`. stdin EOF semantics must follow this binding.
- §3.3 atomic-commit doc rule: this ADR's implementation must ship zh
  + en getting-started doc updates in the same commit.

### Existing surface — what is already wired

Critical for scoping the work. Per `grep -rnE "CAPTURED_ARGS|__cobrust_capture_argv|read_line"` at HEAD `769a5d8`:

| Surface | Status | File:line |
|---|---|---|
| `std.io.read_line() -> Result<String, Error>` | **Rust-side exists** | `crates/cobrust-stdlib/src/io.rs:110` |
| `std.io.stdin().read_line() / read_all()` | **Rust-side exists** | `crates/cobrust-stdlib/src/io.rs:144,151` |
| `std.env.args() -> Vec<String>` | **Rust-side exists**, reads from `CAPTURED_ARGS` | `crates/cobrust-stdlib/src/env.rs:21` |
| `__cobrust_capture_argv(argc, argv)` C ABI | **Wired into every executable** | `crates/cobrust-stdlib/src/runtime.rs:172` |
| `cobrust_main.c` shim calls `__cobrust_capture_argv` | **Linked by build.rs** | `crates/cobrust-cli/runtime/cobrust_main.c:25` |
| `__cobrust_println(*const u8, usize)` C ABI | **Wired**, called by codegen via `runtime_helper_signatures` | `crates/cobrust-codegen/src/cranelift_backend.rs:1801` |
| `__cobrust_input` C ABI | **does NOT exist** | — |
| `__cobrust_read_line` C ABI | **does NOT exist** | — |
| `__cobrust_argv_get / _len` C ABI | **does NOT exist** | — |
| Source-level `print("...")` → runtime shim | **Wired via PRELUDE+intrinsic-rewrite** | `crates/cobrust-cli/src/build.rs:37,92` + `crates/cobrust-cli/src/build/intrinsics.rs:88` |

**Key insight**: the work to make `input(prompt) -> str` and
`argv() -> list[str]` callable from `.cb` is **architecturally identical
to how `print(s: str)` was wired in M11** — extend PRELUDE with new
`fn` declarations, add intrinsic-rewrite passes for the new callsites,
add new entries to `runtime_helper_signatures()`, and add the
corresponding C-ABI shims in `cobrust-stdlib::io`.

This ADR therefore is **codification + minimal extension**, not a
greenfield design.

## Options considered

### Decision 1 — Canonical stdin API surface

#### Option 1A — Python-compat `input(prompt: str) -> str` only

- Pros:
  - 100% Python-familiar; LeetCode wedge users expect this exactly.
  - Single API to teach in getting-started doc.
  - The prompt arg is optional in Python (`input()` and
    `input("> ")` both valid); we adopt this with a non-prompt
    overload `input() -> str` via PRELUDE stub.
- Cons:
  - Python `input()` raises `EOFError` on EOF; Cobrust drops
    exceptions-as-default, so semantics diverge.
  - No way to read entire stdin (would need `read_all()`).

#### Option 1B — Rust-compat `std.io.stdin().read_line() -> Result[str, Error]` only

- Pros:
  - Result-typed; aligns with constitution §2.2.
  - Method-chain ergonomics; allows `stdin().read_all()` symmetry.
- Cons:
  - Method-on-handle is **NOT yet supported in MIR codegen** —
    method dispatch on user-defined types requires Aggregate
    fields + a vtable, which the current codegen does not emit
    (ADR-0027 only does for-protocol iteration via opaque
    `*mut u8` handles).
  - Method-on-stdlib-object would require a new MIR primitive
    or a sugar pass; both are out-of-scope for W2.
  - Python-familiar users don't reach for `stdin().read_line()`
    first.

#### Option 1C — Both, with `input()` canonical and `stdin().read_line()` deprecated

- Pros:
  - Belt-and-suspenders for power users.
- Cons:
  - Violates constitution §2.1 "One way to do each thing in the core
    language". Forces docs to explain both. **Rejected.**

#### Option 1D — `input()` canonical + free function `read_line()` for whole-stream cases (CHOSEN)

- Pros:
  - `input(prompt)` is Python-compat for the leetcode wedge.
    Returns plain `str`; EOF → empty string (matches Python's
    no-input degenerate case while sidestepping `EOFError`).
  - `read_line()` (free fn, no method dispatch) is the
    Cobrust-idiomatic Result-typed read for power users:
    `read_line() -> Result[str, IoError]`.
  - Two functions, two semantic tiers, both flat (no method
    dispatch), both implementable as PRELUDE+intrinsic-rewrite
    today.
  - W2 leetcode examples can use either; doc will show
    `input()` for the wedge audience.
- Cons:
  - Two functions instead of one (mild §2.1 tension).
  - We commit to a soft contract that `input()` returns "" on
    EOF — a deliberate divergence from Python's `EOFError`.
    Documented in the doc tree.

**Chosen**: **Option 1D**. The split mirrors Python's pragmatic
`input()` (no Result) vs Rust's `read_line()` (Result). `input()` is
the user-facing canonical for W2; `read_line()` is the
Result-typed primitive that future translator output can target.

#### W2 Phase 2 scope cap (per [P10-RATIFY-0044], 2026-05-11)

`Result[str, IoError]` typed-HIR is not yet supported by the type
checker / MIR lowering pipeline. For W2 Phase 2 only, `read_line()`
ships with the simplified signature `read_line() -> str` and EOF
surfaces as `""` (matching `input()`'s EOF→"" convention). The
Result-typed end-state per Decision 3C remains the target; it lands
in **follow-up ADR-0044a** once typed-Result lowering is in scope
(Phase F.1.x candidate). `input()` and `argv()` ship as designed in
W2 Phase 2 (MUST-SHIP).

Trailing newline preservation per Decision 5 still applies:
`read_line()` returns the line *with* its trailing `\n` (when
present); `input()` strips it.

### Decision 2 — Canonical argv API surface

#### Option 2A — Signature-extension `fn main(args: list[str]) -> i64`

- Pros:
  - Idiomatic Rust-flavor entry; args are explicit at main's
    signature.
  - argv arrives via the same param machinery as user fn args.
- Cons:
  - Codegen amendment: `_cobrust_user_main` currently has
    signature `() -> i64`. Adding a parameter requires
    coordinating with `cobrust_main.c` (would need to pass the
    args list to `_cobrust_user_main`).
  - **Materialization cost**: list[str] in MIR is an Aggregate of
    Refs to .rodata strings (ADR-0027). Constructing it at the
    C ABI boundary requires allocating a list, mallocing each str
    pointer, and copying argv elements. The runtime helper for
    `__cobrust_list_new` + `__cobrust_str_new` exist
    (`crates/cobrust-codegen/src/cranelift_backend.rs:1755,1804`)
    but stringing them together at C-ABI dispatch is non-trivial.
  - Breaks backward compat with existing `fn main() -> i64` (every
    example would need updating).

#### Option 2B — Stdlib free function `std.env.args() -> list[str]`

- Pros:
  - Symmetric with the existing Rust stdlib surface (already
    docced in ADR-0019 §M11 + ADR-0025).
- Cons:
  - Method-path resolution `std.env.args` does NOT yet lower in
    MIR — see Decision 1B same problem with method dispatch on
    module paths.
  - Requires a new MIR pass to resolve `std.X.Y` references to
    runtime calls.

#### Option 2C — Prelude-bound `argv() -> list[str]` free function (CHOSEN)

- Pros:
  - **Architecturally identical** to how `print` was wired in M11.
  - PRELUDE declares `fn argv() -> list[str]` stub; user calls
    `argv()` directly.
  - intrinsic-rewrite pass replaces callsites with the runtime
    symbol `__cobrust_argv` (new C-ABI shim).
  - C-ABI shim allocates the list, fills with str-pointer +
    length pairs from `CAPTURED_ARGS`, returns the list handle.
  - No `fn main` signature change → backward-compat preserved.
  - `std.env.args()` (Rust-side) remains the implementation; the
    Cobrust-source `argv()` is a thin alias that hits the same
    `CAPTURED_ARGS`.
- Cons:
  - Two names for the same concept across language/Rust
    boundary. Doc clarifies that `argv()` is Cobrust-source,
    `std::env::args()` is Rust-side, both read the same data.

**Chosen**: **Option 2C**. Mirrors the proven `print` PRELUDE
pattern. Future `argv: list[str]` module-global form (ADR-bumpable)
can be added once MIR module-path resolution lands.

### Decision 3 — EOF semantics for `input()`

#### Option 3A — Python-compat: raise `EOFError`

- Cons: Cobrust dropped exceptions-as-default. **Rejected.**

#### Option 3B — Return empty string `""` on EOF

- Pros:
  - Matches what `read_line` already returns (`io.rs:114-117`).
  - Lets idiomatic Cobrust code `while !s.is_empty(): ...`
    process stdin line-by-line until EOF, no exception handling.
  - Same convention as Bash `read` returning empty + exit 1
    on EOF (broadly Python's `sys.stdin.readline()` returns ""
    on EOF too).
- Cons:
  - Cannot distinguish "user pressed Enter on empty line" from
    "EOF". Acceptable for leetcode wedge (test input never
    has both).

#### Option 3C — Return `Result[str, IoError]` with `IoError::Eof`

- Pros: Pure Cobrust-idiomatic.
- Cons:
  - For `input()`, the Python-compat surface is part of the value
    proposition; forcing `let s? = input(...)` at every call site
    breaks the wedge.
  - The Result-typed primitive `read_line()` already provides
    this.

**Chosen**: **Option 3B** for `input()` (returns `""` on EOF +
trailing newline stripped); **Option 3C** for `read_line()`
(returns `Result[str, IoError]`).

The "two functions" split (Decision 1D) buys us BOTH semantics.

### Decision 4 — Encoding

UTF-8 default. Invalid UTF-8 bytes are replaced with the Unicode
replacement character `U+FFFD` (matches `String::from_utf8_lossy`
which `__cobrust_capture_argv` already uses at `runtime.rs:188`).

This is the same lossy-replacement contract as `argv` capture, so
the user-facing semantic is consistent across stdin and argv.

For binary stdin (not text), users would need a future `read_bytes()`
primitive — explicit out-of-scope (deferred to Phase F.2.x).

### Decision 5 — `read_line()` newline handling

`input(prompt) -> str`: **strips trailing newline**, like Python's
`input()`.

`read_line() -> Result[str, IoError]`: **preserves trailing newline**,
like Rust's `io::BufRead::read_line` (matches existing
`io.rs:117` behavior on the Rust side).

Documented difference; lets `read_line()` round-trip stdin to stdout
byte-perfect without re-injecting newlines.

## Decision

Adopt Options **1D + 2C + 3B/3C + UTF-8 lossy + 5**.

### Cobrust source-level surface (binding)

```python
# Read one line from stdin. EOF returns "". Trailing newline stripped.
fn input(prompt: str) -> str

# Same as input("").
fn input_no_prompt() -> str

# Read one line from stdin. EOF returns "". Trailing newline preserved.
# W2 Phase 2 scope cap per ADR-0044 Decision 1D — typed
# Result[str, IoError] deferred to ADR-0044a.
fn read_line() -> str

# Process argv as a list of strings. First element is argv[0] (program
# path). Captured at process start via __cobrust_capture_argv.
fn argv() -> list[str]
```

Stub bodies live in PRELUDE; rewrite pass redirects callsites to
runtime symbols. Stubs are dropped from MIR before codegen.

### New runtime C-ABI surface

Implemented in `crates/cobrust-stdlib/src/io.rs` (`input` / `read_line`)
and `crates/cobrust-stdlib/src/env.rs` (`argv`).

| Symbol | Signature | Behavior |
|---|---|---|
| `__cobrust_input` | `extern "C" fn(*const u8, usize) -> *mut Str` | Writes `prompt` (ptr+len) to stdout (flushed), reads one line from stdin, strips trailing `\n`, returns owned Str pointer. EOF returns empty Str. UTF-8 lossy. |
| `__cobrust_input_no_prompt` | `extern "C" fn() -> *mut Str` | Same as `__cobrust_input` with empty prompt. |
| `__cobrust_read_line` | `extern "C" fn() -> *mut Str` | Reads one line from stdin, **preserves** trailing `\n` (unlike `__cobrust_input`). EOF returns empty Str. UTF-8 lossy. W2 Phase 2 scope cap per ADR-0044 Decision 1D — typed `Result[str, IoError]` deferred to ADR-0044a. |
| `__cobrust_argv` | `extern "C" fn() -> *mut List_Str` | Materializes `CAPTURED_ARGS` into a Cobrust List<Str>. Each Str is heap-allocated via `__cobrust_str_new` and populated via `__cobrust_str_push_static`. Returns owned List handle. |

The pointer layout for `*mut Str` matches the M12.x f-string runtime
(`__cobrust_str_new` at `runtime_helper_signatures` line 1804). The
`*mut List_Str` layout is new — see § "Implementation map" for the
Aggregate shape. (`*mut Result_StrIo` deferred to ADR-0044a per
Decision 1D W2 Phase 2 scope cap.)

### Codegen amendment

Add four new entries to `runtime_helper_signatures()` at
`crates/cobrust-codegen/src/cranelift_backend.rs:1745`:

```rust
out.push(("__cobrust_input", sig(call_conv, &[p, i64], Some(p))));
out.push(("__cobrust_input_no_prompt", sig(call_conv, &[], Some(p))));
out.push(("__cobrust_read_line", sig(call_conv, &[], Some(p))));
out.push(("__cobrust_argv", sig(call_conv, &[], Some(p))));
```

No new MIR primitives. No new codegen passes. The existing
`intrinsics::rewrite_print` machinery extends to cover the four new
callsites.

### PRELUDE amendment

`crates/cobrust-cli/src/build.rs:37` PRELUDE constant adds:

```python
fn input(prompt: str) -> str:
    return ""

fn input_no_prompt() -> str:
    return ""

# W2 Phase 2 scope cap per ADR-0044 Decision 1D — typed
# Result[str, IoError] deferred to ADR-0044a
fn read_line() -> str:
    return ""

fn argv() -> list[str]:
    return []
```

The stub Bodies are dropped from MIR after the intrinsic rewrite,
identical to the `print` / `print_int` flow at
`crates/cobrust-cli/src/build/intrinsics.rs:204`.

### main signature — NO change

`fn main() -> i64` remains the canonical entry. argv is accessed via
free-function `argv()` calls in the body, not via main's parameter.
This preserves backward-compat with all existing examples.

## Implementation map (binding)

### Crate touch list

| Crate | File | What changes |
|---|---|---|
| `cobrust-stdlib` | `src/io.rs` | Add `input(prompt) -> String` Rust-side + `__cobrust_input` C-ABI shim + `__cobrust_input_no_prompt` + `__cobrust_read_line` (Result-returning C-ABI). |
| `cobrust-stdlib` | `src/env.rs` | Add `__cobrust_argv` C-ABI shim that materializes List<Str> from `CAPTURED_ARGS`. |
| `cobrust-cli` | `src/build.rs` | Extend `PRELUDE` to declare four new stub fns. |
| `cobrust-cli` | `src/build/intrinsics.rs` | Extend `rewrite_print` (or split into `rewrite_io_intrinsics`) to recognize + rewrite the four new callsites. Add new `INPUT_RUNTIME_SYMBOL` / `READ_LINE_RUNTIME_SYMBOL` / `ARGV_RUNTIME_SYMBOL` consts. |
| `cobrust-codegen` | `src/cranelift_backend.rs` | Add four entries to `runtime_helper_signatures`. |
| `examples/leetcode/` | (new dir) | 10 .cb files + per-problem README + parent README (Phase 3 deliverable). |
| `docs/human/zh/getting-started-leetcode.md` | (new) | 双语 入门文档 (Phase 4). |
| `docs/human/en/getting-started-leetcode.md` | (new) | English mirror (Phase 4). |
| `docs/agent/modules/stdlib.md` | edit | Add `input`/`read_line`/`argv` surface to §"Public surface (M11)" section + cite this ADR. |
| `docs/agent/modules/cli.md` | edit | Note the PRELUDE extension + intrinsic rewrite extension. |
| `docs/human/{zh,en}/architecture.md` | edit | Add `input` / `read_line` / `argv` to the std.io / std.env tables (lines ~1618 / ~1637 zh; mirror en). |
| `scripts/doc-coverage.sh` | edit | Add new surface terms to the M11 stdlib check list: `input`, `read_line`, `argv`, `__cobrust_input`, `__cobrust_read_line`, `__cobrust_argv`, `ADR-0044`. |
| `docs/agent/adr/README.md` | edit | Append ADR-0044 row to the roster. |
| `~/.claude/.../memory/project_state_snapshot.md` | edit | Append ADR-0044 row to roster + W2 deliverable in §"main branch state". |

### MIR / type-checker — NO change

- No new MIR opcodes.
- No new HIR forms.
- The four new prelude fns type-check exactly like `print` does today.
- intrinsic-rewrite pass operates on MIR `Terminator::Call` only;
  same machinery that rewrites `print` callsites today.

## Backward compatibility

- Every existing `.cb` example continues to compile (no PRELUDE-name
  collision — `input`/`argv`/`read_line` are new identifiers in the
  prelude namespace, no overshadow of user-defined names).
- `fn main() -> i64` signature unchanged.
- `__cobrust_capture_argv` C-ABI surface unchanged.
- `std::env::args()` Rust-side surface unchanged.
- All M11/M12/M12.x/M13/M14 integration tests pass unmodified.

## Test plan (M-level — Phase 2 P7 sonnet must satisfy)

### Tier 1 — Well-typed lowering (≥ 30 tests)

Per ADR-0019 §M11's testing rubric; lives in
`crates/cobrust-cli/tests/intrinsics_input.rs` and
`crates/cobrust-stdlib/tests/io_input.rs`:

1. `input("")` returns empty Str on empty stdin.
2. `input("> ")` writes "> " to stdout (captured), reads stdin.
3. `input(prompt)` strips trailing `\n` from input.
4. `input(prompt)` does NOT strip `\r\n` differently from `\n` —
   document the convention (we keep `\r`, strip only `\n`; users
   doing cross-platform should strip themselves).
5. `input(prompt)` returns `""` on EOF (stdin closed before newline).
6. `read_line()` returns `"hello\n"` preserving newline (W2 Phase 2
   scope cap per ADR-0044 Decision 1D; ADR-0044a will return
   `Ok("hello\n")`).
7. `read_line()` returns `""` at EOF (W2 Phase 2 scope cap; ADR-0044a
   will return `Err(IoError::Eof)`).
8. `argv()` returns a list whose length matches `argc`.
9. `argv()[0]` matches the program path string.
10. `argv()[1..]` match the user-supplied args.
11. `argv()` empty when only `argv[0]` passed.
12. UTF-8: input with multi-byte chars round-trips identically.
13. UTF-8 lossy: invalid-byte stdin replaced with U+FFFD, no panic.
14. Tab-completion-friendly: `input(">> ")` followed by long input
    (≥ 4 KiB) still works.
15. Repeated `input()` calls drain stdin line by line.
16-30. Well-typed combinations (input result used in if/while/match,
    argv result iterated via for-protocol, read_line composed with
    Result.unwrap_or pattern).

### Tier 2 — Ill-typed rejection (≥ 30 tests)

1. `input(123)` — int arg → TypeError.
2. `input(["a"])` — list arg → TypeError.
3. `input()` (zero args, must call `input_no_prompt`) — ArityError.
4. `argv(1)` — arg given to argv → ArityError.
5. `read_line(1)` — arg given to read_line → ArityError.
6. Assigning `argv()` to `i64` → TypeError.
7. Assigning `input(prompt)` to `i64` → TypeError.
8-30. Type-and-arity rejection corpus.

### Tier 3 — End-to-end Cobrust→stdout (≥ 10 tests)

Lives in `crates/cobrust-cli/tests/cli_stdin_argv_e2e.rs`:

1. `echo "hello" | cobrust run examples/leetcode/two_sum.cb` →
   stdout matches expected.
2. `echo "" | cobrust run two_sum.cb` → graceful handling.
3. `printf "1\n2\n3\n" | cobrust run sum_lines.cb` → stdout "6".
4-10. The ten leetcode examples each gated.

### Tier 4 — Fuzz (≥ 1024 inputs)

`proptest` strategy for stdin contents (random UTF-8, random length
0..16 KiB). Property: `cobrust run echo.cb < random` → exit code 0,
no panic.

### 5-gate baseline

Phase 2 P7 sonnet must achieve:
- `cargo fmt --check` → 0 violations.
- `cargo clippy -- -D warnings -W clippy::pedantic` → 0 violations
  in non-test paths.
- `cargo build --release` → 0 warnings.
- `cargo test --workspace` → +60 tests pass, 0 fails, ignored count
  unchanged from baseline 8.
- `bash scripts/doc-coverage.sh` → exit 0.

## Done means (P9 Phase 1 — ADR landed)

- [x] ADR-0044 written, status `accepted` (was `proposed`; ratified
      via `[P10-RATIFY-0044-WITH-AMENDMENTS]` 2026-05-11 — see
      Decision 1D W2 Phase 2 scope cap sub-section + Follow-up
      ADR-0044a section for the amendment delta).
- [x] CTO ratified via `[P10-RATIFY-0044-WITH-AMENDMENTS]` with 5
      amendments folded in (see Decision 1D W2 Phase 2 scope cap,
      PRELUDE / C-ABI / Tier 1 test plan updates, and Follow-up
      ADR-0044a section).
- [x] ADR atomic commits on `feature/w2-leetcode-wedge`:
  - `0d58cd0` — initial Phase 1 proposed ADR + roster row.
  - (this commit) — amendments + status flip to `accepted`.
  - No code changes (Phase 2 is impl).
- [ ] Snapshot memory updated to reference ADR-0044 + W2 sprint in
  flight (CTO-side; tracked separately from the worktree atomic).

## Done means (W2 sprint — Phase 2/3/4)

Per `dispatches/w2-leetcode-wedge-sprint.md`:

- Phase 2: 5-gate green + 60+ new tests pass + fuzz 1024 panic-free.
- Phase 3: 10 LeetCode `.cb` files in `examples/leetcode/` + per-file
  README + parent README, all compile + stdout matches expected
  oracle output (paste 3 examples to CTO report).
- Phase 4: zh + en getting-started doc + README "Quick Start for
  LeetCode" section + doc-coverage exit 0 + a non-Cobrust user can
  follow the doc to run Two Sum in ≤ 30 min.

## Consequences

### Positive

- **W2 user wedge闭环**: project owner can write LeetCode in
  Cobrust. First real user = self. ADR-0038 §F.1 wedge "AI Python
  加速器" gains a complementary language-side anchor.
- **8 M12.x stub examples** (`cat.cb`/`wc.cb`/etc.) can be rewritten
  to use real stdin in a follow-up (stretch goal per dispatch §
  "Stretch goals").
- **Architectural conservatism**: zero new MIR/HIR/types primitives.
  The wire is "prelude + intrinsic-rewrite + runtime helper" —
  same pattern proven by M11 `print` + M11.1 `print_int`.
- **Backward-compat clean**: no existing example breaks. `fn main()
  -> i64` signature preserved.
- **Translator surface preview**: future Python-library translations
  (e.g. `input()` usage in `argparse`, `sys.argv` references) have a
  binding target that matches Python semantics. This unblocks
  Phase F.1.6 (second translated library) for any Python module
  that touches stdin/argv.

### Negative

- **Two stdin functions** (Decision 1D's `input` + `read_line`)
  imply two doc explanations and two test surfaces. Mild §2.1
  "one way" tension; mitigated by tier-naming (`input` = "Python
  wedge", `read_line` = "Cobrust primitive").
- **Naming asymmetry**: Cobrust-source `argv()` vs Rust-side
  `std::env::args()`. Doc clarifies; future `std.env.args` source
  syntax may emerge when MIR module-path lowering lands.
- **EOF semantics divergence from Python** (no `EOFError`). User
  doc must call this out explicitly.
- **`Result_StrIo` ABI shape** deferred to ADR-0044a (W2 Phase 2
  scope cap per Decision 1D). Under ADR-0044, `read_line()` returns
  `*mut Str`; the typed-Result tagged-union layout will land with
  ADR-0044a once typed-`Result[T, E]` lowering is in scope.

### Neutral / unknown

- Whether the project later promotes `std.env.args()` module-path
  syntax to first-class — ADR-bumpable when MIR module-path
  lowering lands (Phase F.2.x candidate).
- Whether `read_line()` should be lifted to a `for line in stdin`
  iterator protocol — Decision 1D scope keeps it flat for W2;
  the for-protocol lift is a Phase F.1.x candidate aligned with
  `__cobrust_iter_init` machinery.
- Windows line-ending convention (`\r\n` vs `\n`) — adopt POSIX
  `\n`-only strip in `input()`, document. Windows-CRLF support is
  a Phase F.2.x candidate.

## Evidence

### Existing surface — grep evidence (run at HEAD `769a5d8`)

```bash
grep -rnE "CAPTURED_ARGS|__cobrust_capture_argv" crates/ | wc -l
# 14 references — argv plumbing fully wired

grep -nE "fn read_line|fn input" crates/cobrust-stdlib/src/io.rs
# 110:pub fn read_line() -> Result<String, Error>
# (no fn input — confirms gap)
```

### Cross-references

- Constitution `CLAUDE.md` §1.1, §1.2, §2.1 "one way", §2.2
  drop-list, §3.3 atomic-commit doc rule.
- ADR-0019 (Phase E roadmap) — M11 stdlib + runtime; this ADR
  amends M11's stdin surface plumbing.
- ADR-0024 (M10 CLI driver) — original `print` intrinsic narrowing;
  this ADR extends the rewrite pass.
- ADR-0025 (M11 stdlib + runtime) — `__cobrust_println` /
  `__cobrust_capture_argv` C-ABI binding; this ADR extends.
- ADR-0027 (M12.x codegen amendments) — runtime helper table +
  iter protocol; this ADR adds 4 new helper entries.
- ADR-0030 (M11.1 print_int) — second intrinsic added via the same
  pattern; precedent for adding 4 more.
- ADR-0034 (FnRef Call lowering) — confirms that user-fn
  identifier resolution is robust; the prelude stub bodies use
  this path before being dropped.
- ADR-0038 (Phase F roadmap) — §F.1 wedge "AI Python 加速器";
  this ADR is the language-half complement.
- dispatch file `w2-leetcode-wedge-sprint.md` — Phase 1/2/3/4
  plan that this ADR scopes.

### Run-time anchor evidence

```
crates/cobrust-stdlib/src/io.rs:110             # read_line() exists
crates/cobrust-stdlib/src/runtime.rs:172        # __cobrust_capture_argv
crates/cobrust-cli/runtime/cobrust_main.c:25    # argv capture wired
crates/cobrust-codegen/src/cranelift_backend.rs:1745  # runtime_helper_signatures
crates/cobrust-cli/src/build.rs:37              # PRELUDE
crates/cobrust-cli/src/build/intrinsics.rs:88   # rewrite_print pattern
```

## Follow-up: ADR-0044a (queued)

ADR-0044a is queued as the typed-Result completion of the W2 Phase 2
scope cap (see Decision 1D W2 Phase 2 scope cap sub-section).

### Trigger

Land ADR-0044a once **typed-`Result[T, E]` lowering** is in scope:
the type checker and MIR lowering pipeline must recognize generic
tagged-union `Result[T, E]` end-to-end (Ok / Err variants typed,
exhaustive `match` lowering, `?`-operator desugar). Today, the
pipeline only has opaque `IoError` shapes from f-string / runtime
intrinsics; generic Result lowering is a Phase F.1.x prereq.

### Scope

- Flip `read_line()` source signature `-> str` → `-> Result[str, IoError]`.
- Re-introduce `Result_StrIo` opaque tagged-union shape in
  `crates/cobrust-stdlib/src/runtime.rs` (matching f-string Str ABI).
- Re-introduce `__cobrust_io_err_*` accessor C-ABI helpers if the
  Result discriminant needs runtime-side branching (otherwise codegen
  can emit branch directly).
- Change `__cobrust_read_line` C-ABI signature `() -> *mut Str` →
  `() -> *mut Result_StrIo`.
- Update Tier 1 tests #6 / #7 to assert `Ok("hello\n")` /
  `Err(IoError::Eof)` (was `"hello\n"` / `""` under W2 scope cap).
- Update PRELUDE stub `fn read_line() -> Result[str, IoError]: return
  Err(IoError())`.
- All `input()` / `argv()` surfaces remain unchanged — they ship
  fully under ADR-0044.

### Phase

Phase F.1.x candidate (post-W2, pre-numerical Phase). Sequencing
depends on the typed-Result lowering ADR (not yet drafted). When
that lands, ADR-0044a is a small follow-on (≤ 1 sprint, D2 — single
crate stdlib API change + codegen ABI shim flip).

### Non-goal of ADR-0044a

ADR-0044a does NOT touch `input()` or `argv()`. Those remain at the
ADR-0044 W2 Phase 2 design (plain `-> str` / plain `-> list[str]`).
The Result-typed `read_line()` is the only surface in scope.

## Why this ADR now

User question 2026-05-11 "刷不了 leetcode" is the **strongest
milestone-vs-user-traction漂移 signal** in the Cobrust history. v0.1.1
shipped Wednesday, but the project owner cannot use it to do LeetCode
on Saturday. This ADR is the unblock — and the binding for what "ship
a usable wedge" means at the language half.

review-claude Option C §B verdict: this is **P1 STRATEGIC**, parallel
to §A's v0.1.1 install-path hotfix. Both close gaps between "internal
green CI" and "external user can do useful thing". The two together
make v0.2.x credible.

— P9 opus tech-lead, 2026-05-11
