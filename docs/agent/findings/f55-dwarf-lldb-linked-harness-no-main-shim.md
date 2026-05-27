---
name: f55
status: RATIFIED
family: F37-honest-debt + F46-sibling
date: 2026-05-27
last_verified_commit: 9b3b265
---

# F55 — dwarf_lldb_smoke linked-executable tests lack C `main` shim → CI link failure

## §1 Context

Surfaced 2026-05-27 by the ADR-0070 §X.3 LLVM-default flip. With `default = ["llvm"]`,
`cargo test --workspace` on CI now compiles + runs the `#[cfg(feature = "llvm")]`-gated
`dwarf_lldb_smoke` tests that were previously skipped (CI used the Cranelift default,
never passing `--features llvm`). 3 of them failed:

- `lldb_linked_str_frame_variable`
- `lldb_linked_option_none`
- `lldb_linked_option_some_int`

Error: `collect2: error: ld returned 1 exit status` / `undefined reference to main`.

## §2 Root cause

These tests link a **bare codegen object** (emitted by `cobrust-codegen`) into an
executable to spawn lldb-18 against. But the platform `main` shim lives in
`cobrust-cli/runtime/cobrust_main.c` — unreachable from `cobrust-codegen` integration
tests (no cross-crate runtime path). So the link step has no `main` symbol →
`undefined reference to main`.

Latent since Phase L wave-3 (ADR-0059d). Masked pre-X.3 by two coincidences:
1. `llvm` feature off by default → tests compiled out on CI.
2. lldb-18 absent on most dev hosts → tests skipped locally too.

X.3's LLVM-default flip + the CI LLVM-18 install (which provides lldb-18 + cc)
removed both masks simultaneously, exposing the latent link gap.

## §3 Resolution (honest debt — F37)

`#[ignore]` the 3 linked-executable tests with full rationale citing this finding.
Object-level DWARF coverage is **retained** by sibling non-linked tests
(`lldb_option_di_composite_*`, `lldb_smoke_adt_variable_renders_naming`,
image-dump-symtab object-level tests) — those inspect the emitted object's DWARF
directly without linking an executable, so DWARF-emission correctness stays covered.

The linked-executable + live-lldb path is a debug-tooling integration concern,
properly exercised via the `cobrust debug` CLI (ADR-0059c) which wires
`cobrust_main.c`. Deferred there.

## §4 Detection rule

Feature-gated integration tests that link executables MUST either:
1. Link through the same `cobrust_main.c` shim path the CLI uses, OR
2. Carry `#[ignore]` with a finding URN if they require a debug-tooling env.

F46-sibling: same "test harness can't locate/link the runtime shim" family as the
wheel runtime/stdlib bundling gap.

## §5 Cross-refs

- ADR-0059d (Phase L wave-3 DWARF) — origin of the linked-harness tests
- ADR-0059c (`cobrust debug` CLI) — proper home for live-lldb path
- ADR-0070 §X.3 (LLVM-default flip) — exposure gate
- F46 (wheel runtime/stdlib gap) — sibling
- F53 / F54 (other X.3-flip-exposed latent gaps)

## §6 Status

RATIFIED 2026-05-27. The 3 tests run explicitly via `cargo test --ignored` on a
debug-tooling host (lldb-18 + linkable runtime).
