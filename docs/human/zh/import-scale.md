# `import scale` —— 在 Cobrust 中编码/解码 msgpack

> 状态:ADR-0072 第四个模块验证(fourth-module proof)。继 `den`(SQLite,
> 句柄模式)、`nest`(TOML,值模式)、`strike`(HTTP,句柄+自由函数)
> 之后,`scale`(`msgpack-python` 的更名版,msgpack 库)是接到同一条
> 生态导入链路上的**第四个**模块 —— 在 nest 的基础上做了一次值模式
> 推广,JSON 进、十六进制(HEX)出的往返。

## 先看例子

```python
import scale

fn main() -> i64:
    let packed: str = scale.dumps_str("{\"key\":\"value\"}")
    let back: str = scale.loads_str(packed)
    print(back)
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
# {"key":"value"}
```

## 你能用到什么(第四个模块验证的接口面)

- **`scale.dumps_str(json_input) -> str`** —— 把 `json_input` 当 JSON
  解析,用 msgpack 编码值树,然后把结果字节用小写十六进制(HEX)塞进
  一个 Cobrust `str`。可打印到 stdout,也容易往回还原。
- **`scale.loads_str(packed) -> str`** —— 把 HEX 解回 msgpack 字节,
  `unpack` 出值树,然后把值渲染回规范化 JSON 字符串。形状和
  `nest.loads_str` 一致(一段规范化 JSON 文本)。

目前的接口就这两个:用最小可用的值模式往返,把链路自顶向下走通,证明它
能推广到第四个模块。

## 当输入不合法时会怎样?

msgpack 接口面**永远不会 panic,也不会返回 null**。任何错误(`dumps_str`
拿到非法 JSON;`loads_str` 拿到非 hex / 损坏字节 / 非 msgpack 输入)
都会返回**空字符串哨兵**。惯用写法是:

```python
let packed = scale.dumps_str(maybe_bad_json)
if str_len(packed) == 0:
    print("input was not valid JSON")
else:
    print(packed)
```

这与 Cobrust 运行时对所有值模式 shim 的约定一致 —— 优雅失败,绝不
跨 C-ABI 边界 panic。

## 为什么这样设计?

- **证明这条链路能承载第二个值模式模块。** `nest` 是第一个值模式模块,
  `scale` 是第二个。链路的每一层都直接复用了 den/nest/strike 验证已
  落地的代码,没有改动;本次新增的只是数据(manifest 行 + codegen
  extern 行 + recognizer 一行),外加新 shim crate。
- **HEX 渲染把接口面留在 str→str。** msgpack 原生 ABI 是裸字节,但
  把 `*mut u8` 字节 ABI 接到这条链路上自成一次重新设计(ADR-0072
  Q5 处理的是字符串而非字节)。首验证的形状沿用 nest 已经走通的
  str→str 路径,把 msgpack 字节渲染成可打印的 HEX。裸字节 ABI 是
  已记录的后续项。
- **只链接你 import 的东西。** import 了 `scale` 的程序会链接
  `libscale.a`;没 import 的就不会。不臃肿。

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸的顶层语句是工具链另一块尚未
  完成的部分)。
- 编码格式是 HEX-of-msgpack,不是裸字节。需要把字节写到二进制文件的
  下游工具现在要自己去 hex。裸 `bytes` 接口是已记录的后续项。
- `loads_str` 的规范化 JSON 渲染与 `nest.loads_str` 的形状一致;带类型
  的 Cobrust 值树是已记录的后续项。
- 错误路径是空字符串哨兵;带类型的 `Result[str, ScaleError]` 接口是
  已记录的后续项。

这些都是已记录在案的后续项,而非死路 —— 这套接线方式从这里就可以推广
到其余生态库。
