# Cobrust

> Cobra 🐍 + Rust 🦀

A Rust-implemented Python successor with an AI-native compiler that
closed-loop translates the entire Python ecosystem.

## Status

**M0 → M12 delivered. M12.x / M13 / M14 in flight.** As of
2026-05-09 (HEAD `cc15f0b`): 100 commits, 26 ADRs accepted,
2,088 tests passing on cold integrated rebuild. The compiler skeleton
is end-to-end (`.cb` → AST → HIR → typed-HIR → MIR → Cranelift →
Mach-O on macOS arm64); the AI translation subsystem ships a synthetic-
LLM mode pipeline with five translated libraries (`cobrust-tomli`,
`cobrust-dateutil`, `cobrust-msgpack`, `cobrust-requests`,
`cobrust-click`) plus the `cobrust-numpy` numerical tier (M7.0..M7.6).
A 16-module stdlib (`std.{io,collections,string,math,panic,env,fmt}`)
and a content-addressed package format (`cobrust.toml` +
`cobrust.lock`) ship at M11 + M12.

**Honest caveats** (per
[`docs/agent/findings/`](docs/agent/findings/README.md)):

- The four working `.cb` programs (`hello / fizzbuzz / fib / notebook`)
  use literal-string `print(...)` calls; real Cobrust expression
  (loops, recursion, arithmetic Rvalues, f-strings) lands at the
  in-flight M12.x sprint per
  [`finding:examples-literal-print-debt`](docs/agent/findings/examples-literal-print-debt.md).
- The translation subsystem's L0 → L1 → L2 → L3 closed loop has
  been validated synthetically only; a real-LLM end-to-end run on
  `tomli` is queued per
  [`finding:translator-real-vs-synthetic-status`](docs/agent/findings/translator-real-vs-synthetic-status.md).
- The LLM Router's wire protocol (provider, cache, ledger,
  failure isolation) **is** real-LLM validated against an
  OpenAI-compatible endpoint per
  [`finding:m5-m7-real-llm-validation`](docs/agent/findings/m5-m7-real-llm-validation.md);
  the open question is whether the L1 translation prompt + L2
  verification gates converge under real diagnostic feedback
  (vs. canned-response tables).

See
[milestones (en)](docs/human/en/milestones.md) /
[里程碑 (zh)](docs/human/zh/milestones.md) for the full roadmap.

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
