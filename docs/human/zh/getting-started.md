# 入门 — 30 秒安装

## 第一步：安装 Cobrust

**方式 A — cargo install**（需要 Rust 工具链）：

```bash
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
# （crates.io 发布计划在 v0.2.0）
```

**方式 B — 预编译二进制**（无需 Rust）：

```bash
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/

# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

验证：`cobrust --version` → `cobrust 0.1.2`

## 第二步：Hello, world

```bash
cobrust new hello && cd hello && cobrust run src/main.cb
```

预期输出：

```
hello, world
```

## 第 2.5 步：for 循环（M-F.3.1）

Cobrust 提供 Python 风格的 `for ... in ...` 循环，可以遍历 `list[T]`，并通过
prelude 的 `range(start, stop)` 辅助函数生成整数序列。依据 ADR-0050b，
`range(start, stop)` 会物化一个包含 `start, start+1, ..., stop-1` 的
`list[i64]`；空区间（`start >= stop`）会跳过循环体。

```cobrust
fn main() -> i64:
    # 正向区间：依次打印 0 1 2 3 4
    for i in range(0, 5):
        print_int(i)

    # 空区间：循环体不会执行
    for i in range(0, 0):
        print_int(-1)

    # 遍历 list
    let xs: list[i64] = list_new(3)
    let _0 = list_set(xs, 0, 10)
    let _1 = list_set(xs, 1, 20)
    let _2 = list_set(xs, 2, 30)
    for v in xs:
        print_int(v)        # 10  20  30

    # 遍历 argv（list[str]）
    for arg in argv():
        print(arg)

    return 0
```

Phase F.3 提供两参数形式 `range(start, stop)`。三参数 `range(start, stop, step)`
延后至 Phase G，与完整迭代器协议一起落地。字符串遍历
（`for c in "hello":`）同样属于 Phase G 工作 —— 详见 ADR-0050b §"Iter source type checking"。

循环语义：
- 循环变量每次迭代都是全新绑定；在循环体内创建的闭包，在第 N 次迭代时
  捕获的是第 N 次迭代的值（宪法 §2.2 —— 拒绝 Python 的延迟绑定）。
- 允许 `for` 嵌套；变量遮蔽遵循 Rust 规则。
- 非 `list[T]` 的迭代源（例如 `for x in 42:`）在类型检查阶段就会被拒绝
  （`TypeError::NotIterable`）。

可运行示例见 [examples/for_range.cb](../../../examples/for_range.cb)
与 [examples/for_list.cb](../../../examples/for_list.cb)。

## 第三步：试用 AI alpha 能力（可选）

1. 复制 router 示例配置，并填入你的 provider 凭据：

```bash
cp cobrust.toml.example cobrust.toml
```

2. 在 `cobrust.toml` 中声明你需要的路由：
   - `[routing.structured]`：用于 `llm_complete_structured(prompt, schema_json)`
   - `[routing.tools]`：用于 `llm_complete_with_tools(prompt, registry_json)`
   - 任意自定义 `[routing.<task>]`：用于 `llm_dispatch(task, prompt)`

3. 当前 AI 能力以平铺的 prelude 函数形式调用：
   - `llm_complete(provider, model, prompt)`
   - `llm_dispatch(task, prompt)`
   - `llm_stream(provider, model, prompt)`
   - `llm_complete_structured(prompt, schema_json)`
   - `llm_complete_with_tools(prompt, registry_json)`

当前 alpha 说明：
- 这些还不是 `cobrust.llm.*`、`cobrust.prompt.*`、`cobrust.tool.*` 这种模块路径调用。
- 如果缺少路由或 provider 配置，当前 alpha 会返回 `""`（`llm_stream` 则返回 `[]`），而不是更详细的运行时错误。

配置形状见 [cobrust.toml.example](../../../cobrust.toml.example)，完整设计说明见[架构](architecture.md)。

## 第四步：翻译 Python 库（可选）

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
