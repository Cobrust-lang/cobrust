# Cobrust 错误参考手册

每个 Cobrust 编译器错误都属于以下四个类别之一。
类别显示在每条错误信息开头的方括号中，例如：

```
error[Type]: type mismatch: expected `i64`, found `str`
  --> src/main.cb:3:18
  hint: add a type annotation or fix the expression type
```

---

## 语法错误（Syntax）

**看到 `error[Syntax]` 时**，问题出在源代码的书写方式上 —
词法分析器或解析器无法理解代码结构。

### 示例 1 — 使用 Python 的 `def` 关键字

```python
# 错误：`def` 不是 Cobrust 语法
def greet(name: str) -> str:
    return "hello " + name
```

```
error[Syntax]: expected end of statement, found identifier
  --> src/greet.cb:1:5
```

**修复方法：** 使用 `fn` 代替 `def`。

```cobrust
fn greet(name: str) -> str:
    return "hello " + name
```

### 示例 2 — 未关闭的字符串

```cobrust
fn main() -> i64:
    print("hello, world)   # 缺少结尾的 "
    return 0
```

```
error[Syntax]: unterminated string literal
  --> src/main.cb:2:11
  hint: add a closing `"`
```

**修复方法：** 用 `"` 关闭字符串。

### 示例 3 — 链式赋值（不支持）

```cobrust
fn main() -> i64:
    let x: i64 = 0
    let y: i64 = 0
    x = y = 1   # Python 风格的链式赋值不支持
    return x
```

```
error[Syntax]: expected end of statement, found `=`
  --> src/main.cb:4:11
```

**修复方法：** 拆分为两条赋值语句。

```cobrust
    y = 1
    x = y
```

---

## 类型错误（Type）

**看到 `error[Type]` 时**，程序中的类型不一致 —
类型检查器或 HIR 降级过程发现了类型不匹配或未解析的名称。

### 示例 1 — 类型不匹配

```cobrust
fn main() -> i64:
    let x: i64 = "hello"   # 字符串不能赋值给 i64
    return x
```

```
error[Type]: type mismatch: expected `i64`, found `str`
  --> src/main.cb:2:18
  hint: add a type annotation or fix the expression type
```

**修复方法：** 使用正确的字面量类型。

```cobrust
    let x: i64 = 42
```

### 示例 2 — 未知名称

```cobrust
fn main() -> i64:
    print(undefined_name)   # 名称未声明
    return 0
```

```
error[Type]: unknown name `undefined_name`
  --> src/main.cb:2:11
  hint: did you mean to declare it with `let undefined_name = …`?
```

**修复方法：** 在使用前声明变量。

```cobrust
    let undefined_name: str = "hello"
    print(undefined_name)
```

### 示例 3 — 隐式真值（不允许）

```cobrust
fn main() -> i64:
    if 1:            # i64 不能作为布尔条件
        print("yes")
    return 0
```

```
error[Type]: cannot use `i64` as a boolean condition
  --> src/main.cb:2:8
  hint: Cobrust requires an explicit bool — try `if x != 0:` or `if x.is_some():`
```

**修复方法：** 写明确的比较表达式。

```cobrust
    if 1 != 0:
        print("yes")
```

### 示例 4 — 隐式类型转换（不允许）

```cobrust
fn main() -> i64:
    let x: i64 = 1 + "two"   # i64 和 str 不能相加
    return x
```

```
error[Type]: type mismatch: expected `i64`, found `str`
  --> src/main.cb:2:22
  hint: add a type annotation or fix the expression type
```

**修复方法：** 使用一致的类型。

```cobrust
    let x: i64 = 1 + 2
```

### 示例 5 — 所有权 / 借用错误

```
error[Type]: use of moved value `_x` after it was moved
  --> src/main.cb:5:10
  hint: each value can only be used once after being moved
```

**可能的修复：** 在移动之前克隆该值，或重构代码以避免在移动后使用。

### 示例 6 — 类实例上的未知字段

`class` 的字段由类型检查器跟踪（ADR-0080）。访问类未声明的字段是**编译期**
错误，绝不会变成运行时 `KeyError`。错误消息会列出已声明的字段，便于你选择
正确的字段（或修正拼写）。

```
error[Type]: no field `nonexistent` on `Score`; declared fields: name, rank
  --> src/main.cb:6:14
```

```cobrust
class Score:
    let name: str = ""
    let rank: i64 = 0

fn f() -> i64:
    let s = Score()
    return s.nonexistent   # 错误 —— 见上面的消息

# 修复：使用已声明字段。其类型在编译期已知：
#   s.rank 是 i64，s.name 是 str。
fn f() -> i64:
    let s = Score()
    return s.rank
```

**可能的修复：** 访问消息中列出的某个已声明字段。字段类型在编译期已知,
因此类型错误的用法(如 `s.name + s.rank`,即 str + i64)同样会被捕获为类型不匹配。

---

### 示例 7 — 不受支持的 refinement `where` 谓词

带校验的请求体(`route_validated`,ADR-0080)可以为每个字段带一个
`where` 子句。只接受固定的 refinement 形式;任何其他谓词都是编译期错误,
并会打印出可接受的形式,方便你下一次改对。

四种可接受形式:

- **i64 整数范围** —— `0 <= self and self <= 100`(闭区间)
- **f64 浮点范围** —— `0.0 <= self and self <= 1.0`(**仅**闭区间 `<=`/`>=`
  —— 严格 `<`/`>` 被拒绝,因为实数稠密,没有干净的 `±1` 改写)
- **str 长度** —— `len(self) <= n`(或 `len(self) >= n`)
- **str 正则** —— `pattern(self, "<regex>")`

```
error[Type]: unsupported refinement `where`-predicate on field `rank`: use one
of the fixed refinement forms — an i64 int-range `0 <= self and self <= 100`
(inclusive); an f64 float-range `0.0 <= self and self <= 1.0` (inclusive
`<=`/`>=` ONLY — a strict `<`/`>` is rejected, the reals are dense); a str
length `len(self) <= n` (or `len(self) >= n`); or a str pattern
`pattern(self, "<regex>")`
  --> src/main.cb:3:20
```

```cobrust
class CreateScore:
    name: str
    rank: i64 where weird(self)   # 错误 —— 不是固定 refinement 形式

# 修复:在 i64 字段上使用固定的闭区间整数范围。
class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100
```

**可能的修复:** 把 `where` 子句改写成上面四种固定形式中与字段类型匹配的那种
(`i64` 上的整数范围、`f64` 上的闭区间浮点范围、`str` 上的 `len(self)` 长度界
或 `pattern(self, …)` 正则)。

---

## 运行时错误（Runtime）

**看到 `error[Runtime]` 时**，程序本身 panic 了，或者
`cobrust run` 驱动程序在执行编译后的二进制文件时遇到了问题。

```
error[Runtime]: process exited with status 1
  --> cobrust run
```

这通常是断言失败或程序中未处理的 `Result::Err`。
可以添加 `print` 调用，或使用 REPL（`:mir EXPR`）来检查状态。

---

## 内部错误（Internal）

**看到 `error[Internal]` 时**，*编译器*本身遇到了 bug —
不是你的代码问题。你无法通过修改源码来修复这个问题。

```
error[Internal]: CraneliftError: inst441 has type i64, expected i8

  This is a compiler bug.  Please collect a bug report and file a GitHub issue:

    cobrust report-bug --include-mir

  Repro command: cobrust build src/main.cb
```

**处理步骤：**

1. 运行 `cobrust report-bug --include-mir --source-file src/main.cb`。
2. 打开输出的 GitHub URL，附上生成的 `.txt` 报告文件。
3. 在 bug 修复之前，可以尝试简化程序作为临时方案。

**注意：** Conway 玩具程序 bug（早期 0.1.0-beta 会话中的 3000 行 Cranelift IR 转储）
已在 ADR-0033 中修复。如果你之前遇到过该错误，请更新到最新构建 ——
现在应该会显示简洁的 `error[Internal]` 和 `cobrust report-bug` 提示，
而不是原始 IR 转储。

---

## 快速查询表

| 症状 | 类别 | 退出码 | 处理方法 |
|---|---|---|---|
| `def f():` 无法识别 | `Syntax` | 2 | 改用 `fn f():` |
| 字符串未关闭 | `Syntax` | 2 | 添加结尾 `"` |
| `let x: i64 = "hi"` | `Type` | 2 | 匹配类型 |
| `if x:` 其中 x 是 i64 | `Type` | 2 | 改写为 `if x != 0:` |
| 表达式中有 `undefined_name` | `Type` | 2 | 用 `let` 声明 |
| 类实例上访问 `s.typo` | `Type` | 2 | 使用已声明字段(错误消息会列出) |
| 程序在运行时 panic | `Runtime` | 4 | 调试程序逻辑 |
| Cranelift / 链接器错误 | `Internal` | 3 | 运行 `cobrust report-bug` |

---

## 翻译 crate 错误（不可信输入加固）

经翻译的生态系统 crate 对不可信输入强制执行安全限制。这些错误以 `Result::Err` 形式返回——
不会发生 panic 或进程终止。

### `cobrust-nest` — `TomliError`

| 条件 | 错误消息 | 原因 |
|---|---|---|
| TOML 嵌套深度 > 100 | `"nesting depth exceeds maximum (100); possible adversarial input"` | 对抗性的深度嵌套数组 / 内联表格若无限制会溢出调用栈（B4 修复）。 |
| 无效语法 | `"unexpected character '…' at pos N"` | 标准解析错误。 |
| 未关闭的字符串 | `"unterminated string"` | 标准解析错误。 |

**常量**：`nest::MAX_DEPTH = 100`（已导出，调用方可引用）。

### `cobrust-strike` — `HttpError` / `HttpErrorKind`

| `HttpErrorKind` | 含义 | 处理方法 |
|---|---|---|
| `InvalidUrl` | URL 解析失败或 scheme 不受支持 | 检查 URL 字符串。 |
| `Network` | DNS、TCP 或 TLS 层错误 | 检查网络连接 / 证书。 |
| `Timeout` | 传输超时（默认：30 秒） | 重试或增大超时时间。 |
| `DecodeBody` | 响应体不是有效 UTF-8 或 JSON | 检查服务器的 `Content-Type`。 |
| `BodyTooLarge` | 响应体超出 64 MiB 上限 | 服务器发送数据过多；使用流式处理或提高上限（B5 修复）。 |

**常量**：`strike::MAX_BODY_BYTES = 64 * 1024 * 1024`（64 MiB）。

### `cobrust-scale` — `MsgError` / `MsgErrorKind`

| `MsgErrorKind` | 含义 | 处理方法 |
|---|---|---|
| `Pack` | 值无法被编码（超出 M6 范围）。 | 检查 `MsgValue` 变体。 |
| `Unpack` | msgpack 字节格式错误或被截断。 | 检查输入字节。 |
| `OverflowSize` | `pos + length` 造成 `usize` 溢出——可能是带有接近 `u32::MAX` 长度字段的对抗性输入。 | 拒绝该输入；这不是有效的 msgpack 数据(B6 修复)。 |

---

## 错误消息打印修复方法 (ADR-0052b)

从 Phase G Wave 2 (ADR-0052b) 开始，每个编译器诊断都携带一个机器可
解析的 `suggestion` 字段，指明具体的**修复路径**，而不仅仅是问题描
述。CLI 渲染器将其呈现为 `hint:` 行；未来的 LSP / `--emit-json`
消费者直接读取 `&'static str` 字段。

### 为什么这很重要

CLAUDE.md §2.5 将 Cobrust 定位为 "LLM 智能体一次就能写对的语言"。
LLM 智能体从 stderr 中提取修复方法时应当是确定性的，无需 prose
剥离：

```
error[Type]: cannot use `Int` as a boolean condition
  --> src/main.cb:3:8
  hint: change to `if x != 0:` (use `.is_some()` for Option)
```

`hint:` 文本现在是一个 `&'static str` 字面量，在错误构造点处填充，
对同一变体的所有触发场景都一致。修复路径是可复现、结构化、
LLM 友好的。

### 三大特性

- **构造时写入**。编译器中每个 `Err(TypeError::Foo { ... })` 站点
  都在该调用位置填充 `suggestion`，包含最具操作性的修复文本。
- **静态 `&'static str`**。Suggestion 文本是编译时字面量——不含动
  态格式参数。主错误行仍然携带失败标识符（`unknown name \`foo\``），
  以便 LLM stderr 解析保留它；suggestion 文本是通用且可操作的。
- **渲染器是结构化的**。CLI 的 `error_ux.rs` 中的 `From<...>` 实
  现直接读取 `suggestion.map(str::to_owned)`——不再在渲染时硬编
  码 per-variant 文本。

### 覆盖的错误类型

- `cobrust_types::TypeError` — 24 个变体（每个 S 类变体都携带
  `Some(...)`；如 `Multiple` 这样的 N 类变体携带 `None`）。
- `cobrust_mir::MirError` — 10 个变体（use-after-move、borrow 冲
  突、drop-schedule 违反）。
- `cobrust_hir::LoweringError` — 6 个变体（未知名称、被弃用功
  能、可变默认参数、重复绑定）。

未来对 `cobrust_frontend::{LexError, ParseError}` 的 Direction-B
扩展已经被跟踪，但不在 Wave-2 范围内。

## 修复安全梯度（ADR-0062）

每条建议都额外携带一个 `fix_safety` 等级，LSP 代码动作层和 JSON
诊断输出据此判断该建议是否可以自动应用。从风险最低（总是可以自动
应用）到风险最高（永不自动应用）共六个等级：

| 等级 | 序列化形式 | 自动应用行为 |
|---|---|---|
| FormatOnly | `format-only` | 保存 / 格式化时自动应用 |
| BehaviorPreserving | `behavior-preserving` | 用户接受后应用 |
| LocalEdit | `local-edit` | 用户接受后应用（可能需要修改相邻测试） |
| ApiChanging | `api-changing` | 仅建议——无一键应用 |
| TargetChanging | `target-changing` | 仅诊断——绝不自动应用 |
| RequiresHumanReview | `requires-human-review` | 仅诊断——需人工审查 |

### 各等级触发场景

- **BehaviorPreserving**：`if x:` → `if x != 0:`；可变默认值 → `None`-默认值重写；f64 字典键 → `.to_bits() as i64`。编译器强制的语义等价重写。
- **LocalEdit**：拼写修复（`UnknownName`）、arity / 关键字不匹配、类型注解添加（`AmbiguousType`）、`break`/`continue`/`return` 位置。调用点或单绑定范围内的修复。
- **RequiresHumanReview**：`OccursCheck`（递归类型）、`UseOfDroppedFeature`（改用不同构造）、`DictSpreadNotSupported`（等待 Phase G）、`MirError::EscapingBorrow` / `DoubleDrop`（生命周期重构）。

### LSP 代码动作分级

当连接到 Cobrust LSP 服务器（`cobrust-lsp` 二进制）时，编辑器的
「快速修复」菜单只为 `FormatOnly` / `BehaviorPreserving` /
`LocalEdit` 三个等级显示代码动作。`ApiChanging` 建议会作为
`Refactor` 出现（仅建议）。`TargetChanging` /
`RequiresHumanReview` 建议会出现在诊断消息中，但不生成代码动作——
代理必须自行推理修复方案。
