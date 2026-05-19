---
doc_kind: adr
adr_id: 0059c
parent_adr: 0059
title: "Phase L wave-3 — `cobrust debug` CLI subcommand"
status: proposed
date: 2026-05-19
last_verified_commit: 12e1d87
supersedes: []
superseded_by: []
relates_to: [adr:0059, adr:0059a, adr:0059b, adr:0058c, adr:0024]
discovered_by: P9 Phase L wave-3 dispatch eve (2026-05-19)
ratification_path: P9 sub-ADR review; ratifies on impl + tests merge to main
---

# ADR-0059c: Phase L wave-3 — `cobrust debug` CLI subcommand

## 1. Motivation

Phase L wave-1 (ADR-0059a, lldb pretty-printers, `tools/lldb-cobrust/printers.py`)
and Phase L wave-2 (ADR-0059b, `cobrust-dap` stdio DAP server) ship the two
backend surfaces a Cobrust user needs for interactive debugging — but **neither
is one-command launchable**. Today the user must:

- For terminal lldb: `cobrust build --debug fib.cb` (placeholder until 0059d) →
  `lldb-18 out.bin` → manually `command script import
  tools/lldb-cobrust/printers.py` → manually `breakpoint set ...` → `run`.
- For editor DAP: edit `.vscode/launch.json` to point at `cobrust-dap` binary
  with explicit program path + cwd + argv wiring per editor.

This is friction the LLM-first design principle (CLAUDE.md §2.5) treats as
debt: an LLM agent asked "debug this fib.cb" has to compose 4-5 shell
commands instead of 1. Wave-3 wraps the wave-1 printer assets + wave-2 DAP
binary into a single `cobrust debug` subcommand entrypoint:

```bash
cobrust debug fib.cb              # interactive lldb session
cobrust debug --dap fib.cb        # stdio DAP server forward
cobrust debug fib.cb --bp 10      # auto-breakpoint at line 10
```

Constitutional anchors:

- **CLAUDE.md §2.5** — LLM-first design (training-data overlap §B): canonical
  Python+Rust priors have a `<tool> debug <file>` shape (`python -m pdb fib.py`,
  `cargo debug` in proposals, `rust-lldb out.bin`). One-command entry matches.
- **ADR-0059 §3.3** — frame ADR defines wave-3 scope: 3-mode `cobrust debug`
  registry extension.
- **ADR-0024** — CLI registry contract; wave-3 lands as a new subcommand row.

## 2. §2.5 LLM-first design audit

| §2.5 axis | Wave-3 impact | Rationale |
|---|---|---|
| §A compile-time-catch-errors | Neutral | `cobrust debug` is a runtime tool; type/borrow errors already surface at `cobrust check`. |
| §B training-data-overlap | **Positive** | `<lang> debug <file>` shape mirrors `python -m pdb fib.py`, `gdb out.bin`, `rust-lldb out.bin`. LLM prior recognises one-command debug entry. |
| §B method-form sugar adjacency | Neutral | N/A; CLI surface only. |

Net: **§2.5 §B positive** — wave-3 makes the canonical "debug this file"
phrase resolvable in one LLM-emitted shell command instead of a 4-step
recipe. This is a meaningful UX win the LLM-prior already encodes.

## 3. Scope — 3-mode `cobrust debug` subcommand

### 3.1 Interactive mode (default)

```
cobrust debug <source.cb> [--bp <line>]...  [--lldb-path <path>]
```

Behavior:

1. Build `<source.cb>` with `OptLevel::None` + DWARF emission on (mirror
   ADR-0058c `cobrust build --debug` semantics; until that flag lands, wave-3
   forwards through the existing release=false default which yields debug
   builds with DWARF). Output to a temp file (RAII-cleaned via `tempfile::TempDir`).
2. Generate a temporary `.lldbrc` file:
   - `command script import <repo>/tools/lldb-cobrust/printers.py`
   - One `breakpoint set --file <source.cb> --line <N>` per `--bp` flag.
3. Spawn `lldb-18 -s <tmp.lldbrc> <built-binary>` with inherited stdio so
   the user lands at lldb's `(lldb)` prompt.
4. Wait for `lldb-18` to exit; forward its exit code.

### 3.2 DAP-stdio mode (`--dap`)

```
cobrust debug --dap [<source.cb>]
```

Behavior:

1. Locate the `cobrust-dap` binary alongside `cobrust` (same `target/`
   directory). Fail with `error[E0059c.1]: cobrust-dap binary not found`
   if absent.
2. Spawn `cobrust-dap` as a child process with `stdin`/`stdout`/`stderr`
   set to **inherit** (the parent's stdio is the editor's stdio per DAP
   stdio-transport convention).
3. Wait for the child to exit; forward its exit code.
4. The `<source.cb>` argument is optional in this mode (DAP `Launch`
   request carries the program path); if provided, it is passed via
   `--launch-source` for editor convenience (see §5 non-goals — wave-3
   does NOT pre-build in `--dap` mode; `Launch` handler does that).

### 3.3 Breakpoint shorthand (`--bp`)

```
cobrust debug fib.cb --bp 5 --bp 12
```

Convenience flag for the most common interactive workflow ("debug fib.cb,
stop at line 5 + line 12"). Expanded to N `breakpoint set --file fib.cb
--line N` directives in the generated `.lldbrc`. Repeatable.

Wave-3 ships line-number breakpoints only. Conditional / function-name
breakpoints stay in interactive lldb prompt scope (per ADR-0059 §4
non-goals).

## 4. Implementation

### 4.1 Subcommand registration

`crates/cobrust-cli/src/main.rs` extends the `Command` enum with a `Debug`
variant:

```rust
/// Interactive lldb / DAP-stdio debugging launcher (Phase L wave-3, ADR-0059c).
Debug {
    /// Source `.cb` file (optional only in --dap mode).
    file: Option<PathBuf>,
    /// Spawn cobrust-dap stdio server (forwarded stdio).
    #[arg(long)]
    dap: bool,
    /// Auto-set a line breakpoint (repeatable: --bp 5 --bp 12).
    #[arg(long)]
    bp: Vec<u32>,
    /// Override the lldb binary path (default: `lldb-18`, fallback `lldb`).
    #[arg(long)]
    lldb_path: Option<PathBuf>,
    /// Suppress informational stderr.
    #[arg(short, long)]
    quiet: bool,
},
```

### 4.2 Helper module: `crates/cobrust-cli/src/debug.rs` (new)

Public surface:

```rust
pub struct DebugArgs {
    pub file: Option<PathBuf>,
    pub dap: bool,
    pub bp: Vec<u32>,
    pub lldb_path: Option<PathBuf>,
    pub quiet: bool,
}

pub fn run(args: DebugArgs) -> u8;
```

Internal helpers:

- `fn run_interactive(args: &DebugArgs) -> u8` — builds source, writes
  `.lldbrc`, spawns lldb, forwards exit code.
- `fn run_dap_stdio(args: &DebugArgs) -> u8` — locates `cobrust-dap`,
  spawns as child with inherited stdio, forwards exit code.
- `fn locate_lldb(override_path: Option<&Path>) -> Result<PathBuf, DebugError>` —
  resolves to `--lldb-path`, then `lldb-18`, then `lldb` on `$PATH`.
- `fn locate_cobrust_dap() -> Result<PathBuf, DebugError>` — derives the
  `cobrust-dap` binary path from `current_exe()` parent (same target/
  directory convention).
- `fn write_lldbrc(source: &Path, breakpoints: &[u32], printer_path:
  &Path) -> Result<NamedTempFile, DebugError>` — emits the temp .lldbrc.
- `fn printer_script_path() -> PathBuf` — locates
  `<repo>/tools/lldb-cobrust/printers.py` via `CARGO_MANIFEST_DIR` parent
  traversal.

Error model:

```rust
#[derive(thiserror::Error, Debug)]
pub enum DebugError {
    #[error("source file required for interactive mode (use --dap for editor mode)")]
    MissingSource,
    #[error("source file not found: {0}")]
    SourceNotFound(PathBuf),
    #[error("lldb binary not found (tried: lldb-18, lldb); install LLVM 18 or pass --lldb-path")]
    LldbNotFound,
    #[error("cobrust-dap binary not found alongside cobrust at {0}")]
    DapBinaryNotFound(PathBuf),
    #[error("build failed: {0}")]
    BuildFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

Exit codes per ADR-0024 §"Exit-code scheme":

- `0` — success (lldb / cobrust-dap exited 0).
- `2` — user error (missing source, missing lldb, missing dap binary).
- `3` — build failure (forwarded from `build::run`).
- N — lldb / cobrust-dap non-zero exit forwarded verbatim.

### 4.3 Reuse-don't-add invariant

Per F23 / ADR-0012 (bind-the-core) and HARD-BANNED rule #1 — wave-3
adds **ZERO new Cargo dependencies**:

- `tempfile` already in cli deps (per build.rs RAII tempdir).
- `clap` already in cli deps.
- `thiserror` already in cli deps (per `error_ux::*`).
- `std::process::Command` for child-process spawning (mirrors
  `build::run`'s `cc` invocation and `cli_smoke.rs` test pattern).

No `tokio` runtime in wave-3 — interactive mode is synchronous (lldb
holds the foreground); DAP-stdio mode just forwards stdio via blocking
`std::process` (the child `cobrust-dap` has its own tokio runtime per
ADR-0059b).

## 5. Non-goals (deferred to Phase L+ or out-of-scope)

- **No direct `lldb-rs` binding**: wave-3 shells out to `lldb-18`; F-binding
  to LLDB's C++ SDK would re-open ADR-0012 bind-the-core boundary.
- **No advanced TUI / curses overlay**: lldb's own TUI (`lldb -X` or
  gui-mode) is delegated-to verbatim. Phase L+ might add `cobrust debug
  --tui` if demand surfaces.
- **No concurrent multi-target debug**: wave-3 spawns ONE debug session
  per invocation. Multi-process / fork-debug stays in lldb's own
  process-attach surface.
- **No pre-build in --dap mode**: the DAP `Launch` request carries the
  program path + builds via its own handler chain (ADR-0059b
  `handlers::launch`). Wave-3's `--dap` mode is a pure stdio forwarder.
- **No `--debug` flag activation on `cobrust build`**: that's ADR-0058c
  / a future wave; until then, default (release=false) build emits
  DWARF on supported backends, which is sufficient for wave-3
  interactive lldb sessions on examples like fib.cb.
- **No conditional / function-name breakpoints from CLI**: shell out
  inside lldb prompt for those (`(lldb) breakpoint set --condition 'i > 10'`).

## 6. Acceptance gate — 3 integration tests

`crates/cobrust-cli/tests/debug_subcommand.rs` (new), per F36
test-fixture-name = behavior:

### 6.1 `debug_help_lists_subcommand`

```rust
#[test]
fn debug_help_lists_subcommand() {
    let out = Command::new(cobrust_binary())
        .args(["debug", "--help"])
        .output()
        .expect("invoke cobrust debug --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--dap"));
    assert!(stdout.contains("--bp"));
    assert!(stdout.contains("--lldb-path"));
}
```

### 6.2 `debug_dap_stdio_initialize_disconnect_handshake`

`#[ignore]`-gated (spawns subprocess; ADR-0059b §6.2 precedent):

```rust
#[test]
#[ignore = "spawns cobrust-dap subprocess; run with --ignored on DG"]
fn debug_dap_stdio_initialize_disconnect_handshake() {
    // Spawn `cobrust debug --dap` → forwards to cobrust-dap stdio loop.
    // Send Initialize → assert response success + capability shape.
    // Send Disconnect → assert clean child exit.
}
```

This proves wave-3's `--dap` mode correctly stdio-forwards to the wave-2
DAP server. Implementation mirrors `crates/cobrust-dap/tests/dap_e2e_smoke.rs`
but spawns `cobrust debug --dap` instead of `cobrust-dap` directly.

### 6.3 `debug_missing_source_in_interactive_mode_errors_clean`

```rust
#[test]
fn debug_missing_source_in_interactive_mode_errors_clean() {
    let out = Command::new(cobrust_binary())
        .arg("debug")
        .output()
        .expect("invoke cobrust debug");
    assert!(!out.status.success());
    // Exit 2 = user error per ADR-0024.
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("source file required"));
}
```

This proves the §4.2 `DebugError::MissingSource` user-error path emits a
human-readable hint instead of clap's raw usage dump.

The ADR-0059c §6.2 test name spec was originally `cobrust debug --dap-stdio
fib.cb`; wave-3 ships the CLI flag as `--dap` (mirrors clap idiomatic
short-flag convention and the wave-2 cobrust-dap stdio transport — there
is only one DAP transport in scope today, so the disambiguator suffix is
noise). The test renaming is recorded here per F36.

## 7. Risk register

### 7.1 Cross-platform lldb path (apt vs brew)

- **Risk**: `lldb-18` is the canonical binary name on apt-based distros
  (Ubuntu 24.04 + LLVM apt repo). brew on macOS installs as
  `/opt/homebrew/opt/llvm@18/bin/lldb` (no `-18` suffix per brew naming
  convention). `cobrust debug` interactive mode needs cross-OS resolution.
- **Mitigation**: `locate_lldb()` per §4.2 tries `lldb-18` first, then
  bare `lldb`, then errors with `--lldb-path` hint. Mac users on Phase G+
  `cobrust install` add LLVM bin dir to `$PATH` (ADR-0058 §4 mac-dev
  doc); a future wave can teach the locator about
  `/opt/homebrew/opt/llvm@18/bin/lldb` explicitly.

### 7.2 Subcommand arg parsing complexity

- **Risk**: clap's `Vec<u32>` for repeatable `--bp` flag has gotchas with
  the trailing `<source>` positional argument. Phase G+ `cobrust run`
  uses `--` separator (`#[arg(last = true, allow_hyphen_values = true)]`)
  for similar shape.
- **Mitigation**: `<source>` is `Option<PathBuf>` (required only outside
  `--dap` mode). The `--bp` flag is positional-free (`#[arg(long)]`); no
  ambiguity. Test §6.1 (--help output shape) catches accidental
  positional-shadowing regression.

### 7.3 Printer script path resolution

- **Risk**: `printer_script_path()` walks `CARGO_MANIFEST_DIR` parents to
  find `tools/lldb-cobrust/printers.py`. This works for `cargo test` and
  `cargo run` (where `CARGO_MANIFEST_DIR` resolves to the crate root)
  but not for the installed `~/.cargo/bin/cobrust` binary post-`cargo
  install`. The installed binary has no `tools/` directory adjacent.
- **Mitigation**: locate via a fallback chain:
  1. `CARGO_MANIFEST_DIR/../../tools/lldb-cobrust/printers.py` (dev).
  2. `current_exe().parent()?.parent()?/share/cobrust/lldb/printers.py`
     (post-install convention; deferred to a later wave).
  3. `--printers-path` CLI override (deferred to a later wave; wave-3
     ships fallback (1) only, since wave-3's primary use case is
     in-repo `cobrust debug examples/fib.cb` dev workflow per ADR-0059
     §3.3 §1).

  Wave-3 ships fallback (1) only with a clean error if absent
  (`error[E0059c.3]: pretty-printers not found at <expected path>`);
  installer-side packaging is a Phase L+ followup.

## 8. Closure binding

ADR-0059c ratifies on the wave-3 impl + tests merge to main. The
ADR-0059 frame §3.3 row closes at the same merge; ADR-0058 §13 Phase L
row promotes to "FULL CLOSED" with all 3 waves landed (printers + DAP +
CLI).

— P9 Phase L wave-3 author, 2026-05-19
