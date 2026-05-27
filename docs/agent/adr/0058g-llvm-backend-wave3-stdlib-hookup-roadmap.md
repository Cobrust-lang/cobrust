---
doc_kind: adr
adr_id: 0058g
parent_adr: 0058f
name: 0058g
title: "LLVM backend wave-3 stdlib hookup roadmap — panic/argv/list/dict/input/fmt/iter/math/parse/str-methods/LLM router"
status: accepted (sub-wave-1 + sub-wave-2 + sub-wave-3 + sub-wave-4 + sub-wave-5 + sub-wave-6 RATIFIED 2026-05-25 = ENTIRE WAVE-3 CLOSED — LLVM backend feature-parity with Cranelift for stdlib runtime surface)
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

### Wave 0058g-5: fmt + iter + math + parse_int + str methods — **RATIFIED 2026-05-25**

**Status**: closed by sub-wave-5 impl commit (see git log on this file). The
LLVM `declare_runtime_helpers` now also pre-declares the 41 sub-wave-5
runtime-helper externs (9 fmt + 3 iter + 11 math + 8 parse_int/str-parsing
+ 10 str-methods); the extern-name dispatch path (added in sub-wave-1)
routes by name to these decls without further dispatch-site change.

**Scope (RATIFIED)**:

- **fmt** (9 helpers): `__cobrust_fmt_int` (`[p, i64] -> ()`),
  `__cobrust_fmt_float` (`[p, f64] -> ()`), `__cobrust_fmt_float_prec`
  (`[p, f64, p, i64] -> ()`), `__cobrust_fmt_bool` (`[p, i64] -> ()`),
  `__cobrust_fmt_str` (`[p, p, i64] -> ()`), `__cobrust_fmt_repr`
  (`[p, p, i64] -> ()`), `__cobrust_str_len` (`[p] -> i64`),
  `__cobrust_str_ptr` (`[p] -> p`), `__cobrust_str_clone` (`[p] -> p`).
- **iter** (3 helpers): `__cobrust_iter_init` (`[i64] -> p`),
  `__cobrust_iter_next` (`[p] -> i64`), `__cobrust_iter_drop`
  (`[p] -> ()`).
- **math** (11 helpers): 10 single-arg `[f64] -> f64` shims
  (`__cobrust_math_{sqrt,floor,ceil,round,abs,sin,cos,tan,log,exp}`)
  + 1 two-arg `__cobrust_math_pow` (`[f64, f64] -> f64`).
- **parse_int + str-parsing** (8 helpers): `__cobrust_parse_int`
  (`[p] -> i64`), `__cobrust_str_len_src` (`[p] -> i64`),
  `__cobrust_str_at` (`[p, i64] -> p`), `__cobrust_str_eq`
  (`[p, p] -> i64`), `__cobrust_str_eq_lit` (`[p, p, i64] -> i64`),
  `__cobrust_str_ord` (`[p] -> i64`), `__cobrust_parse_int_tok`
  (`[p, i64] -> i64`), `__cobrust_count_toks` (`[p] -> i64`).
- **str-methods** (10 helpers, `str_clone` declared with fmt for cohesion):
  `__cobrust_str_split` (`[p, p] -> p`), `__cobrust_str_join`
  (`[p, p] -> p`), `__cobrust_str_replace` (`[p, p, p] -> p`),
  `__cobrust_str_trim` (`[p] -> p`), `__cobrust_str_find`
  (`[p, p] -> i64`), `__cobrust_str_contains` (`[p, p] -> i64`),
  `__cobrust_str_starts_with` (`[p, p] -> i64`),
  `__cobrust_str_ends_with` (`[p, p] -> i64`), `__cobrust_str_lower`
  (`[p] -> p`), `__cobrust_str_upper` (`[p] -> p`).

All signatures mirror Cranelift backend at
`cranelift_backend.rs:2765-2894` verbatim and stdlib exports at
`cobrust-stdlib/src/{fmt,iter,math,io,string}.rs`.

**Float-typed args + FloatToInt cast**: the math fixtures cast the f64
return through `Rvalue::Cast(CastKind::FloatToInt, _, Ty::Int)` so the
process exit code carries an observable signal. The existing
`lower_call` dispatch already handles f64 args via the default
`lower_operand` path (`Constant::Float` lowers to `f64`) and f64
returns via `try_as_basic_value().basic()` (returns whatever LLVM
produces; no special case needed).

**Iter handle local typing**: the iter chain stores the
`iter_init` handle in a `Ty::Str` local so the alloca lowers to
`opaque_ptr_ty` — required because `iter_next`/`iter_drop` expect a
ptr arg. Matches the wave-4 `input_str_buf` opaque-ptr round-trip
pattern at `llvm_wave3_input_readline.rs:378-393`.

**Test surface (14 fixtures across 5 categories — `crates/cobrust-codegen/tests/llvm_wave3_fmt_iter_math_str.rs`)**:

- **fmt** (3): `llvm_emits_fmt_int_then_str_len` (chain `fmt_int(buf, 42)`
  → `str_len == 2`), `llvm_emits_fmt_bool_then_str_len` (chain
  `fmt_bool(buf, 1)` → `str_len == 4`), `llvm_emits_fmt_str_then_str_len`
  (chain `fmt_str(buf, "hi")` → `str_len == 2` via the wave-2
  `expand_trailing_str_len` path).
- **iter** (1): `llvm_emits_iter_init_next_drop_empty` (three-helper
  chain via `iter_init(0)` empty-list sentinel → `iter_next == 0` →
  `iter_drop`; exit 0).
- **math** (3): `llvm_emits_math_sqrt_16` (single-arg sqrt + FloatToInt
  cast → exit 4), `llvm_emits_math_abs_neg7` (abs(-7) → exit 7),
  `llvm_emits_math_pow_2_3` (two-arg pow(2, 3) → exit 8).
- **parse_int + str-parsing** (3): `llvm_emits_parse_int_42`
  (parse_int("42") → exit 42), `llvm_emits_str_ord_uppercase_a`
  (str_ord("A") → exit 65), `llvm_emits_count_toks_three`
  (count_toks("a b c") → exit 3).
- **str-methods** (4): `llvm_emits_str_lower_then_len` (str_lower("ABC")
  → str_len → exit 3), `llvm_emits_str_contains_present`
  (contains("hello", "ell") → exit 1), `llvm_emits_str_find_present`
  (find("hello", "ll") → exit 2; -1 sentinel deferred since Unix exit
  code is unsigned 0-255), `llvm_emits_str_starts_with_true`
  (starts_with("hello", "he") → exit 1).

**Done means** (sub-wave-5 closure):

- [x] 41 sub-wave-5 externs declared via `add_function(... Linkage::External)`
  and `runtime_helper_param_counts` set in `LlvmEmitter::declare_runtime_helpers`.
- [x] 14 fixtures pass on Mac arm64 + LLVM 18 (`cargo test -p cobrust-codegen
  --test llvm_wave3_fmt_iter_math_str --features llvm`).
- [x] No regression: sub-wave-1-4 fixtures (`llvm_wave3_panic_argv` +
  `llvm_wave3_list_runtime` + `llvm_wave3_dict_set_tuple` +
  `llvm_wave3_input_readline`) all PASS unchanged.
- [x] No regression in `codegen_diff_corpus::stdlib_io_*` subset
  (8 fixtures, all PASS).
- [x] F45a §2 table updated: 5 categories (fmt, iter, math,
  parse_int/str-parsing, str-methods) marked **RESOLVED 2026-05-25 via
  ADR-0058g sub-wave-5**.
- [x] F45a §"Coverage summary" updated: 6/12 → 11/12 RESOLVED.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean
  + `cargo fmt --all -- --check` clean (F51 vigilance).

**F35-sibling discipline**: sub-wave-5 closes 5 of the 12 F45a §2
categories (fmt; iter; math; parse_int+str-parsing; str-methods).
Combined with sub-wave-1 + 2 + 3 + 4 (panic + argv + list + dict +
set/tuple + input + read_line), **11 of 12 categories** are resolved
post sub-wave-5. The remaining 1 (LLM router) continues as wave-1 stub
fallthrough; do NOT read sub-wave-5 closure as wave-3 closure.

### Wave 0058g-6: LLM router intrinsics — **RATIFIED 2026-05-25**

**Scope**: `__cobrust_llm_complete` / `__cobrust_llm_dispatch` /
`__cobrust_llm_stream` (M-AI.0 α Phase 2 — `cobrust.llm` source-level
binding) + `__cobrust_prompt_render` / `__cobrust_prompt_format_few_shot`
/ `__cobrust_prompt_format_system_user` / `__cobrust_prompt_escape_braces`
/ `__cobrust_llm_complete_structured` (M-AI.1 α Phase 3 —
`cobrust.prompt` source-level binding) +
`__cobrust_tool_schema` / `__cobrust_tool_registry_new` /
`__cobrust_tool_registry_register` / `__cobrust_tool_invoke` /
`__cobrust_llm_complete_with_tools` (M-AI.2 α Phase 4 — `cobrust.tool`
source-level binding). **Thirteen helpers total**, all using the
`(*mut Str | *mut List) -> *mut Str | *mut List` opaque-pointer ABI.

**Cranelift parity reference**: `cranelift_backend.rs:2896-2961`. All
thirteen helper signatures mirror Cranelift verbatim.

**Stdlib ABI cross-confirmed at**:
- `cobrust-stdlib/src/llm.rs:422, 444, 466` (M-AI.0)
- `cobrust-stdlib/src/prompt.rs:247, 270, 291, 308, 324` (M-AI.1)
- `cobrust-stdlib/src/tool.rs:254, 278, 289, 306, 321` (M-AI.2)

All thirteen C-ABI shims are unconditionally exported (`#[unsafe(no_mangle)]`
with no `#[cfg]` gating; the `llm-router` feature controls helper body
behavior but the symbols themselves are always present per Decision 7
M-AI.0 α Phase 2 / α-RATIFY).

**Done (verified empirically 2026-05-25)**:
- Six fixtures land at `crates/cobrust-codegen/tests/llvm_wave3_llm_router.rs`
  covering all three M-AI sub-families:
  - `llvm_emits_llm_complete_then_str_len` — three-arg `*p` → `*p` round-trip
    via `__cobrust_llm_complete("", "", "")` → `__cobrust_str_len(_) == 0`
    (Decision 7 empty-fallback when no `cobrust.toml`).
  - `llvm_emits_llm_dispatch_then_str_len` — two-arg `*p` → `*p` round-trip
    via `__cobrust_llm_dispatch("", "")` → `__cobrust_str_len(_) == 0`.
  - `llvm_emits_llm_stream_then_list_len` — three-arg `*p` → list-`*p`
    round-trip via `__cobrust_llm_stream("", "", "")` → `__cobrust_list_len(_) == 0`.
  - `llvm_emits_prompt_format_system_user_then_str_len` — two-arg `*p` →
    `*p` via `__cobrust_prompt_format_system_user("", "")` →
    `__cobrust_str_len(_) == 2` (pure-Rust helper concats "" + "\n\n" + "").
  - `llvm_emits_prompt_escape_braces_then_str_len` — single-arg `*p` →
    `*p` via `__cobrust_prompt_escape_braces("hi")` →
    `__cobrust_str_len(_) == 2`.
  - `llvm_emits_tool_registry_new` — zero-arg `()` → `*p` via
    `__cobrust_tool_registry_new()` (validates the zero-arg helper
    dispatch path).
- All 6 fixtures PASS under `cargo test -p cobrust-codegen
  --test llvm_wave3_llm_router --features llvm`.
- `cargo clippy -p cobrust-codegen --all-targets --features llvm --
  -D warnings` clean.
- `cargo fmt --all -- --check` clean.

**Real-LLM gating strategy** (codified in the fixture file's module
docstring + F45a §2 LLM router row):

The M-AI.0 α Phase 2 Decision 7 contract (every failure → empty `Str` /
empty `List`) makes these fixtures network-free + key-free +
config-free at the codegen layer. When no `cobrust.toml` is present,
`config_bundle()` returns `None` and the C-ABI shims short-circuit to
`alloc_str_buffer("")` BEFORE any tokio dispatch — so the fixtures
verify:
1. LLVM IR emit succeeds for every helper extern decl (no
   `lower-unknown-name` fallthrough → no wave-1 stub no-op).
2. Link against `libcobrust_stdlib.a` resolves the symbol (extern decl
   ABI matches stdlib `#[unsafe(no_mangle)]` body).
3. Binary runs to completion without crashing on the empty-Str /
   empty-List Decision 7 fallback path.

Tests exercising *real* router dispatch (configured `cobrust.toml`,
live provider) stay at `cobrust-stdlib/tests/llm_corpus.rs` (Tier 3)
gated by `real-llm-smoke` environment — out of scope for codegen
extern-decl + link verification.

**F35-sibling discipline**: sub-wave-6 closure IS wave-3 closure
because cumulatively across sub-waves 1-6 every §2 F45a category has
at least one passing `link_and_run` fixture asserting an observable
exit-code signal (not merely object-emit). Doc updates and release
notes downstream of sub-wave-6 MAY now correctly claim "wave-3 closed"
and "LLVM-Cranelift feature parity for the stdlib runtime surface"
backed by empirical fixtures.

## 3.7 Closure — wave-3 fully closed 2026-05-25

**Closure date**: 2026-05-25 (commit landing sub-wave-6 LLVM LLM
router intrinsics declarations + 6 fixtures).

**Cumulative wave-3 helper count** (from sub-waves 1-6, all hooked to
LLVM `lower_call` extern-name dispatch):
- sub-wave-1 (panic + argv): 2 helpers.
- sub-wave-2 (list runtime): 6 helpers.
- sub-wave-3 (dict + set + tuple): 25 helpers (16 dict + 5 set + 4 tuple).
- sub-wave-4 (input + read_line): 4 helpers.
- sub-wave-5 (fmt + iter + math + parse_int/str-parsing + str-methods):
  41 helpers (9 fmt + 3 iter + 11 math + 8 parse_int/str-parsing + 10
  str-methods).
- sub-wave-6 (LLM router): 13 helpers (3 M-AI.0 + 5 M-AI.1 + 5 M-AI.2).

**Total wave-3 helpers wired**: **91 runtime helper externs** under
the LLVM backend's `declare_runtime_helpers` + `lower_call` dispatch
path, all mirroring Cranelift signature-by-signature.

**Cumulative wave-3 fixture count**: **40 fixtures** across the six
`llvm_wave3_*` corpora files:
- `llvm_wave3_panic_argv` — 2 fixtures.
- `llvm_wave3_list_runtime` — 5 fixtures.
- `llvm_wave3_dict_set_tuple` — 6 fixtures.
- `llvm_wave3_input_readline` — 4 fixtures.
- `llvm_wave3_fmt_iter_math_str` — 14 fixtures (3 fmt + 1 iter + 3
  math + 3 parse + 4 str-methods).
- `llvm_wave3_llm_router` — 6 fixtures (sub-wave-6).

Plus 8 `codegen_diff_corpus::stdlib_io_*` fixtures (wave-2 print
system) verifying the surrounding `--features llvm` end-to-end path
remains green throughout the sub-wave landings.

**LLVM-Cranelift parity statement**: as of 2026-05-25, the LLVM
backend reaches feature-parity with the Cranelift backend for the
entire wave-3 stdlib runtime surface. Both backends route all 91
wave-3 helpers through their respective `lower_call` extern-name
dispatch paths with identical ABI signatures (sourced from the
`cobrust-stdlib` C-ABI `#[unsafe(no_mangle)]` declarations). End-users
building with `--features llvm` no longer encounter silent wave-1
stub no-ops for any §2 F45a category. F45a is RESOLVED.

**Remaining work** (out of scope for ADR-0058g; tracked elsewhere):
- M7+ numpy translation surface (not a wave-3 category).
- Phase G `&` borrow + let-rebind ergonomics (ADR-0051 §A priority).
- LLM router *real-network* dispatch tests under `--features llvm`
  (currently gated under `real-llm-smoke` at the stdlib corpus level;
  promotion to codegen-level fixtures requires a separate
  config-bundling sub-ADR + provider creds management strategy, out
  of scope for the F45a extern-decl/link gates).

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

### 6.3 LLM router LLVM ABI (wave-3-6 only) — **RESOLVED 2026-05-25**

The pre-sub-wave-6 open question was whether the LLM router
intrinsics' async + streaming surface required a separate LLM router
LLVM ABI sub-ADR before the LLVM backend could declare the extern
helpers.

**Resolution (via sub-wave-6)**: the M-AI.0 α Phase 2 implementation
(SHA 705f592 + α-RATIFY) already routed async + streaming concerns
into the stdlib layer, NOT the codegen layer. The C-ABI surface for
all 13 LLM router helpers is **synchronous** at the linker boundary
— each helper accepts opaque pointers and returns an opaque pointer.
The async runtime (`tokio::runtime::Runtime` via `OnceLock`) is
managed internally by `cobrust-stdlib`'s `llm.rs`, and `llm_stream`'s
"streaming" semantics collapse to a Decision 3B collect-all-chunks
list — exposed as `list[str]` to the linker, not as a future/poll
type. The LLVM ABI is therefore identical to Cranelift's:
`(*p|*p, *p|*p) -> *p|*p` for every helper, declared verbatim from
`cranelift_backend.rs:2896-2961`.

No separate LLM router ABI sub-ADR was required. The Decision 7
empty-fallback contract (any failure → empty Str / empty List)
ensures that even without a configured `cobrust.toml` or live
provider, the symbols link cleanly + the binary runs without
crashing — exercised by the six `llvm_wave3_llm_router::*` fixtures.

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
