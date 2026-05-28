# `import strike` —— 在 Cobrust 中发起 HTTP 调用

> 状态:ADR-0072 第三个模块验证(third-module proof)。在 `den`
> (SQLite)跑通整条生态导入链路、`nest`(TOML)证明链路能推广到纯
> 字符串变换之后,`strike`(`requests` 的更名版,HTTP 客户端)证明
> 同一条链路能承载第二个**带句柄**的模块 —— 自己的 `Response` 类型、
> 自己的 drop 符号、自己的"借用而非消费"的方法面 —— 而无需改动链路逻辑
> 中的任何一行。

## 先看例子

```python
import strike

fn main() -> i64:
    let resp = strike.get("http://127.0.0.1:8080/ping")
    let body: str = resp.text()
    let code: i64 = resp.status_code()
    print(body)
    print(code)
    return 0
```

编译并对一个监听该 URL 的 HTTP 服务运行:

```bash
cobrust build prog.cb -o prog
./prog
# pong
# 200
```

## 你能用到什么(第三个模块验证的接口面)

- **`strike.get(url) -> Response`** —— 对 `url` 发起一个 HTTP `GET`
  请求,返回一个调用方持有的 `Response` 句柄。
- **`strike.post(url, body) -> Response`** —— 对 `url` 发起 `POST`,
  请求体是一个字符串。
- **`Response.text() -> str`** —— 把响应体读成 UTF-8 字符串(非 UTF-8
  字节会用替换字符兜底)。
- **`Response.status_code() -> i64`** —— 读 HTTP 状态码
  (`200`、`404` 等)。
- **`Response.json() -> str`** —— 把响应体按 JSON 解析、再渲染成规范化
  的紧凑 JSON 字符串。形状与第一验证里 `den.fetchall() -> str` 一致;
  带类型的结构化值接口是后续项。

`Response` 句柄归它所在的 `let` 绑定所有;编译器在作用域结束时刚好释放
它一次。你不用写任何 `del` / `close` / `free` —— drop 调度会替你做。

## 网络挂了会怎样?

HTTP 这条接口面**不会 panic、不会返回 null**。任何网络错误、URL 不合法、
DNS 抽风的情况下你都会拿到一个 `Response` —— 只是它的 `status_code()`
是 `0`、它的 `text()` 是空串。规范的判断写法是:

```python
let resp = strike.get(some_url)
if resp.status_code() == 0:
    print("network unreachable")
else:
    print(resp.text())
```

`json()` 在响应体不是合法 JSON 时返回 `{}` 这个哨兵。和 Cobrust 运行时
其它地方一致的约定 —— 干净失败,绝不在 C-ABI 边界上 panic。

## 为什么这样设计?

- **证明链路能承载第二个带句柄的模块。** `den` 是第一个,`strike` 是
  第二个。本次接线复用了前两次验证落地的每一层 —— manifest、类型检查、
  MIR retarget、codegen extern、drop 调度、链接定位器 —— 一处没改。
  只新增了数据。
- **每个模块预留 256 个 AdtId。** `den` 占
  `0xE000_0000..0xE000_00FF`,`strike` 占
  `0xE000_0100..0xE000_01FF`。新的带句柄模块各自分配新的 256 位区块。
  模块之间永远不会撞 id,每个模块也有大约 256 个句柄类型的余量。
- **方法借用句柄,而非消费句柄。** `resp.text()`、`resp.status_code()`、
  `resp.json()` 都**借用**句柄(和 `den` 里 `cur.fetchall()` 同款)。
  运行时不会把 Response 从你手上拿走 —— 你可以连着调
  `status_code()`、再调 `text()`、再调一次 `status_code()`,都作用在
  同一个 `resp` 上。
- **只链接你 import 的东西。** import 了 `strike` 的程序链接
  `libstrike.a`,没 import 的就不链接。不臃肿。

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸顶层语句是工具链另一块尚未
  完成的部分)。
- `json()` 目前返回规范化 JSON 字符串,不是带类型的 Cobrust 值树。
  下游代码先用任意 JSON 解析器把它解回来即可 —— 和今天的
  `den.fetchall()` 返回行文本是同样的形状。
- 源码层上写显式的句柄类型标注(`let resp: strike.Response = ...`)
  尚未走通生态 manifest 的类型解析路径。先去掉标注、让类型推导来做,
  就像上面例子那样。已记录为后续项。
- 错误路径目前用 `status_code() == 0` 哨兵;带类型的
  `Result[Response, HttpError]` 接口是已记录的后续项。
- `Response` 句柄是作用域局部的(不能 return / 存到容器 / 被闭包
  捕获)。单线程使用。Cobrust 结构化并发运行时在 M8+ 落地;之前
  `strike` 仅同步。

这些都是已记录在案的后续项,而非死路 —— 这套接线方式从这里就可以推广
到其余生态库。
