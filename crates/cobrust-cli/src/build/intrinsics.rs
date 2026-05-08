//! M10 print-intrinsic rewrite (per ADR-0024 §"Hello-world contract" option 3).
//!
//! Walks every `Body`'s terminators. When a `Terminator::Call` references
//! a callee whose resolved Body name is `print`, validate the literal
//! argument is exactly `"hello, world"` and rewrite the `func` operand
//! from `Operand::Constant(Constant::FnRef(_))` to
//! `Operand::Constant(Constant::Str("__cobrust_println_static".into()))`,
//! clearing the args.
//!
//! Any other shape — different literal, multiple args, runtime-string
//! argument — returns [`IntrinsicError::M10ScopeNarrowed`] with an
//! M11-deferral diagnostic.

use std::collections::HashSet;

use cobrust_mir::{Constant, Module, Operand, Terminator};

/// External symbol provided by `crates/cobrust-cli/runtime/m10_runtime.c`.
pub const PRINTLN_STATIC_SYMBOL: &str = "__cobrust_println_static";

/// The only literal `print` argument M10 supports.
pub const SUPPORTED_LITERAL: &str = "hello, world";

/// Errors from the print-intrinsic rewrite.
#[derive(Debug, thiserror::Error)]
pub enum IntrinsicError {
    #[error(
        "M10 narrowing: `print` accepts exactly the literal {expected:?} at this milestone; \
         got {found}. Arbitrary `print(s: str)` lowering is M11 stdlib scope (see ADR-0024)."
    )]
    M10ScopeNarrowed { expected: &'static str, found: String },
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

/// Rewrite every `print(...)` callsite in `module` to the M10 runtime helper.
///
/// # Errors
///
/// Returns [`IntrinsicError::M10ScopeNarrowed`] if any `print` callsite
/// has an argument shape M10 doesn't support.
pub fn rewrite_print(module: &mut Module) -> Result<(), IntrinsicError> {
    let print_ids = collect_print_def_ids(module);
    if print_ids.is_empty() {
        return Ok(());
    }

    // Names of the print stub Bodies. The MIR `lookup_local_for_resolved`
    // path registers a synthetic local per name reference; identifying
    // those locals by `LocalDecl::name == "print"` is the most robust
    // dataflow proxy available without re-running HIR resolution.
    let print_names: std::collections::HashSet<String> = module
        .bodies
        .iter()
        .filter(|b| print_ids.contains(&b.def_id.0))
        .map(|b| b.name.clone())
        .collect();

    // First pass: rewrite every Call callsite whose `func` operand
    // resolves to a local whose name is in `print_names`.
    for body in &mut module.bodies {
        // Skip the prelude body itself — its callsites (none) needn't
        // be rewritten and removing it later requires it to remain
        // unmodified.
        if print_ids.contains(&body.def_id.0) {
            continue;
        }

        // Build local-id → name map.
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

                // Validate the M10-narrowed argument shape.
                if args.len() != 1 {
                    return Err(IntrinsicError::M10ScopeNarrowed {
                        expected: SUPPORTED_LITERAL,
                        found: format!("{} arg(s)", args.len()),
                    });
                }
                let s_lit = match &args[0] {
                    Operand::Constant(Constant::Str(s)) => Some(s.clone()),
                    _ => None,
                };
                let s_lit = match s_lit {
                    Some(s) => s,
                    None => {
                        return Err(IntrinsicError::M10ScopeNarrowed {
                            expected: SUPPORTED_LITERAL,
                            found: format!("non-literal arg {:?}", &args[0]),
                        });
                    }
                };
                if s_lit != SUPPORTED_LITERAL {
                    return Err(IntrinsicError::M10ScopeNarrowed {
                        expected: SUPPORTED_LITERAL,
                        found: format!("literal {s_lit:?}"),
                    });
                }

                // Rewrite: callee → external runtime symbol; args → empty.
                *func = Operand::Constant(Constant::Str(PRINTLN_STATIC_SYMBOL.to_string()));
                args.clear();
            }
        }
    }

    // Second pass: drop the M10 prelude `print` stub Body entirely.
    // After the rewrite, no callsite references it; keeping it would
    // force codegen to lower the trivial stub, and the M8 drop schedule
    // for `s: str` parameters in a body that doesn't move them produces
    // dangling drop-chain blocks targeting the entry block — a known
    // M8 issue tracked separately. Removing the body sidesteps the issue
    // cleanly because the intrinsic call now goes to an imported symbol.
    module.bodies.retain(|body| !print_ids.contains(&body.def_id.0));

    Ok(())
}
