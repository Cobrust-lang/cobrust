---
doc_kind: adr
adr_id: 0050f
title: "M-F.3.6 — file-IO completion (read_file_lines / read_file / append_file / stdin_read_all / stdout_write / stderr_write at source level)"
status: accepted
date: 2026-05-16
last_verified_commit: 0ddcd27
supersedes: []
superseded_by: []
relates_to: [adr:0025, adr:0027, adr:0034, adr:0044, adr:0044a, adr:0049, adr:0050, adr:0050a, adr:0050b, adr:0050c, adr:0050d]
parent_adr: adr:0050
sub_adr_of: 0050 (Phase F.3 batch §"P1 follow-ups / M-F.3.6 file IO completion")
discovered_by: P10 CTO 2026-05-16 — Phase F.3 P1 §M-F.3.6 named the gap; ADR-0050f is the design-only sprint that scopes it
ratification_path: in-session review per ADR-0050 audit-teammate pattern (read-only opus general-purpose audit; M-F.3.6 PAIR dispatches post-ratification on `feature/f3-file-io`)
---

# ADR-0050f: M-F.3.6 — file-IO completion at the source level

## Context

### Phase F.3 P1 §M-F.3.6 scope

ADR-0050 §"P1 follow-ups" lists M-F.3.6 verbatim:

> **M-F.3.6 file IO completion** — `read_file_lines() -> list[str]` /
> `append_file` / fully-binding `stdin().read_all()` / fully-binding
> `stdout().write` at source level. ~2-3 days, D2.

This is the Phase F.3 P1 file-IO completion milestone. It builds on the
ADR-0044 W2 Phase 2/3 PRELUDE+intrinsic-rewrite+C-ABI pattern that already
shipped `input(prompt) -> str` / `input_no_prompt() -> str` /
`read_line() -> str` / `argv() -> list[str]`, plus the ADR-0049
input-str-buf fix for non-literal prompts.

The user-facing wedge: today a Cobrust `.cb` source can read **one line**
from stdin (`read_line()` / `input()`) and process **per-line** logic
program-after-program, but it cannot:

- Read an entire file into a `str` (`read_file(path)`).
- Read a file as a `list[str]` of newline-stripped lines
  (`read_file_lines(path)`).
- Append-write to a file (`append_file(path, contents)`).
- Read entire stdin until EOF (`stdin_read_all() -> str`).
- Write a non-newline-terminated `str` to stdout / stderr from source.

Without these primitives the LeetCode wedge (ADR-0044) cannot extend to
problems whose input is "the entire file" rather than "one line at a
time", JSON parsing (M-F.3.7) cannot bootstrap on file inputs, and the
ADR-0048 M-AI corpus cannot author end-to-end "load training data from
file → call LLM → write summary" demos at source level.

### Constitution alignment

| Clause | This ADR's adherence |
|---|---|
| §1.1 "syntactically familiar to Python users" | User expects `read_file(path)` to return the file contents as a string; `read_file_lines(path)` to return a `list[str]`. Both match Python idioms (`open(path).read()` / `open(path).readlines()` after newline strip). |
| §2.1 "one way to do each thing in the core language" | Each fn is the canonical Cobrust-source-level way to perform its operation. The Rust-side `std.io.read_file` / `std.io.stdout().write` etc. remain the impl tier; the source-level surface is the wedge. |
| §2.2 "Exceptions as default error path → `Result<T, E>` is default" | The full Cobrust-idiomatic shape is `Result<str, IoError>` for every fallible op. M-F.3.6 ships the **i64-sentinel scope cap** per Q1 below (same precedent as ADR-0044 W2 Phase 2's `read_line() -> str` cap; typed-Result lift deferred to ADR-0044a). |
| §2.2 "no silent coercion" | The path/contents arguments are `str` (no implicit `int → str`); `read_file_lines` returns `list[str]` (not `list[bytes]`) — binary file handling is explicit `read_bytes(path) -> list[i64]` deferred to Phase G (Q4 below). |
| §3.3 "atomic-commit doc rule" | The M-F.3.6 implementation PAIR ships zh + en getting-started doc updates in the same commit. |
| §5.1 "elegant — one way to do each thing; no `.unwrap()` in non-test code" | The 7-fn surface is flat (no method dispatch); error reporting uses the i64-sentinel pattern from ADR-0044 (caller decides via comparison; no unwrap path leaks to user). |
| §5.3 "efficient — allocations visible via the type system" | Returns of `str` and `list[str]` are owned per ADR-0050c Option A; the drop schedule covers them automatically. Every allocation surface (return value, list element) is visible at MIR. |

### Inheriting from ADR-0050c Option A

ADR-0050c §"Decision" pinned **Str non-Copy uniformly** across operand-level
and drop-level. M-F.3.6's surface inherits this:

- Every return of `str` (e.g. `read_file(path) -> str`) is owned; the
  caller's binding scope owns the drop.
- Every return of `list[str]` (e.g. `read_file_lines(path) -> list[str]`)
  is a non-Copy list of non-Copy Strs; the drop schedule emits
  `__cobrust_list_drop_elems(list, __cobrust_str_drop)` at scope exit per
  ADR-0050c Phase 2 + Phase 3.
- Every `str` argument (e.g. `path` in `read_file(path)`) is consumed
  (moved) by the call. The caller cannot re-use the path str after the
  call without explicit clone. Implications enumerated in §"Consequences"
  below.

This ADR does **NOT** re-introduce Str=Copy-at-operand. The string-stdlib
P9-G sprint (M-F.3.5) running parallel to this design decides whether
ergonomic `clone()` belongs in PRELUDE; M-F.3.6 is consumer-side only.

The LC-100 honest-debt disposition (`findings/lc100-str-use-after-move-regression-from-adr0050c.md`
Path D, accepted 2026-05-16) keeps Option A intact; M-F.3.6 ships under
the same disposition. New M-F.3.6 corpus programs that exercise the
file-IO surface MUST follow the same "single-consume-per-Str" pattern as
the existing LC-100 corpus or the new dict programs from ADR-0050d.

### F30 predicate-flip discipline cross-reference

`findings/predicate-flip-cascade-discovery-deficit.md` (F30 ADSD candidate
filed alongside ADR-0050c) requires every ADR introducing surface that
consumes a shared infrastructure type to enumerate consumers per §"Consequences"
with `also-fixed` / `fixed-later-with-anchor` / `accepted-as-known-debt`
labels. M-F.3.6 inherits Str non-Copy and adds 7 new Str-consuming
surfaces; the §"Consequences" enumeration is binding.

## Verified-at-HEAD audit table (F27 SOP)

Per `findings/adr-scope-reality-divergence.md` F27 SOP: every claim below
cross-checked by reading at HEAD `0ddcd27` (`feature/f3-file-io` branch off
`main@0ddcd27`).

| Claim | File:line | Verbatim shape | Classification |
|---|---|---|---|
| `read_file(path: &str) -> Result<String, Error>` Rust-side | `crates/cobrust-stdlib/src/io.rs:338` | `pub fn read_file(path: &str) -> Result<String, Error> { std::fs::read_to_string(path).map_err(...) }` | **already-shipped-Rust-side** (no source binding) |
| `write_file(path: &str, contents: &str) -> Result<(), Error>` Rust-side | `crates/cobrust-stdlib/src/io.rs:343` | `pub fn write_file(path: &str, contents: &str) -> Result<(), Error> { std::fs::write(path, contents).map_err(...) }` | **already-shipped-Rust-side** (no source binding) |
| `read_line() -> Result<String, Error>` Rust-side | `crates/cobrust-stdlib/src/io.rs:327` | `pub fn read_line() -> Result<String, Error>` | **already-shipped-source-level** via ADR-0044 (source signature `read_line() -> str` per Phase 2 cap) |
| `Stdin::read_line` Rust-side method | `crates/cobrust-stdlib/src/io.rs:361` | `pub fn read_line(&self) -> Result<String, Error>` | **already-shipped-Rust-side** (method form; no source binding) |
| `Stdin::read_all` Rust-side method | `crates/cobrust-stdlib/src/io.rs:368` | `pub fn read_all(&self) -> Result<String, Error>` | **already-shipped-Rust-side** (method form; no source binding) |
| `Stdout::write` Rust-side method | `crates/cobrust-stdlib/src/io.rs:393` | `pub fn write(&self, s: &str) -> Result<(), Error>` | **already-shipped-Rust-side** (method form; no source binding) |
| `Stderr::write` Rust-side method | `crates/cobrust-stdlib/src/io.rs:414` | `pub fn write(&self, s: &str) -> Result<(), Error>` | **already-shipped-Rust-side** (method form; no source binding) |
| `read_file_lines` anywhere | (grep returns nothing in `crates/`) | — | **not-yet** — net new |
| `append_file` anywhere | (grep returns nothing in `crates/`) | — | **not-yet** — net new |
| `__cobrust_read_file*` C-ABI | (grep returns nothing in `crates/`) | — | **not-yet** — net new |
| `__cobrust_write_file*` C-ABI | (grep returns nothing in `crates/`) | — | **not-yet** — net new |
| `__cobrust_append_file*` C-ABI | (grep returns nothing in `crates/`) | — | **not-yet** — net new |
| `__cobrust_stdin_read_all` / `__cobrust_stdout_write` / `__cobrust_stderr_write` C-ABI | (grep returns nothing in `crates/`) | — | **not-yet** — net new |
| W2 Phase 3 PRELUDE+intrinsic-rewrite pattern precedent | `crates/cobrust-cli/src/build.rs:51` (single-line PRELUDE) + `crates/cobrust-cli/src/build/intrinsics.rs:53-141` (RUNTIME_SYMBOL consts, 47 total) | `pub const INPUT_RUNTIME_SYMBOL: &str = "__cobrust_input"` ... `pub const PRINT_NO_NL_LIT_RUNTIME_SYMBOL: &str = "__cobrust_print_no_nl_lit"` | **already-shipped-pattern** — M-F.3.6 extends with 7 new RUNTIME_SYMBOL consts |
| W2 Phase 3 Str alloc helper for source-side returns | `crates/cobrust-stdlib/src/io.rs:167` | `fn alloc_str_buffer(s: &str) -> *mut u8` (wraps `__cobrust_str_new + push_static`) | **already-shipped** — M-F.3.6 reuses this for every fn that returns `str` |
| Str non-Copy uniformly (ADR-0050c Option A) | `crates/cobrust-mir/src/drop.rs:122-129` + `crates/cobrust-mir/src/lower.rs:1716-1725` | non-Copy at both predicates | **already-shipped** (binding) |
| `__cobrust_list_new` / `_set` / `_get` / `_len` Rust-side C-ABI | `crates/cobrust-stdlib/src/collections.rs:390/419/440/459` | `pub unsafe extern "C" fn __cobrust_list_new(_elem_size: i64, len: i64) -> *mut u8` etc. | **already-shipped** — M-F.3.6 `read_file_lines` constructs a list using these |
| `__cobrust_list_str_*` slot accessors | (grep returns nothing — list slots are i64-typed; Str pointers cast to i64) | — | **already-shipped via i64-slot reinterpret** per ADR-0050c list[str] DEV recovery (Phase 2a' walk-back history); M-F.3.6 mirrors the same pattern |

Audit summary:

- **6 of 7 target fns have Rust-side impl** (`read_file`, `write_file`,
  `Stdin::read_all`, `Stdout::write`, `Stderr::write`, plus existing
  `read_line` via ADR-0044).
- **1 of 7 has no Rust-side impl** (`read_file_lines` — net new Rust-side fn).
- **1 of 7 has no Rust-side impl** (`append_file` — net new Rust-side fn).
- **Zero C-ABI shims exist for any of the 7** — every target fn needs a
  new `__cobrust_<name>` shim wired into `runtime_helper_signatures()` at
  `crates/cobrust-codegen/src/cranelift_backend.rs` (line range
  `1745-1978` per ADR-0044 §"Codegen amendment" + ADR-0050c F29
  enumeration).
- **Zero source-level bindings exist for any of the 7** — every target
  fn needs a new PRELUDE stub, a new `*_RUNTIME_SYMBOL` const, and a new
  intrinsic-rewrite arm.

Counting fresh deltas total: 2 Rust-side fns + 7 C-ABI shims + 7
runtime-helper signatures + 7 PRELUDE stubs + 7 RUNTIME_SYMBOL consts +
7 intrinsic-rewrite arms = ~30 small additions across 3 crates. This
matches ADR-0050 §"P1 follow-ups" estimate ("~2-3 days, D2") — smaller
than M-F.3.5 string stdlib (which has more elaborate type interactions
via `split` returning `list[str]`).

## Options considered

### Option A — PRELUDE-fn form (free fns) for all 7 surfaces

**Mechanism**: every surface ships as a free fn callable from `.cb`
source. `read_file_lines(path)` not `path.read_lines()`; `stdout_write(s)`
not `stdout().write(s)`. Each gets a PRELUDE stub + RUNTIME_SYMBOL + C-ABI
shim matching the ADR-0044 W2 Phase 3 pattern.

**Surface shape**:

```cobrust
fn read_file(path: str) -> str
fn read_file_lines(path: str) -> list[str]
fn write_file(path: str, contents: str) -> i64
fn append_file(path: str, contents: str) -> i64
fn stdin_read_all() -> str
fn stdout_write(s: str) -> i64
fn stderr_write(s: str) -> i64
```

**Pros**:

- **Architecturally identical to ADR-0044 W2 Phase 3 precedent.** Every
  PRELUDE+intrinsic-rewrite+C-ABI path is proven; the 7 new fns are
  copy-paste-and-rename of `input` / `read_line` / `argv` / `parse_int`
  scaffolding. Zero new MIR primitives, zero new codegen passes.
- **No method dispatch.** Cobrust MIR codegen does not yet support
  method-call-on-handle for stdlib newtypes (`std.io.stdin().read_all()`)
  per ADR-0044 Option 1B's rejection rationale. Option A sidesteps that.
- **Single source-level call shape per surface.** `stdout_write(s)` —
  no two ways. §5.1 "one way" satisfied.
- **Future-compatible.** When Phase G method-dispatch lands (ADR-0050b
  §"Maintenance burden" addendum + REPL planning), `stdout().write(s)`
  can be added as a method-form sugar on top of `stdout_write(s)` without
  breaking source compat. Phase G picks the migration path.
- **Trivial extension of intrinsic-rewrite.** `kind_for_name` at
  `crates/cobrust-cli/src/build/intrinsics.rs:729` gains 7 new arms
  matching by source-fn-name; the existing `kind_for_def_id` dispatch
  preserves the W2 Phase 3 pattern.

**Cons**:

- **Naming asymmetry with `std.io.stdout().write()` Rust-side surface.**
  ADR-0025 §"Public surface" pins `std.io.stdout() -> Stdout` + method
  `Stdout::write`; M-F.3.6 source-level surface uses
  `stdout_write(s)` which doesn't compose via dot-call. Mitigation: doc
  in zh/en getting-started clarifies the two surfaces; Phase G method-form
  ships when dispatch lands.
- **7 fns adds 7 PRELUDE lines.** PRELUDE size grows from ~50 fns to
  ~57. Mild constitution §5.1 "no struct has more than 7 public fields"
  echo, but the PRELUDE is not a struct — the rule does not apply.

### Option B — Method-call form via Stdin / Stdout / Stderr newtypes

**Mechanism**: ship `stdin()` / `stdout()` / `stderr()` as PRELUDE fns
returning opaque handle values (analog of ADR-0027 for-protocol's
`*mut u8` handle), then bind `.read_all()` / `.write(s)` / etc. as method
calls dispatched at MIR via a new "method on opaque handle" pass.

**Pros**:

- **Symmetric with Rust-side `std.io.Stdout::write` surface.**
- **Eventual goal** — Phase G is going there per ADR-0050b §"Maintenance
  burden" addendum (range-form vs method-form alignment).

**Cons**:

- **Requires net-new MIR method-dispatch primitive.** Today codegen
  has no path for `value.method(args)` where `value` is a stdlib
  newtype handle. ADR-0044 Option 1B explicitly rejected this for the
  W2 Phase 2 wedge ("method-on-stdlib-object would require a new MIR
  primitive or a sugar pass; both are out-of-scope for W2"). M-F.3.6
  is a Phase F.3 P1 follow-up (D2 ~2-3 days); adding method dispatch is
  D5 multi-week scope.
- **Blocks on Phase G.** Method dispatch is the natural Phase G
  consolidation alongside REPL + LSP (ADR-0050 §"P1 wave / M-F.3.8").
  M-F.3.6 is queued in Phase F.3 P1; gating it on Phase G inverts the
  dependency.
- **Method dispatch on `stdin()` reads a global resource** — the
  handle is not a value-typed thing, it is a singleton. The newtype is
  a no-op wrapper; the method form gains no abstraction power over the
  free-fn form.

### Option C — Both: PRELUDE-fn form now (Wave F.3.6); defer method-form to Phase G (CHOSEN)

**Mechanism**: Phase F.3.6 ships PRELUDE-fn form for all 7 surfaces per
Option A; Phase G adds method-form sugar `stdin().read_all()` /
`stdout().write(s)` as a forward-compatible layer once MIR
method-dispatch lands.

**Pros** (all Option A pros, plus):

- **Honest about the staging.** "We picked the cheap form now; the
  method form is queued for the dispatch sprint." Mirrors ADR-0044
  W2 Phase 2 Option 1D's two-fn split rationale: pragmatic now,
  forward-compatible later.
- **Zero coupling to Phase G timeline.** M-F.3.6 ships independently of
  REPL / LSP / method-dispatch scheduling.

**Cons** (all Option A cons, plus):

- **Doc tree carries the migration note** — zh/en getting-started must
  mention that Phase G will add method-form. Mild overhead; one extra
  paragraph.

**Chosen**: **Option C**. Mirrors ADR-0044 W2 Phase 2 precedent +
ADR-0050d's "Phase F.3 ships X, Phase G ships Y" staging pattern.

## Decision

Adopt **Option C** — PRELUDE-fn form for all 7 surfaces in Phase F.3.6;
method-form sugar deferred to Phase G.

### Source-level surface (binding)

```cobrust
# Read entire file at `path` as UTF-8 string. Empty file returns "".
# Path errors / I/O errors return "" + the i64-sentinel via read_file_status()
# is NOT shipped — error visibility is via write_file/append_file's i64 return.
# Q1 resolution: i64-sentinel for write; bare str return for read; honest
# trade-off documented in §"Open questions".
fn read_file(path: str) -> str

# Read entire file at `path`, split into a list of newline-stripped lines.
# Each line has its trailing `\n` (and `\r` if `\r\n` line ending) stripped.
# Empty file returns empty list.
fn read_file_lines(path: str) -> list[str]

# Write `contents` to file at `path`, creating or truncating. Returns 0
# on success, non-zero error code on failure (Q1: i64-sentinel matches
# ADR-0044 read_line return-bare-Str+EOF-empty precedent).
fn write_file(path: str, contents: str) -> i64

# Append `contents` to file at `path`, creating if absent. Returns 0
# on success, non-zero error code on failure.
fn append_file(path: str, contents: str) -> i64

# Read entire stdin until EOF as UTF-8 string. EOF returns "".
fn stdin_read_all() -> str

# Write `s` to stdout (no trailing newline). Returns 0 on success.
# Differs from print(s)+println(s)+print_no_nl(s) (ADR-0044 W2 Phase 2/3
# + ADR-0047) in: stdout_write does NOT append newline AND returns i64
# success code (the print fns return i64 = 0 by convention but the
# semantic is "write succeeded"). See §"Consequences" cross-surface
# dispatch table.
fn stdout_write(s: str) -> i64

# Write `s` to stderr (no trailing newline). Returns 0 on success.
fn stderr_write(s: str) -> i64
```

### Drop-schedule per fn (per ADR-0050c Option A)

| Fn | path consumed? | contents consumed? | Returns | Drop-schedule notes |
|---|---|---|---|---|
| `read_file(path: str)` | yes (str arg moved into call) | — | `str` (owned; new alloc via `alloc_str_buffer` at io.rs:167) | Caller's binding scope owns the returned str; drop at scope exit per ADR-0050c Phase 2. |
| `read_file_lines(path: str)` | yes | — | `list[str]` (owned; list of owned str slots) | List drop at scope exit via `__cobrust_list_drop_elems(list, __cobrust_str_drop)` per ADR-0050c Phase 3. |
| `write_file(path: str, contents: str)` | yes | yes (both moved) | `i64` (Copy) | No alloc; no drop schedule effect on return. |
| `append_file(path: str, contents: str)` | yes | yes | `i64` (Copy) | Same. |
| `stdin_read_all()` | — | — | `str` (owned) | Returned str dropped at caller's scope exit. |
| `stdout_write(s: str)` | s consumed | — | `i64` (Copy) | s drops inside the call (or never returns — runtime owns lifetime); no caller-side drop emission. Same shape as ADR-0044 W2 Phase 2 `print(s: str) -> i64`. |
| `stderr_write(s: str)` | s consumed | — | `i64` (Copy) | Same. |

### i64-sentinel error reporting (Q1 resolution)

Phase F.3.6 follows the ADR-0044 W2 Phase 2 i64-sentinel pattern: write/
append/stdout_write/stderr_write return `i64` where `0 = success` and
non-zero is an error code. Read fns return bare `str` / `list[str]`
where empty represents either "the file is empty" or "the operation
failed".

This is intentionally less expressive than typed-Result; the trade-off
is documented at §"Open questions" Q1. Typed-`Result[T, IoError]` lands
at ADR-0044a (queued; not yet drafted; trigger = generic
tagged-union lowering in scope per ADR-0044 §"Follow-up: ADR-0044a").
When ADR-0044a lands, M-F.3.6's signatures flip:

| M-F.3.6 (this ADR) | ADR-0044a-target |
|---|---|
| `read_file(path) -> str` | `read_file(path) -> Result[str, IoError]` |
| `read_file_lines(path) -> list[str]` | `read_file_lines(path) -> Result[list[str], IoError]` |
| `write_file(path, contents) -> i64` | `write_file(path, contents) -> Result[None, IoError]` |
| `append_file(path, contents) -> i64` | `append_file(path, contents) -> Result[None, IoError]` |
| `stdin_read_all() -> str` | `stdin_read_all() -> Result[str, IoError]` |
| `stdout_write(s) -> i64` | `stdout_write(s) -> Result[None, IoError]` |
| `stderr_write(s) -> i64` | `stderr_write(s) -> Result[None, IoError]` |

The flip is mechanical; M-F.3.6 commits to the i64-sentinel scope cap
and ADR-0044a closes the typed-Result completion.

### Error code convention for write/append/stdout/stderr i64 returns

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | I/O error (file not found / permission denied / disk full / etc.). The Rust-side `Error::io` variant collapses to this code under M-F.3.6's i64-sentinel cap. |
| `2` | UTF-8 encoding error (shouldn't occur for writes since `contents` is already valid UTF-8 from MIR-Str invariant; reserved for future binary-write surface per Q4). |
| Other | Reserved for ADR-0044a expansion. |

For Phase F.3.6 the corpus exercises code `0` (success) and code `1` (I/O
error via "path does not exist + parent dir doesn't exist" + "path is a
directory" cases). Codes 2+ are reserved.

### Convention for newline handling (Q2 resolution)

`read_file_lines(path)` strips **both** `\n` and `\r\n` (cross-platform
honest). The rstrip logic at the Rust-side `__cobrust_read_file_lines`
shim normalizes:

```rust
// Pseudocode:
let raw = std::fs::read_to_string(path)?;
let mut lines: Vec<String> = Vec::new();
for line in raw.split('\n') {
    let stripped = line.strip_suffix('\r').unwrap_or(line);
    lines.push(stripped.to_string());
}
// Trailing empty line from final \n is preserved as "" — matches
// Python's str.split('\n') semantics, NOT splitlines() semantics.
```

The "trailing empty line preserved" choice diverges from Python's
`open(path).readlines()` (which drops the final empty line) and aligns
instead with `s.split('\n')`. Rationale: round-trip identity — if a
program does `read_file_lines(path)` then `len(lines)`, the count should
match `s.count('\n') + 1` so users can reason about file shape. The
behavior is documented in zh/en getting-started.

### Path encoding (Q3 resolution)

UTF-8 paths only. Same convention as ADR-0044 Decision 4 (stdin/argv
UTF-8 lossy with U+FFFD replacement). On non-UTF-8 OS paths (POSIX
allows arbitrary bytes in path components):

- The Rust-side `std::fs::read_to_string(path)` accepts `&str` (UTF-8);
  non-UTF-8 paths cannot be expressed from Cobrust source today.
- Workaround for users with non-UTF-8 paths: rename the file or use the
  shell to redirect (`cobrust run prog.cb < /weird-path-file`).

Documented limitation; Phase G `OsString`-style path-encoding ADR can
revisit if user pressure surfaces. M-F.3.6 i64-sentinel error code `1`
covers the "path not found / I/O error" case for users hitting this
edge case.

## Implementation map (binding — split per F30 §"Consequences" SOP)

Each sub-sprint below estimates wall-time + recommended dispatch shape
per ADR-0050 §A7 PAIR pattern.

### Sub-sprint 1 — PRELUDE + intrinsic-rewrite (CLI crate, ~1.5 hours)

| File | Change | Estimated LoC |
|---|---|---|
| `crates/cobrust-cli/src/build.rs:51` | Append 7 stub fns to PRELUDE constant: `read_file`, `read_file_lines`, `write_file`, `append_file`, `stdin_read_all`, `stdout_write`, `stderr_write`. Each stub returns the obvious zero value (`""` for str, `[]` for list[str], `0` for i64). | +15 lines |
| `crates/cobrust-cli/src/build/intrinsics.rs` after line 141 (`PRINT_NO_NL_LIT_RUNTIME_SYMBOL`) | Add 7 new RUNTIME_SYMBOL consts: `READ_FILE_RUNTIME_SYMBOL`, `READ_FILE_LINES_RUNTIME_SYMBOL`, `WRITE_FILE_RUNTIME_SYMBOL`, `APPEND_FILE_RUNTIME_SYMBOL`, `STDIN_READ_ALL_RUNTIME_SYMBOL`, `STDOUT_WRITE_RUNTIME_SYMBOL`, `STDERR_WRITE_RUNTIME_SYMBOL`. | +28 lines (4 lines per const incl. doc) |
| `crates/cobrust-cli/src/build/intrinsics.rs:729` (`kind_for_name`) | Add 7 new arms mapping source fn name to `Kind` enum. May require new `Kind` variants depending on existing enum shape; align with `INPUT_KIND` / `READ_LINE_KIND` style. | +14 lines |
| `crates/cobrust-cli/src/build/intrinsics.rs:874` (`rewrite_print`) | Extend the rewrite pass to recognize the 7 new Kinds and rewrite their `Terminator::Call` to point at the corresponding runtime symbol. | +21 lines |

**Dispatch shape**: P10-direct PAIR per ADR-0050 §A7. TEST sonnet + DEV
sonnet, parallel. D2 scope (well-scoped pattern extension, no novel
design).

### Sub-sprint 2 — stdlib C-ABI shims (cobrust-stdlib crate, ~2.5 hours)

| File | Change | Estimated LoC |
|---|---|---|
| `crates/cobrust-stdlib/src/io.rs` after line 345 (`write_file` Rust-side) | Add `read_file_lines(path: &str) -> Result<Vec<String>, Error>` Rust-side fn. Use `std::fs::read_to_string` + manual `\n` / `\r\n` strip per §"Convention for newline handling". | +20 lines |
| `crates/cobrust-stdlib/src/io.rs` same vicinity | Add `append_file(path: &str, contents: &str) -> Result<(), Error>` Rust-side fn. Use `std::fs::OpenOptions::new().append(true).create(true).open(path)` + write_all + flush. | +15 lines |
| `crates/cobrust-stdlib/src/io.rs` after line 426 (stderr def) | Add 7 new C-ABI shims: `__cobrust_read_file(path: *mut u8) -> *mut u8` (returns *mut Str); `__cobrust_read_file_lines(path: *mut u8) -> *mut u8` (returns *mut List_Str); `__cobrust_write_file(path: *mut u8, contents: *mut u8) -> i64`; `__cobrust_append_file(path: *mut u8, contents: *mut u8) -> i64`; `__cobrust_stdin_read_all() -> *mut u8`; `__cobrust_stdout_write(s: *mut u8) -> i64`; `__cobrust_stderr_write(s: *mut u8) -> i64`. Each shim: read path/contents via the existing `Str` decoder (mirror `__cobrust_input` at io.rs:194 — extract `(ptr, len)` from the buffer, decode UTF-8 lossy, call Rust-side fn, allocate return via `alloc_str_buffer` or construct `list[str]` via `__cobrust_list_new` + per-slot `alloc_str_buffer`). | +140 lines (20 per shim incl. # Safety clauses) |

**Dispatch shape**: P10-direct PAIR. TEST sonnet + DEV sonnet, parallel
to Sub-sprint 1 (Sub-sprint 1 is CLI crate; Sub-sprint 2 is stdlib
crate; the runtime-signature glue in Sub-sprint 3 below depends on both
landing). D2 scope.

### Sub-sprint 3 — codegen runtime-helper signatures (cobrust-codegen crate, ~0.5 hours)

| File | Change | Estimated LoC |
|---|---|---|
| `crates/cobrust-codegen/src/cranelift_backend.rs` after line 1978 (`__cobrust_str_drop` signature decl) | Add 7 new entries to `runtime_helper_signatures()`: `out.push(("__cobrust_read_file", sig(call_conv, &[p], Some(p))))` ... etc. | +9 lines (one per signature + 2-line scaffolding) |

**Dispatch shape**: Bundled into Sub-sprint 2 DEV agent (the same agent
that owns stdlib io.rs adds the 7 codegen signature entries — they are
coupled because the signatures must match the shim signatures
character-by-character).

### Sub-sprint 4 — corpus + triple-tree docs + doc-coverage (~2 hours)

| File | Change | Estimated LoC |
|---|---|---|
| `crates/cobrust-cli/tests/file_io_e2e.rs` (new test file) | Tier-A well-typed corpus: ≥20 tests covering (a) read_file round-trip, (b) read_file_lines with `\n` / `\r\n` / mixed line endings, (c) write_file then read_file equality, (d) append_file accumulates correctly, (e) stdin_read_all consumes piped input, (f) stdout_write does NOT append newline (assert exact bytes captured), (g) stderr_write goes to stderr not stdout. | +400 lines test corpus |
| `crates/cobrust-cli/tests/file_io_ill_typed.rs` (new test file) | Tier-A ill-typed corpus: ≥10 tests covering (a) `read_file(123)` int-arg type error, (b) `write_file("/path")` arity error, (c) `read_file_lines(path: str) -> str` return-type mismatch, (d) `stdout_write()` zero-arg arity error, etc. | +150 lines |
| `crates/cobrust-stdlib/tests/io_file_unit.rs` (new file or extend existing `io_input.rs`) | Tier-C unit tests for Rust-side `read_file_lines` / `append_file` newline + create semantics. ≥15 tests. | +200 lines |
| `examples/file_io/read_lines.cb` (new example) | Tier-C E2E example: read a file, print each line numbered. | +20 lines |
| `examples/file_io/round_trip.cb` (new example) | Tier-C E2E example: write_file then read_file demonstrating equality. | +25 lines |
| `examples/file_io/stdin_pipe.cb` (new example) | Tier-C E2E example: stdin_read_all then count words. | +15 lines |
| `docs/agent/modules/stdlib.md` | Add M-F.3.6 §"File IO completion" surface table cross-referencing ADR-0050f. | +25 lines |
| `docs/human/zh/getting-started-file-io.md` (new) | 中文文件 IO 入门文档 + ADR-0050f 引用 + 7 fns examples + Phase G method-form pointer + i64-sentinel honest disclosure. | ~150 lines |
| `docs/human/en/getting-started-file-io.md` (new) | English mirror. | ~150 lines |
| `docs/human/{zh,en}/architecture.md` | Add the 7 source-level fns to the std.io tables. | +15 lines each |
| `scripts/doc-coverage.sh` | Add `read_file`, `read_file_lines`, `write_file`, `append_file`, `stdin_read_all`, `stdout_write`, `stderr_write`, `__cobrust_read_file`, `__cobrust_read_file_lines`, `__cobrust_write_file`, `__cobrust_append_file`, `__cobrust_stdin_read_all`, `__cobrust_stdout_write`, `__cobrust_stderr_write`, `ADR-0050f` to coverage check terms. | +14 lines |
| `docs/agent/adr/README.md` | Append ADR-0050f row to roster. | +1 line |

**Dispatch shape**: P10-direct PAIR per ADR-0050 §A7. TEST sonnet
authors the corpus; DEV sonnet wires the doc updates after impl
sub-sprints close. The doc updates run sequentially after Sub-sprints
1+2+3 finalize so the doc-coverage check has stable surface to verify.

### Estimated total wall-time for M-F.3.6 PAIR

| Sub-sprint | Wall-time | Notes |
|---|---|---|
| Sub-sprint 1 (CLI PRELUDE + intrinsic-rewrite) | 1.5 h | Pattern extension |
| Sub-sprint 2 (stdlib C-ABI shims) | 2.5 h | 7 shims + 2 Rust-side fns |
| Sub-sprint 3 (codegen sig glue) | 0.5 h | Bundled with Sub-sprint 2 DEV |
| Sub-sprint 4 (corpus + triple-tree docs) | 2 h | TEST corpus + zh/en/agent docs |
| P10 coordinator review + 5-gate verify | 0.5 h | Per ADR-0050 §A7 |
| **Total** | **~7 h** | matches ADR-0050 §"P1 follow-ups" "~2-3 days, D2" estimate (assumed 8h workday; 1 day actual) |

## F30 §"Consequences" enumeration (binding)

Per F30 SOP from `findings/predicate-flip-cascade-discovery-deficit.md`:
every consumer of shared Str / List infrastructure must be classified.
M-F.3.6 adds 7 new Str/list[str] consumer surfaces; each row below
enumerates the cross-surface impact.

### `__cobrust_str_*` consumers (M-F.3.6 additions)

| New shim | Str consumption pattern | Status |
|---|---|---|
| `__cobrust_read_file(path)` | Reads `path` Str buffer (extracts ptr+len; does NOT free — caller owns drop per ADR-0050c). Returns owned `*mut Str` via `alloc_str_buffer` (io.rs:167). | **also-fixed** — inherits ADR-0050c Phase 2/3 drop schedule. Caller binding scope drops both `path` arg (after move) and returned `str`. |
| `__cobrust_read_file_lines(path)` | Same path consumption as above. Returns owned `*mut List` of owned `*mut Str` slots. | **also-fixed** — inherits ADR-0050c Phase 3 `__cobrust_list_drop_elems` schedule. Per-slot Str drop runs before list drop at caller scope exit. |
| `__cobrust_write_file(path, contents)` | Reads both Str buffers (extracts ptr+len; caller owns drop). Returns i64. | **also-fixed** — inherits ADR-0050c. Both `path` and `contents` move into the call; drops run at caller scope exit (the call doesn't extend their lifetime beyond the call return). |
| `__cobrust_append_file(path, contents)` | Same as `__cobrust_write_file`. | **also-fixed**. |
| `__cobrust_stdin_read_all()` | No Str consumption (no args). Returns owned `*mut Str`. | **also-fixed** — inherits ADR-0050c Phase 2 drop schedule for the return value. |
| `__cobrust_stdout_write(s)` | Reads s buffer (extracts ptr+len). Returns i64. | **also-fixed** — `s` moves into call; drops at caller scope exit. Mirrors `__cobrust_print_no_nl(s)` at io.rs:629. |
| `__cobrust_stderr_write(s)` | Same as `__cobrust_stdout_write`. | **also-fixed**. |

**Total new Str consumer count: 7. All `also-fixed` (no deferral, no
known-debt — ADR-0050c Option A drop schedule covers every case
automatically by construction).**

### `__cobrust_list_*` consumers (M-F.3.6 additions)

| New shim | List consumption pattern | Status |
|---|---|---|
| `__cobrust_read_file_lines(path)` | Returns owned `*mut List` containing `N` owned `*mut Str` slots. Constructed via `__cobrust_list_new(8, line_count)` + per-line `__cobrust_list_set(list, i, alloc_str_buffer(&line) as i64)`. List slots are i64-typed at C-ABI level (cast from Str pointer); MIR `Ty::List(Ty::Str)` enforces element-type semantics. | **also-fixed** — element-type-aware drop emits `__cobrust_list_drop_elems(list, __cobrust_str_drop)` per ADR-0050c Phase 3. Per-slot Str drop runs before list drop. |

**Total new List consumer count: 1. `also-fixed` via ADR-0050c Phase 3
`__cobrust_list_drop_elems` shim.**

### f-string Str hole dispatch (cross-surface)

| Site | Impact | Status |
|---|---|---|
| f-string composition over M-F.3.6 return Strs (e.g. `let contents = read_file(p); print(f"File: {contents}")`) | The f-string lowers via existing `Aggregate::FormatString` at `crates/cobrust-mir/src/lower.rs:1081-1087` + the per-Str-hole dispatch at `crates/cobrust-codegen/src/cranelift_backend.rs:1394+`. The fix at HEAD `09006f6` (commit `9c8b1d2`) per `findings/lc100-tier-a-summary.md` advances the f-string hole iterator in the `is_str` branch; M-F.3.6 returns inherit this fix. | **already-fixed** — no carry-forward. M-F.3.6 corpus includes ≥3 f-string-over-read_file-result tests to lock the integration. |

### Comp-lowering 0-sentinel collision (open finding)

| Site | Impact | Status |
|---|---|---|
| Comprehension lowering producing `[s for s in read_file_lines(p)]` | The comprehension iter-protocol at `crates/cobrust-mir/src/lower.rs:1493-1576` has the open finding `comp-lowering-zero-sentinel-collision.md` (P2). If M-F.3.6 corpus includes a comprehension over `read_file_lines(p)` it inherits both the 0-sentinel bug AND the inherited leak per ADR-0050c §"Aggregate::List comprehensions" `fixed-later-with-anchor` row. | **fixed-later-with-anchor** — same anchor as ADR-0050c. M-F.3.6 corpus MUST NOT include positive comprehension tests over read_file_lines; only negative documented-gap tests until Phase G consolidation closes the open finding. |

### Cross-surface dispatch table — print vs stdout_write vs println

Constitution §5.1 "one way to do each thing" must hold; M-F.3.6 adds
`stdout_write` which overlaps existing `print` / `println` / `print_no_nl`
surfaces. The disambiguation:

| Source call | Trailing newline? | i64 return semantic | Anchor |
|---|---|---|---|
| `print(<literal>)` | yes (newline appended) | always 0 (no error path) | ADR-0024 + ADR-0025 |
| `print(<Str-buf>)` | yes | always 0 | ADR-0044 W2 Phase 2 |
| `println(<literal>)` | yes (alias of `print` per intrinsic-rewrite) | always 0 | ADR-0025 (alias) |
| `print_no_nl(<literal>)` | no | always 0 (no error path) | ADR-0044 W2 Phase 3 |
| `print_no_nl(<Str-buf>)` | no | always 0 | ADR-0044 W2 Phase 3 |
| `stdout_write(<Str-buf>)` | no | 0 = success, 1 = I/O error | **M-F.3.6 (this ADR)** |
| `stderr_write(<Str-buf>)` | no | 0 = success, 1 = I/O error | **M-F.3.6 (this ADR)** |

The user-facing distinction: `print` family is the "fire and forget"
canonical print (any I/O error swallowed); `stdout_write` /
`stderr_write` are the "I want to know if the write succeeded" surface.
For the LeetCode wedge and most user programs, `print` is correct;
for programs that must detect a closed pipe (`SIGPIPE` recovery /
pipeline halt), `stdout_write` is correct.

The dispatch table lives in zh + en getting-started-file-io.md;
PRELUDE has all 7 entries. **No name collision** — `stdout_write` is
not `print`, `stderr_write` is not `print`. §5.1 satisfied with two
tiers of intent ("fire and forget" vs "report error").

### Comprehensive enumeration count

- 7 new `__cobrust_*` shims, all `also-fixed` (no deferral).
- 1 new list-of-str consumer (`__cobrust_read_file_lines`), `also-fixed`.
- 0 f-string interaction regressions (already-fixed at `09006f6`).
- 1 comprehension `fixed-later-with-anchor` (mirrors ADR-0050c +
  existing finding).
- 1 cross-surface dispatch table (print family vs stdout_write family);
  no name collision; honest tier separation.

**Total enumeration count: 10 consumer-tier rows; 9 `also-fixed`; 1
`fixed-later-with-anchor` (existing anchor; no new debt introduced).**

## Open questions

### Q1 — Typed-Result vs i64-sentinel error reporting

**Question**: Should M-F.3.6 ship typed `Result[T, IoError]` returns from
the start, OR commit to i64-sentinel + typed-Result lift via ADR-0044a?

**Decision (recommended + adopted)**: **i64-sentinel + ADR-0044a lift.**

**Rationale**:

- ADR-0044 W2 Phase 2 already set the Phase F precedent for "ship the
  i64-sentinel form; defer typed-Result to ADR-0044a." Adopting typed-
  Result for M-F.3.6 in Phase F.3 would force ADR-0044a to land first
  (or M-F.3.6 to drag the entire typed-tagged-union lowering ADR into
  scope), inverting the dependency.
- Constitution §2.2 "Result<T, E> default" is the long-term shape;
  M-F.3.6 commits to the i64-sentinel cap explicitly with the ADR-0044a
  flip table documented (above). The trade-off is HONEST — i64-sentinel
  collapses every error to a single code today; ADR-0044a expansion
  preserves the M-F.3.6 source-level surface (no breaking change at the
  call sites, only the return type widens).
- LC-100 wedge audience benefit of typed-Result today: marginal (most
  LeetCode programs `assert(write_file(...) == 0)` style). Real LSP /
  REPL benefit lands when ADR-0044a does, post-Phase G.

**Cross-reference**: ADR-0044 §"Decision 1D / W2 Phase 2 scope cap" +
ADR-0044 §"Follow-up: ADR-0044a (queued)".

### Q2 — Newline stripping convention for `read_file_lines`

**Question**: Strip `\n` only, or both `\n` and `\r\n` cross-platform?

**Decision (recommended + adopted)**: **Both `\n` and `\r\n` stripped
cross-platform.**

**Rationale**: Python `open(path).readlines()` preserves trailing
newlines (so `\r\n` files retain `\r` in each line); Cobrust diverges
toward "clean lines, no embedded `\r`" per §2.2 "no silent coercion"
applied to line endings — the user wrote `read_file_lines`, they don't
want to inspect each line for an embedded `\r`. Trade-off documented in
zh + en getting-started: round-trip via `write_file('\n'.join(lines) +
'\n', path)` is line-ending-normalizing (Windows-CRLF files become
LF-only on save). Acceptable for LeetCode wedge + JSON parser
(M-F.3.7); Phase G can revisit if a real round-trip-preserving surface
proves needed.

**Trailing empty line**: preserved (matches `s.split('\n')` semantics,
NOT `s.splitlines()` semantics) — documented at §"Convention for
newline handling" above.

### Q3 — Path encoding: UTF-8 only or OS-native?

**Question**: Should paths accept arbitrary bytes (POSIX `OsString`-style)
or commit to UTF-8 only?

**Decision (recommended + adopted)**: **UTF-8 only with documented
limitation.**

**Rationale**: matches ADR-0044 Decision 4 stdin/argv UTF-8 lossy
convention. Cobrust today has no `bytes` / `OsString` type at the source
level; introducing one would force a Phase G surface change. Users with
non-UTF-8 paths can work around via shell redirection. Workaround
sufficiency reviewed at Phase G; if real users hit it, an
`OsString`-style path ADR opens.

### Q4 — Binary file handling: `read_bytes(path) -> list[i64]`?

**Question**: Should M-F.3.6 ship a binary-file primitive?

**Decision (recommended + adopted)**: **Deferred to Phase G.**

**Rationale**: M-F.3.6 scope is "complete the TEXT file-IO surface".
Binary files (`read_bytes(path) -> list[i64]` or `read_bytes(path) ->
bytes` once `bytes` type lands) is a separate design. The
JSON parser (M-F.3.7) needs only text. The M-AI corpus needs only text.
The LeetCode wedge needs only text. Phase G picks up `bytes` alongside
the `OsString` path discussion (Q3) — both surface when long-lived
programs (REPL/LSP) and non-text data motivate.

### Q5 — `stdout_write` vs `print` ergonomic overlap (raised during draft)

**Question**: Is `stdout_write(s)` worth shipping given `print_no_nl(s)`
already exists?

**Decision**: **Yes — ship both.**

**Rationale** (per §"Cross-surface dispatch table" above):
`print_no_nl(s)` returns 0 with no error path (the print family commits
to "fire and forget"). `stdout_write(s)` returns 0/1 with explicit error
reporting. Programs that need to detect a closed pipe or partial-write
condition need the explicit form; the print family's "always 0" is the
short-form convenience. The two-tier split mirrors ADR-0044's
"`input` vs `read_line`" two-tier split (Decision 1D): pragmatic
Python-shape + Cobrust-honest Result-shape coexist, neither shadows the
other.

Documented in zh + en getting-started-file-io.md: "print family =
short-form, no error reporting; stdout_write / stderr_write = full-form,
error code returned." This is the cleanest §5.1 "one way per intent"
resolution available.

## Consequences

### Positive

- **§1.1 language-half completeness extends.** Cobrust source-level
  programs can now read/write files, consume stdin to EOF, write to
  stdout/stderr with error reporting. The Phase F.3 P1 §M-F.3.6 milestone
  closes.
- **Constitution §2.2 "Result<T, E> default" trajectory honest.** The
  i64-sentinel cap is explicit + ADR-0044a typed-Result lift is queued.
  Same precedent as ADR-0044 W2 Phase 2 read_line scope cap.
- **JSON parser (M-F.3.7) unblocks.** The JSON parser P9-G sprint can
  bootstrap on `read_file(path) -> str` without waiting for typed-Result.
- **LeetCode wedge extends.** Problems whose input is "the entire file
  rather than one-line" become tractable. The "刷不了 leetcode" wedge
  from ADR-0044 / ADR-0049 gains an additional surface.
- **M-AI corpus end-to-end-demo viability.** AI-translation E2E demos
  ("load training data from file → call LLM → write summary") become
  authorable at source level.
- **ADR-0050c Option A drop schedule reaches more surfaces.** The
  F29-style enumeration adds 7 new Str consumers + 1 new list[str]
  consumer; all `also-fixed`. ADR-0050c's "one ownership model for
  heap-allocated value types" extends to file-IO returns by
  construction.
- **Architectural conservatism reinforced.** Zero new MIR primitives.
  Zero new codegen passes. The 7 new surfaces ride the proven ADR-0044
  W2 Phase 2/3 pattern. M-F.3.6 PAIR estimated ~7 hours total — within
  the D2 scope.

### Negative

- **i64-sentinel error reporting collapses every error to a single
  code.** Programs cannot distinguish "file not found" from "permission
  denied" from "disk full" at the source level until ADR-0044a lands.
  Mitigation: the most common error case is "path doesn't exist", which
  programs can pre-check via a future `path_exists(path) -> bool`
  primitive (queued for Phase F.3.x or Phase G as a smaller follow-up).
- **PRELUDE size grows by 7 fns.** Cobrust's PRELUDE was originally
  designed for tight, ergonomic, language-tier names; M-F.3.6 adds 7
  function-like surfaces that are arguably std-library-tier. The trade-off
  is honest: without PRELUDE binding, the surfaces would not be callable
  at the source level until module-path resolution lands (Phase F.2.x
  candidate). Phase G method-form sugar (Option B path) absorbs the
  PRELUDE bloat when MIR method dispatch ships.
- **Naming asymmetry across Rust-side and source-level.** Rust-side
  `std.io.stdout().write(s)` (method form) vs source-level
  `stdout_write(s)` (free fn form). Documented; Phase G resolves.
- **Newline-stripping convention diverges from Python's
  `readlines()`.** Some users transitioning from Python expect
  `read_file_lines(p)` to preserve `\n` per line. M-F.3.6 documents the
  divergence; rationale: §2.2 "no silent coercion" applied to line
  endings argues for clean lines.
- **No binary file handling.** Programs that need to read non-UTF-8
  data cannot use M-F.3.6. Phase G `bytes` + `OsString` ADR addresses.

### Neutral / unknown

- **Whether `read_file_lines` should accept an optional `keep_newlines:
  bool` parameter.** Today's M-F.3.6 surface is unparameterized
  (Cobrust does not yet have default arguments — ADR-0050b §"`range(a,
  b, step)` 3-arg form" defers default-arg sugar to Phase G). When
  default args land, `keep_newlines` parameter could surface as a
  forward-compatible extension.
- **Whether `append_file` should expose `create: bool` to error-out on
  non-existent file.** Today M-F.3.6 commits to "always create if
  absent" matching `OpenOptions::new().append(true).create(true)`.
  Future Phase G can add `append_file_strict(path, contents) -> i64`
  that errors if the file doesn't exist.
- **Whether `stdin_read_all()` should accept a max-byte limit.** Today
  unbounded; could exhaust memory on a pathological input. M-F.3.6
  documents the limitation; Phase G `stdin_read_max(max_bytes: i64) ->
  str` is the natural extension once `bytes` type clarifies.
- **Whether `read_file(p)` followed by `read_file(p)` should re-read or
  cache.** Always re-read (no caching) — matches Python idiom +
  constitution §2.2 "no silent coercion" applied to file freshness.
  M-F.3.6 corpus locks this with an "external write + second-read sees
  fresh content" test.

## Cross-references + dependencies

### Depends on

- **ADR-0050c** (Str ownership Option A) — every `str` argument
  consumed; every `str` / `list[str]` return owned with drop schedule.
- **ADR-0044 W2 Phase 2** (PRELUDE+intrinsic-rewrite+C-ABI pattern) —
  the architectural precedent the 7 new surfaces mirror.
- **ADR-0044 W2 Phase 3** (parse_int / str_at / list_set / etc.
  RUNTIME_SYMBOL extension) — the specific intrinsic-rewrite pattern
  M-F.3.6 extends.
- **ADR-0049** (input-str-buf fix for non-literal prompts) — proves
  the non-literal Str-buf C-ABI dispatch works for non-trivial inputs;
  M-F.3.6 paths/contents inherit the same Str-buf pattern.
- **ADR-0050** §"P1 follow-ups" §M-F.3.6 — the parent ADR that names
  this work.
- **ADR-0025** §"Public surface" — the Rust-side `std.io.read_file` /
  `write_file` / `stdin().read_all()` etc. surface that M-F.3.6 lifts
  to source-level callsites.

### Blocks

- Nothing in Phase F.3 critical path. M-F.3.5 string stdlib + M-F.3.4
  dict impl run in parallel; M-F.3.6 is independent.

### Relates to

- **ADR-0044a (queued)** — typed-Result lift. M-F.3.6's i64-sentinel
  surface flips to typed-Result when ADR-0044a lands. The flip table
  is documented at §"i64-sentinel error reporting (Q1 resolution)"
  above.
- **M-F.3.5 string stdlib** (ADR not yet drafted; P9-G parallel) —
  whether `clone(s)` ships at source level is M-F.3.5's call; M-F.3.6's
  surface inherits whatever clone ergonomic M-F.3.5 picks.
- **M-F.3.7 JSON parser** (ADR not yet drafted) — uses
  `read_file(path)` directly. ADR-0050f is M-F.3.7's primary dependency.
- **Phase G method-dispatch ADR** (not yet drafted) — adds
  `stdin().read_all()` / `stdout().write(s)` etc. as sugar on M-F.3.6's
  free-fn forms when MIR method dispatch lands.
- **F30 ADSD candidate** (`findings/predicate-flip-cascade-discovery-deficit.md`)
  — this ADR's §"Consequences" enumeration uses the F30 SOP.
- **F27 ADSD candidate** (`findings/adr-scope-reality-divergence.md`)
  — this ADR's §"Verified-at-HEAD audit table" uses the F27 SOP.
- **LC-100 honest-debt disposition** (`findings/lc100-str-use-after-move-regression-from-adr0050c.md`
  Path D) — M-F.3.6 ships under the same disposition; new corpus
  programs MUST use single-consume-per-Str patterns.

## Evidence

### Greppable anchors (every claim cross-checked at HEAD `0ddcd27`)

```
crates/cobrust-stdlib/src/io.rs:327                          # read_line() Rust-side
crates/cobrust-stdlib/src/io.rs:338                          # read_file(path) Rust-side
crates/cobrust-stdlib/src/io.rs:343                          # write_file(path, contents) Rust-side
crates/cobrust-stdlib/src/io.rs:354                          # struct Stdin newtype
crates/cobrust-stdlib/src/io.rs:361                          # Stdin::read_line method
crates/cobrust-stdlib/src/io.rs:368                          # Stdin::read_all method
crates/cobrust-stdlib/src/io.rs:387                          # struct Stdout newtype
crates/cobrust-stdlib/src/io.rs:393                          # Stdout::write method
crates/cobrust-stdlib/src/io.rs:408                          # struct Stderr newtype
crates/cobrust-stdlib/src/io.rs:414                          # Stderr::write method
crates/cobrust-stdlib/src/io.rs:167                          # alloc_str_buffer helper (reused by M-F.3.6 returns)
crates/cobrust-stdlib/src/io.rs:194                          # __cobrust_input C-ABI shim (M-F.3.6 mirror precedent)
crates/cobrust-stdlib/src/io.rs:629                          # __cobrust_print_no_nl C-ABI shim (M-F.3.6 stdout_write precedent)
crates/cobrust-cli/src/build.rs:51                           # PRELUDE constant (M-F.3.6 extends with 7 stubs)
crates/cobrust-cli/src/build/intrinsics.rs:53-141            # RUNTIME_SYMBOL consts (M-F.3.6 extends with 7 new)
crates/cobrust-cli/src/build/intrinsics.rs:729               # kind_for_name dispatch (M-F.3.6 adds 7 arms)
crates/cobrust-cli/src/build/intrinsics.rs:874               # rewrite_print main pass (M-F.3.6 extends)
crates/cobrust-codegen/src/cranelift_backend.rs:1745         # runtime_helper_signatures (M-F.3.6 adds 7 entries)
crates/cobrust-codegen/src/cranelift_backend.rs:1978         # __cobrust_str_drop signature decl (M-F.3.6 7 new entries after)
crates/cobrust-stdlib/src/collections.rs:390                 # __cobrust_list_new C-ABI (used by read_file_lines return)
crates/cobrust-stdlib/src/collections.rs:419                 # __cobrust_list_set C-ABI
crates/cobrust-stdlib/src/collections.rs:440                 # __cobrust_list_get C-ABI
crates/cobrust-stdlib/src/collections.rs:459                 # __cobrust_list_len C-ABI
crates/cobrust-mir/src/drop.rs:122-129                       # is_copy match arm (ADR-0050c Option A binding)
crates/cobrust-mir/src/lower.rs:1716-1725                    # is_copy_type match arm (ADR-0050c)
crates/cobrust-codegen/src/cranelift_backend.rs:1022-1026    # Terminator::Drop arm (ADR-0050c Phase 2)
```

### Cross-references (ADR + finding tree)

- Constitution `CLAUDE.md` §1.1 (Python-familiar), §2.1 (one way),
  §2.2 (no silent coercion, Result<T,E> default), §3.3 (atomic doc),
  §5.1 (elegant), §5.3 (efficient).
- ADR-0025 — M11 stdlib + runtime; M-F.3.6 lifts 7 Rust-side surfaces
  to source level.
- ADR-0027 — M12.x codegen amendments + Aggregate::List + drop schedule
  baseline.
- ADR-0034 — Constant::FnRef Call lowering; M-F.3.6 PRELUDE stubs use
  this path before intrinsic-rewrite retargets them.
- ADR-0044 — W2 Phase 2/3 PRELUDE+intrinsic-rewrite pattern; M-F.3.6
  extends.
- ADR-0044a (queued) — typed-Result lift; M-F.3.6 flips at landing.
- ADR-0049 — alpha honesty lanes + input-str-buf fix; M-F.3.6 inherits
  Str-buf C-ABI dispatch.
- ADR-0050 §"P1 follow-ups" §M-F.3.6 — parent.
- ADR-0050a — break/continue (no direct relation; landed in same Phase
  F.3 wave).
- ADR-0050b — for-loop; M-F.3.6 corpus uses for-loops over
  `read_file_lines(p)` for E2E.
- ADR-0050c — Str ownership Option A; binding for every M-F.3.6 surface.
- ADR-0050d — Dict design; potential M-F.3.6 corpus extension (`{path:
  contents}` dict mapping; out-of-scope this sprint).
- `findings/predicate-flip-cascade-discovery-deficit.md` — F30 SOP that
  §"Consequences" follows.
- `findings/adr-scope-reality-divergence.md` — F27 SOP that
  §"Verified-at-HEAD audit table" follows.
- `findings/lc100-str-use-after-move-regression-from-adr0050c.md` —
  Path D honest-debt disposition; M-F.3.6 corpus follows
  single-consume pattern.
- `findings/comp-lowering-zero-sentinel-collision.md` — open finding;
  M-F.3.6 §"F30 enumeration" `fixed-later-with-anchor` row.

## Why this ADR now

ADR-0050 §"P1 follow-ups" pinned M-F.3.6 as ~2-3 day D2 work. With
Wave 2 list[str] + Wave 3 dict tranche 1 in flight on parallel
branches, M-F.3.6 is a clean isolated sprint that unblocks the
JSON parser (M-F.3.7) and the AI-corpus end-to-end demo. Landing the
design ADR now means the M-F.3.6 PAIR can dispatch on `feature/f3-file-io`
immediately post-ratification without waiting for Wave 3 close. The
7-fn surface is small enough that a P10-direct PAIR (per ADR-0050
§A7) closes in ~7 hours wall time + 5-gate green.

Per `feedback_third_party_audit_2026_05_09.md` "the project owner has
flagged forever-deferral as the dominant honesty risk", M-F.3.6 is
the natural Phase F.3 closing item before v0.2.0 stable tag readiness
review (ADR-0050 §"v0.2.0 stable tag binding"). Shipping file-IO
completeness is the last language-half tile before the tag.

— P9-H opus tech-lead, 2026-05-16
