# `for c in <str>:` —— 按码点(codepoint)迭代

> ADR-0101(关闭 F88)。`for` 循环现在可以直接迭代 `str`,把每个循环变量
> 绑定到 **一个 Unicode 码点**(一个长度为 1 的 `str`)—— 与 Python 完全
> 一致。§2.5 LLM-first:这是最常见的 Python 写法之一,编译器现在直接接受。

## 先看示例

```cobrust
fn main() -> i64:
    for c in "hi":
        print(c)          # h
                          # i
    return 0
```

与 Python 一致:`for c in "hi": print(c)` 先打印 `h` 再打印 `i`。

可以迭代变量,而不只是字面量 —— 并且字符串在循环之后仍然可用(它只是被
**借用**(borrow),从不被消耗):

```cobrust
fn main() -> i64:
    let s: str = "abc"
    for c in s:
        print(c)          # a, b, c
    for c in s:
        print(c)          # a, b, c   (s 仍然可用)
    return 0
```

## 为什么是“码点”,不是“字节”

`str` 按 **Unicode 码点** 迭代 —— 一个多字节字符是 **一次** 迭代,绝不会
被拆成它的 UTF-8 字节:

```cobrust
fn main() -> i64:
    for c in "héllo":     # 'é' 是 2 个 UTF-8 字节,1 个码点
        print(c)          # h, é, l, l, o   (五次迭代,而不是六次)
    return 0
```

所以 `"héllo"` 产生 5 次迭代(h、é、l、l、o),并且每个 `c` 都是一个完整的
长度为 1 的 `str`,可以拼接、求长度、比较:

```cobrust
fn main() -> i64:
    for c in "xy":
        print(c + "!")    # x!  然后  y!
    return 0
```

> **注意 —— `len(str)` 现在同样返回码点数(F91 / ADR-0103)。** 与 Python 一致,
> `len("héllo") == 5`(不是 6 个字节),`len("é") == 1`。因此 `len(s)` 精确等于
> `for c in s:` 的迭代次数,也等于 `s[i]` 的有效下标范围 —— 整个字符串表面统一
> 按码点计数。需要原始 UTF-8 字节长度?先编码:`len(s.encode())`
> (`bytes` 就是字节 —— `len(b"…")` 仍然是字节数)。

## `continue` 与 `break` 都可用

字符串循环复用与列表循环相同的“按长度索引”机制,因此 `continue`(跳过一个
码点)和 `break`(提前结束)的行为与预期完全一致 —— 并且循环总会 **终止**:

```cobrust
fn main() -> i64:
    for c in "hello":
        if c == "l":
            continue          # 跳过两个 'l'
        print(c)              # h, e, o
    return 0
```

## 空字符串

`for c in "":` 执行循环体 0 次(没有迭代),与 Python 一致。

## 内存与所有权

- 每个循环变量 `c` 都是那次迭代新铸造的、**自有的(owned)** 长度为 1 的
  `str`。**不会发生双重释放(double-free)**:源字符串只被读取(从不消耗),
  每个 `c` 拥有自己的副本。
- 一个 1000 码点的字符串可以干净地迭代并正常退出。

> 在一个独立的、既有的循环体 drop 缺口(发现 F82)下,每次迭代分配的 `c`
> 会被 **泄漏(leak)** —— 这是单独跟踪的问题,不影响正确性(不崩溃、不双重
> 释放)。它是已命名的后续工作,不属于 F88。

## 适用范围:目前仅限 `for` 循环

字符串迭代目前接入的是 **`for` 循环**。`str` **暂时还不能** 在列表/集合/字典
推导式(comprehension)里迭代,也不能用在 `in` 运算符右侧 —— 这两种写法仍然在
**编译期被拒绝**(一个干净的类型错误,而不是崩溃):

```cobrust
fn main() -> i64:
    let xs: list[str] = [c for c in "hi"]   # 在 `cobrust check` 阶段被拒绝
    if "e" in "hello":                       # 在 `cobrust check` 阶段被拒绝
        print("x")
    return 0
```

这是有意为之:这些写法在更底层暂时还没有字符串支持,所以提前拒绝(§2.5-A
“在编译期捕获错误”原则)胜过在后续阶段给出令人困惑的失败。今天想遍历字符串
的码点,请使用 `for` 循环。

## 设计说明

- **F88 修复**:在 ADR-0101 之前,`for c in "hi":` 是一个干净的编译期拒绝
  (`str` 不能用于 `for` 循环,退出码 2)—— 从不是静默误编译。ADR-0101 为
  `str` 解除了这个延期。
- **运行时**:循环上界是 `__cobrust_str_char_count`(码点数,**不是**字节
  长度),每个值是 `__cobrust_str_char_at(s, i)`(按码点寻址,与 `s[i]` 使用
  的是同一个原语 —— 见
  [str 索引与切片文档](str-slicing.md))。两者按码点逐一一致,所以一个多字节
  字符正好是一次迭代。
- 这直接服务于 §2.5 的“LLM-first”原则:LLM 会根据它的 Python 先验写出
  `for c in s:`;拒绝它会迫使其改写成不地道的索引循环。
