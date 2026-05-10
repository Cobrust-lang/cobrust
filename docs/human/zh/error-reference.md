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
| 程序在运行时 panic | `Runtime` | 4 | 调试程序逻辑 |
| Cranelift / 链接器错误 | `Internal` | 3 | 运行 `cobrust report-bug` |
