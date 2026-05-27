//! L3 differential gate for cobrust-hood.
//!
//! Per ADR-0022 §3 (decorator chain → clap derive). We exercise the
//! translated public surface against a matrix of argv shapes that
//! the click upstream test bank exercises (subset). Since click is
//! pure-Python parsing and we bind clap — both deterministic — the
//! L3 path is pure-Rust subprocess-free.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use hood::{ArgumentSpec, ClickErrorKind, Command, OptionSpec, ParamType};

fn echo() -> Command {
    Command::new("echo")
        .about("emit a message")
        .option(
            OptionSpec::new("message")
                .short("m")
                .type_(ParamType::Str)
                .default("hello"),
        )
        .option(OptionSpec::new("times").type_(ParamType::Int).default("1"))
}

fn cli_with_args() -> Command {
    Command::new("cp")
        .argument(ArgumentSpec::new("src").type_(ParamType::Str))
        .argument(ArgumentSpec::new("dst").type_(ParamType::Str))
}

#[test]
fn l3_default_option_resolves_when_omitted() {
    let result = echo().run(vec!["echo"]).expect("default ok");
    assert_eq!(result.option("message"), Some("hello"));
    assert_eq!(result.option("times"), Some("1"));
}

#[test]
fn l3_explicit_option_overrides_default() {
    let result = echo()
        .run(vec!["echo", "--message", "world", "--times", "3"])
        .expect("explicit");
    assert_eq!(result.option("message"), Some("world"));
    assert_eq!(result.option("times"), Some("3"));
}

#[test]
fn l3_short_option_is_recognised() {
    let result = echo().run(vec!["echo", "-m", "yo"]).expect("short option");
    assert_eq!(result.option("message"), Some("yo"));
}

#[test]
fn l3_int_option_validates_payload() {
    let err = echo()
        .run(vec!["echo", "--times", "not-int"])
        .expect_err("must reject");
    assert_eq!(err.kind, ClickErrorKind::InvalidValue);
}

#[test]
fn l3_positional_arguments_dispatch_in_order() {
    let result = cli_with_args()
        .run(vec!["cp", "/from/path", "/to/path"])
        .expect("positional");
    assert_eq!(result.argument("src"), Some("/from/path"));
    assert_eq!(result.argument("dst"), Some("/to/path"));
}

#[test]
fn l3_missing_positional_fails_with_missing_argument_kind() {
    let err = cli_with_args()
        .run(vec!["cp", "/from/only"])
        .expect_err("missing dst");
    assert_eq!(err.kind, ClickErrorKind::MissingArgument);
}

#[test]
fn l3_unknown_option_fails_with_usage_kind() {
    let err = echo()
        .run(vec!["echo", "--mystery", "x"])
        .expect_err("unknown");
    assert_eq!(err.kind, ClickErrorKind::UsageError);
}

#[test]
fn l3_required_option_fails_when_omitted() {
    let cmd =
        Command::new("strict").option(OptionSpec::new("api-key").type_(ParamType::Str).required());
    let err = cmd.run(vec!["strict"]).expect_err("missing required");
    assert_eq!(err.kind, ClickErrorKind::MissingOption);
}

#[test]
fn l3_bool_flag_records_set_or_default() {
    let cmd = Command::new("toggle").option(OptionSpec::new("verbose").type_(ParamType::Bool));
    let off = cmd.run(vec!["toggle"]).expect("flag absent");
    assert_eq!(off.option("verbose"), Some("false"));
    let on = cmd.run(vec!["toggle", "--verbose"]).expect("flag present");
    assert_eq!(on.option("verbose"), Some("true"));
}

#[test]
fn l3_optional_argument_can_be_omitted() {
    let cmd = Command::new("greet").argument(ArgumentSpec::new("name").optional());
    let r = cmd.run(vec!["greet"]).expect("optional omitted");
    assert!(r.argument("name").is_none());
    let r2 = cmd.run(vec!["greet", "ada"]).expect("optional present");
    assert_eq!(r2.argument("name"), Some("ada"));
}

#[test]
fn l3_pyo3_wrapper_directory_layout() {
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(crate_dir.join("python/hood_init.py").exists());
    assert!(crate_dir.join("python/setup.py").exists());
    assert!(crate_dir.join("PROVENANCE.toml").exists());
}
