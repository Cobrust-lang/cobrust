---
finding_id: f76
title: "Strategy/ADR/example docs claimed dora inputs/outputs metadata is 'dropped' / 'must become load-bearing' — the multi-IO sprint had ALREADY threaded it"
status: candidate
date: 2026-06-01
severity: medium
surface: docs/agent/strategy/dora-real-integration-plan.md, docs/agent/adr/0076c-dora-arrow-payload-surface.md, examples/dora_hello/main.cb, crates/cobrust-hir/src/lower.rs, crates/cobrust-cli/tests/dora_multi_io_e2e.rs
introduced_commit: 8020f22 (TDD-origin narration); pre-ADR-0076-Phase-2 docs
detected_commit: HEAD
siblings: [f37-silent-rot-on-accepted-debt, f36-fixture-name-vs-behavior-drift, f35-sibling-commit-msg-vs-diff-drift, f44-ci-cache-stale-green-false-pass]
rule_refs: [CLAUDE.md §5.2, "verify the gap before dispatching"]
---

# F76 — A strategy doc's "future-work X must become load-bearing" is a CLAIM about the present code; VERIFY it against the code before dispatching X

## One-line

Four docs (one strategy plan, one ADR, one `.cb` example header, one TDD-origin
test header) + one stale code comment all asserted the dora
`@dora.node(inputs=[...], outputs=[...])` metadata was **"dropped at HIR"** and
**"must become load-bearing"** — when the multi-IO sprint had **already threaded
it** (load-bearing via `declare_input`/`declare_output`, proven GREEN by the LIVE
`dora_multi_io_e2e` multi-input test). The stale claim nearly triggered a wasted
"make the metadata load-bearing" re-implementation sprint; a read-only research
pass caught it BEFORE any dispatch.

## What the docs CLAIMED (stale)

- `dora-real-integration-plan.md` table row (~L123): "metadata validated then
  **dropped**".
- `dora-real-integration-plan.md` §2.3 item 3 (~L177-181): "`inputs=`/`outputs=`
  metadata is **dropped at HIR** (F68 outcome) — validated as list-of-str
  literals then discarded ... The metadata **must become load-bearing** in the
  real path."
- `0076c-dora-arrow-payload-surface.md` (~L285-287): "the **dropped**
  `inputs/outputs` metadata threading (the one real compiler increment)".
- `0076c-dora-arrow-payload-surface.md` §4.2 (~L400-406): "thread the
  **F68-dropped** `@dora.node(inputs/outputs)` metadata into the manifest so the
  real loop dispatches on `id.as_str()` per declared port".
- `examples/dora_hello/main.cb` header (~L18-21): "validated as list-of-str
  literals at the desugar layer, then **dropped**".
- `crates/cobrust-hir/src/lower.rs` comment (~L2260-2268): "validated here as
  list-of-str literals, then **DROPPED** at synthesis ... the metadata wires the
  real dataflow graph in Phase 2".
- `dora_multi_io_e2e.rs` header (~L1-30): "It is **RED at HEAD `8020f22`** ...
  the decorator ... VALIDATES the `inputs=`/`outputs=` lists ... then **DROPS**
  them ... the IO metadata never reaches the runtime."

## What the code ACTUALLY does (verified by reading it at HEAD)

Trace HIR desugar → C-ABI runtime → live test:

1. **`build_eco_module_register_calls` (`lower.rs` ~L2436)** — the
   `@dora.node(inputs=[...], outputs=[...])` desugar EMITS one
   `dora.declare_input("<id>")` / `dora.declare_output("<id>")` HIR register-call
   PER port id, in source order, at `main`'s prologue. The metadata is NOT
   dropped — it is THREADED into these declare-calls. (The function's own
   docstring ~L2360-2371 already correctly describes this; only the upstream
   comment block ~L2260 lagged.)
2. **`__cobrust_dora_declare_input`/`_declare_output` shims (`cabi.rs` ~L382 /
   ~L403; the module-doc at ~L108-122 describes them)** — push each id onto the process-global `DECLARED_INPUTS` /
   `DECLARED_OUTPUTS`. When synthetic `__cobrust_dora_node_run` sees a NON-EMPTY
   `DECLARED_INPUTS`, it injects ONE canned event PER declared input id (each
   `event.id()` returns its id) — the metadata is **load-bearing** for synthetic
   multi-input dispatch. `send_output` validates the id against
   `DECLARED_OUTPUTS` at RUNTIME (an undeclared id ⇒ `eprintln` + `-1`).
3. **`dora_multi_io_e2e.rs::test_e2e_dora_multi_input_dispatch_and_send_output`
   (~L189)** — LIVE (NOT `#[ignore]`) and GREEN at current HEAD. It proves
   2-input dispatch + `send_output` in DEFAULT CI. The header's "RED at HEAD
   `8020f22`" is TDD-origin narration (the test was written red-first at the
   pre-impl commit `8020f22`, then the sprint landed it green) — NOT the current
   state.

`grep` for `DoraUnknownOutputId` / `UnknownOutputId` across
`crates/cobrust-types/src` + `crates/cobrust-hir/src` returns **zero hits** — so
the only thing genuinely unbuilt is the COMPILE-TIME output-id check, not the
metadata threading.

## Why the conclusion was wrong even where a sub-claim was right

One sub-claim WAS correct: the synthesised `dora.node(handler)` call itself still
takes ONLY the `EcoParam::Callback` slot (no `inputs`/`outputs` args). The error
was the inference **"therefore the metadata is dropped"** — the metadata reaches
the runtime via the SEPARATE `declare_input`/`declare_output` register-calls, not
via the `dora.node` arg list. A locally-true observation ("the node() call is
single-arg") masked a globally-false conclusion ("the metadata is dropped") — the
same shape as F74 (a plausible mechanism masking a wrong one) and F36
(fixture-name vs behaviour).

Worse, the strategy doc **contradicted itself**: §2.2 already correctly described
the desugar EMITTING the `declare_input`/`declare_output` shims that populate
`DECLARED_INPUTS`/`DECLARED_OUTPUTS`, while §2.3 three paragraphs later said the
metadata "is dropped ... must become load-bearing". The shipped-code reality had
overtaken §2.3 without §2.3 being updated — classic doc-rot drift behind a
shipped sprint.

## Why it nearly cost a sprint

"The metadata must become **load-bearing**" / "the **one real compiler
increment**" reads as a crisp, ready-to-dispatch unit of work. Dispatching it
would have re-implemented `declare_input`/`declare_output` threading that already
exists and is already covered by a green test — pure waste, plus the risk of a
second divergent code path. The only reason it did not ship is that a read-only
research pass read `lower.rs` + `cabi.rs` + the test FIRST and found the gap
already closed.

## Lesson (the rule)

A strategy/ADR doc sentence of the form **"future-work X must become Y"** /
**"X is the one real remaining increment"** is a **CLAIM about the present state
of the code**, not a fact. Before dispatching X:

1. **VERIFY the gap against the code** — read the function the doc names (here
   `build_eco_module_register_calls`) and the runtime it feeds (here `cabi.rs`),
   and check for a test that already covers the behaviour (here the LIVE
   `dora_multi_io_e2e`). Strategy docs drift behind shipped sprints; the code +
   the test suite are ground truth.
2. **Distrust a doc that contradicts itself** — when §2.2 says "emits shims" and
   §2.3 says "dropped", the newer/lower section is usually the stale one; read
   the code to break the tie.
3. **Mark TDD-origin narration as such** — a test header written red-first
   ("RED at HEAD `<pre-impl-sha>`") must carry a leading STATUS note once it
   lands green, or future readers mistake the red-baseline description for the
   current state (this header confused exactly that reader).

This is the direct sibling of **F37** (silent-rot on accepted-debt: a claim of
"deferred" not matched by an `#[ignore]`) and **F36** (fixture name vs
behaviour). The unifying CLAUDE.md §5.2 thread: a claim must be verified against
the artifact it describes, not inferred from a doc's framing.

## Genuinely-remaining dora increments (for the record)

1. **Compile-time `send_output` output-id check** — `TypeError::DoraUnknownOutputId
   { id, declared, suggestion }` (ADR-0076 §6 Phase-2 done-means 2). CONFIRMED
   ABSENT (grep empty); validation is runtime-only today (`eprintln` + `-1`).
   This — NOT metadata threading — is the real Phase-B compiler increment.
2. **Arrow ↔ `coil.Buffer` typed-payload bridge** (ADR-0076c) — the orthogonal,
   higher-value UNBUILT data increment (typed numeric arrays over the wire vs
   today's UTF-8-string-only payload).

Both are genuinely unbuilt; neither is "thread the metadata" (done).

## Resolution

1. **Docs corrected (this finding):** all 5 stale locations rewritten to state
   the metadata is threaded into `declare_input`/`declare_output` and is ALREADY
   load-bearing (proven by `dora_multi_io_e2e.rs`); the strategy §2.3 divergence
   marked RESOLVED; the ADR re-scoped to name the compile-time
   `DoraUnknownOutputId` check as the one remaining real-path compiler increment;
   the test header given a leading "STATUS: LANDED — GREEN" note over the
   TDD-origin narration. The `lower.rs` + `main.cb` edits are COMMENT-ONLY (no
   code statement changed; `cargo build -p cobrust-hir` still passes).
2. **Recurrence prevention:** before dispatching any doc-stated "must become" /
   "one real increment" work, a read-only verify-the-gap pass reads the named
   function + runtime + any covering test and confirms the gap is still open.

## Evidence (file:line, HEAD)

- `crates/cobrust-hir/src/lower.rs` ~L2436 `build_eco_module_register_calls` —
  emits one `dora.declare_input`/`declare_output` call per port id.
- `crates/cobrust-dora/src/cabi.rs` ~L382/~L403 (shim fns; module-doc at ~L108-122) — declare shims push to
  `DECLARED_INPUTS`/`DECLARED_OUTPUTS`; synthetic `node_run` replays one event
  per declared input; `send_output` validates against `DECLARED_OUTPUTS`.
- `crates/cobrust-cli/tests/dora_multi_io_e2e.rs` ~L189
  `test_e2e_dora_multi_input_dispatch_and_send_output` — LIVE (not `#[ignore]`),
  GREEN at HEAD; proves 2-input dispatch + `send_output`.
- `grep -rn "DoraUnknownOutputId\|UnknownOutputId" crates/cobrust-types/src
  crates/cobrust-hir/src` → empty (compile-time output-id check ABSENT).
