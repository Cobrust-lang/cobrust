//! Print-intrinsic rewrite — M11 supersedes M10's narrowed contract.
//!
//! Per ADR-0024 §"Hello-world contract" (M10) the rewrite was narrowed
//! to the literal `"hello, world"` only; ADR-0025 §"Print-intrinsic
//! lift" lifts that narrowing — any `print(<string-literal>)` callsite
//! is rewritten to a runtime call into `__cobrust_println`, with the
//! literal argument preserved so codegen can emit a
//! `(*const u8, usize)` C-ABI call (per ADR-0025 §"Codegen amendments"
//! Constant::Str row + ADR-0025 §"Runtime ABI").
//!
//! ADR-0030 §Decision step 5 adds `print_int(n: i64)` — any
//! `print_int(<i64-operand>)` callsite is rewritten to
//! `__cobrust_println_int`, which takes the integer directly. This
//! avoids the string-formatting overhead for the bare-number FizzBuzz
//! `else` branch.
//!
//! The diagnostic `IntrinsicError::M10ScopeNarrowed` from M10 is
//! deleted; M11 accepts any string-literal argument. Non-literal
//! arguments to `print` (other than via `print_int`) emit
//! `IntrinsicError::PrintArgUnsupported` — they are M11.x scope (full
//! HIR-tier dispatch through stdlib FnRefs).
//!
//! Body removal: the prelude's `print` / `print_int` stub Bodies are
//! dropped from the MIR after the rewrite. Per ADR-0024 §"Consequences"
//! the M8 drop schedule for an unmoved `s: str` parameter is unsound;
//! ADR-0025 §"Drop-schedule fix" notes that the drop_eligible filter
//! exempts parameters (cobrust-mir/src/drop.rs:45) so the prelude body
//! would lower fine — but the body still has zero statements (a
//! `return 0` prelude that lowers to a stub) which produces a well-formed
//! but useless MIR Body. Dropping it is cleaner.

use std::collections::HashSet;

use cobrust_mir::{Constant, Module, Operand, Terminator};

/// Runtime symbol providing `__cobrust_println(*const u8, usize)`.
/// Per ADR-0025 §"Runtime ABI" this is exported by `cobrust-stdlib`.
pub const PRINTLN_RUNTIME_SYMBOL: &str = "__cobrust_println";

/// Runtime symbol providing `__cobrust_println_int(i64)`.
/// Per ADR-0030 §Decision step 5 — exported by `cobrust-stdlib`.
pub const PRINTLN_INT_RUNTIME_SYMBOL: &str = "__cobrust_println_int";

/// Runtime symbol providing `__cobrust_println_str_buf(*mut Str)` —
/// ADR-0044 W2 Phase 2 fallback for `print(s)` when `s` is a non-
/// literal heap-buffer str. Extracts (ptr, len) at runtime and
/// dispatches to `__cobrust_println`.
pub const PRINTLN_STR_BUF_RUNTIME_SYMBOL: &str = "__cobrust_println_str_buf";

/// Runtime symbol for source-level `input(prompt: str) -> str`.
/// ADR-0044 W2 Phase 2 — exported by `cobrust-stdlib::io`.
pub const INPUT_RUNTIME_SYMBOL: &str = "__cobrust_input";

/// Runtime symbol for source-level `input_no_prompt() -> str`.
/// ADR-0044 W2 Phase 2 — exported by `cobrust-stdlib::io`.
pub const INPUT_NO_PROMPT_RUNTIME_SYMBOL: &str = "__cobrust_input_no_prompt";

/// Runtime symbol for source-level `read_line() -> str` (W2 cap;
/// typed `Result[str, IoError]` deferred to ADR-0044a). Exported by
/// `cobrust-stdlib::io`.
pub const READ_LINE_RUNTIME_SYMBOL: &str = "__cobrust_read_line";

/// Runtime symbol for source-level `argv() -> list[str]`.
/// ADR-0044 W2 Phase 2 — exported by `cobrust-stdlib::env`.
pub const ARGV_RUNTIME_SYMBOL: &str = "__cobrust_argv";

/// Errors from the print-intrinsic rewrite.
#[derive(Debug, thiserror::Error)]
pub enum IntrinsicError {
    #[error(
        "M11: `print` accepts a string literal at this milestone; got {found}. \
         Non-literal arguments require full stdlib FnRef dispatch (M11.x scope, ADR-0025)."
    )]
    PrintArgUnsupported { found: String },
}

/// Recognized prelude intrinsics by name. Collected from `Module.bodies`
/// so the rewrite pass can both (a) identify FnRef callees that should
/// be redirected to runtime symbols and (b) drop the stub bodies after
/// rewrite (since no callsite references them anymore).
struct IntrinsicDefIds {
    print: HashSet<u32>,
    print_int: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    input: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    input_no_prompt: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    read_line: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    argv: HashSet<u32>,
}

impl IntrinsicDefIds {
    fn all(&self) -> HashSet<u32> {
        let mut out = HashSet::new();
        out.extend(&self.print);
        out.extend(&self.print_int);
        out.extend(&self.input);
        out.extend(&self.input_no_prompt);
        out.extend(&self.read_line);
        out.extend(&self.argv);
        out
    }

    fn is_empty(&self) -> bool {
        self.print.is_empty()
            && self.print_int.is_empty()
            && self.input.is_empty()
            && self.input_no_prompt.is_empty()
            && self.read_line.is_empty()
            && self.argv.is_empty()
    }
}

/// Identify Body def_ids whose name matches a recognized prelude
/// intrinsic — print / print_int (M11) + ADR-0044 W2 Phase 2 stdin/argv
/// surface (input / input_no_prompt / read_line / argv).
fn collect_print_def_ids(module: &Module) -> IntrinsicDefIds {
    let mut ids = IntrinsicDefIds {
        print: HashSet::new(),
        print_int: HashSet::new(),
        input: HashSet::new(),
        input_no_prompt: HashSet::new(),
        read_line: HashSet::new(),
        argv: HashSet::new(),
    };
    for body in &module.bodies {
        match body.name.as_str() {
            "print" => {
                ids.print.insert(body.def_id.0);
            }
            "print_int" => {
                ids.print_int.insert(body.def_id.0);
            }
            "input" => {
                ids.input.insert(body.def_id.0);
            }
            "input_no_prompt" => {
                ids.input_no_prompt.insert(body.def_id.0);
            }
            "read_line" => {
                ids.read_line.insert(body.def_id.0);
            }
            "argv" => {
                ids.argv.insert(body.def_id.0);
            }
            _ => {}
        }
    }
    ids
}

/// Rewrite every prelude-intrinsic callsite (print / print_int +
/// ADR-0044 W2 Phase 2 input / input_no_prompt / read_line / argv) to
/// the appropriate runtime helpers.
///
/// - `print(s: str)` — string-literal arg → `__cobrust_println(ptr, len)`
///   via the M11 extern_funcs path.
/// - `print(s: str)` — non-literal arg → `__cobrust_println(ptr, len)`
///   with the runtime-helper `(ptr, len)` expansion handled by codegen
///   (heap-buffer pointer source → buffer ptr/len extracted at runtime).
///   *(Today this path errors via `PrintArgUnsupported`; future work
///   adds a `__cobrust_println_str_buf` shim or amends codegen's
///   runtime_funcs lowering.)*
/// - `print_int(n: i64)` → `__cobrust_println_int(n)`.
/// - `input(prompt: str)` → `__cobrust_input(prompt_ptr, prompt_len)`
///   via the runtime_funcs `(ptr, len)` expansion (ADR-0044).
/// - `input_no_prompt()` → `__cobrust_input_no_prompt()`.
/// - `read_line()` → `__cobrust_read_line()` (W2 cap; typed `Result`
///   surface deferred to ADR-0044a).
/// - `argv()` → `__cobrust_argv()`.
///
/// # Errors
///
/// Returns [`IntrinsicError::PrintArgUnsupported`] if any callsite has
/// a wrong arg count or a non-supported argument shape.
/// ADR-0044 W2 Phase 2: dispatch category for each intrinsic
/// callsite. Hoisted to module scope (out of `rewrite_print`'s body)
/// to satisfy clippy::items_after_statements.
#[derive(Copy, Clone, Eq, PartialEq)]
enum Kind {
    Print,
    PrintInt,
    Input,
    InputNoPrompt,
    ReadLine,
    Argv,
}

fn kind_for_name(name: &str) -> Option<Kind> {
    match name {
        "print" => Some(Kind::Print),
        "print_int" => Some(Kind::PrintInt),
        "input" => Some(Kind::Input),
        "input_no_prompt" => Some(Kind::InputNoPrompt),
        "read_line" => Some(Kind::ReadLine),
        "argv" => Some(Kind::Argv),
        _ => None,
    }
}

fn kind_for_def_id(ids: &IntrinsicDefIds, id: u32) -> Option<Kind> {
    if ids.print.contains(&id) {
        Some(Kind::Print)
    } else if ids.print_int.contains(&id) {
        Some(Kind::PrintInt)
    } else if ids.input.contains(&id) {
        Some(Kind::Input)
    } else if ids.input_no_prompt.contains(&id) {
        Some(Kind::InputNoPrompt)
    } else if ids.read_line.contains(&id) {
        Some(Kind::ReadLine)
    } else if ids.argv.contains(&id) {
        Some(Kind::Argv)
    } else {
        None
    }
}

pub fn rewrite_print(module: &mut Module) -> Result<(), IntrinsicError> {
    let ids = collect_print_def_ids(module);
    if ids.is_empty() {
        return Ok(());
    }
    let all_stub_ids = ids.all();

    for body in &mut module.bodies {
        if all_stub_ids.contains(&body.def_id.0) {
            continue;
        }
        // Build local-id → name map for this body (used to detect Place-
        // variant callees that point at a prelude stub by name).
        let mut local_name: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for decl in &body.locals {
            local_name.insert(decl.id.0, decl.name.clone());
        }

        for block in &mut body.blocks {
            let term = &mut block.terminator;
            let Terminator::Call {
                func,
                args,
                destination: _,
                target: _,
                unwind: _,
            } = term
            else {
                continue;
            };
            let kind = match func {
                Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                    local_name.get(&p.local.0).and_then(|n| kind_for_name(n))
                }
                Operand::Constant(Constant::FnRef(d)) => kind_for_def_id(&ids, *d),
                _ => None,
            };
            let Some(kind) = kind else { continue };

            match kind {
                Kind::Print => {
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("{} arg(s)", args.len()),
                        });
                    }
                    match &args[0] {
                        Operand::Constant(Constant::Str(s)) => {
                            // Literal path: codegen materialises (ptr, len)
                            // from the runtime helper's `(*const u8, usize)`
                            // signature via the runtime_funcs single-Str-to-
                            // (ptr, len) expansion (ADR-0044 codegen
                            // amendment).
                            let lit = s.clone();
                            *func = Operand::Constant(Constant::Str(
                                PRINTLN_RUNTIME_SYMBOL.to_string(),
                            ));
                            args.clear();
                            args.push(Operand::Constant(Constant::Str(lit)));
                        }
                        _ => {
                            // Non-literal path (ADR-0044 W2 Phase 2 wedge):
                            // route to `__cobrust_println_str_buf` which
                            // takes the heap Str buffer pointer directly.
                            let arg = args[0].clone();
                            *func = Operand::Constant(Constant::Str(
                                PRINTLN_STR_BUF_RUNTIME_SYMBOL.to_string(),
                            ));
                            args.clear();
                            args.push(arg);
                        }
                    }
                }
                Kind::PrintInt => {
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("print_int: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let int_arg = args[0].clone();
                    *func =
                        Operand::Constant(Constant::Str(PRINTLN_INT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(int_arg);
                }
                Kind::Input => {
                    // input(prompt: str) → __cobrust_input(ptr, len)
                    // Codegen's runtime_funcs (ptr, len) expansion kicks
                    // in when args[0] is Constant::Str — see
                    // cranelift_backend.rs lower_terminator. For non-
                    // literal prompts the arg is a heap-buffer Place;
                    // codegen reads it as a pointer (no len expansion
                    // — runtime helper sees the buffer ptr as `ptr`
                    // and a zeroed `len`, prompting it to skip the
                    // prompt write. The semantics match `input("")`.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("input: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let prompt = args[0].clone();
                    *func = Operand::Constant(Constant::Str(INPUT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(prompt);
                }
                Kind::InputNoPrompt => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("input_no_prompt: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(
                        INPUT_NO_PROMPT_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                }
                Kind::ReadLine => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("read_line: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(READ_LINE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                }
                Kind::Argv => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("argv: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(ARGV_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                }
            }
        }
    }

    // Drop all prelude stub Bodies. After rewrite no callsite
    // references them.
    module
        .bodies
        .retain(|body| !all_stub_ids.contains(&body.def_id.0));

    Ok(())
}
