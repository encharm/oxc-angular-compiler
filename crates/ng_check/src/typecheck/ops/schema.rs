//! `packages/compiler/src/typecheck/ops/schema.ts`

use oxc_angular_compiler::ast::expression::BindingType;
use oxc_angular_compiler::ast::r3::R3BoundAttribute;

use crate::angular::schema::dom_element_schema_registry::DomElementSchemaRegistry;
use crate::angular::schema::element_schema_registry::ElementSchemaRegistry;
use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::api::Set;
use crate::typecheck::ops::base::{TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::selectorless::get_component_tag_name;

thread_local! {
    // schema.ts:12 REGISTRY
    static REGISTRY: DomElementSchemaRegistry = DomElementSchemaRegistry::new();
}

// schema.ts:28 TcbDomSchemaCheckerOp
/// A `TcbOp` which feeds elements and unclaimed properties to the `DomSchemaChecker`.
///
/// The DOM schema is not checked via TCB code generation. Instead, the `DomSchemaChecker` ingests
/// elements and property bindings and accumulates synthetic `ts.Diagnostic`s out-of-band. These are
/// later merged with the diagnostics generated from the TCB.
///
/// For convenience, the TCB iteration of the template is used to drive the `DomSchemaChecker` via
/// the `TcbDomSchemaCheckerOp`.
pub struct TcbDomSchemaCheckerOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    /// `Element | Component | HostElement`
    element: DirectiveOwner<'a>,
    check_element: bool,
    claimed_inputs: Option<Set<String>>,
}

impl<'a, 'b> TcbDomSchemaCheckerOp<'a, 'b> {
    // schema.ts:29 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        element: DirectiveOwner<'a>,
        check_element: bool,
        claimed_inputs: Option<Set<String>>,
    ) -> Self {
        Self { tcb, element, check_element, claimed_inputs }
    }

    // schema.ts:94 getTagName
    fn get_tag_name(node: DirectiveOwner<'a>) -> &'a str {
        match node {
            DirectiveOwner::Element(node) => node.name.as_str(),
            DirectiveOwner::Component(node) => get_component_tag_name(node),
            _ => unreachable!("an Element or Component"),
        }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbDomSchemaCheckerOp<'a, 'b> {
    // schema.ts:38 optional
    fn optional(&self) -> bool {
        false
    }

    // schema.ts:42 execute
    fn execute(&self) -> Option<TcbExpr> {
        let element = self.element;
        let is_template_element =
            matches!(element, DirectiveOwner::Element(_) | DirectiveOwner::Component(_));
        let bindings: &'a [R3BoundAttribute<'a>] = match element {
            DirectiveOwner::Element(node) => &node.inputs,
            DirectiveOwner::Component(node) => &node.inputs,
            DirectiveOwner::HostElement(node) => &node.bindings,
            DirectiveOwner::Template(_) | DirectiveOwner::Directive(_) => {
                unreachable!("an Element, Component or HostElement")
            }
        };

        if self.check_element && is_template_element {
            let start_source_span = match element {
                DirectiveOwner::Element(node) => node.start_source_span,
                DirectiveOwner::Component(node) => node.start_source_span,
                _ => unreachable!(),
            };
            self.tcb.dom_schema_checker.check_element(
                &self.tcb.id,
                Self::get_tag_name(element),
                start_source_span,
                self.tcb.schemas,
                self.tcb.host_is_standalone,
            );
        }

        // TODO(alxhub): this could be more efficient.
        for binding in bindings {
            let is_property_binding = binding.binding_type == BindingType::Property
                || binding.binding_type == BindingType::TwoWay;

            if is_property_binding
                && self
                    .claimed_inputs
                    .as_ref()
                    .is_some_and(|claimed| claimed.contains(binding.name.as_str()))
            {
                // Skip this binding as it was claimed by a directive.
                continue;
            }

            if is_property_binding && binding.name != "style" && binding.name != "class" {
                // A direct binding to a property.
                let property_name =
                    REGISTRY.with(|registry| registry.get_mapped_prop_name(&binding.name));

                if is_template_element {
                    self.tcb.dom_schema_checker.check_template_element_property(
                        &self.tcb.id,
                        Self::get_tag_name(element),
                        &property_name,
                        binding.source_span,
                        self.tcb.schemas,
                        self.tcb.host_is_standalone,
                    );
                } else {
                    let DirectiveOwner::HostElement(host) = element else { unreachable!() };
                    self.tcb.dom_schema_checker.check_host_element_property(
                        &self.tcb.id,
                        host,
                        &property_name,
                        binding.key_span,
                        self.tcb.schemas,
                    );
                }
            }
        }
        None
    }
}
