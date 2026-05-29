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

use cobrust_hir::BinOp;

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
// hood (rebrand of click) reserves the SIXTH block
// (`0xE000_0500..0xE000_05FF`) — second proof of the ADR-0073
// callback chain (`Command.handler(fn_name)`).

/// `AdtId` for the `pit.App` handle (ADR-0073 §2 D1).
pub const PIT_APP_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x400);
/// `AdtId` for the `pit.Request` handle (ADR-0073).
pub const PIT_REQUEST_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x401);
/// `AdtId` for the `pit.Response` handle (ADR-0073).
pub const PIT_RESPONSE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x402);
/// `AdtId` for the `pit.ServerHandle` handle (ADR-0073).
pub const PIT_SERVER_HANDLE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x403);
/// ADR-0080 Phase-1b-ii — SENTINEL `AdtId` for "any validated-body class"
/// in the `app.route_validated` callback `FnTy` (Q5). It is NOT a real
/// handle (no constructor, no `_drop` symbol — `handle_drop_symbol`
/// returns `None`); it is a placeholder in the 2nd-param slot of
/// [`pit_validated_handler_fn_ty`] that the callback-shape check
/// (`check_validated_body_param`) special-cases: it accepts ANY
/// field-tracked USER class (a `Ty::Adt` whose id is OUTSIDE the
/// ecosystem-handle range) and rejects a non-class 2nd param (e.g. `i64`)
/// or a 1-arg handler with `CallbackSignatureMismatch` (the §6 negatives).
/// Lives in the pit block (`0xE000_0404`) so the pit-block range assertion
/// still holds.
pub const PIT_VALIDATED_BODY_SENTINEL_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x404);

/// `AdtId` for the `hood.Command` handle (ADR-0073 second proof —
/// click-style command-callback wiring; the SIXTH per-module 256-slot
/// block `0xE000_0500..0xE000_05FF`).
pub const HOOD_COMMAND_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x500);

// ADR-0076 Phase 1 — `dora` (dora-rs robotics dataflow, ninth ecosystem
// module) handle ADT block reservation. The SEVENTH per-module 256-slot
// block (`0xE000_0600..0xE000_06FF`). `dora` ships TWO handles in its
// Phase-1 first proof:
// - `Node`            — the dataflow node handle the `.cb` source builds
//                       via `dora.Node("detector")`; owns the registered
//                       handler closure.
// - `Event`           — incoming dataflow event passed to a handler fn
//                       (Rust-owned per ADR-0073 §2 D6, mirrors
//                       pit.Request — `.cb` side never drops).
//
// THIRD module exercising the ADR-0073 cross-boundary callback chain
// (after pit + hood). Phase 1 runtime is SYNTHETIC — `dora.node(handler)`
// installs into a process-global slot and `node.run()` mocks one canned
// message arrival without the real dora-rs daemon (mirrors F65 synthetic-
// LLM provider pattern). Phase 2+3 will wire real dora-rs orchestration.
//
// Reserved-but-unused slots `0x602..0x6FF` are saved for Phase 2 follow-
// ups (ArrowArray, Metadata, Ros2Subscription, Operator handles).

/// `AdtId` for the `dora.Node` handle (ADR-0076 Phase 1; the SEVENTH
/// per-module 256-slot block `0xE000_0600..0xE000_06FF`).
pub const DORA_NODE_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x600);
/// `AdtId` for the `dora.Event` handle (ADR-0076 Phase 1). Rust-owned
/// per ADR-0073 §2 D6 — the trampoline allocates + frees the `Box<Event>`
/// per callback invocation; the `.cb` side must not drop a `dora.Event`
/// local (`handle_drop_symbol` returns `None`).
pub const DORA_EVENT_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x601);

// ADR-0072 — `coil` (numpy-rebrand, ndarray foundation) handle ADT
// block reservation. The EIGHTH per-module 256-slot block
// (`0xE000_0700..0xE000_07FF`). The seventh block (`0xE000_0600..`)
// is reserved for `dora` per ADR-0076 (2 ADT slots claimed in Phase 1,
// remaining 0x602..0x6FF reserved for Phase 2 ArrowArray/Metadata
// follow-ups), so `coil` takes the next block. `coil` ships ONE handle
// in its first proof — `Buffer`, a thin wrapper over `coil::Array` —
// wired off the proven data-module value-handle chain (no callbacks).
// Operator dispatch (`a + b`) + index dispatch (`a[i]`) are
// explicitly deferred to a sub-ADR per ADR-0072 §"coil deep
// operator/index" — first proof scope is constructors + repr only.

/// `AdtId` for the `coil.Buffer` handle (ADR-0072 8/8 module proof —
/// numpy ndarray foundation; the EIGHTH per-module 256-slot block
/// `0xE000_0700..0xE000_07FF`).
pub const COIL_BUFFER_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x700);

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

/// ADR-0080 Phase-1b-ii — the handler `FnTy` `pit.App.route_validated`
/// expects in its 3rd argument (Q5): a 2-arg top-level fn
/// `fn(pit.Request, <Body>) -> pit.Response`. The 2nd positional is the
/// SENTINEL [`PIT_VALIDATED_BODY_SENTINEL_ADT`], NOT a concrete class —
/// the manifest cannot name the user's body class. The callback-shape
/// gate (`check_validated_body_param`, reached via the `route_validated`
/// runtime-symbol special-case in `check_eco_sig`) substitutes the
/// "any field-tracked user class" rule for this slot: a 2nd param that is
/// a tracked class `Ty::Adt` (id OUTSIDE the ecosystem-handle range)
/// passes; a non-class 2nd param or a 1-arg handler is a
/// `CallbackSignatureMismatch` with a §2.5-B FIX (the §6 negatives). The
/// 1st param (`pit.Request`) and the return (`pit.Response`) unify
/// through the normal callback path.
#[must_use]
pub fn pit_validated_handler_fn_ty() -> FnTy {
    FnTy {
        positional: vec![
            pit_request_ty(),
            Ty::Adt(PIT_VALIDATED_BODY_SENTINEL_ADT, vec![]),
        ],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(pit_response_ty()),
    }
}

/// The Cobrust `Ty` for the `hood.Command` opaque handle (ADR-0073
/// second proof).
#[must_use]
pub fn hood_command_ty() -> Ty {
    Ty::Adt(HOOD_COMMAND_ADT, vec![])
}

/// The handler `FnTy` `hood.Command.handler` expects in its argument
/// (ADR-0073 second proof — click-style command callback). The
/// `.cb` source's `fn handle_greet() -> i64: …` compiles to a fn
/// with the fixed C-ABI shape (`extern "C" fn(*mut u8) -> *mut u8`)
/// per ADR-0073 §5.1 — the trampoline calls it with a null pointer
/// arg and discards the return pointer (the source-level
/// `-> i64` return is the user's exit-code intent; the wire-level
/// `*mut u8` is the marshalling shim).
#[must_use]
pub fn hood_command_handler_fn_ty() -> FnTy {
    FnTy {
        positional: vec![],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Int),
    }
}

/// The Cobrust `Ty` for the `coil.Buffer` opaque handle (ADR-0072
/// 8/8 first proof — numpy ndarray foundation; wraps `coil::Array`).
#[must_use]
pub fn coil_buffer_ty() -> Ty {
    Ty::Adt(COIL_BUFFER_ADT, vec![])
}

/// The Cobrust `Ty` for the `dora.Node` opaque handle (ADR-0076 Phase 1).
#[must_use]
pub fn dora_node_ty() -> Ty {
    Ty::Adt(DORA_NODE_ADT, vec![])
}

/// The Cobrust `Ty` for the `dora.Event` opaque handle (ADR-0076 Phase 1).
#[must_use]
pub fn dora_event_ty() -> Ty {
    Ty::Adt(DORA_EVENT_ADT, vec![])
}

/// The handler `FnTy` `dora.node` expects in its callback argument
/// (ADR-0076 Phase 1 + ADR-0073 §2 D1+D4): a top-level fn taking a
/// single `dora.Event` and returning an `i64` (the user-level exit-code
/// intent surfaces here, mirroring hood's `() -> i64` shape but with a
/// receiver-style Event arg like pit). The cross-boundary trampoline in
/// `cobrust-dora/src/cabi.rs` enforces the same C-ABI shape
/// (`extern "C" fn(*mut u8) -> *mut u8`) as pit + hood per ADR-0073 §5.1.
#[must_use]
pub fn dora_event_handler_fn_ty() -> FnTy {
    FnTy {
        positional: vec![dora_event_ty()],
        named: vec![],
        var_positional: None,
        var_keyword: None,
        return_ty: Box::new(Ty::Int),
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
        // ADR-0073 second proof — hood.Command opaque handle.
        HOOD_COMMAND_ADT => Some("__cobrust_hood_command_drop"),
        // ADR-0072 8/8 first proof — coil.Buffer opaque handle (the
        // EIGHTH and final ecosystem module — completes the
        // workspace-vendored cobra batch).
        COIL_BUFFER_ADT => Some("__cobrust_coil_buffer_drop"),
        // ADR-0076 Phase 1 — dora.Node opaque handle (ninth module).
        DORA_NODE_ADT => Some("__cobrust_dora_node_drop"),
        // DORA_EVENT_ADT — Rust-owned, never dropped from `.cb`
        // (ADR-0073 §2 D6, mirrors PIT_REQUEST_ADT pattern). The
        // trampoline allocates + frees the `Box<DoraEventHandle>` per
        // callback invocation.
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
        // ADR-0073 second proof — `hood` (click, CLI command parsing,
        // ecosystem rebrand of Python's `click` library) `.cb` wiring.
        // First proof exposes the trio sufficient to register a single
        // command with a callback and dispatch it:
        // - `hood.Command(name, help) -> Command` — construct a Command.
        // - `Command.handler(fn)`     — bind the click-style callback.
        // - `Command.run() -> i64`    — invoke the bound callback.
        // The handle methods are wired in `lookup_handle_method`.
        ("hood", "Command") => Some(EcoSig::from_values(
            "__cobrust_hood_command_new",
            vec![Ty::Str, Ty::Str],
            hood_command_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0072 8/8 first proof — `coil` (numpy ndarray, ecosystem
        // rebrand of Python's `numpy` library) `.cb` wiring. Pure
        // value-handle pattern (no callbacks): three constructors that
        // each return a fresh `Buffer` handle the caller owns +
        // scope-exit drops via `__cobrust_coil_buffer_drop`, plus one
        // read method that borrows the handle for printing.
        // - `coil.zeros(n) -> Buffer`        — n-element f64-zero
        //   buffer (1-D shape `[n]`).
        // - `coil.ones(n) -> Buffer`         — n-element f64-one buffer.
        // - `coil.eye(n) -> Buffer`          — `n x n` identity matrix
        //   (f64; the 2-D shape proves the chain handles non-1-D too).
        // - `coil.print_buffer(b) -> i64`    — print the buffer's repr
        //   to stdout (verifies handle pass-through; the `-> i64`
        //   return is a 0 sentinel for `let _ = ...` discard).
        //
        // Tier `Semantic` — numpy's bit-stable repr depends on a
        // column-aligned multi-line layout coil does NOT reproduce
        // (ADR-0013 §4); the values + shape + dtype are equivalent.
        // Operator dispatch (`a + b`) + index dispatch (`a[i]`) are
        // explicitly OUT of first-proof scope per ADR-0072
        // §"coil deep operator/index" — those want their own sub-ADR.
        ("coil", "zeros") => Some(EcoSig::from_values(
            "__cobrust_coil_zeros",
            vec![Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "ones") => Some(EcoSig::from_values(
            "__cobrust_coil_ones",
            vec![Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "eye") => Some(EcoSig::from_values(
            "__cobrust_coil_eye",
            vec![Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "print_buffer") => Some(EcoSig::from_values(
            "__cobrust_coil_print_buffer",
            vec![coil_buffer_ty()],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // Stream W P0 增量 (2026-05-29) — 8 free functions extending
        // coil toward "basic scientific computing" surface coverage
        // per the numpy-translation roadmap. All composed from the
        // existing reduce + constructors machinery; same value-handle
        // ABI as the first proof.
        //
        // Grid + broadcast + split (Buffer-returning):
        // - `coil.mgrid(start, stop) -> Buffer`    — 1-D form of mgrid.
        // - `coil.ogrid(start, stop) -> Buffer`    — 1-D form of ogrid.
        // - `coil.broadcast_to(a, n) -> Buffer`    — 1-D tile-to-n.
        // - `coil.split(a, n) -> Buffer`           — first chunk of n-way.
        //
        // Aggregate reductions (f64-returning):
        // - `coil.mean(a) -> f64`                  — arithmetic mean.
        // - `coil.median(a) -> f64`                — order statistic.
        // - `coil.std(a) -> f64`                   — population std.
        // - `coil.var(a) -> f64`                   — population variance.
        //
        // Tier `Semantic` — numpy's bit-exact reductions depend on
        // implementation-defined pairwise grouping; the values agree
        // to `rtol = 1e-12` on the M7.3 reduce corpus and that is the
        // contractual semantic-tier shape per ADR-0016.
        ("coil", "mgrid") => Some(EcoSig::from_values(
            "__cobrust_coil_mgrid",
            vec![Ty::Int, Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "ogrid") => Some(EcoSig::from_values(
            "__cobrust_coil_ogrid",
            vec![Ty::Int, Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "broadcast_to") => Some(EcoSig::from_values(
            "__cobrust_coil_broadcast_to",
            vec![coil_buffer_ty(), Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "split") => Some(EcoSig::from_values(
            "__cobrust_coil_split",
            vec![coil_buffer_ty(), Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "mean") => Some(EcoSig::from_values(
            "__cobrust_coil_mean",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "median") => Some(EcoSig::from_values(
            "__cobrust_coil_median",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "std") => Some(EcoSig::from_values(
            "__cobrust_coil_std",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "var") => Some(EcoSig::from_values(
            "__cobrust_coil_var",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        // ADR-0079 Phase 1 — minimal `.cb`-constructible 2-D / explicit-
        // data buffers, the genuine prerequisite for exercising the
        // `coil.linalg.*` sub-namespace on NON-identity matrices (the
        // only 2-D `.cb` constructor before this was `coil.eye(n)`, the
        // identity alone — degenerate for det/solve/inv proofs). Each is
        // an all-scalar-arg shim over the EXISTING `coil::array_f64(values,
        // shape)` Rust ctor (no `list[f64]`→coil marshalling — the cheapest
        // path per ADR-0079 §8 / the TEST prerequisite banner). Kept
        // deliberately minimal (fixed small shapes, no `np.matrix` legacy
        // footgun, §5 elegance ledger): a general `coil.array([[..]])` over
        // nested-list marshalling is a follow-up once `list[f64]`→coil
        // lands. Tier `Numerical` (these feed the rtol=1e-6 linalg gate).
        // - `coil.array2x2(a,b,c,d) -> Buffer`        — row-major `2 x 2`.
        // - `coil.array2x3(a,b,c,d,e,f) -> Buffer`    — row-major `2 x 3`
        //                                               (non-square, for the
        //                                               det shape-error test).
        // - `coil.array1d2(a,b) -> Buffer`            — 2-element 1-D vector
        //                                               with explicit data
        //                                               (an arbitrary RHS the
        //                                               `ones`/`mgrid` ctors
        //                                               cannot produce).
        ("coil", "array2x2") => Some(EcoSig::from_values(
            "__cobrust_coil_array2x2",
            vec![Ty::Float, Ty::Float, Ty::Float, Ty::Float],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "array2x3") => Some(EcoSig::from_values(
            "__cobrust_coil_array2x3",
            vec![
                Ty::Float,
                Ty::Float,
                Ty::Float,
                Ty::Float,
                Ty::Float,
                Ty::Float,
            ],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "array1d2") => Some(EcoSig::from_values(
            "__cobrust_coil_array1d2",
            vec![Ty::Float, Ty::Float],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // ADR-0076 Phase 1 — `dora` (dora-rs robotics dataflow,
        // ninth ecosystem module). Phase 1 ships SYNTHETIC runtime;
        // the explicit registration form `dora.node(handler)` stands in
        // for the Phase 2 `@dora.node(inputs=..., outputs=...)`
        // decorator desugar (see findings/f68-dora-phase1-followups).
        //
        // - `dora.Node(name) -> Node`    — construct a synthetic Node.
        // - `dora.node(handler) -> i64`  — install handler in the
        //                                  process-global slot
        //                                  (Phase 1 single-handler).
        //   The callback FnTy is `fn(dora.Event) -> i64` — `Event` arg
        //   matches pit.Request's borrow shape; `i64` return matches
        //   hood's exit-code intent.
        //
        // The handle methods (`Node.run`, `Node.shutdown`,
        // `Event.id`, `Event.data_str`) are wired in
        // `lookup_handle_method`.
        ("dora", "Node") => Some(EcoSig::from_values(
            "__cobrust_dora_node_new",
            vec![Ty::Str],
            dora_node_ty(),
            PyCompatTier::Semantic,
        )),
        ("dora", "node") => Some(EcoSig {
            runtime_symbol: "__cobrust_dora_node_node",
            params: vec![EcoParam::Callback(dora_event_handler_fn_ty())],
            ret: Ty::Int,
            tier: PyCompatTier::Semantic,
        }),
        // ADR-0078 backend Phase 2 — `fang` (auth/security, the
        // cobra-themed wrapper over the `argon2` crate; the TENTH
        // ecosystem module and FIRST backend Phase-2 crate). Pure
        // value-pattern, no handles, no `AdtId`:
        // - `hash_password(pw) -> str` returns the full argon2id PHC
        //   string (`$argon2id$…`) with a fresh random salt embedded.
        // - `verify_password(pw, hash) -> bool` is the FIRST `-> bool`
        //   value-fn return on the chain (prior value-fns are str→str /
        //   str→i64); constant-time, a wrong pw is a normal `false`.
        // Tier `Semantic` — the PHC hash string is nondeterministic (a
        // random salt per call), so this is behavioral parity (a hash
        // verifies the password that produced it) NOT bit-for-bit
        // output parity with any CPython oracle.
        ("fang", "hash_password") => Some(EcoSig::from_values(
            "__cobrust_fang_hash_password",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        ("fang", "verify_password") => Some(EcoSig::from_values(
            "__cobrust_fang_verify_password",
            vec![Ty::Str, Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        _ => None,
    }
}

/// ADR-0079 Q4-a — is `<module>.<subns>` a known ecosystem **sub-namespace**?
///
/// A sub-namespace is a numpy-style dotted grouping under an ecosystem
/// module (`coil.linalg`, mirroring `np.linalg`). It is NOT a bindable
/// handle (there is no state — ADR-0079 Q4-b rejected the handle form);
/// it is purely a name in the import manifest's namespace, off which
/// [`lookup_subnamespace_fn`] resolves a flat per-namespace-prefixed
/// runtime symbol (`__cobrust_coil_linalg_<fn>`). The first proof ships
/// exactly one: `coil.linalg`. Future numpy-style groupings
/// (`coil.fft`, `coil.special`, a re-homed `coil.random`) extend this
/// predicate + [`lookup_subnamespace_fn`] off the same rule.
#[must_use]
pub fn is_subnamespace(module: &str, subns: &str) -> bool {
    matches!((module, subns), ("coil", "linalg"))
}

/// ADR-0079 Q4-a — resolve a sub-namespaced free function
/// `<module>.<subns>.<fn>` (e.g. `coil.linalg.solve`) to its signature.
///
/// The `.cb` callee is `Attr(Attr(Name(coil-alias), "linalg"), "solve")`;
/// the type checker ([`crate::check`]) + MIR lowering recognise the
/// dotted base via [`is_subnamespace`] and dispatch the leaf here. The
/// returned `runtime_symbol` is a flat `__cobrust_coil_linalg_<fn>` — the
/// symbol space stays flat at the C-ABI (a new prefix sibling of the
/// existing `__cobrust_coil_*`, already covered by the `__cobrust_coil_`
/// build/intrinsics recognizer). The underlying numerical kernels
/// (`coil::linalg::{solve, det, inv}`) ALREADY EXIST and pass the
/// ADR-0017 `rtol=1e-6` differential gate — Phase 1 only WIRES them, so
/// tier `Numerical`. Returns `None` for an unknown member (the
/// `coil.linalg.solveX` typo case — surfaced as a compile-time
/// `UnknownName` by the caller, §2.5 compile-time-catch).
///
/// `solve(a, b) -> Buffer` (LU solve, `*gesv` analogue), `det(a) -> f64`
/// (LU determinant; the 0-d numpy scalar → `f64`, ADR-0077 Q2 / ADR-0079
/// §9 honesty), `inv(a) -> Buffer` (`solve(a, I)`). Shape / singularity
/// is invisible to the static type (a `coil.Buffer` carries no rank /
/// conditioning) — a non-square or singular input is a RUNTIME panic in
/// the shim (ADR-0079 Q4 / ADR-0017 `LinalgShapeError` / `SingularMatrix`),
/// the inherited ADR-0077 §11 shape-correctness-is-runtime-only deficit.
#[must_use]
pub fn lookup_subnamespace_fn(module: &str, subns: &str, func: &str) -> Option<EcoSig> {
    match (module, subns, func) {
        ("coil", "linalg", "solve") => Some(EcoSig::from_values(
            "__cobrust_coil_linalg_solve",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "linalg", "det") => Some(EcoSig::from_values(
            "__cobrust_coil_linalg_det",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("coil", "linalg", "inv") => Some(EcoSig::from_values(
            "__cobrust_coil_linalg_inv",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
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
        // ADR-0080 Phase-1b-ii — `app.route_validated(method, path,
        // handler)`: the type-driven request-validation route (Q5). SIBLING
        // of `route` — the only differences are (a) the runtime symbol
        // (`__cobrust_pit_app_route_validated`) and (b) the callback `FnTy`
        // is the 2-ARG validated-handler shape (`fn(pit.Request, <Body>) ->
        // pit.Response`) whose 2nd param is the validated-body class. The
        // existing `EcoParam::Callback` gate type-checks the 2-arg arity +
        // the `pit.Request` 1st param + the `pit.Response` return for free;
        // the body-class 2nd-param slot is special-cased (sentinel — see
        // `pit_validated_handler_fn_ty` + `check_validated_body_param`).
        // The MIR retarget injects a 4th schema-descriptor `Str` arg
        // synthesised from the body class's field table + refinement
        // side-table (ADR-0080 §5.4); the trampoline validates the JSON
        // body against it, dispatching on `Ok` / synthesising a typed 422
        // on `Err` WITHOUT entering the handler (footgun #1 + #2). `Ty::None`
        // return mirrors `route`'s in-place-effect discard discipline.
        (PIT_APP_ADT, "route_validated") => Some(EcoSig {
            runtime_symbol: "__cobrust_pit_app_route_validated",
            params: vec![
                EcoParam::Value(Ty::Str),
                EcoParam::Value(Ty::Str),
                EcoParam::Callback(pit_validated_handler_fn_ty()),
            ],
            ret: Ty::None,
            tier: PyCompatTier::Semantic,
        }),
        // ADR-0078 §6.1 Phase-1 — tower-http canned-preset middleware.
        //
        // `app.use_cors()` / `app.use_trace()` / `app.use_compression()`
        // are zero-value-arg, `Ty::None`-returning App methods that flip
        // a middleware flag on the live `App` (the cabi shim borrows
        // `&mut App`; the flag is read once by `serve` when the axum
        // `Router` is built, applying `CorsLayer::permissive()` /
        // `TraceLayer::new_for_http()` / `CompressionLayer::new()`).
        //
        // `Ty::None` return MIRRORS `route`'s discipline (NOT another App
        // handle): the effect is a side-effect on the receiver in place,
        // so returning the same pointer through a second binding would
        // alias a second drop-eligible App and double-fire
        // `__cobrust_pit_app_drop`. The canonical `.cb` shape is
        // `let _ = app.use_cors()` (Ty::None discard). No new handle, no
        // new `_drop` symbol — the cheapest ecosystem-chain extension
        // (ADR-0078 §6.1 "Honest difficulty read"). MUST be called BEFORE
        // serve (the before-serve contract — the flag is read at the
        // moment the Router is constructed).
        (PIT_APP_ADT, "use_cors") => Some(EcoSig::from_values(
            "__cobrust_pit_app_use_cors",
            vec![],
            Ty::None,
            PyCompatTier::Semantic,
        )),
        (PIT_APP_ADT, "use_trace") => Some(EcoSig::from_values(
            "__cobrust_pit_app_use_trace",
            vec![],
            Ty::None,
            PyCompatTier::Semantic,
        )),
        (PIT_APP_ADT, "use_compression") => Some(EcoSig::from_values(
            "__cobrust_pit_app_use_compression",
            vec![],
            Ty::None,
            PyCompatTier::Semantic,
        )),
        // ADR-0080 Phase-1b-iii — `app.serve_openapi(doc_path: str) -> None`:
        // the EXPLICIT OpenAPI-serving opt-in (§5.3 / the elegance-law — NOT
        // a magic auto-route, NOT an import-time side effect). Registers a
        // `GET <doc_path>` route serving the OpenAPI doc DERIVED from the
        // `route_validated` body-schema descriptors the App accumulated (the
        // SAME source the validator reads — footgun #4, cannot drift). One
        // `Ty::Str` value arg (the doc path), `Ty::None` return mirroring
        // `route` / `use_cors`'s in-place-effect discard discipline so a
        // `let _ = app.serve_openapi(...)` form does not alias a second
        // drop-eligible App handle. The cabi shim
        // (`__cobrust_pit_app_serve_openapi`) is in the `__cobrust_pit_*`
        // family, so the CLI pit-prefix recognizer matches it for free.
        (PIT_APP_ADT, "serve_openapi") => Some(EcoSig::from_values(
            "__cobrust_pit_app_serve_openapi",
            vec![Ty::Str],
            Ty::None,
            PyCompatTier::Semantic,
        )),
        (PIT_APP_ADT, "serve_in_background") => Some(EcoSig::from_values(
            "__cobrust_pit_app_serve_in_background",
            vec![Ty::Str, Ty::Int],
            pit_server_handle_ty(),
            PyCompatTier::Semantic,
        )),
        // F65 G2 — `app.run(host, port) -> i64`. Blocking variant of
        // `serve_in_background`: drives the singleton tokio runtime via
        // `Runtime::block_on` and returns 0 on a clean shutdown (currently
        // unreachable: the loop only exits on process kill). Mirrors the
        // Rust `App::run` ergonomic shape — the `.cb` source's
        // `return app.run("127.0.0.1", 8080)` replaces a busy-wait keep-
        // alive (the pit_pong_e2e first proof's `while i < 10000000000:`
        // counter loop) with the natural blocking call.
        (PIT_APP_ADT, "run") => Some(EcoSig::from_values(
            "__cobrust_pit_app_run",
            vec![Ty::Str, Ty::Int],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // F65 G1 — `req.body() -> str`. Borrow-shim returning a freshly-
        // allocated Cobrust `Str` carrying the request body as a UTF-8
        // string. Non-UTF-8 bytes are lossily replaced (the `.cb` `str`
        // contract is "always valid UTF-8"). The Request itself stays
        // Rust-owned (ADR-0073 §2 D6); only the returned Str is on the
        // `.cb` drop schedule.
        (PIT_REQUEST_ADT, "body") => Some(EcoSig::from_values(
            "__cobrust_pit_request_body",
            vec![],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // F65 G5 enabling — `req.path_param(name: str) -> str`. Returns
        // the captured value for `<name>` in the matched route pattern,
        // or empty string when the name is not a registered param (fail-
        // clean sentinel — matches the other shim discard channels).
        (PIT_REQUEST_ADT, "path_param") => Some(EcoSig::from_values(
            "__cobrust_pit_request_path_param",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0073 second proof — `hood.Command` handle methods.
        //
        // `command.handler(fn)` is the load-bearing callback site:
        // the parameter is an `EcoParam::Callback` whose `FnTy` is
        // `fn() -> i64`. The MIR lowering emits `Constant::FnRef(def_id)`
        // for this slot and codegen materialises the fn pointer via the
        // `function_ids` table.
        //
        // `command.run()` dispatches the bound callback. Returns the
        // i64 the callback returned (exit-code intent at the source
        // level), so a `.cb` `fn main() -> i64:` can directly
        // `return cmd.run()`.
        //
        // `handler` returns `Ty::Int` (not Command) to match pit's
        // `route -> Ty::None` discipline (mutation on the receiver in
        // place; the return-value channel is a no-op sentinel — i64
        // zero is harmless when the source pattern is `let _ = cmd.handler(...)`).
        (HOOD_COMMAND_ADT, "handler") => Some(EcoSig {
            runtime_symbol: "__cobrust_hood_command_handler",
            params: vec![EcoParam::Callback(hood_command_handler_fn_ty())],
            ret: Ty::Int,
            tier: PyCompatTier::Semantic,
        }),
        (HOOD_COMMAND_ADT, "run") => Some(EcoSig::from_values(
            "__cobrust_hood_command_run",
            vec![],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // ADR-0076 Phase 1 — `dora.Node` handle methods (synthetic
        // runtime).
        //
        // `node.run()` invokes the registered handler exactly once with
        // a canned ("camera", "frame_001") Event (Phase 1 single-tick
        // mock; Phase 2 replaces with the real `EventStream` loop).
        //
        // `node.shutdown()` flips a soft flag on the Node (Phase 1 no-op
        // toward dora coordinator; Phase 2 sends the real signal).
        (DORA_NODE_ADT, "run") => Some(EcoSig::from_values(
            "__cobrust_dora_node_run",
            vec![],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (DORA_NODE_ADT, "shutdown") => Some(EcoSig::from_values(
            "__cobrust_dora_node_shutdown",
            vec![],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // ADR-0076 Phase 1 — `dora.Event` borrow methods (mirror pit's
        // Request body() / path_param() pattern). Both borrow the
        // receiver; both allocate fresh Cobrust `Str` buffers the caller
        // owns + scope-exit drops via `__cobrust_str_drop`. The Event
        // itself stays Rust-owned (ADR-0073 §2 D6); only the returned
        // Strs are on the `.cb` drop schedule.
        (DORA_EVENT_ADT, "id") => Some(EcoSig::from_values(
            "__cobrust_dora_event_id",
            vec![],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        (DORA_EVENT_ADT, "data_str") => Some(EcoSig::from_values(
            "__cobrust_dora_event_data_str",
            vec![],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0077 Q5 / Phase 2a — `coil.Buffer` method-form op `a.dot(b)`.
        // The FIRST method-form ecosystem-handle operator (the §10 precedent
        // for any handle wanting `.dot` / `.transpose` / `.matmul`). Reuses
        // the ADR-0073 handle-method chain VERBATIM — no new mechanism: the
        // receiver is the implicit LHS Buffer (borrowed via the
        // `try_lower_ecosystem_call` Case-2 Move→Copy upgrade), the single
        // explicit param is the RHS Buffer. Returns a plain `f64`: Phase 2a
        // ships the 1-D dot product → scalar (`linalg::dot` at array.rs:494
        // returns a 0-d Array for 1-D × 1-D; the shim extracts the scalar,
        // mirroring `coil.mean`/`std`'s f64-scalar return ABI). The 2-D
        // matmul → `Buffer` rank case is a Phase-3 follow-up (a manifest can
        // carry only one return type; ADR-0077 §7 picks the scalar 1-D
        // first-proof here, recorded as the per-rank divergence). Length
        // mismatch is NOT in the type — the shim's runtime shape-check
        // aborts via `coil_panic` (ADR-0077 Q4 panic-on-violation), exactly
        // as `buffer_binop` does for `a + b`.
        (COIL_BUFFER_ADT, "dot") => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_dot",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        _ => None,
    }
}

/// Resolve a binary operator `lhs <op> rhs` where both operands are an
/// ecosystem handle that overloads the operator (ADR-0077 Q1 — the
/// FIRST ecosystem-handle operator). Returns the runtime symbol +
/// result type the MIR `lower_bin` Buffer guard retargets onto, or
/// `None` when the operator is not overloaded for the handle.
///
/// Phase 1 (ADR-0077 §3/§8) ships `coil.Buffer` `+` / `-` / `*` only —
/// same-shape, f64-only, elementwise. `/` / `%` / `**` / `@` and the
/// scalar-broadcast forms (`a + 1`) are explicit §12 deferrals and
/// return `None` here so `synth_bin` rejects them with a clear "operator
/// not yet supported on coil.Buffer" diagnostic rather than silently
/// admitting them.
///
/// The implicit receiver is the LHS (mirroring `lookup_handle_method`'s
/// receiver-is-implicit convention); the single `params` slot is the
/// RHS handle. This is the precedent-setting first operator entry — a
/// future `decimal.Decimal` / `fraction.Fraction` / matrix handle that
/// wants `a + b` adds its own `(AdtId, op)` arm here (ADR-0077 §10).
#[must_use]
pub fn lookup_buffer_binop(receiver: &Ty, op: BinOp) -> Option<EcoSig> {
    // Unwrap a shared borrow (`&a + &b` → both operands `Ty::Ref(Buffer)`)
    // so the LLM-idiomatic explicit-borrow form resolves identically to
    // the bare `a + b` form (ADR-0052a: `coil.Buffer` is non-Copy, so the
    // training-data-frequent reuse pattern `&a * &a` requires borrows).
    let resolved = match receiver {
        Ty::Ref(inner) => inner.as_ref(),
        other => other,
    };
    let Ty::Adt(id, _) = resolved else {
        return None;
    };
    match (*id, op) {
        (COIL_BUFFER_ADT, BinOp::Add) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_add",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::Sub) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_sub",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::Mul) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_mul",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        _ => None,
    }
}

/// The runtime symbol a `coil.Buffer` scalar index read (`a[i]`)
/// retargets onto (ADR-0077 Q2). Kept as a dedicated const rather than a
/// fake `lookup_handle_method` row (the source surface is `a[i]`
/// indexing, NOT an `a.getitem(i)` method call — §9 table row 3 picks
/// this cleaner shape). The shim is `(ptr, i64) -> f64`; the result type
/// is a plain `f64` (numpy's 0-d scalar is not a Cobrust type, ADR-0077
/// §4 — a known divergence in the coil PROVENANCE manifest).
#[must_use]
pub fn coil_buffer_getitem_symbol() -> &'static str {
    "__cobrust_coil_buffer_getitem"
}

/// The runtime symbol a `coil.Buffer` scalar index WRITE (`a[i] = v`)
/// retargets onto (ADR-0077 Q2 write-path, Phase 2a). Kept as a
/// dedicated const (the twin of [`coil_buffer_getitem_symbol`]) rather
/// than a fake method row — the source surface is an index-assign
/// statement `a[i] = v`, retargeted in the `lower_assign` Buffer branch
/// beside the Dict `d[k] = v` precedent (lower.rs:594), NOT a method
/// call. The shim is `(ptr, i64, f64) -> ()`: it borrows `a` mutably,
/// bounds-checks `i`, and writes `v` in place (sound because the `.cb`
/// scope owns the only handle to the box — ADR-0077 §4 / ADR-0072 Q4).
/// An out-of-bounds index aborts via `coil_panic` (ADR-0077 Q4).
#[must_use]
pub fn coil_buffer_setitem_symbol() -> &'static str {
    "__cobrust_coil_buffer_setitem"
}

/// The runtime symbol a `coil.Buffer` contiguous slice read
/// (`a[lo:hi]`) retargets onto (ADR-0077 Q2 slice-path, Phase 2a).
/// A dedicated const (the slice surface is `a[lo:hi]` index syntax with
/// an `IndexKind::Slice`, retargeted in the `lower_expr` Index arm
/// beside the scalar-getitem branch — NOT a method). The shim is
/// `(ptr, i64, i64) -> ptr`: it borrows `a`, bounds-checks `[lo, hi)`
/// against the first-axis length (an out-of-bounds `hi` aborts via
/// `coil_panic` per ADR-0077 Q4 panic-on-violation — the Cobrust-honest
/// trap rather than numpy's silent clamp), and returns a freshly-owned
/// `Buffer` (a COPY of `a[lo..hi]`) the `.cb` scope drops once. Phase 2a
/// is the simple contiguous `lo:hi` form only (step / negative bounds
/// are ADR-0077 §12 deferrals).
#[must_use]
pub fn coil_buffer_slice_symbol() -> &'static str {
    "__cobrust_coil_buffer_slice"
}

/// Resolve a parens-free attribute access `<receiver-handle>.<attr>`
/// (e.g. `a.shape`, `a.ndim`, `a.size`) to its runtime symbol + return
/// type (ADR-0077 Q3). The structural twin of [`lookup_handle_method`]
/// but for attributes (NO call parens) — numpy's `a.shape` is an
/// attribute, `a.dot(b)` is a method, and §2.5 training-data overlap is
/// higher when `a.shape` (parens-free) type-checks (that is exactly what
/// LLMs write). Returns `None` for non-handle receivers / unknown attrs.
///
/// Phase 1 (ADR-0077 §5): `shape` → owned `list[i64]` (reuses the
/// existing List drop schedule, ADR-0050c — the runtime allocates the
/// list, the `.cb` scope drops it once); `ndim` / `size` → by-value
/// `i64`. `dtype` is deferred to Phase 2+ (f64-only Phase 1 would ship a
/// constant).
#[must_use]
pub fn lookup_handle_attr(receiver: &Ty, attr: &str) -> Option<EcoSig> {
    let Ty::Adt(id, _) = receiver else {
        return None;
    };
    match (*id, attr) {
        (COIL_BUFFER_ADT, "shape") => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_shape",
            vec![],
            Ty::List(Box::new(Ty::Int)),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, "ndim") => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_ndim",
            vec![],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, "size") => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_size",
            vec![],
            Ty::Int,
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
    matches!(
        name,
        "den" | "nest" | "strike" | "scale" | "molt" | "pit" | "hood" | "coil" | "dora" | "fang"
    )
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

    // ADR-0078 backend Phase 2 — `fang` (auth/security, argon2 wrapper).
    // No handles; pure value pattern; FIRST `-> bool` value-fn return.

    #[test]
    fn fang_is_a_known_module() {
        assert!(is_ecosystem_module("fang"));
    }

    #[test]
    fn fang_hash_password_signature_is_str_to_str() {
        let sig =
            lookup_module_fn("fang", "hash_password").expect("fang.hash_password in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_fang_hash_password");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, Ty::Str);
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn fang_verify_password_signature_is_str_str_to_bool() {
        // The FIRST `-> bool` value-fn return on the ecosystem chain.
        let sig =
            lookup_module_fn("fang", "verify_password").expect("fang.verify_password in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_fang_verify_password");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(sig.ret, Ty::Bool);
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn unknown_fang_fn_is_none() {
        assert!(lookup_module_fn("fang", "nope").is_none());
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

    // F65 G1 — `req.body() -> str` Request borrow shim lands. The
    // previously-ratified `pit_request_has_no_methods_today` test is
    // superseded by the positive tests below (F65 explicitly directs
    // the demo-repair sprint to remove that test as part of closing G1).
    //
    // The other ADR-0073 §5 follow-ups (`req.method()`, `req.path()`)
    // are NOT in scope for the F65 sprint — the demo only needs `body()`
    // and `path_param()` to close. They land as a separate sprint when
    // the next demo / proof needs them.
    #[test]
    fn pit_request_body_method_returns_str() {
        let sig =
            lookup_handle_method(&pit_request_ty(), "body").expect("Request.body in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_request_body");
        assert!(sig.params.is_empty());
        assert_eq!(sig.ret, Ty::Str);
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn pit_request_path_param_method_takes_name_returns_str() {
        let sig = lookup_handle_method(&pit_request_ty(), "path_param")
            .expect("Request.path_param in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_request_path_param");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, Ty::Str);
    }

    #[test]
    fn pit_app_run_takes_host_and_port_returns_i64() {
        let sig = lookup_handle_method(&pit_app_ty(), "run").expect("App.run in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_pit_app_run");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str, Ty::Int]);
        assert_eq!(sig.ret, Ty::Int);
    }

    #[test]
    fn pit_request_still_has_no_drop_symbol() {
        // F65 closes the manifest gap for `body` + `path_param` but the
        // Request handle is STILL Rust-owned (ADR-0073 §2 D6 — the
        // trampoline allocates + frees the Box<Request> per callback
        // invocation). The drop schedule MUST therefore continue to
        // return None for PIT_REQUEST_ADT so the `.cb` side does not
        // double-free a Request local.
        assert_eq!(handle_drop_symbol(PIT_REQUEST_ADT), None);
    }

    // ADR-0073 second proof — `hood` (click, CLI commands). First module
    // wired off pit's proven callback chain.

    #[test]
    fn hood_is_a_known_module() {
        assert!(is_ecosystem_module("hood"));
    }

    #[test]
    fn hood_command_handle_id_is_in_reserved_sixth_block() {
        assert!(is_ecosystem_handle(HOOD_COMMAND_ADT));
        const _: () = {
            assert!(HOOD_COMMAND_ADT.0 >= ECO_ADT_BASE + 0x500);
            assert!(HOOD_COMMAND_ADT.0 < ECO_ADT_BASE + 0x600);
        };
    }

    #[test]
    fn hood_command_drop_symbol_resolves() {
        assert_eq!(
            handle_drop_symbol(HOOD_COMMAND_ADT),
            Some("__cobrust_hood_command_drop")
        );
    }

    #[test]
    fn hood_command_constructor_takes_name_and_help() {
        let sig = lookup_module_fn("hood", "Command").expect("hood.Command in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_hood_command_new");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(sig.ret, hood_command_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn hood_command_handler_carries_callback_slot() {
        let sig = lookup_handle_method(&hood_command_ty(), "handler").expect("Command.handler");
        assert_eq!(sig.runtime_symbol, "__cobrust_hood_command_handler");
        assert_eq!(sig.params.len(), 1);
        match &sig.params[0] {
            EcoParam::Callback(fn_ty) => {
                assert!(fn_ty.positional.is_empty());
                assert_eq!(*fn_ty.return_ty, Ty::Int);
            }
            EcoParam::Value(other) => {
                panic!("handler param must be Callback; got Value({other:?})")
            }
        }
        // Returns Ty::Int (zero sentinel) — matches pit.route's "no
        // double-alias of the receiver" pattern.
        assert_eq!(sig.ret, Ty::Int);
    }

    #[test]
    fn hood_command_run_returns_i64() {
        let sig = lookup_handle_method(&hood_command_ty(), "run").expect("Command.run");
        assert_eq!(sig.runtime_symbol, "__cobrust_hood_command_run");
        assert!(sig.params.is_empty());
        assert_eq!(sig.ret, Ty::Int);
    }

    #[test]
    fn hood_methods_only_match_command_receiver() {
        // Cross-handle: strike.Response must never resolve hood methods.
        //
        // F65 G2 note: `App.run` USED TO BE an exclusive hood method
        // proving cross-handle isolation, but F65 graduated `App.run` to
        // a real `pit.App` method (the demo's blocking `app.run(host,
        // port)` keep-alive). The isolation invariant is now proven via
        // `handler` (hood-only — pit's callback site is `route`, not
        // `handler`) and `nope` (unknown name).
        assert!(lookup_handle_method(&pit_app_ty(), "handler").is_none());
        assert!(lookup_handle_method(&strike_response_ty(), "handler").is_none());
        // Non-handle receivers never match.
        assert!(lookup_handle_method(&Ty::Str, "handler").is_none());
        // Unknown method on the right receiver is None.
        assert!(lookup_handle_method(&hood_command_ty(), "nope").is_none());
    }

    #[test]
    fn unknown_hood_fn_is_none() {
        assert!(lookup_module_fn("hood", "nope").is_none());
    }

    // ADR-0072 8/8 first proof — `coil` (numpy, ndarray foundation).
    // Last and EIGHTH ecosystem module — completes the cobra batch.

    #[test]
    fn coil_is_a_known_module() {
        assert!(is_ecosystem_module("coil"));
    }

    #[test]
    fn coil_buffer_handle_id_is_in_reserved_eighth_block() {
        assert!(is_ecosystem_handle(COIL_BUFFER_ADT));
        // Per-module 256-slot reservation: coil lives in the EIGHTH
        // block (`0xE000_0700..0xE000_07FF`); the SEVENTH block
        // (`0xE000_0600..0xE000_06FF`) is reserved for dora per
        // ADR-0076 (4 ADT slots claimed there).
        const _: () = {
            assert!(COIL_BUFFER_ADT.0 >= ECO_ADT_BASE + 0x700);
            assert!(COIL_BUFFER_ADT.0 < ECO_ADT_BASE + 0x800);
        };
    }

    #[test]
    fn coil_buffer_drop_symbol_resolves() {
        assert_eq!(
            handle_drop_symbol(COIL_BUFFER_ADT),
            Some("__cobrust_coil_buffer_drop")
        );
    }

    #[test]
    fn coil_zeros_signature_returns_buffer_handle() {
        let sig = lookup_module_fn("coil", "zeros").expect("coil.zeros in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_zeros");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn coil_ones_signature_returns_buffer_handle() {
        let sig = lookup_module_fn("coil", "ones").expect("coil.ones in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_ones");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_eye_signature_returns_buffer_handle() {
        let sig = lookup_module_fn("coil", "eye").expect("coil.eye in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_eye");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_print_buffer_takes_buffer_returns_int() {
        let sig = lookup_module_fn("coil", "print_buffer").expect("coil.print_buffer in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_print_buffer");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Int);
    }

    #[test]
    fn unknown_coil_fn_is_none() {
        assert!(lookup_module_fn("coil", "nope").is_none());
    }

    // Stream W P0 增量 (2026-05-29) — 8 free-function manifest tests.

    #[test]
    fn coil_mgrid_signature() {
        let sig = lookup_module_fn("coil", "mgrid").expect("coil.mgrid in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_mgrid");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int, Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_ogrid_signature() {
        let sig = lookup_module_fn("coil", "ogrid").expect("coil.ogrid in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_ogrid");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int, Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_broadcast_to_signature() {
        let sig = lookup_module_fn("coil", "broadcast_to").expect("coil.broadcast_to in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_broadcast_to");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_split_signature() {
        let sig = lookup_module_fn("coil", "split").expect("coil.split in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_split");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_mean_returns_float() {
        let sig = lookup_module_fn("coil", "mean").expect("coil.mean in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_mean");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_median_returns_float() {
        let sig = lookup_module_fn("coil", "median").expect("coil.median in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_median");
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_std_returns_float() {
        let sig = lookup_module_fn("coil", "std").expect("coil.std in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_std");
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_var_returns_float() {
        let sig = lookup_module_fn("coil", "var").expect("coil.var in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_var");
        assert_eq!(sig.ret, Ty::Float);
    }

    // ADR-0076 Phase 1 first proof — `dora` (dora-rs robotics dataflow,
    // ninth ecosystem module). Third module on the ADR-0073 callback
    // chain (after pit + hood).

    #[test]
    fn dora_is_a_known_module() {
        assert!(is_ecosystem_module("dora"));
    }

    #[test]
    fn dora_node_handle_id_is_in_reserved_seventh_block() {
        assert!(is_ecosystem_handle(DORA_NODE_ADT));
        assert!(is_ecosystem_handle(DORA_EVENT_ADT));
        // Per-module 256-slot reservation: dora lives in the SEVENTH
        // block (`0xE000_0600..0xE000_06FF`). Const-block so the
        // compile-time-constant comparisons trip a real ABI mistake
        // (someone bumping `ECO_ADT_BASE` without resizing) rather than a
        // clippy::assertions_on_constants false-positive at test time.
        const _: () = {
            assert!(DORA_NODE_ADT.0 >= ECO_ADT_BASE + 0x600);
            assert!(DORA_NODE_ADT.0 < ECO_ADT_BASE + 0x700);
            assert!(DORA_EVENT_ADT.0 >= ECO_ADT_BASE + 0x600);
            assert!(DORA_EVENT_ADT.0 < ECO_ADT_BASE + 0x700);
        };
    }

    #[test]
    fn dora_handle_drop_symbols_resolve() {
        assert_eq!(
            handle_drop_symbol(DORA_NODE_ADT),
            Some("__cobrust_dora_node_drop")
        );
        // ADR-0073 §2 D6 — Event is Rust-owned, never dropped from
        // `.cb`. Mirrors PIT_REQUEST_ADT's None entry.
        assert_eq!(handle_drop_symbol(DORA_EVENT_ADT), None);
    }

    #[test]
    fn dora_node_constructor_takes_name_returns_node_handle() {
        let sig = lookup_module_fn("dora", "Node").expect("dora.Node in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_dora_node_new");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, dora_node_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn dora_node_free_fn_carries_callback_slot() {
        let sig = lookup_module_fn("dora", "node").expect("dora.node in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_dora_node_node");
        assert_eq!(sig.params.len(), 1);
        match &sig.params[0] {
            EcoParam::Callback(fn_ty) => {
                assert_eq!(fn_ty.positional, vec![dora_event_ty()]);
                assert_eq!(*fn_ty.return_ty, Ty::Int);
            }
            EcoParam::Value(other) => {
                panic!("dora.node param must be Callback; got Value({other:?})")
            }
        }
        // Returns Ty::Int sentinel — matches hood.handler's "no
        // double-alias of the receiver" pattern (here the
        // "receiver" is the process-global handler slot).
        assert_eq!(sig.ret, Ty::Int);
    }

    #[test]
    fn dora_node_run_and_shutdown_return_i64() {
        let run = lookup_handle_method(&dora_node_ty(), "run").expect("Node.run in manifest");
        assert_eq!(run.runtime_symbol, "__cobrust_dora_node_run");
        assert!(run.params.is_empty());
        assert_eq!(run.ret, Ty::Int);

        let shutdown =
            lookup_handle_method(&dora_node_ty(), "shutdown").expect("Node.shutdown in manifest");
        assert_eq!(shutdown.runtime_symbol, "__cobrust_dora_node_shutdown");
        assert!(shutdown.params.is_empty());
        assert_eq!(shutdown.ret, Ty::Int);
    }

    #[test]
    fn dora_event_borrow_methods_return_str() {
        let id = lookup_handle_method(&dora_event_ty(), "id").expect("Event.id in manifest");
        assert_eq!(id.runtime_symbol, "__cobrust_dora_event_id");
        assert!(id.params.is_empty());
        assert_eq!(id.ret, Ty::Str);

        let data =
            lookup_handle_method(&dora_event_ty(), "data_str").expect("Event.data_str in manifest");
        assert_eq!(data.runtime_symbol, "__cobrust_dora_event_data_str");
        assert!(data.params.is_empty());
        assert_eq!(data.ret, Ty::Str);
    }

    #[test]
    fn dora_methods_only_match_correct_receiver() {
        // Cross-handle: pit.App + hood.Command must never resolve dora
        // methods.
        assert!(lookup_handle_method(&pit_app_ty(), "run").is_some()); // pit's own
        assert!(lookup_handle_method(&hood_command_ty(), "run").is_some()); // hood's own
        // But Node methods are NOT exposed on pit/hood handles.
        assert!(lookup_handle_method(&pit_app_ty(), "data_str").is_none());
        assert!(lookup_handle_method(&hood_command_ty(), "data_str").is_none());
        assert!(lookup_handle_method(&strike_response_ty(), "data_str").is_none());
        // Event methods are NOT exposed on Node receiver.
        assert!(lookup_handle_method(&dora_node_ty(), "id").is_none());
        assert!(lookup_handle_method(&dora_node_ty(), "data_str").is_none());
        // Node methods are NOT exposed on Event receiver.
        assert!(lookup_handle_method(&dora_event_ty(), "run").is_none());
        assert!(lookup_handle_method(&dora_event_ty(), "shutdown").is_none());
        // Non-handle receivers never match.
        assert!(lookup_handle_method(&Ty::Str, "id").is_none());
        // Unknown method on the right receiver is None.
        assert!(lookup_handle_method(&dora_node_ty(), "nope").is_none());
        assert!(lookup_handle_method(&dora_event_ty(), "nope").is_none());
    }

    #[test]
    fn unknown_dora_fn_is_none() {
        assert!(lookup_module_fn("dora", "nope").is_none());
    }

    #[test]
    fn dora_event_handler_fn_ty_is_event_to_int() {
        let fnty = dora_event_handler_fn_ty();
        assert_eq!(fnty.positional, vec![dora_event_ty()]);
        assert!(fnty.named.is_empty());
        assert!(fnty.var_positional.is_none());
        assert!(fnty.var_keyword.is_none());
        assert_eq!(*fnty.return_ty, Ty::Int);
    }
}
