# `import molt` —— 在 Cobrust 中读取当前时间并格式化

> 状态:ADR-0072 第五个模块验证(fifth-module proof)。继 `den`(SQLite)、
> `nest`(TOML)、`strike`(HTTP)、`scale`(msgpack)之后,`molt`
> (`python-dateutil` 的更名版,datetime 库)是接到同一条生态导入链路
> 上的**第五个**模块 —— 在 den 和 strike 的基础上又做了一次句柄模式
> 推广,带有 `DateTime` 句柄和借用式访问方法。

## 先看例子

```python
import molt

fn main() -> i64:
    let now = molt.now()
    let iso: str = now.isoformat()
    let stamp: i64 = now.unix_timestamp()
    print(iso)
    print(stamp)
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
# 2026-05-28T12:34:56.789012Z
# 1748434496
```

(具体值取决于运行时的实际墙钟。)

## 你能用到什么(第五个模块验证的接口面)

- **`molt.now() -> DateTime`** —— 拿到当前 UTC 时间,返回一个由你持有
  的 `DateTime` 句柄。
- **`DateTime.isoformat() -> str`** —— 把 datetime 渲染成 RFC3339 字符串
  (Python `datetime.isoformat()` 在 UTC-aware datetime 上产生的
  ISO-8601 子集)。
- **`DateTime.unix_timestamp() -> i64`** —— 读取 UNIX 纪元秒数(UTC),
  语义与 Python `int(dt.timestamp())` 在 UTC-aware datetime 上一致。

`DateTime` 句柄归你拿到它的 `let` 绑定所有;编译器在作用域出口处恰好
释放一次。你不必写任何 `del` / `close` / `free` —— drop 调度替你
搞定。

## 出错时会怎样?

两个访问方法**永远不会 panic,也不会返回 null**。在空句柄上,
`isoformat()` 返回空字符串,`unix_timestamp()` 返回 `0`。`molt.now()`
本身在所有支持平台上是全函数(永远成功)。

这与 Cobrust 运行时的约定一致 —— 优雅失败,绝不跨 C-ABI 边界 panic。

## 为什么这样设计?

- **证明这条链路能承载第三个句柄模式模块。** `den` 是第一个句柄模块,
  `strike` 是第二个,`molt` 是第三个。本次接线复用了前面验证已经落地
  的每一层 —— manifest、类型检查、MIR retarget、codegen extern、drop
  调度、链接定位 —— 没有修改,只新增了数据。
- **每模块 256 槽 AdtId 块。** `den` 占
  `0xE000_0000..0xE000_00FF`;`strike` 占
  `0xE000_0100..0xE000_01FF`;`scale` 占
  `0xE000_0200..0xE000_02FF`(目前还没有句柄,但块属于它,以备将来裸字节
  ABI 需要);`molt` 占 `0xE000_0300..0xE000_03FF` 给 `DateTime`。每个
  新句柄模块拿走下一块 256 槽 —— 跨模块永不冲突。
- **借用式方法。** `isoformat()` 和 `unix_timestamp()` 都是借用句柄
  (和 `den` 的 `cur.fetchall()`、`strike` 的 `resp.text()` 一样)。
  你可以在同一个 `now` 绑定上随便调几次;句柄会一直活到作用域出口。
- **只链接你 import 的东西。** import 了 `molt` 的程序会链接
  `libmolt.a`;没 import 的就不会。不臃肿。

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸的顶层语句是工具链另一块尚未
  完成的部分)。
- 首验证只露出 `now()` + `isoformat()` + `unix_timestamp()`。一个
  `parse(s: str) -> DateTime` 构造器以及完整的 `python-dateutil`
  解析接口面是已记录的后续项。
- 源码级别给 `DateTime` 句柄写显式类型注解(`let now: molt.DateTime
  = ...`)目前还没接到生态 manifest;请把注解去掉,让类型推导接管 ——
  就像上面例子那样。这是 ADR-0072 已记录的后续项。
- 错误路径是空字符串 / `0` 哨兵;带类型的 `Result[DateTime,
  MoltError]` 接口是已记录的后续项。
- `DateTime` 句柄是作用域局部的(不可 return / 不可存到结构体里 /
  不可被闭包捕获跨越作用域)。单线程使用。Cobrust 结构化并发运行时
  在 M8+ 才会到位;在那之前 `molt` 是 sync-only。

这些都是已记录在案的后续项,而非死路 —— 这套接线方式从这里就可以推广
到其余生态库。
