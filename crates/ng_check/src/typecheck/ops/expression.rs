//! `packages/compiler/src/typecheck/ops/expression.ts`
//!
//! `TcbExpressionTranslator` is a trait (it has subclasses): see its comment.

use std::rc::Rc;

use oxc_angular_compiler::ast::expression::{AngularExpression, PropertyRead};
use oxc_angular_compiler::ast::r3::R3LetDeclaration;

use crate::angular::t2_api::TemplateEntity;
use crate::typecheck::expression::ast_to_tcb_expr;
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::context::Context;
use crate::typecheck::ops::references::LocalSymbol;

// expression.ts:35 tcbExpression
/// Process an `AST` expression and convert it into a `ts.Expression`, generating references to the
/// correct identifiers in the current scope.
pub fn tcb_expression<'a, 'b>(
    ast: &'a AngularExpression<'a>,
    tcb: &TcbRef<'a, 'b>,
    scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    let translator = BaseTcbExpressionTranslator::new(Rc::clone(tcb), Rc::clone(scope));
    translator.translate(ast)
}

// expression.ts:43 unwrapWritableSignal
/// Wraps an expression in an `unwrapSignal` call which extracts the signal's value.
#[expect(clippy::needless_pass_by_value, reason = "TypeScript's signature, which the callers use")]
pub fn unwrap_writable_signal(expression: TcbExpr, tcb: &Context<'_, '_>) -> TcbExpr {
    // `R3Identifiers.unwrapWritableSignal`
    let unwrap_ref = tcb.env.reference_external_symbol("@angular/core", "ɵunwrapWritableSignal");
    TcbExpr::new(format!("{}({})", unwrap_ref.p(), expression.p()))
}

// expression.ts:56 TcbExpressionOp
/// A `TcbOp` which renders an Angular expression (e.g. `{{foo() && bar.baz}}`).
///
/// Executing this operation returns nothing.
pub struct TcbExpressionOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    expression: &'a AngularExpression<'a>,
}

impl<'a, 'b> TcbExpressionOp<'a, 'b> {
    // expression.ts:57 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        expression: &'a AngularExpression<'a>,
    ) -> Self {
        Self { tcb, scope, expression }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbExpressionOp<'a, 'b> {
    // expression.ts:65 optional
    fn optional(&self) -> bool {
        false
    }

    // expression.ts:69 execute
    fn execute(&self) -> Option<TcbExpr> {
        let expr = tcb_expression(self.expression, &self.tcb, &self.scope);
        self.scope.add_statement(expr);
        None
    }
}

// expression.ts:83 TcbConditionOp
/// A `TcbOp` which renders an Angular expression inside a conditional context.
///
/// This is used for `@defer` triggers (`when`, `prefetch when`, `hydrate when`)
/// to enable TypeScript's TS2774 diagnostic for uninvoked functions/signals.
///
/// Executing this operation returns nothing.
pub struct TcbConditionOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    expression: &'a AngularExpression<'a>,
}

impl<'a, 'b> TcbConditionOp<'a, 'b> {
    // expression.ts:84 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        expression: &'a AngularExpression<'a>,
    ) -> Self {
        Self { tcb, scope, expression }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbConditionOp<'a, 'b> {
    // expression.ts:92 optional
    fn optional(&self) -> bool {
        false
    }

    // expression.ts:96 execute
    fn execute(&self) -> Option<TcbExpr> {
        let expr = tcb_expression(self.expression, &self.tcb, &self.scope);
        // Wrap in an if-statement to enable TS2774 for uninvoked signals/functions.
        self.scope.add_statement(TcbExpr::new(format!("if ({}) {{}}", expr.p())));
        None
    }
}

// expression.ts:104 TcbExpressionTranslator. A class with subclasses (`TcbEventHandlerTranslator` in events.rs,
// `TcbForLoopTrackTranslator` in for_block.rs): a trait whose provided methods are the base class's. A subclass
// implements the accessors and overrides `resolve` / `is_valid_let_declaration_access`, reaching the base class's
// `resolve` (`super.resolve`) through `base_resolve`. The base class itself is `BaseTcbExpressionTranslator`.
pub trait TcbExpressionTranslator<'a: 'b, 'b> {
    /// `this.tcb`
    fn tcb(&self) -> &TcbRef<'a, 'b>;

    /// `this.scope`
    fn scope(&self) -> &ScopeRef<'a, 'b>;

    // expression.ts:110 translate
    fn translate(&self, ast: &'a AngularExpression<'a>) -> TcbExpr {
        // `astToTcbExpr` actually does the conversion. A special resolver `tcbResolve` is passed
        // which interprets specific expression nodes that interact with the `ImplicitReceiver`. These
        // nodes actually refer to identifiers within the current scope.
        let config = self.tcb().env.config();
        ast_to_tcb_expr(ast, &|ast| self.resolve(ast), config)
    }

    // expression.ts:123 resolve
    /// Resolve an `AST` expression within the given scope.
    ///
    /// Some `AST` expressions refer to top-level concepts (references, variables, the component
    /// context). This method assists in resolving those.
    fn resolve(&self, ast: &'a AngularExpression<'a>) -> Option<TcbExpr> {
        self.base_resolve(ast)
    }

    /// expression.ts:123 resolve, the base class's (`super.resolve`)
    fn base_resolve(&self, ast: &'a AngularExpression<'a>) -> Option<TcbExpr> {
        use AngularExpression as E;
        let tcb = self.tcb();
        match ast {
            E::PropertyRead(read) if matches!(read.receiver, E::ImplicitReceiver(_)) => {
                // Try to resolve a bound target for this expression. If no such target is available, then
                // the expression is referencing the top-level component context. In that case, `null` is
                // returned here to let it fall through resolution so it will be caught when the
                // `ImplicitReceiver` is resolved in the branch below.
                let target = tcb.bound_target.get_expression_target(ast);
                let target_expression =
                    target.map(|target| get_target_node_expression(self, target, ast));
                if let Some(TemplateEntity::LetDeclaration(target)) = target
                    && !self.is_valid_let_declaration_access(target, read)
                {
                    tcb.oob_recorder.let_used_before_definition(&tcb.id, read, target);
                    // Cast the expression to `any` so we don't produce additional diagnostics.
                    // We don't use `markIgnoreForDiagnostics` here, because it won't prevent duplicate
                    // diagnostics for nested accesses in cases like `@let value = value.foo.bar.baz`.
                    if let Some(target_expression) = &target_expression {
                        return Some(TcbExpr::new(format!("{} as any", target_expression.p())));
                    }
                }

                target_expression
            }
            // (`Unary.isUpdateOperation`: the fork's `Unary` has no `++`/`--` operators, so only assignments are
            // writes.)
            E::Binary(binary) if binary.operation.is_assignment() => {
                let read = unwrap_write_target(&binary.left);

                let E::PropertyRead(read_node) = read else {
                    return None;
                };
                if !matches!(read_node.receiver, E::ImplicitReceiver(_) | E::ThisReceiver(_)) {
                    return None;
                }

                let target = tcb.bound_target.get_expression_target(read)?;

                let target_expression = get_target_node_expression(self, target, read);

                let expr = self.translate(&binary.right);
                let mut result =
                    TcbExpr::new(format!("({} = {})", target_expression.p(), expr.p()));

                result = result.add_parse_span_info(read.source_span());

                // Ignore diagnostics from TS produced for writes to `@let` and re-report them using
                // our own infrastructure. We can't rely on the TS reporting, because it includes
                // the name of the auto-generated TCB variable name.
                if let TemplateEntity::LetDeclaration(target) = target {
                    result = result.mark_ignore_diagnostics();
                    tcb.oob_recorder.illegal_write_to_let_declaration(&tcb.id, read, target);
                }

                Some(result)
            }
            E::ImplicitReceiver(_) | E::ThisReceiver(_) => {
                // AST instances representing variables and references look very similar to property reads
                // or method calls from the component context: both have the shape
                // PropertyRead(ImplicitReceiver, 'propName') or Call(ImplicitReceiver, 'methodName').
                //
                // `translate` will first try to `resolve` the outer PropertyRead/Call. If this works,
                // it's because the `BoundTarget` found an expression target for the whole expression, and
                // therefore `translate` will never attempt to `resolve` the ImplicitReceiver of that
                // PropertyRead/Call.
                //
                // Therefore if `resolve` is called on an `ImplicitReceiver`, it's because no outer
                // PropertyRead/Call resolved to a variable or reference, and therefore this is a
                // property read or method call on the component context itself.
                Some(TcbExpr::new("this"))
            }
            E::BindingPipe(pipe_ast) => {
                let expr = self.translate(&pipe_ast.exp);
                let pipe_meta = tcb.get_pipe_by_name(&pipe_ast.name);
                let pipe = match pipe_meta {
                    None => {
                        // No pipe by that name exists in scope. Record this as an error.
                        tcb.oob_recorder.missing_pipe(&tcb.id, pipe_ast, tcb.host_is_standalone);

                        // Use an 'any' value to at least allow the rest of the expression to be checked.
                        TcbExpr::new("(0 as any)")
                    }
                    Some(pipe_meta) if pipe_meta.is_explicitly_deferred => {
                        let enclosing_blocks = tcb.bound_target.get_defer_blocks_of_pipe(pipe_ast);
                        let is_deferred = !enclosing_blocks.is_empty();
                        if !is_deferred {
                            tcb.oob_recorder
                                .deferred_pipe_used_eagerly(&tcb.id, pipe_ast, None, None);
                            TcbExpr::new("(0 as any)")
                        } else if let Some(deferred_blocks) = &pipe_meta.deferred_blocks {
                            // `b.definedName !== null && pipeMeta.deferredBlocks.has(b.definedName)`: the fork's
                            // `@defer` blocks have no name (`definedName` is always null).
                            let is_allowed_in_block = false;
                            if is_allowed_in_block {
                                tcb.env.pipe_inst(pipe_meta)
                            } else {
                                // `enclosingBlocks[enclosingBlocks.length - 1].definedName ?? 'unnamed'`
                                let current_block_name = "unnamed";
                                let declared_blocks: Vec<String> =
                                    deferred_blocks.iter().cloned().collect();
                                tcb.oob_recorder.deferred_pipe_used_eagerly(
                                    &tcb.id,
                                    pipe_ast,
                                    Some(current_block_name),
                                    Some(&declared_blocks),
                                );
                                TcbExpr::new("(0 as any)")
                            }
                        } else {
                            tcb.env.pipe_inst(pipe_meta)
                        }
                    }
                    // Use a variable declared as the pipe's type.
                    Some(pipe_meta) => tcb.env.pipe_inst(pipe_meta),
                };
                let args: Vec<String> =
                    pipe_ast.args.iter().map(|arg| self.translate(arg).p()).collect();
                let mut method_access = TcbExpr::new(format!("{}.transform", pipe.p()))
                    .add_parse_span_info(pipe_ast.name_span);

                if !tcb.env.config().check_type_of_pipes {
                    method_access = TcbExpr::new(format!("({} as any)", method_access.p()));
                }

                let mut all_args = vec![expr.p()];
                all_args.extend(args);
                let result =
                    TcbExpr::new(format!("{}({})", method_access.p(), all_args.join(", ")));
                Some(result.add_parse_span_info(pipe_ast.source_span))
            }
            E::Call(_) | E::SafeCall(_) => {
                let (receiver, args, source_span) = match ast {
                    E::Call(call) => (&call.receiver, &call.args, call.source_span),
                    E::SafeCall(call) => (&call.receiver, &call.args, call.source_span),
                    _ => unreachable!(),
                };
                let (receiver_receiver, receiver_name, receiver_name_span) = match receiver {
                    E::PropertyRead(read) => (&read.receiver, read.name.as_str(), read.name_span),
                    E::SafePropertyRead(read) => {
                        (&read.receiver, read.name.as_str(), read.name_span)
                    }
                    // This AST isn't special after all.
                    _ => return None,
                };

                // Resolve the special `$safeNavigationMigration(expr)` syntax to evaluate as the wrapped expression.
                // `$safeNavigationMigration(expr)` -> `expr`
                // In the TCB this magic function will not affect the emitted TS
                // as Safe avigation expressions already returned `undefined` instead of null
                // see bug https://github.com/angular/angular/issues/37622
                if matches!(receiver_receiver, E::ImplicitReceiver(_))
                    && receiver_name == "$safeNavigationMigration"
                    && args.len() == 1
                {
                    let expr = self.translate(&args[0]);
                    let result = TcbExpr::new(format!("({})", expr.p()));
                    return Some(result.add_parse_span_info(source_span));
                }

                // Resolve the special `$any(expr)` syntax to insert a cast of the argument to type `any`.
                // `$any(expr)` -> `expr as any`
                if matches!(receiver_receiver, E::ImplicitReceiver(_))
                    && receiver_name == "$any"
                    && args.len() == 1
                {
                    let expr = self.translate(&args[0]);
                    let result = TcbExpr::new(format!("({} as any)", expr.p()));
                    return Some(result.add_parse_span_info(source_span));
                }

                // Attempt to resolve a bound target for the method, and generate the method call if a target
                // could be resolved. If no target is available, then the method is referencing the top-level
                // component context, in which case `null` is returned to let the `ImplicitReceiver` being
                // resolved to the component context.
                let target = tcb.bound_target.get_expression_target(ast)?;

                let method = get_target_node_expression(self, target, ast);
                let method = method.add_parse_span_info(receiver_name_span).wrap_for_type_checker();
                let args: Vec<String> = args.iter().map(|arg| self.translate(arg).p()).collect();
                let node = TcbExpr::new(format!("{}({})", method.p(), args.join(", ")));
                Some(node.add_parse_span_info(source_span))
            }
            // This AST isn't special after all.
            _ => None,
        }
    }

    // expression.ts:311 isValidLetDeclarationAccess
    fn is_valid_let_declaration_access(
        &self,
        target: &'a R3LetDeclaration<'a>,
        ast: &'a PropertyRead<'a>,
    ) -> bool {
        let target_start = target.source_span.start;
        let target_end = target.source_span.end;
        let ast_start = ast.source_span.start;

        // We only flag local references that occur before the declaration, because embedded views
        // are updated before the child views. In practice this means that something like
        // `<ng-template [ngIf]="true">{{value}}</ng-template> @let value = 1;` is valid.
        (target_start < ast_start && ast_start > target_end)
            || !self.scope().is_local(TemplateEntity::LetDeclaration(target))
    }
}

/// `new TcbExpressionTranslator(tcb, scope)`: the base class itself
pub struct BaseTcbExpressionTranslator<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
}

impl<'a, 'b> BaseTcbExpressionTranslator<'a, 'b> {
    // expression.ts:105 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>) -> Self {
        Self { tcb, scope }
    }
}

impl<'a, 'b> TcbExpressionTranslator<'a, 'b> for BaseTcbExpressionTranslator<'a, 'b> {
    fn tcb(&self) -> &TcbRef<'a, 'b> {
        &self.tcb
    }

    fn scope(&self) -> &ScopeRef<'a, 'b> {
        &self.scope
    }
}

// expression.ts:305 getTargetNodeExpression
fn get_target_node_expression<'a: 'b, 'b, T: TcbExpressionTranslator<'a, 'b> + ?Sized>(
    this: &T,
    target_node: TemplateEntity<'a>,
    expression_node: &'a AngularExpression<'a>,
) -> TcbExpr {
    let expr = this.scope().resolve(LocalSymbol::from(target_node), None);
    expr.add_parse_span_info(expression_node.source_span())
}

/// `packages/compiler/src/expression_parser/ast.ts`:550 unwrapWriteTarget (not in the fork)
fn unwrap_write_target<'a>(ast: &'a AngularExpression<'a>) -> &'a AngularExpression<'a> {
    let mut current = ast;
    loop {
        match current {
            AngularExpression::ParenthesizedExpression(e) => current = &e.expression,
            AngularExpression::NonNullAssert(e) => current = &e.expression,
            _ => return current,
        }
    }
}
