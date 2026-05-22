# Cobrust v0.6.0 — wheel layout standardization (FHS bin/lib/share) + subcommand collapse

**Released:** 2026-05-22
**Commits since v0.5.2:** 9 (this sprint)
**Tag:** v0.6.0
**Type:** Major (breaking wheel layout from v0.5.x)

---

## TL;DR

- **v0.5.1 + v0.5.2 wheels were 100% broken for `cobrust run`** — the
  binary baked the GH Actions runner workspace path
  (`/Users/runner/work/cobrust/cobrust/...`) and the wheel tarball did
  not bundle `libcobrust_stdlib.a` or `runtime/cobrust_main.c`. Wheel
  users got `cannot locate runtime/cobrust_main.c (checked
  /Users/runner/work/cobrust/cobrust/...)` on the first invocation.
  This finding is filed as **F46**.
- **v0.6.0 fixes both sides.** Wheels now extract to a self-contained
  `cobrust-v0.6.0/{bin,lib/cobrust,share/cobrust/runtime}/` FHS tree
  (ADR-0069); the binary's lookup chain gains a **Phase 0**
  `std::env::current_exe()`-rooted path so it discovers
  `../lib/cobrust/libcobrust_stdlib.a` and
  `../share/cobrust/runtime/cobrust_main.c` at user run-time without
  env vars or workspace context. End state: `tar xz && bin/cobrust
  run hello.cb` works zero-config.
- **release.yml gains a post-package smoke gate** (F46 §3): every
  packaged tarball is extracted in a fresh directory and `cobrust run
  hello.cb` is executed against the extracted binary BEFORE the
  artifact is uploaded. Failure blocks publication. This closes the
  recurrence channel that allowed v0.5.1 + v0.5.2 to ship broken.
- **`cobrust-lsp` and `cobrust-dap` collapse to `cobrust lsp` and
  `cobrust dap` subcommands** (ADR-0068). Editor extension v0.1.x
  users keep working via transitional shim binaries that preserve the
  `cobrust-lsp` and `cobrust-dap` names on `$PATH`; shims are deleted
  at v0.7.0.

If you were on v0.5.x and `cargo install`'d, you are unaffected —
the source-tree fallback chain still works. If you downloaded a v0.5.x
wheel and the install seemed to fail at `cobrust run`, this is why; the
v0.6.0 wheel is the first wheel since v0.5.0 that actually compiles.

---

## What was broken in v0.5.1 + v0.5.2 (F46)

The wheel-distribution path for `cobrust run` had two compounding
defects that no CI gate caught:

1. **`build.rs:387` used `env!("CARGO_MANIFEST_DIR")`** — a compile-time
   constant that bakes the build host's workspace path
   (`/Users/runner/work/cobrust/cobrust/crates/cobrust-cli/runtime/cobrust_main.c`)
   directly into the released binary. At user run-time that directory
   does not exist.
2. **`release.yml`'s tarball packaged only the three binaries
   (`cobrust + cobrust-lsp + cobrust-dap`)** — `runtime/cobrust_main.c`,
   `runtime/cpu_features.c`, and `libcobrust_stdlib.a` were not
   bundled. Even if the binary knew where to look, the files would
   not be on disk.

Source-built users (`cargo install --git ... cobrust-cli`) were
unaffected: the workspace `target/` directory still exists on the
build machine, and `env!("CARGO_MANIFEST_DIR")` happens to resolve to
a real directory there. This masked the gap through every prior
release.

Empirical reproduction from Mac M1 against the v0.5.2 wheel:

```bash
tar xzf cobrust-v0.5.2-aarch64-apple-darwin-m1.tar.gz
echo 'fn main() -> i64: print("hello"); return 0' > hello.cb
./cobrust run hello.cb
# error: Internal error: cannot locate runtime/cobrust_main.c (checked
# /Users/runner/work/cobrust/cobrust/crates/cobrust-cli/runtime/cobrust_main.c)
```

The error message itself prints the GH Actions runner path —
making the build-host-rooted lookup explicit at the user surface.

---

## What v0.6.0 ships (ADR-0068 + ADR-0069)

### Wheel tarball layout (ADR-0069 §4.1)

```
cobrust-v0.6.0-<triple>-<cpu_level>.tar.gz
└── cobrust-v0.6.0/
    ├── bin/
    │   ├── cobrust          # cobrust-cli main binary
    │   ├── cobrust-lsp      # transitional shim (ADR-0068 §4.2; deleted v0.7.0)
    │   └── cobrust-dap      # transitional shim (ADR-0068 §4.2; deleted v0.7.0)
    ├── lib/
    │   └── cobrust/
    │       └── libcobrust_stdlib.a    # prebuilt static archive
    └── share/
        └── cobrust/
            └── runtime/
                ├── cobrust_main.c     # runtime C entrypoint
                └── cpu_features.c     # CPU feature detection helpers
```

The single top-level directory is intentional: `tar xz` extracts to
a self-contained tree that the user can `mv` or `ln -s` into place
atomically. No loose files at the user's CWD.

### Binary lookup chain (ADR-0069 §4.2)

`crates/cobrust-cli/src/build.rs` now starts each lookup with:

**Phase 0 (wheel-layout, NEW)** — derive `<install_prefix>` from the
running binary via `std::env::current_exe()`. The wheel extracts to
`<prefix>/bin/cobrust`, so the binary finds:
- `<prefix>/share/cobrust/runtime/cobrust_main.c` — runtime C source
- `<prefix>/lib/cobrust/libcobrust_stdlib.a` — prebuilt static archive

**Phase 1+ (existing fallback chain)** — for dev / `cargo install`
flows where wheel layout doesn't apply. Unchanged from v0.5.x.

### Subcommand collapse (ADR-0068)

`cobrust-lsp` and `cobrust-dap` are no longer standalone bin crates;
they expose `pub fn run()` as lib entries. The `cobrust` CLI gains
`Lsp` + `Dap` `Commands` variants dispatching through those entries.

Two transitional **shim crates** (`crates/cobrust-lsp-shim/` and
`crates/cobrust-dap-shim/`) carry the standalone binary names
(`cobrust-lsp`, `cobrust-dap`) on `$PATH` so editor extension v0.1.x
users do not break across the v0.5.x → v0.6.x compiler upgrade. Each
shim is a 2-line `main` calling the lib `run()` — byte-for-byte
identical behavior to the subcommand.

Per ADR-0068 §4.4, shims are **deleted at v0.7.0**. Extension v0.2.0
(future release per ADR-0067 wave-2) prefers `cobrust lsp` directly.

### release.yml post-package smoke gate (F46 §3)

Every packaged tarball is extracted to a fresh `mktemp -d` directory
and the extracted binary runs against a one-line source file:

```bash
cd "$(mktemp -d)"
tar xzf "$ARCHIVE"
echo 'fn main() -> i64: print("smoke"); return 0' > t.cb
"$STAGE_DIR/bin/cobrust" run t.cb 2>&1 | tee out.txt
grep -q "^smoke$" out.txt || exit 1
```

Failure fails the job **before** the artifact upload step, so a broken
wheel never reaches the GH Release page.

Native-only: cross-compiled tarballs (e.g. `aarch64-unknown-linux-gnu`
built on `ubuntu-latest`) cannot execute their own binary; cross
correctness is verified by post-publish user-side install tests.

---

## Breaking change: wheel layout

Users who manually `cp cobrust /usr/local/bin/` from a flat v0.5.x
tarball MUST switch to the v0.6.0 `tar xz && ln -s` flow:

```bash
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.6.0/cobrust-v0.6.0-<triple>-<cpu>.tar.gz \
  | tar xz -C $HOME/.local/
ln -sf $HOME/.local/cobrust-v0.6.0/bin/cobrust $HOME/.local/bin/cobrust
```

Important: do NOT copy `bin/cobrust` out of its `bin/` directory. The
wheel-layout Phase 0 lookup walks up from the binary's own dir to
find `lib/` and `share/` siblings; an isolated binary breaks the
chain.

Per F46 background: the v0.5.x flat-binary wheels were unusable for
`cobrust run` regardless of where the binary was placed, because the
runtime + stdlib were not bundled. So this "breaking change" only
breaks an install pattern that was already broken.

---

## Editor extension compatibility (ADR-0068 §6)

| User on compiler | User on extension | After v0.6.0 ships | Action required |
|---|---|---|---|
| v0.5.1 | v0.1.0 | LSP works via direct `cobrust-lsp` on PATH (broken `cobrust run`; F46) | Upgrade compiler to v0.6.0 |
| v0.5.2 | v0.1.0 | LSP works (wheel bundled standalone bin); `cobrust run` broken (F46) | Upgrade compiler to v0.6.0 |
| v0.6.0 | v0.1.0 | LSP works via shim binary; subcommand `cobrust lsp` also available | Optional: upgrade extension to v0.2.0 when released |
| v0.6.0 | v0.2.0 (future) | LSP works via `cobrust lsp`; DAP works via `cobrust dap` | Ideal terminal state |
| v0.7.0 | v0.1.0 | LSP **breaks** (shim removed) | Required: upgrade extension to v0.2.0 |

---

## What did NOT change

- **The default Cranelift backend continues to be production-ready.**
  No regression in the user path that matters most (`cobrust build` /
  `cobrust run` of well-formed source).
- **LLVM `--features llvm` backend wave-3 stubs remain** — F45a still
  applies. No claim of "feature-complete" for LLVM is made in v0.6.0.
  Wave-3 closure is tracked separately in ADR-0058g.
- **The 13-handler LSP surface + 17-handler DAP surface (v0.5.0)
  remain feature-complete** — collapsing the binary entry-point shape
  does not touch the protocol surface. Editor agents experience
  unchanged code-intelligence quality.
- **No changes to `@py_compat` tier semantics, type-checker behavior,
  MIR / codegen IR, stdlib API, or LLM router routing tables.** This
  release is packaging discipline only.

---

## F-family lineage

This release closes **F46** at the empirical user-install layer AND
at the systemic CI-gate layer:

- **F35-sibling** — claim-vs-landed drift; v0.5.2 commit-msg "wheel
  bundles LSP + DAP" was technically true at the file-presence layer
  but materially false at the user-run layer.
- **F37** — silent rot on accepted debt; `env!(CARGO_MANIFEST_DIR)`
  predates Phase O wheel distribution by months but was never audited
  against the wheel layout when ADR-0065 shipped.
- **F44** — CI green != working; release.yml had no post-package
  smoke step; nothing caught the binary-cannot-find-runtime path at
  publication time.
- **F45a** — direct sibling; LLVM wave-3 stubs ship under `--features
  llvm` opt-in with no stdout-diff gate; runtime+stdlib bundle gap
  ships in the default wheel with no post-package smoke gate. Same
  "object-emit green != end-user working" class.
- **F46** — wheel not installable: runtime+stdlib gap. v0.6.0
  resolution.

The systemic detection rule from F46 §3 is now operationalized in
`.github/workflows/release.yml`: any future packaging change re-runs
the smoke gate.

---

## Sprint summary

9 atomic commits:

1. `docs(findings): F46 wheel not installable — runtime+stdlib missing (F45a sibling)`
2. `docs(adr): author 0069 wheel layout standardization (FHS bin/lib/share; closes F46)`
3. `refactor(lsp+dap): expose pub fn run() as lib entry (ADR-0068 Phase B)`
4. `feat(cli): add 'lsp' + 'dap' subcommands wired to lib entries (ADR-0068)`
5. `feat(shims): cobrust-lsp + cobrust-dap shim crates (transitional binary names; ADR-0068 §4.2)`
6. `fix(build): wheel-layout-aware runtime+stdlib lookup (FHS bin/lib/share per ADR-0069 §3)`
7. `fix(release): wheel layout bin/lib/share + post-package smoke gate (ADR-0069)`
8. `chore(release): bump workspace to v0.6.0`
9. `docs(readme+skill+release-notes+extension): v0.6.0 wheel layout + subcommand collapse honest-cite`

ADRs landed: **0068** (subcommand collapse) + **0069** (wheel layout).
Findings landed: **F46** (wheel not installable).

---

## Acknowledgements

F46 was surfaced by the user's empirical install test of the v0.5.2
wheel on Mac M1 the day v0.5.2 was published. The same install test
post-v0.6.0 release is the post-publication verification gate (see
the sprint's Phase G verification entry).

---

## Future-tense disclaimer (F35-sibling discipline)

- **Homebrew formula** — not landed; mentioned in ADR-0069 §4.4 as
  the canonical install path under FHS layout. No `brew install` URL
  exists yet.
- **Extension v0.2.0 with `cobrust lsp` + `cobrust dap` preference**
  — not landed; the v0.1.0 extension currently published on the
  marketplace continues to spawn `cobrust-lsp` (shim path).
- **v0.7.0 shim removal** — not landed; v0.6.x will continue to ship
  shims until extension v0.2.0 marketplace adoption thresholds
  documented in ADR-0068 §9.3 are met.
- **Windows wheel layout** — deferred to ADR-0058b MSVC tier-2
  promotion. v0.6.0 tier-1 covers darwin + linux only.

All four items are documented in the parent ADRs (0068, 0069) and
their respective `§Open Questions` sections. No claim of presence in
v0.6.0.
