---
name: f58
status: RESOLVED
family: F53-sibling
date: 2026-05-27
last_verified_commit: d276076
---

# F58 — LLVM `target_cpu="native"` passed verbatim aborts on cloud x86_64 (X.3 flip surface)

## §1 Context

Surfaced 2026-05-27 during ADR-0070 §X.3 LLVM-default follow-up. CI `cargo test
(ubuntu-latest)` aborted the whole workspace test run with a hard LLVM error:

```
LLVM ERROR: 64-bit code requested on a subtarget that doesn't support it!
error: test failed, to rerun pass `-p cobrust-codegen --test tier2_target_cpu_smoke`
```

`tier2_target_cpu_smoke` passed 3/3 locally on macOS aarch64 but aborted on the
GH Actions ubuntu-latest (x86_64) runner — an environment-specific divergence
masked because the macOS path happened to be benign.

## §2 Root cause

`build_target_machine` (llvm_backend.rs §625) handled the Tier-2 `target_cpu`
field as:

```rust
let cpu = spec.target_cpu.as_deref().unwrap_or("generic");
target.create_target_machine(&triple, cpu, "", opt, ...)
```

The doc comment claimed `"native"` "asks LLVM to auto-detect the host CPU" — **this
was false**. LLVM's `create_target_machine` (and the underlying
`LLVMCreateTargetMachine`) does NOT interpret the literal string `"native"`; only
front-end tools (clang/llc) resolve it by calling `sys::getHostCPUName()`
themselves. Passing `"native"` verbatim yields an "unknown CPU" subtarget with an
empty feature string.

- On macOS aarch64: the unknown-CPU fallback lands on a 64-bit-capable generic
  Apple subtarget → benign, test passes.
- On the ubuntu x86_64 runner: the unknown-CPU + empty-features subtarget loses
  64-bit mode, so when 64-bit code is requested LLVM aborts the process.

This is a real user-facing bug, not merely a test artifact: `cobrust build
--release --target-cpu=native` (Tier-2 host-tuning mode,
numerical-compute-hardware-tiering.md §Tier 2) would abort on those CPUs.

## §3 Resolution

Expand `"native"` ourselves via LLVM's host-detection helpers so LLVM receives a
recognised CPU name + an explicit feature string carrying 64-bit mode:

```rust
let (cpu, features): (String, String) = match spec.target_cpu.as_deref() {
    Some("native") => (
        TargetMachine::get_host_cpu_name().to_string(),
        TargetMachine::get_host_cpu_features().to_string(),
    ),
    Some(name) => (name.to_string(), String::new()),
    None => ("generic".to_string(), String::new()),
};
target.create_target_machine(&triple, &cpu, &features, opt, ...)
```

Named CPUs (`"skylake"`, `"apple-m1"`, …) still pass verbatim with empty features;
`None` keeps the `"generic"` baseline (pre-Tier-2 behaviour, unchanged).

## §4 Verification

- `cargo test -p cobrust-codegen --test tier2_target_cpu_smoke` — 3/3 PASS (macOS
  aarch64, LLVM 18).
- CI `cargo test (ubuntu-latest)` re-run is the authoritative oracle for the abort
  fix (cannot reproduce the abort on the macOS host).

## §5 Lineage / siblings

- Sibling of F53 / F54 / F55 / F56: all latent llvm-gated gaps surfaced by the
  §X.3 LLVM-default flip acting as a detection gate (the flip routes every
  `cobrust build` + every default-backend test through LLVM, exposing paths that
  the previous Cranelift-default never exercised).
- Reinforces F35-sibling discipline: the misleading doc comment ("auto-detect the
  host CPU") described intended behaviour that the code never implemented — claim
  vs. landed-behaviour drift.
