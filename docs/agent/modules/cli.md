---
doc_kind: module
module_id: mod:cli
crate: cobrust-cli
last_verified_commit: b0b69d0
dependencies: [mod:frontend, mod:hir, mod:types, mod:mir, mod:codegen, mod:translator, mod:pkg]
---

# Module: cli

## Purpose

`cobrust` binary entrypoint. Subcommand dispatch + global flags. Stitches
the M1..M9 pipeline (lex → parse → HIR → types → MIR → codegen) into an
end-to-end driver and ships the M10 hello-world contract.

## Status

- **M10 — delivered.** ADR-0024 binds the subcommand registry, exit-code
  scheme, runtime-helper contract for hello-world, and `[package]`
  placeholder for the `cobrust.toml` collision deferred to ADR-0025 (M12).
- **M11 — delivered.** ADR-0025 lifts the print-intrinsic to accept any
  string literal (via `cobrust-stdlib::io::println` runtime helper).
- **M12 — delivered.** ADR-0026 wires `cobrust build` / `cobrust test`
  to a manifest-aware package-mode driver (`mod:cli::pkg_build`) and
  adds `cobrust add <dep>` for in-place manifest editing. `cobrust new`
  scaffolds the full ADR-0026 schema (not the M10 placeholder).
- **M14 — delivered.** ADR-0029 lifts the M10 `cobrust repl` stub to
  full functionality: line editing via `rustyline = "14"`, multi-line
  input detection, tab completion against directive + keyword + stdlib
  + session-binding sources, and seven directives
  (`:type / :ast / :hir / :mir / :clear / :help / :quit`).
  Stateful HIR-interpreter evaluation for literals + arithmetic +
  comparison + boolean + var-lookup + `let`-binding. Cold start <200ms
  (~10ms release on macOS arm64). 50-session golden corpus at
  `examples/repl-session.txt`.

## Public surface (M10)

```rust
fn main() -> std::process::ExitCode;
```

The entrypoint is a [`clap::Parser`]-derived dispatcher. Subcommands per
ADR-0024 §"Subcommand contracts":

| Subcommand | Argv shape | Outputs (success) | Exit codes |
|---|---|---|---|
| `cobrust build [<file-or-dir>] [-o <out>] [--emit obj\|exe] [--release] [--target <triple>]` | optional input path | object or executable | 0/1/2/3 |
| `cobrust run <file.cb> [--release] [--target <triple>]` | one input file | invokes the linked exe | 0/1/2/3/4 |
| `cobrust check <file.cb>` | one input file | "ok" on success | 0/1/2 |
| `cobrust fmt <file.cb> [--check]` | one input file | rewrite or diff exit | 0/1/2/5 |
| `cobrust translate <library> [--out-dir <dir>]` | a library name (under `corpus/<lib>/`) | `cobrust-<lib>` crate | 0/1/100..127 |
| `cobrust new <name> [--path <dir>]` | a package name | scaffolds full ADR-0026 package | 0/1 |
| `cobrust test [--quiet]` | (none) | summary + per-test verdict (manifest-aware) | 0/1/2/3/6 |
| `cobrust add <name> [--path PATH \| --git URL --rev REV \| --version REQ] [--dev]` | a dep name + source | appends to nearest `cobrust.toml` | 0/1 |
| `cobrust repl` | (none) | interactive shell + directives (M14) | 0 |

### Exit-code constants

```rust
pub const SUCCESS: u8 = 0;
pub const USER_ERROR: u8 = 1;
pub const TYPE_ERROR: u8 = 2;
pub const INTERNAL_PANIC: u8 = 3;
pub const VERIFIER_REJECTED: u8 = INTERNAL_PANIC;  // alias; Cranelift verifier rejection = 3
pub const RUNTIME_PANIC: u8 = 4;
pub const FMT_DIFF: u8 = 5;
pub const TEST_FAILURE: u8 = 6;
pub const TRANSLATOR_BASE: u8 = 100;
pub const TRANSLATOR_MAX: u8 = 127;
```

**Cranelift verifier rejection → exit 3** (P0 CLI hardening, 2026-05-09):
`cobrust build` on a program whose generated IR fails the Cranelift verifier
exits 3 (INTERNAL_PANIC / VERIFIER_REJECTED). The propagation path:
`cranelift_backend::define_body` → `obj.define_function(...)?` →
`CodegenError::CraneliftError(detail)` → `build.rs::build` maps via
`.map_err(|e| BuildError::Internal(format!("{e}")))?` → exit 3.
Error message is on stderr; stdout is empty. See ADR-0024 §"Exit code 3 —
Cranelift verifier rejection" and `tests/cli_verifier_exit_corpus.rs` v01/v03.

### Hello-world contract (M10 → M11 supersession)

`examples/hello.cb` remains the canonical hello-world:

```cobrust
fn main() -> i64:
    print("hello, world")
    return 0
```

**M11 status (ADR-0025 §"Print-intrinsic lift")**: the M10 narrowing
to the literal `"hello, world"` is **lifted**. The CLI's
`build::intrinsics::rewrite_print` pass now accepts any string-literal
argument; the runtime symbol is `__cobrust_println` (provided by
`cobrust-stdlib::io`); codegen materializes the literal payload via
the `.rodata` interning path (ADR-0025 §"Codegen amendments"
Constant::Str row).

The end-to-end pipeline at M11:

1. The CLI prepends `fn print(s: str) -> i64` so the source type-checks.
2. After `mir_lower`, `build::intrinsics::rewrite_print`:
   - finds Call terminators whose callee resolves to a `print` Body,
   - rewrites the `func` operand to `Constant::Str("__cobrust_println")`,
   - **preserves** the literal arg so codegen can extract it,
   - drops the prelude `print` stub Body.
3. `cobrust_codegen::emit` declares `__cobrust_println` as an imported
   symbol with `(*const u8, usize)` signature, interns the literal
   payload as a `.rodata` data symbol, and emits a real Cranelift call
   passing `(ptr, len)`.
4. Codegen exports the user's `main` as `_cobrust_user_main`; the
   linker step pulls in the C runtime shim
   (`crates/cobrust-cli/runtime/cobrust_main.c`) which provides the
   platform `int main(int, char**)`, captures argv into the stdlib
   runtime, and dispatches to `_cobrust_user_main`.
5. The link step invokes
   `cc <user>.o <cobrust_main>.o <libcobrust_stdlib.a> -o <out>`
   (plus `-lpthread -ldl -lm` on Linux for std + mimalloc).
6. Running the linked executable prints `hello, world\n` to stdout
   and exits 0. The same pipeline accepts any `print(<literal>)`
   callsite — `examples/fizzbuzz.cb` exercises this.

### Package config skeleton

**M12 (ADR-0026)**: `cobrust new my_app` writes the full schema:

```toml
# my_app/cobrust.toml
[package]
name = "my_app"
version = "0.1.0"
cobrust-version = "0.0.1"
description = "A Cobrust package."
license = "Apache-2.0 OR MIT"

[dependencies]

[bin]
name = "my_app"
path = "src/main.cb"

[[test]]
name = "smoke"
path = "tests/smoke.cb"
```

The namespace is disjoint from the M3 LLM-router config (`[router]`,
`[providers.*]`, `[routing.*]`); ADR-0026 §B Option C closes the
ambiguity by rejecting on cross-load (a `[router]`-only file rejects
as `ManifestError::IsRouterConfig`; a `[package]`-bearing file is a
user crate).

### Package-mode build / test (M12)

`cobrust build` (no `.cb` argument, or a directory argument) walks up
to the nearest `cobrust.toml` and dispatches to
`mod:cli::pkg_build::run_build`:

1. `cobrust_pkg::find_manifest(cwd)` — walk up.
2. `cobrust_pkg::load_manifest(path)` — parse + validate.
3. `cobrust_pkg::Registry::open_default()` — open
   `~/.cobrust/registry/`.
4. `cobrust_pkg::resolve_and_lock(&manifest, &workspace_root, &registry)`
   — resolve deps + emit canonical lockfile.
5. `cobrust_pkg::save_lockfile(&lock, &workspace_root.join("cobrust.lock"))`
   — atomic write.
6. `mod:cli::build::build(&[bin].path, ...)` — invoke the M11 single-file
   pipeline on the bin (or lib) source.

`cobrust test` (any cwd with a manifest reachable upward) walks the
manifest's `[[test]]` array, builds + invokes each entry, and
collates pass/fail counts. The M11 dir-walking fallback engages only
when no manifest is reachable.

### Interactive REPL (M14, ADR-0029)

`cobrust repl` is an interactive shell delivered at M14 per ADR-0019
§"M14 — REPL". The implementation lives entirely in
`crates/cobrust-cli/src/repl.rs` (`pub fn run() -> u8`).

Directive table (per ADR-0029 §"Directive table (binding)"):

| Directive | Argv | Behaviour |
|---|---|---|
| `:type EXPR` | one expression | print inferred type via `mod:types::check` of `fn _t() -> _: return EXPR` |
| `:ast EXPR` | one expression | pretty-print `ast::Expr` (Debug `{:#?}`) |
| `:hir EXPR` | one expression | pretty-print `hir::Expr` after lowering |
| `:mir EXPR` | one expression | pretty-print `mir::Body` of the synthetic `_t` |
| `:clear` | (none) | drop accumulated session bindings |
| `:help` | (none) | list directives + brief usage |
| `:quit` | (none) | exit with `SUCCESS` (aliases: `:q`, `:exit`; or Ctrl-D) |

Multi-line input contract (`is_input_incomplete`):

- unbalanced parens / brackets / braces → continuation
- unterminated string literal → continuation
- last non-blank line ends with `:` and no subsequent indented body
  line → continuation (block opener)
- otherwise the input is fed to `parse_str` of a synthetic `fn _repl()`
  wrapper; `ParseError::UnexpectedEof` also triggers continuation

Tab completion sources (in priority order, all merged):

1. **Directives** (`:type / :ast / :hir / :mir / :clear / :help /
   :quit / :q / :exit`) — only when the cursor is at column 0 and
   the line begins with `:`.
2. **Keywords** (28 fixed: `fn / let / if / else / elif / for /
   while / return / match / case / class / True / False / None /
   and / or / not / in / pass / break / continue / import / from /
   as / with / try / except / raise`).
3. **Stdlib top-level seeded names** (12: `print / panic / assert /
   args / var / len / print_err / read_line / int / str / float /
   bool`).
4. **Session bindings** — every name introduced via `let X = …` in
   the current session, sorted lexically.

Evaluation surface (M14 binding per ADR-0029 §"Evaluation surface"):

| Form | Status |
|---|---|
| Integer / float / bool / string / None literals | ✅ |
| Binary arithmetic (`+ - * / %`) on numeric types | ✅ |
| Comparison (`== != < <= > >=`) | ✅ |
| Boolean (`and / or / not`) | ✅ |
| Variable read (looks up `bindings.get(name)`) | ✅ |
| `let X = EXPR` (writes `bindings.insert(name, value)`) | ✅ |
| Function calls (user-defined) | ❌ — defer to M14.1 |
| Loops / if-else / match / comprehensions | ❌ — defer to M14.1 |
| Stdlib calls (e.g. `print(...)`) | ❌ — defer to M14.1 |

Cold-start budget (per ADR-0029 §"Cold-start budget"):

- Target: < 200ms primary-prompt latency.
- Measured: ~10ms release / ~18ms debug on macOS arm64 (M2 Pro).
- Asserted in `tests/repl_smoke.rs::cold_start_budget` with 2s CI
  headroom.

History persistence: `~/.cobrust/repl_history` (1024-entry bound,
managed by rustyline).

## Done means (M14)

- [x] `cobrust repl` lifts the M10 stub to full functionality.
- [x] Seven directives delivered: `:type / :ast / :hir / :mir / :clear / :help / :quit`.
- [x] Multi-line input detection (block-opener + bracket continuation).
- [x] Tab completion against four sources.
- [x] Cold start <200ms verified.
- [x] 50-session golden corpus at `examples/repl-session.txt` replays
      successfully via `tests/repl_session_corpus.rs`.
- [x] 26 inline `repl::tests::*` + 22 `repl_smoke.rs` + 3 corpus tests
      = 51 net new M14 tests; 72 cobrust-cli tests total green.
- [x] `cli_exit_codes::ec_repl_returns_success_on_eof` updated to the
      M14 contract (EOF → SUCCESS, was M10 stub returning USER_ERROR).

## Invariants

- Exits with stable, documented status codes per ADR-0024 §"Exit-code scheme".
- All non-zero exits write a structured diagnostic to stderr.
- Never panics on invalid CLI input — invalid input → diagnostic + exit
  code, not panic.
- The `cobrust build`/`run` pipeline is purely additive on top of the
  M1..M9 surfaces; no public surface in `mod:frontend`/`mod:hir`/`mod:types`/`mod:mir`/`mod:codegen`
  is mutated by M10. The M10 amendment to ADR-0023 §"Per-MIR-form
  lowering rules" Call row is documented in ADR-0024.

## Done means (M10)

- [x] All subcommands above land except `repl` (M14 stub).
- [x] `cobrust build examples/hello.cb` produces an executable that
      prints `hello, world\n` on macOS arm64 (Linux x86_64 verified
      separately by CTO).
- [x] Exit-code scheme documented in ADR-0024.
- [x] `tests/cli_smoke.rs` enforces hello-world end-to-end.
- [x] `tests/cli_subcommands.rs` exercises build/run/check/fmt/new/help.
- [x] `tests/cli_exit_codes.rs` enforces the closed exit-code scheme.
- [x] `tests/cli_translate_smoke.rs` exercises the translate CLI surface.

## Non-goals

- No daemon mode / persistent server — every invocation is independent.
- No M14.1 evaluation surface (Turing-complete + stdlib calls) yet —
  per ADR-0029 §"Evaluation surface (M14 binding)".
- No cross-compilation matrix beyond what ADR-0023 §"Target triple matrix"
  pins (macOS arm64 + Linux x86_64 at M10).
- No arbitrary `print(s: str)` lowering at M10 — narrowed to the literal
  `"hello, world"`. M11 stdlib `std.io.println` widens.

## User-facing error pipeline (T1.4 — 0.1.0-beta)

Added at T1.4. Every internal error is mapped through `error_ux::UserError`
before reaching stderr. Raw Cranelift IR, debug `{:#?}` dumps, and
multi-thousand-line verifier output never reach the terminal.

### Four-class taxonomy

| Variant | Exit | Source | Rendered lines |
|---|---|---|---|
| `Syntax` | 2 | Lex / parse (`FrontendError`, `LexError`, `ParseError`) | ≤ 3 |
| `Type` | 2 | HIR lower (`LoweringError`), type check (`TypeError`), MIR (`MirError`) | ≤ 3 |
| `Runtime` | 4 | `cobrust run` process exit | ≤ 2 |
| `Internal` | 3 | Codegen (`CodegenError`), linker, invariant violations | ≤ 7 |

### Public API (`crates/cobrust-cli/src/error_ux.rs`)

```rust
pub enum UserError {
    Syntax    { file: PathBuf, line: u32, col: u32, msg: String, hint: Option<String> },
    Type      { file: PathBuf, line: u32, col: u32, msg: String, hint: Option<String> },
    Runtime   { msg: String, location: String },
    Internal  { internal_kind: String, repro_cmd: String },
}

impl UserError {
    pub fn exit_code(&self) -> u8;
    pub fn category(&self) -> Category;
    pub fn report_and_exit_code(&self) -> u8;  // eprintln + return exit_code
    // Convenience constructors: syntax, syntax_with_hint, type_err,
    //   type_err_with_hint, internal
}

impl Display for UserError { /* ≤ 30 lines guaranteed */ }

// From impls for every internal error type:
impl From<FrontendError> for UserError { ... }
impl From<LexError>      for UserError { ... }
impl From<ParseError>    for UserError { ... }
impl From<LoweringError> for UserError { ... }
impl From<TypeError>     for UserError { ... }
impl From<MirError>      for UserError { ... }
impl From<CodegenError>  for UserError { ... }
impl From<BuildError>    for UserError { ... }
```

### `cobrust report-bug` subcommand (`crates/cobrust-cli/src/report_bug.rs`)

```
cobrust report-bug [--include-mir] [--source-file <path>] [--out-dir <dir>]
```

- Collects: version, OS, arch, optional MIR dump (first 500 lines, paths
  stripped), optional source file.
- Writes a `cobrust-bug-<timestamp>.txt` to `--out-dir` (default: cwd).
- Prints a GitHub issue URL and a `curl` upload command.
- Exit codes: 0 on success, 1 on I/O failure.

### Wiring

`check.rs` (`cobrust check`) uses `UserError::from(e)` + `set_ue_file()` for
all error paths. `build.rs` retains `BuildError` (which has a
`From<BuildError> for UserError` impl) so that `cobrust build` can also
route through the UX layer when callers opt in.

`Internal` errors produced from `CodegenError` truncate the raw Cranelift /
LLVM message to the first line only — preventing 3000-line IR dumps.

### Invariants

- `rendered_line_count(e) <= MAX_LINES` (30) for every `UserError` variant.
- Every `Syntax` / `Type` render includes a `file:line:col` pointer (`-->`).
- Every `Internal` render includes the text `cobrust report-bug --include-mir`.
- Exit codes are stable per ADR-0024.

### Known gaps (as of 2026-05-09)

- Missing-return-path not enforced by type checker (corpus case 2 exits 0).
- `List<T>` not wired; `[].push(1)` type error not surfaced (corpus case 8 exits 0).
- Line/col from spans are byte-offset approximations until full source-map
  lands (M15).

### Test coverage

- Unit: `error_ux.rs` inline tests (4 cases).
- Integration: `tests/error_ux_corpus.rs` (11 cases — 10 corpus + Conway 4-cell).
- Existing: `tests/cli_exit_codes.rs` (6 cases) all green.

## Cross-references

- `mod:frontend` — `parse_str`, `unparse` (used by build / check / fmt).
- `mod:hir` — `Session`, `lower` (used by build / check).
- `mod:types` — `check` (used by build / check).
- `mod:mir` — `lower`, `Module`, `Terminator::Call`, `Constant::Str` /
  `Constant::FnRef` (consumed by the M10 intrinsic-rewrite pass).
- `mod:codegen` — `emit`, `TargetSpec`, `Backend`, `Artifact` (used by build / run / test).
- `mod:translator` — `pipeline::translate` (used by translate).
- ADR-0019 §"M10 — CLI driver" — milestone scope.
- ADR-0023 §"Per-MIR-form lowering rules" — M10 amendment to the Call row.
- ADR-0024 — M10 design (the stub this M14 supersedes).
- ADR-0029 — M14 design (interactive REPL).
- T1.4 — error UX rewrite for 0.1.0-beta release.
