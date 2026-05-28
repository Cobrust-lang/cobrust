//! Ecosystem-module manifest — the type-side table that lets a `.cb`
//! program `import den` and call `den.connect(...)` / `conn.execute(...)`
//! / `cur.fetchall()` with full compile-time type safety.
//!
//! Per ADR-0072 §2/§3 (Q1 built-in privileged namespaces, Q2 Rust-table
//! manifest, Q3 nominal handle types, Q6 `@py_compat` tier). This is the
//! L0 of the proven flat-intrinsic chain reused for ecosystem modules:
//!
//! ```text
//! .cb `import den` + `den.connect(...)`
//!   → cobrust-types ecosystem manifest (THIS FILE)          [L1 typecheck]
//!   → cobrust-cli intrinsic-rewrite (retarget → __cobrust_den_*)  [L2 MIR]
//!   → cobrust-codegen externs + handle drop                  [L3 codegen]
//!   → cobrust-den C-ABI shims (libden.a)                     [L4 runtime]
//!   → cobrust-cli build.rs per-import static link            [L5 link]
//! ```
//!
//! # Handle modeling (Q3)
//!
//! Each opaque handle (`den.Connection`, `den.Cursor`) is a **nominal**
//! [`Ty::Adt`] with a reserved [`AdtId`] in the [`ECO_ADT_BASE`] range.
//! Reusing `Ty::Adt` means the existing non-`Copy` drop-schedule path
//! (`cobrust-mir::drop::is_copy` returns `false` for `Ty::Adt`, so the
//! drop pass inserts a `Terminator::Drop` at scope exit) carries the
//! handle for free — codegen's `emit_drop_for_ty` then dispatches the
//! per-handle drop symbol. The reserved-id range keeps these from
//! colliding with user `class` ADTs (whose `AdtId == DefId`, always
//! small).
//!
//! # First proof (ADR-0072 §4)
//!
//! Only `den` is wired, with three calls: `connect`, `Connection.execute`,
//! `Cursor.fetchall`. The remaining cobra modules generalize off this
//! proven chain.

use crate::ty::{AdtId, Ty};

/// Base for reserved ecosystem-handle [`AdtId`]s. User `class` ADTs use
/// `AdtId == DefId` which is allocated densely from 0, so a high base
/// guarantees no collision in any realistic program.
pub const ECO_ADT_BASE: u32 = 0xE000_0000;

/// `AdtId` for the `den.Connection` handle.
pub const DEN_CONNECTION_ADT: AdtId = AdtId(ECO_ADT_BASE);
/// `AdtId` for the `den.Cursor` handle.
pub const DEN_CURSOR_ADT: AdtId = AdtId(ECO_ADT_BASE + 1);

/// `AdtId` for the `strike.Response` handle (ADR-0072 third-module
/// generalization — HTTP client, rebrand of `requests`).
///
/// Per-module reservation convention: each ecosystem module reserves a
/// 256-slot block starting at `ECO_ADT_BASE + N*0x100`. `den` occupies
/// the first block (`0xE000_0000..0xE000_00FF`); `strike` occupies the
/// second (`0xE000_0100..0xE000_01FF`). This leaves ample room for new
/// handles per module without colliding across modules. When a third
/// handle-typed module lands, allocate the next 256-slot block.
pub const STRIKE_RESPONSE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x100);

/// The Cobrust `Ty` for the `den.Connection` opaque handle.
#[must_use]
pub fn den_connection_ty() -> Ty {
    Ty::Adt(DEN_CONNECTION_ADT, vec![])
}

/// The Cobrust `Ty` for the `den.Cursor` opaque handle.
#[must_use]
pub fn den_cursor_ty() -> Ty {
    Ty::Adt(DEN_CURSOR_ADT, vec![])
}

/// The Cobrust `Ty` for the `strike.Response` opaque handle.
#[must_use]
pub fn strike_response_ty() -> Ty {
    Ty::Adt(STRIKE_RESPONSE_ADT, vec![])
}

/// Is this `AdtId` one of the reserved ecosystem-handle ids?
#[must_use]
pub fn is_ecosystem_handle(id: AdtId) -> bool {
    id.0 >= ECO_ADT_BASE
}

/// The drop symbol for a reserved ecosystem-handle `AdtId`, or `None`
/// when the id is not a known handle. Consumed by codegen's
/// `emit_drop_for_ty` to schedule the foreign drop at scope exit
/// (ADR-0072 §3 / §5 risk 1).
#[must_use]
pub fn handle_drop_symbol(id: AdtId) -> Option<&'static str> {
    match id {
        DEN_CONNECTION_ADT => Some("__cobrust_den_connection_drop"),
        DEN_CURSOR_ADT => Some("__cobrust_den_cursor_drop"),
        STRIKE_RESPONSE_ADT => Some("__cobrust_strike_response_drop"),
        _ => None,
    }
}

/// `@py_compat` compatibility tier for a manifest entry (Q6). Recorded
/// now; the L2-verifier hard-bind (CLAUDE.md §2.5-C) is deferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PyCompatTier {
    /// Bit-for-bit CPython parity on the supported surface.
    Strict,
    /// Behaviorally equivalent with declared divergences.
    Semantic,
    /// Numerical parity within a tolerance.
    Numerical,
}

/// One ecosystem-module function or method signature.
#[derive(Clone, Debug)]
pub struct EcoSig {
    /// The C-ABI runtime symbol the call retargets onto (e.g.
    /// `__cobrust_den_connect`). Used verbatim by the MIR
    /// intrinsic-rewrite and the codegen extern declaration.
    pub runtime_symbol: &'static str,
    /// Parameter types (the receiver, for a method, is implicit and
    /// NOT listed here — it is the base of the `.attr` access).
    pub params: Vec<Ty>,
    /// Return type.
    pub ret: Ty,
    /// `@py_compat` tier (Q6).
    pub tier: PyCompatTier,
}

/// Resolve a module-level free function `<module>.<fn>` (e.g.
/// `den.connect`) to its signature. Returns `None` when the module is
/// not a known ecosystem module or the function is not in the manifest.
#[must_use]
pub fn lookup_module_fn(module: &str, func: &str) -> Option<EcoSig> {
    match (module, func) {
        ("den", "connect") => Some(EcoSig {
            runtime_symbol: "__cobrust_den_connect",
            params: vec![Ty::Str],
            ret: den_connection_ty(),
            tier: PyCompatTier::Strict,
        }),
        // ADR-0072 second-module generalization — `nest` (TOML, the
        // rebrand of `tomli`). Pure value-in-value-out (`Str → Str`):
        // parses the TOML source and returns its canonical JSON
        // rendering. No handles, no callbacks; the chain handles this
        // case natively via the existing Str drop schedule.
        // Tier `Semantic` — nest produces a JSON canonicalization of
        // CPython `tomllib`'s parse output (behaviorally equivalent;
        // not a bit-for-bit CPython parity surface).
        ("nest", "loads_str") => Some(EcoSig {
            runtime_symbol: "__cobrust_nest_loads_str",
            params: vec![Ty::Str],
            ret: Ty::Str,
            tier: PyCompatTier::Semantic,
        }),
        // ADR-0072 third-module generalization — `strike` (HTTP client,
        // the rebrand of `requests`). Pairs handle-pattern (Response,
        // like `den.Connection`/`Cursor`) with free-function entrypoints
        // (`get`/`post`, like `den.connect`). Tier `Semantic` — HTTP is
        // not a bit-for-bit parity surface (timing, headers ordering,
        // connection-pool side effects); behaviorally equivalent for
        // the supported verb/method set.
        ("strike", "get") => Some(EcoSig {
            runtime_symbol: "__cobrust_strike_get",
            params: vec![Ty::Str],
            ret: strike_response_ty(),
            tier: PyCompatTier::Semantic,
        }),
        ("strike", "post") => Some(EcoSig {
            runtime_symbol: "__cobrust_strike_post",
            params: vec![Ty::Str, Ty::Str],
            ret: strike_response_ty(),
            tier: PyCompatTier::Semantic,
        }),
        _ => None,
    }
}

/// Resolve a method call `<receiver-handle>.<method>` to its signature.
/// The receiver type pins which handle the method belongs to (so
/// `conn.execute` and an imagined `cur.execute` never collide). Returns
/// `None` when the receiver is not an ecosystem handle or the method is
/// not in the manifest.
#[must_use]
pub fn lookup_handle_method(receiver: &Ty, method: &str) -> Option<EcoSig> {
    let Ty::Adt(id, _) = receiver else {
        return None;
    };
    match (*id, method) {
        (DEN_CONNECTION_ADT, "execute") => Some(EcoSig {
            runtime_symbol: "__cobrust_den_connection_execute",
            // Receiver is implicit; the explicit param is the SQL str.
            params: vec![Ty::Str],
            ret: den_cursor_ty(),
            tier: PyCompatTier::Strict,
        }),
        (DEN_CURSOR_ADT, "fetchall") => Some(EcoSig {
            runtime_symbol: "__cobrust_den_cursor_fetchall",
            // First proof: fetchall renders the rows to a `str`
            // (ADR-0072 §4; row→list[tuple] is the immediate follow-up).
            params: vec![],
            ret: Ty::Str,
            tier: PyCompatTier::Strict,
        }),
        // ADR-0072 third-module generalization — `strike.Response`
        // methods. All borrow the receiver; `status_code` returns an
        // i64 (u16 widened to i64 at the C-ABI boundary); `text`/`json`
        // allocate fresh Cobrust `Str` buffers the caller owns. `json`
        // returns the canonicalized JSON rendering of the body (mirrors
        // den's `fetchall() -> str` first-proof rendering shape; a
        // structured-value surface is a tracked follow-up).
        (STRIKE_RESPONSE_ADT, "text") => Some(EcoSig {
            runtime_symbol: "__cobrust_strike_response_text",
            params: vec![],
            ret: Ty::Str,
            tier: PyCompatTier::Semantic,
        }),
        (STRIKE_RESPONSE_ADT, "status_code") => Some(EcoSig {
            runtime_symbol: "__cobrust_strike_response_status_code",
            params: vec![],
            ret: Ty::Int,
            tier: PyCompatTier::Semantic,
        }),
        (STRIKE_RESPONSE_ADT, "json") => Some(EcoSig {
            runtime_symbol: "__cobrust_strike_response_json",
            params: vec![],
            ret: Ty::Str,
            tier: PyCompatTier::Semantic,
        }),
        _ => None,
    }
}

/// Is `name` a known built-in ecosystem-module alias (Q1)? The HIR
/// binds `import den` as a `DefKind::ImportAlias` with surface name
/// `den`; the typechecker uses this to mark the alias `def_id` so
/// `den.attr` accesses resolve against the manifest.
#[must_use]
pub fn is_ecosystem_module(name: &str) -> bool {
    matches!(name, "den" | "nest" | "strike")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_handle_ids_are_recognized() {
        assert!(is_ecosystem_handle(DEN_CONNECTION_ADT));
        assert!(is_ecosystem_handle(DEN_CURSOR_ADT));
        assert!(!is_ecosystem_handle(AdtId(7)));
    }

    #[test]
    fn handle_drop_symbols_resolve() {
        assert_eq!(
            handle_drop_symbol(DEN_CONNECTION_ADT),
            Some("__cobrust_den_connection_drop")
        );
        assert_eq!(
            handle_drop_symbol(DEN_CURSOR_ADT),
            Some("__cobrust_den_cursor_drop")
        );
        assert_eq!(handle_drop_symbol(AdtId(7)), None);
    }

    #[test]
    fn connect_signature_returns_connection_handle() {
        let sig = lookup_module_fn("den", "connect").expect("den.connect in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_den_connect");
        assert_eq!(sig.params, vec![Ty::Str]);
        assert_eq!(sig.ret, den_connection_ty());
        assert_eq!(sig.tier, PyCompatTier::Strict);
    }

    #[test]
    fn unknown_module_fn_is_none() {
        assert!(lookup_module_fn("den", "nope").is_none());
        assert!(lookup_module_fn("nope", "connect").is_none());
    }

    #[test]
    fn execute_method_on_connection_returns_cursor() {
        let sig =
            lookup_handle_method(&den_connection_ty(), "execute").expect("Connection.execute");
        assert_eq!(sig.runtime_symbol, "__cobrust_den_connection_execute");
        assert_eq!(sig.params, vec![Ty::Str]);
        assert_eq!(sig.ret, den_cursor_ty());
    }

    #[test]
    fn fetchall_method_on_cursor_returns_str() {
        let sig = lookup_handle_method(&den_cursor_ty(), "fetchall").expect("Cursor.fetchall");
        assert_eq!(sig.runtime_symbol, "__cobrust_den_cursor_fetchall");
        assert!(sig.params.is_empty());
        assert_eq!(sig.ret, Ty::Str);
    }

    #[test]
    fn method_on_wrong_handle_is_none() {
        // fetchall is a Cursor method, not a Connection method.
        assert!(lookup_handle_method(&den_connection_ty(), "fetchall").is_none());
        // execute is a Connection method, not a Cursor method.
        assert!(lookup_handle_method(&den_cursor_ty(), "execute").is_none());
        // Non-handle receivers never match.
        assert!(lookup_handle_method(&Ty::Str, "execute").is_none());
    }

    #[test]
    fn den_is_a_known_module() {
        assert!(is_ecosystem_module("den"));
        assert!(!is_ecosystem_module("os"));
    }

    // ADR-0072 second-module proof — `nest` (TOML, rebrand of tomli).

    #[test]
    fn nest_is_a_known_module() {
        assert!(is_ecosystem_module("nest"));
    }

    #[test]
    fn nest_loads_str_signature_is_str_to_str() {
        let sig = lookup_module_fn("nest", "loads_str").expect("nest.loads_str in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_nest_loads_str");
        assert_eq!(sig.params, vec![Ty::Str]);
        assert_eq!(sig.ret, Ty::Str);
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn unknown_nest_fn_is_none() {
        assert!(lookup_module_fn("nest", "nope").is_none());
    }

    // ADR-0072 third-module proof — `strike` (HTTP, rebrand of requests).

    #[test]
    fn strike_is_a_known_module() {
        assert!(is_ecosystem_module("strike"));
    }

    #[test]
    fn strike_response_handle_id_recognized_and_in_reserved_block() {
        assert!(is_ecosystem_handle(STRIKE_RESPONSE_ADT));
        // Per-module 256-slot reservation: strike lives in the second
        // block, well outside den's first block. Const-block so the
        // compile-time-constant comparisons trip a real ABI mistake
        // (someone bumping ECO_ADT_BASE without resizing) rather than a
        // clippy::assertions_on_constants false-positive at test time.
        const _: () = {
            assert!(STRIKE_RESPONSE_ADT.0 >= ECO_ADT_BASE + 0x100);
            assert!(STRIKE_RESPONSE_ADT.0 < ECO_ADT_BASE + 0x200);
        };
    }

    #[test]
    fn strike_response_drop_symbol_resolves() {
        assert_eq!(
            handle_drop_symbol(STRIKE_RESPONSE_ADT),
            Some("__cobrust_strike_response_drop")
        );
    }

    #[test]
    fn strike_get_signature_returns_response_handle() {
        let sig = lookup_module_fn("strike", "get").expect("strike.get in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_strike_get");
        assert_eq!(sig.params, vec![Ty::Str]);
        assert_eq!(sig.ret, strike_response_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn strike_post_signature_takes_url_and_body() {
        let sig = lookup_module_fn("strike", "post").expect("strike.post in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_strike_post");
        assert_eq!(sig.params, vec![Ty::Str, Ty::Str]);
        assert_eq!(sig.ret, strike_response_ty());
    }

    #[test]
    fn strike_response_methods_resolve() {
        let text =
            lookup_handle_method(&strike_response_ty(), "text").expect("Response.text in manifest");
        assert_eq!(text.runtime_symbol, "__cobrust_strike_response_text");
        assert!(text.params.is_empty());
        assert_eq!(text.ret, Ty::Str);

        let code = lookup_handle_method(&strike_response_ty(), "status_code")
            .expect("Response.status_code in manifest");
        assert_eq!(code.runtime_symbol, "__cobrust_strike_response_status_code");
        assert!(code.params.is_empty());
        assert_eq!(code.ret, Ty::Int);

        let json =
            lookup_handle_method(&strike_response_ty(), "json").expect("Response.json in manifest");
        assert_eq!(json.runtime_symbol, "__cobrust_strike_response_json");
        assert!(json.params.is_empty());
        assert_eq!(json.ret, Ty::Str);
    }

    #[test]
    fn strike_methods_only_match_response_receiver() {
        // Cross-handle: den.Connection should never resolve strike methods.
        assert!(lookup_handle_method(&den_connection_ty(), "text").is_none());
        assert!(lookup_handle_method(&Ty::Str, "status_code").is_none());
        // Unknown method on the right receiver is None.
        assert!(lookup_handle_method(&strike_response_ty(), "nope").is_none());
    }

    #[test]
    fn unknown_strike_fn_is_none() {
        assert!(lookup_module_fn("strike", "nope").is_none());
    }
}
