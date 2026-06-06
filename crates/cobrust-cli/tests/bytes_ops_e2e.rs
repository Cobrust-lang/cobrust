//! ADR-0093 Phase 2 end-to-end corpus for the `bytes` byte-buffer
//! surface: slice / concat / encode / decode / hex.
//!
//! Phase 1 (`bytes_primitive_e2e.rs`) minted `bytes` as a runtime value
//! (`b"..."` literal, `len(b)`, `b[i]`, exactly-once drop). Phase 2 lands
//! the byte-buffer OPERATIONS, each of which MINTS a fresh heap value the
//! `.cb` scope owns + drops EXACTLY ONCE while BORROWING (never freeing)
//! its inputs:
//!
//! - `b[lo:hi]` slice → a fresh `bytes` (`__cobrust_bytes_slice`, the
//!   coil-buffer-slice mirror, with Python clamp on OOB).
//! - `b1 + b2` concat → a fresh `bytes` (`__cobrust_bytes_concat`, the
//!   `__cobrust_str_concat` mirror).
//! - `s.encode()` → a fresh `bytes` (`__cobrust_bytes_from_str`, UTF-8).
//! - `b.decode()` → a fresh `str` (`__cobrust_bytes_decode`, UTF-8;
//!   **invalid UTF-8 TRAPS — §2.2, never lossy / replacement-char**).
//! - `b.hex()` → a fresh `str` (`__cobrust_bytes_hex`, lowercase hex).
//!
//! Each test writes a `.cb` program, invokes `cobrust build`, runs the
//! produced executable, and asserts stdout byte-identical to the
//! CPython-3 oracle (e.g. `b"hello"[1:4] == b"ell"`, `b"hello".hex() ==
//! "68656c6c6f"`).
//!
//! Test families:
//!
//! - `bytes_ops_e2e_01_slice` — `b[lo:hi]` basic + Python clamp on OOB.
//! - `bytes_ops_e2e_02_concat` — `b1 + b2` byte-exact concatenation.
//! - `bytes_ops_e2e_03_encode_decode_roundtrip` — the LOAD-BEARING
//!   round-trip: `s.encode().decode() == s` (incl. multi-byte UTF-8).
//! - `bytes_ops_e2e_04_decode_invalid_utf8_traps` — the §2.2 design
//!   point: decoding `b"\xff\xfe"` TRAPS (non-zero exit + stderr
//!   diagnostic), NEVER silently lossy-replaces.
//! - `bytes_ops_e2e_05_hex` — `b.hex()` lowercase hex (incl. non-UTF-8
//!   bytes the old lossy str path would corrupt).
//! - `bytes_ops_e2e_06_drop_hammer_loop` — 1000 iterations each minting a
//!   fresh slice + concat (bytes) + decode + hex (str); a double-free /
//!   leak would crash or diverge. Asserts the exact accumulator.
//! - `bytes_ops_e2e_07_inputs_borrowed_not_consumed` — every op BORROWS
//!   its input(s); a source `bytes` survives a slice/concat/decode/hex
//!   and is still usable afterward (drops once at scope exit).
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

/// Assert a `.cb` program is REJECTED at compile time (non-zero build exit)
/// and the diagnostic on stderr CONTAINS `needle` (the §2.5-B fix-printing
/// substring). The build must NOT produce a runnable executable — this is
/// the §2.5-A compile-time-catch / §5.1 no-panic guard: a non-zero exit
/// proves a Cobrust DIAGNOSTIC, not a raw inkwell/codegen panic and not a
/// silent exit-0 miscompile.
fn assert_build_rejects(name: &str, src: &str, needle: &str) {
    let path = write_cb(name, src);
    let (build_code, _exe, build_stderr) = run_build_exe(&path);
    assert_ne!(
        build_code, 0,
        "{name}: build must REJECT (non-zero exit), got 0; \
         stderr=\n{build_stderr}\n--- source ---\n{src}"
    );
    assert!(
        build_stderr.contains(needle),
        "{name}: reject diagnostic must contain {needle:?}; \
         got stderr=\n{build_stderr}"
    );
}

// =====================================================================
// bytes_ops_e2e_01 — `b[lo:hi]` slice. CPython 3 oracle:
//   b"hello"[1:4] == b"ell"  (len 3; [0] == 101 'e')
//   b"hello"[1:99] == b"ello"  (Python clamps hi to len, NOT an abort)
//   b"hello"[3:1] == b""       (hi < lo → empty)
// The slice mints a FRESH bytes the `.cb` scope drops once.
// =====================================================================

#[test]
fn bytes_ops_e2e_01_slice() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"hello\"
    let s: bytes = b[1:4]
    print(len(s))
    print(s[0])
    print(s[2])
    let clamped: bytes = b[1:99]
    print(len(clamped))
    let empty: bytes = b[3:1]
    print(len(empty))
    return 0
";
    // len(b"ell")=3, [0]=101('e'), [2]=108('l'); clamp len 4; empty len 0.
    assert_build_run("bytes_ops_e2e_01", src, "3\n101\n108\n4\n0\n");
}

// =====================================================================
// bytes_ops_e2e_02 — `b1 + b2` concat. CPython 3 oracle:
//   b"ab" + b"cd" == b"abcd"  (len 4; [0]==97 'a', [3]==100 'd')
// The concat mints a FRESH bytes the `.cb` scope drops once.
// =====================================================================

#[test]
fn bytes_ops_e2e_02_concat() {
    let src = "\
fn main() -> i64:
    let a: bytes = b\"ab\"
    let b: bytes = b\"cd\"
    let c: bytes = a + b
    print(len(c))
    print(c[0])
    print(c[3])
    return 0
";
    assert_build_run("bytes_ops_e2e_02", src, "4\n97\n100\n");
}

// =====================================================================
// bytes_ops_e2e_03 — the LOAD-BEARING round-trip: `s.encode().decode()
// == s`. CPython 3: `"héllo".encode().decode() == "héllo"`; the encode
// mints a fresh bytes (UTF-8 bytes of the str), the decode mints a fresh
// str (back from those bytes). `é` is 2 UTF-8 bytes, so len(encode) == 6
// for the 5-character "héllo".
// =====================================================================

#[test]
fn bytes_ops_e2e_03_encode_decode_roundtrip() {
    let src = "\
fn main() -> i64:
    let s: str = \"héllo\"
    let enc: bytes = s.encode()
    print(len(enc))
    let back: str = enc.decode()
    print(back)
    let ascii: str = \"world\"
    print(ascii.encode().decode())
    return 0
";
    // "héllo" is 6 UTF-8 bytes (é=2); decode round-trips byte-exact.
    assert_build_run("bytes_ops_e2e_03", src, "6\nhéllo\nworld\n");
}

// =====================================================================
// bytes_ops_e2e_04 — THE §2.2 DESIGN POINT. Decoding INVALID UTF-8
// (`b"\xff\xfe"`) must NOT silently lossy-replace (no U+FFFD) and must
// NOT silently truncate — CLAUDE.md §2.2 forbids silent coercion. It
// TRAPS: a non-zero exit + a structured `bytes.decode: invalid utf-8 at
// byte N` diagnostic on stderr (the `std.panic` trap every Cobrust
// domain error surfaces through). This is the build-run-then-assert-FAIL
// path (the program builds fine; the trap fires at RUNTIME).
// =====================================================================

#[test]
fn bytes_ops_e2e_04_decode_invalid_utf8_traps() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"\\xff\\xfe\"
    let s: str = b.decode()
    print(s)
    return 0
";
    let path = write_cb("bytes_ops_e2e_04", src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(
        build_code, 0,
        "bytes_ops_e2e_04: build failed (the trap is RUNTIME, not \
         compile-time); stderr=\n{build_stderr}"
    );
    let (run_code, stdout, run_stderr) = run_exe(&exe);
    // The trap exits non-zero (std.panic exit code 3 — INTERNAL_PANIC).
    assert_ne!(
        run_code, 0,
        "bytes_ops_e2e_04: invalid-UTF-8 decode MUST trap (non-zero \
         exit), NOT lossy-replace; stdout={stdout:?} stderr={run_stderr:?}"
    );
    // NOTHING is printed (the trap fires BEFORE the `print(s)`); the
    // bad bytes are NEVER lossily emitted.
    assert_eq!(
        stdout, "",
        "bytes_ops_e2e_04: invalid-UTF-8 decode must NOT emit ANY \
         (lossy / replacement-char) output before trapping"
    );
    // The diagnostic names the failure + the byte offset (for the
    // LLM/user to locate the bad input — §2.5-B).
    assert!(
        run_stderr.contains("invalid utf-8"),
        "bytes_ops_e2e_04: trap diagnostic must name 'invalid utf-8'; \
         got stderr={run_stderr:?}"
    );
    assert!(
        run_stderr.contains("byte 0"),
        "bytes_ops_e2e_04: trap diagnostic must report the first bad \
         byte offset; got stderr={run_stderr:?}"
    );
}

// =====================================================================
// bytes_ops_e2e_05 — `b.hex()` lowercase hex. CPython 3 oracle:
//   b"hello".hex() == "68656c6c6f"
//   b"\xff\x00\x10".hex() == "ff0010"  (non-UTF-8 bytes, byte-exact;
//   the old lossy str-buffer path would have corrupted \xff)
// =====================================================================

#[test]
fn bytes_ops_e2e_05_hex() {
    let src = "\
fn main() -> i64:
    let a: bytes = b\"hello\"
    print(a.hex())
    let b: bytes = b\"\\xff\\x00\\x10\"
    print(b.hex())
    return 0
";
    assert_build_run("bytes_ops_e2e_05", src, "68656c6c6f\nff0010\n");
}

// =====================================================================
// bytes_ops_e2e_06 — the DROP/UB hammer. 1000 iterations each minting a
// fresh slice + concat (`bytes`, dropped once) + decode + hex (`str`,
// dropped once); the inputs are borrowed throughout. A double-free /
// use-after-free crashes or diverges; a leak shows under valgrind. The
// exact accumulator is the drop-schedule correctness proof.
//
// Per iter: len(b"payload" + b[1:5]) = 7 + 4 = 11; len(b.hex()) =
// len("payload".hex()) = 14 (2 chars/byte). acc += 11 + 14 = 25.
// 1000 * 25 == 25000.
// =====================================================================

#[test]
fn bytes_ops_e2e_06_drop_hammer_loop() {
    let src = "\
fn main() -> i64:
    let acc: i64 = 0
    let i: i64 = 0
    while i < 1000:
        let b: bytes = b\"payload\"
        let s: bytes = b[1:5]
        let c: bytes = b + s
        acc = acc + len(c)
        let dec: str = c.decode()
        let hx: str = b.hex()
        acc = acc + len(hx)
        i = i + 1
    print(acc)
    return 0
";
    assert_build_run("bytes_ops_e2e_06", src, "25000\n");
}

// =====================================================================
// bytes_ops_e2e_07 — every op BORROWS its input(s). A source `bytes`
// passed to slice / concat / decode / hex SURVIVES the call and is still
// usable afterward (it is NOT consumed); it drops exactly once at scope
// exit. (If the op consumed its input, the second use would
// use-after-free or fail the borrow check.)
// =====================================================================

#[test]
fn bytes_ops_e2e_07_inputs_borrowed_not_consumed() {
    let src = "\
fn main() -> i64:
    let b: bytes = b\"abcdef\"
    let s: bytes = b[0:3]
    print(len(s))
    let c: bytes = b + b
    print(len(c))
    let hx: str = b.hex()
    print(hx)
    # b is STILL usable after slice / concat / hex (borrowed, not moved).
    print(len(b))
    print(b[0])
    return 0
";
    // len(b[0:3])=3; len(b+b)=12; b"abcdef".hex()="616263646566";
    // b survives: len 6, b[0]==97 ('a').
    assert_build_run("bytes_ops_e2e_07", src, "3\n12\n616263646566\n6\n97\n");
}

// =====================================================================
// bytes_ops_e2e_08 — ADR-0093 Phase-2 §"Slice-shape soundness". An
// UNSUPPORTED `bytes` slice shape (open-ended `b[1:]`/`b[:3]`, a non-unit
// step `b[0:4:2]`, or a negative bound `b[1:-1]`) is REJECTED at COMPILE
// TIME (`TypeError::UnsupportedSliceShape`, §2.5-A) — NOT a silent exit-0
// whole-buffer miscompile (§2.2). BEFORE the fix, `b"hello"[1:]` built +
// ran exit 0 and printed `5` (CPython `4`); each shape below now fails the
// build with the fix-printing diagnostic naming the supported `b[lo:hi]`
// form. The MIR slice guard had the identical latent silent-fallthrough
// (now a defense-in-depth `MirError`).
// =====================================================================

#[test]
fn bytes_ops_e2e_08_unsupported_slice_shapes_reject() {
    // Open-ended high bound `b[1:]` — was silent `len 5` (CPython 4).
    assert_build_rejects(
        "bytes_ops_e2e_08a",
        "\
fn main() -> i64:
    let b: bytes = b\"hello\"
    let s: bytes = b[1:]
    print(len(s))
    return 0
",
        "b[1:len(b)]",
    );
    // Open-ended low bound `b[:3]` — was silent `len 5` (CPython 3).
    assert_build_rejects(
        "bytes_ops_e2e_08b",
        "\
fn main() -> i64:
    let b: bytes = b\"hello\"
    let s: bytes = b[:3]
    print(len(s))
    return 0
",
        "b[1:len(b)]",
    );
    // Non-unit step `b[0:4:2]` — was silent `len 5` (step dropped).
    assert_build_rejects(
        "bytes_ops_e2e_08c",
        "\
fn main() -> i64:
    let b: bytes = b\"hello\"
    let s: bytes = b[0:4:2]
    print(len(s))
    return 0
",
        "b[1:len(b)]",
    );
    // Negative bound `b[1:-1]` — was silent `len 0` (CPython 3, b\"ell\").
    assert_build_rejects(
        "bytes_ops_e2e_08d",
        "\
fn main() -> i64:
    let b: bytes = b\"hello\"
    let s: bytes = b[1:-1]
    print(len(s))
    return 0
",
        "b[1:len(b)]",
    );
}

// =====================================================================
// bytes_ops_e2e_09 — ADR-0093 Phase-2 §"bytes comparison". `bytes cmp
// bytes` (`==`/`!=`/`<`/`>`/`<=`/`>=`) is REJECTED at COMPILE TIME with a
// fix-printing `TypeMismatch` — NOT the raw inkwell ICE it was before (the
// codegen comparison path called `into_int_value()` on the opaque `bytes`
// POINTER operand → `expected the IntValue variant` panic, a §2.5 + §5.1
// violation). Lexicographic `bytes` comparison is an ADR-0093 §Phasing
// follow-up (the `__cobrust_bytes_eq`/`cmp` shim); until then the
// diagnostic prints the fix (compare `len`, or `.decode()` on UTF-8).
// =====================================================================

#[test]
fn bytes_ops_e2e_09_bytes_comparison_rejects_not_ice() {
    // `==` was the inkwell ICE (exit 101); now a clean compile reject.
    assert_build_rejects(
        "bytes_ops_e2e_09a",
        "\
fn main() -> i64:
    let a: bytes = b\"abc\"
    let b: bytes = b\"abc\"
    if a == b:
        print(1)
    return 0
",
        "comparing `bytes` values",
    );
    // `<` (lexicographic) likewise — clean reject, not an ICE.
    assert_build_rejects(
        "bytes_ops_e2e_09b",
        "\
fn main() -> i64:
    let a: bytes = b\"ab\"
    let b: bytes = b\"cd\"
    if a < b:
        print(1)
    return 0
",
        "comparing `bytes` values",
    );
}
