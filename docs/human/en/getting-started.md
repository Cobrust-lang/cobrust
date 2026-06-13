# Getting started — 30-second install

## Step 1: install

**Option A — cargo install** (requires Rust toolchain):

```bash
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
# (crates.io publish queued for v0.2.0)
```

**Option B — prebuilt binary** (no Rust needed):

```bash
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/

# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

Verify: `cobrust --version` → `cobrust 0.1.2`

## Step 2: hello, world

```bash
cobrust new hello && cd hello && cobrust run src/main.cb
```

Expected output:

```
hello, world
```

## Step 2.5: for loop (M-F.3.1)

Cobrust ships Python-style `for ... in ...` loops over `list[T]` and the
prelude `range(start, stop)` helper. Per ADR-0050b, `range(start, stop)`
materialises a `list[i64]` containing `start, start+1, ..., stop-1`;
empty ranges (`start >= stop`) skip the body.

```cobrust
fn main() -> i64:
    # Forward range: prints 0 1 2 3 4
    for i in range(0, 5):
        print(i)

    # Empty range: body never executes
    for i in range(0, 0):
        print(-1)

    # Iteration over a list
    let xs: list[i64] = list_new(3)
    let _0 = list_set(xs, 0, 10)
    let _1 = list_set(xs, 1, 20)
    let _2 = list_set(xs, 2, 30)
    for v in xs:
        print(v)        # 10  20  30

    # Iteration over argv (list[str])
    for arg in argv():
        print(arg)

    return 0
```

Phase F.3 ships the 2-argument `range(start, stop)` form. The 3-argument
`range(start, stop, step)` form is deferred to Phase G alongside the
full iterator protocol. String iteration (`for c in "hello":`) is also
Phase G work — see ADR-0050b §"Iter source type checking".

Loop semantics:
- Loop variables rebind fresh each iteration; closures captured inside
  the body see the iter-N value when created at iter N (constitution
  §2.2 — no Python-style late-binding).
- Nested `for` is legal; var shadowing follows Rust rules.
- `for x in 42:` and other non-`list[T]` iter sources are rejected at
  type-check (`TypeError::NotIterable`).

See [examples/for_range.cb](../../../examples/for_range.cb) and
[examples/for_list.cb](../../../examples/for_list.cb) for runnable
demos.

## Step 2.6: f64 and `as`-cast (M-F.3.3)

Cobrust ships first-class `f64` (IEEE-754 double precision). Explicit
`as` casts are required between `i64` and `f64` — no silent coercion
(constitution §2.2).

```cobrust
fn main() -> i64:
    # Float literals
    let x: f64 = 3.14
    let y: f64 = 1e-3
    let big: f64 = inf      # IEEE 754 infinity
    let nothing: f64 = nan  # IEEE 754 NaN

    # Explicit as-cast: i64 → f64 and f64 → i64
    let n: i64 = 42
    let f: f64 = (n as f64)        # 42.0
    let back: i64 = (3.9 as i64)   # 3 (truncates toward zero)

    # Math intrinsics (all return f64)
    let s: f64 = sqrt(4.0)         # 2.0
    let p: f64 = pow(2.0, 10.0)    # 1024.0
    let fl: f64 = floor(3.7)        # 3.0
    let ce: f64 = ceil(3.2)         # 4.0
    let ro: f64 = round(2.5)        # 3.0
    let ab: f64 = abs(-5.5)         # 5.5

    # f-string float formatting
    print(f"{x:.2f}")               # "3.14"
    print(f"{sqrt(2.0):.4f}")       # "1.4142"

    return 0
```

Key rules:
- `i64 → f64` requires explicit `(n as f64)`. No implicit promotion.
- `f64 → i64` truncates toward zero (C semantics, not floor).
- `//` (floor division) FLOORS toward -∞, Python-style: `-7 // 2 == -4`,
  `7 // -2 == -4`. `/` on integers TRUNCATES toward zero (`-7 / 2 == -3`).
  `%` matches `//` so `(a // b) * b + (a % b) == a` for every sign (ADR-0099).
- `inf / 0.0` is not a trap — IEEE 754 defines float division by zero.
- `nan != nan` is `true` per IEEE 754.
- `print(<float expr>)` works inline: `print(7.0 / 2.0)` prints `3.5`. An
  integer-valued float prints WITHOUT a trailing `.0` (`print(7.0 + 2.0)` →
  `9`, not `9.0`) (ADR-0089).
- Math functions: `sqrt`, `floor`, `ceil`, `round`, `abs`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`.
- f-string format spec: `{x:.2f}` (fixed), `{x:e}` (scientific), `{x:g}` (general).

## Step 2.7: list[str] and Str ownership (M-F.3.2)

Cobrust now ships `list[str]` end-to-end with the Rust-style ownership
schedule mandated by ADR-0050c. Per constitution §2.3, every Str is an
owning value (the slot of a `list[str]` owns its element); the compiler
auto-drops at scope exit, mirroring Rust's `String`.

```cobrust
fn main() -> i64:
    # Literal list[str] — each element materialised on the heap.
    let xs: list[str] = ["alpha", "beta", "gamma"]
    for s in xs:
        print(s)                       # alpha beta gamma
    # xs drops here: each Str slot freed, then the list container.

    # `list_is_empty` is the §2.2-mandated emptiness predicate
    # (`if xs:` is rejected as implicit truthiness).
    let empty: list[str] = []
    if list_is_empty(empty):
        print("empty branch")
    else:
        print("non-empty branch")     # not reached

    return 0
```

What changed at M-F.3.2:
- `Ty::Str` and `Ty::List(_)` are non-Copy in the MIR drop pass
  (ADR-0050c §"Phase 1"). The codegen emits `__cobrust_str_drop` for
  Str slots and `__cobrust_list_drop_elems` for `list[str]` at every
  reachable scope exit.
- The `list_len` / `list_get` / `list_set` / `list_new` / `list_is_empty`
  intrinsics are row-polymorphic — they accept `list[T]` for any
  element type, not just `list[i64]`.
- For-loop `for s in xs:` over a `list[str]` clones each slot into the
  loop variable (`__cobrust_str_clone`) so the loop binding owns its
  own copy; the slot's ownership stays with `xs`.

Compile-rejected (per ADR-0050c "Decision"):
- Use after move for Str-typed locals (`let a = s; let b = s` requires
  an explicit clone — Phase G surfaces a `clone(s)` builtin).

Known honest-debt (per Phase 2a walk-back):
- `list[T]` is Copy at the operand level, so passing a `list[str]` to
  a fn that takes it by value DOES NOT compile-reject the post-call
  use; double-use is allowed today and Phase G's explicit borrow
  syntax will close this.

See `crates/cobrust-cli/tests/list_str_e2e.rs` for the end-to-end
corpus and `crates/cobrust-stdlib/tests/list_str_drop_corpus.rs` for
the C-ABI link-time tests.

## Step 2.7a: explicit `&s` borrow (ADR-0052a Phase G, Wave-1)

Per CLAUDE.md §2.5 (Direction A) and ADR-0052a §2, Cobrust ships
explicit immutable shared-borrow syntax `&s` so the LLM-friendly
fix path for the LC-100 multi-read pattern is **`&s`**, not
`clone(s)`.

The use case: reading a Str local twice today moves the Str on the
first read and surfaces `MirError::UseAfterMove` on the second.
The `&s` form constructs a shared borrow that the PRELUDE Str
helpers accept transparently:

```cobrust
fn main() -> i64:
    let s = input("")
    # Multiple borrowed reads — `s` is never consumed.
    let n = str_len(&s)
    let i: i64 = n - 1
    while i >= 0:
        let c = str_at(&s, i)
        print_no_nl(c)
        i = i - 1
    print("")
    return 0
```

Wave-1 admits three borrow shapes (per ADR-0052a §8):
- `&ident`            — `&s`
- `&ident.field`      — `&p.0` / `&p.1` tuple-field projection, or
                         `&record.name` once ADT fields land
- `&ident[idx]`       — `&xs[0]`

Plus the **let-rebind shortcut** (ADR-0052a §4.4):

```cobrust
fn main() -> i64:
    let s = input("")
    let s = &s              # let-rebind: outer `s` (str) → inner `s` (&str)
    let n = str_len(s)
    let m = str_len(s)
    return n + m
```

`let s = &s` is the §2.5-honest replacement for `let s = clone(s)`.
The new `s` shadows the outer binding for the rest of the scope; the
type narrows from `str` to `&str` and PRELUDE transparency continues
to admit it at call-arg positions.

Parse-rejected (Wave-1 scope cap):
- `&"literal"`        — literal-borrow deferred (future sub-ADR).
- `&[1, 2, 3]`        — collection-literal borrow deferred.
- `&call(...)`        — call-result borrow deferred.
- `&&s`               — nested borrow deferred (Phase H).
- `&mut s`            — mutable borrow deferred (Phase H).

How the type checker handles this (ADR-0052a §3): `&s` synthesises
type `&Str`. PRELUDE Str helpers (`str_len(s: str)`, `str_at(s: str,
i: i64)`, etc.) accept `&Str` via a **one-way call-site coercion**
— the type checker locally drops the `&` wrapper at the call-arg
binding position. The coercion is unidirectional (`&Str → Str`,
never `Str → &Str`) and scoped to call-arg only. Annotation slots,
arithmetic, and `if`-conditions still reject `&T` ≠ `T` mismatches:

```cobrust
fn main() -> i64:
    let s: str = "hi"
    let n: i64 = &s        # rejected: TypeMismatch (annot is i64, &s is &Str)
    let total = (&n) + (&s) # rejected: TypeMismatch (arithmetic)
    return 0
```

Why `&` was chosen over `clone(s)`, `borrow(s)`, or `ref s` — see
the cross-reference in `design-philosophy.md` §"Why `&s` not
`clone(s)`".

## Step 2.8: string stdlib (M-F.3.5)

Eleven PRELUDE fns make Cobrust usable for daily string-processing
programs — log parsing, CSV slicing, simple text transforms (per
[ADR-0050e](../../agent/adr/0050e-string-stdlib-m-f-3-5.md)).

Surface:

- `split(s: str, sep: str) -> list[str]`
- `join(parts: list[str], sep: str) -> str`
- `replace(s: str, old: str, new: str) -> str`
- `trim(s: str) -> str` (whitespace, both sides)
- `find(s: str, needle: str) -> i64` (`-1` if absent — see idiom below)
- `contains(s: str, needle: str) -> bool`
- `starts_with(s: str, prefix: str) -> bool`
- `ends_with(s: str, suffix: str) -> bool`
- `lower(s: str) -> str` / `upper(s: str) -> str`
- `clone(s: str) -> str` (deep-copy; LC-100 honest-debt mitigation)

#### Python-named string methods (ADR-0085, the §2.5-recommended spelling)

Cobrust is a Python successor (§2.1) and the language LLM agents write
correctly on the first try (§2.5). An LLM writing Python reaches for
`s.strip()` / `s.startswith()` / `s.endswith()`, not the Rust-named
`trim` / `starts_with` / `ends_with`. So the **Python names are the
canonical spelling**; the Rust names stay accepted (non-breaking — they
keep existing `.cb` programs working) but are documented as deprecated
aliases.

Six methods added:

- `s.strip()` — strip whitespace from both ends (equivalent to
  `s.trim()`; CPython `'  hi  '.strip() == 'hi'`).
- `s.lstrip()` — strip the LEFT side only (`'  hi  '.lstrip() == 'hi  '`).
- `s.rstrip()` — strip the RIGHT side only (`'  hi  '.rstrip() == '  hi'`).
- `s.startswith(p) -> bool` — equivalent to `s.starts_with(p)`.
- `s.endswith(p) -> bool` — equivalent to `s.ends_with(p)`.
- `s.count(sub) -> i64` — NON-overlapping count (CPython
  `'banana'.count('a') == 3`; `'aaa'.count('aa') == 1`, not 2).

```cobrust
fn main() -> i64:
    let s: str = input("")        # "  hello  \n"
    print(s.strip())              # "hello"
    let n: i64 = "banana".count("a")
    print(n)                      # 3
    if "hello".startswith("he"):
        print(1)                  # 1
    return 0
```

`strip` / `startswith` / `endswith` are pure aliases: the MIR rewrite
routes them to the SAME runtime symbol as the Rust twin
(`__cobrust_str_trim` / `__cobrust_str_starts_with` /
`__cobrust_str_ends_with`) — no new shim. `lstrip` / `rstrip` / `count`
are new shims (`__cobrust_str_lstrip` / `_rstrip` / `_count`). All
semantics are differentially verified against CPython 3.11. Deferred to
a follow-up: `join` / `title` / `capitalize` / `zfill` / `splitlines` /
`isdigit`.

Example (`hello_csv.cb`):

```cobrust
fn main() -> i64:
    let line: str = "alpha,beta,gamma"
    let parts: list[str] = split(line, ",")
    for p in parts:
        let _ = print(upper(p))
    return 0
```

```bash
cobrust run hello_csv.cb
# ALPHA
# BETA
# GAMMA
```

`find` returns `i64` with the `-1` sentinel (Decision 5 / Q2). The
documented idiom is `if pos != -1:`, NOT `if find(...):` — Cobrust
forbids implicit truthy/falsy (§2.2). Worked example:

```cobrust
let pos: i64 = find("hello world", "world")
if pos != -1:
    print(pos)
else:
    let _ = print("not found")
```

`clone(s)` is the LC-100 honest-debt mitigation. Because every Str
parameter is Move-consumed under ADR-0050c, a multi-use pattern like
`let n = str_len(s); let c = str_at(s, 0)` is rejected as
use-after-move. Insert `clone()` so each surface call gets a fresh
buffer (the original `s` is preserved for the final use):

```cobrust
let s: str = input("")
let n: i64 = str_len(clone(s))      # consumes a fresh clone of s
let i: i64 = n - 1
while i >= 0:
    let c: str = str_at(clone(s), i)  # another fresh clone per read
    let _ = print(c)
    i = i - 1
let _ = print(upper(s))              # final use; no clone needed
return 0
```

What changed at M-F.3.5:
- 11 new PRELUDE stubs in `crates/cobrust-cli/src/build.rs`; eleven
  matching intrinsic-rewrite arms in `intrinsics.rs` route each call
  to the C-ABI shim `__cobrust_str_<fn>`.
- `crates/cobrust-stdlib/src/string.rs` ships the ten new C-ABI
  shims (`__cobrust_str_clone` was already shipped with ADR-0050c).
- Rust-side `string::strip` renamed to `string::trim` (Decision 4).

Edge cases (per ADR-0050e Decision 8):
- `split("", ",") -> [""]` (singleton)
- `split(s, "") -> [s]` (Rust-style empty-sep behavior)
- `join([], sep) -> ""`
- `replace(s, "", new)` inserts `new` at every byte position
- `find(s, "") -> 0`
- `contains(s, "") -> true` (universal sub-needle)

See `crates/cobrust-cli/tests/string_stdlib_e2e.rs` for the
end-to-end corpus and `crates/cobrust-stdlib/src/string.rs` for the
C-ABI shim definitions.

## Step 2.9: dict (M-F.3.4)

Cobrust dicts mirror Python's mental model: `{}` is dict (not set),
insertion-order iteration (Python 3.7+ guarantee), `d[k]` panics on
missing key, `.get(k, default)` is the safe-escape idiom. The Phase
F.3 surface is locked by ADR-0050d; sub-sprint a+b lands the parser +
type checker + dict_is_empty intrinsic; **sub-sprint c+d (this
milestone) wires codegen + `indexmap::IndexMap<KeyEnum, ValueEnum>`
backing + type-dispatched `__cobrust_dict_{set,get,contains}_K_V`
shims, so dict literals, `d[k]` reads, `d[k] = v` writes, `key in d`
membership tests, and `len(d)` length queries are now runtime-shipped**;
sub-sprint e wires `for k in d:` / `d.items()` / `d.keys()` /
`d.values()` iter desugar + `.get()` method dispatch (some tests
remain `#[ignore]` pending that).

```cobrust
fn main() -> i64:
    # Literal: empty {} is dict, not set.
    let empty: Dict[str, i64] = {}
    let scores: Dict[str, i64] = {"alice": 90, "bob": 85, "carol": 92}

    # Indexing read — panics on missing key.
    let a: i64 = scores["alice"]                   # 90

    # Indexing write — rebind or insert.
    scores["dave"] = 78

    # Membership — `in` returns bool; canonical workaround for `not in`.
    if "alice" in scores:
        print("found alice")
    if not ("zoey" in scores):
        print("zoey absent")

    # dict_is_empty (canonical predicate — `if d:` is rejected by §2.2).
    if dict_is_empty(empty):
        print("empty is empty")

    # Method-intrinsic surface (recognised at type-check; codegen lands
    # in sub-sprint d/e per ADR-0050d):
    let ks: List[str] = scores.keys()              # insertion order
    let vs: List[i64] = scores.values()
    let kvs: List[Tuple[str, i64]] = scores.items()
    let v: i64 = scores.get("alice")               # 90
    let safe: i64 = scores.get("missing", 0)       # 0 (sentinel-pair scope cap)
    let copy: Dict[str, i64] = scores.copy()       # shallow clone

    # Comprehension.
    let xs: List[i64] = [1, 2, 3]
    let squares: Dict[i64, i64] = {x: (x * x) for x in xs}

    return 0
```

Key rules (M-F.3.4 / ADR-0050d):
- `{}` is empty dict (matches Python; set literal requires `set()`
  ctor — Phase G).
- `d[k]` panics + aborts on missing key (matches Python's `KeyError`
  but using Rust's abort path — see `__cobrust_dict_keyerror_abort`).
  Use `d.get(k, default)` for the safe-escape (no Option lowering at
  Phase F.3 — Phase F.3-late or Phase G adds typed Option).
- `key in d` returns `bool` (Decision 4A). The canonical idiom for
  negated membership is `not (k in d)` — `BinOp::NotIn` Pratt-loop
  bookkeeping is a Phase G follow-up.
- `len(d)` returns `i64` (Decision 5A — uniform with list/str).
- `dict_is_empty(d)` is the `bool` predicate canonical per
  constitution §2.2 implicit-truthy ban (no `if d:`).
- Iteration is insertion-order (Decision 6A — backed by
  `indexmap::IndexMap` post-sub-sprint d). **Implementation detail
  (sub-sprint d landed)**: `__cobrust_dict_new(k_tag, v_tag, len)`
  reinterprets the `k_size`/`v_size` arguments as type tags (0=i64,
  1=str); `__cobrust_dict_set_K_V` / `__cobrust_dict_get_K_V` /
  `__cobrust_dict_contains_K` dispatch on the static (K, V) shape;
  the legacy untyped `__cobrust_dict_*` symbols remain aliased to the
  (i64, i64) variants for M12.x backward compat.
- Type parameters: `K ∈ {i64, str}` for Phase F.3; reject `f64`
  keys at type-check (NaN != NaN breaks Hash invariants — see
  `TypeError::NotHashable`).
- `d.copy()` is shallow clone (Decision 10A).
- `{**other}` dict-spread is Phase G — Phase F.3 rejects at
  `TypeError::DictSpreadNotSupported`.

Compile-rejected (M-F.3.4):
- `Dict[f64, V]` and `Dict[List[T], V]` (non-hashable K) — see
  `TypeError::NotHashable` taxonomy.
- `let d = {}` (no annotation, no use site that pins K/V) →
  `TypeError::AmbiguousType` at the final resolution pass. Annotate
  explicitly.
- `if d:` (implicit truthiness) — use `dict_is_empty(d)` or
  `len(d) > 0`.
- `def f(d: Dict[K, V] = {})` (mutable default) — same rule as
  `list = []` (ADR-0006).
- `{"a": 1, **other}` (spread in dict literal) — dict-merge is
  Phase G.

See `crates/cobrust-cli/tests/dict_e2e.rs` for the end-to-end corpus
(many ignored pre-sub-sprint c/d codegen close) and the dict block
in `crates/cobrust-types/tests/well_typed.rs` w116..w145 for the
type-checker surface.

## Step 2.10: file IO (M-F.3.6)

Cobrust now ships 7 flat source-level functions for file and stdio IO
([ADR-0050f](../../agent/adr/0050f-file-io-completion-m-f-3-6.md)).

```cobrust
fn main() -> i64:
    # Write a file; returns 0 on success (i64-sentinel Q1).
    let rc: i64 = write_file("/tmp/hello.txt", "hello, cobrust\n")
    if rc != 0:
        return rc

    # Read entire file as a str.
    let contents: str = read_file("/tmp/hello.txt")
    let _ = print(contents)           # prints: hello, cobrust

    # Read as list[str] — each line stripped of \n / \r\n (Q2).
    let lines: list[str] = read_file_lines("/tmp/hello.txt")
    let n: i64 = list_len(lines)
    print(n)                      # prints: 2 (trailing empty elem)

    # Append to an existing file; creates if absent (Q3).
    let rc2: i64 = append_file("/tmp/hello.txt", "more text")

    # Read all stdin until EOF.
    let stdin_data: str = stdin_read_all()

    # Write to stdout WITHOUT trailing newline (differs from print).
    let rc3: i64 = stdout_write("no newline here")

    # Write to stderr WITHOUT trailing newline; stdout unchanged.
    let rc4: i64 = stderr_write("error note")

    return 0
```

### 7 functions at a glance

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `read_file` | `(path: str) -> str` | file contents as str | Empty str on I/O error (i64-sentinel Q1). |
| `read_file_lines` | `(path: str) -> list[str]` | lines stripped of `\n`/`\r\n` | Trailing empty element preserved (Q2): `"a\nb\n"` → `["a","b",""]`. |
| `write_file` | `(path: str, contents: str) -> i64` | `0` = success, `1` = I/O error | Creates or truncates. Both args consumed (Move). |
| `append_file` | `(path: str, contents: str) -> i64` | `0` = success, `1` = I/O error | Creates if absent (Q3). Both args consumed. |
| `stdin_read_all` | `() -> str` | stdin until EOF | Empty str on EOF. |
| `stdout_write` | `(s: str) -> i64` | `0`/`1` sentinel | No trailing newline; differs from `print`. |
| `stderr_write` | `(s: str) -> i64` | `0`/`1` sentinel | Goes to stderr only; stdout unchanged. |

### i64-sentinel idiom

`write_file` / `append_file` / `stdout_write` / `stderr_write` return
`0` on success, non-zero on failure. The pattern:

```cobrust
let rc: i64 = write_file("/tmp/out.txt", "data")
if rc != 0:
    return rc   # propagate error
```

`read_file` returns an empty `str` on error (no separate sentinel — bare
str return per Q1). Use `str_len(contents)` to distinguish empty file
from read failure.

### `read_file_lines` trailing-empty-element rule (Q2)

`read_file_lines(p)` splits on `\n` using `s.split('\n')` semantics —
NOT Python's `readlines()`. A file ending with `\n` always has a
trailing empty string element:

```
"alpha\nbeta\ngamma\n" → ["alpha", "beta", "gamma", ""]  (4 elements)
"a\nb"                 → ["a", "b"]                       (2 elements)
""                     → [""]                              (1 element)
```

Count matches `s.count('\n') + 1` for any file content.

### `print` vs `stdout_write` (ADR-0050f cross-surface table)

| Call | Trailing newline? | i64 return |
|---|---|---|
| `print("literal")` | yes | always 0 |
| `print(s: str)` | yes | always 0 |
| `print_no_nl(s)` | no | always 0 |
| `stdout_write(s)` | no | 0 = success, 1 = error |
| `stderr_write(s)` | no | 0 = success, 1 = error |

`print` / `print_no_nl` are "fire and forget"; `stdout_write` /
`stderr_write` surface the write result for programs that need to
detect a closed pipe.

What changed at M-F.3.6:
- 7 new PRELUDE stubs: `read_file`, `read_file_lines`, `write_file`,
  `append_file`, `stdin_read_all`, `stdout_write`, `stderr_write`.
- 7 new C-ABI shims at `crates/cobrust-stdlib/src/io.rs`.
- 7 new intrinsic-rewrite arms in `crates/cobrust-cli/src/build/intrinsics.rs`.
- Copy-at-operand discipline for str args (ADR-0050c Phase 2a walk-back
  precedent): shims READ the Str buffer without freeing; caller scope owns drop.
- Phase G: method-form sugar `stdin().read_all()` / `stdout().write(s)` deferred
  until MIR method dispatch lands.

See `crates/cobrust-cli/tests/file_io_e2e.rs` for the end-to-end corpus
and `crates/cobrust-types/tests/well_typed.rs` w176..w195 for the
type-checker surface.

## Step 3: try the AI alpha surfaces (optional)

1. Copy the router example and add your provider credentials:

```bash
cp cobrust.toml.example cobrust.toml
```

2. Configure the routes you need in `cobrust.toml`:
   - `[routing.structured]` for `llm_complete_structured(prompt, schema_json)`
   - `[routing.tools]` for `llm_complete_with_tools(prompt, registry_json)`
   - any custom `[routing.<task>]` for `llm_dispatch(task, prompt)`

3. Call the current AI surfaces as flat prelude functions:
   - `llm_complete(provider, model, prompt)`
   - `llm_dispatch(task, prompt)`
   - `llm_stream(provider, model, prompt)`
   - `llm_complete_structured(prompt, schema_json)`
   - `llm_complete_with_tools(prompt, registry_json)`

Current alpha note:
- These are not `cobrust.llm.*`, `cobrust.prompt.*`, or `cobrust.tool.*` module calls yet.
- If routing or provider configuration is missing, the current alpha returns `""` (or `[]` for `llm_stream`) instead of a detailed runtime error.

See [cobrust.toml.example](../../../cobrust.toml.example) for the config shape and [Architecture](architecture.md) for the full AI stdlib design notes.

## Step 3.5: loops and control flow

### `while` loops

Cobrust ships `while` loops out of the box. For `for` loops over `range(start, stop)` or a `list[i64]`, see [§"for loop (M-F.3.1)"](#step-25-for-loop-m-f31) above.

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 5:
        print(i)
        i = i + 1
    return 0
```

Output:

```
0
1
2
3
4
```

### `break` and `continue`

- `break` exits the **innermost** enclosing loop immediately, skipping any remaining body **and** the next condition check.
- `continue` skips the rest of the current iteration's body and jumps back to the condition for the next iteration.
- Both keywords stand alone on their own line (Cobrust does not have Python's `break <label>` — per constitution §2.2 minimalism, bare keywords only).
- They are accepted **only** inside a loop. Using them in a function body without an enclosing loop is a type error.

Example — break out of a search loop the moment a hit is found:

```cobrust
fn first_multiple(n: i64, of: i64) -> i64:
    let i: i64 = 1
    while i <= n:
        if i % of == 0:
            return i        # could also break + return, equivalent here
        i = i + 1
    return -1
```

Example — skip elements with `continue`:

```cobrust
fn sum_skip_seven(limit: i64) -> i64:
    let i: i64 = 0
    let s: i64 = 0
    while i < limit:
        i = i + 1
        if i == 7:
            continue        # skip 7 and resume next iteration
        s = s + i
    return s
```

Example — nested loops; break always binds innermost:

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 3:
        let j: i64 = 0
        while j < 3:
            if j == 1:
                break       # exits inner only; outer i loop continues
            j = j + 1
        i = i + 1
    return 0
```

See [`examples/early_exit.cb`](../../../examples/early_exit.cb) for a combined `break` + `continue` demonstration with an expected output you can verify with `cobrust build` + `./early_exit`.

Why this design?

- One way to do each thing: bare `break` / `continue` covers the early-exit and skip patterns. Labelled break is a sharp tool; if you need it, structure the inner loop as a helper function and `return` instead.
- Hard error on out-of-loop usage: prevents a class of typo-driven runtime bugs that Python pushes to runtime.

## Step 4: translate a Python library (optional)

```bash
cobrust translate tomli
```

See [ADR-0007 translator pipeline](../../agent/adr/0007-translator-pipeline.md) for the full translation workflow and verification gates.

## Step 4.5: method-call form (ADR-0052d-prereq, Phase G P0)

Cobrust supports a Python/Rust-style method-call form on built-in
types as syntactic sugar over the PRELUDE-fn form. See
[ADR-0052d-prereq](../../agent/adr/0052d-prereq-method-dispatch-infra.md):

```cobrust
# Method form (preferred — matches LLM training-data distribution
# per CLAUDE.md §2.5 §B "training-data-overlap rule").
let n: i64 = s.len()
let xs: list[str] = s.split(",")
let y: f64 = x.floor()
let m: i64 = n.abs()

# PRELUDE-fn form (canonical equivalent — method form rewrites to this
# at type-check time, zero runtime overhead).
let n: i64 = str_len(s)
let xs: list[str] = split(s, ",")
let y: f64 = floor(x)
let m: i64 = abs(n)
```

The method form resolves statically at type-check time — no vtable,
no dynamic dispatch, no boxing. Typos surface as
`TypeError::UnknownMethod` at compile time with a "did you mean…"
hint. Method-table coverage today (25 methods): `str` (10),
`list[T]` (5), `f64` (5), `i64` (5). Dict methods (`d.keys()`,
`d.values()`, `d.items()`, `d.get(k)`, `d.copy()`) ship under
[ADR-0050d sub-sprint b/d](../../agent/adr/0050d-dict-types.md).

## Development workflows (contributor path)

```bash
# Clone and build from source
git clone https://github.com/Cobrust-lang/cobrust && cd cobrust
cargo build --workspace

# Run all tests
cargo test --workspace

# Run lints
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Run doc-coverage
bash scripts/doc-coverage.sh
```

## Further reading

- [Overview](overview.md)
- [Design philosophy](design-philosophy.md)
- [Architecture](architecture.md)
- [Milestones](milestones.md)
- Project constitution [`CLAUDE.md`](../../../CLAUDE.md)
