//! cobrust-llm-router — first-class compiler subsystem for LLM dispatch.
//!
//! M0 skeleton; first delivery at M3.
//!
//! Responsibilities (target):
//! - Provider-agnostic LLM API (Anthropic + OpenAI-compatible).
//! - Per-task routing (cost / quality / latency / consensus).
//! - Content-addressed cache and append-only token ledger.
//! - Failure isolation per provider.
//!
//! See `docs/agent/modules/llm-router.md` for the agent-facing spec
//! and `cobrust.toml.example` for the configuration shape.
