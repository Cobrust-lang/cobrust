//! DAP `evaluate` handler (ADR-0059f §3.1).
//!
//! Routes watch / repl / hover / clipboard expressions through lldb's
//! `expression` REPL command via [`crate::lldb_driver::LldbDriver::evaluate`].
//! The result is lldb's stdout summary verbatim — wave-1 pretty-printers
//! shape Cobrust types when loaded.

use serde_json::Value;

use crate::Adapter;
use crate::dap_types::{EvaluateArguments, EvaluateResponse, Request};
use crate::handlers::{DapHandlerError, parse_args};

/// Handle the `evaluate` DAP request.
///
/// Per ADR-0059f §3.1, evaluates `args.expression` in the optional
/// `args.frame_id` frame's locals scope. Returns the lldb stdout
/// summary verbatim plus the parsed DWARF type name (when present).
pub async fn handle_evaluate(
    adapter: &Adapter,
    request: &Request,
) -> Result<Value, DapHandlerError> {
    let args: EvaluateArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let (result, type_name) = driver.evaluate(&args.expression, args.frame_id).await?;
    let response = EvaluateResponse {
        result,
        type_name,
        variables_reference: 0,
    };
    Ok(serde_json::to_value(response)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::missing_panics_doc)]
mod tests {
    use super::*;
    use crate::lldb_driver::LldbDriver;

    fn req(seq: i64, command: &str, args: Value) -> Request {
        Request {
            seq,
            type_field: "request".to_string(),
            command: command.to_string(),
            arguments: Some(args),
        }
    }

    #[tokio::test]
    async fn evaluate_with_typed_result_parses_type() {
        let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
            "expression --".to_string(),
            "(int) $0 = 42\n".to_string(),
        )]));
        let request = req(
            1,
            "evaluate",
            serde_json::json!({"expression": "i + 1", "context": "repl"}),
        );
        let result = handle_evaluate(&adapter, &request).await.unwrap();
        assert_eq!(result["result"], "42");
        assert_eq!(result["type"], "int");
        assert_eq!(result["variablesReference"], 0);
    }

    #[tokio::test]
    async fn evaluate_with_pretty_printer_output_preserves_summary() {
        let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
            "expression --".to_string(),
            "(cobrust::List) $1 = [1, 2, 3]\n".to_string(),
        )]));
        let request = req(
            2,
            "evaluate",
            serde_json::json!({"expression": "xs", "context": "watch"}),
        );
        let result = handle_evaluate(&adapter, &request).await.unwrap();
        assert_eq!(result["result"], "[1, 2, 3]");
        assert_eq!(result["type"], "cobrust::List");
    }

    #[tokio::test]
    async fn evaluate_with_no_canned_response_returns_empty_string() {
        let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
        let request = req(
            3,
            "evaluate",
            serde_json::json!({"expression": "undefined_var"}),
        );
        let result = handle_evaluate(&adapter, &request).await.unwrap();
        // Empty stub stdout falls through to the no-type path.
        assert_eq!(result["result"], "");
    }
}
