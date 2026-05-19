---
doc_kind: skill
skill_id: cobrust-first-try
title: "Write Cobrust correctly on the first try"
audience: any LLM agent (Claude Code / Cursor / OpenClaw / Hermes / Aider / OpenAI Codex / etc.)
load_when: before writing or editing any `.cb` source file
last_verified_commit: current-main
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

**Scope caution**: Phase J wave-1 is closed; Phase J wave-2+ (hover, completion, rename) has NOT landed. Do not assume those capabilities exist.

## 9d. JIT (preview / wave-1 only) (ADR-0056a)

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

## 9e. Debugger (Phase L wave-1 / ADR-0059a/b/c)

**Phase L wave-1 is closed.** Three user-facing surfaces:

**lldb pretty-printers** (0059a): Install once, then `cobrust` types print readably in lldb/gdb.
```bash
# Enable pretty-printers in ~/.lldbinit (done once):
command script import /path/to/cobrust/tools/lldb/cobrust_printers.py

# Then in lldb:
(lldb) p my_list   # prints: CobList<i64>[1, 2, 3]  (not raw memory)
(lldb) p my_dict   # prints: CobDict{"a": 1, "b": 2}
```

**cobrust-dap server** (0059b): DAP protocol over stdio; attach any DAP-capable editor.
```bash
cobrust-dap           # starts DAP server on stdio
# Neovim nvim-dap / VSCode launch.json: "type": "cobrust", "request": "launch"
```

**cobrust debug CLI** (0059c): Command-line debugging without an editor.
```bash
cobrust debug src/main.cb          # launch with debugger attached; interactive
cobrust debug attach <pid>         # attach to running process
cobrust debug --breakpoint 42 src/main.cb   # stop at line 42
```

**Scope caution (honest-debt per ADR-0059a §6)**: Wave-1 does NOT include runtime frame variable inspection, Dict iteration display, or Option Adt DI. These are queued wave-2+. Do not assume them.

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

— P10 lineage 2026-05-18; seeded after Phase G Wave 2 (`25ee43f`). Refreshed 2026-05-19: added §9a fn-redef / §9b Session / §9c LSP / §9d JIT preview + §12 Done-means; Phase I FULL CLOSED + Phase J wave-1 closed (`793032d`). Refreshed 2026-05-19 (P7 maintenance #30/#34): added §9e Phase L debugger / §9f Phase M 6-gap / §10 LC-100 stress ref updated + §10a F-pattern caveats; Phase K/L/M closed + LC-100 真 100/100.
