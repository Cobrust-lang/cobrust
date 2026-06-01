# `import redis` —— 在 Cobrust 中使用 Redis 缓存 / 键值存储

> 状态:ADR-0078 Phase-1c/1d。这是第十一个可以在 `.cb` 程序里 `import`
> 并真正端到端调用(编译 → 链接 → 运行)的生态库。它把 `redis`(Cobrust
> 版的缓存 / KV 客户端,`redis-py` 的更名版)接到了编译器的 intrinsic /
> C-ABI / 静态链接链路上,底层用的是 Rust `redis` crate(redis-rs)的
> **同步路径** —— 因此完全不涉及任何异步运行时。Phase 1d 补上了一次性读
> 回*整个*集合的几个动作(`lrange` / `smembers` / `hkeys` / `hgetall`),
> 它们返回一个 `list[str]`。

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

## 你能用到什么(Phase C —— 列表与集合)

Redis 列表(一个双端队列)与集合(成员唯一):

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")

    # 列表 —— 两端都能 push / pop。
    let n1: i64 = client.lpush("tasks", "a")   # -> 1(新长度;在头部前插)
    let n2: i64 = client.rpush("tasks", "b")   # -> 2(在尾部追加)
    let count: i64 = client.llen("tasks")      # -> 2
    let head: str = client.lpop("tasks")       # -> "a"
    let tail: str = client.rpop("tasks")       # -> "b"

    # 集合 —— 成员唯一,成员判定很快。
    let added: i64 = client.sadd("tags", "x")          # -> 1(已存在则 0)
    let present: bool = client.sismember("tags", "x")  # -> true
    let card: i64 = client.scard("tags")               # -> 1
    let removed: i64 = client.srem("tags", "x")        # -> 1(不存在则 0)
    print(head)
    return 0
```

- **`client.lpush(key, value)`** —— 在列表头部前插一个值。返回列表的新
  长度。
- **`client.rpush(key, value)`** —— 在列表尾部追加一个值。返回列表的新
  长度。
- **`client.lpop(key)`** —— 从头部弹出一个元素,作为 `str` 返回。空列表
  或键不存在时读作空字符串 `""`。
- **`client.rpop(key)`** —— 从尾部弹出一个元素。同样的 `""` 规则。
- **`client.llen(key)`** —— 列表里的元素个数(键不存在则为 `0`)。
- **`client.sadd(key, member)`** —— 往集合里加一个成员。返回新增的个
  数:成员是新的返回 `1`,已存在返回 `0`。
- **`client.srem(key, member)`** —— 移除一个成员。返回移除的个数(`1`
  或 `0`)。
- **`client.sismember(key, member)`** —— 成员在集合中时返回 `true`。
- **`client.scard(key)`** —— 集合里的成员个数(键不存在则为 `0`)。

这些同样正是 redis-py 的方法名(`lpush` / `rpush` / `lpop` / `rpop` /
`llen` / `sadd` / `srem` / `sismember` / `scard`)。

## 你能用到什么(Phase 1d —— 一次性读回整个列表 / 集合 / 哈希)

一次性读回*整个*集合的几个动作。它们都返回一个 `list[str]`(字符串列
表)—— 因此你可以用 `for` 循环遍历它、用下标取值(`xs[0]`)、问它的长度
(`xs.len()`),和任何其他 Cobrust 列表一样。

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")

    client.rpush("tasks", "a")
    client.rpush("tasks", "b")
    client.rpush("tasks", "c")

    # 把整个列表读回来。start=0、stop=-1 表示"全部"。
    let xs: list[str] = client.lrange("tasks", 0, -1)   # -> ["a", "b", "c"]
    print(xs.len())                                      # -> 3
    for task in xs:
        print(task)                                      # -> a / b / c

    # 一个集合的全部成员。
    client.sadd("tags", "x")
    let tags: list[str] = client.smembers("tags")        # -> ["x"]

    # 一个哈希的全部字段名。
    client.hset("user:1", "name", "ada")
    let fields: list[str] = client.hkeys("user:1")       # -> ["name"]

    # 一个哈希的全部字段/值对 —— 见下面的说明。
    let pairs: list[str] = client.hgetall("user:1")      # -> ["name", "ada"]
    return 0
```

- **`client.lrange(key, start, stop)`** —— 列表里 `start..stop` 下标区间
  内的元素(两端都含;负数下标从尾部倒数,所以 `0, -1` 就是整个列表 ——
  正是 redis 自己的规则)。键不存在时给出空列表 `[]`。
- **`client.smembers(key)`** —— 一个集合的全部成员,作为 `list[str]`
  返回(redis 集合无序)。键不存在时给出 `[]`。
- **`client.hkeys(key)`** —— 一个哈希的全部字段名,作为 `list[str]`
  返回。键不存在时给出 `[]`。
- **`client.hgetall(key)`** —— 一个哈希的全部字段/值对。

这些还是 redis-py 的方法名(`lrange` / `smembers` / `hkeys` /
`hgetall`)。

### `hgetall` 返回的是扁平列表,而不是 dict

Python 的 `redis` 把 `hgetall` 返回成一个 `dict`。Cobrust 把它返回成一个
**扁平的** `list[str]` —— `[字段1, 值1, 字段2, 值2, ...]` —— 所以上面的
例子给出 `["name", "ada"]`。请两两一组地读(先字段、后它的值)。这是一
处有意为之、且已记录在案的差异,和 `coil` 的 `buffer.shape` 是同一种差异
(numpy 返回元组,`coil` 返回 `list[i64]`):扁平列表是当前已经在用的列
表机制能干净支持的诚实形态,无需为此发明一种跨边界返回 dict 的新形态。
一个返回 `dict` 的 `hgetall` 是已记录的后续项。

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
`hget` / `lpop` / `rpop` → `""`,`delete` / `incr` / `incr_by` /
`lpush` / `rpush` / `llen` / `sadd` / `srem` / `scard` → `0`,`exists` /
`expire` / `hset` / `sismember` → `false`,而一次性读回整个集合的
`lrange` / `smembers` / `hkeys` / `hgetall` → 空列表 `[]`),`set` 则安静
地变成空操作。你的程序在边界处永远不会崩溃。正是这一点让测试套件无需真
的跑一个 Redis 服务器,就能证明整条流水线是通的 —— 包括对一个返回列表的
`for` 循环。

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
- `hgetall` 返回的是扁平的 `list[str]`(`[字段, 值, ...]`),而不是
  `dict`;一个返回 `dict` 的 `hgetall` 是已记录的后续项。

这些都是已记录在案的后续项,而非死路。

## 署名(Attribution)

Cobrust 的 `redis` 模块构建在采用 BSD-3-Clause 许可证的 `redis` crate
(redis-rs)之上。该许可证宽松且与 Cobrust 兼容;署名记录在
`crates/cobrust-redis/NOTICE` 中。
