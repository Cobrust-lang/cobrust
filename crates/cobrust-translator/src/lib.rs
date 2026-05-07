//! Cobrust AI Translation Subsystem.
//!
//! L0 (spec extraction) → L1 (translation) → L2 (verification) →
//! L3 (integration) closed loop. Constitution `CLAUDE.md` §4.2 binds the
//! pipeline shape; `adr:0007` binds the M4 implementation contract.
//!
//! # Architecture
//!
//! - [`spec`] — L0 spec extraction: read corpus → emit `spec.toml` +
//!   harness directory layout.
//! - [`translate`] — L1 translation engine: function-level, bottom-up
//!   by dependency graph. Dispatches via
//!   [`cobrust_llm_router::Router`].
//! - [`manifest`] — provenance manifest builder + writer + verifier.
//! - [`synthetic`] — canned-LLM provider for the gate path.
//! - [`pipeline`] — orchestrator: read source → L0 → L1 → write crate.
//! - [`deterministic`] — `deterministic_id` computation per ADR-0007.
//! - [`error`] — `TranslatorError` taxonomy.
//!
//! # Modes
//!
//! - **Synthetic-LLM mode (default)**: pre-recorded responses served by
//!   [`synthetic::SyntheticProvider`]. M4 gate runs in this mode.
//! - **Real-LLM mode (`--features real-llm`)**: production providers
//!   from `cobrust_llm_router::{AnthropicProvider, OpenAiProvider}`.
//!   The translator code path is identical; only provider registration
//!   changes.
//!
//! See `docs/agent/modules/translator.md` for the full agent-facing
//! spec and `docs/agent/adr/0007-translator-pipeline.md` for the
//! decision record.

pub mod config;
pub mod deterministic;
pub mod error;
pub mod manifest;
pub mod pipeline;
pub mod spec;
pub mod synthetic;
pub mod translate;

// Public re-exports — keep the surface small and declarative.
pub use crate::config::TranslatorConfig;
pub use crate::deterministic::deterministic_id;
pub use crate::error::TranslatorError;
pub use crate::manifest::{
    BuildSection, GatesSection, OracleSection, ProvenanceManifest, RouterSection, SourceSection,
    VerificationSection,
};
pub use crate::pipeline::{PyLibrary, TranslatedCrate, translate};
pub use crate::spec::{FunctionSpec, SpecError, SpecToml};
pub use crate::synthetic::{CannedResponse, CannedTable, SyntheticProvider};
pub use crate::translate::{EmittedFile, FunctionTranslation, TranslationOutput, TranslationPlan};
