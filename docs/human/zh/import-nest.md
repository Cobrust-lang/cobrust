# `import nest` —— 在 Cobrust 中解析 TOML

> 状态:ADR-0072 第二个模块验证(second-module proof)。在 `den`
> 跑通整条生态导入链路之后,`nest`(即 `tomli` 的更名版,TOML 解析库)
> 是把这条链路推广开的最便宜的一步 —— 一个纯字符串进、字符串出的函数,
> 没有任何需要管理的句柄。

## 先看例子

```python
import nest

fn main() -> i64:
    let toml_input: str = "title = \"hello\"\n[server]\nport = 8080\n"
    let canonical_json: str = nest.loads_str(toml_input)
    print(canonical_json)
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
# {"server":{"port":8080},"title":"hello"}
```

## 你能用到什么(第二个模块验证的接口面)

- **`nest.loads_str(toml) -> str`** —— 把 `toml` 里的 TOML 源码解析成
  规范化的 JSON 字符串。字符串进,字符串出。解析失败时返回一个
  `{"err": "<message>"}` 形状的 JSON 哨兵(带类型的 `Result` 接口是
  后续项)。

目前的接口就这一个:用最小可用的自由函数把链路自顶向下走通,证明它能
推广到第二个模块。

## 为什么这样设计?

- **证明这条链路不是 `den` 专属。** `nest` 的接线复用了 `den` 首验证
  落地的每一层 —— manifest、类型检查、MIR retarget、codegen extern、
  链接定位 —— 没有任何修改。本次只新增了一条 manifest 行、一段
  codegen extern 声明、新的 C-ABI shim、以及在符号前缀识别表里加了
  一行。
- **没有句柄就没有逃逸规则。** TOML→JSON 规范化是纯粹的值变换;不需要
  让什么东西跨作用域存活,也不需要显式释放任何东西。编译器现有的字符串
  drop 调度就把事情做对了。
- **只链接你 import 的东西。** import 了 `nest` 的程序会链接
  `libnest.a`;没 import 的就不会。不臃肿。

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸的顶层语句是工具链另一块尚未
  完成的部分)。
- 返回的字符串目前是 JSON 规范化文本,而不是带类型的 Cobrust 值树 ——
  下游代码先用任意一个 JSON 解析器把它解回来即可(与今天的
  `den.fetchall()` 的渲染形状一致)。
- 解析失败用的是 JSON 字符串哨兵(`{"err":"…"}`);带类型的
  `Result[str, Error]` 接口是已记录的后续项。

这些都是已记录在案的后续项,而非死路 —— 这套接线方式从这里就可以推广
到其余生态库。
