//! `std.re` — regular-expression processing (`import re`).
//!
//! ADR-0084 pins this surface. The four stateless functions are the
//! clean subset of CPython's `re` module — the ones that take only
//! strings and return a string / list[str] / bool, with NO Match-object
//! state (the `.group()` form is a documented follow-up):
//!
//! - `re.sub(pattern, repl, s) -> str` — replace ALL non-overlapping
//!   matches (`re.sub("a", "X", "banana") == "bXnXnX"`).
//! - `re.findall(pattern, s) -> list[str]` — ALL non-overlapping
//!   matches as a list of the matched strings
//!   (`re.findall("[0-9]+", "a1b22c333") == ["1", "22", "333"]`; `[]`
//!   on no match).
//! - `re.match(pattern, s) -> bool` — does `pattern` match at the START
//!   of `s` (Python `re.match` is START-anchored;
//!   `re.match("bc", "abc") == False`).
//! - `re.search(pattern, s) -> bool` — does `pattern` match ANYWHERE in
//!   `s` (`re.search("bc", "abc") == True`).
//!
//! **Backing engine** (ADR-0084 §"Backing"): the `regex` crate (1.x).
//! Its flavor matches Python `re` for the common patterns
//! (character classes, quantifiers, alternation, anchors, groups) but
//! has NO backreferences and NO lookaround (linear-time guarantee). The
//! `@py_compat` tier is therefore `Semantic` — a documented divergence,
//! not Strict parity.
//!
//! **Invalid pattern** (ADR-0084 §"Invalid pattern"): a malformed
//! runtime pattern (e.g. `"["`) makes `regex::Regex::new` return `Err`.
//! The shim turns that into a CLEAN process trap via the stdlib
//! `__cobrust_panic` (non-zero exit) — NEVER a silent no-match and
//! NEVER a Rust unwind across the C-ABI (CPython raises `re.error`;
//! Cobrust traps). A compile-time check for a LITERAL pattern is a §2.5
//! follow-up noted in the ADR.
//!
//! **ABI** — assembled entirely from shipped mechanisms (no new ABI is
//! invented):
//!
//! - Str ARGS (pattern / repl / s) read via the f-string-buffer ABI
//!   (`__cobrust_str_ptr` / `__cobrust_str_len`), mirroring
//!   `string::str_buf_as_str_local` (`string.rs:205`).
//! - Str RETURN (`re.sub`) allocated via `__cobrust_str_new` +
//!   `__cobrust_str_push_static`, mirroring
//!   `string::alloc_str_buffer_local` (`string.rs:226`).
//! - list[str] RETURN (`re.findall`) minted via `__cobrust_list_new` +
//!   `__cobrust_list_set` storing one heap-`Str` pointer per slot,
//!   mirroring `string::__cobrust_str_split` (`string.rs:257`) +
//!   `llm::__cobrust_llm_stream` (`llm.rs:466`).
//! - bool RETURN (`re.match` / `re.search`) via the Rust C-ABI
//!   `-> bool` (LLVM `i1`), mirroring `math::__cobrust_math_isnan`.

// =====================================================================
// Rust-side helpers (testable without the C-ABI).
// =====================================================================

/// Compile `pattern` or trap with a clean process abort. The single
/// fallible point of the module: an invalid runtime pattern
/// (`regex::Regex::new` → `Err`) becomes a `__cobrust_panic`
/// (non-zero exit), mirroring `cobrust-coil`'s `coil_panic` discipline
/// (a domain error is a clean trap, NEVER a silent wrong value and
/// NEVER a Rust unwind across the C-ABI). CPython raises `re.error`.
fn compile_or_trap(pattern: &str) -> regex::Regex {
    match regex::Regex::new(pattern) {
        Ok(re) => re,
        Err(e) => {
            let msg = format!("re: invalid pattern {pattern:?}: {e}");
            // SAFETY: `msg` is a valid UTF-8 `&str`; `__cobrust_panic`
            // reads exactly `msg.len()` bytes at `msg.as_ptr()` and
            // diverges (mirrors `cobrust-coil::cabi::coil_panic`).
            unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
        }
    }
}

/// `re.sub(pattern, repl, s)` — replace ALL non-overlapping matches of
/// `pattern` in `s` with `repl`. Traps on an invalid pattern.
fn sub(pattern: &str, repl: &str, s: &str) -> String {
    compile_or_trap(pattern).replace_all(s, repl).into_owned()
}

/// `re.findall(pattern, s)` — every non-overlapping FULL match as an
/// owned `String`. `[]` on no match. Traps on an invalid pattern.
///
/// ADR-0084 §"findall semantics": this returns the FULL matches (the
/// no-group form, which equals CPython exactly). CPython's group-capture
/// behavior (1 group → that group's text; >1 → tuples) is the documented
/// deferral — a grouped pattern returns the FULL match here, a Semantic
/// divergence noted in the ADR + docs.
fn findall(pattern: &str, s: &str) -> Vec<String> {
    compile_or_trap(pattern)
        .find_iter(s)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// `re.match(pattern, s)` — START-anchored (CPython `re.match`). True
/// iff `pattern` matches at index 0. Implemented as `find().start() ==
/// 0` so the caller's `pattern` is NOT mutated (anchoring by prepending
/// `\A` would corrupt a pattern that already begins with an anchor or a
/// group). Traps on an invalid pattern.
fn is_match_start(pattern: &str, s: &str) -> bool {
    compile_or_trap(pattern)
        .find(s)
        .is_some_and(|m| m.start() == 0)
}

/// `re.search(pattern, s)` — match ANYWHERE (CPython `re.search`). The
/// load-bearing distinction from [`is_match_start`]: `search("bc",
/// "abc")` is True but `match("bc", "abc")` is False. Traps on an
/// invalid pattern.
fn is_search(pattern: &str, s: &str) -> bool {
    compile_or_trap(pattern).is_match(s)
}

// =====================================================================
// C-ABI shims — the `__cobrust_re_*` symbols codegen declares + calls.
//
// Each shim reads its Str args via `str_buf_as_str_local`, calls the
// Rust helper above, and returns a fresh Str / list[str] / bool. The
// shim does NOT drop its Str args (the caller's MIR drop pass owns the
// input lifetime, ADR-0050e §"shim-drop-owner"); fresh return buffers
// are owned by the call-site drop schedule.
// =====================================================================

/// Read a Str buffer pointer as a `&str`. Tolerates null + empty.
/// Mirrors `string::str_buf_as_str_local` (`string.rs:205`).
///
/// # Safety
///
/// `buf` must be a non-null pointer to a `StringBuffer` produced by
/// `__cobrust_str_new` (or `__cobrust_str_clone`), or null.
unsafe fn str_buf_as_str_local<'a>(buf: *mut u8) -> &'a str {
    if buf.is_null() {
        return "";
    }
    // SAFETY: caller-attestation per the `# Safety` clause.
    let len = unsafe { crate::fmt::__cobrust_str_len(buf) } as usize;
    if len == 0 {
        return "";
    }
    // SAFETY: same.
    let ptr = unsafe { crate::fmt::__cobrust_str_ptr(buf) };
    if ptr.is_null() {
        return "";
    }
    // SAFETY: `ptr` points to `len` bytes of UTF-8 maintained by every
    // StringBuffer write path.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).unwrap_or("")
}

/// Allocate a fresh heap StringBuffer carrying `s`'s bytes. Mirrors
/// `string::alloc_str_buffer_local` (`string.rs:226`).
fn alloc_str_buffer_local(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` returns a valid empty buffer pointer;
    // `__cobrust_str_push_static` is safe for valid (ptr, len) pairs.
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// C-ABI shim for `re.sub(pattern, repl, s) -> str`. Returns a fresh
/// heap `Str` buffer (the call-site drop schedule frees it). Traps on an
/// invalid pattern (ADR-0084 §"Invalid pattern").
///
/// # Safety
///
/// `pattern`, `repl`, `s` must each be valid `Str` buffer pointers
/// produced by `__cobrust_str_new` (or null). The returned pointer is
/// heap-owned; the caller's MIR drop pass invokes `__cobrust_str_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_re_sub(pattern: *mut u8, repl: *mut u8, s: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let pat = unsafe { str_buf_as_str_local(pattern) };
    // SAFETY: same.
    let rep = unsafe { str_buf_as_str_local(repl) };
    // SAFETY: same.
    let hay = unsafe { str_buf_as_str_local(s) };
    alloc_str_buffer_local(&sub(pat, rep, hay))
}

/// C-ABI shim for `re.findall(pattern, s) -> list[str]`. Mints a heap
/// `List<i64>` whose i64 slots store owned `Str` pointers (one per FULL
/// match). The codegen `Ty::List(Str)` drop schedule frees each element
/// + the list. Empty list on no match. Traps on an invalid pattern.
///
/// # Safety
///
/// `pattern`, `s` must each be valid `Str` buffer pointers or null. The
/// returned list must be passed to `__cobrust_list_drop_elems(list,
/// __cobrust_str_drop)` exactly once (codegen does this automatically).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_re_findall(pattern: *mut u8, s: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let pat = unsafe { str_buf_as_str_local(pattern) };
    // SAFETY: same.
    let hay = unsafe { str_buf_as_str_local(s) };
    let matches = findall(pat, hay);
    // SAFETY: `__cobrust_list_new(8, len)` returns a valid `List<i64>`
    // pointer with `len` zeroed slots; `__cobrust_list_set` writes the
    // i64 slot at index `i` with a `Str` pointer from
    // `alloc_str_buffer_local`. Mirrors `string::__cobrust_str_split`.
    unsafe {
        let list = crate::collections::__cobrust_list_new(8, matches.len() as i64);
        for (i, m) in matches.iter().enumerate() {
            let buf = alloc_str_buffer_local(m);
            crate::collections::__cobrust_list_set(list, i as i64, buf as i64);
        }
        list
    }
}

/// C-ABI shim for `re.match(pattern, s) -> bool` — START-anchored
/// (CPython `re.match`). The `-> bool` lowers to an LLVM `i1`, mirroring
/// `math::__cobrust_math_isnan`. Traps on an invalid pattern.
///
/// # Safety
///
/// `pattern`, `s` must each be valid `Str` buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_re_match(pattern: *mut u8, s: *mut u8) -> bool {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let pat = unsafe { str_buf_as_str_local(pattern) };
    // SAFETY: same.
    let hay = unsafe { str_buf_as_str_local(s) };
    is_match_start(pat, hay)
}

/// C-ABI shim for `re.search(pattern, s) -> bool` — match ANYWHERE
/// (CPython `re.search`). The load-bearing distinction from
/// `__cobrust_re_match`. Traps on an invalid pattern.
///
/// # Safety
///
/// `pattern`, `s` must each be valid `Str` buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_re_search(pattern: *mut u8, s: *mut u8) -> bool {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let pat = unsafe { str_buf_as_str_local(pattern) };
    // SAFETY: same.
    let hay = unsafe { str_buf_as_str_local(s) };
    is_search(pat, hay)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- re.sub: replace ALL non-overlapping matches --------------------
    // Oracle (/opt/homebrew/bin/python3.11): re.sub('a','X','banana')
    // == 'bXnXnX'.

    #[test]
    fn sub_replaces_all_occurrences() {
        assert_eq!(sub("a", "X", "banana"), "bXnXnX");
    }

    #[test]
    fn sub_replaces_class_matches() {
        // re.sub('[0-9]+', '#', 'a1b22c333') == 'a#b#c#'.
        assert_eq!(sub("[0-9]+", "#", "a1b22c333"), "a#b#c#");
    }

    #[test]
    fn sub_no_match_is_identity() {
        // re.sub('z', 'X', 'banana') == 'banana'.
        assert_eq!(sub("z", "X", "banana"), "banana");
    }

    // -- re.findall: ALL non-overlapping FULL matches -------------------
    // Oracle: re.findall('[0-9]+','a1b22c333') == ['1','22','333'];
    // re.findall('[0-9]+','abc') == [].

    #[test]
    fn findall_returns_all_numeric_runs() {
        assert_eq!(findall("[0-9]+", "a1b22c333"), vec!["1", "22", "333"]);
    }

    #[test]
    fn findall_empty_on_no_match() {
        assert!(findall("[0-9]+", "abc").is_empty());
    }

    #[test]
    fn findall_single_char_repeats() {
        // re.findall('a','banana') == ['a','a','a'].
        assert_eq!(findall("a", "banana"), vec!["a", "a", "a"]);
    }

    // -- re.match vs re.search: the anchor is load-bearing --------------
    // Oracle: re.match('bc','abc') is None (False), re.search('bc','abc')
    // is a Match (True). THIS is the distinguishing test.

    #[test]
    fn match_is_start_anchored_search_is_anywhere() {
        // The load-bearing pair: same pattern + haystack, different verb.
        assert!(!is_match_start("bc", "abc"));
        assert!(is_search("bc", "abc"));
    }

    #[test]
    fn match_true_at_start() {
        // re.match('ab','abc') == True.
        assert!(is_match_start("ab", "abc"));
    }

    #[test]
    fn match_vs_search_on_class() {
        // re.match('[0-9]+','123abc') == True (starts with digits);
        // re.match('[0-9]+','abc123') == False (digits not at start);
        // re.search('[0-9]+','abc123') == True (digits later).
        assert!(is_match_start("[0-9]+", "123abc"));
        assert!(!is_match_start("[0-9]+", "abc123"));
        assert!(is_search("[0-9]+", "abc123"));
    }

    // -- invalid-pattern compile (Rust-side `Err`) ----------------------
    // The shim turns this `Err` into a `__cobrust_panic`; here we assert
    // the underlying `regex::Regex::new` rejects it (the trap is exercised
    // end-to-end by the `.cb` e2e's non-zero-exit test).

    #[test]
    fn invalid_pattern_is_compile_err() {
        // Build the malformed pattern at RUNTIME (a bare `"["` literal
        // trips clippy's `invalid_regex` static lint — which is, fittingly,
        // exactly the literal-pattern compile-check ADR-0084 defers; here we
        // exercise the RUNTIME `Err` path the shim turns into a trap).
        let open_bracket = String::from("[");
        assert!(regex::Regex::new(&open_bracket).is_err());
    }
}
