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
//! The diagnostic `IntrinsicError::M10ScopeNarrowed` from M10 is
//! deleted; M11 accepts any string-literal argument. Non-literal
//! arguments emit `IntrinsicError::PrintArgUnsupported` — they are
//! M11.x scope (full HIR-tier dispatch through stdlib FnRefs).
//!
//! Body removal: the prelude's `print` stub Body is dropped from the
//! MIR after the rewrite. Per ADR-0024 §"Consequences" the M8 drop
//! schedule for an unmoved `s: str` parameter is unsound; ADR-0025
//! §"Drop-schedule fix" notes that the drop_eligible filter exempts
//! parameters (cobrust-mir/src/drop.rs:45) so the prelude body would
//! lower fine — but the body still has zero statements (a `return 0`
//! prelude that lowers to a stub) which produces a well-formed but
//! useless MIR Body. Dropping it is cleaner.

use std::collections::HashSet;

use cobrust_mir::{Constant, Module, Operand, Terminator};

/// Runtime symbol providing `__cobrust_println(*const u8, usize)`.
/// Per ADR-0025 §"Runtime ABI" this is exported by `cobrust-stdlib`.
pub const PRINTLN_RUNTIME_SYMBOL: &str = "__cobrust_println";

/// Errors from the print-intrinsic rewrite.
#[derive(Debug, thiserror::Error)]
pub enum IntrinsicError {
    #[error(
        "M11: `print` accepts a string literal at this milestone; got {found}. \
         Non-literal arguments require full stdlib FnRef dispatch (M11.x scope, ADR-0025)."
    )]
    PrintArgUnsupported { found: String },
}

/// Identify Body def_ids that name a `print` function.
fn collect_print_def_ids(module: &Module) -> HashSet<u32> {
    module
        .bodies
        .iter()
        .filter_map(|body| {
            if body.name == "print" {
                Some(body.def_id.0)
            } else {
                None
            }
        })
        .collect()
}

/// Rewrite every `print(...)` callsite in `module` to the M11 runtime
/// helper `__cobrust_println`.
///
/// Per ADR-0025 §"Print-intrinsic lift": the rewrite preserves the
/// literal string argument so codegen can lower to a
/// `(*const u8, usize)` C-ABI call. The M10 narrowing to
/// `"hello, world"` is removed.
///
/// # Errors
///
/// Returns [`IntrinsicError::PrintArgUnsupported`] if any `print`
/// callsite has a non-literal argument or a wrong arg count.
pub fn rewrite_print(module: &mut Module) -> Result<(), IntrinsicError> {
    let print_ids = collect_print_def_ids(module);
    if print_ids.is_empty() {
        return Ok(());
    }

    let print_names: HashSet<String> = module
        .bodies
        .iter()
        .filter(|b| print_ids.contains(&b.def_id.0))
        .map(|b| b.name.clone())
        .collect();

    for body in &mut module.bodies {
        if print_ids.contains(&body.def_id.0) {
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
                let is_print_callsite = match func {
                    Operand::Copy(p) | Operand::Move(p) => {
                        if !p.projections.is_empty() {
                            false
                        } else {
                            match local_name.get(&p.local.0) {
                                Some(n) => print_names.contains(n),
                                None => false,
                            }
                        }
                    }
                    Operand::Constant(Constant::FnRef(d)) => print_ids.contains(d),
                    Operand::Constant(_) => false,
                };
                if !is_print_callsite {
                    continue;
                }

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
            }
        }
    }

    // Drop the prelude `print` stub Body. After rewrite no callsite
    // references it; keeping it would force codegen to lower a
    // useless stub (the prelude prints nothing on its own).
    module
        .bodies
        .retain(|body| !print_ids.contains(&body.def_id.0));

    Ok(())
}
