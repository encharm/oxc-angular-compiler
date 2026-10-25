//! R3 nodes to the schema: the nodes in order, their expressions as trees, and what a binder knows of them.

use std::collections::HashMap;

use oxc_angular_compiler::ast::expression::{
    ASTWithSource, AngularExpression, BindingType, LiteralMapKey, LiteralValue, UnaryOperator,
};
use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Element, R3LetDeclaration, R3Node, R3Reference, R3Template,
    R3TemplateAttr, R3TextAttribute, R3Variable,
};
use oxc_span::Span;

use crate::positions::Positions;
use crate::schema::{
    Attr, BindingKind, BoundText, Consumer, Content, DeferBlock, DirectiveMatch, Element, Expr,
    ExprKind, ExprTarget, ForBlock, IfBlock, IfBranch, Input, Leaf, LetDeclaration, Node, NodeId,
    ObjectEntry, Output, Reference, ReferenceTarget, SwitchBlock, SwitchGroup, Template,
    TemplateAttr, Text, UnknownBlock, Variable,
};

/// An element or a template: what directives match on.
#[derive(Clone, Copy)]
pub enum Owner<'a> {
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
}

/// A binding a directive input or output can consume.
#[derive(Clone, Copy)]
pub enum BindingRef<'a> {
    Attribute(&'a R3TextAttribute<'a>),
    Input(&'a R3BoundAttribute<'a>),
    Output(&'a R3BoundEvent<'a>),
}

/// What a reference names, as a binder answers it.
pub enum BoundReference<'a> {
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
    /// A directive matched on `owner`, by its class name
    Directive {
        owner: Owner<'a>,
        directive: String,
    },
}

/// The template entity a name resolves to, as a binder answers it.
pub enum BoundEntity<'a> {
    Reference(&'a R3Reference<'a>),
    Variable(&'a R3Variable<'a>),
    Let(&'a R3LetDeclaration<'a>),
}

/// What binding the template against its component's declarations tells (ng-check's binder); every answer
/// defaults to unknown.
pub trait Binder<'a> {
    /// The directives and components matched on an element or a template.
    fn matches(&self, _owner: Owner<'a>) -> Option<Vec<DirectiveMatch>> {
        None
    }
    fn reference(&self, _reference: &'a R3Reference<'a>) -> Option<BoundReference<'a>> {
        None
    }
    /// For a name read with no receiver: the template entity it is; none for a component member.
    fn expression_target(&self, _expr: &'a AngularExpression<'a>) -> Option<BoundEntity<'a>> {
        None
    }
    fn consumer(&self, _binding: BindingRef<'a>) -> Option<Consumer> {
        None
    }
}

/// A binder that knows nothing: the tree alone.
pub struct NoBinder;
impl Binder<'_> for NoBinder {}

/// Whether whitespace-only text nodes are written; whitespace inside other text always is.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteOptions {
    pub whitespace_text: bool,
}

fn addr<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

/// The ids of the nodes, and the nodes that declare each reference and variable.
#[derive(Default)]
struct Ids {
    nodes: HashMap<usize, NodeId>,
    reference_owners: HashMap<usize, NodeId>,
    variable_scopes: HashMap<usize, NodeId>,
    next: NodeId,
}

impl Ids {
    fn assign<T>(&mut self, node: &T) -> NodeId {
        let id = self.next;
        self.next += 1;
        self.nodes.insert(addr(node), id);
        id
    }

    fn collect<'a>(&mut self, nodes: &'a [R3Node<'a>]) {
        for node in nodes {
            match node {
                R3Node::Element(e) => {
                    let id = self.assign(&**e);
                    for r in &e.references {
                        self.reference_owners.insert(addr(r), id);
                    }
                    self.collect(&e.children);
                }
                R3Node::Template(t) => {
                    let id = self.assign(&**t);
                    for r in &t.references {
                        self.reference_owners.insert(addr(r), id);
                    }
                    for v in &t.variables {
                        self.variable_scopes.insert(addr(v), id);
                    }
                    self.collect(&t.children);
                }
                R3Node::IfBlock(block) => {
                    self.assign(&**block);
                    for b in &block.branches {
                        let id = self.assign(b);
                        if let Some(alias) = &b.expression_alias {
                            self.variable_scopes.insert(addr(alias), id);
                        }
                        self.collect(&b.children);
                    }
                }
                R3Node::ForLoopBlock(block) => {
                    let id = self.assign(&**block);
                    self.variable_scopes.insert(addr(&block.item), id);
                    for v in &block.context_variables {
                        self.variable_scopes.insert(addr(v), id);
                    }
                    self.collect(&block.children);
                    if let Some(empty) = &block.empty {
                        self.collect(&empty.children);
                    }
                }
                R3Node::SwitchBlock(block) => {
                    self.assign(&**block);
                    for g in &block.groups {
                        self.collect(&g.children);
                    }
                }
                R3Node::DeferredBlock(block) => {
                    self.assign(&**block);
                    self.collect(&block.children);
                    if let Some(p) = &block.placeholder {
                        self.collect(&p.children);
                    }
                    if let Some(l) = &block.loading {
                        self.collect(&l.children);
                    }
                    if let Some(e) = &block.error {
                        self.collect(&e.children);
                    }
                }
                R3Node::LetDeclaration(l) => {
                    self.assign(&**l);
                }
                _ => {}
            }
        }
    }

    fn node<T>(&self, node: &T) -> NodeId {
        self.nodes.get(&addr(node)).copied().unwrap_or(NodeId::MAX)
    }
}

/// Writes nodes parsed from one source.
pub struct Writer<'s, 'b, 'a> {
    positions: Positions<'s>,
    binder: &'b dyn Binder<'a>,
    options: WriteOptions,
    ids: Ids,
}

impl<'s, 'b, 'a> Writer<'s, 'b, 'a> {
    /// `source` is the text the nodes' spans index into.
    pub fn new(source: &'s str, binder: &'b dyn Binder<'a>, options: WriteOptions) -> Self {
        Self { positions: Positions::new(source), binder, options, ids: Ids::default() }
    }

    pub fn positions(&self) -> &Positions<'s> {
        &self.positions
    }

    /// The nodes, ids given in document order.
    pub fn write(&mut self, nodes: &'a [R3Node<'a>]) -> Vec<Node> {
        self.ids = Ids::default();
        self.ids.collect(nodes);
        self.nodes(nodes)
    }

    fn position(&self, span: Span) -> crate::schema::Position {
        self.positions.of(span.start, span.end)
    }

    fn nodes(&self, nodes: &'a [R3Node<'a>]) -> Vec<Node> {
        nodes.iter().filter_map(|node| self.node(node)).collect()
    }

    fn node(&self, node: &'a R3Node<'a>) -> Option<Node> {
        Some(match node {
            R3Node::Text(t) => {
                if !self.options.whitespace_text && t.value.as_str().trim().is_empty() {
                    return None;
                }
                Node::Text(Text {
                    position: self.position(t.source_span),
                    value: t.value.as_str().to_string(),
                })
            }
            R3Node::BoundText(t) => Node::BoundText(BoundText {
                position: self.position(t.source_span),
                expr: self.expr(&t.value),
            }),
            R3Node::Element(e) => Node::Element(self.element(e)),
            R3Node::Template(t) => Node::Template(self.template(t)),
            R3Node::Content(c) => Node::Content(Content {
                position: self.position(c.source_span),
                selector: c.selector.as_str().to_string(),
            }),
            R3Node::Icu(i) => Node::Icu(Leaf { position: self.position(i.source_span) }),
            R3Node::IfBlock(block) => Node::If(IfBlock {
                id: self.ids.node(&**block),
                position: self.position(block.source_span),
                branches: block
                    .branches
                    .iter()
                    .map(|b| IfBranch {
                        id: self.ids.node(b),
                        position: self.position(b.source_span),
                        condition: b.expression.as_ref().map(|e| self.expr(e)),
                        alias: b.expression_alias.as_ref().map(|a| a.name.as_str().to_string()),
                        children: self.nodes(&b.children),
                    })
                    .collect(),
            }),
            R3Node::ForLoopBlock(block) => Node::For(ForBlock {
                id: self.ids.node(&**block),
                position: self.position(block.source_span),
                expr: self.ast_with_source(&block.expression),
                item: block.item.name.as_str().to_string(),
                track: self.ast_with_source(&block.track_by),
                context_vars: block.context_variables.iter().map(variable).collect(),
                children: self.nodes(&block.children),
                empty: block.empty.as_ref().map(|e| self.nodes(&e.children)),
            }),
            R3Node::SwitchBlock(block) => Node::Switch(SwitchBlock {
                id: self.ids.node(&**block),
                position: self.position(block.source_span),
                expr: self.expr(&block.expression),
                groups: block
                    .groups
                    .iter()
                    .map(|g| SwitchGroup {
                        cases: g
                            .cases
                            .iter()
                            .map(|c| c.expression.as_ref().map(|e| self.expr(e)))
                            .collect(),
                        children: self.nodes(&g.children),
                    })
                    .collect(),
            }),
            R3Node::DeferredBlock(block) => Node::Defer(DeferBlock {
                id: self.ids.node(&**block),
                position: self.position(block.source_span),
                children: self.nodes(&block.children),
                placeholder: block.placeholder.as_ref().map(|b| self.nodes(&b.children)),
                loading: block.loading.as_ref().map(|b| self.nodes(&b.children)),
                error: block.error.as_ref().map(|b| self.nodes(&b.children)),
            }),
            R3Node::LetDeclaration(l) => Node::Let(LetDeclaration {
                id: self.ids.node(&**l),
                position: self.position(l.source_span),
                name: l.name.as_str().to_string(),
                expr: self.expr(&l.value),
            }),
            R3Node::UnknownBlock(b) => Node::UnknownBlock(UnknownBlock {
                position: self.position(b.source_span),
                name: b.name.as_str().to_string(),
            }),
            // Written with what owns them (a branch, a reference, a variable), or not written: comments and the host
            // element type checking adds
            R3Node::Comment(_)
            | R3Node::Variable(_)
            | R3Node::Reference(_)
            | R3Node::HostElement(_)
            | R3Node::IfBlockBranch(_)
            | R3Node::ForLoopBlockEmpty(_)
            | R3Node::SwitchBlockCaseGroup(_)
            | R3Node::DeferredBlockPlaceholder(_)
            | R3Node::DeferredBlockLoading(_)
            | R3Node::DeferredBlockError(_) => return None,
            R3Node::Component(c) => {
                Node::Selectorless(Leaf { position: self.position(c.source_span) })
            }
            R3Node::Directive(d) => {
                Node::Selectorless(Leaf { position: self.position(d.source_span) })
            }
        })
    }

    fn element(&self, e: &'a R3Element<'a>) -> Element {
        Element {
            id: self.ids.node(e),
            position: self.position(e.source_span),
            name: e.name.as_str().to_string(),
            attrs: e.attributes.iter().map(|a| self.attr(a)).collect(),
            inputs: e.inputs.iter().map(|i| self.input(i)).collect(),
            outputs: e.outputs.iter().map(|o| self.output(o)).collect(),
            refs: e.references.iter().map(|r| self.reference(r)).collect(),
            self_closing: e.is_self_closing || e.is_void,
            matches: self.binder.matches(Owner::Element(e)),
            children: self.nodes(&e.children),
        }
    }

    fn template(&self, t: &'a R3Template<'a>) -> Template {
        Template {
            id: self.ids.node(t),
            position: self.position(t.source_span),
            tag: t.tag_name.as_ref().map(|tag| tag.as_str().to_string()),
            attrs: t.attributes.iter().map(|a| self.attr(a)).collect(),
            inputs: t.inputs.iter().map(|i| self.input(i)).collect(),
            outputs: t.outputs.iter().map(|o| self.output(o)).collect(),
            refs: t.references.iter().map(|r| self.reference(r)).collect(),
            template_attrs: t
                .template_attrs
                .iter()
                .map(|a| match a {
                    R3TemplateAttr::Bound(b) => TemplateAttr {
                        name: b.name.as_str().to_string(),
                        value: None,
                        expr: Some(self.expr(&b.value)),
                        consumer: self.binder.consumer(BindingRef::Input(b)),
                    },
                    R3TemplateAttr::Text(a) => TemplateAttr {
                        name: a.name.as_str().to_string(),
                        value: Some(a.value.as_str().to_string()),
                        expr: None,
                        consumer: self.binder.consumer(BindingRef::Attribute(a)),
                    },
                })
                .collect(),
            vars: t.variables.iter().map(variable).collect(),
            matches: self.binder.matches(Owner::Template(t)),
            children: self.nodes(&t.children),
        }
    }

    fn attr(&self, a: &'a R3TextAttribute<'a>) -> Attr {
        Attr {
            name: a.name.as_str().to_string(),
            value: a.value.as_str().to_string(),
            position: self.position(a.source_span),
            consumer: self.binder.consumer(BindingRef::Attribute(a)),
        }
    }

    fn input(&self, b: &'a R3BoundAttribute<'a>) -> Input {
        Input {
            name: b.name.as_str().to_string(),
            binding: match b.binding_type {
                BindingType::Property => BindingKind::Property,
                BindingType::Attribute => BindingKind::Attribute,
                BindingType::Class => BindingKind::Class,
                BindingType::Style => BindingKind::Style,
                BindingType::TwoWay => BindingKind::TwoWay,
                BindingType::Animation | BindingType::LegacyAnimation => BindingKind::Animation,
            },
            expr: self.expr(&b.value),
            position: self.position(b.source_span),
            consumer: self.binder.consumer(BindingRef::Input(b)),
        }
    }

    fn output(&self, o: &'a R3BoundEvent<'a>) -> Output {
        Output {
            name: o.name.as_str().to_string(),
            handler: self.expr(&o.handler),
            position: self.position(o.source_span),
            consumer: self.binder.consumer(BindingRef::Output(o)),
        }
    }

    fn reference(&self, r: &'a R3Reference<'a>) -> Reference {
        let target = self.binder.reference(r).map(|t| match t {
            BoundReference::Element(e) => ReferenceTarget::Element { node: self.ids.node(e) },
            BoundReference::Template(t) => ReferenceTarget::Template { node: self.ids.node(t) },
            BoundReference::Directive { owner, directive } => {
                let node = match owner {
                    Owner::Element(e) => self.ids.node(e),
                    Owner::Template(t) => self.ids.node(t),
                };
                ReferenceTarget::Directive { node, directive }
            }
        });
        Reference { name: r.name.as_str().to_string(), value: r.value.as_str().to_string(), target }
    }

    fn ast_with_source(&self, ast: &'a ASTWithSource<'a>) -> Expr {
        self.expr(&ast.ast)
    }

    fn boxed(&self, e: &'a AngularExpression<'a>) -> Box<Expr> {
        Box::new(self.expr(e))
    }

    fn exprs(&self, list: &'a [AngularExpression<'a>]) -> Vec<Expr> {
        list.iter().map(|e| self.expr(e)).collect()
    }

    /// An expression as a tree; its `source` is the text it spans.
    pub fn expr(&self, e: &'a AngularExpression<'a>) -> Expr {
        use AngularExpression as E;
        let span = e.source_span();
        let kind = match e {
            E::Empty(_) => ExprKind::Empty,
            E::ImplicitReceiver(_) | E::ThisReceiver(_) => ExprKind::This,
            E::Chain(c) => ExprKind::Chain { exprs: self.exprs(&c.expressions) },
            E::Conditional(c) => ExprKind::Conditional {
                condition: self.boxed(&c.condition),
                then: self.boxed(&c.true_exp),
                otherwise: self.boxed(&c.false_exp),
            },
            E::PropertyRead(r) => self.read(e, r.name.as_str(), &r.receiver, false),
            E::SafePropertyRead(r) => self.read(e, r.name.as_str(), &r.receiver, true),
            E::KeyedRead(k) => ExprKind::Keyed {
                receiver: self.boxed(&k.receiver),
                key: self.boxed(&k.key),
                safe: false,
            },
            E::SafeKeyedRead(k) => ExprKind::Keyed {
                receiver: self.boxed(&k.receiver),
                key: self.boxed(&k.key),
                safe: true,
            },
            E::BindingPipe(p) => ExprKind::Pipe {
                name: p.name.as_str().to_string(),
                input: self.boxed(&p.exp),
                args: self.exprs(&p.args),
            },
            E::LiteralPrimitive(l) => match &l.value {
                LiteralValue::String(s) => {
                    ExprKind::Literal { value: s.as_str().into(), undefined: None }
                }
                LiteralValue::Number(n) => ExprKind::Literal {
                    value: serde_json::Number::from_f64(*n)
                        .map_or(serde_json::Value::Null, serde_json::Value::Number),
                    undefined: None,
                },
                LiteralValue::Boolean(b) => {
                    ExprKind::Literal { value: (*b).into(), undefined: None }
                }
                LiteralValue::Null => {
                    ExprKind::Literal { value: serde_json::Value::Null, undefined: None }
                }
                LiteralValue::Undefined => {
                    ExprKind::Literal { value: serde_json::Value::Null, undefined: Some(true) }
                }
            },
            E::LiteralArray(a) => ExprKind::Array { items: self.exprs(&a.expressions) },
            E::LiteralMap(m) => ExprKind::Object {
                entries: m
                    .keys
                    .iter()
                    .zip(m.values.iter())
                    .map(|(k, v)| ObjectEntry {
                        key: match k {
                            LiteralMapKey::Property(p) => Some(p.key.as_str().to_string()),
                            LiteralMapKey::Spread(_) => None,
                        },
                        value: self.expr(v),
                    })
                    .collect(),
            },
            E::Interpolation(i) => ExprKind::Interpolation {
                strings: i.strings.iter().map(|s| s.as_str().to_string()).collect(),
                exprs: self.exprs(&i.expressions),
            },
            E::Binary(b) => ExprKind::Binary {
                operator: b.operation.as_str().to_string(),
                left: self.boxed(&b.left),
                right: self.boxed(&b.right),
            },
            E::Unary(u) => ExprKind::Unary {
                operator: match u.operator {
                    UnaryOperator::Plus => "+".to_string(),
                    UnaryOperator::Minus => "-".to_string(),
                },
                operand: self.boxed(&u.expr),
            },
            E::PrefixNot(n) => ExprKind::Not { operand: self.boxed(&n.expression) },
            E::TypeofExpression(t) => ExprKind::Typeof { operand: self.boxed(&t.expression) },
            E::VoidExpression(v) => ExprKind::Void { operand: self.boxed(&v.expression) },
            E::NonNullAssert(n) => ExprKind::NonNull { operand: self.boxed(&n.expression) },
            E::Call(c) => ExprKind::Call {
                callee: self.boxed(&c.receiver),
                args: self.exprs(&c.args),
                safe: false,
            },
            E::SafeCall(c) => ExprKind::Call {
                callee: self.boxed(&c.receiver),
                args: self.exprs(&c.args),
                safe: true,
            },
            E::TaggedTemplateLiteral(t) => ExprKind::TemplateLiteral {
                strings: t.template.elements.iter().map(|s| s.text.as_str().to_string()).collect(),
                exprs: self.exprs(&t.template.expressions),
                tag: Some(self.boxed(&t.tag)),
            },
            E::TemplateLiteral(t) => ExprKind::TemplateLiteral {
                strings: t.elements.iter().map(|s| s.text.as_str().to_string()).collect(),
                exprs: self.exprs(&t.expressions),
                tag: None,
            },
            E::ParenthesizedExpression(p) => ExprKind::Paren { operand: self.boxed(&p.expression) },
            E::RegularExpressionLiteral(r) => ExprKind::Regex {
                body: r.body.as_str().to_string(),
                flags: r.flags.as_ref().map(|f| f.as_str().to_string()),
            },
            E::SpreadElement(s) => ExprKind::Spread { operand: self.boxed(&s.expression) },
            E::ArrowFunction(a) => ExprKind::Arrow {
                params: a.parameters.iter().map(|p| p.name.as_str().to_string()).collect(),
                body: self.boxed(&a.body),
            },
        };
        Expr {
            source: self.positions.text(span.start, span.end).trim().to_string(),
            position: self.positions.of(span.start, span.end),
            kind,
        }
    }

    /// A name with no receiver of its own is read on the component or a template entity, as the binder resolves it.
    fn read(
        &self,
        e: &'a AngularExpression<'a>,
        name: &str,
        receiver: &'a AngularExpression<'a>,
        safe: bool,
    ) -> ExprKind {
        let implicit = matches!(
            receiver,
            AngularExpression::ImplicitReceiver(_) | AngularExpression::ThisReceiver(_)
        );
        if !implicit {
            return ExprKind::Read {
                name: name.to_string(),
                receiver: Some(self.boxed(receiver)),
                safe,
                target: None,
            };
        }
        let target = self.binder.expression_target(e).map(|entity| match entity {
            BoundEntity::Reference(r) => ExprTarget::Reference {
                node: self.ids.reference_owners.get(&addr(r)).copied().unwrap_or(NodeId::MAX),
            },
            BoundEntity::Variable(v) => ExprTarget::Variable {
                node: self.ids.variable_scopes.get(&addr(v)).copied().unwrap_or(NodeId::MAX),
            },
            BoundEntity::Let(l) => ExprTarget::Let { node: self.ids.node(l) },
        });
        ExprKind::Read { name: name.to_string(), receiver: None, safe, target }
    }
}

fn variable(v: &R3Variable<'_>) -> Variable {
    Variable { name: v.name.as_str().to_string(), value: v.value.as_str().to_string() }
}
