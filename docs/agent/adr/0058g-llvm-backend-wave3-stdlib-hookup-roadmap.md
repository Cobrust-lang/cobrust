---
doc_kind: adr
adr_id: 0058g
parent_adr: 0058f
name: 0058g
title: "LLVM backend wave-3 stdlib hookup roadmap — panic/argv/list/dict/input/fmt/iter/math/parse/str-methods/LLM router"
status: proposed (sub-wave-1 + sub-wave-2 + sub-wave-3 + sub-wave-4 RATIFIED 2026-05-25)
date: 2026-05-22
phase: Phase K wave-3 (LLVM backend full stdlib parity)
last_verified_commit: cb8893c
supersedes: []
superseded_by: []
relates_to: [adr:0058a, adr:0058f, adr:0030, adr:0044, adr:0050e, adr:0050c, adr:0049, adr:0051]
discovered_by: "Playground machine independent audit 2026-05-22 post-v0.5.1; F45a finding catalogue"
ratification_path: "P9 ADR review; each wave ratifies on impl-merge gate per §5 Done-means"
---

# ADR-0058g: LLVM backend wave-3 stdlib hookup roadmap

## 1. Context

**Default user path = Cranelift = full stdlib parity.** This ADR concerns only
the `--features llvm` experimental opt-in path. Release wheels do NOT enable
`--features llvm`. End-users on the standard `cobrust install` or
`cargo install cobrust-cli` path are not affected by wave-3 deferrals.

ADR-0058f (wave-2) landed the print system + str-buffer subroutines for the
LLVM backend in v0.5.1. The wave-2 landing closes the most critical defect
(`print("hi")` LLVM AOT emitting empty stdout) but leaves a large extern
surface still stub-forwarded.

F45a finding (2026-05-22) catalogued the full wave-3 scope from an independent
playground audit: every Cobrust program beyond pure arithmetic + print silently
misbehaves under `--features llvm`. The categories are:

- **input / stdin** — `input("> ")`, `read_line()`
- **argv** — `sys.argv`
- **list runtime** — `list_new` / `list_set` / `list_get` / `list_append` / `list_len` / `list_is_empty`
- **dict runtime** — `dict_new` / `dict_set_*` / `dict_get_*` / `dict_contains_*`
- **set + tuple** — `set_*` / `tuple_*`
- **panic** — `__cobrust_panic` source-level (note: already declared in wave-1 `runtime_helper_decls`; the gap is `lower_call` dispatch, not declaration)
- **fmt** — f-string runtime `__cobrust_fmt_*`
- **iter** — `iter_init` / `iter_next` / `iter_drop`
- **math** — `math.sqrt` / `.floor` / `.ceil` / `.round` / `.abs` / `.sin` / `.cos` / `.tan` / `.log` / `.exp` / `.pow`
- **parse_int / str parsing** — `parse_int` / `str_eq` / `str_at` / `str_len_src` / `str_ord` / `count_toks` / `parse_int_tok` / `str_eq_lit`
- **str methods (ADR-0050e)** — `split` / `join` / `replace` / `trim` / `find` / `contains` / `starts_with` / `ends_with` / `lower` / `upper` / `clone`
- **LLM router intrinsics** — `llm_complete` / `llm_dispatch` / `llm_stream` / `prompt_*` / `tool_*` (α Phase 2/3/4 surface, ADR-0049)

The wave-3 strategy mirrors the wave-2 approach: take the Cranelift backend's
`cranelift_backend.rs` extern-call dispatch as the authoritative reference,
and add the corresponding LLVM lowering in batches by extern category.

## 2. Decision

Batch wave-3 work into 6 sequential waves ordered by impact and implementation
difficulty. Each wave:

1. Adds the LLVM `runtime_helper_decls` entries for the category.
2. Adds `lower_call` extern-name dispatch branch mirroring the Cranelift path.
3. Adds `codegen_diff_corpus::category_*` fixtures (stdout-diff, not
   object-emit) that must PASS before the wave merges.
4. Updates RELEASE_NOTES + README + skill doc with F35-sibling honest-cite:
   lists exactly which externs landed in this wave vs which remain stub.

## 3. Phasing

### Wave 0058g-1: panic + argv (small, high-signal) — **RATIFIED 2026-05-25**

**Status**: closed by sub-wave-1 impl commit (see git log on this file). The
implementation landed two extern hookups + two regression fixtures, matching
the wave-2 pattern.

**Scope** (as landed):
- `__cobrust_argv() -> ptr` extern declaration in `runtime_helper_decls`
  (mirrors Cranelift `cranelift_backend.rs:2822` zero-arg ptr-return shape).
- `__cobrust_panic` `unreachable` terminator special case in `lower_call`'s
  extern-dispatch branch (the panic decl already existed from wave-2 prep at
  `llvm_backend.rs:1095-1101`).
- `__cobrust_capture_argv` deliberately NOT declared at LLVM level: it is
  invoked exclusively from the C shim (`cobrust-cli/runtime/cobrust_main.c:21-25`),
  not from MIR; Cranelift also omits it; LLVM matches for parity.

**Resolved complexity**: per §6.2 below, the panic dispatch emits
`call` + `unreachable` (no `invoke` / EH unwind table). This matches
Cranelift's `InstructionData::Unreachable` and is consistent with
CLAUDE.md §2.2 ("exceptions reserved for truly unrecoverable").

**Done means** (landed): `llvm_wave3_panic_argv::llvm_emits_argv_extern_call_and_exits_zero`
+ `llvm_wave3_panic_argv::llvm_emits_panic_extern_call_with_unreachable`
fixtures PASS on Mac arm64 + LLVM 18 (`--features llvm`). Argv binary
exits 0; panic binary exits with `INTERNAL_PANIC = 3` and writes the
panic message to stderr.

**F35-sibling discipline**: sub-wave-1 closes 2 of the 12 F45a §2
categories (panic + argv). The remaining 10 categories (list / dict /
set / tuple / input / fmt / iter / math / parse_int+str-parsing /
str-methods / LLM router) remain wave-1 stub fallthrough; do NOT
read sub-wave-1 closure as wave-3 closure.

### Wave 0058g-2: list runtime (largest, deepest) — **RATIFIED 2026-05-25**

**Status**: closed by sub-wave-2 impl commit (see git log on this file). The
implementation landed 6 extern hookups + 5 regression fixtures.

**Scope** (as landed):
- `__cobrust_list_new(elem_size: i64, len: i64) -> *mut ListBuffer` —
  mirrors Cranelift `cranelift_backend.rs:2670` ABI verbatim.
- `__cobrust_list_set(list, i, v) -> void` — mirrors Cranelift line 2671.
- `__cobrust_list_get(list, i) -> i64` — mirrors Cranelift line 2672.
- `__cobrust_list_len(list) -> i64` — mirrors Cranelift line 2673.
- `__cobrust_list_is_empty(list) -> i64` (0/1 per SwitchInt convention) —
  mirrors Cranelift line 2680.
- `__cobrust_list_append(list, v) -> void` — mirrors Cranelift line 2682.

All 6 added to `runtime_helper_decls` + `runtime_helper_param_counts` in
`LlvmEmitter::declare_runtime_helpers`. The `lower_call` extern-name
dispatch path (added in sub-wave-1) routes by name to these decls without
any per-helper special-case needed.

**Resolved complexity** (§6.1 below): list allocation/Drop interaction is
already handled at the wave-1 layer. `__cobrust_list_drop` +
`__cobrust_list_drop_elems` were declared and wired in
`emit_drop_for_ty` (see `llvm_backend.rs::emit_drop_for_ty` Drop dispatch
arm; the corresponding `Terminator::Drop` lowering at `lower_terminator`
emits the right helper for `Ty::List(Ty::Str)` vs `Ty::List(_)`). The
MIR `compute_drop_schedule` pass inserts `Terminator::Drop` for owning
locals reaching end-of-scope. Sub-wave-2 adds only the
constructor/accessor surface; no Drop-path change required.

**Done means** (landed): 5 fixtures in
`crates/cobrust-codegen/tests/llvm_wave3_list_runtime.rs`:
1. `llvm_emits_list_new_extern_call` — `list_new(8, 0)` lowers + links, exits 0.
2. `llvm_emits_list_append_then_len` — `new + append + len` → exit 1.
3. `llvm_emits_list_set_then_get` — `new(3) + set(1, 99) + get(1)` → exit 99.
4. `llvm_emits_list_is_empty_after_new` — `new(0) + is_empty` → exit 1.
5. `llvm_emits_list_end_to_end_roundtrip` — all 6 helpers chained →
   exit 243 (10+200+30+3+0). Acts as the integration capstone.

All 5 PASS on Mac arm64 + LLVM 18 (`--features llvm`).

**F35-sibling discipline**: sub-wave-2 closes 1 of the 12 F45a §2
categories (list runtime). Combined with sub-wave-1 (panic + argv), **3
of 12 categories** are resolved post sub-wave-2. The remaining 9
(dict / set+tuple / input / fmt / iter / math / parse_int+str-parsing /
str-methods / LLM router) remain wave-1 stub fallthrough; do NOT read
sub-wave-2 closure as wave-3 closure.

### Wave 0058g-3: dict + set + tuple — **RATIFIED 2026-05-25**

**Status**: closed by sub-wave-3 impl commit (see git log on this file). The
implementation landed 25 extern hookups (16 dict + 5 set + 4 tuple) + 6
regression fixtures + extension of `emit_drop_for_ty` to dispatch
`__cobrust_dict_drop` on `Ty::Dict(_, _)` (closes ADR-0058g §6.1 TD-1
dict portion below).

**Scope** (as landed):

*Dict family — 16 externs, mirrors Cranelift `cranelift_backend.rs:2684-2742`:*
- Erased helpers (4): `__cobrust_dict_new(i64, i64, i64) -> *mut` /
  `__cobrust_dict_drop(*mut) -> void` / `__cobrust_dict_len(*mut) -> i64` /
  `__cobrust_dict_is_empty(*mut) -> i64`.
- Legacy untyped (i64, i64) shims (2): `__cobrust_dict_set` /
  `__cobrust_dict_get` (M12.x backward-compat per ADR-0050d Decision 7).
- Typed (K, V) shims (10, per ADR-0050d Decision 7A): the cross-product
  `_set_K_V` / `_get_K_V` / `_contains_K` across `K ∈ {i64, str}` ×
  `V ∈ {i64, str}` — `_set_i64_i64`, `_set_i64_str`, `_set_str_i64`,
  `_set_str_str` + matching `_get_*` + 2 `_contains_*` shims.

*Set<i64> family — 5 externs, mirrors Cranelift `cranelift_backend.rs:2745-2752`:*
- `__cobrust_set_new(i64, i64) -> *mut` /
  `__cobrust_set_insert(*mut, i64) -> void` /
  `__cobrust_set_contains(*mut, i64) -> i64` /
  `__cobrust_set_len(*mut) -> i64` /
  `__cobrust_set_drop(*mut) -> void`.

*Tuple family — 4 externs, mirrors Cranelift `cranelift_backend.rs:2755-2758`:*
- `__cobrust_tuple_new(i64) -> *mut` /
  `__cobrust_tuple_set(*mut, i64, i64) -> void` /
  `__cobrust_tuple_get(*mut, i64) -> i64` /
  `__cobrust_tuple_drop(*mut, i64) -> void` (note: 2-arg ABI; arity is
  passed as the second arg, unlike `list_drop`'s 1-arg ABI).

All 25 added to `runtime_helper_decls` + `runtime_helper_param_counts` in
`LlvmEmitter::declare_runtime_helpers`. The `lower_call` extern-name
dispatch path (added in sub-wave-1) routes by name to these decls without
any per-helper special-case needed.

**Resolved complexity** (§6.1 below, dict portion): `emit_drop_for_ty`
gained a `Ty::Dict(_, _) → __cobrust_dict_drop(ptr)` arm matching
Cranelift's `lower_drop` at `cranelift_backend.rs:1232-1237`. `Ty::Set`
/ `Ty::Tuple` Drop stays as no-op fallthrough — Cranelift explicitly
no-ops these ("Tuple/Set drops are not yet plumbed; M12.x leaves these
as no-op" at `cranelift_backend.rs:1238-1240`); strict parity preserved.
Phase G widening will lift both backends together.

**Done means** (landed): 6 fixtures in
`crates/cobrust-codegen/tests/llvm_wave3_dict_set_tuple.rs`:
1. `llvm_emits_dict_new_len_is_empty` — `dict_new + len + is_empty` →
   exit 1 (empty dict: len=0 + is_empty=1).
2. `llvm_emits_dict_set_then_get_i64_i64` — typed `_set_i64_i64 + _get_i64_i64`
   round-trip → exit 77.
3. `llvm_emits_dict_contains_after_set` — `_set_i64_i64 + _contains_i64`
   → exit 1.
4. `llvm_emits_set_end_to_end` — `set_new + insert(11) + insert(22) +
   insert(11) [dup] + contains(11) + len` → exit 3 (contains=1 +
   distinct=2).
5. `llvm_emits_tuple_end_to_end` — `tuple_new(3) + set × 3 + get × 2 +
   tuple_drop(p, 3)` → exit 150 (200 - 50). Verifies tuple_drop's
   2-arg ABI lowers correctly.
6. `llvm_emits_dict_end_to_end_with_drop` — capstone exercising every
   untyped + typed-i64-i64 helper, ending with `Terminator::Drop` on the
   dict local so `emit_drop_for_ty`'s new `Ty::Dict → dict_drop` arm
   fires → exit 33 (10+20+2+0+1).

All 6 PASS on Mac arm64 + LLVM 18 (`--features llvm`).

**F35-sibling discipline**: sub-wave-3 closes 2 of the 12 F45a §2
categories (dict; set+tuple combined). Combined with sub-wave-1 + 2
(panic + argv + list), **5 of 12 categories** are resolved post
sub-wave-3. The remaining 7 (input / fmt / iter / math /
parse_int+str-parsing / str-methods / LLM router) remain wave-1 stub
fallthrough; do NOT read sub-wave-3 closure as wave-3 closure.

### Wave 0058g-4: input + read_line — **RATIFIED 2026-05-25**

**Status**: closed by sub-wave-4 impl commit (see git log on this file). The
implementation landed four extern hookups + four regression fixtures.

**Scope** (as landed):
- `__cobrust_input(prompt_ptr, prompt_len) -> *mut Str` extern declaration in
  `runtime_helper_decls` (mirrors Cranelift `cranelift_backend.rs:2811`
  `[p, i64] -> p`). The single source-Str arg routes through the wave-2
  `expand_str_to_ptr_len` path in `lower_call` (literal prompt → ptr/len pair).
- `__cobrust_input_str_buf(prompt_buf) -> *mut Str` extern declaration
  (mirrors Cranelift `cranelift_backend.rs:2813` `[p] -> p`). The runtime
  Str-buffer overload — single ptr param, no expansion.
- `__cobrust_input_no_prompt() -> *mut Str` zero-arg extern declaration
  (mirrors Cranelift `cranelift_backend.rs:2815` `[] -> p`).
- `__cobrust_read_line() -> *mut Str` zero-arg extern declaration
  (mirrors Cranelift `cranelift_backend.rs:2819` `[] -> p`). W2 cap;
  typed `Result[str, IoError]` deferred to ADR-0044a per
  `cobrust-codegen/src/cranelift_backend.rs:2816-2818` comment.

**Stdlib ABI cross-confirmed** at `cobrust-stdlib/src/io.rs:224,248,268,343`.
All four helpers return `*mut StrBuffer` (`ptr_ty`); none Drop-schedule at
this layer (the str return value is owned by the caller; the existing
`__cobrust_str_drop` covers the Drop path).

**Test fixtures** (4 fixtures shipped in
`crates/cobrust-codegen/tests/llvm_wave3_input_readline.rs`):
- `llvm_emits_input_extern_call_with_prompt` — `__cobrust_input("> ")` with
  piped stdin `b"hello\n"`; verifies the wave-2 `expand_str_to_ptr_len`
  dispatch path handles the single-Str-arg → (ptr, len) expansion for the
  input family.
- `llvm_emits_input_no_prompt_extern_call` — zero-arg variant with piped
  stdin `b"world\n"`.
- `llvm_emits_read_line_extern_call` — zero-arg low-level reader with
  piped stdin `b"line one\n"`.
- `llvm_emits_input_str_buf_extern_call` — Str-buffer prompt overload
  using a fresh `__cobrust_str_new()`-allocated buf passed by-pointer.
  The buf local is declared `Ty::Str` so its alloca lowers to
  `opaque_ptr_ty` (matches the wave-2 list fixture pattern where
  `Ty::List(...)` also lowers to opaque ptr); this avoids the LLVM
  verifier rejecting an i64→ptr arg mismatch.

**Stdin handling pattern**: `Command::stdin(Stdio::piped())` +
`child.stdin.as_mut().write_all(...)` + `wait_with_output()`. Mirrors
`cobrust-cli/tests/intrinsics_input.rs:164-183`.

**Done means** (sub-wave-4 closure):
- Both Mac arm64 and Linux x86_64 PASS the four fixtures under
  `--features llvm`.
- F35-sibling honest cite: input + read_line are RESOLVED in F45a §2.
- F37 silent-rot guard: each fixture asserts `status.success()` AND
  surfaces stdout/stderr on assertion failure (no silent skip when
  binary misbehaves).
- F51 vigilance: test file ships with module-level
  `#![allow(clippy::items_after_statements, clippy::similar_names,
  clippy::unwrap_used, clippy::expect_used, reason = "test corpus style
  (F51 lint discipline)")]` to prevent clippy lint silent-rot under
  `--features llvm`.

**Sub-wave-4 narrowly scoped (F35-sibling discipline)**: this wave
addresses input + read_line ONLY. Six of twelve F45a §2 categories
remain wave-1 stubs after this sprint (fmt / iter / math /
parse_int+str-parsing / str-methods / LLM router). DO NOT read
sub-wave-4 closure as wave-3 closure.

### Wave 0058g-5: fmt + iter + math + parse_int + str methods

**Scope**: `__cobrust_fmt_*` / `__cobrust_iter_*` / `__cobrust_math_*` /
`__cobrust_parse_int` + str-parsing family / ADR-0050e str method family.

**Rationale**: this is the largest batch by extern count but each individual
helper is mechanically similar to the wave-2 pattern (declare → lower_call
dispatch → test). Grouped as one wave for ADR cleanliness; may be split into
sub-waves at implementation time if complexity warrants.

**F35-sibling discipline**: each sub-batch within this wave must list
which helpers LANDED and which remain stub in its merge commit message.
Do not claim "wave-5 complete" until every extern in the scope list passes
its `codegen_diff_corpus` fixture.

**Done means**: `codegen_diff_corpus::category_fmt_01_fstring_int` +
`category_iter_01_for_loop_list` + `category_math_01_sqrt` +
`category_parse_int_01_from_str` + `category_str_methods_01_split` all PASS
on Mac arm64 + LLVM 18.

### Wave 0058g-6: LLM router intrinsics (gated, special)

**Scope**: `__cobrust_llm_complete` / `__cobrust_llm_dispatch` /
`__cobrust_llm_stream` / `__cobrust_prompt_*` / `__cobrust_tool_*`.

**Rationale**: AI-native surface of ADR-0049 alpha. Deferred last because:
1. Requires `cobrust-llm-router` crate to be importable from `cobrust-codegen`
   (or the LLVM backend to stub the call-site ABI contract separately).
2. LLM router intrinsics involve async + streaming (thread spawning) — the
   LLVM ABI for these is non-trivial and not yet specified.
3. End-users testing `--features llvm` are unlikely to hit this surface before
   hitting the wave-3 collection gaps.

**Gate**: wave-6 impl MUST wait until waves 0058g-1 through 0058g-5 are
merged. The LLM router intrinsic ABI must be specified in a separate
sub-ADR (0058g-6a or equivalent) before implementation begins.

**Done means**: `codegen_diff_corpus::category_llm_router_01_llm_complete_stub`
fixture compiles + links under `--features llvm,cobrust-llm-router`; LLVM AOT
binary produces the same output as Cranelift for a synthetic (non-network)
`llm_complete` call; no wave-1 stub no-op on the happy path.

## 4. Implementation pattern (all waves)

Each wave follows the wave-2 template:

1. **Declare** — add the category's helpers to `declare_runtime_helpers`
   in `LlvmEmitter::emit()`. Mirror `cranelift_backend.rs` extern declarations.
2. **Lower** — add the extern-name dispatch branch in
   `BodyLowerer::lower_call`. Mirror the Cranelift dispatch arm.
3. **Materialize** — add any new `BodyLowerer::materialize_*` helpers needed
   for the category's arg shapes (e.g., list-pointer passing, dict-key-hash).
4. **Test** — add `codegen_diff_corpus::category_*` fixtures (stdout-diff,
   not object-emit). Minimum: one fixture per distinct extern name in the
   category.
5. **Annotate** — update all `// Wave-N stub` comments in the category's
   dispatch path to cross-reference this ADR's wave number + the test fixture
   URN (F45a §3.2 contract).
6. **Document** — update RELEASE_NOTES, README Phase K bullet, skill doc §9k
   with the exact list of what LANDED vs what remains stub.

## 5. Done means (full wave-3 closure)

Wave-3 is closed when ALL of the following hold simultaneously:

- LLVM stdout/exit matches Cranelift on the **full LC-100 corpus** (not just
  `stdlib_io_*` fixtures) when compiled with `--features llvm`. Target: same
  `100/100` pass rate as Cranelift's current production baseline.
- Every category in §3 has at least one `codegen_diff_corpus::category_*`
  fixture (stdout-diff semantics) that PASSES.
- Every `// Wave-1 stub` comment in `llvm_backend.rs` has been either
  removed (if the extern is now wired) or updated to `// Wave-3 stub →
  finding:f45a §2; tracked in ADR-0058g-N; see test: category_X_01` per
  the F45a §3.2 contract.
- RELEASE_NOTES for the wave-3 release includes the full per-extern parity
  table (F45a §3.3 honest-cite contract).
- README Phase K bullet and skill doc §9k both lead with
  "Default backend = Cranelift = full stdlib parity" before describing
  LLVM backend status.

## 6. Open questions

### 6.1 Drop schedule interaction with List/Dict allocations (ADR-0050c TD-1) — **RESOLVED FOR LIST + DICT 2026-05-25**

The Cranelift list/dict allocations interact with the drop schedule
(ADR-0050c TD-1 tracked debt). The pre-sub-wave-2 open question was
whether the LLVM lowering of `list_new` needs a corresponding
`list_drop` call at scope-exit in the LLVM CFG, or whether the
drop-schedule MIR lowering produces explicit `Drop(list)` terminators
that `lower_call` can catch generically.

**Resolution (list portion)**: the MIR `compute_drop_schedule` pass at
`crates/cobrust-mir/src/drop.rs` inserts explicit `Terminator::Drop`
nodes for owning locals reaching end-of-scope. The LLVM backend's
`lower_terminator` dispatches `Terminator::Drop` to `emit_drop_for_ty`
at `llvm_backend.rs:1903-1907`, which emits `__cobrust_list_drop` for
`Ty::List(_)` and `__cobrust_list_drop_elems` for `Ty::List(Ty::Str)`
(both decls already in `runtime_helper_decls` from wave-1 prep at
`llvm_backend.rs:1077-1093`). Sub-wave-2 confirmed this path requires
no change for list constructor/accessor wiring.

**Dict portion (RESOLVED 2026-05-25 via sub-wave-3)**: same audit
repeated. Sub-wave-3 declared `__cobrust_dict_drop` / `__cobrust_set_drop`
/ `__cobrust_tuple_drop` externs (mirroring Cranelift signatures at
`cranelift_backend.rs:2684-2758`) AND extended `emit_drop_for_ty` with a
`Ty::Dict(_, _) → __cobrust_dict_drop(ptr)` arm. The MIR
`compute_drop_schedule` pass already emits `Terminator::Drop` for owning
dict locals — same path as list, no MIR change required. `Ty::Set` /
`Ty::Tuple` Drop kept as no-op for strict parity with Cranelift
`lower_drop` (`cranelift_backend.rs:1238-1240` explicit no-op comment
"Tuple/Set drops are not yet plumbed; M12.x leaves these as no-op").
Both backends widen together in Phase G. Test:
`llvm_wave3_dict_set_tuple::llvm_emits_dict_end_to_end_with_drop` builds
MIR ending in `Terminator::Drop { place: _d, ... }` and asserts the
binary exits with the expected value (the bug-trigger would be a
double-free or use-after-free if the dispatch were broken, manifesting
as non-zero exit / signal).

### 6.2 Panic semantics: does LLVM unwind table need wiring? — **RESOLVED 2026-05-25**

`__cobrust_panic` is a `noreturn` C function. In LLVM IR, a `noreturn`
call must be followed by an `unreachable` instruction to satisfy the basic
block terminator constraint. The Cranelift path uses
`cranelift_ir::InstructionData::Unreachable` for this.

**Resolution**: the LLVM backend emits `call` + `unreachable` (NOT
`invoke` / EH unwind table) for `__cobrust_panic`. Rationale:
- Cobrust does not use exceptions as the default error path
  (CLAUDE.md §2.2 — "exceptions reserved for truly unrecoverable";
  ADR-0049 alpha honesty contract — `Result<T, E>` is default).
- The stdlib `__cobrust_panic` handler (`cobrust-stdlib/src/panic.rs:47`)
  calls `std::process::exit(INTERNAL_PANIC)` directly; there is no
  unwind path to propagate. An `invoke` instruction with a landing pad
  would be dead infrastructure.
- DWARF unwind propagation for debugger backtrace is handled by the
  C runtime's `crt0` + stdlib's panic handler emitting frames before
  `_exit`; the LLVM-level CFG only needs `unreachable` to satisfy
  the verifier.

**Implementation**: in `BodyLowerer::lower_call`, after the extern-name
dispatch path emits the `build_call`, special-case `name ==
"__cobrust_panic"` to emit `build_unreachable()` and return early
(skipping the post-call `write_place` + `build_unconditional_branch`,
which would be dead code after a noreturn callee). See sub-wave-1 impl
commit on this file.

Future panic-family helpers (`__cobrust_assert` non-cond branch,
`__cobrust_result_err_panic`) that are also `-> !` should follow the
same pattern — extend the special case to a name-set match when those
hookups land.

### 6.3 LLM router LLVM ABI (wave-3-6 only)

The LLM router intrinsics involve async / streaming. Their LLVM-level ABI
(thread handle types, future/poll representation) is unspecified. This is
gated on waves 1-5 completing and on a separate LLM router ABI ADR.

## 7. Cross-references

- ADR-0058a — wave-1 LLVM backend (origin of the deferral chain).
- ADR-0058f — wave-2 print system (this ADR's predecessor; §7 is the
  original stub catalogue that F45a + this ADR expand).
- ADR-0050c — Drop schedule + list/dict TD-1 (§6.1 open question).
- ADR-0050e — str method family (wave-3-5 scope).
- ADR-0044 — `input` / argv / parse_int family surface spec (wave-3-1, 3-4).
- ADR-0049 — α honesty + LLM router intrinsics surface (wave-3-6 scope).
- ADR-0051 — LLM-first design principle (§2.5 north star: LLM agents write
  `s.split(",")` not `__cobrust_str_split(s, ",")`; wave-3-5 unblocks this
  for LLVM AOT).
- F45 — parent finding; F45a — child catalogue; this ADR is the wave-3 roadmap.
- F35-sibling — §3.5 implementation pattern step 6 operationalizes
  F35-sibling discipline for every wave merge.
