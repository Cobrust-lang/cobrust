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

## 现在能做什么

| 形式 | 结果 | 说明 | 阶段 |
|---|---|---|---|
| `b"..."` | `bytes` | 字节串字面量(任意字节,支持 `\xNN` 转义) | 1 |
| `len(b)` | `int` | 字节数 | 1 |
| `b[i]` | `int` | 第 `i` 个字节,范围 `0..255`(与 Python 的 `b"abc"[0] == 97` 一致) | 1 |
| `b[lo:hi]` | `bytes` | 切片(返回全新的 `bytes`);越界时像 Python 一样夹取 | 2 |
| `b1 + b2` | `bytes` | 拼接(返回全新的 `bytes`) | 2 |
| `s.encode()` | `bytes` | `str` 的 UTF-8 字节 | 2 |
| `b.decode()` | `str` | 把 UTF-8 字节解码回 `str`(见下) | 2 |
| `b.hex()` | `str` | 小写十六进制,例如 `b"\xff\x00".hex() == "ff00"` | 2 |

`bytes` 值与其他 Cobrust 堆值行为一致:它由你的 `.cb` 作用域拥有,作用域
结束时自动释放一次。你永远不需要手写释放 —— 也没有垃圾回收器。这与
`str`、`list` 采用的所有权纪律完全相同。上表中每个产生新 `bytes` 或 `str`
的操作(切片 / 拼接 / encode / decode / hex)都给你一个由作用域拥有的 **全新**
值;输入只被读取,绝不会被消耗。

## 切片、拼接与 `str` 桥接(第 2 阶段)

```cobrust
fn main() -> i64:
    let b: bytes = b"hello"
    print(len(b[1:4]))       # 3   (b"ell")
    print(len(b + b))        # 10  (b"hellohello")

    # str <-> bytes 往返
    let s: str = "world"
    let encoded: bytes = s.encode()
    print(len(encoded))      # 5
    print(encoded.decode())  # world

    print(b.hex())           # 68656c6c6f
    return 0
```

切片采用与 Python 相同的夹取语义 —— 越界的上界会被收窄到长度,反向区间
得到空 `bytes`(从不报错):

```cobrust
fn main() -> i64:
    let b: bytes = b"abcd"
    print(len(b[1:99]))   # 3   (夹取为 b"bcd")
    print(len(b[3:1]))    # 0   (空)
    return 0
```

## 解码非法字节 —— 不做隐式强制转换

`b.decode()` 把字节按 UTF-8 读取。如果字节 **不是** 合法 UTF-8,Cobrust
**不会** 悄悄替换成替代字符,也 **不会** 悄悄截断 —— 那正是 Cobrust 拒绝的
隐式强制转换(CLAUDE.md §2.2)。它会 **停止程序**,并给出明确指出第一个非法
字节位置的诊断:

```cobrust
fn main() -> i64:
    let b: bytes = b"\xff\xfe"
    let s: str = b.decode()   # 在这里停止
    print(s)                  # 永不执行
    return 0
```

```
cobrust panic: bytes.decode: invalid utf-8 at byte 0
```

这与 Cobrust 中其他所有不可恢复错误“在前置条件被破坏时大声停止”的行为一致。
未来版本会在该风格于标准库铺开后加入可恢复的、返回 `Result` 的形式;在此之前,
解码非法 UTF-8 是硬停止 —— 但 **绝不** 是悄无声息的损坏。

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

第 2 阶段已落地切片、拼接、`.encode()` / `.decode()` / `.hex()`。以下功能
仍 **尚未** 进入 —— 每一项都是 **清晰的编译期报错**(会告诉你受支持的写法),
绝不是悄无声息的错误结果:

- **两个 `bytes` 的比较**(`b1 == b2`、`<`、`>` 等)是编译期错误。报错会提示
  你改用 `len(a)` 与 `len(b)` 比较,或在两侧都确定是合法 UTF-8 时改用
  `a.decode()` 与 `b.decode()` 比较。(此前这会让编译器崩溃;现在是一条干净的
  诊断。)
- **负数 / 开放端 / 带步长的切片**(`b[1:]`、`b[:3]`、`b[0:4:2]`、`b[1:-1]`)
  是编译期错误 —— 目前只支持简单的 `b[lo:hi]` 形式(两个边界都为非负且都给出)。
  报错会提示你写出两个边界,例如 `b[1:len(b)]`。(此前这会悄悄返回整个缓冲区;
  现在编译器会带着修复建议拦下你。)
- **字面量负标量索引**(`b[-1]`、`b[-2]`)同样是编译期错误 —— 报错会提示你:
  取最后一个字节请写 `b[len(b) - 1]`(非负索引)。(此前 `b[-1]` 会悄悄返回
  哨兵值 `-1`;CPython `b"abc"[-1] == 99`。这是 F79 的修复,是 `str` 的
  `s[-1]` 拒绝的同步孪生。)只有**字面量**负索引会被捕获 —— 当 `i` 是变量时,
  非字面量索引 `b[i]` 仍能通过类型检查;完整的从末尾索引与越界 panic 是
  ADR-0093 列出的后续工作。
- 可恢复的、返回 `Result` 的 `decode()`(今天非法 UTF-8 会停止程序,见上)。

dora 流访问器 `event.data_bytes()` / `event.send_output_bytes(...)` 已
**落地**(ADR-0076c B-1b)—— 机器人节点现在可以把一个 Arrow `Binary`/`UInt8`
载荷读成 `bytes` 并原样发回, 字节精确。表面见
`docs/human/zh/import-dora.md`。

## 另见

- `docs/agent/adr/0093-bytes-runtime-c-abi.md` —— 运行期 + C-ABI 设计。
- `docs/human/zh/design-philosophy.md` —— 为什么 Cobrust 摒弃 Python 的隐式强制转换。
