---
doc_kind: adr
adr_id: 0058g
parent_adr: 0058f
name: 0058g
title: "LLVM backend wave-3 stdlib hookup roadmap — panic/argv/list/dict/input/fmt/iter/math/parse/str-methods/LLM router"
status: proposed
date: 2026-05-22
phase: Phase K wave-3 (LLVM backend full stdlib parity)
last_verified_commit: 4425310
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

### Wave 0058g-1: panic + argv (small, high-signal)

**Scope**: wire `__cobrust_panic` call emission + `__cobrust_argv` /
`__cobrust_capture_argv`.

**Rationale**: smallest category; unblocks two high-signal runtime behaviours
that make test programs fail silently:
- `unwrap_err()` / `assert` / `panic("msg")` source-level currently
  produces no abort signal under LLVM — programs that should crash continue
  executing with undefined state.
- Command-line programs reading `sys.argv` silently see NULL / empty.

**Known complexity**: `__cobrust_panic` needs to emit an `unreachable`
terminator after the call (the call is `noreturn`); LLVM requires explicit
CFG termination that Cranelift's `build_unreachable_inst()` handles.
Unwind table interaction is an open question (see §6).

**Done means**: `codegen_diff_corpus::category_panic_01_abort_on_panic` and
`category_argv_01_first_arg` fixtures PASS; LLVM AOT binary aborts on
`panic("msg")` with non-zero exit; `argv[0]` is accessible.

### Wave 0058g-2: list runtime (largest, deepest)

**Scope**: `__cobrust_list_new` / `__cobrust_list_set` / `__cobrust_list_get` /
`__cobrust_list_append` / `__cobrust_list_len` / `__cobrust_list_is_empty`.

**Rationale**: list is the most pervasive collection; without it, effectively
no real Cobrust program works under LLVM AOT. LC-100 corpus depends on list
almost universally.

**Known complexity**: list allocation interacts with the Drop schedule
(ADR-0050c TD-1 open question — see §6). The LLVM lowering must decide how to
handle the `__cobrust_list_drop` / `__cobrust_list_clone` ABI. The Cranelift
path is authoritative; consult `cranelift_backend.rs` List category before
implementing.

**Done means**: `codegen_diff_corpus::category_list_*` fixtures cover
`list_new` / `list_append` / `list_get` / `list_len` / `list_is_empty` +
a round-trip `[1, 2, 3]` → `print(list[1])` end-to-end; all PASS.

### Wave 0058g-3: dict + set + tuple

**Scope**: full `__cobrust_dict_*`, `__cobrust_set_*`, `__cobrust_tuple_*`
families.

**Rationale**: dict + set are the next most common collections after list.
Tuple is lighter (often stack-allocated in Cranelift; verify ABI before
implementing LLVM path).

**Done means**: `codegen_diff_corpus::category_dict_*` + `category_set_*` +
`category_tuple_*` fixtures cover construction + access + membership test +
iteration for each type; all PASS.

### Wave 0058g-4: input + read_line

**Scope**: `__cobrust_input` / `__cobrust_input_str_buf` /
`__cobrust_input_no_prompt` / `__cobrust_read_line`.

**Rationale**: stdin family deferred until after collection runtimes are stable
(input result is often stored in a list or dict). Lower priority than
collections but blocking for any interactive or stdin-parsing program.

**Known complexity**: stdin helpers allocate heap `Str` objects; requires
`__cobrust_str_new` + push chain already wired in wave-2. The LLVM path
should reuse the wave-2 str-buffer subroutines.

**Done means**: `codegen_diff_corpus::category_input_*` fixture reads a
hardcoded stdin via pipe, produces a string, prints it; PASS.

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

### 6.1 Drop schedule interaction with List/Dict allocations (ADR-0050c TD-1)

The Cranelift list/dict allocations interact with the drop schedule
(ADR-0050c TD-1 tracked debt). It is not yet clear whether the LLVM lowering
of `list_new` / `dict_new` needs a corresponding `list_drop` / `dict_drop`
call at scope-exit in the LLVM CFG, or whether the Cranelift drop-schedule
MIR lowering produces explicit `Drop(list)` terminators that `lower_call`
can catch generically.

**Resolution before wave-3-2 impl**: read ADR-0050c §TD-1 + audit the MIR
for a Cobrust program that creates + drops a list; confirm whether an explicit
`__cobrust_list_drop` call appears in the MIR terminator stream.

### 6.2 Panic semantics: does LLVM unwind table need wiring?

`__cobrust_panic` is a `noreturn` C function. In LLVM IR, a `noreturn`
call must be followed by an `unreachable` instruction to satisfy the basic
block terminator constraint. The Cranelift path uses
`cranelift_ir::InstructionData::Unreachable` for this.

Additionally: should the LLVM backend emit `invoke` (with unwind table) or
`call` + `unreachable` for panic? Cobrust does not use exceptions as the
default error path (CLAUDE.md §2.2), but LLVM's EH mechanisms may still
be needed for C++ interop or for DWARF unwind propagation.

**Resolution before wave-3-1 impl**: decision must be captured in a
sub-note to this ADR (or a sub-ADR) before 0058g-1 impl begins.

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
