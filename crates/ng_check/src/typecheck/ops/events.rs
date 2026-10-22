//! `packages/compiler/src/typecheck/ops/events.ts`

use std::rc::Rc;

use oxc_angular_compiler::ast::expression::{AngularExpression, ParsedEventType, PropertyRead};
use oxc_angular_compiler::ast::r3::{R3BoundAttribute, R3BoundEvent, R3LetDeclaration};

use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::api::{DirectiveRef, Set};
use crate::typecheck::comments::ExpressionIdentifier;
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::bindings::{
    check_split_two_way_binding, directive_owner_as_local_symbol,
};
use crate::typecheck::ops::codegen::{TcbExpr, get_statements_block};
use crate::typecheck::ops::expression::TcbExpressionTranslator;
use crate::typecheck::ops::references::LocalSymbol;

// events.ts:22 EVENT_PARAMETER
const EVENT_PARAMETER: &str = "$event";

// events.ts:24 EventParamType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventParamType {
    /* Generates code to infer the type of `$event` based on how the listener is registered. */
    Infer,

    /* Declares the type of the `$event` parameter as `any`. */
    Any,
}

/// `EventParamType | string`: how the `$event` parameter of a handler is typed
#[derive(Debug, Clone)]
enum EventType {
    ParamType(EventParamType),
    /// An explicit type
    String(String),
}

// events.ts:37 tcbEventHandlerExpression: similar to `tcbExpression`, this function converts the provided `AST`
// expression into a `ts.Expression`, with special handling of the `$event` variable that can be used within event
// bindings.
fn tcb_event_handler_expression<'a, 'b>(
    ast: &'a AngularExpression<'a>,
    tcb: &TcbRef<'a, 'b>,
    scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    let translator = TcbEventHandlerTranslator::new(Rc::clone(tcb), Rc::clone(scope));
    translator.translate(ast)
}

// events.ts:48 TcbDirectiveOutputsOp: a `TcbOp` which generates code to check event bindings on an element that
// correspond with the outputs of a directive.
//
// Executing this operation returns nothing.
pub struct TcbDirectiveOutputsOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    node: DirectiveOwner<'a>,
    inputs: Option<&'a [R3BoundAttribute<'a>]>,
    outputs: &'a [R3BoundEvent<'a>],
    dir: DirectiveRef,
}

impl<'a, 'b> TcbDirectiveOutputsOp<'a, 'b> {
    // events.ts:49 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        node: DirectiveOwner<'a>,
        inputs: Option<&'a [R3BoundAttribute<'a>]>,
        outputs: &'a [R3BoundEvent<'a>],
        dir: DirectiveRef,
    ) -> Self {
        Self { tcb, scope, node, inputs, outputs, dir }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbDirectiveOutputsOp<'a, 'b> {
    // events.ts:60 optional
    fn optional(&self) -> bool {
        false
    }

    // events.ts:64 execute
    fn execute(&self) -> Option<TcbExpr> {
        let mut dir_id: Option<TcbExpr> = None;
        let outputs = &self.dir.outputs;

        for output in self.outputs {
            if output.event_type == ParsedEventType::LegacyAnimation
                || !outputs.has_binding_property_name(&output.name)
            {
                continue;
            }

            if self.tcb.env.config().check_type_of_output_events
                && let Some(inputs) = self.inputs
                && output.name.ends_with("Change")
            {
                let input_name = &output.name[..output.name.len() - 6];
                check_split_two_way_binding(input_name, output, inputs, &self.tcb);
            }
            // TODO(alxhub): consider supporting multiple fields with the same property name for outputs.
            let field =
                &outputs.get_by_binding_property_name(&output.name).expect("a claimed output")[0]
                    .class_property_name;

            let dir_id = dir_id.get_or_insert_with(|| {
                self.scope.resolve(directive_owner_as_local_symbol(self.node), Some(&self.dir))
            });
            let output_field =
                TcbExpr::new(format!("{}[{}]", dir_id.p(), TcbExpr::quote_and_escape(field)))
                    .add_parse_span_info(output.key_span);

            if self.tcb.env.config().check_type_of_output_events {
                // For strict checking of directive events, generate a call to the `subscribe` method
                // on the directive's output field to let type information flow into the handler function's
                // `$event` parameter.
                let handler = tcb_create_event_handler(
                    output,
                    &self.tcb,
                    &self.scope,
                    EventType::ParamType(EventParamType::Infer),
                    None,
                );
                let call = TcbExpr::new(format!("{}.subscribe({})", output_field.p(), handler.p()))
                    .add_parse_span_info(output.source_span);
                self.scope.add_statement(call);
            } else {
                // If strict checking of directive events is disabled:
                //
                // * We still generate the access to the output field as a statement in the TCB so consumers
                //   of the `TemplateTypeChecker` can still find the node for the class member for the
                //   output.
                // * Emit a handler function where the `$event` parameter has an explicit `any` type.
                self.scope.add_statement(output_field);
                let handler = tcb_create_event_handler(
                    output,
                    &self.tcb,
                    &self.scope,
                    EventType::ParamType(EventParamType::Any),
                    None,
                );
                self.scope.add_statement(handler);
            }
        }

        None
    }
}

// events.ts:125 TcbUnclaimedOutputsOp: a `TcbOp` which generates code to check "unclaimed outputs" - event bindings
// on an element which were not attributed to any directive or component, and are instead processed against the HTML
// element itself.
//
// Executing this operation returns nothing.
pub struct TcbUnclaimedOutputsOp<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
    target: LocalSymbol<'a>,
    outputs: &'a [R3BoundEvent<'a>],
    inputs: Option<&'a [R3BoundAttribute<'a>]>,
    claimed_outputs: Option<Set<String>>,
    has_directives: bool,
    has_component: bool,
}

impl<'a, 'b> TcbUnclaimedOutputsOp<'a, 'b> {
    // events.ts:126 constructor (`hasDirectives` and `hasComponent` default to `false`)
    #[expect(clippy::too_many_arguments, reason = "the TypeScript constructor's parameters")]
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        target: LocalSymbol<'a>,
        outputs: &'a [R3BoundEvent<'a>],
        inputs: Option<&'a [R3BoundAttribute<'a>]>,
        claimed_outputs: Option<Set<String>>,
        has_directives: bool,
        has_component: bool,
    ) -> Self {
        Self { tcb, scope, target, outputs, inputs, claimed_outputs, has_directives, has_component }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbUnclaimedOutputsOp<'a, 'b> {
    // events.ts:139 optional
    fn optional(&self) -> bool {
        false
    }

    // events.ts:143 execute
    fn execute(&self) -> Option<TcbExpr> {
        let mut el_id: Option<TcbExpr> = None;
        let config = self.tcb.env.config();

        // TODO(alxhub): this could be more efficient.
        for output in self.outputs {
            if self
                .claimed_outputs
                .as_ref()
                .is_some_and(|claimed| claimed.contains(output.name.as_str()))
            {
                // Skip this event handler as it was claimed by a directive.
                continue;
            }

            if config.check_type_of_output_events
                && let Some(inputs) = self.inputs
                && output.name.ends_with("Change")
            {
                let input_name = &output.name[..output.name.len() - 6];
                if check_split_two_way_binding(input_name, output, inputs, &self.tcb) {
                    // Skip this event handler as the error was already handled.
                    continue;
                }
            }

            if config.check_unclaimed_event_names
                && self.has_directives
                && output.event_type == ParsedEventType::Regular
                && output.target.is_none()
                && let LocalSymbol::Element(target) = self.target
            {
                self.tcb.dom_schema_checker.check_template_element_event(
                    &self.tcb.id,
                    &target.name,
                    &output.name,
                    output.key_span,
                    self.tcb.schemas,
                    self.has_component,
                );
            }

            if output.event_type == ParsedEventType::LegacyAnimation {
                // Animation output bindings always have an `$event` parameter of type `AnimationEvent`.
                let event_type = if config.check_type_of_animation_events {
                    EventType::String(
                        self.tcb
                            .env
                            .reference_external_symbol("@angular/animations", "AnimationEvent")
                            .p(),
                    )
                } else {
                    EventType::ParamType(EventParamType::Any)
                };

                let handler =
                    tcb_create_event_handler(output, &self.tcb, &self.scope, event_type, None);
                self.scope.add_statement(handler);
            } else if output.event_type == ParsedEventType::Animation {
                let event_type = self
                    .tcb
                    .env
                    .reference_external_symbol("@angular/core", "AnimationCallbackEvent");

                let handler = tcb_create_event_handler(
                    output,
                    &self.tcb,
                    &self.scope,
                    EventType::String(event_type.p()),
                    None,
                );
                self.scope.add_statement(handler);
            } else if config.check_type_of_dom_events {
                // If strict checking of DOM events is enabled, generate a call to `addEventListener` on
                // the element instance so that TypeScript's type inference for
                // `HTMLElement.addEventListener` using `HTMLElementEventMap` to infer an accurate type for
                // `$event` depending on the event name. For unknown event names, TypeScript resorts to the
                // base `Event` type.
                let mut dom_event_assertion: Option<TcbExpr> = None;

                // Only check for `window` and `document` since in theory any target can be passed.
                let target = match output.target {
                    Some(output_target)
                        if output_target == "window" || output_target == "document" =>
                    {
                        TcbExpr::new(output_target.as_str())
                    }
                    _ => el_id.get_or_insert_with(|| self.scope.resolve(self.target, None)).clone(),
                };

                // By default the target of an event is `EventTarget | null`, because of bubbling
                // and custom events. This can be inconvenient in some common cases like `input` elements
                // since we don't have the ability to type cast in templates. We can improve the type
                // checking for some of these cases by inferring the target based on the element it was
                // bound to. We can only do this safely if the element is a void element (e.g. `input` or
                // `img`), because we know that it couldn't have bubbled from a child. The event handler
                // with the assertion would look as follows:
                //
                // ```
                // const _t1 = document.createElement('input');
                //
                // _t1.addEventListener('input', ($event) => {
                //   ɵassertType<typeof _t1>($event.target);
                //   handler($event.target);
                // });
                // ```
                if let LocalSymbol::Element(element) = self.target
                    && element.is_void
                    && config.allow_dom_event_assertion
                {
                    let assert_util =
                        self.tcb.env.reference_external_symbol("@angular/core", "ɵassertType");
                    dom_event_assertion = Some(TcbExpr::new(format!(
                        "{}<typeof {}>({EVENT_PARAMETER}.target)",
                        assert_util.p(),
                        target.p()
                    )));
                }

                let property_access = TcbExpr::new(format!("{}.addEventListener", target.p()))
                    .add_parse_span_info(output.key_span);
                let handler = tcb_create_event_handler(
                    output,
                    &self.tcb,
                    &self.scope,
                    EventType::ParamType(EventParamType::Infer),
                    dom_event_assertion,
                );
                let call = TcbExpr::new(format!(
                    "{}({}, {})",
                    property_access.p(),
                    TcbExpr::quote_and_escape(&output.name),
                    handler.p()
                ))
                .add_parse_span_info(output.source_span);
                self.scope.add_statement(call);
            } else {
                // If strict checking of DOM inputs is disabled, emit a handler function where the `$event`
                // parameter has an explicit `any` type.
                let handler = tcb_create_event_handler(
                    output,
                    &self.tcb,
                    &self.scope,
                    EventType::ParamType(EventParamType::Any),
                    None,
                );
                self.scope.add_statement(handler);
            }
        }

        None
    }
}

// events.ts:270 TcbEventHandlerTranslator
struct TcbEventHandlerTranslator<'a, 'b> {
    tcb: TcbRef<'a, 'b>,
    scope: ScopeRef<'a, 'b>,
}

impl<'a, 'b> TcbEventHandlerTranslator<'a, 'b> {
    /// expression.ts:105 the `TcbExpressionTranslator` constructor
    fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>) -> Self {
        Self { tcb, scope }
    }
}

impl<'a, 'b> TcbExpressionTranslator<'a, 'b> for TcbEventHandlerTranslator<'a, 'b> {
    fn tcb(&self) -> &TcbRef<'a, 'b> {
        &self.tcb
    }

    fn scope(&self) -> &ScopeRef<'a, 'b> {
        &self.scope
    }

    // events.ts:271 resolve
    fn resolve(&self, ast: &'a AngularExpression<'a>) -> Option<TcbExpr> {
        // Recognize a property read on the implicit receiver corresponding with the event parameter
        // that is available in event bindings. Since this variable is a parameter of the handler
        // function that the converted expression becomes a child of, just create a reference to the
        // parameter by its name.
        if let AngularExpression::PropertyRead(read) = ast
            && matches!(read.receiver, AngularExpression::ImplicitReceiver(_))
            && read.name == EVENT_PARAMETER
        {
            return Some(TcbExpr::new(EVENT_PARAMETER).add_parse_span_info(read.name_span));
        }

        self.base_resolve(ast)
    }

    // events.ts:287 isValidLetDeclarationAccess
    fn is_valid_let_declaration_access(
        &self,
        _target: &'a R3LetDeclaration<'a>,
        _ast: &'a PropertyRead<'a>,
    ) -> bool {
        // Event listeners are allowed to read `@let` declarations before
        // they're declared since the callback won't be executed immediately.
        true
    }
}

// events.ts:305 tcbCreateEventHandler: creates an arrow function to be used as handler function for event bindings.
// The handler function has a single parameter `$event` and the bound event's handler `AST` represented as a
// TypeScript expression as its body.
//
// When `eventType` is set to `Infer`, the `$event` parameter will not have an explicit type. This
// allows for the created handler function to have its `$event` parameter's type inferred based on
// how it's used, to enable strict type checking of event bindings. When set to `Any`, the `$event`
// parameter will have an explicit `any` type, effectively disabling strict type checking of event
// bindings. Alternatively, an explicit type can be passed for the `$event` parameter.
fn tcb_create_event_handler<'a, 'b>(
    event: &'a R3BoundEvent<'a>,
    tcb: &TcbRef<'a, 'b>,
    scope: &ScopeRef<'a, 'b>,
    event_type: EventType,
    assertion_expression: Option<TcbExpr>,
) -> TcbExpr {
    let handler = tcb_event_handler_expression(&event.handler, tcb, scope);
    let mut statements: Vec<TcbExpr> = Vec::new();

    if let Some(assertion_expression) = assertion_expression {
        statements.push(assertion_expression);
    }

    statements.push(handler);

    let event_param_type: Option<String> = match event_type {
        EventType::ParamType(EventParamType::Infer) => None,
        EventType::ParamType(EventParamType::Any) => Some("any".to_string()),
        EventType::String(event_type) => Some(event_type),
    };

    // Obtain all guards that have been applied to the scope and its parents, as they have to be
    // repeated within the handler function for their narrowing to be in effect within the handler.
    let guards = scope.guards();
    let mut body = format!("{{\n{} }}", get_statements_block(&statements, false));

    if let Some(guards) = guards {
        // Wrap the body in an `if` statement containing all guards that have to be applied.
        body = format!("{{ if ({}) {body} }}", guards.p());
    }

    let event_param = TcbExpr::new(format!(
        "{EVENT_PARAMETER}{}",
        match event_param_type {
            None => String::new(),
            Some(event_param_type) => format!(": {event_param_type}"),
        }
    ))
    .add_expression_identifier(ExpressionIdentifier::EventParameter, None);

    // Return an arrow function instead of a function expression to preserve the `this` context.
    TcbExpr::new(format!("({}): any => {body}", event_param.p()))
}
