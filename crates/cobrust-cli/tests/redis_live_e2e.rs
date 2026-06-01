//! ADR-0078 Phase-1c — the LIVE round-trip `.cb` e2e for the `redis`
//! ecosystem-import wiring. Self-SKIPS when no redis server is reachable
//! (CI has none), mirroring the cross-target / python-version self-skip
//! pattern (a runtime probe + a clean `eprintln!` + `return`, since Rust
//! has no first-class runtime `#[ignore]`).
//!
//! When a redis IS reachable (local dev, or a CI redis service-container
//! reachable at `$REDIS_URL` or `127.0.0.1:6379`), this runs the full
//! `set "greeting" "hello"` → `get` (prints `hello`) → `delete` (prints
//! `1`) → `exists` (prints `False`) round-trip and asserts the printed
//! values — the ADR-0078 Phase-1c done-means #2. A second live test
//! (Phase-B) exercises `set`+`expire`(+post-`exists`), the `incr`/
//! `incr_by` counter round-trip, and the `hset`/`hget` hash round-trip. A
//! third live test (Phase-C) exercises the list round-trip (`lpush`/
//! `rpush`/`llen`/`lpop`/`rpop`) and the set round-trip (`sadd`/`srem`/
//! `sismember`/`scard`).
//!
//! The hermetic, always-on proof of the wiring + the error path is
//! `redis_fail_clean_e2e.rs` (server-less); this file adds the
//! best-effort live confirmation on top.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

/// The redis URL the live test targets. `$REDIS_URL` overrides the
/// `127.0.0.1:6379` loopback default (so a CI service-container can point
/// the test at its address). Loopback `127.0.0.1` is not a real host.
fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string())
}

/// Extract `host:port` from a `redis://host:port/...` URL for the TCP
/// reachability probe. Best-effort; defaults the port to 6379.
fn host_port(url: &str) -> String {
    let rest = url
        .strip_prefix("redis://")
        .or_else(|| url.strip_prefix("rediss://"))
        .unwrap_or(url);
    // Drop any `user:pass@` credentials prefix.
    let rest = rest.rsplit('@').next().unwrap_or(rest);
    // Drop the `/db` path + any query.
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:6379")
    }
}

/// Probe whether a redis server is reachable at `host:port` within a
/// short timeout. Returns `false` (→ self-skip) on any resolution /
/// connect failure — the no-server CI path.
fn redis_reachable(host_port: &str) -> bool {
    let Ok(mut addrs) = host_port.to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

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

/// ADR-0078 Phase-1c done-means #2 — the live KV round-trip. Self-skips
/// (clean `return`) when no redis is reachable, so CI (no server) is
/// green; runs the full assert when a server is present.
///
/// The `.cb` program uses a process-unique key so a shared dev/CI redis
/// is not polluted across concurrent runs, and `delete`s it at the end.
#[test]
fn test_e2e_redis_live_round_trip_or_skip() {
    let url = redis_url();
    let hp = host_port(&url);
    if !redis_reachable(&hp) {
        eprintln!(
            "redis_live_e2e: skipping cleanly: no redis server reachable at {hp} \
             (set REDIS_URL to a reachable redis://host:port/ to run the live round-trip)"
        );
        return;
    }

    // Process-unique key so concurrent runs / a shared server don't
    // collide. (std::process::id is stable for the test process.)
    let key = format!("cobrust:redis_live_e2e:{}", std::process::id());

    let source = format!(
        concat!(
            "import redis\n",
            "\n",
            "fn main() -> i64:\n",
            "    let client = redis.connect(\"{url}\")\n",
            "    client.set(\"{key}\", \"hello\")\n",
            "    let v: str = client.get(\"{key}\")\n",
            "    let n: i64 = client.delete(\"{key}\")\n",
            "    let present: bool = client.exists(\"{key}\")\n",
            "    print(v)\n",
            "    print(n)\n",
            "    print(present)\n",
            "    return 0\n",
        ),
        url = url,
        key = key,
    );

    let stdout = build_and_run_source(&source);
    // get prints the stored value; delete removes exactly 1 key; exists
    // is then False (the key was just deleted).
    assert_eq!(
        stdout, "hello\n1\nFalse\n",
        "live redis round-trip: set/get/delete/exists"
    );
}

/// ADR-0078 Phase-1c Phase-B — the LIVE round-trip for the new
/// cache/counter/hash verbs. Self-skips (clean `return`) when no redis is
/// reachable. Three independent round-trips, each on a process-unique key
/// (cleaned up with `delete` at the end so a shared dev/CI redis is not
/// polluted):
///
/// 1. counter: `set k "10"` → `incr k` (prints `11`) → `incr_by k 5`
///    (prints `16`).
/// 2. expire: `set k2 "v"` → `expire k2 100` (prints `True`) → `exists k2`
///    (prints `True` — still present, well within the TTL; the actual
///    TTL-expiry timing is deliberately NOT asserted to avoid a slow/flaky
///    sleep, per ADR-0078 §Phase-B heaviest-risk note).
/// 3. hash: `hset h f "a"` (prints `True` — new field) → `hget h f`
///    (prints `a`) → `hset h f "b"` (prints `False` — overwrite) →
///    `hget h f` (prints `b`).
#[test]
fn test_e2e_redis_live_phase_b_round_trip_or_skip() {
    let url = redis_url();
    let hp = host_port(&url);
    if !redis_reachable(&hp) {
        eprintln!(
            "redis_live_e2e (phase-b): skipping cleanly: no redis server reachable at {hp} \
             (set REDIS_URL to a reachable redis://host:port/ to run the live round-trip)"
        );
        return;
    }

    // Process-unique key roots so concurrent runs / a shared server don't
    // collide across the three round-trips.
    let pid = std::process::id();
    let kc = format!("cobrust:redis_live_e2e:phaseb:counter:{pid}");
    let ke = format!("cobrust:redis_live_e2e:phaseb:expire:{pid}");
    let kh = format!("cobrust:redis_live_e2e:phaseb:hash:{pid}");

    let source = format!(
        concat!(
            "import redis\n",
            "\n",
            "fn main() -> i64:\n",
            "    let client = redis.connect(\"{url}\")\n",
            // 1. counter round-trip: seed 10, +1 -> 11, +5 -> 16.
            "    client.set(\"{kc}\", \"10\")\n",
            "    let c1: i64 = client.incr(\"{kc}\")\n",
            "    let c2: i64 = client.incr_by(\"{kc}\", 5)\n",
            // 2. expire round-trip: set, set TTL (True), still-present (True).
            "    client.set(\"{ke}\", \"v\")\n",
            "    let ttl_set: bool = client.expire(\"{ke}\", 100)\n",
            "    let still_present: bool = client.exists(\"{ke}\")\n",
            // 3. hash round-trip: new field (True), read, overwrite (False), read.
            "    let new_field: bool = client.hset(\"{kh}\", \"f\", \"a\")\n",
            "    let h1: str = client.hget(\"{kh}\", \"f\")\n",
            "    let overwrite: bool = client.hset(\"{kh}\", \"f\", \"b\")\n",
            "    let h2: str = client.hget(\"{kh}\", \"f\")\n",
            // Clean up the three keys (deletes are not asserted on).
            "    let _c: i64 = client.delete(\"{kc}\")\n",
            "    let _e: i64 = client.delete(\"{ke}\")\n",
            "    let _h: i64 = client.delete(\"{kh}\")\n",
            "    print(c1)\n",
            "    print(c2)\n",
            "    print(ttl_set)\n",
            "    print(still_present)\n",
            "    print(new_field)\n",
            "    print(h1)\n",
            "    print(overwrite)\n",
            "    print(h2)\n",
            "    return 0\n",
        ),
        url = url,
        kc = kc,
        ke = ke,
        kh = kh,
    );

    let stdout = build_and_run_source(&source);
    assert_eq!(
        stdout, "11\n16\nTrue\nTrue\nTrue\na\nFalse\nb\n",
        "live redis phase-b round-trip: incr/incr_by, expire+exists, hset/hget"
    );
}

/// ADR-0078 Phase-1c Phase-C — the LIVE round-trip for the new list/set
/// verbs. Self-skips (clean `return`) when no redis is reachable. Two
/// independent round-trips, each on a process-unique key (cleaned up with
/// `delete` at the end so a shared dev/CI redis is not polluted):
///
/// 1. list: `lpush l "a"` (prepend, len -> 1) → `rpush l "b"` (append,
///    len -> 2) → `llen l` (-> 2) → `lpop l` (pop head -> "a") →
///    `rpop l` (pop tail -> "b") → `llen l` (-> 0, now empty).
/// 2. set: `sadd s "x"` (new -> 1) → `sadd s "x"` (already there -> 0) →
///    `sismember s "x"` (-> True) → `scard s` (-> 1) → `srem s "x"`
///    (removed -> 1) → `sismember s "x"` (-> False, now gone).
#[test]
fn test_e2e_redis_live_phase_c_round_trip_or_skip() {
    let url = redis_url();
    let hp = host_port(&url);
    if !redis_reachable(&hp) {
        eprintln!(
            "redis_live_e2e (phase-c): skipping cleanly: no redis server reachable at {hp} \
             (set REDIS_URL to a reachable redis://host:port/ to run the live round-trip)"
        );
        return;
    }

    // Process-unique key roots so concurrent runs / a shared server don't
    // collide across the two round-trips.
    let pid = std::process::id();
    let kl = format!("cobrust:redis_live_e2e:phasec:list:{pid}");
    let ks = format!("cobrust:redis_live_e2e:phasec:set:{pid}");

    let source = format!(
        concat!(
            "import redis\n",
            "\n",
            "fn main() -> i64:\n",
            "    let client = redis.connect(\"{url}\")\n",
            // 1. list round-trip: lpush "a" (len 1), rpush "b" (len 2),
            //    llen 2, lpop "a" (head), rpop "b" (tail), llen 0.
            "    let len1: i64 = client.lpush(\"{kl}\", \"a\")\n",
            "    let len2: i64 = client.rpush(\"{kl}\", \"b\")\n",
            "    let len_full: i64 = client.llen(\"{kl}\")\n",
            "    let head: str = client.lpop(\"{kl}\")\n",
            "    let tail: str = client.rpop(\"{kl}\")\n",
            "    let len_empty: i64 = client.llen(\"{kl}\")\n",
            // 2. set round-trip: sadd "x" (new 1), sadd "x" (dup 0),
            //    sismember "x" (True), scard 1, srem "x" (1), sismember False.
            "    let added: i64 = client.sadd(\"{ks}\", \"x\")\n",
            "    let dup: i64 = client.sadd(\"{ks}\", \"x\")\n",
            "    let is_member: bool = client.sismember(\"{ks}\", \"x\")\n",
            "    let card: i64 = client.scard(\"{ks}\")\n",
            "    let removed: i64 = client.srem(\"{ks}\", \"x\")\n",
            "    let still_member: bool = client.sismember(\"{ks}\", \"x\")\n",
            // Clean up the two keys (deletes are not asserted on).
            "    let _l: i64 = client.delete(\"{kl}\")\n",
            "    let _s: i64 = client.delete(\"{ks}\")\n",
            "    print(len1)\n",
            "    print(len2)\n",
            "    print(len_full)\n",
            "    print(head)\n",
            "    print(tail)\n",
            "    print(len_empty)\n",
            "    print(added)\n",
            "    print(dup)\n",
            "    print(is_member)\n",
            "    print(card)\n",
            "    print(removed)\n",
            "    print(still_member)\n",
            "    return 0\n",
        ),
        url = url,
        kl = kl,
        ks = ks,
    );

    let stdout = build_and_run_source(&source);
    assert_eq!(
        stdout, "1\n2\n2\na\nb\n0\n1\n0\nTrue\n1\n1\nFalse\n",
        "live redis phase-c round-trip: lpush/rpush/llen/lpop/rpop, sadd/srem/sismember/scard"
    );
}
