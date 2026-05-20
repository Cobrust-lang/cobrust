//! `JitHandle` — owns the live `JITModule` and exposes
//! `call::<R, A>`.
//!
//! ADR-0056a §4 sets out the lifetime contract:
//!
//! > Owned by the REPL Session, not per-eval. ... `JITModule` is
//! > the sole reclaim — drop frees RWX `memmap2` pages.
//!
//! For wave-1 the handle IS the JIT module; the engine has
//! consumed itself and is gone. ADR-0056c will re-engineer this
//! into a `Session::JitEngine + JitHandle::Refresh` pattern with
//! cross-turn fn persistence.
//!
//! ## Safety
//!
//! The single unsafe surface in this crate. `JitHandle::call`
//! transmutes a `*const u8` to an `extern "C" fn(A) -> R`. Caller
//! contract:
//!
//! - `R` MUST be one of the 4-arm primitives at ADR-0056a §4
//!   (`i64` / `f64` / `*const u8` / `()`).
//! - `A` MUST be a tuple of primitives matching the Cranelift
//!   signature recorded at compile time. The `ArgsList` trait
//!   below pins the validation surface.
//!
//! Mismatch → SIGSEGV (parent §5 risk 1). The pre-transmute
//! signature assertion lives in `validate_signature` below;
//! it converts type-mismatch into a typed [`JitError`].

use std::collections::HashMap;
use std::marker::PhantomData;

use cranelift_codegen::ir::{self, Signature};
use cranelift_jit::JITModule;

use crate::error::JitError;

/// A finalized JIT compilation result.
///
/// Owns the `JITModule` (RWX JIT pages); drop releases the
/// pages and invalidates all collected fn pointers.
pub struct JitHandle {
    // SAFETY: the module's lifetime gates every fn pointer in
    // `fn_table`. `JITModule::drop` frees the JIT pages, after
    // which every pointer in `fn_table` is dangling. The
    // PhantomData below ties any `&JitHandle` borrow to the
    // module's lifetime so callers cannot smuggle a fn pointer
    // out past `JitHandle::drop`.
    module: JITModule,
    fn_table: HashMap<String, (*const u8, Signature)>,
    _marker: PhantomData<*mut JITModule>,
}

// SAFETY: `JITModule` itself is `Send` (cranelift-jit guarantees);
// the fn pointers are raw and never crossed across threads by
// this handle (the API takes `&self`, callers stay on one
// thread). We don't auto-derive `Send` because of the raw pointer.
unsafe impl Send for JitHandle {}

impl JitHandle {
    pub(crate) fn new(
        module: JITModule,
        fn_table: HashMap<String, (*const u8, Signature)>,
    ) -> Self {
        Self {
            module,
            fn_table,
            _marker: PhantomData,
        }
    }

    /// Borrow the inner `JITModule`. ADR-0056c will need this for
    /// cross-turn FuncId redefinition; wave-1 exposes it for
    /// debugging / test introspection only.
    pub fn module(&self) -> &JITModule {
        &self.module
    }

    /// Names of every JIT-compiled function in the handle.
    pub fn function_names(&self) -> Vec<&str> {
        self.fn_table.keys().map(String::as_str).collect()
    }

    /// Look up a compiled function's recorded Cranelift signature.
    /// Used by the REPL Session pre-transmute validation step.
    pub fn signature(&self, name: &str) -> Result<&Signature, JitError> {
        self.fn_table
            .get(name)
            .map(|(_, s)| s)
            .ok_or_else(|| JitError::NoSuchFunction {
                name: name.to_string(),
            })
    }

    /// Invoke a compiled function.
    ///
    /// ## Safety
    ///
    /// The caller MUST guarantee:
    ///
    /// 1. The compiled function exists (`function_names()` lists
    ///    `name`).
    /// 2. The compiled signature matches `A` → `R` as `extern "C"`.
    ///    The crate validates the structural shape via the
    ///    `ArgsList` trait + `R::CRANELIFT_TYPE` constants, but
    ///    cannot validate that the user-supplied `R` is the
    ///    EXACT same scalar as the body's return value (e.g.
    ///    swapping `i64` for `u64` will look identical on the
    ///    Cranelift side but UB on integer-overflow signedness).
    ///
    /// On signature mismatch the call returns
    /// [`JitError::SignatureMismatch`] BEFORE transmute.
    ///
    /// # Errors
    ///
    /// Returns [`JitError::NoSuchFunction`] if `name` is unknown,
    /// or [`JitError::SignatureMismatch`] if the
    /// `(A, R)` shape disagrees with the compiled `Signature`.
    pub unsafe fn call<R, A>(&self, name: &str, args: A) -> Result<R, JitError>
    where
        R: JitReturn,
        A: ArgsList,
    {
        let (ptr, sig) = self
            .fn_table
            .get(name)
            .ok_or_else(|| JitError::NoSuchFunction {
                name: name.to_string(),
            })?;

        validate_signature::<R, A>(sig)?;

        // SAFETY: the caller's `unsafe` contract above covers the
        // transmute. We've validated the structural shape; the only
        // unvalidatable axis is signedness within the same width
        // (caller responsibility).
        let result = unsafe { A::invoke::<R>(*ptr, args) };
        Ok(result)
    }
}

impl std::fmt::Debug for JitHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JitHandle")
            .field("functions", &self.fn_table.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// Cranelift-IR-type witness for a primitive scalar.
///
/// The 4-arm extern table at ADR-0056a §4: `i64` / `f64` /
/// `*const u8` / `()`. Wave-1 ships `i64` only (and `()` for
/// nullary unit-return tests when 0056b lights them up).
pub trait JitReturn: Sized {
    /// The Cranelift IR type this Rust type maps to. The
    /// `validate_signature` helper compares this to the
    /// compiled signature's `returns[0]`.
    const CRANELIFT_TYPE: ir::Type;
}

impl JitReturn for i64 {
    const CRANELIFT_TYPE: ir::Type = ir::types::I64;
}

impl JitReturn for () {
    const CRANELIFT_TYPE: ir::Type = ir::types::INVALID;
}

/// Tuple-of-primitives args list.
///
/// `(A1, ..., An)` invokes a Cranelift function whose
/// `Signature.params` exactly matches `[A1, ..., An]`. Wave-1
/// supports tuples of arity 0, 1, and 2 (test surface);
/// 0056b grows arities as needed.
///
/// ## Safety
///
/// `invoke` transmutes a raw `*const u8` to an `extern "C" fn`
/// whose signature matches `Self` → `R`. The caller is
/// responsible for the conditions in [`JitHandle::call`].
pub trait ArgsList {
    /// The expected Cranelift `Signature.params` types, in order.
    fn expected_param_types() -> Vec<ir::Type>;
    /// Perform the transmute + call. Returns `R`.
    ///
    /// # Safety
    ///
    /// Caller guarantees `ptr` is the finalized fn ptr of a body
    /// whose `Signature.params == expected_param_types()` and
    /// whose `Signature.returns[0] == R::CRANELIFT_TYPE`.
    unsafe fn invoke<R: JitReturn>(ptr: *const u8, args: Self) -> R;
}

impl ArgsList for () {
    fn expected_param_types() -> Vec<ir::Type> {
        Vec::new()
    }
    unsafe fn invoke<R: JitReturn>(ptr: *const u8, _args: Self) -> R {
        // SAFETY: see `JitHandle::call`. The fn pointer is finalized;
        // signature has been validated; the transmute is well-formed.
        let f: extern "C" fn() -> R = unsafe { std::mem::transmute(ptr) };
        f()
    }
}

impl ArgsList for (i64,) {
    fn expected_param_types() -> Vec<ir::Type> {
        vec![ir::types::I64]
    }
    unsafe fn invoke<R: JitReturn>(ptr: *const u8, args: Self) -> R {
        let f: extern "C" fn(i64) -> R = unsafe { std::mem::transmute(ptr) };
        f(args.0)
    }
}

impl ArgsList for (i64, i64) {
    fn expected_param_types() -> Vec<ir::Type> {
        vec![ir::types::I64, ir::types::I64]
    }
    unsafe fn invoke<R: JitReturn>(ptr: *const u8, args: Self) -> R {
        let f: extern "C" fn(i64, i64) -> R = unsafe { std::mem::transmute(ptr) };
        f(args.0, args.1)
    }
}

impl ArgsList for (i64, i64, i64) {
    fn expected_param_types() -> Vec<ir::Type> {
        vec![ir::types::I64, ir::types::I64, ir::types::I64]
    }
    unsafe fn invoke<R: JitReturn>(ptr: *const u8, args: Self) -> R {
        let f: extern "C" fn(i64, i64, i64) -> R = unsafe { std::mem::transmute(ptr) };
        f(args.0, args.1, args.2)
    }
}

/// Pre-transmute signature validation. Converts a would-be
/// SIGSEGV on signature drift into a typed `JitError`.
fn validate_signature<R: JitReturn, A: ArgsList>(sig: &Signature) -> Result<(), JitError> {
    let expected_params = A::expected_param_types();
    let actual_params: Vec<ir::Type> = sig.params.iter().map(|p| p.value_type).collect();
    if expected_params != actual_params {
        return Err(JitError::SignatureMismatch {
            expected: format!("params={expected_params:?}"),
            actual: format!("params={actual_params:?}"),
        });
    }

    let expected_ret = R::CRANELIFT_TYPE;
    if expected_ret == ir::types::INVALID {
        // Unit return; compiled body must have no returns.
        if !sig.returns.is_empty() {
            return Err(JitError::SignatureMismatch {
                expected: "no returns (()-typed)".to_string(),
                actual: format!("returns={:?}", sig.returns),
            });
        }
    } else {
        let actual_ret = sig.returns.first().map(|p| p.value_type).ok_or_else(|| {
            JitError::SignatureMismatch {
                expected: format!("returns=[{expected_ret:?}]"),
                actual: "returns=[]".to_string(),
            }
        })?;
        if actual_ret != expected_ret {
            return Err(JitError::SignatureMismatch {
                expected: format!("returns=[{expected_ret:?}]"),
                actual: format!("returns=[{actual_ret:?}]"),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_list_param_types_match_shape() {
        assert_eq!(
            <() as ArgsList>::expected_param_types(),
            Vec::<ir::Type>::new()
        );
        assert_eq!(
            <(i64,) as ArgsList>::expected_param_types(),
            vec![ir::types::I64]
        );
        assert_eq!(
            <(i64, i64) as ArgsList>::expected_param_types(),
            vec![ir::types::I64, ir::types::I64]
        );
    }
}
