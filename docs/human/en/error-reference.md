# Cobrust Error Reference

Every Cobrust compiler error belongs to one of four categories.
The category appears in square brackets at the start of each error message, for example:

```
error[Type]: type mismatch: expected `i64`, found `str`
  --> src/main.cb:3:18
  hint: add a type annotation or fix the expression type
```

---

## Syntax errors

**When you see `error[Syntax]`**, the problem is in how your source code is written —
the lexer or parser could not understand the structure.

### Example 1 — using Python's `def` keyword

```python
# Wrong: `def` is not Cobrust syntax
def greet(name: str) -> str:
    return "hello " + name
```

```
error[Syntax]: expected end of statement, found identifier
  --> src/greet.cb:1:5
```

**Fix:** use `fn` instead of `def`.

```cobrust
fn greet(name: str) -> str:
    return "hello " + name
```

### Example 2 — unterminated string

```cobrust
fn main() -> i64:
    print("hello, world)   # missing closing "
    return 0
```

```
error[Syntax]: unterminated string literal
  --> src/main.cb:2:11
  hint: add a closing `"`
```

**Fix:** close the string with `"`.

### Example 3 — chained assignment (not supported)

```cobrust
fn main() -> i64:
    let x: i64 = 0
    let y: i64 = 0
    x = y = 1   # Python-style chain not supported
    return x
```

```
error[Syntax]: expected end of statement, found `=`
  --> src/main.cb:4:11
```

**Fix:** split into two assignments.

```cobrust
    y = 1
    x = y
```

---

## Type errors

**When you see `error[Type]`**, the types in your program are inconsistent —
the type checker or HIR lowering found a mismatch or an unresolved name.

### Example 1 — type mismatch

```cobrust
fn main() -> i64:
    let x: i64 = "hello"   # string cannot be assigned to i64
    return x
```

```
error[Type]: type mismatch: expected `i64`, found `str`
  --> src/main.cb:2:18
  hint: add a type annotation or fix the expression type
```

**Fix:** use the right literal type.

```cobrust
    let x: i64 = 42
```

### Example 2 — unknown name

```cobrust
fn main() -> i64:
    print(undefined_name)   # name not declared
    return 0
```

```
error[Type]: unknown name `undefined_name`
  --> src/main.cb:2:11
  hint: did you mean to declare it with `let undefined_name = …`?
```

**Fix:** declare the variable before using it.

```cobrust
    let undefined_name: str = "hello"
    print(undefined_name)
```

### Example 3 — implicit truthiness (not allowed)

```cobrust
fn main() -> i64:
    if 1:            # i64 cannot be used as a bool condition
        print("yes")
    return 0
```

```
error[Type]: cannot use `i64` as a boolean condition
  --> src/main.cb:2:8
  hint: Cobrust requires an explicit bool — try `if x != 0:` or `if x.is_some():`
```

**Fix:** write an explicit comparison.

```cobrust
    if 1 != 0:
        print("yes")
```

### Example 4 — silent coercion (not allowed)

```cobrust
fn main() -> i64:
    let x: i64 = 1 + "two"   # cannot add i64 and str
    return x
```

```
error[Type]: type mismatch: expected `i64`, found `str`
  --> src/main.cb:2:22
  hint: add a type annotation or fix the expression type
```

**Fix:** use consistent types.

```cobrust
    let x: i64 = 1 + 2
```

### Example 5 — ownership / borrow errors

```
error[Type]: use of moved value `_x` after it was moved
  --> src/main.cb:5:10
  hint: each value can only be used once after being moved
```

**Likely fix:** clone the value before moving it, or restructure to avoid
using it after the move.

### Example 6 — unknown field on a class instance

A `class`'s fields are tracked by the type checker (ADR-0080). Accessing a
field the class does not declare is a compile-time error — never a runtime
`KeyError`. The message lists the declared fields so you can pick the right
one (or fix a typo).

```
error[Type]: no field `nonexistent` on `Score`; declared fields: name, rank
  --> src/main.cb:6:14
```

```cobrust
class Score:
    let name: str = ""
    let rank: i64 = 0

fn f() -> i64:
    let s = Score()
    return s.nonexistent   # ERROR — see message above

# Fix: use a declared field. Its type is known statically:
#   s.rank is i64, s.name is str.
fn f() -> i64:
    let s = Score()
    return s.rank
```

**Likely fix:** access one of the listed declared fields. Field types are
known at compile time, so a wrong-typed use (e.g. `s.name + s.rank`, str + i64)
is also caught as a type mismatch.

---

### Example 7 — unsupported refinement `where`-predicate

A validated request body (`route_validated`, ADR-0080) may carry a per-field
`where`-clause. Only the FIXED refinement forms are accepted; any other
predicate is a compile-time error that prints the accepted forms so you can
rewrite it on the next try.

The four accepted forms are:

- **i64 int-range** — `0 <= self and self <= 100` (inclusive)
- **f64 float-range** — `0.0 <= self and self <= 1.0` (inclusive `<=`/`>=`
  **only** — a strict `<`/`>` is rejected, because the reals are dense and
  there is no clean `±1` rewrite)
- **str length** — `len(self) <= n` (or `len(self) >= n`)
- **str pattern** — `pattern(self, "<regex>")`

```
error[Type]: unsupported refinement `where`-predicate on field `rank`: use one
of the fixed refinement forms — an i64 int-range `0 <= self and self <= 100`
(inclusive); an f64 float-range `0.0 <= self and self <= 1.0` (inclusive
`<=`/`>=` ONLY — a strict `<`/`>` is rejected, the reals are dense); a str
length `len(self) <= n` (or `len(self) >= n`); or a str pattern
`pattern(self, "<regex>")`
  --> src/main.cb:3:20
```

```cobrust
class CreateScore:
    name: str
    rank: i64 where weird(self)   # ERROR — not a fixed refinement form

# Fix: a fixed inclusive int-range bound on the i64 field.
class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100
```

**Likely fix:** rewrite the `where`-clause into one of the four fixed forms
above that matches the field's type (an int-range on `i64`, an inclusive
float-range on `f64`, a `len(self)` bound or a `pattern(self, …)` on `str`).

---

### Example 8 — `len(x)` on a non-sized type

The Python-canonical free-function `len(x)` works on any **sized** value — a
`str`, a `list[T]`, or a `dict[K, V]` — and returns an `i64`:

```cobrust
let n1: i64 = len("hello")     # 5
let n2: i64 = len([1, 2, 3])   # 3
let n3: i64 = len(d)           # entry count for a dict
```

Calling `len` on a number (or any non-sized value) is a compile-time error
whose message names the accepted types — it does **not** mislead you toward a
dict (an ADR-0088 §2.5-B fix):

```
error[Type]: `len(x)` needs a sized argument but got `i64`: the free-function
`len` accepts a `str`, a `list[T]`, or a `dict[K, V]` (for a number use a
comparison; `len` is not defined on `i64`)
  --> src/main.cb:2:18
```

```cobrust
let bad = len(5)        # ERROR — i64 is not sized

# Fix: use a comparison for a number; use len on a sized value.
let xs: list[i64] = [1, 2, 3]
let ok = len(xs)        # 3
```

The Rust-style method-form `s.len()` / `xs.len()` also works and agrees with
the free `len(s)` / `len(xs)` exactly — for a `str` both are the Python
codepoint count (F91 / ADR-0103: `len("é") == 1`, not the UTF-8 byte length).

**Likely fix:** pass a `str` / `list` / `dict` to `len`; for a number, compare
it directly (`x >= 0`) instead of taking its length.

---

## Runtime errors

**When you see `error[Runtime]`**, the program itself panicked or the
`cobrust run` driver encountered a problem executing the compiled binary.

```
error[Runtime]: process exited with status 1
  --> cobrust run
```

This is usually an assertion failure or an unhandled `Result::Err` in
your program. Add `print` calls or use the REPL (`:mir EXPR`) to
inspect state.

---

## Internal errors

**When you see `error[Internal]`**, the *compiler* has encountered a bug —
not your code. You cannot fix this by editing source.

```
error[Internal]: CraneliftError: inst441 has type i64, expected i8

  This is a compiler bug.  Please collect a bug report and file a GitHub issue:

    cobrust report-bug --include-mir

  Repro command: cobrust build src/main.cb
```

**What to do:**

1. Run `cobrust report-bug --include-mir --source-file src/main.cb`.
2. Open the printed GitHub URL and attach the generated `.txt` report.
3. As a workaround, simplify your program while the bug is being fixed.

**Note:** the Conway-toy bug (3000-line Cranelift IR dump in early
0.1.0-beta sessions) was fixed in ADR-0033. If you saw that error before,
update to the latest build — it should now show a clean `error[Internal]`
with a `cobrust report-bug` hint instead of the raw IR dump.

---

## Quick lookup table

| Symptom | Category | Exit code | Action |
|---|---|---|---|
| `def f():` not recognised | `Syntax` | 2 | Use `fn f():` |
| String not terminated | `Syntax` | 2 | Add closing `"` |
| `let x: i64 = "hi"` | `Type` | 2 | Match types |
| `if x:` where x is i64 | `Type` | 2 | Write `if x != 0:` |
| `undefined_name` in expression | `Type` | 2 | Declare with `let` |
| `s.typo` on a class instance | `Type` | 2 | Use a declared field (the error lists them) |
| `len(5)` on a non-sized value | `Type` | 2 | Pass a `str` / `list` / `dict`; compare numbers directly |
| Program panics at runtime | `Runtime` | 4 | Debug program logic |
| Cranelift / linker failure | `Internal` | 3 | Run `cobrust report-bug` |

---

## Translated-crate errors (untrusted-input hardening)

The translated ecosystem crates enforce safety limits on untrusted input. These
errors are returned as `Result::Err` — they never panic or abort the process.

### `cobrust-nest` — `TomliError`

| Condition | Error message | Why |
|---|---|---|
| TOML nesting depth > 100 | `"nesting depth exceeds maximum (100); possible adversarial input"` | Adversarial deeply-nested arrays / inline tables would overflow the call stack without this cap (B4 fix). |
| Invalid syntax | `"unexpected character '…' at pos N"` | Standard parse error. |
| Unterminated string | `"unterminated string"` | Standard parse error. |

**Constant**: `nest::MAX_DEPTH = 100` (exported; callers can reference it).

### `cobrust-strike` — `HttpError` / `HttpErrorKind`

| `HttpErrorKind` | Meaning | Action |
|---|---|---|
| `InvalidUrl` | URL did not parse or scheme is unsupported | Check the URL string. |
| `Network` | DNS, TCP, or TLS failure | Check connectivity / certificates. |
| `Timeout` | Transport timed out (default: 30 s) | Retry or increase timeout. |
| `DecodeBody` | Response body is not valid UTF-8 or JSON | Check server's `Content-Type`. |
| `BodyTooLarge` | Response body exceeded 64 MiB cap | The server sent too much data; use streaming or raise the cap (B5 fix). |

**Constant**: `strike::MAX_BODY_BYTES = 64 * 1024 * 1024` (64 MiB).

### `cobrust-scale` — `MsgError` / `MsgErrorKind`

| `MsgErrorKind` | Meaning | Action |
|---|---|---|
| `Pack` | Value could not be encoded (out of M6 scope). | Check the `MsgValue` variant. |
| `Unpack` | Malformed or truncated msgpack bytes. | Check the input bytes. |
| `OverflowSize` | `pos + length` overflowed `usize` — likely adversarial input with a crafted length field near `u32::MAX`. | Reject the input; it is not a valid msgpack payload (B6 fix). |

---

## Error messages print the FIX (ADR-0052b)

Since Phase G Wave 2 (ADR-0052b), every compiler diagnostic carries a
machine-structured `suggestion` field that names the specific FIX path,
not just the diagnosis. The CLI renderer surfaces this as a `hint:`
line; LSP / `--emit-json` consumers (planned) read the `&'static str`
field directly.

### Why this matters

CLAUDE.md §2.5 binds Cobrust as "the language LLM agents write
correctly on the first try". An LLM agent consuming stderr should
extract the FIX deterministically, without prose-stripping:

```
error[Type]: cannot use `Int` as a boolean condition
  --> src/main.cb:3:8
  hint: change to `if x != 0:` (use `.is_some()` for Option)
```

The `hint:` text is now a `&'static str` literal populated at the
error-construction site, identical across all triggers of the same
variant. The fix path is reproducible, structured, and LLM-friendly.

### Three properties

- **Construction-time write**. Each `Err(TypeError::Foo { ... })` site
  in the compiler populates `suggestion` with the most actionable fix
  string at that call site.
- **Static `&'static str`**. Suggestion text is a compile-time literal —
  no dynamic format arguments. The primary error line still carries the
  failing identifier (`unknown name \`foo\``) so LLM stderr parsing
  retains it; the suggestion text is generic-and-actionable.
- **Renderer is structural**. The CLI's `error_ux.rs` `From<...>` impls
  read `suggestion.map(str::to_owned)` directly — no per-variant prose
  hard-coded at render time.

### Covered error types

- `cobrust_types::TypeError` — 31 variants (every S-class variant
  carries `Some(...)`; class-N variants such as `Multiple` carry
  `None`). Includes `LenArgNotSized` (ADR-0088) — `len(x)` on a
  non-sized value.
- `cobrust_mir::MirError` — 10 variants (use-after-move, borrow
  conflicts, drop-schedule violations).
- `cobrust_hir::LoweringError` — 6 variants (unknown name, dropped
  feature, mutable default, duplicate binding).

Future Direction-B extensions to `cobrust_frontend::{LexError,
ParseError}` are tracked but out-of-scope for Wave-2.

## Fix-safety ladder (ADR-0062)

Every suggestion now also carries a `fix_safety` tier the LSP code-
action layer and JSON diagnostic emit consume to decide whether the
suggestion is safe to auto-apply. The six tiers from lowest-risk
(always-safe-to-auto-apply) to highest-risk (never-auto-apply):

| Tier | Wire form | Auto-apply behaviour |
|---|---|---|
| FormatOnly | `format-only` | Applied on save / format pass |
| BehaviorPreserving | `behavior-preserving` | Apply on user accept |
| LocalEdit | `local-edit` | Apply on user accept (may require adjacent test update) |
| ApiChanging | `api-changing` | Suggest only — no one-click apply |
| TargetChanging | `target-changing` | Diagnostic only — never auto-apply |
| RequiresHumanReview | `requires-human-review` | Diagnostic only — manual review required |

### When each tier appears

- **BehaviorPreserving**: `if x:` → `if x != 0:`; mutable-default → `None`-default rewrite; f64 dict key → `.to_bits() as i64`. Compiler-mandated rewrites that preserve user intent.
- **LocalEdit**: typo fixes (`UnknownName`), arity / keyword mismatches, type-annotation adds (`AmbiguousType`), `break`/`continue`/`return` placement. Call-site or one-binding fixes.
- **RequiresHumanReview**: `OccursCheck` (recursive type), `UseOfDroppedFeature` (use a different construct), `DictSpreadNotSupported` (wait for Phase G), `MirError::EscapingBorrow` / `DoubleDrop` (lifetime restructuring).

### LSP code-action gating

When connected to a Cobrust LSP server (`cobrust-lsp` binary), the
editor's "quick fix" menu shows code actions only for tiers
`FormatOnly` / `BehaviorPreserving` / `LocalEdit`. `ApiChanging`
suggestions surface as `Refactor` (suggest-only). `TargetChanging` /
`RequiresHumanReview` suggestions appear in the diagnostic message
but generate no code action — the agent must reason about the fix.
