# Getting started

## Prerequisites

- **Rust 1.94.1** — pinned via [`rust-toolchain.toml`](../../../rust-toolchain.toml)
- **Git**

`rustup` honors `rust-toolchain.toml` automatically — you do **not** need to switch toolchains by hand.

## Build from source

```bash
git clone https://github.com/cobrust/cobrust
cd cobrust
cargo build --workspace
```

Produces `target/debug/cobrust` — currently an M0 stub with no subcommands.

## Run tests

```bash
cargo test --workspace
```

M0 ships no tests; the first test suite lands at M1.

## Run lints

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs clippy with `-D warnings` — any warning fails the PR.

## Run doc-coverage

```bash
bash scripts/doc-coverage.sh
```

This is the M0 placeholder check — it currently verifies the three doc trees exist and ADR-0001 is in place. M1+ extends it to a real "public-item ↔ triple-doc" mapping check.

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
