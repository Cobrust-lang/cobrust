//! M-AI.2 α Phase 4 — Rust-side helper tests for `cobrust.tool`.
//!
//! TEST-FIRST corpus per spike `docs/agent/spike/m-ai-2-cobrust-tool-spike.md`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]

#[test]
fn test_tool_schema_add_i64_deterministic_json() {
    let schema = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "Add two integers",
        r#"[{"name":"a","type":"i64"},{"name":"b","type":"i64"}]"#,
        "i64",
    );
    assert_eq!(
        schema,
        r#"{"name":"add_i64","description":"Add two integers","parameters":[{"name":"a","type":"i64"},{"name":"b","type":"i64"}],"returns":"i64"}"#
    );
}

#[test]
fn test_tool_schema_malformed_params_returns_empty() {
    assert_eq!(
        cobrust_stdlib::tool::tool_schema_helper("bad", "Bad", "not json", "i64"),
        ""
    );
}

#[test]
fn test_tool_schema_escapes_quote_newline_brace_description() {
    let schema = cobrust_stdlib::tool::tool_schema_helper(
        "quote_tool",
        "Quote \"x\"\nbrace { ok }",
        r#"[{"name":"a","type":"i64"}]"#,
        "i64",
    );
    let parsed: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert_eq!(parsed["description"], "Quote \"x\"\nbrace { ok }");
    assert_eq!(
        schema,
        r#"{"name":"quote_tool","description":"Quote \"x\"\nbrace { ok }","parameters":[{"name":"a","type":"i64"}],"returns":"i64"}"#
    );
}

#[test]
fn test_tool_registry_new_empty_manifest() {
    assert_eq!(
        cobrust_stdlib::tool::tool_registry_new_helper(),
        r#"{"tools":[]}"#
    );
}

#[test]
fn test_tool_registry_register_valid_schema() {
    let schema = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "Add two integers",
        r#"[{"name":"a","type":"i64"},{"name":"b","type":"i64"}]"#,
        "i64",
    );
    let registry = cobrust_stdlib::tool::tool_registry_register_helper(
        &cobrust_stdlib::tool::tool_registry_new_helper(),
        &schema,
    );
    assert_eq!(registry, format!(r#"{{"tools":[{schema}]}}"#));
}

#[test]
fn test_tool_registry_duplicate_policy_last_schema_wins() {
    let first = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "First",
        r#"[{"name":"a","type":"i64"},{"name":"b","type":"i64"}]"#,
        "i64",
    );
    let second = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "Second",
        r#"[{"name":"x","type":"i64"},{"name":"y","type":"i64"}]"#,
        "i64",
    );
    let reg1 = cobrust_stdlib::tool::tool_registry_register_helper(
        &cobrust_stdlib::tool::tool_registry_new_helper(),
        &first,
    );
    let reg2 = cobrust_stdlib::tool::tool_registry_register_helper(&reg1, &second);
    assert_eq!(reg2, format!(r#"{{"tools":[{second}]}}"#));
}

#[test]
fn test_tool_registry_invalid_registry_returns_empty() {
    let schema = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "Add two integers",
        r#"[{"name":"a","type":"i64"},{"name":"b","type":"i64"}]"#,
        "i64",
    );
    assert_eq!(
        cobrust_stdlib::tool::tool_registry_register_helper("not json", &schema),
        ""
    );
}

#[test]
fn test_tool_registry_existing_semantically_invalid_schema_returns_empty() {
    let schema = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "Add two integers",
        r#"[{"name":"a","type":"i64"},{"name":"b","type":"i64"}]"#,
        "i64",
    );
    let invalid_registry =
        r#"{"tools":[{"name":"","description":"bad","parameters":[],"returns":"i64"}]}"#;
    assert_eq!(
        cobrust_stdlib::tool::tool_registry_register_helper(invalid_registry, &schema),
        ""
    );
}

#[test]
fn test_llm_complete_with_tools_semantically_invalid_registry_returns_empty() {
    let invalid_registry = r#"{"tools":[{"name":"bad_tool","description":"bad","parameters":[{"name":"x","type":""}],"returns":"i64"}]}"#;
    assert_eq!(
        cobrust_stdlib::tool::llm_complete_with_tools_helper("What is 1+2?", invalid_registry),
        ""
    );
}

#[test]
fn test_tool_invoke_add_i64_locked_result() {
    assert_eq!(
        cobrust_stdlib::tool::tool_invoke_helper("add_i64", r#"{"a":1,"b":2}"#),
        "3"
    );
}

#[test]
fn test_tool_invoke_unknown_tool_returns_empty() {
    assert_eq!(
        cobrust_stdlib::tool::tool_invoke_helper("missing", r#"{"a":1}"#),
        ""
    );
}

#[test]
fn test_tool_invoke_malformed_args_returns_empty() {
    assert_eq!(
        cobrust_stdlib::tool::tool_invoke_helper("add_i64", "not json"),
        ""
    );
}

#[test]
fn test_llm_complete_with_tools_prompt_augmentation_helper_deterministic() {
    let schema = cobrust_stdlib::tool::tool_schema_helper(
        "add_i64",
        "Add two integers",
        r#"[{"name":"a","type":"i64"},{"name":"b","type":"i64"}]"#,
        "i64",
    );
    let registry = cobrust_stdlib::tool::tool_registry_register_helper(
        &cobrust_stdlib::tool::tool_registry_new_helper(),
        &schema,
    );
    let augmented =
        cobrust_stdlib::tool::augment_prompt_with_tools_helper("What is 1+2?", &registry);
    assert_eq!(
        augmented,
        format!(
            "What is 1+2?\n\nAvailable tools:\n{registry}\n\nIf a tool is needed, respond with JSON: {{\"tool\":\"<name>\",\"args\":{{...}}}}\nOtherwise answer directly."
        )
    );
}
