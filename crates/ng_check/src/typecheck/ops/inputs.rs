//! `packages/compiler/src/typecheck/ops/inputs.ts`

use crate::angular::schema::element_schema_registry::ElementSchemaRegistry;
use oxc_angular_compiler::ast::expression::BindingType;
use oxc_angular_compiler::ast::r3::R3BoundAttribute;

use crate::angular::property_mapping::{BindingPropertyName, ClassPropertyName};
use crate::angular::schema::dom_element_schema_registry::DomElementSchemaRegistry;
use crate::typecheck::api::{DirectiveRef, Set};
use crate::typecheck::oob::DirectiveHost;
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::bindings::{
    TcbBindingValue, directive_host_as_owner, directive_owner_as_local_symbol,
    get_bound_attributes, widen_binding,
};
use crate::typecheck::ops::codegen::{TcbExpr, declare_variable};
use crate::typecheck::ops::expression::{tcb_expression, unwrap_writable_signal};
use crate::typecheck::ops::references::LocalSymbol;
use crate::typecheck::ops::signal_forms::{
    CUSTOM_FORM_CONTROL_BANNED_INPUT_FIELDS, CustomFormControlType,
    check_unsupported_field_bindings, expand_bound_attributes_for_field,
};

thread_local! {
    // inputs.ts:19 REGISTRY
    static REGISTRY: DomElementSchemaRegistry = DomElementSchemaRegistry::new();
}

// render3/r3_identifiers.ts:505 Identifiers.InputSignalBrandWriteType
const INPUT_SIGNAL_BRAND_WRITE_TYPE: (&str, &str) =
    ("@angular/core", "ɵINPUT_SIGNAL_BRAND_WRITE_TYPE");

// inputs.ts:34 translateInput: translates the given attribute binding to a `ts.Expression`.
pub fn translate_input<'a, 'b>(
    value: TcbBindingValue<'a>,
    tcb: &TcbRef<'a, 'b>,
    scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    match value {
        TcbBindingValue::String(value) => {
            // For regular attributes with a static string value, use the represented string literal.
            TcbExpr::new(TcbExpr::quote_and_escape(value))
        }
        // Produce an expression representing the value of the binding.
        TcbBindingValue::Ast(value) => tcb_expression(value, tcb, scope),
        TcbBindingValue::SyntheticField(value) => value.translate(tcb, scope),
    }
}

// inputs.ts:50 TcbDirectiveInputsOp: a `TcbOp` which generates code to check input bindings on an element that
// correspond with the members of a directive.
//
// Executing this operation returns nothing.
pub struct TcbDirectiveInputsOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    node: DirectiveHost<'a>,
    dir: DirectiveRef,
    is_form_control: bool,
    custom_form_control_type: Option<CustomFormControlType>,
}

impl<'a, 'b> TcbDirectiveInputsOp<'a, 'b> {
    // inputs.ts:51 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        node: DirectiveHost<'a>,
        dir: DirectiveRef,
        is_form_control: bool,
        custom_form_control_type: Option<CustomFormControlType>,
    ) -> Self {
        Self { tcb, scope, node, dir, is_form_control, custom_form_control_type }
    }

    // inputs.ts:211 checkRequiredInputs
    fn check_required_inputs(&self, seen_required_inputs: &Set<ClassPropertyName>) {
        let mut missing: Vec<BindingPropertyName> = Vec::new();

        for input in self.dir.inputs.iter() {
            if input.required && !seen_required_inputs.contains(&input.input.class_property_name) {
                missing.push(input.input.binding_property_name.clone());
            }
        }

        if !missing.is_empty() {
            self.tcb.oob_recorder.missing_required_inputs(
                &self.tcb.id,
                self.node,
                &self.dir.name,
                self.dir.is_component,
                &missing,
            );
        }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbDirectiveInputsOp<'a, 'b> {
    // inputs.ts:62 optional
    fn optional(&self) -> bool {
        false
    }

    // inputs.ts:66 execute
    fn execute(&self) -> Option<TcbExpr> {
        let mut dir_id: Option<TcbExpr> = None;

        // TODO(joost): report duplicate properties
        let mut seen_required_inputs: Set<ClassPropertyName> = Set::default();
        let mut bound_attrs = get_bound_attributes(&self.dir, self.node);

        if self.custom_form_control_type.is_some() {
            check_unsupported_field_bindings(
                directive_host_as_owner(self.node),
                &CUSTOM_FORM_CONTROL_BANNED_INPUT_FIELDS,
                &self.tcb,
            );
        }

        if self.custom_form_control_type.is_some() || self.is_form_control {
            let additional_bindings = expand_bound_attributes_for_field(
                &self.dir,
                self.node,
                self.custom_form_control_type,
            );

            if let Some(additional_bindings) = additional_bindings {
                bound_attrs.extend(additional_bindings);
            }
        }

        let config = self.tcb.env.config();
        for attr in &bound_attrs {
            // For bound inputs, the property is assigned the binding expression.
            let mut assignment = widen_binding(
                translate_input(attr.value, &self.tcb, &self.scope),
                &self.tcb,
                attr.value,
            );
            assignment = assignment.wrap_for_type_checker();

            for input in &attr.inputs {
                let field_name = &input.field_name;
                let mut target: TcbExpr;

                if input.required {
                    seen_required_inputs.insert(field_name.clone());
                }

                // Note: There is no special logic for transforms/coercion with signal inputs.
                // For signal inputs, a `transformType` will never be set as we do not capture
                // the transform in the compiler metadata. Signal inputs incorporate their
                // transform write type into their member type, and we extract it below when
                // setting the `WriteT` of such `InputSignalWithTransform<_, WriteT>`.

                if self.dir.coerced_input_fields.contains(field_name) {
                    let r#type = if let Some(transform_type) = &input.transform_type {
                        TcbExpr::new(transform_type.clone())
                    } else {
                        // The input has a coercion declaration which should be used instead of assigning the
                        // expression into the input field directly. To achieve this, a variable is declared
                        // with a type of `typeof Directive.ngAcceptInputType_fieldName` which is then used as
                        // target of the assignment.
                        let dir_type_ref = self.tcb.env.reference_tcb_value(&self.dir.r#ref);
                        let prop_name = format!("ngAcceptInputType_{field_name}");
                        let access = if is_unsafe_object_key(field_name) {
                            format!("[{}]", TcbExpr::quote_and_escape(&prop_name))
                        } else {
                            format!(".{prop_name}")
                        };
                        TcbExpr::new(format!("typeof {}{access}", dir_type_ref.p()))
                    };

                    let id = TcbExpr::new(self.tcb.allocate_id());
                    self.scope.add_statement(declare_variable(&id, r#type));
                    target = id;
                } else if self.dir.undeclared_input_fields.contains(field_name) {
                    // If no coercion declaration is present nor is the field declared (i.e. the input is
                    // declared in a `@Directive` or `@Component` decorator's `inputs` property) there is no
                    // assignment target available, so this field is skipped.
                    continue;
                } else if !config.honor_access_modifiers_for_input_bindings
                    && self.dir.restricted_input_fields.contains(field_name)
                {
                    // If strict checking of access modifiers is disabled and the field is restricted
                    // (i.e. private/protected/readonly), generate an assignment into a temporary variable
                    // that has the type of the field. This achieves type-checking but circumvents the access
                    // modifiers.
                    let dir_id = dir_id.get_or_insert_with(|| self.resolve_dir());

                    let id = TcbExpr::new(self.tcb.allocate_id());
                    let r#type = TcbExpr::new(format!(
                        "(typeof {})[{}]",
                        dir_id.p(),
                        TcbExpr::quote_and_escape(field_name)
                    ));
                    let temp = declare_variable(&id, r#type);
                    self.scope.add_statement(temp);
                    target = id;
                } else {
                    let dir_id = dir_id.get_or_insert_with(|| self.resolve_dir());

                    // To get errors assign directly to the fields on the instance, using property access
                    // when possible. String literal fields may not be valid JS identifiers so we use
                    // literal element access instead for those cases.
                    target = if self.dir.string_literal_input_fields.contains(field_name) {
                        TcbExpr::new(format!(
                            "{}[{}]",
                            dir_id.p(),
                            TcbExpr::quote_and_escape(field_name)
                        ))
                    } else {
                        TcbExpr::new(format!("{}.{field_name}", dir_id.p()))
                    };
                }

                // For signal inputs, we unwrap the target `InputSignal`. Note that
                // we intentionally do the following things:
                //   1. keep the direct access to `dir.[field]` so that modifiers are honored.
                //   2. follow the existing pattern where multiple targets assign a single expression.
                //      This is a significant requirement for language service auto-completion.
                if input.is_signal {
                    let input_signal_brand_write_symbol = self.tcb.env.reference_external_symbol(
                        INPUT_SIGNAL_BRAND_WRITE_TYPE.0,
                        INPUT_SIGNAL_BRAND_WRITE_TYPE.1,
                    );

                    target = TcbExpr::new(format!(
                        "{}[{}]",
                        target.p(),
                        input_signal_brand_write_symbol.p()
                    ));
                }

                if let Some(key_span) = attr.key_span {
                    target = target.add_parse_span_info(key_span);
                }

                // Two-way bindings accept `T | WritableSignal<T>` so we have to unwrap the value.
                if input.is_two_way_binding && config.allow_signals_in_two_way_bindings {
                    assignment = unwrap_writable_signal(assignment, &self.tcb);
                }

                // Finally the assignment is extended by assigning it into the target expression.
                assignment = TcbExpr::new(format!("{} = {}", target.p(), assignment.p()));
            }

            assignment = assignment.add_parse_span_info(attr.source_span);

            // Ignore diagnostics for text attributes if configured to do so.
            if !config.check_type_of_attributes && matches!(attr.value, TcbBindingValue::String(_))
            {
                assignment = assignment.mark_ignore_diagnostics();
            }

            self.scope.add_statement(assignment);
        }

        self.check_required_inputs(&seen_required_inputs);

        None
    }
}

impl TcbDirectiveInputsOp<'_, '_> {
    /// `this.scope.resolve(this.node, this.dir)`
    fn resolve_dir(&self) -> TcbExpr {
        self.scope.resolve(
            directive_owner_as_local_symbol(directive_host_as_owner(self.node)),
            Some(&self.dir),
        )
    }
}

// inputs.ts:242 TcbUnclaimedInputsOp: a `TcbOp` which generates code to check "unclaimed inputs" - bindings on an
// element which were not attributed to any directive or component, and are instead processed against the HTML element
// itself.
//
// Currently, only the expressions of these bindings are checked. The targets of the bindings are
// checked against the DOM schema via a `TcbDomSchemaCheckerOp`.
//
// Executing this operation returns nothing.
pub struct TcbUnclaimedInputsOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    inputs: &'a [R3BoundAttribute<'a>],
    target: LocalSymbol<'a>,
    claimed_inputs: Option<Set<String>>,
}

impl<'a, 'b> TcbUnclaimedInputsOp<'a, 'b> {
    // inputs.ts:243 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        inputs: &'a [R3BoundAttribute<'a>],
        target: LocalSymbol<'a>,
        claimed_inputs: Option<Set<String>>,
    ) -> Self {
        Self { tcb, scope, inputs, target, claimed_inputs }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbUnclaimedInputsOp<'a, 'b> {
    // inputs.ts:253 optional
    fn optional(&self) -> bool {
        false
    }

    // inputs.ts:257 execute
    fn execute(&self) -> Option<TcbExpr> {
        // `this.inputs` contains only those bindings not matched by any directive. These bindings go to
        // the element itself.
        let mut el_id: Option<TcbExpr> = None;

        // TODO(alxhub): this could be more efficient.
        for binding in self.inputs {
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

            let expr = widen_binding(
                tcb_expression(&binding.value, &self.tcb, &self.scope),
                &self.tcb,
                TcbBindingValue::Ast(&binding.value),
            );

            if self.tcb.env.config().check_type_of_dom_bindings && is_property_binding {
                if binding.name != "style" && binding.name != "class" {
                    let el_id = el_id.get_or_insert_with(|| self.scope.resolve(self.target, None));
                    // A direct binding to a property.
                    let property_name =
                        REGISTRY.with(|registry| registry.get_mapped_prop_name(&binding.name));
                    let stmt = TcbExpr::new(format!(
                        "{}[{}] = {}",
                        el_id.p(),
                        TcbExpr::quote_and_escape(&property_name),
                        expr.wrap_for_type_checker().p()
                    ))
                    .add_parse_span_info(binding.source_span);
                    self.scope.add_statement(stmt);
                } else {
                    self.scope.add_statement(expr);
                }
            } else {
                // A binding to an animation, attribute, class or style. For now, only validate the right-
                // hand side of the expression.
                // TODO: properly check class and style bindings.
                self.scope.add_statement(expr);
            }
        }

        None
    }
}

// render3/util.ts:24 UNSAFE_OBJECT_KEY_NAME_REGEXP, render3/util.ts:179 isUnsafeObjectKey
fn is_unsafe_object_key(key: &str) -> bool {
    key.contains(['-', '.'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_object_keys() {
        assert!(is_unsafe_object_key("a-b"));
        assert!(is_unsafe_object_key("a.b"));
        assert!(!is_unsafe_object_key("a_b$"));
    }
}
