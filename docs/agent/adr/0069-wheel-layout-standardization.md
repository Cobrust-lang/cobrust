---
doc_kind: adr
adr_id: 0069
title: Wheel layout standardization — FHS bin/lib/share + bundled runtime + post-package smoke gate
status: accepted
date: 2026-05-22
last_verified_commit: <closure-sprint-final>
supersedes: []
superseded_by: []
---

# ADR-0069: Wheel layout standardization — FHS `bin/lib/share` + bundled runtime + post-package smoke gate

## 1. Context

- **F46 packaging gap (surfaced 2026-05-22)** — v0.5.1 + v0.5.2 wheels
  ship a `cobrust` binary that **cannot compile a single Cobrust source
  file on a user machine** because:
  - `crates/cobrust-cli/src/build.rs:387` uses
    `env!("CARGO_MANIFEST_DIR")` — a compile-time constant baked at
    release-build time as the GH Actions runner path
    (`/Users/runner/work/cobrust/cobrust/...`). At user run-time that
    directory does not exist.
  - `crates/cobrust-cli/src/build.rs:419-466` `locate_stdlib_archive`
    walks `workspace/target/{release,debug}/libcobrust_stdlib.a` — same
    build-host-rooted scheme.
  - The v0.5.2 wheel tarball schema (`release.yml:175-186`) packages
    only `cobrust + cobrust-lsp + cobrust-dap` — `runtime/cobrust_main.c`,
    `runtime/cpu_features.c`, `libcobrust_stdlib.a` are not bundled.
    Even if the binary's lookup chain had a current_exe()-rooted
    fallback, the files would not be on disk for it to find.
- **ADR-0068 v0.6.0 frame (in flight)** — collapses `cobrust-lsp` +
  `cobrust-dap` into `cobrust` subcommands with transitional shim
  binaries. v0.6.0 is the natural integration point for wheel-layout
  changes (one breaking-layout cut, not two).
- **ADR-0065 Tier-3 wheel distribution** — established the wheel-as-
  primary-distribution-channel for non-Rust users. F46 is the structural
  failure mode of that decision when the binary's lookup chain assumes
  in-workspace build context.
- **Mainstream layout precedent** — most language toolchains follow
  FHS-ish `bin/lib/share` for their tarball distributions:
  - `node-v20.x-darwin-arm64.tar.gz`: `bin/node`, `include/`,
    `lib/node_modules/`, `share/{man,doc}/`.
  - `go1.22.x.darwin-arm64.tar.gz`: `go/bin/go`, `go/pkg/`, `go/src/`,
    `go/lib/`.
  - `rustc-1.94.0-aarch64-apple-darwin.tar.gz`: `rustc/bin/rustc`,
    `rustc/lib/`, `rustc/share/`.
  - Cobrust v0.5.x flat-binary layout is the outlier.

## 2. Why now

- **v0.5.1 + v0.5.2 wheels are unusable for the default user path.**
  Every release this week shipped a wheel that cannot compile
  `hello.cb`. Deferring the fix past v0.6.0 lets the broken wheel
  continue to be the published Tier-3 install artifact.
- **ADR-0068 v0.6.0 cut breaks layout once.** Bundling runtime + stdlib
  + adopting subcommand shape in the same major-cut amortizes the
  user-visible disruption (and the matching documentation rewrite) into
  one release.
- **F46 §3 systemic gate.** Without a post-package smoke gate, any
  future `release.yml` change risks the same regression. Adding the
  gate alongside the layout change closes the recurrence channel.
- **§2.5 LLM-first alignment.** A `tar xz && ./bin/cobrust run hello.cb`
  zero-config flow matches the LLM agent's training-data prior for
  "language toolchain tarball install" (`node` / `go` / `rustc` /
  `deno` / `bun` all follow this pattern). Flat-binary layouts
  (current Cobrust v0.5.x) are the outlier and bias the LLM toward
  guessing at `sudo mv` or `cp` steps that aren't atomic.

## 3. Options considered

1. **A. Status quo (flat binary + workspace-rooted lookup)** — the
   v0.5.x scheme. Confirmed broken by F46. **Rejected.**
2. **B. Flat binary + bundle runtime files at tarball root** —
   `cobrust + cobrust-lsp + cobrust-dap + libcobrust_stdlib.a +
   runtime/{cobrust_main.c, cpu_features.c}` all at tarball root.
   `build.rs` `current_exe()`-rooted lookup checks the binary's own
   directory for sibling files. Minimal layout change. **Rejected**:
   tarball root pollution; mixing binaries with .a archives and .c
   sources at the top level confuses package-manager keg installs
   (Homebrew, MacPorts) and user `cp -r` install scripts.
3. **C. FHS-ish bin/lib/share layout (selected)** — wheel extracts to
   a single directory containing `bin/`, `lib/cobrust/`,
   `share/cobrust/runtime/`. `build.rs` discovers the binary's own
   directory via `std::env::current_exe()`, walks up one level, then
   checks `../lib/cobrust/libcobrust_stdlib.a` and
   `../share/cobrust/runtime/{cobrust_main.c, cpu_features.c}`.
   Symmetric with `node`/`go`/`rustc`/`brew` keg layouts. **Selected.**
4. **D. Embed runtime + stdlib via rust-embed at compile time** —
   bake `runtime/cobrust_main.c` + `libcobrust_stdlib.a` into the
   binary itself, extract to `$TMPDIR/cobrust-rt-<sha>/` on first
   invocation. Zero filesystem dependency on the wheel layout.
   **Rejected**: `libcobrust_stdlib.a` is ~12 MB; embedding 12+ MB
   in every wheel binary inflates wheel size 3-9x. First-invocation
   extraction cost adds startup latency. Per-binary extraction
   directory leaks files into `$TMPDIR` (housekeeping burden).

**Decision: C (FHS-ish layout) + new post-package smoke gate.** The
smoke gate is non-negotiable per F46 §3; without it, the same
regression class re-occurs at the next packaging change.

## 4. Architectural decision

### 4.1 Wheel tarball layout (v0.6.0+)

```
cobrust-v0.6.0-<triple>-<cpu_level>.tar.gz
└── cobrust-v0.6.0/
    ├── bin/
    │   ├── cobrust          # cobrust-cli main binary
    │   ├── cobrust-lsp      # shim (ADR-0068 transitional; deleted v0.7.0)
    │   └── cobrust-dap      # shim (ADR-0068 transitional; deleted v0.7.0)
    ├── lib/
    │   └── cobrust/
    │       └── libcobrust_stdlib.a
    └── share/
        └── cobrust/
            └── runtime/
                ├── cobrust_main.c
                └── cpu_features.c
```

The single top-level directory (`cobrust-v0.6.0/`) is intentional:
`tar xz` extracts to a self-contained tree that the user can `mv` or
`ln -s` into place atomically. Avoids littering the user's CWD with
loose files at extraction time.

### 4.2 Binary lookup chain (`build.rs`)

Both `locate_runtime_source` and `locate_stdlib_archive` adopt a
new **Phase 0** lookup that runs before the existing fallback chain:

**Phase 0 (wheel-layout, NEW)** — derive `<install_prefix>` from the
running binary's own path:

1. `let exe = std::env::current_exe()?;`
2. `let bin_dir = exe.parent()?;` (e.g. `/opt/cobrust-v0.6.0/bin/`)
3. `let prefix = bin_dir.parent()?;` (e.g. `/opt/cobrust-v0.6.0/`)
4. Check `prefix.join("share/cobrust/runtime/cobrust_main.c")` —
   return if exists.
5. Check `prefix.join("share/cobrust/runtime/cpu_features.c")` —
   return if exists.
6. Check `prefix.join("lib/cobrust/libcobrust_stdlib.a")` — return
   if exists.

**Phase 1+ (existing fallback chain, unchanged for dev builds)**:

- `runtime`: `CARGO_MANIFEST_DIR/runtime/<name>.c` (workspace dev
  build path). Already present.
- `stdlib`: `COBRUST_STDLIB_ARCHIVE_PATH` baked compile-time env var
  → `COBRUST_STDLIB_ARCHIVE` runtime override → workspace
  `target/{release,debug}/libcobrust_stdlib.a`. Already present.

Phase 0 fires first so wheel-installed users get a zero-config path.
Phase 1+ remains as fallback for `cargo install` and source-tree
`cargo build` flows.

### 4.3 release.yml restructure (v0.6.0+)

- **Build step**: builds `cobrust-cli` + `cobrust-lsp-shim` +
  `cobrust-dap-shim` (ADR-0068 §4.2) — three binaries renamed to
  `cobrust`, `cobrust-lsp`, `cobrust-dap` at package time.
- **Package step**: creates the FHS tree on disk
  (`cobrust-v0.6.0/{bin,lib/cobrust,share/cobrust/runtime}/`),
  copies the three binaries into `bin/`, copies
  `target/<triple>/release/libcobrust_stdlib.a` into `lib/cobrust/`,
  copies `crates/cobrust-cli/runtime/*.c` into
  `share/cobrust/runtime/`, tars the single top-level directory.
- **Post-package smoke step (NEW, per F46 §3)**:
  - `cd $(mktemp -d) && tar xzf <tarball>`
  - `echo 'fn main() -> i64: print("smoke"); return 0' > t.cb`
  - `./cobrust-v0.6.0/bin/cobrust run t.cb 2>&1 | tee out.txt`
  - `grep -q "smoke" out.txt || exit 1`
  - On smoke failure: the job fails BEFORE the artifact upload step,
    so a broken wheel is never published.
- **Cross-compile note**: smoke step only runs on native targets
  (`use_cross == false`). Cross-compiled tarballs (e.g.
  `aarch64-unknown-linux-gnu` built on `ubuntu-latest`) cannot
  execute their own binary; smoke is gated by `if: !matrix.use_cross`.
  Cross-compile bundle correctness is verified by post-publish
  user-side install tests against the GH Releases tarball.

### 4.4 Install flows

| Path | Command | Notes |
|---|---|---|
| Manual (recommended) | `tar xzf cobrust-v0.6.0-<triple>-<cpu>.tar.gz -C $HOME/.local && ln -s $HOME/.local/cobrust-v0.6.0/bin/cobrust $HOME/.local/bin/cobrust` | Self-contained tree; symlink for `$PATH` |
| `cobrust install` (future) | `cobrust install cobrust --version 0.6.0` | Resolver-mediated; same tarball, same layout |
| Homebrew (future) | `brew install cobrust-lang/cobrust/cobrust` | Formula: `bin.install Dir["bin/*"]; lib.install "lib/cobrust"; share.install "share/cobrust"` — maps to brew keg verbatim |
| `cargo install` (dev) | `cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli` | Workspace-rooted lookup chain (Phase 1+) still functions; wheel layout not produced |

### 4.5 Migration path (v0.5.x → v0.6.0)

Users on v0.5.x with a working setup:
- If they `cargo install`'d → no action; the source-tree fallback
  chain (Phase 1+) still works.
- If they manually `cp cobrust /usr/local/bin/` from a v0.5.x
  tarball → their binary is broken per F46. Re-install via v0.6.0
  tarball + `ln -s` per §4.4.

Per-release downstream notification: `RELEASE_NOTES_v0.6.0.md`
section "Breaking: wheel layout" calls out the `tar xz` → `ln -s`
flow change.

## 5. Tests + verification

- **build.rs unit tests**: `locate_runtime_source_phase0_finds_share()`
  + `locate_stdlib_archive_phase0_finds_lib()` set up a temp dir tree
  matching §4.1, set `current_exe()` mock, assert Phase 0 hits return
  the expected paths. Existing Phase 1+ tests remain.
- **release.yml smoke gate** (per §4.3) is itself the integration
  test; failure blocks artifact upload.
- **Post-release verification**: P10 CTO runs the
  `tar xz && ./bin/cobrust run hello.cb` flow on a real wheel
  download from GH Releases per Phase G of the v0.6.0 sprint.

## 6. Consequences

- **Positive**
  - Wheel users get a working install in one `tar xz` invocation
    (no env vars, no manual `cp` of runtime files).
  - F46 silent-failure mode is closed at the user surface AND at the
    CI gate (recurrence-proof via §4.3 smoke step).
  - Future package-manager integrations (Homebrew, MacPorts, distro
    .deb / .rpm) drop into the FHS layout natively.
  - Training-data overlap (§2.5) with mainstream toolchain tarball
    layouts; LLM agents reach for `tar xz` first.
- **Negative**
  - **Breaking layout change for v0.5.x users** who symlinked the
    binary directly from the flat tarball. Documented + flagged at
    `RELEASE_NOTES_v0.6.0.md` per §4.5.
  - Wheel size increases by ~12 MB (the bundled
    `libcobrust_stdlib.a`). Acceptable: matches `rustc`'s ~50 MB
    wheel-equivalent footprint at a fraction of the size.
- **Neutral / unknown**
  - Windows layout: see §7 Q1. Reasonable default is
    `cobrust-v0.6.0\bin\cobrust.exe` + `\lib\cobrust\` +
    `\share\cobrust\runtime\` per Windows port of the same FHS shape.
    Confirmation deferred to the first MSVC tier-2 build (ADR-0058b
    follow-up).

## 7. Open questions

1. **Windows path conventions.** Should Windows wheels follow the
   same `bin/lib/share` shape or adopt a Windows-native
   `<prefix>\cobrust.exe + \stdlib\* + \runtime\*` shape? Decision:
   defer to MSVC tier-2 promotion (ADR-0058b follow-up). v0.6.0
   tier-1 is darwin + linux only, so Windows is non-blocking.
2. **Homebrew formula timing.** The §4.4 brew install path documents
   the eventual formula shape. Actual formula publication is gated
   on Phase Q (post-Phase-O wheel distribution maturity). Open
   question: should the v0.6.0 release notes link to a draft
   formula, or wait for the formula to land first?
3. **`cargo install` parity.** `cargo install cobrust-cli` produces
   a flat binary (no wheel layout); the source-tree fallback chain
   still works. Should v0.6.0 add a `cargo install` post-build step
   that emits a wheel layout under `~/.cargo/share/cobrust/`?
   Decision: NO for v0.6.0 — `cargo install` is the dev path; wheel
   layout is the user path; conflating them adds maintenance
   surface without user-side benefit.

## 8. Cross-references

- **ADR-0065** — Tier 3 prebuilt multi-wheel distribution. The wheel
  format this ADR layout-standardizes.
- **ADR-0068** — Single-binary subcommand collapse. v0.6.0 sibling
  ADR; impl is co-shipped in the same sprint as ADR-0069 impl.
- **F46** — Wheel not installable: runtime+stdlib gap. The finding
  this ADR resolves; F46 §3 detection rule is implemented at §4.3.
- **F45a** — LLVM backend wave-3 scope systemic. Direct packaging-
  discipline sibling; same "object-emit green != end-user working"
  class.
- **F35-sibling, F37, F44** — F-family lineage via F46 §4.

## 9. Done means (v0.6.0 impl sprint gates)

- [ ] `crates/cobrust-cli/src/build.rs`:`locate_runtime_source` +
      `locate_stdlib_archive` both adopt Phase 0
      `current_exe()`-rooted lookup ahead of existing chains.
- [ ] `.github/workflows/release.yml`: build step builds the three
      shim/cli crates; package step creates the FHS tree from §4.1
      and tars the single top-level directory; post-package smoke
      step extracts the tarball and runs `cobrust run hello.cb`
      with `grep -q smoke` assertion.
- [ ] First v0.6.0 wheel published via release.yml passes the smoke
      gate (failure would block publication).
- [ ] Post-release: P10 CTO downloads the published wheel from GH
      Releases, runs `tar xz && ./cobrust-v0.6.0/bin/cobrust run
      hello.cb`, observes stdout match without manual env-var or
      file-copy steps.
- [ ] `README.md` + `README.zh.md` + `RELEASE_NOTES_v0.6.0.md` +
      `editors/vscode-cobrust/README.md` updated to document the
      new `tar xz` → `ln -s` flow.
- [ ] ADR-0069 frontmatter `last_verified_commit:` set to impl
      HEAD; status flipped from `accepted` to
      `accepted (v0.6.0 SHIPPED)`.

**Claim audit (F35-sibling discipline)**: this ADR ships design only.
Body language consistently uses future-tense for v0.6.0 impl ("will
build", "after merge", "v0.6.0 ships"). The single committed file is
this ADR markdown. The F46 finding (committed separately) documents
the empirical user-side failure; ADR-0069 documents the chosen
resolution shape.
