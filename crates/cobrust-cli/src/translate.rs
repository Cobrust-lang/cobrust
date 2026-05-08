//! `cobrust translate <library>` — wraps `cobrust_translator::pipeline::translate`
//! per ADR-0024 §"`cobrust translate` argv mapping".
//!
//! Looks up `corpus/<lib>/spec.toml`, `corpus/<lib>/upstream/`,
//! `corpus/<lib>/upstream_tests/`, and `corpus/<lib>/canned_llm_responses.toml`,
//! constructs a [`PyLibrary`], registers the synthetic provider, and runs
//! the L0..L1 translator pipeline.
//!
//! Failure exit codes live in the `[100, 127]` band per ADR-0024.

use std::path::{Path, PathBuf};

use cobrust_llm_router::RouterConfig;
use cobrust_translator::{PyLibrary, TranslatorConfig, pipeline};

use crate::exit_codes;

/// Run `cobrust translate <library>`.
pub fn run(library: &str, out_dir: Option<&Path>, quiet: bool) -> u8 {
    let corpus_root = match locate_corpus_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cobrust translate: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    let lib_dir = corpus_root.join(library);
    if !lib_dir.exists() {
        eprintln!(
            "cobrust translate: corpus directory not found: {}",
            lib_dir.display()
        );
        return exit_codes::USER_ERROR;
    }

    let spec_file = lib_dir.join("spec.toml");
    let canned = lib_dir.join("canned_llm_responses.toml");
    let upstream_tests = lib_dir.join("upstream_tests");

    let upstream_file = match find_first_python_file(&lib_dir.join("upstream")) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cobrust translate: {e}");
            return exit_codes::TRANSLATOR_BASE;
        }
    };

    let library_value = library.to_string();
    let py_lib = PyLibrary {
        library: library_value.clone(),
        version: "0.0.0".to_string(),
        source_file: upstream_file,
        spec_file,
        upstream_tests,
        canned_responses: Some(canned),
        seeds: vec![0xC0BE_1057_u64],
        fuzz_inputs_per_fn: 100,
    };

    let out = out_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("target/cobrust/crates"));
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("cobrust translate: cannot create out dir: {e}");
        return exit_codes::TRANSLATOR_BASE;
    }

    let router_cfg = match build_synthetic_router(&out) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cobrust translate: cannot build router config: {e}");
            return exit_codes::TRANSLATOR_BASE;
        }
    };
    let translator_cfg = TranslatorConfig::m4_synthetic(router_cfg, out);

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("cobrust translate: cannot start runtime: {e}");
            return exit_codes::TRANSLATOR_BASE;
        }
    };
    let result = runtime.block_on(pipeline::translate(&py_lib, &translator_cfg));

    match result {
        Ok(translated) => {
            if !quiet {
                println!(
                    "cobrust translate: produced crate at {}",
                    translated.crate_dir.display()
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("cobrust translate: pipeline failure: {e:?}");
            // M10 maps every translator error to TRANSLATOR_BASE; finer
            // mapping (one code per L0..L3 stage) is a Phase F follow-up.
            exit_codes::TRANSLATOR_BASE
        }
    }
}

/// Build the M4-style synthetic router config in memory; mirrors the
/// shape used by the existing translator pipeline tests.
fn build_synthetic_router(out_root: &Path) -> Result<RouterConfig, String> {
    let cache = out_root.join(".cobrust/llm_cache");
    let ledger = out_root.join(".cobrust/ledger.jsonl");
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.synthetic]
kind = "openai"
base_url = "http://synthetic.invalid"
api_key_env = "COBRUST_M10_SYNTHETIC_KEY"
models = ["m10-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:m10-canned-v1"]

[routing.spec_extract]
strategy = "quality"
preferred = ["synthetic:m10-canned-v1"]

[routing.repair]
strategy = "cost"
preferred = ["synthetic:m10-canned-v1"]
"#,
        cache = cache.display(),
        ledger = ledger.display(),
    );
    RouterConfig::from_toml_str(&toml).map_err(|e| format!("router config parse: {e:?}"))
}

fn locate_corpus_root() -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var("COBRUST_CORPUS_ROOT") {
        let p = PathBuf::from(custom);
        if p.is_dir() {
            return Ok(p);
        }
    }
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let cand = cwd.join("corpus");
    if cand.is_dir() {
        return Ok(cand);
    }
    let mut p = cwd.clone();
    while let Some(parent) = p.parent() {
        let cand = parent.join("corpus");
        if cand.is_dir() {
            return Ok(cand);
        }
        p = parent.to_path_buf();
    }
    Err(format!("corpus/ directory not found from {}", cwd.display()))
}

fn find_first_python_file(dir: &Path) -> Result<PathBuf, String> {
    if !dir.is_dir() {
        return Err(format!("upstream dir missing: {}", dir.display()));
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("readdir {}: {e}", dir.display()))?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("py") {
            candidates.push(p);
        }
    }
    candidates.sort();
    candidates.into_iter().next().ok_or_else(|| {
        format!(
            "no .py file found in upstream dir {}; expected at least one",
            dir.display()
        )
    })
}
