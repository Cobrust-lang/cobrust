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

## 第 2.6 步：f64 与 `as` 类型转换（M-F.3.3）

Cobrust 支持一等公民 `f64`（IEEE-754 双精度浮点数）。
`i64` 与 `f64` 之间必须使用显式 `as` 转换——不允许隐式类型提升（宪法 §2.2）。

```cobrust
fn main() -> i64:
    # 浮点字面量
    let x: f64 = 3.14
    let y: f64 = 1e-3
    let big: f64 = inf      # IEEE 754 正无穷
    let nothing: f64 = nan  # IEEE 754 NaN

    # 显式 as 转换：i64 → f64 和 f64 → i64
    let n: i64 = 42
    let f: f64 = (n as f64)        # 42.0
    let back: i64 = (3.9 as i64)   # 3（向零截断）

    # 数学内建函数（全部返回 f64）
    let s: f64 = sqrt(4.0)         # 2.0
    let p: f64 = pow(2.0, 10.0)    # 1024.0
    let fl: f64 = floor(3.7)        # 3.0

    # f-string 浮点格式化
    print(f"{x:.2f}")               # "3.14"

    return 0
```

核心规则：
- `i64 → f64` 必须写 `(n as f64)`，不支持隐式提升。
- `f64 → i64` 向零截断（C 语义，不是 floor）。
- 浮点除以零不是陷阱——IEEE 754 定义为 ±inf。
- `nan != nan` 为 `true`（IEEE 754 语义）。
- 数学函数：`sqrt`、`floor`、`ceil`、`round`、`abs`、`pow`、`sin`、`cos`、`tan`、`log`、`exp`。
- f-string 格式说明符：`{x:.2f}`（定点）、`{x:e}`（科学计数）、`{x:g}`（通用）。

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

## 第三步半：循环与控制流

### `while` 循环

Cobrust 已经支持 `while` 循环;`for` 循环正在 M-F.3.1 sprint 中（参见 [ADR-0050](../../agent/adr/0050-phase-f3-language-completeness-batch.md)）。

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 5:
        print_int(i)
        i = i + 1
    return 0
```

输出:

```
0
1
2
3
4
```

### `break` 与 `continue`

- `break` 立即退出**最内层**的循环,跳过剩余循环体**以及**下一次条件判断。
- `continue` 跳过当前迭代剩余的循环体,直接回到循环头继续判断条件。
- 两个关键字必须单独成行(Cobrust 不支持 Python 的 `break <label>` —— 按宪章 §2.2 的极简主义,只保留裸关键字)。
- 仅在循环内部合法。若在函数体里没有外层循环,类型检查就会报错。

示例 —— 找到目标就立即跳出搜索循环:

```cobrust
fn first_multiple(n: i64, of: i64) -> i64:
    let i: i64 = 1
    while i <= n:
        if i % of == 0:
            return i        # 这里也可以 break + return,等价
        i = i + 1
    return -1
```

示例 —— 用 `continue` 跳过某些元素:

```cobrust
fn sum_skip_seven(limit: i64) -> i64:
    let i: i64 = 0
    let s: i64 = 0
    while i < limit:
        i = i + 1
        if i == 7:
            continue        # 跳过 7,直接进入下一次迭代
        s = s + i
    return s
```

示例 —— 嵌套循环;`break` 永远绑定最内层:

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 3:
        let j: i64 = 0
        while j < 3:
            if j == 1:
                break       # 仅退出内层;外层 i 循环继续
            j = j + 1
        i = i + 1
    return 0
```

完整的 `break` + `continue` 演示见 [`examples/early_exit.cb`](../../../examples/early_exit.cb),可用 `cobrust build` + `./early_exit` 验证预期输出。

设计动机:

- 一件事一种做法:裸 `break` / `continue` 已经覆盖了 "提早退出" 和 "跳过当前迭代" 两种最常见模式。带 label 的 break 是一把利刃;若真的需要,把内层循环抽成一个函数用 `return` 退出即可。
- 在循环外使用直接编译报错:防止 Python 把此类拼写错误推迟到运行时才暴露。

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
