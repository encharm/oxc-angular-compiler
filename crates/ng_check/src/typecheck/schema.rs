//! `packages/compiler/src/typecheck/schema.ts`

use oxc_angular_compiler::ast::r3::R3HostElement;
use oxc_span::Span;

use crate::angular::core::SchemaMetadata;
use crate::typecheck::api::TypeCheckId;

// schema.ts DomSchemaChecker: checks element and property names against the DOM schema, recording diagnostics. The
// methods take `&self` (an implementation keeps its diagnostics in a cell); spans are the parser's (`ParseSourceSpan`).
pub trait DomSchemaChecker<'a> {
    fn check_element(
        &self,
        id: &TypeCheckId,
        tag_name: &str,
        source_span_for_diagnostics: Span,
        schemas: &[SchemaMetadata],
        host_is_standalone: bool,
    );
    fn check_template_element_property(
        &self,
        id: &str,
        tag_name: &str,
        name: &str,
        span: Span,
        schemas: &[SchemaMetadata],
        host_is_standalone: bool,
    );
    fn check_template_element_event(
        &self,
        id: &TypeCheckId,
        tag_name: &str,
        event_name: &str,
        span: Span,
        schemas: &[SchemaMetadata],
        has_component: bool,
    );
    fn check_host_element_property(
        &self,
        id: &str,
        element: &'a R3HostElement<'a>,
        name: &str,
        span: Span,
        schemas: &[SchemaMetadata],
    );
}
