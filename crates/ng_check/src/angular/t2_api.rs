//! `packages/compiler/src/render3/view/t2_api.ts`: what a template binder knows about a template, over the fork's
//! template AST (`oxc_angular_compiler::ast::r3`).
//!
//! TypeScript keys the binder's maps by node object; here a node is identified by its address (`node_key`,
//! `expr_key`), and the node enums below compare and hash by it.

use std::hash::{Hash, Hasher};

use indexmap::IndexSet;
use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipe};
use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Component, R3Content, R3DeferredBlock, R3DeferredBlockError,
    R3DeferredBlockLoading, R3DeferredBlockPlaceholder, R3Directive, R3Element, R3HoverDeferredTrigger,
    R3InteractionDeferredTrigger, R3ViewportDeferredTrigger,
    R3ForLoopBlock, R3ForLoopBlockEmpty, R3HostElement, R3IfBlockBranch, R3LetDeclaration, R3Node,
    R3Reference, R3SwitchBlockCaseGroup, R3Template, R3TextAttribute, R3Variable,
};
use oxc_str::Ident;
use rustc_hash::FxBuildHasher;

use crate::angular::property_mapping::{AsInputOrOutput, ClassPropertyMapping, InputOrOutput};

/// The identity of a template AST node: its address (the struct inside its `Box`).
pub fn node_key<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

/// The identity of an expression: the address of its variant's boxed struct, so that a `&AngularExpression` and the
/// struct it boxes (`&PropertyRead`) have the same key.
pub fn expr_key(expr: &AngularExpression<'_>) -> usize {
    use AngularExpression as E;
    match expr {
        E::Empty(b) => node_key(&**b),
        E::ImplicitReceiver(b) => node_key(&**b),
        E::ThisReceiver(b) => node_key(&**b),
        E::Chain(b) => node_key(&**b),
        E::Conditional(b) => node_key(&**b),
        E::PropertyRead(b) => node_key(&**b),
        E::SafePropertyRead(b) => node_key(&**b),
        E::KeyedRead(b) => node_key(&**b),
        E::SafeKeyedRead(b) => node_key(&**b),
        E::BindingPipe(b) => node_key(&**b),
        E::LiteralPrimitive(b) => node_key(&**b),
        E::LiteralArray(b) => node_key(&**b),
        E::LiteralMap(b) => node_key(&**b),
        E::Interpolation(b) => node_key(&**b),
        E::Binary(b) => node_key(&**b),
        E::Unary(b) => node_key(&**b),
        E::PrefixNot(b) => node_key(&**b),
        E::TypeofExpression(b) => node_key(&**b),
        E::VoidExpression(b) => node_key(&**b),
        E::NonNullAssert(b) => node_key(&**b),
        E::Call(b) => node_key(&**b),
        E::SafeCall(b) => node_key(&**b),
        E::TaggedTemplateLiteral(b) => node_key(&**b),
        E::TemplateLiteral(b) => node_key(&**b),
        E::ParenthesizedExpression(b) => node_key(&**b),
        E::RegularExpressionLiteral(b) => node_key(&**b),
        E::SpreadElement(b) => node_key(&**b),
        E::ArrowFunction(b) => node_key(&**b),
    }
}

/// Equality and hashing by `key()`.
macro_rules! keyed {
    ($name:ident) => {
        impl PartialEq for $name<'_> {
            fn eq(&self, other: &Self) -> bool {
                self.key() == other.key()
            }
        }
        impl Eq for $name<'_> {}
        impl Hash for $name<'_> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.key().hash(state);
            }
        }
    };
}

// t2_api.ts:39 ScopedNode
// (Angular's `ContentBlock` is not in the fork's AST: its parser does not produce one.)
#[derive(Debug, Clone, Copy)]
pub enum ScopedNode<'a> {
    Template(&'a R3Template<'a>),
    SwitchBlockCaseGroup(&'a R3SwitchBlockCaseGroup<'a>),
    IfBlockBranch(&'a R3IfBlockBranch<'a>),
    ForLoopBlock(&'a R3ForLoopBlock<'a>),
    ForLoopBlockEmpty(&'a R3ForLoopBlockEmpty<'a>),
    DeferredBlock(&'a R3DeferredBlock<'a>),
    DeferredBlockError(&'a R3DeferredBlockError<'a>),
    DeferredBlockLoading(&'a R3DeferredBlockLoading<'a>),
    DeferredBlockPlaceholder(&'a R3DeferredBlockPlaceholder<'a>),
    Content(&'a R3Content<'a>),
    HostElement(&'a R3HostElement<'a>),
}

impl ScopedNode<'_> {
    pub fn key(&self) -> usize {
        match self {
            Self::Template(n) => node_key(*n),
            Self::SwitchBlockCaseGroup(n) => node_key(*n),
            Self::IfBlockBranch(n) => node_key(*n),
            Self::ForLoopBlock(n) => node_key(*n),
            Self::ForLoopBlockEmpty(n) => node_key(*n),
            Self::DeferredBlock(n) => node_key(*n),
            Self::DeferredBlockError(n) => node_key(*n),
            Self::DeferredBlockLoading(n) => node_key(*n),
            Self::DeferredBlockPlaceholder(n) => node_key(*n),
            Self::Content(n) => node_key(*n),
            Self::HostElement(n) => node_key(*n),
        }
    }
}
keyed!(ScopedNode);

// t2_api.ts:59 TemplateEntity
#[derive(Debug, Clone, Copy)]
pub enum TemplateEntity<'a> {
    Reference(&'a R3Reference<'a>),
    Variable(&'a R3Variable<'a>),
    LetDeclaration(&'a R3LetDeclaration<'a>),
}

impl TemplateEntity<'_> {
    pub fn key(&self) -> usize {
        match self {
            Self::Reference(n) => node_key(*n),
            Self::Variable(n) => node_key(*n),
            Self::LetDeclaration(n) => node_key(*n),
        }
    }
}
keyed!(TemplateEntity);

// t2_api.ts:62 DirectiveOwner
#[derive(Debug, Clone, Copy)]
pub enum DirectiveOwner<'a> {
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
    Component(&'a R3Component<'a>),
    Directive(&'a R3Directive<'a>),
    HostElement(&'a R3HostElement<'a>),
}

impl DirectiveOwner<'_> {
    pub fn key(&self) -> usize {
        match self {
            Self::Element(n) => node_key(*n),
            Self::Template(n) => node_key(*n),
            Self::Component(n) => node_key(*n),
            Self::Directive(n) => node_key(*n),
            Self::HostElement(n) => node_key(*n),
        }
    }
}
keyed!(DirectiveOwner);

// t2_api.ts:51 ReferenceTarget
#[derive(Debug, Clone)]
pub enum ReferenceTarget<'a, D> {
    /// `{directive, node}`: the reference is to a directive on `node` (never a `HostElement`)
    Directive { directive: D, node: DirectiveOwner<'a> },
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
}

/// t2_api.ts:162 getConsumerOfBinding's result: `DirectiveT | Element | Template`
#[derive(Debug, Clone)]
pub enum BindingConsumer<'a, D> {
    Directive(D),
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
}

/// t2_api.ts:159 getConsumerOfBinding's argument: `BoundAttribute | BoundEvent | TextAttribute`
#[derive(Debug, Clone, Copy)]
pub enum Binding<'a> {
    BoundAttribute(&'a R3BoundAttribute<'a>),
    BoundEvent(&'a R3BoundEvent<'a>),
    TextAttribute(&'a R3TextAttribute<'a>),
}

impl Binding<'_> {
    pub fn key(&self) -> usize {
        match self {
            Self::BoundAttribute(n) => node_key(*n),
            Self::BoundEvent(n) => node_key(*n),
            Self::TextAttribute(n) => node_key(*n),
        }
    }
}
keyed!(Binding);

/// `HoverDeferredTrigger | InteractionDeferredTrigger | ViewportDeferredTrigger`: the triggers that reference an
/// element.
///
/// `get_deferred_trigger_target` resolves them (Angular's takes any `DeferredTrigger` and returns `null` for
/// the others). The fork's `R3DeferredBlockTriggers` holds each trigger inline, by kind, so a `&R3DeferredTrigger`
/// cannot be made from one.
#[derive(Debug, Clone, Copy)]
pub enum ReferenceDeferredTrigger<'a> {
    Hover(&'a R3HoverDeferredTrigger<'a>),
    Interaction(&'a R3InteractionDeferredTrigger<'a>),
    Viewport(&'a R3ViewportDeferredTrigger<'a>),
}

impl<'a> ReferenceDeferredTrigger<'a> {
    /// `trigger.reference`
    pub fn reference(&self) -> Option<&'a str> {
        match self {
            Self::Hover(t) => t.reference.as_ref().map(Ident::as_str),
            Self::Interaction(t) => t.reference.as_ref().map(Ident::as_str),
            Self::Viewport(t) => t.reference.as_ref().map(Ident::as_str),
        }
    }
}

// t2_api.ts:65 ConflictingHostDirectiveBinding
#[derive(Debug, Clone)]
pub struct ConflictingHostDirectiveBinding<D> {
    pub directive: D,
    pub class_property_name: String,
    pub conflicting_aliases: IndexSet<String, FxBuildHasher>,
    pub kind: InputOrOutputKind,
}

/// `'input' | 'output'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOrOutputKind {
    Input,
    Output,
}

// t2_api.ts:76 Target
#[derive(Debug, Clone)]
pub struct Target<'a, D> {
    pub template: Option<&'a [R3Node<'a>]>,
    pub host: Option<TargetHost<'a, D>>,
}

#[derive(Debug, Clone)]
pub struct TargetHost<'a, D> {
    pub node: &'a R3HostElement<'a>,
    pub directives: Vec<D>,
}

// t2_api.ts:93 LegacyAnimationTriggerNames
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyAnimationTriggerNames {
    pub includes_dynamic_animations: bool,
    pub static_trigger_names: Vec<String>,
}

// t2_api.ts:139 ForeignComponentMeta
#[derive(Debug, Clone)]
pub struct ForeignComponentMeta {
    pub name: String,
}

// t2_api.ts:144 MatchSource
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchSource {
    Selector,
    HostDirective,
}

// t2_api.ts:101 DirectiveMeta: what the binder reads from a directive
pub trait DirectiveMeta {
    /// What its input mapping holds (`ClassPropertyMapping<T extends InputOrOutput>`)
    type Input: AsInputOrOutput + Clone;
    fn name(&self) -> &str;
    /// `ref.key`
    fn ref_key(&self) -> &str;
    fn selector(&self) -> Option<&str>;
    fn is_component(&self) -> bool;
    fn inputs(&self) -> &ClassPropertyMapping<Self::Input>;
    fn outputs(&self) -> &ClassPropertyMapping<InputOrOutput>;
    fn export_as(&self) -> Option<&[String]>;
    fn is_structural(&self) -> bool;
    fn ng_content_selectors(&self) -> Option<&[String]>;
    fn preserve_whitespaces(&self) -> bool;
    fn animation_trigger_names(&self) -> Option<&LegacyAnimationTriggerNames>;
    fn match_source(&self) -> MatchSource;
}

// t2_api.ts:159 BoundTarget: the result of binding a template. `D` is the directive type (cheap to clone, compared by
// identity: `Rc<TcbDirectiveMetadata>`).
pub trait BoundTarget<'a, D> {
    fn target(&self) -> &Target<'a, D>;
    fn get_directives_of_node(&self, node: DirectiveOwner<'a>) -> Option<Vec<D>>;
    fn get_foreign_component(&self, element: &'a R3Element<'a>) -> Option<ForeignComponentMeta>;
    fn get_reference_target(&self, r#ref: &'a R3Reference<'a>) -> Option<ReferenceTarget<'a, D>>;
    fn get_consumer_of_binding(&self, binding: Binding<'a>) -> Option<BindingConsumer<'a, D>>;
    fn get_expression_target(&self, expr: &AngularExpression<'a>) -> Option<TemplateEntity<'a>>;
    fn get_definition_node_of_symbol(&self, symbol: TemplateEntity<'a>) -> Option<ScopedNode<'a>>;
    fn get_nesting_level(&self, node: ScopedNode<'a>) -> usize;
    fn get_entities_in_scope(&self, node: Option<ScopedNode<'a>>) -> Vec<TemplateEntity<'a>>;
    fn get_used_directives(&self) -> Vec<D>;
    fn get_eagerly_used_directives(&self) -> Vec<D>;
    fn get_used_pipes(&self) -> Vec<String>;
    fn get_eagerly_used_pipes(&self) -> Vec<String>;
    fn get_defer_blocks(&self) -> Vec<&'a R3DeferredBlock<'a>>;
    fn get_deferred_trigger_target(
        &self,
        block: &'a R3DeferredBlock<'a>,
        trigger: ReferenceDeferredTrigger<'a>,
    ) -> Option<&'a R3Element<'a>>;
    fn is_deferred(&self, node: DirectiveOwner<'a>) -> bool;
    fn get_defer_blocks_of_node(&self, node: DirectiveOwner<'a>) -> Vec<&'a R3DeferredBlock<'a>>;
    fn get_defer_blocks_of_pipe(&self, ast: &'a BindingPipe<'a>) -> Vec<&'a R3DeferredBlock<'a>>;
    fn referenced_directive_exists(&self, name: &str) -> bool;
    fn get_conflicting_host_directive_bindings(
        &self,
        node: DirectiveOwner<'a>,
    ) -> Option<Vec<ConflictingHostDirectiveBinding<D>>>;
}
