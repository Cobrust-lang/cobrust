//! Runtime shim — heap allocator + main entry + error taxonomy.
//!
//! ADR-0025 §G binds:
//! - mimalloc as the default global allocator (feature
//!   `mimalloc-alloc`); `system-alloc` opts back to libc.
//! - C-ABI `__cobrust_main_shim` is the entry point codegen emits
//!   calls into; it captures argv into the env-args buffer and
//!   delegates to the user's `_cobrust_user_main`.
//! - `Error` is the unified runtime-error type. Constitution §2.2
//!   binds `Result<T, E>` as the default error path.

use std::sync::OnceLock;

// =====================================================================
// Global allocator
// =====================================================================

#[cfg(all(feature = "mimalloc-alloc", not(feature = "system-alloc")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// =====================================================================
// argv capture for std.env.args
// =====================================================================

/// Captured process argv. Set once at startup by [`__cobrust_main_shim`];
/// read by [`crate::env::args`].
pub(crate) static CAPTURED_ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Capture argv-style arguments into [`CAPTURED_ARGS`].
///
/// Idempotent — first writer wins; subsequent calls are no-ops. The
/// runtime shim calls this from C `main`. Tests can inject args via
/// [`set_test_args`].
pub fn capture_args(args: Vec<String>) {
    let _ = CAPTURED_ARGS.set(args);
}

/// Test-mode helper — wipes + sets argv. Tests need to bypass the
/// once-only semantics. Not exposed at C ABI.
#[doc(hidden)]
pub fn set_test_args(args: Vec<String>) {
    // OnceLock can't be reset, but std::env::args is the fallback,
    // so we just set if not already set; tests that need to reset
    // should run in a dedicated process or via the std::env path.
    let _ = CAPTURED_ARGS.set(args);
}

// =====================================================================
// Error taxonomy (constitution §2.2 binds Result<T, E> as default)
// =====================================================================

/// Error kind classification. Constitution §2.2 binds `Result<T,E>`
/// as the default error path; this enum is the `E` parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// I/O error (file not found, permission denied, ...).
    Io,
    /// Parse error (number parse, json/csv malformed, ...).
    Parse,
    /// User-supplied custom error.
    Custom,
    /// Out of bounds (collection access).
    OutOfBounds,
    /// Key not found (dict access).
    KeyNotFound,
    /// Generic runtime invariant violation.
    Runtime,
}

/// Cobrust's unified runtime error.
///
/// Carries a kind + a human-readable message. Future M12 will widen
/// to carry a structured `cause` chain; M11 keeps it flat for
/// simplicity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Io, message)
    }

    pub fn parse(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Parse, message)
    }

    pub fn custom(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Custom, message)
    }

    pub fn out_of_bounds(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::OutOfBounds, message)
    }

    pub fn key_not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::KeyNotFound, message)
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Runtime, message)
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            match self.kind {
                ErrorKind::Io => "io error",
                ErrorKind::Parse => "parse error",
                ErrorKind::Custom => "error",
                ErrorKind::OutOfBounds => "out of bounds",
                ErrorKind::KeyNotFound => "key not found",
                ErrorKind::Runtime => "runtime error",
            },
            self.message
        )
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::io(value.to_string())
    }
}

// =====================================================================
// C ABI — main shim + drop handlers
// =====================================================================

/// C ABI main shim. Called by the platform's C runtime as the
/// program entry. Captures argv + delegates to user's
/// `_cobrust_user_main`.
///
/// At M11 the user's `main` returns an `i64` (matching M10
/// hello.cb's signature); M12 will widen to `Result<(), Error>`.
///
/// # Safety
///
/// Must be the canonical C ABI entry point installed by codegen's
/// linker step. The platform passes argc + argv per the System V
/// AMD64 / AAPCS64 conventions (ADR-0023 §"Calling convention
/// details").
///
/// At M11 the linker step links a static `int main(int argc, char**
/// argv)` shim from `crates/cobrust-stdlib/runtime/cobrust_main.c`
/// (built via build.rs on consumer's side); that shim calls into
/// this Rust function then dispatches to `_cobrust_user_main`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_capture_argv(argc: i32, argv: *const *const u8) {
    if argv.is_null() || argc <= 0 {
        capture_args(Vec::new());
        return;
    }
    let mut collected = Vec::with_capacity(argc as usize);
    for i in 0..argc {
        // SAFETY: the C runtime guarantees argv is a valid array of
        // `argc` non-null nul-terminated strings.
        let p = unsafe { *argv.add(i as usize) };
        if p.is_null() {
            collected.push(String::new());
            continue;
        }
        // SAFETY: each argv[i] is nul-terminated per POSIX.
        let cstr = unsafe { std::ffi::CStr::from_ptr(p.cast()) };
        collected.push(cstr.to_string_lossy().into_owned());
    }
    capture_args(collected);
}

// Per-type drop handlers (ADR-0025 §"Codegen amendments" Drop row).
//
// At M11 these are emitted as no-ops for `Str` (`.rodata` strings
// don't need freeing) and as concrete frees for the heap-backed
// collections. M12+ will widen to user-defined ADTs.

/// `_cobrust_drop_str(*mut StrLayout)` — drop a Cobrust `str`.
/// At M11 strings are .rodata only; this is a no-op. M12 widens
/// when heap-allocated strings land.
///
/// # Safety
///
/// `place` must be a valid pointer to a Cobrust `str` layout
/// produced by the codegen Aggregate-lowering for `Str`. At M11
/// .rodata strings have no heap state, so this is always a no-op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _cobrust_drop_str(_place: *mut u8) {
    // No-op: .rodata strings don't own heap state at M11.
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
    fn error_kinds_distinct() {
        assert_ne!(Error::io("a"), Error::parse("a"));
        assert_ne!(Error::custom("a"), Error::runtime("a"));
        assert_ne!(Error::out_of_bounds("a"), Error::key_not_found("a"));
    }

    #[test]
    fn error_display_includes_kind() {
        let e = Error::io("file not found");
        assert!(format!("{e}").contains("io error"));
        assert!(format!("{e}").contains("file not found"));
    }

    #[test]
    fn error_kind_accessor() {
        let e = Error::parse("bad json");
        assert_eq!(e.kind(), &ErrorKind::Parse);
        assert_eq!(e.message(), "bad json");
    }

    #[test]
    fn from_io_error() {
        let std_err = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let cob_err: Error = std_err.into();
        assert_eq!(cob_err.kind(), &ErrorKind::Io);
    }

    #[test]
    fn capture_argv_handles_null() {
        // SAFETY: this is the documented null-arg path.
        unsafe {
            __cobrust_capture_argv(0, std::ptr::null());
        }
    }
}

// =====================================================================
// Exit-code constants (mirror cobrust-cli/src/exit_codes.rs per ADR-0024)
// =====================================================================

/// Exit-code scheme constants. Mirrors `crate cobrust-cli`'s
/// `exit_codes` module (per ADR-0024 §"Exit-code scheme") so that
/// runtime-tier code (panic handler, main shim) can reach them
/// without a circular dep on the CLI crate.
pub mod exit_codes {
    pub const SUCCESS: u8 = 0;
    pub const USER_ERROR: u8 = 1;
    pub const TYPE_ERROR: u8 = 2;
    pub const INTERNAL_PANIC: u8 = 3;
    pub const RUNTIME_PANIC: u8 = 4;
}
