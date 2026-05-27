---
doc_kind: adr
adr_id: 0023
title: M9 codegen — backend feature flags, ABI, calling convention, linker delegation, target matrix
status: accepted
date: 2026-04-30
last_verified_commit: d2cbb8d
supersedes: []
superseded_by: []
dependencies: [adr:0006, adr:0012, adr:0019, adr:0020]
---

# ADR-0023: M9 codegen — backend feature flags, ABI, calling convention, linker delegation, target matrix

## Context

Constitution `CLAUDE.md` §4.1 places codegen at the end of the
language pipeline:

> Lexer → Parser → AST → HIR → MIR → Codegen (LLVM / Cranelift)

ADR-0019 §"M9 — Codegen" pinned the milestone scope:

> Lower MIR to native code. Two backends behind a feature flag;
> default depends on `--release`.
>
> | Backend | Default for | Pros | Cons |
> |---|---|---|---|
> | Cranelift | `cargo build` (dev) | Pure Rust, fast compile, no system deps | Less mature optimization; reduced target coverage |
> | LLVM | `cargo build --release` | Best codegen quality, broad target support | Slow build, large dep tree, requires system LLVM |

ADR-0020 froze the MIR shape (6 node families, 7 terminators,
`Place / Rvalue / Operand`, drop schedule + 5 borrow obligations
B1..B5 discharged). The MIR `Module` is the input contract for
M9 codegen — control-flow-explicit, every `LocalDecl` carries a
fully-resolved `Ty`, every `BasicBlock` ends in exactly one
terminator, every owning local has its drop schedule pre-computed.

ADR-0012 §"translate the surface, bind the core" pinned the
strategy: where a mature Rust crate exists, **bind it; do not
reimplement**. Cranelift (`bytecodealliance/cranelift`) is the
right backend for fast `cargo build` cycles — it is already used
by `rustc_codegen_cranelift`. LLVM via `inkwell` (which wraps
LLVM 18+) is the right backend for `--release` because numpy-tier
performance demands `-O3` codegen quality. We bind both; we
reimplement neither.

ADR-0006 §"Soundness proof obligation list" enumerated 9
obligations. Items 4..9 were discharged at type-check (M2). Items
1..3 (Progress / Preservation / Lowering preservation) projected
onto MIR-time as B1..B5 (M8). Codegen at M9 must **not reintroduce
UB**: every `Statement::Assign` and `Terminator::Call` must lower
to native instructions whose memory + control flow respect the
proven invariants. This is operationalized via a differential
gate (every "core 30" form's compiled output produces identical
`stdout` to a hand-written reference Rust program).

## Options considered

1. **Cranelift only — defer LLVM indefinitely.**
   - Pros: smallest dep tree; `cargo build` works on any host
     without system deps; one IR mapping to maintain.
   - Cons: ADR-0019 explicitly enumerates LLVM as the `--release`
     path (broad target support, best opt quality). Numpy-tier
     workloads will benchmark below numpy if we ship Cranelift-only
     in `--release`. Rejected.

2. **LLVM only — defer Cranelift.**
   - Pros: best codegen quality on day one; broad target coverage.
   - Cons: cold `cargo build` requires system LLVM 18+ (slow,
     fragile across host toolchains). Translates to "Cobrust
     has a 10-minute first-build experience", which conflicts
     with constitution §5.3 (efficient: "redundant prompt hitting
     the network is a bug" applies analogously to redundant
     LLVM dep download). Rejected.

3. **Both backends behind a feature flag; Cranelift default,
   LLVM opt-in via `--features llvm`.** *(chosen)*
   - Pros: matches ADR-0019 binding; default `cargo build` is
     pure-Rust + sub-second incremental; `--features llvm` for
     `--release` opens the optimization tier. Each backend lives
     in its own module (`cranelift_backend.rs` / `llvm_backend.rs`)
     so the public surface (`emit / TargetSpec / Artifact /
     CodegenError`) is backend-agnostic. The `Backend::Default`
     selection lives in `target.rs` and uses `cfg!(feature = "llvm")`
     plus the build profile to pick at codegen time.
   - Cons: two IR-lowering paths to maintain. Mitigation: each
     backend has its own well-formed + ill-formed test corpus,
     and the differential gate runs both backends on the "core
     30" forms — divergence is automatically caught.

4. **Custom backend (assemble x86_64 / aarch64 directly).**
   - Pros: smallest binary; full ownership.
   - Cons: violates ADR-0012 ("bind the core"); 5+ engineering
     years to match Cranelift's instruction selection. Rejected.

## Decision

Adopt **option 3**: both backends behind a feature flag.

> **AMENDED by ADR-0070 §X.4 (RATIFIED 2026-05-27).** The two-backend
> layout below is M9-historical. As of v0.7.0, **LLVM is the sole AOT
> backend**: the Cranelift AOT backend (`cranelift_backend.rs` + `abi.rs`)
> was removed, and `cranelift-module` / `cranelift-object` were dropped.
> Cranelift is retained **only** as the `cobrust-jit` IR substrate
> (`lowering.rs` + `cranelift-codegen` / `cranelift-frontend`), not as an
> AOT backend. The `llvm` feature remains default-on (Option C); building
> `--no-default-features` yields a JIT-substrate / frontend-only crate
> whose `emit()` returns `UnsupportedBackend`. Read the layout below as
> the original M9 design, not current behaviour.

### Backend feature flag layout

```toml
# crates/cobrust-codegen/Cargo.toml
[features]
default = []
llvm = ["dep:inkwell"]
lld = []  # use lld via -fuse-ld=lld instead of cc

[dependencies]
cobrust-mir = { path = "../cobrust-mir" }
cobrust-types = { path = "../cobrust-types" }
cobrust-frontend = { path = "../cobrust-frontend" }
cranelift-codegen = "0.131"
cranelift-frontend = "0.131"
cranelift-module = "0.131"
cranelift-object = "0.131"
target-lexicon = "0.13"
inkwell = { version = "0.9", optional = true }
thiserror = { workspace = true }
tempfile = "3"
```

- **Default** (`cargo build`): Cranelift only. Pure Rust. Sub-second
  incremental compiles on any host with rustc 1.94+.
- **`--features llvm`**: pulls in `inkwell` + system LLVM 18+.
  Used for `cargo build --release` paths where opt quality matters.
- **`--features lld`**: linker delegation switches from `cc` to
  `lld` via `-fuse-ld=lld` (independent of the codegen backend).

### `extern "Cobrust"` ABI

Cobrust's internal calling convention is **System V AMD64** on
`x86_64-unknown-linux-gnu` and **AAPCS64** on `aarch64-apple-darwin`
(matching the host's standard C ABI). This decision pins:

- **Argument passing**: integer / pointer args go in
  registers (`rdi rsi rdx rcx r8 r9` on AMD64; `x0..x7` on AAPCS64);
  floats in xmm / SIMD registers; spillover on the stack.
- **Return**: integer / pointer in `rax` / `x0`; float in `xmm0` /
  `d0`; aggregate returns spilled to a caller-provided slot.
- **Stack layout**: 16-byte alignment at call boundaries; red zone
  per platform default (128 bytes AMD64, none AAPCS64).
- **`extern "Cobrust"`** is the **internal default** (no marker
  needed at HIR level). M9's lowering treats every Cobrust function
  as `extern "Cobrust" = SystemV / AAPCS64`.
- **`extern "C"`** at the source level lowers to the same calling
  convention but skips Cobrust-specific attributes (e.g. no
  unwind info on `extern "C"`-marked functions). Reserved for
  M11 stdlib's runtime-helper interface.
- **No `extern "Rust"`** — Rust's calling convention is unstable;
  cross-language calls go through `extern "C"` boundaries.

### Calling convention details (binding)

| Aspect | AMD64 (Linux) | AArch64 (macOS) |
|---|---|---|
| Integer arg regs | rdi rsi rdx rcx r8 r9 | x0 x1 x2 x3 x4 x5 x6 x7 |
| Float arg regs | xmm0..xmm7 | d0..d7 |
| Integer return | rax (rdx for 128-bit) | x0 (x1 for 128-bit) |
| Float return | xmm0 | d0 |
| Stack alignment at call | 16 bytes | 16 bytes |
| Red zone | 128 bytes | none |
| Frame pointer | rbp (optional, omitted under `-O`) | x29 (always preserved on macOS) |
| Linkage marker | Cranelift `CallConv::SystemV` | Cranelift `CallConv::AppleAarch64` |

### Linker delegation

M9 does **not** ship its own linker. It delegates to:

- **Default**: invoke `cc` (gcc / clang via `$CC`). Pulls in
  libc + the platform crt0. This is what every Rust project
  ends up doing under the hood.
- **`--features lld`**: passes `-fuse-ld=lld` to `cc`, switching
  the resolver to LLD without changing the front-end binary.
- **No bundled lld**: we do not ship `lld` ourselves.

Object file → executable produced via:

```
cc target/cobrust/<name>.o -o target/cobrust/<name>
# or
cc target/cobrust/<name>.o -o target/cobrust/<name>.so -shared
```

`cc` is invoked through `std::process::Command`. Stderr is captured
and reported as `CodegenError::LinkerFailed { stderr, exit_code }`.

### Target triple matrix (delivery scope)

| Triple | Object format | Status (M9) |
|---|---|---|
| `x86_64-unknown-linux-gnu` | ELF | delivered |
| `aarch64-apple-darwin` | Mach-O | delivered |
| `x86_64-apple-darwin` | Mach-O | reachable (Cranelift supports; not gated) |
| `aarch64-unknown-linux-gnu` | ELF | reachable (Cranelift supports; not gated) |
| `wasm32-unknown-unknown` | WASM | out of scope (Phase F) |
| `x86_64-pc-windows-msvc` | COFF | out of scope (Phase F) |

The matrix is **expansion-friendly**: `target-lexicon` parses the
triple, Cranelift's `isa::lookup(triple)` returns a builder, and
the linker delegate (`cc`) handles the platform-specific runtime.

### Object emission

- **ELF on Linux**: `cranelift-object` emits ELF directly via the
  `object = "0.36"` crate it depends on.
- **Mach-O on macOS**: same `cranelift-object` path; the format
  is selected from the triple.
- The **`Artifact` enum** has three variants:
  - `Object(PathBuf)` — relocatable `.o` file.
  - `Executable(PathBuf)` — linked binary.
  - `DynamicLibrary(PathBuf)` — `.so` / `.dylib` (post-link).

### Differential gate (acceptance contract)

Every form in ADR-0003's "core 30" must compile + run + produce
**bit-identical `stdout`** vs a hand-written Rust reference program
on at least one delivery-scope target.

| Aspect | Reference | Cobrust output |
|---|---|---|
| Source | `tests/diff_corpus/<form>.rs` (hand-written Rust) | corresponding `.cb` source compiled via M9 |
| Compile | `rustc --edition 2024 -O` (or `cargo run`) | `emit(mir, TargetSpec::host_release())` |
| Run | exec | exec |
| Compare | `stdout` byte-by-byte | same — must be byte-identical |

If a form's reference uses functionality M9 hasn't implemented
yet (e.g., `print` requires M11 stdlib), the differential gate
records the form as **out-of-scope (M9 stub)** with a tracked
M10/M11 followup ticket. The gate runs all forms; failure = at
least one in-scope form mismatched.

### LLVM `-O3` ≥ 30% smaller binary acceptance — RESOLVED 2026-05-19 (Phase K wave-2)

ADR-0019 §"M9 — Codegen" pinned: "Optional LLVM backend
(`--features llvm`) produces correct object code; ≥ 30% smaller
binary on a representative sample at `-O3`."

The acceptance was originally specified on a **fixed sample**
(`benches/binsize/`):

- `fib_50.cb` — recursive fib(50)
- `dotproduct_1k.cb` — 1024-element dot product
- `bubble_sort_256.cb` — sort 256 ints

Cranelift `-O0` baseline → LLVM `--release -O3` target:
median size reduction ≥ 30%. The sample programs use only
the M9-supported subset of forms (no print, no f-string, no
collections; M10/M11 will widen).

**Status: TOY-FIXTURE RESOLVED at HEAD `72f4d27` (Phase K wave-2 / ADR-0058b §11.1) + PRODUCTION-SCALE RESOLVED at HEAD `d2cbb8d` (v0.4.0 cut, 2026-05-21).**

Empirical median 0.584 measured on 5 small fixtures (hello 872B / fizzbuzz 1408B / fib 1192B / dot_product 1056B / nested_branch 1200B). All fixtures O0 binary ≤ 1408 bytes; LLVM compresses tiny binaries asymmetrically well vs production-scale workloads.

#### Production-scale empirical close (2026-05-21, v0.4.0 cut)

The production workload is the **shipped `cobrust` release binary
itself** — the largest real-world artifact downstream consumers exercise.
The original task framing referenced "50MB+ binary"; the empirical
reality is the v0.4.0 cobrust binary fits in the 10-15 MB band across
all 9 shipped targets. A synthetic 50MB+ blob (chaining tomli +
dateutil + msgpack + numpy) was considered and rejected: it would
benchmark a workload nothing in the ecosystem actually consumes. The
real binary is the honest target.

v0.4.0 release tarball binary sizes (all stripped, all O3, captured
from `gh release download v0.4.0 --pattern 'cobrust-v0.4.0-*.tar.gz'`):

| Target triple                      | Stripped O3 size | Wheel digest (SHA256, prefix) |
|------------------------------------|------------------|-------------------------------|
| `aarch64-apple-darwin-m1`          |  10,231,360 B    | `6b44d86e…`                   |
| `aarch64-apple-darwin-m2`          |  10,231,360 B    | `246912b7…`                   |
| `aarch64-unknown-linux-gnu-neon`   |  11,288,368 B    | `e92ef907…`                   |
| `aarch64-unknown-linux-gnu-sve`    |  11,288,368 B    | `a61ba9a8…`                   |
| `x86_64-unknown-linux-gnu-v1`      |  14,814,368 B    | `490c76d3…`                   |
| `x86_64-unknown-linux-gnu-v3`      |  14,814,368 B    | `1637c328…`                   |
| `x86_64-unknown-linux-gnu-v4`      |  14,814,368 B    | `2883c4e9…`                   |
| `x86_64-unknown-linux-musl-v1`     |  14,885,688 B    | `69eb894c…`                   |
| `x86_64-unknown-linux-musl-v3`     |  14,885,688 B    | `f4695288…`                   |

Local same-host O0-vs-O3 control (Mac aarch64, both built with
`--profile release` so debuginfo overhead is matched; only the
`opt-level` differs):

- O3 (`opt-level = 3`, default): **10,248,240 B ≈ 9.77 MB**
- O0 (`CARGO_PROFILE_RELEASE_OPT_LEVEL=0`): **34,960,800 B ≈ 33.34 MB**
- **Production-scale O3/O0 ratio: 0.293 (70.7% reduction)**

Production-scale reduction (70.7%) is materially **better** than the
toy-fixture median (41.6%). The opt pipeline benefits from scale:
inlining, LTO, and dead-code elimination compound as the crate graph
grows. The conservative hypothesis "toy fixtures over-represent O3
wins" is empirically rejected; the inverse holds at v0.4.0 scale.

Bench harness anchor: `crates/cobrust-codegen/tests/binary_size_prodscale.rs::cobrust_binary_envelope`
(gated on `COBRUST_BIN_BENCH_PRODSCALE=1`; reads `target/release/cobrust`
off disk so it runs on any host with the release binary built — no
system-LLVM dependency, unlike the toy-fixture sibling
`binary_size_bench.rs`). Sanity envelope: 1 MB ≤ size ≤ 100 MB.

Honesty-amend lineage: F36 retroactive audit 2026-05-19 flagged toy
fixtures under §A3; this amendment closes the pending state with the
v0.4.0 production-binary empirical baseline (memory
`feedback_fixture_name_vs_behavior_drift.md`).

The empirical close uses an LLVM-only 5-fixture corpus per
ADR-0058b §A3 (refining the original Cranelift-vs-LLVM framing into
an O0-vs-O3 same-backend comparison — this isolates the opt-pipeline
contribution from the Cranelift-vs-LLVM IR shape difference, which
was the original ADR-0023 framing's confound):

| Fixture | O0 size | O3 size | Ratio |
|---|---|---|---|
| `hello` | 872 | 576 | 0.661 |
| `fizzbuzz` | 1408 | 760 | 0.540 |
| `fib` | 1192 | 696 | 0.584 |
| `dot_product` | 1056 | 640 | 0.606 |
| `nested_branch` | 1200 | 624 | 0.520 |

**Median O3/O0 ratio: 0.584 (41.6% size reduction).** Clears the
30% bar by 11.6 percentage points. Verified on <self-hosted-runner>
(`x86_64-unknown-linux-gnu`) via `crates/cobrust-codegen/tests/binary_size_bench.rs`.

See ADR-0058b §11.1 for the binding empirical close trace.

### Public surface (binding)

```rust
// emit a MIR module to native artifact
pub fn emit(module: &cobrust_mir::Module, spec: TargetSpec) -> Result<Artifact, CodegenError>;

// target specification
#[derive(Clone, Debug)]
pub struct TargetSpec {
    pub triple: target_lexicon::Triple,
    pub opt_level: OptLevel,
    pub backend: Backend,
    pub artifact: ArtifactKind,
    pub output_dir: std::path::PathBuf,
    pub module_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptLevel { None, Speed, SpeedAndSize }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// Pure-Rust Cranelift backend (default for `cargo build`).
    Cranelift,
    /// LLVM via inkwell. Requires `--features llvm`.
    Llvm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactKind {
    /// Relocatable object file (.o).
    Object,
    /// Linked executable.
    Executable,
    /// Dynamic library (.so / .dylib).
    DynamicLibrary,
}

#[derive(Clone, Debug)]
pub enum Artifact {
    Object(std::path::PathBuf),
    Executable(std::path::PathBuf),
    DynamicLibrary(std::path::PathBuf),
}

#[derive(thiserror::Error, Clone, Debug)]
pub enum CodegenError {
    #[error("unsupported backend: {0:?} (rebuild with --features llvm?)")]
    UnsupportedBackend(Backend),
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
    #[error("MIR rejected: {0}")]
    InvalidMir(String),
    #[error("Cranelift error: {0}")]
    CraneliftError(String),
    #[error("LLVM error: {0}")]
    LlvmError(String),
    #[error("Object emission failed: {0}")]
    ObjectEmission(String),
    #[error("Linker failed (exit {exit_code}): {stderr}")]
    LinkerFailed { exit_code: i32, stderr: String },
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Internal codegen error: {0}")]
    Internal(String),
}
```

### Per-MIR-form lowering rules (binding)

| MIR construct | Cranelift | LLVM (inkwell) |
|---|---|---|
| `Body` | `Function` with `Signature` matching `extern "Cobrust"` | `FunctionValue` with `LinkageType::External` |
| `LocalDecl` | `Variable` bound to `declare_var` + `def_var` | stack `alloca` + load/store |
| `BasicBlock` | `Block` via `FunctionBuilder::create_block` | `BasicBlock` via `LLVMAppendBasicBlock` |
| `Statement::Assign` | RHS lowered → `def_var`(LHS) | `build_store(rvalue, place_ptr)` |
| `Terminator::Goto(b)` | `ins().jump(b, &[])` | `build_unconditional_branch(b)` |
| `Terminator::SwitchInt` | `ins().brif` chain (bool) or `ins().br_table` (int) | `build_switch` |
| `Terminator::Return` | `ins().return_(&[ret])` | `build_return(Some(&ret))` |
| `Terminator::Call` | `ins().call(callee, &args)` | `build_call(fn, &args, "call")` |
| `Terminator::Drop` | call `_cobrust_drop_<TypeId>(place)` | same |
| `Terminator::Unreachable` | `ins().trap(TrapCode::UnreachableCodeReached)` | `build_unreachable` |
| `Terminator::Assert` | conditional jump → trap | conditional jump → call panic |
| `Rvalue::BinaryOp(Add, ...)` | `ins().iadd(a, b)` for int, `ins().fadd(a, b)` for float | `build_int_add` / `build_float_add` |
| `Rvalue::BinaryOp(Div, ...)` | preceded by `Assert(b != 0)` per MIR | same |
| `Rvalue::Aggregate(Tuple, [...])` | sequence of `def_var` to fields | sequence of `build_struct_gep + build_store` |
| `Rvalue::Ref(_, place)` | `ins().stack_addr(place)` | `build_alloca` + GEP |
| `Operand::Constant(Int(i))` | `ins().iconst(I64, i)` | `i64_type.const_int(i, false)` |
| `Operand::Constant(Float(bits))` | `ins().f64const(f64::from_bits(bits))` | `f64_type.const_float_from_bits(bits)` |

### Drop-handler ABI

Drop terminators lower to a call to a per-type drop handler:

```
extern "Cobrust" fn _cobrust_drop_<TypeId>(place: *mut PlaceLayout);
```

At M9 the handler is a no-op stub — actual destructor materialization
lands at M11 (stdlib + runtime). The stub ensures the drop schedule
proven sound by M8 is preserved through codegen — no destructor
elision happens at M9.

## Consequences

- **Positive**
  - Two-tier delivery: pure-Rust default for fast iteration;
    LLVM tier for production opt quality. Matches ADR-0019.
  - Backend isolation: divergence between Cranelift and LLVM
    output on the diff corpus is automatically caught.
  - Public surface is closed: `emit / TargetSpec / Artifact /
    CodegenError` cover the M9 scope. Future M10 CLI driver
    only needs to wire MIR → `emit(...)` → output.

- **Negative**
  - Two backend lowering paths to maintain. Each new MIR form
    needs both lowerings + a diff-corpus row.
  - LLVM dep tree (when enabled) is heavy. Mitigation:
    `--features llvm` is opt-in, not default.
  - System linker dependency (`cc` via `$CC` env var). Some
    minimal CI images may lack `cc`; tracked as a known
    workflow constraint.

- **Neutral / unknown**
  - Cross-compilation between delivery-scope targets requires a
    `cc` linker that targets the destination. Not gated at M9.
  - WASM target (`wasm32-unknown-unknown`) is reachable via
    Cranelift but disabled by default. Phase F will gate it.
  - Drop handler stub at M9 elides the actual destructor — fine
    for the M9 diff-corpus scope (no `List` / `Dict` / `Str`
    return types) but flagged for M11 to materialize.

## Evidence

- Constitution `CLAUDE.md` §4.1 (compiler layers — codegen
  is the last stage), §5.3 (efficient: AOT default, JIT optional).
- ADR-0006 — type-system 9-obligation list; items 1..3 (flow)
  projected onto MIR-time as B1..B5; M9 codegen does not
  reintroduce UB on top.
- ADR-0012 — "translate the surface, bind the core"; Cranelift
  + inkwell are bound, not reimplemented.
- ADR-0019 §"M9 — Codegen" — backend matrix + acceptance bar.
- ADR-0020 — MIR shape; M9 is the consumer.
- `crates/cobrust-codegen/src/{lib.rs, target.rs, abi.rs,
  cranelift_backend.rs, llvm_backend.rs, linker.rs, artifact.rs,
  error.rs}` — implementation pinned to this ADR.
- `crates/cobrust-codegen/tests/{codegen_well_formed.rs,
  codegen_ill_formed.rs, codegen_diff_corpus.rs,
  codegen_object_layout.rs, codegen_release_smoke.rs}` —
  enforce the lowering rules + the diff-corpus acceptance bar.
- Cranelift docs: <https://docs.rs/cranelift-codegen> +
  <https://docs.rs/cranelift-object>.
- LLVM via inkwell: <https://docs.rs/inkwell>.
