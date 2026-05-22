---
doc_kind: adr
adr_id: 0058f
parent_adr: 0058a
name: 0058f
title: "LLVM backend wave-2 stdlib I/O hookup — runtime helpers + str materialize + extern call"
status: accepted
date: 2026-05-22
phase: Phase K wave-2 (LLVM backend stdlib I/O)
last_verified_commit: c8ba2bd
supersedes: []
superseded_by: []
relates_to: [adr:0058a, adr:0058d, adr:0058e, adr:0030, adr:0064, adr:0044, adr:0047]
discovered_by: "User-reported critical defect 2026-05-22 — `print(\"hi\")` LLVM AOT emits empty stdout; `print(fib(40))` computes silently then drops result. F45 finding filed concurrent (claim-vs-landed-scope drift)."
ratification_path: P9 ADR review; ratifies on impl-merge gate
---

# ADR-0058f: LLVM backend wave-2 stdlib I/O hookup

## 1. Context

ADR-0058a shipped the LLVM backend's **core lowering pass** (wave-1).
The wave-1 surface explicitly deferred (§8) "Runtime-helper / extern-name
Call lowering" to a follow-up. That follow-up — informally tagged
"wave-2" — never landed before v0.5.0 went out. The visible symptoms:

- `print("hi")` compiled via `--features llvm`: object built, link
  succeeded, executable produced empty stdout.
- `print(fib(40))` compiled via `--features llvm`: `fib` executed
  (CPU got busy) but no integer was ever printed; the `__cobrust_println_int`
  call was lowered to a wave-1 stub `write_place(dest, 0); branch target`.

Two stubs are responsible:

- `llvm_backend.rs:1606-1616` — `BodyLowerer::lower_call`'s wave-1
  fallthrough. When `func` is `Operand::Constant(Constant::Str(name))`
  (the extern-name callee shape MIR emits for stdlib intrinsics),
  the call is silently dropped and the destination written with `0`.
- `llvm_backend.rs:1746-1752` — `BodyLowerer::lower_constant(Constant::Str | Bytes)`.
  Returns `opaque_ptr_ty.const_null()`, leaving every `Str`-typed local
  null-pointered. Even paths that dispatched to a runtime helper would
  see `(NULL, 0)` arguments.

Cranelift backend (`cranelift_backend.rs:1383-1530` + `1130-1163` for
str-materialize) implements the full extern-call surface and is the
authoritative reference. The user's `print("hi")` works under
`--features ""` (Cranelift default) because of this asymmetry.

## 2. Why-Now

- User-facing: v0.5.0 LLVM backend is unusable for any program that
  prints (essentially every Cobrust program — `print` is the canonical
  side-effect surface).
- F35-sibling family (`docs/agent/findings/f35-sibling-commit-msg-vs-diff-drift.md`):
  v0.5.0 README + RELEASE_NOTES claimed "Phase K feature-complete"
  cascading off Drop / DI / IR-opt / JIT-conv landings without
  re-checking the I/O surface. F45 finding (filed concurrent with this
  ADR) ratifies the pattern: explicit "wave-N stub" comments in
  backend code MUST be cross-referenced to tracked tasks or excluded
  from feature-complete claims.

## 3. Decision

Mirror the Cranelift backend's extern-call + str-materialize ABI
verbatim, scoped to **the print system + the str-buffer subroutines
they depend on** (wave-2). Remaining extern callees stay wave-1 stubs
and are tracked in §7 Open Questions.

### 3.1 Scope

Wave-2 lights up these runtime helpers in the LLVM backend:

| Helper | C signature | Source-level shape |
|---|---|---|
| `__cobrust_println_int(i64)` | `i64 → void` | `print(x: i64)` |
| `__cobrust_println_bool(i8)` | `i8 → void` | `print(x: bool)` |
| `__cobrust_println_float(f64)` | `f64 → void` | `print(x: f64)` |
| `__cobrust_println_str_buf(*mut Str)` | `ptr → void` | `print(s: str)` (runtime Str) |
| `__cobrust_println(*const u8, usize)` | `(ptr, len) → void` | `print("literal")` legacy |
| `__cobrust_print_no_nl(*mut Str)` | `ptr → void` | `print_no_nl(s)` runtime path |
| `__cobrust_print_no_nl_lit(*const u8, usize)` | `(ptr, len) → void` | `print_no_nl("literal")` lit path |
| `__cobrust_str_new()` | `() → ptr` | str-buffer construction |
| `__cobrust_str_push_static(ptr, ptr, i64)` | `(buf, ptr, len) → void` | str-buffer literal pour-in |
| `__cobrust_str_drop(ptr)` | `ptr → void` | str-buffer drop (already wave-1) |

### 3.2 Module-level str-data interning

LLVM globals are module-scoped (vs Cranelift's per-function `GlobalValue`).
We move str-data interning to the `LlvmEmitter` and run it once during
`emit()`:

- New `LlvmEmitter::str_data_globals: HashMap<String, PointerValue<'ctx>>`
  tracks the rodata pointer for each unique payload.
- New `LlvmEmitter::intern_str_payloads(module)` walks every body's
  statements (Assign rvalues — Use + Aggregate operands) and
  terminator args, collecting `Constant::Str` payloads, and emits each
  as a private `unnamed_addr` `[N x i8]` constant global.
- The global's `i8*` pointer becomes the value reused by
  `materialize_str_data` / `materialize_str_buffer` lowering.

### 3.3 New BodyLowerer methods

Mirror the Cranelift names + ABIs:

- `materialize_str_data(payload) -> (ptr, len)` — returns the rodata
  pointer + i64 byte-length pair. Caller chooses whether to pass both
  to a `(ptr, len)` extern signature, or pour them through `str_buffer`.
- `materialize_str_buffer(payload) -> ptr` — calls `__cobrust_str_new()`,
  then `__cobrust_str_push_static(buf, ptr, len)` (when payload non-empty),
  returns the heap StringBuffer pointer. Used for runtime helpers whose
  C signatures take a single `*mut Str` (`println_str_buf` / `print_no_nl`).

### 3.4 lower_call dispatch (extern branch)

Mirror `cranelift_backend.rs:1439-1521`. The decision tree after the
user-FnRef branch:

1. If `func` is `Operand::Constant(Constant::Str(name))`:
   - Look up `runtime_helper_decls.get(name)`.
   - If found, lower each arg, expanding `Constant::Str` operands per
     the param-count map:
     - param_count == 2 && args.len() == 1 && arg[0] is Str → expand
       single Str into `(ptr, len)`.
     - args.len() + 1 == param_count && last arg is Str → expand
       trailing Str into `(ptr, len)`.
     - else → materialize as str-buffer pointer.
   - Emit `build_call`, write_place(dest, return_value), branch to target.
2. Else: fall through to wave-1 stub (preserves wave-1 surface for
   unknown extern names — input, file I/O, sys.argv).

### 3.5 lower_constant(Str | Bytes) fix

Replace the `const_null()` stub with:

- `Constant::Str(payload)` → call `materialize_str_buffer(payload)`
  → return the heap pointer. This mirrors the Cranelift Assign-side
  cascade fix at `cranelift_backend.rs:1266-1276` and is required for
  `let s: str = "hello"` to land a valid pointer in the slot.
- `Constant::Bytes(_)` → materialize_str_buffer of UTF-8 lossy payload
  (wave-2 keeps the Cranelift symmetry; Bytes-specific paths are
  out-of-scope).

## 4. ABI ratification

| Helper | LLVM ptr arg shape | Wave-1 Cranelift verified | Wave-2 LLVM mirrors |
|---|---|---|---|
| `__cobrust_println_int` | scalar i64 | yes | yes |
| `__cobrust_println_bool` | i8 (NOT i1 — bools widen at the call site) | yes | yes (zext i1→i8 if needed) |
| `__cobrust_println_float` | f64 | yes | yes |
| `__cobrust_println_str_buf` | `*mut StringBuffer` | yes | yes |
| `__cobrust_println` | `(*const u8, usize)` | yes | yes |
| `__cobrust_print_no_nl_lit` | `(*const u8, usize)` | yes | yes |
| `__cobrust_str_new` | `() -> ptr` | yes | yes |
| `__cobrust_str_push_static` | `(buf, ptr, len)` | yes | yes |

Bool widening note: MIR `Constant::Bool(b)` lowers to LLVM `i1` via
`bool_type().const_int`. The `__cobrust_println_bool(i8)` C ABI takes
`i8`, so the lowering must `build_int_z_extend(i1 → i8)` before the
call. The arg-expansion loop handles this in §3.4 step 1 inline.

## 5. Test gates

Extend `crates/cobrust-codegen/tests/codegen_diff_corpus.rs` with a
new `stdlib_io_*` section. Each fixture compiles + links a small
Cobrust source via `Backend::Llvm` then runs the resulting binary and
asserts the stdout matches a golden line.

Coverage (as-landed reality; F36-sibling spec-vs-code drift closed — original §5 described fixture-06 as `print(fib(10))` but landed code shipped `println_literal_path` instead; drift caught by Tier-2 audit aebbe278; closed here by amend + new fixture-08):

- `stdlib_io_01_println_int_42` — `print(42)` → `"42\n"`.
- `stdlib_io_02_println_bool_true` — `print(True)` → `"True\n"`.
- `stdlib_io_03_println_bool_false` — `print(False)` → `"False\n"`.
- `stdlib_io_04_println_float` — `print(1.5)` → `"1.5\n"`.
- `stdlib_io_05_println_str_literal` — `print("hello")` → `"hello\n"`.
- `stdlib_io_06_println_literal_path` — `__cobrust_println("world")` →
  `"world\n"`. Exercises the single-Str-arg → `(ptr, len)` expansion
  case (extern-name + `Constant::Str` arg path — independent value,
  retained as-landed).
- `stdlib_io_07_println_str_let_binding` — `let s: str = "hi"; print(s)`
  → `"hi\n"`.
- `stdlib_io_08_println_fib_result` — `print(fib(10))` → `"55\n"`.
  Exercises the user-fn `FnRef` call chain (recursive `fib` body[0] +
  `main` body[1] calling `fib(10)` then `__cobrust_println_int`). This
  is the exact failure mode from the user bug report (2026-05-22
  playground): `fib` computed correctly but the result was swallowed by
  the wave-1 `println` stub. High-value fixture: tests the full
  FnRef-recursive + extern-call integration path end-to-end.

Each fixture is `#[cfg(feature = "llvm")]` + gates on `linker_available()`
+ presence of `libcobrust_stdlib.a`. Tests skip gracefully when the
staticlib isn't on disk (no false-fail in default CI matrix; the LLVM
release-with-stdlib lane runs them concretely).

Pre-fix expectation: every stdlib_io_* fixture FAILS — stdout empty
or all-zero.

Post-fix expectation: every stdlib_io_* fixture PASSES.

## 6. Consequences

### 6.1 Positive

- LLVM AOT backend printable surface reaches feature-parity with
  Cranelift for the print system.
- F35-sibling claim drift closes: README + RELEASE_NOTES updated to
  cite v0.5.1 stdlib I/O hookup as a hotfix landing, not a v0.5.0
  feature-complete claim.
- Closes the most visible LLVM backend defect on the §2.5 "LLM agents
  write correctly on the first try" gate — an LLM emitting `print(x)`
  now sees stdout regardless of backend.

### 6.2 Negative

- Wave-2 scope is **print system only**. Other extern-call surfaces
  (input / read_line / sys.argv / file I/O / panic / list+dict
  constructors / format string runtime / math intrinsics / set / tuple
  / iter runtime / parse_int family / str method family) remain wave-1
  stubs in the LLVM backend (see §7).
- The full mirror introduces ~150 lines of new code in
  `llvm_backend.rs`, raising the file's LOC from 3018 to ~3170. The
  cost is borne to close a critical defect.

## 7. Open Questions / wave-3 deferrals

The following extern names are recognized by Cranelift but stay
wave-1 stubs in LLVM after this ADR. Each entry has a tracked F45
finding cross-reference + a "demonstrable surface" for a future ADR:

- `__cobrust_input` / `__cobrust_input_str_buf` / `__cobrust_input_no_prompt`
  / `__cobrust_read_line` — stdin family. Surface: `input("> ")`.
- `__cobrust_argv` / `__cobrust_capture_argv` — argv family. Surface: `import sys; sys.argv`.
- `__cobrust_list_new` / `__cobrust_list_set` / `__cobrust_list_get` /
  `__cobrust_list_append` / `__cobrust_list_len` /
  `__cobrust_list_is_empty` — list runtime.
- `__cobrust_dict_new` / `__cobrust_dict_set_*` / `__cobrust_dict_get_*` /
  `__cobrust_dict_contains_*` — dict runtime.
- `__cobrust_set_*` / `__cobrust_tuple_*` — set + tuple runtimes.
- `__cobrust_panic` — currently declared in wave-1 runtime_helper_decls
  but never called from `lower_call` extern branch (was used only by
  Assert in wave-1, which is fine; runtime-panic emission per
  `panic("msg")` source-level is wave-3).
- `__cobrust_iter_init` / `__cobrust_iter_next` / `__cobrust_iter_drop` —
  iter runtime. Surface: `for x in [1,2,3]`.
- `__cobrust_fmt_*` family — f-string runtime. Surface: `f"x = {x}"`.
- `__cobrust_math_*` family — math intrinsics. Surface: `import math; math.sqrt(2.0)`.
- `__cobrust_parse_int` / `__cobrust_str_eq` / `__cobrust_str_at` /
  `__cobrust_count_toks` / `__cobrust_parse_int_tok` etc — str parsing.
- `__cobrust_str_split` / `__cobrust_str_join` / `__cobrust_str_clone`
  / predicate family — str stdlib (ADR-0050e).
- LLM Router intrinsics (`__cobrust_llm_*`, `__cobrust_prompt_*`,
  `__cobrust_tool_*`) — α Phase 2/3/4 surfaces.

Honest-cite: until wave-3 lands these, programs that use any
non-print extern surface compile under `--features llvm` but produce
no observable side effect — same defect class this ADR fixes for print.
A user-facing warning on first compile under `--features llvm` is
out-of-scope for this ADR (would require a static analyzer scan over
the MIR for unknown extern names; tracked under F45 §"How-to-apply
forward").

## 8. Cross-references

- ADR-0058a — LLVM backend wave-1 (the deferral that this ADR closes).
- ADR-0058d — JIT/AOT lowering convergence (the Cranelift wave-1
  pattern this mirror is built off).
- ADR-0058e — AOT cranelift_backend substrate delegation (related
  Cranelift mirror; analog wave for AOT-side).
- ADR-0030 — `print` intrinsic surface and runtime ABI.
- ADR-0064 — print monomorphization source-surface cleanup (the
  `__cobrust_println_{int,bool,float}` family naming contract).
- ADR-0044 — `input` / argv / parse_int family (the wave-3 target).
- ADR-0047 Option H — `print_no_nl_lit` raw-bytes variant for literal
  callsites.
- F45 — LLVM backend wave-1 stub silently shipped (filed concurrent).

## 9. Done means

- `crates/cobrust-codegen/src/llvm_backend.rs` declares the 9 print
  family runtime helpers + has new `materialize_str_data` /
  `materialize_str_buffer` methods on `BodyLowerer` + new
  `intern_str_payloads` method on `LlvmEmitter` + extern-name dispatch
  in `lower_call`.
- `lower_constant(Str)` returns a heap StringBuffer pointer (not null).
- 8 stdlib_io_* fixtures in `codegen_diff_corpus.rs` PASS post-fix
  (fixture-08 added to close F36-sibling spec drift).
- F45 finding committed (cited in §2, §6, §7).
- README + skill doc updated (no "feature-parity" claim; explicit
  "stdlib I/O hookup landed v0.5.1; wave-3 surfaces tracked here").
- v0.5.1 RELEASE_NOTES file landed.
- Cargo.toml workspace.package.version bumped to 0.5.1.
