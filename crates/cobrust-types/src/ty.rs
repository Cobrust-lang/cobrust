//! Type universe (`Ty`) — pinned by ADR-0006 §"Type universe".
//!
//! Two equality views matter:
//!
//! - **Structural unification**: two types unify iff they describe
//!   the same shape after substitution, with `TypeVar` filled in.
//! - **Display equality**: useful for diagnostics. Implemented via
//!   [`std::fmt::Display`].
//!
//! No subtyping. No implicit coercion. `Never` is bottom *for flow
//! analysis only* — the type system treats joining `T` with `Never`
//! as `T`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

/// `TypeVar` ids are allocated by the inference engine; they do not
/// share a namespace with HIR `DefId`s.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct VarId(pub u32);

/// Generic-parameter identifier (universally quantified type
/// variable; distinct from inference unknowns).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct GenericVar(pub u32);

/// ADT identifier. M2 allocates one ADT per `class_def`; the same
/// `DefId` from HIR maps to the corresponding ADT.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct AdtId(pub u32);

/// Type-alias identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct AliasId(pub u32);

/// The full type universe. See ADR-0006.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ty {
    /// `bool`.
    Bool,
    /// Integer (M2 single-width, `i64`).
    Int,
    /// `f64`.
    Float,
    /// Imaginary literal stub — accepted as a literal type but
    /// arithmetic is rejected at M2.
    Imag,
    /// `str`.
    Str,
    /// `bytes`.
    Bytes,
    /// Unit type.
    None,
    /// Bottom — the result type of `raise` and never-returning
    /// calls. ADR-0006 §"`Never` is a *bottom* type."
    Never,
    /// Positional, fixed-size tuple.
    Tuple(Vec<Ty>),
    /// Homogeneous list `List[T]`.
    List(Box<Ty>),
    /// Homogeneous set `Set[T]`.
    Set(Box<Ty>),
    /// Homogeneous dict `Dict[K, V]`.
    Dict(Box<Ty>, Box<Ty>),
    /// Closed structural record.
    Record(Record),
    /// Function type with positional + named params and a return.
    Fn(FnTy),
    /// User-declared ADT (from `class_def`).
    Adt(AdtId, Vec<Ty>),
    /// Transparent type-alias application.
    Alias(AliasId, Vec<Ty>),
    /// Universally quantified type-parameter use.
    Generic(GenericVar),
    /// Inference unknown.
    Var(VarId),
}

/// Closed structural record (M2: closed; row variables deferred to
/// a future ADR).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    /// Sorted by name for canonical equality.
    pub fields: BTreeMap<String, Ty>,
}

impl Record {
    #[must_use]
    pub fn from_pairs(pairs: Vec<(String, Ty)>) -> Self {
        let mut fields = BTreeMap::new();
        for (k, v) in pairs {
            fields.insert(k, v);
        }
        Self { fields }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnTy {
    pub positional: Vec<Ty>,
    pub named: Vec<(String, Ty)>,
    pub var_positional: Option<Box<Ty>>,
    pub var_keyword: Option<Box<Ty>>,
    pub return_ty: Box<Ty>,
}

impl FnTy {
    #[must_use]
    pub fn arity(&self) -> usize {
        self.positional.len() + self.named.len()
    }
}

/// Allocate fresh `TypeVar`s. Process-global. Inference is
/// single-threaded at M2 so a relaxed atomic counter is sufficient
/// and avoids threading the allocator through every typing rule.
#[derive(Debug)]
pub struct VarAllocator {
    next: AtomicU32,
}

impl Default for VarAllocator {
    fn default() -> Self {
        Self {
            next: AtomicU32::new(0),
        }
    }
}

impl VarAllocator {
    pub fn fresh(&self) -> VarId {
        VarId(self.next.fetch_add(1, Ordering::Relaxed))
    }
}

// =====================================================================
// Display
// =====================================================================

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Bool => f.write_str("bool"),
            Ty::Int => f.write_str("i64"),
            Ty::Float => f.write_str("f64"),
            Ty::Imag => f.write_str("imag"),
            Ty::Str => f.write_str("str"),
            Ty::Bytes => f.write_str("bytes"),
            Ty::None => f.write_str("None"),
            Ty::Never => f.write_str("Never"),
            Ty::Tuple(items) => {
                f.write_str("(")?;
                for (i, t) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{t}")?;
                }
                if items.len() == 1 {
                    f.write_str(",")?;
                }
                f.write_str(")")
            }
            Ty::List(t) => write!(f, "List[{t}]"),
            Ty::Set(t) => write!(f, "Set[{t}]"),
            Ty::Dict(k, v) => write!(f, "Dict[{k}, {v}]"),
            Ty::Record(r) => {
                f.write_str("{")?;
                for (i, (k, v)) in r.fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                f.write_str("}")
            }
            Ty::Fn(fn_ty) => {
                f.write_str("(")?;
                for (i, t) in fn_ty.positional.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{t}")?;
                }
                for (i, (n, t)) in fn_ty.named.iter().enumerate() {
                    if i > 0 || !fn_ty.positional.is_empty() {
                        f.write_str(", ")?;
                    }
                    write!(f, "{n}: {t}")?;
                }
                f.write_str(") -> ")?;
                write!(f, "{}", fn_ty.return_ty)
            }
            Ty::Adt(id, args) => {
                write!(f, "Adt#{}", id.0)?;
                if !args.is_empty() {
                    f.write_str("[")?;
                    for (i, t) in args.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{t}")?;
                    }
                    f.write_str("]")?;
                }
                Ok(())
            }
            Ty::Alias(id, args) => {
                write!(f, "Alias#{}", id.0)?;
                if !args.is_empty() {
                    f.write_str("[")?;
                    for (i, t) in args.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{t}")?;
                    }
                    f.write_str("]")?;
                }
                Ok(())
            }
            Ty::Generic(g) => write!(f, "T{}", g.0),
            Ty::Var(v) => write!(f, "?{}", v.0),
        }
    }
}

// =====================================================================
// Helpers
// =====================================================================

impl Ty {
    /// True if the type is one of the M2 "mutable container" types
    /// for the mutable-default-argument rule (ADR-0006 §"Mutable
    /// default arguments").
    #[must_use]
    pub fn is_mutable_container(&self) -> bool {
        matches!(self, Ty::List(_) | Ty::Set(_) | Ty::Dict(_, _))
    }

    /// ADR-0050d Decision 7A + §"Type-checker amendments" item 2 —
    /// Hashable predicate for dict-key admissibility.
    ///
    /// **Hashable** (Phase F.3): `bool`, `i64`, `str`, `bytes`,
    /// `None`, `Never`, and `Tuple(items)` if every item is hashable.
    ///
    /// **Not hashable** (Phase F.3): `f64` (NaN != NaN breaks Hash
    /// invariants per IEEE 754 — constitution §2.2 "no silent
    /// coercion"); `Imag` (same numerical concerns); `List` / `Set`
    /// / `Dict` / `Record` (mutable + structural); `Fn` (no canonical
    /// hash for closures); `Adt` / `Alias` / `Generic` (Phase G adds
    /// trait-based hashability); `Var` (under-determined — the type
    /// checker resolves the var before consulting this predicate, so
    /// callers should `subst.apply` first).
    ///
    /// Used at `synth_dict_lit` (after key/value unification) and at
    /// every `Dict[K, V]` annotation site (`lower_generic_type`) to
    /// emit `TypeError::NotHashable` when K is rejected.
    #[must_use]
    pub fn is_hashable(&self) -> bool {
        match self {
            Ty::Bool | Ty::Int | Ty::Str | Ty::Bytes | Ty::None | Ty::Never => true,
            Ty::Tuple(items) => items.iter().all(Ty::is_hashable),
            Ty::Float
            | Ty::Imag
            | Ty::List(_)
            | Ty::Set(_)
            | Ty::Dict(_, _)
            | Ty::Record(_)
            | Ty::Fn(_)
            | Ty::Adt(_, _)
            | Ty::Alias(_, _)
            | Ty::Generic(_)
            | Ty::Var(_) => false,
        }
    }

    /// Substitute a [`VarId`] throughout a type.
    pub fn subst_var(&self, v: VarId, replacement: &Ty) -> Ty {
        match self {
            Ty::Var(id) if *id == v => replacement.clone(),
            Ty::Tuple(items) => {
                Ty::Tuple(items.iter().map(|t| t.subst_var(v, replacement)).collect())
            }
            Ty::List(t) => Ty::List(Box::new(t.subst_var(v, replacement))),
            Ty::Set(t) => Ty::Set(Box::new(t.subst_var(v, replacement))),
            Ty::Dict(k, val) => Ty::Dict(
                Box::new(k.subst_var(v, replacement)),
                Box::new(val.subst_var(v, replacement)),
            ),
            Ty::Record(r) => Ty::Record(Record {
                fields: r
                    .fields
                    .iter()
                    .map(|(k, t)| (k.clone(), t.subst_var(v, replacement)))
                    .collect(),
            }),
            Ty::Fn(fn_ty) => Ty::Fn(FnTy {
                positional: fn_ty
                    .positional
                    .iter()
                    .map(|t| t.subst_var(v, replacement))
                    .collect(),
                named: fn_ty
                    .named
                    .iter()
                    .map(|(n, t)| (n.clone(), t.subst_var(v, replacement)))
                    .collect(),
                var_positional: fn_ty
                    .var_positional
                    .as_ref()
                    .map(|t| Box::new(t.subst_var(v, replacement))),
                var_keyword: fn_ty
                    .var_keyword
                    .as_ref()
                    .map(|t| Box::new(t.subst_var(v, replacement))),
                return_ty: Box::new(fn_ty.return_ty.subst_var(v, replacement)),
            }),
            Ty::Adt(id, args) => Ty::Adt(
                *id,
                args.iter().map(|t| t.subst_var(v, replacement)).collect(),
            ),
            Ty::Alias(id, args) => Ty::Alias(
                *id,
                args.iter().map(|t| t.subst_var(v, replacement)).collect(),
            ),
            other => other.clone(),
        }
    }

    /// Free type variables.
    pub fn free_vars(&self) -> Vec<VarId> {
        let mut out = Vec::new();
        self.collect_vars(&mut out);
        out
    }

    fn collect_vars(&self, out: &mut Vec<VarId>) {
        match self {
            Ty::Var(v) => {
                if !out.contains(v) {
                    out.push(*v);
                }
            }
            Ty::Tuple(items) => {
                for t in items {
                    t.collect_vars(out);
                }
            }
            Ty::List(t) | Ty::Set(t) => t.collect_vars(out),
            Ty::Dict(k, v) => {
                k.collect_vars(out);
                v.collect_vars(out);
            }
            Ty::Record(r) => {
                for t in r.fields.values() {
                    t.collect_vars(out);
                }
            }
            Ty::Fn(fn_ty) => {
                for t in &fn_ty.positional {
                    t.collect_vars(out);
                }
                for (_, t) in &fn_ty.named {
                    t.collect_vars(out);
                }
                if let Some(t) = &fn_ty.var_positional {
                    t.collect_vars(out);
                }
                if let Some(t) = &fn_ty.var_keyword {
                    t.collect_vars(out);
                }
                fn_ty.return_ty.collect_vars(out);
            }
            Ty::Adt(_, args) | Ty::Alias(_, args) => {
                for t in args {
                    t.collect_vars(out);
                }
            }
            _ => {}
        }
    }
}
