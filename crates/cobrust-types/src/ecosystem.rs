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

use crate::ty::{AdtId, FnTy, Ty};

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
/// second (`0xE000_0100..0xE000_01FF`); `scale` (msgpack) would occupy
/// the third block (`0xE000_0200..0xE000_02FF`) but ships no handles
/// in its first proof — value-pattern only, like `nest`; `molt`
/// (dateutil) occupies the fourth block
/// (`0xE000_0300..0xE000_03FF`). Each new handle-typed module gets the
/// next 256-slot block.
pub const STRIKE_RESPONSE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x100);

/// `AdtId` for the `molt.DateTime` handle (ADR-0072 fifth-module
/// generalization — datetime/parsing, rebrand of `python-dateutil`).
/// Reserved in the FOURTH per-module 256-slot block (the third block
/// `0xE000_0200..0xE000_02FF` is reserved for `scale` (msgpack), which
/// ships no handles in its first proof, but the block stays bound to
/// scale so a future raw-bytes ABI can populate it without renumbering
/// molt).
pub const MOLT_DATETIME_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x300);

// ADR-0073 — `pit` (Flask, web-server) handle ADT block reservation.
// The FIFTH per-module 256-slot block (`0xE000_0400..0xE000_04FF`).
// `pit` ships FOUR handles in its first proof:
// - `App`             — the application object the `.cb` source builds.
// - `Request`         — incoming HTTP request passed to a handler fn.
// - `Response`        — outbound HTTP response a handler returns.
// - `ServerHandle`    — the `serve_in_background` join handle.
// hood (rebrand of click) gets the SIXTH block (`0xE000_0500..0x05FF`)
// in its sibling sprint.

/// `AdtId` for the `pit.App` handle (ADR-0073 §2 D1).
pub const PIT_APP_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x400);
/// `AdtId` for the `pit.Request` handle (ADR-0073).
pub const PIT_REQUEST_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x401);
/// `AdtId` for the `pit.Response` handle (ADR-0073).
pub const PIT_RESPONSE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x402);
/// `AdtId` for the `pit.ServerHandle` handle (ADR-0073).
pub const PIT_SERVER_HANDLE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x403);

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

/// The Cobrust `Ty` for the `molt.DateTime` opaque handle.
#[must_use]
pub fn molt_datetime_ty() -> Ty {
    Ty::Adt(MOLT_DATETIME_ADT, vec![])
}

/// The Cobrust `Ty` for the `pit.App` opaque handle (ADR-0073).
#[must_use]
pub fn pit_app_ty() -> Ty {
    Ty::Adt(PIT_APP_ADT, vec![])
}

/// The Cobrust `Ty` for the `pit.Request` opaque handle (ADR-0073).
#[must_use]
pub fn pit_request_ty() -> Ty {
    Ty::Adt(PIT_REQUEST_ADT, vec![])
}

/// The Cobrust `Ty` for the `pit.Response` opaque handle (ADR-0073).
#[must_use]
pub fn pit_response_ty() -> Ty {
    Ty::Adt(PIT_RESPONSE_ADT, vec![])
}

/// The Cobrust `Ty` for the `pit.ServerHandle` opaque handle (ADR-0073).
#[must_use]
pub fn pit_server_handle_ty() -> Ty {
    Ty::Adt(PIT_SERVER_HANDLE_ADT, vec![])
}

/// The handler `FnTy` `pit.App.route` expects in its 3rd argument
/// (ADR-0073 §2 D1+D4): a top-level fn taking a single `pit.Request`
/// and returning a `pit.Response`. The cross-boundary trampoline in
/// `cobrust-pit/src/cabi.rs` enforces the same C-ABI shape (`extern "C"
/// fn(*mut u8) -> *mut u8`).
#[must_use]
pub fn pit_handler_fn_ty() -> FnTy {
    FnTy {
        positional: vec![pit_request_ty()],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(pit_response_ty()),
    }
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
///
/// Note `PIT_REQUEST_ADT` returns `None` deliberately (ADR-0073 §2 D6):
/// a `.cb` handler receives a `Request` borrowed from Rust through the
/// trampoline (the trampoline owns the `Box<Request>` and frees it on
/// the callback return) — the `.cb` side must not free it.
#[must_use]
pub fn handle_drop_symbol(id: AdtId) -> Option<&'static str> {
    match id {
        DEN_CONNECTION_ADT => Some("__cobrust_den_connection_drop"),
        DEN_CURSOR_ADT => Some("__cobrust_den_cursor_drop"),
        STRIKE_RESPONSE_ADT => Some("__cobrust_strike_response_drop"),
        MOLT_DATETIME_ADT => Some("__cobrust_molt_datetime_drop"),
        PIT_APP_ADT => Some("__cobrust_pit_app_drop"),
        // PIT_REQUEST_ADT — Rust-owned, never dropped from `.cb` (ADR-0073 §2 D6).
        PIT_RESPONSE_ADT => Some("__cobrust_pit_response_drop"),
        PIT_SERVER_HANDLE_ADT => Some("__cobrust_pit_server_handle_drop"),
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

/// One ecosystem-call parameter slot. Most parameters are plain values
/// crossing the C-ABI as scalars or opaque pointers (the `Value` variant
/// is the source of truth for all den / nest / strike / scale / molt
/// rows). ADR-0073 adds `Callback`: a `.cb` top-level `fn` name passed
/// across the boundary as a raw C-ABI fn-pointer.
#[derive(Clone, Debug)]
pub enum EcoParam {
    /// A plain Cobrust `Ty` crossing the boundary as a value (scalar
    /// or opaque handle pointer). The pre-ADR-0073 shape — every
    /// existing manifest row uses this variant.
    Value(Ty),
    /// A callback fn pointer crossing the boundary as a C-ABI
    /// `extern "C" fn(*mut u8) -> *mut u8`. The argument MUST be a
    /// top-level `fn` name (no closures, no fn-typed locals, no
    /// call-results) whose signature unifies with the embedded
    /// `FnTy` (ADR-0073 §2 D1+D8). The MIR lowering emits
    /// `Operand::Constant(Constant::FnRef(def_id))` for this slot
    /// so codegen materialises the fn pointer via the
    /// `function_ids` table at the call site.
    Callback(FnTy),
}

/// One ecosystem-module function or method signature.
#[derive(Clone, Debug)]
pub struct EcoSig {
    /// The C-ABI runtime symbol the call retargets onto (e.g.
    /// `__cobrust_den_connect`). Used verbatim by the MIR
    /// intrinsic-rewrite and the codegen extern declaration.
    pub runtime_symbol: &'static str,
    /// Parameter slots (the receiver, for a method, is implicit and
    /// NOT listed here — it is the base of the `.attr` access). Most
    /// rows pass `EcoParam::Value(Ty)`; ADR-0073 callback slots use
    /// `EcoParam::Callback(FnTy)`.
    pub params: Vec<EcoParam>,
    /// Return type.
    pub ret: Ty,
    /// `@py_compat` tier (Q6).
    pub tier: PyCompatTier,
}

impl EcoSig {
    /// Helper for the common all-`Value` case — builds an `EcoSig`
    /// from a flat `Vec<Ty>` so existing manifest rows stay terse.
    fn from_values(
        runtime_symbol: &'static str,
        params: Vec<Ty>,
        ret: Ty,
        tier: PyCompatTier,
    ) -> Self {
        Self {
            runtime_symbol,
            params: params.into_iter().map(EcoParam::Value).collect(),
            ret,
            tier,
        }
    }
}

/// Resolve a module-level free function `<module>.<fn>` (e.g.
/// `den.connect`) to its signature. Returns `None` when the module is
/// not a known ecosystem module or the function is not in the manifest.
#[must_use]
pub fn lookup_module_fn(module: &str, func: &str) -> Option<EcoSig> {
    match (module, func) {
        ("den", "connect") => Some(EcoSig::from_values(
            "__cobrust_den_connect",
            vec![Ty::Str],
            den_connection_ty(),
            PyCompatTier::Strict,
        )),
        // ADR-0072 second-module generalization — `nest` (TOML, the
        // rebrand of `tomli`). Pure value-in-value-out (`Str → Str`):
        // parses the TOML source and returns its canonical JSON
        // rendering. No handles, no callbacks; the chain handles this
        // case natively via the existing Str drop schedule.
        // Tier `Semantic` — nest produces a JSON canonicalization of
        // CPython `tomllib`'s parse output (behaviorally equivalent;
        // not a bit-for-bit CPython parity surface).
        ("nest", "loads_str") => Some(EcoSig::from_values(
            "__cobrust_nest_loads_str",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0072 third-module generalization — `strike` (HTTP client,
        // the rebrand of `requests`). Pairs handle-pattern (Response,
        // like `den.Connection`/`Cursor`) with free-function entrypoints
        // (`get`/`post`, like `den.connect`). Tier `Semantic` — HTTP is
        // not a bit-for-bit parity surface (timing, headers ordering,
        // connection-pool side effects); behaviorally equivalent for
        // the supported verb/method set.
        ("strike", "get") => Some(EcoSig::from_values(
            "__cobrust_strike_get",
            vec![Ty::Str],
            strike_response_ty(),
            PyCompatTier::Semantic,
        )),
        ("strike", "post") => Some(EcoSig::from_values(
            "__cobrust_strike_post",
            vec![Ty::Str, Ty::Str],
            strike_response_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0072 fourth-module generalization — `scale` (msgpack,
        // the rebrand of `msgpack-python`). Pure value-in-value-out
        // (`Str → Str`) for the first proof — the JSON-string-in /
        // msgpack-hex-rendering-out round trip mirrors `nest`'s
        // value pattern. A raw `*mut u8` bytes ABI is a tracked
        // follow-up; the proven str→str shape keeps the first proof
        // honest about chain generality and defers the bytes-ABI
        // design to its own sub-ADR.
        // Tier `Semantic` — msgpack's binary encoding is canonical-
        // form behavioral parity (CPython msgpack-python's pack/
        // unpack output) but the str-rendering wrapper here is
        // Cobrust-specific (hex over the canonical msgpack bytes,
        // for a printable Str surface).
        ("scale", "dumps_str") => Some(EcoSig::from_values(
            "__cobrust_scale_dumps_str",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        ("scale", "loads_str") => Some(EcoSig::from_values(
            "__cobrust_scale_loads_str",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0072 fifth-module generalization — `molt` (datetime,
        // the rebrand of `python-dateutil`). Handle-pattern (DateTime,
        // like `den.Connection`/`Cursor`) with free-function
        // entrypoint `now()` (like `den.connect`). Tier `Semantic` —
        // datetime parsing / formatting variants (ISO-8601 vs Python
        // strftime defaults vs locale) are behavior-equivalent rather
        // than bit-for-bit CPython parity.
        ("molt", "now") => Some(EcoSig::from_values(
            "__cobrust_molt_now",
            vec![],
            molt_datetime_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0073 — `pit` (Flask, web-server) ecosystem-module wiring.
        // First proof exposes the trio sufficient to register a route and
        // serve it on an ephemeral port:
        // - `pit.App()`                — construct an empty App.
        // - `pit.text_response(i, s)`  — build a Response with `i` status
        //                                and `s` as the text body.
        // The handle methods (`App.route`, `App.serve_in_background`) are
        // wired in `lookup_handle_method`.
        ("pit", "App") => Some(EcoSig::from_values(
            "__cobrust_pit_app_new",
            vec![],
            pit_app_ty(),
            PyCompatTier::Semantic,
        )),
        ("pit", "text_response") => Some(EcoSig::from_values(
            "__cobrust_pit_text_response",
            vec![Ty::Int, Ty::Str],
            pit_response_ty(),
            PyCompatTier::Semantic,
        )),
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
        (DEN_CONNECTION_ADT, "execute") => Some(EcoSig::from_values(
            "__cobrust_den_connection_execute",
            // Receiver is implicit; the explicit param is the SQL str.
            vec![Ty::Str],
            den_cursor_ty(),
            PyCompatTier::Strict,
        )),
        (DEN_CURSOR_ADT, "fetchall") => Some(EcoSig::from_values(
            "__cobrust_den_cursor_fetchall",
            // First proof: fetchall renders the rows to a `str`
            // (ADR-0072 §4; row→list[tuple] is the immediate follow-up).
            vec![],
            Ty::Str,
            PyCompatTier::Strict,
        )),
        // ADR-0072 third-module generalization — `strike.Response`
        // methods. All borrow the receiver; `status_code` returns an
        // i64 (u16 widened to i64 at the C-ABI boundary); `text`/`json`
        // allocate fresh Cobrust `Str` buffers the caller owns. `json`
        // returns the canonicalized JSON rendering of the body (mirrors
        // den's `fetchall() -> str` first-proof rendering shape; a
        // structured-value surface is a tracked follow-up).
        (STRIKE_RESPONSE_ADT, "text") => Some(EcoSig::from_values(
            "__cobrust_strike_response_text",
            vec![],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        (STRIKE_RESPONSE_ADT, "status_code") => Some(EcoSig::from_values(
            "__cobrust_strike_response_status_code",
            vec![],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (STRIKE_RESPONSE_ADT, "json") => Some(EcoSig::from_values(
            "__cobrust_strike_response_json",
            vec![],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0072 fifth-module generalization — `molt.DateTime`
        // methods. Both borrow the receiver; `isoformat` allocates a
        // fresh Cobrust `Str` (RFC3339 rendering — the canonical
        // ISO-8601 subset Python's `datetime.isoformat()` produces);
        // `unix_timestamp` returns an i64 (seconds since the UNIX
        // epoch in UTC).
        (MOLT_DATETIME_ADT, "isoformat") => Some(EcoSig::from_values(
            "__cobrust_molt_datetime_isoformat",
            vec![],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        (MOLT_DATETIME_ADT, "unix_timestamp") => Some(EcoSig::from_values(
            "__cobrust_molt_datetime_unix_timestamp",
            vec![],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // ADR-0073 — `pit.App` handle methods.
        //
        // `app.route(method, path, handler)` is the load-bearing
        // callback site: the 3rd parameter is an `EcoParam::Callback`
        // whose `FnTy` is `fn(pit.Request) -> pit.Response`. The MIR
        // lowering emits `Constant::FnRef(def_id)` for this slot and
        // codegen materialises the fn pointer via `function_ids`.
        //
        // `app.serve_in_background(host, port)` consumes the App via
        // `std::mem::take` in the trampoline (the empty `App::default()`
        // that replaces it is still safe to `_drop` later), returning a
        // boxed `ServerHandle` whose drop aborts the server task.
        //
        // `route` returns `Ty::None` (NOT another App handle) to avoid
        // double-drop on the `let app2 = app.route(...)` chaining form —
        // the trampoline mutates the receiver in place; returning the
        // same pointer through a second binding would have both `app`
        // and `app2` schedule `__cobrust_pit_app_drop` at scope exit on
        // the same box. The canonical .cb shape is therefore
        // `let _ = app.route("GET", "/x", handler)` (Ty::None discard).
        (PIT_APP_ADT, "route") => Some(EcoSig {
            runtime_symbol: "__cobrust_pit_app_route",
            params: vec![
                EcoParam::Value(Ty::Str),
                EcoParam::Value(Ty::Str),
                EcoParam::Callback(pit_handler_fn_ty()),
            ],
            ret: Ty::None,
            tier: PyCompatTier::Semantic,
        }),
        (PIT_APP_ADT, "serve_in_background") => Some(EcoSig::from_values(
            "__cobrust_pit_app_serve_in_background",
            vec![Ty::Str, Ty::Int],
            pit_server_handle_ty(),
            PyCompatTier::Semantic,
        )),
        _ => None,
    }
}

/// Is `name` a known built-in ecosystem-module alias (Q1)? The HIR
/// binds `import den` as a `DefKind::ImportAlias` with surface name
/// `den`; the typechecker uses this to mark the alias `def_id` so
/// `den.attr` accesses resolve against the manifest.
#[must_use]
pub fn is_ecosystem_module(name: &str) -> bool {
    matches!(name, "den" | "nest" | "strike" | "scale" | "molt" | "pit")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: extract the `Ty` from a `Value` slot, panicking on
    /// `Callback` (these tests assert pre-ADR-0073 manifest rows whose
    /// every slot is a `Value`).
    fn value_tys(params: &[EcoParam]) -> Vec<Ty> {
        params
            .iter()
            .map(|p| match p {
                EcoParam::Value(t) => t.clone(),
                EcoParam::Callback(_) => panic!("expected Value, got Callback"),
            })
            .collect()
    }

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
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
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
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
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
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
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
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, strike_response_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn strike_post_signature_takes_url_and_body() {
        let sig = lookup_module_fn("strike", "post").expect("strike.post in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_strike_post");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str, Ty::Str]);
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

    // ADR-0072 fourth-module proof — `scale` (msgpack, rebrand of
    // msgpack-python). No handles in the first proof; pure str→str.

    #[test]
    fn scale_is_a_known_module() {
        assert!(is_ecosystem_module("scale"));
    }

    #[test]
    fn scale_dumps_str_signature_is_str_to_str() {
        let sig = lookup_module_fn("scale", "dumps_str").expect("scale.dumps_str in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_scale_dumps_str");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, Ty::Str);
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn scale_loads_str_signature_is_str_to_str() {
        let sig = lookup_module_fn("scale", "loads_str").expect("scale.loads_str in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_scale_loads_str");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, Ty::Str);
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn unknown_scale_fn_is_none() {
        assert!(lookup_module_fn("scale", "nope").is_none());
    }

    // ADR-0072 fifth-module proof — `molt` (datetime, rebrand of
    // python-dateutil). Handle pattern (DateTime) + free `now()`.

    #[test]
    fn molt_is_a_known_module() {
        assert!(is_ecosystem_module("molt"));
    }

    #[test]
    fn molt_datetime_handle_id_recognized_and_in_reserved_block() {
        assert!(is_ecosystem_handle(MOLT_DATETIME_ADT));
        // Per-module 256-slot reservation: molt lives in the FOURTH
        // block (scale reserves the third for a future bytes ABI but
        // ships no handles today). Const-block so the compile-time-
        // constant comparisons trip a real ABI mistake (someone
        // bumping `ECO_ADT_BASE` without resizing) rather than a
        // clippy::assertions_on_constants false-positive.
        const _: () = {
            assert!(MOLT_DATETIME_ADT.0 >= ECO_ADT_BASE + 0x300);
            assert!(MOLT_DATETIME_ADT.0 < ECO_ADT_BASE + 0x400);
        };
    }

    #[test]
    fn molt_datetime_drop_symbol_resolves() {
        assert_eq!(
            handle_drop_symbol(MOLT_DATETIME_ADT),
            Some("__cobrust_molt_datetime_drop")
        );
    }

    #[test]
    fn molt_now_signature_returns_datetime_handle() {
        let sig = lookup_module_fn("molt", "now").expect("molt.now in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_molt_now");
        assert!(sig.params.is_empty());
        assert_eq!(sig.ret, molt_datetime_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn molt_datetime_methods_resolve() {
        let iso = lookup_handle_method(&molt_datetime_ty(), "isoformat")
            .expect("DateTime.isoformat in manifest");
        assert_eq!(iso.runtime_symbol, "__cobrust_molt_datetime_isoformat");
        assert!(iso.params.is_empty());
        assert_eq!(iso.ret, Ty::Str);

        let stamp = lookup_handle_method(&molt_datetime_ty(), "unix_timestamp")
            .expect("DateTime.unix_timestamp in manifest");
        assert_eq!(
            stamp.runtime_symbol,
            "__cobrust_molt_datetime_unix_timestamp"
        );
        assert!(stamp.params.is_empty());
        assert_eq!(stamp.ret, Ty::Int);
    }

    #[test]
    fn molt_methods_only_match_datetime_receiver() {
        // Cross-handle: den.Connection / strike.Response must never
        // resolve molt methods.
        assert!(lookup_handle_method(&den_connection_ty(), "isoformat").is_none());
        assert!(lookup_handle_method(&strike_response_ty(), "unix_timestamp").is_none());
        // Non-handle receivers never match.
        assert!(lookup_handle_method(&Ty::Str, "isoformat").is_none());
        // Unknown method on the right receiver is None.
        assert!(lookup_handle_method(&molt_datetime_ty(), "nope").is_none());
    }

    #[test]
    fn unknown_molt_fn_is_none() {
        assert!(lookup_module_fn("molt", "nope").is_none());
    }

    // ADR-0073 — `pit` (Flask, web-server). First module with a
    // callback-typed parameter slot.

    #[test]
    fn pit_is_a_known_module() {
        assert!(is_ecosystem_module("pit"));
    }

    #[test]
    fn pit_app_handle_id_is_in_reserved_fifth_block() {
        assert!(is_ecosystem_handle(PIT_APP_ADT));
        assert!(is_ecosystem_handle(PIT_REQUEST_ADT));
        assert!(is_ecosystem_handle(PIT_RESPONSE_ADT));
        assert!(is_ecosystem_handle(PIT_SERVER_HANDLE_ADT));
        const _: () = {
            assert!(PIT_APP_ADT.0 >= ECO_ADT_BASE + 0x400);
            assert!(PIT_SERVER_HANDLE_ADT.0 < ECO_ADT_BASE + 0x500);
        };
    }

    #[test]
    fn pit_handle_drop_symbols_resolve() {
        assert_eq!(
            handle_drop_symbol(PIT_APP_ADT),
            Some("__cobrust_pit_app_drop")
        );
        assert_eq!(
            handle_drop_symbol(PIT_RESPONSE_ADT),
            Some("__cobrust_pit_response_drop")
        );
        assert_eq!(
            handle_drop_symbol(PIT_SERVER_HANDLE_ADT),
            Some("__cobrust_pit_server_handle_drop")
        );
        // ADR-0073 §2 D6 — Request is Rust-owned, never dropped from
        // `.cb`. The drop pass therefore must NOT schedule a foreign drop
        // for a `pit.Request` local.
        assert_eq!(handle_drop_symbol(PIT_REQUEST_ADT), None);
    }

    #[test]
    fn pit_app_constructor_returns_app() {
        let sig = lookup_module_fn("pit", "App").expect("pit.App in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_app_new");
        assert!(sig.params.is_empty());
        assert_eq!(sig.ret, pit_app_ty());
    }

    #[test]
    fn pit_text_response_takes_status_and_body() {
        let sig = lookup_module_fn("pit", "text_response").expect("pit.text_response in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_text_response");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int, Ty::Str]);
        assert_eq!(sig.ret, pit_response_ty());
    }

    #[test]
    fn pit_app_route_carries_callback_slot() {
        let sig = lookup_handle_method(&pit_app_ty(), "route").expect("App.route");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_app_route");
        assert_eq!(sig.params.len(), 3);
        // First two slots are Str values; third is the Callback. Use
        // explicit-variant matches over wildcards so a future EcoParam
        // variant doesn't silently widen the test's intent.
        match &sig.params[0] {
            EcoParam::Value(Ty::Str) => {}
            EcoParam::Value(other) => {
                panic!("first param must be Value(Str); got Value({other:?})")
            }
            EcoParam::Callback(_) => panic!("first param must be Value(Str), not Callback"),
        }
        match &sig.params[1] {
            EcoParam::Value(Ty::Str) => {}
            EcoParam::Value(other) => {
                panic!("second param must be Value(Str); got Value({other:?})")
            }
            EcoParam::Callback(_) => panic!("second param must be Value(Str), not Callback"),
        }
        match &sig.params[2] {
            EcoParam::Callback(fn_ty) => {
                assert_eq!(fn_ty.positional, vec![pit_request_ty()]);
                assert_eq!(*fn_ty.return_ty, pit_response_ty());
            }
            EcoParam::Value(other) => panic!("third param must be Callback; got Value({other:?})"),
        }
        // Returns `Ty::None` (not App) so `let _ = app.route(...)` is
        // the canonical single-binding form, dodging the double-drop
        // hazard from a hypothetical `let app2 = app.route(...)`
        // (route's trampoline mutates the receiver in place; the same
        // pointer flowing into two drop-eligible locals would
        // double-free at scope exit).
        assert_eq!(sig.ret, Ty::None);
    }

    #[test]
    fn pit_app_serve_in_background_takes_host_and_port() {
        let sig = lookup_handle_method(&pit_app_ty(), "serve_in_background")
            .expect("App.serve_in_background");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_app_serve_in_background");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str, Ty::Int]);
        assert_eq!(sig.ret, pit_server_handle_ty());
    }

    #[test]
    fn pit_request_has_no_methods_today() {
        // ADR-0073 §5 / first proof: the .cb handler receives a Request
        // and returns a Response; reading Request fields (path/method/
        // body) lands in a paired follow-up sprint along with the borrow
        // shims. The handler can still ignore the Request and emit a
        // canned Response (`pit.text_response(200, "pong")`) — the
        // "pong" first proof.
        assert!(lookup_handle_method(&pit_request_ty(), "path").is_none());
    }
}
