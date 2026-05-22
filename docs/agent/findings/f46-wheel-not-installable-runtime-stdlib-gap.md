---
name: f46
status: ratified
family: F46 (packaging discipline) — F45a sibling + F35-sibling (claim drift) + F37 (silent rot) + F44 (CI green != working)
last_verified_commit: c55f859
date: 2026-05-22
---

# F46 — Wheel not installable: runtime + stdlib bundle gap

## §1 Context

**v0.5.1 + v0.5.2 wheels are 100% broken for `cobrust run` / `cobrust build`
on the user's machine.** Both releases ship a `cobrust` binary that cannot
locate the runtime C source (`runtime/cobrust_main.c`,
`runtime/cpu_features.c`) or the prebuilt stdlib archive
(`libcobrust_stdlib.a`) at run-time, because:

1. `crates/cobrust-cli/src/build.rs:387` uses
   `env!("CARGO_MANIFEST_DIR")` — a compile-time constant that bakes the
   GH Actions runner workspace path
   (`/Users/runner/work/cobrust/cobrust/crates/cobrust-cli`) directly
   into the released binary.
2. `crates/cobrust-cli/src/build.rs:419-466` `locate_stdlib_archive`
   fallback chain walks `workspace/target/{debug,release}/` — also
   build-host-rooted; this directory does not exist on user machines.
3. The wheel tarball schema (per v0.5.2 `release.yml`:175-186) packages
   only the three binaries `cobrust + cobrust-lsp + cobrust-dap` — it
   **does not bundle** `runtime/cobrust_main.c`,
   `runtime/cpu_features.c`, or `libcobrust_stdlib.a`. Even if the
   binary knew where to look, the files would not be on disk.

A `cobrust install` user, or any human who downloads the prebuilt
tarball from the GH Releases page and adds `cobrust` to their `$PATH`,
gets a binary that cannot compile a single Cobrust source file.

The source-built path (`cargo install --git ... cobrust-cli`) works
fine on the build machine because the workspace `target/` directory
still exists locally, and `env!("CARGO_MANIFEST_DIR")` happens to
resolve to a real directory on that machine. This masked the gap
through every prior release.

This finding is a **sibling of F45a** (same packaging-discipline
family: "right thing in wrong shape"; LLVM wave-3 stubs vs. wheel
bundle gap have the same `commit-msg / docs claim coverage that does
not match user reality` shape).

## §2 Empirical proof

Reproduced by user on Mac M1 against v0.5.2 wheel
`cobrust-v0.5.2-aarch64-apple-darwin-m1.tar.gz`:

```bash
mkdir /tmp/v052test && cd /tmp/v052test
curl -L -o w.tar.gz \
  https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.2/cobrust-v0.5.2-aarch64-apple-darwin-m1.tar.gz
tar xzf w.tar.gz
echo 'fn main() -> i64: print("hello"); return 0' > hello.cb
./cobrust run hello.cb
# error: Internal error: cannot locate runtime/cobrust_main.c (checked
# /Users/runner/work/cobrust/cobrust/crates/cobrust-cli/runtime/cobrust_main.c)
```

The error message itself prints the GH Actions runner workspace path —
making the build-host-rooted lookup explicit at the user surface.

`/Users/runner/work/cobrust/cobrust/...` is a temporary directory that
exists only during the GH Actions job and is garbage-collected within
hours of release publication.

## §3 Detection rule (systemic)

**Every `release.yml` MUST include a post-package smoke step that
extracts the produced tarball and runs `cobrust run hello.cb` against
the extracted binary BEFORE the artifact is uploaded for publication.**

The smoke step must:

1. `cd $(mktemp -d) && tar xzf <tarball>`
2. Write a one-line source file (`echo 'fn main() -> i64: print("smoke"); return 0' > t.cb`)
3. Invoke the extracted binary at its post-extraction path (NOT the
   workspace `target/` binary)
4. `grep -q "smoke" <stdout> || exit 1`

This gate would have caught both v0.5.1 (no LSP bundle) and v0.5.2
(no runtime/stdlib bundle, broken binary) before publication.

**Recurrence: any packaging change to `release.yml` MUST re-run this
gate.** Adding a binary, changing tarball schema, adjusting tarball
layout, switching tarball compression — all packaging-side changes
share the same systemic-fragility class.

Without the smoke gate, packaging changes drift from "binary appears
in tarball" (object-emit gate, equivalent of F45's CI cache stale
green) to "binary actually works zero-config for end users" (stdout-
diff gate, equivalent of F45a §3.1 forward rule). The former is what
the v0.5.1 + v0.5.2 release.yml verified; the latter is what F46
mandates.

## §4 Family lineage

This finding is the latest instance of a 4-finding packaging-discipline
catalogue:

- **F35-sibling** (`docs/agent/findings/f35-sibling-commit-msg-vs-diff-drift.md`):
  commit-msg / claim vs. landed diff drift. v0.5.2 commit-msg
  "wheel bundles LSP + DAP" was technically true (binaries appear in
  tarball) but materially false at user-level (runtime missing,
  binary cannot compile a single source file).
- **F37** (`docs/agent/findings/f37-silent-rot-on-accepted-debt.md`):
  silent rot on accepted debt. The `env!("CARGO_MANIFEST_DIR")` use
  in `build.rs` predates Phase O (Tier-3 prebuilt wheel distribution
  per ADR-0065) by months; the M11 stdlib + M10 runtime were
  workspace-internal, never cross-machine. ADR-0065 wave-1 shipped
  wheels without rewriting the lookup chain.
- **F44** (`docs/agent/findings/f44-ci-cache-stale-green-false-pass.md`):
  CI green != workspace clean. v0.5.1 + v0.5.2 CI green on every
  build job; release.yml had no post-package smoke step; nothing
  caught the binary-cannot-find-runtime path at publication time.
- **F45a** (`docs/agent/findings/f45a-llvm-backend-wave3-scope-systemic.md`):
  direct sibling. LLVM wave-3 stubs ship under `--features llvm` opt-in
  with no stdout-diff gate; runtime + stdlib bundle gap ships in the
  default wheel with no post-package smoke gate. Both share the
  "object-emit green != end-user working" class.

## §5 Status

**RATIFIED 2026-05-22** by P10 CTO empirical install test on Mac M1
against v0.5.2 wheel. Reproduces in `~/Downloads/cobrust-v0.5.2-...`
tarball extracted to `/tmp/v052test`. Root cause confirmed at
`build.rs:387` (`env!("CARGO_MANIFEST_DIR")`) and `build.rs:419-466`
(workspace-rooted fallback chain). Wheel tarball schema confirmed at
`release.yml:175-186` (binaries-only; no runtime, no libcobrust_stdlib.a).

## §6 Resolution

Resolved in v0.6.0 by:

- **ADR-0069** — wheel layout standardization (FHS-ish bin/lib/share
  scheme; runtime C files bundled under `share/cobrust/runtime/`;
  prebuilt static archive bundled under `lib/cobrust/`).
- **`build.rs` wheel-layout-aware lookup** — `current_exe()`-rooted
  Phase 0 lookup ahead of the existing `CARGO_MANIFEST_DIR` + env-var
  + workspace `target/` fallback chain.
- **release.yml post-package smoke gate** — extracts each tarball,
  runs `cobrust run` on a one-line source file, fails the job before
  upload if stdout doesn't match.

## §7 Cross-references

- **ADR-0065** — Tier 3 prebuilt multi-wheel distribution. The wheel
  format this finding shows broken at the runtime+stdlib layer.
- **ADR-0068** — single-binary subcommand collapse. v0.6.0 sibling
  ADR; F46 closure is co-shipped with ADR-0068 impl.
- **ADR-0069** — wheel layout standardization. F46 §6 resolution ADR.
- **F45a** — LLVM backend wave-3 scope systemic finding. Direct
  packaging-discipline sibling.
- **F35-sibling, F37, F44** — F-family lineage per §4.
