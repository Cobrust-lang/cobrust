---
doc_kind: adr
adr_id: 0075
title: RISC-V + WebAssembly target enablement — scope, phasing, decision points
status: proposed
date: 2026-05-28
last_verified_commit: 0010653
relates_to: [adr:0023, adr:0058b, adr:0070, "claude.md:§2.2"]
---

# ADR-0075: RISC-V + WebAssembly target enablement

## 1. Context

User directive 2026-05-28:「RV WASM 的完整支持也提上日程」(put RV + WASM full
support on the agenda). Two motivations align with v0.7.0:

- **RISC-V**: dora-cb robotics path (Stream Y) — SiFive HiFive Unmatched, Espressif
  ESP32-C/H, BeagleV all run riscv64-linux-gnu; embedded robotics work increasingly
  picks RV over ARM.
- **WebAssembly**: browser-side robotics tooling, sandboxed `.cb` programs, the
  natural deploy target for the LLM-router web demo + future doc playground.

## 2. Current state — what already works "for free"

- `TargetSpec.triple: target_lexicon::Triple` (`crates/cobrust-codegen/src/target.rs:17`)
  accepts any LLVM triple. LLVM 18 has `riscv32` / `riscv64` / `wasm32` backends
  enabled by default; `Target::from_triple` succeeds for them.
- ADR-0058b §"Multi-target" notes: "the wave-1 emit path is functionally correct for
  cross-targets when the underlying LLVM toolchain supports the cross-target"
  (docs/agent/adr/0058b-…:129). So the **codegen layer is target-agnostic** by design.
- F61 (platform-invariant `Architecture::Unknown` probe) was added 2026-05-27 — confirms
  cross-target handling at the typecheck layer is there.

What is **NOT** there:
1. **Rust target std for cross**: `rustup target add` not run for `riscv64gc-unknown-linux-gnu`
   nor `wasm32-wasip1`. `cobrust-stdlib` (the static archive linked into every `.cb`
   binary) is built only for the host target today.
2. **C runtime shim** (`cobrust_main.c`): single platform-`main(argc, argv)` body. Linux
   ABI assumed; WASM/WASI has `_start` entrypoint convention; bare-metal riscv32 has
   none.
3. **Cross linker**: `cobrust-cli/src/build.rs:283-316` invokes `cc` (the host C
   compiler). riscv64-linux-gnu needs `riscv64-linux-gnu-gcc` / `lld`; wasm needs
   `wasm-ld`.
4. **Test harness**: integration tests run the compiled binary natively. RV needs
   QEMU user-mode (`qemu-riscv64`); WASM needs `wasmtime` (WASI) or wasm-bindgen-test
   (browser).
5. **CI matrix**: 5 jobs today (ubuntu/macos × {build, test, clippy} + audit + udeps).
   Cross matrix expansion needed.

## 3. Decision — phased, scoped to "functional subset"

### Scope WITHIN v0.7.0 (proposed)

- **Phase 1 — `riscv64gc-unknown-linux-gnu`**: cross-compile + QEMU-user run of the LC-100
  corpus. Linux + glibc + std + Rust target tier-2 — well-trodden. Ecosystem libs
  (den/coil/pit/strike/scale/molt/nest/hood) cross-compile too (rusqlite / time / serde
  all support riscv64). Done-means: a `.cb` program built with
  `--target riscv64gc-unknown-linux-gnu` runs under `qemu-riscv64` and prints "pong" /
  reads from den / etc. CI riscv64 job runs the LC-100 corpus subset.
- **Phase 2 — `wasm32-wasip1`** (the modern WASI preview-1 target): WASM with WASI
  filesystem + stdio. Computation-only `.cb` programs work end-to-end. Network ecosystem
  libs (pit, strike) are **explicitly out of scope** (no socket API in WASI p1);
  den/json/scale/molt should work. Done-means: `.cb` "hello world" + `.cb` numpy-on-coil
  small program runs under `wasmtime`. CI wasm32-wasip1 job exists.

### Scope POST-v0.7.0 (acknowledge, defer)

- **`wasm32-unknown-unknown`** (no WASI — browser host): I/O via JS imports. Needed for
  the future doc playground. Requires a JS shim runtime. Sub-ADR after v0.7.0.
- **`riscv32-unknown-none-elf`** (bare metal embedded): no std, no allocator beyond what
  Cobrust ships; entire stdlib model needs `no_std` rework. Sub-ADR if/when dora-cb
  demands it.
- **`wasm32-wasip2`** (component-model WASI): too young (WASI Preview 2 still
  stabilising), revisit when wasmtime ships stable p2 by default.

## 4. Q1–Q5 open questions for user

- **Q1 — v0.7.0 envelope**: with Stream X (LLVM-default) + cobra-rebrand + `.cb`
  ecosystem-import chain (6/8 modules done) + Z.8 REST demo + dora-cb + numpy +
  housekeeping all already in v0.7.0 scope, is **Phase 1 (riscv64) sufficient for
  v0.7.0** and Phase 2 (wasm32-wasip1) a v0.7.x add-on? Or push both into v0.7.0?
  Phases 1+2 estimated ~3 work-weeks combined; the v0.7.0 envelope is getting tight.
- **Q2 — single concurrency runtime under WASM**: CLAUDE.md §2.2 mandates a single
  structured-concurrency runtime (tokio currently). WASM has **no threads** in baseline
  WASI; the `task::spawn` surface either degrades to single-threaded async (Rust's
  `wasm-bindgen-futures` pattern) or is rejected at compile time on the WASM target.
  Recommend: **degrade silently** — `task::spawn` works but runs serially on the same
  JS/wasm event loop. Document divergence.
- **Q3 — CI cost**: adding riscv64-QEMU + wasm32-wasip1-wasmtime jobs adds ~10 minutes
  per CI run (cross-compile is heavier than native). With v0.7.0's already-slow workspace
  test, total run hits ~40 minutes. Acceptable, or gate behind `[ci-cross]` PR label?
- **Q4 — runtime ABI for handles cross-target**: opaque-pointer ABI (ADR-0072) uses
  `*mut u8` widths that vary (32-bit on riscv32/wasm32 vs 64-bit on riscv64). Verify
  pit/den/strike trampolines (ADR-0073) and the manifest signatures don't bake in
  pointer width. `Box::into_raw` / `from_raw` are pointer-width-correct in Rust but
  the codegen extern signatures (`*mut u8`) must reflect target width.
- **Q5 — Cobrust binary distribution**: do we ship `cobrust` (the CLI) as a riscv64
  binary too? Or only the `.cb`-program output? Recommend output only — the toolchain
  itself stays on traditional dev hosts. Embedded dev cross-compiles from
  desktop / CI.

## 5. Implementation per phase

### Phase 1 — riscv64gc-unknown-linux-gnu (~1.5 work-weeks)

1. **Toolchain prep**: `rustup target add riscv64gc-unknown-linux-gnu` documented in
   project README; the `riscv64-linux-gnu-gcc` cross compiler (Debian apt
   `gcc-riscv64-linux-gnu` / Homebrew `riscv64-elf-gcc` + sysroot) documented in
   `docs/agent/setup/cross-toolchain.md`.
2. **build.rs cross-mode**: `cobrust-cli/src/build.rs:283-316` learns a `--target` flag,
   routes to `riscv64-linux-gnu-gcc` (or `clang --target=riscv64-linux-gnu`) when set.
   `locate_stdlib_archive` (build.rs:459) reads from
   `target/riscv64gc-unknown-linux-gnu/<profile>/libcobrust_stdlib.a` instead of native
   `target/<profile>/`. Cross-build `cobrust-stdlib` via
   `cargo build -p cobrust-stdlib --target=riscv64gc-unknown-linux-gnu`.
3. **C shim cross-build**: `cobrust_main.c` already pure C — cross-compile with the same
   gcc. No code change, only build-system thread.
4. **Test harness**: a `runs_under_qemu` helper in `cobrust-cli/tests/` invokes
   `qemu-riscv64 -L /usr/riscv64-linux-gnu <binary>` and asserts stdout. Gate on
   `cfg!(target_arch = "x86_64")` || `cfg!(target_arch = "aarch64")` (any host) +
   `qemu-riscv64` available.
5. **CI job**: `cargo build --target=riscv64gc-unknown-linux-gnu --workspace` + a small
   LC-100 sub-corpus run under QEMU. Cache the riscv64 sysroot.
6. **F58-sibling check**: `target_cpu` Tier-2 (host CPU detection) gates on host arch
   today (`smoke_target_cpu_native`). Verify a non-Linux host + a riscv64 cross-target
   doesn't trip the F58 path; the cross-target's `target_cpu` should default to
   `"generic-rv64"` not host-resolved.

### Phase 2 — wasm32-wasip1 (~1.5 work-weeks)

1. **Toolchain prep**: `rustup target add wasm32-wasip1`; `wasm-ld` (ships with LLVM 18)
   + wasmtime documented.
2. **build.rs cross-mode**: when `--target wasm32-wasip1`, swap linker to `wasm-ld`,
   emit `.wasm` artifact (no `cc` invocation; LLVM-ld direct). Stdlib archive built for
   wasi target.
3. **C shim cross-build**: `cobrust_main.c` adapted with a `_start()` entrypoint for
   WASI convention (Cobrust's `_cobrust_user_main` is called from `_start` after WASI
   `args_get` / `args_sizes_get`). Net: a thin wasi-cobrust_main.c sibling, picked by
   target.
4. **Ecosystem exclusion**: pit (network) + strike (network) + hood (CLI argv via WASI)
   declared OUT for WASM in the ecosystem manifest. Attempting `import pit` on a wasi
   build emits a clear `TypeError::EcosystemUnavailableOnTarget { module: "pit", target:
   "wasm32-wasip1" }`. den + scale + molt + nest + json + coil tested.
5. **Test harness**: a `runs_under_wasmtime` helper invokes `wasmtime run <bin>.wasm`.
6. **CI job**: parallel to Phase 1's riscv64 CI job, just s/qemu/wasmtime/, s/riscv64gc-
   unknown-linux-gnu/wasm32-wasip1/.

### Combined risk surfaces

- **Drop discipline cross-target**: the ADR-0072 handle pattern + ADR-0073 trampoline
  Box::into_raw/from_raw all use pointer-width-correct Rust. Verify no `as i64`-style
  pointer casts that bake in 64-bit width.
- **WASM panic ABI**: ADR-0073 §3 Q5 mandates abort-on-panic (cross-C-ABI). WASM has no
  abort; `unreachable!()` instruction triggers trap. Map `__cobrust_panic` to `unreachable`
  on WASM.
- **macOS host × riscv64 target × LLVM brew**: Homebrew LLVM @18 may not bundle the
  riscv64 lld linker on every formula; verify or fall back to `riscv64-elf-gcc`.

## 6. Acceptance test (per phase)

- **Phase 1 done-means**: `cobrust build --target=riscv64gc-unknown-linux-gnu
  examples/pit_pong/main.cb -o blog`; `qemu-riscv64 -L /usr/riscv64-linux-gnu blog &`;
  `curl http://127.0.0.1:8080/ping` returns "pong" + 200. (pit works because riscv64
  Linux has full socket API; this is the strongest cross-target proof.) Plus LC-100 sub-
  corpus runs green.
- **Phase 2 done-means**: `cobrust build --target=wasm32-wasip1
  examples/z8_rest_blog/main.cb -o blog.wasm` REJECTS at typecheck with
  `EcosystemUnavailableOnTarget { module: "pit" }`. Switch to a pure-computation
  `.cb` program (`fib(30)`, `coil.eye(3).sum()`) → `wasmtime run` returns expected
  output. LC-100 sub-corpus (no I/O / no network) runs green under wasmtime.

## 7. Risks (top)

1. **CI matrix cost**: +10 min/run × 2 targets = +20 min/run. Mitigation per Q3:
   gate behind `[ci-cross]` label, or run cross only on `main` push not on PR.
2. **Stdlib `tokio` story under WASM**: tokio doesn't compile to WASI without
   significant feature surgery. Either swap to `tokio_with_wasm` / `wasm-bindgen-futures`
   on WASM, OR declare `task::spawn` unavailable on WASM. Recommend the latter for v0.7.0.
3. **F58 sibling on cross**: `target_cpu = native` host-detection has no meaning when
   cross-targeting. Must default to `generic-<arch>` for cross builds, ignoring host.
4. **Ecosystem manifest target-availability flags**: requires a new `available_on:
   Vec<TargetMatcher>` per ecosystem entry. ADR-0072 manifest grows.

## 8. Recommended plan (CTO call, awaiting user Q1 confirmation)

- **v0.7.0**: Phase 1 (riscv64) — strong, achievable, aligns dora-cb.
- **v0.7.x or v0.8.0**: Phase 2 (wasm32-wasip1) — alone or paired with the doc
  playground.
- Phases 3 + 4 (wasm32-unknown, riscv32-none) — opportunistic, no commitment.

If user confirms Phase 1 in v0.7.0:
1. Sprint A — toolchain bootstrap + cross-build cobrust-stdlib (~3 days).
2. Sprint B — build.rs cross-mode + qemu test harness (~3 days).
3. Sprint C — CI matrix add + LC-100 sub-corpus baseline (~2 days).
4. Audit + ratify + push.

If user pushes both into v0.7.0:
- Add Sprints D/E/F for Phase 2 (~7 days incremental).

## 9. Task tracking

A new umbrella task #152 will be created post-this-ADR ratification:
"v0.7.0 RV+WASM enablement — Phase 1 (riscv64) MUST-ship, Phase 2 (wasi) stretch".
