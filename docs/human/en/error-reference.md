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
| Program panics at runtime | `Runtime` | 4 | Debug program logic |
| Cranelift / linker failure | `Internal` | 3 | Run `cobrust report-bug` |

---

## Translated-crate errors (untrusted-input hardening)

The translated ecosystem crates enforce safety limits on untrusted input. These
errors are returned as `Result::Err` — they never panic or abort the process.

### `cobrust-tomli` — `TomliError`

| Condition | Error message | Why |
|---|---|---|
| TOML nesting depth > 100 | `"nesting depth exceeds maximum (100); possible adversarial input"` | Adversarial deeply-nested arrays / inline tables would overflow the call stack without this cap (B4 fix). |
| Invalid syntax | `"unexpected character '…' at pos N"` | Standard parse error. |
| Unterminated string | `"unterminated string"` | Standard parse error. |

**Constant**: `cobrust_tomli::MAX_DEPTH = 100` (exported; callers can reference it).

### `cobrust-requests` — `HttpError` / `HttpErrorKind`

| `HttpErrorKind` | Meaning | Action |
|---|---|---|
| `InvalidUrl` | URL did not parse or scheme is unsupported | Check the URL string. |
| `Network` | DNS, TCP, or TLS failure | Check connectivity / certificates. |
| `Timeout` | Transport timed out (default: 30 s) | Retry or increase timeout. |
| `DecodeBody` | Response body is not valid UTF-8 or JSON | Check server's `Content-Type`. |
| `BodyTooLarge` | Response body exceeded 64 MiB cap | The server sent too much data; use streaming or raise the cap (B5 fix). |

**Constant**: `cobrust_requests::MAX_BODY_BYTES = 64 * 1024 * 1024` (64 MiB).

### `cobrust-msgpack` — `MsgError` / `MsgErrorKind`

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

- `cobrust_types::TypeError` — 24 variants (every S-class variant
  carries `Some(...)`; class-N variants such as `Multiple` carry
  `None`).
- `cobrust_mir::MirError` — 10 variants (use-after-move, borrow
  conflicts, drop-schedule violations).
- `cobrust_hir::LoweringError` — 6 variants (unknown name, dropped
  feature, mutable default, duplicate binding).

Future Direction-B extensions to `cobrust_frontend::{LexError,
ParseError}` are tracked but out-of-scope for Wave-2.
