# 入门

## 前置依赖

- **Rust 1.94.1** — 项目通过 [`rust-toolchain.toml`](../../../rust-toolchain.toml) 锁定
- **Git**

`rustup` 会自动按 `rust-toolchain.toml` 安装匹配版本，你**不需要**手动切换。

## 从源码构建

```bash
git clone https://github.com/cobrust/cobrust
cd cobrust
cargo build --workspace
```

完成后会得到 `target/debug/cobrust` 二进制——当前是 M0 占位，无子命令。

## 跑测试

```bash
cargo test --workspace
```

M0 没有测试用例；首批测试在 M1 落地。

## 跑 lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI 会以 `-D warnings` 跑 clippy——任何警告都会让 PR 红。

## 跑文档覆盖检查

```bash
bash scripts/doc-coverage.sh
```

这是 M0 的占位检查——当前只验证三棵文档树的目录结构和 ADR-0001 的存在。M1+ 起会扩展为真正的"public item ↔ 三树文档"双向映射检查。

## 工作流约束

提交代码前请确认：

- [ ] 公共条目同时存在于 `docs/human/zh/`、`docs/human/en/`、`docs/agent/` 三棵树
- [ ] 影响两个及以上文件的决定写了 ADR（`docs/agent/adr/NNNN-*.md`）
- [ ] `cargo fmt`、`cargo clippy`、`cargo test`、`bash scripts/doc-coverage.sh` 全过
- [ ] 单次提交是原子的（代码 + 测试 + 文档 + ADR 同步）
- [ ] commit 信息符合 [conventional commits](https://www.conventionalcommits.org/)，scope 用 crate 名（如 `feat(router): add anthropic adapter`）

## 进一步阅读

- [项目概览](overview.md)
- [设计哲学](design-philosophy.md)
- [架构](architecture.md)
- [里程碑](milestones.md)
- 项目宪章 [`CLAUDE.md`](../../../CLAUDE.md)（仓库根目录）
