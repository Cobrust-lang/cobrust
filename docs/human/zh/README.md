# Cobrust 中文文档

> Cobra 🐍 + Rust 🦀 — 用 Rust 实现的 Python 继任者，自带 AI 原生编译器闭环翻译整个 Python 生态。

## 文档地图

- [项目概览](overview.md) — 一句话理解 Cobrust 在做什么
- [设计哲学](design-philosophy.md) — 保留什么、抛弃什么、为什么
- [架构](architecture.md) — 编译器分层 + AI 翻译子系统
- [里程碑](milestones.md) — M0 到 M7+ 的路线图
- [入门](getting-started.md) — 从源码构建到第一个翻译

## 阅读路径

| 你是谁 | 推荐顺序 |
|---|---|
| 第一次接触 Cobrust 的工程师 | 概览 → 设计哲学 → 架构 → 里程碑 → 入门 |
| 想动手编译运行 | 入门 → 概览 → 架构 |
| 想理解某个设计决策 | `docs/agent/adr/` |
| 想接续 LLM Agent 的工作 | `docs/agent/` |

## 文档协议

- **双语并列**：本文档树（`docs/human/zh/`）与英文树（`docs/human/en/`）一一对应
- **三树同步**：任何 public item 必须同时存在于 zh / en / agent 三棵树
- **CI 强制**：doc-coverage 检查未通过则 CI 红
- **风格**：列表优先于散文；非平凡流程必须配 mermaid 图；先示例再抽象

> 本文档树面向"读懂 Cobrust 在做什么"的人类工程师。Agent 文档树（`docs/agent/`）面向接续工作的 LLM Agent，两者风格不同，不要混用。
