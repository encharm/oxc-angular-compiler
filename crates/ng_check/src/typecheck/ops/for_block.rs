//! `packages/compiler/src/typecheck/ops/for_block.ts`

use std::rc::Rc;

use oxc_angular_compiler::ast::expression::{ASTWithSource, AngularExpression};
use oxc_angular_compiler::ast::r3::R3ForLoopBlock;
use oxc_span::Span;

use crate::angular::t2_api::{NodeKey, ScopedNode, TemplateEntity, node_key};
use crate::typecheck::api::Set;
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::{TcbExpr, get_statements_block};
use crate::typecheck::ops::expression::{TcbExpressionTranslator, tcb_expression};
use crate::typecheck::ops::references::LocalSymbol;

// for_block.ts:22 TcbForOfOp: a `TcbOp` which renders a `for` block as a TypeScript `for...of` loop.
//
// Executing this operation returns nothing.
pub struct TcbForOfOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    block: &'a R3ForLoopBlock<'a>,
}

impl<'a, 'b> TcbForOfOp<'a, 'b> {
    // for_block.ts:23 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        block: &'a R3ForLoopBlock<'a>,
    ) -> Self {
        Self { tcb, scope, block }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbForOfOp<'a, 'b> {
    // for_block.ts:31 optional
    fn optional(&self) -> bool {
        false
    }

    // for_block.ts:35 execute
    fn execute(&self) -> Option<TcbExpr> {
        let loop_scope = self.scope.create_child_scope(
            &self.scope,
            Some(ScopedNode::ForLoopBlock(self.block)),
            Some(if self.tcb.env.config().check_control_flow_bodies {
                &self.block.children[..]
            } else {
                &[]
            }),
            None,
        );
        let initializer_id = loop_scope.resolve(LocalSymbol::Variable(&self.block.item), None);
        let initializer = TcbExpr::new(format!("const {}", initializer_id.p()))
            .add_parse_span_info(self.block.item.key_span);

        // It's common to have a for loop over a nullable value (e.g. produced by the `async` pipe).
        // Add a non-null expression to allow such values to be assigned.
        let expr = tcb_expression(&self.block.expression.ast, &self.tcb, &self.scope)
            .wrap_for_type_checker();
        let expression = TcbExpr::new(format!("{}!", expr.p()))
            .add_parse_span_info(ast_with_source_span(&self.block.expression));

        // `this.block.trackBy === null` cannot be: the fork's `@for` block always has a tracking expression.
        let statements: Vec<TcbExpr> = {
            let track_translator = TcbForLoopTrackTranslator::new(
                Rc::clone(&self.tcb),
                Rc::clone(&loop_scope),
                self.block,
            );
            let track_expression = track_translator.translate(&self.block.track_by.ast);
            let mut statements = loop_scope.render();
            statements.push(track_expression);
            statements
        };

        self.scope.add_statement(TcbExpr::new(format!(
            "for ({} of {}) {{\n{} }}",
            initializer.p(),
            expression.p(),
            get_statements_block(&statements, false)
        )));
        None
    }
}

/// expression_parser/ast.ts:609 the `sourceSpan` of an `ASTWithSource`: from its offset over its source text
fn ast_with_source_span(ast: &ASTWithSource<'_>) -> Span {
    let length = ast
        .source
        .map_or(0, |source| u32::try_from(source.len()).expect("a template's length fits in u32"));
    Span::new(ast.absolute_offset, ast.absolute_offset + length)
}

// for_block.ts:72 TcbForLoopTrackTranslator
pub struct TcbForLoopTrackTranslator<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    block: &'a R3ForLoopBlock<'a>,
    /// By `node_key`
    allowed_variables: Set<NodeKey>,
}

impl<'a, 'b> TcbForLoopTrackTranslator<'a, 'b> {
    // for_block.ts:75 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        block: &'a R3ForLoopBlock<'a>,
    ) -> Self {
        // Tracking expressions are only allowed to read the `$index`,
        // the item and properties off the component instance.
        let mut allowed_variables: Set<NodeKey> = Set::default();
        allowed_variables.insert(node_key(&block.item));
        for variable in &block.context_variables {
            if variable.value == "$index" {
                allowed_variables.insert(node_key(variable));
            }
        }
        Self { tcb, scope, block, allowed_variables }
    }
}

impl<'a, 'b> TcbExpressionTranslator<'a, 'b> for TcbForLoopTrackTranslator<'a, 'b> {
    fn tcb(&self) -> &TcbRef<'a, 'b> {
        &self.tcb
    }

    fn scope(&self) -> &ScopeRef<'a, 'b> {
        &self.scope
    }

    // for_block.ts:92 resolve
    fn resolve(&self, ast: &'a AngularExpression<'a>) -> Option<TcbExpr> {
        if let AngularExpression::PropertyRead(read) = ast
            && matches!(
                read.receiver,
                AngularExpression::ImplicitReceiver(_) | AngularExpression::ThisReceiver(_)
            )
        {
            let target = self.tcb.bound_target.get_expression_target(ast);

            if let Some(target) = target {
                let allowed = match target {
                    TemplateEntity::Variable(variable) => {
                        self.allowed_variables.contains(&node_key(variable))
                    }
                    TemplateEntity::Reference(_) | TemplateEntity::LetDeclaration(_) => false,
                };
                if !allowed {
                    self.tcb.oob_recorder.illegal_for_loop_track_access(
                        &self.tcb.id,
                        self.block,
                        read,
                    );
                }
            }
        }

        self.base_resolve(ast)
    }
}
