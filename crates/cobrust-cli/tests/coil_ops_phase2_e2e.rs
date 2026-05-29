//! ADR-0077 **Phase 2a** — `.cb` end-to-end proof obligation for the
//! second tranche of the `coil.Buffer` operator / index surface. Phase 1
//! (commit `73c2747`: `a + b` / `a[i]`-read / `a.shape`) is GREEN; this
//! corpus is the *failing* contract for the tractable Phase-2 subset:
//!
//!   - **`a.dot(b)`** (Q5 method-form op) — 1-D dot product →
//!     `f64` scalar. `coil::Array::dot` already implements 1-D × 1-D →
//!     0-d scalar (`array.rs:494` → `linalg::dot`); the gap is purely the
//!     `.cb`-side wiring (no `(COIL_BUFFER_ADT, "dot")` row in
//!     `lookup_handle_method`, no `__cobrust_coil_buffer_dot` shim).
//!   - **`a[i] = v`** (Q2 write path) — scalar write that MUST mutate
//!     (read-back proves it). ADR-0077 §4 write-path: retarget the
//!     assignment-target Index site (`lower.rs:594`, today Dict-only) to
//!     `__cobrust_coil_buffer_setitem(a, i, v) -> ()`.
//!   - **`a[lo:hi]`** (Q2 slice read) — slice returning a fresh `Buffer`,
//!     observed via `.size` (== `hi - lo`) and `[0]` (== `a[lo]`). ADR-0077
//!     §4 Phase-2 slice: a `start,stop,step` C-ABI →
//!     `__cobrust_coil_buffer_slice`. `coil::Array::slice` already exists
//!     (`array.rs:296`); the gap is the slice ABI + MIR retarget.
//!
//! TEST-FIRST status (ADSD). AT HEAD `93781da` NONE of the three surfaces
//! is wired — empirically (probed against the HEAD `cobrust` binary):
//!
//!   - `a.dot(b)` → `cobrust check` REJECTS with exit 2:
//!     `UnknownMethod { method_name: "dot" }` — "method `dot` not found on
//!     `Adt#…`" (`lookup_handle_method` has no `dot` row). So a `.dot`
//!     program never reaches build.
//!   - `a[i] = v` PARSES + type-checks (`a[1] = 5.0` with plain `let a`,
//!     NOT `let mut` — Cobrust has no `mut` keyword here) and BUILDS, but
//!     the assignment-target Index site (`lower.rs:594`) only retargets
//!     `Ty::Dict`; a Buffer base FALLS THROUGH to the legacy
//!     `Place::Index` projection (a Wave-1 no-op on an opaque handle
//!     pointer) — the write is silently dropped and a subsequent read-back
//!     SEGFAULTS (observed run exit 139). The DEV must add a Buffer
//!     assignment-target arm so the write mutates and the read-back returns
//!     the written value.
//!   - `a[lo:hi]` PASSES `cobrust check` (the typecheck Index arm's
//!     `(other, Slice) => Ok(other.clone())` catch-all returns
//!     `coil.Buffer`) but FAILS at build: `lower_index` collapses a
//!     `Slice` to `Constant::Int(0)` and the legacy `Projection::Index`
//!     path mis-types the handle, so LLVM module-verify rejects the
//!     module ("Call parameter type does not match function signature… ptr
//!     … call … (double …)"). The DEV must add a Buffer slice retarget +
//!     a `__cobrust_coil_buffer_slice` shim.
//!
//! Per ADR-0077 Q4 / §5.1 elegance: a `coil.Buffer` handle carries NO
//! shape/bounds in its static type (it is a single `Ty::Adt(COIL_BUFFER_
//! ADT)` regardless of length). Therefore shape / bounds violations are
//! **RUNTIME** errors, NOT compile errors. The negative cases below are
//! written as **build-succeeds + run-FAILS** (non-zero exit), NOT as
//! `cobrust check` rejections — exactly the Q4 panic-on-violation
//! discipline `buffer_binop` already uses for `a + b` shape mismatch.
//!
//! NONE of these are `#[ignore]`d: they are the contract the Phase-2a DEV
//! must turn green (corpus + impl land atomically). Mirrors the
//! compile→spawn→assert-stdout harness of `coil_ops_e2e.rs` /
//! `coil_p0_e2e.rs`. Reads are observed via `(x as i64)` casts to dodge
//! f64 print-format drift (the same robustness trick the Phase-1 corpus
//! uses).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments: the multi-line prose
// continuations after `-`/`coil.*` lines read as "lazy" list items to
// clippy, but they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. The caller spawns + asserts. Mirrors `coil_ops_e2e.rs`.
fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
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
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

/// Build-only helper — returns `(build_succeeded, stderr)`. Used by the Q4
/// runtime-error negatives, which must BUILD then FAIL at run (shape /
/// bounds is invisible to the type, so the build cannot reject them).
fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// POSITIVE — Q5: a.dot(b)  (1-D dot product → f64 scalar)
// =====================================================================

/// Positive #1 (Q5, `.dot` of uniform buffers) — `coil.ones(3).dot(
/// coil.ones(3))` is the 1-D dot product `1*1 + 1*1 + 1*1 = 3.0` →
/// `(3.0 as i64)` → "3". `coil::Array::dot` (array.rs:494) already returns
/// a 0-d scalar for 1-D × 1-D; the DEV adds the `(COIL_BUFFER_ADT, "dot")`
/// method row (`vec![Value(coil_buffer_ty())]`, ret `Ty::Float`) + the
/// `__cobrust_coil_buffer_dot(a, b) -> f64` shim that extracts the scalar.
///
/// `&a.dot(&b)` is NOT used — `dot` consumes its operands by value here
/// (each `coil.ones(3)` is a fresh handle); the method-form `recv.dot(arg)`
/// borrows the receiver via the existing `upgrade_move_to_copy_handle`
/// ecosystem-method lowering, and the single `arg` is a Value param.
///
/// PROOF OBLIGATION: `a.dot(b)` is REJECTED at typecheck today —
/// `method `dot` not found on `Adt#…`` (`UnknownMethod`, exit 2). RED.
#[test]
fn test_e2e_buffer_dot_ones_is_three() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(3)\n",
        "    let d: f64 = a.dot(b)\n",
        "    print((d as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-dot-ones");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "3",
        "expected ones(3).dot(ones(3)) == 3.0 → '3'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 (Q5, `.dot` of a non-uniform buffer with itself) — exercises
/// the actual multiply-accumulate, not just a count of ones. `coil.mgrid(0,
/// 4)` → `[0, 1, 2, 3]`; `a.dot(a)` = `0*0 + 1*1 + 2*2 + 3*3 = 14` →
/// `(14.0 as i64)` → "14". A dot that wrongly summed-without-squaring (`0+1
/// +2+3 = 6`) or counted length (`4`) would fail this case. The receiver is
/// reused for both operands (`a.dot(a)`) — the ecosystem-method lowering
/// must borrow the receiver and pass the same live handle as the arg
/// without consuming it (mirrors the Phase-1 `&a * &a` reused-handle
/// contract).
#[test]
fn test_e2e_buffer_dot_self_squares_and_sums() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 4)\n",
        "    let d: f64 = a.dot(a)\n",
        "    print((d as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-dot-self");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "14",
        "expected mgrid(0,4).dot(self) == 0+1+4+9 == 14 → '14'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — Q2: a[i] = v  scalar write (MUST mutate; read-back proves it)
// =====================================================================

/// Positive #3 (Q2 write) — `a[1] = 5.0` MUST mutate the buffer in place;
/// a read-back of `a[1]` proves it. `coil.zeros(3)` → `[0, 0, 0]`; write
/// `a[1] = 5.0` → `[0, 5, 0]`; read `a[1]` → `f64` `5.0` →
/// `(5.0 as i64)` → "5".
///
/// PROOF OBLIGATION: at HEAD this PARSES + type-checks + BUILDS, but the
/// assignment-target Index site (`lower.rs:594`) retargets only `Ty::Dict`;
/// a Buffer base falls through to the legacy `Place::Index` no-op (a Wave-1
/// stub on an opaque handle pointer) — the write is silently dropped and
/// the subsequent read-back SEGFAULTS (observed run exit 139). The DEV adds
/// a Buffer assignment-target arm retargeting to
/// `__cobrust_coil_buffer_setitem(a, i, v) -> ()` (borrows `a` mutably; the
/// `.cb` scope owns the only handle, ADR-0077 §4 / ADR-0072 Q4) so the
/// write mutates and the read-back returns 5.0. NOTE: Cobrust has NO `mut`
/// keyword (`let mut a` is a parse error); a plain `let a` binding is
/// mutated in place — this matches the Dict/List `d[k] = v` / `xs[i] = v`
/// precedent, which also uses plain `let`.
#[test]
fn test_e2e_buffer_setitem_mutates_readback() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    a[1] = 5.0\n",
        "    let x: f64 = a[1]\n",
        "    print((x as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-setitem");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "5",
        "expected a[1]=5.0 to MUTATE: read-back a[1] == 5.0 → '5' (got '0' means the \
         write was a no-op); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #4 (Q2 write — isolation, no aliasing of other slots) — write
/// to ONE slot, read a DIFFERENT untouched slot, to prove the write is
/// targeted (not a broadcast-fill). `coil.zeros(3)` → `[0, 0, 0]`; write
/// `a[2] = 7.0` → `[0, 0, 7]`; read the untouched `a[0]` → `f64` `0.0` →
/// `(0.0 as i64)` → "0". A write that wrongly filled every slot would print
/// "7" and fail; a no-op write (HEAD) segfaults on the read-back. Together
/// with #3 (the written slot reads back the value) this pins down "writes
/// exactly the indexed slot".
#[test]
fn test_e2e_buffer_setitem_targets_one_slot() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    a[2] = 7.0\n",
        "    let x: f64 = a[0]\n",
        "    print((x as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn setitem-targeted");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "0",
        "expected a[2]=7.0 to leave a[0] untouched at 0.0 → '0' (got '7' means the write \
         broadcast-filled); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — Q2: a[lo:hi]  slice read → fresh Buffer
// =====================================================================

/// Positive #5 (Q2 slice — length) — `coil.mgrid(0, 5)[1:3]` slices
/// `[0, 1, 2, 3, 4]` to a fresh `Buffer` `[1, 2]`; observe its length via
/// `.size` → `i64` `2`. Prints "2".
///
/// PROOF OBLIGATION: `a[1:3]` PASSES `cobrust check` (the typecheck Index
/// arm `(other, Slice) => Ok(other.clone())` returns `coil.Buffer`) but
/// FAILS at build today — `lower_index` collapses the `Slice` to
/// `Constant::Int(0)` and the legacy `Projection::Index` path mis-types the
/// handle, so LLVM module-verify rejects ("Call parameter type does not
/// match function signature… call … (double …)"). The DEV adds a Buffer
/// `Slice` retarget in the `lower_expr` Index arm →
/// `__cobrust_coil_buffer_slice(a, lo, hi) -> Buffer` (a start/stop ABI;
/// `coil::Array::slice` at array.rs:296 already produces the view/copy) +
/// the extern + the shim. The returned `Buffer` drops once at `.cb` scope
/// exit (existing Buffer drop schedule).
#[test]
fn test_e2e_buffer_slice_size_is_two() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let s: coil.Buffer = a[1:3]\n",
        "    let n: i64 = s.size\n",
        "    print(n)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn buffer-slice-size");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "2",
        "expected mgrid(0,5)[1:3].size == 2 → '2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #6 (Q2 slice — content offset) — proves the slice starts at
/// `lo` (not at 0): `coil.mgrid(0, 5)[1:3]` → `[1, 2]`; its element `[0]`
/// is the ORIGINAL `a[1]` == `1.0` → `(1.0 as i64)` → "1". A slice that
/// wrongly started at index 0 would yield `[0, 1]` and print "0"; a slice
/// that aliased the whole buffer would also print "0" (its `[0]` == `a[0]`
/// == 0.0). This case + #5 (correct length) together pin down "fresh buffer
/// of `a[lo..hi]`".
#[test]
fn test_e2e_buffer_slice_offset_first_element() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let s: coil.Buffer = a[1:3]\n",
        "    let x: f64 = s[0]\n",
        "    print((x as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe)
        .output()
        .expect("spawn buffer-slice-first");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "expected mgrid(0,5)[1:3][0] == a[1] == 1.0 → '1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// RUNTIME errors — Q4: shape / bounds violations are RUNTIME (handles
// carry no shape/length in the type). These programs BUILD (typecheck +
// codegen succeed) and TRAP at run (non-zero exit). NOT compile errors.
// =====================================================================

/// Negative #1 (Q5 / Q4 — `.dot` shape mismatch is a RUNTIME error) —
/// `coil.ones(3).dot(coil.ones(4))` is a 1-D dot of incompatible lengths.
/// Per ADR-0077 Q4 the handle type carries no shape, so this must PASS
/// typecheck (both are `coil.Buffer`) and TRAP at runtime (the dot shim's
/// shape check aborts via `coil_panic`, exactly as `buffer_binop` does for
/// `a + b` mismatch). Written as build-succeeds + run-FAILS, NOT a compile
/// error.
///
/// PROOF OBLIGATION (two-part):
///   1. The program must BUILD — once the DEV adds the `dot` method row +
///      shim, typecheck accepts `Buffer.dot(Buffer)` regardless of length.
///      At HEAD this build FAILS (the `dot` method does not exist →
///      `UnknownMethod`); that is also a legitimate RED state, so we
///      surface the build stderr to make the failure mode unambiguous.
///   2. The built binary must EXIT NON-ZERO (the dot shim's runtime
///      shape-check aborts). We assert `!success` (not a specific code) so
///      the test is robust to the exact abort convention.
#[test]
fn test_runtime_dot_shape_mismatch_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(4)\n",
        "    let d: f64 = a.dot(b)\n",
        "    print((d as i64))\n",
        "    return 0\n",
    );
    // Part 1 — must build (length is not in the type, so typecheck cannot
    // reject `Buffer.dot(Buffer)` once the method row exists).
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Q4: `ones(3).dot(ones(4))` must BUILD (length is not part of the type — typecheck \
         cannot reject it; the DEV must wire the `dot` method + shim); build stderr=\n{build_stderr}",
    );
    // Part 2 — the built binary must trap at run (runtime shape-check).
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn dot-mismatch");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Q4: `ones(3).dot(ones(4))` must TRAP at runtime (non-zero exit — incompatible \
         dot lengths); got success exit. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative #2 (Q2 write / Q4 — out-of-bounds write is a RUNTIME error) —
/// `coil.zeros(3)` then `a[9] = 1.0`. Index 9 is out of bounds for a
/// 3-element buffer. The bound is not in the type (the handle is just
/// `coil.Buffer`), so this must PASS typecheck + BUILD and TRAP at runtime
/// (the setitem shim's bounds-check aborts via `coil_panic`). Build-
/// succeeds + run-FAILS.
///
/// PROOF OBLIGATION (two-part):
///   1. Must BUILD — `a[i] = v` already parses + type-checks + builds at
///      HEAD (the assignment-target site accepts any base; bounds are
///      invisible). Stays building once the DEV wires the real setitem.
///   2. Must EXIT NON-ZERO once the real (bounds-checked) setitem lands.
///      AT HEAD this is a FALSE GREEN — the no-op write silently "succeeds"
///      with exit 0 (observed) because the legacy `Place::Index` path never
///      touches the buffer. So this case is RED now because the binary
///      exits 0 instead of trapping; the DEV's bounds-checked
///      `__cobrust_coil_buffer_setitem` makes it exit non-zero. (This is
///      the F37-style silent-rot hazard the real setitem closes.)
#[test]
fn test_runtime_setitem_out_of_bounds_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    a[9] = 1.0\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Q4: out-of-bounds write `a[9]=1.0` must BUILD (the bound is not part of the type); \
         build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn setitem-oob");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Q4: out-of-bounds write `a[9]=1.0` on a 3-element buffer must TRAP at runtime \
         (non-zero exit — a real bounds-checked setitem aborts; exit 0 means the write was \
         a silent no-op). stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Negative #3 (Q2 slice / Q4 — out-of-bounds slice is a RUNTIME error) —
/// `coil.mgrid(0, 5)` then `a[1:99]`. The stop bound 99 exceeds the
/// 5-element buffer. Bounds are not in the type, so this must PASS
/// typecheck + BUILD and TRAP at runtime (the slice shim's bounds-check
/// aborts). Build-succeeds + run-FAILS, NOT a compile error.
///
/// PROOF OBLIGATION (two-part):
///   1. Must BUILD once the DEV wires the slice retarget + shim. At HEAD
///      the build FAILS at LLVM module-verify (the unwired `Slice` path
///      mis-types the handle) — a legitimate RED state; the build stderr is
///      surfaced.
///   2. Must EXIT NON-ZERO (the slice shim's out-of-bounds check aborts).
///      We assert `!success` (not a specific code) for robustness to the
///      abort convention.
///
/// (numpy itself CLAMPS an over-long slice stop rather than raising; this
/// case asserts the Cobrust-honest "out-of-bounds slice traps" contract per
/// ADR-0077 Q4 panic-on-violation. If the Phase-2a DEV chooses numpy-style
/// clamping instead, this is the one negative the DEV may re-spec to a
/// positive `[1:99].size == 4` clamp assertion — flagged here so the choice
/// is explicit, not silent.)
#[test]
fn test_runtime_slice_out_of_bounds_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let s: coil.Buffer = a[1:99]\n",
        "    let n: i64 = s.size\n",
        "    print(n)\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Q4: out-of-bounds slice `a[1:99]` must BUILD (bounds are not part of the type — \
         the DEV must wire the slice retarget + shim); build stderr=\n{build_stderr}",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn slice-oob");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Q4: out-of-bounds slice `a[1:99]` on a 5-element buffer must TRAP at runtime \
         (non-zero exit). stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}
