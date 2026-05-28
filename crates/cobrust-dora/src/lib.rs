//! cobrust-dora — Cobrust ↔ dora-rs robotics dataflow bridge.
//!
//! ADR-0076 Phase 1 deliverable: the NINTH ecosystem-module proof on the
//! ratified `.cb` ecosystem-import chain (ADR-0072 manifest + intrinsic
//! retarget + codegen extern + C-ABI shim + per-import static link) and
//! the THIRD module exercising the ADR-0073 cross-boundary callback
//! marshalling pattern (after pit and hood).
//!
//! # Phase 1 scope — SYNTHETIC runtime
//!
//! Per ADR-0076 §5 Phase 1, this crate ships a **synthetic** dora runtime
//! that proves the .cb→Rust→back-to-.cb-callback chain WITHOUT depending
//! on a real dora-rs coordinator + daemon. `__cobrust_dora_node_run` mocks
//! one canned message arrival (`"camera"` input id, `"frame_001"` body)
//! then returns. This mirrors the synthetic-LLM precedent from F65: the
//! chain is proven end-to-end, the real runtime integration is a
//! follow-up sprint (Phase 2).
//!
//! # Module roster
//!
//! - [`cabi`] — `#[no_mangle] extern "C"` shims `.cb` programs bind
//!   onto. Six trampolines + two drops; mirrors the hood/pit cabi.rs
//!   shape.
//!
//! # The chain (verbatim from ADR-0072 §"prior modules")
//!
//! ```text
//! .cb `import dora` + `dora.Node("detector")` + `dora.node(handler)`
//!   + `node.run()` + a top-level `fn handler(event: dora.Event) -> i64:`
//!   → cobrust-types ecosystem manifest                       [L1 typecheck]
//!   → cobrust-mir intrinsic-rewrite (retarget → __cobrust_dora_*) [L2 MIR]
//!   → cobrust-codegen externs + Constant::FnRef for the callback  [L3 codegen]
//!   → cobrust-dora C-ABI shims (libdora.a) + trampoline closure   [L4 runtime]
//!   → cobrust-cli build.rs per-import static link                 [L5 link]
//! ```
//!
//! # Drop discipline
//!
//! - [`cabi::DROP_COUNT`] instruments Node + Event drops; the in-crate
//!   tests assert exactly-once.
//! - `Event` is **Rust-owned** per ADR-0073 §2 D6 (the trampoline owns
//!   the `Box<Event>` and frees it on callback return) — the `.cb` side
//!   must not free it. The manifest `handle_drop_symbol(DORA_EVENT_ADT)`
//!   accordingly returns `None`.

// ADR-0076 Phase 1 — `.cb` ecosystem-import C-ABI shims. The `cabi`
// module declares the `__cobrust_dora_*` symbols `.cb` programs bind to
// at link time after `cobrust build` retargets `dora.Node(...)` +
// `dora.node(fn_name)` + `node.run()` calls onto these symbols. See
// `cabi::DROP_COUNT` for the drop-once instrument used by the test
// suite.
pub mod cabi;
