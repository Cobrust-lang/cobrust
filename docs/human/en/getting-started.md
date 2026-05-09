# Getting started

## Prerequisites

- **Rust 1.94.1** — pinned via [`rust-toolchain.toml`](../../../rust-toolchain.toml)
- **Git**

`rustup` honors `rust-toolchain.toml` automatically — you do **not** need to switch toolchains by hand.

## Quick start (5 steps)

### 1. Clone

```bash
git clone https://github.com/cobrust/cobrust
cd cobrust
```

### 2. Build

```bash
cargo build --workspace
```

Produces `target/debug/cobrust` — the compiler CLI.

### 3. Hello world

Create `hello.cb`:

```cobrust
fn main() -> i64:
    print("hello, world")
    return 0
```

Compile and run:

```bash
./target/debug/cobrust build hello.cb
./hello
```

### 4. Real algorithm: FizzBuzz

Create `fizzbuzz.cb`:

```cobrust
fn main() -> i64:
    let n: i64 = 1
    while n <= 15:
        if n % 15 == 0:
            print("FizzBuzz")
        elif n % 3 == 0:
            print("Fizz")
        elif n % 5 == 0:
            print("Buzz")
        else:
            print_int(n)
        n = n + 1
    return 0
```

Compile and run:

```bash
./target/debug/cobrust build fizzbuzz.cb
./fizzbuzz
```

This demonstrates real Cobrust: `while` loops, `if/elif/else` branching,
modulo arithmetic, and mutable bindings (M11.1 enablement, ADR-0030).

### 5. Interactive REPL

```bash
./target/debug/cobrust repl
```

Try:

```
> let x: i64 = 42
> :type x
> let y: i64 = x + 1
> print_int(y)
> :hir let y
> :quit
```

Directives: `:type <var>`, `:ast`, `:hir <stmt>`, `:mir <stmt>`, `:clear`, `:help`.

## Development workflows

### Run tests

```bash
cargo test --workspace
```

2,088 tests passing on Phase E complete (M11.1..M14).

### Run lints

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs clippy with `-D warnings` — any warning fails the PR.

### Run doc-coverage

```bash
bash scripts/doc-coverage.sh
```

Verifies all public items have documentation in `docs/human/zh/`, `docs/human/en/`, and `docs/agent/` trees.

## Workflow checklist

Before you push:

- [ ] Public items exist in `docs/human/zh/`, `docs/human/en/`, `docs/agent/` simultaneously
- [ ] Decisions affecting two or more files have an ADR (`docs/agent/adr/NNNN-*.md`)
- [ ] `cargo fmt`, `cargo clippy`, `cargo test`, `bash scripts/doc-coverage.sh` all pass
- [ ] Each commit is atomic (code + tests + docs + ADR shipped together)
- [ ] Commit messages follow [conventional commits](https://www.conventionalcommits.org/) with crate-scoped tags (e.g. `feat(router): add anthropic adapter`)

## Further reading

- [Overview](overview.md)
- [Design philosophy](design-philosophy.md)
- [Architecture](architecture.md)
- [Milestones](milestones.md)
- Project constitution [`CLAUDE.md`](../../../CLAUDE.md) (repo root)
