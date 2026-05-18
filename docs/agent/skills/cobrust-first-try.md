---
doc_kind: skill
skill_id: cobrust-first-try
title: "Write Cobrust correctly on the first try"
audience: any LLM agent (Claude Code / Cursor / OpenClaw / Hermes / Aider / OpenAI Codex / etc.)
load_when: before writing or editing any `.cb` source file
last_verified_commit: 25ee43f
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

## 10. When in doubt — read the canonical example programs

`examples/leetcode/*.cb` is the largest reference corpus (LeetCode #1-100 in Cobrust). When unsure of the idiomatic form, grep there first.

## 11. Where to file findings / follow-ups

- **ADR roster** at `docs/agent/adr/README.md` — every language decision lives here.
- **Findings ledger** at `docs/agent/findings/` — empirical defects + ADSD F-pattern sediment.
- **Constitution** at `CLAUDE.md` — the non-negotiables (§2.2 drops, §2.5 LLM-first).
- **This skill** at `docs/agent/skills/cobrust-first-try.md` — updated atomically with any language-surface ADR.

## 12. Maintenance rules (for whoever updates this file)

- **One source of truth**: every claim in this file must be derivable from a specific ADR + verifiable at the latest commit. The frontmatter `last_verified_commit` must be bumped on every edit.
- **Surface drift kills the skill**: when a new ADR changes the language (new keyword, new method, dropped pattern), update §3 + §4 + §6 + §8 in the SAME commit as the ADR ratification. CI doc-coverage should flag drift.
- **Examples are load-bearing**: every example in §3-§5 must `cobrust check` clean. If it doesn't, fix the example or fix the language — never let the skill silently lie.
- **Cross-platform fidelity**: this skill is plain Markdown — no Claude-Code-specific frontmatter, no Cursor-specific tags. Any agent system can paste this into a system prompt or treat it as a `read-first` doc.

— P10 lineage 2026-05-18; skill seeded after Phase G Wave 2 round 2 partial closure (`25ee43f`).
