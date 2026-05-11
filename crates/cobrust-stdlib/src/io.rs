//! `std.io` — print / println / read_line / read_file / write_file
//! plus stdin / stdout / stderr handles.
//!
//! ADR-0025 §"Public surface" pins the API. Constitution §2.2 binds
//! `Result<T, E>` as the default error path — every fallible op
//! returns `Result<_, Error>`, never panics on user-driven failure.

use std::io::{BufRead, Read, Write};

use crate::runtime::Error;

// =====================================================================
// print / println
// =====================================================================

/// Write `s` to stdout without a trailing newline. Flushes
/// stdout to ensure the bytes are visible before any subsequent
/// stderr write or process exit.
///
/// Per ADR-0025 §"Codegen amendments", codegen-emitted `print(s)`
/// callsites lower to a C-ABI call to [`__cobrust_print`], which
/// shims into this function.
pub fn print(s: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(s.as_bytes());
    let _ = stdout.flush();
}

/// Like [`print`] but appends `\n`.
pub fn println(s: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(s.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

// =====================================================================
// C ABI shims — what codegen-emitted calls land on
// =====================================================================

/// C-ABI shim for `std.io.print`. Codegen emits a call here when
/// it lowers a Cobrust `print(s)` callsite. Per ADR-0025 §D the
/// argument is `(*const u8, usize)` — a pointer to the UTF-8
/// payload + length in bytes.
///
/// # Safety
///
/// `ptr` must be a valid pointer to `len` bytes of UTF-8-encoded
/// text. The text need not be nul-terminated. `ptr` must outlive
/// the call. Codegen always emits this call with a `.rodata`
/// pointer + a compile-time-known length, satisfying the contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_print(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // SAFETY: caller-attestation per the `# Safety` clause.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if let Ok(s) = std::str::from_utf8(bytes) {
        print(s);
    }
}

/// C-ABI shim for `std.io.println` — the M11 lift of M10's
/// `__cobrust_println_static`. Codegen emits a call here when it
/// lowers a Cobrust `println(s)` callsite.
///
/// # Safety
///
/// Same as [`__cobrust_print`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_println(ptr: *const u8, len: usize) {
    if ptr.is_null() {
        // Empty input still prints a newline — matches Python's
        // `print()` with no args.
        println("");
        return;
    }
    // SAFETY: caller-attestation per the `# Safety` clause.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if let Ok(s) = std::str::from_utf8(bytes) {
        println(s);
    }
}

/// C-ABI shim for `print_int(n: i64)` — emitted by the M11.1 print-int
/// intrinsic rewrite when a `print_int(v)` callsite is lowered.
/// Formats `v` as a decimal integer followed by a newline on stdout.
///
/// ADR-0030 §Decision step 5: required so `examples/fizzbuzz.cb` can
/// print bare integers in the `else` branch.
///
/// # Safety
///
/// No pointer argument — always safe to call.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_println_int(v: i64) {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{v}");
    let _ = stdout.flush();
}

// =====================================================================
// ADR-0044 W2 Phase 2 — source-level `input()` / `read_line()` plumbing
// =====================================================================
//
// Two Rust-side helpers parameterised over a `BufRead` reader so the
// behaviour is unit-testable without touching real stdin, plus three
// C-ABI shims that codegen lowers `input(prompt)` / `input_no_prompt()`
// / `read_line()` callsites into. The shims always read from
// `std::io::stdin()`; the helpers split out the reader so the test
// corpus (`crates/cobrust-stdlib/tests/io_input.rs`) can drive them
// from `std::io::Cursor<Vec<u8>>` deterministically.
//
// Per ADR-0044 Decision 5:
//   - `input(prompt)` strips a single trailing `\n`; `\r` is preserved.
//   - `read_line()` returns the line *with* the trailing `\n` (W2 cap;
//     Result-typed end state lands in ADR-0044a).
//
// Per Decision 4: UTF-8 lossy — invalid bytes become `U+FFFD`.

/// Read one line from `reader`, write `prompt` to stdout flushed first
/// (matching Python's `input(prompt)`), strip a single trailing `\n`
/// from the result, return UTF-8 lossy. EOF returns empty `String`.
///
/// Per ADR-0044 §"Implementation map", this is the unit-testable
/// helper that the `__cobrust_input` C-ABI shim wraps. Splitting the
/// reader as a parameter lets the test corpus drive deterministic
/// `Cursor<Vec<u8>>` inputs.
pub fn input_from<R: BufRead>(prompt: &str, reader: &mut R) -> String {
    if !prompt.is_empty() {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(prompt.as_bytes());
        let _ = stdout.flush();
    }
    let mut buf = Vec::new();
    let _ = reader.read_until(b'\n', &mut buf);
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Read one line from `reader`, **preserving** the trailing `\n` if
/// present. EOF returns empty `String`. UTF-8 lossy per Decision 4.
///
/// Per ADR-0044 §"Decision 5", `read_line()` preserves the newline so
/// downstream consumers can round-trip stdin to stdout byte-perfect
/// without re-injecting newlines. The Result-typed end state is
/// deferred to ADR-0044a (W2 Phase 2 scope cap).
pub fn read_line_from<R: BufRead>(reader: &mut R) -> String {
    let mut buf = Vec::new();
    let _ = reader.read_until(b'\n', &mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

// ---------- C-ABI shims (ADR-0044 §"New runtime C-ABI surface") -----

/// Heap-allocated Str pointer wrapper. Wraps an owned `String` in the
/// same shape as the f-string runtime's `StringBuffer` so codegen can
/// pass the result back into `__cobrust_str_len` / `__cobrust_str_ptr`
/// / `__cobrust_str_drop` interchangeably.
///
/// Constructs a fresh buffer via `__cobrust_str_new()`, appends `s` via
/// `__cobrust_str_push_static`, and returns the opaque pointer.
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

/// C-ABI shim for source-level `input(prompt: str) -> str`. Writes
/// `prompt` to stdout flushed, reads one line from stdin (stripping
/// the trailing `\n`), returns an owned Str pointer.
///
/// ADR-0044 §"New runtime C-ABI surface": codegen emits a call here
/// when the intrinsic-rewrite pass redirects `input(...)` callsites.
///
/// # Safety
///
/// `ptr` must be a valid pointer to `len` bytes of UTF-8-encoded
/// prompt text (or null + 0 for the no-prompt case). The text need
/// not be nul-terminated. Codegen emits this call with a `.rodata` or
/// f-string-buffer pointer + a compile-time-known length, satisfying
/// the contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_input(ptr: *const u8, len: usize) -> *mut u8 {
    let prompt: &str = if ptr.is_null() || len == 0 {
        ""
    } else {
        // SAFETY: caller-attestation per the `# Safety` clause.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        std::str::from_utf8(bytes).unwrap_or("")
    };
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let s = input_from(prompt, &mut lock);
    alloc_str_buffer(&s)
}

/// C-ABI shim for source-level `input_no_prompt() -> str`. Equivalent
/// to `__cobrust_input(NULL, 0)` — no prompt write, reads one line
/// from stdin and strips trailing `\n`. Returns an owned Str pointer.
///
/// # Safety
///
/// No pointer arguments — always safe to call. The returned Str must
/// be freed via `__cobrust_str_drop` when no longer needed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_input_no_prompt() -> *mut u8 {
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let s = input_from("", &mut lock);
    alloc_str_buffer(&s)
}

/// C-ABI shim for source-level `print(s: str)` when the argument is a
/// non-literal `str` (a heap-allocated buffer pointer produced by
/// `__cobrust_input`, `__cobrust_read_line`, `__cobrust_str_new` etc.).
/// Extracts the buffer's `(ptr, len)` pair via the f-string runtime's
/// `__cobrust_str_ptr` / `__cobrust_str_len` accessors and dispatches
/// to `__cobrust_println`. Buffer ownership is unchanged — codegen's
/// drop schedule still owns the eventual `__cobrust_str_drop`.
///
/// ADR-0044 W2 Phase 2: needed so `print(input(...))` / `print(s)`
/// where `s` came from `read_line()` round-trips end to end without
/// requiring full stdlib FnRef dispatch (which is M11.x scope).
///
/// # Safety
///
/// `buf` must be a pointer returned by `__cobrust_str_new` (or any
/// of the W2 shims that wrap it: `__cobrust_input`,
/// `__cobrust_input_no_prompt`, `__cobrust_read_line`) and not yet
/// dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_println_str_buf(buf: *mut u8) {
    if buf.is_null() {
        println("");
        return;
    }
    // SAFETY: caller-attestation per `# Safety` clause. Both
    // accessors are no-ops on null.
    unsafe {
        let ptr = crate::fmt::__cobrust_str_ptr(buf);
        let len = crate::fmt::__cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            println("");
            return;
        }
        // SAFETY: `__cobrust_str_ptr` returns a valid slice for
        // `len` bytes; the f-string runtime maintains UTF-8 validity.
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        if let Ok(s) = std::str::from_utf8(bytes) {
            println(s);
        }
    }
}

/// C-ABI shim for source-level `read_line() -> str` (W2 Phase 2 scope
/// cap per ADR-0044 Decision 1D — typed `Result[str, IoError]` deferred
/// to ADR-0044a). Reads one line from stdin **preserving** the
/// trailing `\n`; EOF returns an empty Str.
///
/// # Safety
///
/// No pointer arguments — always safe to call. The returned Str must
/// be freed via `__cobrust_str_drop` when no longer needed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_read_line() -> *mut u8 {
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let s = read_line_from(&mut lock);
    alloc_str_buffer(&s)
}

// =====================================================================
// read_line / read_file / write_file
// =====================================================================

/// Read a single line from stdin, including the trailing newline if
/// present. Returns an empty string on EOF.
pub fn read_line() -> Result<String, Error> {
    let stdin = std::io::stdin();
    let mut buf = String::new();
    let n = stdin.lock().read_line(&mut buf).map_err(Error::from)?;
    if n == 0 {
        return Ok(String::new());
    }
    Ok(buf)
}

/// Read the entire file at `path` as a UTF-8 string.
pub fn read_file(path: &str) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|e| Error::io(format!("{path}: {e}")))
}

/// Write `contents` to the file at `path`, creating or truncating.
pub fn write_file(path: &str, contents: &str) -> Result<(), Error> {
    std::fs::write(path, contents).map_err(|e| Error::io(format!("{path}: {e}")))
}

// =====================================================================
// Stream handles
// =====================================================================

/// Opaque newtype around `std::io::Stdin`. Cobrust source uses
/// `std.io.stdin().read_line()` rather than the free function; the
/// free [`read_line`] is the convenience wrapper.
pub struct Stdin {
    inner: std::io::Stdin,
}

impl Stdin {
    /// Read one line from the stream. Returns an empty string on
    /// EOF.
    pub fn read_line(&self) -> Result<String, Error> {
        let mut buf = String::new();
        let _ = self.inner.lock().read_line(&mut buf).map_err(Error::from)?;
        Ok(buf)
    }

    /// Read the entire stream until EOF.
    pub fn read_all(&self) -> Result<String, Error> {
        let mut buf = String::new();
        let _ = self
            .inner
            .lock()
            .read_to_string(&mut buf)
            .map_err(Error::from)?;
        Ok(buf)
    }
}

/// Returns a [`Stdin`] handle.
pub fn stdin() -> Stdin {
    Stdin {
        inner: std::io::stdin(),
    }
}

/// Opaque newtype around `std::io::Stdout`.
pub struct Stdout {
    inner: std::io::Stdout,
}

impl Stdout {
    /// Write `s` to stdout.
    pub fn write(&self, s: &str) -> Result<(), Error> {
        let mut g = self.inner.lock();
        g.write_all(s.as_bytes()).map_err(Error::from)?;
        g.flush().map_err(Error::from)
    }
}

/// Returns a [`Stdout`] handle.
pub fn stdout() -> Stdout {
    Stdout {
        inner: std::io::stdout(),
    }
}

/// Opaque newtype around `std::io::Stderr`.
pub struct Stderr {
    inner: std::io::Stderr,
}

impl Stderr {
    /// Write `s` to stderr.
    pub fn write(&self, s: &str) -> Result<(), Error> {
        let mut g = self.inner.lock();
        g.write_all(s.as_bytes()).map_err(Error::from)?;
        g.flush().map_err(Error::from)
    }
}

/// Returns a [`Stderr`] handle.
pub fn stderr() -> Stderr {
    Stderr {
        inner: std::io::stderr(),
    }
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::format_push_string,
    clippy::let_unit_value,
    clippy::ignored_unit_patterns,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::manual_is_multiple_of,
    clippy::redundant_closure_for_method_calls,
    clippy::match_same_arms,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn read_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.txt");
        write_file(path.to_str().unwrap(), "hello world").unwrap();
        let read = read_file(path.to_str().unwrap()).unwrap();
        assert_eq!(read, "hello world");
    }

    #[test]
    fn read_file_missing_yields_io_error() {
        let res = read_file("/nonexistent/path/cobrust-m11-test");
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.kind(), &crate::runtime::ErrorKind::Io);
    }

    #[test]
    fn write_file_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.txt");
        write_file(path.to_str().unwrap(), "AAAA").unwrap();
        write_file(path.to_str().unwrap(), "B").unwrap();
        let read = read_file(path.to_str().unwrap()).unwrap();
        assert_eq!(read, "B");
    }

    #[test]
    fn write_file_creates_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested.txt");
        write_file(path.to_str().unwrap(), "x").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn stdout_handle_writes_without_panic() {
        // Functional test: write doesn't panic. Real stdout capture
        // is in the integration tests (cli driver).
        let out = stdout();
        let r = out.write("");
        assert!(r.is_ok());
    }

    #[test]
    fn stderr_handle_writes_without_panic() {
        let err = stderr();
        let r = err.write("");
        assert!(r.is_ok());
    }

    #[test]
    fn print_does_not_panic() {
        print("");
    }

    #[test]
    fn println_does_not_panic() {
        println("");
    }

    #[test]
    fn cabi_print_handles_null() {
        // SAFETY: documented null-arg path.
        unsafe {
            __cobrust_print(std::ptr::null(), 0);
        }
    }

    #[test]
    fn cabi_println_handles_null() {
        // SAFETY: documented null-arg path.
        unsafe {
            __cobrust_println(std::ptr::null(), 0);
        }
    }

    #[test]
    fn cabi_print_with_data() {
        let bytes = b"hi";
        // SAFETY: bytes is a valid 2-byte slice.
        unsafe {
            __cobrust_print(bytes.as_ptr(), bytes.len());
        }
    }

    #[test]
    fn cabi_println_with_data() {
        let bytes = b"hi";
        // SAFETY: bytes is a valid 2-byte slice.
        unsafe {
            __cobrust_println(bytes.as_ptr(), bytes.len());
        }
    }
}
