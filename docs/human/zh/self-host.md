# 自托管类型检查器（Phase H）

> **状态：proposed（实现待完成）** — Phase H 设计已完成，实现尚未开始。

## 目标

将 `crates/cobrust-types`（类型检查器核心）从 Rust 翻译为 Cobrust，
实现编译器局部自托管。这是 CLAUDE.md §4.4 自托管路线图的第一个可交付阶段。

## 范围

| 文件 | LOC | 阶段 | ADR |
|---|---|---|---|
| `ty.rs`（类型宇宙） | ~220 | Wave 2 | ADR-0055a |
| `error.rs` + `lib.rs`（错误枚举 + 入口） | ~346 | Wave 2 | ADR-0055b |
| `infer.rs`（推断 + 统一） | ~300 | Wave 2 | ADR-0055c |
| `check.rs`（双向检查器） | ~2402 | Wave 3 | ADR-0055d |
| 奇偶校验测试框架 | — | Wave 1 | ADR-0055e |
| **合计** | **~3368** | | ADR-0055 |

## 阶段概览

```
Wave 1: 0055e — 奇偶校验测试框架（先行）
Wave 2: 0055a + 0055b + 0055c — 并行（Tier-1，类型/错误/推断）
Wave 3: 0055d — 双向检查器（Tier-2，最大子冲刺）
```

## 挂钟时间估算

- Wave 1：~1 周（奇偶测试框架）
- Wave 2：~1 周（3 个 ADR 并行）
- Wave 3：~1-2 周（`check.rs` ~2402 LOC，项目迄今最大单子冲刺）
- **总计：约 2.5 周**

## 当前状态

Phase H 所有 ADR（0055 + 0055a–0055e）均处于 **proposed** 状态。
实现将在 Wave 1（0055e）获批后于 DG 工作站（2×RTX 3090）上分发。

## 相关 ADR

- [ADR-0055](../../agent/adr/0055-phase-h-self-host-type-checker.md) — 框架 ADR
- [ADR-0055a](../../agent/adr/0055a-ty-rs-cb-port.md) — `ty.rs` cb 移植
- [ADR-0055b](../../agent/adr/0055b-error-rs-lib-rs-cb-port.md) — `error.rs` + `lib.rs` cb 移植
- [ADR-0055c](../../agent/adr/0055c-infer-rs-cb-port.md) — `infer.rs` cb 移植
- [ADR-0055d](../../agent/adr/0055d-check-rs-cb-port.md) — `check.rs` cb 移植
- [ADR-0055e](../../agent/adr/0055e-phase-h-parity-harness.md) — 奇偶校验测试框架
