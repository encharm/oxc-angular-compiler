//! `packages/compiler/src/typecheck/host_bindings.ts`: the host element of a directive, built from its host
//! bindings, for type-checking them.
//!
//! Differences from the fork:
//! - The nodes are built in `allocator` (the template AST's arena).
//! - Angular's `BindingParser` is the fork's `parser::expression::BindingParser`; the parts of Angular's
//!   `parseBinding` (for a host binding: `parseSimpleBinding`, which reports pipes) and `parseEvent` the generator
//!   relies on are ported below (`parse_host_binding`, `parse_event`, `parse_action`), as the fork's parser does not
//!   expose them.
//! - The fork's `ParseSpan` offsets are unsigned: Angular's dummy `ParseSpan(-1, -1)` is `u32::MAX` (it only ever
//!   reaches type-check blocks through `sourceSpan`s, which are real).

use oxc_allocator::{Allocator, Box, FromIn, Vec as ArenaVec};
use oxc_angular_compiler::ast::expression::{
    AbsoluteSourceSpan, AngularExpression, BindingType, Call, ImplicitReceiver, KeyedRead,
    LiteralPrimitive, LiteralValue, ParseSpan, ParsedEventType, PropertyRead, ThisReceiver,
};
use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3HostElement, SecurityContext,
};
use oxc_angular_compiler::parser::expression::{BindingParser, SimpleExpressionChecker};
use oxc_span::Span;
use oxc_str::Ident;

use crate::angular::directive_matching::CssSelector;

// host_bindings.ts:36 HOST_BINDING_GUARD_COMMENT_TEXT
/// Comment attached to an AST node that serves as a guard to distinguish nodes
/// used for type checking host bindings from ones used for templates.
pub const HOST_BINDING_GUARD_COMMENT_TEXT: &str = "hostBindingsBlockGuard";

// host_bindings.ts:39 SourceNode
/// Represents information extracted from the source AST.
#[derive(Debug, Clone)]
pub enum SourceNode {
    Static(StaticSourceNode),
    /// `kind: 'unspecified'`
    Unspecified {
        source_span: Span,
    },
}

/// `'string' | 'identifier'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticSourceNodeKind {
    String,
    Identifier,
}

// host_bindings.ts:47 StaticSourceNode
/// A `SourceNode` which represents a static expression.
#[derive(Debug, Clone)]
pub struct StaticSourceNode {
    pub kind: StaticSourceNodeKind,
    /// Raw source code of the node (e.g. strings include the quotes).
    pub source: String,
    /// Actual text of the node (e.g. value inside the quotes in strings).
    pub text: String,
    /// Location information about the node.
    pub source_span: Span,
}

impl SourceNode {
    /// The node if its `kind` is `'string'`
    fn as_string(&self) -> Option<&StaticSourceNode> {
        match self {
            Self::Static(node) if node.kind == StaticSourceNodeKind::String => Some(node),
            _ => None,
        }
    }

    /// `sourceSpan`
    fn source_span(&self) -> Span {
        match self {
            Self::Static(node) => node.source_span,
            Self::Unspecified { source_span } => *source_span,
        }
    }
}

// host_bindings.ts:58 HostObjectLiteralBinding
/// A single binding inside the `host` object of a directive.
#[derive(Debug, Clone)]
pub struct HostObjectLiteralBinding {
    /// Node representing the key of the binding.
    pub key: SourceNode,
    /// Node representing the value of the binding.
    pub value: SourceNode,
    /// Location information about the entire binding.
    pub source_span: Span,
}

// host_bindings.ts:68 HostListenerDecorator
/// A single binding declared by a `@HostListener` decorator on a class member.
#[derive(Debug, Clone)]
pub struct HostListenerDecorator {
    /// Node declaring the name of the event (e.g. first argument of `@HostListener`).
    pub event_name: Option<SourceNode>,
    /// Node representing the name of the member that was decorated.
    pub member_name: StaticSourceNode,
    /// Location information about the member that the decorator is set on.
    pub member_span: Span,
    /// Arguments passed to the event.
    pub arguments: Vec<SourceNode>,
    /// Location information about the decorator.
    pub decorator_span: Span,
}

// host_bindings.ts:82 HostBindingDecorator
/// A single binding declared by the `@HostBinding` decorator on a class member.
#[derive(Debug, Clone)]
pub struct HostBindingDecorator {
    /// Node representing the name of the member that was decorated.
    pub member_name: StaticSourceNode,
    /// Location information about the member that the decorator is set on.
    pub member_span: Span,
    /// Arguments passed into the decorator
    pub arguments: Vec<SourceNode>,
    /// Location information about the decorator.
    pub decorator_span: Span,
}

// host_bindings.ts:98 createHostElement
/// Creates an AST node that represents the host element of a directive.
/// Can return null if there are no valid bindings to be checked.
/// `type`: `"component"` or `"directive"`.
pub fn create_host_element<'a>(
    allocator: &'a Allocator,
    r#type: &str,
    selector: Option<&str>,
    name_span: Span,
    host_object_literal_bindings: &[HostObjectLiteralBinding],
    host_binding_decorators: &[HostBindingDecorator],
    host_listener_decorators: &[HostListenerDecorator],
) -> Option<&'a R3HostElement<'a>> {
    let mut bindings: ArenaVec<'a, R3BoundAttribute<'a>> = ArenaVec::new_in(&allocator);
    let mut listeners: ArenaVec<'a, R3BoundEvent<'a>> = ArenaVec::new_in(&allocator);
    let mut parser: Option<BindingParser<'a>> = None;

    for binding in host_object_literal_bindings {
        // We only support type checking of static bindings.
        let parser = parser.get_or_insert_with(|| BindingParser::new(allocator));
        create_node_from_host_literal_property(
            allocator,
            binding,
            parser,
            &mut bindings,
            &mut listeners,
        );
    }

    for decorator in host_binding_decorators {
        create_node_from_binding_decorator(allocator, decorator, &mut bindings);
    }

    for decorator in host_listener_decorators {
        let parser = parser.get_or_insert_with(|| BindingParser::new(allocator));
        create_node_from_listener_decorator(allocator, decorator, parser, &mut listeners);
    }

    // The element will be a no-op if there are no bindings.
    if bindings.is_empty() && listeners.is_empty() {
        return None;
    }

    let mut tag_names: ArenaVec<'a, Ident<'a>> = ArenaVec::new_in(&allocator);

    if let Some(selector) = selector {
        let parts = CssSelector::parse(selector);

        for part in &parts {
            if let Some(element) = &part.element {
                tag_names.push(Ident::from_in(element.as_str(), allocator));
            }
        }
    }

    // If none of the selectors have a tag name, fall back to `ng-component`/`ng-directive`.
    // This is how the runtime handles components without tag names as well.
    if tag_names.is_empty() {
        let element_type = r#type;
        tag_names.push(Ident::from_in(format!("ng-{element_type}").as_str(), allocator));
    }

    Some(allocator.alloc(R3HostElement { tag_names, bindings, listeners, source_span: name_span }))
}

// host_bindings.ts:155 createHostBindingsBlockGuard
/// Creates an AST node that can be used as a guard in `if` statements to distinguish TypeScript
/// nodes used for checking host bindings from ones used for checking templates.
pub fn create_host_bindings_block_guard() -> String {
    // Note that the comment text is quite generic. This doesn't really matter, because it is
    // used only inside a TCB and there's no way for users to produce a comment there.
    // `true /*hostBindingsBlockGuard*/`.
    // Wrap the expression in parentheses to ensure that the comment is attached to the correct node.
    format!("(true /*{HOST_BINDING_GUARD_COMMENT_TEXT}*/)")
}

// host_bindings.ts:171 createNodeFromHostLiteralProperty
/// If possible, creates and tracks the relevant AST node for a binding declared
/// through a property on the `host` literal.
fn create_node_from_host_literal_property<'a>(
    allocator: &'a Allocator,
    binding: &HostObjectLiteralBinding,
    parser: &BindingParser<'a>,
    bindings: &mut ArenaVec<'a, R3BoundAttribute<'a>>,
    listeners: &mut ArenaVec<'a, R3BoundEvent<'a>>,
) {
    // TODO(crisbeto): surface parsing errors here, because currently they just get ignored.
    // They'll still get reported when the handler tries to parse the bindings, but here we
    // can highlight the nodes more accurately.
    let source_span = binding.source_span;

    let (Some(key), Some(value)) = (binding.key.as_string(), binding.value.as_string()) else {
        return;
    };

    if key.text.starts_with('[') && key.text.ends_with(']') {
        let (attr_name, r#type) = infer_bound_attribute(&key.text[1..key.text.len() - 1]);
        let (mut ast, errors) =
            parse_host_binding(allocator, parser, &value.text, value.source_span);
        if !errors.is_empty() {
            return; // See TODO above.
        }

        fixup_spans(&mut ast, value);
        bindings.push(R3BoundAttribute {
            name: Ident::from_in(attr_name.as_str(), allocator),
            binding_type: r#type,
            security_context: SecurityContext::None,
            value: ast,
            unit: None,
            source_span,
            key_span: key.source_span,
            value_span: Some(value.source_span),
            i18n: None,
        });
    } else if key.text.starts_with('(') && key.text.ends_with(')') {
        let event = parse_event(
            allocator,
            parser,
            &key.text[1..key.text.len() - 1],
            &value.text,
            source_span,
            value.source_span,
            key.source_span,
        );

        if !event.handler_errors.is_empty() {
            return; // See TODO above.
        }

        let mut event = event;
        fixup_spans(&mut event.handler, value);
        listeners.push(bound_event_from_parsed_event(allocator, event));
    }
}

// host_bindings.ts:240 createNodeFromBindingDecorator
/// If possible, creates and tracks a bound attribute node from a `HostBinding` decorator.
fn create_node_from_binding_decorator<'a>(
    allocator: &'a Allocator,
    decorator: &HostBindingDecorator,
    bindings: &mut ArenaVec<'a, R3BoundAttribute<'a>>,
) {
    let args = &decorator.arguments;

    // The first parameter is optional. If omitted, the name
    // of the class member is used as the property.
    let name_node: &StaticSourceNode = if args.is_empty() {
        &decorator.member_name
    } else if let Some(arg) = args[0].as_string() {
        arg
    } else {
        return;
    };

    // (`nameNode.kind !== 'string' && nameNode.kind !== 'identifier'`: a `StaticSourceNode` is one of them.)

    // We can't synthesize a fake expression here and pass it through the binding parser, because
    // it constructs all the spans based on the source code origin and they aren't easily mappable
    // back to the source. E.g. `@HostBinding('foo') id = '123'` in source code would look
    // something like `[foo]="this.id"` in the AST. Instead we construct the expressions
    // manually here. Note that we use a dummy span with -1/-1 as offsets, because it isn't
    // used for type checking and constructing it accurately would take some effort.
    let span = dummy_span();
    let property_start = decorator.member_span.start;
    let receiver = AngularExpression::ThisReceiver(Box::new_in(
        ThisReceiver { span, source_span: AbsoluteSourceSpan::new(property_start, property_start) },
        &allocator,
    ));
    let name_span = AbsoluteSourceSpan::new(name_node.source_span.start, name_node.source_span.end);
    let read = if decorator.member_name.kind == StaticSourceNodeKind::String {
        AngularExpression::KeyedRead(Box::new_in(
            KeyedRead {
                span,
                source_span: name_span,
                receiver,
                key: AngularExpression::LiteralPrimitive(Box::new_in(
                    LiteralPrimitive {
                        span,
                        source_span: name_span,
                        value: LiteralValue::String(Ident::from_in(
                            decorator.member_name.text.as_str(),
                            allocator,
                        )),
                    },
                    &allocator,
                )),
            },
            &allocator,
        ))
    } else {
        AngularExpression::PropertyRead(Box::new_in(
            PropertyRead {
                span,
                source_span: name_span,
                name_span,
                receiver,
                name: Ident::from_in(decorator.member_name.text.as_str(), allocator),
            },
            &allocator,
        ))
    };
    let (attr_name, r#type) = infer_bound_attribute(&name_node.text);

    bindings.push(R3BoundAttribute {
        name: Ident::from_in(attr_name.as_str(), allocator),
        binding_type: r#type,
        security_context: SecurityContext::None,
        value: read,
        unit: None,
        source_span: decorator.decorator_span,
        key_span: name_node.source_span,
        value_span: Some(decorator.decorator_span),
        i18n: None,
    });
}

// host_bindings.ts:306 createNodeFromListenerDecorator
/// If possible, creates and tracks a bound event node from a `HostBinding` decorator.
fn create_node_from_listener_decorator<'a>(
    allocator: &'a Allocator,
    decorator: &HostListenerDecorator,
    parser: &BindingParser<'a>,
    listeners: &mut ArenaVec<'a, R3BoundEvent<'a>>,
) {
    let Some(event_name_node) = decorator.event_name.as_ref().and_then(SourceNode::as_string)
    else {
        return;
    };

    // We can't synthesize a fake expression here and pass it through the binding parser, because
    // it constructs all the spans based on the source code origin and they aren't easily mappable
    // back to the source. E.g. `@HostListener('foo') handleFoo() {}` in source code would look
    // something like `(foo)="handleFoo()"` in the AST. Instead we construct the expressions
    // manually here. Note that we use a dummy span with -1/-1 as offsets, because it isn't
    // used for type checking and constructing it accurately would take some effort.
    let dummy_span = dummy_span();
    let mut arg_nodes: ArenaVec<'a, AngularExpression<'a>> = ArenaVec::new_in(&allocator);
    let method_start = decorator.member_span.start;
    let method_receiver = AngularExpression::ThisReceiver(Box::new_in(
        ThisReceiver {
            span: dummy_span,
            source_span: AbsoluteSourceSpan::new(method_start, method_start),
        },
        &allocator,
    ));
    let name_span = AbsoluteSourceSpan::new(
        decorator.member_name.source_span.start,
        decorator.member_name.source_span.end,
    );
    let receiver = if decorator.member_name.kind == StaticSourceNodeKind::String {
        AngularExpression::KeyedRead(Box::new_in(
            KeyedRead {
                span: dummy_span,
                source_span: name_span,
                receiver: method_receiver,
                key: AngularExpression::LiteralPrimitive(Box::new_in(
                    LiteralPrimitive {
                        span: dummy_span,
                        source_span: name_span,
                        value: LiteralValue::String(Ident::from_in(
                            decorator.member_name.text.as_str(),
                            allocator,
                        )),
                    },
                    &allocator,
                )),
            },
            &allocator,
        ))
    } else {
        AngularExpression::PropertyRead(Box::new_in(
            PropertyRead {
                span: dummy_span,
                source_span: name_span,
                name_span,
                receiver: method_receiver,
                name: Ident::from_in(decorator.member_name.text.as_str(), allocator),
            },
            &allocator,
        ))
    };

    for arg in &decorator.arguments {
        // If the parameter is a static string, parse it using the binding parser since it can be any
        // expression, otherwise treat it as `any` so the rest of the parameters can be checked.
        if let Some(arg) = arg.as_string() {
            let span = arg.source_span;
            let (mut ast, _) = parse_host_binding(allocator, parser, &arg.text, span);
            fixup_spans(&mut ast, arg);
            arg_nodes.push(ast);
        } else {
            // Represents `$any(0)`. We need to construct it manually in order to set the right spans.
            let arg_span = arg.source_span();
            let expression_span = AbsoluteSourceSpan::new(arg_span.start, arg_span.end);
            let any_read = AngularExpression::PropertyRead(Box::new_in(
                PropertyRead {
                    span: dummy_span,
                    source_span: expression_span,
                    name_span: expression_span,
                    receiver: AngularExpression::ImplicitReceiver(Box::new_in(
                        ImplicitReceiver { span: dummy_span, source_span: expression_span },
                        &allocator,
                    )),
                    name: Ident::from_in("$any", allocator),
                },
                &allocator,
            ));
            let mut any_args = ArenaVec::new_in(&allocator);
            any_args.push(AngularExpression::LiteralPrimitive(Box::new_in(
                LiteralPrimitive {
                    span: dummy_span,
                    source_span: expression_span,
                    value: LiteralValue::Number(0.0),
                },
                &allocator,
            )));
            let any_call = AngularExpression::Call(Box::new_in(
                Call {
                    span: dummy_span,
                    source_span: expression_span,
                    receiver: any_read,
                    args: any_args,
                    argument_span: expression_span,
                },
                &allocator,
            ));
            arg_nodes.push(any_call);
        }
    }

    let call_node = AngularExpression::Call(Box::new_in(
        Call {
            span: dummy_span,
            source_span: name_span,
            receiver,
            args: arg_nodes,
            // `dummySpan` (a `ParseSpan` where an `AbsoluteSourceSpan` goes)
            argument_span: AbsoluteSourceSpan::new(dummy_span.start, dummy_span.end),
        },
        &allocator,
    ));
    let r#type: ParsedEventType;
    let event_name: String;
    let phase: Option<String>;
    let target: Option<String>;

    if event_name_node.text.starts_with('@') {
        let (parsed_event_name, parsed_phase) =
            parse_legacy_animation_event_name(&event_name_node.text);
        r#type = ParsedEventType::LegacyAnimation;
        event_name = parsed_event_name;
        phase = parsed_phase;
        target = None;
    } else {
        let (parsed_event_name, parsed_target) = parse_event_listener_name(&event_name_node.text);
        r#type = if parsed_event_name.starts_with("animate.") {
            ParsedEventType::Animation
        } else {
            ParsedEventType::Regular
        };
        event_name = parsed_event_name;
        target = parsed_target;
        phase = None;
    }

    listeners.push(R3BoundEvent {
        name: Ident::from_in(event_name.as_str(), allocator),
        event_type: r#type,
        handler: call_node,
        target: target.map(|target| Ident::from_in(target.as_str(), allocator)),
        phase: phase.map(|phase| Ident::from_in(phase.as_str(), allocator)),
        source_span: decorator.decorator_span,
        handler_span: decorator.decorator_span,
        key_span: event_name_node.source_span,
    });
}

// host_bindings.ts:415 inferBoundAttribute
/// Infers the attribute name and binding type of a bound attribute based on its raw name.
fn infer_bound_attribute(name: &str) -> (String, BindingType) {
    let attr_prefix = "attr.";
    let class_prefix = "class.";
    let style_prefix = "style.";
    let animation_prefix = "animate.";
    let legacy_animation_prefix = "@";

    // Infer the binding type based on the prefix.
    if let Some(attr_name) = name.strip_prefix(attr_prefix) {
        (attr_name.to_string(), BindingType::Attribute)
    } else if let Some(attr_name) = name.strip_prefix(class_prefix) {
        (attr_name.to_string(), BindingType::Class)
    } else if let Some(attr_name) = name.strip_prefix(style_prefix) {
        (attr_name.to_string(), BindingType::Style)
    } else if name.starts_with(animation_prefix) {
        (name.to_string(), BindingType::Animation)
    } else if let Some(attr_name) = name.strip_prefix(legacy_animation_prefix) {
        (attr_name.to_string(), BindingType::LegacyAnimation)
    } else {
        (name.to_string(), BindingType::Property)
    }
}

// host_bindings.ts:453 fixupSpans
/// Adjusts the spans of a parsed AST so that they're appropriate for a host bindings context.
fn fixup_spans(ast: &mut AngularExpression<'_>, node: &StaticSourceNode) {
    // When parsing the initializer as a property/event binding, we use `.text` which excludes escaped
    // quotes and is generally what we want, because preserving them would result in a parser error,
    // however it has the downside in that the spans of the expressions following the escaped
    // characters are no longer accurate relative to the source code. The more escaped characters
    // there are before a node, the more inaccurate its span will be. If we detect cases like that,
    // we override the spans of all nodes following the escaped string to point to the entire
    // initializer string so that we don't surface diagnostics with mangled spans. This isn't ideal,
    // but is likely rare in user code. (See the TypeScript for the workarounds that were discarded.)
    let escape_index = node.source.get(1..).and_then(|rest| rest.find('\\')).map(|index| index + 1);

    if let Some(escape_index) = escape_index {
        let start = node.source_span.start;
        let end = node.source_span.end;
        let new_span = ParseSpan::new(0, end - start);
        let new_source_span = AbsoluteSourceSpan::new(start, end);
        // `ast.visit(visitor)` on the `ASTWithSource`: the visitor has no `visitASTWithSource`, so the wrapped
        // expression's own `visit` dispatches straight to its children, and only they go through the overridden
        // `visit`.
        let visitor = ReplaceSpanVisitor::new(
            u32::try_from(escape_index).expect("an offset"),
            new_span,
            new_source_span,
        );
        visitor.visit_children(ast);
    }
}

// host_bindings.ts:493 ReplaceSpanVisitor
/// Visitor that replaces the spans of all nodes with new ones,
/// if they're defined after a specific index.
struct ReplaceSpanVisitor {
    after_index: u32,
    override_span: ParseSpan,
    override_source_span: AbsoluteSourceSpan,
}

impl ReplaceSpanVisitor {
    // host_bindings.ts:494 constructor
    fn new(
        after_index: u32,
        override_span: ParseSpan,
        override_source_span: AbsoluteSourceSpan,
    ) -> Self {
        Self { after_index, override_span, override_source_span }
    }

    // host_bindings.ts:502 visit
    fn visit(&self, ast: &mut AngularExpression<'_>) {
        use AngularExpression as E;
        // Only nodes after the index need to be adjusted, but all nodes should be visited.
        let span = ast.span();
        if span.start >= self.after_index || span.end >= self.after_index {
            let (span, source_span) = expression_spans_mut(ast);
            *span = self.override_span;
            *source_span = self.override_source_span;

            // `ast instanceof ASTWithName`
            match ast {
                E::PropertyRead(e) => e.name_span = self.override_source_span,
                E::SafePropertyRead(e) => e.name_span = self.override_source_span,
                E::BindingPipe(e) => e.name_span = self.override_source_span,
                _ => {}
            }

            match ast {
                E::Call(e) => e.argument_span = self.override_source_span,
                E::SafeCall(e) => e.argument_span = self.override_source_span,
                _ => {}
            }
        }
        // `super.visit(ast)`
        self.visit_children(ast);
    }

    /// `ast.visit(this)` of `RecursiveAstVisitor` (`expression_parser/ast.ts:742`): visits the node's children
    fn visit_children(&self, ast: &mut AngularExpression<'_>) {
        use AngularExpression as E;
        match ast {
            E::Unary(e) => self.visit(&mut e.expr),
            E::Binary(e) => {
                self.visit(&mut e.left);
                self.visit(&mut e.right);
            }
            E::Chain(e) => self.visit_all(&mut e.expressions),
            E::Conditional(e) => {
                self.visit(&mut e.condition);
                self.visit(&mut e.true_exp);
                self.visit(&mut e.false_exp);
            }
            E::BindingPipe(e) => {
                self.visit(&mut e.exp);
                self.visit_all(&mut e.args);
            }
            E::Interpolation(e) => self.visit_all(&mut e.expressions),
            E::KeyedRead(e) => {
                self.visit(&mut e.receiver);
                self.visit(&mut e.key);
            }
            E::LiteralArray(e) => self.visit_all(&mut e.expressions),
            E::LiteralMap(e) => self.visit_all(&mut e.values),
            E::PrefixNot(e) => self.visit(&mut e.expression),
            E::TypeofExpression(e) => self.visit(&mut e.expression),
            E::VoidExpression(e) => self.visit(&mut e.expression),
            E::NonNullAssert(e) => self.visit(&mut e.expression),
            E::PropertyRead(e) => self.visit(&mut e.receiver),
            E::SafePropertyRead(e) => self.visit(&mut e.receiver),
            E::SafeKeyedRead(e) => {
                self.visit(&mut e.receiver);
                self.visit(&mut e.key);
            }
            E::Call(e) => {
                self.visit(&mut e.receiver);
                self.visit_all(&mut e.args);
            }
            E::SafeCall(e) => {
                self.visit(&mut e.receiver);
                self.visit_all(&mut e.args);
            }
            E::TemplateLiteral(e) => self.visit_template_literal(e),
            E::TaggedTemplateLiteral(e) => {
                self.visit(&mut e.tag);
                // `this.visit(ast.template)`: the template literal goes through `visit` too.
                self.visit_template_literal_node(&mut e.template);
            }
            E::ParenthesizedExpression(e) => self.visit(&mut e.expression),
            E::ArrowFunction(e) => self.visit(&mut e.body),
            E::SpreadElement(e) => self.visit(&mut e.expression),
            E::ImplicitReceiver(_)
            | E::ThisReceiver(_)
            | E::LiteralPrimitive(_)
            | E::RegularExpressionLiteral(_)
            | E::Empty(_) => {}
        }
    }

    /// `visitAll`
    fn visit_all(&self, asts: &mut [AngularExpression<'_>]) {
        for ast in asts {
            self.visit(ast);
        }
    }

    /// `visitTemplateLiteral`: the elements (which are `AST`s in Angular) and the expressions, interleaved
    fn visit_template_literal(
        &self,
        ast: &mut oxc_angular_compiler::ast::expression::TemplateLiteral<'_>,
    ) {
        for i in 0..ast.elements.len() {
            let element = &mut ast.elements[i];
            if element.span.start >= self.after_index || element.span.end >= self.after_index {
                element.span = self.override_span;
                element.source_span = self.override_source_span;
            }
            if let Some(expression) = ast.expressions.get_mut(i) {
                self.visit(expression);
            }
        }
    }

    /// `visit(template)` for a tagged template's template literal (not an `AngularExpression` in the fork)
    fn visit_template_literal_node(
        &self,
        ast: &mut oxc_angular_compiler::ast::expression::TemplateLiteral<'_>,
    ) {
        if ast.span.start >= self.after_index || ast.span.end >= self.after_index {
            ast.span = self.override_span;
            ast.source_span = self.override_source_span;
        }
        self.visit_template_literal(ast);
    }
}

/// `ast.span` and `ast.sourceSpan`, to assign
fn expression_spans_mut<'e>(
    ast: &'e mut AngularExpression<'_>,
) -> (&'e mut ParseSpan, &'e mut AbsoluteSourceSpan) {
    use AngularExpression as E;
    macro_rules! spans {
        ($e:expr) => {{
            let e = &mut **$e;
            (&mut e.span, &mut e.source_span)
        }};
    }
    match ast {
        E::Empty(e) => spans!(e),
        E::ImplicitReceiver(e) => spans!(e),
        E::ThisReceiver(e) => spans!(e),
        E::Chain(e) => spans!(e),
        E::Conditional(e) => spans!(e),
        E::PropertyRead(e) => spans!(e),
        E::SafePropertyRead(e) => spans!(e),
        E::KeyedRead(e) => spans!(e),
        E::SafeKeyedRead(e) => spans!(e),
        E::BindingPipe(e) => spans!(e),
        E::LiteralPrimitive(e) => spans!(e),
        E::LiteralArray(e) => spans!(e),
        E::LiteralMap(e) => spans!(e),
        E::Interpolation(e) => spans!(e),
        E::Binary(e) => spans!(e),
        E::Unary(e) => spans!(e),
        E::PrefixNot(e) => spans!(e),
        E::TypeofExpression(e) => spans!(e),
        E::VoidExpression(e) => spans!(e),
        E::NonNullAssert(e) => spans!(e),
        E::Call(e) => spans!(e),
        E::SafeCall(e) => spans!(e),
        E::TaggedTemplateLiteral(e) => spans!(e),
        E::TemplateLiteral(e) => spans!(e),
        E::ParenthesizedExpression(e) => spans!(e),
        E::RegularExpressionLiteral(e) => spans!(e),
        E::SpreadElement(e) => spans!(e),
        E::ArrowFunction(e) => spans!(e),
    }
}

/// `new ParseSpan(-1, -1)` (see the module comment)
fn dummy_span() -> ParseSpan {
    ParseSpan::new(u32::MAX, u32::MAX)
}

/// template_parser/binding_parser.ts:524 `parseBinding(value, /* isHostBinding */ true, sourceSpan,
/// sourceSpan.start.offset)`: `parseSimpleBinding`, whose errors include the `SimpleExpressionChecker`'s (pipes are
/// not allowed in host bindings). Returns the expression and its `errors`.
fn parse_host_binding<'a>(
    allocator: &'a Allocator,
    parser: &BindingParser<'a>,
    value: &str,
    source_span: Span,
) -> (AngularExpression<'a>, Vec<String>) {
    let value: &'a str = allocator.alloc_str(value);
    let result = parser.parse_binding(value, source_span);
    let mut errors: Vec<String> = result.errors.iter().map(|error| format!("{error:?}")).collect();
    let checker_errors = SimpleExpressionChecker::check(&result.ast);
    if !checker_errors.is_empty() {
        errors.push(format!("Host binding expression cannot contain {}", checker_errors.join(" ")));
    }
    (result.ast, errors)
}

/// A `ParsedEvent` (expression_parser/ast.ts), as `parse_event` makes one
struct ParsedEvent<'a> {
    name: String,
    target_or_phase: Option<String>,
    r#type: ParsedEventType,
    handler: AngularExpression<'a>,
    /// `handler.errors`
    handler_errors: Vec<String>,
    source_span: Span,
    handler_span: Span,
    key_span: Span,
}

/// template_parser/binding_parser.ts:646 `parseEvent(name, expression, /* isAssignmentEvent */ false, sourceSpan,
/// handlerSpan, [], targetEvents, keySpan)`: the event it pushes (its error reports are not used here).
fn parse_event<'a>(
    allocator: &'a Allocator,
    parser: &BindingParser<'a>,
    name: &str,
    expression: &str,
    source_span: Span,
    handler_span: Span,
    key_span: Span,
) -> ParsedEvent<'a> {
    // `isLegacyAnimationLabel(name)`
    if let Some(name) = name.strip_prefix('@') {
        let key_span = Span::new(key_span.start + 1, key_span.end);
        // binding_parser.ts:709 _parseLegacyAnimationEvent
        let (event_name, phase) = parse_legacy_animation_event_name(name);
        let (handler, handler_errors) = parse_action(allocator, parser, expression, handler_span);
        ParsedEvent {
            name: event_name,
            target_or_phase: phase,
            r#type: ParsedEventType::LegacyAnimation,
            handler,
            handler_errors,
            source_span,
            handler_span,
            key_span,
        }
    } else {
        // binding_parser.ts:749 _parseRegularEvent
        // long format: 'target: eventName'
        let (event_name, target) = parse_event_listener_name(name);
        let (handler, handler_errors) = parse_action(allocator, parser, expression, handler_span);

        let mut event_type = ParsedEventType::Regular;
        if name.starts_with("animate.") {
            event_type = ParsedEventType::Animation;
        }

        ParsedEvent {
            name: event_name,
            target_or_phase: target,
            r#type: event_type,
            handler,
            handler_errors,
            source_span,
            handler_span,
            key_span,
        }
    }
}

/// template_parser/binding_parser.ts:787 _parseAction: the handler and its `errors`. An empty expression becomes the
/// literal `'ERROR'` (`wrapLiteralPrimitive`), without errors.
fn parse_action<'a>(
    allocator: &'a Allocator,
    parser: &BindingParser<'a>,
    value: &str,
    source_span: Span,
) -> (AngularExpression<'a>, Vec<String>) {
    let absolute_offset = source_span.start;
    let value: &'a str = allocator.alloc_str(value);
    let result = parser.parse_event(value, source_span);
    if matches!(result.ast, AngularExpression::Empty(_)) {
        // `this._exprParser.wrapLiteralPrimitive('ERROR', sourceSpan, absoluteOffset)`
        let error = "ERROR";
        let span = ParseSpan::new(0, u32::try_from(error.len()).expect("a length"));
        let literal = AngularExpression::LiteralPrimitive(Box::new_in(
            LiteralPrimitive {
                span,
                source_span: span.to_absolute(absolute_offset),
                value: LiteralValue::String(Ident::from_in(error, allocator)),
            },
            &allocator,
        ));
        return (literal, Vec::new());
    }
    let errors = result.errors.iter().map(|error| format!("{error:?}")).collect();
    (result.ast, errors)
}

/// template_parser/binding_parser.ts:699 parseEventListenerName: `(eventName, target)`
fn parse_event_listener_name(raw_name: &str) -> (String, Option<String>) {
    match split_at(raw_name, ':') {
        Some((target, event_name)) => (event_name, Some(target)),
        None => (raw_name.to_string(), None),
    }
}

/// template_parser/binding_parser.ts:704 parseLegacyAnimationEventName: `(eventName, phase)`
fn parse_legacy_animation_event_name(raw_name: &str) -> (String, Option<String>) {
    match split_at(raw_name, '.') {
        Some((event_name, phase)) => (event_name, Some(phase.to_lowercase())),
        None => (raw_name.to_string(), None),
    }
}

/// util.ts:25 _splitAt: the trimmed parts before and after the first `character`
fn split_at(input: &str, character: char) -> Option<(String, String)> {
    let character_index = input.find(character)?;
    Some((
        js_trim(&input[..character_index]).to_string(),
        js_trim(&input[character_index + character.len_utf8()..]).to_string(),
    ))
}

/// JavaScript's `String.prototype.trim`
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

// render3/r3_ast.ts:133 BoundEvent.fromParsedEvent
fn bound_event_from_parsed_event<'a>(
    allocator: &'a Allocator,
    event: ParsedEvent<'a>,
) -> R3BoundEvent<'a> {
    let target =
        if event.r#type == ParsedEventType::Regular { event.target_or_phase.clone() } else { None };
    let phase =
        if event.r#type == ParsedEventType::LegacyAnimation { event.target_or_phase } else { None };
    R3BoundEvent {
        name: Ident::from_in(event.name.as_str(), allocator),
        event_type: event.r#type,
        handler: event.handler,
        target: target.map(|target| Ident::from_in(target.as_str(), allocator)),
        phase: phase.map(|phase| Ident::from_in(phase.as_str(), allocator)),
        source_span: event.source_span,
        handler_span: event.handler_span,
        key_span: event.key_span,
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_angular_compiler::ast::expression::{AngularExpression, BindingType, ParsedEventType};
    use oxc_span::Span;
    use oxc_str::Ident;

    use super::{
        HostBindingDecorator, HostListenerDecorator, HostObjectLiteralBinding, SourceNode,
        StaticSourceNode, StaticSourceNodeKind, create_host_bindings_block_guard,
        create_host_element,
    };

    /// A string literal `'text'` at `start` in the TypeScript source
    fn string(text: &str, start: u32) -> StaticSourceNode {
        let source = format!("'{text}'");
        let end = start + u32::try_from(source.len()).unwrap();
        StaticSourceNode {
            kind: StaticSourceNodeKind::String,
            source,
            text: text.to_string(),
            source_span: Span::new(start, end),
        }
    }

    fn identifier(text: &str, start: u32) -> StaticSourceNode {
        let end = start + u32::try_from(text.len()).unwrap();
        StaticSourceNode {
            kind: StaticSourceNodeKind::Identifier,
            source: text.to_string(),
            text: text.to_string(),
            source_span: Span::new(start, end),
        }
    }

    #[test]
    fn guard() {
        assert_eq!(create_host_bindings_block_guard(), "(true /*hostBindingsBlockGuard*/)");
    }

    #[test]
    fn no_bindings() {
        let allocator = Allocator::default();
        assert!(
            create_host_element(&allocator, "directive", None, Span::new(0, 1), &[], &[], &[])
                .is_none()
        );
    }

    #[test]
    fn host_element() {
        let allocator = Allocator::default();
        let literal = [
            HostObjectLiteralBinding {
                key: SourceNode::Static(string("[class.active]", 10)),
                value: SourceNode::Static(string("isActive", 28)),
                source_span: Span::new(10, 38),
            },
            HostObjectLiteralBinding {
                key: SourceNode::Static(string("(window:click)", 40)),
                value: SourceNode::Static(string("onClick($event)", 58)),
                source_span: Span::new(40, 75),
            },
            HostObjectLiteralBinding {
                key: SourceNode::Static(string("(@fade.done)", 80)),
                value: SourceNode::Static(string("done()", 94)),
                source_span: Span::new(80, 102),
            },
            // Not a static string: skipped.
            HostObjectLiteralBinding {
                key: SourceNode::Static(string("[title]", 110)),
                value: SourceNode::Unspecified { source_span: Span::new(120, 125) },
                source_span: Span::new(110, 125),
            },
            // A pipe in a host binding is an error: skipped.
            HostObjectLiteralBinding {
                key: SourceNode::Static(string("[id]", 130)),
                value: SourceNode::Static(string("a | b", 138)),
                source_span: Span::new(130, 145),
            },
        ];
        let decorators = [HostBindingDecorator {
            member_name: identifier("disabled", 200),
            member_span: Span::new(200, 208),
            arguments: vec![SourceNode::Static(string("attr.aria-disabled", 180))],
            decorator_span: Span::new(170, 199),
        }];
        let listeners = [HostListenerDecorator {
            event_name: Some(SourceNode::Static(string("keydown", 300))),
            member_name: identifier("onKey", 330),
            member_span: Span::new(330, 335),
            arguments: vec![
                SourceNode::Static(string("$event", 310)),
                SourceNode::Unspecified { source_span: Span::new(320, 325) },
            ],
            decorator_span: Span::new(290, 328),
        }];

        let host = create_host_element(
            &allocator,
            "component",
            None,
            Span::new(5, 9),
            &literal,
            &decorators,
            &listeners,
        )
        .expect("a host element");
        assert_eq!(host.tag_names.iter().map(Ident::as_str).collect::<Vec<_>>(), ["ng-component"]);
        assert_eq!(host.source_span, Span::new(5, 9));

        let bindings: Vec<(&str, BindingType)> =
            host.bindings.iter().map(|b| (b.name.as_str(), b.binding_type)).collect();
        assert_eq!(
            bindings,
            [("active", BindingType::Class), ("aria-disabled", BindingType::Attribute)]
        );
        assert!(matches!(host.bindings[1].value, AngularExpression::PropertyRead(_)));

        let events: Vec<(&str, ParsedEventType, Option<&str>, Option<&str>)> = host
            .listeners
            .iter()
            .map(|l| {
                (
                    l.name.as_str(),
                    l.event_type,
                    l.target.as_ref().map(Ident::as_str),
                    l.phase.as_ref().map(Ident::as_str),
                )
            })
            .collect();
        assert_eq!(
            events,
            [
                ("click", ParsedEventType::Regular, Some("window"), None),
                ("fade", ParsedEventType::LegacyAnimation, None, Some("done")),
                ("keydown", ParsedEventType::Regular, None, None),
            ]
        );
        // The legacy animation listener's key span skips the `@`.
        assert_eq!(host.listeners[1].key_span, Span::new(81, 94));
        let AngularExpression::Call(call) = &host.listeners[2].handler else { panic!("a call") };
        assert_eq!(call.args.len(), 2);
        assert!(matches!(call.args[1], AngularExpression::Call(_)));
    }

    #[test]
    fn escaped_strings_get_the_whole_initializer_span() {
        let allocator = Allocator::default();
        // `'a + \'b\' + c'` in the source; its text is `a + 'b' + c`.
        let value = StaticSourceNode {
            kind: StaticSourceNodeKind::String,
            source: "'a + \\'b\\' + c'".to_string(),
            text: "a + 'b' + c".to_string(),
            source_span: Span::new(100, 115),
        };
        let literal = [HostObjectLiteralBinding {
            key: SourceNode::Static(string("[title]", 90)),
            value: SourceNode::Static(value),
            source_span: Span::new(90, 115),
        }];
        let host =
            create_host_element(&allocator, "directive", None, Span::new(0, 1), &literal, &[], &[])
                .expect("a host element");
        let AngularExpression::Binary(sum) = &host.bindings[0].value else { panic!("a binary") };
        // The root keeps its span; the nodes after the escape (index 5 of the source) span the initializer.
        assert_eq!((sum.source_span.start, sum.source_span.end), (100, 111));
        let AngularExpression::Binary(left) = &sum.left else { panic!("a binary") };
        assert_eq!((left.source_span.start, left.source_span.end), (100, 115));
        let AngularExpression::PropertyRead(c) = &sum.right else { panic!("a property read") };
        assert_eq!((c.name_span.start, c.name_span.end), (100, 115));
    }
}
