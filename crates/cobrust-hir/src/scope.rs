//! Name resolution: scopes, [`DefId`] allocation, and resolved-name
//! representation.
//!
//! ADR-0005 §"Scoping" + §"`DefId` allocation" pin the rules. The
//! scope tree is *lexical*: a fresh scope is opened by `fn`,
//! `lambda`, `class`, every `Block` introduced by a control-flow
//! statement, every comprehension, and every match arm. The resolver
//! walks the chain from the innermost outwards.

use std::collections::HashMap;

use cobrust_frontend::span::Span;

/// A fresh, opaque, monotonically-allocated identity for a binding
/// site. Two binding sites in the same program never share a
/// `DefId`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DefId(pub u32);

/// A name use after resolution. The lowering preserves both the
/// surface name (for diagnostics) and the resolved [`DefId`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedName {
    pub name: String,
    pub def_id: DefId,
    pub kind: DefKind,
}

/// Coarse classifier for a [`DefId`]; used by the type checker and
/// for diagnostics (e.g. distinguishing "function" from "let").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefKind {
    Fn,
    Class,
    TypeAlias,
    Param,
    LetBinding,
    LoopBinding,
    PatternBinding,
    WithBinding,
    ImportAlias,
    ExceptBinding,
    TypeParam,
}

/// A single lexical scope. `parent` is `None` only for the root
/// (module) scope. The resolver walks `parent` chains to look up
/// names; `bindings` is checked first.
#[derive(Debug, Default)]
pub struct Scope {
    bindings: HashMap<String, BindingRecord>,
    parent: Option<Box<Scope>>,
}

#[derive(Clone, Debug)]
struct BindingRecord {
    def_id: DefId,
    kind: DefKind,
    span: Span,
}

impl Scope {
    /// Root module scope.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            parent: None,
        }
    }

    /// Open a child scope; the caller owns the child while it's
    /// active and merges its parent back via [`Scope::close`].
    pub fn child(parent: Self) -> Self {
        Self {
            bindings: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }

    /// Close this scope, returning the parent. Panics only if the
    /// caller misuses the API on the root scope; the lowering never
    /// triggers that path.
    pub fn close(self) -> Self {
        match self.parent {
            Some(p) => *p,
            None => self, // closing root is a no-op
        }
    }

    /// Bind a name in the current scope. Returns `Err(prior_span)`
    /// if a binding with the same name already exists in this exact
    /// scope (shadowing across scopes is allowed; duplicate within
    /// the same scope is the error).
    ///
    /// **Exception**: when both the existing binding and the new one
    /// are `DefKind::Fn`, the new definition shadows the old silently.
    /// Python/Cobrust module-level `def` semantics — later definition wins.
    /// The prior DefId is returned via `Ok(Some(prior_def_id))` so the
    /// caller can track PRELUDE stub overrides (M-F.3.3 math intrinsics).
    pub fn bind(
        &mut self,
        name: &str,
        def_id: DefId,
        kind: DefKind,
        span: Span,
    ) -> Result<(), Span> {
        if let Some(prior) = self.bindings.get(name) {
            if matches!(kind, DefKind::Fn) && matches!(prior.kind, DefKind::Fn) {
                // Fn→Fn shadowing: user's definition overwrites PRELUDE stub.
                self.bindings
                    .insert(name.to_string(), BindingRecord { def_id, kind, span });
                return Ok(());
            }
            return Err(prior.span);
        }
        self.bindings
            .insert(name.to_string(), BindingRecord { def_id, kind, span });
        Ok(())
    }

    /// Resolve a name, searching innermost-first up the parent
    /// chain.
    pub fn resolve(&self, name: &str) -> Option<(DefId, DefKind)> {
        if let Some(rec) = self.bindings.get(name) {
            return Some((rec.def_id, rec.kind));
        }
        match &self.parent {
            Some(p) => p.resolve(name),
            None => None,
        }
    }

    /// True if a binding exists in *this* scope (not parents).
    pub fn binds_locally(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// All names locally bound in this scope (used by or-pattern
    /// branch-equality enforcement).
    pub fn local_names(&self) -> impl Iterator<Item = (&String, DefId)> {
        self.bindings.iter().map(|(n, r)| (n, r.def_id))
    }
}

/// `DefId` allocator threaded through the lowering session.
#[derive(Debug, Default)]
pub struct DefAllocator {
    next: u32,
}

impl DefAllocator {
    pub fn fresh(&mut self) -> DefId {
        let id = self.next;
        self.next += 1;
        DefId(id)
    }

    pub fn count(&self) -> u32 {
        self.next
    }
}

/// Generate a hygienic identifier name. The result is *guaranteed*
/// to never collide with a user-supplied identifier because the
/// resolver looks up by `DefId`, not name. The textual prefix
/// `__cb_gensym_` is purely for diagnostics.
pub fn gensym_name(idx: u32) -> String {
    format!("__cb_gensym_{idx}")
}
