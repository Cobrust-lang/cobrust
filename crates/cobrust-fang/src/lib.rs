//! Cobrust `fang` ecosystem module — password hashing / verification.
//!
//! ADR-0078 backend Phase 2, the FIRST backend Phase-2 crate (tenth
//! ecosystem module overall, after den/nest/strike/scale/molt/pit/hood/
//! coil/dora). `fang` (cobra-themed name for the auth/security toolkit)
//! wraps the [`argon2`] crate to expose two FLAT value-functions to
//! `.cb` programs:
//!
//! - `fang.hash_password(pw: str) -> str` — argon2id PHC hash of `pw`,
//!   with a fresh random salt embedded in the returned `$argon2id$…`
//!   string.
//! - `fang.verify_password(pw: str, hash: str) -> bool` — constant-time
//!   verification of `pw` against a PHC `hash`. A wrong password is a
//!   normal `false` return (NOT a panic / error).
//!
//! # Why argon2id, no algo/params knob (elegance law)
//!
//! Per the Cobrust ecosystem design law (CLAUDE.md §2.2/§2.5 +
//! feedback_elegant_ecosystem_surface): the surface drops the footguns
//! every other language's auth library carries:
//!
//! - **argon2id is THE secure default** — [`argon2::Argon2::default`] is
//!   argon2id with OWASP-recommended parameters. Phase 1 exposes NO
//!   algorithm or cost-parameter selection, so a `.cb` author cannot
//!   accidentally pick a weak algo (argon2i/argon2d) or weak params.
//! - **The returned hash is the FULL PHC string** (`$argon2id$v=…$m=…,
//!   t=…,p=…$<salt>$<hash>`) — the salt and parameters travel WITH the
//!   hash, so there is no separate-salt-management footgun.
//! - **Verification is constant-time** ([`argon2::Argon2::verify_password`]),
//!   so there is no timing-attack footgun.
//! - **No plaintext password is ever logged.**
//!
//! # The chain
//!
//! `fang` is a pure value-pattern module (like `nest`/`scale`): no
//! handles, no `AdtId`, no callbacks. The Cobrust toolchain retargets
//! `fang.hash_password` / `fang.verify_password` onto the
//! `__cobrust_fang_*` C-ABI symbols exported by [`cabi`]; `cobrust
//! build` static-links `libfang.a` after `libcobrust_stdlib.a` only
//! when a program imports `fang`.

// ADR-0078 backend Phase 2 — C-ABI shims for `.cb` programs doing
// `import fang` + `fang.hash_password(pw)` / `fang.verify_password(pw,
// hash)`. The shims are `#[no_mangle] extern "C"` and live behind their
// own module so the rlib / cdylib paths still compile cleanly; the
// `staticlib` archive carries the symbols out to `cobrust build`.
pub mod cabi;
