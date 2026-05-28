//! Build script — ADR-0073 second-proof link wiring for the `cabi`
//! shims.
//!
//! The `cabi` module declares the Cobrust `__cobrust_str_*` primitives
//! `extern "C"` and binds them from `libcobrust_stdlib.a` at the
//! `cobrust build` link step (NO Rust-level production dependency on
//! cobrust-stdlib — that is the Q5 constraint from ADR-0072 §2/§3 Q5,
//! preserved verbatim for the hood sprint). The `rlib` and `staticlib`
//! outputs tolerate these undefined symbols (an archive resolves them
//! at the final link). The `cdylib` output (PyO3 native module) must,
//! however, be told to defer them: a dylib normally requires every
//! symbol resolved at its own build time.
//!
//! On macOS, `-undefined dynamic_lookup` defers the `__cobrust_str_*`
//! resolution to load time (the PyO3 path never calls them; the
//! `.cb` static-link path resolves them from `libcobrust_stdlib.a`).
//! This is the same flag PyO3 extension modules already rely on, and
//! mirrors the pit / den / nest / strike / scale / molt build.rs pattern.

fn main() {
    // cdylib-only flag so the rlib / staticlib builds are unaffected.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
