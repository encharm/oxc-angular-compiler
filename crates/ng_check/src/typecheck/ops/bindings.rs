//! `packages/compiler/src/typecheck/ops/bindings.ts`: STUB by `tcb-core` (what `directive_constructor.rs` uses), to be
//! replaced by `tcb-ops`'s port.

use oxc_angular_compiler::ast::expression::AngularExpression;
use oxc_span::Span;

use crate::typecheck::api::TcbDirectiveMetadata;
use crate::typecheck::oob::DirectiveHost;
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::context::Context;

/// `AST | string` (stub: tcb-ops' also has a `SyntheticField` variant)
#[derive(Debug, Clone, Copy)]
pub enum TcbBindingValue<'a> {
    String(&'a str),
    Ast(&'a AngularExpression<'a>),
}

// bindings.ts:26 TcbBoundAttribute (stub)
#[derive(Debug, Clone)]
pub struct TcbBoundAttribute<'a> {
    pub value: TcbBindingValue<'a>,
    pub source_span: Span,
    pub key_span: Option<Span>,
    pub inputs: Vec<TcbBoundAttributeInput>,
}

/// An element of `TcbBoundAttribute.inputs`
#[derive(Debug, Clone)]
pub struct TcbBoundAttributeInput {
    pub field_name: String,
    pub required: bool,
    pub is_signal: bool,
    pub transform_type: Option<String>,
    pub is_two_way_binding: bool,
}

// bindings.ts:42 TcbDirectiveBoundInput (stub)
#[derive(Debug, Clone)]
pub struct TcbDirectiveBoundInput<'a> {
    pub field: String,
    pub expression: TcbExpr,
    pub original_expression: TcbBindingValue<'a>,
    pub source_span: Span,
    pub is_two_way_binding: bool,
}

// bindings.ts:74 TcbDirectiveUnsetInput (stub)
#[derive(Debug, Clone)]
pub struct TcbDirectiveUnsetInput {
    pub field: String,
}

// bindings.ts:83 TcbDirectiveInput (stub)
#[derive(Debug, Clone)]
pub enum TcbDirectiveInput<'a> {
    Binding(TcbDirectiveBoundInput<'a>),
    Unset(TcbDirectiveUnsetInput),
}

// bindings.ts:85 getBoundAttributes (stub)
pub fn get_bound_attributes<'a>(_directive: &TcbDirectiveMetadata, _node: DirectiveHost<'a>) -> Vec<TcbBoundAttribute<'a>> {
    todo!("bindings.ts getBoundAttributes: ported by tcb-ops")
}

// bindings.ts:170 widenBinding (stub)
pub fn widen_binding(_expr: TcbExpr, _tcb: &Context<'_, '_>, _original_value: TcbBindingValue<'_>) -> TcbExpr {
    todo!("bindings.ts widenBinding: ported by tcb-ops")
}
