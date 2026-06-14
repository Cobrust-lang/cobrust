# 字符串比较 — `<` `<=` `>` `>=`(字典序)

> ADR-0104(关闭 F92)。两个字符串可以用排序运算符 `<`、`<=`、`>`、`>=`
> 进行比较 —— **按 Unicode 码点的字典序**,与 Python 完全一致。§2.5
> LLM-first:对字符串排序和比较是 LLM agent 最常写的操作之一,因此编译器
> 现在能够开箱即用地接受它(在 F92 之前这会让编译器崩溃)。

## 先看例子

```cobrust
fn main() -> i64:
    print("abc" < "abd")      # True
    print("abc" > "abd")      # False
    print("a" <= "a")         # True
    print("b" >= "a")         # True
    return 0
```

与 Python 完全一致:`"abc" < "abd"` 为 `True`。

作为另一个字符串**前缀**的字符串**更小**,空字符串是最小的:

```cobrust
fn main() -> i64:
    print("ab" < "abc")       # True  (前缀更小)
    print("abc" < "ab")       # False
    print("" < "a")           # True  (空串是最小值)
    return 0
```

在任何你期望的地方使用它 —— 在 `if` 中、在变量上、在排序时:

```cobrust
fn main() -> i64:
    let a: str = "apple"
    let b: str = "banana"
    if a < b:
        print("a before b")   # a before b
    return 0
```

`==` 和 `!=` 早已可用,且保持不变:

```cobrust
fn main() -> i64:
    print("abc" == "abc")     # True
    print("abc" != "abd")     # True
    return 0
```

## 比较按码点进行

字符串按 **Unicode 码点的字典序**比较,与 Python 使用的顺序相同。非 ASCII
字符按其码点值比较:

```cobrust
fn main() -> i64:
    print("é" < "f")          # False  (é 是 U+00E9 = 233,f 是 U+0066 = 102)
    print("é" > "f")          # True
    return 0
```

这与 CPython 完全一致,其中 `ord('é') == 233 > ord('f') == 102`。

## 哪些仍会(干净地)拒绝

- **混合类型** —— 将 `str` 与数字比较是**编译期类型错误**(退出码 2),
  绝不会崩溃:

  ```cobrust
  fn main() -> i64:
      print("abc" < 5)        # error[Type]: type mismatch: expected `str`, found `i64`
      return 0
  ```

- **`bytes` 排序** —— `b"a" < b"b"` **尚不支持**,会在编译期拒绝并打印
  修复建议(比较 `len(a)` 与 `len(b)`,或在两侧都是有效 UTF-8 时对两侧
  `.decode()`)。这是一次干净的拒绝,绝不会崩溃。

## 设计说明

- **F92 修复**:在 ADR-0104 之前,`"abc" < "abd"` 会**让编译器崩溃**
  (`cobrust build` 以 101 退出,codegen panic)—— 类型检查器接受了它,
  但 codegen 没有处理字符串操作数的路径。这违反了"编译器绝不能在已通过
  类型检查的输入上 panic"的规则(§5.1)。F92 选择实现该运算符,而非拒绝它。
- **运行时**:每次比较都调用 `__cobrust_str_cmp(a, b)`,它返回 -1 / 0 /
  +1(Rust `str::cmp` 的符号)。然后用对应运算符将结果与 0 比较(`a < b`
  变为 `cmp(a, b) < 0`,以此类推)。字符串只是被**借用**,绝不消耗,因此
  之后仍可使用。
- **字节序 = 码点序**:Rust 的 `str::cmp` 比较 UTF-8 字节,但 UTF-8 是
  保序的,因此对有效文本而言字节序等于码点序 —— 这正是 Python 的语义。
- 这服务于 §2.5 "LLM-first" 原则:LLM 会根据其 Python 先验写出 `s1 < s2`
  (排序、比较、二分查找);在其上崩溃会迫使其改写为非惯用形式。
