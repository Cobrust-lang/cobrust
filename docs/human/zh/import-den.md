# `import den` —— 在 Cobrust 中使用 SQLite 数据库

> 状态:ADR-0072 首个验证(first proof)。这是第一个可以在 `.cb` 程序里
> `import` 并真正端到端调用(编译 → 链接 → 运行)的生态库。它把 `den`
> (Cobrust 版的 `sqlite3`)接到了编译器的 intrinsic / C-ABI / 静态链接
> 链路上。

## 先看例子

```python
import den

fn main() -> i64:
    let conn = den.connect(":memory:")
    let cur = conn.execute("CREATE TABLE t(x INTEGER)")
    let _ = conn.execute("INSERT INTO t VALUES (42)")
    let rows = conn.execute("SELECT x FROM t").fetchall()
    print(rows)        # -> [(42,)]
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
# [(42,)]
```

## 你能用到什么(首个验证的接口面)

- **`den.connect(path)`** —— 打开数据库。传 `":memory:"` 用内存库,或传
  文件路径。返回一个 `Connection`。
- **`conn.execute(sql)`** —— 执行一条 SQL 语句(CREATE / INSERT /
  SELECT / …)。返回一个 `Cursor`。
- **`cur.fetchall()`** —— 返回结果行。在这个首个验证里,行会以 Python 的
  打印形式被渲染成一个字符串 —— 一个元组列表,例如 `[(42,)]`。(返回带
  类型的 `list[tuple]` 是下一步。)

`Connection` 和 `Cursor` 是真实而独立的句柄类型:编译器知道 `execute`
是 `Connection` 的方法、`fetchall` 是 `Cursor` 的方法,用错会在编译期
报错,而不是在运行期出意外。

## 为什么这样设计?

- **复用已验证的路径。** 调用 `den.connect` 编译后,落到与 `print`、
  `json_loads` 完全相同的那类 C-ABI 调用;运行期没有任何新东西,所以
  既快又可预测。
- **句柄自动清理。** 每个 `Connection` / `Cursor` 在离开作用域时正好被
  释放一次 —— 不需要手动 `close()`,不漏内存,也不会重复释放。清理由
  编译器替你调度。
- **只链接你 import 的东西。** import 了 `den` 的程序会链接
  `libden.a`;没 import 的就不会。不臃肿。

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸的顶层语句是工具链另一块尚未完成
  的部分)。
- 暂时让句柄保持在函数局部 —— 先不要把 `Connection` / `Cursor` 返回或
  跨作用域存储。
- 单线程:不要把连接交给 spawn 出去的任务。

这些都是已记录在案的后续项,而非死路 —— 这套接线方式可以从这里推广到
其余生态库(`coil`、`pit` …)。
