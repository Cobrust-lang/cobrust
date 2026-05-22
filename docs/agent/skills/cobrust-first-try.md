---
doc_kind: skill
skill_id: cobrust-first-try
title: "Write Cobrust correctly on the first try"
audience: any LLM agent (Claude Code / Cursor / OpenClaw / Hermes / Aider / OpenAI Codex / etc.)
load_when: before writing or editing any `.cb` source file
last_verified_commit: 407c1df
maintainers: P10/user; updated atomically with language-surface ADRs
---

# Skill — Write Cobrust correctly on the first try

> **Constitutional binding (CLAUDE.md §2.5)**: Cobrust is not the language most pleasant for humans to write — it is the language **LLM agents write correctly on the first try**. This skill is the agent-facing surface of that promise. Read it before producing any Cobrust source.

If you are writing Cobrust source (`.cb` files), the rules below override your priors from Python and Rust. They are surfaced so the compile-error feedback loop (your strongest correction signal) lands the smallest possible number of round-trips.

## 1. The one-paragraph onboarding

Cobrust looks like Python: indentation blocks, `def`/`fn`-style functions, `if`/`elif`/`else`, `for x in xs:`, `match`, f-strings, comprehensions, decorators. Cobrust acts like Rust: every value has an owner, mutation crosses ownership boundaries explicitly with `&s` (shared borrow), errors return `Result<T, E>` instead of throwing, types are static and structural (no `Any` default), and there is no GIL or two-color async. When Python intuition and Rust intuition conflict, **Rust wins**.

## 2. The 12 Python patterns to drop (constitution §2.2)

Each is a **compile-time error** in Cobrust. Memorize the rewrite — your first compile pass will fail without it.

| Drop | Use instead |
|---|---|
| `if x:` (implicit truthy) where `x: int` | `if x != 0:` |
| `if x:` where `x: list[T]` | `if !x.is_empty()` or `if list_len(x) > 0:` |
| `if x:` where `x: Optional[T]` | `if x.is_some():` |
| `"1" + 1` (silent coercion) | `parse_int("1") + 1` |
| `0 == False` (cross-type ==) | type error; convert explicitly |
| `is` operator | removed; use `==` for value, `same_object(a, b)` for identity |
| `try` / `except` for error handling | `Result<T, E>` + `match` or `?`-style propagation |
| `async`/`await` colored functions | one structured-concurrency runtime; no coloring |
| Mutable default args (`def f(xs=[])`) | compile error; use `Optional` + default-init in body |
| Late closure binding (`for i: lambdas.append(lambda: i)`) | explicit `copy` / `ref` / `move` capture |
| Multiple inheritance / MRO | composition + traits |
| Monkey-patching across module boundaries | forbidden; compile error |

## 3. Core syntax cheatsheet

```cobrust
# top-level fn (note `fn` keyword, return type required)
fn add(a: i64, b: i64) -> i64:
    return a + b

# explicit let binding (Rust-style annotation when needed)
fn main() -> i64:
    let s: str = input("")            # read line from stdin
    let n: i64 = str_len(&s)           # &s borrows — see §4
    let xs: list[i64] = [1, 2, 3]
    let d: dict[str, i64] = {"a": 1, "b": 2}

    # for-loop (Python form)
    for x in xs:
        print(x)
    for k, v in d.items():
        print(k, v)

    # if / elif / else (no implicit truthy)
    if n > 0:
        print("positive")
    elif n == 0:
        print("zero")
    else:
        print("negative")

    # match (Rust-style exhaustive)
    match xs.get(0):
        Some(x):
            print(x)
        None:
            print("empty")

    # f-string with precision spec
    let pi: f64 = 3.14159
    print(f"pi to 2dp = {pi:.2f}")

    return 0
```

## 4. Ownership + borrow rules (post-Wave-1, current as of `25ee43f`)

- **`str`, `list[T]`, `dict[K,V]` are non-Copy at drop time**. Reading them once moves them.
- **To read more than once, use `&s`** (immutable shared borrow). The borrow does not consume; the original binding stays usable.
- **The `&` glyph binds tighter than method-call**: `&s.method()` parses as `&(s.method())`. To borrow the receiver, write `(&s).method()` or use method-form `s.method()` directly (which receives `&s` semantically without extra glyph).
- **`clone(s)` builtin exists** as a mitigation when `&s` cannot apply (e.g. `Aggregate` constructor takes owned). Prefer `&s` over `clone(s)` whenever possible — `clone()` heap-allocates.
- **Primitives (`i64`, `f64`, `bool`, `None`) are Copy** — read freely.

```cobrust
# WRONG (consumes s on first read; second read = MirError::UseAfterMove)
fn count_a(s: str) -> i64:
    let n = str_len(s)
    let c = str_at(s, 0)   # ERROR
    return n + c

# RIGHT (borrow on both reads)
fn count_a(s: str) -> i64:
    let n = str_len(&s)
    let c = str_at(&s, 0)
    return n + c
```

## 5. Errors are values, not exceptions

```cobrust
# fns that can fail return Result<T, E>
fn parse_or_zero(s: str) -> i64:
    let r: Result<i64, ParseError> = parse_int_safe(&s)
    match r:
        Ok(v):
            return v
        Err(_):
            return 0
```

Cobrust does NOT have `try`/`except`. If you write it, the compiler rejects. Default error path is `Result<T, E>` everywhere. Sentinel-error helpers (e.g. `find(s, sub)` returns `-1`-sentinel; pre-Wave-2 idiom) still exist as PRELUDE fns and are valid.

## 6. PRELUDE surface (don't import; just call)

These names are always in scope:

```
# I/O
input(prompt: str) -> str
print(...args)                  # variadic; newline-terminated
read_line() -> str
argv() -> list[str]
read_file(path: str) -> str
write_file(path: str, content: str) -> ()
read_file_lines(path: str) -> list[str]

# String (post Phase F.3.5 — also available as method-form per 0052d-prereq)
str_len(s) / s.len() -> i64
str_at(s, i) -> str             # single-char-as-str (no `char` type yet)
split(s, sep) / s.split(sep) -> list[str]
replace(s, a, b) / s.replace(a, b) -> str
trim(s) / s.trim() -> str
find(s, sub) / s.find(sub) -> i64   # -1 sentinel
contains(s, sub) / s.contains(sub) -> bool
starts_with(s, p) / s.starts_with(p) -> bool
ends_with(s, p) / s.ends_with(p) -> bool
lower(s) / s.lower() -> str
upper(s) / s.upper() -> str

# Numeric
abs(n: i64) / n.abs() -> i64
pow(n, k) / n.pow(k) -> i64
min(a, b) / a.min(b) -> i64
max(a, b) / a.max(b) -> i64
floor(f) / f.floor() -> f64
ceil(f) / f.ceil() -> f64
is_nan(f) / f.is_nan() -> bool
is_finite(f) / f.is_finite() -> bool

# List
len(xs) / xs.len() -> i64
list_push(xs, v) / xs.push(v) -> ()
list_get(xs, i) / xs.get(i) -> T
list_set(xs, i, v) / xs.set(i, v) -> ()
list_is_empty(xs) / xs.is_empty() -> bool

# Dict (insertion-ordered per ADR-0050d, indexmap-backed)
d[k]               # panics on miss
d.get(k)           # safe; returns Option<V>
d.keys() / .values() / .items()
d.is_empty()

# Conversion
parse_int(s) -> i64    # crashes on bad input — use parse_int_safe for Result
str(n) -> str          # int/float to string

# Type-check helpers
None / Some(x) / Ok(v) / Err(e)
```

## 7. f-string format spec (Python protocol)

```cobrust
let f: f64 = 3.14159
print(f"{f}")          # "3.14159"
print(f"{f:.2f}")      # "3.14" — fixed, 2 decimals
print(f"{f:.0f}")      # "3"    — fixed, 0 decimals
print(f"{f:e}")        # scientific
let n: i64 = 42
print(f"{n}")          # "42"
print(f"{n:04d}")      # "0042" — zero-padded width
```

## 8. Compile-error → fix mapping (read your stderr)

Cobrust diagnostics carry a structured `suggestion` field. Per ADR-0052b, your stderr looks like:

```
error[TypeError]: implicit truthiness on type Int
  at src/main.cb:5:8
help: change to 'if x != 0:' or 'if x.is_some():'
```

You should **read `help:` first** — it is the rewrite. Common mappings:

- `MirError::UseAfterMove` → "change to `&s` to borrow without consuming"
- `TypeError::ImplicitTruthiness { actual: Int }` → "change to 'if x != 0:'"
- `TypeError::ImplicitTruthiness { actual: List }` → "change to '!xs.is_empty()'"
- `TypeError::TypeMismatch { expected, actual }` → "change the expression type or add `: <expected>` annotation"
- `TypeError::UnknownMethod { type_name, method_name }` → "did you mean '<closest method>'?"
- `TypeError::BorrowOfNonPlace` → "borrow only `Name`, `Name.field`, `Name[idx]`, or `Name.method()`"
- `TypeError::AmbiguousType` → "add a type annotation"

## 9. Build + run

```bash
cobrust new my_project           # scaffold
cd my_project
cobrust run src/main.cb          # build + execute
cobrust build src/main.cb        # produces native binary
cobrust check src/main.cb        # type-check only
cobrust repl                     # interactive
```

## 9a. REPL function redefinition (Phase I wave-3 / ADR-0056c)

In `cobrust repl`, a `def` (or `fn`) at the top level may **redefine** an existing binding. The REPL returns a `RedefineOutcome` value on every definition:

| Outcome | Meaning | When it appears |
|---|---|---|
| `Created` | First binding of this name in the session | Always on first `def f` |
| `Identical` | New body is byte-identical to existing body | Pasting same code twice |
| `SignatureChanged { old_sig, new_sig }` | Types changed | `def f(x: i64)` → `def f(x: str)` |

**Rules**:
- `SignatureChanged` is a **warning**, not an error. The new signature wins; callers already typed against the old signature may now fail on next typecheck.
- `Identical` silently re-binds (no message displayed to user; outcome available programmatically).
- `:type f` reflects the **new** signature immediately after any redef.
- `:clear` resets the entire session; the next `def f` produces `Created` again.

```
cobrust> def f(x: i64) -> i64:
...         return x + 1
Created: f(x: i64) -> i64

cobrust> def f(x: i64) -> i64:
...         return x * 2
Identical: f — rebinding with same signature

cobrust> def f(x: str) -> str:
...         return x.upper()
warning[RedefineOutcome::SignatureChanged]: f
  old: (x: i64) -> i64
  new: (x: str) -> str
```

## 9b. Session machinery for LSP/REPL tooling clients (Phase I wave-2 / ADR-0056b)

Agents building editors, LSP servers, or REPL front-ends interact with `cobrust_session::Session`:

```rust
// O(1) Arc-bump — safe to clone per request; no deep copy
let snapshot: Session = session.clone();

// Incremental typecheck consumers
let ctx: &TypeContext = session.type_ctx();   // borrow; updated in place after each eval

// Invalidate a single file's parse + type cache (e.g. on didChange)
session.invalidate(file_id);   // next type_ctx() access re-checks only this file
```

**Invariants**:
- `clone()` is O(1) — it Arc-bumps the internal AST/type maps. Never clones the heap.
- `invalidate(file_id)` is idempotent; safe to call on a file_id not yet in the session.
- `type_ctx()` returns a reference valid until the next `eval()` or `invalidate()` call.
- Do not cache `type_ctx()` across `eval()` boundaries — the reference may alias stale data.

## 9c. LSP integration (Phase J wave-1 / ADR-0057a)

**Start the LSP server**:
```bash
cobrust lsp           # stdio transport; editor spawns as child process
cobrust-lsp           # standalone binary (same binary, alternate entrypoint)
```

**Editor wiring** (brief; full config at `docs/human/{zh,en}/editor-setup.md`):
- **VSCode / Cursor**: add `cobrust-lsp` to `"cobrust.server.path"` in settings; language ID `"cobrust"`, file extension `.cb`.
- **Neovim** (`nvim-lspconfig`): `require('lspconfig').cobrust_lsp.setup({})` — uses stdio transport by default.

**Protocol surface (wave-1 only)**:
- `textDocument/didOpen` → triggers parse + typecheck → `textDocument/publishDiagnostics`
- 42 error variants mapped → LSP `Diagnostic`; all at severity `Error`; source field `"cobrust"`.
- Suggestion text (from `help:` field in compiler diagnostics) → `relatedInformation[0].message`.
- `textDocument/didChange`, `textDocument/hover`, `textDocument/completion` — **NOT yet implemented** in wave-1.

**LSP v1.3 is feature-complete at v0.5.0. All 13 handlers shipped.**

Wave-1: `textDocument/publishDiagnostics` (ADR-0057a). Wave-2: `didChange` + snapshot reuse (ADR-0057b). Wave-3: `hover` + `completion` + `rename` + goto-def + codeAction + cross-file rename (ADR-0057c/d/e). Wave-4: inlay hints + semantic tokens + call hierarchy (ADR-0057f). Wave-5: delta sync + resolve + cross-file refactor (ADR-0057g) — ALL CLOSED at v0.5.0 (`6b3905c`). Wave-6+: proposed.

Full 13-handler surface available to any LSP-capable editor (Cursor, Neovim, VSCode, Continue, Cody).

## 9d. DAP v1.2 — debugger protocol (Phase L wave-1 through wave-5 / ADR-0059a-g)

**DAP v1.2 is feature-complete at v0.5.0. All 17 handlers shipped.**

Wave-1: lldb pretty-printers (ADR-0059a). Wave-2: cobrust-dap server 9-handler core + cobrust debug CLI (ADR-0059b/c). Wave-3: advanced debugger UX (ADR-0059d/e). Wave-4: `evaluate` request + conditional breakpoints + multi-thread support + exception breakpoints (ADR-0059f). Wave-5: logpoints + data breakpoints + stepIn + result_err transport; 0059f §3.4 RESOLVED (ADR-0059g) — ALL CLOSED at v0.5.0. Wave-6+: proposed.

**User-facing surfaces (all available)**:
```bash
cobrust debug src/main.cb                    # interactive; all 17 DAP handlers active
cobrust debug attach <pid>                   # attach to running process
cobrust debug --breakpoint 42 src/main.cb   # stop at line 42
cobrust-dap                                  # raw DAP stdio server
```

**New in wave-4 (ADR-0059f)**:
- `evaluate` — inspect expressions in stopped frames
- Conditional breakpoints (`--condition "x > 0"`)
- Multi-thread: thread list + per-thread stack traces
- Exception breakpoints: break on `panic!` or any unhandled `Result::Err`

**New in wave-5 (ADR-0059g)**:
- Logpoints: non-breaking print on hit (`--logpoint 42 "x={x}"`)
- Data breakpoints: break on memory address write
- `stepIn` for function call stepping
- `result_err` transport: structured error on DAP response failure (0059f §3.4 RESOLVED)

Do not claim wave-6+ features exist — they are proposed, not shipped.

## 9d-jit. JIT (preview / wave-1 only) (ADR-0056a)

**Availability**:
```bash
cargo add cobrust-jit   # crate; not yet in cobrust CLI as a top-level subcommand
```

**Wave-1 supported MIR shapes**:
- Arithmetic: `Add`, `Sub`, `Mul` on `i64` / `f64`
- Simple control flow: `if`/`else` branches, unconditional `return`

**Everything else returns `JitError::UnsupportedMirFeature`** — this is a **clean rejection**, not a panic. The JIT does not crash; it signals the shape is out of scope.

```rust
match cobrust_jit::compile(&mir_fn) {
    Ok(compiled) => compiled.call(args),
    Err(JitError::UnsupportedMirFeature(shape)) => {
        // fall through to AOT path — this is expected for wave-1
        aot_execute(&mir_fn, args)
    }
    Err(e) => return Err(e.into()),
}
```

**AOT path (`cobrust build`) is canonical for production.** JIT is preview/experimental; never use it in the translation pipeline or L2 verification gates. Wave-2 scope (loops, function calls, closures) has NOT landed.

## 9e. Phase L summary — DAP v1.2 feature-complete

Phase L is TRULY FULL CLOSED at v0.5.0. See §9d for the complete handler inventory and wave-by-wave breakdown.

**Quick reference**:
- Wave-1 (0059a): lldb pretty-printers — install `cobrust_printers.py` in `~/.lldbinit`
- Wave-2 (0059b/c): `cobrust-dap` server + `cobrust debug` CLI (3-mode)
- Wave-3 (0059d/e): Str runtime §6.1 closure + advanced debugger UX
- Wave-4 (0059f): evaluate + conditional bp + multi-thread + exception bp — SHIPPED
- Wave-5 (0059g): logpoints + data bp + stepIn + result_err; 0059f §3.4 RESOLVED — SHIPPED

All 17 DAP handlers are live. Wave-6+ is proposed, not shipped.

## 9f. Phase M language-surface gaps now supported

Six surface forms that previously rejected at the type-checker or parser now compile correctly:

```cobrust
# i32 / i8 narrow-int literals (Phase M)
let x: i32 = 42i32
let y: i8 = -1i8
let z: i32 = x + 1i32

# -> None return annotation
fn side_effect(s: str) -> None:
    print(s)

# &T reference annotation in fn signatures
fn needs_ref(s: &str) -> i64:
    return str_len(s)

# [T; N] array literal syntax
let arr: [i64; 3] = [1, 2, 3]
let first: i64 = arr[0]      # static-index OK; dynamic-index pending (M follow-up)
```

**Still pending (Phase M follow-ups — not yet landed)**:
- BinOp between IntN types (e.g., `i32 + i8` without explicit widening)
- Dynamic array indexing (`arr[n]` where `n` is a variable — `#![forbid(unsafe_code)]` blocks GEP)
- Empty-dict key-flow inference (`let d: dict[str, i64] = {}`)

Do NOT write these patterns yet — they will produce a type error.

## 9g. Hardware tiering (Tier 1 / Tier 2 / Tier 3 W1)

Three-tier hardware dispatch shipped post-Phase-K:

- **Tier 1 — runtime dispatch** (ADR-0058b, SHIPPED): `cobrust build` auto-selects the best available runtime code-gen path at process startup.
- **Tier 2 — `--target-cpu`** (SHIPPED at `5186c27` / `a4c2532`): compile-time CPU feature targeting; `cobrust build --target-cpu native` enables host-specific ISA optimizations.
- **Tier 3 W1 — release.yml 9-wheel matrix** (SHIPPED at `ba5bfcb`): CI produces 9 platform wheels (linux x86_64/aarch64 musl/gnu + macOS arm64/x86_64 + Windows) on each tag.
- **Tier 3 W2-W4** (queued): `cobrust install` from registry, registry publishing, ABI hardening for stable public API.

Do not assume Tier 3 W2-W4 features exist — they are queued, not shipped.

## 9h. Cluster A let-rebind + `&p.field` SHIPPED

Explicit `&` borrow shorthand and let-rebind (CLAUDE.md §2.5 direction A) is fully operational:

- `&s` as a call-site borrow — eliminates `clone()` noise; single-direction coercion per ADR-0052a §4.4.
- `let s = &p.field` — rebind to borrow a struct field without moving; Wave-1 of let-rebind per ADR-0052a §8.
- `&s.method()` — borrow-of-call-result path unblocked per ADR-0052f + 0052g.

Empirical baseline: identified as the LARGEST LLM-friendliness deficit in LC-100 honest-debt audit (Cluster A finding). Now fully closed.

## 9i. FixSafety ladder available (ADR-0062)

All 41 error-suggestion variants (`TypeError` + `MirError` + `LoweringError`) are classified on a `FixSafety` ladder with four tiers:

- `DefinitelySafe` — apply without human review
- `LikelySafe` — apply; flag in commit message
- `NeedsReview` — propose to human; do not auto-apply
- `Structural` — architecture-level change; always escalate

LLM consumers routing via `cobrust skills get cobrust-language` receive the FixSafety tier per error variant in the structured output. Route `DefinitelySafe` fixes directly; route `Structural` fixes to P10/human.

## 9j. v0.5.0 install paths

**v0.5.0 is the current stable release** (tag `v0.5.0`, commit `6b3905c`). 10 assets: 9 wheel variants + SHA256SUMS.

```bash
# Option A — cargo install (Rust 1.94+)
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli

# Option B — prebuilt wheel (9 variants; replace <variant> with your CPU tier)
# Variants: x86_64-linux-gnu-v1 / -v3 / -v4  |  x86_64-linux-musl-v1 / -v3
#           aarch64-linux-gnu-neon / -sve      |  aarch64-apple-darwin-m1 / -m2
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-<variant>.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/

# SHA256SUMS: https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/SHA256SUMS

# Option C — cobrust install (Tier 3 auto-select; requires cobrust-cli already installed)
cobrust install <pkg>
```

Do NOT use `v0.4.0` URLs — that release is superseded by `v0.5.0`.

## 9k. LLVM backend stdlib I/O — wave-2 landed, wave-3 roadmap (ADR-0058f/g)

**Default backend = Cranelift = full stdlib parity.** `cobrust build foo.cb`
(no flag) uses Cranelift. Release wheels do NOT enable `--features llvm`.
End-users on `cobrust install` or `cargo install cobrust-cli` use Cranelift
and all externs (list, dict, input, argv, panic, fmt, iter, math, parse_int,
str methods, LLM router) work correctly today. This section only affects the
`--features llvm` **experimental** opt-in path.

**What happened**: v0.5.0 LLVM backend had a critical defect — `print("hi")`
AOT-compiled to empty stdout; `print(fib(40))` computed silently with no output.
**v0.5.1 (ADR-0058f wave-2) fixes the print system.** Default Cranelift was
never affected.

**What works in v0.5.1 LLVM AOT** (`--features llvm`):

- `print(x: i64)` → `__cobrust_println_int(i64)`
- `print(b: bool)` → `__cobrust_println_bool(i8)` (i1 → i8 widening at call site)
- `print(f: f64)` → `__cobrust_println_float(f64)`
- `print(s: str)` runtime path → `__cobrust_println_str_buf(*mut Str)`
- `print("literal")` legacy path → `__cobrust_println(ptr, len)`
- `print_no_nl(s)` runtime + literal paths
- `let s: str = "hi"; print(s)` end-to-end (Assign-side cascade)
- `print(fib(N))` end-to-end (FnRef + extern dispatch composition)

**Wave-3 stub catalogue** (compiles under `--features llvm`, silently no-ops):
See [F45a](docs/agent/findings/f45a-llvm-backend-wave3-scope-systemic.md) §2
for the full per-category table. Summary:

- **input / argv**: `input("> ")`, `read_line()`, `sys.argv` — all silent
- **list**: `list_new` / `_set` / `_get` / `_append` / `_len` / `_is_empty` — all silent
- **dict**: `dict_new` + full CRUD family — silent
- **set / tuple**: construction + access — silent
- **panic**: `panic("msg")` / `unwrap_err()` — no abort signal
- **fmt**: f-string runtime (`f"x = {x}"`) — empty string
- **iter**: `for x in [1,2,3]` body never executes
- **math**: all `math.*` intrinsics return 0 / no-op
- **parse_int / str parsing**: `int(s)`, `s == t`, `s[i]` — silent
- **str methods (ADR-0050e)**: `s.split(",")` / `.join()` / `.replace()` etc. — silent
- **LLM router**: `cobrust.llm.*` α surface (ADR-0049) — fully silent

Wave-3 closure roadmap: [ADR-0058g](docs/agent/adr/0058g-llvm-backend-wave3-stdlib-hookup-roadmap.md)
(6-wave phased plan: panic+argv → list → dict+set+tuple → input → fmt+iter+math+parse+str-methods → LLM router).
Full catalogue finding: [F45a](docs/agent/findings/f45a-llvm-backend-wave3-scope-systemic.md).

## 9l. VSCode / Cursor extension v0.1.0 staged (ADR-0067)

Editor integration scaffold at `editors/vscode-cobrust/` (Node TS, not a Rust workspace member). Wraps `cobrust-lsp` v1.3 (§9c) via `vscode-languageclient/node` stdio transport; bundles TextMate grammar + Python-like indent rules + 11 snippets (`fn` / `if` / `for` / `while` / `class` / `struct` / `match` / `matchres` / `matchopt` / `@py` / `main`).

Build + install (Node 20+):

```bash
cd editors/vscode-cobrust
npm install && npx vsce package
code   --install-extension ./cobrust-0.1.0.vsix   # VSCode
cursor --install-extension ./cobrust-0.1.0.vsix   # Cursor (VSCode-API compatible)
codium --install-extension ./cobrust-0.1.0.vsix   # VSCodium
```

Settings: `cobrust.lspPath` (default `cobrust-lsp` on `$PATH`) + `cobrust.trace.server` (`off` / `messages` / `verbose`).

Marketplace publish is **user-side action** (Azure DevOps PAT + Open VSX token required); steps documented in `editors/vscode-cobrust/PUBLISHING.md`. Do NOT claim "marketplace LIVE" until user runs `vsce publish` and `ovsx publish` and the listings resolve.

OOS for v0.1.0: DAP launch.json contribution (Phase L wave-6 follow-up), bundled binary (rejected per ADR-0067 §Options), REPL embed.

## 10. When in doubt — read the canonical example programs

`examples/leetcode-stress/` is the production-validated stress corpus: **LC-100 真 100/100** (leetcode_corpus_e2e 12/0 + stress 100/0 as of 2026-05-19). When unsure of the idiomatic form, grep there first.

`examples/leetcode/*.cb` covers problems #1-100 individually; use for per-problem reference.

## 10a. F-pattern caveats for agent authors (F35 / F36 / F37 lessons)

Three recurring drift patterns that have caused incorrect claims in this project's history. Agents writing doc-updates or status reports must avoid them:

**F35-sibling — commit-msg vs diff drift**: A commit message says "Phase X FULL CLOSED" but the diff only closes one sub-strand. Rule: commit message scope = what the diff actually contains, nothing more. If the diff closes strand A, say "close strand A", not "Phase X closed."

**F36 — fixture-name vs behavior drift**: A test file named `test_full_roundtrip.rs` does not prove full round-trip unless the test body actually exercises it. Fixture names are labels, not proof. Read the test body before citing the fixture name as evidence.

**F37 — silent rot on accepted debt**: A known failing test that is accepted-debt must be annotated `#[ignore = "<finding-id>: <one-line reason>"]`. A bare `#[ignore]` with no annotation is a finding. Agents adding ignored tests MUST include the finding ID.

**Operational rule for this skill**: when adding a "X is now supported" entry to this document, cite the ADR + commit that landed it. Do not forward-declare. If the ADR is ratified but the impl commit is not yet on main, write "queued" not "supported."

- **ADR roster** at `docs/agent/adr/README.md` — every language decision lives here.
- **Findings ledger** at `docs/agent/findings/` — empirical defects + ADSD F-pattern sediment.
- **Constitution** at `CLAUDE.md` — the non-negotiables (§2.2 drops, §2.5 LLM-first).
- **This skill** at `docs/agent/skills/cobrust-first-try.md` — updated atomically with any language-surface ADR.

## 12. Done means (onboarding checklist)

A freshly-onboarded agent is ready when it can do all of the following without a compile error:

- [ ] Write a small Cobrust program (`fn main() -> i64: return 0`) and pass `cobrust check`
- [ ] Redefine a function in `cobrust repl` and read the `RedefineOutcome` string correctly
- [ ] Run the program via `cobrust build src/main.cb` (AOT, canonical path)
- [ ] Know that `cobrust lsp` / `cobrust-lsp` exists and which editor wiring file to consult
- [ ] Know that JIT is wave-1 preview only: `Add`/`Sub`/`Mul` + simple control flow; everything else → `JitError::UnsupportedMirFeature`; fall through to AOT

## 13. Maintenance rules (for whoever updates this file)

- **One source of truth**: every claim in this file must be derivable from a specific ADR + verifiable at the latest commit. The frontmatter `last_verified_commit` must be bumped on every edit.
- **Surface drift kills the skill**: when a new ADR changes the language (new keyword, new method, dropped pattern), update §3 + §4 + §6 + §8 in the SAME commit as the ADR ratification. CI doc-coverage should flag drift.
- **Examples are load-bearing**: every example in §3-§5 must `cobrust check` clean. If it doesn't, fix the example or fix the language — never let the skill silently lie.
- **Cross-platform fidelity**: this skill is plain Markdown — no Claude-Code-specific frontmatter, no Cursor-specific tags. Any agent system can paste this into a system prompt or treat it as a `read-first` doc.

— P10 lineage 2026-05-18; seeded after Phase G Wave 2 (`25ee43f`). Refreshed 2026-05-19: added §9a fn-redef / §9b Session / §9c LSP / §9d JIT preview + §12 Done-means; Phase I FULL CLOSED + Phase J wave-1 closed (`793032d`). Refreshed 2026-05-19 (P7 maintenance #30/#34): added §9e Phase L debugger / §9f Phase M 6-gap / §10 LC-100 stress ref updated + §10a F-pattern caveats; Phase K/L/M closed + LC-100 真 100/100. Refreshed 2026-05-21 (P7 Tier-2 doc audit P0): §9c Phase J wave-2 FULL CLOSED at `53b5ed2`; §9e Phase L wave-2 landed at `171700b`+`05aa137`; added §9g Tier 1/2/3 W1 hardware tiering + §9h Cluster A let-rebind SHIPPED + §9i FixSafety ladder (ADR-0062); `last_verified_commit` sentinel → `6a25ec8`. Refreshed 2026-05-22 (P7 v0.5.0 refresh): §9c expanded wave-4+5 (LSP v1.3 feature-complete 13 handlers); §9d rewritten as DAP v1.2 wave-4+5 (17 handlers, 0059f+0059g); §9e condensed to Phase L full summary; §9j added v0.5.0 install paths; `last_verified_commit` → THIS commit SHA (post-commit update pending). Refreshed 2026-05-22 (P7 F45a+ADR-0058g sprint): §9k rewritten — default Cranelift disclosure leads; wave-3 stub catalogue (F45a §2) + ADR-0058g roadmap link; `last_verified_commit` → post-commit-4 bump pending.
