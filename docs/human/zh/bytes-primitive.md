# `bytes` —— 一等公民的不可变字节缓冲

> ADR-0093。`bytes` 现在是真正的运行期值:你可以写 `b"..."` 字面量、
> 用 `len(b)` 取长度、用 `b[i]` 读取单个字节。

## 先看示例

```cobrust
fn main() -> i64:
    let b: bytes = b"abc"
    print(len(b))   # 3
    print(b[0])     # 97  ('a' 的字节值,是一个 int)
    print(b[1])     # 98
    print(b[2])     # 99
    return 0
```

`bytes` 就是 **“没有 UTF-8 约束的 `str`”**:一段不可变、存放在堆上的原始
字节序列。与 `str`(始终是合法 UTF-8)不同,`bytes` 可以容纳任意字节 ——
包括非文本字节:

```cobrust
fn main() -> i64:
    let raw: bytes = b"\xff\x00\xfe"
    print(len(raw))   # 3
    print(raw[0])     # 255
    print(raw[1])     # 0
    print(raw[2])     # 254
    return 0
```

## 现在能做什么(第 1 阶段)

| 形式 | 结果 | 说明 |
|---|---|---|
| `b"..."` | `bytes` | 字节串字面量(任意字节,支持 `\xNN` 转义) |
| `len(b)` | `int` | 字节数 |
| `b[i]` | `int` | 第 `i` 个字节,范围 `0..255`(与 Python 的 `b"abc"[0] == 97` 一致) |

`bytes` 值与其他 Cobrust 堆值行为一致:它由你的 `.cb` 作用域拥有,作用域
结束时自动释放一次。你永远不需要手写释放 —— 也没有垃圾回收器。这与
`str`、`list` 采用的所有权纪律完全相同。

## 为什么这样设计?

- **贴合 LLM 的书写习惯。** `b"..."`、`len(b)`、`b[i]` 正是 Python 的写法。
  `b[i]` 返回 `int`(字节值)而非 1 字节的 `bytes` —— 这是 Python 3 的语义,
  也是智能体一次就能写对的形式(CLAUDE.md §2.5,LLM-first 北极星)。
- **字节保持精确。** 在 ADR-0093 之前,`b"..."` 字面量被强行走字符串机制,
  而字符串假定 UTF-8 —— 于是像 `\xff` 这样的非文本字节会被悄悄破坏。专用的
  `bytes` 运行期保证每个字节都原样保留。
- **不会重复释放,也不会泄漏。** 即使在紧凑的循环里,`bytes` 值也只在作用域
  退出时释放一次(运行期通过 1000 次“分配/读取/释放”压力测试验证)。

## 已推迟的部分(诚实的路线图)

以下功能 **尚未** 进入第 1 阶段(ADR-0093 第 2 阶段):

- 切片 `b[lo:hi] -> bytes`
- 拼接 `b1 + b2` 与相等比较 `b1 == b2`
- 方法 `.hex()`、`.decode()`(bytes → str)与 `str.encode()`(str → bytes)
- dora 流访问器 `event.data_bytes()` / `event.send_output_bytes(...)`

今天你已经可以持有、测量并索引 `bytes` 值;切片与拼接将在后续增量中落地。

## 另见

- `docs/agent/adr/0093-bytes-runtime-c-abi.md` —— 运行期 + C-ABI 设计。
- `docs/human/zh/design-philosophy.md` —— 为什么 Cobrust 摒弃 Python 的隐式强制转换。
