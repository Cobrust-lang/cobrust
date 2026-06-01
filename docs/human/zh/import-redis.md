# `import redis` —— 在 Cobrust 中使用 Redis 缓存 / 键值存储

> 状态:ADR-0078 Phase-1c。这是第十一个可以在 `.cb` 程序里 `import` 并
> 真正端到端调用(编译 → 链接 → 运行)的生态库。它把 `redis`(Cobrust
> 版的缓存 / KV 客户端,`redis-py` 的更名版)接到了编译器的 intrinsic /
> C-ABI / 静态链接链路上,底层用的是 Rust `redis` crate(redis-rs)的
> **同步路径** —— 因此完全不涉及任何异步运行时。

## 先看例子

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")
    client.set("greeting", "hello")
    let v: str = client.get("greeting")          # -> "hello"
    let n: i64 = client.delete("greeting")       # -> 1(被删除的键数)
    let present: bool = client.exists("greeting") # -> false
    print(v)
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
# hello
```

## 你能用到什么(Phase A 接口面)

- **`redis.connect(url)`** —— 打开到 Redis 服务器的连接。传一个规范的
  `redis://[:password@]host[:port][/db]` URL(数据库索引、密码、TLS 全都
  写在 URL 里 —— 没有一堆 `db=` / `decode_responses=` 关键字参数)。返回
  一个 `Client`。
- **`client.set(key, value)`** —— 把一个字符串值存到某个键下。这是个副
  作用(不返回值)。
- **`client.get(key)`** —— 把值作为 `str` 读回来。键不存在时读到的是空
  字符串 `""`。
- **`client.delete(key)`** —— 删除一个键。返回被删除的键数(`0` 或
  `1`)。
- **`client.exists(key)`** —— 键存在时返回 `true`。

这些方法名正是你在 Python `redis` 包里早已熟悉的那些(`set` / `get` /
`delete` / `exists`),所以无论是 LLM 还是你,都能第一次就写对。

## 你能用到什么(Phase B —— 过期、计数器、哈希)

基础键值动作之后你最常用到的几类缓存模式:

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")

    # 原子计数器 —— 自增并一步读回新值。
    client.set("hits", "10")
    let n: i64 = client.incr("hits")          # -> 11
    let m: i64 = client.incr_by("hits", 5)    # -> 16

    # 过期(TTL)—— 让某个键在 N 秒后自动消失。
    let ttl_set: bool = client.expire("hits", 60)  # -> true(已设置 TTL)

    # 哈希 —— 在一个键下存放具名字段。
    let created: bool = client.hset("user:1", "name", "ada")  # -> true(新字段)
    let name: str = client.hget("user:1", "name")             # -> "ada"
    print(name)
    return 0
```

- **`client.expire(key, seconds)`** —— 设置一个键的存活时间(TTL)。当键
  存在且超时被设置时返回 `true`,否则返回 `false`。
- **`client.incr(key)`** —— 原子地给计数器加 `1` 并返回新值。尚不存在的
  键从 `0` 起算,因此第一次 `incr` 返回 `1`。
- **`client.incr_by(key, n)`** —— 原子地加 `n` 并返回新值。
- **`client.hset(key, field, value)`** —— 在哈希里设置一个字段。字段是新
  增的返回 `true`,覆盖已有字段则返回 `false`。
- **`client.hget(key, field)`** —— 把哈希字段作为 `str` 读回来。字段(或
  哈希本身)不存在时读作空字符串 `""`,与 `get` 一致。

这些同样正是 redis-py 的方法名(`incr` / `expire` / `hset` / `hget`);
`incr_by` 是 `r.incr(key, n)` 的更可读拼写。

## 为什么这样设计?

- **类型化方法,而非命令字符串。** 你调用的是 `client.set(k, v)`,而不是
  `client.execute("SET k v")`。没有裸命令的逃生口,因此不存在命令注入或
  引号转义这类坑。
- **只有一种句柄类型。** 就是 `Client` —— 没有 Python 库里那种
  `Redis()` / `ConnectionPool()` / `StrictRedis()` 的混乱。
- **不用异常做控制流。** 键不存在就是空字符串;连接失败(没有服务器、
  URL 错误)时你拿到的是一个"未连接"的 client,它的读操作会安静地返回
  空 / `0` / `false` —— 既不崩溃,也没有你必须 catch 的异常。这正是
  Cobrust 其余部分采用的 `Result` 式错误纪律。
- **复用已验证的路径。** `redis.connect` 编译后落到与 `print`、
  `den.connect` 完全相同的那类 C-ABI 调用;运行期没有任何新东西。
- **连接自动清理。** `Client` 在离开作用域时正好被释放一次,这会关闭
  TCP 连接 —— 不需要手动 `close()`,不漏。
- **只链接你 import 的东西。** import 了 `redis` 的程序会链接
  `libredis.a`;没 import 的就不会。不臃肿。而且因为我们走同步路径,
  不会引入任何异步运行时(`tokio`)。

## 关于"干净失败"(fail-clean)行为的说明

如果服务器不可达或 URL 非法,`connect` 仍然会交给你一个可用的
`Client` —— 一个"未连接"的。对它的每个操作都返回安全的默认值(`get` /
`hget` → `""`,`delete` / `incr` / `incr_by` → `0`,`exists` / `expire` /
`hset` → `false`),`set` 则安静地变成空操作。你的程序在边界处永远不会
崩溃。正是这一点让测试套件无需真的跑一个 Redis 服务器,就能证明整条流水
线是通的。

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸的顶层语句是工具链另一块尚未完成
  的部分)。
- 值目前是字符串(`get_int` / `get_bytes` 是后续项)。
- 键不存在与键存着空字符串,目前都读作 `""`(一个能区分二者、返回
  `Option` 的 `get` 是已记录的后续项)。
- 让 `Client` 保持在函数局部;单线程 —— 不要把同一个连接跨 spawn 出去的
  任务共享(连接池是后续项)。
- 一步完成"设置并带过期"的 `SETEX`(`set_expiry`)是个小的后续项;现在
  请用 `set` 再 `expire`。

这些都是已记录在案的后续项,而非死路。

## 署名(Attribution)

Cobrust 的 `redis` 模块构建在采用 BSD-3-Clause 许可证的 `redis` crate
(redis-rs)之上。该许可证宽松且与 Cobrust 兼容;署名记录在
`crates/cobrust-redis/NOTICE` 中。
