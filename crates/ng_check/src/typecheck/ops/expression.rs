//! `packages/compiler/src/typecheck/ops/expression.ts`: not ported yet (PORTING.md).
//!
//! STUB (tcb-ops): only the signatures the ops of tcb-ops call, so that they compile before the merge; the real
//! `expression.rs` (tcb-core) replaces this file.

use oxc_angular_compiler::ast::expression::{AngularExpression, PropertyRead};
use oxc_angular_compiler::ast::r3::R3LetDeclaration;

use crate::typecheck::ops::base::{ScopeRef, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::context::Context;

// expression.ts:35 tcbExpression (STUB)
pub fn tcb_expression<'a, 'b>(
    _ast: &'a AngularExpression<'a>,
    _tcb: &TcbRef<'a, 'b>,
    _scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    todo!()
}

// expression.ts:43 unwrapWritableSignal (STUB)
pub fn unwrap_writable_signal(_expression: TcbExpr, _tcb: &Context<'_, '_>) -> TcbExpr {
    todo!()
}

// expression.ts:104 TcbExpressionTranslator (STUB). A class with subclasses (`TcbEventHandlerTranslator` in
// events.rs, `TcbForLoopTrackTranslator` in for_block.rs): a trait whose provided methods are the base class's; a
// subclass implements the accessors and overrides `resolve` / `is_valid_let_declaration_access`, reaching the base
// class's `resolve` (`super.resolve`) through `base_resolve`.
pub trait TcbExpressionTranslator<'a, 'b> {
    /// `this.tcb`
    fn tcb(&self) -> &TcbRef<'a, 'b>;

    /// `this.scope`
    fn scope(&self) -> &ScopeRef<'a, 'b>;

    // expression.ts:110 translate (STUB)
    fn translate(&self, _ast: &'a AngularExpression<'a>) -> TcbExpr {
        todo!()
    }

    // expression.ts:123 resolve
    fn resolve(&self, ast: &'a AngularExpression<'a>) -> Option<TcbExpr> {
        self.base_resolve(ast)
    }

    /// expression.ts:123 resolve, the base class's (`super.resolve`) (STUB)
    fn base_resolve(&self, _ast: &'a AngularExpression<'a>) -> Option<TcbExpr> {
        todo!()
    }

    // expression.ts:311 isValidLetDeclarationAccess (STUB)
    fn is_valid_let_declaration_access(
        &self,
        _target: &'a R3LetDeclaration<'a>,
        _ast: &'a PropertyRead<'a>,
    ) -> bool {
        todo!()
    }
}
