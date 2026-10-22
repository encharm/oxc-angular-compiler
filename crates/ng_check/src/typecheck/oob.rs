//! `packages/compiler/src/typecheck/oob.ts`: the diagnostics the generator finds itself ("out of band"), as opposed
//! to the ones TypeScript finds in the type-check block.

use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipe, PropertyRead};
use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Component, R3Directive, R3Element, R3ForLoopBlock,
    R3ForLoopBlockEmpty, R3HoverDeferredTrigger, R3IfBlockBranch, R3InteractionDeferredTrigger,
    R3LetDeclaration, R3Reference, R3SwitchBlockCase, R3SwitchBlockCaseGroup, R3Template, R3TextAttribute,
    R3Variable, R3ViewportDeferredTrigger,
};

use crate::typecheck::api::{TcbDirectiveMetadata, TypeCheckId};

// oob.ts:35 OutOfBandDiagnosticCategory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfBandDiagnosticCategory {
    Error,
    Warning,
}

/// `Element | Template`
#[derive(Debug, Clone, Copy)]
pub enum ElementOrTemplate<'a> {
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
}

/// `Element | Template | Component | Directive`
#[derive(Debug, Clone, Copy)]
pub enum DirectiveHost<'a> {
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
    Component(&'a R3Component<'a>),
    Directive(&'a R3Directive<'a>),
}

/// `Component | Directive`
#[derive(Debug, Clone, Copy)]
pub enum ComponentOrDirective<'a> {
    Component(&'a R3Component<'a>),
    Directive(&'a R3Directive<'a>),
}

/// `IfBlockBranch | SwitchBlockCase | ForLoopBlock | ForLoopBlockEmpty | BoundaryBlock` (Angular's `BoundaryBlock` is
/// not in the fork's AST).
///
/// `TcbControlFlowContentProjectionOp` passes a `SwitchBlockCaseGroup` (which TypeScript accepts structurally, as a
/// `ForLoopBlockEmpty`), hence that variant.
#[derive(Debug, Clone, Copy)]
pub enum ControlFlowNode<'a> {
    IfBlockBranch(&'a R3IfBlockBranch<'a>),
    SwitchBlockCase(&'a R3SwitchBlockCase<'a>),
    SwitchBlockCaseGroup(&'a R3SwitchBlockCaseGroup<'a>),
    ForLoopBlock(&'a R3ForLoopBlock<'a>),
    ForLoopBlockEmpty(&'a R3ForLoopBlockEmpty<'a>),
}

/// `HoverDeferredTrigger | InteractionDeferredTrigger | ViewportDeferredTrigger`
#[derive(Debug, Clone, Copy)]
pub enum ReferenceDeferredTrigger<'a> {
    Hover(&'a R3HoverDeferredTrigger<'a>),
    Interaction(&'a R3InteractionDeferredTrigger<'a>),
    Viewport(&'a R3ViewportDeferredTrigger<'a>),
}

/// `BoundAttribute | TextAttribute | BoundEvent`
#[derive(Debug, Clone, Copy)]
pub enum DirectiveBindingNode<'a> {
    BoundAttribute(&'a R3BoundAttribute<'a>),
    TextAttribute(&'a R3TextAttribute<'a>),
    BoundEvent(&'a R3BoundEvent<'a>),
}

/// `BoundAttribute | TextAttribute`
#[derive(Debug, Clone, Copy)]
pub enum AttributeNode<'a> {
    BoundAttribute(&'a R3BoundAttribute<'a>),
    TextAttribute(&'a R3TextAttribute<'a>),
}

/// The `Pick<TcbDirectiveMetadata, 'name' | 'isComponent' | 'ref'> | Element` of `splitTwoWayBinding`
#[derive(Debug, Clone, Copy)]
pub enum TwoWayConsumer<'a, 'b> {
    Directive(&'b TcbDirectiveMetadata),
    Element(&'a R3Element<'a>),
}

/// `'input' | 'output'`
pub use crate::angular::t2_api::InputOrOutputKind;

// oob.ts:49 OutOfBandDiagnosticRecorder. The methods take `&self` (an implementation keeps its diagnostics in a
// cell).
pub trait OutOfBandDiagnosticRecorder<'a> {
    fn missing_reference_target(&self, id: &TypeCheckId, r#ref: &'a R3Reference<'a>);
    fn missing_pipe(&self, id: &TypeCheckId, ast: &'a BindingPipe<'a>, is_standalone: bool);
    fn deferred_pipe_used_eagerly(
        &self,
        id: &TypeCheckId,
        ast: &'a BindingPipe<'a>,
        current_block_name: Option<&str>,
        declared_blocks: Option<&[String]>,
    );
    fn deferred_component_used_eagerly(
        &self,
        id: &TypeCheckId,
        element: ElementOrTemplate<'a>,
        dir_meta: &TcbDirectiveMetadata,
        current_block_name: Option<&str>,
        declared_blocks: Option<&[String]>,
    );
    fn duplicate_template_var(
        &self,
        id: &TypeCheckId,
        variable: &'a R3Variable<'a>,
        first_decl: &'a R3Variable<'a>,
    );
    fn suboptimal_type_inference(&self, id: &TypeCheckId, variables: &[&'a R3Variable<'a>]);
    fn split_two_way_binding(
        &self,
        id: &TypeCheckId,
        input: &'a R3BoundAttribute<'a>,
        output: &'a R3BoundEvent<'a>,
        input_consumer: &TcbDirectiveMetadata,
        output_consumer: TwoWayConsumer<'a, '_>,
    );
    fn missing_required_inputs(
        &self,
        id: &TypeCheckId,
        element: DirectiveHost<'a>,
        directive_name: &str,
        is_component: bool,
        input_aliases: &[String],
    );
    fn illegal_for_loop_track_access(
        &self,
        id: &TypeCheckId,
        block: &'a R3ForLoopBlock<'a>,
        access: &'a PropertyRead<'a>,
    );
    fn inaccessible_deferred_trigger_element(&self, id: &TypeCheckId, trigger: ReferenceDeferredTrigger<'a>);
    #[expect(clippy::too_many_arguments, reason = "the TypeScript method's parameters")]
    fn control_flow_preventing_content_projection(
        &self,
        id: &TypeCheckId,
        category: OutOfBandDiagnosticCategory,
        projection_node: ElementOrTemplate<'a>,
        component_name: &str,
        slot_selector: &str,
        control_flow_node: ControlFlowNode<'a>,
        preserves_whitespaces: bool,
    );
    fn illegal_write_to_let_declaration(
        &self,
        id: &TypeCheckId,
        node: &'a AngularExpression<'a>,
        target: &'a R3LetDeclaration<'a>,
    );
    fn let_used_before_definition(
        &self,
        id: &TypeCheckId,
        node: &'a PropertyRead<'a>,
        target: &'a R3LetDeclaration<'a>,
    );
    fn conflicting_declaration(&self, id: &TypeCheckId, current: &'a R3LetDeclaration<'a>);
    fn missing_named_template_dependency(&self, id: &TypeCheckId, node: ComponentOrDirective<'a>);
    fn incorrect_template_dependency_type(&self, id: &TypeCheckId, node: ComponentOrDirective<'a>);
    fn unclaimed_directive_binding(
        &self,
        id: &TypeCheckId,
        directive: &'a R3Directive<'a>,
        node: DirectiveBindingNode<'a>,
    );
    fn defer_implicit_trigger_missing_placeholder(&self, id: &TypeCheckId, trigger: ReferenceDeferredTrigger<'a>);
    fn defer_implicit_trigger_invalid_placeholder(&self, id: &TypeCheckId, trigger: ReferenceDeferredTrigger<'a>);
    fn form_field_unsupported_binding(&self, id: &TypeCheckId, node: AttributeNode<'a>);
    fn multiple_matching_components(
        &self,
        id: &TypeCheckId,
        element: &'a R3Element<'a>,
        component_names: &[String],
    );
    fn conflicting_host_directive_binding(
        &self,
        id: &TypeCheckId,
        node: DirectiveHost<'a>,
        directive_name: &str,
        kind: InputOrOutputKind,
        class_property_name: &str,
        aliases: &[String],
    );
}
