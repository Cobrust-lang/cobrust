---
doc_kind: adr
adr_id: 0068
title: Single-binary subcommand collapse — cobrust-lsp / cobrust-dap → cobrust lsp / cobrust dap subcommands
status: accepted
date: 2026-05-22
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0068: Single-binary subcommand collapse — `cobrust-lsp` / `cobrust-dap` → `cobrust lsp` / `cobrust dap` subcommands

## 1. Context

- **v0.5.1 packaging gap (F35-sibling, surfaced 2026-05-22)** — the v0.5.1
  wheel shipped only the `cobrust` binary while the editor extension README
  (ADR-0067) and skill §9c documentation both claimed `cobrust-lsp` was
  "available after pip install". An empirical packaging audit by the user
  on 2026-05-22 surfaced the drift: wheel tarballs did not include the LSP or
  DAP binaries despite the documentation contract. Pattern is a textbook
  F35-sibling claim-vs-reality drift incident.
- **v0.5.2 transitional fix (in flight, `a1e5aafbc352c6480`)** — bundles the
  three binaries `cobrust` + `cobrust-lsp` + `cobrust-dap` inside every wheel
  variant. Closes the gap for v0.5.x users, but ships three independent
  `[[bin]]` artifacts whose versions can in principle drift independently
  from `cobrust-cli`. This is the right outcome in the wrong shape.
- **REPL precedent** — `cobrust repl` has been a subcommand of the main
  binary since v0.3.0 (Phase G closure). Phase J (LSP, ADR-0057) and Phase L
  (DAP, ADR-0059) instead emitted standalone bins, which is now inconsistent
  with the REPL pattern.
- **Pattern survey across mainstream language toolchains** — `cargo build` /
  `cargo run` / `cargo check`, `deno run` / `deno fmt` / `deno lsp`, `bun
  install` / `bun run` / `bun test`, `go build` / `go run` / `go fmt`. All
  ship a single binary with a verb-noun subcommand shape. Cobrust three-bin
  layout (`cobrust` + `cobrust-lsp` + `cobrust-dap`) is the outlier.
- **Current binary layout (post-v0.5.2)**:
  - `cobrust` — `build` / `run` / `check` / `fmt` / `translate` / `new` /
    `test` / `repl` / `debug` / `skills` / `install` / … (12 subcommands)
  - `cobrust-lsp` — standalone bin emitting a stdio LSP server (13 handlers,
    v1.3 per ADR-0057a)
  - `cobrust-dap` — standalone bin emitting a stdio DAP server (17 handlers,
    v1.2 per ADR-0059b/c/d/e/f/g)

## 2. Why now

- **v0.5.2 is the right thing in the wrong shape.** Bundling three bins
  papers over the symptom (binary missing) without addressing the cause
  (three independent artifacts whose versions can skew). A subcommand
  collapse is the structurally honest fix; deferring it past v0.6.0 lets
  the inconsistency calcify across more channels (marketplace extension,
  skill docs, third-party tutorials).
- **Future tooling slot.** Phase P / Phase Q planning (formatter, linter,
  profiler, package-graph-resolver) will each want their own surface. A
  flat namespace under `cobrust <verb>` scales naturally; another four
  `[[bin]]` artifacts does not. `cobrust fmt` already exists; `cobrust
  lint`, `cobrust profile`, `cobrust deps` are anticipated.
- **Extension wiring simpler.** ADR-0067 §Wave-1 specifies the extension
  spawns `cobrust-lsp` from `$PATH`. A subcommand layout reduces this to
  spawning `cobrust lsp`, which is one PATH lookup instead of two and one
  binary version atomicity guarantee instead of zero. DAP integration
  (Wave-6 follow-up) inherits the same simplification for free.
- **§2.5 LLM-first alignment.** Training-data overlap with `cargo build` +
  `deno lsp` + `bun test` is high; standalone-bin layout (`rust-analyzer`
  + `pyright-langserver` + `gopls`) has overlap too but Cobrust already
  committed to the subcommand pattern with `cobrust repl`, and consistency
  within Cobrust's surface outweighs cross-project pattern-matching.

## 3. Options considered

1. **A. Keep three bins (status quo post-v0.5.2)** — minimum surface area,
   no churn, but three independent artifacts whose versions can drift
   silently and four future-tool slots (`fmt`, `lint`, `profile`, `deps`)
   want subcommand shape anyway. **Rejected.**
2. **B. Collapse all three into `cobrust` subcommands** — `cobrust lsp` +
   `cobrust dap` join the existing 12 subcommands. Single binary in PATH,
   single version, single wheel artifact post-transition. Recommended,
   but standalone-bin install paths for the extension (v0.1.0) break at
   the transition cut. **Selected for canonical layout (v0.6.0+).**
3. **C. Hybrid: B canonical + transitional shim bins for v0.6.x** — same
   subcommand collapse as B for the canonical wheel, plus two two-line
   shim binaries (`cobrust-lsp` / `cobrust-dap`) that simply call into
   the lib crates' `pub fn run()`. Extension v0.1.0 keeps working;
   extension v0.2.0 (subcommand-aware) ships alongside. v0.7.0 drops the
   shims. **Selected as the transition strategy alongside B.**
4. **D. Symlink standalone bins to the cobrust binary with arg injection**
   — `cobrust-lsp` is a symlink to `cobrust`, and `cobrust` inspects
   `argv[0]` to dispatch. Saves wheel size by ~30 MB × 2. **Rejected**:
   symlinks are fragile on Windows + macOS sandboxed installs; `argv[0]`
   inspection is brittle when the launcher is invoked via a shell alias
   or shim wrapper (uv, pipx).

**Decision: B + C transitional.** v0.6.0 ships subcommands as the canonical
layout; v0.6.x keeps `cobrust-lsp` + `cobrust-dap` as thin shim binaries
(2-line `main` calling into lib crates) so extension v0.1.x users do not
break. v0.7.0 drops the shims.

## 4. Architectural decision

### 4.1 Crate refactor (per-crate)

| Crate | Before | After (v0.6.0) | Notes |
|---|---|---|---|
| `crates/cobrust-lsp/` | `[[bin]] name = "cobrust-lsp" path = "src/main.rs"` + `[lib]` not declared | `[lib]` only, no `[[bin]]` section | Add `pub fn run() -> Result<(), Error>` wrapping the current `main` body verbatim |
| `crates/cobrust-dap/` | `[[bin]] name = "cobrust-dap"` + `[lib]` not declared | `[lib]` only, no `[[bin]]` section | Same `pub fn run()` extraction pattern as LSP |
| `crates/cobrust-cli/` | depends on `cobrust-lsp` / `cobrust-dap`? no (today they are leaf bins) | add `cobrust-lsp` + `cobrust-dap` as workspace deps | Workspace member set unchanged |
| `crates/cobrust-cli/src/main.rs` | `Commands` enum has 12 variants | add 2 variants `Lsp` + `Dap`; dispatch into wrappers | `clap` derive style; reuse existing pattern from `Commands::Repl` |
| `crates/cobrust-cli/src/lsp.rs` | does not exist | new file, ~10 lines: `pub fn run() -> Result<(), Error> { cobrust_lsp::run() }` + optional CLI-arg shim if LSP gains flags | Thin wrapper; no logic |
| `crates/cobrust-cli/src/dap.rs` | does not exist | new file, ~10 lines: `pub fn run() -> Result<(), Error> { cobrust_dap::run() }` | Same shape as `lsp.rs` |

### 4.2 Transitional shim bin crates (v0.6.x only)

| Crate | Cargo.toml | src/main.rs | Lifecycle |
|---|---|---|---|
| `crates/cobrust-lsp-shim/` | `[[bin]] name = "cobrust-lsp" path = "src/main.rs"` + dep `cobrust-lsp = { path = "../cobrust-lsp" }` | `fn main() -> std::process::ExitCode { match cobrust_lsp::run() { Ok(()) => 0.into(), Err(e) => { eprintln!("{e}"); 1.into() } } }` | Created v0.6.0; deleted v0.7.0 |
| `crates/cobrust-dap-shim/` | `[[bin]] name = "cobrust-dap" path = "src/main.rs"` + dep `cobrust-dap = { path = "../cobrust-dap" }` | same shape as shim above, with `cobrust_dap::run()` | Created v0.6.0; deleted v0.7.0 |

Shim crate locations: `crates/cobrust-lsp-shim/` and `crates/cobrust-dap-shim/`
(workspace-member uniform; see §9 Q1 below).

### 4.3 release.yml (v0.6.x)

- Build command: `cargo build -p cobrust-cli -p cobrust-lsp-shim -p cobrust-dap-shim --release --target $TARGET`
- Tarball schema: `cobrust` + `cobrust-lsp` + `cobrust-dap` (drop-in
  compatible with v0.5.2 wheel layout — extension v0.1.0 sees the
  shim binary on PATH and is unaffected).

### 4.4 release.yml (v0.7.0)

- Build command: `cargo build -p cobrust-cli --release --target $TARGET`
  (shim crates deleted from workspace).
- Tarball schema: `cobrust` only.
- Extension prerequisite: extension v0.2.0+ for v0.7.0 compiler;
  extension v0.1.0 + v0.5.x compiler still functions via separate-bin
  path; extension v0.1.0 + v0.7.0 compiler **breaks** (documented at
  v0.6.0 release notes + flagged at v0.7.0 release notes).

## 5. Extension impact (v0.2.0 wave-2)

Editor extension v0.2.0 (ADR-0067 wave-2 follow-up) changes:

- **LSP launch**: try `cobrust lsp` first (stdio LSP), fallback to
  `cobrust-lsp` (legacy standalone bin path). Implementation: probe
  `cobrust --version` to check ≥ 0.6.0; if so, spawn `cobrust lsp`; else
  spawn `cobrust-lsp`. Detection is one `Command::output()` call at
  activation.
- **DAP integration (new in v0.2.0)**: `contributes.debuggers` manifest
  entry + `DebugAdapterDescriptorFactory` that launches `cobrust dap`
  with fallback to `cobrust-dap`. Wave-1 of ADR-0067 explicitly deferred
  DAP integration; v0.2.0 of the extension picks it up.
- Result: extension v0.2.0 works with v0.5.x compiler (separate bins)
  AND v0.6.x compiler (subcommands + shim bins) AND v0.7.0 compiler
  (subcommands only).

## 6. Migration paths (matrix)

| User on compiler | User on extension | After v0.6.0 ships | Action required |
|---|---|---|---|
| v0.5.1 | v0.1.0 | LSP works via direct `cobrust-lsp` on PATH from manual `cargo install` | None |
| v0.5.2 | v0.1.0 | LSP works (wheel bundled `cobrust-lsp` standalone bin) | None |
| v0.6.0 | v0.1.0 | LSP works (shim `cobrust-lsp` still bundled in wheel) | Optional: upgrade extension to v0.2.0 for DAP support |
| v0.6.0 | v0.2.0 | LSP works via `cobrust lsp`; DAP works via `cobrust dap` | Ideal terminal state |
| v0.7.0 | v0.1.0 | LSP **breaks** (shim removed; extension calls `cobrust-lsp` which no longer exists on PATH) | Required: upgrade extension to v0.2.0 |

## 7. Tests + verification

Three smoke fixtures gate the v0.6.0 sprint:

1. **`crates/cobrust-cli/tests/lsp_subcommand_smoke.rs`** — spawn
   `cobrust lsp` as a child process; feed a synthetic LSP `initialize`
   JSON-RPC request over stdin; assert the server emits a valid
   `InitializeResult` response on stdout (within 1 s timeout). Same
   handshake the extension performs.
2. **`crates/cobrust-cli/tests/dap_subcommand_smoke.rs`** — spawn
   `cobrust dap`; feed a synthetic DAP `initialize` request; assert
   `initialized` event + `InitializeResponse` emitted within 1 s.
3. **`crates/cobrust-lsp-shim/tests/shim_smoke.rs`** (mirrored for
   `cobrust-dap-shim/`) — spawn the shim binary directly; verify that
   the LSP / DAP `initialize` handshake matches subcommand behaviour
   byte-for-byte. Guarantees the shim is a truly transparent wrapper.

All three smoke tests run in CI as part of the release.yml workflow
post-build; failure blocks tag publishing.

## 8. Consequences

- **Positive**
  - Single binary in PATH matches REPL precedent + `cargo` / `deno` /
    `bun` / `go` toolchain conventions (training-data overlap, §2.5 D).
  - Version atomicity: no `cobrust-lsp v1.2` shipping alongside
    `cobrust-cli v0.5.0` cross-skew (F35-sibling class permanently
    closed for the LSP/DAP surfaces).
  - Wheel size unchanged: LLVM / codegen crates are already shared via
    workspace deps; the three subcommands share the same compiled core.
  - Future tooling (`cobrust fmt` already exists; `cobrust lint`,
    `cobrust profile`, `cobrust deps` anticipated for Phase P/Q) slots
    in naturally.
  - Extension wiring simpler at v0.2.0: one PATH lookup instead of two.
- **Negative**
  - Transitional shim crates add 4 files (2 `Cargo.toml` + 2
    `src/main.rs`, each ~10 lines) during the v0.6.x window. Deleted
    at v0.7.0; net overhead is zero post-transition.
  - Extension v0.1.0 users break at the v0.7.0 cut. Documented + flagged
    at v0.6.0 release notes + dropped-shim warning printed by
    v0.7.0-rc1.
  - LSP/DAP process invocation pattern changes — but the extension is
    the only known external caller; documentation update is bounded.
- **Neutral / unknown**
  - Shim binary stripped size: estimated ~50 KB each (lib statically
    linked into both shim and cobrust main; LTO deduplicates). Wheel
    bloat is below detection threshold.
  - Subcommand spawn latency: identical to standalone bin (`cobrust`
    binary's command-dispatch overhead is ~1 µs per `clap` benchmark).

## 9. Open questions

1. **Should shim crates live in `crates/` or in a sub-tree like `compat/`?**
   Decision: `crates/`. Rationale: workspace-member uniformity, no
   per-crate Cargo.toml special-casing, easier `cargo metadata` consumers.
2. **Should v0.6.0 stage as `v0.6.0-rc1` first?** Decision: yes, gated on
   ≥ 1 week soak of the shim binaries against extension v0.1.0 in CI;
   then v0.6.0 final. If user prefers single tag (no rc1) the gate
   collapses to "all three smoke tests + integration extension test
   PASS in CI" and proceeds direct to v0.6.0.
3. **v0.7.0 timeline.** Proposal: 2026-Q3, gated on extension v0.2.0
   marketplace adoption metric — either downloads > 100 OR 30 days
   elapsed post-v0.6.0 release, whichever comes later. v0.7.0 is a
   coordinated cut: ADR-0067 wave-2 (extension v0.2.0 marketplace
   publish) ships first; v0.7.0 follows once the adoption gate clears.

## 10. Cross-references

- **ADR-0001** — license (Apache-2.0 + MIT dual; applies to all new shim
  crates and the editor extension wave-2)
- **ADR-0057** / **ADR-0057a** — Phase J LSP frame (v1.3 closure; the
  13-handler surface preserved here verbatim, just relocated)
- **ADR-0059** + **ADR-0059b** / **0059c** / **0059d** / **0059e** /
  **0059f** / **0059g** — Phase L DAP frame (v1.2; 17-handler surface
  preserved verbatim)
- **ADR-0065** — Tier 3 prebuilt multi-wheel distribution (the wheel
  tarball schema this ADR mutates; v0.6.x preserves the 3-binary
  schema, v0.7.0 collapses to single-binary)
- **ADR-0067** — VSCode/Cursor editor extension (wave-1 spawns
  `cobrust-lsp`; wave-2 picks up subcommand + DAP integration per §5
  above)
- **F45a** — LLVM backend wave-3 scope systemic finding (sibling
  packaging-discipline incident; same "right thing in wrong shape"
  pattern)
- **F35-sibling** — DEV agent commit-msg vs diff drift catalogue
  (directly motivates this ADR; v0.5.1 wheel claim-vs-reality drift is
  the latest instance of the F35 family)

## 11. Done means (v0.6.0 impl sprint gates)

This ADR ships **design only**; impl is a separate sprint. The author
audit (§Frontmatter `last_verified_commit: TBD`) flips to the impl HEAD
when the following gates all pass:

- [ ] `crates/cobrust-lsp/Cargo.toml` declares `[lib]` only (no
      `[[bin]]`); exports `pub fn run() -> Result<(), Error>`.
- [ ] `crates/cobrust-dap/Cargo.toml` declares `[lib]` only; exports
      `pub fn run()`.
- [ ] `crates/cobrust-cli/src/{lsp,dap}.rs` exist as thin wrappers
      (≤ 15 lines each).
- [ ] `crates/cobrust-cli/src/main.rs` `Commands` enum gains `Lsp` +
      `Dap` variants with dispatch to the wrappers.
- [ ] `crates/cobrust-lsp-shim/` + `crates/cobrust-dap-shim/` exist as
      workspace members with 2-line `main()` bodies.
- [ ] `release.yml` builds all three packages (`cobrust-cli` +
      `cobrust-lsp-shim` + `cobrust-dap-shim`); wheel tarball schema
      unchanged from v0.5.2 (drop-in compatible).
- [ ] All three smoke tests (`lsp_subcommand_smoke.rs`,
      `dap_subcommand_smoke.rs`, `shim_smoke.rs` × 2) PASS in CI.
- [ ] ADR-0067 §Editor-prereq updated to "compiler v0.6.0+ uses
      `cobrust lsp` subcommand; v0.5.x uses `cobrust-lsp` standalone".
- [ ] ADR-0068 frontmatter `last_verified_commit:` set to impl HEAD
      and `status` flipped from `accepted` to
      `accepted (v0.6.0 SHIPPED)`.

**Claim audit (F35-sibling discipline)**: this ADR ships design only.
No source crate is mutated, no `release.yml` is mutated, no `[[bin]]`
section is removed in this commit. Body language consistently uses
future-tense ("v0.6.0 ships", "after v0.7.0", "the impl sprint will")
and gate language ("Done means" predicate list to be satisfied by a
future commit). The single committed file is this ADR markdown.
