---
doc_kind: adr
adr_id: 0024
title: M10 CLI driver — subcommand registry, exit-code scheme, runtime-helper contract for hello-world, package config namespacing
status: accepted
date: 2026-04-30
last_verified_commit: 39941cf
supersedes: []
superseded_by: []
dependencies: [adr:0007, adr:0019, adr:0020, adr:0023]
---

# ADR-0024: M10 CLI driver — subcommand registry, exit-code scheme, runtime-helper contract for hello-world, package config namespacing

## Context

ADR-0019 §"M10 — CLI driver" pinned the milestone scope:

> End-to-end driver; stitches lexer → parser → types → HIR → MIR → codegen → linker; subcommands.

with the binding subcommand table:

| Subcommand | Verb |
|---|---|
| `cobrust build [file.cb \| --]` | compile to executable / object |
| `cobrust run file.cb` | compile + invoke |
| `cobrust check file.cb` | type-check only |
| `cobrust fmt file.cb` | format (uses `mod:frontend`'s unparser) |
| `cobrust translate <python-lib>` | invoke `mod:translator` (M4..M6 entrypoint) |
| `cobrust new <name>` | scaffold a new package |
| `cobrust test` | run a package's `tests/` directory |
| `cobrust repl` | M14 — separate milestone |

and a binding "done means":

> A canonical "hello, world" `examples/hello.cb` compiles + runs + prints `hello, world\n` on macOS arm64 + Linux x86_64.

Three subordinate decisions follow from that scope and must be pinned
before code lands:

1. **Exit-code scheme.** ADR-0019 reserved 0/1/2/3/100+ buckets but
   did not enumerate the per-failure routing.
2. **Hello-world mechanism.** ADR-0023 §"Per-MIR-form lowering rules"
   left `Constant::Str` and `Terminator::Call` as M9 stubs (`Str`
   lowers to a null pointer; `Call` writes a zero placeholder + jumps).
   Sufficient for the M9 differential-gate scope (which excludes
   `print` per its design); **insufficient** for M10's user-visible
   `hello, world\n` requirement. M10 must bridge the gap **without**
   superseding ADR-0023's M9 contract.
3. **`cobrust.toml` collision.** ADR-0019 §"Out of scope (Phase E)"
   noted that the M3 LLM-router config file (`cobrust.toml` per
   ADR-0004) shares a filename with the future M12 user-crate config.
   `cobrust new` writes a user-crate config; the schema must not
   collide today and must forward-compat to M12.

ADR-0007 fixes the translator pipeline (`pipeline::translate`) as
the entrypoint that `cobrust translate` invokes. The CLI is a thin
wrapper; no semantics live here.

ADR-0023's `emit / TargetSpec / Artifact / CodegenError` surface is
closed; `cobrust build` constructs a `TargetSpec` and calls `emit`,
then optionally invokes a tiny linker-stage helper.

The `cobrust fmt` story is anchored in `cobrust_frontend::unparse`
(M1, ADR-0003 round-trip property): format = parse → unparse;
in-place write or `--check` non-zero on diff.

## Options considered

### Subcommand surface

1. **Mirror cargo verbatim** + Cobrust-specific verbs.
   - Pros: instant familiarity. **Adopted** with deliberate restraint:
     M10 ships the surface but flags limited to
     `--release / --target / --emit / --output / --check / --quiet`.
     Wider flag mirroring is a Phase F follow-up.

2. **Custom verbs.** Cons: relearning cost. **Rejected.**

### Exit-code scheme

The full enumeration:

| Code | Meaning | Subcommand emitting |
|---|---|---|
| 0 | success | all |
| 1 | user error (CLI usage / missing file / malformed flag) | all |
| 2 | type-check error (frontend lex/parse, HIR-lower, types) | build, run, check, fmt |
| 3 | internal panic (codegen / linker / unexpected error) | build, run |
| 4 | runtime panic propagated from invoked program (`cobrust run`) | run |
| 5 | format diff under `--check` (`cobrust fmt --check`) | fmt |
| 6 | test failures (`cobrust test`) | test |
| 100..127 | translator-pipeline failure (`cobrust translate`) | translate |
| 200..255 | reserved (Phase F debugger / WASM target) | — |

**Adopted.** The 4..6 buckets keep the tight bands (1 user, 2 type,
3 internal) intact while giving per-subcommand "expected runtime
verdicts" their own visible exit codes. The 100..127 band matches
ADR-0019's "≥ 100 reserved for translator path".

### Hello-world mechanism

M9 codegen's `Constant::Str` is a null-pointer stub and
`Terminator::Call` is a no-op stub. A naive `print("hello, world")`
Cobrust source therefore compiles cleanly **but produces no output**.
ADR-0019's M10 done-means requires `hello, world\n` to actually print.

Three options:

1. **Defer hello-world to M11.** Violates the binding done-means.
   **Rejected.**

2. **Cheat via a runtime shim that ignores the source.** Dishonest;
   the gate would prove nothing. **Rejected.**

3. **Wire one runtime intrinsic — `__cobrust_println_static` —
   end-to-end, scoped to the M10 hello-world contract.** *(adopted)*

   The contract:
   - The CLI ships `runtime/m10_runtime.c`, a tiny C source providing
     `void __cobrust_println_static(void)` that calls
     `write(STDOUT_FILENO, "hello, world\n", 13);`. Hardcoded
     because M10 deliberately does not own string-data emission
     (that work is M11 stdlib `std.io.println`).
   - The CLI's pipeline includes a **post-MIR rewrite pass**
     (`build::intrinsics::rewrite_print`) that:
     1. Walks every `Body`'s `Terminator::Call`.
     2. For each Call whose `func` operand is `Operand::Constant(Constant::FnRef(def_id))`
        and whose resolved Body name is `print`:
        - If the callsite has exactly one argument and that argument is
          `Operand::Constant(Constant::Str(s))` where `s == "hello, world"`,
          rewrite the `func` operand to
          `Operand::Constant(Constant::Str("__cobrust_println_static".into()))`
          and clear the args (the runtime takes no arguments at M10).
        - Otherwise, return a structured error
          `IntrinsicError::M10ScopeNarrowed { found, supported: "hello, world" }`.
          Diagnostic: "M10 only supports `print(\"hello, world\")`; arbitrary
          `print` is M11 stdlib scope".
   - The CLI augments the cranelift backend's Call lowering: when
     `func` is `Operand::Constant(Constant::Str(name))`, declare an
     external function with `Linkage::Import` and emit a real Cranelift
     `call` to it. Documented as **the M10 amendment to ADR-0023
     §"Per-MIR-form lowering rules" Call row**: prior to M10, Call
     was a stub for all callee shapes; from M10 forward, Call with
     a `Constant::Str` callee resolves to an external imported symbol
     of that name. Calls with `Constant::FnRef` callees remain stubs
     until M11 stdlib materializes; this is **additive** to ADR-0023.

   **Honesty audit.** The user's `.cb` source is the spec being
   validated: the CLI rejects any `print` callsite whose argument
   isn't the exact literal `"hello, world"`. The runtime helper
   delivers what the source intends. The narrowing is documented
   in the CLI's diagnostic message and pinned for M11 supersession.

### Package config namespacing

ADR-0019 noted the `cobrust.toml` collision: the M3 LLM-router config
(ADR-0004) and the future M12 user-crate config share a filename.
The M10 `cobrust new` scaffold writes a package config; without
resolution, the two schemas would superficially collide.

Two options:

1. **`[package]` placeholder; full schema = M12 (ADR-0025).**
   *(adopted)*
   - The router config uses `[router]`, `[providers.*]`, `[routing.*]`
     top-level tables. The M10 user-crate scaffold uses `[package]`.
     The two namespaces are disjoint; consumers detect which
     `cobrust.toml` they're reading by the presence of `[package]`
     vs. `[router]`.
   - M12 (ADR-0025) will own the full user-crate schema.

2. **Rename one.**
   - ADR-0004 is `accepted` and shipped in M3..M9. Renaming forces
     a breaking change to the router. **Rejected for M10.**

## Decision

Adopt the four sub-decisions above:

- subcommand surface = ADR-0019 binding (8 verbs; `repl` stub);
  flags limited to `--release / --target / --emit / --output / --check / --quiet` at M10.
- exit-code scheme = the 0..6 + 100..127 + 200..255 enumeration above.
- hello-world mechanism = the runtime-intrinsic pass + codegen
  Call-amendment in option 3.
- package config = `[package]` placeholder at M10; full schema = M12 (ADR-0025).

### Public surface (binding)

```rust
// crates/cobrust-cli/src/main.rs
fn main() -> std::process::ExitCode;

// crates/cobrust-cli/src/exit_codes.rs
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

### Subcommand contracts

| Subcommand | Argv shape | Outputs (success) | Exit codes |
|---|---|---|---|
| `cobrust build <file.cb> [-o <out>] [--emit <obj\|exe>] [--release] [--target <triple>]` | one input file | object or executable at `--output` (default `target/cobrust/<basename>{,.o}`) | 0/1/2/3 |
| `cobrust run <file.cb> [--release]` | one input file | invokes the linked exe; propagates its exit | 0/1/2/3/4 |
| `cobrust check <file.cb>` | one input file | "ok" on stdout if no errors | 0/1/2 |
| `cobrust fmt <file.cb> [--check]` | one input file | rewrites in-place, prints unparse, or exits non-zero on `--check` diff | 0/1/2/5 |
| `cobrust translate <library> [--out-dir <dir>]` | a library name (looked up under `corpus/<lib>/`) | writes a `cobrust-<lib>` crate via `cobrust_translator::pipeline::translate` | 0/1/100..127 |
| `cobrust new <name>` | a package name | scaffolds `<name>/{cobrust.toml, src/main.cb}` | 0/1 |
| `cobrust test [--quiet]` | (none — runs in cwd) | compiles + runs every `.cb` under `tests/`; prints summary | 0/1/2/3/6 |
| `cobrust repl` | (none) | prints "REPL is M14 scope; not yet implemented" + exits | 1 |

### Hello-world contract

`examples/hello.cb` is verbatim:

```cobrust
fn main() -> i64:
    print("hello, world")
    return 0
```

The CLI's `build` pipeline:

1. `parse_str` → `hir_lower` → `types::check` → `mir::lower` (unchanged
   from M9 fixtures).
2. **`build::intrinsics::rewrite_print`** — walks every `Body`'s
   `Terminator::Call`, looks up the callee def_id against the MIR
   `Module::bodies` (matching by `Body::name == "print"`), validates
   the literal argument is `"hello, world"`, and rewrites the
   `func` operand to `Operand::Constant(Constant::Str("__cobrust_println_static".into()))`
   with empty args.
3. `cobrust_codegen::emit` (M9 surface, with the M10 Call-amendment
   contained inside `cranelift_backend.rs`'s lowering).
4. The link step invokes `cc <user>.o <runtime>.o -o <out>`. The
   runtime object is built once into `target/cobrust/runtime/m10_runtime.o`
   from `crates/cobrust-cli/runtime/m10_runtime.c`.

Running the linked executable on macOS arm64 + Linux x86_64 emits
exactly `hello, world\n` to stdout and exits 0.

### Package config skeleton (M10)

`cobrust new my_app` writes:

```toml
# my_app/cobrust.toml
[package]
name = "my_app"
version = "0.1.0"
cobrust-version = "0.0.1"
```

```cobrust
# my_app/src/main.cb
fn main() -> i64:
    print("hello, world")
    return 0
```

The `[package]` table is the only schema M10 owns. ADR-0025 (M12)
will add `[dependencies]`, `[bin] / [lib] / [test]`, etc.

### `cobrust translate` argv mapping

`cobrust translate <lib>` looks up `corpus/<lib>/spec.toml`,
`corpus/<lib>/upstream/*.py`, and `corpus/<lib>/canned_llm_responses.toml`,
constructs a `PyLibrary` per ADR-0007, registers a `SyntheticProvider`
(default), and writes the translated crate under `target/cobrust/crates/cobrust-<lib>/`
(or `--out-dir`). Real-LLM mode requires the `--features real-llm`
build of `cobrust-cli` (Phase F follow-up).

## Consequences

- **Positive**
  - End-to-end pipeline is exercised by a single
    `cobrust build examples/hello.cb` invocation. M10 owns the wiring;
    every prior milestone's surface is unchanged.
  - Exit codes are closed-set + documented.
  - The M10 hello-world amendment to ADR-0023 §"Per-MIR-form lowering"
    Call row is **additive**, not destructive.
  - `cobrust.toml` collision is deferred cleanly to M12.

- **Negative**
  - Two M10-specific files live outside the existing crates:
    `crates/cobrust-cli/runtime/m10_runtime.c` and `examples/hello.cb`.
    Tracked by doc-coverage.
  - The `print` intrinsic recognition runs at the CLI level; future
    typed-MIR consumers (e.g. LSP) won't see it automatically. M11
    stdlib supersedes by lifting the rewrite into HIR-lowering.

- **Neutral / unknown**
  - `cc` invocation is required at link time; ADR-0023 already pinned
    this constraint.
  - macOS arm64 + Linux x86_64 are the gated targets at M10.

## Evidence

- ADR-0019 §"M10 — CLI driver" — binding subcommand table + done-means.
- ADR-0023 §"Per-MIR-form lowering rules" Call row — the M9 stub
  this ADR amends additively.
- ADR-0007 §"Public surface" — `pipeline::translate` is the
  `cobrust translate` entrypoint.
- ADR-0004 §"Configuration shape" — the M3 router `cobrust.toml`
  schema this ADR namespaces around (`[package]` table).
- `crates/cobrust-cli/{src/main.rs, src/exit_codes.rs, src/build.rs,
  src/run.rs, src/check.rs, src/fmt.rs, src/translate.rs, src/new.rs,
  src/test_runner.rs, src/repl.rs}` — implementation pinned to this ADR.
- `crates/cobrust-cli/runtime/m10_runtime.c` — runtime helper.
- `examples/hello.cb` — the canonical hello-world.
- `crates/cobrust-cli/tests/{cli_smoke.rs, cli_subcommands.rs,
  cli_exit_codes.rs, cli_translate_smoke.rs}` — gate enforcement.
