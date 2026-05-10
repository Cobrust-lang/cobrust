//! Cobrust AI Translation Subsystem.
//!
//! L0 (spec extraction) → L1 (translation) → L2 (verification) →
//! L3 (integration) closed loop. Constitution `CLAUDE.md` §4.2 binds the
//! pipeline shape; `adr:0007` binds the M4 implementation contract;
//! `adr:0008` adds L2.perf + repair-loop; `adr:0009` adds L3
//! downstream-dependents partial-coverage policy.
//!
//! # Architecture
//!
//! - [`spec`] — L0 spec extraction: read corpus → emit `spec.toml` +
//!   harness directory layout.
//! - [`translate`] — L1 translation engine: function-level, bottom-up
//!   by dependency graph. Dispatches via
//!   [`cobrust_llm_router::Router`].
//! - [`manifest`] — provenance manifest builder + writer + verifier.
//! - [`synthetic`] — canned-LLM provider for the gate path. M5 added
//!   per-attempt routing for the repair loop.
//! - [`pipeline`] — orchestrator: read source → L0 → L1 → L2 (build,
//!   behavior, perf) → L3 (PyO3-shaped wrapper, downstream
//!   dependents) → repair loop on failure.
//! - [`deterministic`] — `deterministic_id` computation per ADR-0007.
//! - [`error`] — `TranslatorError` taxonomy (extended with
//!   `EscalationExceeded`, `PerfGate` per ADR-0008).
//! - [`repair`] — repair-loop driver: `GateFailure` blob +
//!   `repair_translation()` call + `failure_report.md` writer.
//! - [`bench`] — L2.perf harness: hand-rolled timing loops with
//!   median ns + JSON report writer.
//! - [`downstream`] — L3 downstream-dependents driver: vendored test
//!   subsets via subprocess `python3`.
//!
//! # Modes
//!
//! - **Synthetic-LLM mode (default)**: pre-recorded responses served by
//!   [`synthetic::SyntheticProvider`]. The M4/M5 default gate path.
//! - **Real-LLM mode (`--features real-llm`)**: production providers
//!   from `cobrust_llm_router::{AnthropicProvider, OpenAiProvider}`.
//!   The translator code path is identical; only provider registration
//!   changes.
//!
//! See `docs/agent/modules/translator.md` for the full agent-facing
//! spec.

pub mod bench;
pub mod config;
pub mod cython;
pub mod deterministic;
pub mod downstream;
pub mod error;
pub mod manifest;
pub mod pipeline;
pub mod repair;
pub mod spec;
pub mod synthetic;
pub mod translate;

// Public re-exports — keep the surface small and declarative.
pub use crate::bench::{
    BenchmarkReport, BenchmarkResult, PerfTarget, classify_result, hardware_tag, short_commit_sha,
    time_median,
};
pub use crate::config::TranslatorConfig;
pub use crate::cython::{
    CythonFunction, CythonFunctionKind, CythonParam, CythonSource, CythonType,
    ShimError as CythonShimError, parse as parse_cython,
};
pub use crate::deterministic::deterministic_id;
pub use crate::downstream::{
    DependentResult, DependentSpec, DependentStatus, DownstreamReport, dateutil_m5_deferred,
    dateutil_m5_dependents, dateutil_m6_dependents, msgpack_m6_deferred, msgpack_m6_dependents,
    run_dependent,
};
pub use crate::error::TranslatorError;
pub use crate::manifest::{
    BuildSection, DependentsSection, GatesSection, OracleSection, ProvenanceManifest,
    RouterSection, SourceSection, VerificationSection,
};
pub use crate::pipeline::{
    AcceptAll, AcceptAllPerf, BehaviorVerifier, GateOutcome, GateOutcomes, PerfVerdict,
    PerfVerifier, PyLibrary, TranslatedCrate, VerifierVerdict, translate, translate_with_verifier,
    translate_with_verifiers,
};
pub use crate::repair::{
    GateFailure, repair_translation, repair_translation_with_task, write_failure_report,
};
pub use crate::spec::{FunctionSpec, SpecError, SpecToml};
pub use crate::synthetic::{CannedResponse, CannedTable, PromptHeader, SyntheticProvider};
pub use crate::translate::{
    EmittedFile, FunctionTranslation, TranslationOutput, TranslationPlan, WorkspaceContext,
    build_translation_prompt_rich,
};
