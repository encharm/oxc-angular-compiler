//! `packages/compiler/src/typecheck/ops/bindings.ts`

use std::rc::Rc;

use oxc_angular_compiler::ast::expression::{AngularExpression, BindingType};
use oxc_angular_compiler::ast::r3::{R3BoundAttribute, R3BoundEvent, R3TemplateAttr};
use oxc_span::Span;

use crate::angular::property_mapping::ClassPropertyName;
use crate::angular::t2_api::{Binding, BindingConsumer, DirectiveOwner};
use crate::typecheck::api::{TcbDirectiveMetadata, TcbInputMapping};
use crate::typecheck::oob::{AttributeNode, DirectiveHost, TwoWayConsumer};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::context::Context;
use crate::typecheck::ops::references::LocalSymbol;
use crate::typecheck::ops::signal_forms::SyntheticFieldValue;

/// `AST | string`: the value of a binding.
///
/// Angular's template bindings hold an `ASTWithSource`, which the fork's AST does not keep (it stores the expression
/// the `ASTWithSource` wraps): `Ast` is such a value. The signal forms bindings `getSyntheticFieldBoundInput` builds
/// hold new `AST` nodes around the binding's expression, which the fork's arena-allocated AST cannot share: they are
/// `SyntheticField` (`signal_forms.rs`).
#[derive(Debug, Clone, Copy)]
pub enum TcbBindingValue<'a> {
    /// `string`: a text attribute's value
    String(&'a str),
    /// The `ASTWithSource` of a template binding, by the expression it wraps
    Ast(&'a AngularExpression<'a>),
    /// A synthetic signal forms field value
    SyntheticField(SyntheticFieldValue<'a>),
}

// bindings.ts:26 TcbBoundAttribute
#[derive(Debug, Clone)]
pub struct TcbBoundAttribute<'a> {
    pub value: TcbBindingValue<'a>,
    pub source_span: Span,
    pub key_span: Option<Span>,
    pub inputs: Vec<TcbBoundAttributeInput>,
}

/// bindings.ts:30 an element of `TcbBoundAttribute.inputs`
#[derive(Debug, Clone)]
pub struct TcbBoundAttributeInput {
    pub field_name: ClassPropertyName,
    pub required: bool,
    pub is_signal: bool,
    pub transform_type: Option<String>,
    pub is_two_way_binding: bool,
}

// bindings.ts:42 TcbDirectiveBoundInput: an input binding that corresponds with a field of a directive.
#[derive(Debug, Clone)]
pub struct TcbDirectiveBoundInput<'a> {
    /// The name of a field on the directive that is set.
    pub field: String,
    /// The `TcbExpr` corresponding with the input binding expression.
    pub expression: TcbExpr,
    /// The input's original value expression.
    pub original_expression: TcbBindingValue<'a>,
    /// The source span of the full attribute binding.
    pub source_span: Span,
    /// Whether the binding is part of a two-way binding.
    pub is_two_way_binding: bool,
}

// bindings.ts:74 TcbDirectiveUnsetInput: indicates that a certain field of a directive does not have a corresponding
// input binding.
#[derive(Debug, Clone)]
pub struct TcbDirectiveUnsetInput {
    /// The name of a field on the directive for which no input binding is present.
    pub field: String,
}

// bindings.ts:83 TcbDirectiveInput
#[derive(Debug, Clone)]
pub enum TcbDirectiveInput<'a> {
    /// `type: 'binding'`
    Binding(TcbDirectiveBoundInput<'a>),
    /// `type: 'unset'`
    Unset(TcbDirectiveUnsetInput),
}

// bindings.ts:85 getBoundAttributes
pub fn get_bound_attributes<'a>(
    directive: &TcbDirectiveMetadata,
    node: DirectiveHost<'a>,
) -> Vec<TcbBoundAttribute<'a>> {
    let mut bound_inputs: Vec<TcbBoundAttribute<'a>> = Vec::new();

    let mut process_attribute = |attr: AttributeNode<'a>| {
        // Skip non-property bindings.
        if let AttributeNode::BoundAttribute(attr) = attr
            && attr.binding_type != BindingType::Property
            && attr.binding_type != BindingType::TwoWay
        {
            return;
        }

        let (name, value, source_span, key_span, is_two_way_binding) = match attr {
            AttributeNode::BoundAttribute(attr) => (
                attr.name.as_str(),
                TcbBindingValue::Ast(&attr.value),
                attr.source_span,
                Some(attr.key_span),
                attr.binding_type == BindingType::TwoWay,
            ),
            AttributeNode::TextAttribute(attr) => (
                attr.name.as_str(),
                TcbBindingValue::String(attr.value.as_str()),
                attr.source_span,
                attr.key_span,
                false,
            ),
        };

        // Skip the attribute if the directive does not have an input for it.
        let inputs = directive.inputs.get_by_binding_property_name(name);

        if let Some(inputs) = inputs {
            bound_inputs.push(TcbBoundAttribute {
                value,
                source_span,
                key_span,
                inputs: inputs
                    .iter()
                    .map(|input: &TcbInputMapping| TcbBoundAttributeInput {
                        field_name: input.input.class_property_name.clone(),
                        required: input.required,
                        transform_type: input.transform_type.clone(),
                        is_signal: input.input.is_signal,
                        is_two_way_binding,
                    })
                    .collect(),
            });
        }
    };

    match node {
        DirectiveHost::Template(node) => {
            if let Some(tag_name) = node.tag_name
                && !tag_name.is_empty()
                && is_ng_template(tag_name.as_str())
            {
                node.inputs
                    .iter()
                    .for_each(|attr| process_attribute(AttributeNode::BoundAttribute(attr)));
                node.attributes
                    .iter()
                    .for_each(|attr| process_attribute(AttributeNode::TextAttribute(attr)));
            }

            node.template_attrs.iter().for_each(|attr| {
                process_attribute(match attr {
                    R3TemplateAttr::Bound(attr) => AttributeNode::BoundAttribute(attr),
                    R3TemplateAttr::Text(attr) => AttributeNode::TextAttribute(attr),
                });
            });
        }
        DirectiveHost::Element(node) => {
            node.inputs
                .iter()
                .for_each(|attr| process_attribute(AttributeNode::BoundAttribute(attr)));
            node.attributes
                .iter()
                .for_each(|attr| process_attribute(AttributeNode::TextAttribute(attr)));
        }
        DirectiveHost::Component(node) => {
            node.inputs
                .iter()
                .for_each(|attr| process_attribute(AttributeNode::BoundAttribute(attr)));
            node.attributes
                .iter()
                .for_each(|attr| process_attribute(AttributeNode::TextAttribute(attr)));
        }
        DirectiveHost::Directive(node) => {
            node.inputs
                .iter()
                .for_each(|attr| process_attribute(AttributeNode::BoundAttribute(attr)));
            node.attributes
                .iter()
                .for_each(|attr| process_attribute(AttributeNode::TextAttribute(attr)));
        }
    }

    bound_inputs
}

// bindings.ts:137 checkSplitTwoWayBinding
///
/// # Panics
///
/// When the input's consumer is unknown (TypeScript reads `ref` of `null`).
#[expect(clippy::match_same_arms, reason = "the TypeScript's conditions")]
pub fn check_split_two_way_binding<'a>(
    input_name: &str,
    output: &'a R3BoundEvent<'a>,
    inputs: &'a [R3BoundAttribute<'a>],
    tcb: &Context<'a, '_>,
) -> bool {
    let input = inputs.iter().find(|input| input.name == input_name);
    // TypeScript compares the `ParseSourceSpan` objects, which a two-way binding's input and output share; distinct
    // bindings never have the same span.
    let Some(input) = input.filter(|input| input.source_span == output.source_span) else {
        return false;
    };
    // Input consumer should be a directive because it's claimed
    let input_consumer = tcb.bound_target.get_consumer_of_binding(Binding::BoundAttribute(input));
    let output_consumer = tcb.bound_target.get_consumer_of_binding(Binding::BoundEvent(output));
    let Some(output_consumer) = output_consumer else {
        return false;
    };
    let input_consumer = match input_consumer {
        Some(BindingConsumer::Directive(dir)) => dir,
        // An element or template has no `ref`: `inputConsumer.ref === undefined`.
        Some(BindingConsumer::Element(_) | BindingConsumer::Template(_)) => return false,
        // `null.ref` throws in TypeScript.
        None => panic!("Cannot read properties of null (reading 'ref')"),
    };
    match output_consumer {
        BindingConsumer::Template(_) => false,
        BindingConsumer::Element(element) => {
            tcb.oob_recorder.split_two_way_binding(
                &tcb.id,
                input,
                output,
                &input_consumer,
                TwoWayConsumer::Element(element),
            );
            true
        }
        // Each directive has its own `ref` object: comparing them is comparing the directives.
        BindingConsumer::Directive(output_consumer)
            if !Rc::ptr_eq(&output_consumer, &input_consumer) =>
        {
            tcb.oob_recorder.split_two_way_binding(
                &tcb.id,
                input,
                output,
                &input_consumer,
                TwoWayConsumer::Directive(&output_consumer),
            );
            true
        }
        BindingConsumer::Directive(_) => false,
    }
}

// bindings.ts:170 widenBinding: potentially widens the type of `expr` according to the type-checking configuration.
#[expect(clippy::match_same_arms, reason = "the TypeScript's cases")]
pub fn widen_binding(
    expr: TcbExpr,
    tcb: &Context<'_, '_>,
    original_value: TcbBindingValue<'_>,
) -> TcbExpr {
    if !tcb.env.config().check_type_of_input_bindings {
        // If checking the type of bindings is disabled, cast the resulting expression to 'any'
        // before the assignment.
        TcbExpr::new(format!("(({}) as any)", expr.p()))
    } else if !tcb.env.config().strict_null_input_bindings {
        let is_literal_map_or_array = match original_value {
            // An `ASTWithSource` in Angular, never a `LiteralMap` or `LiteralArray` itself.
            TcbBindingValue::Ast(_) => false,
            // A `string`, or a synthetic `Call`, `SafeCall` or `PropertyRead`.
            TcbBindingValue::String(_) | TcbBindingValue::SyntheticField(_) => false,
        };
        if is_literal_map_or_array {
            // Object literals and array literals should not be wrapped in non-null assertions as that
            // would cause literals to be prematurely widened, resulting in type errors when assigning
            // into a literal type.
            expr
        } else {
            // If strict null checks are disabled, erase `null` and `undefined` from the type by
            // wrapping the expression in a non-null assertion.
            TcbExpr::new(format!("({})!", expr.p()))
        }
    } else {
        // No widening is requested, use the expression as is.
        expr
    }
}

// ml_parser/tags.ts:56 isNgTemplate
fn is_ng_template(tag_name: &str) -> bool {
    split_ns_name(tag_name).1 == "ng-template"
}

// ml_parser/tags.ts:27 splitNsName (with `fatal`)
fn split_ns_name(element_name: &str) -> (Option<&str>, &str) {
    if !element_name.starts_with(':') {
        return (None, element_name);
    }

    let Some(colon_index) = element_name[1..].find(':').map(|i| i + 1) else {
        panic!("Unsupported format \"{element_name}\" expecting \":namespace:name\"");
    };

    (Some(&element_name[1..colon_index]), &element_name[colon_index + 1..])
}

/// `Template | Element | Component | Directive` as a `DirectiveOwner`
pub(crate) fn directive_host_as_owner(node: DirectiveHost<'_>) -> DirectiveOwner<'_> {
    match node {
        DirectiveHost::Element(n) => DirectiveOwner::Element(n),
        DirectiveHost::Template(n) => DirectiveOwner::Template(n),
        DirectiveHost::Component(n) => DirectiveOwner::Component(n),
        DirectiveHost::Directive(n) => DirectiveOwner::Directive(n),
    }
}

/// A `DirectiveOwner` as a `LocalSymbol`
pub(crate) fn directive_owner_as_local_symbol(node: DirectiveOwner<'_>) -> LocalSymbol<'_> {
    match node {
        DirectiveOwner::Element(n) => LocalSymbol::Element(n),
        DirectiveOwner::Template(n) => LocalSymbol::Template(n),
        DirectiveOwner::Component(n) => LocalSymbol::Component(n),
        DirectiveOwner::Directive(n) => LocalSymbol::Directive(n),
        DirectiveOwner::HostElement(n) => LocalSymbol::HostElement(n),
    }
}

/// What the tests of the ops of this crate build their inputs with: templates parsed by the fork, and recording
/// implementations of the generation context's traits.
#[cfg(test)]
pub(crate) mod test_support {
    use std::cell::RefCell;
    use std::rc::Rc;

    use oxc_allocator::Allocator;
    use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipe, PropertyRead};
    use oxc_angular_compiler::ast::r3::{
        R3BoundAttribute, R3BoundEvent, R3DeferredBlock, R3Directive, R3Element,
        R3ForLoopBlock, R3HostElement, R3LetDeclaration, R3Node, R3Reference, R3Variable,
    };
    use oxc_angular_compiler::parser::ParseTemplateOptions;
    use oxc_angular_compiler::parser::html::HtmlParser;
    use oxc_angular_compiler::transform::html_to_r3::{HtmlToR3Transform, TransformOptions};
    use oxc_span::Span;

    use crate::angular::core::SchemaMetadata;
    use crate::angular::property_mapping::{ClassPropertyMapping, InputOrOutput};
    use crate::angular::t2_api::{
        Binding, BindingConsumer, BoundTarget, ConflictingHostDirectiveBinding, DirectiveOwner,
        ForeignComponentMeta, MatchSource, ReferenceTarget, ScopedNode, Target, TemplateEntity,
    };
    use crate::typecheck::api::{
        DiagnosticCategoryLabel, DirectiveRef, Set, TcbDirectiveMetadata, TcbEnvironment,
        TcbInputMapping, TcbPipeMetadata, TcbReferenceMetadata, TypeCheckId, TypeCheckingConfig,
    };
    use crate::typecheck::oob::{
        AttributeNode, ComponentOrDirective, ControlFlowNode, DirectiveBindingNode, DirectiveHost,
        ElementOrTemplate, InputOrOutputKind, OutOfBandDiagnosticCategory,
        OutOfBandDiagnosticRecorder, ReferenceDeferredTrigger, TwoWayConsumer,
    };
    use crate::typecheck::ops::codegen::TcbExpr;
    use crate::typecheck::ops::context::Context;
    use crate::typecheck::schema::DomSchemaChecker;

    /// Parses `template` into the fork's template AST, in `allocator`.
    pub fn parse<'a>(allocator: &'a Allocator, template: &'a str) -> &'a [R3Node<'a>] {
        let options = ParseTemplateOptions {
            enable_block_syntax: true,
            enable_let_syntax: true,
            tokenize_expansion_forms: true,
            ..Default::default()
        };
        let html = HtmlParser::with_options(allocator, template, "test.html", &options).parse();
        assert!(html.errors.is_empty(), "{:?}", html.errors);
        let r3 = HtmlToR3Transform::new(allocator, template, TransformOptions::default())
            .transform(&html.nodes);
        assert!(r3.errors.is_empty(), "{:?}", r3.errors);
        allocator.alloc(r3.nodes)
    }

    /// The first node of `nodes`, an element
    pub fn element<'a>(nodes: &'a [R3Node<'a>]) -> &'a R3Element<'a> {
        match &nodes[0] {
            R3Node::Element(element) => element,
            node => panic!("not an element: {node:?}"),
        }
    }

    /// Angular's `BASE_CONFIG` of its type-check block tests (`ngtsc/typecheck/testing`)
    pub fn config() -> TypeCheckingConfig {
        TypeCheckingConfig {
            check_type_of_input_bindings: true,
            honor_access_modifiers_for_input_bindings: false,
            strict_null_input_bindings: true,
            check_type_of_attributes: true,
            check_type_of_dom_bindings: false,
            check_type_of_output_events: true,
            check_type_of_animation_events: true,
            check_type_of_dom_events: true,
            check_unclaimed_event_names: true,
            check_type_of_dom_references: true,
            check_type_of_non_dom_references: true,
            enable_template_type_checker: false,
            check_type_of_pipes: true,
            apply_template_context_guards: true,
            strict_safe_navigation_types: true,
            check_template_bodies: true,
            always_check_schema_in_template_bodies: true,
            control_flow_preventing_content_projection: DiagnosticCategoryLabel::Warning,
            unused_standalone_imports: DiagnosticCategoryLabel::Warning,
            use_context_generic_type: true,
            strict_literal_types: true,
            use_inline_type_constructors: true,
            allow_signals_in_two_way_bindings: true,
            allow_dom_event_assertion: true,
            check_control_flow_bodies: true,
            check_unknown_elements: true,
        }
    }

    pub fn reference(name: &str) -> TcbReferenceMetadata {
        TcbReferenceMetadata {
            name: name.to_string(),
            module_name: Some("./test".to_string()),
            is_local: false,
            unexported_diagnostic: None,
            key: format!("./test#{name}"),
            node_name_span: None,
            node_file_path: None,
        }
    }

    /// A directive with the given inputs (class property and binding names, required) and outputs
    pub fn directive(
        name: &str,
        inputs: &[(&str, &str, bool)],
        outputs: &[(&str, &str)],
    ) -> TcbDirectiveMetadata {
        TcbDirectiveMetadata {
            r#ref: reference(name),
            name: name.to_string(),
            selector: None,
            is_component: false,
            is_generic: false,
            is_structural: false,
            is_standalone: true,
            is_explicitly_deferred: false,
            deferred_blocks: None,
            preserve_whitespaces: false,
            export_as: None,
            match_source: MatchSource::Selector,
            type_parameters: None,
            inputs: ClassPropertyMapping::from_entries(inputs.iter().map(
                |(class_name, binding_name, required)| {
                    let input = TcbInputMapping {
                        input: InputOrOutput {
                            class_property_name: (*class_name).to_string(),
                            binding_property_name: (*binding_name).to_string(),
                            is_signal: false,
                        },
                        required: *required,
                        transform_type: None,
                    };
                    ((*class_name).to_string(), input)
                },
            )),
            outputs: ClassPropertyMapping::from_binding_names(outputs.iter().map(
                |(class_name, binding_name)| {
                    ((*class_name).to_string(), (*binding_name).to_string())
                },
            )),
            requires_inline_type_ctor: false,
            ng_template_guards: Vec::new(),
            has_ng_template_context_guard: false,
            has_ng_field_directive: false,
            coerced_input_fields: Set::default(),
            restricted_input_fields: Set::default(),
            string_literal_input_fields: Set::default(),
            undeclared_input_fields: Set::default(),
            public_methods: Set::default(),
            ng_content_selectors: None,
            animation_trigger_names: None,
        }
    }

    pub struct Env {
        pub config: TypeCheckingConfig,
    }

    impl TcbEnvironment for Env {
        fn config(&self) -> &TypeCheckingConfig {
            &self.config
        }
        fn reference_tcb_value(&self, r#ref: &TcbReferenceMetadata) -> TcbExpr {
            TcbExpr::new(format!("i0.{}", r#ref.name))
        }
        fn reference_external_symbol(&self, _module_name: &str, name: &str) -> TcbExpr {
            TcbExpr::new(format!("i1.{name}"))
        }
        fn pipe_inst(&self, pipe_meta: &TcbPipeMetadata) -> TcbExpr {
            TcbExpr::new(format!("_pipe_{}", pipe_meta.name))
        }
        fn type_ctor_for(&self, dir: &TcbDirectiveMetadata) -> TcbExpr {
            TcbExpr::new(format!("_ctor_{}", dir.name))
        }
        fn get_prelude_statements(&self) -> Vec<TcbExpr> {
            Vec::new()
        }
    }

    /// Records the diagnostics reported to it, as `<method>(<what identifies the arguments>)`
    #[derive(Default)]
    pub struct Recorder {
        pub calls: RefCell<Vec<String>>,
    }

    impl Recorder {
        fn record(&self, call: String) {
            self.calls.borrow_mut().push(call);
        }
    }

    fn attribute_name(node: AttributeNode<'_>) -> String {
        match node {
            AttributeNode::BoundAttribute(attr) => format!("[{}]", attr.name),
            AttributeNode::TextAttribute(attr) => attr.name.to_string(),
        }
    }

    impl<'a> OutOfBandDiagnosticRecorder<'a> for Recorder {
        fn missing_reference_target(&self, _id: &TypeCheckId, r#ref: &'a R3Reference<'a>) {
            self.record(format!("missing_reference_target({})", r#ref.name));
        }
        fn missing_pipe(&self, _id: &TypeCheckId, ast: &'a BindingPipe<'a>, _is_standalone: bool) {
            self.record(format!("missing_pipe({})", ast.name));
        }
        fn deferred_pipe_used_eagerly(
            &self,
            _id: &TypeCheckId,
            ast: &'a BindingPipe<'a>,
            _current_block_name: Option<&str>,
            _declared_blocks: Option<&[String]>,
        ) {
            self.record(format!("deferred_pipe_used_eagerly({})", ast.name));
        }
        fn deferred_component_used_eagerly(
            &self,
            _id: &TypeCheckId,
            _element: ElementOrTemplate<'a>,
            dir_meta: &TcbDirectiveMetadata,
            _current_block_name: Option<&str>,
            _declared_blocks: Option<&[String]>,
        ) {
            self.record(format!("deferred_component_used_eagerly({})", dir_meta.name));
        }
        fn duplicate_template_var(
            &self,
            _id: &TypeCheckId,
            variable: &'a R3Variable<'a>,
            _first_decl: &'a R3Variable<'a>,
        ) {
            self.record(format!("duplicate_template_var({})", variable.name));
        }
        fn suboptimal_type_inference(&self, _id: &TypeCheckId, _variables: &[&'a R3Variable<'a>]) {
            self.record("suboptimal_type_inference".to_string());
        }
        fn split_two_way_binding(
            &self,
            _id: &TypeCheckId,
            input: &'a R3BoundAttribute<'a>,
            output: &'a R3BoundEvent<'a>,
            input_consumer: &TcbDirectiveMetadata,
            output_consumer: TwoWayConsumer<'a, '_>,
        ) {
            let output_consumer = match output_consumer {
                TwoWayConsumer::Directive(dir) => dir.name.clone(),
                TwoWayConsumer::Element(element) => format!("<{}>", element.name),
            };
            self.record(format!(
                "split_two_way_binding({}, {}, {}, {output_consumer})",
                input.name, output.name, input_consumer.name
            ));
        }
        fn missing_required_inputs(
            &self,
            _id: &TypeCheckId,
            _element: DirectiveHost<'a>,
            directive_name: &str,
            _is_component: bool,
            input_aliases: &[String],
        ) {
            self.record(format!("missing_required_inputs({directive_name}, {input_aliases:?})"));
        }
        fn illegal_for_loop_track_access(
            &self,
            _id: &TypeCheckId,
            _block: &'a R3ForLoopBlock<'a>,
            access: &'a PropertyRead<'a>,
        ) {
            self.record(format!("illegal_for_loop_track_access({})", access.name));
        }
        fn inaccessible_deferred_trigger_element(
            &self,
            _id: &TypeCheckId,
            _trigger: ReferenceDeferredTrigger<'a>,
        ) {
            self.record("inaccessible_deferred_trigger_element".to_string());
        }
        fn control_flow_preventing_content_projection(
            &self,
            _id: &TypeCheckId,
            category: OutOfBandDiagnosticCategory,
            projection_node: ElementOrTemplate<'a>,
            component_name: &str,
            slot_selector: &str,
            control_flow_node: ControlFlowNode<'a>,
            preserves_whitespaces: bool,
        ) {
            let projection_node = match projection_node {
                ElementOrTemplate::Element(element) => element.name.to_string(),
                ElementOrTemplate::Template(_) => "ng-template".to_string(),
            };
            let control_flow_node = match control_flow_node {
                ControlFlowNode::IfBlockBranch(_) => "IfBlockBranch",
                ControlFlowNode::SwitchBlockCase(_) => "SwitchBlockCase",
                ControlFlowNode::SwitchBlockCaseGroup(_) => "SwitchBlockCaseGroup",
                ControlFlowNode::ForLoopBlock(_) => "ForLoopBlock",
                ControlFlowNode::ForLoopBlockEmpty(_) => "ForLoopBlockEmpty",
            };
            self.record(format!(
                "control_flow_preventing_content_projection({category:?}, {projection_node}, {component_name}, \
                 {slot_selector}, {control_flow_node}, {preserves_whitespaces})"
            ));
        }
        fn illegal_write_to_let_declaration(
            &self,
            _id: &TypeCheckId,
            _node: &'a AngularExpression<'a>,
            target: &'a R3LetDeclaration<'a>,
        ) {
            self.record(format!("illegal_write_to_let_declaration({})", target.name));
        }
        fn let_used_before_definition(
            &self,
            _id: &TypeCheckId,
            _node: &'a PropertyRead<'a>,
            target: &'a R3LetDeclaration<'a>,
        ) {
            self.record(format!("let_used_before_definition({})", target.name));
        }
        fn conflicting_declaration(&self, _id: &TypeCheckId, current: &'a R3LetDeclaration<'a>) {
            self.record(format!("conflicting_declaration({})", current.name));
        }
        fn missing_named_template_dependency(
            &self,
            _id: &TypeCheckId,
            _node: ComponentOrDirective<'a>,
        ) {
            self.record("missing_named_template_dependency".to_string());
        }
        fn incorrect_template_dependency_type(
            &self,
            _id: &TypeCheckId,
            _node: ComponentOrDirective<'a>,
        ) {
            self.record("incorrect_template_dependency_type".to_string());
        }
        fn unclaimed_directive_binding(
            &self,
            _id: &TypeCheckId,
            directive: &'a R3Directive<'a>,
            _node: DirectiveBindingNode<'a>,
        ) {
            self.record(format!("unclaimed_directive_binding({})", directive.name));
        }
        fn defer_implicit_trigger_missing_placeholder(
            &self,
            _id: &TypeCheckId,
            _trigger: ReferenceDeferredTrigger<'a>,
        ) {
            self.record("defer_implicit_trigger_missing_placeholder".to_string());
        }
        fn defer_implicit_trigger_invalid_placeholder(
            &self,
            _id: &TypeCheckId,
            _trigger: ReferenceDeferredTrigger<'a>,
        ) {
            self.record("defer_implicit_trigger_invalid_placeholder".to_string());
        }
        fn form_field_unsupported_binding(&self, _id: &TypeCheckId, node: AttributeNode<'a>) {
            self.record(format!("form_field_unsupported_binding({})", attribute_name(node)));
        }
        fn multiple_matching_components(
            &self,
            _id: &TypeCheckId,
            _element: &'a R3Element<'a>,
            component_names: &[String],
        ) {
            self.record(format!("multiple_matching_components({component_names:?})"));
        }
        fn conflicting_host_directive_binding(
            &self,
            _id: &TypeCheckId,
            _node: DirectiveHost<'a>,
            directive_name: &str,
            _kind: InputOrOutputKind,
            class_property_name: &str,
            _aliases: &[String],
        ) {
            self.record(format!(
                "conflicting_host_directive_binding({directive_name}, {class_property_name})"
            ));
        }
    }

    impl<'a> DomSchemaChecker<'a> for Recorder {
        fn check_element(
            &self,
            _id: &TypeCheckId,
            tag_name: &str,
            _source_span_for_diagnostics: Span,
            _schemas: &[SchemaMetadata],
            _host_is_standalone: bool,
        ) {
            self.record(format!("check_element({tag_name})"));
        }
        fn check_template_element_property(
            &self,
            _id: &str,
            tag_name: &str,
            name: &str,
            _span: Span,
            _schemas: &[SchemaMetadata],
            _host_is_standalone: bool,
        ) {
            self.record(format!("check_template_element_property({tag_name}, {name})"));
        }
        fn check_template_element_event(
            &self,
            _id: &TypeCheckId,
            tag_name: &str,
            event_name: &str,
            _span: Span,
            _schemas: &[SchemaMetadata],
            has_component: bool,
        ) {
            self.record(format!(
                "check_template_element_event({tag_name}, {event_name}, {has_component})"
            ));
        }
        fn check_host_element_property(
            &self,
            _id: &str,
            _element: &'a R3HostElement<'a>,
            name: &str,
            _span: Span,
            _schemas: &[SchemaMetadata],
        ) {
            self.record(format!("check_host_element_property({name})"));
        }
    }

    /// A bound target that knows the consumers of bindings (by `Binding::key`) and nothing else
    pub struct Bound<'a> {
        pub target: Target<'a, DirectiveRef>,
        pub consumers: RefCell<Vec<(usize, BindingConsumer<'a, DirectiveRef>)>>,
    }

    impl Default for Bound<'_> {
        fn default() -> Self {
            Self { target: Target { template: None, host: None }, consumers: RefCell::default() }
        }
    }

    impl<'a> BoundTarget<'a, DirectiveRef> for Bound<'a> {
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
            binding: Binding<'a>,
        ) -> Option<BindingConsumer<'a, DirectiveRef>> {
            self.consumers
                .borrow()
                .iter()
                .find(|(key, _)| *key == binding.key())
                .map(|(_, consumer)| consumer.clone())
        }
        fn get_expression_target(
            &self,
            _expr: &AngularExpression<'a>,
        ) -> Option<TemplateEntity<'a>> {
            None
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
            _trigger: crate::angular::t2_api::ReferenceDeferredTrigger<'a>,
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
            true
        }
        fn get_conflicting_host_directive_bindings(
            &self,
            _node: DirectiveOwner<'a>,
        ) -> Option<Vec<ConflictingHostDirectiveBinding<DirectiveRef>>> {
            None
        }
    }

    /// The parts of a generation context, to make one from
    pub struct Parts<'a> {
        pub env: Env,
        pub recorder: Recorder,
        pub bound: Bound<'a>,
        pub preserve_whitespaces: bool,
    }

    impl<'a> Parts<'a> {
        pub fn new(config: TypeCheckingConfig) -> Self {
            Self {
                env: Env { config },
                recorder: Recorder::default(),
                bound: Bound::default(),
                preserve_whitespaces: false,
            }
        }

        pub fn context<'b>(&'b self) -> Rc<Context<'a, 'b>> {
            Rc::new(Context::new(
                &self.env,
                &self.recorder,
                &self.recorder,
                "tcb1".to_string(),
                &self.bound,
                None,
                &[],
                true,
                self.preserve_whitespaces,
            ))
        }

        pub fn calls(&self) -> Vec<String> {
            self.recorder.calls.borrow().clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use oxc_allocator::Allocator;
    use oxc_angular_compiler::ast::r3::R3Node;

    use super::test_support::{Parts, config, directive, element, parse};
    use super::*;
    use crate::angular::t2_api::BindingConsumer;

    fn describe(attrs: &[TcbBoundAttribute<'_>]) -> Vec<String> {
        attrs
            .iter()
            .map(|attr| {
                let value = match attr.value {
                    TcbBindingValue::String(value) => format!("{value:?}"),
                    TcbBindingValue::Ast(value) => format!("ast@{}", value.source_span().start),
                    TcbBindingValue::SyntheticField(value) => {
                        format!("synthetic {}", value.field_property_name)
                    }
                };
                let inputs: Vec<String> = attr
                    .inputs
                    .iter()
                    .map(|input| {
                        format!(
                            "{}{}{}",
                            input.field_name,
                            if input.required { " required" } else { "" },
                            if input.is_two_way_binding { " two-way" } else { "" }
                        )
                    })
                    .collect();
                format!("{value} {}..{} {inputs:?}", attr.source_span.start, attr.source_span.end)
            })
            .collect()
    }

    #[test]
    fn bound_attributes_of_an_element() {
        let allocator = Allocator::default();
        let nodes =
            parse(&allocator, r#"<div [a]="x" b="lit" [attr.c]="y" [(d)]="z" [e]="w"></div>"#);
        let dir = directive(
            "Dir",
            &[("a", "a", true), ("bField", "b", false), ("c", "c", false), ("d", "d", false)],
            &[],
        );
        let attrs = get_bound_attributes(&dir, DirectiveHost::Element(element(nodes)));
        assert_eq!(
            describe(&attrs),
            [
                "ast@10 5..12 [\"a required\"]",
                "ast@41 34..43 [\"d two-way\"]",
                "\"lit\" 13..20 [\"bField\"]",
            ]
        );
    }

    #[test]
    fn bound_attributes_of_templates() {
        let allocator = Allocator::default();
        // An `ng-template`: its inputs, attributes and template attributes.
        let nodes = parse(&allocator, r#"<ng-template [a]="x" b="y"></ng-template>"#);
        let R3Node::Template(template) = &nodes[0] else { panic!() };
        let dir =
            directive("Dir", &[("a", "a", false), ("b", "b", false), ("ngIf", "ngIf", false)], &[]);
        assert_eq!(
            describe(&get_bound_attributes(&dir, DirectiveHost::Template(template))).len(),
            2
        );

        // A structural directive: only its template attributes.
        let nodes = parse(&allocator, r#"<div *ngIf="x" [a]="y"></div>"#);
        let R3Node::Template(template) = &nodes[0] else { panic!() };
        assert_eq!(
            describe(&get_bound_attributes(&dir, DirectiveHost::Template(template))),
            ["ast@12 6..13 [\"ngIf\"]"]
        );
    }

    #[test]
    fn widening() {
        let mut parts = Parts::new(config());
        let expr = || TcbExpr::new("((this).a)");
        let value = TcbBindingValue::String("x");
        assert_eq!(widen_binding(expr(), &parts.context(), value).p(), "((this).a)");
        parts.env.config.strict_null_input_bindings = false;
        assert_eq!(widen_binding(expr(), &parts.context(), value).p(), "(((this).a))!");
        parts.env.config.check_type_of_input_bindings = false;
        assert_eq!(widen_binding(expr(), &parts.context(), value).p(), "((((this).a)) as any)");
    }

    #[test]
    fn widening_keeps_literals_wrapped_as_angular_does() {
        // Angular's template binding values are `ASTWithSource`s, which are never `LiteralMap`s.
        let allocator = Allocator::default();
        let nodes = parse(&allocator, r#"<div [a]="{b: 1}"></div>"#);
        let mut parts = Parts::new(config());
        parts.env.config.strict_null_input_bindings = false;
        let value = TcbBindingValue::Ast(&element(nodes).inputs[0].value);
        assert_eq!(
            widen_binding(TcbExpr::new("{ \"b\": 1 }"), &parts.context(), value).p(),
            "({ \"b\": 1 })!"
        );
    }

    #[test]
    fn split_two_way_bindings() {
        let allocator = Allocator::default();
        let nodes = parse(&allocator, r#"<div [(value)]="x" (otherChange)="y()"></div>"#);
        let div = element(nodes);
        let input = &div.inputs[0];
        let output = &div.outputs[0];
        assert_eq!(output.name, "valueChange");
        let input_dir = Rc::new(directive("InputDir", &[("value", "value", false)], &[]));
        let output_dir = Rc::new(directive("OutputDir", &[], &[("valueChange", "valueChange")]));

        let parts = Parts::new(config());
        parts.bound.consumers.borrow_mut().extend([
            (
                Binding::BoundAttribute(input).key(),
                BindingConsumer::Directive(Rc::clone(&input_dir)),
            ),
            (Binding::BoundEvent(output).key(), BindingConsumer::Directive(Rc::clone(&input_dir))),
        ]);
        // Same directive: not split.
        assert!(!check_split_two_way_binding("value", output, &div.inputs, &parts.context()));
        // No such input.
        assert!(!check_split_two_way_binding(
            "other",
            &div.outputs[1],
            &div.inputs,
            &parts.context()
        ));

        // Another directive.
        parts.bound.consumers.borrow_mut()[1].1 = BindingConsumer::Directive(output_dir);
        assert!(check_split_two_way_binding("value", output, &div.inputs, &parts.context()));
        // The element.
        parts.bound.consumers.borrow_mut()[1].1 = BindingConsumer::Element(div);
        assert!(check_split_two_way_binding("value", output, &div.inputs, &parts.context()));
        assert_eq!(
            parts.calls(),
            [
                "split_two_way_binding(value, valueChange, InputDir, OutputDir)",
                "split_two_way_binding(value, valueChange, InputDir, <div>)",
            ]
        );

        // An input claimed by the element itself.
        parts.bound.consumers.borrow_mut()[0].1 = BindingConsumer::Element(div);
        assert!(!check_split_two_way_binding("value", output, &div.inputs, &parts.context()));
    }

    #[test]
    fn ng_template_names() {
        assert!(is_ng_template("ng-template"));
        assert!(is_ng_template(":svg:ng-template"));
        assert!(!is_ng_template("div"));
    }
}
