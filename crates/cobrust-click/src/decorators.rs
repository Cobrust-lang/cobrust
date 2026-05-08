// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: click 8.1.7
// oracle: cpython 3.11 (module: click)
// functions translated: 16 (Command + OptionSpec + ArgumentSpec builders + run dispatcher)
// see PROVENANCE.toml for the full manifest.

//! Translated click body — decorator chains expressed as fluent Rust
//! builders that wire to clap = "4" at `Command::run`. Per-function
//! provenance lines follow.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

use std::collections::HashMap;

/// Click parameter type closed enum (constitution §2.2 forbids open
/// enums). Mirrors the four common click types we support in M-batch
/// scope: `STRING / INT / BOOL / FLOAT`. Out of scope: `Choice`,
/// `Path`, `File`, `DateTime`, `IntRange`, `UUID` (M9+).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType {
    Str,
    Int,
    Bool,
    Float,
}

// fn:OptionSpec::new provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate

/// One `@click.option(...)` decorator-call captured as data. The
/// fluent builder chain mirrors click's keyword arguments: `name`,
/// `short`, `type`, `default`, `help`, `required`.
#[derive(Clone, Debug)]
pub struct OptionSpec {
    name: String,
    short: Option<String>,
    param_type: ParamType,
    default: Option<String>,
    help: Option<String>,
    required: bool,
}

impl OptionSpec {
    /// Create from a long flag (e.g. `"--verbose"` or `"verbose"` —
    /// the leading `--` is added if missing). The bare name (without
    /// dashes) is the lookup key in `RunResult::option`.
    pub fn new(long: impl Into<String>) -> Self {
        let raw = long.into();
        let stripped = raw.trim_start_matches('-').to_string();
        Self {
            name: stripped,
            short: None,
            param_type: ParamType::Str,
            default: None,
            help: None,
            required: false,
        }
    }

    // fn:OptionSpec::short provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Add a short flag, e.g. `.short("v")` for `-v`. The leading `-`
    /// is added if missing.
    pub fn short(mut self, short: impl Into<String>) -> Self {
        let raw = short.into();
        let stripped = raw.trim_start_matches('-').to_string();
        self.short = Some(format!("-{stripped}"));
        self
    }

    // fn:OptionSpec::type_ provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Set parameter type. Mirrors click's `type=` keyword.
    pub fn type_(mut self, p: ParamType) -> Self {
        self.param_type = p;
        self
    }

    // fn:OptionSpec::default provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Set default value. Mirrors click's `default=` keyword. Stored
    /// as a string; `RunResult::option` returns the parsed default
    /// when the flag is omitted.
    pub fn default(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    // fn:OptionSpec::help provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Set help text. Mirrors click's `help=` keyword.
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    // fn:OptionSpec::required provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Mark this option as required. Mirrors click's `required=True`.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Project the bare name (without dashes) — used by
    /// `RunResult::option`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Translate self to a clap::Arg. Used by `Command::run`.
    fn to_clap(&self) -> clap::Arg {
        // Clap names live for the program lifetime; leak to satisfy
        // the &'static str bound.
        let name_static: &'static str = Box::leak(self.name.clone().into_boxed_str());
        let long_static: &'static str = Box::leak(self.name.clone().into_boxed_str());
        let mut arg = clap::Arg::new(name_static).long(long_static);
        if let Some(s) = &self.short {
            // s is `-v`; clap wants the bare char.
            if let Some(c) = s.trim_start_matches('-').chars().next() {
                arg = arg.short(c);
            }
        }
        match self.param_type {
            ParamType::Bool => {
                arg = arg.action(clap::ArgAction::SetTrue);
            }
            _ => {
                arg = arg.action(clap::ArgAction::Set);
            }
        }
        if let Some(d) = &self.default {
            let d_static: &'static str = Box::leak(d.clone().into_boxed_str());
            arg = arg.default_value(d_static);
        }
        if let Some(h) = &self.help {
            let h_static: &'static str = Box::leak(h.clone().into_boxed_str());
            arg = arg.help(h_static);
        }
        arg.required(self.required)
    }
}

// fn:ArgumentSpec::new provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate

/// One `@click.argument(...)` decorator-call captured as data.
/// Mirrors click's positional argument semantics: required by
/// default; `optional()` flips that.
#[derive(Clone, Debug)]
pub struct ArgumentSpec {
    name: String,
    param_type: ParamType,
    required: bool,
}

impl ArgumentSpec {
    /// Create a positional argument with the given name (uppercase
    /// in click; we preserve the caller's case).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: ParamType::Str,
            required: true,
        }
    }

    // fn:ArgumentSpec::type_ provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    pub fn type_(mut self, p: ParamType) -> Self {
        self.param_type = p;
        self
    }

    // fn:ArgumentSpec::optional provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Mark this argument as optional. Default is required (matching
    /// click's positional default).
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Project the argument name — used by `RunResult::argument`.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn to_clap(&self) -> clap::Arg {
        let name_static: &'static str = Box::leak(self.name.clone().into_boxed_str());
        let mut arg = clap::Arg::new(name_static);
        arg = arg.action(clap::ArgAction::Set);
        arg.required(self.required)
    }
}

// fn:Command::new provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate

/// One `@click.command(...)` decorator-call captured as data.
/// Constitution §5.1: 4 public fields (name + about + options +
/// arguments) is well within the 7-field limit, but the fields stay
/// private and the fluent builders project them.
#[derive(Clone, Debug)]
pub struct Command {
    name: String,
    about: Option<String>,
    options: Vec<OptionSpec>,
    arguments: Vec<ArgumentSpec>,
}

impl Command {
    /// Create a new command. Mirrors `@click.command(name=...)`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: None,
            options: Vec::new(),
            arguments: Vec::new(),
        }
    }

    // fn:Command::about provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Set help/about text. Mirrors `@click.command(help=...)`.
    pub fn about(mut self, help: impl Into<String>) -> Self {
        self.about = Some(help.into());
        self
    }

    // fn:Command::option provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Append an option spec — mirrors stacking a `@click.option(...)`
    /// decorator above the function.
    pub fn option(mut self, opt: OptionSpec) -> Self {
        self.options.push(opt);
        self
    }

    // fn:Command::argument provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// Append a positional argument spec — mirrors stacking a
    /// `@click.argument(...)` decorator above the function.
    pub fn argument(mut self, arg: ArgumentSpec) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Project the command name — used by tests + `RunResult` book-keeping.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Project the about text (if any).
    pub fn about_text(&self) -> Option<&str> {
        self.about.as_deref()
    }

    /// Number of registered options.
    pub fn option_count(&self) -> usize {
        self.options.len()
    }

    /// Number of registered arguments.
    pub fn argument_count(&self) -> usize {
        self.arguments.len()
    }

    // fn:Command::run provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate

    /// Parse `argv` and produce a `RunResult` keyed by option/argument
    /// name. Mirrors clicking through the click runtime — the
    /// click decorator stack is materialised here as a fluent builder
    /// chain that lowers to clap's derive-style API.
    ///
    /// The first element of `argv` is taken as the program name (per
    /// the unix convention); pass `&["prog", ...rest]` to mirror
    /// `sys.argv`.
    ///
    /// # Errors
    /// Returns [`ClickError`] kinds: `MissingOption` /
    /// `MissingArgument` / `InvalidValue` / `UsageError` per the
    /// click error taxonomy.
    pub fn run<I, T>(&self, argv: I) -> Result<RunResult, ClickError>
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let collected: Vec<String> = argv.into_iter().map(Into::into).collect();
        let name_static: &'static str = Box::leak(self.name.clone().into_boxed_str());
        let mut clap_cmd = clap::Command::new(name_static).disable_help_flag(true);
        if let Some(a) = &self.about {
            let about_static: &'static str = Box::leak(a.clone().into_boxed_str());
            clap_cmd = clap_cmd.about(about_static);
        }
        for o in &self.options {
            clap_cmd = clap_cmd.arg(o.to_clap());
        }
        for a in &self.arguments {
            clap_cmd = clap_cmd.arg(a.to_clap());
        }
        let matches = match clap_cmd.try_get_matches_from(collected.clone()) {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("{e}");
                let kind_lower = format!("{:?}", e.kind()).to_ascii_lowercase();
                let kind = if kind_lower.contains("invalidvalue")
                    || kind_lower.contains("valuevalidation")
                {
                    ClickErrorKind::InvalidValue
                } else if kind_lower.contains("missingrequired") {
                    // Use clap's error context to discover which arg is
                    // missing; cross-check against our own option list to
                    // route MissingOption vs MissingArgument.
                    let mut missing_names: Vec<String> = Vec::new();
                    for (key, value) in e.context() {
                        if matches!(key, clap::error::ContextKind::InvalidArg) {
                            if let clap::error::ContextValue::Strings(ss) = value {
                                for s in ss {
                                    missing_names.push(s.clone());
                                }
                            } else if let clap::error::ContextValue::String(s) = value {
                                missing_names.push(s.clone());
                            }
                        }
                    }
                    let opt_names: std::collections::HashSet<String> =
                        self.options.iter().map(|o| o.name.clone()).collect();
                    let any_option_missing = missing_names.iter().any(|raw| {
                        // clap renders "--name <NAME>"; strip dashes + value
                        // tag to compare against our recorded option name.
                        let bare = raw
                            .trim_start_matches('-')
                            .split_whitespace()
                            .next()
                            .unwrap_or(raw)
                            .trim_start_matches('-');
                        opt_names.contains(bare)
                    });
                    if any_option_missing {
                        ClickErrorKind::MissingOption
                    } else {
                        ClickErrorKind::MissingArgument
                    }
                } else {
                    ClickErrorKind::UsageError
                };
                return Err(ClickError { kind, message: msg });
            }
        };

        let mut option_values: HashMap<String, String> = HashMap::new();
        for o in &self.options {
            let key = o.name.clone();
            let value = match o.param_type {
                ParamType::Bool => {
                    let v = matches.get_flag(&o.name);
                    if v { "true".into() } else { "false".into() }
                }
                _ => {
                    if let Some(v) = matches.get_one::<String>(&o.name) {
                        v.clone()
                    } else if let Some(d) = &o.default {
                        d.clone()
                    } else {
                        continue;
                    }
                }
            };
            // Validate type for non-bool kinds.
            if o.param_type == ParamType::Int && value.parse::<i64>().is_err() {
                return Err(ClickError {
                    kind: ClickErrorKind::InvalidValue,
                    message: format!("option --{} requires an int, got {value:?}", o.name),
                });
            }
            if o.param_type == ParamType::Float && value.parse::<f64>().is_err() {
                return Err(ClickError {
                    kind: ClickErrorKind::InvalidValue,
                    message: format!("option --{} requires a float, got {value:?}", o.name),
                });
            }
            option_values.insert(key, value);
        }

        let mut argument_values: HashMap<String, String> = HashMap::new();
        for a in &self.arguments {
            let key = a.name.clone();
            if let Some(v) = matches.get_one::<String>(&a.name) {
                let value = v.clone();
                if a.param_type == ParamType::Int && value.parse::<i64>().is_err() {
                    return Err(ClickError {
                        kind: ClickErrorKind::InvalidValue,
                        message: format!("argument {} requires an int, got {value:?}", a.name),
                    });
                }
                if a.param_type == ParamType::Float && value.parse::<f64>().is_err() {
                    return Err(ClickError {
                        kind: ClickErrorKind::InvalidValue,
                        message: format!("argument {} requires a float, got {value:?}", a.name),
                    });
                }
                argument_values.insert(key, value);
            }
        }

        Ok(RunResult {
            options: option_values,
            arguments: argument_values,
        })
    }
}

/// Parsed result of `Command::run`. Mirrors click's
/// `Context.params` keyed by parameter name.
#[derive(Clone, Debug)]
pub struct RunResult {
    options: HashMap<String, String>,
    arguments: HashMap<String, String>,
}

impl RunResult {
    /// Look up an option by bare name (no leading dashes).
    pub fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }

    /// Look up a positional argument by name.
    pub fn argument(&self, name: &str) -> Option<&str> {
        self.arguments.get(name).map(String::as_str)
    }

    /// Number of options resolved on this run.
    pub fn option_count(&self) -> usize {
        self.options.len()
    }

    /// Number of arguments resolved on this run.
    pub fn argument_count(&self) -> usize {
        self.arguments.len()
    }
}

/// Single error type for click failures. Mirrors the union of
/// `click.exceptions.{UsageError, MissingParameter, BadParameter}`
/// from the Python form — collapsed into one Rust enum because
/// `Result<T, E>` is the default error path (constitution §2.2).
#[derive(Clone, Debug)]
pub struct ClickError {
    pub kind: ClickErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickErrorKind {
    /// `click.UsageError` — invalid command-line shape.
    UsageError,
    /// `click.MissingParameter` for an option.
    MissingOption,
    /// `click.MissingParameter` for a positional argument.
    MissingArgument,
    /// `click.BadParameter` — value present but type mismatch.
    InvalidValue,
}

impl std::fmt::Display for ClickError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            ClickErrorKind::UsageError => "usage",
            ClickErrorKind::MissingOption => "missing option",
            ClickErrorKind::MissingArgument => "missing argument",
            ClickErrorKind::InvalidValue => "invalid value",
        };
        write!(f, "click {kind}: {}", self.message)
    }
}

impl std::error::Error for ClickError {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn cmd_say_hello() -> Command {
        Command::new("say-hello")
            .about("emit a greeting")
            .option(
                OptionSpec::new("name")
                    .short("n")
                    .type_(ParamType::Str)
                    .default("world")
                    .help("name to greet"),
            )
            .option(OptionSpec::new("loud").type_(ParamType::Bool))
            .argument(ArgumentSpec::new("count").type_(ParamType::Int))
    }

    #[test]
    fn command_builder_records_decorator_chain() {
        let c = cmd_say_hello();
        assert_eq!(c.name(), "say-hello");
        assert_eq!(c.about_text(), Some("emit a greeting"));
        assert_eq!(c.option_count(), 2);
        assert_eq!(c.argument_count(), 1);
    }

    #[test]
    fn run_resolves_default_when_option_missing() {
        let c = cmd_say_hello();
        let result = c
            .run(vec!["say-hello", "5"])
            .expect("should parse with defaults");
        assert_eq!(result.option("name"), Some("world"));
        assert_eq!(result.argument("count"), Some("5"));
        assert_eq!(result.option("loud"), Some("false"));
    }

    #[test]
    fn run_picks_up_explicit_option() {
        let c = cmd_say_hello();
        let result = c
            .run(vec!["say-hello", "--name", "ada", "7"])
            .expect("explicit option");
        assert_eq!(result.option("name"), Some("ada"));
    }

    #[test]
    fn run_picks_up_short_option() {
        let c = cmd_say_hello();
        let result = c
            .run(vec!["say-hello", "-n", "lin", "3"])
            .expect("short option");
        assert_eq!(result.option("name"), Some("lin"));
    }

    #[test]
    fn run_routes_bool_flag_correctly() {
        let c = cmd_say_hello();
        let result = c.run(vec!["say-hello", "--loud", "1"]).expect("bool flag");
        assert_eq!(result.option("loud"), Some("true"));
    }

    #[test]
    fn run_returns_invalid_value_for_int_arg() {
        let c = cmd_say_hello();
        let err = c
            .run(vec!["say-hello", "not-a-number"])
            .expect_err("must reject");
        assert_eq!(err.kind, ClickErrorKind::InvalidValue);
    }

    #[test]
    fn run_returns_missing_argument_when_required_omitted() {
        let c = cmd_say_hello();
        let err = c.run(vec!["say-hello"]).expect_err("must miss arg");
        assert_eq!(err.kind, ClickErrorKind::MissingArgument);
    }

    #[test]
    fn run_returns_missing_option_when_required_omitted() {
        let c = Command::new("strict").option(
            OptionSpec::new("required-flag")
                .type_(ParamType::Str)
                .required(),
        );
        let err = c.run(vec!["strict"]).expect_err("required missing");
        assert_eq!(err.kind, ClickErrorKind::MissingOption);
    }

    #[test]
    fn argument_optional_can_be_omitted() {
        let c = Command::new("opt-arg").argument(ArgumentSpec::new("name").optional());
        let result = c.run(vec!["opt-arg"]).expect("optional arg can be omitted");
        assert!(result.argument("name").is_none());
    }

    #[test]
    fn unknown_option_is_usage_error() {
        let c = Command::new("strict");
        let err = c.run(vec!["strict", "--mystery"]).expect_err("unknown");
        // clap routes unknown args to `UnknownArgument` which we map
        // to UsageError.
        assert_eq!(err.kind, ClickErrorKind::UsageError);
    }

    #[test]
    fn float_option_validates() {
        let c = Command::new("calc").option(OptionSpec::new("rate").type_(ParamType::Float));
        let ok = c
            .run(vec!["calc", "--rate", "0.125"])
            .expect("float parses");
        assert_eq!(ok.option("rate"), Some("0.125"));
        let err = c
            .run(vec!["calc", "--rate", "not-float"])
            .expect_err("invalid float");
        assert_eq!(err.kind, ClickErrorKind::InvalidValue);
    }

    #[test]
    fn click_error_display_carries_kind() {
        let e = ClickError {
            kind: ClickErrorKind::MissingOption,
            message: "missing --required-flag".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("missing option"));
        assert!(s.contains("--required-flag"));
    }
}
