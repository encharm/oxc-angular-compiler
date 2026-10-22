//! `packages/compiler/src/typecheck/ops/host.ts`

use oxc_angular_compiler::ast::r3::R3HostElement;

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;

// host.ts:20 TcbHostElementOp
/// A `TcbOp` which creates an expression for a the host element of a directive.
///
/// Executing this operation returns a reference to the element variable.
pub struct TcbHostElementOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    element: &'a R3HostElement<'a>,
}

impl<'a, 'b> TcbHostElementOp<'a, 'b> {
    // host.ts:23 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, element: &'a R3HostElement<'a>) -> Self {
        Self { tcb, scope, element }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbHostElementOp<'a, 'b> {
    // host.ts:21 optional
    fn optional(&self) -> bool {
        true
    }

    // host.ts:31 execute
    fn execute(&self) -> Option<TcbExpr> {
        let id = self.tcb.allocate_id();

        let tag_names = if self.element.tag_names.len() == 1 {
            format!("\"{}\"", self.element.tag_names[0])
        } else {
            format!(
                "null! as {}",
                self.element.tag_names.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(" | ")
            )
        };

        let initializer = TcbExpr::new(format!("document.createElement({tag_names})"))
            .add_parse_span_info(self.element.source_span);
        self.scope.add_statement(TcbExpr::new(format!("var {id} = {}", initializer.p())));
        Some(TcbExpr::new(id))
    }
}
