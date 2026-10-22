//! `packages/compiler/src/typecheck/type_check_block.ts`

use std::rc::Rc;

use crate::angular::t2_api::ScopedNode;
use crate::typecheck::api::{TcbComponentMetadata, TcbEnvironment, TcbTypeCheckBlockMetadata};
use crate::typecheck::host_bindings::create_host_bindings_block_guard;
use crate::typecheck::oob::OutOfBandDiagnosticRecorder;
use crate::typecheck::ops::codegen::get_statements_block;
use crate::typecheck::ops::context::Context;
use crate::typecheck::ops::scope::Scope;
use crate::typecheck::schema::DomSchemaChecker;

// type_check_block.ts:29 generateTypeCheckBlock
/// Given a component and metadata, compose a "type check block" function.
///
/// `env`: an `TcbEnvironment` into which type-checking code will be generated. `component`: metadata about the
/// component class. `name`: name of the generated function. `meta`: metadata about the component's template and the
/// function being generated. `dom_schema_checker`: used to check and record errors regarding improper usage of DOM
/// elements and bindings. `oob_recorder`: used to record errors regarding template elements which could not be
/// correctly translated into types during TCB generation.
///
/// Span comments are written as JavaScript string indices: run it inside `crate::span::with_source_text` with the
/// template's text.
pub fn generate_type_check_block<'a, 'b>(
    env: &'b dyn TcbEnvironment,
    component: &TcbComponentMetadata,
    name: &str,
    meta: &'b TcbTypeCheckBlockMetadata<'a, 'b>,
    dom_schema_checker: &'b dyn DomSchemaChecker<'a>,
    oob_recorder: &'b dyn OutOfBandDiagnosticRecorder<'a>,
) -> String {
    let tcb = Rc::new(Context::new(
        env,
        dom_schema_checker,
        oob_recorder,
        meta.id.clone(),
        meta.bound_target,
        meta.pipes.as_ref(),
        &meta.schemas,
        meta.is_standalone,
        meta.preserve_whitespaces,
    ));
    let ctx_raw_type = env.reference_tcb_value(&component.r#ref);
    let type_parameters = &component.type_parameters;
    let type_arguments = &component.type_arguments;

    let type_params_str = match type_parameters {
        Some(type_parameters)
            if env.config().use_context_generic_type && !type_parameters.is_empty() =>
        {
            format!(
                "<{}>",
                type_parameters
                    .iter()
                    .map(|p| p.representation.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        _ => String::new(),
    };
    let type_args_str = match type_arguments {
        Some(type_arguments) if !type_arguments.is_empty() => {
            format!("<{}>", type_arguments.join(", "))
        }
        _ => String::new(),
    };

    let this_param_str = format!("this: {}{type_args_str}", ctx_raw_type.p());
    let mut statements: Vec<String> = Vec::new();

    // Add the template type checking code.
    if let Some(template) = tcb.bound_target.target().template {
        let template_scope =
            Scope::for_nodes(&tcb, None, None, Some(template), /* guard */ None);

        statements.push(render_block_statements(env, &template_scope, "true"));
    }

    // Add the host bindings type checking code.
    if let Some(host) = &tcb.bound_target.target().host {
        let host_scope =
            Scope::for_nodes(&tcb, None, Some(ScopedNode::HostElement(host.node)), None, None);
        statements.push(render_block_statements(
            env,
            &host_scope,
            &create_host_bindings_block_guard(),
        ));
    }

    let body_str = format!("{{\n{}\n}}", statements.join("\n"));
    let func_decl_str = format!("function {name}{type_params_str}({this_param_str}) {body_str}");

    format!("/*{}*/\n{func_decl_str}", meta.id)
}

// type_check_block.ts:86 renderBlockStatements
fn render_block_statements(
    env: &dyn TcbEnvironment,
    scope: &Scope<'_, '_>,
    wrapper_expression: &str,
) -> String {
    // Note: this needs to be called first so that it can populate the prelude statements.
    let scope_statements = scope.render();
    let mut all_statements = env.get_prelude_statements();
    all_statements.extend(scope_statements);
    let statements = get_statements_block(&all_statements, false);

    // Wrap the body in an if statement. This serves two purposes:
    // 1. It allows us to distinguish between the sections of the block (e.g. host or template).
    // 2. It allows the `ts.Printer` to produce better-looking output.
    format!("if ({wrapper_expression}) {{\n{statements}\n}}")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use oxc_allocator::Allocator;
    use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipe, PropertyRead};
    use oxc_angular_compiler::ast::r3::{
        R3BoundAttribute, R3BoundEvent, R3DeferredBlock, R3Directive, R3Element, R3ForLoopBlock,
        R3HostElement, R3LetDeclaration, R3Node, R3Reference, R3Template, R3Variable,
    };
    use oxc_angular_compiler::parser::ParseTemplateOptions;
    use oxc_angular_compiler::parser::html::{HtmlParser, remove_whitespaces};
    use oxc_angular_compiler::transform::html_to_r3::{TransformOptions, html_ast_to_r3_ast};
    use oxc_span::Span;

    use super::generate_type_check_block;
    use crate::angular::core::SchemaMetadata;
    use crate::angular::t2_api::{
        Binding, BindingConsumer, BoundTarget, ConflictingHostDirectiveBinding, DirectiveOwner,
        ForeignComponentMeta, InputOrOutputKind, ReferenceDeferredTrigger, ReferenceTarget,
        ScopedNode, Target, TemplateEntity,
    };
    use crate::typecheck::api::{
        DiagnosticCategoryLabel, DirectiveRef, TcbComponentMetadata, TcbDirectiveMetadata,
        TcbEnvironment, TcbPipeMetadata, TcbReferenceMetadata, TcbTypeCheckBlockMetadata,
        TypeCheckId, TypeCheckingConfig,
    };
    use crate::typecheck::oob::{
        AttributeNode, ComponentOrDirective, ControlFlowNode, DirectiveBindingNode, DirectiveHost,
        ElementOrTemplate, OutOfBandDiagnosticCategory, OutOfBandDiagnosticRecorder,
        TwoWayConsumer,
    };
    use crate::typecheck::ops::codegen::TcbExpr;
    use crate::typecheck::schema::DomSchemaChecker;

    /// Angular's `tcb()` test configuration (`typecheck/testing/index.ts`)
    fn config() -> TypeCheckingConfig {
        TypeCheckingConfig {
            apply_template_context_guards: true,
            check_type_of_input_bindings: true,
            honor_access_modifiers_for_input_bindings: false,
            strict_null_input_bindings: true,
            check_type_of_attributes: true,
            check_type_of_dom_bindings: false,
            check_type_of_output_events: true,
            check_type_of_animation_events: true,
            check_type_of_dom_events: true,
            check_unclaimed_event_names: false,
            check_type_of_dom_references: true,
            check_type_of_non_dom_references: true,
            check_type_of_pipes: true,
            check_template_bodies: true,
            check_control_flow_bodies: true,
            always_check_schema_in_template_bodies: true,
            control_flow_preventing_content_projection: DiagnosticCategoryLabel::Warning,
            unused_standalone_imports: DiagnosticCategoryLabel::Warning,
            strict_safe_navigation_types: true,
            use_context_generic_type: true,
            strict_literal_types: true,
            enable_template_type_checker: false,
            use_inline_type_constructors: true,
            allow_signals_in_two_way_bindings: true,
            allow_dom_event_assertion: true,
            check_unknown_elements: true,
        }
    }

    struct Env(TypeCheckingConfig);

    impl TcbEnvironment for Env {
        fn config(&self) -> &TypeCheckingConfig {
            &self.0
        }
        fn reference_tcb_value(&self, r#ref: &TcbReferenceMetadata) -> TcbExpr {
            TcbExpr::new(format!("i0.{}", r#ref.name))
        }
        fn reference_external_symbol(&self, _module_name: &str, name: &str) -> TcbExpr {
            TcbExpr::new(format!("i1.{name}"))
        }
        fn pipe_inst(&self, _pipe_meta: &TcbPipeMetadata) -> TcbExpr {
            unreachable!("no pipes")
        }
        fn type_ctor_for(&self, _dir: &TcbDirectiveMetadata) -> TcbExpr {
            unreachable!("no directives")
        }
        fn get_prelude_statements(&self) -> Vec<TcbExpr> {
            Vec::new()
        }
    }

    /// A binder for templates without directives: an expression `name` (or `name(...)`) on the implicit receiver
    /// targets the template variable or `@let` declaration of that name (the first one found).
    struct Binder<'a> {
        target: Target<'a, DirectiveRef>,
        entities: Vec<(&'a str, TemplateEntity<'a>)>,
    }

    impl<'a> Binder<'a> {
        fn new(nodes: &'a [R3Node<'a>]) -> Self {
            let mut entities = Vec::new();
            Self::collect(nodes, &mut entities);
            Self { target: Target { template: Some(nodes), host: None }, entities }
        }

        fn collect(nodes: &'a [R3Node<'a>], entities: &mut Vec<(&'a str, TemplateEntity<'a>)>) {
            for node in nodes {
                match node {
                    R3Node::LetDeclaration(decl) => {
                        let decl: &'a R3LetDeclaration<'a> = decl;
                        entities.push((decl.name.as_str(), TemplateEntity::LetDeclaration(decl)));
                    }
                    R3Node::Template(template) => {
                        let template: &'a R3Template<'a> = template;
                        for variable in &template.variables {
                            entities
                                .push((variable.name.as_str(), TemplateEntity::Variable(variable)));
                        }
                        Self::collect(&template.children, entities);
                    }
                    R3Node::Element(element) => Self::collect(&element.children, entities),
                    _ => {}
                }
            }
        }

        fn entity(&self, name: &str) -> Option<TemplateEntity<'a>> {
            self.entities
                .iter()
                .find(|(entity_name, _)| *entity_name == name)
                .map(|(_, entity)| *entity)
        }
    }

    impl<'a> BoundTarget<'a, DirectiveRef> for Binder<'a> {
        fn target(&self) -> &Target<'a, DirectiveRef> {
            &self.target
        }
        fn get_directives_of_node(&self, _node: DirectiveOwner<'a>) -> Option<Vec<DirectiveRef>> {
            None
        }
        fn get_foreign_component(
            &self,
            _element: &'a R3Element<'a>,
        ) -> Option<ForeignComponentMeta> {
            None
        }
        fn get_reference_target(
            &self,
            _ref: &'a R3Reference<'a>,
        ) -> Option<ReferenceTarget<'a, DirectiveRef>> {
            None
        }
        fn get_consumer_of_binding(
            &self,
            _binding: Binding<'a>,
        ) -> Option<BindingConsumer<'a, DirectiveRef>> {
            None
        }
        fn get_expression_target(
            &self,
            expr: &AngularExpression<'a>,
        ) -> Option<TemplateEntity<'a>> {
            let read = match expr {
                AngularExpression::Call(call) => &call.receiver,
                _ => expr,
            };
            match read {
                AngularExpression::PropertyRead(read)
                    if matches!(read.receiver, AngularExpression::ImplicitReceiver(_)) =>
                {
                    self.entity(&read.name)
                }
                _ => None,
            }
        }
        fn get_definition_node_of_symbol(
            &self,
            _symbol: TemplateEntity<'a>,
        ) -> Option<ScopedNode<'a>> {
            None
        }
        fn get_nesting_level(&self, _node: ScopedNode<'a>) -> usize {
            0
        }
        fn get_entities_in_scope(&self, _node: Option<ScopedNode<'a>>) -> Vec<TemplateEntity<'a>> {
            Vec::new()
        }
        fn get_used_directives(&self) -> Vec<DirectiveRef> {
            Vec::new()
        }
        fn get_eagerly_used_directives(&self) -> Vec<DirectiveRef> {
            Vec::new()
        }
        fn get_used_pipes(&self) -> Vec<String> {
            Vec::new()
        }
        fn get_eagerly_used_pipes(&self) -> Vec<String> {
            Vec::new()
        }
        fn get_defer_blocks(&self) -> Vec<&'a R3DeferredBlock<'a>> {
            Vec::new()
        }
        fn get_deferred_trigger_target(
            &self,
            _block: &'a R3DeferredBlock<'a>,
            _trigger: ReferenceDeferredTrigger<'a>,
        ) -> Option<&'a R3Element<'a>> {
            None
        }
        fn is_deferred(&self, _node: DirectiveOwner<'a>) -> bool {
            false
        }
        fn get_defer_blocks_of_node(
            &self,
            _node: DirectiveOwner<'a>,
        ) -> Vec<&'a R3DeferredBlock<'a>> {
            Vec::new()
        }
        fn get_defer_blocks_of_pipe(
            &self,
            _ast: &'a BindingPipe<'a>,
        ) -> Vec<&'a R3DeferredBlock<'a>> {
            Vec::new()
        }
        fn referenced_directive_exists(&self, _name: &str) -> bool {
            false
        }
        fn get_conflicting_host_directive_bindings(
            &self,
            _node: DirectiveOwner<'a>,
        ) -> Option<Vec<ConflictingHostDirectiveBinding<DirectiveRef>>> {
            None
        }
    }

    struct SchemaChecker;

    impl<'a> DomSchemaChecker<'a> for SchemaChecker {
        fn check_element(&self, _: &TypeCheckId, _: &str, _: Span, _: &[SchemaMetadata], _: bool) {}
        fn check_template_element_property(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: Span,
            _: &[SchemaMetadata],
            _: bool,
        ) {
        }
        fn check_template_element_event(
            &self,
            _: &TypeCheckId,
            _: &str,
            _: &str,
            _: Span,
            _: &[SchemaMetadata],
            _: bool,
        ) {
        }
        fn check_host_element_property(
            &self,
            _: &str,
            _: &'a R3HostElement<'a>,
            _: &str,
            _: Span,
            _: &[SchemaMetadata],
        ) {
        }
    }

    /// Records the out-of-band diagnostics the tests look at, by name.
    #[derive(Default)]
    struct Oob(RefCell<Vec<String>>);

    impl<'a> OutOfBandDiagnosticRecorder<'a> for Oob {
        fn missing_reference_target(&self, _: &TypeCheckId, _: &'a R3Reference<'a>) {}
        fn missing_pipe(&self, _: &TypeCheckId, _: &'a BindingPipe<'a>, _: bool) {}
        fn deferred_pipe_used_eagerly(
            &self,
            _: &TypeCheckId,
            _: &'a BindingPipe<'a>,
            _: Option<&str>,
            _: Option<&[String]>,
        ) {
        }
        fn deferred_component_used_eagerly(
            &self,
            _: &TypeCheckId,
            _: ElementOrTemplate<'a>,
            _: &TcbDirectiveMetadata,
            _: Option<&str>,
            _: Option<&[String]>,
        ) {
        }
        fn duplicate_template_var(
            &self,
            _: &TypeCheckId,
            variable: &'a R3Variable<'a>,
            _: &'a R3Variable<'a>,
        ) {
            self.0.borrow_mut().push(format!("duplicateTemplateVar {}", variable.name));
        }
        fn suboptimal_type_inference(&self, _: &TypeCheckId, _: &[&'a R3Variable<'a>]) {}
        fn split_two_way_binding(
            &self,
            _: &TypeCheckId,
            _: &'a R3BoundAttribute<'a>,
            _: &'a R3BoundEvent<'a>,
            _: &TcbDirectiveMetadata,
            _: TwoWayConsumer<'a, '_>,
        ) {
        }
        fn missing_required_inputs(
            &self,
            _: &TypeCheckId,
            _: DirectiveHost<'a>,
            _: &str,
            _: bool,
            _: &[String],
        ) {
        }
        fn illegal_for_loop_track_access(
            &self,
            _: &TypeCheckId,
            _: &'a R3ForLoopBlock<'a>,
            _: &'a PropertyRead<'a>,
        ) {
        }
        fn inaccessible_deferred_trigger_element(
            &self,
            _: &TypeCheckId,
            _: ReferenceDeferredTrigger<'a>,
        ) {
        }
        fn control_flow_preventing_content_projection(
            &self,
            _: &TypeCheckId,
            _: OutOfBandDiagnosticCategory,
            _: ElementOrTemplate<'a>,
            _: &str,
            _: &str,
            _: ControlFlowNode<'a>,
            _: bool,
        ) {
        }
        fn illegal_write_to_let_declaration(
            &self,
            _: &TypeCheckId,
            _: &'a AngularExpression<'a>,
            target: &'a R3LetDeclaration<'a>,
        ) {
            self.0.borrow_mut().push(format!("illegalWriteToLetDeclaration {}", target.name));
        }
        fn let_used_before_definition(
            &self,
            _: &TypeCheckId,
            _: &'a PropertyRead<'a>,
            target: &'a R3LetDeclaration<'a>,
        ) {
            self.0.borrow_mut().push(format!("letUsedBeforeDefinition {}", target.name));
        }
        fn conflicting_declaration(&self, _: &TypeCheckId, current: &'a R3LetDeclaration<'a>) {
            self.0.borrow_mut().push(format!("conflictingDeclaration {}", current.name));
        }
        fn missing_named_template_dependency(&self, _: &TypeCheckId, _: ComponentOrDirective<'a>) {}
        fn incorrect_template_dependency_type(&self, _: &TypeCheckId, _: ComponentOrDirective<'a>) {
        }
        fn unclaimed_directive_binding(
            &self,
            _: &TypeCheckId,
            _: &'a R3Directive<'a>,
            _: DirectiveBindingNode<'a>,
        ) {
        }
        fn defer_implicit_trigger_missing_placeholder(
            &self,
            _: &TypeCheckId,
            _: ReferenceDeferredTrigger<'a>,
        ) {
        }
        fn defer_implicit_trigger_invalid_placeholder(
            &self,
            _: &TypeCheckId,
            _: ReferenceDeferredTrigger<'a>,
        ) {
        }
        fn form_field_unsupported_binding(&self, _: &TypeCheckId, _: AttributeNode<'a>) {}
        fn multiple_matching_components(
            &self,
            _: &TypeCheckId,
            _: &'a R3Element<'a>,
            _: &[String],
        ) {
        }
        fn conflicting_host_directive_binding(
            &self,
            _: &TypeCheckId,
            _: DirectiveHost<'a>,
            _: &str,
            _: InputOrOutputKind,
            _: &str,
            _: &[String],
        ) {
        }
    }

    /// The type-check block of `template` and the out-of-band diagnostics, as Angular's `tcb()` test helper makes it.
    fn tcb(template: &str) -> (String, Vec<String>) {
        let allocator = Allocator::default();
        let template = allocator.alloc_str(template);
        let options = ParseTemplateOptions {
            enable_block_syntax: true,
            enable_let_syntax: true,
            tokenize_expansion_forms: true,
            ..Default::default()
        };
        let html = HtmlParser::with_options(&allocator, template, "test.html", &options).parse();
        assert!(html.errors.is_empty());
        let html_nodes = remove_whitespaces(&allocator, &html.nodes, true);
        let r3 = html_ast_to_r3_ast(&allocator, template, &html_nodes, TransformOptions::default());
        assert!(r3.errors.is_empty());
        let nodes: &[R3Node<'_>] = allocator.alloc(r3.nodes);

        let env = Env(config());
        let binder = Binder::new(nodes);
        let meta = TcbTypeCheckBlockMetadata {
            id: "tcb1".to_string(),
            bound_target: &binder,
            pipes: None,
            schemas: Vec::new(),
            is_standalone: false,
            preserve_whitespaces: false,
        };
        let component = TcbComponentMetadata {
            r#ref: TcbReferenceMetadata {
                name: "Test".to_string(),
                module_name: None,
                is_local: true,
                unexported_diagnostic: None,
                key: "Test".to_string(),
                node_name_span: None,
                node_file_path: None,
            },
            type_parameters: None,
            type_arguments: None,
        };
        let oob = Oob::default();
        let text = crate::span::with_source_text(template, || {
            generate_type_check_block(&env, &component, "_tcb1", &meta, &SchemaChecker, &oob)
        });
        let diagnostics = oob.0.borrow().clone();
        (text, diagnostics)
    }

    /// The text as Angular's tests compare it: without comments (the generator writes each after a space), white
    /// space collapsed.
    fn normalize(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find(" /*") {
            out.push_str(&rest[..start]);
            let end = rest[start..].find("*/").expect("a closed comment");
            rest = &rest[start + end + 2..];
        }
        out.push_str(rest);
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn whole_block() {
        let (text, _) = tcb("{{a}}");
        assert_eq!(
            text,
            "/*tcb1*/\nfunction _tcb1(this: i0.Test) {\nif (true) {\n\"\" + (((this).a /*2,3*/) /*2,3*/);\n\n}\n}"
        );
    }

    #[test]
    fn let_declarations() {
        let (text, diagnostics) =
            tcb("@let one = 1;\n@let two = 2;\n@let sum = one + two;\n{{sum}}");
        let text = normalize(&text);
        for expected in [
            "const _t1 = (1);",
            "const _t2 = (2);",
            "const _t3 = ((_t1) + (_t2));",
            "\"\" + (_t3);",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        assert_eq!(diagnostics, Vec::<String>::new());
    }

    #[test]
    fn let_used_before_definition() {
        let (text, diagnostics) = tcb("{{value}} @let value = 1;");
        let text = normalize(&text);
        assert!(text.contains("const _t1 = (1); \"\" + (_t1 as any);"), "{text}");
        assert_eq!(diagnostics, ["letUsedBeforeDefinition value"]);
    }

    #[test]
    fn conflicting_let_declarations() {
        let (_, diagnostics) = tcb("@let value = 1; @let value = 2;");
        assert_eq!(diagnostics, ["conflictingDeclaration value"]);
    }

    #[test]
    fn deferred_blocks() {
        let (text, _) = tcb(
            "@defer {\n{{main()}}\n} @placeholder {\n{{placeholder()}}\n} @loading {\n{{loading()}}\n} @error {\n{{error()}}\n}",
        );
        let expected = "\"\" + ((this).main()); \"\" + ((this).placeholder()); \"\" + ((this).loading()); \"\" + ((this).error());";
        assert!(normalize(&text).contains(expected), "{text}");

        for trigger in ["when", "prefetch when", "hydrate when"] {
            let (text, _) = tcb(&format!(
                "@defer ({trigger} shouldShow() && isVisible) {{\n{{{{main()}}}}\n}}"
            ));
            assert!(
                normalize(&text).contains("if (((this).shouldShow()) && (((this).isVisible))) {}"),
                "{trigger}: {text}"
            );
        }
    }

    #[test]
    fn arrow_functions() {
        let (text, _) = tcb("{{() => () => (a) => () => () => a + 1}}");
        assert!(normalize(&text).contains("(() => () => a => () => () => (a) + (1)"), "{text}");
    }

    #[test]
    fn template_context_variables() {
        let (text, diagnostics) = tcb("<ng-template let-a let-a=\"b\">{{a}}</ng-template>");
        let text = normalize(&text);
        assert!(text.contains("var _t1 = null! as any;"), "{text}");
        assert!(text.contains("var _t2 = _t1.$implicit;"), "{text}");
        assert!(text.contains("var _t3 = (_t1.b);"), "{text}");
        assert!(text.contains("\"\" + (_t2);"), "{text}");
        assert_eq!(diagnostics, ["duplicateTemplateVar a"]);
    }
}
