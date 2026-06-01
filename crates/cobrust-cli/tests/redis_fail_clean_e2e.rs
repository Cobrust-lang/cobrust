//! ADR-0078 Phase-1c — end-to-end `.cb` source → compile → link → run
//! for the `redis` ecosystem-import wiring (cache/KV, rebrand of
//! redis-py), the **always-on, server-LESS** fail-clean proof.
//!
//! Twin of `ecosystem_den_e2e.rs` / `ecosystem_strike_e2e.rs`. Pairs the
//! handle pattern (Client, a `den.Connection`-shaped stateful resource)
//! with a free-function entrypoint (`redis.connect`, like `den.connect`).
//!
//! ```text
//! `import redis` + `redis.connect(url)` + `client.set/get/delete/exists`
//!   + the Phase-B `client.expire/incr/incr_by/hset/hget` verbs
//!   + the Phase-C `client.lpush/rpush/lpop/rpop/llen` (lists)
//!     + `client.sadd/srem/sismember/scard` (sets) verbs
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_redis_* Constant::Str)
//!   → cobrust-codegen externs + Client handle drop schedule
//!   → cobrust-redis C-ABI shims (libredis.a)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → stdout
//! ```
//!
//! # Why server-less is the PRIMARY e2e (ADR-0078 §3.6 option-2)
//!
//! CI has no Redis server (and, unlike strike — which spins its own
//! `pit::App` loopback server — there is no in-process Rust redis server
//! the workspace can start). So the always-on e2e connects to a
//! **definitely-absent** redis (`redis://127.0.0.1:1/` — port 1 has
//! nothing listening): the `connect` fails clean into a disconnected
//! sentinel `Client`, and every subsequent verb returns its per-type
//! sentinel (empty-str / `0` / `false`). This exercises the FULL
//! compile→link→run vertical slice + the no-panic-at-C-ABI guarantee
//! WITHOUT a server, so it is GREEN in CI every run. The live round-trip
//! (set→get→delete→exists) lives in `redis_live_e2e.rs`, which self-skips
//! when no server is reachable.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::process::Command;

/// Compile + link + run a `.cb` source, returning its stdout. Asserts
/// the build and the run both succeed.
fn build_and_run_source(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).current_dir(dir.path()).output().unwrap();
    assert!(
        run.status.success(),
        "run failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// ADR-0078 Phase-1c done-means #1 — the server-LESS fail-clean vertical
/// slice. `redis.connect` to an unreachable port yields the disconnected
/// sentinel; `get` prints the empty-string sentinel (blank line),
/// `delete` prints `0`, `exists` prints `False`. This proves the full
/// `compile -> link -> run` chain plus the no-panic-at-C-ABI guarantee
/// with NO redis server, so it stays ALWAYS green in CI. `set` is also
/// exercised (a silent no-op on the sentinel; the program must not crash
/// on it).
#[test]
fn test_e2e_redis_unreachable_server_yields_fail_clean_sentinels() {
    let stdout = build_and_run_source(concat!(
        "import redis\n",
        "\n",
        "fn main() -> i64:\n",
        // Port 1 has nothing listening → connect fails clean → the
        // disconnected sentinel Client (never null, never a panic).
        "    let client = redis.connect(\"redis://127.0.0.1:1/\")\n",
        "    client.set(\"greeting\", \"hello\")\n",
        "    let v: str = client.get(\"greeting\")\n",
        "    let n: i64 = client.delete(\"greeting\")\n",
        "    let present: bool = client.exists(\"greeting\")\n",
        "    print(v)\n",
        "    print(n)\n",
        "    print(present)\n",
        "    return 0\n",
    ));
    // empty-str sentinel (blank line) + 0 keys removed + not present.
    assert_eq!(stdout, "\n0\nFalse\n");
}

/// ADR-0078 Phase-1c Phase-B — the server-LESS fail-clean slice for the
/// new cache/counter/hash verbs. Connecting to an unreachable port yields
/// the disconnected sentinel; `expire` prints `False` (TTL not set),
/// `incr` / `incr_by` print `0` (no atomic increment on a dead
/// connection), `hset` prints `False` (no new field created), `hget`
/// prints the empty-string sentinel (blank line). This proves the new
/// shims' FULL `compile -> link -> run` chain + the no-panic-at-C-ABI
/// guarantee with NO redis server — ALWAYS green in CI. This is the
/// always-on proof that genuinely exercises the Phase-B error paths.
#[test]
fn test_e2e_redis_phase_b_unreachable_server_yields_fail_clean_sentinels() {
    let stdout = build_and_run_source(concat!(
        "import redis\n",
        "\n",
        "fn main() -> i64:\n",
        // Port 1 has nothing listening → connect fails clean → the
        // disconnected sentinel Client (never null, never a panic).
        "    let client = redis.connect(\"redis://127.0.0.1:1/\")\n",
        // expire on the dead connection → False (TTL not set).
        "    let ttl_set: bool = client.expire(\"counter\", 60)\n",
        // incr / incr_by → 0 sentinel (no atomic increment).
        "    let n1: i64 = client.incr(\"counter\")\n",
        "    let n2: i64 = client.incr_by(\"counter\", 5)\n",
        // hset → False (no new field created); hget → "" sentinel.
        "    let created: bool = client.hset(\"h\", \"field\", \"value\")\n",
        "    let hv: str = client.hget(\"h\", \"field\")\n",
        "    print(ttl_set)\n",
        "    print(n1)\n",
        "    print(n2)\n",
        "    print(created)\n",
        "    print(hv)\n",
        "    return 0\n",
    ));
    // expire False + incr 0 + incr_by 0 + hset False + hget "" (blank).
    assert_eq!(stdout, "False\n0\n0\nFalse\n\n");
}

/// ADR-0078 Phase-1c Phase-C — the server-LESS fail-clean slice for the
/// new list/set verbs. Connecting to an unreachable port yields the
/// disconnected sentinel; `lpush`/`rpush` print `0` (no element pushed),
/// `lpop`/`rpop` print the empty-string sentinel (blank line), `llen`
/// prints `0`, `sadd`/`srem` print `0` (no member added/removed),
/// `sismember` prints `False`, `scard` prints `0`. This proves the new
/// shims' FULL `compile -> link -> run` chain + the no-panic-at-C-ABI
/// guarantee with NO redis server — ALWAYS green in CI. This is the
/// always-on proof that genuinely exercises the Phase-C error paths.
#[test]
fn test_e2e_redis_phase_c_unreachable_server_yields_fail_clean_sentinels() {
    let stdout = build_and_run_source(concat!(
        "import redis\n",
        "\n",
        "fn main() -> i64:\n",
        // Port 1 has nothing listening → connect fails clean → the
        // disconnected sentinel Client (never null, never a panic).
        "    let client = redis.connect(\"redis://127.0.0.1:1/\")\n",
        // lpush / rpush on the dead connection → 0 (no push).
        "    let l1: i64 = client.lpush(\"mylist\", \"a\")\n",
        "    let l2: i64 = client.rpush(\"mylist\", \"b\")\n",
        // lpop / rpop → "" sentinel (empty/absent list).
        "    let p1: str = client.lpop(\"mylist\")\n",
        "    let p2: str = client.rpop(\"mylist\")\n",
        // llen → 0 (absent list).
        "    let n: i64 = client.llen(\"mylist\")\n",
        // sadd / srem → 0 (no member added/removed).
        "    let s1: i64 = client.sadd(\"myset\", \"x\")\n",
        "    let s2: i64 = client.srem(\"myset\", \"x\")\n",
        // sismember → False; scard → 0.
        "    let member: bool = client.sismember(\"myset\", \"x\")\n",
        "    let card: i64 = client.scard(\"myset\")\n",
        "    print(l1)\n",
        "    print(l2)\n",
        "    print(p1)\n",
        "    print(p2)\n",
        "    print(n)\n",
        "    print(s1)\n",
        "    print(s2)\n",
        "    print(member)\n",
        "    print(card)\n",
        "    return 0\n",
    ));
    // lpush 0 + rpush 0 + lpop "" + rpop "" + llen 0 + sadd 0 + srem 0 +
    // sismember False + scard 0.
    assert_eq!(stdout, "0\n0\n\n\n0\n0\n0\nFalse\n0\n");
}

/// A second fail-clean shape — connect with a bare unparseable URL (the
/// invalid-URL branch of the fail-clean path, distinct from the
/// unreachable-port branch). Must ALSO yield the non-null disconnected
/// sentinel and the same per-type sentinels, never a panic.
#[test]
fn test_e2e_redis_invalid_url_yields_fail_clean_sentinels() {
    let stdout = build_and_run_source(concat!(
        "import redis\n",
        "\n",
        "fn main() -> i64:\n",
        // A bare non-URL string is rejected by redis-rs's URL parser
        // before any I/O → the InvalidUrl fail-clean path.
        "    let client = redis.connect(\"not-a-redis-url\")\n",
        "    let v: str = client.get(\"k\")\n",
        "    let present: bool = client.exists(\"k\")\n",
        "    print(v)\n",
        "    print(present)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "\nFalse\n");
}
