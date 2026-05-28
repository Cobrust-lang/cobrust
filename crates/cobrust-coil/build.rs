//! Build script — ADR-0072 8/8 first-proof link wiring for the `cabi`
//! shims.
//!
//! The first-proof `cabi` surface here is callback-free and string-free
//! at the wire (zeros/ones/eye take an `i64` count; print_buffer takes
//! an opaque handle pointer; the only side effect is `println!` on the
//! Rust side, NOT a `.cb`-owned `Str` buffer). So unlike den/hood/pit
//! this build.rs does not yet need to defer `__cobrust_str_*` external
//! resolution. The script is kept (mirroring the den/hood/pit pattern)
//! so a future `Buffer.tolist() -> str` extension that DOES need the
//! str-buffer extern shape just edits this script — the wiring is
//! already in place.
//!
//! On macOS, `-undefined dynamic_lookup` defers any future externs to
//! load time (the PyO3 path never calls them; the `.cb` static-link
//! path resolves them from `libcobrust_stdlib.a`). This is the same
//! flag PyO3 extension modules already rely on, and mirrors the
//! pit / hood / den / nest / strike / scale / molt build.rs pattern.

fn main() {
    // cdylib-only flag so the rlib / staticlib builds are unaffected.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
