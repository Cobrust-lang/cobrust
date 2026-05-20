---
doc_kind: skill
skill_id: cobrust-language
title: "Cobrust language core syntax reference"
audience: any LLM agent writing or reviewing .cb source files
load_when: before writing or editing any .cb source file
last_verified_commit: 396df70
maintainers: P10/user; updated atomically with language-surface ADRs
relates_to: [adr:0051, adr:0052a, adr:0052b, adr:0052d-prereq]
---

# Cobrust Language Core Syntax

Cobrust = Python ergonomics + Rust ownership + static structural types + AI-native correctness.
**Constitutional north star (CLAUDE.md §2.5)**: designed so LLM agents write it correctly on the first try.

## 1. Function definition

```cobrust
# fn keyword required; return type required
fn add(a: i64, b: i64) -> i64:
    return a + b

# -> None for side-effect functions
fn greet(name: str) -> None:
    print(f"Hello, {name}!")

# &T reference parameter (borrow without consuming)
fn measure(s: &str) -> i64:
    return str_len(s)
```

## 2. Variable binding

```cobrust
let x: i64 = 42              # explicit type
let y = 42                   # inferred i64
let pi: f64 = 3.14159
let flag: bool = True
let s: str = "hello"
let xs: list[i64] = [1, 2, 3]
let d: dict[str, i64] = {"a": 1, "b": 2}
let maybe: Option[i64] = Some(5)
let nothing: Option[i64] = None
```

## 3. Control flow

```cobrust
# Requires bool — NO implicit truthiness
if n > 0:
    print("positive")
elif n == 0:
    print("zero")
else:
    print("negative")

# For-loop (Python form)
for x in xs:
    print(x)

for i, v in enumerate(xs):
    print(f"{i}: {v}")

for k, v in d.items():
    print(f"{k}={v}")

# While loop
while n > 0:
    n = n - 1
```

## 4. Ownership + borrows (ADR-0052a)

Key rule: `str`, `list[T]`, `dict[K,V]` are non-Copy. Reading once moves them.
Use `&s` (shared borrow) to read without consuming.

```cobrust
# WRONG: second use of s = MirError::UseAfterMove
fn count_a(s: str) -> i64:
    let n = str_len(s)
    let c = str_at(s, 0)   # ERROR — s already moved
    return n + c

# RIGHT: borrow on each read
fn count_a(s: str) -> i64:
    let n = str_len(&s)
    let c = str_at(&s, 0)
    return n + c
```

`&` borrow rules:
- `&s` = immutable shared borrow; original stays usable
- `&` binds tighter than method: `&s.method()` = `&(s.method())`; use `(&s).method()` to borrow receiver
- Method-form `s.method()` receives `&s` semantically without extra glyph (ADR-0050e)
- Primitives (`i64`, `f64`, `bool`, `None`) are Copy — no borrow needed
- `clone(s)` exists as fallback when `&s` cannot apply; heap-allocates; prefer `&s`

Wave-1 admitted borrow operand shapes (ADR-0052a §8):
- `&ident`              — `&s`
- `&p.<N>`              — `&p.0` / `&p.1` tuple-field projection
- `&xs[i]`              — index-projection borrow
- `&(ident)`            — parenthesised identifier

Let-rebind shortcut (ADR-0052a §4.4) — the §2.5-honest replacement
for `let s = clone(s)`:

```cobrust
fn main() -> i64:
    let s = input("")
    let s = &s            # outer `s: str` -> inner `s: &str`
    let n = str_len(s)    # transparency: &str admitted at `s: str` slot
    let m = str_len(s)
    return n + m
```

The new `s` shadows the outer binding in the same scope; the type
narrows from `str` to `&str`. Sub-patterns inside tuple / dict /
class patterns still reject same-name duplicates with
`DuplicateBinding`.

## 5. match (exhaustive)

```cobrust
match xs.get(0):
    Some(x):
        print(x)
    None:
        print("empty list")

match result:
    Ok(v):
        return v
    Err(e):
        return 0

# Wildcard pattern
match direction:
    "north": move_north()
    "south": move_south()
    _:       print("unknown direction")
```

## 6. Structs and classes

```cobrust
# Struct (value type)
struct Point:
    x: f64
    y: f64

let p = Point { x: 1.0, y: 2.0 }

# Class (with methods)
class Counter:
    count: i64

    fn new() -> Counter:
        return Counter { count: 0 }

    fn increment(&self) -> None:
        self.count = self.count + 1

    fn get(&self) -> i64:
        return self.count
```

## 7. Enums

```cobrust
enum Color:
    Red
    Green
    Blue

enum Shape:
    Circle(f64)          # radius
    Rect(f64, f64)       # width, height

match shape:
    Shape::Circle(r):
        return pi * r * r
    Shape::Rect(w, h):
        return w * h
```

## 8. @py_compat decorator

Declares Python-compatibility tier. Required on every stdlib/translated module item.

```cobrust
@py_compat(strict)                    # byte-exact Python match
@py_compat(numerical(rtol=1e-7))      # within float tolerance
@py_compat(semantic)                  # same behavior, different representation
@py_compat(none)                      # no Python compatibility claim

@py_compat(strict)
fn str_strip(s: str) -> str:
    return s.trim()
```

## 9. f-string format spec

```cobrust
let f: f64 = 3.14159
let n: i64 = 42
print(f"{f}")          # "3.14159"
print(f"{f:.2f}")      # "3.14" — fixed 2 decimals
print(f"{f:.0f}")      # "3"
print(f"{f:e}")        # scientific notation
print(f"{n}")          # "42"
print(f"{n:04d}")      # "0042" — zero-padded width 4
```

## 10. Result and error handling

```cobrust
# No try/except — use Result<T, E>
fn parse_or_zero(s: str) -> i64:
    let r: Result[i64, ParseError] = parse_int_safe(&s)
    match r:
        Ok(v): return v
        Err(_): return 0

# ? propagation (where return type is Result)
fn load_config(path: str) -> Result[Config, IoError]:
    let text = read_file_safe(&path)?   # propagates Err automatically
    return parse_config(&text)
```

## 11. Comprehensions

```cobrust
let squares = [x * x for x in xs]
let evens = [x for x in xs if x % 2 == 0]
let str_map = {k: str_len(&v) for k, v in d.items()}
```

## 12. Dropped Python patterns (compile errors in Cobrust)

| Drop | Use instead |
|---|---|
| `if x:` where `x: i64` | `if x != 0:` |
| `if x:` where `x: list[T]` | `if !x.is_empty():` |
| `if x:` where `x: Option[T]` | `if x.is_some():` |
| `"1" + 1` | `parse_int("1") + 1` |
| `is` operator | `==` for value; `same_object(a,b)` for identity |
| `try`/`except` | `Result<T, E>` + `match` |
| Mutable default args | `Option` + default-init in body |
| Multiple inheritance | composition + traits |
| Monkey-patching across modules | compile error — forbidden |
| `async def` / `await` | one structured-concurrency runtime; no coloring |

## 13. Narrow integer types (Phase M)

```cobrust
let x: i32 = 42i32
let y: i8 = -1i8

# &T reference annotation in fn signatures
fn needs_ref(s: &str) -> i64:
    return str_len(s)

# [T; N] array literal
let arr: [i64; 3] = [1, 2, 3]
let first: i64 = arr[0]
```

## 14. CLI quick reference

```bash
cobrust new my_project           # scaffold
cobrust run src/main.cb          # build + execute
cobrust build src/main.cb        # AOT binary
cobrust check src/main.cb        # type-check only (no codegen)
cobrust fmt src/main.cb          # format
cobrust skills list               # list available skill docs
cobrust skills get <name>         # fetch skill doc to stdout
cobrust skills get <name> --json  # JSON form for programmatic use
```
