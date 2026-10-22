//! `packages/compiler/src/typecheck/ops/signal_forms.ts`

use std::sync::LazyLock;

use oxc_angular_compiler::ast::expression::{AngularExpression, BindingType};
use oxc_angular_compiler::ast::r3::{R3BoundAttribute, R3Element};
use oxc_span::Span;

use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::api::{
    DirectiveRef, Set, TcbDirectiveMetadata, TcbInputMapping, TypeCheckingConfig,
};
use crate::typecheck::oob::{AttributeNode, DirectiveHost};
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::bindings::{TcbBindingValue, TcbBoundAttribute, TcbBoundAttributeInput};
use crate::typecheck::ops::codegen::{TcbExpr, declare_variable};
use crate::typecheck::ops::context::Context;
use crate::typecheck::ops::expression::tcb_expression;

// signal_forms.ts:30 CustomFormControlType: possible types of custom form control directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomFormControlType {
    /// `'value'`
    Value,
    /// `'checkbox'`
    Checkbox,
}

// signal_forms.ts:33 formControlInputFields: names of the input fields on custom controls.
const FORM_CONTROL_INPUT_FIELDS: [&str; 16] = [
    // Should be kept in sync with the `FormUiControl` bindings,
    // defined in `packages/forms/signals/src/api/control.ts`.
    "errors",
    "dirty",
    "disabled",
    "disabledReasons",
    "hidden",
    "invalid",
    "name",
    "pending",
    "readonly",
    "touched",
    "max",
    "maxLength",
    "min",
    "minLength",
    "pattern",
    "required",
];

// signal_forms.ts:55 customFormControlBannedInputFields: names of input fields to which users aren't allowed to bind
// when using a `field` directive.
pub static CUSTOM_FORM_CONTROL_BANNED_INPUT_FIELDS: LazyLock<Set<&'static str>> =
    LazyLock::new(|| FORM_CONTROL_INPUT_FIELDS.into_iter().chain(["value", "checked"]).collect());

// signal_forms.ts:62 formControlOptionalFields: names of the fields that are optional on a control.
const FORM_CONTROL_OPTIONAL_FIELDS: [&str; 4] = [
    // Should be kept in sync with the `FormUiControl` bindings,
    // defined in `packages/forms/signals/src/api/control.ts`.
    "max",
    "maxLength",
    "min",
    "minLength",
];

// signal_forms.ts:74 TcbNativeFieldOp: a `TcbOp` which constructs an instance of the signal forms `FormField`
// directive on a native element.
pub struct TcbNativeFieldOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    node: &'a R3Element<'a>,
    input_type: Option<&'a str>,
    /// Bindings that aren't supported on signal form fields.
    unsupported_binding_fields: Set<&'static str>,
    /// Whether the host element has a dynamic `type` binding, meaning we cannot
    /// statically determine the input type.
    has_dynamic_type: bool,
}

impl<'a, 'b> TcbNativeFieldOp<'a, 'b> {
    // signal_forms.ts:94 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        node: &'a R3Element<'a>,
        input_type: Option<&'a str>,
    ) -> Self {
        let mut unsupported_binding_fields: Set<&'static str> = FORM_CONTROL_INPUT_FIELDS
            .into_iter()
            .chain(["value", "checked", "maxlength", "minlength"])
            .collect();

        let has_dynamic_type = input_type.is_none()
            && node.inputs.iter().any(|input| {
                (input.binding_type == BindingType::Property
                    || input.binding_type == BindingType::Attribute)
                    && input.name == "type"
            });

        let is_possibly_date_or_time = has_dynamic_type
            || input_type == Some("date")
            || input_type == Some("time")
            || input_type == Some("month")
            || input_type == Some("week")
            || input_type == Some("datetime-local");

        if is_possibly_date_or_time {
            unsupported_binding_fields.shift_remove("min");
            unsupported_binding_fields.shift_remove("max");
        }

        Self { tcb, scope, node, input_type, unsupported_binding_fields, has_dynamic_type }
    }

    // signal_forms.ts:171 getExpectedTypeFromDomNode
    fn get_expected_type_from_dom_node(&self, node: &'a R3Element<'a>) -> Option<&'static str> {
        if node.name == "textarea" || node.name == "select" {
            // `<textarea>` and `<select>` are always strings.
            return Some("string");
        }

        if node.name != "input" {
            return Some(self.get_unsupported_type());
        }

        match self.input_type {
            Some("checkbox") => return Some("boolean"),

            Some("radio") => return Some("string"),

            Some("number" | "range" | "datetime-local") => return Some("string | number | null"),

            Some("date" | "month" | "time" | "week") => {
                return Some("string | number | Date | null");
            }

            _ => {}
        }

        // If the type is dynamic, check it as if it can be any of the types above.
        if self.has_dynamic_type {
            return Some("string | number | boolean | Date | null");
        }

        if self.input_type == Some("text") || self.input_type.is_none() {
            // Return null to signal the invariant check for text-like inputs.
            return None;
        }

        Some("string")
    }

    // signal_forms.ts:213 getUnsupportedType
    #[expect(clippy::unused_self, reason = "a method in TypeScript")]
    fn get_unsupported_type(&self) -> &'static str {
        "never"
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbNativeFieldOp<'a, 'b> {
    // signal_forms.ts:90 optional
    fn optional(&self) -> bool {
        false
    }

    // signal_forms.ts:124 execute
    fn execute(&self) -> Option<TcbExpr> {
        // `this.node instanceof HostElement ? this.node.bindings : this.node.inputs`: the node is an `Element`.
        let inputs = &self.node.inputs;
        let field_binding = inputs
            .iter()
            .find(|input| input.binding_type == BindingType::Property && input.name == "formField");

        // This should only happen if there's something like `<input formField="static"/>`
        // which will be caught by the input type checking of the `FormField` directive.
        let field_binding = field_binding?;

        check_unsupported_field_bindings(
            DirectiveOwner::Element(self.node),
            &self.unsupported_binding_fields,
            &self.tcb,
        );

        let raw_expected_type = self.get_expected_type_from_dom_node(self.node);

        match raw_expected_type {
            None => {
                // For text-like <input> elements, use an invariant check on the value signal.
                // WritableSignal<T> is invariant in T, so assigning it to a union of structural types
                // gives us exact type matching: only Field<string> or Field<number|null> are accepted.
                let signal =
                    extract_field_value_signal(&field_binding.value, &self.tcb, &self.scope);
                let id = TcbExpr::new(self.tcb.allocate_id());
                let union_type = TcbExpr::new(
                    "{ (): string; set: (v: string) => void; } | { (): number | null; set: (v: number | null) => void; }",
                );
                let assignment = TcbExpr::new(format!("{} = {}", id.p(), signal.p()))
                    .add_parse_span_info(
                        field_binding.value_span.unwrap_or(field_binding.source_span),
                    );

                self.scope.add_statement(declare_variable(&id, union_type));
                self.scope.add_statement(assignment);
            }
            Some(raw_expected_type) => {
                let expected_type = TcbExpr::new(raw_expected_type);
                let value = extract_field_value(&field_binding.value, &self.tcb, &self.scope);

                // Create a variable with the expected type and check that the field value is assignable, e.g.
                // var t1 = null! as string | number; t1 = f().value()`.
                let id = TcbExpr::new(self.tcb.allocate_id());
                let assignment = TcbExpr::new(format!("{} = {}", id.p(), value.p()))
                    .add_parse_span_info(
                        field_binding.value_span.unwrap_or(field_binding.source_span),
                    );

                self.scope.add_statement(declare_variable(&id, expected_type));
                self.scope.add_statement(assignment);
            }
        }

        None
    }
}

// signal_forms.ts:221 TcbNativeRadioButtonFieldOp: a variation of the `TcbNativeFieldOp` with specific logic for
// radio buttons.
pub struct TcbNativeRadioButtonFieldOp<'a, 'b> {
    /// `super`
    base: TcbNativeFieldOp<'a, 'b>,
}

impl<'a, 'b> TcbNativeRadioButtonFieldOp<'a, 'b> {
    // signal_forms.ts:222 constructor
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, node: &'a R3Element<'a>) -> Self {
        let mut base = TcbNativeFieldOp::new(tcb, scope, node, Some("radio"));
        base.unsupported_binding_fields.shift_remove("value");
        Self { base }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbNativeRadioButtonFieldOp<'a, 'b> {
    fn optional(&self) -> bool {
        self.base.optional()
    }

    // signal_forms.ts:227 execute
    fn execute(&self) -> Option<TcbExpr> {
        self.base.execute();

        let value_binding = self
            .base
            .node
            .inputs
            .iter()
            .find(|attr| attr.binding_type == BindingType::Property && attr.name == "value");

        if let Some(value_binding) = value_binding {
            // Include an additional expression to check that the `value` is a string.
            let id = TcbExpr::new(self.base.tcb.allocate_id());
            let value = tcb_expression(&value_binding.value, &self.base.tcb, &self.base.scope);
            let assignment = TcbExpr::new(format!("{} = {}", id.p(), value.p()))
                .add_parse_span_info(value_binding.source_span);
            self.base.scope.add_statement(declare_variable(&id, TcbExpr::new("string")));
            self.base.scope.add_statement(assignment);
        }

        None
    }
}

// signal_forms.ts:249 expandBoundAttributesForField: expands the set of bound inputs with the ones from custom field
// directives.
pub fn expand_bound_attributes_for_field<'a>(
    directive: &TcbDirectiveMetadata,
    node: DirectiveHost<'a>,
    custom_form_control_type: Option<CustomFormControlType>,
) -> Option<Vec<TcbBoundAttribute<'a>>> {
    let inputs: &'a [R3BoundAttribute<'a>] = match node {
        DirectiveHost::Element(node) => &node.inputs,
        DirectiveHost::Template(node) => &node.inputs,
        DirectiveHost::Component(node) => &node.inputs,
        DirectiveHost::Directive(node) => &node.inputs,
    };
    let field_binding = inputs
        .iter()
        .find(|input| input.binding_type == BindingType::Property && input.name == "formField")?;

    let mut bound_inputs: Option<Vec<TcbBoundAttribute<'a>>> = None;

    let primary_input = match custom_form_control_type {
        Some(CustomFormControlType::Value) => get_synthetic_field_bound_input(
            directive,
            "value",
            "value",
            field_binding,
            custom_form_control_type,
        ),
        Some(CustomFormControlType::Checkbox) => get_synthetic_field_bound_input(
            directive,
            "checked",
            "value",
            field_binding,
            custom_form_control_type,
        ),
        None => None,
    };

    if let Some(primary_input) = primary_input {
        bound_inputs.get_or_insert_with(Vec::new).push(primary_input);
    }

    for name in FORM_CONTROL_INPUT_FIELDS {
        let input = get_synthetic_field_bound_input(
            directive,
            name,
            name,
            field_binding,
            custom_form_control_type,
        );

        if let Some(input) = input {
            bound_inputs.get_or_insert_with(Vec::new).push(input);
        }
    }

    bound_inputs
}

// signal_forms.ts:308 isFieldDirective
fn is_field_directive(meta: &TcbDirectiveMetadata) -> bool {
    if meta.name != "FormField" {
        return false;
    }

    // Fast path, relevant for all external users.
    if meta.r#ref.module_name.as_deref() == Some("@angular/forms/signals") {
        return true;
    }

    // Slightly slower, but more accurate path. Fallback for internal / local compilation where we don't have the exact module.
    meta.has_ng_field_directive
}

/// The synthetic `AST` `getSyntheticFieldBoundInput` builds around a `[formField]` binding's expression.
///
/// That is, around `F`: `F().<field>` (`Read`), `F().<field>()` (`Call`) or `F().<field>?.()` (`SafeCall`), all
/// with the span of `F`'s `ASTWithSource`.
///
/// The fork's arena-allocated AST cannot build a node whose receiver is the binding's own expression node (whose
/// identity the binder keys its targets by), so the synthetic nodes are described here and `translate` produces what
/// `astToTcbExpr` produces for them: none of them resolves (the binder has never seen them), the outer `Call`'s
/// receiver is the `ASTWithSource` (so it is neither a `PropertyRead` receiver nor part of a safe navigation chain),
/// and only `F` itself is translated as the template's expression it is.
#[derive(Debug, Clone, Copy)]
pub struct SyntheticFieldValue<'a> {
    /// The `[formField]` binding's expression (`F`)
    pub field_binding_value: &'a AngularExpression<'a>,
    /// The `span`/`sourceSpan` of `F`'s `ASTWithSource`, which every synthetic node has
    pub source_span: Span,
    /// `fieldPropertyName`
    pub field_property_name: &'static str,
    pub kind: SyntheticFieldValueKind,
}

/// The outermost node of a `SyntheticFieldValue`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticFieldValueKind {
    /// `read`: `PropertyRead(Call(F, []), fieldPropertyName)`
    Read,
    /// `new Call(span, sourceSpan, read, [], sourceSpan)`
    Call,
    /// `new SafeCall(span, sourceSpan, read, [], sourceSpan)`
    SafeCall,
}

impl<'a> SyntheticFieldValue<'a> {
    /// `tcbExpression(value, tcb, scope)` (`translateInput`)
    pub fn translate<'b>(&self, tcb: &TcbRef<'a, 'b>, scope: &ScopeRef<'a, 'b>) -> TcbExpr {
        let field = tcb_expression(self.field_binding_value, tcb, scope);
        self.translate_around(&field, tcb.env.config())
    }

    /// The translation, given `F`'s
    fn translate_around(&self, field: &TcbExpr, config: &TypeCheckingConfig) -> TcbExpr {
        let span = self.source_span;
        // expression.ts visitCall of `outerCall`: its receiver is no `PropertyRead`.
        let outer_call = TcbExpr::new(format!("{}()", field.p())).add_parse_span_info(span);
        // expression.ts visitPropertyRead / visitCall of `read`'s receiver: `outerCall` is no safe navigation chain.
        let receiver = outer_call.wrap_for_type_checker();
        let property = TcbExpr::new(format!("{}.{}", receiver.p(), self.field_property_name))
            .add_parse_span_info(span);
        match self.kind {
            // expression.ts visitPropertyRead
            SyntheticFieldValueKind::Read => {
                property.wrap_for_type_checker().add_parse_span_info(span)
            }
            // expression.ts visitCall (a `PropertyRead` receiver, unresolved)
            SyntheticFieldValueKind::Call => {
                TcbExpr::new(format!("{}()", property.p())).add_parse_span_info(span)
            }
            // expression.ts visitSafeCall and convertToSafeCall (`VeSafeLhsInferenceBugDetector` visits the
            // `PropertyRead` receiver: false)
            SyntheticFieldValueKind::SafeCall => {
                let expr = property
                    .wrap_for_type_checker()
                    .add_parse_span_info(span)
                    .wrap_for_type_checker();
                let node = if config.strict_safe_navigation_types {
                    TcbExpr::new(format!("{}?.()", expr.p()))
                } else {
                    TcbExpr::new(format!("({}!() as any)", expr.p()))
                };
                node.add_parse_span_info(span)
            }
        }
    }
}

// signal_forms.ts:322 getSyntheticFieldBoundInput
fn get_synthetic_field_bound_input<'a>(
    dir: &TcbDirectiveMetadata,
    input_name: &str,
    field_property_name: &'static str,
    field_binding: &'a R3BoundAttribute<'a>,
    custom_field_type: Option<CustomFormControlType>,
) -> Option<TcbBoundAttribute<'a>> {
    let inputs = dir.inputs.get_by_binding_property_name(input_name);

    let inputs = inputs.filter(|inputs| !inputs.is_empty())?;

    // `fieldBinding.value` is an `ASTWithSource`: its span starts at the value's offset and spans its text, which the
    // fork keeps as the binding's value span.
    let source_span = field_binding
        .value_span
        .unwrap_or(Span::new(field_binding.source_span.start, field_binding.source_span.start));
    let is_two_way_binding = (custom_field_type == Some(CustomFormControlType::Value)
        && input_name == "value")
        || (custom_field_type == Some(CustomFormControlType::Checkbox) && input_name == "checked");

    let kind = if is_two_way_binding {
        SyntheticFieldValueKind::Read
    } else if FORM_CONTROL_OPTIONAL_FIELDS.contains(&field_property_name) {
        // Some fields can be optional so we need to invoke them using a `SafeCall`.
        SyntheticFieldValueKind::SafeCall
    } else {
        SyntheticFieldValueKind::Call
    };
    let value = SyntheticFieldValue {
        field_binding_value: &field_binding.value,
        source_span,
        field_property_name,
        kind,
    };

    Some(TcbBoundAttribute {
        value: TcbBindingValue::SyntheticField(value),
        source_span: field_binding.source_span,
        key_span: Some(field_binding.key_span),
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
    })
}

// signal_forms.ts:368 getCustomFieldDirectiveType: determines if a directive is a custom field and its type.
pub fn get_custom_field_directive_type(
    meta: &TcbDirectiveMetadata,
) -> Option<CustomFormControlType> {
    if has_model_input("value", meta) {
        return Some(CustomFormControlType::Value);
    } else if has_model_input("checked", meta) {
        return Some(CustomFormControlType::Checkbox);
    }
    None
}

// signal_forms.ts:380 isNativeField: determines if a directive usage is on a native field (an `input`, `select` or
// `textarea` element: TypeScript's type guard).
pub fn is_native_field(
    dir: &TcbDirectiveMetadata,
    node: DirectiveHost<'_>,
    all_directive_matches: &[DirectiveRef],
) -> bool {
    // Only applies to the `FormField` directive.
    if !is_field_directive(dir) {
        return false;
    }

    // Only applies to input, select and textarea elements.
    let DirectiveHost::Element(node) = node else {
        return false;
    };
    if node.name != "input" && node.name != "select" && node.name != "textarea" {
        return false;
    }

    // Only applies if there are no custom fields or ControlValueAccessors.
    all_directive_matches.iter().all(|meta| {
        get_custom_field_directive_type(meta).is_none() && !is_control_value_accessor_like(meta)
    })
}

// signal_forms.ts:408 isControlValueAccessorLike: determines if a directive is shaped like a `ControlValueAccessor`.
// Note that this isn't 100% reliable, because we don't know if the directive was actually provided at runtime.
fn is_control_value_accessor_like(meta: &TcbDirectiveMetadata) -> bool {
    meta.public_methods.contains("writeValue")
        && meta.public_methods.contains("registerOnChange")
        && meta.public_methods.contains("registerOnTouched")
}

// signal_forms.ts:417 checkUnsupportedFieldBindings: checks whether a node has bindings that aren't supported on
// fields.
#[expect(clippy::if_same_then_else, reason = "the TypeScript's conditions")]
pub fn check_unsupported_field_bindings<'a>(
    node: DirectiveOwner<'a>,
    unsupported_binding_fields: &Set<&'static str>,
    tcb: &Context<'a, '_>,
) {
    let inputs: &'a [R3BoundAttribute<'a>] = match node {
        DirectiveOwner::HostElement(node) => &node.bindings,
        DirectiveOwner::Element(node) => &node.inputs,
        DirectiveOwner::Template(node) => &node.inputs,
        DirectiveOwner::Component(node) => &node.inputs,
        DirectiveOwner::Directive(node) => &node.inputs,
    };

    for input in inputs {
        if input.binding_type == BindingType::Property
            && unsupported_binding_fields.contains(input.name.as_str())
        {
            tcb.oob_recorder
                .form_field_unsupported_binding(&tcb.id, AttributeNode::BoundAttribute(input));
        } else if input.binding_type == BindingType::Attribute
            && unsupported_binding_fields.contains(input.name.to_lowercase().as_str())
        {
            tcb.oob_recorder
                .form_field_unsupported_binding(&tcb.id, AttributeNode::BoundAttribute(input));
        } else if input.binding_type == BindingType::TwoWay
            && unsupported_binding_fields.contains(input.name.as_str())
        {
            tcb.oob_recorder
                .form_field_unsupported_binding(&tcb.id, AttributeNode::BoundAttribute(input));
        }
    }

    let attributes = match node {
        DirectiveOwner::HostElement(_) => None,
        DirectiveOwner::Element(node) => Some(&node.attributes),
        DirectiveOwner::Template(node) => Some(&node.attributes),
        DirectiveOwner::Component(node) => Some(&node.attributes),
        DirectiveOwner::Directive(node) => Some(&node.attributes),
    };
    if let Some(attributes) = attributes {
        for attr in attributes {
            if unsupported_binding_fields.contains(attr.name.to_lowercase().as_str()) {
                tcb.oob_recorder
                    .form_field_unsupported_binding(&tcb.id, AttributeNode::TextAttribute(attr));
            }
        }
    }
}

// signal_forms.ts:449 extractFieldValue: gets an expression that extracts the value of a field binding.
fn extract_field_value<'a, 'b>(
    expression: &'a AngularExpression<'a>,
    tcb: &TcbRef<'a, 'b>,
    scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    // Unwraps the field, e.g. `[field]="f"` turns into `f()`.
    let inner_call = TcbExpr::new(tcb_expression(expression, tcb, scope).p() + "()");

    // Note: we ignore diagnostics on this call, because it might not be callable
    // (e.g. `undefined` is passed in). Whether the value conforms to `FieldTree` is
    // checked using the common inputs op.
    let inner_call = inner_call.mark_ignore_diagnostics();

    // Extract the value from the field, e.g. `f().value()`.
    TcbExpr::new(format!("{}.value()", inner_call.p()))
}

// signal_forms.ts:465 extractFieldValueSignal: gets an expression that extracts the value signal of a field binding
// (without calling it).
fn extract_field_value_signal<'a, 'b>(
    expression: &'a AngularExpression<'a>,
    tcb: &TcbRef<'a, 'b>,
    scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    // Unwraps the field, e.g. `[field]="f"` turns into `f()`.
    let inner_call = TcbExpr::new(tcb_expression(expression, tcb, scope).p() + "()");
    let inner_call = inner_call.mark_ignore_diagnostics();

    // Extract the value signal from the field, e.g. `f().value` (not called).
    TcbExpr::new(format!("{}.value", inner_call.p()))
}

// signal_forms.ts:475 hasModelInput: checks whether a directive has a model-like input with a specific name.
fn has_model_input(name: &str, meta: &TcbDirectiveMetadata) -> bool {
    meta.inputs.has_binding_property_name(name)
        && meta.outputs.has_binding_property_name(&format!("{name}Change"))
}

// signal_forms.ts:487 isFormControl: determines whether a node is a form control based on its matching directives.
//
// A node is a form control if it has a matching `FormField` directive, and no other directives match
// the `field` input.
pub fn is_form_control(all_directive_matches: &[DirectiveRef]) -> bool {
    let mut result = false;
    for r#match in all_directive_matches {
        if r#match.inputs.has_binding_property_name("formField") {
            if !is_field_directive(r#match) {
                return false;
            }
            result = true;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use oxc_allocator::Allocator;

    use super::*;
    use crate::typecheck::ops::bindings::test_support::{Parts, config, directive, element, parse};

    fn synthetic(kind: SyntheticFieldValueKind, field_property_name: &'static str) -> String {
        let allocator = Allocator::default();
        let nodes = parse(&allocator, r#"<x [formField]="f"></x>"#);
        let field_binding = &element(nodes).inputs[0];
        let value = SyntheticFieldValue {
            field_binding_value: &field_binding.value,
            source_span: field_binding.value_span.unwrap(),
            field_property_name,
            kind,
        };
        let mut config = config();
        let strict = value.translate_around(&TcbExpr::new("((this).f)"), &config).p();
        config.strict_safe_navigation_types = false;
        let loose = value.translate_around(&TcbExpr::new("((this).f)"), &config).p();
        format!("{strict} | {loose}")
    }

    /// What Angular's type-check block tests compare (`ngtsc/typecheck/testing`: `/\s+\/\*[\s\S]*?\*\//g`
    /// removed)
    fn strip_comments(text: &str) -> String {
        let mut result = text.to_string();
        while let Some(start) = result.find(" /*") {
            let end = start + result[start..].find("*/").unwrap() + 2;
            result.replace_range(start..end, "");
        }
        result
    }

    /// Angular's type_check_block_spec.ts "should add implicit bindings to other field-related inputs on a custom
    /// control" and "should produce safe reads for the fields that are optional" (which print without comments, and
    /// wrap the value for the assignment)
    #[test]
    fn synthetic_field_values_translate_as_angular_does() {
        let wrapped = |kind, name| {
            let allocator = Allocator::default();
            let nodes = parse(&allocator, r#"<x [formField]="f"></x>"#);
            let field_binding = &element(nodes).inputs[0];
            let value = SyntheticFieldValue {
                field_binding_value: &field_binding.value,
                source_span: field_binding.value_span.unwrap(),
                field_property_name: name,
                kind,
            };
            strip_comments(
                &value
                    .translate_around(&TcbExpr::new("((this).f)"), &config())
                    .wrap_for_type_checker()
                    .p(),
            )
        };
        assert_eq!(wrapped(SyntheticFieldValueKind::Read, "value"), "(((((this).f)()).value))");
        assert_eq!(
            wrapped(SyntheticFieldValueKind::Call, "required"),
            "((((this).f)()).required())"
        );
        assert_eq!(
            wrapped(SyntheticFieldValueKind::SafeCall, "max"),
            "((((((this).f)()).max))?.())"
        );
    }

    #[test]
    fn synthetic_field_values_with_their_spans() {
        assert_eq!(
            synthetic(SyntheticFieldValueKind::Read, "value"),
            "((((this).f)() /*16,17*/).value /*16,17*/) /*16,17*/ | \
             ((((this).f)() /*16,17*/).value /*16,17*/) /*16,17*/"
        );
        assert_eq!(
            synthetic(SyntheticFieldValueKind::Call, "required"),
            "(((this).f)() /*16,17*/).required /*16,17*/() /*16,17*/ | \
             (((this).f)() /*16,17*/).required /*16,17*/() /*16,17*/"
        );
        assert_eq!(
            synthetic(SyntheticFieldValueKind::SafeCall, "max"),
            "(((((this).f)() /*16,17*/).max /*16,17*/) /*16,17*/)?.() /*16,17*/ | \
             ((((((this).f)() /*16,17*/).max /*16,17*/) /*16,17*/)!() as any) /*16,17*/"
        );
    }

    fn form_field() -> TcbDirectiveMetadata {
        let mut dir = directive("FormField", &[("field", "formField", true)], &[]);
        dir.r#ref.module_name = Some("@angular/forms/signals".to_string());
        dir
    }

    #[test]
    fn expanded_bindings_of_a_custom_control() {
        let allocator = Allocator::default();
        let nodes = parse(&allocator, r#"<custom-control [formField]="f" />"#);
        let node = DirectiveHost::Element(element(nodes));
        let control = directive(
            "CustomControl",
            &[("value", "value", false), ("disabled", "disabled", false), ("max", "max", false)],
            &[("valueChange", "valueChange")],
        );
        assert_eq!(get_custom_field_directive_type(&control), Some(CustomFormControlType::Value));

        let attrs =
            expand_bound_attributes_for_field(&control, node, Some(CustomFormControlType::Value))
                .unwrap();
        let described: Vec<String> = attrs
            .iter()
            .map(|attr| {
                let TcbBindingValue::SyntheticField(value) = attr.value else { panic!() };
                format!(
                    "{} {:?} {:?} {}",
                    value.field_property_name,
                    value.kind,
                    value.source_span,
                    attr.inputs[0].is_two_way_binding
                )
            })
            .collect();
        assert_eq!(
            described,
            [
                "value Read Span { start: 29, end: 30 } true",
                "disabled Call Span { start: 29, end: 30 } false",
                "max SafeCall Span { start: 29, end: 30 } false",
            ]
        );

        // No `[formField]` binding.
        let nodes = parse(&allocator, r#"<custom-control [field]="f" />"#);
        assert!(
            expand_bound_attributes_for_field(
                &control,
                DirectiveHost::Element(element(nodes)),
                None
            )
            .is_none()
        );
    }

    #[test]
    fn form_controls_and_native_fields() {
        let allocator = Allocator::default();
        let nodes = parse(&allocator, r#"<input [formField]="f" />"#);
        let input = DirectiveHost::Element(element(nodes));
        let field = Rc::new(form_field());
        let other = Rc::new(directive("Other", &[("formField", "formField", false)], &[]));
        let mut accessor = directive("Accessor", &[], &[]);
        accessor
            .public_methods
            .extend(["writeValue", "registerOnChange", "registerOnTouched"].map(String::from));
        let accessor = Rc::new(accessor);

        assert!(is_form_control(&[Rc::clone(&field)]));
        assert!(!is_form_control(&[Rc::clone(&field), Rc::clone(&other)]));
        assert!(!is_form_control(&[Rc::clone(&accessor)]));

        assert!(is_native_field(&field, input, &[Rc::clone(&field)]));
        assert!(!is_native_field(&field, input, &[Rc::clone(&field), accessor]));
        assert!(!is_native_field(&other, input, &[Rc::clone(&other)]));
        let nodes = parse(&allocator, r#"<div [formField]="f"></div>"#);
        assert!(!is_native_field(
            &field,
            DirectiveHost::Element(element(nodes)),
            &[Rc::clone(&field)]
        ));
    }

    #[test]
    fn unsupported_field_bindings() {
        let allocator = Allocator::default();
        let nodes = parse(
            &allocator,
            r#"<input [formField]="f" [value]="v" [attr.MAXLENGTH]="1" [(disabled)]="d" Required [class.min]="m" />"#,
        );
        let parts = Parts::new(config());
        check_unsupported_field_bindings(
            DirectiveOwner::Element(element(nodes)),
            &CUSTOM_FORM_CONTROL_BANNED_INPUT_FIELDS,
            &parts.context(),
        );
        assert_eq!(
            parts.calls(),
            [
                "form_field_unsupported_binding([value])",
                "form_field_unsupported_binding([disabled])",
                "form_field_unsupported_binding(Required)",
            ]
        );
    }
}
