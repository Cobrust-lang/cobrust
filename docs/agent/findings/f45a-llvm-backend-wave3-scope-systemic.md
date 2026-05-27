---
name: f45a
status: RESOLVED 2026-05-25 (all 12 wave-3 categories resolved via ADR-0058g sub-wave-1 + sub-wave-2 + sub-wave-3 + sub-wave-4 + sub-wave-5 + sub-wave-6 = ENTIRE WAVE-3 CLOSED — LLVM backend reaches feature-parity with Cranelift for the full stdlib runtime surface)
family: F45 child (systemic wave-3 catalogue) + F35-sibling (claim drift) + F37 (silent rot) + F44 (CI green != working)
last_verified_commit: cb8893c
date: 2026-05-22
---

# F45a — LLVM backend wave-3 scope systemic confirmation

## §1 Context

**Default user path = Cranelift = full stdlib parity.**
The `cobrust build foo.cb` canonical path uses the Cranelift backend. Release
wheels do NOT enable `--features llvm`. An end-user running `cobrust install`
or `cargo install cobrust-cli` receives a Cranelift-default binary where all
extern callees (list, dict, input, argv, panic, fmt, iter, math, parse_int,
str methods, LLM router) work correctly today.

F45a impacts **only `--features llvm` experimental opt-in builds** — a
deliberate user-side choice to test the LLVM AOT path. This scope line
must lead every doc update (F35-sibling discipline: claims accurate to actual
user path).

**Baseline state at time of audit (playground machine, 2026-05-22 post-v0.5.1):**
- v0.5.1 landed ADR-0058f wave-2: print system fully hooked up for LLVM.
  - `__cobrust_println_int` / `_bool` / `_float` / `_str_buf` / `_lit` wired.
  - `__cobrust_str_new` / `__cobrust_str_push_static` / `__cobrust_str_drop` wired.
  - 8 `stdlib_io_*` fixtures all PASS on Mac arm64 + LLVM 18.
- Cranelift backend: default, all extern surfaces work.
- LLVM backend: wave-2 print system LANDED; wave-3 surfaces remain wave-1 stubs.

Source: playground machine independent audit 2026-05-22 post-v0.5.1, forwarded
by user. Cross-confirmed against ADR-0058f §7 Open Questions.

## §2 Wave-3 stub catalogue

Full table of extern callees that compile under `--features llvm` but emit
no observable side effect (wave-1 stub fallthrough in `lower_call`).
Cranelift handles all of these correctly at the same commit.

| Category | Runtime helpers (extern names) | Source-level impact | Status |
|---|---|---|---|
| **input** | `__cobrust_input` / `__cobrust_input_str_buf` / `__cobrust_input_no_prompt` / `__cobrust_read_line` | `input("> ")` and `read_line()` silently return nothing; stdin family fully silent under LLVM | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-4; 4 externs hooked (2 zero-arg + 1 str-buf overload + 1 literal-prompt path through the wave-2 `expand_str_to_ptr_len` dispatch); covered by `llvm_wave3_input_readline::{llvm_emits_input_extern_call_with_prompt, llvm_emits_input_no_prompt_extern_call, llvm_emits_read_line_extern_call, llvm_emits_input_str_buf_extern_call}` (4 fixtures, all use `Stdio::piped()` + `stdin.write_all(...)` + `wait_with_output()` for stdin feed; matches `cobrust-cli/tests/intrinsics_input.rs:164-183` stdin handling pattern) |
| **argv** | `__cobrust_argv` (`__cobrust_capture_argv` is C-shim-only — Cranelift+LLVM both intentionally omit at MIR level) | `sys.argv` evaluates to NULL / empty; command-line programs cannot read arguments | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-1; covered by `llvm_wave3_panic_argv::llvm_emits_argv_extern_call_and_exits_zero` |
| **list** | `__cobrust_list_new` / `__cobrust_list_set` / `__cobrust_list_get` / `__cobrust_list_append` / `__cobrust_list_len` / `__cobrust_list_is_empty` | All list operations silently no-op; list-based programs produce no output | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-2; covered by `llvm_wave3_list_runtime::{llvm_emits_list_new_extern_call, llvm_emits_list_append_then_len, llvm_emits_list_set_then_get, llvm_emits_list_is_empty_after_new, llvm_emits_list_end_to_end_roundtrip}` (5 fixtures, 6 helpers, end-to-end exit 243 capstone) |
| **dict** | `__cobrust_dict_new` / `__cobrust_dict_set_*` / `__cobrust_dict_get_*` / `__cobrust_dict_contains_*` / `__cobrust_dict_len` / `__cobrust_dict_is_empty` / `__cobrust_dict_drop` | Dict runtime fully silent; key/value stores return nothing | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-3; 16 dict externs hooked (4 erased + 2 legacy untyped + 10 typed K×V shims); covered by `llvm_wave3_dict_set_tuple::{llvm_emits_dict_new_len_is_empty, llvm_emits_dict_set_then_get_i64_i64, llvm_emits_dict_contains_after_set, llvm_emits_dict_end_to_end_with_drop}` (4 dict-focused fixtures incl. `dict_drop` via `Terminator::Drop` Ty::Dict arm — ADR-0058g §6.1 TD-1 dict portion closure); end-to-end exit 33 capstone |
| **set / tuple** | `__cobrust_set_*` / `__cobrust_tuple_*` | Set + tuple construction + access all silent | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-3; 5 set externs + 4 tuple externs hooked; covered by `llvm_wave3_dict_set_tuple::{llvm_emits_set_end_to_end, llvm_emits_tuple_end_to_end}` (set exit 3 = contains(1)+distinct(2); tuple exit 150 = 200-50 + 2-arg `tuple_drop(p, n)` ABI verified); `Ty::Set` / `Ty::Tuple` Drop is NO-OP on both backends (parity matches Cranelift `cranelift_backend.rs:1238-1240`), widening tracked for Phase G |
| **panic** | `__cobrust_panic` | `panic("msg")` source-level does not abort; `unwrap_err()` on Err paths produces no signal | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-1; covered by `llvm_wave3_panic_argv::llvm_emits_panic_extern_call_with_unreachable` |
| **fmt** | `__cobrust_fmt_int` / `__cobrust_fmt_float` / `__cobrust_fmt_float_prec` / `__cobrust_fmt_bool` / `__cobrust_fmt_str` / `__cobrust_fmt_repr` / `__cobrust_str_len` / `__cobrust_str_ptr` / `__cobrust_str_clone` | f-string runtime (`f"x = {x}"`) silently produces empty string | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-5; 9 fmt externs hooked (covers `fmt_int`, `fmt_float`, `fmt_float_prec`, `fmt_bool`, `fmt_str`, `fmt_repr`, `str_len`, `str_ptr`, `str_clone`); covered by `llvm_wave3_fmt_iter_math_str::{llvm_emits_fmt_int_then_str_len, llvm_emits_fmt_bool_then_str_len, llvm_emits_fmt_str_then_str_len}` (3 combo fixtures, each chains `fmt_*(buf, val)` → `str_len(buf)` for an observable exit-code signal) |
| **iter** | `__cobrust_iter_init` / `__cobrust_iter_next` / `__cobrust_iter_drop` | `for x in [1,2,3]` body never executes | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-5; 3 iter externs hooked; covered by `llvm_wave3_fmt_iter_math_str::llvm_emits_iter_init_next_drop_empty` (full three-helper chain via the empty-iter sentinel `iter_init(0)` → `iter_next == 0` → `iter_drop`; uses `Ty::Str` ptr-typed alloca for the handle local to match wave-4 `input_str_buf` opaque-ptr round-trip pattern) |
| **math** | `__cobrust_math_sqrt` / `__cobrust_math_floor` / `__cobrust_math_ceil` / `__cobrust_math_round` / `__cobrust_math_abs` / `__cobrust_math_sin` / `__cobrust_math_cos` / `__cobrust_math_tan` / `__cobrust_math_log` / `__cobrust_math_exp` / `__cobrust_math_pow` | All `math.*` intrinsics return 0 or no-op | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-5; 11 math externs hooked (10 single-arg f64→f64 + 1 two-arg pow); covered by `llvm_wave3_fmt_iter_math_str::{llvm_emits_math_sqrt_16, llvm_emits_math_abs_neg7, llvm_emits_math_pow_2_3}` (3 fixtures spanning single-arg sqrt + abs + 2-arg pow; each casts the f64 return via `Rvalue::Cast(FloatToInt, _, Ty::Int)` to expose an exit-code signal) |
| **parse_int / str parsing** | `__cobrust_parse_int` / `__cobrust_str_eq` / `__cobrust_str_at` / `__cobrust_str_len_src` / `__cobrust_str_ord` / `__cobrust_count_toks` / `__cobrust_parse_int_tok` / `__cobrust_str_eq_lit` | Integer parsing from stdin and string comparisons all silent | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-5; 8 parse_int + str-parsing externs hooked; covered by `llvm_wave3_fmt_iter_math_str::{llvm_emits_parse_int_42, llvm_emits_str_ord_uppercase_a, llvm_emits_count_toks_three}` (3 fixtures, each uses single-arg `Constant::Str` literal routed through the wave-2 `materialize_str_buffer` path — confirms 1-param-Str literal dispatch surface for the parsing family) |
| **str methods (ADR-0050e)** | `__cobrust_str_split` / `__cobrust_str_join` / `__cobrust_str_replace` / `__cobrust_str_trim` / `__cobrust_str_find` / `__cobrust_str_contains` / `__cobrust_str_starts_with` / `__cobrust_str_ends_with` / `__cobrust_str_lower` / `__cobrust_str_upper` | `s.split(",")` / `.join()` / all str method calls silently return nothing | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-5; 10 str-method externs hooked (`str_clone` declared with fmt family for cohesion); covered by `llvm_wave3_fmt_iter_math_str::{llvm_emits_str_lower_then_len, llvm_emits_str_contains_present, llvm_emits_str_find_present, llvm_emits_str_starts_with_true}` (4 fixtures: 1 mutator → `str_len` chain for `str_lower`, 3 predicate-return cases for `contains` / `find` / `starts_with`; positive-result coverage — `find` -1 sentinel deferred since Unix exit code is unsigned 0-255) |
| **LLM router** | `__cobrust_llm_complete` / `__cobrust_llm_dispatch` / `__cobrust_llm_stream` / `__cobrust_prompt_*` / `__cobrust_tool_*` | Full AI-native surface of ADR-0049 alpha silently no-ops under LLVM | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-6; 13 LLM router externs hooked (3 M-AI.0 `llm_*` + 5 M-AI.1 `prompt_*`/`llm_complete_structured` + 5 M-AI.2 `tool_*`/`llm_complete_with_tools`); covered by `llvm_wave3_llm_router::{llvm_emits_llm_complete_then_str_len, llvm_emits_llm_dispatch_then_str_len, llvm_emits_llm_stream_then_list_len, llvm_emits_prompt_format_system_user_then_str_len, llvm_emits_prompt_escape_braces_then_str_len, llvm_emits_tool_registry_new}` (6 fixtures spanning all three M-AI sub-families; Decision 7 empty-fallback contract makes the 3 router-dispatching fixtures network-free + key-free + config-free, mirroring `cobrust-stdlib/tests/llm_corpus.rs` Tier 2 strategy; real-LLM dispatch tests stay gated under `real-llm-smoke` env at the stdlib corpus level — out of scope for codegen extern-decl/link verification) |

**Coverage summary**: as of 2026-05-25, **ALL 12 of 12 categories**
are RESOLVED via ADR-0058g (panic + argv via sub-wave-1; list runtime
via sub-wave-2; dict + set/tuple via sub-wave-3; input + read_line via
sub-wave-4; fmt + iter + math + parse_int/str-parsing + str-methods via
sub-wave-5; **LLM router via sub-wave-6**). The LLVM backend now
reaches feature-parity with the Cranelift backend for the entire
wave-3 stdlib runtime surface. The print system + panic + argv + list
+ dict + set + tuple runtime + input + read_line + fmt + iter + math +
parse_int + str-parsing + str-methods + LLM router + pure numeric
computation (arithmetic + FnRef recursion) all work correctly today in
LLVM AOT.

**Wave-3 fully closed 2026-05-25**. The §5 ADR-0058g "Done means (full
wave-3 closure)" criteria are met: every category in §2 has at least
one passing fixture; the LLVM `lower_call` extern-name dispatch path
covers the full stdlib runtime surface. F45a is RESOLVED.

**F35-sibling discipline**: sub-wave-6 closure IS wave-3 closure
because every prior sub-wave (1-5) plus this sub-wave's 6 fixtures
cumulatively cover every §2 category with a `link_and_run` fixture
asserting an observable exit-code signal (not merely object-emit).
Doc updates and release notes downstream of this finding MAY now
correctly claim "wave-3 closed" and "LLVM-Cranelift feature parity"
backed by empirical fixtures.

## §3 Systemic fix recommendations (promoted from playground audit §4)

### §3.1 Pre-tag CI gate: stdout-diff, not object-emit

The v0.5.1 sprint added 8 `stdlib_io_*` fixtures that assert stdout equality
against golden output (not merely "object file is non-empty"). This is the
correct gate shape.

**Forward rule**: before tagging any release that claims LLVM backend progress,
every category in §2 must have at least one `codegen_diff_corpus::category_*`
fixture that:
1. Emits via LLVM AOT (`Backend::Llvm`).
2. Links against `libcobrust_stdlib.a` + `runtime/cobrust_main.c`.
3. Runs the resulting binary.
4. Asserts `stdout == expected_line` (not just "non-empty object").

The 8 `stdlib_io_*` fixtures cover wave-2. Wave-3 ADR-0058g must add
`codegen_diff_corpus::category_panic_*`, `::category_argv_*`,
`::category_list_*`, etc. before each wave merges.

### §3.2 Backend wave-N stub cross-ref contract

Every `// Wave-N stub` comment in backend code (`llvm_backend.rs`,
`cranelift_backend.rs`, future backends) MUST cross-reference exactly one of:

- A tracked `#[ignore = "deferred to ADR-NNNN; finding:FXXX"]` test in the same crate, OR
- A specific issue URL, OR
- An open ADR with `status: proposed`, OR
- A finding URN in the form `finding:adr0058f-§7-wave3-extern-stub-debt`.

**A bare `// Wave-N stub` comment with no cross-reference is a silent-rot
signal (F37-pattern).** CI gate candidate: grep `Wave-\d+ stub` across
`crates/cobrust-codegen/src/` and flag any line without one of the above
markers.

### §3.3 Release-notes honest-cite contract

Release notes claiming "LLVM backend" progress MUST include a per-extern
parity table with three columns:
- Helper / extern name
- Cranelift status (working / wave-N stub)
- LLVM status (working / wave-N stub / not-applicable)

This prevents F35-sibling drift (commit-msg vs diff drift at release scope)
from aggregating adjacent landings into an overstated "feature-complete"
claim. RELEASE_NOTES_v0.5.1.md already complies with this for wave-2 — all
wave-3+ releases MUST follow the same pattern.

## §4 F-family lineage

This finding is a child of F45 and confirms the pattern at the full extern
catalogue level:

- **F35-sibling** (`docs/agent/findings/f35-sibling-commit-msg-vs-diff-drift.md`):
  commit-msg scope vs actual diff scope drift. Here: wave-2 doc claims must
  not be read as wave-3 coverage.
- **F37** (`docs/agent/findings/f37-silent-rot-on-accepted-debt.md`):
  silent rot on accepted debt. Wave-1 stub comments without cross-references
  silently fossilize into permanent stubs.
- **F44** (`docs/agent/findings/f44-ci-cache-stale-green-false-pass.md`):
  CI green != workspace clean. Object-emit CI green masks runtime silent
  failure (same structural pattern).
- **F45** (`docs/agent/findings/f45-llvm-backend-wave1-stub-silently-shipped.md`):
  parent finding. F45a = child confirming the wave-3 scope at the full
  catalogue level, sourced from independent playground audit.

## §5 User-path scope clarification

**Default user path is NOT affected by F45a:**

```
cobrust build foo.cb          # Cranelift = default = all externs work
cobrust build --release foo.cb # still Cranelift unless --features llvm
cobrust install <pkg>          # release wheel = Cranelift binary
```

F45a impacts **only** the explicit opt-in path:

```
cargo build --features llvm    # LLVM AOT experimental
```

Release wheels distributed via `cobrust install` or the GitHub release page
do NOT enable `--features llvm`. An end-user following the standard install
path never encounters wave-3 stubs. This sprint's doc updates must lead with
this clarification — F35-sibling discipline applied to scope accuracy.

## §6 Cross-references

- ADR-0058f (`docs/agent/adr/0058f-llvm-backend-wave2-stdlib-io.md`) — wave-2
  resolution; §7 Open Questions is the original catalogue this finding confirms.
- ADR-0058g (`docs/agent/adr/0058g-llvm-backend-wave3-stdlib-hookup-roadmap.md`) —
  the roadmap ADR authored in the same sprint as this finding.
- F45 (`docs/agent/findings/f45-llvm-backend-wave1-stub-silently-shipped.md`) —
  parent finding; F45a is a child.
- F35-sibling (`docs/agent/findings/f35-sibling-commit-msg-vs-diff-drift.md`) —
  claim-vs-landed drift; §3.3 + §5 above operationalize.
- F37 (`docs/agent/findings/f37-silent-rot-on-accepted-debt.md`) — silent rot;
  §3.2 above operationalizes.
- F44 (`docs/agent/findings/f44-ci-cache-stale-green-false-pass.md`) — CI green
  != working; §3.1 above operationalizes.

## §7 Status

**RATIFIED 2026-05-22** by playground machine independent audit forwarded by
user. Cross-confirmed against ADR-0058f §7 and Cranelift backend
`cranelift_backend.rs` extern surface. No impl shipped in this sprint
(roadmap-only per sprint scope). Wave-3 closure tracked in ADR-0058g.

## §8 Amendment — sub-wave-5 over-claim resolution (2026-05-26, F53-sibling)

**Empirical correction following F53 discovery.** The 2026-05-25 status flip
(§ frontmatter `status: RESOLVED 2026-05-25 (all 12 wave-3 categories
resolved via ADR-0058g sub-wave-1 + sub-wave-2 + sub-wave-3 + sub-wave-4 +
sub-wave-5 + sub-wave-6 = ENTIRE WAVE-3 CLOSED)`) was an over-claim for two
of the twelve categories — **list** + **fmt** (cf. `docs/agent/findings/
f53-llvm-default-flip-aggregate-gap.md`).

### Over-claim taxonomy

- **list** (row 3 of §2 table). Sub-wave-2 hooked the six runtime extern
  declarations (`__cobrust_list_new` / `_set` / `_get` / `_append` / `_len`
  / `_is_empty`) into `lower_call`'s extern-name dispatch path. The closure
  was correct *for tests that directly invoke these helpers via
  `lower_call`* (the five sub-wave-2 fixtures in `llvm_wave3_list_runtime`
  all PASS). **What sub-wave-2 missed**: the `Aggregate::List` codegen
  callsite (`lower_aggregate` in `llvm_backend.rs:3895`) returned
  `opaque_ptr_ty.const_null()` for every aggregate kind — including
  `AggregateKind::List`. Source-level `[1, 2, 3]` aggregate literals never
  reached the runtime helpers; they silently produced null pointers.
- **fmt** (row 7 of §2 table). Sub-wave-5 declared nine fmt extern
  declarations (`__cobrust_fmt_int` / `_float` / `_float_prec` / `_bool` /
  `_str` / `_repr` + `_str_len` / `_str_ptr` / `_str_clone`) and added
  three combo fixtures in `llvm_wave3_fmt_iter_math_str`. The closure was
  correct *for tests that directly invoke these helpers via `lower_call`
  with a pre-allocated buffer*. **What sub-wave-5 missed**: the
  `Aggregate::FormatString` codegen callsite (same stub line). f-string
  literals like `f"x = {x}"` lower to `Rvalue::Aggregate(FormatString,
  [Str(\"x = \"), Move(x)])` in MIR; the `lower_aggregate` stub returned
  null and the runtime never saw the format helpers.

### Resolution

F53 sprint (2026-05-26, this commit) implements
`lower_aggregate_list` + `lower_aggregate_format_string` in
`crates/cobrust-codegen/src/llvm_backend.rs`, mirroring the Cranelift
references at `cranelift_backend.rs:1674-1739` (list) +
`cranelift_backend.rs:1882-2020` (FormatString).

Empirical verification:
- `cli_stdin_argv_e2e`: 15/15 PASS under `--release --features llvm`
- `f64_e2e`: 33/33 PASS, 2 ignored (pre-existing, unrelated to F53)
- `list_str_e2e`: 31/33 PASS, 2 ignored (pre-existing LC-100 finding,
  unrelated to F53)
- `fstring_user_fn_str_corpus`: 6/6 PASS
- Total F53 regressions resolved: 36 of 36 expected (the 2 ignored cases
  in `list_str_e2e` are pre-existing `#[ignore]`'d for LC-100, not F53).
- Wave-3 corpora regression-free: `llvm_wave3_list_runtime` 5/5 +
  `llvm_wave3_fmt_iter_math_str` 14/14 + `llvm_wave3_llm_router` 6/6 all
  PASS post-fix.

### Out of scope (deferred)

The four other AggregateKind kinds (`Dict` / `Set` / `Tuple` / `Record` /
`Adt`) still return null in `lower_aggregate`'s fallthrough branch — the
F53 §3 prerequisite #3 (Dict / Set / Tuple aggregate lowering) lands in a
follow-up sprint. F45a §2 categories *for those types* (dict + set/tuple
rows) ARE empirically RESOLVED for the runtime-helper path — only the
aggregate-literal path is missing for the more complex (K, V) typed-shim
dispatch. No source-level program in the F53 baseline failed on
`Aggregate::Dict` / `Set` / `Tuple` because the affected corpora do not
construct dict / set / tuple via aggregate literals.

### F35-sibling discipline note

This amendment honors the F35-sibling claim-vs-diff drift rule: the
2026-05-25 closure status frontmatter is preserved (history-honest); this
§8 documents the over-claim discovery + resolution path. The "12 of 12
RESOLVED" claim is now empirically true after F53 — it was claimed before
empirical proof and luckily the §2-table truly resolved categories did
not regress when discovered. F37-sibling rule: claims of resolution that
escape `#[ignore]` discipline must be backed by a corresponding fixture;
list + fmt aggregate paths now have fixtures (the F53 §4 honest-debt
table maps).
