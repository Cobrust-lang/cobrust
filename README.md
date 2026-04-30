# Cobrust

> Cobra 🐍 + Rust 🦀

A Rust-implemented Python successor with an AI-native compiler that
closed-loop translates the entire Python ecosystem.

## Status

**M0 — repository skeleton.** Compiler, runtime, and AI translation
subsystem are not yet implemented. See
[milestones (en)](docs/human/en/milestones.md) /
[里程碑 (zh)](docs/human/zh/milestones.md).

## Documentation

- 中文文档: [`docs/human/zh/`](docs/human/zh/README.md)
- English docs: [`docs/human/en/`](docs/human/en/README.md)
- Agent docs (LLM-facing): [`docs/agent/`](docs/agent/README.md)

## Constitution

The project's design constitution lives in [`CLAUDE.md`](CLAUDE.md). It
binds all engineering work and AI-agent contributions. When intuition
disagrees with the constitution, the constitution wins.

## Building from source

```bash
git clone https://github.com/cobrust/cobrust
cd cobrust
cargo build --workspace
```

`rustup` reads [`rust-toolchain.toml`](rust-toolchain.toml) and pulls
the pinned toolchain (1.94.1) automatically.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option. See
[ADR-0001](docs/agent/adr/0001-license.md) for rationale.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual-licensed as above, without any additional terms
or conditions.
