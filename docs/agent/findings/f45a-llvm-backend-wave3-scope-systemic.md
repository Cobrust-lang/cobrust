---
name: f45a
status: ratified (panic + argv + list runtime categories resolved 2026-05-25 via ADR-0058g sub-wave-1 + sub-wave-2)
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
| **input** | `__cobrust_input` / `__cobrust_input_str_buf` / `__cobrust_input_no_prompt` / `__cobrust_read_line` | `input("> ")` and `read_line()` silently return nothing; stdin family fully silent under LLVM | wave-1 stub |
| **argv** | `__cobrust_argv` (`__cobrust_capture_argv` is C-shim-only — Cranelift+LLVM both intentionally omit at MIR level) | `sys.argv` evaluates to NULL / empty; command-line programs cannot read arguments | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-1; covered by `llvm_wave3_panic_argv::llvm_emits_argv_extern_call_and_exits_zero` |
| **list** | `__cobrust_list_new` / `__cobrust_list_set` / `__cobrust_list_get` / `__cobrust_list_append` / `__cobrust_list_len` / `__cobrust_list_is_empty` | All list operations silently no-op; list-based programs produce no output | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-2; covered by `llvm_wave3_list_runtime::{llvm_emits_list_new_extern_call, llvm_emits_list_append_then_len, llvm_emits_list_set_then_get, llvm_emits_list_is_empty_after_new, llvm_emits_list_end_to_end_roundtrip}` (5 fixtures, 6 helpers, end-to-end exit 243 capstone) |
| **dict** | `__cobrust_dict_new` / `__cobrust_dict_set_*` / `__cobrust_dict_get_*` / `__cobrust_dict_contains_*` | Dict runtime fully silent; key/value stores return nothing | wave-1 stub |
| **set / tuple** | `__cobrust_set_*` / `__cobrust_tuple_*` | Set + tuple construction + access all silent | wave-1 stub |
| **panic** | `__cobrust_panic` | `panic("msg")` source-level does not abort; `unwrap_err()` on Err paths produces no signal | **RESOLVED 2026-05-25** via ADR-0058g sub-wave-1; covered by `llvm_wave3_panic_argv::llvm_emits_panic_extern_call_with_unreachable` |
| **fmt** | `__cobrust_fmt_*` family | f-string runtime (`f"x = {x}"`) silently produces empty string | wave-1 stub |
| **iter** | `__cobrust_iter_init` / `__cobrust_iter_next` / `__cobrust_iter_drop` | `for x in [1,2,3]` body never executes | wave-1 stub |
| **math** | `__cobrust_math_sqrt` / `__cobrust_math_floor` / `__cobrust_math_ceil` / `__cobrust_math_round` / `__cobrust_math_abs` / `__cobrust_math_sin` / `__cobrust_math_cos` / `__cobrust_math_tan` / `__cobrust_math_log` / `__cobrust_math_exp` / `__cobrust_math_pow` | All `math.*` intrinsics return 0 or no-op | wave-1 stub |
| **parse_int / str parsing** | `__cobrust_parse_int` / `__cobrust_str_eq` / `__cobrust_str_at` / `__cobrust_str_len_src` / `__cobrust_str_ord` / `__cobrust_count_toks` / `__cobrust_parse_int_tok` / `__cobrust_str_eq_lit` | Integer parsing from stdin and string comparisons all silent | wave-1 stub |
| **str methods (ADR-0050e)** | `__cobrust_str_split` / `__cobrust_str_join` / `__cobrust_str_replace` / `__cobrust_str_trim` / `__cobrust_str_find` / `__cobrust_str_contains` / `__cobrust_str_starts_with` / `__cobrust_str_ends_with` / `__cobrust_str_lower` / `__cobrust_str_upper` / `__cobrust_str_clone` | `s.split(",")` / `.join()` / all str method calls silently return nothing | wave-1 stub |
| **LLM router** | `__cobrust_llm_complete` / `__cobrust_llm_dispatch` / `__cobrust_llm_stream` / `__cobrust_prompt_*` / `__cobrust_tool_*` | Full AI-native surface of ADR-0049 alpha silently no-ops under LLVM | wave-1 stub |

**Coverage summary**: as of 2026-05-25, **3 of 12 categories** (panic +
argv via sub-wave-1; list runtime via sub-wave-2) are resolved by
ADR-0058g. The remaining 9 categories (dict / set+tuple / input / fmt /
iter / math / parse_int+str-parsing / str-methods / LLM router) continue
to silently misbehave under `--features llvm` until subsequent waves
land. The print system + panic + argv + list runtime + pure numeric
computation (arithmetic + FnRef recursion) are the surfaces that work
correctly today in LLVM AOT.

**F35-sibling discipline**: sub-wave-2 closure is NOT wave-3 closure.
The §5 ADR-0058g "Done means (full wave-3 closure)" criteria still
require every category in §2 to have a passing fixture; 9 categories
remain. Doc updates and release notes downstream of this finding MUST
distinguish "panic + argv + list landed" from "wave-3 closed". The
list runtime closure unblocks LC-100 corpus list-based programs but
leaves dict / set / tuple / iter / fmt / math / parse / str-methods /
LLM router still no-op under LLVM AOT.

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
