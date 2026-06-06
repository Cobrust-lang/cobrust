# `str` 索引与切片 —— 按码点(codepoint)寻址

> ADR-0094(修复 F78)。`str` 的索引运算符 `s[i]` 和切片运算符
> `s[lo:hi]` 现在能正确工作了,语义与 Python 完全一致 —— **按 Unicode
> 码点(codepoint)寻址,而不是按字节**。

## 先看示例

```cobrust
fn main() -> i64:
    let s: str = "hello"
    print(s[1:4])         # ell    (不再是整个 "hello")
    print(s[1])           # e      (单个码点,是一个 str)
    print(len(s[1:4]))    # 3
    return 0
```

与 Python 一致:`"hello"[1:4] == "ell"`、`"hello"[1] == "e"`。

## 为什么是“码点”,不是“字节”

这是 `str` 与 `bytes` 最关键的区别。Python 的 `str` 按 **Unicode 码点**
索引,一次切片**绝不会**把一个多字节的 UTF-8 码点从中间切开:

```cobrust
fn main() -> i64:
    let u: str = "héllo"     # 'é' 占 2 个 UTF-8 字节
    print(u[1:3])            # él    (码点 [1,3),不是字节)
    print(u[1])              # é     (单个码点)
    print(u[0:2])            # hé

    let z: str = "你好世界"   # 每个汉字 3 字节
    print(z[1:3])            # 好世
    return 0
```

如果按字节切片,`"héllo"[1:3]` 会切坏 `é`,产生非法 UTF-8 —— Cobrust
的 `str` 始终是合法 UTF-8(§2.2 禁止任何静默的数据损坏)。按码点切片
时,边界永远落在字符边界上,因此结果**永远是合法 UTF-8**,无需任何
“吸附到边界”或“中途报错”的处理。

> 对比:`bytes` 是**按字节**索引的(`b[i] -> int`,见
> [bytes 文档](bytes-primitive.md)),因为每个字节都是独立的;而 `str`
> 是按码点索引的(`s[i] -> str`,一个长度为 1 的字符串)。

## 现在能做什么

| 形式 | 结果 | 说明 |
|---|---|---|
| `s[i]` | `str` | 第 `i` 个**码点**,是一个长度为 1 的 `str`(与 Python `"héllo"[1] == "é"` 一致,不是字节) |
| `s[lo:hi]` | `str` | 切片(返回全新的 `str`);按码点区间 `[lo, hi)`;越界时像 Python 一样夹取 |

切片采用与 Python 相同的夹取语义 —— 越界的上界收窄到长度,反向区间得到
空字符串(从不报错):

```cobrust
fn main() -> i64:
    let s: str = "hello"
    print(s[1:99])   # ello   (上界夹取到长度)
    print(s[3:1])    # (空行,反向区间 -> "")
    print(s[0:5])    # hello
    return 0
```

每个产生新 `str` 的索引 / 切片操作都给你一个由作用域拥有的**全新**值,
作用域结束时自动释放一次;输入 `s` 只被**借用**(读取),绝不会被消耗 ——
所以你可以对同一个 `s` 连续做多次索引:

```cobrust
fn main() -> i64:
    let s: str = "hello"
    let mid: str = s[1:4]
    print(mid)        # ell
    print(s[0])       # h    (s 仍然可用)
    print(s[1:3])     # el
    return 0
```

## 暂不支持的切片形态(会在编译期报错)

只有“两端都写明、非负、步长为 1”的 `s[lo:hi]` 形态被支持。其余形态会在
`cobrust check` 阶段**被拒绝**(`UnsupportedSliceShape`),而不是像
修复前那样静默算错 —— 这是 §2.5-A “编译期捕获错误” 的体现,报错信息会
直接告诉你正确写法 `s[1:len(s)]`:

| 形态 | 现状 |
|---|---|
| `s[1:]` / `s[:3]` / `s[:]`(开区间) | 拒绝 |
| `s[0:4:2]`(带步长) | 拒绝 |
| `s[1:-1]` / `s[-1]`(负索引) | 拒绝 |

这些形态的支持是 ADR-0094 列出的后续工作。在它们落地之前,请写明两端的
非负边界。

## 设计说明

- **F78 修复**:修复前 `print("hello"[1:4])` 会在退出码 0 下静默打印
  `hello`(整个字符串),`s[i]` 标量索引也有同样的问题。两者现已修复并
  与 CPython 3 逐字节对齐。
- **运行时**:`__cobrust_str_char_at` / `__cobrust_str_slice`,镜像
  `bytes` 第 2 阶段的切片机制,但按码点寻址。
- 这是 §2.5 “LLM 优先” 原则的直接体现:LLM 凭 Python 直觉写下 `s[i]`,
  期望的就是码点语义;按字节语义会在每个非 ASCII 字符串上静默偏离。
