//! `cobrust.prompt` — source-level binding to prompt-composition
//! primitives.
//!
//! ADR-0048 §"M-AI.1 — cobrust.prompt" pins this module; refined by
//! `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` (SHA TBD) and
//! ratified by `[P10-ALPHA-PHASE-3-RATIFY]`.
//!
//! Five source-level intrinsics live in `PRELUDE`:
//!
//! - `prompt_render(system, user, vars) -> str`
//! - `prompt_format_few_shot(examples_in, examples_out, current_input) -> str`
//! - `prompt_format_system_user(system, user) -> str`
//! - `prompt_escape_braces(text) -> str`
//! - `llm_complete_structured(prompt, schema_json) -> str`
//!   (gated by `#[cfg(feature = "llm-router")]` via the M-AI.0 path)
//!
//! Decision references: see spike §Decision 1 (flat-fn), §Decision 3
//! (list[str] even-indexed vars), §Decision 4 (`{k}` interpolation +
//! `{{` `}}` escapes), §Decision 5 (canonical few-shot format),
//! §Decision 6 (structured wraps `llm_dispatch`), §Decision 7
//! (empty-on-failure error surface).

use std::collections::BTreeMap;

// =====================================================================
// Rust-side blocking helpers — unit-testable counterparts of the C-ABI
// shims. Decision 7: failures collapse to empty String.
// =====================================================================

/// `prompt_render` — variable interpolation pass per Decision 4.
///
/// Builds a `BTreeMap` from even-indexed `vars` pairs (dropping trailing
/// odd key silently per Decision 3 + 7). Performs a single-pass
/// substitution of `{key}` placeholders in `format!("{system}\n{user}")`.
/// `{{` becomes `{`, `}}` becomes `}` (escape mechanism per Decision 4).
/// Unknown keys remain as literal `{key}` text.
#[must_use]
pub fn prompt_render_helper(system: &str, user: &str, vars: &[String]) -> String {
    // Build BTreeMap from even-indexed pairs. Drop trailing odd key
    // silently (Decision 3 + 7). Later same-key bindings override.
    let mut map: BTreeMap<&str, &str> = BTreeMap::new();
    let mut i = 0;
    while i + 1 < vars.len() {
        map.insert(vars[i].as_str(), vars[i + 1].as_str());
        i += 2;
    }

    let combined = format!("{system}\n{user}");
    let mut out = String::with_capacity(combined.len());
    let bytes = combined.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' if idx + 1 < bytes.len() && bytes[idx + 1] == b'{' => {
                out.push('{');
                idx += 2;
            }
            b'}' if idx + 1 < bytes.len() && bytes[idx + 1] == b'}' => {
                out.push('}');
                idx += 2;
            }
            b'{' => {
                // Find matching `}`.
                let start = idx + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'}' {
                    end += 1;
                }
                if end >= bytes.len() {
                    // Unterminated `{` — emit literal rest.
                    out.push_str(&combined[idx..]);
                    break;
                }
                let key = &combined[start..end];
                if let Some(v) = map.get(key) {
                    out.push_str(v);
                } else {
                    // Unknown key — keep literal (Decision 4).
                    out.push_str(&combined[idx..=end]);
                }
                idx = end + 1;
            }
            _ => {
                // Push char (handle multi-byte via str slicing).
                let c_start = idx;
                let ch = combined[c_start..].chars().next().unwrap_or('\0');
                out.push(ch);
                idx += ch.len_utf8();
            }
        }
    }
    out
}

/// `prompt_format_few_shot` — canonical format per Decision 5.
///
/// Renders `"Input: <in_i>\nOutput: <out_i>\n\n"` for each pair
/// (truncating to the shorter list per Decision 5 + 7), then appends
/// `"Input: <current_input>\nOutput:"` trailer (no trailing newline).
/// Empty examples lists produce just the trailer.
#[must_use]
pub fn prompt_format_few_shot_helper(
    examples_in: &[String],
    examples_out: &[String],
    current_input: &str,
) -> String {
    let n = examples_in.len().min(examples_out.len());
    let mut out = String::new();
    for i in 0..n {
        out.push_str("Input: ");
        out.push_str(&examples_in[i]);
        out.push('\n');
        out.push_str("Output: ");
        out.push_str(&examples_out[i]);
        out.push_str("\n\n");
    }
    out.push_str("Input: ");
    out.push_str(current_input);
    out.push_str("\nOutput:");
    out
}

/// `prompt_format_system_user` — simple system+user concatenator.
///
/// Returns `"<system>\n\n<user>"` without variable interpolation.
/// Always succeeds (Decision 7: cannot fail).
#[must_use]
pub fn prompt_format_system_user_helper(system: &str, user: &str) -> String {
    format!("{system}\n\n{user}")
}

/// `prompt_escape_braces` — escape `{` and `}` literals.
///
/// Each `{` becomes `{{`, each `}` becomes `}}`. Symmetric to
/// Python's `str.replace`. Always succeeds (Decision 7).
#[must_use]
pub fn prompt_escape_braces_helper(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            _ => out.push(ch),
        }
    }
    out
}

/// `llm_complete_structured` — only available with `llm-router` feature.
///
/// Augments `prompt` with a structured-output instruction, then routes
/// through `llm_dispatch(task="structured", prompt=augmented)`.
/// Returns the raw response text; caller parses JSON themselves.
/// Decision 6 + 7: failure returns `""`.
#[cfg(feature = "llm-router")]
#[must_use]
pub fn llm_complete_structured_helper(prompt: &str, schema_json: &str) -> String {
    let augmented = format!(
        "{prompt}\n\nRespond with valid JSON matching this schema:\n{schema_json}"
    );
    crate::llm::llm_dispatch_blocking("structured", &augmented)
}

// =====================================================================
// Internal ABI helpers — mirrors M-AI.0 `llm.rs` helpers.
// =====================================================================

/// Read a heap `Str` pointer as a `String`. Tolerates null + empty.
/// Mirrors the M-AI.0 `read_str_buf` helper in `llm.rs`.
///
/// # Safety
///
/// `buf` must be either null or a valid `Str` buffer pointer produced
/// by `__cobrust_str_new` and friends, valid until the corresponding
/// `__cobrust_str_drop`.
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    // SAFETY: caller-attestation per `# Safety` clause. `buf` is a valid
    // Str buffer pointer produced by `__cobrust_str_new` (and push helpers)
    // or by codegen's `materialize_str_data` path.
    unsafe {
        let ptr = crate::fmt::__cobrust_str_ptr(buf);
        let len = crate::fmt::__cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

/// Allocate a heap `Str` buffer carrying `s`. Mirrors M-AI.0
/// `alloc_str_buffer` in `llm.rs`.
fn alloc_str_buffer(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` returns a valid buffer pointer that
    // we immediately populate via `__cobrust_str_push_static`. Both
    // contracts are satisfied — empty strings produce an empty buffer.
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// Read a `list[str]` heap pointer into a `Vec<String>`. Each slot is
/// an i64 storing a heap-Str pointer per the `__cobrust_argv` /
/// `__cobrust_llm_stream` precedent.
///
/// # Safety
///
/// `list_ptr` must be either null or a valid List pointer produced by
/// `__cobrust_list_new`, valid until the corresponding `__cobrust_list_drop`.
unsafe fn read_list_of_str(list_ptr: *mut u8) -> Vec<String> {
    if list_ptr.is_null() {
        return Vec::new();
    }
    // SAFETY: caller-attestation per `# Safety` clause.
    unsafe {
        let len = crate::collections::__cobrust_list_len(list_ptr);
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..len {
            // SAFETY: `__cobrust_list_get` returns the i64 slot which
            // stores a heap-Str pointer cast to i64 per the argv ABI.
            let elem = crate::collections::__cobrust_list_get(list_ptr, i) as *mut u8;
            out.push(read_str_buf(elem));
        }
        out
    }
}

// =====================================================================
// C-ABI shims (codegen targets these via the intrinsic-rewrite pass)
// =====================================================================

/// C-ABI shim for source-level `prompt_render(system, user, vars) -> str`.
///
/// # Safety
///
/// Each pointer must be either null (signals empty string/list) or a valid
/// heap-pointer produced by the Cobrust runtime (`__cobrust_str_new` for
/// Str args; `__cobrust_list_new` for the list arg). The returned pointer
/// is heap-owned; caller must eventually invoke `__cobrust_str_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_render(
    system: *mut u8,
    user: *mut u8,
    vars: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let s = unsafe { read_str_buf(system) };
    // SAFETY: same.
    let u = unsafe { read_str_buf(user) };
    // SAFETY: `vars` is either null (treated as empty list) or a valid
    // List pointer from `__cobrust_list_new`.
    let vs = unsafe { read_list_of_str(vars) };
    let result = prompt_render_helper(&s, &u, &vs);
    alloc_str_buffer(&result)
}

/// C-ABI shim for source-level
/// `prompt_format_few_shot(examples_in, examples_out, current_input) -> str`.
///
/// # Safety
///
/// Same contract as [`__cobrust_prompt_render`] for list + str pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_format_few_shot(
    examples_in: *mut u8,
    examples_out: *mut u8,
    current_input: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let xin = unsafe { read_list_of_str(examples_in) };
    // SAFETY: same.
    let xout = unsafe { read_list_of_str(examples_out) };
    // SAFETY: `current_input` is a valid Str pointer or null.
    let cur = unsafe { read_str_buf(current_input) };
    let result = prompt_format_few_shot_helper(&xin, &xout, &cur);
    alloc_str_buffer(&result)
}

/// C-ABI shim for source-level `prompt_format_system_user(system, user) -> str`.
///
/// # Safety
///
/// Same contract as [`__cobrust_prompt_render`] for Str pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_format_system_user(
    system: *mut u8,
    user: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let s = unsafe { read_str_buf(system) };
    // SAFETY: same.
    let u = unsafe { read_str_buf(user) };
    alloc_str_buffer(&prompt_format_system_user_helper(&s, &u))
}

/// C-ABI shim for source-level `prompt_escape_braces(text) -> str`.
///
/// # Safety
///
/// Same contract as [`__cobrust_prompt_render`] for Str pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_escape_braces(text: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let t = unsafe { read_str_buf(text) };
    alloc_str_buffer(&prompt_escape_braces_helper(&t))
}

/// C-ABI shim for source-level `llm_complete_structured(prompt, schema_json) -> str`.
///
/// Gated by `llm-router` feature; if the feature is off, the shim
/// still compiles but returns empty (preserves PRELUDE callsites at
/// no-feature builds). Decision 7: failure returns `""`.
///
/// # Safety
///
/// Same contract as [`__cobrust_prompt_render`] for Str pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_complete_structured(
    prompt: *mut u8,
    schema_json: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let p = unsafe { read_str_buf(prompt) };
    // SAFETY: same.
    let s = unsafe { read_str_buf(schema_json) };
    #[cfg(feature = "llm-router")]
    let result = alloc_str_buffer(&llm_complete_structured_helper(&p, &s));
    #[cfg(not(feature = "llm-router"))]
    let result = {
        let _ = (p, s);
        alloc_str_buffer("")
    };
    result
}
