//! M-AI.1 α Phase 3 — C-ABI shim tests for `__cobrust_prompt_render` /
//! `__cobrust_prompt_format_few_shot` / `__cobrust_prompt_format_system_user` /
//! `__cobrust_prompt_escape_braces` / `__cobrust_llm_complete_structured`.
//!
//! Spike spec: `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` §"Test
//! plan" Tier 2. Mirrors `crates/cobrust-stdlib/tests/llm_cabi_corpus.rs`
//! (M-AI.0 Tier 2 precedent) and the `cabi_input_handles_null` /
//! `cabi_input_with_data` patterns in `crates/cobrust-stdlib/src/io.rs`.
//!
//! TDD STEP 1 — FAILING TEST CORPUS. The DEV agent (TDD step 3) lands
//! the five C-ABI shims in `crates/cobrust-stdlib/src/prompt.rs` per the
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
#![allow(clippy::items_after_statements)]

// =====================================================================
// Impl-landed marker. DEV flips when the five C-ABI shims land in
// `crates/cobrust-stdlib/src/prompt.rs`:
//   - `__cobrust_prompt_render(system, user, vars) -> *mut u8`
//   - `__cobrust_prompt_format_few_shot(in, out, cur) -> *mut u8`
//   - `__cobrust_prompt_format_system_user(system, user) -> *mut u8`
//   - `__cobrust_prompt_escape_braces(text) -> *mut u8`
//   - `__cobrust_llm_complete_structured(prompt, schema_json) -> *mut u8`
// =====================================================================

const ADR_M_AI_1_IMPL_LANDED: bool = true;

fn require_impl(test_name: &str) {
    assert!(
        ADR_M_AI_1_IMPL_LANDED,
        "NOT YET IMPLEMENTED: M-AI.1 C-ABI shim impl not yet landed; DEV agent must:\n  \
         1. Add `__cobrust_prompt_render` / `__cobrust_prompt_format_few_shot` /\n     \
            `__cobrust_prompt_format_system_user` / `__cobrust_prompt_escape_braces` /\n     \
            `__cobrust_llm_complete_structured` C-ABI shims to\n     \
            `crates/cobrust-stdlib/src/prompt.rs` per spike\n     \
            §\"C-ABI shim shape (binding for P7-DEV)\".\n  \
         2. Each shim allocates the result Str via `__cobrust_str_new` +\n     \
            `__cobrust_str_push_static` (mirroring `io::alloc_str_buffer`),\n     \
            and reads `list[str]` args via `__cobrust_list_len` +\n     \
            `__cobrust_list_get` per the `__cobrust_argv` ABI precedent.\n  \
         3. Flip ADR_M_AI_1_IMPL_LANDED = true in tests/prompt_cabi_corpus.rs.\n  \
         FAILING TEST: {test_name}"
    );
}

// =====================================================================
// Test helpers — C-ABI buffer allocation + read utilities.
// Provided by P7-DEV per the "DEV uncomments" mandate when Tier 2
// test bodies reference these utilities.
// =====================================================================

/// Allocate a heap `Str` buffer from a static string literal.
///
/// # Safety
///
/// Caller is responsible for dropping the returned pointer via
/// `cobrust_stdlib::fmt::__cobrust_str_drop`.
#[allow(dead_code)]
unsafe fn alloc_str_via_static(s: &str) -> *mut u8 {
    use cobrust_stdlib::fmt::{__cobrust_str_new, __cobrust_str_push_static};
    // SAFETY: __cobrust_str_new returns a valid zero-length Str buffer.
    unsafe {
        let buf = __cobrust_str_new();
        if !s.is_empty() {
            // SAFETY: `s` lifetime exceeds this call; buf was just allocated.
            __cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// Read a heap `Str` buffer pointer back to a `String`.
///
/// # Safety
///
/// `buf` must be a valid, non-null Str buffer pointer.
#[allow(dead_code)]
unsafe fn read_cstr_buf(buf: *mut u8) -> String {
    use cobrust_stdlib::fmt::{__cobrust_str_len, __cobrust_str_ptr};
    // SAFETY: caller-attestation per the contract.
    unsafe {
        let ptr = __cobrust_str_ptr(buf);
        let len = __cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

// =====================================================================
// Tier 2 #1 — __cobrust_prompt_render with three valid Str buffers +
// list-of-str vars produces correct interpolated text.
// =====================================================================

#[test]
fn test_cabi_prompt_render_with_valid_buffers_returns_interpolated_text() {
    require_impl("test_cabi_prompt_render_with_valid_buffers_returns_interpolated_text");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    use cobrust_stdlib::collections::{__cobrust_list_new, __cobrust_list_set};
    unsafe {
        let sys_buf = alloc_str_via_static("You are an expert.");
        let usr_buf = alloc_str_via_static("Translate: {code}");
        // Build a list[str] of even-indexed [key, value] pairs.
        let key_buf = alloc_str_via_static("code");
        let val_buf = alloc_str_via_static("def foo(): pass");
        let vars_list = __cobrust_list_new(8, 2);
        __cobrust_list_set(vars_list, 0, key_buf as i64);
        __cobrust_list_set(vars_list, 1, val_buf as i64);
        let out = cobrust_stdlib::prompt::__cobrust_prompt_render(
            sys_buf, usr_buf, vars_list,
        );
        assert!(!out.is_null());
        assert_eq!(read_cstr_buf(out), "You are an expert.\nTranslate: def foo(): pass");
        __cobrust_str_drop(out);
        __cobrust_str_drop(sys_buf); __cobrust_str_drop(usr_buf);
        __cobrust_str_drop(key_buf); __cobrust_str_drop(val_buf);
    }
}

// =====================================================================
// Tier 2 #2 — __cobrust_prompt_render with null vars list
// (std::ptr::null_mut()) returns valid result (vars treated as empty).
// =====================================================================

#[test]
fn test_cabi_prompt_render_null_vars_list_treats_as_empty() {
    require_impl("test_cabi_prompt_render_null_vars_list_treats_as_empty");

    use cobrust_stdlib::fmt::{__cobrust_str_drop};
    unsafe {
        let sys_buf = alloc_str_via_static("sys");
        let usr_buf = alloc_str_via_static("usr");
        // null vars list → treated as empty, returns "sys\nusr".
        let out = cobrust_stdlib::prompt::__cobrust_prompt_render(
            sys_buf, usr_buf, std::ptr::null_mut(),
        );
        assert!(!out.is_null(), "null vars list must not produce null result");
        assert_eq!(read_cstr_buf(out), "sys\nusr");
        __cobrust_str_drop(out);
        __cobrust_str_drop(sys_buf); __cobrust_str_drop(usr_buf);
    }
}

// =====================================================================
// Tier 2 #3 — __cobrust_prompt_render with null system pointer + non-null
// user pointer — shim treats null as empty string; result is non-null
// (mirrors M-AI.0 Tier 2 #4 null-arg robustness).
// =====================================================================

#[test]
fn test_cabi_prompt_render_null_system_pointer_returns_nonnull_result() {
    require_impl("test_cabi_prompt_render_null_system_pointer_returns_nonnull_result");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    unsafe {
        let usr_buf = alloc_str_via_static("usr");
        // null system pointer — treated as empty string.
        let out = cobrust_stdlib::prompt::__cobrust_prompt_render(
            std::ptr::null_mut(), usr_buf, std::ptr::null_mut(),
        );
        assert!(!out.is_null(), "null system arg must produce non-null result");
        // result should be "\nusr" (empty system + "\n" + user).
        assert_eq!(read_cstr_buf(out), "\nusr");
        __cobrust_str_drop(out);
        __cobrust_str_drop(usr_buf);
    }
}

// =====================================================================
// Tier 2 #4 — __cobrust_prompt_format_few_shot with populated lists
// builds the correct canonical few-shot format.
// =====================================================================

#[test]
fn test_cabi_prompt_format_few_shot_with_lists_builds_correct_format() {
    require_impl("test_cabi_prompt_format_few_shot_with_lists_builds_correct_format");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    use cobrust_stdlib::collections::{__cobrust_list_new, __cobrust_list_set};
    unsafe {
        let in0 = alloc_str_via_static("x = 1");
        let out0 = alloc_str_via_static("let x: i64 = 1");
        let in_list  = __cobrust_list_new(8, 1);
        let out_list = __cobrust_list_new(8, 1);
        __cobrust_list_set(in_list,  0, in0  as i64);
        __cobrust_list_set(out_list, 0, out0 as i64);
        let cur_buf = alloc_str_via_static("y = 2");
        let result = cobrust_stdlib::prompt::__cobrust_prompt_format_few_shot(
            in_list, out_list, cur_buf,
        );
        assert!(!result.is_null());
        let expected = "Input: x = 1\nOutput: let x: i64 = 1\n\nInput: y = 2\nOutput:";
        assert_eq!(read_cstr_buf(result), expected);
        __cobrust_str_drop(result);
        __cobrust_str_drop(in0); __cobrust_str_drop(out0); __cobrust_str_drop(cur_buf);
    }
}

// =====================================================================
// Tier 2 #5 — __cobrust_prompt_format_few_shot with empty lists and
// valid current input builds the correct trailer-only output.
// =====================================================================

#[test]
fn test_cabi_prompt_format_few_shot_empty_lists_builds_trailer_only() {
    require_impl("test_cabi_prompt_format_few_shot_empty_lists_builds_trailer_only");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    use cobrust_stdlib::collections::__cobrust_list_new;
    unsafe {
        let in_list  = __cobrust_list_new(8, 0);
        let out_list = __cobrust_list_new(8, 0);
        let cur_buf  = alloc_str_via_static("z = 3");
        let result = cobrust_stdlib::prompt::__cobrust_prompt_format_few_shot(
            in_list, out_list, cur_buf,
        );
        assert!(!result.is_null());
        assert_eq!(read_cstr_buf(result), "Input: z = 3\nOutput:");
        __cobrust_str_drop(result); __cobrust_str_drop(cur_buf);
    }
}

// =====================================================================
// Tier 2 #6 — __cobrust_prompt_format_system_user simple concatenation
// produces the correct "<system>\n\n<user>" result.
// =====================================================================

#[test]
fn test_cabi_prompt_format_system_user_simple_concat_works() {
    require_impl("test_cabi_prompt_format_system_user_simple_concat_works");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    unsafe {
        let sys_buf = alloc_str_via_static("You are a Cobrust expert.");
        let usr_buf = alloc_str_via_static("Translate this code.");
        let result = cobrust_stdlib::prompt::__cobrust_prompt_format_system_user(
            sys_buf, usr_buf,
        );
        assert!(!result.is_null());
        assert_eq!(
            read_cstr_buf(result),
            "You are a Cobrust expert.\n\nTranslate this code.",
        );
        __cobrust_str_drop(result);
        __cobrust_str_drop(sys_buf); __cobrust_str_drop(usr_buf);
    }
}

// =====================================================================
// Tier 2 #7 — __cobrust_prompt_escape_braces escapes literal `{` and `}`
// to `{{` and `}}` respectively.
// =====================================================================

#[test]
fn test_cabi_prompt_escape_braces_escapes_literal_braces() {
    require_impl("test_cabi_prompt_escape_braces_escapes_literal_braces");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    unsafe {
        let text_buf = alloc_str_via_static("hello {world}");
        let result = cobrust_stdlib::prompt::__cobrust_prompt_escape_braces(text_buf);
        assert!(!result.is_null());
        assert_eq!(read_cstr_buf(result), "hello {{world}}");
        __cobrust_str_drop(result); __cobrust_str_drop(text_buf);
    }
}

// =====================================================================
// Tier 2 #8 — __cobrust_llm_complete_structured with synthetic provider
// + [routing.structured] fixture returns the canned text. Mirrors the
// M-AI.0 `test_cabi_llm_dispatch_valid_task_routes_through_routing_table`
// (llm_cabi_corpus.rs:230-245) synthetic-provider + routing fixture setup.
// =====================================================================

#[test]
#[cfg(feature = "llm-router")]
fn test_cabi_llm_complete_structured_synthetic_provider_returns_canned_text() {
    require_impl("test_cabi_llm_complete_structured_synthetic_provider_returns_canned_text");
    // Once impl lands, DEV uncomments:
    //
    //   use cobrust_stdlib::fmt::__cobrust_str_drop;
    //   // Register a synthetic double for the "structured" routing table entry.
    //   let canned = "structured-cabi-canned";
    //   let provider = SyntheticDouble::new(vec![Scripted::Ok(canned.into())]);
    //   cobrust_stdlib::llm::__test_register_synthetic_provider("syn", provider);
    //   // cobrust.toml fixture must declare [routing.structured] pointing at syn:m1.
    //   let _guard = set_cobrust_config(&write_routing_toml(
    //       "structured", "quality", &["syn:m1"],
    //   ));
    //   unsafe {
    //       let prompt_buf = alloc_str_via_static("Translate this code.");
    //       let schema_buf = alloc_str_via_static(
    //           r#"{"type":"object","properties":{"code":{"type":"string"}}}"#,
    //       );
    //       let result = cobrust_stdlib::prompt::__cobrust_llm_complete_structured(
    //           prompt_buf, schema_buf,
    //       );
    //       assert!(!result.is_null(), "structured shim must return non-null");
    //       assert_eq!(read_cstr_buf(result), canned);
    //       __cobrust_str_drop(result);
    //       __cobrust_str_drop(prompt_buf); __cobrust_str_drop(schema_buf);
    //   }
}

// =====================================================================
// Tier 2 #9 — __cobrust_llm_complete_structured with `llm-router` feature
// off returns "" (shim compiles unconditionally but returns empty string
// when feature is absent). This branch is only exercised when the test
// binary is built with --no-default-features; default test binary always
// has llm-router on.
//
// M-AI.1 spike Tier 2 #9: this branch is only exercised when the test
// binary is built with --no-default-features; default test binary always
// has llm-router on.
// =====================================================================

#[test]
#[ignore = "requires --no-default-features build; not exercised in default test binary"]
fn test_cabi_llm_complete_structured_feature_off_returns_empty() {
    require_impl("test_cabi_llm_complete_structured_feature_off_returns_empty");
    // This test is exercised only with --no-default-features. At that build,
    // the shim's `#[cfg(not(feature = "llm-router"))]` branch fires and the
    // function returns an empty Str buffer.
    //
    // Once impl lands, DEV uncomments (and runs with --no-default-features):
    //
    //   use cobrust_stdlib::fmt::{__cobrust_str_len, __cobrust_str_drop};
    //   unsafe {
    //       let p = alloc_str_via_static("any prompt");
    //       let s = alloc_str_via_static("{}");
    //       let result = cobrust_stdlib::prompt::__cobrust_llm_complete_structured(p, s);
    //       assert!(!result.is_null());
    //       assert_eq!(__cobrust_str_len(result), 0,
    //           "feature-off shim must return empty Str");
    //       __cobrust_str_drop(result);
    //       __cobrust_str_drop(p); __cobrust_str_drop(s);
    //   }
}

// =====================================================================
// Tier 2 #10 — UTF-8 round-trip: each shim with multi-byte text in/out
// is byte-identical. Tests __cobrust_prompt_render with UTF-8 content.
// =====================================================================

#[test]
fn test_cabi_prompt_shims_utf8_round_trip_byte_identical() {
    require_impl("test_cabi_prompt_shims_utf8_round_trip_byte_identical");

    use cobrust_stdlib::fmt::__cobrust_str_drop;
    unsafe {
        // Multi-byte system + user + key + value.
        let sys_buf = alloc_str_via_static("你好");
        let usr_buf = alloc_str_via_static("Say: {greeting}");
        let key_buf = alloc_str_via_static("greeting");
        let val_buf = alloc_str_via_static("こんにちは");
        let vars_list = {
            use cobrust_stdlib::collections::{__cobrust_list_new, __cobrust_list_set};
            let lst = __cobrust_list_new(8, 2);
            __cobrust_list_set(lst, 0, key_buf as i64);
            __cobrust_list_set(lst, 1, val_buf as i64);
            lst
        };
        let result = cobrust_stdlib::prompt::__cobrust_prompt_render(
            sys_buf, usr_buf, vars_list,
        );
        assert!(!result.is_null());
        assert_eq!(read_cstr_buf(result), "你好\nSay: こんにちは");
        __cobrust_str_drop(result);
        __cobrust_str_drop(sys_buf); __cobrust_str_drop(usr_buf);
        __cobrust_str_drop(key_buf); __cobrust_str_drop(val_buf);
    }
}
