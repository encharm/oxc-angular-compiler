//! `packages/compiler-cli/preprocessor/src/dom_schema_checker.ts`: checks non-Angular elements and properties against
//! the `DomElementSchemaRegistry`.
//!
//! Diagnostics are recorded at JavaScript string (UTF-16) indices, as in TypeScript: spans (the parser's byte offsets)
//! convert through `crate::span::utf16`, so checking happens while generating, inside `span::with_source_text` for the
//! text the spans index into.

use std::cell::{Ref, RefCell};
use std::sync::LazyLock;

use oxc_angular_compiler::ast::r3::R3HostElement;
use oxc_span::Span;

use crate::angular::core::{CUSTOM_ELEMENTS_SCHEMA, NO_ERRORS_SCHEMA, SchemaMetadata};
use crate::angular::schema::dom_element_schema_registry::DomElementSchemaRegistry;
use crate::angular::schema::element_schema_registry::ElementSchemaRegistry;
use crate::ngp::diagnostic::Diagnostic;
use crate::typecheck::api::TypeCheckId;
use crate::typecheck::oob::OutOfBandDiagnosticCategory;
use crate::typecheck::schema::DomSchemaChecker;

// dom_schema_checker.ts:23 REGISTRY
static REGISTRY: LazyLock<DomElementSchemaRegistry> = LazyLock::new(DomElementSchemaRegistry::new);

// dom_schema_checker.ts:24 REMOVE_XHTML_REGEX: `/^:xhtml:/`
const REMOVE_XHTML_PREFIX: &str = ":xhtml:";

// dom_schema_checker.ts:25 UNCLAIMED_EVENT_CANDIDATE_REGEX: `/^[a-zA-Z][a-zA-Z0-9$_]*$/`
fn is_unclaimed_event_candidate(event_name: &str) -> bool {
    let mut chars = event_name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '$' || c == '_')
}

/// The span's offsets, as JavaScript string indices
fn js_offsets(span: Span) -> (u32, u32) {
    (crate::span::utf16(span.start), crate::span::utf16(span.end))
}

// dom_schema_checker.ts:31 RegistryDomSchemaChecker
#[derive(Debug, Default)]
pub struct RegistryDomSchemaChecker {
    diagnostics: RefCell<Vec<Diagnostic>>,
}

impl RegistryDomSchemaChecker {
    pub fn new() -> Self {
        Self::default()
    }

    // dom_schema_checker.ts:34 diagnostics
    pub fn diagnostics(&self) -> Ref<'_, Vec<Diagnostic>> {
        self.diagnostics.borrow()
    }

    fn push(&self, id: &str, code: u32, message: String, span: Span) {
        let (start, end) = js_offsets(span);
        self.diagnostics.borrow_mut().push(Diagnostic {
            type_check_id: id.to_string(),
            category: OutOfBandDiagnosticCategory::Error,
            code: Some(code),
            message,
            start,
            end,
        });
    }
}

impl<'a> DomSchemaChecker<'a> for RegistryDomSchemaChecker {
    // dom_schema_checker.ts:38 checkElement
    fn check_element(
        &self,
        id: &TypeCheckId,
        tag_name: &str,
        source_span_for_diagnostics: Span,
        schemas: &[SchemaMetadata],
        host_is_standalone: bool,
    ) {
        // HTML elements inside an SVG `foreignObject` are declared in the `xhtml` namespace.
        // We need to strip it before handing it over to the registry because all HTML tag names
        // in the registry are without a namespace.
        let name = tag_name.strip_prefix(REMOVE_XHTML_PREFIX).unwrap_or(tag_name);

        if !REGISTRY.has_element(name, schemas) {
            let schemas_str = format!("'{}.schemas'", if host_is_standalone { "@Component" } else { "@NgModule" });
            let mut error_msg = format!("'{name}' is not a known element:\n");
            error_msg.push_str(&format!(
                "1. If '{name}' is an Angular component, then verify that it is {}.\n",
                if host_is_standalone {
                    "included in the '@Component.imports' of this component"
                } else {
                    "part of this module"
                }
            ));
            if name.contains('-') {
                error_msg.push_str(&format!(
                    "2. If '{name}' is a Web Component then add 'CUSTOM_ELEMENTS_SCHEMA' to the {schemas_str} of this component to suppress this message."
                ));
            } else {
                error_msg.push_str(&format!(
                    "2. To allow any element add 'NO_ERRORS_SCHEMA' to the {schemas_str} of this component."
                ));
            }

            self.push(id, 8001, error_msg, source_span_for_diagnostics);
        }
    }

    // dom_schema_checker.ts:75 checkTemplateElementProperty
    fn check_template_element_property(
        &self,
        id: &str,
        tag_name: &str,
        name: &str,
        span: Span,
        schemas: &[SchemaMetadata],
        host_is_standalone: bool,
    ) {
        if !REGISTRY.has_property(tag_name, name, schemas) {
            let decorator = if host_is_standalone { "@Component" } else { "@NgModule" };
            let schemas_str = format!("'{decorator}.schemas'");
            let mut error_msg = format!("Can't bind to '{name}' since it isn't a known property of '{tag_name}'.");
            if tag_name.starts_with("ng-") {
                error_msg.push_str(&format!(
                    "\n1. If '{name}' is an Angular directive, then add 'CommonModule' to the '{decorator}.imports' of this component.\
                     \n2. To allow any property add 'NO_ERRORS_SCHEMA' to the {schemas_str} of this component."
                ));
            } else if tag_name.contains('-') {
                error_msg.push_str(&format!(
                    "\n1. If '{tag_name}' is an Angular component and it has '{name}' input, then verify that it is {}.\
                     \n2. If '{tag_name}' is a Web Component then add 'CUSTOM_ELEMENTS_SCHEMA' to the {schemas_str} of this component to suppress this message.\
                     \n3. To allow any property add 'NO_ERRORS_SCHEMA' to the {schemas_str} of this component.",
                    if host_is_standalone {
                        "included in the '@Component.imports' of this component"
                    } else {
                        "part of this module"
                    }
                ));
            }

            self.push(id, 8002, error_msg, span);
        }
    }

    // dom_schema_checker.ts:115 checkHostElementProperty
    fn check_host_element_property(
        &self,
        id: &str,
        element: &'a R3HostElement<'a>,
        name: &str,
        span: Span,
        schemas: &[SchemaMetadata],
    ) {
        for tag_name in &element.tag_names {
            if REGISTRY.has_property(tag_name, name, schemas) {
                continue;
            }

            let error_message = format!("Can't bind to '{name}' since it isn't a known property of '{tag_name}'.");
            self.push(id, 8002, error_message, span);
            break;
        }
    }

    // dom_schema_checker.ts:140 checkTemplateElementEvent
    fn check_template_element_event(
        &self,
        id: &TypeCheckId,
        tag_name: &str,
        event_name: &str,
        span: Span,
        schemas: &[SchemaMetadata],
        has_component: bool,
    ) {
        // Native DOM events are almost all lowercase and custom events conventionally use
        // dash-separated names, so only names that look like misspelled directive outputs (single
        // camelCase identifiers) are candidates for this check.
        if !is_unclaimed_event_candidate(event_name) || !event_name.chars().any(|c| c.is_ascii_uppercase()) {
            return;
        }

        // Events bubble, so a native event of any element may legitimately be observed on this
        // element, regardless of its tag. Some native events do have camelCase names (e.g. the
        // vendor-prefixed `webkitAnimationEnd`, which the schema stores in lowercase), so they have
        // to be exempted explicitly.
        if REGISTRY.is_known_event_of_any_element(event_name) {
            return;
        }

        // `CUSTOM_ELEMENTS_SCHEMA` signals that custom elements are in use, whose custom events may
        // bubble up to any element in the template. It doesn't exempt elements with a matched
        // component though: those are Angular components rather than custom elements, no matter
        // their tag name.
        if schemas.iter().any(|schema| {
            schema.name == NO_ERRORS_SCHEMA || (schema.name == CUSTOM_ELEMENTS_SCHEMA && !has_component)
        }) {
            return;
        }

        let error_msg = format!(
            "Event '{event_name}' is not emitted by any directive applied to '{tag_name}' and it isn't a known native DOM event.\
             \n1. If '{event_name}' is an output of a directive, make sure the directive is applied to the element and check the output's name for typos.\
             \n2. If you're listening to a custom event dispatched by a descendant element, dash-separated event names (e.g. 'my-event') are exempt from this check.\
             \n3. To disable this check entirely, set 'strictUnclaimedEventNames' to false or remove it from the compiler options."
        );

        self.push(id, 8030, error_msg, span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unclaimed_event_candidates() {
        assert!(is_unclaimed_event_candidate("valueChange"));
        assert!(is_unclaimed_event_candidate("a$_1"));
        assert!(!is_unclaimed_event_candidate("my-event"));
        assert!(!is_unclaimed_event_candidate("1abc"));
        assert!(!is_unclaimed_event_candidate(""));
    }
}
