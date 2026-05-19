---
doc_kind: adr
adr_id: 0058c
parent_adr: 0058
title: "Phase K wave-3 — LLVM DWARF debug-info emission"
status: proposed
date: 2026-05-19
ratified_at: TBD
last_verified_commit: 2575a4d
supersedes: []
superseded_by: []
relates_to: [adr:0058, adr:0058a, adr:0058b, adr:0023, adr:0046, adr:0059]
discovered_by: P10 Phase K wave-3 third sub-sprint per ADR-0058 §"Sub-ADR roster"
ratification_path: P9 sub-ADR review; ratify on DEV landing + DG verify clean + lldb-18 smoke
---

# ADR-0058c: Phase K wave-3 — LLVM DWARF debug-info emission

## 1. Motivation

ADR-0058a (wave-1, accepted `3d60e63`) shipped MIR → LLVM IR lowering core.
ADR-0058b (wave-2, accepted at HEAD `2575a4d` / `72f4d27`) shipped the
PassBuilder pipeline + multi-target dispatch. Both waves left **DWARF
debug-info emission** as an explicit deferral:

> ADR-0058b §4 non-goals: DWARF debug-info emission (`DIBuilder`,
> `dbg.declare`) is sub-ADR 0058c.

ADR-0058 §13 codifies the binding contract:

> Phase L Debugger (ADR-0059) blocks on Phase K's ADR-0058c sub-ADR
> landing. ADR-0058c emits DWARF v5 lines + variables + types. ADR-0059
> consumes via lldb / gdb / VS Code DAP — standard DWARF consumers.

This bar has been **open** since the Phase K frame author at HEAD
`bc10842`. ADR-0058c is the empirical close: wire `DebugInfoBuilder` into
`llvm_backend::emit`, emit per-function `DISubprogram` + per-`Span`
line-table debug locations, validate that `llvm-dwarfdump-18` shows
non-empty DWARF sections + `lldb-18` can set breakpoints on Cobrust
source line numbers and hit them, and amend ADR-0058 §13 to RESOLVED
status (Phase L Debugger ADR-0059 unblocked).

## 2. §2.5 LLM-first design — neutral

Per ADR-0058 §2 "§2.5 ROI position", Phase K is §2.5-neutral. ADR-0058c
inherits that neutrality: DWARF emission is **debugger UX
infrastructure**, not LLM-friendliness. §2.5 §A
(compile-time-catch-errors) is unaffected — DWARF lives at the codegen
layer, downstream of type-check and borrow-check. §2.5 §B
(training-data-overlap) is unaffected — DWARF is an LLVM-consumed binary
format; source-level surface stays identical to wave-1/2.

§2.5 audit: must not regress error UX. `DebugInfoBuilder::finalize`
failures propagate as `CodegenError::LlvmError(String)` — identical
shape to the wave-1/2 path. No new `TypeError::*` variants. The LLM
writes Cobrust source identically; the only observable change is the
emitted object file now carries `.debug_*` sections.

## 3. Decision

### 3.1 DIBuilder scaffold

Extend `LlvmEmitter<'ctx>` with two new fields:

```rust
pub struct LlvmEmitter<'ctx> {
    // ... existing fields ...
    /// inkwell DWARF builder; one per LLVM module (per source file).
    di_builder: DebugInfoBuilder<'ctx>,
    /// Compile-unit DI scope; root of every Cobrust function's DIScope chain.
    di_cu: DICompileUnit<'ctx>,
    /// The DIFile inside `di_cu`; reused for every DISubprogram + DILocation.
    di_file: DIFile<'ctx>,
    /// Cached basic-type DIs (Int / Float / Bool / opaque-ptr) so each
    /// signature lowering reuses the same DIType objects.
    di_basic_types: HashMap<&'static str, DIBasicType<'ctx>>,
    /// Cached source-string for `LineMap` lookups. Empty when the source
    /// path isn't known (e.g. tests run on synthetic modules).
    di_line_map: LineMap,
}
```

Add the module-level "Debug Info Version" metadata flag at construction
time per inkwell 0.9 `debug_info` module example:

```rust
let dbg_ver = ctx.i32_type().const_int(3, false);
module.add_basic_value_flag(
    "Debug Info Version",
    FlagBehavior::Warning,
    dbg_ver,
);
module.add_basic_value_flag(
    "Dwarf Version",
    FlagBehavior::Warning,
    ctx.i32_type().const_int(5, false),
);
```

Construct the `(DebugInfoBuilder, DICompileUnit)` via
`Module::create_debug_info_builder` with `DWARFSourceLanguage::C` (DWARF
spec does not yet enumerate a Cobrust-specific tag; LLVM-18's
`DWARFSourceLanguage` exposes `Rust` + `C99` + `C11` + `C17`; we pick
`C` as the safest fallback — debuggers all recognise it).

### 3.2 Per-function DISubprogram

For each `Body`, before lowering its IR, build a `DISubroutineType` from
the param + return DI basic types, then a `DISubprogram` rooted at the
compile-unit scope. Attach the subprogram to the LLVM `FunctionValue`
via `func.set_subprogram(sp)`.

| Cobrust `Ty` | DWARF basic type |
|---|---|
| `Int` | `int64_t` (64-bit, encoding `DW_ATE_signed`) |
| `Float` | `double` (64-bit, encoding `DW_ATE_float`) |
| `Bool` | `bool` (8-bit, encoding `DW_ATE_boolean`) |
| `Str` / `Bytes` / `List` / `Dict` / `Set` / `Ref` / `None` / `Tuple` / etc. | opaque pointer (`ptr`, 64-bit, encoding `DW_ATE_address`) |

A single shared cache (`di_basic_types`) deduplicates the four DI basic
types per module — each signature reuses the same `DIType` pointers.

### 3.3 Per-Span line-table debug locations

Build a minimal `LineMap` inline in `cobrust-codegen` (reusing the
cobrust-lsp algorithm but avoiding the LSP dep). Each MIR
`Statement::span` + `Terminator`-carrying-span resolves to (line,
column) via the LineMap; before each LLVM instruction emission, call
`builder.set_current_debug_location(loc)` with the per-statement
DILocation. The DILocation is rooted at the current `DISubprogram`'s
scope.

Source-path resolution: `TargetSpec` extends with an optional
`source_path: Option<PathBuf>` field (None ⇒ synthetic / test). When
`None`, the DIFile uses the spec's `module_name` as filename + `.` as
directory; line table maps statement spans against an empty source
(line/column → 0/0 fallback). When `Some`, the LineMap is built from
the file's contents at emit time.

### 3.4 DIBuilder finalize + module verification

Before the optimization pipeline runs (per `emit`'s `run_passes`
invocation), call `di_builder.finalize()` to write all deferred DIEs
into the module's IR. The module verifier (`emitter.module.verify()`)
runs after finalize so DI-shape errors surface as
`CodegenError::LlvmError(String)`.

### 3.5 lldb smoke harness

Integration tests at `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs`:

- Compile 3-5 .cb-equivalent MIR fixtures via the emit pipeline (object
  output, host triple, `OptLevel::None`).
- Spawn `lldb-18 -b` (batch mode) with a script that sets a breakpoint
  on a known source line + verifies the breakpoint resolves (matches
  the per-function DISubprogram + per-Span DILocation).
- Skip cleanly when `lldb-18` is not on `$PATH` (Mac dev machines may
  lack it; DG-Workstation has it via the LLVM apt install).

Acceptance gate per §"Acceptance":

- `llvm-dwarfdump-18 fixture.o | grep DW_TAG_subprogram` returns ≥1 hit
  per Cobrust function emitted.
- `llvm-dwarfdump-18 fixture.o | grep DW_TAG_compile_unit` returns
  exactly 1 entry per module.
- `lldb-18` breakpoint set on a known statement's source line resolves
  (the line number in the `.debug_line` section maps to a valid PC in
  the object).

## 4. Non-goals

Wave-3 ships the **per-function + per-line** DWARF baseline. Out of
scope (Phase L+ or follow-on sub-ADR):

- **Source-level variable inspection** — `DILocalVariable` /
  `DIFormalParameter` entries for `lldb frame variable` to walk Cobrust
  locals. Per ADR-0058 §7 wave-3 ships DWARF lines + types; full
  local-variable inspection is Phase L UX, not Phase K codegen.
- **macOS dSYM packaging** — the linker post-step that bundles
  `.debug_*` sections into a `.dSYM` directory is a separate concern
  (handled by `dsymutil` invocation in `release.yml`, not the LLVM
  backend). On Mach-O, the `.debug_*` sections still write into the
  emitted object directly per LLVM-18 default behaviour.
- **Inlined-frame chains** — `DILocation::inlined_at` for the
  inliner's frame DIE chain. Per ADR-0058 §7: "Inlining: deferred to
  post-Phase-K. Inlined-frame DWARF is Phase-L+ if debugger demand
  surfaces it."
- **DWARF v4 fallback** — wave-3 emits DWARF v5 (LLVM-18 default).
  Older toolchains stuck at v4 must regenerate; not in scope.

## 5. Risk register

### 5.1 inkwell DI surface gaps

- **Risk**: inkwell 0.9's `DebugInfoBuilder` may not surface every
  `DIBuilder` C-API. The cached check (per ADR-0058 §10.2): inkwell
  exposes `create_compile_unit`, `create_file`, `create_function`,
  `create_subroutine_type`, `create_basic_type`,
  `create_debug_location`, `create_lexical_block`, `finalize`. These
  cover wave-3's full surface.
- **Mitigation**: per ADR-0058 §10.2 mitigation, if any specific call
  surfaces as missing, drop to `llvm-sys` for that one feature +
  document the carve-out inline. Wave-3 deliberately scopes to the
  surface inkwell covers — `DILocalVariable` (variable inspection)
  intentionally deferred per §4.

### 5.2 Cross-platform DWARF differences

- **Risk**: ELF (Linux) emits `.debug_*` sections; Mach-O (Mac) uses
  `__DWARF` segment + post-link `dsymutil` to package `.dSYM/`. The
  inkwell + LLVM-18 path handles both transparently in `Module::run_passes`
  + `TargetMachine::write_to_file`. No platform-specific code in
  `llvm_backend`; verification by reading the object file's section
  table is platform-agnostic via the `object` crate (already present).
- **Mitigation**: §3.5 acceptance gate uses `llvm-dwarfdump-18` which
  handles both ELF + Mach-O object formats. lldb smoke runs only when
  `lldb-18` is available (DG-Workstation has it from `llvm.sh`).

### 5.3 Synthetic-test source path

- **Risk**: existing `tests` mod in `llvm_backend.rs` constructs MIR
  modules by hand without an on-disk source file. Per §3.3, `None`
  source_path means LineMap-empty fallback; DILocation lines collapse
  to 0/0. This is fine for object verification (DI structure still
  validates) but degrades lldb breakpoint matching.
- **Mitigation**: smoke tests construct a real on-disk `.cb` fixture in
  `tempfile::tempdir()` + write the source body. The MIR's per-statement
  span byte-offsets must align with the fixture's actual content
  (existing harness uses synthetic spans `Span::point(SYNTHETIC, 0)`
  which collapse cleanly to line 1 / col 0). Smoke tests assert
  `DW_TAG_subprogram` exists + line table is non-empty, not specific
  line numbers — robust against synthetic-span quirks.

## 6. Acceptance

Wave-3 lands DEV (LlvmEmitter scaffold + per-fn DISubprogram + per-Span
DILocation + finalize + 5 inline smoke tests + 3-5 lldb smoke fixtures)
+ DG verify clean (codegen test suite + dwarf_lldb_smoke pass; previous
0058a + 0058b baselines preserved at 404+ tests) when:

- `cargo test -p cobrust-codegen --features llvm` returns exit 0 on
  DG-Workstation. Test count ≥ 404 (wave-2 baseline) + 5 new inline
  smoke + 3-5 new lldb integration.
- Object emitted by `emit()` on a 2-line fixture contains a
  `DW_TAG_compile_unit` entry + one `DW_TAG_subprogram` per emitted
  Cobrust function per `llvm-dwarfdump-18`.
- `lldb-18 -b` script setting a breakpoint on a known source line
  resolves to a non-zero PC. Skip cleanly when lldb-18 unavailable.
- POSTFLIGHT: `/tmp/cobrust-*` clean (≤ 0 entries) per heavy-build
  offload policy.

## 7. Consequences

### 7.1 Positive

- ADR-0058 §13 RESOLVED: Phase L Debugger ADR-0059 dispatch-readiness
  gate met. Phase L can frame-author immediately on this sub-ADR's
  acceptance.
- Tier-1 multi-target binaries (per ADR-0058b §3.4) gain DWARF in their
  release builds; downstream lldb / gdb / VS Code DAP consumers benefit
  with zero Cobrust-side bridging (bind-the-core, ADR-0012).
- Source-line debugging works end-to-end without new MIR features —
  span infrastructure (carried since M5) is the foundation.

### 7.2 Negative

- ~800 LOC delta + ~200 LOC test (per dispatch budget). Object file
  size grows modestly per DWARF section overhead (typical 20-30% larger
  unstripped; strippable via `strip --strip-debug` post-link).
- One new `tempfile` + `object` dev-dep usage (both already present
  per `binary_size_bench.rs`). No new top-level dependencies.

### 7.3 Neutral

- Variable-inspection deferral (§4) is a known Phase L UX gap;
  documented inline so Phase L author knows the boundary.
- DWARF v5 vs v4 emission is LLVM-18-default behaviour; we don't
  reverse-engineer to v4 even for distro compatibility (LLVM-18 is the
  tier-1 toolchain per ADR-0058 §10.1).
- `DWARFSourceLanguage::C` fallback (vs. a future `DWARFSourceLanguage::Cobrust`)
  is documented as a known-bind; debuggers recognise C and render
  source lines correctly. Future DWARF revision adding a Cobrust tag
  would be a backend-only swap.

## 8. Evidence

- ADR-0058 (Phase K frame, `bc10842`) — §7 DWARF emission contract; §13
  Phase K × L handoff binding gate.
- ADR-0058a (wave-1, `3d60e63`) — §3 LlvmEmitter scaffold (extended by
  this ADR with DIBuilder fields).
- ADR-0058b (wave-2, `72f4d27`) — §3.2 emit flow + PassBuilder
  finalize boundary (DIBuilder finalize happens at the parallel hook).
- `crates/cobrust-codegen/src/llvm_backend.rs:259-305` (HEAD `2575a4d`) —
  `LlvmEmitter::new` constructor to extend.
- `crates/cobrust-frontend/src/span.rs:30-87` — `Span(FileId, start,
  end)` half-open byte range; the foundation for DILocation lookup.
- `crates/cobrust-mir/src/tree.rs:60-62, 111-112, 119-122` — Spans on
  every Body / BasicBlock / Statement; per-Span line table input.
- `crates/cobrust-lsp/src/span_convert.rs:23-90` — LineMap algorithm
  reused inline (cobrust-codegen avoids LSP dep).
- `inkwell 0.9` debug_info module — `DebugInfoBuilder` API surface
  per `~/.cargo/registry/cache/.../inkwell-0.9.0.crate` `src/debug_info.rs`:
  `create_compile_unit`, `create_file`, `create_function`,
  `create_subroutine_type`, `create_basic_type`,
  `create_debug_location`, `create_lexical_block`, `finalize`.
- CLAUDE.md §2.5 (HEAD `2575a4d`) — LLM-first design; wave-3 §2.5-neutral.

— P10 strict dispatcher, Phase K wave-3, 2026-05-19
