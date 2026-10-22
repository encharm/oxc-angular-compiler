//! `packages/compiler/src/typecheck/host_bindings.ts`: not ported yet (PORTING.md).
//!
//! STUB (ngc-glue): the types and the signature of `createHostElement` that `ngp/tcb_adapter.rs` uses. The merge
//! drops this in favour of the port. Spans are the parser's (`ParseSourceSpan`): byte offsets into the component's
//! file.

use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::r3::R3HostElement;
use oxc_span::Span;

/// `'string' | 'identifier'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticSourceNodeKind {
    String,
    Identifier,
}

// host_bindings.ts StaticSourceNode
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticSourceNode {
    pub kind: StaticSourceNodeKind,
    /// Raw source code of the node (e.g. strings include the quotes).
    pub source: String,
    /// Actual text of the node (e.g. value inside the quotes in strings).
    pub text: String,
    pub source_span: Span,
}

// host_bindings.ts SourceNode: `StaticSourceNode | {kind: 'unspecified'; sourceSpan}`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceNode {
    Static(StaticSourceNode),
    Unspecified { source_span: Span },
}

// host_bindings.ts HostObjectLiteralBinding
#[derive(Debug, Clone)]
pub struct HostObjectLiteralBinding {
    pub key: SourceNode,
    pub value: SourceNode,
    pub source_span: Span,
}

// host_bindings.ts HostListenerDecorator
#[derive(Debug, Clone)]
pub struct HostListenerDecorator {
    pub event_name: Option<SourceNode>,
    pub member_name: StaticSourceNode,
    pub member_span: Span,
    pub arguments: Vec<SourceNode>,
    pub decorator_span: Span,
}

// host_bindings.ts HostBindingDecorator
#[derive(Debug, Clone)]
pub struct HostBindingDecorator {
    pub member_name: StaticSourceNode,
    pub member_span: Span,
    pub arguments: Vec<SourceNode>,
    pub decorator_span: Span,
}

/// `'component' | 'directive'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostElementType {
    Component,
    Directive,
}

// host_bindings.ts createHostElement (stub): the element lives in `allocator`
pub fn create_host_element<'a>(
    _allocator: &'a Allocator,
    _type: HostElementType,
    _selector: Option<&str>,
    _name_span: Span,
    _host_object_literal_bindings: &[HostObjectLiteralBinding],
    _host_binding_decorators: &[HostBindingDecorator],
    _host_listener_decorators: &[HostListenerDecorator],
) -> Option<&'a R3HostElement<'a>> {
    panic!("stub: create_host_element is not ported yet")
}
