//! `packages/compiler/src/typecheck/ops/scope.ts`: not ported yet (PORTING.md).
//!
//! STUB (tcb-ops): only the signatures the ops of tcb-ops call, so that they compile before the merge; the real
//! `scope.rs` (tcb-core) replaces this file.

use oxc_angular_compiler::ast::r3::R3Node;

use crate::angular::t2_api::ScopedNode;
use crate::typecheck::api::DirectiveRef;
use crate::typecheck::ops::base::ScopeRef;
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::references::LocalSymbol;

/// scope.ts:89 Scope (being ported)
pub struct Scope<'a, 'b> {
    _marker: std::marker::PhantomData<(&'a (), &'b ())>,
}

impl<'a, 'b> Scope<'a, 'b> {
    // scope.ts:315 resolve (STUB)
    pub fn resolve(&self, _node: LocalSymbol<'a>, _directive: Option<&DirectiveRef>) -> TcbExpr {
        todo!()
    }

    // scope.ts:331 addStatement (STUB)
    pub fn add_statement(&self, _stmt: TcbExpr) {
        todo!()
    }

    // scope.ts:338 render (STUB)
    pub fn render(&self) -> Vec<TcbExpr> {
        todo!()
    }

    // scope.ts:352 guards (STUB)
    pub fn guards(&self) -> Option<TcbExpr> {
        todo!()
    }

    // scope.ts:397 createChildScope (STUB)
    pub fn create_child_scope(
        &self,
        _parent_scope: &ScopeRef<'a, 'b>,
        _scoped_node: Option<ScopedNode<'a>>,
        _children: Option<&'a [R3Node<'a>]>,
        _guard: Option<TcbExpr>,
    ) -> ScopeRef<'a, 'b> {
        todo!()
    }
}
