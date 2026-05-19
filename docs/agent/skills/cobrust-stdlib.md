---
doc_kind: skill
skill_id: cobrust-stdlib
title: "Cobrust stdlib API surface reference"
audience: any LLM agent calling stdlib functions in .cb source
load_when: before using stdlib functions in .cb source files
last_verified_commit: 396df70
maintainers: P10/user; updated atomically with stdlib ADRs
relates_to: [adr:0019, adr:0025, adr:0028, adr:0050d, adr:0052d-prereq]
---

# Cobrust Stdlib API Surface

All items below are in the PRELUDE — no import required. Method-form equivalents (ADR-0050e / ADR-0052d-prereq) noted where available.

## 1. I/O

```cobrust
# Console output
print(...)                           # variadic; newline-terminated
println(s: str) -> None              # explicit single string + newline

# Console input
input(prompt: str) -> str            # read line from stdin (strips trailing \n)
read_line() -> str                   # raw stdin line including \n

# File I/O
read_file(path: str) -> str          # read entire file; panics on error
write_file(path: str, content: str) -> None  # overwrite/create; panics on error
read_file_lines(path: str) -> list[str]      # lines (no trailing \n)
read_file_safe(path: str) -> Result[str, IoError]        # non-panicking variant
write_file_safe(path: str, c: str) -> Result[None, IoError]  # non-panicking

# Program args
argv() -> list[str]                  # argv[0] = executable path
```

## 2. String functions

Both function-form and method-form are valid (ADR-0052d-prereq).

```cobrust
str_len(s: &str) -> i64          # s.len()
str_at(s: &str, i: i64) -> str   # s[i] — single char as str (no char type)
split(s: &str, sep: str) -> list[str]        # s.split(sep)
replace(s: &str, a: str, b: str) -> str      # s.replace(a, b)
trim(s: &str) -> str             # s.trim() — strips both ends
trim_start(s: &str) -> str       # s.trim_start()
trim_end(s: &str) -> str         # s.trim_end()
find(s: &str, sub: str) -> i64   # s.find(sub) — returns -1 if not found
contains(s: &str, sub: str) -> bool          # s.contains(sub)
starts_with(s: &str, p: str) -> bool         # s.starts_with(p)
ends_with(s: &str, p: str) -> bool           # s.ends_with(p)
lower(s: &str) -> str            # s.lower()
upper(s: &str) -> str            # s.upper()
str_repeat(s: &str, n: i64) -> str   # s.repeat(n)
str_join(xs: &list[str], sep: str) -> str    # sep.join(xs)
```

## 3. Numeric conversion and math

```cobrust
# Conversions
parse_int(s: &str) -> i64        # panics on bad input
parse_int_safe(s: &str) -> Result[i64, ParseError]
parse_float(s: &str) -> f64      # panics on bad input
parse_float_safe(s: &str) -> Result[f64, ParseError]
str(n: i64) -> str               # int to string
str(f: f64) -> str               # float to string

# Math (from math module, all in prelude)
abs(n: i64) -> i64               # n.abs()
abs_f(f: f64) -> f64             # f.abs()
pow(n: i64, k: i64) -> i64       # n.pow(k)
pow_f(base: f64, exp: f64) -> f64  # base.pow_f(exp)
sqrt(f: f64) -> f64
sin(f: f64) -> f64
cos(f: f64) -> f64
tan(f: f64) -> f64
floor(f: f64) -> f64             # f.floor()
ceil(f: f64) -> f64              # f.ceil()
round(f: f64) -> f64             # f.round()
min(a: i64, b: i64) -> i64       # a.min(b)
max(a: i64, b: i64) -> i64       # a.max(b)
min_f(a: f64, b: f64) -> f64     # a.min(b)
max_f(a: f64, b: f64) -> f64     # a.max(b)
is_nan(f: f64) -> bool           # f.is_nan()
is_finite(f: f64) -> bool        # f.is_finite()

# Constants
PI: f64      # 3.141592653589793
E: f64       # 2.718281828459045
```

## 4. List functions

```cobrust
len(xs: &list[T]) -> i64         # xs.len()
list_push(xs: list[T], v: T) -> None  # xs.push(v) — mutates
list_get(xs: &list[T], i: i64) -> T   # xs.get(i) — panics out-of-bounds
list_get_safe(xs: &list[T], i: i64) -> Option[T]   # xs.get_safe(i)
list_set(xs: list[T], i: i64, v: T) -> None        # xs.set(i, v)
list_is_empty(xs: &list[T]) -> bool  # xs.is_empty()
list_pop(xs: list[T]) -> Option[T]   # xs.pop() — removes last
list_sort(xs: list[T]) -> list[T]    # returns sorted copy (T: Ord)
list_reverse(xs: &list[T]) -> list[T]  # returns reversed copy
list_contains(xs: &list[T], v: &T) -> bool  # xs.contains(v)
list_concat(a: list[T], b: list[T]) -> list[T]   # a + b
enumerate(xs: &list[T]) -> list[(i64, T)]        # yields (index, value) pairs
zip(a: &list[T], b: &list[U]) -> list[(T, U)]

# Slice: creates a copy
list_slice(xs: &list[T], start: i64, end: i64) -> list[T]  # xs[start:end]
```

## 5. Dict functions

Dict is insertion-ordered (indexmap-backed per ADR-0050d Decision 5).

```cobrust
d[k]                             # index — panics on miss
d.get(k) -> Option[V]            # safe get
d.keys() -> list[K]
d.values() -> list[V]
d.items() -> list[(K, V)]
d.is_empty() -> bool
d.len() -> i64                   # len(d)
d.contains_key(k: &K) -> bool
d.remove(k: K) -> Option[V]      # removes entry; returns old value
```

**Key type restrictions**: only `i64`, `str`, `bool`, `None` are valid key types.
`f64` is NOT a valid key type (NaN ≠ NaN breaks Hash invariant → `TypeError::NotHashable`).

## 6. Set functions

```cobrust
let s: set[i64] = {1, 2, 3}
s.len() -> i64
s.contains(v: &T) -> bool
s.add(v: T) -> None             # mutates
s.remove(v: &T) -> bool         # returns true if present
s.union(other: &set[T]) -> set[T]
s.intersect(other: &set[T]) -> set[T]
s.difference(other: &set[T]) -> set[T]
s.is_empty() -> bool
```

## 7. Option and Result helpers

```cobrust
# Option[T]
x.is_some() -> bool              # use instead of `if x:` on Option
x.is_none() -> bool
x.unwrap() -> T                  # panics if None
x.unwrap_or(default: T) -> T    # safe fallback
x.map(f) -> Option[U]           # transform inner value

# Result[T, E]
r.is_ok() -> bool
r.is_err() -> bool
r.unwrap() -> T                  # panics if Err
r.unwrap_or(default: T) -> T
r.ok() -> Option[T]             # convert to Option (discards Err)
```

## 8. Concurrency (ADR-0028 / M13)

No async/sync coloring. All functions are regular `fn`.

```cobrust
# Spawn a concurrent task
let handle: JoinHandle[i64] = task::spawn(fn() -> i64: return compute())
let result: i64 = handle.join()   # blocks until completion

# Bounded MPSC channel
let (tx, rx): (Sender[str], Receiver[str]) = sync::channel(capacity=10)
tx.send("hello")                  # blocks if buffer full
let msg: str = rx.recv()          # blocks until message available
```

## 9. Environment

```cobrust
argv() -> list[str]              # program arguments; argv()[0] = executable
env_var(name: str) -> Option[str]  # read environment variable
```

## 10. Error types

```cobrust
enum IoError:
    NotFound(str)            # file path
    PermissionDenied(str)
    AlreadyExists(str)
    Other(str)               # message

enum ParseError:
    InvalidInt(str)          # input that failed
    InvalidFloat(str)
    InvalidFormat(str)       # context message
```

All I/O and parse errors carry their context string. Pattern-match to recover:

```cobrust
match read_file_safe("config.toml"):
    Ok(content): parse_config(&content)
    Err(IoError::NotFound(path)): write_default_config(&path)
    Err(e): return Err(e.into())
```
