//! `packages/compiler/src/typecheck/ops/variables.ts`

use oxc_angular_compiler::ast::r3::{R3Template, R3Variable};

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::{TcbExpr, declare_variable};
use crate::typecheck::ops::references::LocalSymbol;

// variables.ts:21 TcbBlockImplicitVariableOp
/// A `TcbOp` which renders a variable that is implicitly available within a block (e.g. `$count`
/// in a `@for` block).
///
/// Executing this operation returns the identifier which can be used to refer to the variable.
pub struct TcbBlockImplicitVariableOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    r#type: TcbExpr,
    variable: &'a R3Variable<'a>,
}

impl<'a, 'b> TcbBlockImplicitVariableOp<'a, 'b> {
    // variables.ts:22 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, r#type: TcbExpr, variable: &'a R3Variable<'a>) -> Self {
        Self { tcb, scope, r#type, variable }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbBlockImplicitVariableOp<'a, 'b> {
    // variables.ts:31 optional
    fn optional(&self) -> bool {
        true
    }

    // variables.ts:33 execute
    fn execute(&self) -> Option<TcbExpr> {
        let id = TcbExpr::new(self.tcb.allocate_id());
        let id = id.add_parse_span_info(self.variable.key_span);
        let variable = declare_variable(&id, self.r#type.clone());
        let variable = variable.add_parse_span_info(self.variable.source_span);
        self.scope.add_statement(variable);
        Some(id)
    }
}

// variables.ts:49 TcbTemplateVariableOp
/// A `TcbOp` which creates an expression for particular let- `Variable` on a
/// `Template`'s context.
///
/// Executing this operation returns a reference to the variable variable (lol).
pub struct TcbTemplateVariableOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    template: &'a R3Template<'a>,
    variable: &'a R3Variable<'a>,
}

impl<'a, 'b> TcbTemplateVariableOp<'a, 'b> {
    // variables.ts:50 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        template: &'a R3Template<'a>,
        variable: &'a R3Variable<'a>,
    ) -> Self {
        Self { tcb, scope, template, variable }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbTemplateVariableOp<'a, 'b> {
    // variables.ts:59 optional
    fn optional(&self) -> bool {
        false
    }

    // variables.ts:63 execute
    fn execute(&self) -> Option<TcbExpr> {
        // Look for a context variable for the template.
        let ctx = self.scope.resolve(LocalSymbol::Template(self.template), None);

        // Allocate an identifier for the Variable, and initialize it to a read of the variable
        // on the template context.
        let id = TcbExpr::new(self.tcb.allocate_id());
        let value = if self.variable.value.is_empty() { "$implicit" } else { self.variable.value.as_str() };
        let mut initializer = TcbExpr::new(format!("{}.{value}", ctx.p()));
        let id = id.add_parse_span_info(self.variable.key_span);

        // Declare the variable, and return its identifier.
        if let Some(value_span) = self.variable.value_span {
            initializer = initializer.add_parse_span_info(value_span).wrap_for_type_checker();
        }
        let variable = TcbExpr::new(format!("var {} = {}", id.p(), initializer.p()));
        let variable = variable.add_parse_span_info(self.variable.source_span);
        self.scope.add_statement(variable);
        Some(id)
    }
}

// variables.ts:90 TcbBlockVariableOp
/// A `TcbOp` which renders a variable defined inside of block syntax (e.g. `@if (expr; as var) {}`).
///
/// Executing this operation returns the identifier which can be used to refer to the variable.
pub struct TcbBlockVariableOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    initializer: TcbExpr,
    variable: &'a R3Variable<'a>,
}

impl<'a, 'b> TcbBlockVariableOp<'a, 'b> {
    // variables.ts:91 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        initializer: TcbExpr,
        variable: &'a R3Variable<'a>,
    ) -> Self {
        Self { tcb, scope, initializer, variable }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbBlockVariableOp<'a, 'b> {
    // variables.ts:100 optional
    fn optional(&self) -> bool {
        false
    }

    // variables.ts:104 execute
    fn execute(&self) -> Option<TcbExpr> {
        let id = TcbExpr::new(self.tcb.allocate_id());
        let id = id.add_parse_span_info(self.variable.key_span);
        // (TypeScript wraps `this.initializer` in place; an op executes once.)
        let initializer = self.initializer.clone().wrap_for_type_checker();
        let variable = TcbExpr::new(format!("var {} = {}", id.p(), initializer.p()));
        let variable = variable.add_parse_span_info(self.variable.source_span);
        self.scope.add_statement(variable);
        Some(id)
    }
}
