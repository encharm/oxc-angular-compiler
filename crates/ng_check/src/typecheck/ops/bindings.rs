//! `packages/compiler/src/typecheck/ops/bindings.ts`: STUB by `tcb-core` (what `directive_constructor.rs` uses), to be
//! replaced by `tcb-ops`'s port.

use oxc_angular_compiler::ast::expression::AngularExpression;
use oxc_span::Span;

use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::api::TcbDirectiveMetadata;
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::context::Context;

/// `AST | string`, the value of a `TcbBoundAttribute`
#[derive(Debug, Clone)]
pub enum TcbBoundAttributeValue<'a> {
    Ast(&'a AngularExpression<'a>),
    String(String),
}

// bindings.ts:26 TcbBoundAttribute (stub)
#[derive(Debug, Clone)]
pub struct TcbBoundAttribute<'a> {
    pub value: TcbBoundAttributeValue<'a>,
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

// bindings.ts:83 TcbDirectiveInput (stub): `TcbDirectiveBoundInput | TcbDirectiveUnsetInput`
#[derive(Debug, Clone)]
pub enum TcbDirectiveInput<'a> {
    Binding {
        field: String,
        expression: TcbExpr,
        original_expression: TcbBoundAttributeValue<'a>,
        source_span: Span,
        is_two_way_binding: bool,
    },
    Unset {
        field: String,
    },
}

// bindings.ts:85 getBoundAttributes (stub)
pub fn get_bound_attributes<'a>(_directive: &TcbDirectiveMetadata, _node: DirectiveOwner<'a>) -> Vec<TcbBoundAttribute<'a>> {
    todo!("bindings.ts getBoundAttributes: ported by tcb-ops")
}

// bindings.ts:170 widenBinding (stub)
pub fn widen_binding(_expr: TcbExpr, _tcb: &Context<'_, '_>, _original_value: &TcbBoundAttributeValue<'_>) -> TcbExpr {
    todo!("bindings.ts widenBinding: ported by tcb-ops")
}
