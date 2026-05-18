//! `cobrust-types-cb` — cb mirror of `cobrust-types` (ADR-0055a + ADR-0055b).
//!
//! Proof artifact per ADR-0055 §1.1: every public type in the Rust
//! canonical `cobrust-types` crate is mirrored here in arena-form.
//!
//! ## Re-export surface (mirrors `cobrust-types::lib.rs`)
//!
//! Every `pub use` in Rust `lib.rs` is reproduced here per ADR-0055b §4
//! risk 3 mitigation: Tier-2 ports (`0055c` `infer.rs`, `0055d` `check.rs`)
//! import from this crate with identical name shapes.
//!
//! ADR-0055b §9.4 doc mandate: agent docs in `docs/agent/modules/types-cb.md`.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::todo)]

pub mod error_cb;

// Re-export mirrors: names preserved per ADR-0055b §4 re-export contract.
// Tier-2 (0055c infer.rs, 0055d check.rs) imports `use cobrust_types_cb::{TypeError, ...}`
// with the same shape as Rust `lib.rs`.
pub use error_cb::TypeErrorCb as TypeError;
