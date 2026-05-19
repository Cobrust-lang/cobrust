---
doc_kind: adr
adr_id: 0059
parent_adr: 0054
title: "Phase L frame — Debugger UX (lldb pretty-printers + DAP + `cobrust debug` CLI)"
status: accepted
date: 2026-05-19
last_verified_commit: 3c9382c
supersedes: []
superseded_by: []
relates_to: [adr:0054, adr:0058, adr:0058c, adr:0057, adr:0056b]
discovered_by: P9 Phase L frame-author dispatch post-ADR-0058c ratification (§13 RESOLVED unblocks Phase L)
ratification_path: P9 frame-ADR review; ratifies on first sub-ADR (0059a) impl merge
---

# ADR-0059: Phase L frame — Debugger UX

## 1. Motivation

Per ADR-0019 §"Out of scope for Phase E (defer to Phase F)": "Debugger
(`cobrust debug`)" was deferred at the M8..M14 roadmap; ADR-0054 §6.5
re-slotted it as **Phase L** in the post-Phase-G ordering with rank-5
§2.5 ROI ("~0, human-facing"). ADR-0058 §13 binds the Phase K × L
handoff: **ADR-0058c's DWARF emission is the prerequisite gate**;
without it `lldb` / `gdb` / VS Code DAP cannot resolve Cobrust source
lines to PCs.

ADR-0058c ratified at `a46fe85` (2026-05-19): DWARF v5 emission shipped
(per-function `DISubprogram` + per-`Span` `DILocation` + `DIBuilder::finalize`),
and `tests/dwarf_lldb_smoke.rs` proves the 4-fixture end-to-end loop
(`lldb-18` resolves Cobrust symbols + line table is non-empty). ADR-0058
§13 status: **RESOLVED**. Phase L frame-author + dispatch unblocked.

This ADR frames the Phase L work as a 3-wave roster targeting end-user
debugger UX: step / breakpoint / variable-inspect for Cobrust programs
in the editor agents Cobrust users actually run (Cursor / VSCode + the
`cobrust debug` CLI for terminal-driven workflows). CLAUDE.md §1
audience binding ("Python ergonomics with Rust safety") makes the
debugger the visible product surface — the gap between "DWARF emits"
and "user steps through their code in Cursor" is what Phase L closes.

Constitutional anchors:

- **CLAUDE.md §1** (HEAD `3c9382c`) — Cobrust pitch "Python's ergonomics, Rust's safety";
  debugger UX is the visible "Python ergonomics" surface for a static language.
- **CLAUDE.md §2.5** — LLM-first design principle. Phase L §2.5 audit per §2 below.
- **ADR-0054 §6.5** — Phase L slot in post-G roadmap; rank-5 §2.5 ROI.
- **ADR-0058 §13** — Phase K × L handoff binding gate (RESOLVED at `a46fe85`).
- **ADR-0058c §4** — explicit "variable inspection deferred to Phase L UX, not Phase K codegen".

## 2. §2.5 LLM-first design audit

Phase L is **§2.5-low** but not §2.5-zero. ADR-0054 §2 ranks Phase L
rank-5 ("~0, human-facing"). This ADR's §2.5 audit refines the score:

| §2.5 axis | Phase L impact | Rationale |
|---|---|---|
| §A compile-time-catch-errors | Neutral | Debugger surfaces runtime state; type/borrow checks happen earlier. |
| §B training-data-overlap | **Positive** | `lldb` pretty-printer output (e.g. `xs: List<Int> = [1, 2, 3]`) is the most-trained-on shape in modern debugger transcripts. LLM agents reading lldb output match this prior. |
| §A.1 line-mapping = compile-time-catch via DWARF | Neutral (inherited from 0058c) | DWARF lives at codegen; ADR-0058c shipped the catch. Phase L consumes. |

The §2.5 §B positive comes from **pretty-printer output being
LLM-consumable**. When a debug session's `frame variable` prints
`d: Dict<Int, Str> = {1: "a", 2: "b"}` instead of
`d: __cobrust_dict_t = { ptr: 0x..., len: 2, capacity: 4, ... }`, the
agent reading the lldb log understands the runtime state by inspection;
without pretty-printers it'd need a separate parsing pass. This is a
modest §2.5 §B win, not the §B amplifier Phase J's LSP delivers.

**Net**: Phase L stays rank-5 in ADR-0054's ROI ordering, but the wave-1
pretty-printer work delivers a non-trivial §2.5 §B payoff that Phase K
DWARF emission alone does not.

## 3. Scope — 3-wave sub-ADR roster

Three sub-ADRs land under the Phase L frame. Sequential dispatch:
wave-1 ratifies a foundation that wave-2 and wave-3 build on; wave-2 +
wave-3 may overlap if dispatch capacity permits, but the load-bearing
DWARF + pretty-printer surface lives in wave-1.

### 3.1 Wave-1 — ADR-0059a — lldb pretty-printers

**Scope**: Python lldb scripts (`tools/lldb-cobrust/printers.py`) that
translate raw Cobrust struct internals to Cobrust source-level
appearance. `Str` shows actual UTF-8 string content (not raw `{ptr,
len, cap}` struct). `List<T>` shows `[a, b, c]` with elements
recursively printed. `Dict<K, V>` shows `{k: v, ...}` in insertion
order. `Tuple` shows `(t1, t2, ...)`. `Set<T>` shows `{t1, t2, ...}`.
`Option<T>` shows `None` or `Some(t)`.

**Why wave-1**: variable inspection without pretty-printers shows raw
struct DI; ADR-0058c §4 explicitly deferred this to Phase L UX. The
deferral is closed by pretty-printers, not by extending DI emission.
Pretty-printers are Python-side; the Cobrust codegen surface is
untouched.

**Owner**: ADR-0059a.

### 3.2 Wave-2 — ADR-0059b — DAP (Debug Adapter Protocol) server crate

**Scope**: new `crates/cobrust-dap/` workspace crate (binary + library)
implementing the Debug Adapter Protocol (DAP). stdio transport mirrors
`cobrust-lsp` crate shape (ADR-0057): JSON-RPC over stdin/stdout, async
via `tokio`. The DAP server delegates to `lldb-18` under the hood
(launches `lldb` as a child process, marshals DAP messages to lldb's
command-line API) — bind-the-core per ADR-0012.

VSCode / Cursor "Run > Start Debugging" → DAP handshake → `launch`
request → `cobrust-dap` invokes `cobrust build --debug` → spawns `lldb`
on the resulting binary → DAP `stackTrace` / `scopes` / `variables`
requests delegate to lldb command output → pretty-printers from
wave-1 already loaded via `command script import`.

**Why wave-2**: Cursor + VSCode users live in DAP, not raw lldb.
Without a DAP server they cannot debug Cobrust programs in their editor.

**Owner**: ADR-0059b.

### 3.3 Wave-3 — ADR-0059c — `cobrust debug` CLI subcommand

**Scope**: extend `crates/cobrust-cli/src/main.rs` subcommand registry
with `cobrust debug <file.cb>`. Implementation:

- Build the file (`cobrust build --debug <file.cb>`, equivalent to
  `OptLevel::None` + DWARF on).
- Auto-load the wave-1 pretty-printers script via
  `command script import tools/lldb-cobrust/printers.py` in a
  user-provided or temp-generated `.lldbrc`.
- Launch `lldb-18 <binary>` with the rc file.

**Why wave-3**: terminal-driven workflows (CI-attached debug,
ssh-into-<self-hosted-runner> debug) need a one-command entry. Without it
users hand-compose `cobrust build --debug; lldb out.bin; command script
import ...` every time.

**Owner**: ADR-0059c.

**Status**: CLOSED (2026-05-19) at ADR-0059c acceptance. Shipped
`crates/cobrust-cli/src/debug.rs` (~280 LOC) with `DebugArgs` +
`run()` + closed `DebugError` enum; 3-mode dispatch (interactive /
`--dap` stdio / `--bp` shorthand); ZERO new Cargo deps per
HARD-BANNED #1 (reuses `tempfile`, `clap`, `thiserror`,
`std::process::Command`). 3 integration tests PASS DG
(`cargo test -p cobrust-cli --test debug_subcommand`: 2 PASS + 1
ignored DAP-handshake gated per ADR-0059b §6.2 precedent + 0 failed
+ 0 regression).

## 4. Non-goals (wave-1 scope; later waves may revisit)

Explicitly out of Phase L wave-1:

- **Source-level expression evaluation** (debugger "watch" window —
  e.g. `frame variable --show-types arr[i] + 1`). Requires a Cobrust
  source-level interpreter inside lldb. Future ADR if demand surfaces.
- **Time-travel debugging** (reverse-step, record-replay). Out of scope
  for Phase L entirely; would require `rr` integration or a custom
  record/replay layer.
- **REPL inside debugger** (a `cobrust>>>` prompt during a stopped
  frame). Phase L is read-only inspection + step/breakpoint; an
  interactive REPL mid-debug would re-open the Phase I REPL JIT scope
  (ADR-0056) inside debugger context — separate ADR if needed.
- **Conditional breakpoints with Cobrust syntax** (`break --condition
  'i > 10'` where `i > 10` parses as Cobrust). lldb's existing
  `--condition 'i > 10'` accepts C-style expressions; Cobrust integers
  + comparisons happen to coincide with C, so simple cases work
  passthrough. Source-level Cobrust expressions in conditions are
  out-of-scope.
- **Variable rewrite from the debugger** (mutating `i = 99` mid-step).
  Cobrust's ownership model makes this semantically fraught — what
  about borrow obligations on the rewritten value? Out of scope.

Phase L wave-1 ships **read-only inspection + step/breakpoint** as the
shipped surface. The non-goals above are documented to bound Phase L
author scope and prevent scope drift at sub-ADR dispatch.

## 5. Risk register

Three concrete risks tracked for Phase L dispatch.

### 5.1 lldb Python API version churn

- **Risk**: lldb's Python API (`lldb` module inside the hosted Python)
  changes signatures across LLVM major versions. The
  `SBValue.GetSummary()` / `SBSyntheticValueProvider` interfaces
  drifted between LLVM-17 → LLVM-18; future LLVM-19 may drift further.
  Wave-1 pretty-printers written against LLVM-18 may need adjustment
  per the LLVM-version pin in ADR-0058 §4.
- **Mitigation**: lldb's Python API is more stable than the C++ API
  (Python is a hosted layer). Wave-1 sticks to the most stable subset:
  `SBValue.GetChildAtIndex`, `SBValue.GetData`, `SBData.ReadRawData`,
  `SBSyntheticValueProvider.{num_children, get_child_at_index,
  update}`. If LLVM-19 churns these, version-detect at script load +
  branch to per-version code paths.

### 5.2 macOS dSYM packaging

- **Risk**: on Mach-O (macOS), DWARF goes into `.dSYM` directories via
  post-link `dsymutil`. Wave-1 pretty-printers consume DWARF from the
  binary; if the binary is stripped (`strip --strip-debug`), DWARF
  lives only in the `.dSYM`. lldb auto-discovers `.dSYM` alongside the
  binary by convention, but `cobrust build --debug` must NOT strip + must
  emit + place the `.dSYM` next to the binary.
- **Mitigation**: ADR-0058c §4 explicitly notes `dsymutil` packaging
  is handled by `release.yml`, not the LLVM backend. `cobrust build
  --debug` invokes `dsymutil` per ADR-0046 release-flow; wave-3 CLI
  driver mirrors this. Wave-1 pretty-printers don't deal with .dSYM
  directly — lldb auto-loads it.

### 5.3 Linux gdb compatibility (lldb is primary; gdb users exist)

- **Risk**: Phase L wave-1 ships lldb pretty-printers (Python).
  gdb users have a parallel-but-incompatible pretty-printer system
  (also Python, but different SDK shape). Linux dev workflows split
  ~70% gdb / ~30% lldb in 2026 distros; ignoring gdb constrains the
  Linux user surface.
- **Mitigation**: wave-1 scope is **lldb-only** (primary). gdb
  pretty-printers are a Phase L+ followup if Linux gdb demand
  surfaces; the underlying DWARF is the same, so a gdb script writing
  the same display logic is a port (~1-2 days), not a redesign.
  ADR-0058c lldb-smoke test corpus is lldb-only too; gdb-smoke is
  parallel work, not Phase L wave-1 blocker.

## 6. §2.5-aligned implementation hint

Pretty-printers live in **Python**, hosted by lldb's embedded
interpreter. Wave-1 ships a single `tools/lldb-cobrust/printers.py`
with ~300-500 LOC across 6 type providers. Loaded via either:

- Project-scoped `~/.lldbinit-cobrust` snippet that the user adds to
  `~/.lldbinit` (one-line `command source ~/.lldbinit-cobrust`), or
- Per-fixture `.lldbrc` files for the smoke test harness (extends
  `dwarf_lldb_smoke.rs` Day-3 fixtures with `command script import`
  prefix), or
- `cobrust debug` CLI subcommand (wave-3) auto-loads via temp `.lldbrc`.

The `command script import` API is lldb-Python's stable v1 surface
(unchanged since LLVM-12). Pretty-printers attach to type-name patterns
via `lldb.formatters.Logger` + `type summary add --python-function`.

§2.5 §B (training-data-overlap) compliance audit: pretty-printer output
matches Python `repr` shape (`[1, 2, 3]` not `Vec<i64> [1, 2, 3]`), not
Rust `Debug` shape. The LLM consuming lldb output recognises Python
`repr` from billions of training tokens; Rust `Debug` is
also-recognised but less canonical for debugger output specifically.
Wave-1 deliberately mirrors Python `repr` — see ADR-0059a §4 non-goals
("NO source-level type-name printing Rust-style; use Cobrust source
syntax").

## 7. Acceptance gate

Phase L frame is **proposed** at this commit. Frame ratifies on ADR-0059a
(wave-1) impl merge. Wave-1 acceptance gate per ADR-0059a §6:

- `lldb-18 examples/fib.cb-binary` interactive session.
- `breakpoint set --file fib.cb --line N` resolves (DWARF lines work
  per ADR-0058c §3.3).
- `frame variable` output:
  - `n: Int = 10` (not `n: i64 = 10`, not raw alloca address).
  - `result: List<Int> = [1, 1, 2, 3, 5]` (not
    `result: __cobrust_list_t = { ptr: 0x..., len: 5, ... }`).
- `dwarf_lldb_smoke.rs` extended with 3 new smoke fixtures (Str / List
  / Dict variable inspection) — 4 existing fixtures still PASS (no
  DWARF regression).

Wave-2 + wave-3 acceptance gates are filed by their respective
sub-ADRs at dispatch eve.

## 8. Wave plan — sequential, wave-2 + wave-3 may overlap

```
ADR-0059 (this frame, proposed)
       │
       ▼
ADR-0059a (wave-1 lldb pretty-printers, ~500 LOC Python + ~50 LOC test wiring)
       │
       │ blocks on: §6 dispatch of 6-type printer surface; wave-2/wave-3
       │            consume the printer scripts as `command script import`
       │            target.
       ▼
   ┌───────────────────────┬───────────────────────┐
   ▼                       ▼                       │
ADR-0059b (DAP server)    ADR-0059c (CLI)         │ (may overlap;
~1.5w wall                ~3-day wall              │  no file-path
                                                   │  collision)
                                                   ▼
                                              Phase L close
```

Wave-1 is the only sequential blocker. Wave-2 (DAP) lives in a new
crate `crates/cobrust-dap/`; wave-3 (CLI) touches `crates/cobrust-cli/`.
File-path disjoint, dispatch in parallel after wave-1 ratifies.

## 9. Sub-ADR roster

Three ADRs land under the Phase L frame:

- **ADR-0059** (this ADR) — Phase L frame. 3-wave roster, ROI position,
  6-type pretty-printer scope binding, risk register.
- **ADR-0059a** — wave-1 lldb pretty-printers (~3-4 days wall).
  Implements §3.1; ships `tools/lldb-cobrust/printers.py` + extends
  `dwarf_lldb_smoke.rs` test corpus.
- **ADR-0059b** — wave-2 DAP server (~1.5 weeks wall). Implements §3.2;
  new `crates/cobrust-dap/` workspace crate.
- **ADR-0059c** — wave-3 `cobrust debug` CLI (~3 days wall). Implements
  §3.3; extends `cobrust-cli` subcommand registry.

Total: 3 sub-ADRs after the frame. ADR-0054 §6.5 forecast a Phase L
"~1w" estimate; the 3-wave roster expands this to ~2-3 weeks wall to
deliver the full DAP + CLI surface beyond bare pretty-printers.

## 10. Pre-dispatch acceptance gate

Phase L wave-1 (ADR-0059a) dispatches only when:

- **ADR-0058c accepted**: DWARF v5 emission shipped (verified at
  `a46fe85` ✓ — wave-3 LlvmEmitter + 4-fixture lldb smoke).
- **Frame ADR-0059 ratified**: this ADR moves to `accepted` on wave-1
  impl merge (frame-ADRs ratify on first sub-sprint dispatch per
  ADR-0058 / ADR-0057 precedent).
- **lldb-18 available on dispatch host**: Mac dev machines via
  `brew install llvm@18`; <self-hosted-runner> via `llvm.sh` (already
  installed for ADR-0058c smoke). Verified at dispatch eve.
- **Phase J non-blocking**: LSP server (ADR-0057) does not gate Phase L
  — they touch disjoint editor surfaces (LSP = diagnostics; DAP =
  debugger). Phase L wave-2 (DAP) builds on a separate stdio transport
  pattern Phase J's LSP crate establishes; mirror-don't-share.

## 11. Compression-ratio note

ADR-0054 §6.5 budgets Phase L at ~1w wall. The 3-wave roster expands
to ~2-3w wall:

- **Wave-1** (~3-4 days): Python lldb scripts are external-ecosystem-
  bound (lldb Python API docs + version-detect branching). Compression
  ~3-4x (lower than self-contained Rust phases).
- **Wave-2** (~1.5w): new crate boilerplate + DAP protocol learning
  curve + lldb child-process marshalling. Compression ~3x
  (external-system-bound).
- **Wave-3** (~3 days): pure Rust CLI extension. Compression ~5x
  (self-contained Rust work).

Risk: if wave-1 pretty-printer churn (Risk 5.1) demands per-LLVM-
version branching, wave-1 slips to ~5 days. Buffer +1 day before
triggering wave-2 dispatch slip.

## 12. Consequences

### 12.1 Positive

- ADR-0019 "Out of scope for Phase E" debugger deferral RESOLVED.
- ADR-0058c §4 variable-inspection deferral RESOLVED at wave-1.
- Cursor / VSCode "Run > Start Debugging" works end-to-end after wave-2.
- Terminal-driven `cobrust debug fib.cb` one-command entry after wave-3.
- §2.5 §B modest positive: pretty-printer output is LLM-consumable.
- Bind-the-core (ADR-0012): lldb + DAP + DWARF are externally
  maintained; Cobrust contributes ~500 LOC Python + ~1000 LOC Rust DAP
  wrapper + ~50 LOC CLI driver — not a debugger reimplementation.

### 12.2 Negative

- ~2-3 weeks wall agent-velocity (vs ADR-0054 §6.5 forecast of ~1w).
- Python lldb-script maintenance burden (per Risk 5.1) carries forward;
  every LLVM major upgrade may need wave-1 adjustments.
- gdb users (~70% of Linux dev) get no first-class support in Phase L;
  Phase L+ followup if demand surfaces.
- DAP wave-2 adds a child-process `lldb` invocation per debug session;
  startup latency dominated by lldb cold start (~200-500ms on cold
  cache). Not the LLM-amplifier surface — acceptable per §2.5-low rank.

### 12.3 Neutral

- The `tools/lldb-cobrust/` directory is a new top-level subtree;
  CLAUDE.md §3 doc-coverage applies (zh/en/agent doc entries land in
  same atomic commit as wave-1 impl).
- Wave-2 + wave-3 may overlap (§8 wave plan) — capacity-permitting
  parallel dispatch.
- Future Phase L+: gdb pretty-printers; conditional Cobrust-syntax
  breakpoints; debugger-side REPL; macOS `.dSYM` post-link automation
  in `cobrust build --debug` itself (vs current `release.yml` flow).

## 13. Dispatch readiness — TEST / DEV hours, ~2-3 weeks total wall

- **ADR-0059a** (lldb pretty-printers, wave-1): ~3-4 days wall.
  TEST ~6h (3 new smoke fixtures + corpus-extension wiring);
  DEV ~10h (~500 LOC Python printers + ~50 LOC Rust test wiring).
- **ADR-0059b** (DAP server, wave-2): ~1.5 weeks wall.
  TEST ~12h (DAP protocol unit tests + VSCode launch smoke);
  DEV ~30h (~1000 LOC Rust DAP wrapper + child-process lldb marshal).
- **ADR-0059c** (`cobrust debug` CLI, wave-3): ~3 days wall.
  TEST ~4h (subcommand integration test);
  DEV ~6h (subcommand registry extension + temp .lldbrc generation).
- **Frame ratify** (this ADR): ~1 day; ratifies on wave-1 dispatch.
- **Buffer**: +1 day for §5.1 lldb Python API churn risk.

**Total**: ~2-3 weeks wall agent-velocity. Wave-2 + wave-3 may overlap
per §8; sequential lower-bound is ~3 weeks; parallel-capable
lower-bound is ~2 weeks.

## 14. Why this ADR now

- **ADR-0058c ratified at `a46fe85`**: DWARF emission shipped; §13
  "Phase K × L handoff" status RESOLVED. Phase L unblock.
- **User directive 2026-05-19**: "P9 for Phase L debugger UX frame
  design — author ADR-0059 + ADR-0059a wave-1 spec, NO impl yet,
  design only."
- **Framing-the-3-wave-roster ex-ante prevents scope drift**: codifying
  wave-1 (printers) + wave-2 (DAP) + wave-3 (CLI) at frame-author time
  binds the Phase L author's scope before sub-ADR dispatch. Without
  this frame, individual sub-ADRs could each claim "the visible
  debugger surface" and conflict.
- **ADR-0058c §4 explicitly defers variable inspection here**:
  framing the close ex-ante codifies the boundary the Phase K author
  set.

— P9 Tech Lead, 2026-05-19
