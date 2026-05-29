//! ADR-0077 Phase 1 — `.cb` end-to-end proof obligation for the FIRST
//! ecosystem-handle operator / index / attribute surface on
//! `coil.Buffer`:
//!
//!   - Q1 — `a + b` / `a - b` / `a * b` (same-shape, f64-only, elementwise) → Buffer
//!   - Q2 — `a[i]` scalar read → `f64` (numpy's 0-d scalar is NOT a Cobrust type)
//!   - Q3 — `a.shape` → `list[i64]`; `a.ndim` → `i64`; `a.size` → `i64`
//!   - Q4 — shape-mismatch is a RUNTIME error (panic / non-zero exit), NOT a
//!     compile error (handles carry no shape in the type); dtype is f64-only
//!
//! TEST-FIRST status (ADSD): this corpus is the *failing proof obligation*
//! for the Phase-1 DEV sprint. AT HEAD `e1a9f59` NONE of the operator /
//! index / attribute surface is wired:
//!   - `a + b` is REJECTED at typecheck — `synth_bin` (check.rs:2426) does
//!     `unify(lhs, rhs)` then matches the resolved type; `Ty::Adt(COIL_BUFFER_ADT)`
//!     falls into the `other =>` arm → `TypeError::TypeMismatch` (check.rs:2456).
//!   - `a[i]` / `a.shape` / `a.ndim` / `a.size` currently *fall through* the
//!     typecheck Index/Attr arms to `Ok(self.fresh_var())` (check.rs:1280 / 1250) —
//!     so they may pass `cobrust check` today but have NO MIR retarget, NO codegen
//!     extern, and NO runtime symbol, so the FULL build→link→run pipeline fails.
//!
//! Hence every positive case below is a FULL build-and-run E2E (build must
//! succeed AND the spawned binary must print the expected stdout) — the
//! genuinely-red shape until all five layers (types / mir / codegen / runtime /
//! manifest) are wired per ADR-0077 §9. These cases are NOT `#[ignore]`d on
//! purpose: they are the contract the DEV must turn green.
//!
//! Mirrors the compile→spawn→assert-stdout pattern in `coil_hello_e2e.rs`
//! + `coil_p0_e2e.rs`. The available read primitives at HEAD are
//! `coil.mean(&a)` / `coil.std(&a)` / `coil.median(b)` (scalar reducers,
//! `coil_buffer_ty() -> f64`) and `coil.print_buffer(a)` (repr). Phase 1
//! adds `a[i]` as a second read primitive — observed here via `(a[i] as i64)`
//! casts to dodge f64 print-format drift (`print(f)` for `3.14` emits
//! `"3.14\n"`, so exact-float asserts are brittle; the `as i64` cast pattern
//! is the same robustness trick `coil_p0_e2e.rs` uses).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments: the multi-line prose
// continuations after `-`/`coil.*` lines read as "lazy" list items to
// clippy, but they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. The caller spawns + asserts. Mirrors `coil_p0_e2e.rs`.
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
/// runtime-error case (which must BUILD then FAIL at run) and as a building
/// block.
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

/// Type-check-only helper (`cobrust check`, no codegen) — returns
/// `(check_succeeded, exit_code, stderr)`. Negative typecheck cases use this:
/// `cobrust check` returns exit 2 (TYPE_ERROR) on a type error
/// (cli_exit_codes.rs `ec_2_type_error`). Type-checking the operator/attr
/// surface is the cheapest place to assert the negative contract — a rejected
/// program never reaches the build pipeline.
fn try_check(source: &str) -> (bool, Option<i32>, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    (
        out.status.success(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// POSITIVE — Q1: a + b / a - b / a * b  (same-shape, f64, elementwise)
// =====================================================================

/// Positive #1 (Q1, `+`) — `coil.ones(3) + coil.ones(3)` is an
/// elementwise add yielding `[2, 2, 2]`; observe via `coil.mean(c)` (a
/// fresh handle consumed once → pass by value) → `(2.0 + 2.0 + 2.0)/3 = 2.0`
/// → `(2.0 as i64)` → "2".
///
/// PROOF OBLIGATION: `a + b` on two Buffers is rejected at typecheck today
/// (`synth_bin` `other =>` arm → TypeMismatch). The DEV must add the Buffer
/// arm to `synth_bin` (→ `coil_buffer_ty()`) + the `lower_bin` MIR retarget
/// to `__cobrust_coil_buffer_add` + the codegen extern + the cabi shim.
#[test]
fn test_e2e_buffer_add_then_mean() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let c: coil.Buffer = coil.ones(3) + coil.ones(3)\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-add");
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
        "expected mean([2,2,2]) → '2'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #2 (Q1, `-`) — `coil.ones(3) - coil.ones(3)` is elementwise
/// subtraction yielding `[0, 0, 0]`; observe via `coil.mean(c)` → `0.0` →
/// `(0.0 as i64)` → "0". Distinct op proves the per-op retarget
/// (`__cobrust_coil_buffer_sub`), not a single hard-coded symbol.
#[test]
fn test_e2e_buffer_sub_then_mean() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let c: coil.Buffer = coil.ones(3) - coil.ones(3)\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-sub");
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
        "expected mean([0,0,0]) → '0'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #3 (Q1, `*`) — elementwise multiply. Build a non-uniform
/// buffer with `coil.mgrid(0, 4)` → `[0, 1, 2, 3]`, square it elementwise
/// `a * a` → `[0, 1, 4, 9]`; observe via `coil.mean(c)` → `(0+1+4+9)/4 =
/// 3.5` → `(3.5 as i64)` → "3". The `&a` shared borrow is required so the
/// same handle feeds both operands without being consumed (ADR-0052a:
/// `coil.Buffer` is non-Copy). Squaring a non-uniform buffer also rules out
/// an accidental add/sub masquerading as mul (a uniform [1,1,1] would give
/// the same mean under +/*).
///
/// NOTE: depends on `&a * &a` lowering both operands as shared borrows of
/// the SAME handle. If Phase-1 chooses to only support distinct-handle
/// operands, the DEV may instead need two constructors; the elementwise-
/// square via reused handle is the LLM-idiomatic numpy shape and is the
/// intended contract.
#[test]
fn test_e2e_buffer_mul_self_then_mean() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 4)\n",
        "    let c: coil.Buffer = &a * &a\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-mul");
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
        "expected mean([0,1,4,9]) → '3'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — Q2: a[i] scalar read → f64
// =====================================================================

/// Positive #4 (Q2) — `coil.ones(3)[0]` is a scalar index read returning
/// `f64` `1.0`; cast `(... as i64)` → "1". Per ADR-0077 §4 the result type
/// is a plain `f64` (numpy's 0-d scalar is not a Cobrust type) — `f64` is a
/// usable number that flows into `as i64` + `print`.
///
/// PROOF OBLIGATION: `a[i]` on a Buffer falls through the typecheck Index
/// arm to `fresh_var()` today (no compile error) but has NO MIR retarget,
/// NO `__cobrust_coil_buffer_getitem` extern, NO runtime symbol — so the
/// FULL build→link→run fails. The DEV adds the typecheck Index Buffer case
/// (→ `Ty::Float`) + the `lower_expr` Index Buffer branch + the extern
/// (`ptr,i64 -> f64`) + the bounds-checked cabi shim.
#[test]
fn test_e2e_buffer_index_scalar_read() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let x: f64 = a[0]\n",
        "    print((x as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-index");
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
        "expected coil.ones(3)[0] == 1.0 → '1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #5 (Q2) — read a NON-leading, NON-trivial element so the index
/// is genuinely exercised (not just element 0 of a uniform buffer).
/// `coil.mgrid(0, 5)` → `[0, 1, 2, 3, 4]`; `a[3]` → `f64` `3.0` →
/// `(3.0 as i64)` → "3". A wrong getitem (e.g. always-returns-element-0)
/// would print "0" and fail this case.
#[test]
fn test_e2e_buffer_index_nonzero_position() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let x: f64 = a[3]\n",
        "    print((x as i64))\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-index-3");
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
        "expected coil.mgrid(0,5)[3] == 3.0 → '3'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — Q3: a.shape → list[i64], a.ndim → i64, a.size → i64
// =====================================================================

/// Positive #6 (Q3, `.shape`) — `coil.zeros(3).shape` is a parens-free
/// attribute access returning an owned `list[i64]` `[3]`. Observe BOTH the
/// dim value (`s[0]` → `3`) AND the rank (`s.len()` → `1`, since a 1-D
/// buffer has shape `[3]`, a one-element list). Prints "3\n1".
///
/// PROOF OBLIGATION: `a.shape` falls through the typecheck Attr arm to
/// `fresh_var()` today. The DEV adds the `lookup_handle_attr` manifest table
/// + the typecheck Attr Buffer case (→ `Ty::List(Box::new(Ty::Int))`) + the
/// `lower_expr` Attr Buffer branch retargeting to `__cobrust_coil_buffer_shape`
/// + the extern (`ptr -> ptr` returning a list handle) + the cabi shim that
/// builds a `list[i64]` via the cross-crate stdlib `__cobrust_list_*` externs
/// (ADR-0072 Q5 pattern; first use from coil). The returned list drops once
/// at `.cb` scope exit (existing List drop schedule, ADR-0050c).
#[test]
fn test_e2e_buffer_shape_dim_and_rank() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    let s: list[i64] = a.shape\n",
        "    print(s[0])\n",
        "    print(s.len())\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-shape");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "3\n1".trim_end(),
        "expected zeros(3).shape == [3]: s[0]='3', s.len()='1'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #7 (Q3, `.ndim`) — `coil.zeros(3).ndim` is a parens-free
/// attribute returning `i64` `1` (a 1-D buffer). Prints "1".
///
/// PROOF OBLIGATION: same `lookup_handle_attr` path as `.shape` but the
/// manifest row maps `ndim` → `__cobrust_coil_buffer_ndim` (`ptr -> i64`),
/// return type `Ty::Int`.
#[test]
fn test_e2e_buffer_ndim_is_one() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    let n: i64 = a.ndim\n",
        "    print(n)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-ndim");
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
        "expected zeros(3).ndim == 1; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// Positive #8 (Q3, `.size`) — `coil.zeros(3).size` is a parens-free
/// attribute returning `i64` `3` (total element count). Prints "3".
///
/// PROOF OBLIGATION: same path, `size` → `__cobrust_coil_buffer_size`
/// (`ptr -> i64`), return type `Ty::Int`. A non-trivial count (3, not 0/1)
/// distinguishes `.size` from `.ndim`.
#[test]
fn test_e2e_buffer_size_is_three() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(3)\n",
        "    let sz: i64 = a.size\n",
        "    print(sz)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn buffer-size");
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
        "expected zeros(3).size == 3; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — combined done-means program (ADR-0077 §8 Phase-1 done-means)
// =====================================================================

/// Positive #9 — the literal ADR-0077 §8 Phase-1 "Done-means" program in
/// one .cb file: `a + b` (→ mean observed) + `a[0]` scalar read + `a.shape`
/// dim. Exercises the whole Phase-1 surface in a single compile/run, the way
/// an LLM would actually write numpy. Buffers `a`, `b`, `c` + the shape list
/// must each drop exactly once (ADR-0077 done-means drop-count clause — the
/// DROP_COUNT assertion lives in the cabi unit tests; this E2E proves the
/// observable stdout side).
///
/// `a = ones(3)`, `b = ones(3)`, `c = &a + &b` → [2,2,2]; `mean(c)` → 2.0 →
/// "2"; `a[0]` → 1.0 → "1"; `a.shape[0]` → 3 → "3". Prints "2\n1\n3".
#[test]
fn test_e2e_phase1_done_means_combined() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(3)\n",
        "    let c: coil.Buffer = &a + &b\n",
        "    let m: f64 = coil.mean(c)\n",
        "    print((m as i64))\n",
        "    let x: f64 = a[0]\n",
        "    print((x as i64))\n",
        "    let s: list[i64] = a.shape\n",
        "    print(s[0])\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn done-means");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert_eq!(
        stdout.trim(),
        "2\n1\n3".trim_end(),
        "expected mean([2,2,2])='2', ones(3)[0]='1', shape[0]='3'; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// NEGATIVE / typecheck — must be REJECTED by `cobrust check`
// (these stay red-rejected even AFTER the Phase-1 surface lands — they
//  are the boundary the DEV's typecheck arms must NOT over-accept)
// =====================================================================

/// Negative #1 (Q1 reject — Buffer ⊕ scalar) — `a + 1` (Buffer + Int) must
/// be REJECTED. ADR-0077 §3 Phase 1 supports only Buffer+Buffer same-shape;
/// scalar-broadcast (`a + 1`) is an explicit §12 deferral. The DEV's
/// `synth_bin` Buffer arm must accept `Buffer + Buffer` but still reject
/// `Buffer + Int` (today's `unify(lhs,rhs)` already fails Buffer-vs-Int; the
/// new arm must NOT widen to admit it). Expect a `TypeError` (exit 2).
#[test]
fn test_neg_buffer_plus_int_rejected() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let c: coil.Buffer = a + 1\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "Buffer + Int (scalar-broadcast deferred per §12) must be rejected; stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `a + 1`; got {code:?}; stderr=\n{stderr}",
    );
}

/// Negative #2 (Q1 reject — unsupported operator) — `a / b` (Buffer ÷
/// Buffer) must be REJECTED in Phase 1. ADR-0077 §3 supports only
/// `Add`/`Sub`/`Mul` on Buffers; `Div` (and `Mod`/`Pow`/`FloorDiv`) must
/// reject with a clear "operator not yet supported on coil.Buffer"
/// suggestion. Expect a `TypeError` (exit 2). This guards the Phase-1
/// op-set boundary: the DEV's Buffer arm must enumerate the supported ops,
/// not blanket-accept every arithmetic operator.
#[test]
fn test_neg_buffer_div_unsupported_rejected() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(3)\n",
        "    let c: coil.Buffer = a / b\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "Buffer / Buffer (Div unsupported in Phase 1) must be rejected; stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `a / b`; got {code:?}; stderr=\n{stderr}",
    );
}

/// Negative #3 (Q1 reject — non-Buffer RHS) — `a + s` where `s: str` must
/// be REJECTED. A Buffer added to a non-Buffer, non-scalar operand is a
/// clear type error; the DEV's Buffer arm must require the RHS to also be a
/// Buffer (it must not silently coerce or accept). Expect exit 2. Distinct
/// from Negative #1 (Int) — proves the rejection is "RHS must be Buffer",
/// not merely "RHS must not be Int".
#[test]
fn test_neg_buffer_plus_str_rejected() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let s: str = \"x\"\n",
        "    let c: coil.Buffer = a + s\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "Buffer + str must be rejected (RHS must be a Buffer); stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `a + s`; got {code:?}; stderr=\n{stderr}",
    );
}

// =====================================================================
// RUNTIME error — Q4: shape-mismatch is a RUNTIME panic, NOT a compile
// error (handles carry no shape in the type). This program BUILDS
// successfully (typecheck can't see shape) and TRAPS at run.
// =====================================================================

/// Negative #4 (Q4 — runtime shape-mismatch) — `coil.ones(3) +
/// coil.ones(4)` is a Buffer+Buffer add that PASSES typecheck (Cobrust
/// static types carry no shape — both are `coil.Buffer`) but the shapes are
/// incompatible. Per ADR-0077 Q4 this is a RUNTIME error: the operator
/// returns a plain `Buffer` (NOT `Result`) and a shape mismatch
/// PANICS-and-aborts via `__cobrust_panic`. Therefore this is written as a
/// build-succeeds + run-FAILS expectation (non-zero exit), NOT a compile-
/// error case.
///
/// PROOF OBLIGATION (two-part):
///   1. The program must BUILD (the DEV's `synth_bin` Buffer arm accepts
///      Buffer+Buffer regardless of shape — shape is invisible to the type).
///      At HEAD this build fails because the `+` arm doesn't exist yet; once
///      Phase 1 lands the build succeeds.
///   2. The spawned binary must EXIT NON-ZERO (the runtime shape-check in
///      `__cobrust_coil_buffer_add` panics). We assert `!success` rather than
///      a specific code so the test is robust to the exact abort signal /
///      panic exit convention; a stderr diagnostic mentioning shape is a
///      bonus, not required.
#[test]
fn test_runtime_shape_mismatch_traps() {
    // Part 1 — must build (shape is not in the type, so typecheck passes
    // once the `+` arm exists). If the build itself fails, that is also a
    // legitimate RED state for the DEV (the `+` arm isn't wired yet); we
    // surface the stderr so the failure mode is unambiguous.
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let b: coil.Buffer = coil.ones(4)\n",
        "    let c: coil.Buffer = a + b\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (built, build_stderr) = try_build(source);
    assert!(
        built,
        "Q4: `coil.ones(3) + coil.ones(4)` must BUILD (shape is not part of \
         the type — typecheck cannot reject it); build stderr=\n{build_stderr}",
    );

    // Part 2 — the built binary must trap at run (runtime shape-check
    // panics per Q4 panic-on-mismatch). Re-build to a known exe path to run
    // it (try_build discards its tempdir).
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn shape-mismatch");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "Q4: shape-mismatch add must TRAP at runtime (non-zero exit per \
         panic-on-mismatch); got success exit. stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}
