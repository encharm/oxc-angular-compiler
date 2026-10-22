//! `packages/compiler-cli/preprocessor/src/template_semantics.ts`: verifies the semantics of a component's template,
//! reporting the diagnostics of ngtsc's `TemplateSemanticsCheckerImpl`.
//!
//! The two visitors are TypeScript subclasses of `TmplAstRecursiveVisitor` and `RecursiveAstVisitor`; here they are
//! walks that visit what those visit, in the same order (only bound events and expressions matter). Diagnostics are
//! at JavaScript string (UTF-16) indices: run inside `span::with_source_text` for the template's text.

use oxc_angular_compiler::ast::expression::{AngularExpression, Binary, PropertyRead, Unary};
use oxc_angular_compiler::ast::r3::{R3BoundEvent, R3Directive, R3Node};

use crate::angular::t2_api::{BoundTarget, TemplateEntity};
use crate::ngp::diagnostic::Diagnostic;
use crate::typecheck::api::{DirectiveRef, TypeCheckId};
use crate::typecheck::oob::OutOfBandDiagnosticCategory;

// template_semantics.ts:29 WRITE_TO_READ_ONLY_VARIABLE: `ErrorCode.WRITE_TO_READ_ONLY_VARIABLE`
const WRITE_TO_READ_ONLY_VARIABLE: u32 = 8005;

// template_semantics.ts:36 getTemplateSemanticsDiagnostics
pub fn get_template_semantics_diagnostics<'a>(
    type_check_id: &TypeCheckId,
    nodes: &'a [R3Node<'a>],
    bound_target: &dyn BoundTarget<'a, DirectiveRef>,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let mut expression_visitor = ExpressionsSemanticsVisitor { type_check_id, bound_target, diagnostics: &mut diagnostics };
    let mut template_visitor = TemplateSemanticsVisitor { expression_visitor: &mut expression_visitor };
    template_visitor.visit_all(nodes);
    diagnostics
}

// template_semantics.ts:51 TemplateSemanticsVisitor: visitor that verifies the semantics of a template
struct TemplateSemanticsVisitor<'v, 'w, 'a, 'b> {
    expression_visitor: &'v mut ExpressionsSemanticsVisitor<'w, 'a, 'b>,
}

impl<'a> TemplateSemanticsVisitor<'_, '_, 'a, '_> {
    // template_semantics.ts:56 visitBoundEvent
    fn visit_bound_event(&mut self, event: &'a R3BoundEvent<'a>) {
        // (`super.visitBoundEvent` does nothing)
        self.expression_visitor.visit(&event.handler, event);
    }

    /// r3_ast.ts visitAll, with `RecursiveVisitor`'s visit of each node
    fn visit_all(&mut self, nodes: &'a [R3Node<'a>]) {
        for node in nodes {
            self.visit_node(node);
        }
    }

    fn visit_bound_events(&mut self, events: &'a [R3BoundEvent<'a>]) {
        for event in events {
            self.visit_bound_event(event);
        }
    }

    fn visit_directives(&mut self, directives: &'a [R3Directive<'a>]) {
        for directive in directives {
            // r3_ast.ts RecursiveVisitor.visitDirective: attributes, inputs, outputs, references
            self.visit_bound_events(&directive.outputs);
        }
    }

    /// r3_ast.ts RecursiveVisitor: the nodes and bindings it visits (attributes, inputs, references, variables,
    /// triggers and text visit nothing)
    fn visit_node(&mut self, node: &'a R3Node<'a>) {
        match node {
            R3Node::Element(element) => {
                self.visit_bound_events(&element.outputs);
                self.visit_directives(&element.directives);
                self.visit_all(&element.children);
            }
            R3Node::Template(template) => {
                self.visit_bound_events(&template.outputs);
                self.visit_directives(&template.directives);
                self.visit_all(&template.children);
            }
            R3Node::Component(component) => {
                self.visit_bound_events(&component.outputs);
                self.visit_directives(&component.directives);
                self.visit_all(&component.children);
            }
            R3Node::Directive(directive) => self.visit_bound_events(&directive.outputs),
            R3Node::Content(content) => self.visit_all(&content.children),
            R3Node::DeferredBlock(block) => {
                self.visit_all(&block.children);
                if let Some(placeholder) = &block.placeholder {
                    self.visit_all(&placeholder.children);
                }
                if let Some(loading) = &block.loading {
                    self.visit_all(&loading.children);
                }
                if let Some(error) = &block.error {
                    self.visit_all(&error.children);
                }
            }
            R3Node::DeferredBlockPlaceholder(block) => self.visit_all(&block.children),
            R3Node::DeferredBlockLoading(block) => self.visit_all(&block.children),
            R3Node::DeferredBlockError(block) => self.visit_all(&block.children),
            R3Node::SwitchBlock(block) => {
                for group in &block.groups {
                    self.visit_all(&group.children);
                }
            }
            R3Node::SwitchBlockCaseGroup(group) => self.visit_all(&group.children),
            R3Node::ForLoopBlock(block) => {
                self.visit_all(&block.children);
                if let Some(empty) = &block.empty {
                    self.visit_all(&empty.children);
                }
            }
            R3Node::ForLoopBlockEmpty(block) => self.visit_all(&block.children),
            R3Node::IfBlock(block) => {
                for branch in &block.branches {
                    self.visit_all(&branch.children);
                }
            }
            R3Node::IfBlockBranch(branch) => self.visit_all(&branch.children),
            // (Angular's `BoundaryBlock`, `BoundaryErrorBlock` and `ContentBlock` are not in the fork's AST.)
            R3Node::Comment(_)
            | R3Node::Text(_)
            | R3Node::BoundText(_)
            | R3Node::Variable(_)
            | R3Node::Reference(_)
            | R3Node::Icu(_)
            | R3Node::UnknownBlock(_)
            | R3Node::LetDeclaration(_)
            | R3Node::HostElement(_) => {}
        }
    }
}

// template_semantics.ts:63 ExpressionsSemanticsVisitor: visitor that verifies the semantics of the expressions within
// a template. Its context is always the bound event whose handler it visits.
struct ExpressionsSemanticsVisitor<'w, 'a, 'b> {
    type_check_id: &'w TypeCheckId,
    bound_target: &'w (dyn BoundTarget<'a, DirectiveRef> + 'b),
    diagnostics: &'w mut Vec<Diagnostic>,
}

impl<'a> ExpressionsSemanticsVisitor<'_, 'a, '_> {
    /// expression_parser/ast.ts RecursiveAstVisitor.visit, with the overridden `visitBinary` and `visitUnary`
    fn visit(&mut self, ast: &'a AngularExpression<'a>, context: &'a R3BoundEvent<'a>) {
        use AngularExpression as E;
        match ast {
            E::Binary(ast) => self.visit_binary(ast, context),
            E::Unary(ast) => self.visit_unary(ast, context),
            E::Chain(ast) => self.visit_all(&ast.expressions, context),
            E::Conditional(ast) => {
                self.visit(&ast.condition, context);
                self.visit(&ast.true_exp, context);
                self.visit(&ast.false_exp, context);
            }
            E::BindingPipe(ast) => {
                self.visit(&ast.exp, context);
                self.visit_all(&ast.args, context);
            }
            E::Interpolation(ast) => self.visit_all(&ast.expressions, context),
            E::KeyedRead(ast) => {
                self.visit(&ast.receiver, context);
                self.visit(&ast.key, context);
            }
            E::LiteralArray(ast) => self.visit_all(&ast.expressions, context),
            E::LiteralMap(ast) => self.visit_all(&ast.values, context),
            E::PrefixNot(ast) => self.visit(&ast.expression, context),
            E::TypeofExpression(ast) => self.visit(&ast.expression, context),
            E::VoidExpression(ast) => self.visit(&ast.expression, context),
            E::NonNullAssert(ast) => self.visit(&ast.expression, context),
            E::PropertyRead(ast) => self.visit(&ast.receiver, context),
            E::SafePropertyRead(ast) => self.visit(&ast.receiver, context),
            E::SafeKeyedRead(ast) => {
                self.visit(&ast.receiver, context);
                self.visit(&ast.key, context);
            }
            E::Call(ast) => {
                self.visit(&ast.receiver, context);
                self.visit_all(&ast.args, context);
            }
            E::SafeCall(ast) => {
                self.visit(&ast.receiver, context);
                self.visit_all(&ast.args, context);
            }
            // (template literal elements visit nothing)
            E::TemplateLiteral(ast) => self.visit_all(&ast.expressions, context),
            E::TaggedTemplateLiteral(ast) => {
                self.visit(&ast.tag, context);
                self.visit_all(&ast.template.expressions, context);
            }
            E::ParenthesizedExpression(ast) => self.visit(&ast.expression, context),
            E::ArrowFunction(ast) => self.visit(&ast.body, context),
            E::SpreadElement(ast) => self.visit(&ast.expression, context),
            E::Empty(_)
            | E::ImplicitReceiver(_)
            | E::ThisReceiver(_)
            | E::LiteralPrimitive(_)
            | E::RegularExpressionLiteral(_) => {}
        }
    }

    fn visit_all(&mut self, asts: &'a [AngularExpression<'a>], context: &'a R3BoundEvent<'a>) {
        for ast in asts {
            self.visit(ast, context);
        }
    }

    // template_semantics.ts:72 visitBinary
    fn visit_binary(&mut self, ast: &'a Binary<'a>, context: &'a R3BoundEvent<'a>) {
        if ast.operation.is_assignment()
            && let AngularExpression::PropertyRead(left) = &ast.left
        {
            self.check_for_illegal_write_in_event_binding(&ast.left, left, context);
        }

        // Always recurse since the right-hand side may contain other writes (e.g. `prop = y++`).
        self.visit(&ast.left, context);
        self.visit(&ast.right, context);
    }

    // template_semantics.ts:81 visitUnary. The fork's expressions have no update operators (`++`, `--`: Angular's
    // `Unary.isUpdateOperation` and `unwrapWriteTarget`), so a unary expression is never a write.
    fn visit_unary(&mut self, ast: &'a Unary<'a>, context: &'a R3BoundEvent<'a>) {
        self.visit(&ast.expr, context);
    }

    // TODO(parity): ngtsc also visits property reads (`visitPropertyRead`) to report two-way
    // bindings to template variables and `@let` declarations that don't hold signals. It tells signals apart with the
    // type checker, which the preprocessor doesn't have, so that check isn't ported (template_semantics.ts:95).

    // template_semantics.ts:103 checkForIllegalWriteInEventBinding. `expr` is `ast` as an expression (the binder
    // identifies an expression by its `AngularExpression`).
    fn check_for_illegal_write_in_event_binding(
        &mut self,
        expr: &AngularExpression<'a>,
        ast: &'a PropertyRead<'a>,
        context: &'a R3BoundEvent<'a>,
    ) {
        if !Self::should_check_for_illegal_writes(ast) {
            return;
        }

        let target = self.bound_target.get_expression_target(expr);
        if let Some(TemplateEntity::Variable(target)) = target {
            // TODO(parity): ngtsc attaches related information ("'<name>' is declared here." at the
            // variable's `valueSpan || sourceSpan`), which `Diagnostic` can't carry yet.
            self.report_illegal_write(
                context,
                format!(
                    "Cannot use variable '{}' as the left-hand side of an assignment expression. Template variables are read-only.",
                    target.name
                ),
            );
        }
    }

    // template_semantics.ts:119 reportIllegalWrite
    fn report_illegal_write(&mut self, event: &R3BoundEvent<'a>, message: String) {
        self.diagnostics.push(Diagnostic {
            type_check_id: self.type_check_id.clone(),
            category: OutOfBandDiagnosticCategory::Error,
            code: Some(WRITE_TO_READ_ONLY_VARIABLE),
            message,
            start: crate::span::utf16(event.handler_span.start),
            end: crate::span::utf16(event.handler_span.end),
        });
    }

    // template_semantics.ts:130 shouldCheckForIllegalWrites (the context is always a bound event here)
    fn should_check_for_illegal_writes(ast: &PropertyRead<'a>) -> bool {
        matches!(ast.receiver, AngularExpression::ImplicitReceiver(_) | AngularExpression::ThisReceiver(_))
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_angular_compiler::ast::expression::BindingPipe;
    use oxc_angular_compiler::ast::r3::{R3DeferredBlock, R3Element, R3Reference, R3Variable};

    use super::*;
    use crate::angular::t2_api::{
        Binding, BindingConsumer, ConflictingHostDirectiveBinding, DirectiveOwner, ForeignComponentMeta,
        ReferenceTarget, ScopedNode, Target,
    };
    use crate::ngp::compiler_utils::{TemplateParseOptions, parse_template};

    /// A bound target whose every property read on the component resolves to `variable`
    struct VariableTarget<'a> {
        target: Target<'a, DirectiveRef>,
        variable: &'a R3Variable<'a>,
    }

    impl<'a> BoundTarget<'a, DirectiveRef> for VariableTarget<'a> {
        fn target(&self) -> &Target<'a, DirectiveRef> {
            &self.target
        }
        fn get_expression_target(&self, expr: &AngularExpression<'a>) -> Option<TemplateEntity<'a>> {
            match expr {
                AngularExpression::PropertyRead(read) if read.name == self.variable.name => {
                    Some(TemplateEntity::Variable(self.variable))
                }
                _ => None,
            }
        }
        fn get_directives_of_node(&self, _: DirectiveOwner<'a>) -> Option<Vec<DirectiveRef>> {
            unreachable!()
        }
        fn get_foreign_component(&self, _: &'a R3Element<'a>) -> Option<ForeignComponentMeta> {
            unreachable!()
        }
        fn get_reference_target(&self, _: &'a R3Reference<'a>) -> Option<ReferenceTarget<'a, DirectiveRef>> {
            unreachable!()
        }
        fn get_consumer_of_binding(&self, _: Binding<'a>) -> Option<BindingConsumer<'a, DirectiveRef>> {
            unreachable!()
        }
        fn get_definition_node_of_symbol(&self, _: TemplateEntity<'a>) -> Option<ScopedNode<'a>> {
            unreachable!()
        }
        fn get_nesting_level(&self, _: ScopedNode<'a>) -> usize {
            unreachable!()
        }
        fn get_entities_in_scope(&self, _: Option<ScopedNode<'a>>) -> Vec<TemplateEntity<'a>> {
            unreachable!()
        }
        fn get_used_directives(&self) -> Vec<DirectiveRef> {
            unreachable!()
        }
        fn get_eagerly_used_directives(&self) -> Vec<DirectiveRef> {
            unreachable!()
        }
        fn get_used_pipes(&self) -> Vec<String> {
            unreachable!()
        }
        fn get_eagerly_used_pipes(&self) -> Vec<String> {
            unreachable!()
        }
        fn get_defer_blocks(&self) -> Vec<&'a R3DeferredBlock<'a>> {
            unreachable!()
        }
        fn get_deferred_trigger_target(
            &self,
            _: &'a R3DeferredBlock<'a>,
            _: crate::angular::t2_api::ReferenceDeferredTrigger<'a>,
        ) -> Option<&'a R3Element<'a>> {
            unreachable!()
        }
        fn is_deferred(&self, _: DirectiveOwner<'a>) -> bool {
            unreachable!()
        }
        fn get_defer_blocks_of_node(&self, _: DirectiveOwner<'a>) -> Vec<&'a R3DeferredBlock<'a>> {
            unreachable!()
        }
        fn get_defer_blocks_of_pipe(&self, _: &'a BindingPipe<'a>) -> Vec<&'a R3DeferredBlock<'a>> {
            unreachable!()
        }
        fn referenced_directive_exists(&self, _: &str) -> bool {
            unreachable!()
        }
        fn get_conflicting_host_directive_bindings(
            &self,
            _: DirectiveOwner<'a>,
        ) -> Option<Vec<ConflictingHostDirectiveBinding<DirectiveRef>>> {
            unreachable!()
        }
    }

    #[test]
    fn writes_to_template_variables_are_reported_at_the_handler() {
        let allocator = Allocator::default();
        let template =
            allocator.alloc_str("<ng-template let-v><b (click)=\"v = 1; other.v = 2\" (focus)=\"x = v\"></b></ng-template>");
        let options = TemplateParseOptions { preserve_whitespaces: Some(true), ..TemplateParseOptions::default() };
        let parsed = parse_template(&allocator, template, "t.html", &options);
        assert!(parsed.errors.is_none(), "{:?}", parsed.errors);
        let R3Node::Template(ng_template) = &parsed.nodes[0] else { panic!("{:?}", parsed.nodes) };
        let bound_target =
            VariableTarget { target: Target { template: Some(parsed.nodes), host: None }, variable: &ng_template.variables[0] };
        let diagnostics = crate::span::with_source_text(template, || {
            get_template_semantics_diagnostics(&"tcb1".to_string(), parsed.nodes, &bound_target)
        });
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code, Some(8005));
        assert_eq!(
            diagnostic.message,
            "Cannot use variable 'v' as the left-hand side of an assignment expression. Template variables are read-only."
        );
        let handler = "v = 1; other.v = 2";
        let start = template.find(handler).unwrap() as u32;
        assert_eq!((diagnostic.start, diagnostic.end), (start, start + handler.len() as u32));
    }
}
