//! `packages/compiler/src/typecheck/ops/let.ts`

use oxc_angular_compiler::ast::r3::R3LetDeclaration;

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::expression::tcb_expression;

// let.ts:19 TcbLetDeclarationOp
/// A `TcbOp` which generates a constant for a `LetDeclaration`.
pub struct TcbLetDeclarationOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    node: &'a R3LetDeclaration<'a>,
}

impl<'a, 'b> TcbLetDeclarationOp<'a, 'b> {
    // let.ts:20 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, node: &'a R3LetDeclaration<'a>) -> Self {
        Self { tcb, scope, node }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbLetDeclarationOp<'a, 'b> {
    // let.ts:32 optional
    /// `@let` declarations are mandatory, because their expressions
    /// should be checked even if they aren't referenced anywhere.
    fn optional(&self) -> bool {
        false
    }

    // let.ts:34 execute
    fn execute(&self) -> Option<TcbExpr> {
        let id = TcbExpr::new(self.tcb.allocate_id()).add_parse_span_info(self.node.name_span);
        let value = tcb_expression(&self.node.value, &self.tcb, &self.scope).wrap_for_type_checker();
        // Value needs to be wrapped, because spans for the expressions inside of it can
        // be picked up incorrectly as belonging to the full variable declaration.
        let var_statement = TcbExpr::new(format!("const {} = {}", id.p(), value.p()));
        let var_statement = var_statement.add_parse_span_info(self.node.source_span);
        self.scope.add_statement(var_statement);
        Some(id)
    }
}
