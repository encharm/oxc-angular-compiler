//! `packages/compiler/src/typecheck/ops/selectorless.ts`

use oxc_angular_compiler::ast::r3::R3Component;

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;

// selectorless.ts:18 getComponentTagName
// TODO(crisbeto): the logic for determining the fallback tag name of a Component node is
// still being designed. For now fall back to `ng-component`, but this will have to be
// revisited once the design is finalized.
pub fn get_component_tag_name<'a>(node: &'a R3Component<'a>) -> &'a str {
    match &node.tag_name {
        Some(tag_name) if !tag_name.is_empty() => tag_name.as_str(),
        _ => "ng-component",
    }
}

// selectorless.ts:27 TcbComponentNodeOp
/// A `TcbOp` which creates an expression for a native DOM element from a `Component`.
///
/// Executing this operation returns a reference to the element variable.
pub struct TcbComponentNodeOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    component: &'a R3Component<'a>,
}

impl<'a, 'b> TcbComponentNodeOp<'a, 'b> {
    // selectorless.ts:30 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        component: &'a R3Component<'a>,
    ) -> Self {
        Self { tcb, scope, component }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbComponentNodeOp<'a, 'b> {
    // selectorless.ts:28 optional
    fn optional(&self) -> bool {
        true
    }

    // selectorless.ts:38 execute
    fn execute(&self) -> Option<TcbExpr> {
        let id = self.tcb.allocate_id();
        // (`this.component.startSourceSpan || this.component.sourceSpan`: the start span is always set.)
        let initializer = TcbExpr::new(format!(
            "document.createElement(\"{}\")",
            get_component_tag_name(self.component)
        ))
        .add_parse_span_info(self.component.start_source_span);
        self.scope.add_statement(TcbExpr::new(format!("var {id} = {}", initializer.p())));
        Some(TcbExpr::new(id))
    }
}
