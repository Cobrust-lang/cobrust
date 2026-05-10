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
