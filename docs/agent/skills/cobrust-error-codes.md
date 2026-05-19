---
doc_kind: skill
skill_id: cobrust-error-codes
title: "Cobrust error code taxonomy with FIX hints"
audience: any LLM agent reading Cobrust compiler stderr and applying fixes
load_when: when reading or reacting to Cobrust compiler error output
last_verified_commit: 396df70
maintainers: P10/user; updated atomically with ADR-0052b
relates_to: [adr:0006, adr:0052a, adr:0052b, adr:0052d-prereq]
---

# Cobrust Error Code Taxonomy

Per ADR-0052b §2 (Direction B / CLAUDE.md §2.5): every error message includes a `help:` field with the rewrite. **Read `help:` first — it is the fix.**

```
error[TypeError::ImplicitTruthiness]: non-bool used in truthiness position: got `Int` at src/main.cb:5:8
help: change to 'if x != 0:' or 'if x.is_some():'
```

## TypeError variants

### TypeError::UnknownName

**Message**: `unknown name 'X' at <span>`
**FIX**: Check spelling. Check imports. Name may be from another module — use fully-qualified form.
```
help: did you mean '<closest_name>'?  (when a close match exists)
```

---

### TypeError::ArityMismatch

**Message**: `expected N arguments, got M at <span>`
**FIX**: Count the parameters in the function signature. Either pass fewer/more arguments, or add/remove parameters.
```cobrust
# E.g. fn f(a: i64, b: i64) -> i64  called as  f(1)
# FIX: f(1, 2)
```

---

### TypeError::KeywordArgMismatch

**Message**: `unknown keyword argument 'name' at <span>`
**FIX**: Remove the keyword or rename it to match a declared parameter name.
```cobrust
# fn f(x: i64, y: i64)  called as  f(x=1, z=2)
# FIX: f(x=1, y=2)
```

---

### TypeError::MissingArgument

**Message**: `missing required argument 'name' at <span>`
**FIX**: Supply the missing argument. If the argument is optional, add a default in the function signature.

---

### TypeError::TypeMismatch

**Message**: `type mismatch: expected 'X', found 'Y' at <span>`
**FIX**: Change the expression to match the expected type, or add an explicit conversion.
```cobrust
# Expected str, got i64
# FIX: str(n)   -- convert i64 to str
# Or:  parse_int(s) -- convert str to i64
help: "change the expression type or add ': <expected>' annotation"
```

---

### TypeError::NonExhaustiveMatch

**Message**: `non-exhaustive match: missing case(s) ["X", "Y"] at <span>`
**FIX**: Add the listed cases, or add a wildcard `_:` arm.
```cobrust
# FIX option A: add the missing arms
match shape:
    Shape::Circle(r): ...
    Shape::Rect(w, h): ...   # <- was missing

# FIX option B: add wildcard
match shape:
    Shape::Circle(r): ...
    _: default_case()
```

---

### TypeError::ImplicitTruthiness

**Message**: `non-bool used in truthiness position: got 'X' at <span>`
**FIX**: Convert to explicit boolean comparison.

| Actual type | FIX |
|---|---|
| `Int` | `if x != 0:` |
| `Float` | `if x != 0.0:` |
| `Str` | `if !x.is_empty():` or `if str_len(&x) > 0:` |
| `List[T]` | `if !x.is_empty():` |
| `Dict[K,V]` | `if !x.is_empty():` |
| `Option[T]` | `if x.is_some():` |
| `Result[T,E]` | `if x.is_ok():` |

```
help: "change to 'if x != 0:' or 'if x.is_some():'"
```

---

### TypeError::UseOfDroppedFeature

**Message**: `the form 'X' is not part of Cobrust (dropped feature) at <span>`
**FIX**: Use the Cobrust equivalent.

| Dropped feature | FIX |
|---|---|
| `try`/`except` | `Result<T, E>` + `match` |
| `async def` / `await` | use `task::spawn` / `handle.join()` |
| `is` operator | `==` for value equality; `same_object(a, b)` for identity |
| Multiple inheritance | composition + traits |
| Monkey-patching | not allowed across module boundaries |

---

### TypeError::MutableDefault

**Message**: `mutable default argument is forbidden at <span>`
**FIX**: Replace the mutable default with `Option` and initialize in the function body.
```cobrust
# WRONG
fn f(xs: list[i64] = []):  # ERROR

# FIX
fn f(xs: Option[list[i64]] = None) -> None:
    let actual = xs.unwrap_or([])
    ...
```

---

### TypeError::AmbiguousType

**Message**: `ambiguous type at <span> (consider adding an annotation)`
**FIX**: Add a type annotation to the binding or expression.
```cobrust
# WRONG
let x = []           # what type is the element?

# FIX
let x: list[i64] = []
```
```
help: "add a type annotation"
```

---

### TypeError::DuplicateField

**Message**: `duplicate field 'name' at <span>`
**FIX**: Remove the duplicate field from the struct literal or record.

---

### TypeError::NotCallable

**Message**: `not callable: 'X' at <span>`
**FIX**: The expression is not a function. Check that you're calling a `fn` item, not a value.

---

### TypeError::NotIndexable

**Message**: `not indexable: 'X' at <span>`
**FIX**: Only `list[T]`, `dict[K,V]`, `str`, and `[T; N]` arrays support `[]` indexing.

---

### TypeError::NotIterable

**Message**: `not iterable: 'X' at <span>`
**FIX**: Only `list[T]`, `dict[K,V]` (iterates keys), `str` (iterates chars), ranges, and generators are iterable.

---

### TypeError::BreakOutsideLoop

**Message**: `` `break` outside any loop at <span> ``
**FIX**: Move `break` inside a `for` or `while` loop.

---

### TypeError::ContinueOutsideLoop

**Message**: `` `continue` outside any loop at <span> ``
**FIX**: Move `continue` inside a `for` or `while` loop.

---

### TypeError::ReturnOutsideFn

**Message**: `` `return` outside any function at <span> ``
**FIX**: Move `return` inside a `fn` or method body.

---

### TypeError::YieldOutsideFn

**Message**: `` `yield` outside any function at <span> ``
**FIX**: Move `yield` inside a generator function body.

---

### TypeError::NotHashable

**Message**: `dict key type 'X' is not Hashable at <span>`
**FIX**: Use a hashable key type: `i64`, `str`, `bool`, or `None`.
```cobrust
# WRONG: f64 keys not allowed (NaN != NaN breaks hash invariant)
let d: dict[f64, str] = {1.0: "one"}   # ERROR

# FIX: use str keys
let d: dict[str, str] = {"1.0": "one"}
```

---

### TypeError::BorrowOfNonPlace

**Message**: `cannot borrow non-place expression at <span>`
**FIX**: Only borrow a name, field, or index expression — not a call result or literal.
```cobrust
# WRONG
let n = str_len(&"hello")        # can't borrow a literal
let m = str_len(&f(s))           # can't borrow a call result

# FIX
let tmp = "hello"
let n = str_len(&tmp)

let result = f(s)
let m = str_len(&result)
```
```
help: "borrow only `Name`, `Name.field`, `Name[idx]`, or `Name.method()`"
```

---

### TypeError::UnknownMethod

**Message**: `method 'method_name' not found on 'type_name' at <span>`
**FIX**: Check the method name. Use `cobrust skills get cobrust-stdlib` to see available methods.
```
help: "did you mean '<closest_method>'?"  (when a close match exists)
```

---

### TypeError::RowConflict

**Message**: `conflicting field 'field' in record types at <span>: 'T1' vs 'T2'`
**FIX**: Ensure the two record types agree on the field's type, or use a type annotation to resolve the conflict.

---

### TypeError::OccursCheck

**Message**: `occurs check: cannot unify '?N' with 'T' at <span>`
**FIX**: This signals a recursive type inference cycle. Add explicit type annotations to break the cycle.

---

## MIR-level errors

### MirError::UseAfterMove

**Message**: `use of moved value 'X' at <span>` (surfaced as MIR lowering error)
**FIX**: Borrow the value instead of moving it.
```cobrust
# WRONG: s is consumed on first use
fn count_a(s: str) -> i64:
    let n = str_len(s)
    let c = str_at(s, 0)   # ERROR: s already moved

# FIX: use &s to borrow
fn count_a(s: str) -> i64:
    let n = str_len(&s)
    let c = str_at(&s, 0)
    return n + c
```
```
help: "change to '&s' to borrow without consuming"
```

---

## Translation manifest errors (L0–L3 pipeline)

These appear in `cobrust translate` output, not in type-check output.

| Code | Meaning | FIX |
|---|---|---|
| `L0::SpecExtractionFailed` | LLM couldn't extract behavioral spec from Python source | Add more Python tests / docstrings to the target library |
| `L1::TranslationFailed` | LLM translation produced code that doesn't parse | Router retries automatically; escalates after 50 attempts |
| `L2::BuildGateFailed` | `cargo build` failed on translated output | Inspect `target/cobrust/<lib>/build.log` |
| `L2::BehaviorGateFailed` | Differential tests disagree with CPython oracle | Check `target/cobrust/<lib>/diff_failures/` |
| `L2::PerformanceGateFailed` | Benchmark < 0.8× of original | Accept with `@py_compat(semantic)` or optimize |
| `L3::DownstreamFailed` | Top-5 dependent libraries fail | File issue; mark function `@py_compat(none)` |

---

## Error UX format (ADR-0052b §2)

Every Cobrust error on stderr follows this schema:

```
error[<Variant>]: <message>
  at <file>:<line>:<col>
help: <FIX TEXT — read this first>
  (optional multi-line suggestion)
```

The `help:` line is the LLM's primary input for the next edit. **Always read `help:` before attempting a fix.**
