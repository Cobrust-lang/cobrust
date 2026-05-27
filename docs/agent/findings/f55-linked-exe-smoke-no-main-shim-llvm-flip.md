---
finding_id: F55
title: dwarf_lldb_smoke linked-executable harness links bare object with no C `main` shim — surfaced by ADR-0070 §X.3 LLVM-default flip
status: open (3 tests #[ignore]'d with deferred-fix cite per F37 discipline)
date: 2026-05-27
severity: medium
siblings: [F53, F54, F37, F49]
last_verified_commit: 81cfc1f
---

# F55 — linked-executable lldb smoke tests have no `main` shim

## Symptom

After ADR-0070 §X.3 flipped `cobrust-codegen` `default = ["llvm"]` (main HEAD
`66057a4`), the GitHub Actions `cargo test` job went red on **both** ubuntu-latest
and macos-latest. Build + clippy jobs are GREEN (LLVM-18 install, CI commit
`89d30e4`, works). The 3 failing tests:

- `lldb_linked_str_frame_variable`
- `lldb_linked_option_none`
- `lldb_linked_option_some_int`

all in `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs`, fail at the LINK step:

```
/usr/bin/ld: Scrt1.o: in function `_start': undefined reference to `main'
collect2: error: ld returned 1 exit status
emit linked executable: LinkerFailed { exit_code: 1, ... }
```

## Root cause

The three tests use `build_linked_executable(body)` →
`emit(module, ArtifactKind::Executable)` → `linker::link(object, ...)`. This links
the **bare codegen object alone**.

The platform C `main(argc, argv)` entry point is NOT emitted by the codegen
backend. Per ADR-0025 §G, a user top-level `main` body is exported as the symbol
`_cobrust_user_main`; the real `main` comes from a C runtime shim
(`cobrust-cli/runtime/cobrust_main.c`) that the **CLI** link step compiles and
links in. That shim is unreachable from a `cobrust-codegen` integration test, so
the linked object has no `main` symbol → `ld` failure.

The test fixtures (`option_some_int_smoke` etc.) are not even named `main`, so no
`_cobrust_user_main` alias is produced either; the harness was never capable of
producing a runnable executable.

## Why it only surfaced now

The whole file is `#![cfg(feature = "llvm")]` (line 22). Three masking layers
hid the latent bug since Phase L wave-3:

1. **Pre-X.3 CI** ran Cranelift-default → `llvm` feature OFF → the file compiled
   to **0 tests**. (The X.3 attempt commentary in ADR-0070 §X.3 noted exactly
   this "compiles to 0 tests" property and mis-classified the whole file as
   environment-gated.)
2. **Dev hosts (Mac)** lack `lldb-18` / `lldb` on PATH → `find_lldb()` returns
   None → all 3 tests SKIP (early `return`).
3. The tests also gate on `linker::linker_available()`.

The §X.3 flip turned the `llvm` feature ON in CI. The CI Ubuntu runner's apt
`llvm-18` package provides `lldb-18`, and `cc` is always present — so **both**
gates pass for the first time ever, the tests execute, and the latent link bug
fires. (macOS CI runner likewise: brew `llvm@18` ships `lldb` + `cc`.)

This is a direct F53/F54 sibling: a stub/incomplete path masked by the
Cranelift-default that the LLVM flip exposed. The flip is the detection gate
working as intended (CLAUDE.md §2.5 compile-time/CI-catch-errors).

## Resolution (this commit)

Per F37 discipline (no silent rot; `accepted_as_honest_debt` MUST cite a specific
`#[ignore = "reason; deferred to X"]`), the 3 tests are `#[ignore]`'d with a
full-rationale reason string pointing here and to the deferred fix path. The
object-level DWARF/symtab assertions they intended are already covered by sibling
non-linked tests (`lldb_option_di_composite_type_fields`,
`lldb_smoke_*`, `image dump symtab` object-level variants), so coverage is not
lost — only the (broken) linked-executable round-trip is parked.

## Real fix (deferred)

The linked-executable round-trip belongs with the ADR-0059c `cobrust debug` CLI
path, which already wires `cobrust_main.c`. Options:

- (a) Move the linked-executable smoke tests into a `cobrust-cli` integration
  test that links the shim (preferred — the shim lives there).
- (b) Add a minimal in-test C `main` stub compiled + linked alongside the object
  in `build_linked_executable`.
- (c) Have the codegen backend optionally synthesize a trivial `main` when
  `ArtifactKind::Executable` and no user `main` body exists (changes backend
  contract — needs ADR).

Tracked as ADR-0070 §X.6 follow-on.

## Prevention

- CI gate candidate: a smoke that runs `cargo test -p cobrust-codegen --features
  llvm` on a runner WITH lldb present would have caught this pre-flip. The X.3
  flip itself is now that gate.
- Lesson (F53/F54/F55 triad): every `#![cfg(feature = "llvm")]` test file is a
  blind spot until the flip; audit all such files for paths that depend on
  CLI-only runtime shims.
