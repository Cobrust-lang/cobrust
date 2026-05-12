//! M-AI.0 α Phase 2 — C-ABI shim tests for `__cobrust_llm_complete` /
//! `__cobrust_llm_dispatch` / `__cobrust_llm_stream`.
//!
//! Spike spec: `docs/agent/spike/m-ai-0-cobrust-llm-spike.md` §"Test
//! plan" Tier 2. Mirrors the existing `cabi_input_handles_null` /
//! `cabi_input_with_data` patterns in
//! `crates/cobrust-stdlib/src/io.rs:730-761`.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The DEV agent (TDD step 3) lands
//! the three C-ABI shims in `crates/cobrust-stdlib/src/llm.rs` per the
//! spike's §"C-ABI shim shape (binding for P7-DEV)". Each test body
//! calls `require_impl()` which panics until the impl-landed marker is
//! flipped.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` (2026-05-09) — module-
//! level 18-lint test-only allow header at the TOP of every test file
//! authored under this corpus.

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

// =====================================================================
// Impl-landed marker. DEV flips when the three C-ABI shims land in
// `crates/cobrust-stdlib/src/llm.rs`:
//   - `__cobrust_llm_complete(p, m, q) -> *mut u8`
//   - `__cobrust_llm_dispatch(t, q)   -> *mut u8`
//   - `__cobrust_llm_stream(p, m, q)  -> *mut u8`  (returns list ptr)
// =====================================================================

const ADR_M_AI_0_IMPL_LANDED: bool = false;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_0_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.0 C-ABI shim impl not yet landed; DEV agent must:\n  \
         1. Add `__cobrust_llm_complete` / `__cobrust_llm_dispatch` /\n     \
            `__cobrust_llm_stream` C-ABI shims to\n     \
            `crates/cobrust-stdlib/src/llm.rs` per spike\n     \
            §\"C-ABI shim shape (binding for P7-DEV)\".\n  \
         2. Each shim allocates the result Str via `__cobrust_str_new` +\n     \
            `__cobrust_str_push_static` (mirroring `io::alloc_str_buffer`),\n     \
            and the stream shim allocates the list via\n     \
            `__cobrust_list_new(8, n)` + per-slot\n     \
            `__cobrust_list_set(list, i, buf as i64)` per spike\n     \
            §Decision 3B.\n  \
         3. Flip ADR_M_AI_0_IMPL_LANDED = true in tests/llm_cabi_corpus.rs.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Tier 2 #1 — __cobrust_llm_complete(p, m, q) with valid str buffers
// returns a non-null pointer whose readable text is the canned fixture.
// =====================================================================

#[test]
fn test_cabi_llm_complete_with_valid_str_buffers_returns_canned_text() {
    require_impl("test_cabi_llm_complete_with_valid_str_buffers_returns_canned_text");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::{__cobrust_str_new, __cobrust_str_push_static,
    //       __cobrust_str_ptr, __cobrust_str_len, __cobrust_str_drop};
    //   // Stand up a synthetic double via the seam, point COBRUST_CONFIG at
    //   // a fixture with provider "syn".
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("cabi-canned".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   // Build three heap Str buffers carrying provider/model/prompt.
    //   unsafe {
    //       let p_buf = __cobrust_str_new();
    //       let p_bytes = b"syn"; __cobrust_str_push_static(p_buf, p_bytes.as_ptr(), p_bytes.len() as i64);
    //       let m_buf = __cobrust_str_new();
    //       let m_bytes = b"m1"; __cobrust_str_push_static(m_buf, m_bytes.as_ptr(), m_bytes.len() as i64);
    //       let q_buf = __cobrust_str_new();
    //       let q_bytes = b"hi"; __cobrust_str_push_static(q_buf, q_bytes.as_ptr(), q_bytes.len() as i64);
    //       let out = cobrust_stdlib::llm::__cobrust_llm_complete(p_buf, m_buf, q_buf);
    //       assert!(!out.is_null());
    //       let ptr = __cobrust_str_ptr(out);
    //       let len = __cobrust_str_len(out);
    //       assert!(len > 0);
    //       let bytes = std::slice::from_raw_parts(ptr, len as usize);
    //       assert_eq!(std::str::from_utf8(bytes).unwrap(), "cabi-canned");
    //       __cobrust_str_drop(out); __cobrust_str_drop(p_buf);
    //       __cobrust_str_drop(m_buf); __cobrust_str_drop(q_buf);
    //   }
}

// =====================================================================
// Tier 2 #2 — input args sourced from `__cobrust_input` round-trip
// (heap str pointer flow — same producer as PRELUDE-emitted source code).
// =====================================================================

#[test]
fn test_cabi_llm_complete_args_from_input_round_trip_heap_str_pointers() {
    require_impl("test_cabi_llm_complete_args_from_input_round_trip_heap_str_pointers");
    // Once impl lands, DEV uncomments:
    //
    //   // Mimic the codegen-emitted shape: `__cobrust_input` produces an
    //   // owned Str buffer whose pointer is the same opaque `*mut u8` shape
    //   // that `__cobrust_llm_complete` expects.
    //   use cobrust_stdlib::fmt::{__cobrust_str_new, __cobrust_str_push_static,
    //       __cobrust_str_ptr, __cobrust_str_len, __cobrust_str_drop};
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("via-input".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   // Build three buffers via the same path __cobrust_input uses
    //   // (`io::alloc_str_buffer`): str_new + str_push_static.
    //   unsafe {
    //       let provider_buf = alloc_str_via_static("syn");
    //       let model_buf    = alloc_str_via_static("m1");
    //       let prompt_buf   = alloc_str_via_static("hi");
    //       let out = cobrust_stdlib::llm::__cobrust_llm_complete(
    //           provider_buf, model_buf, prompt_buf);
    //       assert!(!out.is_null());
    //       let s = read_cstr_buf(out);
    //       assert_eq!(s, "via-input");
    //       __cobrust_str_drop(out);
    //       __cobrust_str_drop(provider_buf);
    //       __cobrust_str_drop(model_buf);
    //       __cobrust_str_drop(prompt_buf);
    //   }
}

// =====================================================================
// Tier 2 #3 — `.rodata` static literal args via the `materialize_str_data`
// codegen path — DEV's shim reads them through the same accessor as
// `__cobrust_println_str_buf` (io.rs:243-265).
// =====================================================================

#[test]
fn test_cabi_llm_complete_with_rodata_static_literal_args_path() {
    require_impl("test_cabi_llm_complete_with_rodata_static_literal_args_path");
    // Once impl lands, DEV uncomments:
    //
    //   // .rodata literals arrive as `*mut u8` pointers that look identical
    //   // to heap Str pointers at the shim boundary. The shim reads them
    //   // through `__cobrust_str_ptr` + `__cobrust_str_len` (the f-string
    //   // runtime's accessors that handle both producers). DEV mirrors this.
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("rodata-path".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   unsafe {
    //       let p = alloc_str_via_static("syn");
    //       let m = alloc_str_via_static("m1");
    //       let q = alloc_str_via_static("hi");
    //       let out = cobrust_stdlib::llm::__cobrust_llm_complete(p, m, q);
    //       assert_eq!(read_cstr_buf(out), "rodata-path");
    //       cobrust_stdlib::fmt::__cobrust_str_drop(out);
    //       cobrust_stdlib::fmt::__cobrust_str_drop(p);
    //       cobrust_stdlib::fmt::__cobrust_str_drop(m);
    //       cobrust_stdlib::fmt::__cobrust_str_drop(q);
    //   }
}

// =====================================================================
// Tier 2 #4 — null-arg robustness: any null pointer arg → returns a
// non-null empty Str (allocated via `__cobrust_str_new`) per the
// __cobrust_input null-arg precedent (io.rs:196-202).
// =====================================================================

#[test]
fn test_cabi_llm_complete_handles_null_args() {
    require_impl("test_cabi_llm_complete_handles_null_args");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::{__cobrust_str_len, __cobrust_str_drop};
    //   // Even when config is missing, the shim must not segfault on null.
    //   unsafe {
    //       let out = cobrust_stdlib::llm::__cobrust_llm_complete(
    //           std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
    //       assert!(!out.is_null(), "shim must return non-null buffer (empty Str)");
    //       assert_eq!(__cobrust_str_len(out), 0, "null args → empty Str");
    //       __cobrust_str_drop(out);
    //   }
}

// =====================================================================
// Tier 2 #5 — empty-string args (zero-length Str buffers) → returns "".
// =====================================================================

#[test]
fn test_cabi_llm_complete_empty_string_args_returns_empty() {
    require_impl("test_cabi_llm_complete_empty_string_args_returns_empty");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::{__cobrust_str_new, __cobrust_str_len, __cobrust_str_drop};
    //   unsafe {
    //       let empty_a = __cobrust_str_new();
    //       let empty_b = __cobrust_str_new();
    //       let empty_c = __cobrust_str_new();
    //       let out = cobrust_stdlib::llm::__cobrust_llm_complete(
    //           empty_a, empty_b, empty_c);
    //       assert!(!out.is_null());
    //       assert_eq!(__cobrust_str_len(out), 0, "empty args → empty result");
    //       __cobrust_str_drop(out);
    //       __cobrust_str_drop(empty_a);
    //       __cobrust_str_drop(empty_b);
    //       __cobrust_str_drop(empty_c);
    //   }
}

// =====================================================================
// Tier 2 #6 — __cobrust_llm_dispatch(t, q) analogous Tier 1 #7 — valid
// task name routes through the routing table to first preferred provider.
// =====================================================================

#[test]
fn test_cabi_llm_dispatch_valid_task_routes_through_routing_table() {
    require_impl("test_cabi_llm_dispatch_valid_task_routes_through_routing_table");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::{__cobrust_str_drop};
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("dispatched".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_routing_toml("greet", "quality",
    //       &["syn:m1"]));
    //   unsafe {
    //       let t = alloc_str_via_static("greet");
    //       let q = alloc_str_via_static("hello");
    //       let out = cobrust_stdlib::llm::__cobrust_llm_dispatch(t, q);
    //       assert_eq!(read_cstr_buf(out), "dispatched");
    //       __cobrust_str_drop(out);
    //       __cobrust_str_drop(t);
    //       __cobrust_str_drop(q);
    //   }
}

// =====================================================================
// Tier 2 #7 — __cobrust_llm_stream(p, m, q) returns a list pointer;
// iterating via `__cobrust_list_get` + `__cobrust_list_len` recovers the
// chunk strings. Each list element is a Str buffer (heap pointer cast to
// i64) per the `__cobrust_argv` precedent (env.rs:62-85).
// =====================================================================

#[test]
fn test_cabi_llm_stream_returns_list_of_str_pointers_iterable() {
    require_impl("test_cabi_llm_stream_returns_list_of_str_pointers_iterable");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::collections::{__cobrust_list_get, __cobrust_list_len};
    //   use cobrust_stdlib::fmt::{__cobrust_str_ptr, __cobrust_str_len, __cobrust_str_drop};
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("ab".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   unsafe {
    //       let p = alloc_str_via_static("syn");
    //       let m = alloc_str_via_static("m1");
    //       let q = alloc_str_via_static("hi");
    //       let list = cobrust_stdlib::llm::__cobrust_llm_stream(p, m, q);
    //       let len = __cobrust_list_len(list);
    //       assert!(len > 0, "stream list must be non-empty");
    //       let mut concat = String::new();
    //       for i in 0..len {
    //           let elem_ptr = __cobrust_list_get(list, i) as *mut u8;
    //           let ep = __cobrust_str_ptr(elem_ptr);
    //           let el = __cobrust_str_len(elem_ptr);
    //           let bytes = std::slice::from_raw_parts(ep, el as usize);
    //           concat.push_str(std::str::from_utf8(bytes).unwrap());
    //       }
    //       assert_eq!(concat, "ab",
    //           "concatenated chunks must equal the synthetic completion text");
    //       __cobrust_str_drop(p); __cobrust_str_drop(m); __cobrust_str_drop(q);
    //   }
}

// =====================================================================
// Tier 2 #8 — UTF-8 round-trip through the C-ABI: prompt with multi-byte
// chars + canned multi-byte response.
// =====================================================================

#[test]
fn test_cabi_llm_complete_utf8_round_trip_through_str_buffers() {
    require_impl("test_cabi_llm_complete_utf8_round_trip_through_str_buffers");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::__cobrust_str_drop;
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("你好世界".into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   unsafe {
    //       let p = alloc_str_via_static("syn");
    //       let m = alloc_str_via_static("m1");
    //       let q = alloc_str_via_static("こんにちは");
    //       let out = cobrust_stdlib::llm::__cobrust_llm_complete(p, m, q);
    //       assert_eq!(read_cstr_buf(out), "你好世界");
    //       __cobrust_str_drop(out);
    //       __cobrust_str_drop(p); __cobrust_str_drop(m); __cobrust_str_drop(q);
    //   }
}

// =====================================================================
// Tier 2 #9 — Concurrent C-ABI invocation: 16 threads each calling
// __cobrust_llm_complete in parallel; no double-frees, no UB.
// =====================================================================

#[test]
fn test_cabi_llm_complete_concurrent_calls_no_ub() {
    require_impl("test_cabi_llm_complete_concurrent_calls_no_ub");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::__cobrust_str_drop;
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("parallel".into()); 16]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let _guard = set_cobrust_config(&write_synthetic_toml_single("syn"));
    //   use std::thread;
    //   let handles: Vec<_> = (0..16).map(|_| {
    //       thread::spawn(|| unsafe {
    //           let p = alloc_str_via_static("syn");
    //           let m = alloc_str_via_static("m1");
    //           let q = alloc_str_via_static("hi");
    //           let out = cobrust_stdlib::llm::__cobrust_llm_complete(p, m, q);
    //           let s = read_cstr_buf(out);
    //           __cobrust_str_drop(out);
    //           __cobrust_str_drop(p);
    //           __cobrust_str_drop(m);
    //           __cobrust_str_drop(q);
    //           s
    //       })
    //   }).collect();
    //   for h in handles {
    //       assert_eq!(h.join().unwrap(), "parallel");
    //   }
}

// =====================================================================
// Tier 2 #10 — Ledger sanity (post-Tier 2 #9): every dispatch path
// captured by __cobrust_llm_complete writes a ledger entry via the same
// Router::dispatch contract as the Rust-side helper. Mirrors Tier 1 #15
// but exercised through the C-ABI surface.
// =====================================================================

#[test]
fn test_cabi_llm_complete_writes_ledger_entries_per_dispatch() {
    require_impl("test_cabi_llm_complete_writes_ledger_entries_per_dispatch");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::__cobrust_str_drop;
    //   let dir = tempfile::tempdir().unwrap();
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok("cabi-ok".into()); 4]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   let cfg = write_synthetic_toml_with_ledger(&dir, "syn");
    //   let _guard = set_cobrust_config(&cfg);
    //   unsafe {
    //       for _ in 0..4 {
    //           let p = alloc_str_via_static("syn");
    //           let m = alloc_str_via_static("m1");
    //           let q = alloc_str_via_static("hi");
    //           let out = cobrust_stdlib::llm::__cobrust_llm_complete(p, m, q);
    //           let _ = read_cstr_buf(out);
    //           __cobrust_str_drop(out);
    //           __cobrust_str_drop(p);
    //           __cobrust_str_drop(m);
    //           __cobrust_str_drop(q);
    //       }
    //   }
    //   let entries = read_ledger(&dir.path().join("ledger.jsonl"));
    //   assert!(entries.iter().all(|e| matches!(e.outcome, Outcome::Ok)),
    //       "every entry must be outcome:ok via the C-ABI shim path");
    //   assert!(entries.len() >= 1, "ledger must have at least one entry");
}
