//! L3 differential gate for cobrust-scale.
//!
//! Pairs the translated Rust `pack` / `unpack` against CPython
//! `msgpack` (the L3 oracle) via subprocess. Pure-Rust subprocess
//! oracle — no PyO3 build step required for the default gate (per
//! ADR-0011 §5). The `--features pyo3` build path is exercised by
//! `tests/msgpack_pyo3_compiles.rs`.
//!
//! L3 dependents (per ADR-0010 §1) — redis-py + msgpack-numpy subsets —
//! are exercised at the workspace level via
//! `crates/cobrust-translator/tests/msgpack_pipeline.rs`.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::cast_lossless)]

use scale::{MsgValue, pack_to_vec, unpack};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const PYTHON: &str = "/opt/homebrew/bin/python3.11";

fn python_available() -> bool {
    Command::new(PYTHON)
        .arg("-c")
        .arg("import struct")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn corpus_oracle_pack(value_repr: &str) -> Result<Vec<u8>, String> {
    let here = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let upstream = PathBuf::from(here)
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .join("corpus/msgpack/upstream");
    let script = format!(
        "import sys, json\n\
         sys.path.insert(0, {upstream:?})\n\
         from msgpack_core import pack as _pack\n\
         val = json.loads(sys.stdin.read())\n\
         out = _pack(val)\n\
         sys.stdout.buffer.write(bytes(out))\n",
        upstream = upstream.to_str().unwrap_or(".")
    );
    let mut py = Command::new(PYTHON)
        .arg("-c")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    py.stdin
        .take()
        .expect("stdin")
        .write_all(value_repr.as_bytes())
        .expect("write stdin");
    let out = py.wait_with_output().map_err(|e| format!("wait: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("python rejected: {stderr}"));
    }
    Ok(out.stdout)
}

#[test]
fn l3_pack_bytes_match_oracle_for_simple_values() {
    if !python_available() {
        eprintln!("L3 msgpack gate: skipping — python3.11 not on PATH");
        return;
    }
    // (json input, equivalent MsgValue)
    let cases: Vec<(&str, MsgValue)> = vec![
        ("null", MsgValue::Nil),
        ("true", MsgValue::Bool(true)),
        ("false", MsgValue::Bool(false)),
        ("0", MsgValue::UInt(0)),
        ("66", MsgValue::UInt(66)),
        ("255", MsgValue::UInt(255)),
        ("65535", MsgValue::UInt(65535)),
        ("-1", MsgValue::Int(-1)),
        ("-128", MsgValue::Int(-128)),
        (r#""""#, MsgValue::Str(String::new())),
        (r#""hello world""#, MsgValue::Str("hello world".into())),
        ("[]", MsgValue::Array(vec![])),
        (
            "[1, 2, 3]",
            MsgValue::Array(vec![
                MsgValue::UInt(1),
                MsgValue::UInt(2),
                MsgValue::UInt(3),
            ]),
        ),
        ("{}", MsgValue::Map(vec![])),
        (
            r#"{"x": 1, "y": 2}"#,
            MsgValue::Map(vec![
                ("x".into(), MsgValue::UInt(1)),
                ("y".into(), MsgValue::UInt(2)),
            ]),
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (json, value) in &cases {
        let oracle = match corpus_oracle_pack(json) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("oracle failed for {json}: {e}"));
                continue;
            }
        };
        let ours = pack_to_vec(value).expect("pack");
        if oracle != ours {
            failures.push(format!(
                "diverged for {json}: oracle={} ours={}",
                hex_of(&oracle),
                hex_of(&ours),
            ));
        }
    }
    if !failures.is_empty() {
        for f in &failures {
            eprintln!("{f}");
        }
        panic!("{} pack case(s) diverged", failures.len());
    }
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[test]
fn l3_round_trip_pack_unpack() {
    let cases: Vec<MsgValue> = vec![
        MsgValue::Nil,
        MsgValue::Bool(true),
        MsgValue::Bool(false),
        MsgValue::UInt(0),
        MsgValue::UInt(66),
        MsgValue::Int(-1),
        MsgValue::Float(std::f64::consts::PI),
        MsgValue::Str("hello".into()),
        MsgValue::Bin(b"raw".to_vec()),
        MsgValue::Array(vec![MsgValue::UInt(1), MsgValue::UInt(2)]),
        MsgValue::Map(vec![("k".into(), MsgValue::UInt(1))]),
    ];
    for value in cases {
        let bytes = pack_to_vec(&value).expect("pack");
        let back = unpack(&bytes).expect("unpack");
        assert_eq!(back, value, "round-trip diverged");
    }
}

#[test]
fn l3_pyo3_wrapper_directory_layout() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(crate_dir.join("python/scale_init.py").exists());
    assert!(crate_dir.join("python/setup.py").exists());
    assert!(crate_dir.join("PROVENANCE.toml").exists());
}

#[test]
fn l3_unknown_marker_is_rejected() {
    // 0xc1 is reserved per the msgpack spec — the unpacker must reject.
    let err = unpack(&[0xc1]);
    assert!(err.is_err());
}

#[test]
fn l3_trailing_bytes_are_rejected() {
    let mut bytes = pack_to_vec(&MsgValue::UInt(42)).expect("pack");
    bytes.push(0x00);
    let err = unpack(&bytes);
    assert!(err.is_err());
}
