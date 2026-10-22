//! `packages/compiler/src/typecheck/ops/intersection_observer.ts`

use oxc_angular_compiler::ast::expression::AngularExpression;

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::expression::tcb_expression;

// intersection_observer.ts:19 TcbIntersectionObserverOp
/// A `TcbOp` which can be used to type check the options of an `IntersectionObserver`.
pub struct TcbIntersectionObserverOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    options: &'a AngularExpression<'a>,
}

impl<'a, 'b> TcbIntersectionObserverOp<'a, 'b> {
    // intersection_observer.ts:20 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, options: &'a AngularExpression<'a>) -> Self {
        Self { tcb, scope, options }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbIntersectionObserverOp<'a, 'b> {
    // intersection_observer.ts:28 optional
    fn optional(&self) -> bool {
        false
    }

    // intersection_observer.ts:30 execute
    fn execute(&self) -> Option<TcbExpr> {
        let options = tcb_expression(self.options, &self.tcb, &self.scope);
        self.scope.add_statement(TcbExpr::new(format!("new IntersectionObserver(null!, {})", options.p())));
        None
    }
}
