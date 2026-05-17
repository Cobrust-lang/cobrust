//! Bidirectional inference engine: substitution + unification.
//!
//! The implementation is first-order with occurs-check, exactly as
//! specified by ADR-0006 §"Inference algorithm: bidirectional". The
//! `Subst` map maintains the running substitution; `unify` produces
//! a new substitution that satisfies the equation `t1 == t2` (under
//! the universal closure of generic variables) or returns a
//! `TypeError`.

use std::collections::HashMap;

use cobrust_frontend::span::Span;

use crate::error::TypeError;
use crate::ty::{FnTy, Record, Ty, VarId};

/// Running substitution: a map from inference variable to the
/// concrete type that variable has been resolved to.
#[derive(Clone, Debug, Default)]
pub struct Subst {
    map: HashMap<VarId, Ty>,
}

impl Subst {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, v: VarId) -> Option<&Ty> {
        self.map.get(&v)
    }

    pub fn extend(&mut self, v: VarId, t: Ty) {
        self.map.insert(v, t);
    }

    /// Apply the substitution to a type: walk through any chained
    /// `Var → Var` indirections.
    #[must_use]
    pub fn apply(&self, t: &Ty) -> Ty {
        match t {
            Ty::Var(v) => match self.map.get(v) {
                Some(inner) => self.apply(inner),
                None => Ty::Var(*v),
            },
            Ty::Tuple(items) => Ty::Tuple(items.iter().map(|i| self.apply(i)).collect()),
            Ty::List(t) => Ty::List(Box::new(self.apply(t))),
            Ty::Set(t) => Ty::Set(Box::new(self.apply(t))),
            Ty::Dict(k, v) => Ty::Dict(Box::new(self.apply(k)), Box::new(self.apply(v))),
            Ty::Record(r) => Ty::Record(Record {
                fields: r
                    .fields
                    .iter()
                    .map(|(k, t)| (k.clone(), self.apply(t)))
                    .collect(),
            }),
            Ty::Fn(fn_ty) => Ty::Fn(FnTy {
                positional: fn_ty.positional.iter().map(|t| self.apply(t)).collect(),
                named: fn_ty
                    .named
                    .iter()
                    .map(|(n, t)| (n.clone(), self.apply(t)))
                    .collect(),
                var_positional: fn_ty
                    .var_positional
                    .as_ref()
                    .map(|t| Box::new(self.apply(t))),
                var_keyword: fn_ty.var_keyword.as_ref().map(|t| Box::new(self.apply(t))),
                return_ty: Box::new(self.apply(&fn_ty.return_ty)),
            }),
            Ty::Adt(id, args) => Ty::Adt(*id, args.iter().map(|t| self.apply(t)).collect()),
            Ty::Alias(id, args) => Ty::Alias(*id, args.iter().map(|t| self.apply(t)).collect()),
            // ADR-0052a Wave-1 — substitution walks into `&T` so an
            // inner `Var` resolves through the surrounding `Ref` (e.g.
            // `Ref(?0)` with `?0 := Str` becomes `Ref(Str)`). This is
            // a structural walk, NOT a transparency rule — `Ref(T)`
            // and `T` remain distinct types per §3 + §13.
            Ty::Ref(inner) => Ty::Ref(Box::new(self.apply(inner))),
            other => other.clone(),
        }
    }

    pub fn fully_resolved(&self, t: &Ty) -> bool {
        let resolved = self.apply(t);
        resolved.free_vars().is_empty()
    }
}

/// Unify two types under the running substitution.
///
/// On success, the substitution is extended in place. On failure,
/// returns a `TypeError`.
pub fn unify(t1: &Ty, t2: &Ty, subst: &mut Subst, span: Span) -> Result<(), TypeError> {
    let t1 = subst.apply(t1);
    let t2 = subst.apply(t2);
    match (t1.clone(), t2.clone()) {
        // `Never` unifies with anything (it is bottom).
        (Ty::Never, _) | (_, Ty::Never) => Ok(()),

        // Inference variables.
        (Ty::Var(v1), Ty::Var(v2)) if v1 == v2 => Ok(()),
        (Ty::Var(v), other) | (other, Ty::Var(v)) => {
            if other.free_vars().contains(&v) {
                return Err(TypeError::OccursCheck {
                    var: v,
                    ty: other,
                    span,
                });
            }
            subst.extend(v, other);
            Ok(())
        }

        // Atomic equality.
        (Ty::Bool, Ty::Bool)
        | (Ty::Int, Ty::Int)
        | (Ty::Float, Ty::Float)
        | (Ty::Imag, Ty::Imag)
        | (Ty::Str, Ty::Str)
        | (Ty::Bytes, Ty::Bytes)
        | (Ty::None, Ty::None) => Ok(()),

        // Compounds — unify pointwise.
        (Ty::Tuple(a), Ty::Tuple(b)) => {
            if a.len() != b.len() {
                return Err(TypeError::TypeMismatch {
                    expected: t1,
                    actual: t2,
                    span,
                });
            }
            for (x, y) in a.iter().zip(b.iter()) {
                unify(x, y, subst, span)?;
            }
            Ok(())
        }
        (Ty::List(a), Ty::List(b)) => unify(&a, &b, subst, span),
        (Ty::Set(a), Ty::Set(b)) => unify(&a, &b, subst, span),
        // ADR-0052a Wave-1 — `&T1` and `&T2` unify pointwise on the
        // inner types. This is structural unification (same shape as
        // List/Set above), **NOT** transparency: `Ref(T)` does not
        // unify with `T` (no `(Ref(a), b)`/`(b, Ref(a))` arm here —
        // the v1+v2 cascade root per §13 "Design lesson 2026-05-17").
        // The §3 Wave-1 transparency rule lives at `synth_call`
        // argument-binding as a one-way coercion.
        (Ty::Ref(a), Ty::Ref(b)) => unify(&a, &b, subst, span),
        (Ty::Dict(ak, av), Ty::Dict(bk, bv)) => {
            unify(&ak, &bk, subst, span)?;
            unify(&av, &bv, subst, span)
        }
        (Ty::Record(a), Ty::Record(b)) => {
            // Closed records: same field set + per-field unify.
            if a.fields.keys().collect::<Vec<_>>() != b.fields.keys().collect::<Vec<_>>() {
                return Err(TypeError::TypeMismatch {
                    expected: Ty::Record(a),
                    actual: Ty::Record(b),
                    span,
                });
            }
            for (k, av) in &a.fields {
                let bv = b.fields.get(k).expect("keys checked above");
                unify(av, bv, subst, span)?;
            }
            Ok(())
        }
        (Ty::Fn(a), Ty::Fn(b)) => {
            if a.positional.len() != b.positional.len() {
                return Err(TypeError::ArityMismatch {
                    expected: a.positional.len(),
                    actual: b.positional.len(),
                    span,
                });
            }
            if a.named.len() != b.named.len() {
                return Err(TypeError::ArityMismatch {
                    expected: a.named.len(),
                    actual: b.named.len(),
                    span,
                });
            }
            for (x, y) in a.positional.iter().zip(b.positional.iter()) {
                unify(x, y, subst, span)?;
            }
            for ((n1, x), (n2, y)) in a.named.iter().zip(b.named.iter()) {
                if n1 != n2 {
                    return Err(TypeError::KeywordArgMismatch {
                        name: n1.clone(),
                        span,
                    });
                }
                unify(x, y, subst, span)?;
            }
            unify(&a.return_ty, &b.return_ty, subst, span)
        }
        (Ty::Adt(id_a, args_a), Ty::Adt(id_b, args_b)) if id_a == id_b => {
            if args_a.len() != args_b.len() {
                return Err(TypeError::TypeMismatch {
                    expected: Ty::Adt(id_a, args_a),
                    actual: Ty::Adt(id_b, args_b),
                    span,
                });
            }
            for (x, y) in args_a.iter().zip(args_b.iter()) {
                unify(x, y, subst, span)?;
            }
            Ok(())
        }
        (Ty::Alias(id_a, args_a), Ty::Alias(id_b, args_b)) if id_a == id_b => {
            if args_a.len() != args_b.len() {
                return Err(TypeError::TypeMismatch {
                    expected: Ty::Alias(id_a, args_a),
                    actual: Ty::Alias(id_b, args_b),
                    span,
                });
            }
            for (x, y) in args_a.iter().zip(args_b.iter()) {
                unify(x, y, subst, span)?;
            }
            Ok(())
        }
        (Ty::Generic(g1), Ty::Generic(g2)) if g1 == g2 => Ok(()),

        // Otherwise: mismatch.
        _ => Err(TypeError::TypeMismatch {
            expected: t1,
            actual: t2,
            span,
        }),
    }
}

/// Resolve a type fully under the running substitution; return
/// `Err(AmbiguousType)` if any inference variable remains.
pub fn finalize(t: &Ty, subst: &Subst, span: Span) -> Result<Ty, TypeError> {
    let resolved = subst.apply(t);
    if resolved.free_vars().is_empty() {
        Ok(resolved)
    } else {
        Err(TypeError::AmbiguousType { span })
    }
}
