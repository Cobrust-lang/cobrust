//! ADR-0093 end-to-end corpus for the first-class `bytes` runtime.
//!
//! `bytes` was a TYPE-SYSTEM-only type (codegen left `Ty::Bytes`
//! unmodeled — the `b"..."` literal routed through the lossy str-buffer
//! path, `len(bytes)` was rejected by the sized set, and a `bytes` local
//! had no drop symbol so it would leak). ADR-0093 mints the
//! `__cobrust_bytes_*` C-ABI family + the codegen so a `.cb` program can
//! bind, measure, index, and drop a `bytes` value.
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the
//! CPython-3 oracle (`b"abc"[0] == 97`, `len(b"abc") == 3`).
//!
//! Test families:
//!
//! - `bytes_e2e_01_len_and_index` — the §2.5 surface: `let b: bytes =
//!   b"abc"; print(len(b)); print(b[0])` → `3 / 97 / 98 / 99`.
//! - `bytes_e2e_02_non_utf8_roundtrip` — the dedicated-family payoff:
//!   `b"\xff\x00\xfe"` round-trips byte-exact (the old lossy str-buffer
//!   path corrupted a non-UTF-8 byte).
//! - `bytes_e2e_03_drop_hammer_loop` — 1000 iterations each minting +
//!   reading + dropping a fresh `bytes`; a double-free / leak would
//!   crash or diverge. Asserts the exact accumulator (drop-schedule
//!   correctness proof).
//! - `bytes_e2e_04_empty_bytes` — `b""` mints a valid empty buffer
//!   (`len == 0`); the empty-literal codegen path (null ptr + 0 len).
//! - `bytes_e2e_05_len_then_index_no_move` — `len(b)` BORROWS (does not
//!   consume) so a subsequent `b[i]` read is valid (the ADR-0093
//!   `len`-arg borrow-not-move discipline).
//!
//! Per `feedback_p9_clippy_stall_pattern.md`: module-level test-only
//! lint allow header.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::assertions_on_constants)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

struct TempPath {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for TempPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn write_cb(name: &str, contents: &str) -> TempPath {
    let dir = tempfile::tempdir().expect("create temp source dir");
    let path = dir.path().join(format!("{name}.cb"));
    std::fs::write(&path, contents).expect("write temp .cb");
    TempPath {
        _temp_dir: dir,
        path,
    }
}

struct BuiltExe {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for BuiltExe {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn run_build_exe(src: &Path) -> (i32, BuiltExe, String) {
    let bin = cobrust_binary();
    let exe_dir = tempfile::tempdir().expect("create temp exe dir");
    let exe = exe_dir.path().join(src.file_stem().unwrap());
    let out = Command::new(&bin)
        .arg("build")
        .arg(src)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (
        code,
        BuiltExe {
            _temp_dir: exe_dir,
            path: exe,
        },
        stderr,
    )
}

fn run_exe(exe: &Path) -> (i32, String, String) {
    let out = Command::new(exe).output().expect("spawn produced exe");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_build_run(name: &str, src: &str, expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "{name}: build failed; stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch (CPython-3 oracle)\nstderr={run_stderr}"
    );
}

// =====================================================================
// bytes_e2e_01 — the §2.5 surface: `len(b)` + `b[i]`.
//
// CPython 3 oracle: `len(b"abc") == 3`; `b"abc"[0] == 97` (an int, the
// byte value 0..255, NOT a 1-byte bytes).
// =====================================================================

#[test]
fn bytes_e2e_01_len_and_index() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"abc\"
    print(len(b))
    print(b[0])
    print(b[1])
    print(b[2])
    return 0
";
    assert_build_run("bytes_e2e_01", src, "3\n97\n98\n99\n");
}

// =====================================================================
// bytes_e2e_02 — the dedicated-family payoff: a NON-UTF-8 byte literal
// round-trips byte-exact. The old `Constant::Bytes` codegen routed
// through `materialize_str_buffer` under lossy UTF-8 and corrupted
// `\xff`; the `__cobrust_bytes_from_raw` mint preserves every byte.
//
// `b"\xff\x00\xfe"` → len 3, bytes 255 / 0 / 254.
// =====================================================================

#[test]
fn bytes_e2e_02_non_utf8_roundtrip() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"\\xff\\x00\\xfe\"
    print(len(b))
    print(b[0])
    print(b[1])
    print(b[2])
    return 0
";
    assert_build_run("bytes_e2e_02", src, "3\n255\n0\n254\n");
}

// =====================================================================
// bytes_e2e_03 — the DROP/UB hammer. 1000 iterations each MINT a fresh
// `bytes`, read `len` + index, and drop it at loop-body scope exit. A
// double-free or use-after-free would crash; a leak shows under a
// sanitizer run of the produced exe. Asserts the EXACT accumulator so a
// silently-wrong drop (e.g. a value freed early then re-read) diverges.
//
// `b"payload"` → len 7, `[0]` == 'p' == 112. Sum over 1000 iters of
// (7 + 112) == 119000.
// =====================================================================

#[test]
fn bytes_e2e_03_drop_hammer_loop() {
    let src = "\
fn main() -> i64:
    let i: i64 = 0
    let acc: i64 = 0
    while i < 1000:
        let b: bytes = b\"payload\"
        acc = acc + len(b)
        acc = acc + b[0]
        i = i + 1
    print(acc)
    return 0
";
    assert_build_run("bytes_e2e_03", src, "119000\n");
}

// =====================================================================
// bytes_e2e_04 — the empty-literal codegen path. `b""` has no interned
// rodata global; `materialize_bytes_buffer` passes a null ptr + 0 len
// and `__cobrust_bytes_from_raw` mints a valid EMPTY buffer (len 0).
// =====================================================================

#[test]
fn bytes_e2e_04_empty_bytes() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"\"
    print(len(b))
    return 0
";
    assert_build_run("bytes_e2e_04", src, "0\n");
}

// =====================================================================
// bytes_e2e_05 — `len(b)` BORROWS (does not consume) its `bytes` arg,
// so a subsequent `b[i]` read is valid. A `bytes` value is operand-Move
// (the Str-mirror); without the ADR-0093 `len`-arg borrow-not-move
// upgrade this program would fail to compile with `UseAfterMove`. This
// test is the compile-time proof the borrow discipline holds (it would
// not BUILD otherwise).
// =====================================================================

#[test]
fn bytes_e2e_05_len_then_index_no_move() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"hello\"
    let total: i64 = len(b)
    let first: i64 = b[0]
    let last: i64 = b[4]
    print(total)
    print(first)
    print(last)
    return 0
";
    // len 5; 'h' == 104; 'o' == 111.
    assert_build_run("bytes_e2e_05", src, "5\n104\n111\n");
}

// =====================================================================
// bytes_e2e_06 — ADR-0076c (D)-B-1b / ADR-0093 Phase 2 LANDED: the dora
// bytes accessor `event.data_bytes() -> bytes` (Arrow Binary/UInt8 →
// bytes) + `event.send_output_bytes(id, b)`. The raw-bytes sibling of
// the `data_buffer` / `send_output_buffer` pair (B-1a).
//
// A REAL dora node round-trip on the SYNTHETIC build (mirrors
// `dora_buffer_io_e2e`): the handler reads `event.data_bytes()` (the
// synthetic canned `b"\x00\xff\x01"`, a NON-UTF-8 payload), prints its
// `.hex()` (proving BYTE-FIDELITY end-to-end — `\xff` survives, the raw
// path is never UTF-8-lossy), and emits it back via
// `event.send_output_bytes("reply", b)` (the synthetic marker
// `output[reply]=bytes[len=3]`). The program exits 0 (the `bytes` it owns
// drops exactly once — no leak / double-free).
#[test]
fn bytes_e2e_06_dora_data_bytes_roundtrip() {
    let src = "\
import dora

@dora.node(inputs=[\"camera\"], outputs=[\"reply\"])
fn handler(event: dora.Event) -> i64:
    let b: bytes = event.data_bytes()
    print(b.hex())
    let _ = event.send_output_bytes(\"reply\", b)
    return 0

fn main() -> i64:
    let node = dora.Node(\"bytes_node\")
    let _ = node.run()
    return 0
";
    // hex of the canned non-UTF-8 `b"\x00\xff\x01"` is `00ff01` (a `\xff`
    // round-trips EXACTLY — the raw bytes path never UTF-8-corrupts it),
    // then the synthetic send_output_bytes capture marker.
    assert_build_run("bytes_e2e_06", src, "00ff01\noutput[reply]=bytes[len=3]\n");
}

// bytes_e2e_07 — B-1b REPAIR regression (the §2.2 drop-balance + §2.5
// use-after-move fix). The B-1b adversarial audit proved a LEAK: a `bytes`
// arg to `send_output_bytes` was operand-Move (NOT upgraded to Copy in the
// eco-value borrow path — `Ty::Bytes` was missing from
// `upgrade_move_to_copy_for_eco_value`), so the minted bytes got NO
// scope-exit `__cobrust_bytes_drop` (leak per call) AND a use-after-send
// (`send_output_bytes(id, b); b.hex()`) wrongly failed `cobrust build` with
// `use of moved value`. The one-line fix (add `Ty::Bytes` to the borrow
// predicate, lower.rs) makes the shim BORROW `b`, so it stays live: this
// node USES `b` AFTER the send (prints its `.hex()`), which MUST build+run.
// Drop-balance verified out-of-band by objdump on the terminal-send handler:
// exactly 1 `send_output_bytes` : 1 `__cobrust_bytes_drop` (was 1:0 = leak).
#[test]
fn bytes_e2e_07_use_after_send_output_bytes_no_move_no_leak() {
    let src = "\
import dora

@dora.node(inputs=[\"camera\"], outputs=[\"reply\"])
fn handler(event: dora.Event) -> i64:
    let b: bytes = event.data_bytes()
    let _ = event.send_output_bytes(\"reply\", b)
    print(b.hex())
    return 0

fn main() -> i64:
    let node = dora.Node(\"bytes_node\")
    let _ = node.run()
    return 0
";
    // The send marker prints during send_output_bytes, THEN `b.hex()` reads
    // the still-live (borrowed, not moved) `b` — proving no move + no leak.
    assert_build_run("bytes_e2e_07", src, "output[reply]=bytes[len=3]\n00ff01\n");
}
