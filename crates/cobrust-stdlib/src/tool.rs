//! `cobrust.tool` — α flat-function tool-schema and closed-world invocation surface.
//!
//! ADR-0048 §"M-AI.2 — cobrust.tool" is refined by
//! `docs/agent/spike/m-ai-2-cobrust-tool-spike.md`. The α surface is
//! deliberately flat functions, not decorators / methods / reflection:
//!
//! - `tool_schema(name, description, parameters_json, return_type) -> str`
//! - `tool_registry_new() -> str`
//! - `tool_registry_register(registry_json, schema_json) -> str`
//! - `tool_invoke(tool_name, args_json) -> str`
//! - `llm_complete_with_tools(prompt, registry_json) -> str`
//!
//! `tool_invoke` is a closed-world α dispatcher. It ships only the
//! deterministic `add_i64` exemplar and does not call arbitrary user-defined
//! Cobrust functions. `llm_complete_with_tools` is intentionally narrower
//! than its name suggests: today it only validates the registry, augments
//! the prompt with tool metadata, and delegates to `llm_dispatch("tools", ...)`.
//! It does not execute tool calls or run a tool loop. Future decorator /
//! `.schema()` / `Registry` / reflection surfaces require separate compiler work.

use serde::{Deserialize, Serialize};
use serde_json::Value;


#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolSchema {
    name: String,
    description: String,
    parameters: Vec<ToolParam>,
    returns: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolRegistry {
    tools: Vec<ToolSchema>,
}

/// Build a canonical compact tool-schema JSON string.
///
/// Returns `""` if `name` is not an identifier, `parameters_json` is not an
/// array of `{name,type}` objects, or `return_type` is empty.
#[must_use]
pub fn tool_schema_helper(
    name: &str,
    description: &str,
    parameters_json: &str,
    return_type: &str,
) -> String {
    if !is_valid_tool_name(name) || return_type.is_empty() {
        return String::new();
    }

    let Ok(parameters) = serde_json::from_str::<Vec<ToolParam>>(parameters_json) else {
        return String::new();
    };
    if parameters
        .iter()
        .any(|param| param.name.is_empty() || param.ty.is_empty())
    {
        return String::new();
    }

    serialize_schema(&ToolSchema {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
        returns: return_type.to_string(),
    })
}

/// Build an empty canonical registry JSON string.
#[must_use]
pub fn tool_registry_new_helper() -> String {
    String::from(r#"{"tools":[]}"#)
}

/// Register one schema into a registry, replacing any prior schema with the
/// same name. Returns `""` on malformed registry or schema JSON.
#[must_use]
pub fn tool_registry_register_helper(registry_json: &str, schema_json: &str) -> String {
    let Some(mut registry) = parse_registry(registry_json) else {
        return String::new();
    };
    let Some(schema) = parse_schema(schema_json) else {
        return String::new();
    };

    registry
        .tools
        .retain(|existing| existing.name != schema.name);
    registry.tools.push(schema);
    serde_json::to_string(&registry).unwrap_or_default()
}

/// Closed-world α tool dispatcher.
///
/// Supports exactly the documented `add_i64` exemplar. Unknown tools,
/// malformed JSON, missing args, non-integer args, and arithmetic overflow all
/// return `""`.
#[must_use]
pub fn tool_invoke_helper(tool_name: &str, args_json: &str) -> String {
    match tool_name {
        "add_i64" => invoke_add_i64(args_json),
        _ => String::new(),
    }
}

/// Build the deterministic prompt text used by `llm_complete_with_tools`.
///
/// This helper only serializes tool metadata into the prompt. It does not
/// execute tools or parse model output.
#[must_use]
pub fn augment_prompt_with_tools_helper(prompt: &str, registry_json: &str) -> String {
    format!(
        "{prompt}\n\nAvailable tools:\n{registry_json}\n\nIf a tool is needed, respond with JSON: {{\"tool\":\"<name>\",\"args\":{{...}}}}\nOtherwise answer directly."
    )
}

/// Prompt-augment and route through M-AI.0 `llm_dispatch(task="tools", ...)`.
///
/// Honest alpha contract: this helper does not execute tools, inspect model
/// output, or perform a second LLM round-trip. It only validates the registry,
/// appends it to the prompt, and dispatches once.
///
/// Returns `""` on malformed registry JSON or router failure. Native provider
/// tool-calling APIs and local tool loops are intentionally deferred.
#[must_use]
pub fn llm_complete_with_tools_helper(prompt: &str, registry_json: &str) -> String {
    if parse_registry(registry_json).is_none() {
        return String::new();
    }
    let augmented = augment_prompt_with_tools_helper(prompt, registry_json);
    #[cfg(feature = "llm-router")]
    {
        crate::llm::llm_dispatch_blocking("tools", &augmented)
    }
    #[cfg(not(feature = "llm-router"))]
    {
        let _ = augmented;
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn augment_prompt_with_tools_is_prompt_only() {
        let registry =
            r#"{"tools":[{"name":"add_i64","description":"Add two integers","parameters":[{"name":"a","type":"i64"},{"name":"b","type":"i64"}],"returns":"i64"}]}"#;
        let augmented = augment_prompt_with_tools_helper("What is 1+2?", registry);
        assert!(augmented.contains("What is 1+2?"));
        assert!(augmented.contains("Available tools:"));
        assert!(augmented.contains(registry));
        assert!(augmented.contains("respond with JSON"));
    }
}

fn is_valid_tool_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn parse_schema(schema_json: &str) -> Option<ToolSchema> {
    let schema = serde_json::from_str::<ToolSchema>(schema_json).ok()?;
    if is_semantically_valid_schema(&schema) {
        Some(schema)
    } else {
        None
    }
}

fn parse_registry(registry_json: &str) -> Option<ToolRegistry> {
    let registry = serde_json::from_str::<ToolRegistry>(registry_json).ok()?;
    if registry.tools.iter().all(is_semantically_valid_schema) {
        Some(registry)
    } else {
        None
    }
}

fn is_semantically_valid_schema(schema: &ToolSchema) -> bool {
    is_valid_tool_name(&schema.name)
        && !schema.returns.is_empty()
        && schema
            .parameters
            .iter()
            .all(|param| !param.name.is_empty() && !param.ty.is_empty())
}

fn serialize_schema(schema: &ToolSchema) -> String {
    serde_json::to_string(schema).unwrap_or_default()
}

fn invoke_add_i64(args_json: &str) -> String {
    let Ok(Value::Object(args)) = serde_json::from_str::<Value>(args_json) else {
        return String::new();
    };
    let Some(a) = args.get("a").and_then(Value::as_i64) else {
        return String::new();
    };
    let Some(b) = args.get("b").and_then(Value::as_i64) else {
        return String::new();
    };
    a.checked_add(b)
        .map_or_else(String::new, |sum| sum.to_string())
}

// =====================================================================
// Internal C-ABI helpers — mirror `llm.rs` / `prompt.rs` patterns.
// =====================================================================

/// Read a heap `Str` pointer as a `String`. Tolerates null and empty.
///
/// # Safety
///
/// `buf` must be null or a valid Cobrust `Str` buffer pointer.
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    // SAFETY: caller attests `buf` is a valid Cobrust Str buffer pointer.
    unsafe {
        let ptr = crate::fmt::__cobrust_str_ptr(buf);
        let len = crate::fmt::__cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

fn alloc_str_buffer(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` allocates a valid Str buffer and
    // `__cobrust_str_push_static` copies `s` into it.
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// C-ABI shim for source-level
/// `tool_schema(name, description, parameters_json, return_type) -> str`.
///
/// # Safety
///
/// Each pointer must be null or a valid Cobrust `Str` buffer pointer. The
/// returned pointer is an owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tool_schema(
    name: *mut u8,
    description: *mut u8,
    parameters_json: *mut u8,
    return_type: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let n = unsafe { read_str_buf(name) };
    // SAFETY: same.
    let d = unsafe { read_str_buf(description) };
    // SAFETY: same.
    let p = unsafe { read_str_buf(parameters_json) };
    // SAFETY: same.
    let r = unsafe { read_str_buf(return_type) };
    alloc_str_buffer(&tool_schema_helper(&n, &d, &p, &r))
}

/// C-ABI shim for source-level `tool_registry_new() -> str`.
///
/// # Safety
///
/// This shim takes no input pointers. The returned pointer is an owned
/// Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tool_registry_new() -> *mut u8 {
    alloc_str_buffer(&tool_registry_new_helper())
}

/// C-ABI shim for source-level
/// `tool_registry_register(registry_json, schema_json) -> str`.
///
/// # Safety
///
/// Each pointer must be null or a valid Cobrust `Str` buffer pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tool_registry_register(
    registry_json: *mut u8,
    schema_json: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let registry = unsafe { read_str_buf(registry_json) };
    // SAFETY: same.
    let schema = unsafe { read_str_buf(schema_json) };
    alloc_str_buffer(&tool_registry_register_helper(&registry, &schema))
}

/// C-ABI shim for source-level `tool_invoke(tool_name, args_json) -> str`.
///
/// # Safety
///
/// Each pointer must be null or a valid Cobrust `Str` buffer pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tool_invoke(tool_name: *mut u8, args_json: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let name = unsafe { read_str_buf(tool_name) };
    // SAFETY: same.
    let args = unsafe { read_str_buf(args_json) };
    alloc_str_buffer(&tool_invoke_helper(&name, &args))
}

/// C-ABI shim for source-level
/// `llm_complete_with_tools(prompt, registry_json) -> str`.
///
/// # Safety
///
/// Each pointer must be null or a valid Cobrust `Str` buffer pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_complete_with_tools(
    prompt: *mut u8,
    registry_json: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let p = unsafe { read_str_buf(prompt) };
    // SAFETY: same.
    let registry = unsafe { read_str_buf(registry_json) };
    alloc_str_buffer(&llm_complete_with_tools_helper(&p, &registry))
}
