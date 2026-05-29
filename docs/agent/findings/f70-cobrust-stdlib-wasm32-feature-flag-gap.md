---
finding_id: F70
title: cobrust-stdlib wasm32 feature-flag matrix gap (mimalloc/tokio/llm-router default trio incompatible with wasm32-wasip1)
status: candidate
date: 2026-05-29
discovered_during: "ADR-0075 Phase 2 Sprint D (446016c) — flagged but deferred; resolved-in-part by Sprint E"
related: [F61, F60, F66, F67]
---

# F70 — cobrust-stdlib wasm32 feature-flag matrix gap

## Summary (one line)

`cobrust-stdlib`'s default feature trio (`mimalloc-alloc` + `tokio-runtime` +
`llm-router`) does not compile for `wasm32-wasip1`; the hello-world path
needs none of them, so Sprint E ships hello-world live by building the
stdlib `--no-default-features`, and DEFERS the full feature-on-wasm matrix
(the `task::spawn` silent-degrade + the network-module target gate) to a
future sprint.

## What happened

### The two distinct blockers along the wasm32 path

ADR-0075 Phase 2 Sprint D (446016c) wired the build.rs cc/linker plumbing
plus a `wasm32-cross-smoke` CI job, but the first live CI run
([26595424952](https://github.com/Cobrust-lang/cobrust/actions/runs/26595424952))
failed. The Sprint D agent's report flagged a feature-matrix gap (this F70)
but deferred writing the finding. Investigating Sprint E split the path into
**two independent issues**:

**Blocker A — wasi-sysroot (the actual Sprint D CI break; resolved by Sprint E).**
The `wasm32-cross-smoke` job step ordering:

```
Cross-build cobrust-stdlib (wasm32-wasip1, --no-default-features) … SUCCESS
Build host cobrust-cli ……………………………………………………………… SUCCESS
cross_compile_wasm32_e2e (live) …………………………………………… FAILURE
```

The failure was NOT in the stdlib cross-build (that step was green). It was
the `cobrust build` runtime-helper cross-cc step:

```
In file included from /usr/lib/llvm-18/lib/clang/18/include/stdint.h:52:
/usr/include/stdint.h:26:10: fatal error: 'bits/libc-header-start.h' file not found
cobrust build: runtime-helper compilation failed via `clang-18`
  (cross-target: Some("wasm32-wasip1"))
```

Root cause: apt's `clang-18` (`/usr/lib/llvm-18`) invoked with
`--target=wasm32-wasip1` but NO wasi-sysroot falls back to the host glibc
include path (`/usr/include/stdint.h`), which pulls the glibc-only
`bits/libc-header-start.h` that has no wasm equivalent. The ADR-0075 §5
Phase 2 / cross-toolchain.md §6 assumption — "LLVM 18's
`clang --target=wasm32-wasip1` bundles the wasi-libc sysroot automatically"
— is **false for the apt clang-18 distribution**. It holds only for clang
builds that ship a wasi-sysroot (e.g. the wasi-sdk's own clang). This is the
analogue of F66 (RISC-V triple normalization) + F67 (cross-link wiring): a
"make cross live" gap surfaced only by the live CI run, not by the
host-side skip-gated test.

**Blocker B — the feature-flag matrix (this finding's core; partially deferred).**
`cobrust-stdlib`'s `default = ["mimalloc-alloc", "tokio-runtime", "llm-router"]`
(crates/cobrust-stdlib/Cargo.toml:20) pulls three transitive trees that do
not build for `wasm32-wasip1`:

| Feature | Pulls | Why it fails on wasm32-wasip1 |
| --- | --- | --- |
| `mimalloc-alloc` | `mimalloc` (C `mimalloc` via `cc`) | Native allocator with thread-local + OS-page-management C code; no wasm32-wasip1 build. Installed as the `#[global_allocator]` in `runtime.rs:18-20`. |
| `tokio-runtime` | `tokio` (full) → `mio` | `mio` is an epoll/kqueue/IOCP socket reactor; WASI preview 1 has no socket syscalls. Gates `task` + `sync` modules. |
| `llm-router` | `cobrust-llm-router` → `reqwest`/`hyper` → TLS + sockets | Network stack; no WASI p1 socket API. Gates the `llm` module. |

The mitigation already in place since Sprint D: the CI job builds the stdlib
with `--no-default-features`. That works because the hello-world path
(`print(s)` → `io::print` → `std::io::stdout().lock().write_all()`) needs
NONE of the three:

- `runtime.rs`'s mimalloc `#[global_allocator]` is `#[cfg(all(feature =
  "mimalloc-alloc", not(feature = "system-alloc")))]` — disabled under
  `--no-default-features`, so wasm falls back to the default Rust/wasi
  allocator. The C-ABI `__cobrust_alloc`/`__cobrust_dealloc` route through
  `std::alloc`, which is allocator-agnostic.
- `task`/`sync` modules are `#[cfg(feature = "tokio-runtime")]` (lib.rs:138-141)
  — not compiled.
- `llm` module is `#[cfg(feature = "llm-router")]` (lib.rs:119-120) — not
  compiled.
- The remaining non-gated modules (`io`, `string`, `collections`, `math`,
  `panic`, `env`, `fmt`, `json`, `iter`, `array`, `runtime`, `prompt`,
  `tool`) use only `std::io` / `std::alloc` / `std::ffi` / `serde` /
  `indexmap` — all of which build for `wasm32-wasip1`.

## What Sprint E resolved

1. **wasi-sysroot in CI (Blocker A).** The `wasm32-cross-smoke` job now
   downloads + SHA256-verifies + extracts `wasi-sdk-25` (a version-matched
   clang 19 + a wasm32-wasip1 sysroot under `share/wasi-sysroot`). It sets
   `WASI_SDK_PATH`, `COBRUST_WASI_SYSROOT`, and
   `COBRUST_CC_WASM32_WASIP1=<sdk>/bin/clang` so the cross-cc is the SDK's
   bundled clang (sidesteps the apt-clang-18-without-sysroot trap), and
   build.rs additionally appends `--sysroot`.
   - SHA256 pinned: `52640dde13599bf127a95499e61d6d640256119456d1af8897ab6725bcf3d89c`
     (wasi-sdk-25.0-x86_64-linux.tar.gz, 114450290 bytes; no official
     SHA256SUMS published upstream, so pinned from a verified local
     download).

2. **`--sysroot` plumbing in build.rs (Blocker A).** New
   `resolve_wasi_sysroot(triple)` reads `$COBRUST_WASI_SYSROOT` then
   `$WASI_SDK_PATH` (deriving `<SDK>/share/wasi-sysroot`); errors with a
   fix-shaped message (CLAUDE.md §2.5-B) when neither is set or the path is
   absent. `select_cc_resolved` folds `--sysroot=<path>` onto every wasm
   cc-resolution branch. The host path is provably unaffected
   (`host_target_never_gets_wasi_sysroot` unit test).

3. **Feature-matrix mitigation kept + documented (Blocker B partial).** The
   `--no-default-features` stdlib cross-build is retained and the
   hello-world path's allocator/threads/socket independence is now
   explicitly documented (cross-toolchain.md §6.4 + this finding). No
   stdlib source `#[cfg(target_arch = "wasm32")]` guards were needed for
   hello-world — the existing Cargo-feature `#[cfg]`s already exclude every
   wasm-incompatible module when features are off.

4. **Skip-gate hardened.** `cross_compile_wasm32_e2e` now probes
   `wasi_sysroot_available()` (same env vars build.rs reads) so it skips
   cleanly on a dev host without a wasi-sdk (e.g. macOS) instead of failing
   mid-build.

## What is DEFERRED (a future sprint, NOT in Sprint E scope)

Sprint E's done-means is **hello-world live**, not full
ecosystem-on-wasm. The following remain open:

1. **Default-features-on-wasm32 build.** Making
   `cargo build -p cobrust-stdlib --target=wasm32-wasip1` (WITHOUT
   `--no-default-features`) succeed. Options, in increasing effort:
   - **(a) Per-target default features.** Cargo has no native
     `[target.'cfg(...)'.features]` for *defaulting* features by target
     (only deps). A `build.rs`-driven cfg, or a `wasm` feature that the CLI
     auto-selects, or simply documenting "`--no-default-features` is the
     wasm contract", is required. Recommendation: have the CLI's
     `locate_or_build_cross_stdlib` pass `--no-default-features` (plus a
     future `wasm-min` feature) automatically when the triple is wasm, so
     end-users never hit the raw default-trio failure. Today the CLI does
     NOT pass `--no-default-features` on the cross-build subprocess (it
     works in CI only because the CI step pre-builds the archive with the
     flag, short-circuiting the subprocess). **This is the first thing the
     next sprint must wire** — otherwise `cobrust build --target=wasm32-wasip1`
     on a clean machine (no pre-built archive) will try the default-feature
     cross-build and fail on mimalloc.
   - **(b) `mimalloc` → conditional.** Gate the `mimalloc-alloc` default
     off for wasm; fall back to `system-alloc` (or the wasi default
     allocator). `dlmalloc`/`wee_alloc` are wasm-friendly alternatives if a
     non-default allocator is desired.
   - **(c) `tokio` → `tokio_with_wasm` or feature-stripped.** Per ADR-0075
     §Q2 the policy is silent single-threaded degrade. Either swap to a
     wasm-compatible async shim or compile `task`/`sync` with a
     single-threaded executor on wasm.

2. **`task::spawn` silent-degrade (ADR-0075 §Q2).** On wasm32 there are no
   threads in baseline WASI; `task::spawn` should degrade to inline
   single-threaded execution (run the closure serially on the same event
   loop) rather than fail to compile. Requires (1c). Until then `task`/`sync`
   are simply absent on wasm (`--no-default-features`), so a `.cb` program
   that imports `std.task` won't link on wasm — acceptable for hello-world
   but not for the full computation tier.

3. **`EcosystemUnavailableOnTarget` typecheck gate (ADR-0075 §5 Phase 2 +
   §Q4).** Network modules (`pit`, `strike`) and CLI-argv-via-WASI (`hood`)
   should be rejected at typecheck with a clear
   `TypeError::EcosystemUnavailableOnTarget { module, target }` when imported
   on wasm, rather than failing deep in the cross-link. This needs an
   `available_on: Vec<TargetMatcher>` field per ecosystem entry in
   `cobrust-types/src/ecosystem.rs` (ADR-0072 manifest growth). **Explicitly
   out of Sprint E scope** — a concurrent F68 sprint reads
   `ecosystem.rs`, so Sprint E does not touch it. Defer to a follow-up that
   owns that file.

4. **WASM panic ABI (ADR-0075 §"Combined risk surfaces").** `panic.rs` uses
   `std::process::exit(...)`, which maps to WASI `proc_exit` — works on
   wasm32-wasip1. The ADR's note about mapping `__cobrust_panic` to the
   `unreachable` instruction applies to `wasm32-unknown-unknown` (no WASI),
   not p1. No action needed for p1; revisit for the browser target.

## Why this matters (LLM-first lens, §2.5)

The opaque `bits/libc-header-start.h file not found` is exactly the kind of
failure CLAUDE.md §2.5-B targets: the old error said only "runtime-helper
compilation failed via clang-18" — the LLM agent reading stderr could not
infer "install a wasi-sysroot". `resolve_wasi_sysroot`'s new error names the
env vars + the install doc + the precise glibc-fallback symptom, so the
correction signal is in-band.

## Status

Candidate. Promote to ratified once the next CI `wasm32-cross-smoke` run is
GREEN end-to-end (stdlib cross-build → host cli → live E2E → wasmtime
hello-world smoke). The deferred items (1)-(3) become their own sprint
(suggested: ADR-0075 Phase 2 Sprint F — "wasm32 full computation tier").
