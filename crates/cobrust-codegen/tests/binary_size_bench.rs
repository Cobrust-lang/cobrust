//! ADR-0058b §A3 empirical-close — binary-size bench harness.
//!
//! Closes ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" by
//! compiling 5 representative fixtures at OptLevel::None + OptLevel::SpeedAndSize
//! through the LLVM backend, capturing object-file size, and asserting
//! the O3 median is ≤ 70% of the O0 median (≥ 30% reduction).
//!
//! The bench operates at object-file granularity (.o) — wave-2 stays
//! within ADR-0023 §"Linker delegation" scope and does NOT exercise
//! linker output. Object size is the dominant component of executable
//! size for these fixtures (the linker adds bootstrap/runtime overhead
//! roughly equal at O0/O3, which would dilute the ratio at the
//! executable layer).
//!
//! ## Coverage matrix (5 fixtures)
//!
//! | # | Fixture | Surface exercised |
//! |---|---|---|
//! | 1 | `hello` | Minimal `fn main()` returning 0; smallest non-trivial body |
//! | 2 | `fizzbuzz` | Modulo arithmetic + branching (control flow + binop dispatch) |
//! | 3 | `fib` | Recursive call + self-reference (Call terminator + register pressure) |
//! | 4 | `dot_product` | Loop body with accumulator (loop-vectorization hot path) |
//! | 5 | `nested_branch` | Multi-level if/else chain (SimplifyCFG hot path) |
//!
//! Each fixture compiles to an `.o` via `Backend::Llvm` at the two
//! opt levels; sizes are recorded and the median ratio asserted.
//!
//! F34 anchors:
//! - `binary_size_bench::bench_fixtures` — full table-driven path
//! - `binary_size_bench::o3_median_under_70pct` — ADR-0023 §A3 close

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::similar_names)]

#![cfg(feature = "llvm")]

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module as MirModule, lower as mir_lower};
use cobrust_types::check;
use target_lexicon::Triple;

/// 5-fixture bench corpus per ADR-0058b §A3.
struct Fixture {
    name: &'static str,
    source: &'static str,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "hello",
        source: "fn main() -> i64:\n    return 0\n",
    },
    Fixture {
        name: "fizzbuzz",
        // Pure-control-flow fizzbuzz body — no print, no f-string (those
        // need M11 stdlib runtime). Returns an int classification:
        //   0 = fizzbuzz, 1 = fizz, 2 = buzz, 3 = neither.
        source: concat!(
            "fn classify(n: i64) -> i64:\n",
            "    if ((n % 15) == 0):\n",
            "        return 0\n",
            "    if ((n % 3) == 0):\n",
            "        return 1\n",
            "    if ((n % 5) == 0):\n",
            "        return 2\n",
            "    return 3\n",
        ),
    },
    Fixture {
        name: "fib",
        source: concat!(
            "fn fib(n: i64) -> i64:\n",
            "    if (n < 2):\n",
            "        return n\n",
            "    return (fib((n - 1)) + fib((n - 2)))\n",
        ),
    },
    Fixture {
        name: "dot_product",
        // Loop accumulator pattern. M9 has no array indexing, so we
        // exercise the loop body via a synthetic accumulator over [0, n).
        source: concat!(
            "fn dot(n: i64) -> i64:\n",
            "    let acc: i64 = 0\n",
            "    let i: i64 = 0\n",
            "    while (i < n):\n",
            "        acc += (i * i)\n",
            "        i += 1\n",
            "    return acc\n",
        ),
    },
    Fixture {
        name: "nested_branch",
        // Multi-level if/else — SimplifyCFG + GVN have plenty to chew on.
        source: concat!(
            "fn classify_range(x: i64) -> i64:\n",
            "    if (x < 0):\n",
            "        if (x < -100):\n",
            "            return -2\n",
            "        return -1\n",
            "    if (x < 100):\n",
            "        if (x < 10):\n",
            "            return 1\n",
            "        return 2\n",
            "    return 3\n",
        ),
    },
];

fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn llvm_spec(name: &str, opt: OptLevel) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-0058b-binsize-{name}-{opt:?}-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: opt,
        backend: Backend::Llvm,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
    }
}

/// Compile fixture at given opt level, return CODE size in bytes
/// (sum of non-DWARF section sizes — `.debug_*` / `__debug_*` sections
/// excluded).
///
/// ADR-0023 §A3 measures *code-size reduction* from O3 opt passes. After
/// ADR-0058c added DWARF v5 emission, both O0 and O3 binaries carry
/// ~equal `.debug_*` payloads which dominate the small machine-code
/// delta and skew the O3/O0 ratio toward 1.0. Excluding debug sections
/// preserves the wave-2 contract empirically.
fn compile_and_size(fixture: &Fixture, opt: OptLevel) -> u64 {
    use object::{Object, ObjectSection};
    let mir = lower_to_mir(fixture.source);
    let spec = llvm_spec(fixture.name, opt);
    let artifact = emit(&mir, spec)
        .unwrap_or_else(|e| panic!("emit `{}` @ {:?}: {}", fixture.name, opt, e));
    let Artifact::Object(path) = artifact else {
        panic!("expected Object artifact for `{}`", fixture.name);
    };
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let obj = object::File::parse(&*bytes)
        .unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e));
    obj.sections()
        .filter(|s| {
            let n = s.name().unwrap_or("");
            !(n.contains("debug_") || n.contains("__debug"))
        })
        .map(|s| s.size())
        .sum()
}

/// Per-fixture: emit at O0 + O3, both succeed and produce non-empty
/// objects. Validates wave-2's opt pipeline does not break the
/// wave-1-functional fixtures.
#[test]
fn bench_fixtures() {
    for fx in FIXTURES {
        let o0 = compile_and_size(fx, OptLevel::None);
        let o3 = compile_and_size(fx, OptLevel::SpeedAndSize);
        assert!(o0 > 0, "O0 size zero for `{}`", fx.name);
        assert!(o3 > 0, "O3 size zero for `{}`", fx.name);
        eprintln!(
            "0058b-bench {:>14} O0={:>6} O3={:>6} ratio={:.3}",
            fx.name,
            o0,
            o3,
            o3 as f64 / o0 as f64
        );
    }
}

/// ADR-0023 §A3 empirical-close: O3 median ≤ 70% of O0 median (≥ 30% reduction).
///
/// Uses **median across the 5-fixture corpus**, not per-fixture — per
/// ADR-0058b §7.2 risk mitigation. Per-fixture ratio is recorded in
/// `bench_fixtures` stderr output for diagnostics.
///
/// If this assertion fails, ADR-0058b §7.2 fall-back path is:
/// drop the `default<Os>` size-overlay from `pass_pipeline_for(SpeedAndSize)`
/// (becomes `default<O3>` alone). Recovery is one-line edit; bench
/// re-runs validate the median.
#[test]
fn o3_median_under_70pct() {
    let mut ratios: Vec<f64> = FIXTURES
        .iter()
        .map(|fx| {
            let o0 = compile_and_size(fx, OptLevel::None);
            let o3 = compile_and_size(fx, OptLevel::SpeedAndSize);
            o3 as f64 / o0 as f64
        })
        .collect();
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = ratios[ratios.len() / 2];
    eprintln!(
        "0058b-bench median O3/O0 ratio = {:.3} (per-fixture sorted: {:?})",
        median, ratios
    );
    assert!(
        median <= 0.70,
        "ADR-0023 §A3 bar: O3 median ratio {:.3} > 0.70 (≥ 30% reduction required). \
         Per-fixture ratios: {:?}. Fall-back per ADR-0058b §7.2: \
         drop default<Os> overlay from SpeedAndSize pipeline.",
        median,
        ratios
    );
}
