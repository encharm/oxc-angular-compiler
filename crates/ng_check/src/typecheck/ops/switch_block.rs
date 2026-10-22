//! `packages/compiler/src/typecheck/ops/switch_block.ts`

use oxc_angular_compiler::ast::r3::{R3SwitchBlock, R3SwitchBlockCaseGroup};

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::{TcbExpr, get_statements_block};
use crate::typecheck::ops::expression::tcb_expression;

// switch_block.ts:21 TcbSwitchOp: a `TcbOp` which renders a `switch` block as a TypeScript `switch` statement.
//
// Executing this operation returns nothing.
pub struct TcbSwitchOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    block: &'a R3SwitchBlock<'a>,
}

impl<'a, 'b> TcbSwitchOp<'a, 'b> {
    // switch_block.ts:22 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, block: &'a R3SwitchBlock<'a>) -> Self {
        Self { tcb, scope, block }
    }

    // switch_block.ts:85 generateGuard
    fn generate_guard(
        &self,
        group: &'a R3SwitchBlockCaseGroup<'a>,
        switch_value: &TcbExpr,
    ) -> Option<TcbExpr> {
        // For non-default cases, the guard needs to compare against the case value, e.g.
        // `switchExpression === caseExpression`.
        let has_default = group.cases.iter().any(|c| c.expression.is_none());

        if !has_default {
            let mut guard: Option<TcbExpr> = None;

            for switch_case in &group.cases {
                if let Some(case_expression) = &switch_case.expression {
                    // The expression needs to be ignored for diagnostics since it has been checked already.
                    let expression = tcb_expression(case_expression, &self.tcb, &self.scope)
                        .mark_ignore_diagnostics();
                    let comparison =
                        TcbExpr::new(format!("{} === {}", switch_value.p(), expression.p()));

                    guard = Some(match guard {
                        None => comparison,
                        Some(guard) => {
                            TcbExpr::new(format!("({}) || ({})", guard.p(), comparison.p()))
                        }
                    });
                }
            }

            return guard;
        }

        // To fully narrow the type in the default case, we need to generate an expression that negates
        // the values of all of the other expressions. For example:
        // @switch (expr) {
        //   @case (1) {}
        //   @case (2) {}
        //   @default {}
        // }
        // Will produce the guard `expr !== 1 && expr !== 2`.
        let mut guard: Option<TcbExpr> = None;

        for current_group in &self.block.groups {
            if std::ptr::eq(current_group, group) {
                continue;
            }

            for switch_case in &current_group.cases {
                let Some(case_expression) = &switch_case.expression else {
                    // Skip the default case.
                    continue;
                };

                // The expression needs to be ignored for diagnostics since it has been checked already.
                let expression = tcb_expression(case_expression, &self.tcb, &self.scope)
                    .mark_ignore_diagnostics();
                let comparison =
                    TcbExpr::new(format!("{} !== {}", switch_value.p(), expression.p()));

                guard = Some(match guard {
                    None => comparison,
                    Some(guard) => TcbExpr::new(format!("({}) && ({})", guard.p(), comparison.p())),
                });
            }
        }

        guard
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbSwitchOp<'a, 'b> {
    // switch_block.ts:30 optional
    fn optional(&self) -> bool {
        false
    }

    // switch_block.ts:34 execute
    fn execute(&self) -> Option<TcbExpr> {
        let switch_expression = tcb_expression(&self.block.expression, &self.tcb, &self.scope);
        let mut clauses: Vec<TcbExpr> = Vec::new();
        for current in &self.block.groups {
            let check_body = self.tcb.env.config().check_control_flow_bodies;
            let children = if check_body { &current.children[..] } else { &[] };
            let guard =
                if check_body { self.generate_guard(current, &switch_expression) } else { None };
            let clause_scope =
                self.scope.create_child_scope(&self.scope, None, Some(children), guard);

            let mut statements = clause_scope.render();
            statements.push(TcbExpr::new("break"));

            for (index, switch_case) in current.cases.iter().enumerate() {
                let statements_str = get_statements_block(
                    if index == current.cases.len() - 1 { &statements } else { &[] },
                    true, /* singleLine */
                );

                let source = match &switch_case.expression {
                    None => format!("default: {statements_str}"),
                    Some(case_expression) => format!(
                        "case {}: {statements_str}",
                        tcb_expression(case_expression, &self.tcb, &self.scope).p()
                    ),
                };

                clauses.push(TcbExpr::new(source));
            }
        }

        if let Some(exhaustive_check) = &self.block.exhaustive_check {
            let mut translate_expression = &self.block.expression;
            if let Some(expression) = &exhaustive_check.expression {
                translate_expression = expression;
            }

            let switch_value = tcb_expression(translate_expression, &self.tcb, &self.scope);
            let exhaustive_id = self.tcb.allocate_id();

            clauses.push(TcbExpr::new(format!(
                "default: const tcbExhaustive{exhaustive_id}: never = {};",
                switch_value.p()
            )));
        }

        self.scope.add_statement(TcbExpr::new(format!(
            "switch ({}) {{ {} }}",
            switch_expression.p(),
            clauses.iter().map(TcbExpr::p).collect::<Vec<_>>().join("\n")
        )));

        None
    }
}
