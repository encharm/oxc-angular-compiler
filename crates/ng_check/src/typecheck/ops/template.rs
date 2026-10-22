//! `packages/compiler/src/typecheck/ops/template.ts`

use oxc_angular_compiler::ast::r3::{R3BoundAttribute, R3Directive, R3Template, R3TemplateAttr};
use oxc_span::Span;

use crate::angular::t2_api::{DirectiveOwner, ScopedNode};
use crate::typecheck::api::{DirectiveRef, TemplateGuardType};
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::{TcbExpr, declare_variable, get_statements_block};
use crate::typecheck::ops::expression::tcb_expression;
use crate::typecheck::ops::references::LocalSymbol;

// template.ts:21 TcbTemplateContextOp
/// A `TcbOp` which generates a variable for a `Template`'s context.
///
/// Executing this operation returns a reference to the template's context variable.
pub struct TcbTemplateContextOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
}

impl<'a, 'b> TcbTemplateContextOp<'a, 'b> {
    // template.ts:22 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>) -> Self {
        Self { tcb, scope }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbTemplateContextOp<'a, 'b> {
    // template.ts:30 optional
    // The declaration of the context variable is only needed when the context is actually referenced.
    fn optional(&self) -> bool {
        true
    }

    // template.ts:32 execute
    fn execute(&self) -> Option<TcbExpr> {
        // Allocate a template ctx variable and declare it with an 'any' type. The type of this variable
        // may be narrowed as a result of template guard conditions.
        let ctx = TcbExpr::new(self.tcb.allocate_id());
        self.scope.add_statement(declare_variable(&ctx, TcbExpr::new("any")));
        Some(ctx)
    }
}

/// `Template | Directive`, the hosts of `addDirectiveGuards`
#[derive(Clone, Copy)]
enum GuardHost<'a> {
    Template(&'a R3Template<'a>),
    Directive(&'a R3Directive<'a>),
}

impl<'a> GuardHost<'a> {
    fn as_local_symbol(self) -> LocalSymbol<'a> {
        match self {
            Self::Template(t) => LocalSymbol::Template(t),
            Self::Directive(d) => LocalSymbol::Directive(d),
        }
    }

    fn inputs(self) -> &'a [R3BoundAttribute<'a>] {
        match self {
            Self::Template(t) => &t.inputs,
            Self::Directive(d) => &d.inputs,
        }
    }

    fn source_span(self) -> Span {
        match self {
            Self::Template(t) => t.source_span,
            Self::Directive(d) => d.source_span,
        }
    }
}

// template.ts:48 TcbTemplateBodyOp
/// A `TcbOp` which descends into a `Template`'s children and generates type-checking code for
/// them.
///
/// This operation wraps the children's type-checking code in an `if` block, which may include one
/// or more type guard conditions that narrow types within the template body.
pub struct TcbTemplateBodyOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    template: &'a R3Template<'a>,
}

impl<'a, 'b> TcbTemplateBodyOp<'a, 'b> {
    // template.ts:49 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, template: &'a R3Template<'a>) -> Self {
        Self { tcb, scope, template }
    }

    // template.ts:132 addDirectiveGuards
    fn add_directive_guards(
        &self,
        guards: &mut Vec<TcbExpr>,
        host_node: GuardHost<'a>,
        directives: Option<Vec<DirectiveRef>>,
    ) {
        let Some(directives) = directives.filter(|directives| !directives.is_empty()) else {
            return;
        };

        let is_template = matches!(host_node, GuardHost::Template(_));

        for dir in &directives {
            let dir_inst_id = self.scope.resolve(host_node.as_local_symbol(), Some(dir));
            let dir_id = self.tcb.env.reference_tcb_value(&dir.r#ref);

            // There are two kinds of guards. Template guards (ngTemplateGuards) allow type narrowing of
            // the expression passed to an @Input of the directive. Scan the directive to see if it has
            // any template guards, and generate them if needed.
            for guard in &dir.ng_template_guards {
                // For each template guard function on the directive, look for a binding to that input.
                let bound_input =
                    host_node.inputs().iter().find(|i| i.name == guard.input_name.as_str()).or_else(|| {
                        match host_node {
                            GuardHost::Template(template) if is_template => {
                                template.template_attrs.iter().find_map(|input| match input {
                                    R3TemplateAttr::Bound(input) if input.name == guard.input_name.as_str() => {
                                        Some(input)
                                    }
                                    _ => None,
                                })
                            }
                            _ => None,
                        }
                    });
                if let Some(bound_input) = bound_input {
                    // If there is such a binding, generate an expression for it.
                    let expr = tcb_expression(&bound_input.value, &self.tcb, &self.scope);

                    // The expression has already been checked in the type constructor invocation, so
                    // it should be ignored when used within a template guard.
                    let expr = expr.mark_ignore_diagnostics();

                    if guard.r#type == TemplateGuardType::Binding {
                        // Use the binding expression itself as guard.
                        guards.push(expr);
                    } else {
                        // Call the guard function on the directive with the directive instance and that
                        // expression.
                        let guard_invoke = TcbExpr::new(format!(
                            "{}.ngTemplateGuard_{}({}, {})",
                            dir_id.p(),
                            guard.input_name,
                            dir_inst_id.p(),
                            expr.p()
                        ));

                        let guard_invoke = guard_invoke.add_parse_span_info(bound_input.value.source_span());
                        guards.push(guard_invoke);
                    }
                }
            }

            // The second kind of guard is a template context guard. This guard narrows the template
            // rendering context variable `ctx`.
            if dir.has_ng_template_context_guard && self.tcb.env.config().apply_template_context_guards {
                let ctx = self.scope.resolve(host_node.as_local_symbol(), None);
                let guard_invoke = TcbExpr::new(format!(
                    "{}.ngTemplateContextGuard({}, {})",
                    dir_id.p(),
                    dir_inst_id.p(),
                    ctx.p()
                ));

                let guard_invoke = guard_invoke.mark_ignore_diagnostics();
                let guard_invoke = guard_invoke.add_parse_span_info(host_node.source_span());
                guards.push(guard_invoke);
            }
        }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbTemplateBodyOp<'a, 'b> {
    // template.ts:57 optional
    fn optional(&self) -> bool {
        false
    }

    // template.ts:61 execute
    fn execute(&self) -> Option<TcbExpr> {
        // An `if` will be constructed, within which the template's children will be type checked. The
        // `if` is used for two reasons: it creates a new syntactic scope, isolating variables declared
        // in the template's TCB from the outer context, and it allows any directives on the templates
        // to perform type narrowing of either expressions or the template's context.
        //
        // The guard is the `if` block's condition. It's usually set to `true` but directives that exist
        // on the template can trigger extra guard expressions that serve to narrow types within the
        // `if`. `guard` is calculated by starting with `true` and adding other conditions as needed.
        // Collect these into `guards` by processing the directives.

        // By default the guard is simply `true`.
        let mut guard: Option<TcbExpr> = None;
        let mut directive_guards: Vec<TcbExpr> = Vec::new();

        self.add_directive_guards(
            &mut directive_guards,
            GuardHost::Template(self.template),
            self.tcb.bound_target.get_directives_of_node(DirectiveOwner::Template(self.template)),
        );

        for directive in &self.template.directives {
            self.add_directive_guards(
                &mut directive_guards,
                GuardHost::Directive(directive),
                self.tcb.bound_target.get_directives_of_node(DirectiveOwner::Directive(directive)),
            );
        }

        // If there are any guards from directives, use them instead.
        if let Some(last) = directive_guards.pop() {
            // Pop the first value and use it as the initializer to reduce(). This way, a single guard
            // will be used on its own, but two or more will be combined into binary AND expressions.
            guard = Some(
                directive_guards
                    .into_iter()
                    .fold(last, |expr, dir_guard| TcbExpr::new(format!("{} && {}", expr.p(), dir_guard.p()))),
            );
        }

        // Create a new Scope for the template. This constructs the list of operations for the template
        // children, as well as tracks bindings within the template.
        let tmpl_scope = self.scope.create_child_scope(
            &self.scope,
            Some(ScopedNode::Template(self.template)),
            Some(&self.template.children),
            guard.clone(),
        );

        // Render the template's `Scope` into its statements.
        let statements = tmpl_scope.render();
        if statements.is_empty() {
            // As an optimization, don't generate the scope's block if it has no statements. This is
            // beneficial for templates that contain for example `<span *ngIf="first"></span>`, in which
            // case there's no need to render the `NgIf` guard expression. This seems like a minor
            // improvement, however it reduces the number of flow-node antecedents that TypeScript needs
            // to keep into account for such cases, resulting in an overall reduction of
            // type-checking time.
            return None;
        }

        let mut tmpl_block = format!("{{\n{}}}", get_statements_block(&statements, false));
        if let Some(guard) = &guard {
            // The scope has a guard that needs to be applied, so wrap the template block into an `if`
            // statement containing the guard expression.
            tmpl_block = format!("if ({}) {tmpl_block}", guard.p());
        }
        self.scope.add_statement(TcbExpr::new(tmpl_block));

        None
    }
}
