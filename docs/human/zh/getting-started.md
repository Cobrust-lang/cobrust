# 入门

## 前置依赖

- **Rust 1.94.1** — 项目通过 [`rust-toolchain.toml`](../../../rust-toolchain.toml) 锁定
- **Git**

`rustup` 会自动按 `rust-toolchain.toml` 安装匹配版本，你**不需要**手动切换。

## 快速开始（5 步）

### 1. 克隆仓库

```bash
git clone https://github.com/cobrust/cobrust
cd cobrust
```

### 2. 构建

```bash
cargo build --workspace
```

得到 `target/debug/cobrust` —— Cobrust 编译器 CLI。

### 3. Hello world

新建 `hello.cb`：

```cobrust
fn main() -> i64:
    print("hello, world")
    return 0
```

编译并运行：

```bash
./target/debug/cobrust build hello.cb
./hello
```

### 4. 真正的算法：FizzBuzz

新建 `fizzbuzz.cb`：

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

编译并运行：

```bash
./target/debug/cobrust build fizzbuzz.cb
./fizzbuzz
```

这展示了真正的 Cobrust：`while` 循环、`if/elif/else` 分支、取模运算和可变绑定
（M11.1 启用，ADR-0030）。

### 5. 交互式 REPL

```bash
./target/debug/cobrust repl
```

尝试：

```
> let x: i64 = 42
> :type x
> let y: i64 = x + 1
> print_int(y)
> :hir let y
> :quit
```

指令：`:type <var>`、`:ast`、`:hir <stmt>`、`:mir <stmt>`、`:clear`、`:help`。

## 开发工作流

### 跑测试

```bash
cargo test --workspace
```

Phase E 完成阶段（M11.1..M14）有 2,088 个通过的测试。

### 跑 lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI 会以 `-D warnings` 跑 clippy——任何警告都会让 PR 失败。

### 跑文档覆盖检查

```bash
bash scripts/doc-coverage.sh
```

验证所有公共项在 `docs/human/zh/`、`docs/human/en/` 和 `docs/agent/` 三棵文档树中都有对应文档。

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
