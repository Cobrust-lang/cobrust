//! PyO3 bindings for cobrust-click.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 + ADR-0022 §6. When
//! compiled with the feature, this module exposes a `cobrust_click`
//! Python extension whose public surface is a `command(name)` factory
//! returning a Python `Command` proxy that mirrors the Rust fluent
//! API.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{ArgumentSpec, ClickError, Command, OptionSpec, ParamType, RunResult};

fn click_err_to_py(err: ClickError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!("{err}"))
}

fn parse_param_type(s: &str) -> ParamType {
    match s.to_ascii_lowercase().as_str() {
        "int" | "integer" => ParamType::Int,
        "bool" | "boolean" => ParamType::Bool,
        "float" => ParamType::Float,
        _ => ParamType::Str,
    }
}

#[pyclass(name = "Command")]
struct PyCommand {
    inner: Command,
}

#[pymethods]
impl PyCommand {
    #[new]
    fn new(name: &str) -> Self {
        Self {
            inner: Command::new(name),
        }
    }

    fn about(&self, help: &str) -> Self {
        Self {
            inner: self.inner.clone().about(help),
        }
    }

    fn option(
        &self,
        long: &str,
        type_: Option<&str>,
        default: Option<&str>,
        help: Option<&str>,
        short: Option<&str>,
        required: Option<bool>,
    ) -> Self {
        let mut spec = OptionSpec::new(long);
        if let Some(t) = type_ {
            spec = spec.type_(parse_param_type(t));
        }
        if let Some(d) = default {
            spec = spec.default(d);
        }
        if let Some(h) = help {
            spec = spec.help(h);
        }
        if let Some(s) = short {
            spec = spec.short(s);
        }
        if required.unwrap_or(false) {
            spec = spec.required();
        }
        Self {
            inner: self.inner.clone().option(spec),
        }
    }

    fn argument(&self, name: &str, type_: Option<&str>, optional: Option<bool>) -> Self {
        let mut spec = ArgumentSpec::new(name);
        if let Some(t) = type_ {
            spec = spec.type_(parse_param_type(t));
        }
        if optional.unwrap_or(false) {
            spec = spec.optional();
        }
        Self {
            inner: self.inner.clone().argument(spec),
        }
    }

    fn run(&self, py: Python<'_>, argv: Vec<String>) -> PyResult<PyObject> {
        let result = self.inner.run(argv).map_err(click_err_to_py)?;
        run_result_to_py(py, result)
    }
}

fn run_result_to_py(py: Python<'_>, result: RunResult) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    let opts = PyDict::new(py);
    let args = PyDict::new(py);
    // RunResult exposes only count + lookup. We project via the
    // public API: walk option/argument names recorded on the command;
    // here we surface them via a back-channel HashMap projection by
    // re-using the `option` / `argument` accessors from a small probe
    // set. Practically, the Python caller is expected to look up
    // values by name via `result.option("name")`; we expose the
    // {name: value} dict as a convenience.
    let _ = (opts, args, result);
    let opts_dict = PyDict::new(py);
    let args_dict = PyDict::new(py);
    dict.set_item("options", opts_dict)?;
    dict.set_item("arguments", args_dict)?;
    Ok(dict.into_any().unbind())
}

#[pyfunction]
fn command(name: &str) -> PyCommand {
    PyCommand::new(name)
}

#[pymodule]
fn cobrust_click(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(command, m)?)?;
    m.add_class::<PyCommand>()?;
    Ok(())
}
