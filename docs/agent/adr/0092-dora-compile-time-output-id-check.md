---
doc_kind: adr
adr_id: 0092
title: "dora `send_output` output-id compile-time-catch (`DoraUnknownOutputId`)"
status: accepted
date: 2026-06-06
last_verified_commit: 3a9c15b
supersedes: []
superseded_by: []
---

# ADR-0092: dora `send_output` output-id compile-time-catch (`DoraUnknownOutputId`)

## Context

`event.send_output("<id>", payload)` emits a payload on a dora output
port. The id MUST be one the node DECLARES via
`@dora.node(outputs=[...])`. Before this ADR, a mistyped id was caught
only at **RUNTIME**: the `cobrust-dora` `__cobrust_dora_event_send_output`
shim (`crates/cobrust-dora/src/cabi.rs`) checked the id against the
process-global `DECLARED_OUTPUTS` set and, on a miss, did an `eprintln!`
plus a `-1` return ("Output dropped."). The output silently vanished;
nothing failed the compile.

This violated the CLAUDE.md §2.5-A **compile-time-catch** north star: a
mistyped output id is a *static* error the type-checker can prove (the
declared-output set is a finite set of string literals known at
type-check time, and a literal id is a constant). It was the ONE
genuinely-remaining real-path dora compiler increment — ADR-0076c §4.2 /
ADR-0076 §6 Phase-2 done-means-2 explicitly deferred it ("a compile-time
check wants the static declared-output set"), and the runtime shim's own
comment named it "the §2.5 compile-time-catch follow-up … Phase B".

The data the check needs already flows through the compiler:

- `@dora.node(outputs=["pose"])` desugars (cobrust-hir
  `lower.rs::build_eco_module_register_calls`) into one
  `dora.declare_output("pose")` module-fn register-call (a string-literal
  arg) inserted at `main`'s prologue, BEFORE the `dora.node(handler)`
  call. The bare `@dora.node` form (no kwargs) declares NOTHING.
- So at type-check time the declared-output set = `{ every string-literal
  arg to a dora.declare_output(...) call in the module }`. One dora node
  per `.cb` program (each node is its own process) ⇒ ONE declared-output
  set per module is correct and COMPLETE (every declare is a static
  literal in `main`).
- `event.send_output(...)` is type-checked via the `DORA_EVENT_ADT`
  handle-method path (`cobrust-types` `ecosystem.rs`,
  `try_synth_ecosystem_call` Case 2). The send-call lives INSIDE the
  handler fn — a DIFFERENT function from `main` where `declare_output`
  runs. The check is therefore inherently **cross-function**: it must
  thread the module-level declared set down to a method-call site in a
  callback body. That cross-function thread is what makes this real
  compiler work (not a local lint).

## Options considered

1. **Runtime-only (status quo)** — keep the `-1` + `eprintln!`. Rejected:
   violates §2.5-A; the LLM gets no compile-error feedback signal; a
   dropped output is an invisible bug.
2. **Carry the declared set on `TypedModule` and check in MIR.** Rejected:
   later than necessary (§2.5-A prefers the EARLIEST catch — type-check
   over MIR), and MIR has no cleaner access to the call-site literal than
   the type-checker already does.
3. **A module pre-pass on `Ctx` + a literal-arg membership check in the
   `send_output` method-synth (CHOSEN).** A pre-pass (run between the
   type-checker's prebind pass and its body-check pass) walks the whole
   HIR module and collects every `dora.declare_output("<lit>")`
   string-literal into `Ctx.dora_declared_outputs: Option<BTreeSet<String>>`.
   The `send_output` synth consults it. Earliest catch, reuses the
   existing manifest-lookup recognition, no new IR field.

## Decision

Implement option 3.

**The module pre-pass** — `check.rs::collect_dora_declared_outputs` runs
in `check_module` AFTER `prebind_items` (so the `import dora` alias is in
`ecosystem_module_defs` and a `dora.declare_output(...)` call is
recognisable) and BEFORE Pass-2 body-checking (so the set is COMPLETE
when any `send_output` is checked). It recursively walks every expression
in the module (a structural `walk_expr_children` visitor, not just
`main`'s top-level stmts, so it is robust to the desugar's placement) and
collects the string-literal id of each call recognised as
`dora.declare_output(...)` — an `Attr { base: Name(rn), name:
"declare_output" }` where `rn.def_id` is a recorded `dora` module alias
AND the manifest row exists (`lookup_module_fn("dora", "declare_output")`,
runtime symbol `__cobrust_dora_declare_output`). The result lands in
`Ctx.dora_declared_outputs`:

- `None` — the module has NO `dora.declare_output(...)` call (a bare
  `@dora.node`, or a non-dora program). The check is **inert**.
- `Some(set)` — the module declares outputs; the set is complete.

A `BTreeSet` is deliberate: the §2.5-B FIX text renders the declared list
in a deterministic, source-stable order.

**The membership check** — in `try_synth_ecosystem_call` Case 2, right
after `lookup_handle_method` resolves the `send_output` sig and BEFORE the
generic `check_eco_sig` (the SAME interception shape as Case 1's
`coil.array` special-case), when the receiver is `DORA_EVENT_ADT` and the
method is `send_output`, call `check_dora_send_output_id(args)`. It
rejects ONLY when ALL hold: the FIRST arg is a STRING LITERAL, the module
declares outputs (`Some(set)`), and the literal is NOT in `set`. Both
SKIP edges return `Ok(())` (fall through to the unchanged `(Str, Str) ->
i64` path):

- **non-literal id** (a variable / computed `str`) — cannot be proven
  statically; SKIP. The runtime `-1` backstop covers it.
- **None set** (no declared outputs) — the full set is unknown; SKIP. No
  false-positive on the un-typed bare surface.
- **id IS in the set** — accept (unchanged path).

**The new error variant** (`cobrust-types` `error.rs`):

```rust
TypeError::DoraUnknownOutputId {
    id: String,                       // the offending literal
    declared: Vec<String>,            // the declared-output list (sorted)
    nearest: Option<String>,          // nearest declared id by edit-distance
    span: Span,                       // the id-arg's own span (precise)
    suggestion: Option<&'static str>, // the uniform ADR-0052b static hint
}
```

Field-style follows the `UnknownField` precedent (the existing rich
owned-`String` + `Vec<String>` variant): the dynamic per-node ids live in
owned-`String` fields rendered in `Display`, while `suggestion` keeps the
uniform `Option<&'static str>` shape every peer variant carries (the
constant clause the LSP `with_suggestion` dispatcher + the `fix_safety`
ladder consume). The dynamic "did you mean" lives in a SEPARATE `nearest:
Option<String>` field rather than overloading `suggestion` to `String` —
this is the reconciliation between the design's "dynamic nearest-match"
intent and the `Option<&'static str>` architecture every cascade consumer
depends on (`type_error_suggestion_text` returns `Option<&'static str>`;
LSP binds `*suggestion`).

**§2.5-B FIX (the error PRINTS the FIX, not just the diagnosis).** The
`#[error(...)]` Display renders:

```
unknown dora output id `twst` — it is not declared in
`@dora.node(outputs=[...])` at <span>; declared outputs: [pose, twist];
did you mean `twist`?
```

The `did you mean` clause is present only when `nearest` is `Some`
(`nearest_declared_output` picks the closest declared id by Levenshtein
edit distance, gated to a close-enough threshold so a wildly-different id
gets no misleading suggestion). The `cobrust check` path renders the same
through the `error_ux` `UserError` layer (`crates/cobrust-cli/src/error_ux.rs`).
An LLM reading stderr fixes the call in one step.

**The nearest-match** is a small inline Levenshtein (`check.rs`) — NO new
dependency (the Cargo.lock-unchanged constraint).

## The renderer cascade

A new `TypeError` variant must be threaded through every exhaustive match
or a parity-test goes red. Files threaded:

- `crates/cobrust-types/src/error.rs` — the variant + `#[error]` Display.
- `crates/cobrust-types/src/fix_safety.rs` — `type_error_suggestion_text`
  (returns the static hint) + `type_error_fix_safety` (`LocalEdit`: the
  fix is a local id-string or `outputs=[...]` edit).
- `crates/cobrust-cli/src/error_ux.rs` — the §2.5-B FIX-text builder (the
  `UserError::Type` `msg` the LLM parses from `cobrust check` stderr).
- `crates/cobrust-lsp/src/diagnostic.rs` — `type_error_to_diagnostic_single`
  (code `"dora-unknown-output-id"`).
- `crates/cobrust-types-cb/src/error_cb.rs` — the `TypeErrorCb` mirror
  variant + `type_error_cb_from_rust` From arm + `Canonicalize` arm +
  byte-parity `Display` arm + `type_error_cb_variant_name` arm.
- `crates/cobrust-types-cb/src/fix_safety_cb.rs` — `type_error_cb_fix_safety`.
- `crates/cobrust-types-cb/tests/error_display_parity.rs` — both
  `SuggestionText` projector impls.
- `crates/cobrust-types-parity/src/lib.rs` — `impl Canonicalize for
  TypeError` arm + `type_error_variant_name` arm.

The `.cb` source mirror (`crates/cobrust-types-cb/src/error.cb`) is a
CURATED SUBSET that intentionally omits the newer rich variants
(`UnknownField`, `LenArgNotSized`, …); `DoraUnknownOutputId` follows that
precedent and is NOT added there. The exhaustive Rust mirror lives in
`error_cb.rs`, which IS extended.

The `.cb` `Display` mirror renders BYTE-IDENTICALLY to the Rust side (the
variant carries NO `Ty` payload, so there is no handle-convention
compromise — it is exactly the Rust message).

## Design principle — footgun ledger (elegant ecosystem surface, no debt)

This increment DROPS a footgun: the **runtime-only stringly-typed output
id**. A stringly id whose validity is checked only at runtime (Flask's
`url_for` name typos, Express's route-name strings) is the exact
"validation deferred to runtime" debt CLAUDE.md §2.2/§2.5 forbids. By
lifting the id check to compile time, the `.cb` dora surface gains the
static guarantee its Python/Rust forebears lack — a §2.5 win, not a
mechanical clone of the dora-rs Python API. The runtime `-1` backstop
remains ONLY for the genuinely-undecidable dynamic-id case (a non-literal
id), which is the correct division of labour: prove what is provable at
compile time, fail-closed at runtime for the rest.

## Consequences

- **Positive**
  - A mistyped `send_output` id is a `cobrust check` / `cobrust build`
    error, not a silent runtime drop (§2.5-A compile-time-catch).
  - The error PRINTS THE FIX — declared ids + nearest-match (§2.5-B). An
    LLM corrects in one step.
  - ZERO new IR fields, ZERO new dependency, ZERO new manifest op. Reuses
    the existing `lookup_module_fn` recognition + the `coil.array`
    special-case interception shape.
  - No false-positive on the dynamic-id or bare-node surfaces (both SKIP).
- **Negative**
  - One more `TypeError` variant ⇒ the full renderer cascade (8 files +
    1 test file). This is the known, paid cost of any new variant.
- **Neutral / unknown**
  - The Arrow-payload surface (`coil.Buffer ↔ Arrow`, `send_output_buffer`)
    is a SEPARATE next dispatch (ADR-0076c open item); this ADR is ONLY
    the output-id check.

## Evidence

- New e2e: `crates/cobrust-cli/tests/dora_output_id_check_e2e.rs` — 5
  tests: NEGATIVE-build (rejects + names `twist_typo` + `pose`),
  NEGATIVE-check (the §2.5-B FIX: `declared outputs: [pose, twist]; did
  you mean \`twist\`?`), POSITIVE (declared id builds + runs, emits
  `output[pose]=frame_001`, exit 0), NON-LITERAL (skip, no false-positive),
  BARE (None-set skip, no false-positive). All pass.
- Regression: the 4 dora e2es (`decorator_dora_e2e` 6/6, `dora_hello_e2e`
  3/3, `dora_multi_io_e2e` 3/3, `dora_real_node_e2e` 2/2) — `dora_multi_io`'s
  `send_output("reading", _)` against `outputs=["reading"]` is the
  happy-path cross-file regression guard and stays green.
- Parity cascade: `cobrust-types-cb` + `cobrust-types-parity` green (the
  byte-parity tripwire).
- Implementation: `crates/cobrust-types/src/check.rs`
  (`collect_dora_declared_outputs`, `check_dora_send_output_id`,
  `nearest_declared_output`, `levenshtein`); `crates/cobrust-types/src/error.rs`.
- Cross-references: [adr:0076](0076-dora-cb-stream-y.md) §6 Phase-2
  done-means-2; [adr:0076c](0076c-dora-arrow-payload-surface.md) §4.2;
  [adr:0080](0080-cb-native-type-driven-request-validation-and-openapi.md)
  (the `UnknownField` precedent this variant mirrors);
  [adr:0052b](0052b-error-ux-fix-suggestions.md) (the §2.5-B suggestion
  shape). CLAUDE.md §2.5-A / §2.5-B.
