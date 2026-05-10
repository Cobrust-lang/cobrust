//! Subprocess JSON bridge for `cobrust-tomli`.
//!
//! Reads TOML source from stdin, writes:
//! - `{"ok": <json>}` to stdout on success;
//! - `{"err": "<message>"}` to stdout on parse error;
//! - exit code 0 in both cases (the err case is a value-level error,
//!   not a process-level failure).
//!
//! The Python wrapper at `python/cobrust_tomli/__init__.py` calls this
//! binary to expose tomli's `loads()` / `load()` API to downstream
//! Python tooling without requiring a native PyO3 extension build.
//! 0.1.0-beta T1.1 chooses the subprocess bridge over PyO3 to keep the
//! release shippable on stock Rust toolchains; PyO3 lands at M-batch+
//! per ADR-0011.

use std::io::Read;

use cobrust_tomli::{loads, table_to_json};

fn main() {
    let mut src = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut src) {
        let payload = serde_json::json!({"err": format!("stdin read failed: {e}")});
        println!("{payload}");
        return;
    }
    match loads(&src) {
        Ok(table) => {
            let payload = serde_json::json!({"ok": table_to_json(&table)});
            println!("{payload}");
        }
        Err(e) => {
            let payload = serde_json::json!({"err": format!("{e}")});
            println!("{payload}");
        }
    }
}
