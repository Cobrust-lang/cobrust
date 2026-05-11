# 用 Cobrust 刷 LeetCode

> 从零到第一题 Two Sum，30 分钟入门指引。

## 预备（Prerequisites）

- 已安装 Cobrust v0.1.2+，参照 [入门 — 30 秒安装](getting-started.md)
- 验证安装：

  ```bash
  cobrust --version
  # 期望输出: cobrust 0.1.2
  ```

- 需要：`cargo`（用于从源码编译 `.cb` 文件）

---

## 5 分钟看懂 Cobrust 跑 LeetCode 的两条路

Cobrust 程序可以通过两种方式接收输入：

### 路 1：stdin（标准输入，OJ 主流方式）

```bash
printf "4\n2\n7\n11\n15\n9\n" | cobrust run examples/leetcode/two_sum.cb
```

- 对应 `input("")` 调用，每次读取一行
- EOF（输入结束）时返回空字符串 `""`，而不是抛出异常
- 最适合 LeetCode / OJ 竞赛场景

### 路 2：argv（命令行参数）

```bash
cargo run -p cobrust-cli -- build examples/leetcode/two_sum.cb -o /tmp/two_sum
/tmp/two_sum arg1 arg2
```

- 对应 `argv()` 调用，返回 `list[str]`，第一个元素是程序路径
- 适合参数化调用或工具类脚本

> 推荐用路 1（stdin + `input()`）编写 LeetCode 题解，与 OJ 平台输入格式一致。

---

## 第 1 题：Two Sum（10 分钟手把手）

### 题目

给定 N 个整数和一个目标值 `target`，找两个下标 `i < j` 使得 `nums[i] + nums[j] == target`。

### 输入格式（stdin，4 行）

```
N           ← 数组长度
nums[0]     ← 第 0 个整数（每行一个）
nums[1]
...
nums[N-1]
target      ← 目标值
```

**示例输入**（N=4，nums=[2,7,11,15]，target=9）：

```
4
2
7
11
15
9
```

**期望输出**：

```
0
1
```

### 完整 Cobrust 代码

```cobrust
# LC-01 Two Sum (ADR-0044 W2 Phase 3).
#
# Input  (stdin):
#   Line 1: N
#   Lines 2..N+1: one integer each
#   Line N+2: target
#
# Output: indices i j (i < j) with nums[i]+nums[j]==target, one per line.
#
# Algorithm: O(N²) brute-force scan.

fn main() -> i64:
    let n = parse_int(input(""))
    let nums = list_new(n)
    let i: i64 = 0
    while i < n:
        let v = parse_int(input(""))
        list_set(nums, i, v)
        i = i + 1
    let target = parse_int(input(""))
    let a: i64 = 0
    while a < n:
        let b: i64 = a + 1
        while b < n:
            if list_get(nums, a) + list_get(nums, b) == target:
                print_int(a)
                print_int(b)
                return 0
            b = b + 1
        a = a + 1
    return 0
```

### 跑起来

```bash
cd /path/to/cobrust
printf "4\n2\n7\n11\n15\n9\n" | cargo run -p cobrust-cli -- run examples/leetcode/two_sum.cb
# 期望输出:
# 0
# 1
```

或先编译再运行：

```bash
cargo run -p cobrust-cli -- build examples/leetcode/two_sum.cb -o /tmp/two_sum
printf "4\n2\n7\n11\n15\n9\n" | /tmp/two_sum
# 期望输出:
# 0
# 1
```

### Cobrust vs Python 对照

| 功能 | Python | Cobrust |
|---|---|---|
| 读取一行 | `s = input()` | `let s = input("")` |
| 解析整数 | `int(s)` | `parse_int(s)` |
| 创建列表 | `nums = [0] * n` | `let nums = list_new(n)` |
| 写入列表 | `nums[i] = v` | `list_set(nums, i, v)` |
| 读取列表 | `nums[i]` | `list_get(nums, i)` |
| 打印整数 | `print(x)` | `print_int(x)` |
| 打印字符串 | `print(s)` | `print(s)` |
| EOF 处理 | 抛出 `EOFError` | 返回 `""` |

---

## 10 题完整 Catalog

完整示例见 [`examples/leetcode/README.md`](../../../examples/leetcode/README.md)。

| # | 题目 | 难度 | 关键语言点 |
|---|---|---|---|
| 01 | Two Sum | Easy | `list_new` / `list_get` / `list_set` |
| 02 | Reverse String | Easy | `str_len` / `str_at` / `print_no_nl` |
| 03 | Fibonacci | Easy | 递归 / DP `while` 循环 |
| 04 | Valid Parentheses | Easy | `str_eq_lit` / `list_new` 模拟栈 |
| 05 | Merge Two Sorted Lists | Easy | `count_toks` / `parse_int_tok` |
| 06 | Maximum Subarray | Easy | Kadane's 算法，`while` + 局部变量 |
| 07 | Binary Search | Easy | `while` 二分查找，返回下标或 -1 |
| 08 | Climbing Stairs | Easy | DP，`while` 滚动变量 |
| 09 | Best Time to Buy and Sell Stock | Easy | 贪心，单次遍历 |
| 10 | Roman to Integer | Easy | `str_ord` / `str_at` 字符映射 |

---

## Cobrust LeetCode 风格指南

### 输入处理

- **推荐**：`input("")` — 读一行，去除末尾换行，EOF 返回 `""`
- **不推荐**：`read_line()` — 保留末尾 `\n`，需手动处理
- 读取整数：`parse_int(input(""))`
- 读取同行多个整数（如 `"3 5 7"`）：`parse_int_tok(line, i)` 读第 `i` 个
- 统计一行中的 token 数：`count_toks(line)`

### argv 用法

```cobrust
fn main() -> i64:
    let args = argv()        # list[str]，args[0] 是程序路径
    let n = parse_int(args[1])
    ...
    return 0
```

### 数据类型现状

| 类型 | 状态 | 备注 |
|---|---|---|
| `i64` | 可用 | 整数，64 位有符号 |
| `str` | 可用 | UTF-8 字符串 |
| `list[i64]` | 可用 | 用 `list_new` / `list_get` / `list_set` |
| `list[str]` | 可用 | `argv()` 返回此类型 |
| `dict` | 暂未实现 | Phase F 路线图 |
| `f64` | 暂未实现 | Phase F 路线图 |

### print 用法

- `print(s)` — 打印字符串并自动加 `\n`
- `print_int(n)` — 打印整数并自动加 `\n`
- `print_no_nl(s)` — 打印字符串，不加 `\n`（适合逐字符输出）

---

## 常见坑（Gotchas）

### 1. 不能用隐式 bool

```cobrust
# 错误写法 — Cobrust 不允许隐式 truthiness
if x:
    ...

# 正确写法
if x > 0:
    ...
if !s.is_empty():
    ...
```

### 2. 变量重赋值不用 `let mut`

```cobrust
# 正确写法 — 直接赋值即可，无需 mut
let x: i64 = 0
x = x + 1

# 声明时类型可推断
let n = parse_int(input(""))
```

### 3. 没有 `is`，只有 `==`

```cobrust
# 错误写法 — Cobrust 删除了 is
if a is b:
    ...

# 正确写法
if a == b:
    ...
```

### 4. EOF 检测

```cobrust
# input() 在 EOF 时返回 ""，不抛异常
let line = input("")
while !str_eq_lit(line, ""):    # str_eq_lit 比较字符串与字面量
    # 处理 line
    line = input("")
```

### 5. 字符串比较

```cobrust
# 正确写法 — 与字符串字面量比较用 str_eq_lit
if str_eq_lit(s, "true"):
    print("matched")
```

---

## 下一步

- 浏览 [`examples/leetcode/README.md`](../../../examples/leetcode/README.md)，查看所有 10 题的输入格式和运行方式
- 想贡献新题？参看 [`CONTRIBUTING.md`](../../../CONTRIBUTING.md)
- 语言路线图：[ADR-0038 Phase F roadmap](../../agent/adr/0038-phase-f-roadmap.md) — 更多 stdlib、Python 题库翻译计划
- stdin/argv 技术详情：[ADR-0044](../../agent/adr/0044-stdin-argv-source-binding.md)
