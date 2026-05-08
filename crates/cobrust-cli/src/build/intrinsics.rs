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

/// Errors from the print-intrinsic rewrite.
#[derive(Debug, thiserror::Error)]
pub enum IntrinsicError {
    #[error(
        "M11: `print` accepts a string literal at this milestone; got {found}. \
         Non-literal arguments require full stdlib FnRef dispatch (M11.x scope, ADR-0025)."
    )]
    PrintArgUnsupported { found: String },
}

/// Identify Body def_ids whose name matches a recognized print intrinsic.
/// Returns `(print_ids, print_int_ids)`.
fn collect_print_def_ids(module: &Module) -> (HashSet<u32>, HashSet<u32>) {
    let mut print_ids = HashSet::new();
    let mut print_int_ids = HashSet::new();
    for body in &module.bodies {
        if body.name == "print" {
            print_ids.insert(body.def_id.0);
        } else if body.name == "print_int" {
            print_int_ids.insert(body.def_id.0);
        }
    }
    (print_ids, print_int_ids)
}

/// Rewrite every `print(...)` and `print_int(...)` callsite in `module`
/// to the appropriate runtime helpers.
///
/// - `print(s: str)` → `__cobrust_println(ptr, len)` (string-literal path)
/// - `print_int(n: i64)` → `__cobrust_println_int(n)` (integer path)
///
/// Per ADR-0025 §"Print-intrinsic lift": the `print` rewrite preserves
/// the literal string argument so codegen can lower to a
/// `(*const u8, usize)` C-ABI call. The M10 narrowing to
/// `"hello, world"` is removed.
///
/// Per ADR-0030 §Decision step 5: the `print_int` rewrite passes the
/// integer operand directly; codegen lowers it to `i64` via the
/// `runtime_funcs` table.
///
/// # Errors
///
/// Returns [`IntrinsicError::PrintArgUnsupported`] if any `print`
/// callsite has a non-literal argument or a wrong arg count.
pub fn rewrite_print(module: &mut Module) -> Result<(), IntrinsicError> {
    let (print_ids, print_int_ids) = collect_print_def_ids(module);
    let all_stub_ids: HashSet<u32> = print_ids.union(&print_int_ids).copied().collect();

    if all_stub_ids.is_empty() {
        return Ok(());
    }

    let print_names: HashSet<String> = module
        .bodies
        .iter()
        .filter(|b| print_ids.contains(&b.def_id.0))
        .map(|b| b.name.clone())
        .collect();

    let print_int_names: HashSet<String> = module
        .bodies
        .iter()
        .filter(|b| print_int_ids.contains(&b.def_id.0))
        .map(|b| b.name.clone())
        .collect();

    for body in &mut module.bodies {
        if all_stub_ids.contains(&body.def_id.0) {
            continue;
        }

        // Build local-id → name map for this body.
        let mut local_name: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for decl in &body.locals {
            local_name.insert(decl.id.0, decl.name.clone());
        }

        for block in &mut body.blocks {
            let term = &mut block.terminator;
            if let Terminator::Call {
                func,
                args,
                destination: _,
                target: _,
                unwind: _,
            } = term
            {
                // Identify callsite kind.
                let is_print = match func {
                    Operand::Copy(p) | Operand::Move(p) => {
                        if p.projections.is_empty() {
                            local_name
                                .get(&p.local.0)
                                .is_some_and(|n| print_names.contains(n))
                        } else {
                            false
                        }
                    }
                    Operand::Constant(Constant::FnRef(d)) => print_ids.contains(d),
                    Operand::Constant(_) => false,
                };
                let is_print_int = match func {
                    Operand::Copy(p) | Operand::Move(p) => {
                        if p.projections.is_empty() {
                            local_name
                                .get(&p.local.0)
                                .is_some_and(|n| print_int_names.contains(n))
                        } else {
                            false
                        }
                    }
                    Operand::Constant(Constant::FnRef(d)) => print_int_ids.contains(d),
                    Operand::Constant(_) => false,
                };

                if is_print {
                    // M11 contract: exactly one string-literal argument.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("{} arg(s)", args.len()),
                        });
                    }
                    let s_lit = match &args[0] {
                        Operand::Constant(Constant::Str(s)) => s.clone(),
                        other => {
                            return Err(IntrinsicError::PrintArgUnsupported {
                                found: format!("non-literal arg {other:?}"),
                            });
                        }
                    };

                    // Rewrite: callee → runtime symbol; preserve the
                    // literal arg. Codegen reads `args[0] = Constant::Str(s)`
                    // to emit the (*const u8, usize) C-ABI call.
                    *func = Operand::Constant(Constant::Str(PRINTLN_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(Operand::Constant(Constant::Str(s_lit)));
                } else if is_print_int {
                    // ADR-0030 §Decision step 5: rewrite print_int(n) to
                    // __cobrust_println_int(n). The integer operand passes
                    // through unchanged; codegen lowers it as i64 via the
                    // runtime_funcs table.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("print_int: expected 1 arg, got {}", args.len()),
                        });
                    }
                    // Preserve the existing integer operand — just redirect
                    // the callee to the runtime symbol.
                    let int_arg = args[0].clone();
                    *func =
                        Operand::Constant(Constant::Str(PRINTLN_INT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(int_arg);
                }
            }
        }
    }

    // Drop all prelude stub Bodies (print + print_int). After rewrite no
    // callsite references them.
    module
        .bodies
        .retain(|body| !all_stub_ids.contains(&body.def_id.0));

    Ok(())
}
