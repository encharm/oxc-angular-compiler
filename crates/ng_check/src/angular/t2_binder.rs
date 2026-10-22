//! `packages/compiler/src/render3/view/t2_binder.ts`: the template binder, over the fork's template AST
//! (`oxc_angular_compiler::ast::r3`).
//!
//! Also here, as they are ported nowhere else: `createCssSelectorFromNode` and `getAttrsForDirectiveMatching`
//! (`render3/view/util.ts`), `splitNsName` and `isNgTemplate` (`ml_parser/tags.ts`), the `Visitor` interface and the
//! nodes' `visit` methods (`render3/r3_ast.ts`), and the traversal `TemplateBinder` inherits from
//! `CombinedRecursiveAstVisitor` (`combined_visitor.ts`) and `RecursiveAstVisitor` (`expression_parser/ast.ts`).
//!
//! - Identity: TypeScript keys the binder's maps by object; here a node is its address (`node_key`, `expr_key`, the
//!   node enums of `t2_api.rs`), and a directive its `BinderDirective::identity`.
//! - Scopes: TypeScript's `Scope`s are a tree of objects pointing at their parent and children; here they live in one
//!   `Scopes` vector and refer to each other by index (`ScopeId`).
//! - Angular 22's `ContentBlock`, `BoundaryBlock` and `BoundaryErrorBlock` are not in the fork's AST (its parser does
//!   not produce them): their code paths are kept as comments.

use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};
use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::expression::{
    ASTWithSource, AngularExpression, ArrowFunction, Binary, BindingPipe, BindingType, Call, Chain, Conditional,
    EmptyExpr, ImplicitReceiver, Interpolation, KeyedRead, LiteralArray, LiteralMap, LiteralPrimitive, NonNullAssert,
    ParenthesizedExpression, PrefixNot, PropertyRead, RegularExpressionLiteral, SafeCall, SafeKeyedRead,
    SafePropertyRead, SpreadElement, TaggedTemplateLiteral, TemplateLiteral, TemplateLiteralElement, ThisReceiver,
    TypeofExpression, Unary, VoidExpression,
};
use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3BoundText, R3Component, R3Content, R3DeferredBlock, R3DeferredBlockError,
    R3DeferredBlockLoading, R3DeferredBlockPlaceholder, R3Directive, R3Element, R3ForLoopBlock,
    R3ForLoopBlockEmpty, R3Icu, R3IcuPlaceholder, R3IfBlock, R3IfBlockBranch, R3LetDeclaration, R3Node, R3Reference,
    R3SwitchBlock, R3SwitchBlockCase, R3SwitchBlockCaseGroup, R3SwitchExhaustiveCheck, R3Template, R3TemplateAttr,
    R3Text, R3TextAttribute, R3UnknownBlock, R3Variable,
};
use oxc_angular_compiler::i18n::serializer::is_i18n_attribute;
use oxc_angular_compiler::parser::ParseTemplateOptions;
use oxc_angular_compiler::parser::html::{HtmlParser, remove_whitespaces};
use oxc_angular_compiler::transform::HtmlToR3Transform;
use oxc_angular_compiler::transform::html_to_r3::TransformOptions as R3TransformOptions;
use rustc_hash::FxBuildHasher;

use crate::angular::directive_matching::{CssSelector, SelectorMatcher, SelectorlessMatcher};
use crate::angular::property_mapping::{AsInputOrOutput, ClassPropertyMapping, ClassPropertyName, InputOrOutput};
use crate::angular::t2_api::{
    Binding, BindingConsumer, BoundTarget, ConflictingHostDirectiveBinding, DirectiveMeta, DirectiveOwner,
    ForeignComponentMeta, InputOrOutputKind, LegacyAnimationTriggerNames, MatchSource, NodeKey, ReferenceDeferredTrigger, ReferenceTarget, ScopedNode,
    Target, TemplateEntity, expr_key, node_key,
};

type Map<K, V> = IndexMap<K, V, FxBuildHasher>;
type Set<T> = IndexSet<T, FxBuildHasher>;

/// The binder's directive type `D` (`DirectiveT`).
///
/// TypeScript compares directives, and collects them in sets, by object; and `dedupeAndMergeDirectives` makes a new
/// directive object from one (`{...dir, inputs, outputs}`).
pub trait BinderDirective: DirectiveMeta + Clone {
    /// The directive object's identity.
    fn identity(&self) -> usize;
    /// `{...this, inputs, outputs}` (t2_binder.ts:818): a new directive object with other mappings.
    #[must_use]
    fn with_inputs_and_outputs(
        &self,
        inputs: ClassPropertyMapping<Self::Input>,
        outputs: ClassPropertyMapping<InputOrOutput>,
    ) -> Self;
}

/// Directive metadata that can be copied with other input and output mappings.
///
/// (The object spread of t2_binder.ts:818.) `Rc<T>` of such metadata is a `BinderDirective` whose identity is the `Rc`'s allocation
/// (`Rc::ptr_eq`): the generator's `DirectiveRef` (`Rc<TcbDirectiveMetadata>`).
pub trait DirectiveSpread: DirectiveMeta + Sized {
    #[must_use]
    fn with_mappings(
        &self,
        inputs: ClassPropertyMapping<Self::Input>,
        outputs: ClassPropertyMapping<InputOrOutput>,
    ) -> Self;
}

impl<T: DirectiveMeta> DirectiveMeta for Rc<T> {
    type Input = T::Input;

    fn name(&self) -> &str {
        (**self).name()
    }
    fn ref_key(&self) -> &str {
        (**self).ref_key()
    }
    fn selector(&self) -> Option<&str> {
        (**self).selector()
    }
    fn is_component(&self) -> bool {
        (**self).is_component()
    }
    fn inputs(&self) -> &ClassPropertyMapping<Self::Input> {
        (**self).inputs()
    }
    fn outputs(&self) -> &ClassPropertyMapping<InputOrOutput> {
        (**self).outputs()
    }
    fn export_as(&self) -> Option<&[String]> {
        (**self).export_as()
    }
    fn is_structural(&self) -> bool {
        (**self).is_structural()
    }
    fn ng_content_selectors(&self) -> Option<&[String]> {
        (**self).ng_content_selectors()
    }
    fn preserve_whitespaces(&self) -> bool {
        (**self).preserve_whitespaces()
    }
    fn animation_trigger_names(&self) -> Option<&LegacyAnimationTriggerNames> {
        (**self).animation_trigger_names()
    }
    fn match_source(&self) -> MatchSource {
        (**self).match_source()
    }
}

impl<T: DirectiveSpread> BinderDirective for Rc<T> {
    fn identity(&self) -> usize {
        Rc::as_ptr(self).addr()
    }

    fn with_inputs_and_outputs(
        &self,
        inputs: ClassPropertyMapping<Self::Input>,
        outputs: ClassPropertyMapping<InputOrOutput>,
    ) -> Self {
        Rc::new(DirectiveSpread::with_mappings(&**self, inputs, outputs))
    }
}

// t2_binder.ts:81 diff
/// Computes a difference between full list (first argument) and
/// list of items that should be excluded from the full list (second
/// argument).
fn diff(full_list: &[String], items_to_exclude: &[String]) -> Vec<String> {
    let exclude: Set<&str> = items_to_exclude.iter().map(String::as_str).collect();
    full_list.iter().filter(|item| !exclude.contains(item.as_str())).cloned().collect()
}

// t2_binder.ts:87 BindingsMap
/// Shorthand for a map between a binding AST node and the entity it's targeting.
type BindingsMap<'a, D> = Map<Binding<'a>, BindingConsumer<'a, D>>;

// t2_binder.ts:93 ReferenceMap
/// Shorthand for a map between a reference AST node (by `node_key`) and the entity it's targeting.
type ReferenceMap<'a, D> = Map<NodeKey, ReferenceTarget<'a, D>>;

// t2_binder.ts:99 MatchedDirectives
/// Mapping between AST nodes and the directives that have been matched on them.
type MatchedDirectives<'a, D> = Map<DirectiveOwner<'a>, Vec<D>>;

// t2_binder.ts:105 ScopedNodeEntities
/// Mapping between a scoped not and the template entities that exist in it.
/// `None` represents the root scope.
type ScopedNodeEntities<'a> = Map<Option<ScopedNode<'a>>, Set<TemplateEntity<'a>>>;

// t2_binder.ts:108 DeferBlockScopes
/// Shorthand tuple type where a defer block is paired with its corresponding scope.
type DeferBlockScopes<'a> = Vec<(&'a R3DeferredBlock<'a>, ScopeId)>;

/// Mapping between nodes and the conflicting host directive bindings detected on them.
type ConflictingHostDirectiveBindings<'a, D> = Map<DirectiveOwner<'a>, Vec<ConflictingHostDirectiveBinding<D>>>;

/// The result of `findMatchingDirectivesAndPipes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchingDirectivesAndPipes {
    pub directives: RegularAndDeferCandidates,
    pub pipes: RegularAndDeferCandidates,
}

/// `{regular, deferCandidates}`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularAndDeferCandidates {
    pub regular: Vec<String>,
    pub defer_candidates: Vec<String>,
}

/// The fake directive of `findMatchingDirectivesAndPipes`: a selector, no exports, no inputs or outputs.
#[derive(Debug, Clone)]
struct FakeDirective {
    selector: String,
    inputs: ClassPropertyMapping<InputOrOutput>,
    outputs: ClassPropertyMapping<InputOrOutput>,
}

impl DirectiveMeta for FakeDirective {
    type Input = InputOrOutput;

    // `name` and `ref` are `undefined` on the fake directive; the binder does not read them for a selector match.
    #[expect(clippy::unnecessary_literal_bound, reason = "the trait's signature")]
    fn name(&self) -> &str {
        ""
    }
    #[expect(clippy::unnecessary_literal_bound, reason = "the trait's signature")]
    fn ref_key(&self) -> &str {
        ""
    }
    fn selector(&self) -> Option<&str> {
        Some(&self.selector)
    }
    fn is_component(&self) -> bool {
        false
    }
    fn inputs(&self) -> &ClassPropertyMapping<InputOrOutput> {
        &self.inputs
    }
    fn outputs(&self) -> &ClassPropertyMapping<InputOrOutput> {
        &self.outputs
    }
    fn export_as(&self) -> Option<&[String]> {
        None
    }
    fn is_structural(&self) -> bool {
        false
    }
    fn ng_content_selectors(&self) -> Option<&[String]> {
        None
    }
    fn preserve_whitespaces(&self) -> bool {
        false
    }
    fn animation_trigger_names(&self) -> Option<&LegacyAnimationTriggerNames> {
        None
    }
    fn match_source(&self) -> MatchSource {
        MatchSource::Selector
    }
}

impl DirectiveSpread for FakeDirective {
    fn with_mappings(
        &self,
        inputs: ClassPropertyMapping<InputOrOutput>,
        outputs: ClassPropertyMapping<InputOrOutput>,
    ) -> Self {
        Self { selector: self.selector.clone(), inputs, outputs }
    }
}

// t2_binder.ts:124 findMatchingDirectivesAndPipes
/// Given a template string and a set of available directive selectors, matches them.
///
/// Computes a list of matching selectors and splits them into 2 buckets:
/// (1) eagerly used in a template and (2) directives used only in defer
/// blocks. Similarly, returns 2 lists of pipes (eager and deferrable).
///
/// Note: deferrable directives selectors and pipes names used in `@defer`
/// blocks are **candidates** and API caller should make sure that:
///
///  * A Component where a given template is defined is standalone
///  * Underlying dependency classes are also standalone
///  * Dependency class symbols are not eagerly used in a TS file
///    where a host component (that owns the template) is located
///
/// `parseTemplate(template, '')` is the fork's parse sequence with `parseTemplate`'s default options (as
/// `oxc_angular_compiler`'s `component/transform.rs` parses a template); its errors are ignored, as here in TypeScript.
pub fn find_matching_directives_and_pipes(
    template: &str,
    directive_selectors: &[String],
) -> MatchingDirectivesAndPipes {
    let mut matcher = SelectorMatcher::<Vec<Rc<FakeDirective>>>::new();
    for selector in directive_selectors {
        // Create a fake directive instance to account for the logic inside
        // of the `R3TargetBinder` class (which invokes the `hasBindingPropertyName`
        // function internally).
        let fake_directive = Rc::new(FakeDirective {
            selector: selector.clone(),
            inputs: ClassPropertyMapping::empty(),
            outputs: ClassPropertyMapping::empty(),
        });
        matcher.add_selectables(&CssSelector::parse(selector), vec![fake_directive]);
    }
    let allocator = Allocator::default();
    let template = allocator.alloc_str(template);
    let parse_options = ParseTemplateOptions {
        enable_block_syntax: true,
        enable_let_syntax: true,
        tokenize_expansion_forms: true,
        ..Default::default()
    };
    let html_result = HtmlParser::with_options(&allocator, template, "", &parse_options).parse();
    let nodes = allocator.alloc(remove_whitespaces(&allocator, &html_result.nodes, true));
    let parsed_template = HtmlToR3Transform::new(&allocator, template, R3TransformOptions::default()).transform(nodes);
    let binder = R3TargetBinder::new(Some(DirectiveMatcher::Selector(matcher)));
    let bound = binder.bind(Target { template: Some(&parsed_template.nodes), host: None });

    let eager_directive_selectors: Vec<String> =
        bound.get_eagerly_used_directives().iter().map(|dir| dir.selector.clone()).collect();
    let all_matched_directive_selectors: Vec<String> =
        bound.get_used_directives().iter().map(|dir| dir.selector.clone()).collect();
    let eager_pipes = bound.get_eagerly_used_pipes();
    MatchingDirectivesAndPipes {
        directives: RegularAndDeferCandidates {
            defer_candidates: diff(&all_matched_directive_selectors, &eager_directive_selectors),
            regular: eager_directive_selectors,
        },
        pipes: RegularAndDeferCandidates {
            defer_candidates: diff(&bound.get_used_pipes(), &eager_pipes),
            regular: eager_pipes,
        },
    }
}

// t2_binder.ts:167 DirectiveMatcher
/// Object used to match template nodes to directives.
pub enum DirectiveMatcher<D> {
    Selector(SelectorMatcher<Vec<D>>),
    Selectorless(SelectorlessMatcher<D>),
}

// t2_binder.ts:175 R3TargetBinder
/// Processes `Target`s with a given set of directives and performs a binding operation, which
/// returns an object similar to TypeScript's `ts.TypeChecker` that contains knowledge about the
/// target.
pub struct R3TargetBinder<D> {
    directive_matcher: Option<DirectiveMatcher<D>>,
    foreign_component_matcher: Option<SelectorlessMatcher<ForeignComponentMeta>>,
}

impl<D: BinderDirective> R3TargetBinder<D> {
    // t2_binder.ts:176 constructor, with `foreignComponentMatcher` at its default (`null`)
    pub fn new(directive_matcher: Option<DirectiveMatcher<D>>) -> Self {
        Self::with_foreign_component_matcher(directive_matcher, None)
    }

    /// t2_binder.ts:176 constructor, with a `foreignComponentMatcher`
    pub fn with_foreign_component_matcher(
        directive_matcher: Option<DirectiveMatcher<D>>,
        foreign_component_matcher: Option<SelectorlessMatcher<ForeignComponentMeta>>,
    ) -> Self {
        Self { directive_matcher, foreign_component_matcher }
    }

    // t2_binder.ts:185 bind
    /// Perform a binding operation on the given `Target` and return a `BoundTarget` which contains
    /// metadata about the types referenced in the template.
    ///
    /// # Panics
    ///
    /// As the TypeScript throws: on an empty target, or an element that matches both a directive and a foreign
    /// component.
    pub fn bind<'a>(&self, target: Target<'a, D>) -> R3BoundTarget<'a, D> {
        assert!(target.template.is_some() || target.host.is_some(), "Empty bound targets are not supported");

        let mut directives: MatchedDirectives<'a, D> = Map::default();
        let mut foreign_components: Map<NodeKey, ForeignComponentMeta> = Map::default();
        let mut eager_directives: Vec<D> = Vec::new();
        let mut missing_directives: Set<String> = Set::default();
        let mut bindings: BindingsMap<'a, D> = Map::default();
        let mut references: ReferenceMap<'a, D> = Map::default();
        let mut scoped_node_entities: ScopedNodeEntities<'a> = Map::default();
        let mut expressions: Map<NodeKey, TemplateEntity<'a>> = Map::default();
        let mut symbols: Map<TemplateEntity<'a>, ScopedNode<'a>> = Map::default();
        let mut nesting_level: Map<ScopedNode<'a>, usize> = Map::default();
        let mut used_pipes: Set<String> = Set::default();
        let mut eager_pipes: Set<String> = Set::default();
        let mut defer_blocks: DeferBlockScopes<'a> = Vec::new();
        let mut pipes: Map<NodeKey, Vec<&'a R3DeferredBlock<'a>>> = Map::default();
        let mut conflicting_host_directive_bindings: ConflictingHostDirectiveBindings<'a, D> = Map::default();
        let mut scopes = Scopes::default();

        if let Some(template) = target.template {
            // First, parse the template into a `Scope` structure. This operation captures the syntactic
            // scopes in the template and makes them available for later use.
            let scope = scopes.apply(NodeOrNodes::Nodes(template));

            // Use the `Scope` to extract the entities present at every level of the template.
            extract_scoped_node_entities(&scopes, scope, &mut scoped_node_entities);

            // Next, perform directive matching on the template using the `DirectiveBinder`. This returns:
            //   - directives: Map of nodes (elements & ng-templates) to the directives on them.
            //   - bindings: Map of inputs, outputs, and attributes to the directive/element that claims
            //     them. TODO(alxhub): handle multiple directives claiming an input/output/etc.
            //   - references: Map of #references to their targets.
            DirectiveBinder::apply(
                template,
                self.directive_matcher.as_ref(),
                self.foreign_component_matcher.as_ref(),
                &mut directives,
                &mut foreign_components,
                &mut eager_directives,
                &mut missing_directives,
                &mut bindings,
                &mut references,
                &mut conflicting_host_directive_bindings,
            );

            // Finally, run the TemplateBinder to bind references, variables, and other entities within the
            // template. This extracts all the metadata that doesn't depend on directive matching.
            TemplateBinder::apply_with_scope(
                NodeOrNodes::Nodes(template),
                &scopes,
                scope,
                &mut expressions,
                &mut symbols,
                &mut nesting_level,
                &mut used_pipes,
                &mut eager_pipes,
                &mut defer_blocks,
                &mut pipes,
            );
        }

        // Bind the host element in a separate scope. Note that it only uses the
        // `TemplateBinder` since directives don't apply inside a host context.
        if let Some(host) = &target.host {
            directives.insert(DirectiveOwner::HostElement(host.node), host.directives.clone());
            let host_scope = scopes.apply(NodeOrNodes::Node(ScopedNode::HostElement(host.node)));
            TemplateBinder::apply_with_scope(
                NodeOrNodes::Node(ScopedNode::HostElement(host.node)),
                &scopes,
                host_scope,
                &mut expressions,
                &mut symbols,
                &mut nesting_level,
                &mut used_pipes,
                &mut eager_pipes,
                &mut defer_blocks,
                &mut pipes,
            );
        }

        R3BoundTarget::new(
            target,
            directives,
            foreign_components,
            eager_directives,
            missing_directives,
            bindings,
            references,
            expressions,
            symbols,
            nesting_level,
            scoped_node_entities,
            used_pipes,
            eager_pipes,
            &defer_blocks,
            pipes,
            conflicting_host_directive_bindings,
            scopes,
        )
    }
}

/// `ScopedNode | Node[]`: what `Scope.apply`, `Scope.ingest` and `TemplateBinder.ingest` process.
#[derive(Debug, Clone, Copy)]
enum NodeOrNodes<'a> {
    Node(ScopedNode<'a>),
    Nodes(&'a [R3Node<'a>]),
}

// r3_ast.ts:790 Visitor: TypeScript's visitor interface, over `&'a` nodes so that visitors can record them.
// (`visit?`, the generic hook, is unused by the binder. `visitDeferredTrigger`: the fork's deferred blocks store their
// triggers as structs, not as `R3DeferredTrigger` nodes, and no visitor here visits them as nodes.)
trait Visitor<'a> {
    fn visit_element(&mut self, element: &'a R3Element<'a>);
    fn visit_template(&mut self, template: &'a R3Template<'a>);
    fn visit_content(&mut self, content: &'a R3Content<'a>);
    fn visit_variable(&mut self, variable: &'a R3Variable<'a>);
    fn visit_reference(&mut self, reference: &'a R3Reference<'a>);
    fn visit_text_attribute(&mut self, attribute: &'a R3TextAttribute<'a>);
    fn visit_bound_attribute(&mut self, attribute: &'a R3BoundAttribute<'a>);
    fn visit_bound_event(&mut self, attribute: &'a R3BoundEvent<'a>);
    fn visit_text(&mut self, text: &'a R3Text<'a>);
    fn visit_bound_text(&mut self, text: &'a R3BoundText<'a>);
    fn visit_icu(&mut self, icu: &'a R3Icu<'a>);
    fn visit_deferred_block(&mut self, deferred: &'a R3DeferredBlock<'a>);
    fn visit_deferred_block_placeholder(&mut self, block: &'a R3DeferredBlockPlaceholder<'a>);
    fn visit_deferred_block_error(&mut self, block: &'a R3DeferredBlockError<'a>);
    fn visit_deferred_block_loading(&mut self, block: &'a R3DeferredBlockLoading<'a>);
    fn visit_switch_block(&mut self, block: &'a R3SwitchBlock<'a>);
    fn visit_switch_block_case(&mut self, block: &'a R3SwitchBlockCase<'a>);
    fn visit_switch_block_case_group(&mut self, block: &'a R3SwitchBlockCaseGroup<'a>);
    fn visit_switch_exhaustive_check(&mut self, block: &'a R3SwitchExhaustiveCheck<'a>);
    fn visit_for_loop_block(&mut self, block: &'a R3ForLoopBlock<'a>);
    fn visit_for_loop_block_empty(&mut self, block: &'a R3ForLoopBlockEmpty<'a>);
    fn visit_if_block(&mut self, block: &'a R3IfBlock<'a>);
    fn visit_if_block_branch(&mut self, block: &'a R3IfBlockBranch<'a>);
    // visitBoundaryBlock, visitBoundaryErrorBlock: not in the fork's AST
    fn visit_unknown_block(&mut self, block: &'a R3UnknownBlock<'a>);
    fn visit_let_declaration(&mut self, decl: &'a R3LetDeclaration<'a>);
    fn visit_component(&mut self, component: &'a R3Component<'a>);
    fn visit_directive(&mut self, directive: &'a R3Directive<'a>);
    // visitContentBlock: not in the fork's AST
}

/// `node.visit(visitor)`: each r3_ast.ts node class's `visit` method calls the visitor's method for its type.
fn visit<'a, V: Visitor<'a> + ?Sized>(node: &'a R3Node<'a>, visitor: &mut V) {
    match node {
        // r3_ast.ts:39 Comment.visit
        R3Node::Comment(_) => panic!("visit() not implemented for Comment"),
        R3Node::Text(n) => visitor.visit_text(n),
        R3Node::BoundText(n) => visitor.visit_bound_text(n),
        R3Node::Element(n) => visitor.visit_element(n),
        R3Node::Template(n) => visitor.visit_template(n),
        R3Node::Content(n) => visitor.visit_content(n),
        R3Node::Variable(n) => visitor.visit_variable(n),
        R3Node::Reference(n) => visitor.visit_reference(n),
        R3Node::Icu(n) => visitor.visit_icu(n),
        R3Node::DeferredBlock(n) => visitor.visit_deferred_block(n),
        R3Node::DeferredBlockPlaceholder(n) => visitor.visit_deferred_block_placeholder(n),
        R3Node::DeferredBlockLoading(n) => visitor.visit_deferred_block_loading(n),
        R3Node::DeferredBlockError(n) => visitor.visit_deferred_block_error(n),
        R3Node::SwitchBlock(n) => visitor.visit_switch_block(n),
        R3Node::SwitchBlockCaseGroup(n) => visitor.visit_switch_block_case_group(n),
        R3Node::ForLoopBlock(n) => visitor.visit_for_loop_block(n),
        R3Node::ForLoopBlockEmpty(n) => visitor.visit_for_loop_block_empty(n),
        R3Node::IfBlock(n) => visitor.visit_if_block(n),
        R3Node::IfBlockBranch(n) => visitor.visit_if_block_branch(n),
        R3Node::UnknownBlock(n) => visitor.visit_unknown_block(n),
        R3Node::LetDeclaration(n) => visitor.visit_let_declaration(n),
        R3Node::Component(n) => visitor.visit_component(n),
        R3Node::Directive(n) => visitor.visit_directive(n),
        // r3_ast.ts:785 HostElement.visit
        R3Node::HostElement(_) => panic!("HostElement cannot be visited"),
    }
}

/// The index of a `Scope` in `Scopes`.
type ScopeId = usize;

// t2_binder.ts:295 Scope
/// Represents a binding scope within a template.
///
/// Any variables, references, or other named entities declared within the template will
/// be captured and available by name in `namedEntities`. Additionally, child templates will
/// be analyzed and have their child `Scope`s available in `childScopes`.
#[derive(Debug)]
#[expect(clippy::struct_field_names, reason = "the TypeScript class's fields")]
struct Scope<'a> {
    /// Named members of the `Scope`, such as `Reference`s or `Variable`s.
    named_entities: Map<String, TemplateEntity<'a>>,
    /// Set of element-like nodes that belong to this scope.
    element_like_in_scope: Set<DirectiveOwner<'a>>,
    /// Child `Scope`s for immediately nested `ScopedNode`s.
    child_scopes: Map<ScopedNode<'a>, ScopeId>,
    /// Whether this scope is deferred or if any of its ancestors are deferred.
    is_deferred: bool,
    parent_scope: Option<ScopeId>,
    root_node: Option<ScopedNode<'a>>,
}

/// The `Scope`s of a binding, which refer to each other by their index here.
#[derive(Debug, Default)]
struct Scopes<'a> {
    scopes: Vec<Scope<'a>>,
}

impl<'a> Scopes<'a> {
    // t2_binder.ts:314 constructor
    fn new_scope(&mut self, parent_scope: Option<ScopeId>, root_node: Option<ScopedNode<'a>>) -> ScopeId {
        let is_deferred = if parent_scope.is_some_and(|parent_scope| self.scopes[parent_scope].is_deferred) {
            true
        } else {
            matches!(root_node, Some(ScopedNode::DeferredBlock(_)))
        };
        self.scopes.push(Scope {
            named_entities: Map::default(),
            element_like_in_scope: Set::default(),
            child_scopes: Map::default(),
            is_deferred,
            parent_scope,
            root_node,
        });
        self.scopes.len() - 1
    }

    // t2_binder.ts:322 newRootScope
    fn new_root_scope(&mut self) -> ScopeId {
        self.new_scope(None, None)
    }

    // t2_binder.ts:330 apply
    /// Process a template (either as a `Template` sub-template with variables, or a plain array of
    /// template `Node`s) and construct its `Scope`.
    fn apply(&mut self, template: NodeOrNodes<'a>) -> ScopeId {
        let scope = self.new_root_scope();
        ScopeVisitor { scopes: self, scope }.ingest(template);
        scope
    }
}

/// `this` in `Scope`'s methods that populate it: the scope being populated, and all scopes.
struct ScopeVisitor<'s, 'a> {
    scopes: &'s mut Scopes<'a>,
    scope: ScopeId,
}

impl<'a> ScopeVisitor<'_, 'a> {
    fn this(&mut self) -> &mut Scope<'a> {
        &mut self.scopes.scopes[self.scope]
    }

    // t2_binder.ts:339 ingest
    /// Internal method to process the scoped node and populate the `Scope`.
    fn ingest(&mut self, node_or_nodes: NodeOrNodes<'a>) {
        match node_or_nodes {
            NodeOrNodes::Node(ScopedNode::Template(template)) => {
                // Variables on an <ng-template> are defined in the inner scope.
                for node in &template.variables {
                    self.visit_variable(node);
                }

                // Process the nodes of the template.
                for node in &template.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::IfBlockBranch(block)) => {
                if let Some(expression_alias) = &block.expression_alias {
                    self.visit_variable(expression_alias);
                }
                for node in &block.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::ForLoopBlock(block)) => {
                self.visit_variable(&block.item);
                for v in &block.context_variables {
                    self.visit_variable(v);
                }
                for node in &block.children {
                    visit(node, self);
                }
            }
            // `BoundaryErrorBlock` (its context variables, then its children): not in the fork's AST.
            NodeOrNodes::Node(ScopedNode::SwitchBlockCaseGroup(block)) => {
                for node in &block.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::ForLoopBlockEmpty(block)) => {
                for node in &block.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::DeferredBlock(block)) => {
                for node in &block.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::DeferredBlockError(block)) => {
                for node in &block.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::DeferredBlockPlaceholder(block)) => {
                for node in &block.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::DeferredBlockLoading(block)) => {
                for node in &block.children {
                    visit(node, self);
                }
            }
            // (`ContentBlock` and `BoundaryBlock` too: not in the fork's AST.)
            NodeOrNodes::Node(ScopedNode::Content(content)) => {
                for node in &content.children {
                    visit(node, self);
                }
            }
            NodeOrNodes::Node(ScopedNode::HostElement(_)) => {}
            NodeOrNodes::Nodes(nodes) => {
                // No overarching `Template` instance, so process the nodes directly.
                for node in nodes {
                    visit(node, self);
                }
            }
        }
    }
}

impl<'a> Visitor<'a> for ScopeVisitor<'_, 'a> {
    // t2_binder.ts:376 visitElement
    fn visit_element(&mut self, element: &'a R3Element<'a>) {
        self.visit_element_like(ElementLike::Element(element));
    }

    // t2_binder.ts:380 visitTemplate
    fn visit_template(&mut self, template: &'a R3Template<'a>) {
        self.this().element_like_in_scope.insert(DirectiveOwner::Template(template));
        for node in &template.directives {
            self.visit_directive(node);
        }

        // References on a <ng-template> are defined in the outer scope, so capture them before
        // processing the template's child scope.
        for node in &template.references {
            self.visit_reference(node);
        }

        // Next, create an inner scope and process the template within it.
        self.ingest_scoped_node(ScopedNode::Template(template));
    }

    // t2_binder.ts:392 visitVariable
    fn visit_variable(&mut self, variable: &'a R3Variable<'a>) {
        // Declare the variable if it's not already.
        self.maybe_declare(TemplateEntity::Variable(variable));
    }

    // t2_binder.ts:397 visitReference
    fn visit_reference(&mut self, reference: &'a R3Reference<'a>) {
        // Declare the variable if it's not already.
        self.maybe_declare(TemplateEntity::Reference(reference));
    }

    // t2_binder.ts:402 visitDeferredBlock
    fn visit_deferred_block(&mut self, deferred: &'a R3DeferredBlock<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlock(deferred));
        if let Some(placeholder) = &deferred.placeholder {
            self.visit_deferred_block_placeholder(placeholder);
        }
        if let Some(loading) = &deferred.loading {
            self.visit_deferred_block_loading(loading);
        }
        if let Some(error) = &deferred.error {
            self.visit_deferred_block_error(error);
        }
    }

    // t2_binder.ts:409 visitDeferredBlockPlaceholder
    fn visit_deferred_block_placeholder(&mut self, block: &'a R3DeferredBlockPlaceholder<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlockPlaceholder(block));
    }

    // t2_binder.ts:413 visitDeferredBlockError
    fn visit_deferred_block_error(&mut self, block: &'a R3DeferredBlockError<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlockError(block));
    }

    // t2_binder.ts:417 visitDeferredBlockLoading
    fn visit_deferred_block_loading(&mut self, block: &'a R3DeferredBlockLoading<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlockLoading(block));
    }

    // t2_binder.ts:421 visitBoundaryBlock, t2_binder.ts:426 visitBoundaryErrorBlock: not in the fork's AST

    // t2_binder.ts:430 visitSwitchBlock
    fn visit_switch_block(&mut self, block: &'a R3SwitchBlock<'a>) {
        for node in &block.groups {
            self.visit_switch_block_case_group(node);
        }
    }

    // t2_binder.ts:434 visitSwitchBlockCase
    fn visit_switch_block_case(&mut self, _block: &'a R3SwitchBlockCase<'a>) {}

    // t2_binder.ts:436 visitSwitchBlockCaseGroup
    fn visit_switch_block_case_group(&mut self, block: &'a R3SwitchBlockCaseGroup<'a>) {
        self.ingest_scoped_node(ScopedNode::SwitchBlockCaseGroup(block));
    }

    // t2_binder.ts:440 visitSwitchExhaustiveCheck
    fn visit_switch_exhaustive_check(&mut self, _block: &'a R3SwitchExhaustiveCheck<'a>) {}

    // t2_binder.ts:442 visitForLoopBlock
    fn visit_for_loop_block(&mut self, block: &'a R3ForLoopBlock<'a>) {
        self.ingest_scoped_node(ScopedNode::ForLoopBlock(block));
        if let Some(empty) = &block.empty {
            self.visit_for_loop_block_empty(empty);
        }
    }

    // t2_binder.ts:447 visitForLoopBlockEmpty
    fn visit_for_loop_block_empty(&mut self, block: &'a R3ForLoopBlockEmpty<'a>) {
        self.ingest_scoped_node(ScopedNode::ForLoopBlockEmpty(block));
    }

    // t2_binder.ts:451 visitIfBlock
    fn visit_if_block(&mut self, block: &'a R3IfBlock<'a>) {
        for node in &block.branches {
            self.visit_if_block_branch(node);
        }
    }

    // t2_binder.ts:455 visitIfBlockBranch
    fn visit_if_block_branch(&mut self, block: &'a R3IfBlockBranch<'a>) {
        self.ingest_scoped_node(ScopedNode::IfBlockBranch(block));
    }

    // t2_binder.ts:459 visitContent
    fn visit_content(&mut self, content: &'a R3Content<'a>) {
        self.ingest_scoped_node(ScopedNode::Content(content));
    }

    // t2_binder.ts:463 visitContentBlock: not in the fork's AST

    // t2_binder.ts:467 visitLetDeclaration
    fn visit_let_declaration(&mut self, decl: &'a R3LetDeclaration<'a>) {
        self.maybe_declare(TemplateEntity::LetDeclaration(decl));
    }

    // t2_binder.ts:471 visitComponent
    fn visit_component(&mut self, component: &'a R3Component<'a>) {
        self.visit_element_like(ElementLike::Component(component));
    }

    // t2_binder.ts:475 visitDirective
    fn visit_directive(&mut self, directive: &'a R3Directive<'a>) {
        self.this().element_like_in_scope.insert(DirectiveOwner::Directive(directive));
        for current in &directive.references {
            self.visit_reference(current);
        }
    }

    // t2_binder.ts:481 Unused visitors.
    fn visit_bound_attribute(&mut self, _attr: &'a R3BoundAttribute<'a>) {}
    fn visit_bound_event(&mut self, _event: &'a R3BoundEvent<'a>) {}
    fn visit_bound_text(&mut self, _text: &'a R3BoundText<'a>) {}
    fn visit_text(&mut self, _text: &'a R3Text<'a>) {}
    fn visit_text_attribute(&mut self, _attr: &'a R3TextAttribute<'a>) {}
    fn visit_icu(&mut self, _icu: &'a R3Icu<'a>) {}
    fn visit_unknown_block(&mut self, _block: &'a R3UnknownBlock<'a>) {}
}

/// `Element | Component`
#[derive(Debug, Clone, Copy)]
enum ElementLike<'a> {
    Element(&'a R3Element<'a>),
    Component(&'a R3Component<'a>),
}

impl<'a> ScopeVisitor<'_, 'a> {
    // t2_binder.ts:490 visitElementLike
    fn visit_element_like(&mut self, node: ElementLike<'a>) {
        let (directives, references, children, owner) = match node {
            ElementLike::Element(n) => (&n.directives, &n.references, &n.children, DirectiveOwner::Element(n)),
            ElementLike::Component(n) => (&n.directives, &n.references, &n.children, DirectiveOwner::Component(n)),
        };
        for current in directives {
            self.visit_directive(current);
        }
        for current in references {
            self.visit_reference(current);
        }
        for current in children {
            visit(current, self);
        }
        self.this().element_like_in_scope.insert(owner);
    }
}

impl<'a> Scopes<'a> {
    // t2_binder.ts:500 getEnclosingDeferBlocks
    /// Returns all enclosing `DeferredBlock`s for this scope, ordered from outermost to innermost.
    fn get_enclosing_defer_blocks(&self, scope: ScopeId) -> Vec<&'a R3DeferredBlock<'a>> {
        let mut blocks = Vec::new();
        let mut current = Some(scope);
        while let Some(scope) = current {
            if let Some(ScopedNode::DeferredBlock(block)) = self.scopes[scope].root_node {
                blocks.push(block);
            }
            current = self.scopes[scope].parent_scope;
        }
        blocks.reverse();
        blocks
    }
}

impl<'a> ScopeVisitor<'_, 'a> {
    // t2_binder.ts:512 maybeDeclare
    fn maybe_declare(&mut self, thing: TemplateEntity<'a>) {
        // Declare something with a name, as long as that name isn't taken.
        let name = entity_name(thing);
        if !self.this().named_entities.contains_key(name) {
            self.this().named_entities.insert(name.to_string(), thing);
        }
    }
}

impl<'a> Scopes<'a> {
    // t2_binder.ts:524 lookup
    /// Look up a variable within this `Scope`.
    ///
    /// This can recurse into a parent `Scope` if it's available.
    fn lookup(&self, scope: ScopeId, name: &str) -> Option<TemplateEntity<'a>> {
        let this = &self.scopes[scope];
        if let Some(entity) = this.named_entities.get(name) {
            // Found in the local scope.
            Some(*entity)
        } else if let Some(parent_scope) = this.parent_scope {
            // Not in the local scope, but there's a parent scope so check there.
            self.lookup(parent_scope, name)
        } else {
            // At the top level and it wasn't found.
            None
        }
    }

    // t2_binder.ts:542 getChildScope
    /// Get the child scope for a `ScopedNode`.
    ///
    /// This should always be defined.
    fn get_child_scope(&self, scope: ScopeId, node: ScopedNode<'a>) -> ScopeId {
        match self.scopes[scope].child_scopes.get(&node) {
            Some(res) => *res,
            // `${node}` of a node object is `[object Object]`.
            None => panic!("Assertion error: child scope for [object Object] not found"),
        }
    }
}

impl<'a> ScopeVisitor<'_, 'a> {
    // t2_binder.ts:550 ingestScopedNode
    fn ingest_scoped_node(&mut self, node: ScopedNode<'a>) {
        let scope = self.scopes.new_scope(Some(self.scope), Some(node));
        ScopeVisitor { scopes: self.scopes, scope }.ingest(NodeOrNodes::Node(node));
        self.this().child_scopes.insert(node, scope);
    }
}

/// The name of a `TemplateEntity` (`thing.name`).
fn entity_name(entity: TemplateEntity<'_>) -> &str {
    match entity {
        TemplateEntity::Reference(n) => n.name.as_str(),
        TemplateEntity::Variable(n) => n.name.as_str(),
        TemplateEntity::LetDeclaration(n) => n.name.as_str(),
    }
}

/// `Element | Template`, the nodes that selectors match.
pub use crate::typecheck::oob::ElementOrTemplate;

impl<'a> ElementOrTemplate<'a> {
    fn owner(self) -> DirectiveOwner<'a> {
        match self {
            Self::Element(n) => DirectiveOwner::Element(n),
            Self::Template(n) => DirectiveOwner::Template(n),
        }
    }

    fn references(self) -> &'a [R3Reference<'a>] {
        match self {
            Self::Element(n) => &n.references,
            Self::Template(n) => &n.references,
        }
    }

    fn inputs(self) -> &'a [R3BoundAttribute<'a>] {
        match self {
            Self::Element(n) => &n.inputs,
            Self::Template(n) => &n.inputs,
        }
    }

    fn attributes(self) -> &'a [R3TextAttribute<'a>] {
        match self {
            Self::Element(n) => &n.attributes,
            Self::Template(n) => &n.attributes,
        }
    }

    fn outputs(self) -> &'a [R3BoundEvent<'a>] {
        match self {
            Self::Element(n) => &n.outputs,
            Self::Template(n) => &n.outputs,
        }
    }

    fn directives(self) -> &'a [R3Directive<'a>] {
        match self {
            Self::Element(n) => &n.directives,
            Self::Template(n) => &n.directives,
        }
    }

    fn children(self) -> &'a [R3Node<'a>] {
        match self {
            Self::Element(n) => &n.children,
            Self::Template(n) => &n.children,
        }
    }

    /// The node as the target of a reference.
    fn as_reference_target<D>(self) -> ReferenceTarget<'a, D> {
        match self {
            Self::Element(n) => ReferenceTarget::Element(n),
            Self::Template(n) => ReferenceTarget::Template(n),
        }
    }

    /// The node as the consumer of a binding.
    fn as_binding_consumer<D>(self) -> BindingConsumer<'a, D> {
        match self {
            Self::Element(n) => BindingConsumer::Element(n),
            Self::Template(n) => BindingConsumer::Template(n),
        }
    }
}

/// `Component | Directive`, the nodes that selectorless directives match.
#[derive(Debug, Clone, Copy)]
enum SelectorlessNode<'a> {
    Component(&'a R3Component<'a>),
    Directive(&'a R3Directive<'a>),
}

impl<'a> SelectorlessNode<'a> {
    fn owner(self) -> DirectiveOwner<'a> {
        match self {
            Self::Component(n) => DirectiveOwner::Component(n),
            Self::Directive(n) => DirectiveOwner::Directive(n),
        }
    }

    fn inputs(self) -> &'a [R3BoundAttribute<'a>] {
        match self {
            Self::Component(n) => &n.inputs,
            Self::Directive(n) => &n.inputs,
        }
    }

    fn attributes(self) -> &'a [R3TextAttribute<'a>] {
        match self {
            Self::Component(n) => &n.attributes,
            Self::Directive(n) => &n.attributes,
        }
    }

    fn outputs(self) -> &'a [R3BoundEvent<'a>] {
        match self {
            Self::Component(n) => &n.outputs,
            Self::Directive(n) => &n.outputs,
        }
    }

    fn references(self) -> &'a [R3Reference<'a>] {
        match self {
            Self::Component(n) => &n.references,
            Self::Directive(n) => &n.references,
        }
    }
}

/// `keyof Pick<DirectiveMeta, 'inputs' | 'outputs'>`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IoType {
    Inputs,
    Outputs,
}

/// `meta[ioType].hasBindingPropertyName(name)`
fn has_binding_property_name<D: DirectiveMeta>(meta: &D, io_type: IoType, name: &str) -> bool {
    match io_type {
        IoType::Inputs => meta.inputs().has_binding_property_name(name),
        IoType::Outputs => meta.outputs().has_binding_property_name(name),
    }
}

/// A binding and its name (`attribute.name`).
fn binding_name(binding: Binding<'_>) -> &str {
    match binding {
        Binding::BoundAttribute(n) => n.name.as_str(),
        Binding::BoundEvent(n) => n.name.as_str(),
        Binding::TextAttribute(n) => n.name.as_str(),
    }
}

// t2_binder.ts:562 DirectiveBinder
/// Processes a template and matches directives on nodes (elements and templates).
///
/// Usually used via the static `apply()` method.
struct DirectiveBinder<'b, 'a, D> {
    /// Indicates whether we are visiting elements within a `defer` block
    is_in_defer_block: bool,
    directive_matcher: Option<&'b DirectiveMatcher<D>>,
    foreign_matcher: Option<&'b SelectorlessMatcher<ForeignComponentMeta>>,
    directives: &'b mut MatchedDirectives<'a, D>,
    /// By element (`node_key`)
    foreign_components: &'b mut Map<NodeKey, ForeignComponentMeta>,
    eager_directives: &'b mut Vec<D>,
    missing_directives: &'b mut Set<String>,
    bindings: &'b mut BindingsMap<'a, D>,
    references: &'b mut ReferenceMap<'a, D>,
    conflicting_host_directive_bindings: &'b mut ConflictingHostDirectiveBindings<'a, D>,
}

impl<'b, 'a, D: BinderDirective> DirectiveBinder<'b, 'a, D> {
    // t2_binder.ts:593 apply
    /// Process a template (list of `Node`s) and perform directive matching against each node.
    ///
    /// @param template the list of template `Node`s to match (recursively).
    /// @param selectorMatcher a `SelectorMatcher` containing the directives that are in scope for
    /// this template.
    /// @returns three maps which contain information about directives in the template: the
    /// `directives` map which lists directives matched on each node, the `bindings` map which
    /// indicates which directives claimed which bindings (inputs, outputs, etc), and the `references`
    /// map which resolves #references (`Reference`s) within the template to the named directive or
    /// template node.
    fn apply(
        template: &'a [R3Node<'a>],
        directive_matcher: Option<&'b DirectiveMatcher<D>>,
        foreign_matcher: Option<&'b SelectorlessMatcher<ForeignComponentMeta>>,
        directives: &'b mut MatchedDirectives<'a, D>,
        foreign_components: &'b mut Map<NodeKey, ForeignComponentMeta>,
        eager_directives: &'b mut Vec<D>,
        missing_directives: &'b mut Set<String>,
        bindings: &'b mut BindingsMap<'a, D>,
        references: &'b mut ReferenceMap<'a, D>,
        conflicting_host_directive_bindings: &'b mut ConflictingHostDirectiveBindings<'a, D>,
    ) {
        // t2_binder.ts:566 constructor
        let mut matcher = DirectiveBinder {
            is_in_defer_block: false,
            directive_matcher,
            foreign_matcher,
            directives,
            foreign_components,
            eager_directives,
            missing_directives,
            bindings,
            references,
            conflicting_host_directive_bindings,
        };
        matcher.ingest(template);
    }

    // t2_binder.ts:622 ingest
    fn ingest(&mut self, template: &'a [R3Node<'a>]) {
        for node in template {
            visit(node, self);
        }
    }
}

impl<'a, D: BinderDirective> Visitor<'a> for DirectiveBinder<'_, 'a, D> {
    // t2_binder.ts:626 visitElement
    fn visit_element(&mut self, element: &'a R3Element<'a>) {
        self.visit_element_or_template(ElementOrTemplate::Element(element));
    }

    // t2_binder.ts:630 visitTemplate
    fn visit_template(&mut self, template: &'a R3Template<'a>) {
        self.visit_element_or_template(ElementOrTemplate::Template(template));
    }

    // t2_binder.ts:634 visitDeferredBlock
    fn visit_deferred_block(&mut self, deferred: &'a R3DeferredBlock<'a>) {
        let was_in_defer_block = self.is_in_defer_block;
        self.is_in_defer_block = true;
        for child in &deferred.children {
            visit(child, self);
        }
        self.is_in_defer_block = was_in_defer_block;

        if let Some(placeholder) = &deferred.placeholder {
            self.visit_deferred_block_placeholder(placeholder);
        }
        if let Some(loading) = &deferred.loading {
            self.visit_deferred_block_loading(loading);
        }
        if let Some(error) = &deferred.error {
            self.visit_deferred_block_error(error);
        }
    }

    // t2_binder.ts:645 visitDeferredBlockPlaceholder
    fn visit_deferred_block_placeholder(&mut self, block: &'a R3DeferredBlockPlaceholder<'a>) {
        for child in &block.children {
            visit(child, self);
        }
    }

    // t2_binder.ts:649 visitDeferredBlockError
    fn visit_deferred_block_error(&mut self, block: &'a R3DeferredBlockError<'a>) {
        for child in &block.children {
            visit(child, self);
        }
    }

    // t2_binder.ts:653 visitDeferredBlockLoading
    fn visit_deferred_block_loading(&mut self, block: &'a R3DeferredBlockLoading<'a>) {
        for child in &block.children {
            visit(child, self);
        }
    }

    // t2_binder.ts:657 visitBoundaryBlock, t2_binder.ts:662 visitBoundaryErrorBlock: not in the fork's AST

    // t2_binder.ts:666 visitSwitchBlock
    fn visit_switch_block(&mut self, block: &'a R3SwitchBlock<'a>) {
        for node in &block.groups {
            self.visit_switch_block_case_group(node);
        }
    }

    // t2_binder.ts:670 visitSwitchBlockCase
    fn visit_switch_block_case(&mut self, _block: &'a R3SwitchBlockCase<'a>) {
        // DirectiveBinder does not visit expressions
    }

    // t2_binder.ts:674 visitSwitchBlockCaseGroup
    fn visit_switch_block_case_group(&mut self, block: &'a R3SwitchBlockCaseGroup<'a>) {
        for node in &block.children {
            visit(node, self);
        }
    }

    // t2_binder.ts:678 visitSwitchExhaustiveCheck
    fn visit_switch_exhaustive_check(&mut self, _block: &'a R3SwitchExhaustiveCheck<'a>) {}

    // t2_binder.ts:680 visitForLoopBlock
    fn visit_for_loop_block(&mut self, block: &'a R3ForLoopBlock<'a>) {
        self.visit_variable(&block.item);
        for v in &block.context_variables {
            self.visit_variable(v);
        }
        for node in &block.children {
            visit(node, self);
        }
        if let Some(empty) = &block.empty {
            self.visit_for_loop_block_empty(empty);
        }
    }

    // t2_binder.ts:687 visitForLoopBlockEmpty
    fn visit_for_loop_block_empty(&mut self, block: &'a R3ForLoopBlockEmpty<'a>) {
        for node in &block.children {
            visit(node, self);
        }
    }

    // t2_binder.ts:691 visitIfBlock
    fn visit_if_block(&mut self, block: &'a R3IfBlock<'a>) {
        for node in &block.branches {
            self.visit_if_block_branch(node);
        }
    }

    // t2_binder.ts:695 visitIfBlockBranch
    fn visit_if_block_branch(&mut self, block: &'a R3IfBlockBranch<'a>) {
        if let Some(expression_alias) = &block.expression_alias {
            self.visit_variable(expression_alias);
        }
        for node in &block.children {
            visit(node, self);
        }
    }

    // t2_binder.ts:700 visitContent
    fn visit_content(&mut self, content: &'a R3Content<'a>) {
        for child in &content.children {
            visit(child, self);
        }
    }

    // t2_binder.ts:704 visitContentBlock: not in the fork's AST

    // t2_binder.ts:708 visitComponent
    fn visit_component(&mut self, node: &'a R3Component<'a>) {
        if let Some(DirectiveMatcher::Selectorless(directive_matcher)) = self.directive_matcher {
            let component_matches = directive_matcher.match_(&node.component_name);

            if component_matches.is_empty() {
                self.missing_directives.insert(node.component_name.to_string());
            } else {
                self.track_selectorless_matches_and_directives(SelectorlessNode::Component(node), &component_matches);
            }
        }

        for directive in &node.directives {
            self.visit_directive(directive);
        }
        for child in &node.children {
            visit(child, self);
        }
    }

    // t2_binder.ts:723 visitDirective
    fn visit_directive(&mut self, node: &'a R3Directive<'a>) {
        if let Some(DirectiveMatcher::Selectorless(directive_matcher)) = self.directive_matcher {
            let directives = directive_matcher.match_(&node.name);

            if directives.is_empty() {
                self.missing_directives.insert(node.name.to_string());
            } else {
                self.track_selectorless_matches_and_directives(SelectorlessNode::Directive(node), &directives);
            }
        }
    }

    // t2_binder.ts:983 Unused visitors.
    fn visit_variable(&mut self, _variable: &'a R3Variable<'a>) {}
    fn visit_reference(&mut self, _reference: &'a R3Reference<'a>) {}
    fn visit_text_attribute(&mut self, _attribute: &'a R3TextAttribute<'a>) {}
    fn visit_bound_attribute(&mut self, _attribute: &'a R3BoundAttribute<'a>) {}
    fn visit_bound_event(&mut self, _attribute: &'a R3BoundEvent<'a>) {}
    // (visitBoundAttributeOrEvent: unused, and no visitor method)
    fn visit_text(&mut self, _text: &'a R3Text<'a>) {}
    fn visit_bound_text(&mut self, _text: &'a R3BoundText<'a>) {}
    fn visit_icu(&mut self, _icu: &'a R3Icu<'a>) {}
    fn visit_unknown_block(&mut self, _block: &'a R3UnknownBlock<'a>) {}
    fn visit_let_declaration(&mut self, _decl: &'a R3LetDeclaration<'a>) {}
}

impl<'a, D: BinderDirective> DirectiveBinder<'_, 'a, D> {
    // t2_binder.ts:735 visitElementOrTemplate
    fn visit_element_or_template(&mut self, node: ElementOrTemplate<'a>) {
        let mut matched_directives: Vec<D> = Vec::new();

        if let Some(DirectiveMatcher::Selector(directive_matcher)) = self.directive_matcher {
            let css_selector = create_css_selector_from_node(node);
            directive_matcher.match_(
                &css_selector,
                Some(&mut |_: &CssSelector, results: &Vec<D>| matched_directives.extend(results.iter().cloned())),
            );
            self.track_selector_based_bindings_and_directives(node, &matched_directives);
        } else {
            for r#ref in node.references() {
                if js_trim(&r#ref.value).is_empty() {
                    self.references.insert(node_key(r#ref), node.as_reference_target());
                }
            }
        }

        if let (Some(foreign_matcher), ElementOrTemplate::Element(element)) = (self.foreign_matcher, node) {
            let foreign_matches = foreign_matcher.match_(&element.name);
            if let Some(first) = foreign_matches.into_iter().next() {
                assert!(
                    matched_directives.is_empty(),
                    "Conflict: Element '{}' matches both an Angular directive and a foreign component.",
                    element.name
                );
                // We assume at most one foreign component matches by name.
                self.foreign_components.insert(node_key(element), first);
            }
        }

        for directive in node.directives() {
            self.visit_directive(directive);
        }
        for child in node.children() {
            visit(child, self);
        }
    }

    // t2_binder.ts:767 trackMatchedDirectives
    fn track_matched_directives(&mut self, node: DirectiveOwner<'a>, matched_directives: &[D]) {
        if !matched_directives.is_empty() {
            let directives = self.dedupe_and_merge_directives(node, matched_directives);
            self.directives.insert(node, directives.clone());
            if !self.is_in_defer_block {
                self.eager_directives.extend(directives);
            }
        }
    }

    // t2_binder.ts:777 dedupeAndMergeDirectives
    fn dedupe_and_merge_directives(&mut self, node: DirectiveOwner<'a>, matches: &[D]) -> Vec<D> {
        if matches.is_empty() || matches.iter().all(|dir| dir.match_source() == MatchSource::Selector) {
            return matches.to_vec();
        }

        let mut selector_matches: Set<String> = Set::default();
        let mut host_directives: Map<String, Vec<D>> = Map::default();
        let mut merged_host_directives: Map<String, D> = Map::default();

        for dir in matches {
            if dir.match_source() == MatchSource::Selector {
                selector_matches.insert(dir.ref_key().to_string());
            } else {
                host_directives.entry(dir.ref_key().to_string()).or_default().push(dir.clone());
            }
        }

        for (key, directives) in &host_directives {
            // Filter out host directives that also matched through the template.
            if selector_matches.contains(key) {
                continue;
            }

            if directives.len() == 1 {
                // Based on the prior loop, we should always have at least one directive.
                merged_host_directives.insert(key.clone(), directives[0].clone());
                continue;
            }

            let mut inputs: Map<ClassPropertyName, D::Input> = Map::default();
            let mut outputs: Map<ClassPropertyName, InputOrOutput> = Map::default();

            // Merge the bindings for all duplicate host directives.
            for dir in directives {
                self.merge_mapping(node, dir, InputOrOutputKind::Input, &mut inputs, dir.inputs());
                self.merge_mapping(node, dir, InputOrOutputKind::Output, &mut outputs, dir.outputs());
            }

            merged_host_directives.insert(
                key.clone(),
                directives[0].with_inputs_and_outputs(
                    ClassPropertyMapping::from_entries(js_object_key_order(inputs)),
                    ClassPropertyMapping::from_entries(js_object_key_order(outputs)),
                ),
            );
        }

        let mut result = Vec::new();
        for dir in matches {
            if dir.match_source() == MatchSource::Selector {
                result.push(dir.clone());
            } else if let Some(merged) = merged_host_directives.shift_remove(dir.ref_key()) {
                result.push(merged);
            }
        }
        result
    }

    // t2_binder.ts:836 mergeMapping
    fn merge_mapping<T: AsInputOrOutput + Clone>(
        &mut self,
        node: DirectiveOwner<'a>,
        directive: &D,
        kind: InputOrOutputKind,
        accumulator: &mut Map<ClassPropertyName, T>,
        bindings: &ClassPropertyMapping<T>,
    ) {
        for binding in bindings.iter() {
            let binding_io = binding.input_or_output();
            let Some(existing) = accumulator.get(&binding_io.class_property_name) else {
                // Untracked binding, track it.
                accumulator.insert(binding_io.class_property_name.clone(), binding.clone());
                continue;
            };
            let existing = existing.input_or_output();

            // If the binding is already tracked, but is equivalent to the existing binding, we can keep it.
            if existing.binding_property_name == binding_io.binding_property_name
                && existing.class_property_name == binding_io.class_property_name
                && existing.is_signal == binding_io.is_signal
            {
                continue;
            }

            // Otherwise track the binding as conflicting so it can be reported later.
            let conflicts_for_node = self.conflicting_host_directive_bindings.entry(node).or_default();
            let conflict_index = conflicts_for_node.iter().position(|current| {
                current.directive.ref_key() == directive.ref_key()
                    && current.kind == kind
                    && current.class_property_name == binding_io.class_property_name
            });

            let conflict_index = conflict_index.unwrap_or_else(|| {
                let mut conflicting_aliases = Set::default();
                conflicting_aliases.insert(existing.binding_property_name.clone());
                conflicts_for_node.push(ConflictingHostDirectiveBinding {
                    directive: directive.clone(),
                    kind,
                    class_property_name: existing.class_property_name.clone(),
                    conflicting_aliases,
                });
                conflicts_for_node.len() - 1
            });

            conflicts_for_node[conflict_index].conflicting_aliases.insert(binding_io.binding_property_name.clone());
        }
    }

    // t2_binder.ts:888 trackSelectorlessMatchesAndDirectives
    fn track_selectorless_matches_and_directives(&mut self, node: SelectorlessNode<'a>, directives: &[D]) {
        if directives.is_empty() {
            return;
        }

        self.track_matched_directives(node.owner(), directives);

        let mut set_binding = |meta: &D, attribute: Binding<'a>, io_type: IoType| {
            if has_binding_property_name(meta, io_type, binding_name(attribute)) {
                self.bindings.insert(attribute, BindingConsumer::Directive(meta.clone()));
            }
        };

        for directive in directives {
            for input in node.inputs() {
                set_binding(directive, Binding::BoundAttribute(input), IoType::Inputs);
            }
            for attr in node.attributes() {
                set_binding(directive, Binding::TextAttribute(attr), IoType::Inputs);
            }
            for output in node.outputs() {
                set_binding(directive, Binding::BoundEvent(output), IoType::Outputs);
            }
        }

        // TODO(crisbeto): currently it's unclear how references should behave under selectorless,
        // given that there's one named class which can bring in multiple host directives.
        // For the time being only register the first directive as the reference target.
        for r#ref in node.references() {
            self.references.insert(
                node_key(r#ref),
                ReferenceTarget::Directive { directive: directives[0].clone(), node: node.owner() },
            );
        }
    }

    // t2_binder.ts:922 trackSelectorBasedBindingsAndDirectives
    fn track_selector_based_bindings_and_directives(&mut self, node: ElementOrTemplate<'a>, directives: &[D]) {
        self.track_matched_directives(node.owner(), directives);

        // Resolve any references that are created on this node.
        for r#ref in node.references() {
            let dir_target: Option<D>;

            // If the reference expression is empty, then it matches the "primary" directive on the node
            // (if there is one). Otherwise it matches the host node itself (either an element or
            // <ng-template> node).
            if js_trim(&r#ref.value).is_empty() {
                // This could be a reference to a component if there is one.
                dir_target = directives.iter().find(|dir| dir.is_component()).cloned();
            } else {
                // This should be a reference to a directive exported via exportAs.
                dir_target = directives
                    .iter()
                    .find(|dir| {
                        dir.export_as().is_some_and(|export_as| export_as.iter().any(|value| value == &*r#ref.value))
                    })
                    .cloned();
                // Check if a matching directive was found.
                if dir_target.is_none() {
                    // No matching directive was found - this reference points to an unknown target. Leave it
                    // unmapped.
                    continue;
                }
            }

            if let Some(dir_target) = dir_target {
                // This reference points to a directive.
                self.references
                    .insert(node_key(r#ref), ReferenceTarget::Directive { directive: dir_target, node: node.owner() });
            } else {
                // This reference points to the node itself.
                self.references.insert(node_key(r#ref), node.as_reference_target());
            }
        }

        // Associate attributes/bindings on the node with directives or with the node itself.
        let mut set_attribute_binding = |attribute: Binding<'a>, io_type: IoType| {
            let dir = directives.iter().find(|dir| has_binding_property_name(*dir, io_type, binding_name(attribute)));
            let binding = match dir {
                Some(dir) => BindingConsumer::Directive(dir.clone()),
                None => node.as_binding_consumer(),
            };
            self.bindings.insert(attribute, binding);
        };

        // Node inputs (bound attributes) and text attributes can be bound to an
        // input on a directive.
        for input in node.inputs() {
            set_attribute_binding(Binding::BoundAttribute(input), IoType::Inputs);
        }
        for attr in node.attributes() {
            set_attribute_binding(Binding::TextAttribute(attr), IoType::Inputs);
        }
        if let ElementOrTemplate::Template(template) = node {
            for attr in &template.template_attrs {
                let attr = match attr {
                    R3TemplateAttr::Bound(attr) => Binding::BoundAttribute(attr),
                    R3TemplateAttr::Text(attr) => Binding::TextAttribute(attr),
                };
                set_attribute_binding(attr, IoType::Inputs);
            }
        }
        // Node outputs (bound events) can be bound to an output on a directive.
        for output in node.outputs() {
            set_attribute_binding(Binding::BoundEvent(output), IoType::Outputs);
        }
    }
}

// t2_binder.ts:1006 TemplateBinder
/// Processes a template and extract metadata about expressions and symbols within.
///
/// This is a companion to the `DirectiveBinder` that doesn't require knowledge of directives matched
/// within the template in order to operate.
///
/// Expressions are visited by the superclass `RecursiveAstVisitor`, with custom logic provided
/// by overridden methods from that visitor. (The superclass `CombinedRecursiveAstVisitor` and its superclass
/// `RecursiveAstVisitor` are ported below, as `TemplateBinder` methods marked with their files.)
struct TemplateBinder<'b, 'a> {
    /// By expression (`expr_key`)
    bindings: &'b mut Map<NodeKey, TemplateEntity<'a>>,
    symbols: &'b mut Map<TemplateEntity<'a>, ScopedNode<'a>>,
    used_pipes: &'b mut Set<String>,
    eager_pipes: &'b mut Set<String>,
    defer_blocks: &'b mut DeferBlockScopes<'a>,
    nesting_level: &'b mut Map<ScopedNode<'a>, usize>,
    scopes: &'b Scopes<'a>,
    scope: ScopeId,
    root_node: Option<ScopedNode<'a>>,
    level: usize,
    /// By pipe (`node_key`)
    pipes: &'b mut Map<NodeKey, Vec<&'a R3DeferredBlock<'a>>>,
}

impl<'b, 'a> TemplateBinder<'b, 'a> {
    // t2_binder.ts:1036 applyWithScope
    /// Process a template and extract metadata about expressions and symbols within.
    ///
    /// @param nodeOrNodes the nodes of the template to process
    /// @param scope the `Scope` of the template being processed.
    /// @returns three maps which contain metadata about the template: `expressions` which interprets
    /// special `AST` nodes in expressions as pointing to references or variables declared within the
    /// template, `symbols` which maps those variables and references to the nested `Template` which
    /// declares them, if any, and `nestingLevel` which associates each `Template` with a integer
    /// nesting level (how many levels deep within the template structure the `Template` is), starting
    /// at 1.
    fn apply_with_scope(
        node_or_nodes: NodeOrNodes<'a>,
        scopes: &'b Scopes<'a>,
        scope: ScopeId,
        expressions: &'b mut Map<NodeKey, TemplateEntity<'a>>,
        symbols: &'b mut Map<TemplateEntity<'a>, ScopedNode<'a>>,
        nesting_level: &'b mut Map<ScopedNode<'a>, usize>,
        used_pipes: &'b mut Set<String>,
        eager_pipes: &'b mut Set<String>,
        defer_blocks: &'b mut DeferBlockScopes<'a>,
        pipes: &'b mut Map<NodeKey, Vec<&'a R3DeferredBlock<'a>>>,
    ) {
        let template = match node_or_nodes {
            NodeOrNodes::Node(node @ ScopedNode::Template(_)) => Some(node),
            _ => None,
        };
        // The top-level template has nesting level 0.
        // t2_binder.ts:1009 constructor
        let mut binder = TemplateBinder {
            bindings: expressions,
            symbols,
            used_pipes,
            eager_pipes,
            defer_blocks,
            nesting_level,
            scopes,
            scope,
            root_node: template,
            level: 0,
            pipes,
        };
        binder.ingest(node_or_nodes);
    }

    // t2_binder.ts:1007 visitNode
    fn visit_node(&mut self, node: &'a R3Node<'a>) {
        visit(node, self);
    }

    // t2_binder.ts:1064 ingest
    fn ingest(&mut self, node_or_nodes: NodeOrNodes<'a>) {
        match node_or_nodes {
            NodeOrNodes::Node(scoped @ ScopedNode::Template(template)) => {
                // For <ng-template>s, process only variables and child nodes. Inputs, outputs, templateAttrs,
                // and references were all processed in the scope of the containing template.
                for node in &template.variables {
                    self.visit_variable(node);
                }
                for node in &template.children {
                    self.visit_node(node);
                }

                // Set the nesting level.
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::IfBlockBranch(block)) => {
                if let Some(expression_alias) = &block.expression_alias {
                    self.visit_variable(expression_alias);
                }
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::ForLoopBlock(block)) => {
                self.visit_variable(&block.item);
                for v in &block.context_variables {
                    self.visit_variable(v);
                }
                self.visit_ast_with_source(&block.track_by);
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::DeferredBlock(block)) => {
                // `${nodeOrNodes}` of a node object is `[object Object]`.
                assert!(
                    self.scopes.scopes[self.scope].root_node == Some(scoped),
                    "Assertion error: resolved incorrect scope for deferred block [object Object]"
                );
                self.defer_blocks.push((block, self.scope));
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            // `BoundaryErrorBlock` (context variables, expression, children, nesting level): not in the fork's AST.
            NodeOrNodes::Node(scoped @ ScopedNode::SwitchBlockCaseGroup(block)) => {
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::ForLoopBlockEmpty(block)) => {
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::DeferredBlockError(block)) => {
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::DeferredBlockPlaceholder(block)) => {
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::DeferredBlockLoading(block)) => {
                for node in &block.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            // (`ContentBlock` and `BoundaryBlock` too: not in the fork's AST.)
            NodeOrNodes::Node(scoped @ ScopedNode::Content(content)) => {
                for node in &content.children {
                    self.visit_node(node);
                }
                self.nesting_level.insert(scoped, self.level);
            }
            NodeOrNodes::Node(scoped @ ScopedNode::HostElement(_)) => {
                // Host elements are always at the top level.
                self.nesting_level.insert(scoped, 0);
            }
            NodeOrNodes::Nodes(nodes) => {
                // Visit each node from the top-level template.
                for node in nodes {
                    self.visit_node(node);
                }
            }
        }
    }
}

impl<'a> Visitor<'a> for TemplateBinder<'_, 'a> {
    // combined_visitor.ts:25 visitElement
    fn visit_element(&mut self, element: &'a R3Element<'a>) {
        for node in &element.attributes {
            self.visit_text_attribute(node);
        }
        for node in &element.inputs {
            self.visit_bound_attribute(node);
        }
        for node in &element.outputs {
            self.visit_bound_event(node);
        }
        for node in &element.directives {
            self.visit_directive(node);
        }
        for node in &element.references {
            self.visit_reference(node);
        }
        self.visit_all_template_nodes(&element.children);
    }

    // t2_binder.ts:1120 visitTemplate
    fn visit_template(&mut self, template: &'a R3Template<'a>) {
        // First, visit inputs, outputs and template attributes of the template node.
        for node in &template.inputs {
            self.visit_bound_attribute(node);
        }
        for node in &template.outputs {
            self.visit_bound_event(node);
        }
        for node in &template.directives {
            self.visit_directive(node);
        }
        for node in &template.template_attrs {
            match node {
                R3TemplateAttr::Bound(node) => self.visit_bound_attribute(node),
                R3TemplateAttr::Text(node) => self.visit_text_attribute(node),
            }
        }
        for node in &template.references {
            self.visit_reference(node);
        }

        // Next, recurse into the template.
        self.ingest_scoped_node(ScopedNode::Template(template));
    }

    // combined_visitor.ts:45 visitContent is overridden by t2_binder.ts:1206 visitContent
    // t2_binder.ts:1206 visitContent
    fn visit_content(&mut self, content: &'a R3Content<'a>) {
        self.ingest_scoped_node(ScopedNode::Content(content));
    }

    // t2_binder.ts:1132 visitVariable
    fn visit_variable(&mut self, variable: &'a R3Variable<'a>) {
        // Register the `Variable` as a symbol in the current `Template`.
        if let Some(root_node) = self.root_node {
            self.symbols.insert(TemplateEntity::Variable(variable), root_node);
        }
    }

    // t2_binder.ts:1139 visitReference
    fn visit_reference(&mut self, reference: &'a R3Reference<'a>) {
        // Register the `Reference` as a symbol in the current `Template`.
        if let Some(root_node) = self.root_node {
            self.symbols.insert(TemplateEntity::Reference(reference), root_node);
        }
    }

    // combined_visitor.ts:166 visitTextAttribute
    fn visit_text_attribute(&mut self, _attribute: &'a R3TextAttribute<'a>) {}

    // combined_visitor.ts:53 visitBoundAttribute
    fn visit_bound_attribute(&mut self, attribute: &'a R3BoundAttribute<'a>) {
        self.visit_ast(&attribute.value);
    }

    // combined_visitor.ts:57 visitBoundEvent
    fn visit_bound_event(&mut self, attribute: &'a R3BoundEvent<'a>) {
        self.visit_ast(&attribute.handler);
    }

    // combined_visitor.ts:167 visitText
    fn visit_text(&mut self, _text: &'a R3Text<'a>) {}

    // combined_visitor.ts:61 visitBoundText
    fn visit_bound_text(&mut self, text: &'a R3BoundText<'a>) {
        self.visit_ast(&text.value);
    }

    // combined_visitor.ts:65 visitIcu
    fn visit_icu(&mut self, icu: &'a R3Icu<'a>) {
        for (_, var) in &icu.vars {
            self.visit_bound_text(var);
        }
        for (_, placeholder) in &icu.placeholders {
            match placeholder {
                R3IcuPlaceholder::Text(text) => self.visit_text(text),
                R3IcuPlaceholder::BoundText(text) => self.visit_bound_text(text),
            }
        }
    }

    // t2_binder.ts:1146 visitDeferredBlock
    fn visit_deferred_block(&mut self, deferred: &'a R3DeferredBlock<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlock(deferred));
        if let Some(when) = &deferred.triggers.when {
            self.visit_ast(&when.value);
        }
        if let Some(when) = &deferred.prefetch_triggers.when {
            self.visit_ast(&when.value);
        }
        if let Some(when) = &deferred.hydrate_triggers.when {
            self.visit_ast(&when.value);
        }
        // `deferred.hydrateTriggers.never?.visit(this)`: `visitDeferredTrigger` (combined_visitor.ts:74) visits only
        // a bound trigger's value or a viewport trigger's options, and does nothing for a `NeverDeferredTrigger`.
        if let Some(placeholder) = &deferred.placeholder {
            self.visit_deferred_block_placeholder(placeholder);
        }
        if let Some(loading) = &deferred.loading {
            self.visit_deferred_block_loading(loading);
        }
        if let Some(error) = &deferred.error {
            self.visit_deferred_block_error(error);
        }
    }

    // t2_binder.ts:1157 visitDeferredBlockPlaceholder
    fn visit_deferred_block_placeholder(&mut self, block: &'a R3DeferredBlockPlaceholder<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlockPlaceholder(block));
    }

    // t2_binder.ts:1161 visitDeferredBlockError
    fn visit_deferred_block_error(&mut self, block: &'a R3DeferredBlockError<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlockError(block));
    }

    // t2_binder.ts:1165 visitDeferredBlockLoading
    fn visit_deferred_block_loading(&mut self, block: &'a R3DeferredBlockLoading<'a>) {
        self.ingest_scoped_node(ScopedNode::DeferredBlockLoading(block));
    }

    // t2_binder.ts:1169 visitBoundaryBlock, t2_binder.ts:1174 visitBoundaryErrorBlock: not in the fork's AST

    // combined_visitor.ts:94 visitSwitchBlock
    fn visit_switch_block(&mut self, block: &'a R3SwitchBlock<'a>) {
        self.visit_ast(&block.expression);
        for node in &block.groups {
            self.visit_switch_block_case_group(node);
        }
        if let Some(exhaustive_check) = &block.exhaustive_check {
            self.visit_switch_exhaustive_check(exhaustive_check);
        }
    }

    // t2_binder.ts:1178 visitSwitchBlockCase
    fn visit_switch_block_case(&mut self, block: &'a R3SwitchBlockCase<'a>) {
        if let Some(expression) = &block.expression {
            self.visit_ast(expression);
        }
    }

    // t2_binder.ts:1182 visitSwitchBlockCaseGroup
    fn visit_switch_block_case_group(&mut self, block: &'a R3SwitchBlockCaseGroup<'a>) {
        for case_node in &block.cases {
            self.visit_switch_block_case(case_node);
        }
        self.ingest_scoped_node(ScopedNode::SwitchBlockCaseGroup(block));
    }

    // t2_binder.ts:1187 visitSwitchExhaustiveCheck
    fn visit_switch_exhaustive_check(&mut self, block: &'a R3SwitchExhaustiveCheck<'a>) {
        if let Some(expression) = &block.expression {
            self.visit_ast(expression);
        }
    }

    // t2_binder.ts:1191 visitForLoopBlock
    fn visit_for_loop_block(&mut self, block: &'a R3ForLoopBlock<'a>) {
        self.visit_ast_with_source(&block.expression);
        self.ingest_scoped_node(ScopedNode::ForLoopBlock(block));
        if let Some(empty) = &block.empty {
            self.visit_for_loop_block_empty(empty);
        }
    }

    // t2_binder.ts:1197 visitForLoopBlockEmpty
    fn visit_for_loop_block_empty(&mut self, block: &'a R3ForLoopBlockEmpty<'a>) {
        self.ingest_scoped_node(ScopedNode::ForLoopBlockEmpty(block));
    }

    // combined_visitor.ts:123 visitIfBlock
    fn visit_if_block(&mut self, block: &'a R3IfBlock<'a>) {
        for node in &block.branches {
            self.visit_if_block_branch(node);
        }
    }

    // t2_binder.ts:1201 visitIfBlockBranch
    fn visit_if_block_branch(&mut self, block: &'a R3IfBlockBranch<'a>) {
        if let Some(expression) = &block.expression {
            self.visit_ast(expression);
        }
        self.ingest_scoped_node(ScopedNode::IfBlockBranch(block));
    }

    // t2_binder.ts:1210 visitContentBlock: not in the fork's AST

    // combined_visitor.ts:168 visitUnknownBlock
    fn visit_unknown_block(&mut self, _block: &'a R3UnknownBlock<'a>) {}

    // t2_binder.ts:1214 visitLetDeclaration
    fn visit_let_declaration(&mut self, decl: &'a R3LetDeclaration<'a>) {
        // combined_visitor.ts:144 visitLetDeclaration (super)
        self.visit_ast(&decl.value);

        if let Some(root_node) = self.root_node {
            self.symbols.insert(TemplateEntity::LetDeclaration(decl), root_node);
        }
    }

    // combined_visitor.ts:148 visitComponent
    fn visit_component(&mut self, component: &'a R3Component<'a>) {
        for node in &component.attributes {
            self.visit_text_attribute(node);
        }
        for node in &component.inputs {
            self.visit_bound_attribute(node);
        }
        for node in &component.outputs {
            self.visit_bound_event(node);
        }
        for node in &component.directives {
            self.visit_directive(node);
        }
        for node in &component.references {
            self.visit_reference(node);
        }
        self.visit_all_template_nodes(&component.children);
    }

    // combined_visitor.ts:157 visitDirective
    fn visit_directive(&mut self, directive: &'a R3Directive<'a>) {
        for node in &directive.attributes {
            self.visit_text_attribute(node);
        }
        for node in &directive.inputs {
            self.visit_bound_attribute(node);
        }
        for node in &directive.outputs {
            self.visit_bound_event(node);
        }
        for node in &directive.references {
            self.visit_reference(node);
        }
    }
}

impl<'a> TemplateBinder<'_, 'a> {
    // combined_visitor.ts:170 visitAllTemplateNodes
    fn visit_all_template_nodes(&mut self, nodes: &'a [R3Node<'a>]) {
        for node in nodes {
            self.visit_node(node);
        }
    }

    // combined_visitor.ts:17 visit, for an `ASTWithSource`: visits its `ast`
    fn visit_ast_with_source(&mut self, node: &'a ASTWithSource<'a>) {
        self.visit_ast(&node.ast);
    }

    // combined_visitor.ts:17 visit, for an expression: `node.visit(this)`, which calls the visitor's method for the
    // expression's type (ast.ts:741 RecursiveAstVisitor, below)
    fn visit_ast(&mut self, ast: &'a AngularExpression<'a>) {
        use AngularExpression as E;
        match ast {
            E::Empty(ast) => self.visit_empty_expr(ast),
            E::ImplicitReceiver(ast) => self.visit_implicit_receiver(ast),
            E::ThisReceiver(ast) => self.visit_this_receiver(ast),
            E::Chain(ast) => self.visit_chain(ast),
            E::Conditional(ast) => self.visit_conditional(ast),
            E::PropertyRead(ast) => self.visit_property_read(ast),
            E::SafePropertyRead(ast) => self.visit_safe_property_read(ast),
            E::KeyedRead(ast) => self.visit_keyed_read(ast),
            E::SafeKeyedRead(ast) => self.visit_safe_keyed_read(ast),
            E::BindingPipe(ast) => self.visit_pipe(ast),
            E::LiteralPrimitive(ast) => self.visit_literal_primitive(ast),
            E::LiteralArray(ast) => self.visit_literal_array(ast),
            E::LiteralMap(ast) => self.visit_literal_map(ast),
            E::Interpolation(ast) => self.visit_interpolation(ast),
            E::Binary(ast) => self.visit_binary(ast),
            E::Unary(ast) => self.visit_unary(ast),
            E::PrefixNot(ast) => self.visit_prefix_not(ast),
            E::TypeofExpression(ast) => self.visit_typeof_expression(ast),
            E::VoidExpression(ast) => self.visit_void_expression(ast),
            E::NonNullAssert(ast) => self.visit_non_null_assert(ast),
            E::Call(ast) => self.visit_call(ast),
            E::SafeCall(ast) => self.visit_safe_call(ast),
            E::TaggedTemplateLiteral(ast) => self.visit_tagged_template_literal(ast),
            E::TemplateLiteral(ast) => self.visit_template_literal(ast),
            E::ParenthesizedExpression(ast) => self.visit_parenthesized_expression(ast),
            E::RegularExpressionLiteral(ast) => self.visit_regular_expression_literal(ast),
            E::SpreadElement(ast) => self.visit_spread_element(ast),
            E::ArrowFunction(ast) => self.visit_arrow_function(ast),
        }
    }

    // ast.ts:748 visitUnary
    fn visit_unary(&mut self, ast: &'a Unary<'a>) {
        self.visit_ast(&ast.expr);
    }

    // ast.ts:751 visitBinary
    fn visit_binary(&mut self, ast: &'a Binary<'a>) {
        self.visit_ast(&ast.left);
        self.visit_ast(&ast.right);
    }

    // ast.ts:755 visitChain
    fn visit_chain(&mut self, ast: &'a Chain<'a>) {
        self.visit_all(&ast.expressions);
    }

    // ast.ts:758 visitConditional
    fn visit_conditional(&mut self, ast: &'a Conditional<'a>) {
        self.visit_ast(&ast.condition);
        self.visit_ast(&ast.true_exp);
        self.visit_ast(&ast.false_exp);
    }

    // t2_binder.ts:1222 visitPipe
    fn visit_pipe(&mut self, ast: &'a BindingPipe<'a>) {
        self.used_pipes.insert(ast.name.to_string());
        if !self.scopes.scopes[self.scope].is_deferred {
            self.eager_pipes.insert(ast.name.to_string());
        }
        self.pipes.insert(node_key(ast), self.scopes.get_enclosing_defer_blocks(self.scope));
        // ast.ts:763 visitPipe (super)
        self.visit_ast(&ast.exp);
        self.visit_all(&ast.args);
    }

    // ast.ts:767 visitImplicitReceiver
    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut, reason = "the TypeScript method")]
    fn visit_implicit_receiver(&mut self, _ast: &'a ImplicitReceiver) {}

    // ast.ts:768 visitThisReceiver
    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut, reason = "the TypeScript method")]
    fn visit_this_receiver(&mut self, _ast: &'a ThisReceiver) {}

    // ast.ts:769 visitInterpolation
    fn visit_interpolation(&mut self, ast: &'a Interpolation<'a>) {
        self.visit_all(&ast.expressions);
    }

    // ast.ts:772 visitKeyedRead
    fn visit_keyed_read(&mut self, ast: &'a KeyedRead<'a>) {
        self.visit_ast(&ast.receiver);
        self.visit_ast(&ast.key);
    }

    // ast.ts:776 visitLiteralArray
    fn visit_literal_array(&mut self, ast: &'a LiteralArray<'a>) {
        self.visit_all(&ast.expressions);
    }

    // ast.ts:779 visitLiteralMap
    fn visit_literal_map(&mut self, ast: &'a LiteralMap<'a>) {
        self.visit_all(&ast.values);
    }

    // ast.ts:782 visitLiteralPrimitive
    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut, reason = "the TypeScript method")]
    fn visit_literal_primitive(&mut self, _ast: &'a LiteralPrimitive<'a>) {}

    // ast.ts:783 visitPrefixNot
    fn visit_prefix_not(&mut self, ast: &'a PrefixNot<'a>) {
        self.visit_ast(&ast.expression);
    }

    // ast.ts:786 visitTypeofExpression
    fn visit_typeof_expression(&mut self, ast: &'a TypeofExpression<'a>) {
        self.visit_ast(&ast.expression);
    }

    // ast.ts:789 visitVoidExpression
    fn visit_void_expression(&mut self, ast: &'a VoidExpression<'a>) {
        self.visit_ast(&ast.expression);
    }

    // ast.ts:792 visitNonNullAssert
    fn visit_non_null_assert(&mut self, ast: &'a NonNullAssert<'a>) {
        self.visit_ast(&ast.expression);
    }

    // These five types of AST expressions can refer to expression roots, which could be variables
    // or references in the current scope.

    // t2_binder.ts:1234 visitPropertyRead
    fn visit_property_read(&mut self, ast: &'a PropertyRead<'a>) {
        self.maybe_map(node_key(ast), &ast.receiver, &ast.name);
        // ast.ts:795 visitPropertyRead (super)
        self.visit_ast(&ast.receiver);
    }

    // t2_binder.ts:1239 visitSafePropertyRead
    fn visit_safe_property_read(&mut self, ast: &'a SafePropertyRead<'a>) {
        self.maybe_map(node_key(ast), &ast.receiver, &ast.name);
        // ast.ts:798 visitSafePropertyRead (super)
        self.visit_ast(&ast.receiver);
    }

    // ast.ts:801 visitSafeKeyedRead
    fn visit_safe_keyed_read(&mut self, ast: &'a SafeKeyedRead<'a>) {
        self.visit_ast(&ast.receiver);
        self.visit_ast(&ast.key);
    }

    // ast.ts:805 visitCall
    fn visit_call(&mut self, ast: &'a Call<'a>) {
        self.visit_ast(&ast.receiver);
        self.visit_all(&ast.args);
    }

    // ast.ts:809 visitSafeCall
    fn visit_safe_call(&mut self, ast: &'a SafeCall<'a>) {
        self.visit_ast(&ast.receiver);
        self.visit_all(&ast.args);
    }

    // ast.ts:813 visitTemplateLiteral
    fn visit_template_literal(&mut self, ast: &'a TemplateLiteral<'a>) {
        // Iterate in the declaration order. Note that there will
        // always be one expression less than the number of elements.
        for (i, element) in ast.elements.iter().enumerate() {
            self.visit_template_literal_element(element);

            if let Some(expression) = ast.expressions.get(i) {
                self.visit_ast(expression);
            }
        }
    }

    // ast.ts:825 visitTemplateLiteralElement
    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut, reason = "the TypeScript method")]
    fn visit_template_literal_element(&mut self, _ast: &'a TemplateLiteralElement<'a>) {}

    // ast.ts:826 visitTaggedTemplateLiteral
    fn visit_tagged_template_literal(&mut self, ast: &'a TaggedTemplateLiteral<'a>) {
        self.visit_ast(&ast.tag);
        self.visit_template_literal(&ast.template);
    }

    // ast.ts:830 visitParenthesizedExpression
    fn visit_parenthesized_expression(&mut self, ast: &'a ParenthesizedExpression<'a>) {
        self.visit_ast(&ast.expression);
    }

    // ast.ts:833 visitArrowFunction
    fn visit_arrow_function(&mut self, ast: &'a ArrowFunction<'a>) {
        self.visit_ast(&ast.body);
    }

    // ast.ts:836 visitRegularExpressionLiteral
    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut, reason = "the TypeScript method")]
    fn visit_regular_expression_literal(&mut self, _ast: &'a RegularExpressionLiteral<'a>) {}

    // ast.ts:837 visitSpreadElement
    fn visit_spread_element(&mut self, ast: &'a SpreadElement<'a>) {
        self.visit_ast(&ast.expression);
    }

    // ast.ts:840 visitEmptyExpr
    #[expect(clippy::unused_self, clippy::needless_pass_by_ref_mut, reason = "the TypeScript method")]
    fn visit_empty_expr(&mut self, _ast: &'a EmptyExpr) {}

    // ast.ts:842 visitAll
    fn visit_all(&mut self, asts: &'a [AngularExpression<'a>]) {
        for ast in asts {
            self.visit_ast(ast);
        }
    }

    // t2_binder.ts:1244 ingestScopedNode
    fn ingest_scoped_node(&mut self, node: ScopedNode<'a>) {
        let child_scope = self.scopes.get_child_scope(self.scope, node);
        let mut binder = TemplateBinder {
            bindings: self.bindings,
            symbols: self.symbols,
            used_pipes: self.used_pipes,
            eager_pipes: self.eager_pipes,
            defer_blocks: self.defer_blocks,
            nesting_level: self.nesting_level,
            scopes: self.scopes,
            scope: child_scope,
            root_node: Some(node),
            level: self.level + 1,
            pipes: self.pipes,
        };
        binder.ingest(NodeOrNodes::Node(node));
    }

    // t2_binder.ts:1261 maybeMap (`ast` is the read's `expr_key`, `receiver` its receiver)
    fn maybe_map(&mut self, ast: NodeKey, receiver: &AngularExpression<'a>, name: &str) {
        // If the receiver of the expression isn't the `ImplicitReceiver`, this isn't the root of an
        // `AST` expression that maps to a `Variable` or `Reference`.
        if !matches!(receiver, AngularExpression::ImplicitReceiver(_)) {
            return;
        }

        // Check whether the name exists in the current scope. If so, map it. Otherwise, the name is
        // probably a property on the top-level component context.
        if let Some(target) = self.scopes.lookup(self.scope, name) {
            self.bindings.insert(ast, target);
        }
    }
}

// t2_binder.ts:1282 R3BoundTarget
/// Metadata container for a `Target` that allows queries for specific bits of metadata.
///
/// See `BoundTarget` for documentation on the individual methods.
pub struct R3BoundTarget<'a, D> {
    target: Target<'a, D>,
    directives: MatchedDirectives<'a, D>,
    /// By element (`node_key`)
    foreign_components: Map<NodeKey, ForeignComponentMeta>,
    eager_directives: Vec<D>,
    missing_directives: Set<String>,
    bindings: BindingsMap<'a, D>,
    references: ReferenceMap<'a, D>,
    /// By expression (`expr_key`)
    expr_targets: Map<NodeKey, TemplateEntity<'a>>,
    symbols: Map<TemplateEntity<'a>, ScopedNode<'a>>,
    nesting_level: Map<ScopedNode<'a>, usize>,
    scoped_node_entities: ScopedNodeEntities<'a>,
    used_pipes: Set<String>,
    eager_pipes: Set<String>,
    /// By pipe (`node_key`)
    pipes: Map<NodeKey, Vec<&'a R3DeferredBlock<'a>>>,
    conflicting_host_directive_bindings: ConflictingHostDirectiveBindings<'a, D>,
    /// Deferred blocks, ordered as they appear in the template.
    deferred_blocks: Vec<&'a R3DeferredBlock<'a>>,
    /// Map of deferred blocks (by `node_key`) to their scope.
    deferred_scopes: Map<NodeKey, ScopeId>,
    /// The scopes that `deferred_scopes` refer to.
    scopes: Scopes<'a>,
}

impl<'a, D: BinderDirective> R3BoundTarget<'a, D> {
    // t2_binder.ts:1289 constructor
    fn new(
        target: Target<'a, D>,
        directives: MatchedDirectives<'a, D>,
        foreign_components: Map<NodeKey, ForeignComponentMeta>,
        eager_directives: Vec<D>,
        missing_directives: Set<String>,
        bindings: BindingsMap<'a, D>,
        references: ReferenceMap<'a, D>,
        expr_targets: Map<NodeKey, TemplateEntity<'a>>,
        symbols: Map<TemplateEntity<'a>, ScopedNode<'a>>,
        nesting_level: Map<ScopedNode<'a>, usize>,
        scoped_node_entities: ScopedNodeEntities<'a>,
        used_pipes: Set<String>,
        eager_pipes: Set<String>,
        raw_deferred: &DeferBlockScopes<'a>,
        pipes: Map<NodeKey, Vec<&'a R3DeferredBlock<'a>>>,
        conflicting_host_directive_bindings: ConflictingHostDirectiveBindings<'a, D>,
        scopes: Scopes<'a>,
    ) -> Self {
        let deferred_blocks = raw_deferred.iter().map(|current| current.0).collect();
        let deferred_scopes = raw_deferred.iter().map(|(block, scope)| (node_key(*block), *scope)).collect();
        Self {
            target,
            directives,
            foreign_components,
            eager_directives,
            missing_directives,
            bindings,
            references,
            expr_targets,
            symbols,
            nesting_level,
            scoped_node_entities,
            used_pipes,
            eager_pipes,
            pipes,
            conflicting_host_directive_bindings,
            deferred_blocks,
            deferred_scopes,
            scopes,
        }
    }

    // t2_binder.ts:1483 findEntityInScope
    /// Finds an entity with a specific name in a scope.
    /// @param rootNode Root node of the scope.
    /// @param name Name of the entity.
    fn find_entity_in_scope(&self, root_node: ScopedNode<'a>, name: &str) -> Option<TemplateEntity<'a>> {
        let entities = self.get_entities_in_scope(Some(root_node));

        entities.into_iter().find(|entity| entity_name(*entity) == name)
    }

    // t2_binder.ts:1496 referenceTargetToElement
    /// Coerces a `ReferenceTarget` to an `Element`, if possible.
    fn reference_target_to_element(target: &ReferenceTarget<'a, D>) -> Option<&'a R3Element<'a>> {
        match *target {
            ReferenceTarget::Element(element) => Some(element),
            ReferenceTarget::Template(_)
            | ReferenceTarget::Directive {
                node: DirectiveOwner::Component(_) | DirectiveOwner::Directive(_) | DirectiveOwner::HostElement(_),
                ..
            } => None,
            ReferenceTarget::Directive { node: DirectiveOwner::Element(node), .. } => {
                Self::reference_target_to_element(&ReferenceTarget::Element(node))
            }
            ReferenceTarget::Directive { node: DirectiveOwner::Template(node), .. } => {
                Self::reference_target_to_element(&ReferenceTarget::Template(node))
            }
        }
    }
}

impl<'a, D: BinderDirective> BoundTarget<'a, D> for R3BoundTarget<'a, D> {
    fn target(&self) -> &Target<'a, D> {
        &self.target
    }

    // t2_binder.ts:1314 getEntitiesInScope
    fn get_entities_in_scope(&self, node: Option<ScopedNode<'a>>) -> Vec<TemplateEntity<'a>> {
        self.scoped_node_entities.get(&node).map(|entities| entities.iter().copied().collect()).unwrap_or_default()
    }

    // t2_binder.ts:1318 getDirectivesOfNode
    fn get_directives_of_node(&self, node: DirectiveOwner<'a>) -> Option<Vec<D>> {
        self.directives.get(&node).cloned()
    }

    // t2_binder.ts:1322 getForeignComponent
    fn get_foreign_component(&self, element: &'a R3Element<'a>) -> Option<ForeignComponentMeta> {
        self.foreign_components.get(&node_key(element)).cloned()
    }

    // t2_binder.ts:1326 getReferenceTarget
    fn get_reference_target(&self, r#ref: &'a R3Reference<'a>) -> Option<ReferenceTarget<'a, D>> {
        self.references.get(&node_key(r#ref)).cloned()
    }

    // t2_binder.ts:1330 getConsumerOfBinding
    fn get_consumer_of_binding(&self, binding: Binding<'a>) -> Option<BindingConsumer<'a, D>> {
        self.bindings.get(&binding).cloned()
    }

    // t2_binder.ts:1336 getExpressionTarget
    fn get_expression_target(&self, expr: &AngularExpression<'a>) -> Option<TemplateEntity<'a>> {
        self.expr_targets.get(&expr_key(expr)).copied()
    }

    // t2_binder.ts:1340 getDefinitionNodeOfSymbol
    fn get_definition_node_of_symbol(&self, symbol: TemplateEntity<'a>) -> Option<ScopedNode<'a>> {
        self.symbols.get(&symbol).copied()
    }

    // t2_binder.ts:1344 getNestingLevel
    fn get_nesting_level(&self, node: ScopedNode<'a>) -> usize {
        self.nesting_level.get(&node).copied().unwrap_or(0)
    }

    // t2_binder.ts:1348 getUsedDirectives
    fn get_used_directives(&self) -> Vec<D> {
        let mut set: Map<usize, D> = Map::default();
        for dirs in self.directives.values() {
            for dir in dirs {
                set.entry(dir.identity()).or_insert_with(|| dir.clone());
            }
        }
        set.into_values().collect()
    }

    // t2_binder.ts:1354 getEagerlyUsedDirectives
    fn get_eagerly_used_directives(&self) -> Vec<D> {
        let mut set: Map<usize, D> = Map::default();
        for dir in &self.eager_directives {
            set.entry(dir.identity()).or_insert_with(|| dir.clone());
        }
        set.into_values().collect()
    }

    // t2_binder.ts:1359 getUsedPipes
    fn get_used_pipes(&self) -> Vec<String> {
        self.used_pipes.iter().cloned().collect()
    }

    // t2_binder.ts:1363 getEagerlyUsedPipes
    fn get_eagerly_used_pipes(&self) -> Vec<String> {
        self.eager_pipes.iter().cloned().collect()
    }

    // t2_binder.ts:1367 getDeferBlocks
    fn get_defer_blocks(&self) -> Vec<&'a R3DeferredBlock<'a>> {
        self.deferred_blocks.clone()
    }

    // t2_binder.ts:1371 getDeferredTriggerTarget
    fn get_deferred_trigger_target(
        &self,
        block: &'a R3DeferredBlock<'a>,
        trigger: ReferenceDeferredTrigger<'a>,
    ) -> Option<&'a R3Element<'a>> {
        // Only triggers that refer to DOM nodes can be resolved (`ReferenceDeferredTrigger` holds only those).
        let name = match trigger {
            ReferenceDeferredTrigger::Interaction(trigger) => &trigger.reference,
            ReferenceDeferredTrigger::Viewport(trigger) => &trigger.reference,
            ReferenceDeferredTrigger::Hover(trigger) => &trigger.reference,
        };

        let Some(name) = name else {
            let mut target: Option<&'a R3Element<'a>> = None;

            if let Some(placeholder) = &block.placeholder {
                for child in &placeholder.children {
                    // Skip over comment nodes. Currently by default the template parser doesn't capture
                    // comments, but we have a safeguard here just in case since it can be enabled.
                    if let R3Node::Comment(_) = child {
                        continue;
                    }

                    // We can only infer the trigger if there's one root element node. Any other
                    // nodes at the root make it so that we can't infer the trigger anymore.
                    if target.is_some() {
                        return None;
                    }

                    if let R3Node::Element(child) = child {
                        target = Some(child);
                    }
                }
            }

            return target;
        };

        let outside_ref = self.find_entity_in_scope(ScopedNode::DeferredBlock(block), name);

        // First try to resolve the target in the scope of the main deferred block. Note that we
        // skip triggers defined inside the main block itself, because they might not exist yet.
        if let Some(outside_ref @ TemplateEntity::Reference(r#ref)) = outside_ref
            && self.get_definition_node_of_symbol(outside_ref) != Some(ScopedNode::DeferredBlock(block))
            && let Some(target) = self.get_reference_target(r#ref)
        {
            return Self::reference_target_to_element(&target);
        }

        // If the trigger couldn't be found in the main block, check the
        // placeholder block which is shown before the main block has loaded.
        if let Some(placeholder) = &block.placeholder {
            let ref_in_placeholder = self.find_entity_in_scope(ScopedNode::DeferredBlockPlaceholder(placeholder), name);
            let target_in_placeholder = match ref_in_placeholder {
                Some(TemplateEntity::Reference(r#ref)) => self.get_reference_target(r#ref),
                _ => None,
            };

            if let Some(target_in_placeholder) = target_in_placeholder {
                return Self::reference_target_to_element(&target_in_placeholder);
            }
        }

        None
    }

    // t2_binder.ts:1436 isDeferred
    fn is_deferred(&self, node: DirectiveOwner<'a>) -> bool {
        !self.get_defer_blocks_of_node(node).is_empty()
    }

    // t2_binder.ts:1440 getDeferBlocksOfNode
    fn get_defer_blocks_of_node(&self, node: DirectiveOwner<'a>) -> Vec<&'a R3DeferredBlock<'a>> {
        let mut blocks = Vec::new();
        for block in &self.deferred_blocks {
            let Some(scope) = self.deferred_scopes.get(&node_key(*block)) else {
                continue;
            };

            let mut stack: Vec<ScopeId> = vec![*scope];

            while let Some(current) = stack.pop() {
                let current = &self.scopes.scopes[current];

                if current.element_like_in_scope.contains(&node) {
                    blocks.push(*block);
                    break;
                }

                stack.extend(current.child_scopes.values().copied());
            }
        }

        blocks
    }

    // t2_binder.ts:1464 getDeferBlocksOfPipe
    fn get_defer_blocks_of_pipe(&self, ast: &'a BindingPipe<'a>) -> Vec<&'a R3DeferredBlock<'a>> {
        self.pipes.get(&node_key(ast)).cloned().unwrap_or_default()
    }

    // t2_binder.ts:1468 referencedDirectiveExists
    fn referenced_directive_exists(&self, name: &str) -> bool {
        !self.missing_directives.contains(name)
    }

    // t2_binder.ts:1472 getConflictingHostDirectiveBindings
    fn get_conflicting_host_directive_bindings(
        &self,
        node: DirectiveOwner<'a>,
    ) -> Option<Vec<ConflictingHostDirectiveBinding<D>>> {
        self.conflicting_host_directive_bindings.get(&node).cloned()
    }
}

/// `entityMap` of `extractScopedNodeEntities`
type EntityMap<'a> = Map<Option<ScopedNode<'a>>, Map<String, TemplateEntity<'a>>>;

// t2_binder.ts:1514 extractScopedNodeEntities
fn extract_scoped_node_entities<'a>(
    scopes: &Scopes<'a>,
    root_scope: ScopeId,
    template_entities: &mut ScopedNodeEntities<'a>,
) {
    // t2_binder.ts:1517 extractScopeEntities
    fn extract_scope_entities<'a>(
        scopes: &Scopes<'a>,
        entity_map: &mut EntityMap<'a>,
        scope: ScopeId,
    ) -> Map<String, TemplateEntity<'a>> {
        let this = &scopes.scopes[scope];
        if let Some(entities) = entity_map.get(&this.root_node) {
            return entities.clone();
        }

        let current_entities = &this.named_entities;

        let entities: Map<String, TemplateEntity<'a>> = if let Some(parent_scope) = this.parent_scope {
            let mut entities = extract_scope_entities(scopes, entity_map, parent_scope);
            for (name, entity) in current_entities {
                entities.insert(name.clone(), *entity);
            }
            entities
        } else {
            current_entities.clone()
        };

        entity_map.insert(this.root_node, entities.clone());
        entities
    }

    let mut entity_map: EntityMap<'a> = Map::default();
    let mut scopes_to_process: Vec<ScopeId> = vec![root_scope];
    while let Some(scope) = scopes_to_process.pop() {
        for child_scope in scopes.scopes[scope].child_scopes.values() {
            scopes_to_process.push(*child_scope);
        }
        extract_scope_entities(scopes, &mut entity_map, scope);
    }

    for (template, entities) in entity_map {
        template_entities.insert(template, entities.into_values().collect());
    }
}

// util.ts:170 createCssSelectorFromNode
/// Creates a `CssSelector` from an AST node.
pub fn create_css_selector_from_node(node: ElementOrTemplate<'_>) -> CssSelector {
    let element_name = match node {
        ElementOrTemplate::Element(element) => element.name.as_str(),
        ElementOrTemplate::Template(_) => "ng-template",
    };
    let attributes = get_attrs_for_directive_matching(node);
    let mut css_selector = CssSelector::new();
    let element_name_no_ns = split_ns_name(element_name).1;

    css_selector.set_element(Some(element_name_no_ns));

    for (name, value) in &attributes {
        let name_no_ns = split_ns_name(name).1;

        css_selector.add_attribute(name_no_ns, value);
        if name.to_lowercase() == "class" {
            for class_name in js_split_whitespace(js_trim(value)) {
                css_selector.add_class_name(class_name);
            }
        }
    }

    css_selector
}

// util.ts:201 getAttrsForDirectiveMatching
/// Extract a map of properties to values for a given element or template node, which can be used
/// by the directive matching machinery.
///
/// @param elOrTpl the element or template in question
/// @return an object set up for directive matching. For attributes on the element/template, this
/// object maps a property name to its (static) value. For any bindings, this map simply maps the
/// property name to an empty string.
///
/// (In the order of `Object.getOwnPropertyNames` on the TypeScript object, which `createCssSelectorFromNode`
/// iterates.)
fn get_attrs_for_directive_matching(el_or_tpl: ElementOrTemplate<'_>) -> Map<String, String> {
    let mut attributes_map: Map<String, String> = Map::default();

    if let ElementOrTemplate::Template(template) = el_or_tpl
        && template.tag_name.as_ref().is_none_or(|tag_name| tag_name.is_empty() || !is_ng_template(tag_name))
    {
        for a in &template.template_attrs {
            let name = match a {
                R3TemplateAttr::Bound(a) => a.name.as_str(),
                R3TemplateAttr::Text(a) => a.name.as_str(),
            };
            attributes_map.insert(name.to_string(), String::new());
        }
    } else {
        for a in el_or_tpl.attributes() {
            if !is_i18n_attribute(&a.name) {
                attributes_map.insert(a.name.to_string(), a.value.to_string());
            }
        }

        for i in el_or_tpl.inputs() {
            if i.binding_type == BindingType::Property || i.binding_type == BindingType::TwoWay {
                attributes_map.insert(i.name.to_string(), String::new());
            }
        }
        for o in el_or_tpl.outputs() {
            attributes_map.insert(o.name.to_string(), String::new());
        }
    }

    js_object_key_order(attributes_map)
}

// tags.ts:27 splitNsName (with `fatal` at its default, `true`)
fn split_ns_name(element_name: &str) -> (Option<&str>, &str) {
    if !element_name.starts_with(':') {
        return (None, element_name);
    }

    let Some(colon_index) = element_name[1..].find(':').map(|index| index + 1) else {
        panic!("Unsupported format \"{element_name}\" expecting \":namespace:name\"");
    };

    (Some(&element_name[1..colon_index]), &element_name[colon_index + 1..])
}

// tags.ts:56 isNgTemplate
fn is_ng_template(tag_name: &str) -> bool {
    split_ns_name(tag_name).1 == "ng-template"
}

/// Whether `c` is whitespace for JavaScript's `String.prototype.trim` and the regular expression `\s`
/// (WhiteSpace and LineTerminator).
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// JavaScript's `s.trim()`.
fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// JavaScript's `s.split(/\s+/)`, for a trimmed `s` (`''.split(/\s+/)` is `['']`).
fn js_split_whitespace(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return vec![""];
    }
    s.split(is_js_whitespace).filter(|piece| !piece.is_empty()).collect()
}

/// The entries of a string map.
type Entries<V> = Vec<(String, V)>;

/// A JavaScript object used as a string map enumerates (`Object.keys`, `Object.getOwnPropertyNames`) its array-index
/// keys first, in ascending numeric order, then its other keys in insertion order.
fn js_object_key_order<V>(map: Map<String, V>) -> Map<String, V> {
    if !map.keys().any(|key| as_array_index(key).is_some()) {
        return map;
    }
    let (mut indices, others): (Entries<V>, Entries<V>) =
        map.into_iter().partition(|(key, _)| as_array_index(key).is_some());
    indices.sort_by_key(|(key, _)| as_array_index(key));
    indices.into_iter().chain(others).collect()
}

/// The array index a property key is, if it is one: the canonical decimal form of an integer below 2^32 - 1.
fn as_array_index(key: &str) -> Option<u32> {
    let index: u32 = key.parse().ok()?;
    (index != u32::MAX && index.to_string() == key).then_some(index)
}
