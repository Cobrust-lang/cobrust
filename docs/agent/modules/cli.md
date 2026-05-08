---
doc_kind: module
module_id: mod:cli
crate: cobrust-cli
last_verified_commit: TBD
dependencies: [mod:frontend, mod:hir, mod:types, mod:mir, mod:codegen, mod:translator]
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

## Public surface (M10)

```rust
fn main() -> std::process::ExitCode;
```

The entrypoint is a [`clap::Parser`]-derived dispatcher. Subcommands per
ADR-0024 §"Subcommand contracts":

| Subcommand | Argv shape | Outputs (success) | Exit codes |
|---|---|---|---|
| `cobrust build <file.cb> [-o <out>] [--emit obj\|exe] [--release] [--target <triple>]` | one input file | object or executable | 0/1/2/3 |
| `cobrust run <file.cb> [--release] [--target <triple>]` | one input file | invokes the linked exe | 0/1/2/3/4 |
| `cobrust check <file.cb>` | one input file | "ok" on success | 0/1/2 |
| `cobrust fmt <file.cb> [--check]` | one input file | rewrite or diff exit | 0/1/2/5 |
| `cobrust translate <library> [--out-dir <dir>]` | a library name (under `corpus/<lib>/`) | `cobrust-<lib>` crate | 0/1/100..127 |
| `cobrust new <name> [--path <dir>]` | a package name | scaffolds package | 0/1 |
| `cobrust test [--quiet]` | (none) | summary + per-test verdict | 0/1/2/3/6 |
| `cobrust repl` | (none) | M14 stub message | 1 |

### Exit-code constants

```rust
pub const SUCCESS: u8 = 0;
pub const USER_ERROR: u8 = 1;
pub const TYPE_ERROR: u8 = 2;
pub const INTERNAL_PANIC: u8 = 3;
pub const RUNTIME_PANIC: u8 = 4;
pub const FMT_DIFF: u8 = 5;
pub const TEST_FAILURE: u8 = 6;
pub const TRANSLATOR_BASE: u8 = 100;
pub const TRANSLATOR_MAX: u8 = 127;
```

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

`cobrust new my_app` writes:

```toml
# my_app/cobrust.toml
[package]
name = "my_app"
version = "0.1.0"
cobrust-version = "0.0.1"
```

The `[package]` table is the only schema M10 owns. ADR-0025 (M12) adds
`[dependencies]`, `[bin]/[lib]/[test]`. The namespace is disjoint from
the M3 LLM-router config (`[router]`, `[providers.*]`, `[routing.*]`),
so the shared filename does not collide today.

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

- No interactive REPL in M10 (M14 ships it).
- No daemon mode / persistent server — every invocation is independent.
- No cross-compilation matrix beyond what ADR-0023 §"Target triple matrix"
  pins (macOS arm64 + Linux x86_64 at M10).
- No arbitrary `print(s: str)` lowering at M10 — narrowed to the literal
  `"hello, world"`. M11 stdlib `std.io.println` widens.

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
- ADR-0024 — M10 design (this milestone).
