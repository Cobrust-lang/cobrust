# `import math` — 在 Cobrust 中做标量数学

> 状态:ADR-0083。第一个被接入的 Python 核心标准库模块(`json` / `re` /
> `datetime` 还在路上)。`math` 提供标量 `f64` 数学运算 —— `math.sqrt`、
> `math.sin`、`math.pi` —— 也就是你每天都在写的那些数值惯用法。

## 先看例子

```python
import math

fn main() -> i64:
    print(math.sqrt(2.0))                                  # 1.4142135623730951
    print(math.pi)                                         # 3.141592653589793
    print(math.pow(2.0, 10.0))                             # 1024
    print(math.hypot(3.0, 4.0))                            # 5
    let h: f64 = math.sqrt(math.pow(3.0, 2.0) + math.pow(4.0, 2.0))
    print(h)                                               # 5
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
```

## 你得到了什么

### 函数(18 个)

- **单参数**(`f64 -> f64`):`math.sqrt`、`math.sin`、`math.cos`、
  `math.tan`、`math.asin`、`math.acos`、`math.atan`、`math.sinh`、
  `math.cosh`、`math.tanh`、`math.exp`、`math.log`(自然对数)、
  `math.log10`、`math.log2`、`math.fabs`。
- **双参数**(`(f64, f64) -> f64`):`math.pow(x, y)`、
  `math.atan2(y, x)`、`math.hypot(x, y)`。

### 常量(5 个)

- `math.pi` → `3.141592653589793`
- `math.e` → `2.718281828459045`
- `math.tau` → `6.283185307179586`

常量就是普通属性 —— 写 `math.pi`,绝不是 `math.pi()`。

无穷与非数请写**裸字面量** `inf` 和 `nan`(例如 `let big: f64 = inf`),**而不是**
`math.inf` / `math.nan`:Cobrust 的词法器已把 `inf`、`nan` 这两个词直接当作浮点
字面量,因此 `math.` 限定写法无法解析。(`math.inf` 形式是后续的解析器增量 —— 见
ADR-0083。)

### 暂未支持(后续补充)

`math.floor` / `math.ceil` / `math.trunc`(在 Python 里返回 **int**,需要
额外的类型转换)以及 `math.factorial` / `math.gcd` / `math.isqrt`(整数
数学)被推迟到后续。

## 需要记住的两条规则

### 1. 参数必须是浮点数 —— 写 `2.0`,而不是 `2`

Cobrust 绝不会悄悄把整数变成浮点数(宪法 §2.2)。`math.sqrt(2)` 是一个
**编译期错误**:

```python
print(math.sqrt(2))    # 错误:TypeMismatch { expected: Float, actual: Int }
print(math.sqrt(2.0))  # 正确
```

这与数组库 `coil` 遵循的规则相同(`coil.power(a, 0.0)`),意味着参数类型
错误会在编译时被抓住,而不是等到运行时。

### 2. 超出定义域的输入返回 `NaN` / `-inf`,而不是抛错

Python 的 `math.sqrt(-1)` 会抛出 `ValueError`。Cobrust 改为遵循底层 C 数学
库,返回 IEEE 浮点值:

```python
print(math.sqrt(-1.0))   # NaN
print(math.log(0.0))     # -inf
```

没有异常,没有陷阱,也绝不会返回一个错误的有限数值 —— 你拿到的是诚实的
浮点结果。(这就是声明的“数值层(numerical-tier)”行为;参见“为什么这样
设计?”。)

## 为什么这样设计?

- **内核就是 C 数学库。** `math.sqrt(x)` 编译为对 `libm` 的一次直接
  `call sqrt(double)`,而 `libm` 本来就已经被链接进来。没有新 crate、没有
  包装层、没有新依赖 —— 这是最快也最简单的路径。
- **`math` 是标量;`coil` 是数组。** `coil.sqrt(a)` 接收整个缓冲区并返回
  一个缓冲区;`math.sqrt(x)` 接收一个数并返回一个数。它们毫无交集,永不
  冲突。
- **数值层,如实声明。** `sqrt` 和各常量是逐位精确且跨平台一致的。而
  超越函数(`sin`、`cos`、`atan2` 等)可能与 CPython 不同 —— 在 macOS 与
  Linux 之间也可能 —— 差在最后一个比特位,因为它们用的是各平台自己的
  `libm`。定义域行为(NaN/-inf 对比 Python 的 `ValueError`)是我们事先就
  写明的那一处刻意分歧。
- **常量是零成本的。** `math.pi` 是在编译期就烘焙进程序里的一个数 ——
  运行时根本没有函数调用。

## 关于打印的说明

浮点打印器在结果是整数值时不显示末尾的 `.0`:`math.hypot(3.0, 4.0)` 打印
`5`,而非 `5.0`;`math.pow(2.0, 10.0)` 打印 `1024`。超出定义域的结果打印为
`NaN` 和 `-inf`。这只是显示上的选择,而非数值上的差异 —— 数值本身完全
符合你的预期。
