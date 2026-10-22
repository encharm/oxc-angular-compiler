//! `packages/compiler/src/typecheck/ops/if_block.ts`

use std::rc::Rc;

use std::cell::RefCell;

use oxc_angular_compiler::ast::r3::{R3IfBlock, R3IfBlockBranch};

use crate::angular::t2_api::{NodeKey, ScopedNode, node_key};
use crate::typecheck::api::Map;
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::{TcbExpr, get_statements_block};
use crate::typecheck::ops::expression::tcb_expression;
use crate::typecheck::ops::references::LocalSymbol;

// if_block.ts:21 TcbIfBlockOp: a `TcbOp` which renders an `if` template block as a TypeScript `if` statement.
//
// Executing this operation returns nothing.
pub struct TcbIfBlockOp<'a, 'b> {
    /// By the branch's `node_key`
    expression_scopes: RefCell<Map<NodeKey, ScopeRef<'a, 'b>>>,
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    block: &'a R3IfBlock<'a>,
}

impl<'a, 'b> TcbIfBlockOp<'a, 'b> {
    // if_block.ts:24 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, block: &'a R3IfBlock<'a>) -> Self {
        Self { expression_scopes: RefCell::new(Map::default()), tcb, scope, block }
    }

    // if_block.ts:42 generateBranch
    fn generate_branch(&self, index: usize) -> Option<TcbExpr> {
        let branch = self.block.branches.get(index)?;

        // If the expression is null, it means that it's an `else` statement.
        let Some(branch_expression) = &branch.expression else {
            let branch_scope = self.get_branch_scope(&self.scope, branch, index);
            return Some(TcbExpr::new(format!(
                "{{\n{}}}",
                get_statements_block(&branch_scope.render(), false)
            )));
        };

        // We process the expression first in the parent scope, but create a scope around the block
        // that the body will inherit from. We do this, because we need to declare a separate variable
        // for the case where the expression has an alias _and_ because we need the processed
        // expression when generating the guard for the body.
        let outer_scope = self.scope.create_child_scope(
            &self.scope,
            Some(ScopedNode::IfBlockBranch(branch)),
            Some(&[]),
            None,
        );
        outer_scope.render().into_iter().for_each(|stmt| self.scope.add_statement(stmt));
        self.expression_scopes.borrow_mut().insert(node_key(branch), Rc::clone(&outer_scope));

        let mut expression = tcb_expression(branch_expression, &self.tcb, &self.scope);
        if let Some(expression_alias) = &branch.expression_alias {
            expression = TcbExpr::new(format!(
                "({}) && {}",
                expression.p(),
                outer_scope.resolve(LocalSymbol::Variable(expression_alias), None).p()
            ));
        }
        let body_scope = self.get_branch_scope(&outer_scope, branch, index);
        let if_statement = format!(
            "if ({}) {{\n{}}}",
            expression.p(),
            get_statements_block(&body_scope.render(), false)
        );
        let else_branch = self.generate_branch(index + 1);

        Some(TcbExpr::new(
            if_statement
                + &else_branch
                    .map(|else_branch| format!(" else {}", else_branch.p()))
                    .unwrap_or_default(),
        ))
    }

    // if_block.ts:76 getBranchScope
    fn get_branch_scope(
        &self,
        parent_scope: &ScopeRef<'a, 'b>,
        branch: &'a R3IfBlockBranch<'a>,
        index: usize,
    ) -> ScopeRef<'a, 'b> {
        let check_body = self.tcb.env.config().check_control_flow_bodies;
        let children = if check_body { &branch.children[..] } else { &[] };
        let guard = if check_body { self.generate_branch_guard(index) } else { None };
        self.scope.create_child_scope(parent_scope, None, Some(children), guard)
    }

    // if_block.ts:86 generateBranchGuard
    fn generate_branch_guard(&self, index: usize) -> Option<TcbExpr> {
        let mut guard: Option<TcbExpr> = None;

        // Since event listeners are inside callbacks, type narrowing doesn't apply to them anymore.
        // To recreate the behavior, we generate an expression that negates all the values of the
        // branches _before_ the current one, and then we add the current branch's expression on top.
        // For example `@if (expr === 1) {} @else if (expr === 2) {} @else if (expr === 3)`, the guard
        // for the last expression will be `!(expr === 1) && !(expr === 2) && expr === 3`.
        for i in 0..=index {
            let branch = &self.block.branches[i];

            // Skip over branches without an expression.
            let Some(branch_expression) = &branch.expression else {
                continue;
            };

            // This shouldn't happen since all the state is handled
            // internally, but we have the check just in case.
            let Some(expression_scope) =
                self.expression_scopes.borrow().get(&node_key(branch)).cloned()
            else {
                panic!("Could not determine expression scope of branch at index {i}");
            };

            // We need to recreate the expression and mark it to be ignored for diagnostics,
            // because it was already checked as a part of the block's condition and we don't
            // want it to produce a duplicate diagnostic.
            let mut expression = tcb_expression(branch_expression, &self.tcb, &expression_scope);
            if let Some(expression_alias) = &branch.expression_alias {
                expression = TcbExpr::new(format!(
                    "({}) && {}",
                    expression.p(),
                    expression_scope.resolve(LocalSymbol::Variable(expression_alias), None).p()
                ));
            }
            let expression = expression.mark_ignore_diagnostics();

            // The expressions of the preceding branches have to be negated
            // (e.g. `expr` becomes `!(expr)`) when comparing in the guard, except
            // for the branch's own expression which is preserved as is.
            let comparison_expression = if i == index {
                expression
            } else {
                TcbExpr::new(format!("!({})", expression.p()))
            };

            // Finally add the expression to the guard with an && operator.
            guard = Some(match guard {
                None => comparison_expression,
                Some(guard) => {
                    TcbExpr::new(format!("({}) && ({})", guard.p(), comparison_expression.p()))
                }
            });
        }

        guard
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbIfBlockOp<'a, 'b> {
    // if_block.ts:32 optional
    fn optional(&self) -> bool {
        false
    }

    // if_block.ts:36 execute
    fn execute(&self) -> Option<TcbExpr> {
        let root = self.generate_branch(0);
        if let Some(root) = root {
            self.scope.add_statement(root);
        }
        None
    }
}
