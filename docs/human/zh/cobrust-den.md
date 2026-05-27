# cobrust-den —— 面向 Cobrust 的 SQLite(PEP 249 DB-API 2.0)

`cobrust-den` 是对 Python 标准库 `sqlite3` 模块的 Cobrust 翻译。它提供
你熟悉的 DB-API 2.0 接口 —— `connect(...).cursor().execute(...).fetchall()`
—— 底层由成熟的 Rust `rusqlite` crate 驱动。SQLite 本身被内置(从源码编译),
因此**无需安装任何系统库**。

它是 v0.7.0 的“必须交付”数据库连接器(子流 Z.7.c)。

## 先看示例

一次完整的往返 —— 建表、用绑定参数插入、再读回:

```rust
use den::{connect, Value, MEMORY};

// 1. 打开一个内存数据库(也可以传文件路径)。
let conn = connect(MEMORY)?;          // MEMORY == ":memory:"
let mut cur = conn.cursor();

// 2. 建表。
cur.execute(
    "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
    &[],
)?;

// 3. 用 qmark(`?`)占位符插入 —— PEP 249 的 "qmark" 参数风格。
cur.execute(
    "INSERT INTO people (name, score) VALUES (?, ?)",
    &[Value::Text("ada".to_owned()), Value::Real(9.5)],
)?;
println!("inserted rowid = {:?}", cur.lastrowid()); // Some(1)

// 4. 查询并读回各行。
cur.execute("SELECT id, name, score FROM people", &[])?;
for row in cur.by_ref() {
    println!("{:?}", row.cells());
}
```

它的形态与你在 Python 中所写的一致:

```python
import sqlite3
conn = sqlite3.connect(":memory:")
cur = conn.cursor()
cur.execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
cur.execute("INSERT INTO people (name, score) VALUES (?, ?)", ("ada", 9.5))
cur.execute("SELECT id, name, score FROM people")
for row in cur:
    print(row)
```

## 你能得到什么

- **`connect(path)`** —— 打开 `":memory:"`(通过 `MEMORY` 常量)或文件路径。
  返回 `Result<Connection, SqliteError>`。
- **`Connection`** —— `.cursor()`、`.execute(sql)`、
  `.execute_params(sql, params)`、`.commit()`、`.rollback()`、`.close()`。
- **`Cursor`** —— `.execute(sql, params)`、`.fetchone()`、`.fetchmany(n)`、
  `.fetchall()`、`.rowcount()`、`.lastrowid()`,并且可以直接迭代它
  (`for row in cursor`)。
- **`Value`** —— SQLite 的五种存储类:
  `Null / Integer / Real / Text / Blob`(对应 Python 的
  `None / int / float / str / bytes`)。
- **`Row`** —— 按位置访问单元格:`row.get(i)`、`row.cells()`。

## 五种 SQLite 类型

SQLite 把每个值存储为五种存储类之一。它们与 `Value`、与 Python 一一对应:

| SQLite      | `Value`             | Python  |
|-------------|---------------------|---------|
| `NULL`      | `Value::Null`       | `None`  |
| `INTEGER`   | `Value::Integer(i64)` | `int`   |
| `REAL`      | `Value::Real(f64)`  | `float` |
| `TEXT`      | `Value::Text(String)` | `str`   |
| `BLOB`      | `Value::Blob(Vec<u8>)` | `bytes` |

## 错误是值,而非异常

Python 的 `sqlite3` 会抛出异常(`OperationalError`、`IntegrityError`、
`ProgrammingError` 等)。Cobrust 改为返回 `Result<T, SqliteError>` —— 你用
`?` 或 `match` 处理失败,编译器确保你不会遗漏。错误种类:

| `SqliteErrorKind` | 何时发生 | Python 对应 |
|---|---|---|
| `CannotOpen` | 数据库文件无法打开 | `OperationalError` |
| `Sql` | SQL 语法错误 / 表或列不存在 | `OperationalError` |
| `Constraint` | 违反 UNIQUE / NOT NULL / FK / CHECK | `IntegrityError` |
| `Parameter` | `?` 参数个数不匹配 | `ProgrammingError` |
| `TypeMismatch` | 某个单元格无法投影 | (少见) |
| `Other` | libsqlite3 的其他错误 | `DatabaseError` |

错误查询绝不会 panic —— 它返回 `Err`:

```rust
let err = cur.execute("SELCT oops", &[]).unwrap_err();
assert_eq!(err.kind, den::SqliteErrorKind::Sql);
```

## 为什么这样设计?

- **贴合 Python 的先验。** 宪章的 LLM 优先原则(§2.5)指出,Cobrust 是 AI
  代理“一次写对”的语言。
  `connect(":memory:").cursor().execute(...).fetchall()` 正是训练数据里的
  规范写法,所以我们保留它。
- **用 `Result`,不用异常。** 宪章(§2.2)把 `Result<T, E>` 定为默认错误路径。
  封闭的 `SqliteErrorKind` 枚举意味着对失败模式的 `match` 是穷尽的 —— 类型
  检查器会抓住你遗漏的分支。
- **同步,而非异步。** SQLite 是嵌入式引擎,没有网络往返;`rusqlite` 是同步的,
  Python 的 `sqlite3` 也是同步的。这里不存在需要引入的“双色”异步问题(§2.2),
  所以接口保持同步。
- **内置 SQLite。** 从源码编译 libsqlite3(`rusqlite` 的 `bundled` 特性)让
  构建可复现、可移植 —— 无需追逐系统包。

## 兼容层级:`semantic`

`cobrust-den` 被标记为 `@py_compat(semantic)`。它保留了 PEP 249 的行为
以及类型映射,但并非与 CPython 逐字节一致。已知差异:

- `SELECT` 的各行在 `execute` 时被一次性读入内存(Python 是惰性获取)。
  `fetchone` / `fetchmany` / `fetchall` / 迭代会以相同顺序返回相同的行。
- 错误是 `Result::Err`,而非抛出的异常。
- 行只支持按位置访问(暂无 `sqlite3.Row` 名称映射)。
- 非 UTF-8 的 `TEXT` 会用 Unicode 替换字符解码(与默认 `text_factory` 一致)。

## 暂不支持

- 命名 / 数字参数风格(`:name`、`?1`)。
- `executemany` / `executescript`。
- `sqlite3.Row` 命名访问与 `row_factory`。
- 直接从 Cobrust `.cb` 源码使用 SQLite(`import den`)—— 该接线是一个
  独立、延后的步骤。
