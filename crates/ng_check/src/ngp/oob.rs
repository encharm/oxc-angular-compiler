//! `packages/compiler-cli/preprocessor/src/oob.ts`: records the diagnostics the type-check block generator finds
//! itself ("out of band").
//!
//! Diagnostics are recorded at JavaScript string (UTF-16) indices, as in TypeScript: spans (the parser's byte offsets)
//! convert through `crate::span::utf16`, so recording happens while generating, inside `span::with_source_text` for
//! the text the spans index into. That text is also the recorder's `source_text` (TypeScript reads a span's text
//! through its `ParseSourceFile`): set it for each template (`set_source_text`).

use std::cell::{Cell, Ref, RefCell};

use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipe, BindingType, PropertyRead};
use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Directive, R3ForLoopBlock, R3LetDeclaration, R3Reference, R3Variable,
};
use oxc_span::Span;

use crate::ngp::diagnostic::Diagnostic;
use crate::typecheck::api::{TcbDirectiveMetadata, TypeCheckId};
use crate::typecheck::oob::{
    AttributeNode, ComponentOrDirective, ControlFlowNode, DirectiveBindingNode, DirectiveHost, ElementOrTemplate,
    InputOrOutputKind, OutOfBandDiagnosticCategory, OutOfBandDiagnosticRecorder, ReferenceDeferredTrigger,
    TwoWayConsumer,
};

/// `AbsoluteSourceSpan` as `error` takes it: already JavaScript string indices (ng-analyze's)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsoluteSpan {
    pub start: u32,
    pub end: u32,
}

/// `ParseSourceSpan | AbsoluteSourceSpan`: a parser span (byte offsets, converted) or one already in JavaScript
/// string indices
#[derive(Debug, Clone, Copy)]
enum DiagnosticSpan {
    Parse(Span),
    Absolute(AbsoluteSpan),
}

impl From<Span> for DiagnosticSpan {
    fn from(span: Span) -> Self {
        Self::Parse(span)
    }
}

impl From<oxc_angular_compiler::ast::expression::AbsoluteSourceSpan> for DiagnosticSpan {
    fn from(span: oxc_angular_compiler::ast::expression::AbsoluteSourceSpan) -> Self {
        Self::Parse(Span::new(span.start, span.end))
    }
}

// oob.ts:41 OutOfBandDiagnosticRecorderImpl
#[derive(Debug, Default)]
pub struct OutOfBandDiagnosticRecorderImpl<'a> {
    diagnostics: RefCell<Vec<Diagnostic>>,
    /// The text the spans being recorded index into
    source_text: Cell<&'a str>,
}

impl<'a> OutOfBandDiagnosticRecorderImpl<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the text the spans recorded from now on index into
    pub fn set_source_text(&self, source_text: &'a str) {
        self.source_text.set(source_text);
    }

    // oob.ts:44 diagnostics
    pub fn diagnostics(&self) -> Ref<'_, Vec<Diagnostic>> {
        self.diagnostics.borrow()
    }

    // oob.ts:48 pushDiagnostic
    fn push_diagnostic(
        &self,
        type_check_id: &TypeCheckId,
        message: String,
        span: impl Into<DiagnosticSpan>,
        category: OutOfBandDiagnosticCategory,
        code: Option<u32>,
    ) {
        let (start, end) = match span.into() {
            DiagnosticSpan::Parse(span) => (crate::span::utf16(span.start), crate::span::utf16(span.end)),
            DiagnosticSpan::Absolute(span) => (span.start, span.end),
        };

        self.diagnostics.borrow_mut().push(Diagnostic {
            type_check_id: type_check_id.clone(),
            message,
            start,
            end,
            category,
            code,
        });
    }

    // oob.ts:76 error
    pub fn error(&self, id: &TypeCheckId, message: String, span: AbsoluteSpan) {
        self.push_diagnostic(
            id,
            message,
            DiagnosticSpan::Absolute(span),
            OutOfBandDiagnosticCategory::Error,
            Some(8000),
        );
    }

    /// `span.toString()`: the source text of a span
    fn span_text(&self, span: Span) -> &'a str {
        let source = self.source_text.get();
        source.get(span.start as usize..span.end as usize).unwrap_or("")
    }

    // oob.ts:458 getTagNameSpan: span covering only the tag name of a node, since highlighting the entire start tag
    // can be noisy. Falls back to the whole start tag for nodes without a name (e.g. `<ng-template>`).
    fn get_tag_name_span(&self, node: DirectiveHost<'a>) -> DiagnosticSpan {
        let (name, start_source_span) = match node {
            DirectiveHost::Element(node) => (Some(node.name.as_str()), node.start_source_span),
            DirectiveHost::Directive(node) => (Some(node.name.as_str()), node.start_source_span),
            DirectiveHost::Component(node) => (Some(node.component_name.as_str()), node.start_source_span),
            DirectiveHost::Template(node) => (None, node.start_source_span),
        };

        let Some(name) = name else {
            return start_source_span.into();
        };

        // `start.moveBy(1)` and `end.moveBy(start.offset + name.length - end.offset)`, in JavaScript string indices
        let start = crate::span::utf16(start_source_span.start) + 1;
        let end = start + name.encode_utf16().count() as u32;
        DiagnosticSpan::Absolute(AbsoluteSpan { start, end })
    }
}

impl<'a> OutOfBandDiagnosticRecorder<'a> for OutOfBandDiagnosticRecorderImpl<'a> {
    // oob.ts:80 missingReferenceTarget
    fn missing_reference_target(&self, id: &TypeCheckId, r#ref: &'a R3Reference<'a>) {
        self.push_diagnostic(
            id,
            format!("No directive found with exportAs '{}'.", r#ref.value.trim()),
            r#ref.value_span.unwrap_or(r#ref.source_span),
            OutOfBandDiagnosticCategory::Error,
            Some(8003),
        );
    }

    // oob.ts:90 missingPipe
    fn missing_pipe(&self, id: &TypeCheckId, ast: &'a BindingPipe<'a>, _is_standalone: bool) {
        self.push_diagnostic(
            id,
            format!("No pipe found with name '{}'.", ast.name),
            ast.name_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8004),
        );
    }

    // oob.ts:100 deferredPipeUsedEagerly
    fn deferred_pipe_used_eagerly(
        &self,
        id: &TypeCheckId,
        ast: &'a BindingPipe<'a>,
        current_block_name: Option<&str>,
        declared_blocks: Option<&[String]>,
    ) {
        let error_msg = if let (Some(current_block_name), Some(declared_blocks)) = (current_block_name, declared_blocks)
            && !declared_blocks.is_empty()
        {
            format!(
                "Pipe '{name}' was imported via `@Component.deferredImports` under block '{blocks}', \
                 but is used in a `@defer` block configured for '{current_block_name}'. \
                 To fix this, add '{name}' to 'deferredImports.{current_block_name}'.",
                name = ast.name,
                blocks = declared_blocks.join(", "),
            )
        } else {
            format!(
                "Pipe '{name}' was imported via `@Component.deferredImports`, \
                 but was used outside of a `@defer` block in a template. To fix this, either \
                 use the '{name}' pipe inside of a `@defer` block or import this dependency \
                 using the `@Component.imports` field.",
                name = ast.name,
            )
        };

        self.push_diagnostic(id, error_msg, ast.name_span, OutOfBandDiagnosticCategory::Error, Some(8012));
    }

    // oob.ts:123 deferredComponentUsedEagerly
    fn deferred_component_used_eagerly(
        &self,
        id: &TypeCheckId,
        element: ElementOrTemplate<'a>,
        dir_meta: &TcbDirectiveMetadata,
        current_block_name: Option<&str>,
        declared_blocks: Option<&[String]>,
    ) {
        let (element_name, start_source_span) = match element {
            ElementOrTemplate::Element(element) => (element.name.as_str(), element.start_source_span),
            ElementOrTemplate::Template(element) => {
                (element.tag_name.as_ref().map_or("ng-template", |tag_name| tag_name.as_str()), element.start_source_span)
            }
        };
        let kind = if dir_meta.is_component { "Component" } else { "Directive" };
        let name = &dir_meta.name;
        let usage = if dir_meta.is_component {
            format!("used as element '{element_name}'")
        } else {
            format!("used on element '{element_name}'")
        };

        let error_msg = if let (Some(current_block_name), Some(declared_blocks)) = (current_block_name, declared_blocks)
            && !declared_blocks.is_empty()
        {
            format!(
                "{kind} '{name}' ({usage}) was imported via `@Component.deferredImports` \
                 under block '{blocks}', but is used in a `@defer` block configured for '{current_block_name}'. \
                 To fix this, add '{name}' to 'deferredImports.{current_block_name}'.",
                blocks = declared_blocks.join(", "),
            )
        } else {
            format!(
                "{kind} '{name}' ({usage}) was imported via `@Component.deferredImports`, \
                 but was used outside of a `@defer` block in a template. To fix this, either \
                 use the '{element_name}' element inside of a `@defer` block or \
                 import '{name}' using the `@Component.imports` field."
            )
        };

        self.push_diagnostic(id, error_msg, start_source_span, OutOfBandDiagnosticCategory::Error, Some(8013));
    }

    // oob.ts:157 duplicateTemplateVar
    fn duplicate_template_var(&self, id: &TypeCheckId, variable: &'a R3Variable<'a>, _first_decl: &'a R3Variable<'a>) {
        self.push_diagnostic(
            id,
            format!(
                "Cannot redeclare variable '{}' as it was previously declared elsewhere for the same template.",
                variable.name
            ),
            variable.key_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8006),
        );
    }

    // oob.ts:167 suboptimalTypeInference
    fn suboptimal_type_inference(&self, id: &TypeCheckId, variables: &[&'a R3Variable<'a>]) {
        let mut diagnostic_var: Option<&R3Variable<'a>> = None;
        for variable in variables {
            if diagnostic_var.is_none() || variable.value.is_empty() || variable.value == "$implicit" {
                diagnostic_var = Some(variable);
            }
        }
        let Some(diagnostic_var) = diagnostic_var else { return };
        let mut var_identification = format!("'{}'", diagnostic_var.name);
        if variables.len() == 2 {
            var_identification.push_str(" (and 1 other)");
        } else if variables.len() > 2 {
            var_identification.push_str(&format!(" (and {} others)", variables.len() - 1));
        }
        self.push_diagnostic(
            id,
            format!(
                "This structural directive supports advanced type inference, but the current compiler \
                 configuration prevents its usage. The variable {var_identification} will have type \
                 'any' as a result.\n\nConsider enabling the 'strictTemplates' option in your \
                 tsconfig.json for better type inference within this template."
            ),
            diagnostic_var.key_span,
            OutOfBandDiagnosticCategory::Warning,
            Some(10002), // SUGGEST_SUBOPTIMAL_TYPE_INFERENCE
        );
    }

    // oob.ts:193 splitTwoWayBinding
    fn split_two_way_binding(
        &self,
        id: &TypeCheckId,
        input: &'a R3BoundAttribute<'a>,
        _output: &'a R3BoundEvent<'a>,
        _input_consumer: &TcbDirectiveMetadata,
        _output_consumer: TwoWayConsumer<'a, '_>,
    ) {
        self.push_diagnostic(
            id,
            format!(
                "The property and event halves of the two-way binding '{}' are not bound \
                 to the same target.\n            \
                 Find more at https://angular.dev/guide/templates/two-way-binding",
                input.name
            ),
            input.key_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8007),
        );
    }

    // oob.ts:205 missingRequiredInputs
    fn missing_required_inputs(
        &self,
        id: &TypeCheckId,
        element: DirectiveHost<'a>,
        directive_name: &str,
        is_component: bool,
        input_aliases: &[String],
    ) {
        let alias_string = input_aliases.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
        let kind = if is_component { "component" } else { "directive" };

        let span = self.get_tag_name_span(element);
        self.push_diagnostic(
            id,
            format!(
                "Required input{} {alias_string} from {kind} {directive_name} must be specified.",
                if input_aliases.len() == 1 { "" } else { "s" },
            ),
            span,
            OutOfBandDiagnosticCategory::Error,
            Some(8008),
        );
    }

    // oob.ts:224 illegalForLoopTrackAccess
    fn illegal_for_loop_track_access(
        &self,
        id: &TypeCheckId,
        block: &'a R3ForLoopBlock<'a>,
        access: &'a PropertyRead<'a>,
    ) {
        let message_vars = std::iter::once(&block.item)
            .chain(block.context_variables.iter().filter(|v| v.value == "$index"))
            .map(|v| format!("'{}'", v.name))
            .collect::<Vec<_>>()
            .join(", ");
        self.push_diagnostic(
            id,
            format!(
                "Cannot access '{}' inside of a track expression. \
                 Only {message_vars} and properties on the containing component are available to this expression.",
                access.name
            ),
            access.source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8009),
        );
    }

    // oob.ts:241 inaccessibleDeferredTriggerElement
    fn inaccessible_deferred_trigger_element(&self, id: &TypeCheckId, trigger: ReferenceDeferredTrigger<'a>) {
        let (reference, source_span) = trigger_reference_and_span(trigger);
        match reference {
            None => self.push_diagnostic(
                id,
                "Trigger cannot find reference. Make sure that the @defer block has a @placeholder with at least one root element node."
                    .to_string(),
                source_span,
                OutOfBandDiagnosticCategory::Error,
                Some(8010),
            ),
            Some(reference) => self.push_diagnostic(
                id,
                format!(
                    "Trigger cannot find reference \"{reference}\".\nCheck that an element \
                     with #{reference} exists in the same template and it's accessible \
                     from the @defer block.\nDeferred blocks can only access triggers in same view, \
                     a parent embedded view or the root view of the @placeholder block."
                ),
                source_span,
                OutOfBandDiagnosticCategory::Error,
                Some(8010),
            ),
        }
    }

    // oob.ts:270 controlFlowPreventingContentProjection
    fn control_flow_preventing_content_projection(
        &self,
        id: &TypeCheckId,
        category: OutOfBandDiagnosticCategory,
        projection_node: ElementOrTemplate<'a>,
        component_name: &str,
        slot_selector: &str,
        control_flow_node: ControlFlowNode<'a>,
        preserves_whitespaces: bool,
    ) {
        // (`BoundaryBlock` is not in the fork's AST.)
        let name_span = match control_flow_node {
            ControlFlowNode::IfBlockBranch(node) => node.name_span,
            ControlFlowNode::SwitchBlockCase(node) => node.name_span,
            ControlFlowNode::ForLoopBlock(node) => node.name_span,
            ControlFlowNode::ForLoopBlockEmpty(node) => node.name_span,
        };
        let block_name = match self.span_text(name_span).trim() {
            "" => "block",
            name => name,
        };
        let mut msg = format!(
            "Node matches the \"{slot_selector}\" slot of the \"{component_name}\" component, but will not be projected into the specific slot because the surrounding {block_name} has more than one node at its root. To project the node in the right slot, you can:\n\n\
             1. Wrap the content of the {block_name} block in an <ng-container/> that matches the \"{slot_selector}\" selector.\n\
             2. Split the content of the {block_name} block across multiple {block_name} blocks such that each one only has a single projectable node at its root.\n\
             3. Remove all content from the {block_name} block, except for the node being projected.\n"
        );
        if preserves_whitespaces {
            msg.push_str(
                "\nNote: the host component has `preserveWhitespaces: true` which may cause whitespace to affect content projection.\n",
            );
        }
        msg.push_str(
            "\nThis check can be disabled using the `extendedDiagnostics.checks.controlFlowPreventingContentProjection = \"suppress\"` compiler option.",
        );
        let start_source_span = match projection_node {
            ElementOrTemplate::Element(node) => node.start_source_span,
            ElementOrTemplate::Template(node) => node.start_source_span,
        };
        self.push_diagnostic(id, msg, start_source_span, category, Some(8011));
    }

    // oob.ts:299 illegalWriteToLetDeclaration
    fn illegal_write_to_let_declaration(
        &self,
        id: &TypeCheckId,
        node: &'a AngularExpression<'a>,
        target: &'a R3LetDeclaration<'a>,
    ) {
        self.push_diagnostic(
            id,
            format!("Cannot assign to @let declaration '{}'.", target.name),
            node.source_span(),
            OutOfBandDiagnosticCategory::Error,
            Some(8015),
        );
    }

    // oob.ts:309 letUsedBeforeDefinition
    fn let_used_before_definition(
        &self,
        id: &TypeCheckId,
        node: &'a PropertyRead<'a>,
        target: &'a R3LetDeclaration<'a>,
    ) {
        self.push_diagnostic(
            id,
            format!("Cannot read @let declaration '{}' before it has been defined.", target.name),
            node.source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8016),
        );
    }

    // oob.ts:319 conflictingDeclaration
    fn conflicting_declaration(&self, id: &TypeCheckId, decl: &'a R3LetDeclaration<'a>) {
        self.push_diagnostic(
            id,
            format!(
                "Cannot declare @let called '{}' as there is another symbol in the template with the same name.",
                decl.name
            ),
            decl.source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8017),
        );
    }

    // oob.ts:329 missingNamedTemplateDependency
    fn missing_named_template_dependency(&self, id: &TypeCheckId, node: ComponentOrDirective<'a>) {
        let (name, start_source_span) = match node {
            ComponentOrDirective::Directive(node) => (node.name.as_str(), node.start_source_span),
            ComponentOrDirective::Component(node) => (node.component_name.as_str(), node.start_source_span),
        };
        self.push_diagnostic(
            id,
            format!(
                "Cannot find name \"{name}\". \
                 Selectorless references are only supported to classes or non-type import statements."
            ),
            start_source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(2024), // MISSING_NAMED_TEMPLATE_DEPENDENCY
        );
    }

    // oob.ts:340 incorrectTemplateDependencyType
    fn incorrect_template_dependency_type(&self, id: &TypeCheckId, node: ComponentOrDirective<'a>) {
        let start_source_span = match node {
            ComponentOrDirective::Directive(node) => node.start_source_span,
            ComponentOrDirective::Component(node) => node.start_source_span,
        };
        self.push_diagnostic(
            id,
            "Incorrect reference type. Type must be a standalone @Component or @Directive.".to_string(),
            start_source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(2025), // INCORRECT_NAMED_TEMPLATE_DEPENDENCY_TYPE
        );
    }

    // oob.ts:350 unclaimedDirectiveBinding
    fn unclaimed_directive_binding(
        &self,
        id: &TypeCheckId,
        directive: &'a R3Directive<'a>,
        node: DirectiveBindingNode<'a>,
    ) {
        let (name, span) = match node {
            DirectiveBindingNode::BoundAttribute(node) => (node.name.as_str(), node.key_span),
            DirectiveBindingNode::TextAttribute(node) => (node.name.as_str(), node.key_span.unwrap_or(node.source_span)),
            DirectiveBindingNode::BoundEvent(node) => (node.name.as_str(), node.key_span),
        };
        self.push_diagnostic(
            id,
            format!(
                "Directive {} does not have an input or output named \"{name}\". \
                 Bindings to directives must target existing inputs or outputs.",
                directive.name
            ),
            span,
            OutOfBandDiagnosticCategory::Error,
            Some(8018),
        );
    }

    // oob.ts:365 deferImplicitTriggerMissingPlaceholder
    fn defer_implicit_trigger_missing_placeholder(&self, id: &TypeCheckId, trigger: ReferenceDeferredTrigger<'a>) {
        let (_, source_span) = trigger_reference_and_span(trigger);
        self.push_diagnostic(
            id,
            "Trigger with no target can only be placed on an @defer that has a @placeholder block".to_string(),
            source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8019),
        );
    }

    // oob.ts:381 deferImplicitTriggerInvalidPlaceholder
    fn defer_implicit_trigger_invalid_placeholder(&self, id: &TypeCheckId, trigger: ReferenceDeferredTrigger<'a>) {
        let (_, source_span) = trigger_reference_and_span(trigger);
        self.push_diagnostic(
            id,
            "Trigger with no target can only be placed on an @defer that has a \
             @placeholder block with exactly one root element node"
                .to_string(),
            source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8020), // DEFER_IMPLICIT_TRIGGER_INVALID_PLACEHOLDER
        );
    }

    // oob.ts:398 formFieldUnsupportedBinding
    fn form_field_unsupported_binding(&self, id: &TypeCheckId, node: AttributeNode<'a>) {
        let (message, source_span) = match node {
            AttributeNode::BoundAttribute(node) => {
                let name = match node.binding_type {
                    BindingType::Property => format!("[{}]", node.name),
                    BindingType::Attribute => format!("[attr.{}]", node.name),
                    // We shouldn't hit this, but we have this logic as a fallback.
                    _ => node.name.to_string(),
                };
                (
                    format!("Binding to '{name}' is not allowed on nodes using the '[formField]' directive"),
                    node.source_span,
                )
            }
            AttributeNode::TextAttribute(node) => (
                format!(
                    "Setting the '{}' attribute is not allowed on nodes using the '[formField]' directive",
                    node.name
                ),
                node.source_span,
            ),
        };

        self.push_diagnostic(id, message, source_span, OutOfBandDiagnosticCategory::Error, Some(8022)); // FORM_FIELD_UNSUPPORTED_BINDING
    }

    // oob.ts:421 multipleMatchingComponents
    fn multiple_matching_components(
        &self,
        id: &TypeCheckId,
        element: &'a oxc_angular_compiler::ast::r3::R3Element<'a>,
        component_names: &[String],
    ) {
        let names = component_names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
        self.push_diagnostic(
            id,
            format!("Multiple components match node with tagname {}: {names}.", element.name),
            element.start_source_span,
            OutOfBandDiagnosticCategory::Error,
            Some(8023), // MULTIPLE_MATCHING_COMPONENTS
        );
    }

    // oob.ts:432 conflictingHostDirectiveBinding
    fn conflicting_host_directive_binding(
        &self,
        id: &TypeCheckId,
        node: DirectiveHost<'a>,
        directive_name: &str,
        kind: InputOrOutputKind,
        class_property_name: &str,
        aliases: &[String],
    ) {
        let (kind_title, kind) = match kind {
            InputOrOutputKind::Input => ("Input", "input"),
            InputOrOutputKind::Output => ("Output", "output"),
        };
        let message = format!(
            "{kind_title} declared in {directive_name}.{class_property_name} \
             is exposed under the following conflicting names: {}. \
             An {kind} can only be exposed under a single name.",
            aliases.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ")
        );

        let span = self.get_tag_name_span(node);
        self.push_diagnostic(id, message, span, OutOfBandDiagnosticCategory::Error, Some(8024)); // CONFLICTING_HOST_DIRECTIVE_BINDING
    }
}

/// `trigger.reference` and `trigger.sourceSpan`
fn trigger_reference_and_span(trigger: ReferenceDeferredTrigger<'_>) -> (Option<&str>, Span) {
    match trigger {
        ReferenceDeferredTrigger::Hover(trigger) => (trigger.reference.as_deref(), trigger.source_span),
        ReferenceDeferredTrigger::Interaction(trigger) => (trigger.reference.as_deref(), trigger.source_span),
        ReferenceDeferredTrigger::Viewport(trigger) => (trigger.reference.as_deref(), trigger.source_span),
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_angular_compiler::ast::r3::R3Node;

    use super::*;
    use crate::ngp::compiler_utils::{TemplateParseOptions, parse_template};

    fn parse<'a>(allocator: &'a Allocator, template: &str) -> &'a [R3Node<'a>] {
        let template = allocator.alloc_str(template);
        let options = TemplateParseOptions { preserve_whitespaces: Some(true), ..TemplateParseOptions::default() };
        let parsed = parse_template(allocator, template, "t.html", &options);
        assert!(parsed.errors.is_none(), "{:?}", parsed.errors);
        parsed.nodes
    }

    #[test]
    fn required_inputs_point_at_the_tag_name_in_utf16() {
        let allocator = Allocator::default();
        let template = "é<my-cmp></my-cmp>";
        let nodes = parse(&allocator, template);
        let R3Node::Element(element) = &nodes[1] else { panic!() };
        let recorder = OutOfBandDiagnosticRecorderImpl::new();
        recorder.set_source_text(template);
        crate::span::with_source_text(template, || {
            recorder.missing_required_inputs(
                &"tcb1".to_string(),
                DirectiveHost::Element(element),
                "MyCmp",
                true,
                &["a".to_string(), "b".to_string()],
            );
        });
        let diagnostics = recorder.diagnostics();
        assert_eq!(
            diagnostics[0],
            Diagnostic {
                type_check_id: "tcb1".to_string(),
                category: OutOfBandDiagnosticCategory::Error,
                code: Some(8008),
                message: "Required inputs 'a', 'b' from component MyCmp must be specified.".to_string(),
                // `é` is one UTF-16 unit (two bytes); `<` is skipped
                start: 2,
                end: 8,
            }
        );
    }

    #[test]
    fn control_flow_names_the_block_by_its_source_text() {
        let allocator = Allocator::default();
        let template = "@if (a) {<span></span><b></b>}";
        let nodes = parse(&allocator, template);
        let R3Node::IfBlock(block) = &nodes[0] else { panic!("{nodes:?}") };
        let branch = &block.branches[0];
        let R3Node::Element(element) = &branch.children[0] else { panic!() };
        let recorder = OutOfBandDiagnosticRecorderImpl::new();
        recorder.set_source_text(template);
        crate::span::with_source_text(template, || {
            recorder.control_flow_preventing_content_projection(
                &"tcb1".to_string(),
                OutOfBandDiagnosticCategory::Warning,
                ElementOrTemplate::Element(element),
                "Cmp",
                "span",
                ControlFlowNode::IfBlockBranch(branch),
                false,
            );
        });
        let diagnostics = recorder.diagnostics();
        assert!(
            diagnostics[0].message.starts_with(
                "Node matches the \"span\" slot of the \"Cmp\" component, but will not be projected into the specific slot because the surrounding @if has more than one node at its root."
            ),
            "{}",
            diagnostics[0].message
        );
        assert!(diagnostics[0].message.ends_with("compiler option."));
    }

    #[test]
    fn errors_keep_their_offsets() {
        let recorder = OutOfBandDiagnosticRecorderImpl::new();
        recorder.error(&"tcb2".to_string(), "Error generating TCB for C: x".to_string(), AbsoluteSpan { start: 3, end: 9 });
        let diagnostics = recorder.diagnostics();
        assert_eq!((diagnostics[0].start, diagnostics[0].end, diagnostics[0].code), (3, 9, Some(8000)));
    }
}
