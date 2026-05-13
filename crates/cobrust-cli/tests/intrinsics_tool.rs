//! M-AI.2 α Phase 4 — end-to-end `.cb` source → compile → run test for
//! the flat-fn `cobrust.tool` surface.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]

use std::process::Command;

fn build_and_run_source(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe)
        .current_dir(dir.path())
        .env_remove("COBRUST_CONFIG")
        .output()
        .unwrap();
    assert!(run.status.success(), "run failed: {:?}", run.status);
    String::from_utf8_lossy(&run.stdout).into_owned()
}

#[test]
fn test_e2e_tool_schema_registry_invoke_prints_result() {
    let stdout = build_and_run_source(concat!(
        "fn main() -> i64:\n",
        "    let schema: str = tool_schema(\"add_i64\", \"Add two integers\", \"[{\\\"name\\\":\\\"a\\\",\\\"type\\\":\\\"i64\\\"},{\\\"name\\\":\\\"b\\\",\\\"type\\\":\\\"i64\\\"}]\", \"i64\")\n",
        "    let registry: str = tool_registry_register(tool_registry_new(), schema)\n",
        "    let result: str = tool_invoke(\"add_i64\", \"{\\\"a\\\":1,\\\"b\\\":2}\")\n",
        "    print(result)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "3\n");
}

#[test]
fn test_e2e_llm_complete_with_tools_without_config_prints_empty_line() {
    let stdout = build_and_run_source(concat!(
        "fn main() -> i64:\n",
        "    let schema: str = tool_schema(\"add_i64\", \"Add two integers\", \"[{\\\"name\\\":\\\"a\\\",\\\"type\\\":\\\"i64\\\"},{\\\"name\\\":\\\"b\\\",\\\"type\\\":\\\"i64\\\"}]\", \"i64\")\n",
        "    let registry: str = tool_registry_register(tool_registry_new(), schema)\n",
        "    let response: str = llm_complete_with_tools(\"What is 1+2?\", registry)\n",
        "    print(response)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "\n");
}

