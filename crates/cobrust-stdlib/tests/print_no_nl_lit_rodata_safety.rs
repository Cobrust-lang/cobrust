//! LC-100 Tier A Sprint 2 TEST — failing-test corpus for the proposed
//! `__cobrust_print_no_nl_lit(ptr: *const u8, len: usize)` C-ABI shim
//! that closes the Pattern A `.rodata` literal misalignment defect.
//!
//! Spec source: `docs/agent/findings/lc100-pattern-a-rodata-literal-misalignment.md`
//! Candidate F1 — preferred fix.
//!
//! ## TDD contract for Sprint 2 DEV agent (next)
//!
//! 1. Add C-ABI shim in `crates/cobrust-stdlib/src/io.rs`:
//!
//!    ```rust,ignore
//!    /// C-ABI shim for source-level `print_no_nl(literal)` where the argument
//!    /// is a compile-time-known string literal lowered to a `.rodata` byte
//!    /// pointer. Unlike `__cobrust_print_no_nl(buf)` which casts `buf` to
//!    /// `*StringBuffer` (requires 8-byte alignment), this shim takes the
//!    /// raw `(ptr, len)` pair — exactly the shape of `__cobrust_println` —
//!    /// and writes the bytes to stdout without a trailing newline.
//!    ///
//!    /// # Safety
//!    ///
//!    /// `ptr` must point to `len` valid UTF-8 bytes for the duration of
//!    /// the call. `ptr` may be null iff `len == 0`.
//!    #[unsafe(no_mangle)]
//!    pub unsafe extern "C" fn __cobrust_print_no_nl_lit(ptr: *const u8, len: usize)
//!    ```
//!
//! 2. Add one `runtime_helper_signatures` entry in
//!    `crates/cobrust-codegen/src/cranelift_backend.rs` (around line 2002):
//!
//!    ```rust,ignore
//!    out.push(("__cobrust_print_no_nl_lit", sig(call_conv, &[p, i64], None)));
//!    ```
//!
//! 3. Update the `Kind::PrintNoNl` arm in
//!    `crates/cobrust-cli/src/build/intrinsics.rs` (~line 722) to detect
//!    `Operand::Constant(Constant::Str(_))` arguments and route to the
//!    new `_lit` runtime symbol with `(ptr, len)` instead of the
//!    existing `__cobrust_print_no_nl(buf_ptr)` shape.
//!
//! 4. Flip `SPRINT_2_DEV_IMPL_LANDED` below to `true`. All 5 tests must pass.
//!
//! ## Test design — gate-constant pattern
//!
//! Each test body asserts `SPRINT_2_DEV_IMPL_LANDED == true` first; on
//! failure it panics with a "NOT YET IMPLEMENTED" message. Sprint 2 DEV
//! replaces the panic body with real `__cobrust_print_no_nl_lit(...)`
//! invocations. This means:
//!
//!   1. Tests COMPILE today (no E0425 — no direct reference to the
//!      not-yet-existing symbol).
//!   2. `cargo test` reports 5 FAILURES today (each panics).
//!   3. Sprint 2 DEV has a precise contract to fulfill.
//!
//! Per `feedback_p9_clippy_stall_pattern.md`: 18-lint test-only allow
//! header at top of file.

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
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::needless_raw_string_hashes)]

// =====================================================================
// Gate constant — flip to `true` once Sprint 2 DEV impl lands.
// =====================================================================
//
// While this is `false`, every test in this file panics inside
// `require_impl()`, producing a FAILING test corpus. Sprint 2 DEV
// MUST:
//   (a) implement `__cobrust_print_no_nl_lit(ptr, len)` in stdlib/src/io.rs
//   (b) add the codegen signature entry
//   (c) add the intrinsic-rewrite dispatch
//   (d) flip this constant to `true`
//   (e) replace each test's `require_impl(...)` call with the real
//       `cobrust_stdlib::io::__cobrust_print_no_nl_lit(...)` invocation
//       per the comments in each test.
// =====================================================================

const SPRINT_2_DEV_IMPL_LANDED: bool = false;

fn require_impl(test_name: &str) {
    assert!(
        SPRINT_2_DEV_IMPL_LANDED,
        "LC-100 Sprint 2 DEV impl not yet landed; DEV agent must:\n  \
         1. Add `pub unsafe extern \"C\" fn __cobrust_print_no_nl_lit(ptr: *const u8, len: usize)` in crates/cobrust-stdlib/src/io.rs\n  \
         2. Add `out.push((\"__cobrust_print_no_nl_lit\", sig(call_conv, &[p, i64], None)))` in crates/cobrust-codegen/src/cranelift_backend.rs::runtime_helper_signatures\n  \
         3. Update `Kind::PrintNoNl` arm in crates/cobrust-cli/src/build/intrinsics.rs to route Constant::Str args to the new _lit shim\n  \
         4. Flip SPRINT_2_DEV_IMPL_LANDED = true in this test file + replace require_impl() bodies with real C-ABI calls\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Test 1 — `__cobrust_print_no_nl_lit` writes a .rodata literal to stdout
// =====================================================================
//
// Contract once impl lands (Sprint 2 DEV replaces the require_impl call
// with the body below):
//
// ```rust,ignore
// let bytes = b"hello";
// // SAFETY: bytes points to 5 valid UTF-8 bytes; the shim never retains
// // the pointer beyond the call.
// unsafe { cobrust_stdlib::io::__cobrust_print_no_nl_lit(bytes.as_ptr(), bytes.len()) };
// // Note: side-effect-only — the assertion is that no panic fires. Use
// // an external stdout capture (e.g. `gag` crate) if byte-for-byte
// // verification is required; for the misalignment-defect repro,
// // "did not panic" is the load-bearing invariant.
// ```

#[test]
fn test_print_no_nl_lit_writes_rodata_literal_to_stdout() {
    require_impl("test_print_no_nl_lit_writes_rodata_literal_to_stdout");

    // POST-IMPL body (DEV agent replaces require_impl above with this):
    //
    // let bytes = b"hello";
    // unsafe {
    //     cobrust_stdlib::io::__cobrust_print_no_nl_lit(bytes.as_ptr(), bytes.len());
    // }
    // // No panic = pass. Stdout side effect is not asserted here
    // // (would require process-level capture). The e2e test corpus
    // // `crates/cobrust-cli/tests/lc100_pattern_a_repro_e2e.rs` covers
    // // the stdout-content assertion via subprocess.
}

// =====================================================================
// Test 2 — empty literal (`b""`, len=0) is a clean no-op
// =====================================================================
//
// Mirrors `__cobrust_println(null, 0)` empty-input semantics already
// covered by `cabi_println_empty` in stdlib_unit.rs. The new shim must
// accept len=0 (with any ptr, possibly null) and return without
// dereferencing.

#[test]
fn test_print_no_nl_lit_empty_literal_is_noop() {
    require_impl("test_print_no_nl_lit_empty_literal_is_noop");

    // POST-IMPL body:
    //
    // let bytes = b"";
    // unsafe {
    //     cobrust_stdlib::io::__cobrust_print_no_nl_lit(bytes.as_ptr(), 0);
    // }
    // // Also assert null-ptr + zero-len is safe (paralleling __cobrust_println):
    // unsafe {
    //     cobrust_stdlib::io::__cobrust_print_no_nl_lit(std::ptr::null(), 0);
    // }
}

// =====================================================================
// Test 3 — non-ASCII UTF-8 literal (`"日本語"`, len=9 bytes) writes cleanly
// =====================================================================
//
// Cobrust strings are UTF-8 (per ADR-0044 Decision 4). The literal
// `"日本語"` in Cobrust source lowers to a 9-byte .rodata sequence
// (3 codepoints × 3 bytes each). The shim must not panic on multi-byte
// codepoints — mirrors the `cabi_print_with_unicode` test in
// stdlib_unit.rs.

#[test]
fn test_print_no_nl_lit_non_ascii_utf8_literal() {
    require_impl("test_print_no_nl_lit_non_ascii_utf8_literal");

    // POST-IMPL body:
    //
    // let s = "日本語";
    // assert_eq!(s.len(), 9, "sanity: 3 codepoints × 3 bytes each");
    // unsafe {
    //     cobrust_stdlib::io::__cobrust_print_no_nl_lit(s.as_ptr(), s.len());
    // }
}

// =====================================================================
// Test 4 — multiple sequential calls compose without panic or corruption
// =====================================================================
//
// Models the LC-093 integer-to-roman pattern: each Roman digit emits a
// separate `print_no_nl("M")` / `print_no_nl("C")` / ... call. Eight
// .rodata pointers in a row are exactly the worst case for alignment
// roulette. Sprint 2 DEV impl must handle the sequence without state
// leakage between calls.

#[test]
fn test_print_no_nl_lit_multiple_sequential_calls() {
    require_impl("test_print_no_nl_lit_multiple_sequential_calls");

    // POST-IMPL body:
    //
    // let parts: &[&[u8]] = &[b"hi", b" ", b"world"];
    // for p in parts {
    //     unsafe {
    //         cobrust_stdlib::io::__cobrust_print_no_nl_lit(p.as_ptr(), p.len());
    //     }
    // }
    // // Followed by a final newline via the existing __cobrust_println:
    // unsafe {
    //     cobrust_stdlib::io::__cobrust_println(std::ptr::null(), 0);
    // }
}

// =====================================================================
// Test 5 — regression guard: existing `__cobrust_print_no_nl(buf)` path
//          still works when `buf` is a real StringBuffer (heap-aligned)
// =====================================================================
//
// CRITICAL: Sprint 2 DEV MUST NOT remove `__cobrust_print_no_nl`
// (it is still required for the runtime-str path — see fixture
// `examples/lc100_pattern_a_fixtures/print_no_nl_from_stdin.cb`).
// DEV adds the `_lit` variant ALONGSIDE the existing shim; the
// intrinsic-rewrite chooses between them based on argument shape
// (Constant::Str → _lit, runtime str → original).
//
// This test exercises the existing shim with a properly-aligned
// StringBuffer (via `__cobrust_str_new` + `__cobrust_str_push_static`)
// to guarantee the runtime-str path still works post-DEV. Today this
// test PASSES (the existing shim is correct on aligned input); after
// Sprint 2 DEV it MUST CONTINUE to pass.

#[test]
fn test_print_no_nl_existing_shim_on_aligned_string_buffer_regression() {
    // SAFETY: standard StringBuffer construction matches the pattern in
    // `alloc_str_buffer` (crates/cobrust-stdlib/src/io.rs:167-178). The
    // buffer is 8-byte-aligned per StringBuffer::layout.
    unsafe {
        let buf = cobrust_stdlib::fmt::__cobrust_str_new();
        let payload = b"buf";
        cobrust_stdlib::fmt::__cobrust_str_push_static(buf, payload.as_ptr(), payload.len() as i64);
        // The existing __cobrust_print_no_nl shim — Sprint 2 DEV MUST
        // preserve this path. We assert it does not panic on a proper
        // StringBuffer pointer (the misalignment defect only fires for
        // raw .rodata pointers, not for buffers allocated via the shim
        // itself).
        cobrust_stdlib::io::__cobrust_print_no_nl(buf);
        // Buffer cleanup: drop via the existing __cobrust_str_drop shim
        // so we do not leak (test runs many times under proptest later).
        cobrust_stdlib::fmt::__cobrust_str_drop(buf);
    }
    // Reaching here = no panic from existing shim on aligned input.
}
