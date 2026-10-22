//! `packages/compiler/src/typecheck/ops/completions.ts`

use crate::typecheck::comments::ExpressionIdentifier;
use crate::typecheck::ops::base::{ScopeRef, TcbOp};
use crate::typecheck::ops::codegen::TcbExpr;

// completions.ts:21 TcbComponentContextCompletionOp
/// A `TcbOp` which generates a completion point for the component context.
///
/// This completion point looks like `this. ;` in the TCB output, and does not produce diagnostics.
/// TypeScript autocompletion APIs can be used at this completion point (after the '.') to produce
/// autocompletion results of properties and methods from the template's component context.
pub struct TcbComponentContextCompletionOp<'a, 'b> {
    scope: ScopeRef<'a, 'b>,
}

impl<'a, 'b> TcbComponentContextCompletionOp<'a, 'b> {
    // completions.ts:22 constructor
    pub fn new(scope: ScopeRef<'a, 'b>) -> Self {
        Self { scope }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbComponentContextCompletionOp<'a, 'b> {
    // completions.ts:26 optional
    fn optional(&self) -> bool {
        false
    }

    // completions.ts:28 execute
    fn execute(&self) -> Option<TcbExpr> {
        let ctx = TcbExpr::new("this.");
        let ctx = ctx.mark_ignore_diagnostics();
        let ctx = ctx.add_expression_identifier(ExpressionIdentifier::ComponentCompletion, None);
        self.scope.add_statement(ctx);
        None
    }
}
