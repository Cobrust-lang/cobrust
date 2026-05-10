// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator via real-LLM round-trips through
// `build_translation_prompt_rich` per ADR-0036. Promoted by T1.1 (ADR-0039).
// source-library: tomli 2.0.1
// oracle: cpython 3.11 (module: tomllib)
// provider: user_codex_t1_1 (gpt-5.5)
// functions translated: 12 (full public surface)
// see PROVENANCE.toml + docs/agent/findings/0.1.0-beta-tomli-full-translation.md.

//! Cobrust 0.1.0-beta translation of `tomli` 2.0.1 — produced via
//! real-LLM end-to-end through the production
//! `cobrust_translator::build_translation_prompt_rich` builder.
//!
//! T1.1 verdict: 5/5 canonical entrypoints PASS; 99.51 % fuzz pass rate
//! over 1024 inputs vs CPython 3.11 `tomllib`; 9-14× faster than
//! CPython on representative parse workloads. See `ADR-0039` for the
//! full translation outcome rationale.
//!
//! Public surface:
//! - `loads(src: &str) -> Result<BTreeMap<String, Value>, TomliError>` — parse a TOML string.
//! - `Value` — heterogeneous TOML value tree.
//! - `TomliError` — single error type.
//! - `to_json` / `table_to_json` — JSON conversion helpers used by the L3 differential gate.

mod parser;

pub use crate::parser::{TomliError, Value, loads, table_to_json, to_json};
