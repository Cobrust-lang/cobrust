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

// ADR-0078 Phase-1c — `redis` (cache/KV, the redis-py rebrand) handle
// ADT block reservation. The NINTH per-module 256-slot block
// (`0xE000_0800..0xE000_08FF`) — the next free block past coil's
// `0x700` (the `0x200` scale gap is deliberately NOT reused: blocks
// stay monotonic with allocation order to match every existing comment
// + the per-block range assertions). `redis` ships ONE handle in its
// first proof — `Client`, a den.Connection-shaped wrapper over a single
// sync `redis::Connection` — wired off the proven den/strike handle
// chain (no callbacks; the sync path needs no async-收编, ADR-0078 §3.5).
// `0x801`+ stay free for a future `redis.Pipeline` / `redis.PubSub`.

/// `AdtId` for the `redis.Client` handle (ADR-0078 Phase-1c — cache/KV
/// over redis-rs sync path; the NINTH per-module 256-slot block
/// `0xE000_0800..0xE000_08FF`).
pub const REDIS_CLIENT_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x800);

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

/// The Cobrust `Ty` for the `redis.Client` opaque handle (ADR-0078
/// Phase-1c — cache/KV; wraps a sync `redis::Connection`).
#[must_use]
pub fn redis_client_ty() -> Ty {
    Ty::Adt(REDIS_CLIENT_ADT, vec![])
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
        // ADR-0078 Phase-1c — redis.Client opaque handle (eleventh
        // ecosystem module). Dropping the Client closes the underlying
        // sync TCP connection (RAII — no forgot-to-close footgun).
        REDIS_CLIENT_ADT => Some("__cobrust_redis_client_drop"),
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
        // ADR-0081 §5.3 Phase-1a — `pit.json_response(status, body) ->
        // Response`. SIBLING of `text_response`: the only delta is the 2nd
        // param. Instead of a `Str` body it takes the VALIDATED-BODY class
        // the `route_validated` handler holds — the boxed `serde_json::Value`
        // the validator already produced (`cabi.rs:464`). The shim
        // re-serialises that SAME Value via `Response::json(&*body)` +
        // `.with_status(status)` (`response.rs:49`/`74`), so the response
        // body cannot drift from the validated body (footgun #4 dropped).
        //
        // The 2nd param is the SENTINEL [`PIT_VALIDATED_BODY_SENTINEL_ADT`]
        // — the manifest cannot name the user's body class, exactly as
        // `route_validated`'s callback body slot. `check_eco_sig`'s
        // `EcoParam::Value` arm special-cases this sentinel to accept any
        // field-tracked user `Ty::Adt` (the SAME `is_tracked_body` rule the
        // callback-shape gate uses), so `json_response(201, body)`
        // type-checks where `body: CreateScore` is the tracked-body class.
        // The shim BORROWS the body box; the `route_validated` trampoline
        // still frees it exactly once (`cabi.rs:479`) — no double-free.
        ("pit", "json_response") => Some(EcoSig::from_values(
            "__cobrust_pit_json_response",
            vec![Ty::Int, Ty::Adt(PIT_VALIDATED_BODY_SENTINEL_ADT, vec![])],
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
        // #163 BATCH 18 — `coil.reshape(a, rows, cols) -> Buffer`. The 2-D
        // C / row-major reshape (the ADR-0077 Q5 two-scalar-arg honest first
        // proof; the shape-tuple `np.reshape(a, (m,n))` form is a deferral).
        // EXACTLY `broadcast_to`'s `[Buffer, Int]` shape + one more `Int`:
        // the GENERIC `try_lower_ecosystem_call` iterates these 3 params
        // (Buffer, Int, Int) over the SAME borrow-Buffer-arg path, so ZERO
        // batch-specific MIR. Exactly one of `rows` / `cols` may be `-1`
        // (inferred); a bad shape `coil_panic`s at runtime (numpy
        // `ValueError`). Tier `Semantic` (the manifest reshape-family tier).
        ("coil", "reshape") => Some(EcoSig::from_values(
            "__cobrust_coil_reshape",
            vec![coil_buffer_ty(), Ty::Int, Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // BATCH 19 — `coil.astype(a, dtype) -> Buffer`. The DTYPE-CONVERSION
        // op; also COMPLETES the dtype story (coil HAS int-dtype Buffers but
        // no `.cb` way to CREATE one until astype). The FIRST coil row whose
        // arg list mixes a Buffer with a `Ty::Str` — `dtype` is a RUNTIME
        // dtype NAME (`"int64"` / `"float64"` / `"float32"` / `"int32"` /
        // `"bool"`). The `[Buffer, Str]` arg list lowers via the GENERIC
        // `try_lower_ecosystem_call` Case-1 path with ZERO new MIR: each
        // `EcoParam::Value` auto-borrows in `lower_eco_arg` —
        // `upgrade_move_to_copy_for_eco_value` already upgrades BOTH a Str
        // (M-F.3.6 borrow-not-move) AND a Buffer handle (ADR-0077) to Copy,
        // exactly as dora `event.send_output(Str, Str)` proves the Str-arg
        // lowering + `coil.broadcast_to(a, n)` proves the Buffer-arg borrow.
        // The dtype Str crosses the C-ABI as a `*mut Str` buffer pointer (the
        // send_output ABI). A float→int cast TRUNCATES TOWARD ZERO; an
        // UNKNOWN dtype `coil_panic`s at runtime (the §2.5 honest-failure;
        // a NON-Str dtype arg is a COMPILE-TIME `unify_call_arg` reject).
        // Tier `Semantic` (the manifest's coil-family tier).
        ("coil", "astype") => Some(EcoSig::from_values(
            "__cobrust_coil_astype",
            vec![coil_buffer_ty(), Ty::Str],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #numpy BATCH 20 (2026-06-05) — `coil.arange(n) -> Buffer`. The
        // FINAL core numpy constructor; VERY HIGH-USE (LLMs write
        // `np.arange(n)` constantly). The 1-ARG (`stop`-only) form, the
        // EXACT `[Ty::Int] -> Buffer` arg shape as `coil.zeros(n)` (an
        // all-scalar-arg producer, NO Buffer input). Lowers via the GENERIC
        // `try_lower_ecosystem_call` `[Int] -> Buffer` path with ZERO new
        // MIR, exactly like `zeros`/`ones`/`eye`. The result is an `Int64`
        // buffer (`np.arange(<int>)` is `int64`-dtype, so a Float64 result
        // would DIVERGE); `n <= 0` is a valid EMPTY int64 buffer (a NEGATIVE
        // `n` gives empty, NOT an error). The 4-arg `arange(start, stop[,
        // step])` is a documented deferral (this fixed-arity EcoSig ships
        // only the dominant `arange(n)` form). Tier `Semantic` (the coil
        // family tier — repr layout differs from numpy's, values agree).
        ("coil", "arange") => Some(EcoSig::from_values(
            "__cobrust_coil_arange",
            vec![Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #numpy core constructor (2026-06-05) — `coil.array([list]) ->
        // Buffer`, the FUNDAMENTAL numpy constructor `np.array([...])`: the
        // BRIDGE from real `.cb` list data to a coil Buffer (parse → list →
        // array → stats). UNIQUE among coil rows: the arg is ELEMENT-DTYPE-
        // POLYMORPHIC (`list[int]` → an int64 Buffer, `list[float]` → a
        // float64 Buffer — `np.array([1,2,3]).dtype == int64`,
        // `np.array([1.0]).dtype == float64`), which an `EcoParam::Value`
        // (a CONCRETE arg type) cannot express. So the type-checker
        // SPECIAL-CASES `("coil","array")` in `try_synth_ecosystem_call`
        // BEFORE `check_eco_sig` (the SAME shape as ADR-0090's
        // `try_synth_reduce_builtin` reading `Ty::List(elem)`): it accepts a
        // `list[int|float]` arg and returns `coil_buffer_ty()` (the dtype is a
        // RUNTIME property of the Buffer, NOT a static type — so the return is
        // uniform). The MIR ecosystem-call lowering then picks
        // `__cobrust_coil_array_int` vs `_float` per the list's STATIC element
        // type (the ADR-0089/0090 dest/element-type dispatch). This EcoSig
        // exists ONLY so `lookup_module_fn` returns `Some` (the special-case
        // reads the real arg); its `runtime_symbol` is the float shim (the MIR
        // override supplies the int shim) and its `param` is a sentinel
        // `list[float]` (unused — the special-case intercepts the arg-check).
        // The NESTED 2-D form (`coil.array([[1,2],[3,4]])` from `list[list]`)
        // is a documented DEFERRAL (needs a recursive list read; this ships
        // the 1-D form). Tier `Semantic` (the coil family tier — the values
        // are numpy-exact; the repr layout differs). See ADR-0091.
        ("coil", "array") => Some(EcoSig::from_values(
            "__cobrust_coil_array_float",
            vec![Ty::List(Box::new(Ty::Float))],
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
        // #145 statistics gap-closure (2026-06-01) — 5 scalar aggregates
        // extending the mean/median/std/var family with the NaN-aware +
        // spread reducers most-used in real numpy code. Same Buffer→f64
        // value-handle ABI as mean/std/var (the borrow-shim path), except
        // `percentile` which takes a trailing `f64` quantile arg — the
        // FIRST coil aggregate with a scalar-besides-handle signature
        // (`(Buffer, f64) -> f64`), lowered by the SAME generic
        // `try_lower_ecosystem_call` (the `array2x2(f64,..)` path already
        // proves `Ty::Float` args cross). All BORROW the handle (the
        // shim never reboxes/frees it).
        //
        // - `coil.ptp(a) -> f64`              — peak-to-peak (max - min).
        // - `coil.nansum(a) -> f64`           — sum treating NaN as zero.
        // - `coil.nanmean(a) -> f64`          — mean ignoring NaN.
        // - `coil.nanstd(a) -> f64`           — population std ignoring NaN.
        // - `coil.percentile(a, q) -> f64`    — q-th percentile (linear).
        //
        // Tier `Semantic` — numpy's bit-exact reductions depend on
        // implementation-defined accumulation order; the VALUES agree to
        // `rtol = 1e-12` against the numpy 2.0.2 oracle (the `aggregates`
        // unit tests carry the bit-confirmed literals), the contractual
        // semantic-tier shape per ADR-0016.
        ("coil", "ptp") => Some(EcoSig::from_values(
            "__cobrust_coil_ptp",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "nansum") => Some(EcoSig::from_values(
            "__cobrust_coil_nansum",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "nanmean") => Some(EcoSig::from_values(
            "__cobrust_coil_nanmean",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "nanstd") => Some(EcoSig::from_values(
            "__cobrust_coil_nanstd",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "percentile") => Some(EcoSig::from_values(
            "__cobrust_coil_percentile",
            vec![coil_buffer_ty(), Ty::Float],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        // #145 array-MANIPULATION gap-closure (2026-06-01) — 6 Buffer-
        // RETURNING combine + reshape ops, the array-manipulation surface
        // most-used in real numpy code per §2.5. Same borrow-Buffer-args →
        // fresh-Buffer-return value-handle ABI as the `@` matmul operator
        // (`__cobrust_coil_buffer_matmul`) and `coil.linalg.solve` — the
        // Buffer args auto-borrow (Move→Copy) in `lower_eco_arg` and the
        // fresh return is drop-scheduled by `emit_ecosystem_call`. The
        // 2-Buffer-arg path (`concatenate`/`vstack`/`hstack`) is proven by
        // `coil.linalg.solve(a, b)`'s identical `(Buffer, Buffer) -> Buffer`
        // shape (NO `_=>"any"` MIR gap — the generic ecosystem-call lowering
        // iterates `sig.params` regardless of arity).
        //
        // - `coil.transpose(a) -> Buffer`        — reverse all axes (`a.T`).
        // - `coil.flatten(a) -> Buffer`          — 1-D C-order copy.
        // - `coil.ravel(a) -> Buffer`            — 1-D C-order copy (view-
        //                                          valued in numpy; owned here).
        // - `coil.concatenate(a, b) -> Buffer`   — join along axis 0.
        // - `coil.vstack(a, b) -> Buffer`        — stack row-wise.
        // - `coil.hstack(a, b) -> Buffer`        — stack column-wise.
        //
        // Tier `Semantic` — the VALUES + shape + dtype agree exactly with
        // numpy (`transpose`/`flatten`/`ravel`/`concatenate`/`vstack`/
        // `hstack` are pure layout/combine ops, no floating arithmetic); the
        // one intentional divergence is `ravel`'s view-vs-copy (numpy may
        // return a view, coil returns an owned copy with identical values)
        // and the equal-dtype combine contract (numpy promotes a mixed pair;
        // coil raises) — both documented in `manipulate.rs`.
        ("coil", "transpose") => Some(EcoSig::from_values(
            "__cobrust_coil_transpose",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "flatten") => Some(EcoSig::from_values(
            "__cobrust_coil_flatten",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "ravel") => Some(EcoSig::from_values(
            "__cobrust_coil_ravel",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "concatenate") => Some(EcoSig::from_values(
            "__cobrust_coil_concatenate",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "vstack") => Some(EcoSig::from_values(
            "__cobrust_coil_vstack",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "hstack") => Some(EcoSig::from_values(
            "__cobrust_coil_hstack",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #163 gap-closure BATCH 14 (2026-06-02) — the LINALG-EXTRACT ops
        // `diag` / `tril` / `triu`. Each is a 1-arg `Buffer -> Buffer` op
        // riding the IDENTICAL borrow-Buffer-arg → fresh-Buffer-return
        // value-handle ABI as the BATCH-2 reshape ops (`transpose` /
        // `flatten` / `ravel`) + the unary ufuncs (`abs` / `exp`) — the
        // Buffer arg auto-borrows (Move→Copy) in `lower_eco_arg`, the fresh
        // return is drop-scheduled by `emit_ecosystem_call` (NO `_=>"any"`
        // MIR gap; the generic ecosystem-call lowering iterates `sig.params`
        // regardless of arity). Codegen rides the SAME `coil_shape_ty`
        // `(ptr) -> ptr` extern as `transpose`. The only batch-specific
        // wrinkle is the cabi shims being FALLIBLE (a disallowed input RANK
        // `coil_panic`s) — fully inside the Rust kernel + the shim's
        // `buffer_unary_fallible` body, INVISIBLE to the type/MIR/codegen
        // layers (the opaque handle ABI is byte-identical).
        //
        // - `coil.diag(a) -> Buffer`  — SHAPE-DEPENDENT (`k=0`): a 1-D
        //   `(n,)` input → the `(n,n)` matrix with `a` on the main diagonal
        //   (`np.diag([1,2,3]) == [[1,0,0],[0,2,0],[0,0,3]]`); a 2-D `(r,c)`
        //   input → the 1-D main-diagonal extract, length `min(r,c)`
        //   (`np.diag([[1,2],[3,4]]) == [1,4]`).
        // - `coil.tril(a) -> Buffer` — LOWER triangle: keep ON+BELOW the
        //   main diagonal, ZERO ABOVE; SAME shape, 2-D-required.
        // - `coil.triu(a) -> Buffer` — UPPER triangle: keep ON+ABOVE, ZERO
        //   BELOW; SAME shape, 2-D-required.
        //
        // Tier `Semantic` — the VALUES + shape + dtype agree exactly with
        // numpy (pure structural extract/mask, no floating arithmetic;
        // dtype-preserving, the zero-fill is the dtype's zero). The two
        // intentional contracts (the `k=` diagonal-offset deferral —
        // `k=0` main diagonal only — and `tril`/`triu`'s 2-D requirement
        // vs numpy's ≥1-D batch form, a clean `coil_panic` trap) live in
        // the Rust kernel (`constructors.rs`) + are documented there.
        ("coil", "diag") => Some(EcoSig::from_values(
            "__cobrust_coil_diag",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "tril") => Some(EcoSig::from_values(
            "__cobrust_coil_tril",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "triu") => Some(EcoSig::from_values(
            "__cobrust_coil_triu",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #163 gap-closure BATCH 13 (2026-06-02) — the elementwise BINARY
        // min/max ufuncs `maximum` / `minimum` / `fmax` / `fmin`. Each is a
        // 2-Buffer `(Buffer, Buffer) -> Buffer` op riding the IDENTICAL
        // borrow-Buffer-args → fresh-Buffer-return value-handle ABI as the
        // `concatenate` / `vstack` / `hstack` combine ops above (and
        // `coil.linalg.solve`) — the Buffer args auto-borrow (Move→Copy) in
        // `lower_eco_arg`, the fresh return is drop-scheduled by
        // `emit_ecosystem_call` (NO `_=>"any"` MIR gap; the generic
        // ecosystem-call lowering iterates `sig.params` regardless of arity).
        //
        // - `coil.maximum(a, b) -> Buffer`  — elementwise max, PROPAGATES NaN.
        // - `coil.minimum(a, b) -> Buffer`  — elementwise min, PROPAGATES NaN.
        // - `coil.fmax(a, b) -> Buffer`     — elementwise max, IGNORES NaN.
        // - `coil.fmin(a, b) -> Buffer`     — elementwise min, IGNORES NaN.
        //
        // Tier `Numerical` (the elementwise-ufunc family tier) — the result
        // VALUES + NaN placement agree exactly with numpy. The NaN split
        // (`maximum`/`minimum` propagate; `fmax`/`fmin` ignore) + the
        // same-shape / same-dtype combine contract (numpy broadcasts +
        // promotes; coil raises `ValueError` via `coil_panic`) live entirely
        // in the Rust kernel (`elementwise.rs`) — both documented there.
        ("coil", "maximum") => Some(EcoSig::from_values(
            "__cobrust_coil_maximum",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "minimum") => Some(EcoSig::from_values(
            "__cobrust_coil_minimum",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "fmax") => Some(EcoSig::from_values(
            "__cobrust_coil_fmax",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "fmin") => Some(EcoSig::from_values(
            "__cobrust_coil_fmin",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // #145 gap-closure BATCH 15 (2026-06-05) — the 2-Buffer FLOAT ufuncs
        // `arctan2` / `hypot` / `logaddexp`. Each is a 2-Buffer `(Buffer,
        // Buffer) -> Buffer` op riding the IDENTICAL borrow-Buffer-args →
        // fresh-Buffer-return value-handle ABI as the BATCH-13 min/max family
        // (`maximum` / `minimum` / `fmax` / `fmin`) + the `concatenate` /
        // `vstack` / `hstack` combine ops above (and `coil.linalg.solve`) —
        // the Buffer args auto-borrow (Move→Copy) in `lower_eco_arg`, the
        // fresh return is drop-scheduled by `emit_ecosystem_call` (NO
        // `_=>"any"` MIR gap; the generic ecosystem-call lowering iterates
        // `sig.params` regardless of arity — ZERO new MIR code).
        //
        // - `coil.arctan2(y, x) -> Buffer` — angle of `(x, y)`, ARG ORDER
        //   `(y, x)` Y FIRST (numpy); robotics-relevant (dora pillar).
        // - `coil.hypot(x, y) -> Buffer`   — Euclidean norm, OVERFLOW-SAFE.
        // - `coil.logaddexp(a, b) -> Buffer` — log-sum-exp, NUMERICALLY STABLE.
        //
        // Tier `Numerical` (the elementwise-ufunc family tier) — the result
        // VALUES agree with numpy within rtol. UNLIKE the DTYPE-PRESERVING
        // min/max family these are FLOAT-PROMOTING (int->f64, f32->f32 —
        // the BATCH-3 transcendental rule, applied per-operand to a
        // same-dtype pair). The float math + the same-shape / same-dtype
        // combine contract (numpy broadcasts + promotes; coil raises
        // `ValueError` via `coil_panic`) live entirely in the Rust kernel
        // (`elementwise.rs`) — documented there.
        ("coil", "arctan2") => Some(EcoSig::from_values(
            "__cobrust_coil_arctan2",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "hypot") => Some(EcoSig::from_values(
            "__cobrust_coil_hypot",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "logaddexp") => Some(EcoSig::from_values(
            "__cobrust_coil_logaddexp",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // #145 gap-closure BATCH 8 (2026-06-01) — `coil.where(cond, a, b)`,
        // the THREE-Buffer elementwise conditional select (`result[i] =
        // cond[i] truthy ? a[i] : b[i]`). This EXTENDS the 2-Buffer combine
        // ops above (and `coil.linalg.solve`) to a THIRD Buffer arg: the
        // `(Buffer, Buffer, Buffer) -> Buffer` shape rides the IDENTICAL
        // generic ecosystem-call machinery (the MIR Case-1 loop iterates
        // `sig.params` regardless of arity — 3 borrowed handles auto-borrow
        // via `lower_eco_arg`'s Move→Copy upgrade, the fresh return is
        // drop-scheduled by `emit_ecosystem_call`; NO `_=>"any"` MIR gap).
        // `cond` is typically a Bool-dtype Buffer from a `a < b` comparison
        // (ADR-0077); a numeric cond is truthy on any nonzero element.
        //
        // Tier `Semantic` — the selected VALUES + shape + dtype agree
        // exactly with numpy (`where` copies a[i]/b[i] verbatim, no
        // floating arithmetic; a NaN in a/b flows through as a value). The
        // intentional divergences (vs numpy's broadcasting + cross-dtype
        // promotion) are the equal-shape + equal-dtype contracts documented
        // in `manipulate::where_select` (both tracked follow-ups). This is
        // the 3-arg `np.where(cond, a, b)` form ONLY; the 1-arg
        // `np.where(cond)` index form (variable-length index arrays) is a
        // separate deferral.
        ("coil", "where") => Some(EcoSig::from_values(
            "__cobrust_coil_where",
            vec![coil_buffer_ty(), coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #145 gap-closure BATCH 9 (2026-06-01) — the FLAT search / order
        // surface (`sort` / `argsort` / `unique` / `flatnonzero`), each a
        // 1-arg `Buffer -> Buffer` op. SAME borrow-Buffer-arg →
        // fresh-Buffer-return value-handle ABI as the BATCH-2 reshape ops
        // (`__cobrust_coil_transpose` / `_flatten` / `_ravel`) and the
        // unary ufunc family: the single Buffer arg auto-borrows (Move→Copy)
        // in `lower_eco_arg` and the fresh return is drop-scheduled by
        // `emit_ecosystem_call` (NO `_=>"any"` MIR gap — the generic
        // ecosystem-call lowering iterates `sig.params` regardless of op,
        // identical 1-Buffer-arg shape to `coil.transpose`).
        //
        // The RETURN TYPE is `coil.Buffer` for all four, but the *element*
        // dtype split lives in the kernel (typecheck sees only the opaque
        // `Buffer` handle): `sort` / `unique` PRESERVE the input dtype;
        // `argsort` / `flatnonzero` produce an `Int64` Buffer (the indices).
        //
        // Tier `Semantic` — the VALUES + order + dtype agree exactly with
        // numpy (confirmed via `/opt/homebrew/bin/python3.11`, numpy 2.4.6):
        // `sort`/`argsort` place all `NaN` last; `unique` collapses multiple
        // `NaN` to one trailing `NaN` (numpy 1.21+); `flatnonzero` counts
        // `NaN` as nonzero (`!= 0.0`). `argsort` uses a STABLE sort (the
        // deterministic, reproducible tie-break). The intentional divergence
        // from numpy's optional `axis` arg (we always flatten no-axis) is
        // documented in `manipulate.rs`.
        ("coil", "sort") => Some(EcoSig::from_values(
            "__cobrust_coil_sort",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "argsort") => Some(EcoSig::from_values(
            "__cobrust_coil_argsort",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "unique") => Some(EcoSig::from_values(
            "__cobrust_coil_unique",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "flatnonzero") => Some(EcoSig::from_values(
            "__cobrust_coil_flatnonzero",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #145 unary TRANSCENDENTAL gap-closure (2026-06-01) — the FLOAT-
        // returning 1-arg elementwise ufunc family, the unary-math surface
        // most-used in real numpy code per §2.5. Same borrow-Buffer-arg →
        // fresh-Buffer-return value-handle ABI as the BATCH-2 reshape ops
        // (`__cobrust_coil_transpose` / `_flatten` / `_ravel`): the single
        // Buffer arg auto-borrows (Move→Copy) in `lower_eco_arg` and the
        // fresh return is drop-scheduled by `emit_ecosystem_call` (NO
        // `_=>"any"` MIR gap — the generic ecosystem-call lowering iterates
        // `sig.params` regardless of op, identical 1-Buffer-arg shape to
        // `coil.transpose`).
        //
        // - `coil.exp(a)   -> Buffer`  — e**x.
        // - `coil.log(a)   -> Buffer`  — natural log (base e).
        // - `coil.log10(a) -> Buffer`  — base-10 log.
        // - `coil.sqrt(a)  -> Buffer`  — square root.
        // - `coil.sin(a)   -> Buffer`  — sine (radians).
        // - `coil.cos(a)   -> Buffer`  — cosine (radians).
        // - `coil.tan(a)   -> Buffer`  — tangent (radians).
        //   (+ optional same-dtype-rule `exp2`/`log2`/`cbrt`/`sinh`/`cosh`/
        //    `tanh`.)
        //
        // Tier `Numerical` — these are floating arithmetic ufuncs whose
        // VALUES agree with numpy at rtol 1e-12 (f64) / 1e-6 (f32); the
        // domain-error inputs (`log(0) -> -inf`, `log(-1) -> NaN`,
        // `sqrt(-1) -> NaN`, `exp(710) -> +inf`) are IEEE-754 special
        // VALUES, not errors (numpy emits a RuntimeWarning, the array value
        // is identical). DTYPE: int / bool inputs promote to Float64,
        // Float32 stays Float32, Float64 stays Float64 (numpy promotes bool
        // to float16 — coil has no float16 so pins bool->Float64, a
        // value-faithful divergence documented in `elementwise.rs`).
        ("coil", "exp") => Some(EcoSig::from_values(
            "__cobrust_coil_exp",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "log") => Some(EcoSig::from_values(
            "__cobrust_coil_log",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "log10") => Some(EcoSig::from_values(
            "__cobrust_coil_log10",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "sqrt") => Some(EcoSig::from_values(
            "__cobrust_coil_sqrt",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "sin") => Some(EcoSig::from_values(
            "__cobrust_coil_sin",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "cos") => Some(EcoSig::from_values(
            "__cobrust_coil_cos",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "tan") => Some(EcoSig::from_values(
            "__cobrust_coil_tan",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "exp2") => Some(EcoSig::from_values(
            "__cobrust_coil_exp2",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "log2") => Some(EcoSig::from_values(
            "__cobrust_coil_log2",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "cbrt") => Some(EcoSig::from_values(
            "__cobrust_coil_cbrt",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "sinh") => Some(EcoSig::from_values(
            "__cobrust_coil_sinh",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "cosh") => Some(EcoSig::from_values(
            "__cobrust_coil_cosh",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "tanh") => Some(EcoSig::from_values(
            "__cobrust_coil_tanh",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // #145 unary INVERSE trig / hyperbolic gap-closure BATCH 16 — the
        // single-arg inverse forms (`arcsin`/`arccos`/`arctan`/`arcsinh`/
        // `arccosh`/`arctanh`), COMPLETING the unary transcendental family
        // (the documented BATCH-3 deferral; BATCH 15 shipped the 2-arg
        // `arctan2`). IDENTICAL 1-arg `Buffer -> Buffer` shape + FLOAT-
        // promoting dtype contract as the forward transcendentals above —
        // they ride the SAME generic ecosystem-call lowering (NO `_=>"any"`
        // MIR gap, the 1-Buffer-arg shape `coil.exp` proves) and the SAME
        // `coil_shape_ty` `(ptr) -> ptr` codegen extern.
        //
        // - `coil.arcsin(a)  -> Buffer`  — inverse sine ([-π/2,π/2]).
        // - `coil.arccos(a)  -> Buffer`  — inverse cosine ([0,π]).
        // - `coil.arctan(a)  -> Buffer`  — inverse tangent ((-π/2,π/2)).
        // - `coil.arcsinh(a) -> Buffer`  — inverse hyperbolic sine.
        // - `coil.arccosh(a) -> Buffer`  — inverse hyperbolic cosine.
        // - `coil.arctanh(a) -> Buffer`  — inverse hyperbolic tangent.
        //
        // Tier `Numerical` — floating-arithmetic ufuncs whose VALUES agree
        // with numpy at rtol 1e-12 (f64) / 1e-6 (f32). The out-of-domain
        // inputs are IEEE-754 special VALUES, NOT errors (numpy emits a
        // RuntimeWarning, the array value is identical): `arcsin(2)=NaN`,
        // `arccosh(0)=NaN`, `arctanh(±1)=±inf`, `arctanh(2)=NaN`. DTYPE:
        // int / bool -> Float64, Float32 stays Float32, Float64 stays
        // Float64 (the BATCH-3 transcendental promotion).
        ("coil", "arcsin") => Some(EcoSig::from_values(
            "__cobrust_coil_arcsin",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "arccos") => Some(EcoSig::from_values(
            "__cobrust_coil_arccos",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "arctan") => Some(EcoSig::from_values(
            "__cobrust_coil_arctan",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "arcsinh") => Some(EcoSig::from_values(
            "__cobrust_coil_arcsinh",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "arccosh") => Some(EcoSig::from_values(
            "__cobrust_coil_arccosh",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "arctanh") => Some(EcoSig::from_values(
            "__cobrust_coil_arctanh",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // #145 unary ROUNDING / SIGN gap-closure BATCH 4 (2026-06-01) — the
        // DTYPE-PRESERVING 1-arg elementwise ufunc family. SAME borrow-
        // Buffer-arg → fresh-Buffer-return value-handle ABI as the BATCH-3
        // transcendentals + BATCH-2 reshape ops (`__cobrust_coil_transpose`
        // / `_exp`): the single Buffer arg auto-borrows (Move→Copy) in
        // `lower_eco_arg` and the fresh return is drop-scheduled by
        // `emit_ecosystem_call` (NO `_=>"any"` MIR gap — the generic
        // ecosystem-call lowering iterates `sig.params` regardless of op,
        // identical 1-Buffer-arg shape to `coil.transpose` / `coil.exp`).
        //
        // - `coil.abs(a)    -> Buffer`  — absolute value (the MODULE fn
        //                                 `coil.abs(buf)`, distinct from any
        //                                 scalar `abs` method on Ty::Int /
        //                                 Ty::Float — resolved here via the
        //                                 `("coil", "abs")` module-fn path,
        //                                 NOT `lookup_handle_method`).
        // - `coil.floor(a)  -> Buffer`  — largest int <= x (int no-op).
        // - `coil.ceil(a)   -> Buffer`  — smallest int >= x (int no-op).
        // - `coil.round(a)  -> Buffer`  — round-half-to-EVEN (int no-op).
        // - `coil.trunc(a)  -> Buffer`  — truncate toward zero (int no-op).
        // - `coil.square(a) -> Buffer`  — x * x.
        // - `coil.sign(a)   -> Buffer`  — -1 / 0 / 1.
        //
        // Tier `Numerical` — the VALUES agree with numpy 2.x exactly
        // (`round` is banker's rounding, `sign(0)=0`, `sign(NaN)=NaN`); the
        // DTYPE is PRESERVING (int->int, f32->f32, f64->f64 — NOT the
        // BATCH-3 int->Float64 promotion) and `floor`/`ceil`/`round`/`trunc`
        // are NO-OPS on integer input. `Bool` input is a documented
        // value-faithful Semantic divergence (coil returns Bool unchanged;
        // numpy would emit float16 / int8 / raise) — see `elementwise.rs`.
        ("coil", "abs") => Some(EcoSig::from_values(
            "__cobrust_coil_abs",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "floor") => Some(EcoSig::from_values(
            "__cobrust_coil_floor",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "ceil") => Some(EcoSig::from_values(
            "__cobrust_coil_ceil",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "round") => Some(EcoSig::from_values(
            "__cobrust_coil_round",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "trunc") => Some(EcoSig::from_values(
            "__cobrust_coil_trunc",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "square") => Some(EcoSig::from_values(
            "__cobrust_coil_square",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "sign") => Some(EcoSig::from_values(
            "__cobrust_coil_sign",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // #163 PREDICATE gap-closure BATCH 12 (2026-06-02) — the
        // per-element predicate ufuncs `isnan` / `isinf` / `isfinite`.
        // Each is a 1-arg `Buffer -> Buffer` op, but UNLIKE every prior
        // unary ufunc the RESULT is a BOOL-dtype Buffer (the per-element
        // MASK), REGARDLESS of the input dtype (`np.isnan(x).dtype ==
        // bool`) — like the `a < b` comparison, but unary. The opaque
        // `coil.Buffer` handle is dtype-agnostic, so the EcoSig return is
        // the SAME `coil_buffer_ty()` as `transpose` / `abs` (the
        // bool-ness rides INSIDE the handle); codegen + MIR ride the SAME
        // `(ptr) -> ptr` generic path with ZERO new code. Tier `Strict` —
        // these are EXACT boolean predicates (no tolerance):
        //
        //   - `coil.isnan(a)    -> Buffer` (bool mask) — element IS NaN.
        //   - `coil.isinf(a)    -> Buffer` (bool mask) — element IS ±inf.
        //   - `coil.isfinite(a) -> Buffer` (bool mask) — NOT NaN AND NOT inf.
        //
        // INT / BOOL input: integers are ALWAYS finite -> `isnan` /
        // `isinf` are all-False, `isfinite` is all-True (the bool rule is
        // entirely inside the Rust kernel — see `elementwise.rs`).
        ("coil", "isnan") => Some(EcoSig::from_values(
            "__cobrust_coil_isnan",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Strict,
        )),
        ("coil", "isinf") => Some(EcoSig::from_values(
            "__cobrust_coil_isinf",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Strict,
        )),
        ("coil", "isfinite") => Some(EcoSig::from_values(
            "__cobrust_coil_isfinite",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Strict,
        )),
        // #145 REDUCTIONS gap-closure BATCH 5 (2026-06-01) — the reduction
        // family in THREE distinct return shapes, all on a single Buffer
        // arg. This batch is the FIRST coil surface to mix Buffer-return
        // AND scalar-return (i64 / bool) ops in one wave:
        //
        //   - `coil.cumsum(a)  -> Buffer`  — cumulative sum (no-axis FLATTEN
        //                                    to 1-D C-order, len a.size).
        //   - `coil.cumprod(a) -> Buffer`  — cumulative product (ditto).
        //   - `coil.argmin(a)  -> i64`     — flat C-order index of the min.
        //   - `coil.argmax(a)  -> i64`     — flat C-order index of the max.
        //   - `coil.any(a)     -> bool`    — True iff any element truthy.
        //   - `coil.all(a)     -> bool`    — True iff all elements truthy.
        //
        // WIRING (all ride the SAME generic `try_lower_ecosystem_call`
        // module-free-fn path — NO new MIR arm). The 3 return shapes differ
        // ONLY in the `EcoSig` ret `Ty`, which drives the `_ecoret` local
        // type + the codegen extern return type:
        //   - cumsum/cumprod → `coil_buffer_ty()` (Buffer): the borrow-arg →
        //     fresh-Buffer-return path proven by `coil.transpose`/`coil.exp`,
        //     codegen extern `(ptr) -> ptr` ≡ `coil_shape_ty`.
        //   - argmin/argmax → `Ty::Int` (i64): mirrors `coil.mean`'s scalar
        //     return, adapting f64 → i64 (`__cobrust_coil_mean(a) -> f64`
        //     becomes `__cobrust_coil_argmin(a) -> i64`); codegen extern
        //     `(ptr) -> i64` ≡ the `coil.Buffer.size`/`.ndim` `coil_attr_i64`
        //     shape.
        //   - any/all → `Ty::Bool`: mirrors `coil.mean` scalar shape +
        //     `fang.verify_password`'s `-> bool` return (the FIRST coil
        //     `-> bool` value fn); codegen extern `(ptr) -> i1` (Rust C-ABI
        //     `-> bool`), landing in the `_ecoret` Bool local.
        //
        // EMPTY-input semantics: `argmin`/`argmax` on an empty array RAISE
        // `ValueError` in numpy — coil cannot raise across the C-ABI, so the
        // shim `coil_panic`s (a clean abort, NEVER a Rust unwind across FFI;
        // tested e2e). `any([])==False` / `all([])==True` (vacuous). NaN is
        // TRUTHY for any/all (`np.any([nan])==True`) and PROPAGATES for
        // argmin/argmax (`np.argmax([1,nan,2])==1`).
        //
        // Tier `Semantic` — the VALUES + flat-index + dtype agree exactly
        // with numpy 2.x (the `reduce` / `aggregates` unit tests carry the
        // bit-confirmed oracle literals). The one documented dtype note:
        // `cumsum`/`cumprod` widen `int32` → `int64` (numpy's platform
        // default accumulator) and `bool` → `int64`; `float32` stays
        // `float32`, `float64` stays `float64` — see `reduce.rs`.
        ("coil", "cumsum") => Some(EcoSig::from_values(
            "__cobrust_coil_cumsum",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "cumprod") => Some(EcoSig::from_values(
            "__cobrust_coil_cumprod",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "argmin") => Some(EcoSig::from_values(
            "__cobrust_coil_argmin",
            vec![coil_buffer_ty()],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        ("coil", "argmax") => Some(EcoSig::from_values(
            "__cobrust_coil_argmax",
            vec![coil_buffer_ty()],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        ("coil", "any") => Some(EcoSig::from_values(
            "__cobrust_coil_any",
            vec![coil_buffer_ty()],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        ("coil", "all") => Some(EcoSig::from_values(
            "__cobrust_coil_all",
            vec![coil_buffer_ty()],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        // #145 gap-closure BATCH 7 (2026-06-01) — the VALUE reductions
        // `min` / `max` / `prod`, completing the scalar-reduction family.
        // Each is `(Buffer) -> Float` — the EXACT shape `coil.mean` proves
        // (codegen extern `(ptr) -> f64` ≡ `coil_agg_ty`), so there is NO
        // new MIR arm + NO new codegen extern type (they reuse the
        // `coil_agg_ty` rows beside `mean`/`median`/`std`/`var`).
        //
        // WHY f64-return now (superseding the BATCH-5 "min/max/prod
        // deferred" note for the f64 case): coil's scalar reductions ALL
        // return `f64` (mean/median/std/var/ptp/percentile) — f64 is the
        // established scalar-reduction convention. Every `.cb` Buffer
        // constructor today yields a Float64 buffer (no int-dtype `.cb`
        // constructor exists), so `min`/`max`/`prod -> f64` is numpy-EXACT
        // for every `.cb`-constructible buffer (`np.max(f64_array) -> f64`).
        // The numpy int-dtype-PRESERVING form (`np.max(int) -> int`) is the
        // SAME documented deferral as before (it needs a tagged / 0-d-Buffer
        // scalar return — its own pass); the f64-return ships the common
        // functionality NOW, value-faithfully + consistent with `mean`.
        //
        // EMPTY-input semantics: `min`/`max` of an empty array RAISE
        // `ValueError` in numpy → coil maps the kernel `Err` to a clean
        // `coil_panic` (mirror argmin/argmax; NEVER a Rust unwind across
        // FFI; tested e2e). `prod([]) == 1.0` (the multiplicative identity —
        // numpy parity, NOT a trap). NaN PROPAGATES for all three
        // (`np.max([1,nan,3]) == nan`), like `coil.mean`. f64 prod overflow
        // → `+inf` (numpy parity).
        //
        // Tier `Semantic` — the VALUES agree exactly with numpy 2.x (the
        // `aggregates` unit tests carry the bit-confirmed oracle literals).
        ("coil", "min") => Some(EcoSig::from_values(
            "__cobrust_coil_min",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "max") => Some(EcoSig::from_values(
            "__cobrust_coil_max",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "prod") => Some(EcoSig::from_values(
            "__cobrust_coil_prod",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        // #145 SCALAR-ARG ufunc gap-closure BATCH 6 (2026-06-01) — `clip` /
        // `power`, the FIRST Buffer-RETURNING ops taking EXTRA f64 SCALAR
        // args beside the handle. `clip(a, lo, hi)` is `(Buffer, Float,
        // Float) -> Buffer`; `power(a, p)` is `(Buffer, Float) -> Buffer`
        // (the SAME scalar-besides-handle shape as `coil.percentile(a, q)` —
        // `(Buffer, Float)` — except these RETURN a fresh Buffer instead of
        // an f64). The Buffer arg auto-borrows (Move→Copy) in `lower_eco_arg`
        // and the trailing f64 scalar(s) lower as plain operands (the MIR
        // retarget casts the `.cb` int / float literal to f64, exactly as
        // `percentile`'s `q` does), so there is NO new MIR arm — the generic
        // `try_lower_ecosystem_call` Case-1 loop iterates `sig.params`
        // regardless of arity / mix. The fresh return is drop-scheduled by
        // `emit_ecosystem_call`.
        //
        // - `coil.clip(a, lo, hi) -> Buffer`  — clamp to [lo, hi],
        //   DTYPE-PRESERVING (int->int, f->f; numpy `np.clip(int_array, lo,
        //   hi).dtype == int64`). Preserves NaN; the UPPER bound wins when
        //   lo > hi (numpy `minimum(maximum(a, lo), hi)`).
        // - `coil.power(a, p) -> Buffer`      — a ** p, FLOAT-PROMOTING with
        //   an f64 exponent (int->f64, f32->f32, f64->f64; numpy
        //   `np.power(int_array, 2.0).dtype == float64`). `power(x, 0.5) =
        //   sqrt(x)`, `power(x, 0) = 1`, `power(neg, 0.5) = NaN` (real
        //   branch). The f64 exponent sidesteps numpy's int**int<0 raise.
        //
        // Tier `Numerical` — floating arithmetic ufuncs (`power`) whose
        // VALUES agree with numpy at rtol 1e-12 (f64) / 1e-6 (f32); `clip`
        // is exact (a clamp, no arithmetic). Domain-error inputs
        // (`power(neg, 0.5) -> NaN`) are IEEE-754 VALUES, not traps — the
        // shims are TOTAL (no `coil_panic` domain path), see `elementwise.rs`.
        ("coil", "clip") => Some(EcoSig::from_values(
            "__cobrust_coil_clip",
            vec![coil_buffer_ty(), Ty::Float, Ty::Float],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        ("coil", "power") => Some(EcoSig::from_values(
            "__cobrust_coil_power",
            vec![coil_buffer_ty(), Ty::Float],
            coil_buffer_ty(),
            PyCompatTier::Numerical,
        )),
        // #145 REARRANGE / REPEAT gap-closure BATCH 10 (2026-06-02) —
        // `diff` / `flip` / `roll` / `repeat` / `tile`, Buffer-RETURNING ops
        // over the C-order FLATTENED array. They split on arity + output
        // shape, all riding the SAME generic `try_lower_ecosystem_call`
        // module-free-fn path (NO new MIR arm):
        //
        //   - `diff` / `flip` are 1-arg `(Buffer) -> Buffer` — the EXACT
        //     shape the BATCH-2 reshape ops (`transpose` / `flatten` /
        //     `ravel`) + the unary ufuncs prove (codegen extern `(ptr) ->
        //     ptr` ≡ `coil_shape_ty`).
        //   - `roll` / `repeat` / `tile` take a trailing i64 SCALAR
        //     (`(Buffer, Int) -> Buffer`) — the SAME scalar-besides-handle
        //     shape as the BATCH-6 `clip(a, lo, hi)` / `power(a, p)` f64
        //     scalar, but `Ty::Int` not `Ty::Float`. The i64 scalar lowers
        //     DIRECTLY (the MIR `EcoSig` param `Ty::Int` lowers the `.cb`
        //     int literal as an i64 operand — NO f64 cast, UNLIKE
        //     `percentile`'s `q`; the codegen extern-call int-width coercion
        //     forwards the i64 into the `(ptr, i64) -> ptr` extern). So
        //     there is NO new MIR arm — the generic Case-1 loop iterates
        //     `sig.params` regardless of arity / scalar mix. The fresh
        //     return is drop-scheduled by `emit_ecosystem_call`.
        //
        // SHAPE / DTYPE contract (numpy 2.x, oracle `python3.11`, numpy
        // 2.4.6): ALL FIVE are DTYPE-PRESERVING (`diff(int) -> int`, etc.).
        // - `coil.diff(a) -> Buffer`   — `a[1:] - a[:-1]` over the flattened
        //   array, 1-D length `max(size - 1, 0)` (`np.diff([1,4,9,16]) ==
        //   [3,5,7]`; len-≤1 / empty → empty).
        // - `coil.flip(a) -> Buffer`   — reverse the flattened array, 1-D
        //   same length reversed (`np.flip([1,2,3]) == [3,2,1]`).
        // - `coil.roll(a, k) -> Buffer`  — cyclic shift by `k`, reshaped
        //   BACK to the ORIGINAL shape (`np.roll([1,2,3,4],1) == [4,1,2,3]`;
        //   negative `k` rolls LEFT; `k` normalised mod size; empty → empty).
        // - `coil.repeat(a, n) -> Buffer` — repeat EACH element `n` times,
        //   1-D length `n * size` (`np.repeat([1,2],2) == [1,1,2,2]`;
        //   `n <= 0` → empty).
        // - `coil.tile(a, n) -> Buffer`  — tile the WHOLE flattened array
        //   `n` times, 1-D length `n * size` (`np.tile([1,2],2) ==
        //   [1,2,1,2]`; `n <= 0` → empty).
        //
        // Tier `Semantic` — the VALUES + shape + dtype agree EXACTLY with
        // numpy 2.x (the `manipulate` unit tests carry the bit-confirmed
        // oracle literals); these are integer-exact rearrange / repeat ops
        // (no floating arithmetic — `diff` is an exact subtract), so they
        // are TOTAL (no `coil_panic` domain path — an empty input or
        // `n <= 0` yields an empty Buffer), see `manipulate.rs`.
        ("coil", "diff") => Some(EcoSig::from_values(
            "__cobrust_coil_diff",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "flip") => Some(EcoSig::from_values(
            "__cobrust_coil_flip",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "roll") => Some(EcoSig::from_values(
            "__cobrust_coil_roll",
            vec![coil_buffer_ty(), Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "repeat") => Some(EcoSig::from_values(
            "__cobrust_coil_repeat",
            vec![coil_buffer_ty(), Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "tile") => Some(EcoSig::from_values(
            "__cobrust_coil_tile",
            vec![coil_buffer_ty(), Ty::Int],
            coil_buffer_ty(),
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
        // #145 gap-closure BATCH 11 (2026-06-02) — the spacing / value
        // CONSTRUCTORS `linspace` / `logspace` / `full`. ALL-SCALAR-ARG
        // Buffer producers (NO Buffer input) that allocate a fresh
        // `Float64` 1-D buffer — the EXACT shape of `coil.zeros(n)` /
        // `coil.array2x2(f64×4)` / `coil.array1d2(f64×2)`. They extend the
        // all-scalar ctor family with a MIXED-scalar-type arg list:
        // - `coil.linspace(start, stop, num) -> Buffer` —
        //   `[Float, Float, Int]`. `num` evenly-spaced f64 samples over
        //   `[start, stop]` INCLUSIVE (numpy `endpoint=True`). The FIRST
        //   coil ctor mixing `Ty::Float` + `Ty::Int` scalar args (proven
        //   to cross by `array2x2`'s f64 args + `roll`'s trailing i64).
        // - `coil.logspace(start, stop, num) -> Buffer` — same arg shape;
        //   `10 ** linspace(start, stop, num)`.
        // - `coil.full(n, value) -> Buffer` — `[Int, Float]`. `n` copies
        //   of `value` (`np.full(3, 5.0) == [5, 5, 5]`).
        //
        // Tier `Semantic` — `linspace` / `logspace` agree with numpy to
        // `rtol = 1e-12` (the docstring-corpus shape, float-producing per
        // `constructors.rs` `@py_compat(numerical(rtol=1e-12))`); `full`
        // is bit-exact (an exact copy, no floating arithmetic) but rides
        // the SAME tier for a uniform constructor surface. NO new MIR /
        // typecheck code — the generic `try_lower_ecosystem_call` Case-1
        // module-fn loop iterates `sig.params` regardless of arity or
        // scalar `Ty` (the `array2x2(f64×4)` + `roll(a, i64)` paths prove
        // both `Ty::Float` and `Ty::Int` scalar args lower + codegen).
        ("coil", "linspace") => Some(EcoSig::from_values(
            "__cobrust_coil_linspace",
            vec![Ty::Float, Ty::Float, Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "logspace") => Some(EcoSig::from_values(
            "__cobrust_coil_logspace",
            vec![Ty::Float, Ty::Float, Ty::Int],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        ("coil", "full") => Some(EcoSig::from_values(
            "__cobrust_coil_full",
            vec![Ty::Int, Ty::Float],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // #163 gap-closure BATCH 17 (2026-06-05) — the LINALG ops `trace` /
        // `norm` (SCALAR-return f64) + `outer` (MATRIX-return Buffer). Two
        // distinct ABIs, both already proven:
        //
        // - `coil.trace(a) -> f64` + `coil.norm(a) -> f64` ride the SAME
        //   `[Buffer] -> Float` shape + `(ptr) -> f64` `coil_agg_ty` extern
        //   as `mean` / `std` / `ptp` (the scalar-reduction family). `trace`
        //   = sum of the main diagonal `a[i,i]` for `i in 0..min(r,c)`,
        //   2-D-REQUIRED (a non-2-D input is a clean `coil_panic` trap; the
        //   offset/axes + ≥3-D forms are deferrals). `norm` = Frobenius / L2
        //   = `sqrt(sum of EVERY element squared)`, 1-D + 2-D (the `ord=`
        //   arg [L1, inf, nuclear] is a deferral — default L2 only). Both
        //   PROMOTE int/bool lanes to f64 in the sum.
        // - `coil.outer(a, b) -> Buffer` rides the SAME `[Buffer, Buffer] ->
        //   Buffer` shape + `(ptr, ptr) -> ptr` `coil_binop_ty` extern as
        //   `concatenate` / `vstack` / `maximum` (the 2-Buffer combine
        //   family). `outer[i,j] = a_flat[i] * b_flat[j]`, a 2-D
        //   `(a.size, b.size)` matrix (both flattened to 1-D first).
        //   DTYPE-PRESERVING with the SAME equal-dtype contract as
        //   `concatenate` (a mixed pair raises via `coil_panic`; numpy
        //   promotes).
        //
        // NO new MIR code (the scalar-reduction Buffer→f64 path + the
        // 2-Buffer→Buffer combine path both pre-exist — the generic
        // ecosystem-call lowering iterates `sig.params` regardless of arity)
        // and NO new codegen extern shape (both `coil_agg_ty` + `coil_binop_ty`
        // already declared).
        //
        // Tier: `trace` / `outer` `Semantic` — the VALUES + shape + dtype
        // agree exactly with numpy 2.4.6 (`trace` is an integer diagonal sum;
        // `outer` is a pure dtype-preserving product, no floating-arithmetic
        // accumulation order). `norm` `Numerical` — the `sqrt(sum-of-squares)`
        // is floating arithmetic (rtol per ADR-0017); the VALUES agree with
        // `np.linalg.norm` to `rtol = 1e-12` (the `aggregates` unit tests
        // carry the bit-confirmed literals).
        ("coil", "trace") => Some(EcoSig::from_values(
            "__cobrust_coil_trace",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("coil", "norm") => Some(EcoSig::from_values(
            "__cobrust_coil_norm",
            vec![coil_buffer_ty()],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("coil", "outer") => Some(EcoSig::from_values(
            "__cobrust_coil_outer",
            vec![coil_buffer_ty(), coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
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
        // ADR-0076 Phase 2 — multi-IO declaration shims. The
        // `@dora.node(inputs=[...], outputs=[...])` decorator desugar
        // (cobrust-hir) threads each declared port id through to the
        // SYNTHETIC trampoline as a `dora.declare_input(id)` /
        // `dora.declare_output(id)` register-call inserted at main's
        // prologue BEFORE `dora.node(handler)`. This is how the decorator's
        // IO metadata — VALIDATED-then-DROPPED in Phase 1 — finally reaches
        // the runtime: `node.run()` reads the declared-input queue and
        // fires the handler once per input (multi-input dispatch); the
        // `event.send_output` shim validates against the declared-output
        // set.
        //
        // Both are str→i64 value-fns (the `-> i64` is a 0 sentinel for the
        // `let _ = ...` discard the desugar emits), routed through the SAME
        // `lookup_module_fn` → `emit_ecosystem_call` chain as `dora.Node`.
        // Phase-1 single-input behavior is preserved when NO inputs are
        // declared (the explicit `dora.node(detect)` form in
        // `dora_hello_e2e`): the trampoline falls back to the single canned
        // `("camera", "frame_001")` event.
        ("dora", "declare_input") => Some(EcoSig::from_values(
            "__cobrust_dora_declare_input",
            vec![Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        ("dora", "declare_output") => Some(EcoSig::from_values(
            "__cobrust_dora_declare_output",
            vec![Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
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
        // ADR-0078 backend Phase 2 — `fang` JWT surface (HS256-signed
        // JSON Web Tokens), the SECOND value-fn family on the auth/
        // security module. Twins of the hash/verify rows: same flat
        // value-pattern (no handles, no `AdtId`), str-in / str-or-bool-out.
        // - `jwt_encode(claims_json, secret) -> str` mints an HS256 token
        //   for the claims JSON, signed with `secret`. Malformed claims
        //   JSON => the empty-string sentinel (fail-clean, never a panic).
        // - `jwt_verify(token, secret) -> bool` is TRUE iff the HS256
        //   signature validates against `secret`. The algorithm is PINNED
        //   to HS256 (NOT taken from the token header), so an `alg:none` /
        //   alg-swapped (RS256-header) forgery is REJECTED downstream in
        //   the cabi shim — the classic JWT algorithm-confusion footgun is
        //   closed. A tampered / wrong-secret / malformed token is a
        //   normal `false`, never a panic.
        // - `jwt_decode(token, secret) -> str` returns the claims JSON on
        //   a valid token, else the empty-string sentinel.
        // Tier `Semantic` — an HS256 token embeds no nondeterministic salt
        // (so two encodes of the same claims+secret match), but the parity
        // claim is behavioral (sign/verify round-trips, forgeries reject),
        // not bit-for-bit against a CPython `PyJWT` oracle.
        ("fang", "jwt_encode") => Some(EcoSig::from_values(
            "__cobrust_fang_jwt_encode",
            vec![Ty::Str, Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        ("fang", "jwt_verify") => Some(EcoSig::from_values(
            "__cobrust_fang_jwt_verify",
            vec![Ty::Str, Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        ("fang", "jwt_decode") => Some(EcoSig::from_values(
            "__cobrust_fang_jwt_decode",
            vec![Ty::Str, Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0078 Phase-1c — `redis` (cache/KV, the redis-py rebrand).
        // The free-function entrypoint `connect(url) -> Client` mirrors
        // `den.connect` (a stateful-resource handle from a single URL).
        // Tier `Semantic` — redis KV behaviour is preserved but not
        // bit-for-bit CPython parity (errors are fail-clean sentinels per
        // constitution §2.2, not the redis-py exception hierarchy). The
        // four `Client` methods are wired in `lookup_handle_method`.
        ("redis", "connect") => Some(EcoSig::from_values(
            "__cobrust_redis_connect",
            vec![Ty::Str],
            redis_client_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0083 — `math` (CPython `math`, the FIRST core-stdlib scalar
        // module). DISTINCT from `coil`: `coil.sqrt(a)` is an elementwise
        // BUFFER ufunc (`Buffer -> Buffer`); `math.sqrt(x)` is a SCALAR
        // `f64 -> f64` op. The `runtime_symbol` is the BARE libm symbol
        // (NOT a `__cobrust_math_*` shim) — libm is already linked (coil's
        // Rust kernels + the embedded Rust std pull it), so `math.sqrt(x)`
        // lowers to a direct `call double @sqrt(double)`. NO new crate, NO
        // cabi, NO ecosystem archive (ADR-0083 §"Lowering").
        //
        // Arg policy (§2.2 no silent coercion): every param is `Ty::Float`.
        // An `Int` arg (`math.sqrt(2)`) is REJECTED at type-check (Int never
        // unifies with Float in `unify_call_arg`) — write `math.sqrt(2.0)`.
        // This MIRRORS coil's scalar-arg convention (`coil.power(a, 0.0)`).
        //
        // Tier `Numerical` — libm's transcendentals may differ from CPython
        // in the last ULP (sqrt is IEEE-correctly-rounded so it is exact;
        // sin/cos/atan2/... are platform-libm and may vary in the final bit).
        // DOMAIN divergence: CPython `math.sqrt(-1)` / `math.log(0)` RAISE
        // `ValueError`; libm returns `NaN` / `-inf`. ADR-0083 chooses the
        // libm behaviour (the documented Numerical-tier surface) — NO silent
        // wrong-finite value, NO trap. Recorded in the @py_compat note.
        //
        // Single-arg `f64 -> f64` (17 of 18 — `atan2`/`pow`/`hypot` below
        // take two). Symbols: sqrt/sin/cos/tan/asin/acos/atan/sinh/cosh/
        // tanh/exp/log/log10/log2/fabs.
        ("math", "sqrt") => Some(EcoSig::from_values(
            "sqrt",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "sin") => Some(EcoSig::from_values(
            "sin",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "cos") => Some(EcoSig::from_values(
            "cos",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "tan") => Some(EcoSig::from_values(
            "tan",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "asin") => Some(EcoSig::from_values(
            "asin",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "acos") => Some(EcoSig::from_values(
            "acos",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "atan") => Some(EcoSig::from_values(
            "atan",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "sinh") => Some(EcoSig::from_values(
            "sinh",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "cosh") => Some(EcoSig::from_values(
            "cosh",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "tanh") => Some(EcoSig::from_values(
            "tanh",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "exp") => Some(EcoSig::from_values(
            "exp",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "log") => Some(EcoSig::from_values(
            "log",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "log10") => Some(EcoSig::from_values(
            "log10",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "log2") => Some(EcoSig::from_values(
            "log2",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "fabs") => Some(EcoSig::from_values(
            "fabs",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        // Two-arg `(f64, f64) -> f64` — `pow(x, y)`, `atan2(y, x)`,
        // `hypot(x, y)`. The generic ecosystem-call path lowers two scalar
        // args identically to one (it iterates `sig.params`); the coil
        // `(Buffer, f64) -> f64` `percentile` row proves a 2-slot scalar
        // signature crosses with no MIR change.
        ("math", "pow") => Some(EcoSig::from_values(
            "pow",
            vec![Ty::Float, Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "atan2") => Some(EcoSig::from_values(
            "atan2",
            vec![Ty::Float, Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        ("math", "hypot") => Some(EcoSig::from_values(
            "hypot",
            vec![Ty::Float, Ty::Float],
            Ty::Float,
            PyCompatTier::Numerical,
        )),
        // -- ADR-0083 PART-2: the INT / BOOL / scaling return shapes ----
        // Deferred from part-1 because they leave the clean `f64 -> f64`
        // libm batch. Two return shapes are ALREADY PROVEN by `coil` and
        // mirrored EXACTLY here (NO new MIR arm — the generic
        // ecosystem-call path drives the return TYPE off the `EcoSig` ret
        // `Ty`):
        //   [Float] -> Int  mirrors `coil.argmin` (Buffer -> i64): codegen
        //     extern `(f64) -> i64`, landing in the `.cb` `_ecoret` Int local.
        //   [Float] -> Bool mirrors `coil.any` / `coil.all` (Buffer -> bool):
        //     codegen extern `(f64) -> i1` (the Rust C-ABI `-> bool`), usable
        //     directly in an `if math.isnan(x):` condition.
        //
        // floor / ceil / trunc return CPython `int` and DIVERGE on a
        // negative input (the load-bearing distinction):
        //   floor(-1.5) == -2 (toward −∞), ceil(-1.5) == -1 (toward +∞),
        //   trunc(-1.5) == -1 (toward ZERO).
        // The `runtime_symbol` is a NEW `cobrust-stdlib` shim
        // (`__cobrust_math_floor_int`, `as i64`) — DISTINCT from the
        // f64-returning `__cobrust_math_floor` (the bare-function `floor(x)`
        // PRELUDE path), which this row does NOT touch. Strict-tier: the
        // result is an exact integer, no last-ULP question.
        ("math", "floor") => Some(EcoSig::from_values(
            "__cobrust_math_floor_int",
            vec![Ty::Float],
            Ty::Int,
            PyCompatTier::Strict,
        )),
        ("math", "ceil") => Some(EcoSig::from_values(
            "__cobrust_math_ceil_int",
            vec![Ty::Float],
            Ty::Int,
            PyCompatTier::Strict,
        )),
        ("math", "trunc") => Some(EcoSig::from_values(
            "__cobrust_math_trunc_int",
            vec![Ty::Float],
            Ty::Int,
            PyCompatTier::Strict,
        )),
        // isnan / isinf / isfinite — IEEE-754 classification, `-> bool`.
        // Strict-tier: the classification of an `f64` is unambiguous +
        // platform-stable. isnan(nan)=True/isnan(1.0)=False;
        // isinf(inf)=True; isfinite(1.0)=True / isfinite(inf)=False /
        // isfinite(nan)=False.
        ("math", "isnan") => Some(EcoSig::from_values(
            "__cobrust_math_isnan",
            vec![Ty::Float],
            Ty::Bool,
            PyCompatTier::Strict,
        )),
        ("math", "isinf") => Some(EcoSig::from_values(
            "__cobrust_math_isinf",
            vec![Ty::Float],
            Ty::Bool,
            PyCompatTier::Strict,
        )),
        ("math", "isfinite") => Some(EcoSig::from_values(
            "__cobrust_math_isfinite",
            vec![Ty::Float],
            Ty::Bool,
            PyCompatTier::Strict,
        )),
        // degrees / radians — exact `x * 180/π` / `x * π/180` scaling via
        // the `cobrust-stdlib` `to_degrees`/`to_radians` shims (`f64 -> f64`).
        // Strict-tier: degrees(pi) == 180.0, radians(180.0) == pi.
        ("math", "degrees") => Some(EcoSig::from_values(
            "__cobrust_math_degrees",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Strict,
        )),
        ("math", "radians") => Some(EcoSig::from_values(
            "__cobrust_math_radians",
            vec![Ty::Float],
            Ty::Float,
            PyCompatTier::Strict,
        )),
        // copysign(x, y) / fmod(x, y) — BARE libm two-arg symbols (like
        // part-1's `pow` / `atan2` / `hypot`), NO shim. BOTH Strict (exact):
        // copysign transplants the sign bit; fmod is the IEEE-754 floating
        // remainder, computed EXACTLY (no rounding) so bit-identical across
        // conforming libm — UNLIKE the transcendental pow/atan2/hypot, which
        // are Numerical/last-ULP. copysign(3.0, -1.0) == -3.0; fmod(7.0, 3.0) == 1.0.
        ("math", "copysign") => Some(EcoSig::from_values(
            "copysign",
            vec![Ty::Float, Ty::Float],
            Ty::Float,
            PyCompatTier::Strict,
        )),
        // fmod is the IEEE-754 floating remainder — an EXACT operation
        // (result = x - n*y computed exactly, no rounding), so it is
        // bit-identical across conforming libm (and to CPython's math.fmod,
        // also libm). Strict, NOT Numerical (unlike the transcendentals).
        ("math", "fmod") => Some(EcoSig::from_values(
            "fmod",
            vec![Ty::Float, Ty::Float],
            Ty::Float,
            PyCompatTier::Strict,
        )),
        // -- ADR-0084: `import re` (regular expressions) ------------------
        // The clean stateless subset of CPython's `re` — string-in,
        // str / list[str] / bool out, NO Match-object state (the
        // `.group()` form is a documented follow-up). Backed by the
        // `regex` crate (`cobrust-stdlib::re`). Tier `Semantic`: the Rust
        // regex flavor matches Python `re` for the common patterns
        // (classes, quantifiers, alternation, anchors, groups) but has NO
        // backreferences and NO lookaround (the linear-time guarantee) — a
        // documented divergence, NOT Strict parity.
        //
        // Each row reuses a PROVEN return shape (NO new MIR / codegen
        // mechanism — the generic ecosystem-call path drives args + return
        // off this `EcoSig`):
        //   sub      [Str, Str, Str] -> Str  — mirrors `den.connect`'s
        //     Str arg + the string-shim Str return (`__cobrust_str_replace`).
        //   findall  [Str, Str]      -> List(Str) — mirrors redis
        //     `lrange`/`smembers` (Str arg + `Ty::List(Box::new(Ty::Str))`
        //     return) + `__cobrust_llm_stream`'s list[str] mint.
        //   match/search [Str, Str]  -> Bool — mirrors `math.isnan`
        //     (`-> Ty::Bool`, an LLVM `i1`), usable in `if re.search(...):`.
        //
        // INVALID PATTERN: a malformed runtime pattern (`"["`) makes the
        // shim's `regex::Regex::new` return `Err`, which becomes a CLEAN
        // `__cobrust_panic` trap (non-zero exit), NEVER a silent no-match
        // (CPython raises `re.error`). A compile-time check for a LITERAL
        // pattern is a §2.5 follow-up (ADR-0084 §"Deferred").
        ("re", "sub") => Some(EcoSig::from_values(
            "__cobrust_re_sub",
            vec![Ty::Str, Ty::Str, Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // findall returns the FULL matches (the no-group form == CPython
        // exactly). CPython's group-capture behavior (1 group -> the
        // group's text; >1 -> tuples) is the documented deferral: a grouped
        // pattern returns the FULL match here (a Semantic divergence noted
        // in the ADR + docs).
        ("re", "findall") => Some(EcoSig::from_values(
            "__cobrust_re_findall",
            vec![Ty::Str, Ty::Str],
            Ty::List(Box::new(Ty::Str)),
            PyCompatTier::Semantic,
        )),
        // match = START-anchored (CPython `re.match`); search = ANYWHERE
        // (CPython `re.search`). The anchor is the load-bearing
        // distinction: `re.match("bc", "abc")` is False but
        // `re.search("bc", "abc")` is True. (CPython returns a Match object
        // / None; this first cut returns BOOL — the `.group()` form is a
        // documented follow-up.)
        ("re", "match") => Some(EcoSig::from_values(
            "__cobrust_re_match",
            vec![Ty::Str, Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        ("re", "search") => Some(EcoSig::from_values(
            "__cobrust_re_search",
            vec![Ty::Str, Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        // -- ADR-0086: `import random` (pseudo-random sampling) ----------
        // The scalar core of CPython's `random` — scalar-in / scalar-out,
        // the SIMPLEST ecosystem-call shape (no Str/list buffer marshalling,
        // unlike `re`). Backed by a thread-local `rand_pcg::Pcg64` MODULE-
        // GLOBAL RNG (`cobrust-stdlib::random`), mirroring CPython's hidden
        // module-level `Random` instance — distinct from `coil.random`'s
        // explicit `Generator` HANDLE.
        //
        // Each row reuses a PROVEN scalar shape (NO new MIR / codegen arm —
        // the generic ecosystem-call path drives args + return off this
        // `EcoSig`; codegen only declares the externs):
        //   random  []            -> Float — the FIRST 0-arg scalar stdlib
        //     fn; the f64 return mirrors `math.sqrt`'s `_ecoret` Float.
        //   randint [Int, Int]    -> Int   — INCLUSIVE [a, b] (CPython
        //     randint(1,6) can return 6; the shim uses `gen_range(a..=b)`).
        //   uniform [Float, Float]-> Float — uniform in [a, b].
        //   seed    [Int]         -> Int   — re-seed; reproducible stream.
        //     CPython returns None; we return a discarded i64 SENTINEL (the
        //     dora `event.send_output` pattern), discarded by the caller
        //     (`let _ = random.seed(n)`), avoiding the `Ty::None -> void`
        //     C-ABI mismatch.
        //
        // TIER `Semantic` (ADR-0086 §"Divergence"): CPython's `random` uses
        // the Mersenne Twister; Cobrust uses `Pcg64`. The two produce
        // DIFFERENT streams for the same seed — Cobrust does NOT reproduce
        // CPython's exact values. The CONTRACT is the DISTRIBUTION + Cobrust-
        // internal seed-reproducibility (`seed(k); x; seed(k); y => x == y`,
        // every host), NOT bit-identical agreement with CPython. Mirrors
        // `coil.random`'s honest "distribution-level, not bit-identical vs
        // numpy" posture.
        ("random", "random") => Some(EcoSig::from_values(
            "__cobrust_random_random",
            vec![],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("random", "randint") => Some(EcoSig::from_values(
            "__cobrust_random_randint",
            vec![Ty::Int, Ty::Int],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        ("random", "uniform") => Some(EcoSig::from_values(
            "__cobrust_random_uniform",
            vec![Ty::Float, Ty::Float],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        // `seed` returns an i64 SENTINEL (CPython returns None) — discarded
        // by the caller, the dora `event.send_output` discard pattern.
        ("random", "seed") => Some(EcoSig::from_values(
            "__cobrust_random_seed",
            vec![Ty::Int],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // -- ADR-0087: `import time` (timing + timestamps) ---------------
        // The timing core of CPython's `time` — scalar-in / scalar-out,
        // the SIMPLEST ecosystem-call shape (no Str/list buffer
        // marshalling, like `random`). Backed by `cobrust-stdlib::time`
        // over std `SystemTime` (wall clock) + a lazy-static `Instant`
        // origin (monotonic) + `thread::sleep`. The 4th core stdlib after
        // math / re / random.
        //
        // Each row reuses a PROVEN scalar shape (NO new MIR / codegen arm —
        // the generic ecosystem-call path drives args + return off this
        // `EcoSig`; codegen only declares the externs):
        //   time         []      -> Float — current Unix-epoch SECONDS
        //     (wall clock); 0-arg, the `random.random` 0-arg precedent.
        //   monotonic    []      -> Float — process-relative seconds,
        //     non-decreasing (a lazy-static `Instant`); for intervals.
        //   perf_counter []      -> Float — the SAME high-res monotonic
        //     clock as `monotonic` (one shared `START` Instant; CPython
        //     names them distinctly, Cobrust unifies them — ADR-0087).
        //   sleep        [Float] -> Int   — suspend the thread `secs` s;
        //     `secs <= 0.0` / NaN is a NO-OP (the shim guards the
        //     `Duration::from_secs_f64(neg)` panic; CPython raises
        //     ValueError, a no-op is the gentler safe path). CPython
        //     returns None; we return a discarded i64 SENTINEL (the
        //     `random.seed` / dora `send_output` discard pattern),
        //     avoiding the `Ty::None -> void` C-ABI mismatch.
        //
        // TIER `Semantic` (ADR-0087 §"Tier"): a clock is ENVIRONMENT STATE,
        // NOT reproducible — `time()` advances every call, `monotonic()`'s
        // origin is process-start, `sleep` is best-effort (the OS may
        // oversleep). Cobrust does NOT reproduce CPython's exact float
        // values (different epoch rounding, different monotonic origin).
        // The CONTRACT is the clock SEMANTICS (wall vs monotonic,
        // seconds-as-float, ordering/range), NOT bit-identity. Mirrors
        // `random`'s honest "raw read non-deterministic; only the contract
        // is assertable" posture.
        ("time", "time") => Some(EcoSig::from_values(
            "__cobrust_time_time",
            vec![],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("time", "monotonic") => Some(EcoSig::from_values(
            "__cobrust_time_monotonic",
            vec![],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        ("time", "perf_counter") => Some(EcoSig::from_values(
            "__cobrust_time_perf_counter",
            vec![],
            Ty::Float,
            PyCompatTier::Semantic,
        )),
        // `sleep` returns an i64 SENTINEL (CPython returns None) — discarded
        // by the caller (`let _ = time.sleep(d)`), the dora `event.send_output`
        // discard pattern. Takes one Float (seconds); a non-positive arg is a
        // shim-side no-op.
        ("time", "sleep") => Some(EcoSig::from_values(
            "__cobrust_time_sleep",
            vec![Ty::Float],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        _ => None,
    }
}

/// ADR-0083 — resolve a `math` module CONSTANT attribute (`math.pi`,
/// `math.e`, `math.tau`, `math.inf`, `math.nan`) to its `f64` value.
///
/// Unlike a function, a constant is a parens-FREE attribute access on the
/// `math` import alias (`math.pi`, NOT `math.pi()`). The type checker's
/// `ExprKind::Attr` synth + the MIR `Attr` lowering call this; the value is
/// emitted as a pure compile-time `Constant::Float` LLVM literal (NO runtime
/// call — a constant is just a number). This is the math-idiomatic surface
/// (CPython exposes `math.pi` as a module attribute, never `math.pi()`).
///
/// Returns the bit-exact `f64` for the name, or `None` for an unknown
/// constant (the caller surfaces an `UnknownName` — §2.5 compile-time-catch,
/// NOT a false-green `fresh_var()`).
///
/// `@py_compat`: `pi`/`e`/`tau` match CPython exactly (Rust `std::f64::consts`
/// are the SAME `f64` rounding of the mathematical constants CPython uses).
/// Strict-tier exact. NOTE: `math.inf`/`math.nan` are intentionally NOT exposed
/// here — the lexer tokenizes the bare words `inf`/`nan` as `f64` literals
/// (M-F.3.3), so `math.inf` fails to PARSE (`.` then a `Float("inf")` token);
/// the idiomatic Cobrust spelling is the BARE `inf` / `nan`. A `math.`-qualified
/// form is a deferred parser follow-up (ADR-0083 §Deferred).
#[must_use]
pub fn lookup_module_const(module: &str, name: &str) -> Option<f64> {
    if module != "math" {
        return None;
    }
    match name {
        "pi" => Some(std::f64::consts::PI),
        "e" => Some(std::f64::consts::E),
        "tau" => Some(std::f64::consts::TAU),
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
        // ADR-0076 Phase 2 — `event.send_output(output_id, payload)`. The
        // handler emits a Str payload on a DECLARED output port. The Event
        // is the ONLY handle in the handler's scope (the `node` local lives
        // in `main`, NOT in the callback — ADR-0076 §5 prose's
        // `node.send_output` cannot type-check inside the handler without a
        // separate ambient-node mechanism), so the send surface hangs off
        // the Event handle: ZERO new scoping machinery, mirrors the
        // existing `event.id()` / `event.data_str()` borrow-shim shape.
        //
        // Both args are `Ty::Str` values (output id + the str payload —
        // Phase 1's Arrow surface is i64+str scalar only per ADR-0076 §4
        // risk 3; `pa.array_i64(...)` list/dict payloads are deferred to
        // ADR-0076c). Returns `Ty::Int` (a 0 sentinel for the
        // `let _ = event.send_output(...)` discard; -1 on an UNDECLARED
        // output id — the trampoline surfaces that as a clear stderr
        // diagnostic, NOT a silent drop). The receiver borrows (Move→Copy
        // upgrade in `try_lower_ecosystem_call` Case 2); the Event stays
        // Rust-owned (the trampoline frees its `Box<Event>` on callback
        // return per ADR-0073 §2 D6).
        (DORA_EVENT_ADT, "send_output") => Some(EcoSig::from_values(
            "__cobrust_dora_event_send_output",
            vec![Ty::Str, Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // ADR-0076c (D)-B-1a — the typed-numeric Arrow↔coil.Buffer
        // round-trip. `event.data_buffer() -> coil.Buffer` reads a typed
        // input payload (the 5 overlapping dtypes Float64/Float32/Int64/
        // Int32/Bool decode INTO a `coil::Array`; non-numeric/unsupported
        // dtypes — the named ADR-0076c divergences — yield an empty
        // Buffer, and `event.data_str()` stays the Utf8 path). REUSES
        // `coil_buffer_ty()` (L292) VERBATIM — no new ADT, no new `Ty`:
        // the boxed `coil::Array` IS the `COIL_BUFFER_ADT` handle, so the
        // returned Buffer is `.cb`-owned + scope-exit-drops via the
        // EXISTING `__cobrust_coil_buffer_drop` (handle_drop_symbol(
        // COIL_BUFFER_ADT) at L364 — NO new drop symbol).
        (DORA_EVENT_ADT, "data_buffer") => Some(EcoSig::from_values(
            "__cobrust_dora_event_data_buffer",
            vec![],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0076c (D)-B-1a — `event.send_output_buffer(output_id,
        // buffer)` emits a typed-numeric Arrow array (bridged from the
        // `coil.Buffer`) on a DECLARED output port. A DISTINCT method name
        // (NOT a `send_output` overload) for §2.5 compile-time clarity — an
        // LLM picks `send_output_buffer` vs `send_output` unambiguously
        // (ADR-0076c §4.2 U4 + the manifest one-return-type-per-row
        // constraint ADR-0077 §7 also hit). Args: `Ty::Str` output-id +
        // `coil_buffer_ty()` Buffer. Returns `Ty::Int` (0 sentinel; -1 on
        // an UNDECLARED id — the runtime fail-closed backstop). The `buf`
        // is BORROWED (the cabi shim reads it, never frees it — the `.cb`
        // scope still drops it once); the receiver Event borrows (the
        // Move→Copy upgrade in `try_lower_ecosystem_call` Case 2). The
        // compile-time `DoraUnknownOutputId` reject (check.rs) fires for
        // THIS method too — a literal typo'd id is caught at `cobrust check`.
        (DORA_EVENT_ADT, "send_output_buffer") => Some(EcoSig::from_values(
            "__cobrust_dora_event_send_output_buffer",
            vec![Ty::Str, coil_buffer_ty()],
            Ty::Int,
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
        // ADR-0078 Phase-1c — `redis.Client` handle methods (the four
        // Phase-A KV verbs). All BORROW the receiver (the cabi shim takes
        // `&mut` internally — redis sync command methods take `&mut self`;
        // the borrow is invisible to the `.cb` aliasing model, exactly
        // like sequential `conn.execute` calls). The `.cb` names are the
        // readable redis-py-idiom verbs (`set`/`get`/`delete`/`exists`,
        // §2.5-aligned); `set` returns `None` (side effect — no second
        // drop-eligible handle minted, mirrors pit `app.route`); `get`
        // returns the str value ("" sentinel if absent, ADR-0078 §2.3-1);
        // `delete` returns the i64 count removed; `exists` returns a bool.
        (REDIS_CLIENT_ADT, "set") => Some(EcoSig::from_values(
            "__cobrust_redis_client_set",
            // Receiver implicit; explicit params are the key + value strs.
            vec![Ty::Str, Ty::Str],
            Ty::None,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "get") => Some(EcoSig::from_values(
            "__cobrust_redis_client_get",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "delete") => Some(EcoSig::from_values(
            "__cobrust_redis_client_delete",
            vec![Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "exists") => Some(EcoSig::from_values(
            "__cobrust_redis_client_exists",
            vec![Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        // ADR-0078 Phase-1c Phase-B — the top cache/counter/hash verbs
        // after the KV core. Same borrow-receiver discipline (the cabi
        // shim takes `&mut` internally), same readable redis-py-idiom
        // verbs (§2.5-aligned), same Result/sentinel-not-exceptions
        // surface. `expire`/`hset` return a `bool` (TTL-set? / new-field?);
        // `incr`/`incr_by` return the i64 new counter value; `hget`
        // returns the str value ("" sentinel if absent, mirroring `get`).
        (REDIS_CLIENT_ADT, "expire") => Some(EcoSig::from_values(
            "__cobrust_redis_client_expire",
            // Receiver implicit; explicit params are the key str + the
            // TTL seconds (i64).
            vec![Ty::Str, Ty::Int],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "incr") => Some(EcoSig::from_values(
            "__cobrust_redis_client_incr",
            vec![Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "incr_by") => Some(EcoSig::from_values(
            "__cobrust_redis_client_incr_by",
            // Key str + the integer delta to add.
            vec![Ty::Str, Ty::Int],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "hset") => Some(EcoSig::from_values(
            "__cobrust_redis_client_hset",
            // Key str + field str + value str.
            vec![Ty::Str, Ty::Str, Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "hget") => Some(EcoSig::from_values(
            "__cobrust_redis_client_hget",
            // Key str + field str.
            vec![Ty::Str, Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        // ADR-0078 Phase-1c Phase-C — the LIST + SET verbs. Same
        // borrow-receiver discipline (the cabi shim takes `&mut`
        // internally), same readable redis-py-idiom verbs (§2.5-aligned),
        // same Result/sentinel-not-exceptions surface. ALL scalar/str
        // returns (the get/hget/incr shapes) — `lpush`/`rpush`/`llen`/
        // `sadd`/`srem`/`scard` return the i64 count/length; `lpop`/`rpop`
        // return the popped str ("" sentinel if the list is empty/absent,
        // mirroring `get`); `sismember` returns a bool. The multi-element
        // LIST-of-str returns (`lrange`/`smembers`/`hgetall`/`hkeys`) ship
        // in Phase-1d below.
        (REDIS_CLIENT_ADT, "lpush") => Some(EcoSig::from_values(
            "__cobrust_redis_client_lpush",
            // Key str + the value str to prepend (head).
            vec![Ty::Str, Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "rpush") => Some(EcoSig::from_values(
            "__cobrust_redis_client_rpush",
            // Key str + the value str to append (tail).
            vec![Ty::Str, Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "lpop") => Some(EcoSig::from_values(
            "__cobrust_redis_client_lpop",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "rpop") => Some(EcoSig::from_values(
            "__cobrust_redis_client_rpop",
            vec![Ty::Str],
            Ty::Str,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "llen") => Some(EcoSig::from_values(
            "__cobrust_redis_client_llen",
            vec![Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "sadd") => Some(EcoSig::from_values(
            "__cobrust_redis_client_sadd",
            // Key str + the member str to add.
            vec![Ty::Str, Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "srem") => Some(EcoSig::from_values(
            "__cobrust_redis_client_srem",
            // Key str + the member str to remove.
            vec![Ty::Str, Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "sismember") => Some(EcoSig::from_values(
            "__cobrust_redis_client_sismember",
            // Key str + the member str to test.
            vec![Ty::Str, Ty::Str],
            Ty::Bool,
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "scard") => Some(EcoSig::from_values(
            "__cobrust_redis_client_scard",
            vec![Ty::Str],
            Ty::Int,
            PyCompatTier::Semantic,
        )),
        // ADR-0078 Phase-1c Phase-1d — the multi-element LIST-of-str
        // returns. Same borrow-receiver discipline, same readable
        // redis-py-idiom verbs (§2.5-aligned), same fail-clean surface (an
        // absent key / disconnected sentinel / command error mints an
        // EMPTY `list[str]`, never a panic). The return type is the
        // first-class `Ty::List(Box::new(Ty::Str))` — the SAME shape
        // `coil.shape -> Ty::List(Box::new(Ty::Int))` (above) prototypes
        // and `__cobrust_llm_stream -> list[str]` produces: codegen derives
        // the extern fn-type + return generically from this `EcoSig.ret`
        // (a `Ty::List` return maps to an LLVM ptr return, NO new codegen
        // fn-type), and the `.cb` for-loop / index / `Ty::List(Str)` drop
        // schedule consume + free it with NO new code. (The stale Phase-C
        // "redis has no list-handle precedent" deferral note is corrected
        // here + in cabi.rs + the redis docs.) `hgetall` returns a FLAT
        // `[k, v, k, v, ...]` list[str] — a documented Semantic divergence
        // from Python's dict, mirroring `coil.shape`'s list-vs-tuple note.
        (REDIS_CLIENT_ADT, "lrange") => Some(EcoSig::from_values(
            "__cobrust_redis_client_lrange",
            // Key str + the start + stop indices (i64, inclusive,
            // redis-native tail-relative on negatives; `0, -1` is the
            // whole list).
            vec![Ty::Str, Ty::Int, Ty::Int],
            Ty::List(Box::new(Ty::Str)),
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "smembers") => Some(EcoSig::from_values(
            "__cobrust_redis_client_smembers",
            vec![Ty::Str],
            Ty::List(Box::new(Ty::Str)),
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "hkeys") => Some(EcoSig::from_values(
            "__cobrust_redis_client_hkeys",
            vec![Ty::Str],
            Ty::List(Box::new(Ty::Str)),
            PyCompatTier::Semantic,
        )),
        (REDIS_CLIENT_ADT, "hgetall") => Some(EcoSig::from_values(
            "__cobrust_redis_client_hgetall",
            vec![Ty::Str],
            // FLAT [field, value, field, value, ...] — the documented
            // dict-vs-flat-list divergence (mirrors coil.shape).
            Ty::List(Box::new(Ty::Str)),
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
/// Phase 1 (ADR-0077 §3/§8) shipped `coil.Buffer` `+` / `-` / `*`; the
/// Phase-1 completion added `/` (true-division). ADR-0077 Phase-2/3 added
/// the six element-wise COMPARISON operators `<` / `<=` / `>` / `>=` /
/// `==` / `!=` — note these still return a **`coil.Buffer`** (a NumPy
/// Bool-dtype mask), NOT a Cobrust `bool` scalar (the runtime
/// `Array::{lt,le,gt,ge,eq_,ne_}` kernels always yield `Dtype::Bool`).
/// ADR-0077 §"@-operator" added `@` (matrix multiplication) →
/// `__cobrust_coil_buffer_matmul`, also returning a `coil.Buffer` (the
/// matrix / matrix-vector result; the 1-D·1-D scalar dot stays the
/// `a.dot(b)` method). `//` / `%` / `**` remain explicit §12 deferrals and
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
        // ADR-0077 Phase-1 completion — `a / b` is numpy **true division**
        // (`true_divide`): the `__cobrust_coil_buffer_div` shim forwards to
        // `Array::true_div`, which promotes int operands to FLOAT (so
        // int/int → float64, int/0 → IEEE inf, NOT the kernel's integer
        // floor-div + `IntegerDivisionByZero`). Same Buffer→Buffer shape as
        // Add/Sub/Mul; broadcasts free through the shared `buffer_binop`.
        (COIL_BUFFER_ADT, BinOp::Div) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_div",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0077 Phase-2/3 — element-wise COMPARISON `a cmp b`. The
        // result is a `coil.Buffer` of dtype Bool (a NumPy mask), NOT a
        // Cobrust `bool` scalar: `np.array([1,2,3]) < np.array([2,2,2])`
        // is `array([True, False, False])`. `ret` is `coil_buffer_ty()`
        // because the static handle type carries no dtype (the
        // dtype-parameterized `Ty::Adt(COIL_BUFFER_ADT, [Bool])` is a §12
        // deferral). Each forwards to `__cobrust_coil_buffer_<cmp>`,
        // wrapping `Array::{lt,le,gt,ge,eq_,ne_}` through the shared
        // broadcast-aware shim. NOTE: the `synth_bin` COMPARISON arm (not
        // the arithmetic arm) hosts the typecheck guard for these — `<`
        // etc. are matched separately from `+`/`-`/`*`/`/` there.
        (COIL_BUFFER_ADT, BinOp::Lt) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_lt",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::LtEq) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_le",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::Gt) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_gt",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::GtEq) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_ge",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::Eq) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_eq",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        (COIL_BUFFER_ADT, BinOp::NotEq) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_ne",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        // ADR-0077 §"@-operator" addition — `a @ b` is numpy MATRIX
        // multiplication (`np.matmul`): `Buffer @ Buffer -> Buffer`. Unlike
        // the elementwise `+`/`-`/`*`/`/` and the comparison ops (whose
        // result has the broadcast/operand shape), matmul CONTRACTS the
        // inner dims — `(m,k)@(k,n) -> (m,n)`, `(m,k)@(k,) -> (m,)`,
        // `(k,)@(k,n) -> (n,)` — and the 1-D·1-D `(k,)@(k,)` degenerate case
        // yields a 0-d `Buffer` (numpy's scalar; Cobrust has no 0-d scalar
        // type, ADR-0077 Q2 — the f64-returning `a.dot(b)` METHOD is the
        // surface for that case, so `@` ALWAYS types to `coil.Buffer`). The
        // static handle type carries no shape (Cobrust types are
        // shape-erased), so inner-dim conformability is a RUNTIME check
        // (panic-on-mismatch, like `a + b`'s broadcast guard — ADR-0077 Q4);
        // the dedicated `__cobrust_coil_buffer_matmul` cabi shim wraps
        // `Array::matmul` and `coil_panic`s on its shape `Err` (NEVER
        // unwinding across the C-ABI). Same `(Buffer, Buffer) -> Buffer`
        // manifest shape as `+`/`-`/`*`/`/`, so the MIR `lower_bin`
        // array-array guard (`lookup_buffer_binop`) and the codegen
        // `(ptr,ptr)->ptr` extern row drive it with NO matmul-specific arm.
        (COIL_BUFFER_ADT, BinOp::MatMul) => Some(EcoSig::from_values(
            "__cobrust_coil_buffer_matmul",
            vec![coil_buffer_ty()],
            coil_buffer_ty(),
            PyCompatTier::Semantic,
        )),
        _ => None,
    }
}

/// The runtime symbol a `coil.Buffer` **scalar-broadcast** arithmetic op
/// (`a ⊕ k`, where `k` is a python `int`/`float` literal) retargets onto
/// (ADR-0077 Phase-1 completion). NumPy's `array ⊕ scalar` is exactly a
/// length-1 broadcast (`a ⊕ array([k])`); the `*_scalar(a, k: f64)` shims
/// materialise `k` as a 1-element f64 `Buffer` and reuse the SAME
/// broadcast kernel as the array-array ops, so all four of `+`/`-`/`*`/`/`
/// get scalar support (and `/` true-divides). Returns `None` for any
/// non-arithmetic op (the scalar surface is the four elementwise binops
/// only). A dedicated lookup (the twin of [`lookup_buffer_binop`]) — the
/// scalar surface needs a distinct `(a, f64) -> ptr` shim, NOT the
/// `(a, b) -> ptr` array-array shape.
#[must_use]
pub fn lookup_buffer_scalar_binop(op: BinOp) -> Option<&'static str> {
    match op {
        BinOp::Add => Some("__cobrust_coil_buffer_add_scalar"),
        BinOp::Sub => Some("__cobrust_coil_buffer_sub_scalar"),
        BinOp::Mul => Some("__cobrust_coil_buffer_mul_scalar"),
        BinOp::Div => Some("__cobrust_coil_buffer_div_scalar"),
        _ => None,
    }
}

/// The runtime symbol a `coil.Buffer` **LEFT-scalar** arithmetic op
/// (`k ⊕ a`, scalar on the LEFT) retargets onto (ADR-0077 Phase-2/3 —
/// the mirror of [`lookup_buffer_scalar_binop`]'s right-scalar `a ⊕ k`).
///
/// The dispatch turns on whether `⊕` COMMUTES:
/// - `+` / `*` commute (`k + a == a + k`, `k * a == a * k`), so they
///   reuse the EXISTING right-scalar shims — no new C-ABI symbol.
/// - `-` / `/` do NOT commute (`k - a != a - k`); they need a REVERSED
///   shim that computes `k - a[i]` / `k / a[i]` (NOT the right-scalar
///   `a[i] - k`). These map onto the dedicated
///   `__cobrust_coil_buffer_{rsub,rdiv}_scalar` shims, which materialise
///   `k` as a length-1 buffer on the LEFT and reuse the array-array
///   kernel (cabi `buffer_binop_scalar_rev`). Keeping the `(ptr, f64) ->
///   ptr` ABI shape (same `coil_scalar_binop_ty` codegen extern) — only
///   the operand order inside the shim flips — is why a reversed shim is
///   cleaner than re-materialising `k` as a buffer at MIR-retarget time
///   and routing through the array-array kernel (which would force the
///   scalar onto the `(ptr, ptr)` path + a fresh handle to drop).
///
/// Returns `None` for any non-arithmetic op (the left-scalar surface is
/// the four element-wise binops only — comparison `k < a` is a §12
/// deferral, tracked with the right-scalar `a < 1` deferral).
#[must_use]
pub fn lookup_buffer_left_scalar_binop(op: BinOp) -> Option<&'static str> {
    match op {
        // Commutative — reuse the right-scalar shims verbatim.
        BinOp::Add => Some("__cobrust_coil_buffer_add_scalar"),
        BinOp::Mul => Some("__cobrust_coil_buffer_mul_scalar"),
        // Non-commutative — REVERSED shims (`k - a[i]` / `k / a[i]`).
        BinOp::Sub => Some("__cobrust_coil_buffer_rsub_scalar"),
        BinOp::Div => Some("__cobrust_coil_buffer_rdiv_scalar"),
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

/// ADR-0081 Phase-1b — resolve a validated-body field READ (`body.field`,
/// where `body` is a `route_validated`-registered handler's validated-body
/// param) to its typed accessor shim + return type, keyed on the field's
/// DECLARED `Ty` (read by the caller from `adt_fields`).
///
/// This is the §2-Q5 swappable seam: the caller (MIR's `Attr` sub-arm)
/// names **a symbol + a `Ty`**, NEVER `serde_json` or a JSON key. Today the
/// symbol indexes the boxed `serde_json::Value` the validator left
/// (`cabi.rs`'s `__cobrust_pit_body_get_*`); a future native-struct ABI
/// (ADR-0081 §7/Phase-4) emits a real struct + a `Projection::Field` load
/// behind the SAME symbol — the `.cb` source and the MIR shape are
/// unchanged.
///
/// The shim shape mirrors `__cobrust_pit_request_path_param`
/// (`(body: *mut u8, name: *mut u8) -> <ret>`): `body` is the boxed Value,
/// `name` is the COMPILER-SYNTHESISED field name `Str` (footgun #1 — never
/// author-written). Phase-1b shipped `i64` + `str`; ADR-0081 **Phase-2**
/// extends this to `f64` (`as_f64`) and `bool` (`as_bool`), plus the NESTED
/// case — a field whose declared `Ty` is itself a field-tracked validated
/// `class` (`Ty::Adt(nested_adt, _)`) resolves to
/// `__cobrust_pit_body_get_nested`, which returns the BORROWED interior
/// `&serde_json::Value` for the nested object so `body.inner.x` recurses
/// (the result temp is re-marked `validated_body_of = Some(nested_adt)`,
/// `lower.rs`).
///
/// Returns `None` for a field whose declared `Ty` has no accessor (the
/// caller then takes the pre-existing `Field(0)` stub path — NOT a serde
/// cast). The receiver (`body`) is gated on the registration MARK, never on
/// the `Ty` (the §5.2 no-UB invariant), so this lookup is only ever reached
/// for a `validated_body_of`-marked local.
#[must_use]
pub fn lookup_validated_body_accessor(field_ty: &Ty) -> Option<EcoSig> {
    let (symbol, ret) = match field_ty {
        // Integer-only `as_i64` in the shim (footgun #3 — NEVER
        // `as_f64`-truncate; CLAUDE.md §2.2 no-silent-coercion).
        Ty::Int => ("__cobrust_pit_body_get_i64", Ty::Int),
        Ty::Str => ("__cobrust_pit_body_get_str", Ty::Str),
        // ADR-0081 Phase-2 — `f64` (`as_f64`) + `bool` (`as_bool`). The
        // shims mirror the i64/str pair: a typed `serde_json::Value::as_*`
        // get over the BORROWED boxed Value (no coercion — validation
        // already proved the field is a JSON number / boolean of the
        // declared type, §2.2). `bool` returns the `.cb` `Bool` repr
        // (LLVM `i1` at the C ABI — the `re.match` / `fang.verify_password`
        // precedent), `f64` returns LLVM `double` (the `math.sqrt`
        // precedent).
        Ty::Float => ("__cobrust_pit_body_get_f64", Ty::Float),
        Ty::Bool => ("__cobrust_pit_body_get_bool", Ty::Bool),
        // ADR-0081 Phase-2 (nested) — a field typed as ANOTHER field-tracked
        // validated `class` (`Ty::Adt(nested_adt, _)`; its id is OUTSIDE the
        // ecosystem-handle range, so it is a user body class, NOT a pit/coil
        // handle). The accessor returns the BORROWED interior
        // `&serde_json::Value` for the nested object (no allocation, no free
        // — it lives inside the parent box that the `route_validated`
        // trampoline owns + frees once at handler exit, `cabi.rs:530`). The
        // caller re-marks the result temp `validated_body_of = Some(*id)` so
        // a further `.field` on it recurses through THIS lookup again. The
        // `_ecoret` carries `Ty::Adt(*id)`, whose codegen drop is a NO-OP
        // (`handle_drop_symbol(user_id) == None`, `llvm_backend.rs:5212`) —
        // so dropping a borrowed interior pointer is harmless (the §5.2
        // no-UB invariant holds: the borrow never outlives the parent box,
        // and no free is emitted on it).
        Ty::Adt(id, _) if !is_ecosystem_handle(*id) => {
            ("__cobrust_pit_body_get_nested", field_ty.clone())
        }
        // ADR-0081 Phase-3 — a `list[T]` field (T ∈ {str, i64, f64, bool}).
        // The validator ALREADY accepted the array + checked its element
        // types (ADR-0080 Phase-4(c), `validation.rs`; a type-mismatched
        // element is a 422 BEFORE the handler), so the accessor is a pure
        // typed READ: it BORROWS the parent body box, reads the JSON array,
        // and MINTS a FRESH `.cb` `list[T]` from it (the redis-`lrange` /
        // coil-`shape` `__cobrust_list_new(8,len)` + per-slot
        // `__cobrust_list_set` recipe, `cabi.rs`). ONE accessor per element
        // type (codegen-extern clarity, mirroring the scalar shims) — the
        // `(body, name) -> *mut List` ABI is shared; only the slot payload
        // differs (a heap-`Str` pointer for `str`, a raw `i64`, a `0`/`1`
        // for `bool`, an `f64::to_bits()` for `f64`). The `ret` carries
        // `Ty::List(elem)` so the `.cb` `_ecoret` temp's codegen drop
        // schedule selects the right drop (`list[str]` →
        // `__cobrust_list_drop_elems`, else `__cobrust_list_drop`,
        // `llvm_backend.rs:5223`) — the minted list drops EXACTLY ONCE,
        // owned by the `.cb` scope; the shim does NOT free it. A
        // `list[<deferred-elem>]` (list-of-list, dict, validated-class
        // element — out of #156 read scope) returns `None` here (the caller
        // then takes the `Field(0)` stub, NEVER a serde cast on an opaque
        // ptr — the §5.2 no-UB invariant).
        Ty::List(elem) => {
            let symbol = match &**elem {
                Ty::Str => "__cobrust_pit_body_get_list_str",
                Ty::Int => "__cobrust_pit_body_get_list_i64",
                Ty::Float => "__cobrust_pit_body_get_list_f64",
                Ty::Bool => "__cobrust_pit_body_get_list_bool",
                // Deferred element forms (list[list[T]], list[dict],
                // list[Class]) — no accessor; fall to the `Field(0)` stub.
                _ => return None,
            };
            (symbol, field_ty.clone())
        }
        _ => return None,
    };
    // The accessor's manifest signature carries ONE `Value` param — the
    // compiler-synthesised field-name `Str`. The receiver (`body`) is the
    // implicit borrowed first arg the MIR retarget prepends (mirroring the
    // `lookup_handle_attr` → `emit_ecosystem_call` borrowed-receiver path).
    Some(EcoSig {
        runtime_symbol: symbol,
        params: vec![EcoParam::Value(Ty::Str)],
        ret,
        tier: PyCompatTier::Semantic,
    })
}

/// Is `name` a known built-in ecosystem-module alias (Q1)? The HIR
/// binds `import den` as a `DefKind::ImportAlias` with surface name
/// `den`; the typechecker uses this to mark the alias `def_id` so
/// `den.attr` accesses resolve against the manifest.
#[must_use]
pub fn is_ecosystem_module(name: &str) -> bool {
    matches!(
        name,
        "den"
            | "nest"
            | "strike"
            | "scale"
            | "molt"
            | "pit"
            | "hood"
            | "coil"
            | "dora"
            | "fang"
            | "redis"
            | "math"
            | "re"
            | "random"
            | "time"
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

    // -- ADR-0083 math (scalar stdlib) manifest tests ------------------

    #[test]
    fn math_is_a_known_ecosystem_module() {
        assert!(is_ecosystem_module("math"));
    }

    #[test]
    fn math_single_arg_fns_are_f64_to_f64_numerical() {
        // The 15 single-arg `f64 -> f64` rows lower to BARE libm symbols
        // (NOT `__cobrust_math_*` shims — that is the distinct bare-function
        // intrinsic path). Numerical tier (libm last-ULP divergence).
        for name in [
            "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp",
            "log", "log10", "log2", "fabs",
        ] {
            let sig =
                lookup_module_fn("math", name).unwrap_or_else(|| panic!("math.{name} in manifest"));
            assert_eq!(
                sig.runtime_symbol, name,
                "runtime symbol is the bare libm name"
            );
            assert_eq!(
                value_tys(&sig.params),
                vec![Ty::Float],
                "math.{name} arg is Float"
            );
            assert_eq!(sig.ret, Ty::Float, "math.{name} returns Float");
            assert_eq!(sig.tier, PyCompatTier::Numerical);
        }
    }

    #[test]
    fn math_two_arg_fns_are_f64_f64_to_f64() {
        for name in ["pow", "atan2", "hypot"] {
            let sig =
                lookup_module_fn("math", name).unwrap_or_else(|| panic!("math.{name} in manifest"));
            assert_eq!(sig.runtime_symbol, name);
            assert_eq!(value_tys(&sig.params), vec![Ty::Float, Ty::Float]);
            assert_eq!(sig.ret, Ty::Float);
            assert_eq!(sig.tier, PyCompatTier::Numerical);
        }
    }

    #[test]
    fn math_deferred_fns_are_absent() {
        // ADR-0083 PART-2 SHIPPED floor/ceil/trunc (INT-returning) — they
        // are NO LONGER deferred (see `math_part2_*` tests). The remaining
        // deferred set is the non-libm integer ops factorial/gcd/isqrt.
        for name in ["factorial", "gcd", "isqrt"] {
            assert!(
                lookup_module_fn("math", name).is_none(),
                "math.{name} must be deferred (not yet shipped)"
            );
        }
    }

    // -- ADR-0083 PART-2 math manifest tests --------------------------
    #[test]
    fn math_part2_int_return_fns_are_float_to_int_strict() {
        // floor/ceil/trunc return CPython `int` — `[Float] -> Int`, via a
        // DISTINCT `__cobrust_math_*_int` shim (NOT the f64-returning
        // `__cobrust_math_floor`). Strict-tier (exact integer result).
        for (name, sym) in [
            ("floor", "__cobrust_math_floor_int"),
            ("ceil", "__cobrust_math_ceil_int"),
            ("trunc", "__cobrust_math_trunc_int"),
        ] {
            let sig =
                lookup_module_fn("math", name).unwrap_or_else(|| panic!("math.{name} in manifest"));
            assert_eq!(sig.runtime_symbol, sym, "math.{name} runtime symbol");
            assert_eq!(
                value_tys(&sig.params),
                vec![Ty::Float],
                "math.{name} arg is Float"
            );
            assert_eq!(sig.ret, Ty::Int, "math.{name} returns Int");
            assert_eq!(sig.tier, PyCompatTier::Strict, "math.{name} is Strict");
        }
    }

    #[test]
    fn math_part2_int_shims_distinct_from_bare_f64_floor() {
        // The Python `math.floor` (`int`) symbol must NOT collide with the
        // bare-function `floor(x)` PRELUDE path's `__cobrust_math_floor`
        // (`f64 -> f64`). They are different symbols + different return Ty.
        let floor =
            lookup_module_fn("math", "floor").unwrap_or_else(|| panic!("math.floor in manifest"));
        assert_eq!(floor.runtime_symbol, "__cobrust_math_floor_int");
        assert_ne!(
            floor.runtime_symbol, "__cobrust_math_floor",
            "math.floor must be the _int shim, NOT the f64 bare-floor shim"
        );
        assert_eq!(floor.ret, Ty::Int);
    }

    #[test]
    fn math_part2_bool_return_fns_are_float_to_bool_strict() {
        // isnan/isinf/isfinite — IEEE-754 classification, `[Float] -> Bool`,
        // mirroring coil.any/all's bool return. Strict-tier.
        for (name, sym) in [
            ("isnan", "__cobrust_math_isnan"),
            ("isinf", "__cobrust_math_isinf"),
            ("isfinite", "__cobrust_math_isfinite"),
        ] {
            let sig =
                lookup_module_fn("math", name).unwrap_or_else(|| panic!("math.{name} in manifest"));
            assert_eq!(sig.runtime_symbol, sym, "math.{name} runtime symbol");
            assert_eq!(
                value_tys(&sig.params),
                vec![Ty::Float],
                "math.{name} arg is Float"
            );
            assert_eq!(sig.ret, Ty::Bool, "math.{name} returns Bool");
            assert_eq!(sig.tier, PyCompatTier::Strict, "math.{name} is Strict");
        }
    }

    #[test]
    fn math_part2_degrees_radians_are_float_to_float_strict() {
        // degrees/radians via `cobrust-stdlib` to_degrees/to_radians shims.
        for (name, sym) in [
            ("degrees", "__cobrust_math_degrees"),
            ("radians", "__cobrust_math_radians"),
        ] {
            let sig =
                lookup_module_fn("math", name).unwrap_or_else(|| panic!("math.{name} in manifest"));
            assert_eq!(sig.runtime_symbol, sym, "math.{name} runtime symbol");
            assert_eq!(
                value_tys(&sig.params),
                vec![Ty::Float],
                "math.{name} arg is Float"
            );
            assert_eq!(sig.ret, Ty::Float, "math.{name} returns Float");
            assert_eq!(sig.tier, PyCompatTier::Strict, "math.{name} is Strict");
        }
    }

    #[test]
    fn math_part2_copysign_fmod_are_bare_libm_two_arg() {
        // copysign/fmod — BARE libm two-arg symbols (like pow/atan2/hypot),
        // NO `__cobrust_math_*` shim. BOTH Strict: copysign is a sign-bit
        // transplant; fmod is the IEEE-754 floating remainder, an EXACT
        // operation (no rounding) so it is bit-identical across conforming
        // libm and to CPython's libm-backed math.fmod (unlike the
        // transcendental pow/atan2/hypot, which are Numerical/last-ULP).
        let copysign = lookup_module_fn("math", "copysign")
            .unwrap_or_else(|| panic!("math.copysign in manifest"));
        assert_eq!(copysign.runtime_symbol, "copysign");
        assert_eq!(copysign.params.len(), 2);
        assert_eq!(copysign.ret, Ty::Float);
        assert_eq!(copysign.tier, PyCompatTier::Strict);

        let fmod =
            lookup_module_fn("math", "fmod").unwrap_or_else(|| panic!("math.fmod in manifest"));
        assert_eq!(fmod.runtime_symbol, "fmod");
        assert_eq!(fmod.params.len(), 2);
        assert_eq!(fmod.ret, Ty::Float);
        assert_eq!(fmod.tier, PyCompatTier::Strict);
    }

    #[test]
    fn math_constants_match_cpython_oracle() {
        // Differential oracle: /opt/homebrew/bin/python3.11 -c
        //   import math; print(repr(math.pi), repr(math.e), repr(math.tau))
        // → 3.141592653589793 2.718281828459045 6.283185307179586
        assert_eq!(
            lookup_module_const("math", "pi"),
            Some(std::f64::consts::PI)
        );
        assert_eq!(lookup_module_const("math", "e"), Some(std::f64::consts::E));
        assert_eq!(
            lookup_module_const("math", "tau"),
            Some(std::f64::consts::TAU)
        );
        // The `Some(consts::X)` equalities above are the bit-exact
        // differential check: Rust's `std::f64::consts` are the SAME f64
        // rounding the python3.11 oracle prints (`3.141592653589793`,
        // `2.718281828459045`, `6.283185307179586`).
    }

    #[test]
    fn math_unknown_const_and_other_module_const_are_none() {
        assert_eq!(lookup_module_const("math", "phi"), None);
        // `inf`/`nan` are BARE f64 literals (lexer M-F.3.3), NOT `math.`
        // constants — `math.inf` does not parse, so they resolve to None here
        // (ADR-0083 §Deferred). Use the bare `inf` / `nan` in `.cb`.
        assert_eq!(lookup_module_const("math", "inf"), None);
        assert_eq!(lookup_module_const("math", "nan"), None);
        // A constant lookup on a non-math module never resolves (the
        // function is math-only by design).
        assert_eq!(lookup_module_const("coil", "pi"), None);
    }

    // -- ADR-0086 random (scalar PRNG stdlib) manifest tests ------------

    #[test]
    fn random_is_a_known_ecosystem_module() {
        assert!(is_ecosystem_module("random"));
    }

    #[test]
    fn random_random_is_zero_arg_float_semantic() {
        // The FIRST 0-arg scalar stdlib fn: `random.random() -> Float`,
        // no parameters. Semantic tier (Pcg64 != CPython's Mersenne
        // Twister — distribution + seed-reproducibility, not exact values).
        let sig = lookup_module_fn("random", "random")
            .unwrap_or_else(|| panic!("random.random in manifest"));
        assert_eq!(sig.runtime_symbol, "__cobrust_random_random");
        assert!(
            value_tys(&sig.params).is_empty(),
            "random.random takes NO arguments"
        );
        assert_eq!(sig.ret, Ty::Float, "random.random returns Float");
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn random_randint_is_int_int_to_int_semantic() {
        // `random.randint(a, b) -> Int`, INCLUSIVE [a, b] (the inclusivity
        // lives in the shim's `gen_range(a..=b)`, asserted by the stdlib
        // unit tests + the .cb e2e — the manifest only pins the types).
        let sig = lookup_module_fn("random", "randint")
            .unwrap_or_else(|| panic!("random.randint in manifest"));
        assert_eq!(sig.runtime_symbol, "__cobrust_random_randint");
        assert_eq!(
            value_tys(&sig.params),
            vec![Ty::Int, Ty::Int],
            "random.randint takes two Int args"
        );
        assert_eq!(sig.ret, Ty::Int, "random.randint returns Int");
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn random_uniform_is_float_float_to_float_semantic() {
        let sig = lookup_module_fn("random", "uniform")
            .unwrap_or_else(|| panic!("random.uniform in manifest"));
        assert_eq!(sig.runtime_symbol, "__cobrust_random_uniform");
        assert_eq!(
            value_tys(&sig.params),
            vec![Ty::Float, Ty::Float],
            "random.uniform takes two Float args"
        );
        assert_eq!(sig.ret, Ty::Float, "random.uniform returns Float");
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn random_seed_is_int_to_int_sentinel_semantic() {
        // CPython `random.seed` returns None; Cobrust returns an i64
        // SENTINEL (discarded by the caller, the dora `send_output`
        // pattern), so the manifest ret is `Ty::Int`, NOT a None form.
        let sig =
            lookup_module_fn("random", "seed").unwrap_or_else(|| panic!("random.seed in manifest"));
        assert_eq!(sig.runtime_symbol, "__cobrust_random_seed");
        assert_eq!(
            value_tys(&sig.params),
            vec![Ty::Int],
            "random.seed takes one Int (the seed) arg"
        );
        assert_eq!(
            sig.ret,
            Ty::Int,
            "random.seed returns an i64 sentinel (CPython None -> discarded Int)"
        );
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn random_unknown_fn_is_none() {
        // `choice` / `shuffle` / `sample` are DEFERRED (list arg / list
        // mutation — ADR-0086 §"Deferred"); they must resolve to None so
        // the type checker surfaces a compile-time UnknownName (§2.5),
        // NOT a false-green binding.
        for name in ["choice", "shuffle", "sample", "randrange", "gauss"] {
            assert!(
                lookup_module_fn("random", name).is_none(),
                "random.{name} must be deferred (not yet shipped)"
            );
        }
    }

    // -- ADR-0087 time (timing + timestamps) manifest tests ------------

    #[test]
    fn time_is_a_known_ecosystem_module() {
        assert!(is_ecosystem_module("time"));
    }

    #[test]
    fn time_time_is_zero_arg_float_semantic() {
        // `time.time() -> Float`, no parameters (a 0-arg scalar fn like
        // `random.random`). Semantic tier (a wall clock is environment
        // state — not reproducible, not bit-identical to CPython).
        let sig =
            lookup_module_fn("time", "time").unwrap_or_else(|| panic!("time.time in manifest"));
        assert_eq!(sig.runtime_symbol, "__cobrust_time_time");
        assert!(
            value_tys(&sig.params).is_empty(),
            "time.time takes NO arguments"
        );
        assert_eq!(
            sig.ret,
            Ty::Float,
            "time.time returns Float (epoch seconds)"
        );
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn time_monotonic_and_perf_counter_are_zero_arg_float_semantic() {
        // Both `monotonic` and `perf_counter` are `[] -> Float`, reading
        // the SAME process-relative `Instant` origin (ADR-0087 unifies
        // them). Distinct runtime symbols, identical signature + tier.
        for (name, sym) in [
            ("monotonic", "__cobrust_time_monotonic"),
            ("perf_counter", "__cobrust_time_perf_counter"),
        ] {
            let sig =
                lookup_module_fn("time", name).unwrap_or_else(|| panic!("time.{name} in manifest"));
            assert_eq!(sig.runtime_symbol, sym, "time.{name} retargets to {sym}");
            assert!(
                value_tys(&sig.params).is_empty(),
                "time.{name} takes NO arguments"
            );
            assert_eq!(sig.ret, Ty::Float, "time.{name} returns Float (seconds)");
            assert_eq!(sig.tier, PyCompatTier::Semantic);
        }
    }

    #[test]
    fn time_sleep_is_float_to_int_sentinel_semantic() {
        // CPython `time.sleep` returns None; Cobrust returns an i64
        // SENTINEL (discarded by the caller, the dora `send_output`
        // pattern), so the manifest ret is `Ty::Int`, NOT a None form.
        // Takes one Float (seconds); a non-positive arg is a shim no-op.
        let sig =
            lookup_module_fn("time", "sleep").unwrap_or_else(|| panic!("time.sleep in manifest"));
        assert_eq!(sig.runtime_symbol, "__cobrust_time_sleep");
        assert_eq!(
            value_tys(&sig.params),
            vec![Ty::Float],
            "time.sleep takes one Float (seconds) arg"
        );
        assert_eq!(
            sig.ret,
            Ty::Int,
            "time.sleep returns an i64 sentinel (CPython None -> discarded Int)"
        );
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn time_unknown_fn_is_none() {
        // Calendar / struct-time machinery (`strftime` / `gmtime` /
        // `localtime` / `time_ns` / `process_time`) is DEFERRED
        // (ADR-0087 §"Deferred"); they must resolve to None so the type
        // checker surfaces a compile-time UnknownName (§2.5), NOT a
        // false-green binding.
        for name in ["strftime", "gmtime", "localtime", "time_ns", "process_time"] {
            assert!(
                lookup_module_fn("time", name).is_none(),
                "time.{name} must be deferred (not yet shipped)"
            );
        }
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

    // ADR-0078 Phase-1c — `redis` (cache/KV, the redis-py rebrand). The
    // ELEVENTH ecosystem module; the NINTH per-module 256-slot ADT block
    // (`0xE000_0800..0xE000_08FF`).

    #[test]
    fn redis_is_a_known_module() {
        assert!(is_ecosystem_module("redis"));
    }

    #[test]
    fn redis_client_id_recognized_and_in_reserved_block() {
        assert!(is_ecosystem_handle(REDIS_CLIENT_ADT));
        // Per-module 256-slot reservation: redis lives in the NINTH
        // block (`0xE000_0800..0xE000_08FF`), the next free block past
        // coil's `0x700` (the `0x200` scale gap is deliberately NOT
        // reused — blocks stay monotonic with allocation order).
        const _: () = {
            assert!(REDIS_CLIENT_ADT.0 >= ECO_ADT_BASE + 0x800);
            assert!(REDIS_CLIENT_ADT.0 < ECO_ADT_BASE + 0x900);
        };
    }

    #[test]
    fn redis_client_drop_symbol_resolves() {
        assert_eq!(
            handle_drop_symbol(REDIS_CLIENT_ADT),
            Some("__cobrust_redis_client_drop")
        );
    }

    #[test]
    fn redis_connect_signature_returns_client_handle() {
        let sig = lookup_module_fn("redis", "connect").expect("redis.connect in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_redis_connect");
        assert_eq!(value_tys(&sig.params), vec![Ty::Str]);
        assert_eq!(sig.ret, redis_client_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn redis_client_methods_resolve_with_expected_shapes() {
        let recv = redis_client_ty();

        let set = lookup_handle_method(&recv, "set").expect("Client.set in manifest");
        assert_eq!(set.runtime_symbol, "__cobrust_redis_client_set");
        assert_eq!(value_tys(&set.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(set.ret, Ty::None);

        let get = lookup_handle_method(&recv, "get").expect("Client.get in manifest");
        assert_eq!(get.runtime_symbol, "__cobrust_redis_client_get");
        assert_eq!(value_tys(&get.params), vec![Ty::Str]);
        assert_eq!(get.ret, Ty::Str);

        let delete = lookup_handle_method(&recv, "delete").expect("Client.delete in manifest");
        assert_eq!(delete.runtime_symbol, "__cobrust_redis_client_delete");
        assert_eq!(value_tys(&delete.params), vec![Ty::Str]);
        assert_eq!(delete.ret, Ty::Int);

        let exists = lookup_handle_method(&recv, "exists").expect("Client.exists in manifest");
        assert_eq!(exists.runtime_symbol, "__cobrust_redis_client_exists");
        assert_eq!(value_tys(&exists.params), vec![Ty::Str]);
        assert_eq!(exists.ret, Ty::Bool);

        // Phase-B verbs — expire / incr / incr_by / hset / hget.
        let expire = lookup_handle_method(&recv, "expire").expect("Client.expire in manifest");
        assert_eq!(expire.runtime_symbol, "__cobrust_redis_client_expire");
        // key str + the TTL seconds (i64).
        assert_eq!(value_tys(&expire.params), vec![Ty::Str, Ty::Int]);
        assert_eq!(expire.ret, Ty::Bool);

        let incr = lookup_handle_method(&recv, "incr").expect("Client.incr in manifest");
        assert_eq!(incr.runtime_symbol, "__cobrust_redis_client_incr");
        assert_eq!(value_tys(&incr.params), vec![Ty::Str]);
        assert_eq!(incr.ret, Ty::Int);

        let incr_by = lookup_handle_method(&recv, "incr_by").expect("Client.incr_by in manifest");
        assert_eq!(incr_by.runtime_symbol, "__cobrust_redis_client_incr_by");
        // key str + the integer delta.
        assert_eq!(value_tys(&incr_by.params), vec![Ty::Str, Ty::Int]);
        assert_eq!(incr_by.ret, Ty::Int);

        let hset = lookup_handle_method(&recv, "hset").expect("Client.hset in manifest");
        assert_eq!(hset.runtime_symbol, "__cobrust_redis_client_hset");
        // key str + field str + value str (the 3-arg shape).
        assert_eq!(value_tys(&hset.params), vec![Ty::Str, Ty::Str, Ty::Str]);
        assert_eq!(hset.ret, Ty::Bool);

        let hget = lookup_handle_method(&recv, "hget").expect("Client.hget in manifest");
        assert_eq!(hget.runtime_symbol, "__cobrust_redis_client_hget");
        // key str + field str.
        assert_eq!(value_tys(&hget.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(hget.ret, Ty::Str);

        // Phase-C verbs — lists (lpush/rpush/lpop/rpop/llen) + sets
        // (sadd/srem/sismember/scard). All scalar/str returns.
        let lpush = lookup_handle_method(&recv, "lpush").expect("Client.lpush in manifest");
        assert_eq!(lpush.runtime_symbol, "__cobrust_redis_client_lpush");
        // key str + the value str to prepend.
        assert_eq!(value_tys(&lpush.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(lpush.ret, Ty::Int);

        let rpush = lookup_handle_method(&recv, "rpush").expect("Client.rpush in manifest");
        assert_eq!(rpush.runtime_symbol, "__cobrust_redis_client_rpush");
        // key str + the value str to append.
        assert_eq!(value_tys(&rpush.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(rpush.ret, Ty::Int);

        let lpop = lookup_handle_method(&recv, "lpop").expect("Client.lpop in manifest");
        assert_eq!(lpop.runtime_symbol, "__cobrust_redis_client_lpop");
        assert_eq!(value_tys(&lpop.params), vec![Ty::Str]);
        assert_eq!(lpop.ret, Ty::Str);

        let rpop = lookup_handle_method(&recv, "rpop").expect("Client.rpop in manifest");
        assert_eq!(rpop.runtime_symbol, "__cobrust_redis_client_rpop");
        assert_eq!(value_tys(&rpop.params), vec![Ty::Str]);
        assert_eq!(rpop.ret, Ty::Str);

        let llen = lookup_handle_method(&recv, "llen").expect("Client.llen in manifest");
        assert_eq!(llen.runtime_symbol, "__cobrust_redis_client_llen");
        assert_eq!(value_tys(&llen.params), vec![Ty::Str]);
        assert_eq!(llen.ret, Ty::Int);

        let sadd = lookup_handle_method(&recv, "sadd").expect("Client.sadd in manifest");
        assert_eq!(sadd.runtime_symbol, "__cobrust_redis_client_sadd");
        // key str + the member str.
        assert_eq!(value_tys(&sadd.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(sadd.ret, Ty::Int);

        let srem = lookup_handle_method(&recv, "srem").expect("Client.srem in manifest");
        assert_eq!(srem.runtime_symbol, "__cobrust_redis_client_srem");
        // key str + the member str.
        assert_eq!(value_tys(&srem.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(srem.ret, Ty::Int);

        let sismember =
            lookup_handle_method(&recv, "sismember").expect("Client.sismember in manifest");
        assert_eq!(sismember.runtime_symbol, "__cobrust_redis_client_sismember");
        // key str + the member str.
        assert_eq!(value_tys(&sismember.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(sismember.ret, Ty::Bool);

        let scard = lookup_handle_method(&recv, "scard").expect("Client.scard in manifest");
        assert_eq!(scard.runtime_symbol, "__cobrust_redis_client_scard");
        assert_eq!(value_tys(&scard.params), vec![Ty::Str]);
        assert_eq!(scard.ret, Ty::Int);
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

    /// #numpy BATCH 20 — `coil.arange(n)` is `[Ty::Int] -> Buffer` (the
    /// EXACT `zeros` arg shape; an all-scalar-arg producer). The result is
    /// a Buffer handle (an `Int64` one at runtime).
    #[test]
    fn coil_arange_signature_int_to_buffer_handle() {
        let sig = lookup_module_fn("coil", "arange").expect("coil.arange in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_arange");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    // #145 BATCH 11 — spacing/value constructor manifest tests. The FIRST
    // coil ctors mixing `Ty::Float` + `Ty::Int` scalar args (linspace /
    // logspace are `[Float, Float, Int]`; full is `[Int, Float]`).

    #[test]
    fn coil_linspace_signature_float_float_int_to_buffer() {
        let sig = lookup_module_fn("coil", "linspace").expect("coil.linspace in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_linspace");
        assert_eq!(value_tys(&sig.params), vec![Ty::Float, Ty::Float, Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn coil_logspace_signature_float_float_int_to_buffer() {
        let sig = lookup_module_fn("coil", "logspace").expect("coil.logspace in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_logspace");
        assert_eq!(value_tys(&sig.params), vec![Ty::Float, Ty::Float, Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn coil_full_signature_int_float_to_buffer() {
        let sig = lookup_module_fn("coil", "full").expect("coil.full in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_full");
        assert_eq!(value_tys(&sig.params), vec![Ty::Int, Ty::Float]);
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
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
    fn coil_reshape_signature() {
        // #163 BATCH 18 — `coil.reshape(a, rows, cols) -> Buffer`: the
        // broadcast_to `[Buffer, Int]` shape + one more `Int`.
        let sig = lookup_module_fn("coil", "reshape").expect("coil.reshape in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_reshape");
        assert_eq!(
            value_tys(&sig.params),
            vec![coil_buffer_ty(), Ty::Int, Ty::Int]
        );
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
    }

    #[test]
    fn coil_split_signature() {
        let sig = lookup_module_fn("coil", "split").expect("coil.split in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_split");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Int]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_astype_signature_takes_buffer_and_str() {
        // BATCH 19 — `coil.astype(a, dtype) -> Buffer`: the FIRST coil row
        // mixing a Buffer with a `Ty::Str` (the runtime dtype name). Same
        // Str-arg shape dora `event.send_output(Str, Str)` proves lowers.
        let sig = lookup_module_fn("coil", "astype").expect("coil.astype in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_astype");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Str]);
        assert_eq!(sig.ret, coil_buffer_ty());
        assert_eq!(sig.tier, PyCompatTier::Semantic);
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

    // #145 BATCH 7 — the VALUE reductions min / max / prod. Each is
    // (Buffer) -> Float, the SAME shape as mean (coil's scalar-reduction
    // convention; the f64-return is numpy-exact for every .cb buffer).

    #[test]
    fn coil_min_returns_float() {
        let sig = lookup_module_fn("coil", "min").expect("coil.min in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_min");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_max_returns_float() {
        let sig = lookup_module_fn("coil", "max").expect("coil.max in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_max");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_prod_returns_float() {
        let sig = lookup_module_fn("coil", "prod").expect("coil.prod in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_prod");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    // #145 statistics gap-closure — ptp / nansum / nanmean / nanstd /
    // percentile. The first four are Buffer→f64; `percentile` is
    // (Buffer, f64)→f64 (the scalar-besides-handle ABI).

    #[test]
    fn coil_ptp_returns_float() {
        let sig = lookup_module_fn("coil", "ptp").expect("coil.ptp in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_ptp");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_nansum_returns_float() {
        let sig = lookup_module_fn("coil", "nansum").expect("coil.nansum in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_nansum");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_nanmean_returns_float() {
        let sig = lookup_module_fn("coil", "nanmean").expect("coil.nanmean in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_nanmean");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_nanstd_returns_float() {
        let sig = lookup_module_fn("coil", "nanstd").expect("coil.nanstd in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_nanstd");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_percentile_takes_buffer_and_float() {
        let sig = lookup_module_fn("coil", "percentile").expect("coil.percentile in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_percentile");
        // Buffer handle FIRST, then the f64 quantile arg.
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Float]);
        assert_eq!(sig.ret, Ty::Float);
    }

    // #145 SCALAR-ARG ufunc BATCH 6 — clip (Buffer, f64, f64) -> Buffer +
    // power (Buffer, f64) -> Buffer (the FIRST Buffer-RETURNING scalar-arg
    // ops; clip is the FIRST coil fn with TWO trailing f64 scalars).

    #[test]
    fn coil_clip_takes_buffer_and_two_floats_returns_buffer() {
        let sig = lookup_module_fn("coil", "clip").expect("coil.clip in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_clip");
        // Buffer handle FIRST, then the lo + hi f64 bounds.
        assert_eq!(
            value_tys(&sig.params),
            vec![coil_buffer_ty(), Ty::Float, Ty::Float]
        );
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    #[test]
    fn coil_power_takes_buffer_and_float_returns_buffer() {
        let sig = lookup_module_fn("coil", "power").expect("coil.power in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_power");
        // Buffer handle FIRST, then the f64 exponent (same shape as
        // percentile's params, but Buffer-returning).
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Float]);
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    // #163 BATCH 17 — trace / norm (Buffer -> Float, the scalar-reduction
    // shape as mean/std) + outer (Buffer, Buffer -> Buffer, the 2-Buffer
    // combine shape as concatenate).

    #[test]
    fn coil_trace_takes_buffer_returns_float() {
        let sig = lookup_module_fn("coil", "trace").expect("coil.trace in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_trace");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_norm_takes_buffer_returns_float() {
        let sig = lookup_module_fn("coil", "norm").expect("coil.norm in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_norm");
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        assert_eq!(sig.ret, Ty::Float);
    }

    #[test]
    fn coil_outer_takes_two_buffers_returns_buffer() {
        let sig = lookup_module_fn("coil", "outer").expect("coil.outer in manifest");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_outer");
        // Two Buffer handles -> a fresh Buffer (the concatenate shape).
        assert_eq!(
            value_tys(&sig.params),
            vec![coil_buffer_ty(), coil_buffer_ty()]
        );
        assert_eq!(sig.ret, coil_buffer_ty());
    }

    // #145 REARRANGE / REPEAT BATCH 10 — diff / flip (1-arg Buffer ->
    // Buffer) + roll / repeat / tile (Buffer + i64-scalar -> Buffer; the
    // i64-scalar mirror of the BATCH-6 clip / power f64-scalar shape, the
    // FIRST coil module fns with a trailing `Ty::Int` scalar).

    #[test]
    fn coil_diff_and_flip_take_buffer_return_buffer() {
        for op in ["diff", "flip"] {
            let sig =
                lookup_module_fn("coil", op).unwrap_or_else(|| panic!("coil.{op} in manifest"));
            assert_eq!(sig.runtime_symbol, format!("__cobrust_coil_{op}"));
            assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
            assert_eq!(sig.ret, coil_buffer_ty());
        }
    }

    // #163 PREDICATE BATCH 12 — `isnan` / `isinf` / `isfinite`. Each is a
    // 1-arg `Buffer -> Buffer` op (the bool MASK rides INSIDE the opaque
    // handle, so the manifest ret is the SAME `coil_buffer_ty()` as
    // `transpose` / `diff`). The distinguishing trait vs the rounding /
    // reshape ops is the `Strict` tier (EXACT boolean predicates, no
    // numerical tolerance).
    #[test]
    fn coil_predicates_take_buffer_return_buffer_strict() {
        for op in ["isnan", "isinf", "isfinite"] {
            let sig =
                lookup_module_fn("coil", op).unwrap_or_else(|| panic!("coil.{op} in manifest"));
            assert_eq!(sig.runtime_symbol, format!("__cobrust_coil_{op}"));
            assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
            // The bool-dtype result is carried INSIDE the opaque Buffer
            // handle, so the static ret type is still the Buffer ADT.
            assert_eq!(sig.ret, coil_buffer_ty());
            assert_eq!(sig.tier, PyCompatTier::Strict, "exact boolean predicate");
        }
    }

    #[test]
    fn coil_roll_repeat_tile_take_buffer_and_int_return_buffer() {
        for op in ["roll", "repeat", "tile"] {
            let sig =
                lookup_module_fn("coil", op).unwrap_or_else(|| panic!("coil.{op} in manifest"));
            assert_eq!(sig.runtime_symbol, format!("__cobrust_coil_{op}"));
            // Buffer handle FIRST, then the i64 scalar (shift / count) —
            // `Ty::Int`, NOT `Ty::Float` (the load-bearing dtype: the `.cb`
            // int literal lowers DIRECTLY as i64, no f64 cast).
            assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty(), Ty::Int]);
            assert_eq!(sig.ret, coil_buffer_ty());
        }
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

    /// ADR-0076 Phase 2 — `event.send_output(output_id, payload)` is an
    /// Event-handle method (NOT a `dora.Node` method): the Event is the
    /// only handle in the handler's scope. Two Str args, i64 return.
    #[test]
    fn dora_event_send_output_takes_two_strs_returns_i64() {
        let so = lookup_handle_method(&dora_event_ty(), "send_output")
            .expect("Event.send_output in manifest");
        assert_eq!(so.runtime_symbol, "__cobrust_dora_event_send_output");
        assert_eq!(value_tys(&so.params), vec![Ty::Str, Ty::Str]);
        assert_eq!(so.ret, Ty::Int);
        assert_eq!(so.tier, PyCompatTier::Semantic);
        // send_output is NOT a Node method — the surface hangs off Event.
        assert!(lookup_handle_method(&dora_node_ty(), "send_output").is_none());
    }

    /// ADR-0076 Phase 2 — the multi-IO declaration free-fns the decorator
    /// desugar threads from `@dora.node(inputs=[...], outputs=[...])`.
    #[test]
    fn dora_declare_input_output_are_str_to_i64_free_fns() {
        let di = lookup_module_fn("dora", "declare_input").expect("dora.declare_input in manifest");
        assert_eq!(di.runtime_symbol, "__cobrust_dora_declare_input");
        assert_eq!(value_tys(&di.params), vec![Ty::Str]);
        assert_eq!(di.ret, Ty::Int);

        let dout =
            lookup_module_fn("dora", "declare_output").expect("dora.declare_output in manifest");
        assert_eq!(dout.runtime_symbol, "__cobrust_dora_declare_output");
        assert_eq!(value_tys(&dout.params), vec![Ty::Str]);
        assert_eq!(dout.ret, Ty::Int);
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

    // ---- ADR-0077 Phase-2/3 — left-scalar + buffer comparison --------

    #[test]
    fn left_scalar_commutative_ops_reuse_right_scalar_shims() {
        // `k + a == a + k`, `k * a == a * k` — the LEFT-scalar form reuses
        // the EXISTING right-scalar shims (no new C-ABI symbol).
        assert_eq!(
            lookup_buffer_left_scalar_binop(BinOp::Add),
            Some("__cobrust_coil_buffer_add_scalar")
        );
        assert_eq!(
            lookup_buffer_left_scalar_binop(BinOp::Mul),
            Some("__cobrust_coil_buffer_mul_scalar")
        );
    }

    #[test]
    fn left_scalar_noncommutative_ops_use_reversed_shims() {
        // `k - a != a - k`, `k / a != a / k` — the LEFT-scalar form needs
        // the REVERSED shims (`k - a[i]` / `k / a[i]`), DISTINCT from the
        // right-scalar `_sub_scalar` / `_div_scalar`.
        assert_eq!(
            lookup_buffer_left_scalar_binop(BinOp::Sub),
            Some("__cobrust_coil_buffer_rsub_scalar")
        );
        assert_eq!(
            lookup_buffer_left_scalar_binop(BinOp::Div),
            Some("__cobrust_coil_buffer_rdiv_scalar")
        );
        // The reversed symbols are NOT the right-scalar ones — the whole
        // point of the left-scalar `-`/`/` is the flipped operand order.
        assert_ne!(
            lookup_buffer_left_scalar_binop(BinOp::Sub),
            lookup_buffer_scalar_binop(BinOp::Sub)
        );
        assert_ne!(
            lookup_buffer_left_scalar_binop(BinOp::Div),
            lookup_buffer_scalar_binop(BinOp::Div)
        );
    }

    #[test]
    fn left_scalar_rejects_non_arithmetic_ops() {
        // The left-scalar surface is the four arithmetic ops only —
        // comparison `k < a` is a §12 deferral.
        assert!(lookup_buffer_left_scalar_binop(BinOp::Mod).is_none());
        assert!(lookup_buffer_left_scalar_binop(BinOp::Pow).is_none());
        assert!(lookup_buffer_left_scalar_binop(BinOp::FloorDiv).is_none());
        assert!(lookup_buffer_left_scalar_binop(BinOp::Lt).is_none());
        assert!(lookup_buffer_left_scalar_binop(BinOp::Eq).is_none());
        assert!(lookup_buffer_left_scalar_binop(BinOp::MatMul).is_none());
    }

    #[test]
    fn buffer_comparison_ops_resolve_to_bool_buffer_symbols() {
        // `a cmp b` resolves through the SAME `lookup_buffer_binop` path
        // as `+`/`-`/`*`/`/`, mapping the six comparison ops onto the
        // `__cobrust_coil_buffer_{lt,le,gt,ge,eq,ne}` shims. The `ret` is
        // `coil_buffer_ty()` (a NumPy Bool-dtype mask — the static handle
        // carries no dtype), NOT `Ty::Bool`.
        let cases = [
            (BinOp::Lt, "__cobrust_coil_buffer_lt"),
            (BinOp::LtEq, "__cobrust_coil_buffer_le"),
            (BinOp::Gt, "__cobrust_coil_buffer_gt"),
            (BinOp::GtEq, "__cobrust_coil_buffer_ge"),
            (BinOp::Eq, "__cobrust_coil_buffer_eq"),
            (BinOp::NotEq, "__cobrust_coil_buffer_ne"),
        ];
        for (op, sym) in cases {
            let sig = lookup_buffer_binop(&coil_buffer_ty(), op)
                .unwrap_or_else(|| panic!("comparison op {op:?} must resolve on coil.Buffer"));
            assert_eq!(sig.runtime_symbol, sym, "wrong symbol for {op:?}");
            assert_eq!(
                sig.ret,
                coil_buffer_ty(),
                "comparison must return a (Bool-dtype) coil.Buffer, not a scalar bool"
            );
            assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
        }
    }

    #[test]
    fn buffer_comparison_resolves_behind_shared_borrow() {
        // `&a < &b` (the LLM-idiomatic explicit-borrow form, ADR-0052a)
        // resolves identically to the bare `a < b` form.
        let sig = lookup_buffer_binop(&Ty::Ref(Box::new(coil_buffer_ty())), BinOp::Lt)
            .expect("comparison resolves behind &borrow");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_buffer_lt");
    }

    #[test]
    fn buffer_binop_still_rejects_unsupported_ops() {
        // The op-set boundary: `//`/`%`/`**` remain §12 deferrals — adding
        // `@` (matmul, below) must NOT blanket-accept every operator.
        assert!(lookup_buffer_binop(&coil_buffer_ty(), BinOp::FloorDiv).is_none());
        assert!(lookup_buffer_binop(&coil_buffer_ty(), BinOp::Mod).is_none());
        assert!(lookup_buffer_binop(&coil_buffer_ty(), BinOp::Pow).is_none());
        // The four arithmetic ops are still mapped (no regression).
        assert!(lookup_buffer_binop(&coil_buffer_ty(), BinOp::Add).is_some());
        assert!(lookup_buffer_binop(&coil_buffer_ty(), BinOp::Div).is_some());
    }

    #[test]
    fn buffer_matmul_resolves_to_matmul_symbol() {
        // ADR-0077 §"@-operator" — `a @ b` resolves through the SAME
        // `lookup_buffer_binop` path as `+`/`-`/`*`/`/` and the comparison
        // ops, mapping `MatMul` onto the dedicated
        // `__cobrust_coil_buffer_matmul` shim. `@` ALWAYS returns a
        // `coil.Buffer` (the matrix / matrix-vector result; the 1-D·1-D
        // scalar case is the `a.dot(b)` method, ADR-0077 Q2).
        let sig = lookup_buffer_binop(&coil_buffer_ty(), BinOp::MatMul)
            .expect("`@` (matmul) must resolve on coil.Buffer");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_buffer_matmul");
        assert_eq!(
            sig.ret,
            coil_buffer_ty(),
            "matmul must return a coil.Buffer (matrix result), not a scalar"
        );
        assert_eq!(value_tys(&sig.params), vec![coil_buffer_ty()]);
    }

    #[test]
    fn buffer_matmul_resolves_behind_shared_borrow() {
        // `&a @ &b` (the LLM-idiomatic explicit-borrow form, ADR-0052a —
        // `coil.Buffer` is non-Copy) resolves identically to the bare
        // `a @ b` form via the same Ref-unwrap as the arithmetic ops.
        let sig = lookup_buffer_binop(&Ty::Ref(Box::new(coil_buffer_ty())), BinOp::MatMul)
            .expect("matmul resolves behind &borrow");
        assert_eq!(sig.runtime_symbol, "__cobrust_coil_buffer_matmul");
    }

    // -- ADR-0081 validated-body field accessor lookup ---------------------

    /// Phase-1b: `i64`/`str` fields resolve to their shims; Phase-2: `f64`/
    /// `bool` resolve to theirs. Each carries the field-name `Str` param +
    /// the field's `Ty` as its return.
    #[test]
    fn validated_body_accessor_scalar_arms() {
        let i = lookup_validated_body_accessor(&Ty::Int).expect("i64 field has an accessor");
        assert_eq!(i.runtime_symbol, "__cobrust_pit_body_get_i64");
        assert_eq!(i.ret, Ty::Int);
        assert_eq!(value_tys(&i.params), vec![Ty::Str]);

        let s = lookup_validated_body_accessor(&Ty::Str).expect("str field has an accessor");
        assert_eq!(s.runtime_symbol, "__cobrust_pit_body_get_str");
        assert_eq!(s.ret, Ty::Str);

        // ADR-0081 Phase-2 — f64 + bool.
        let f = lookup_validated_body_accessor(&Ty::Float).expect("f64 field has an accessor");
        assert_eq!(f.runtime_symbol, "__cobrust_pit_body_get_f64");
        assert_eq!(f.ret, Ty::Float, "the f64 accessor returns Ty::Float");

        let b = lookup_validated_body_accessor(&Ty::Bool).expect("bool field has an accessor");
        assert_eq!(b.runtime_symbol, "__cobrust_pit_body_get_bool");
        assert_eq!(b.ret, Ty::Bool, "the bool accessor returns Ty::Bool");
    }

    /// ADR-0081 Phase-2 (nested): a field typed as a USER class (an AdtId
    /// OUTSIDE the ecosystem-handle range) resolves to the nested accessor,
    /// returning the SAME `Ty::Adt` so the result temp can be re-marked.
    #[test]
    fn validated_body_accessor_nested_arm() {
        // A user body class id (well outside ECO_ADT_BASE).
        let nested = Ty::Adt(AdtId(68), vec![]);
        let n = lookup_validated_body_accessor(&nested).expect("a class-typed field recurses");
        assert_eq!(n.runtime_symbol, "__cobrust_pit_body_get_nested");
        assert_eq!(
            n.ret, nested,
            "the nested accessor returns the SAME Ty::Adt (the result temp re-mark target)"
        );
    }

    /// The nested arm is gated on the id being a USER class: an ECOSYSTEM
    /// handle (`pit.Request`, `coil.Buffer`, …) is NOT a validated body and
    /// must NOT resolve to the nested accessor (it is a foreign opaque
    /// handle, never a `serde_json::Value` object).
    #[test]
    fn validated_body_accessor_rejects_ecosystem_handle_and_unknown() {
        assert!(
            lookup_validated_body_accessor(&Ty::Adt(PIT_REQUEST_ADT, vec![])).is_none(),
            "an ecosystem handle is not a nested validated body"
        );
        assert!(
            lookup_validated_body_accessor(&Ty::Adt(COIL_BUFFER_ADT, vec![])).is_none(),
            "coil.Buffer is not a nested validated body"
        );
        // A DEFERRED list-element form (list-of-list) → None (the caller
        // takes the Field(0) stub, never a serde cast). The scalar-element
        // lists (str/i64/f64/bool) DO resolve — see
        // `validated_body_accessor_list_arms`; only the out-of-#156-scope
        // element forms stay None here.
        assert!(
            lookup_validated_body_accessor(&Ty::List(Box::new(Ty::List(Box::new(Ty::Int)))))
                .is_none(),
            "a list[list[T]] field has no accessor (deferred element form — Field(0) stub)"
        );
    }

    /// ADR-0081 Phase-3: a `list[T]` field (T ∈ str/i64/f64/bool) resolves to
    /// the per-element-type list accessor, returning the SAME `Ty::List(elem)`
    /// so the result temp's codegen drop schedule selects the right list drop.
    #[test]
    fn validated_body_accessor_list_arms() {
        for (elem, sym) in [
            (Ty::Str, "__cobrust_pit_body_get_list_str"),
            (Ty::Int, "__cobrust_pit_body_get_list_i64"),
            (Ty::Float, "__cobrust_pit_body_get_list_f64"),
            (Ty::Bool, "__cobrust_pit_body_get_list_bool"),
        ] {
            let list_ty = Ty::List(Box::new(elem.clone()));
            let a = lookup_validated_body_accessor(&list_ty)
                .unwrap_or_else(|| panic!("list[{elem:?}] field has an accessor"));
            assert_eq!(
                a.runtime_symbol, sym,
                "list[{elem:?}] resolves to its per-element accessor symbol"
            );
            assert_eq!(
                a.ret, list_ty,
                "the list accessor returns the SAME Ty::List(elem) (drives the drop schedule)"
            );
            assert_eq!(
                value_tys(&a.params),
                vec![Ty::Str],
                "the list accessor takes the compiler-synthesised field-name Str"
            );
            assert_eq!(a.tier, PyCompatTier::Semantic);
        }
        // A deferred element form (list of a validated class — out of #156
        // read scope) → None (the Field(0) stub, never a serde cast).
        assert!(
            lookup_validated_body_accessor(&Ty::List(Box::new(Ty::Adt(AdtId(68), vec![]))))
                .is_none(),
            "list[<Class>] is a deferred read form (the element-class read is not wired)"
        );
    }
}
