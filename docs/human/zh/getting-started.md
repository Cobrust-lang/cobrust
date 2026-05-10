# 入门 — 30 秒安装

## 第一步：安装 Cobrust

**方式 A — cargo install**（需要 Rust 工具链）：

```bash
cargo install cobrust-cli
```

**方式 B — 预编译二进制**（无需 Rust）：

```bash
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-0.1.0-beta-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/

# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-0.1.0-beta-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

验证：`cobrust --version` → `cobrust 0.1.0-beta`

## 第二步：Hello, world

```bash
cobrust new hello && cd hello && cobrust run src/main.cb
```

预期输出：

```
hello, world
```

## 第三步：翻译 Python 库（可选）

```bash
cobrust translate tomli
```

完整的翻译工作流和验证门控见 [ADR-0007 translator pipeline](../../agent/adr/0007-translator-pipeline.md)。

## 开发工作流（贡献者路径）

```bash
# 克隆并从源码构建
git clone https://github.com/Cobrust-lang/cobrust && cd cobrust
cargo build --workspace

# 运行所有测试
cargo test --workspace

# 运行代码检查
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 运行文档覆盖检查
bash scripts/doc-coverage.sh
```

## 进一步阅读

- [项目概览](overview.md)
- [设计哲学](design-philosophy.md)
- [架构](architecture.md)
- [里程碑](milestones.md)
- 项目宪章 [`CLAUDE.md`](../../../CLAUDE.md)
