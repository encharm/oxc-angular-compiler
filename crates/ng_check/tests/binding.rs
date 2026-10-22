//! `packages/compiler/test/render3/view/binding_spec.ts`, over the fork's parser (`parseTemplate` is its parse
//! sequence, as `oxc_angular_compiler`'s `component/transform.rs` parses a template).
//!
//! Until `directive_matching.rs` is merged (it is a stub on this branch), the tests that construct a selector matcher
//! panic in the stub and are ignored.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use indexmap::IndexMap;
use ng_check::angular::directive_matching::{CssSelector, SelectorMatcher, SelectorlessMatcher};
use ng_check::angular::property_mapping::{ClassPropertyMapping, InputOrOutput};
use ng_check::angular::t2_api::{
    Binding, BindingConsumer, BoundTarget, DirectiveMeta, DirectiveOwner, ForeignComponentMeta, InputOrOutputKind,
    LegacyAnimationTriggerNames, MatchSource, ReferenceDeferredTrigger, ReferenceTarget, ScopedNode, Target,
    TemplateEntity,
};
use ng_check::angular::t2_binder::{
    DirectiveMatcher, DirectiveSpread, MatchingDirectivesAndPipes, R3BoundTarget, R3TargetBinder,
    RegularAndDeferCandidates, find_matching_directives_and_pipes,
};
use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipe};
use oxc_angular_compiler::ast::r3::{
    R3Component, R3DeferredBlock, R3Element, R3IfBlock, R3Node, R3Template,
};
use oxc_angular_compiler::parser::ParseTemplateOptions;
use oxc_angular_compiler::parser::html::{HtmlParser, remove_whitespaces};
use oxc_angular_compiler::transform::HtmlToR3Transform;
use oxc_angular_compiler::transform::html_to_r3::TransformOptions as R3TransformOptions;
use rustc_hash::FxBuildHasher;

/// The test's `DirectiveMeta` object.
#[derive(Debug, Clone)]
struct TestDirectiveMeta {
    name: String,
    ref_key: String,
    export_as: Option<Vec<String>>,
    inputs: ClassPropertyMapping<InputOrOutput>,
    outputs: ClassPropertyMapping<InputOrOutput>,
    is_component: bool,
    is_structural: bool,
    selector: Option<String>,
    match_source: MatchSource,
}

impl DirectiveMeta for TestDirectiveMeta {
    type Input = InputOrOutput;

    fn name(&self) -> &str {
        &self.name
    }
    fn ref_key(&self) -> &str {
        &self.ref_key
    }
    fn selector(&self) -> Option<&str> {
        self.selector.as_deref()
    }
    fn is_component(&self) -> bool {
        self.is_component
    }
    fn inputs(&self) -> &ClassPropertyMapping<InputOrOutput> {
        &self.inputs
    }
    fn outputs(&self) -> &ClassPropertyMapping<InputOrOutput> {
        &self.outputs
    }
    fn export_as(&self) -> Option<&[String]> {
        self.export_as.as_deref()
    }
    fn is_structural(&self) -> bool {
        self.is_structural
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
        self.match_source
    }
}

impl DirectiveSpread for TestDirectiveMeta {
    fn with_mappings(
        &self,
        inputs: ClassPropertyMapping<InputOrOutput>,
        outputs: ClassPropertyMapping<InputOrOutput>,
    ) -> Self {
        Self { inputs, outputs, ..self.clone() }
    }
}

type Dir = Rc<TestDirectiveMeta>;

static KEY_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct DirectiveConfig<'c> {
    name: &'c str,
    selector: Option<&'c str>,
    inputs: &'c [(&'c str, &'c str)],
    outputs: &'c [(&'c str, &'c str)],
    export_as: Option<&'c [&'c str]>,
    is_component: bool,
    is_structural: bool,
    match_source: Option<MatchSource>,
}

fn mapping(entries: &[(&str, &str)]) -> ClassPropertyMapping<InputOrOutput> {
    ClassPropertyMapping::from_binding_names(entries.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())))
}

#[expect(clippy::needless_pass_by_value, reason = "the TypeScript helper's config object")]
fn make_directive_meta(config: DirectiveConfig<'_>) -> Dir {
    Rc::new(TestDirectiveMeta {
        name: config.name.to_string(),
        ref_key: format!("{}#{}", config.name, KEY_COUNTER.fetch_add(1, Ordering::Relaxed)),
        export_as: config.export_as.map(|e| e.iter().map(ToString::to_string).collect()),
        inputs: mapping(config.inputs),
        outputs: mapping(config.outputs),
        is_component: config.is_component,
        is_structural: config.is_structural,
        selector: config.selector.map(str::to_string),
        match_source: config.match_source.unwrap_or(MatchSource::Selector),
    })
}

fn make_selector_matcher() -> SelectorMatcher<Vec<Dir>> {
    let mut matcher = SelectorMatcher::<Vec<Dir>>::new();
    matcher.add_selectables(
        &CssSelector::parse("[ngFor][ngForOf]"),
        vec![make_directive_meta(DirectiveConfig {
            name: "NgFor",
            inputs: &[("ngForOf", "ngForOf")],
            selector: Some("[ngFor][ngForOf]"),
            is_structural: true,
            ..Default::default()
        })],
    );
    matcher.add_selectables(
        &CssSelector::parse("[dir]"),
        vec![make_directive_meta(DirectiveConfig {
            name: "Dir",
            export_as: Some(&["dir"]),
            selector: Some("[dir]"),
            ..Default::default()
        })],
    );
    matcher.add_selectables(
        &CssSelector::parse("[hasOutput]"),
        vec![make_directive_meta(DirectiveConfig {
            name: "HasOutput",
            outputs: &[("outputBinding", "outputBinding")],
            selector: Some("[hasOutput]"),
            ..Default::default()
        })],
    );
    matcher.add_selectables(
        &CssSelector::parse("[hasInput]"),
        vec![make_directive_meta(DirectiveConfig {
            name: "HasInput",
            inputs: &[("inputBinding", "inputBinding")],
            selector: Some("[hasInput]"),
            ..Default::default()
        })],
    );
    matcher.add_selectables(
        &CssSelector::parse("[sameSelectorAsInput]"),
        vec![make_directive_meta(DirectiveConfig {
            name: "SameSelectorAsInput",
            inputs: &[("sameSelectorAsInput", "sameSelectorAsInput")],
            selector: Some("[sameSelectorAsInput]"),
            ..Default::default()
        })],
    );
    matcher.add_selectables(
        &CssSelector::parse("comp"),
        vec![make_directive_meta(DirectiveConfig {
            name: "Comp",
            is_component: true,
            selector: Some("comp"),
            ..Default::default()
        })],
    );

    let simple_directives = ["a", "b", "c", "d", "e", "f"];
    let defer_block_directives = ["loading", "error", "placeholder"];
    for dir in simple_directives.iter().chain(defer_block_directives.iter()) {
        let name = format!("{}{}", dir[..1].to_uppercase(), dir[1..].to_lowercase());
        let selector = format!("[{dir}]");
        matcher.add_selectables(
            &CssSelector::parse(&selector),
            vec![make_directive_meta(DirectiveConfig {
                name: &format!("Dir{name}"),
                is_structural: true,
                selector: Some(&selector),
                ..Default::default()
            })],
        );
    }

    matcher
}

/// `parseTemplate(template, '', options)`: its nodes.
fn parse_template<'a>(allocator: &'a Allocator, template: &str, enable_selectorless: bool) -> &'a [R3Node<'a>] {
    let template = allocator.alloc_str(template);
    let options = ParseTemplateOptions {
        enable_block_syntax: true,
        enable_let_syntax: true,
        enable_selectorless,
        tokenize_expansion_forms: true,
        ..Default::default()
    };
    let html_result = HtmlParser::with_options(allocator, template, "", &options).parse();
    let nodes = allocator.alloc(remove_whitespaces(allocator, &html_result.nodes, true));
    let r3_result = HtmlToR3Transform::new(allocator, template, R3TransformOptions::default()).transform(nodes);
    allocator.alloc(r3_result.nodes)
}

fn bind<'a>(nodes: &'a [R3Node<'a>], matcher: Option<DirectiveMatcher<Dir>>) -> R3BoundTarget<'a, Dir> {
    R3TargetBinder::new(matcher).bind(Target { template: Some(nodes), host: None })
}

#[expect(clippy::unnecessary_wraps, reason = "the binder's matcher argument")]
fn selector(matcher: SelectorMatcher<Vec<Dir>>) -> Option<DirectiveMatcher<Dir>> {
    Some(DirectiveMatcher::Selector(matcher))
}

fn element<'a>(node: &'a R3Node<'a>) -> &'a R3Element<'a> {
    match node {
        R3Node::Element(n) => n,
        _ => panic!("not an Element"),
    }
}

fn template_node<'a>(node: &'a R3Node<'a>) -> &'a R3Template<'a> {
    match node {
        R3Node::Template(n) => n,
        _ => panic!("not a Template"),
    }
}

fn component<'a>(node: &'a R3Node<'a>) -> &'a R3Component<'a> {
    match node {
        R3Node::Component(n) => n,
        _ => panic!("not a Component"),
    }
}

fn if_block<'a>(node: &'a R3Node<'a>) -> &'a R3IfBlock<'a> {
    match node {
        R3Node::IfBlock(n) => n,
        _ => panic!("not an IfBlock"),
    }
}

fn deferred_block<'a>(node: &'a R3Node<'a>) -> &'a R3DeferredBlock<'a> {
    match node {
        R3Node::DeferredBlock(n) => n,
        _ => panic!("not a DeferredBlock"),
    }
}

/// `(node as BoundText).value`, through its `Interpolation`: `expressions[0]`.
fn first_interpolated<'a>(node: &'a R3Node<'a>) -> &'a AngularExpression<'a> {
    let R3Node::BoundText(text) = node else { panic!("not a BoundText") };
    let AngularExpression::Interpolation(interpolation) = &text.value else { panic!("not an Interpolation") };
    &interpolation.expressions[0]
}

fn entity_name<'a>(entity: &TemplateEntity<'a>) -> &'a str {
    match *entity {
        TemplateEntity::Reference(n) => n.name.as_str(),
        TemplateEntity::Variable(n) => n.name.as_str(),
        TemplateEntity::LetDeclaration(n) => n.name.as_str(),
    }
}

fn names(dirs: &[Dir]) -> Vec<&str> {
    dirs.iter().map(|d| d.name.as_str()).collect()
}

fn consumer_name(consumer: Option<BindingConsumer<'_, Dir>>) -> Option<String> {
    match consumer {
        Some(BindingConsumer::Directive(d)) => Some(d.name.clone()),
        _ => None,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

/// test/render3/view/util.ts:89 findExpression
fn find_expression<'a>(tmpl: &'a [R3Node<'a>], expr: &str) -> Option<&'a AngularExpression<'a>> {
    tmpl.iter().find_map(|node| find_expression_in_node(node, expr))
}

/// test/render3/view/util.ts:106 findExpressionInNode
fn find_expression_in_node<'a>(node: &'a R3Node<'a>, expr: &str) -> Option<&'a AngularExpression<'a>> {
    let in_bindings = |inputs: &'a [oxc_angular_compiler::ast::r3::R3BoundAttribute<'a>],
                       outputs: &'a [oxc_angular_compiler::ast::r3::R3BoundEvent<'a>]| {
        inputs
            .iter()
            .map(|i| &i.value)
            .chain(outputs.iter().map(|o| &o.handler))
            .find(|value| to_string_expression(value) == expr)
    };
    match node {
        R3Node::Element(n) => in_bindings(&n.inputs, &n.outputs).or_else(|| find_expression(&n.children, expr)),
        R3Node::Template(n) => in_bindings(&n.inputs, &n.outputs).or_else(|| find_expression(&n.children, expr)),
        R3Node::Component(n) => in_bindings(&n.inputs, &n.outputs).or_else(|| find_expression(&n.children, expr)),
        R3Node::Directive(n) => in_bindings(&n.inputs, &n.outputs),
        R3Node::BoundText(n) => (to_string_expression(&n.value) == expr).then_some(&n.value),
        _ => None,
    }
}

/// test/render3/view/util.ts:121 toStringExpression
fn to_string_expression(expr: &AngularExpression<'_>) -> String {
    match expr {
        AngularExpression::PropertyRead(read) => match &read.receiver {
            AngularExpression::ImplicitReceiver(_) | AngularExpression::ThisReceiver(_) => read.name.to_string(),
            receiver => format!("{}.{}", to_string_expression(receiver), read.name),
        },
        AngularExpression::ImplicitReceiver(_) => String::new(),
        AngularExpression::Interpolation(interpolation) => {
            let mut str = "{{".to_string();
            for (i, expression) in interpolation.expressions.iter().enumerate() {
                str += &interpolation.strings[i];
                str += &to_string_expression(expression);
            }
            str += &interpolation.strings[interpolation.strings.len() - 1];
            str + "}}"
        }
        _ => panic!("Unsupported type"),
    }
}

/// `block.triggers.viewport!`
fn viewport_trigger<'a>(block: &'a R3DeferredBlock<'a>) -> ReferenceDeferredTrigger<'a> {
    ReferenceDeferredTrigger::Viewport(block.triggers.viewport.as_ref().expect("a viewport trigger"))
}

/// The deferred trigger target of the first defer block's viewport trigger, by name.
fn viewport_trigger_target_name(template: &str) -> Option<String> {
    let allocator = Allocator::default();
    let nodes = parse_template(&allocator, template, false);
    let bound = bind(nodes, selector(make_selector_matcher()));
    let block = bound.get_defer_blocks()[0];
    let trigger_el = bound.get_deferred_trigger_target(block, viewport_trigger(block));
    trigger_el.map(|el| el.name.to_string())
}

mod find_matching_directives_and_pipes_ {
    use super::*;

    #[test]
    fn should_match_directives_and_detect_pipes_in_eager_and_deferrable_parts_of_a_template() {
        let template = "
      <div [title]=\"abc | uppercase\"></div>
      @defer {
        <my-defer-cmp [label]=\"abc | lowercase\" />
      } @placeholder {}
    ";
        let directive_selectors = strings(&["[title]", "my-defer-cmp", "not-matching"]);
        let result = find_matching_directives_and_pipes(template, &directive_selectors);
        assert_eq!(
            result,
            MatchingDirectivesAndPipes {
                directives: RegularAndDeferCandidates {
                    regular: strings(&["[title]"]),
                    defer_candidates: strings(&["my-defer-cmp"]),
                },
                pipes: RegularAndDeferCandidates {
                    regular: strings(&["uppercase"]),
                    defer_candidates: strings(&["lowercase"]),
                },
            }
        );
    }

    #[test]
    fn should_return_empty_directive_list_if_no_selectors_are_provided() {
        let template = "
        <div [title]=\"abc | uppercase\"></div>
        @defer {
          <my-defer-cmp [label]=\"abc | lowercase\" />
        } @placeholder {}
      ";
        let result = find_matching_directives_and_pipes(template, &[]);
        assert_eq!(
            result,
            MatchingDirectivesAndPipes {
                directives: RegularAndDeferCandidates { regular: vec![], defer_candidates: vec![] },
                // Expect pipes to be present still.
                pipes: RegularAndDeferCandidates {
                    regular: strings(&["uppercase"]),
                    defer_candidates: strings(&["lowercase"]),
                },
            }
        );
    }

    #[test]
    fn should_return_a_directive_and_a_pipe_only_once() {
        let template = "
        <my-defer-cmp [label]=\"abc | lowercase\" [title]=\"abc | uppercase\" />
        @defer {
          <my-defer-cmp [label]=\"abc | lowercase\" [title]=\"abc | uppercase\" />
        } @placeholder {}
      ";
        let directive_selectors = strings(&["[title]", "my-defer-cmp", "not-matching"]);
        let result = find_matching_directives_and_pipes(template, &directive_selectors);
        assert_eq!(
            result,
            MatchingDirectivesAndPipes {
                directives: RegularAndDeferCandidates {
                    regular: strings(&["my-defer-cmp", "[title]"]),
                    // All directives/components are used eagerly.
                    defer_candidates: vec![],
                },
                pipes: RegularAndDeferCandidates {
                    regular: strings(&["lowercase", "uppercase"]),
                    // All pipes are used eagerly.
                    defer_candidates: vec![],
                },
            }
        );
    }

    #[test]
    fn should_handle_directives_on_elements_with_local_refs() {
        let template = "
        <input [(ngModel)]=\"name\" #ctrl=\"ngModel\" required />
        @defer {
          <my-defer-cmp [label]=\"abc | lowercase\" [title]=\"abc | uppercase\" />
          <input [(ngModel)]=\"name\" #ctrl=\"ngModel\" required />
        } @placeholder {}
      ";
        let directive_selectors = strings(&[
            "[ngModel]:not([formControlName]):not([formControl])",
            "[title]",
            "my-defer-cmp",
            "not-matching",
        ]);
        let result = find_matching_directives_and_pipes(template, &directive_selectors);
        assert_eq!(
            result,
            MatchingDirectivesAndPipes {
                directives: RegularAndDeferCandidates {
                    // `ngModel` is used both eagerly and in a defer block, thus it's located
                    // in the "regular" (eager) bucket.
                    regular: strings(&["[ngModel]:not([formControlName]):not([formControl])"]),
                    defer_candidates: strings(&["my-defer-cmp", "[title]"]),
                },
                pipes: RegularAndDeferCandidates {
                    regular: vec![],
                    defer_candidates: strings(&["lowercase", "uppercase"]),
                },
            }
        );
    }
}

mod t2_binding {
    use super::*;

    #[test]
    fn should_bind_a_simple_template() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<div *ngFor=\"let item of items\">{{item.name}}</div>", false);
        let res = bind(nodes, selector(SelectorMatcher::new()));

        let Some(AngularExpression::Interpolation(interpolation)) = find_expression(nodes, "{{item.name}}") else {
            panic!("no interpolation")
        };
        let AngularExpression::PropertyRead(item_binding) = &interpolation.expressions[0] else { panic!() };
        let item = &item_binding.receiver;
        let Some(TemplateEntity::Variable(item_target)) = res.get_expression_target(item) else {
            panic!("Expected item to point to a Variable");
        };
        assert_eq!(item_target.value.as_str(), "$implicit");
        let item_template = res.get_definition_node_of_symbol(TemplateEntity::Variable(item_target));
        assert!(item_template.is_some());
        assert_eq!(res.get_nesting_level(item_template.unwrap()), 1);
    }

    #[test]
    fn should_match_directives_when_binding_a_simple_template() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<div *ngFor=\"let item of items\">{{item.name}}</div>", false);
        let res = bind(nodes, selector(make_selector_matcher()));
        let tmpl = template_node(&nodes[0]);
        let directives = res.get_directives_of_node(DirectiveOwner::Template(tmpl)).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].name, "NgFor");
    }

    #[test]
    fn should_match_directives_on_namespaced_elements() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<svg><text dir>SVG</text></svg>", false);
        let mut matcher = SelectorMatcher::<Vec<Dir>>::new();
        matcher.add_selectables(
            &CssSelector::parse("text[dir]"),
            vec![make_directive_meta(DirectiveConfig {
                name: "Dir",
                selector: Some("text[dir]"),
                ..Default::default()
            })],
        );
        let res = bind(nodes, selector(matcher));
        let svg_node = element(&nodes[0]);
        let text_node = element(&svg_node.children[0]);
        let directives = res.get_directives_of_node(DirectiveOwner::Element(text_node)).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].name, "Dir");
    }

    #[test]
    fn should_match_directives_on_ng_template_nested_in_namespaced_elements() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<svg><ng-template [hasInput]=\"true\"></ng-template></svg>", false);
        let res = bind(nodes, selector(make_selector_matcher()));
        let svg_node = element(&nodes[0]);
        let tmpl_node = template_node(&svg_node.children[0]);
        let directives = res.get_directives_of_node(DirectiveOwner::Template(tmpl_node)).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].name, "HasInput");
    }

    #[test]
    fn should_not_match_directives_intended_for_an_element_on_a_microsyntax_template() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<div *ngFor=\"let item of items\" dir></div>", false);
        let res = bind(nodes, selector(make_selector_matcher()));
        let tmpl = template_node(&nodes[0]);
        let tmpl_directives = res.get_directives_of_node(DirectiveOwner::Template(tmpl)).unwrap();
        assert_eq!(tmpl_directives.len(), 1);
        assert_eq!(tmpl_directives[0].name, "NgFor");
        let el_directives = res.get_directives_of_node(DirectiveOwner::Element(element(&tmpl.children[0]))).unwrap();
        assert_eq!(el_directives.len(), 1);
        assert_eq!(el_directives[0].name, "Dir");
    }

    #[test]
    fn should_get_let_declarations_when_resolving_entities_at_the_root() {
        let allocator = Allocator::default();
        let nodes = parse_template(
            &allocator,
            "
        @let one = 1;
        @let two = 2;
        @let sum = one + two;
      ",
            false,
        );
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let entities = res.get_entities_in_scope(None);

        assert_eq!(entities.iter().map(entity_name).collect::<Vec<_>>(), ["one", "two", "sum"]);
    }

    #[test]
    fn should_scope_let_declarations_to_their_current_view() {
        let allocator = Allocator::default();
        let nodes = parse_template(
            &allocator,
            "
        @let one = 1;

        @if (true) {
          @let two = 2;
        }

        @if (true) {
          @let three = 3;
        }
      ",
            false,
        );
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let root_entities = res.get_entities_in_scope(None);
        let first_branch_entities =
            res.get_entities_in_scope(Some(ScopedNode::IfBlockBranch(&if_block(&nodes[1]).branches[0])));
        let second_branch_entities =
            res.get_entities_in_scope(Some(ScopedNode::IfBlockBranch(&if_block(&nodes[2]).branches[0])));

        assert_eq!(root_entities.iter().map(entity_name).collect::<Vec<_>>(), ["one"]);
        assert_eq!(first_branch_entities.iter().map(entity_name).collect::<Vec<_>>(), ["one", "two"]);
        assert_eq!(second_branch_entities.iter().map(entity_name).collect::<Vec<_>>(), ["one", "three"]);
    }

    #[test]
    fn should_resolve_expressions_to_a_let_declaration() {
        let allocator = Allocator::default();
        let nodes = parse_template(
            &allocator,
            "
        @let value = 1;
        {{value}}
      ",
            false,
        );
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let property_read = first_interpolated(&nodes[1]);
        let target = res.get_expression_target(property_read);

        let Some(TemplateEntity::LetDeclaration(target)) = target else { panic!("not a LetDeclaration") };
        assert_eq!(target.name.as_str(), "value");
    }

    #[test]
    fn should_not_resolve_a_this_access_to_a_template_reference() {
        let allocator = Allocator::default();
        let nodes = parse_template(
            &allocator,
            "
        <input #value>
        {{this.value}}
      ",
            false,
        );
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let property_read = first_interpolated(&nodes[1]);
        let target = res.get_expression_target(property_read);

        assert!(target.is_none());
    }

    #[test]
    fn should_not_resolve_a_this_access_to_a_template_variable() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<ng-template let-value>{{this.value}}</ng-template>", false);
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let template_node = template_node(&nodes[0]);
        let property_read = first_interpolated(&template_node.children[0]);
        let target = res.get_expression_target(property_read);

        assert!(target.is_none());
    }

    #[test]
    fn should_not_resolve_a_this_access_to_a_let_declaration() {
        let allocator = Allocator::default();
        let nodes = parse_template(
            &allocator,
            "
        @let value = 1;
        {{this.value}}
      ",
            false,
        );
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let property_read = first_interpolated(&nodes[1]);
        let target = res.get_expression_target(property_read);

        assert!(target.is_none());
    }

    #[test]
    fn should_resolve_the_definition_node_of_let_declarations() {
        let allocator = Allocator::default();
        let nodes = parse_template(
            &allocator,
            "
        @if (true) {
          @let one = 1;
        }

        @if (true) {
          @let two = 2;
        }
      ",
            false,
        );
        let res = bind(nodes, selector(SelectorMatcher::new()));
        let first_branch = &if_block(&nodes[0]).branches[0];
        let R3Node::LetDeclaration(first_let) = &first_branch.children[0] else { panic!() };
        let second_branch = &if_block(&nodes[1]).branches[0];
        let R3Node::LetDeclaration(second_let) = &second_branch.children[0] else { panic!() };

        assert_eq!(
            res.get_definition_node_of_symbol(TemplateEntity::LetDeclaration(first_let)),
            Some(ScopedNode::IfBlockBranch(first_branch))
        );
        assert_eq!(
            res.get_definition_node_of_symbol(TemplateEntity::LetDeclaration(second_let)),
            Some(ScopedNode::IfBlockBranch(second_branch))
        );
    }

    #[test]
    fn should_resolve_an_element_reference_without_a_directive_matcher() {
        let allocator = Allocator::default();
        let nodes = parse_template(&allocator, "<div #foo></div>", false);
        let res = bind(nodes, None);
        let node = element(&nodes[0]);
        let reference = &node.references[0];
        let Some(ReferenceTarget::Element(result)) = res.get_reference_target(reference) else {
            panic!("not an Element")
        };
        assert_eq!(result.name.as_str(), "div");
    }

    mod matching_inputs_to_consuming_directives {
        use super::*;

        #[test]
        fn should_work_for_bound_attributes() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div hasInput [inputBinding]=\"myValue\"></div>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = element(&nodes[0]);
            let attr = &el.inputs[0];
            let consumer = res.get_consumer_of_binding(Binding::BoundAttribute(attr));
            assert_eq!(consumer_name(consumer).as_deref(), Some("HasInput"));
        }

        #[test]
        fn should_work_for_text_attributes_on_elements() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div hasInput inputBinding=\"text\"></div>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = element(&nodes[0]);
            let attr = &el.attributes[1];
            let consumer = res.get_consumer_of_binding(Binding::TextAttribute(attr));
            assert_eq!(consumer_name(consumer).as_deref(), Some("HasInput"));
        }

        #[test]
        fn should_work_for_text_attributes_on_templates() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<ng-template hasInput inputBinding=\"text\"></ng-template>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = template_node(&nodes[0]);
            let attr = &el.attributes[1];
            let consumer = res.get_consumer_of_binding(Binding::TextAttribute(attr));
            assert_eq!(consumer_name(consumer).as_deref(), Some("HasInput"));
        }

        #[test]
        fn should_not_match_directives_on_attribute_bindings_with_the_same_name_as_an_input() {
            let allocator = Allocator::default();
            let nodes =
                parse_template(&allocator, "<ng-template [attr.sameSelectorAsInput]=\"123\"></ng-template>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = template_node(&nodes[0]);
            let input = &el.inputs[0];
            let consumer = res.get_consumer_of_binding(Binding::BoundAttribute(input));
            assert!(matches!(consumer, Some(BindingConsumer::Template(t)) if std::ptr::eq(t, el)));
        }

        #[test]
        fn should_bind_to_the_encompassing_node_when_no_directive_input_is_matched() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<span dir></span>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = element(&nodes[0]);
            let attr = &el.attributes[0];
            let consumer = res.get_consumer_of_binding(Binding::TextAttribute(attr));
            assert!(matches!(consumer, Some(BindingConsumer::Element(e)) if std::ptr::eq(e, el)));
        }
    }

    mod matching_outputs_to_consuming_directives {
        use super::*;

        #[test]
        fn should_work_for_bound_events() {
            let allocator = Allocator::default();
            let nodes =
                parse_template(&allocator, "<div hasOutput (outputBinding)=\"myHandler($event)\"></div>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = element(&nodes[0]);
            let attr = &el.outputs[0];
            let consumer = res.get_consumer_of_binding(Binding::BoundEvent(attr));
            assert_eq!(consumer_name(consumer).as_deref(), Some("HasOutput"));
        }

        #[test]
        fn should_bind_to_the_encompassing_node_when_no_directive_output_is_matched() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<span dir (fakeOutput)=\"myHandler($event)\"></span>", false);
            let res = bind(nodes, selector(make_selector_matcher()));
            let el = element(&nodes[0]);
            let attr = &el.outputs[0];
            let consumer = res.get_consumer_of_binding(Binding::BoundEvent(attr));
            assert!(matches!(consumer, Some(BindingConsumer::Element(e)) if std::ptr::eq(e, el)));
        }
    }

    mod extracting_defer_blocks_info {
        use super::*;

        #[test]
        fn should_extract_top_level_defer_blocks() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
            @defer {<cmp-a />}
            @defer {<cmp-b />}
            <cmp-c />
          ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));
            let defer_blocks = bound.get_defer_blocks();
            assert_eq!(defer_blocks.len(), 2);
        }

        #[test]
        fn should_extract_nested_defer_blocks_and_associated_pipes() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
            @defer {
              {{ name | pipeA }}
              @defer {
                {{ name | pipeB }}
              }
            } @loading {
              @defer {
                {{ name | pipeC }}
              }
              {{ name | loading }}
            } @placeholder {
              @defer {
                {{ name | pipeD }}
              }
              {{ name | placeholder }}
            } @error {
              @defer {
                {{ name | pipeE }}
              }
              {{ name | error }}
            }
            {{ name | pipeF }}
          ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));
            let defer_blocks = bound.get_defer_blocks();

            assert_eq!(defer_blocks.len(), 5);

            // Record all pipes used within :placeholder, :loading and :error sub-blocks,
            // also record pipes used outside of any defer blocks.
            assert_eq!(bound.get_eagerly_used_pipes(), ["placeholder", "loading", "error", "pipeF"]);

            // Record *all* pipes from the template, including the ones from defer blocks.
            assert_eq!(
                bound.get_used_pipes(),
                ["pipeA", "pipeB", "pipeD", "placeholder", "pipeC", "loading", "pipeE", "error", "pipeF"]
            );
        }

        #[test]
        fn should_identify_pipes_used_after_a_nested_defer_block_as_being_lazy() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
          @defer {
            {{ name | pipeA }}
            @defer {
              {{ name | pipeB }}
            }
            {{ name | pipeC }}
          }
          ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));

            assert_eq!(bound.get_used_pipes(), ["pipeA", "pipeB", "pipeC"]);
            assert_eq!(bound.get_eagerly_used_pipes(), Vec::<String>::new());
        }

        #[test]
        fn should_extract_nested_defer_blocks_and_associated_directives() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
            @defer {
              <img *a />
              @defer {
                <img *b />
              }
            } @loading {
              @defer {
                <img *c />
              }
              <img *loading />
            } @placeholder {
              @defer {
                <img *d />
              }
              <img *placeholder />
            } @error {
              @defer {
                <img *e />
              }
              <img *error />
            }
            <img *f />
          ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));
            let defer_blocks = bound.get_defer_blocks();

            assert_eq!(defer_blocks.len(), 5);

            // Record all directives used within placeholder, loading and error sub-blocks,
            // also record directives used outside of any defer blocks.
            let eager_dirs = bound.get_eagerly_used_directives();
            assert_eq!(eager_dirs.len(), 4);
            assert_eq!(names(&eager_dirs), ["DirPlaceholder", "DirLoading", "DirError", "DirF"]);

            // Record *all* directives from the template, including the ones from defer blocks.
            let all_dirs = bound.get_used_directives();
            assert_eq!(all_dirs.len(), 9);
            assert_eq!(
                names(&all_dirs),
                ["DirA", "DirB", "DirD", "DirPlaceholder", "DirC", "DirLoading", "DirE", "DirError", "DirF"]
            );
        }

        #[test]
        fn should_identify_directives_used_after_a_nested_defer_block_as_being_lazy() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
          @defer {
            <img *a />
            @defer {<img *b />}
            <img *c />
          }
          ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));
            let all_dirs = bound.get_used_directives();
            let eager_dirs = bound.get_eagerly_used_directives();

            assert_eq!(names(&all_dirs), ["DirA", "DirB", "DirC"]);
            assert!(eager_dirs.is_empty());
        }

        /// `pipes` of the test below: the pipes of the template's bound texts, as `RecursiveVisitor` and
        /// `RecursiveAstVisitor` find them.
        fn collect_pipes<'a>(nodes: &'a [R3Node<'a>], pipes: &mut Vec<&'a BindingPipe<'a>>) {
            fn visit_ast<'a>(ast: &'a AngularExpression<'a>, pipes: &mut Vec<&'a BindingPipe<'a>>) {
                match ast {
                    AngularExpression::BindingPipe(pipe) => {
                        pipes.push(pipe);
                        visit_ast(&pipe.exp, pipes);
                        for arg in &pipe.args {
                            visit_ast(arg, pipes);
                        }
                    }
                    AngularExpression::Interpolation(interpolation) => {
                        for expression in &interpolation.expressions {
                            visit_ast(expression, pipes);
                        }
                    }
                    _ => {}
                }
            }
            for node in nodes {
                match node {
                    R3Node::BoundText(text) => visit_ast(&text.value, pipes),
                    R3Node::Element(n) => collect_pipes(&n.children, pipes),
                    R3Node::DeferredBlock(n) => {
                        collect_pipes(&n.children, pipes);
                        if let Some(b) = &n.placeholder {
                            collect_pipes(&b.children, pipes);
                        }
                        if let Some(b) = &n.loading {
                            collect_pipes(&b.children, pipes);
                        }
                        if let Some(b) = &n.error {
                            collect_pipes(&b.children, pipes);
                        }
                    }
                    _ => {}
                }
            }
        }

        #[test]
        fn should_track_enclosing_defer_blocks_for_pipes() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
          {{ 'outside' | pipeA }}
          @defer (name blockA) {
            {{ 'in A' | pipeB }}
            @defer (name blockB) {
              {{ 'in B' | pipeC }}
            }
          }
        ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));
            let defer_blocks = bound.get_defer_blocks();
            assert_eq!(defer_blocks.len(), 2);

            let mut pipes = Vec::new();
            collect_pipes(nodes, &mut pipes);

            let pipe_a = pipes.iter().find(|p| p.name.as_str() == "pipeA").unwrap();
            let pipe_b = pipes.iter().find(|p| p.name.as_str() == "pipeB").unwrap();
            let pipe_c = pipes.iter().find(|p| p.name.as_str() == "pipeC").unwrap();

            let ptrs = |blocks: Vec<&R3DeferredBlock<'_>>| {
                blocks.into_iter().map(|b| std::ptr::from_ref(b) as usize).collect::<Vec<_>>()
            };
            assert!(bound.get_defer_blocks_of_pipe(pipe_a).is_empty());
            assert_eq!(ptrs(bound.get_defer_blocks_of_pipe(pipe_b)), ptrs(vec![defer_blocks[0]]));
            assert_eq!(ptrs(bound.get_defer_blocks_of_pipe(pipe_c)), ptrs(vec![defer_blocks[0], defer_blocks[1]]));
        }

        #[test]
        fn should_track_enclosing_defer_blocks_for_element_and_template_nodes() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
          <div id=\"outside\"></div>
          @defer (name blockA) {
            <div id=\"insideA\"></div>
            <div *a id=\"templateInsideA\"></div>
            @defer (name blockB) {
              <div id=\"insideB\"></div>
            }
          }
        ",
                false,
            );
            let bound = bind(nodes, selector(make_selector_matcher()));
            let defer_blocks = bound.get_defer_blocks();
            assert_eq!(defer_blocks.len(), 2);

            let div_outside = element(&nodes[0]);
            let block_a = deferred_block(&nodes[1]);
            let div_inside_a = element(&block_a.children[0]);
            let template_inside_a = template_node(&block_a.children[1]);
            let block_b = deferred_block(&block_a.children[2]);
            let div_inside_b = element(&block_b.children[0]);

            let ptrs = |blocks: Vec<&R3DeferredBlock<'_>>| {
                blocks.into_iter().map(|b| std::ptr::from_ref(b) as usize).collect::<Vec<_>>()
            };
            assert!(bound.get_defer_blocks_of_node(DirectiveOwner::Element(div_outside)).is_empty());
            assert_eq!(
                ptrs(bound.get_defer_blocks_of_node(DirectiveOwner::Element(div_inside_a))),
                ptrs(vec![defer_blocks[0]])
            );
            assert_eq!(
                ptrs(bound.get_defer_blocks_of_node(DirectiveOwner::Template(template_inside_a))),
                ptrs(vec![defer_blocks[0]])
            );
            assert_eq!(
                ptrs(bound.get_defer_blocks_of_node(DirectiveOwner::Element(div_inside_b))),
                ptrs(vec![defer_blocks[0], defer_blocks[1]])
            );
        }

        #[test]
        fn should_identify_a_trigger_element_that_is_a_parent_of_the_deferred_block() {
            let trigger_el = viewport_trigger_target_name(
                "
          <div #trigger>
            @defer (on viewport(trigger)) {}
          </div>
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("div"));
        }

        #[test]
        fn should_identify_a_trigger_element_outside_of_the_deferred_block() {
            let trigger_el = viewport_trigger_target_name(
                "
            <div>
              @defer (on viewport(trigger)) {}
            </div>

            <div>
              <div>
                <button #trigger></button>
              </div>
            </div>
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("button"));
        }

        #[test]
        fn should_identify_a_trigger_element_in_a_parent_embedded_view() {
            let trigger_el = viewport_trigger_target_name(
                "
            <div *ngFor=\"let item of items\">
              <button #trigger></button>

              <div *ngFor=\"let child of item.children\">
                <div *ngFor=\"let grandchild of child.children\">
                  @defer (on viewport(trigger)) {}
                </div>
              </div>
            </div>
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("button"));
        }

        #[test]
        fn should_identify_a_trigger_element_inside_the_placeholder() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport(trigger)) {
              main
            } @placeholder {
              <button #trigger></button>
            }
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("button"));
        }

        #[test]
        fn should_not_identify_a_trigger_inside_the_main_content_block() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport(trigger)) {<button #trigger></button>}
          ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_identify_a_trigger_element_on_a_component() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport(trigger)) {}

            <comp #trigger/>
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("comp"));
        }

        #[test]
        fn should_identify_a_trigger_element_on_a_directive() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport(trigger)) {}

            <button dir #trigger=\"dir\"></button>
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("button"));
        }

        #[test]
        fn should_identify_an_implicit_trigger_inside_the_placeholder_block() {
            let trigger_el = viewport_trigger_target_name(
                "
          <div #trigger>
            @defer (on viewport) {} @placeholder {<button></button>}
          </div>
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("button"));
        }

        #[test]
        fn should_identify_an_implicit_trigger_inside_the_placeholder_block_with_comments() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport) {
              main
            } @placeholder {
              <!-- before -->
              <button #trigger></button>
              <!-- after -->
            }
          ",
            );
            assert_eq!(trigger_el.as_deref(), Some("button"));
        }

        #[test]
        fn should_not_identify_an_implicit_trigger_if_the_placeholder_has_multiple_root_nodes() {
            let trigger_el = viewport_trigger_target_name(
                "
            <div #trigger>
              @defer (on viewport) {} @placeholder {<button></button><div></div>}
            </div>
            ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_not_identify_an_implicit_trigger_if_there_is_no_placeholder() {
            let trigger_el = viewport_trigger_target_name(
                "
          <div #trigger>
            @defer (on viewport) {}
            <button></button>
          </div>
          ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_not_identify_an_implicit_trigger_if_the_placeholder_has_a_single_root_text_node() {
            let trigger_el = viewport_trigger_target_name(
                "
              <div #trigger>
                @defer (on viewport) {} @placeholder {hello}
              </div>
              ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_not_identify_a_trigger_inside_a_sibling_embedded_view() {
            let trigger_el = viewport_trigger_target_name(
                "
            <div *ngIf=\"cond\">
              <button #trigger></button>
            </div>

            @defer (on viewport(trigger)) {}
          ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_not_identify_a_trigger_element_in_an_embedded_view_inside_the_placeholder() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport(trigger)) {
              main
            } @placeholder {
              <div *ngIf=\"cond\"><button #trigger></button></div>
            }
          ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_not_identify_a_trigger_element_inside_the_a_deferred_block_within_the_placeholder() {
            let trigger_el = viewport_trigger_target_name(
                "
                @defer (on viewport(trigger)) {
                  main
                } @placeholder {
                  @defer {
                    <button #trigger></button>
                  }
                }
              ",
            );
            assert_eq!(trigger_el, None);
        }

        #[test]
        fn should_not_identify_a_trigger_element_on_a_template() {
            let trigger_el = viewport_trigger_target_name(
                "
            @defer (on viewport(trigger)) {}

            <ng-template #trigger></ng-template>
          ",
            );
            assert_eq!(trigger_el, None);
        }
    }

    mod used_pipes {
        use super::*;

        fn used_pipes(template: &str) -> Vec<String> {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, template, false);
            let res = bind(nodes, selector(make_selector_matcher()));
            res.get_used_pipes()
        }

        #[test]
        fn should_record_pipes_used_in_interpolations() {
            assert_eq!(used_pipes("{{value|date}}"), ["date"]);
        }

        #[test]
        fn should_record_pipes_used_in_bound_attributes() {
            assert_eq!(used_pipes("<person [age]=\"age|number\"></person>"), ["number"]);
        }

        #[test]
        fn should_record_pipes_used_in_bound_template_attributes() {
            assert_eq!(used_pipes("<ng-template [ngIf]=\"obs|async\"></ng-template>"), ["async"]);
        }

        #[test]
        fn should_record_pipes_used_in_icus() {
            assert_eq!(
                used_pipes(
                    "<span i18n>{count|number, plural,
            =1 { {{value|date}} }
          }</span>"
                ),
                ["number", "date"]
            );
        }
    }

    mod selectorless {
        use super::*;

        enum Registered {
            Single(Dir),
            WithHostDirectives { root: Dir, additional_directives: Vec<Dir> },
        }

        #[expect(clippy::unnecessary_wraps, reason = "the binder's matcher argument")]
        fn make_selectorless_matcher(directives: Vec<Registered>) -> Option<DirectiveMatcher<Dir>> {
            let mut registry: IndexMap<String, Vec<Dir>, FxBuildHasher> = IndexMap::default();

            for dir in directives {
                match dir {
                    Registered::Single(dir) => {
                        registry.insert(dir.name.clone(), vec![dir]);
                    }
                    Registered::WithHostDirectives { root, additional_directives } => {
                        let name = root.name.clone();
                        let mut all = vec![root];
                        all.extend(additional_directives);
                        registry.insert(name, all);
                    }
                }
            }

            Some(DirectiveMatcher::Selectorless(SelectorlessMatcher::new(registry)))
        }

        fn dir(name: &str) -> Dir {
            make_directive_meta(DirectiveConfig { name, selector: None, ..Default::default() })
        }

        fn comp(name: &str) -> Dir {
            make_directive_meta(DirectiveConfig { name, selector: None, is_component: true, ..Default::default() })
        }

        #[test]
        fn should_resolve_directives_applied_on_a_component_node() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<MyComp @Dir @OtherDir/>", true);
            let matcher = make_selectorless_matcher(vec![
                Registered::WithHostDirectives { root: comp("MyComp"), additional_directives: vec![dir("MyHostDir")] },
                Registered::Single(dir("Dir")),
                Registered::Single(dir("OtherDir")),
            ]);
            let res = bind(nodes, matcher);
            let node = component(&nodes[0]);
            assert_eq!(
                names(&res.get_directives_of_node(DirectiveOwner::Component(node)).unwrap()),
                ["MyComp", "MyHostDir"]
            );
        }

        #[test]
        fn should_resolve_directives_applied_on_a_directive_node() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<MyComp @Dir @OtherDir/>", true);
            let matcher = make_selectorless_matcher(vec![
                Registered::Single(comp("MyComp")),
                Registered::WithHostDirectives { root: dir("Dir"), additional_directives: vec![dir("HostDir")] },
                Registered::Single(dir("OtherDir")),
            ]);
            let res = bind(nodes, matcher);
            let dirs = &component(&nodes[0]).directives;
            assert_eq!(
                names(&res.get_directives_of_node(DirectiveOwner::Directive(&dirs[0])).unwrap()),
                ["Dir", "HostDir"]
            );
            assert_eq!(names(&res.get_directives_of_node(DirectiveOwner::Directive(&dirs[1])).unwrap()), ["OtherDir"]);
        }

        #[test]
        fn should_not_apply_selectorless_directives_on_an_element_node() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div @Dir @OtherDir></div>", true);
            let matcher =
                make_selectorless_matcher(vec![Registered::Single(dir("Dir")), Registered::Single(dir("OtherDir"))]);
            let res = bind(nodes, matcher);
            let node = element(&nodes[0]);
            assert!(res.get_directives_of_node(DirectiveOwner::Element(node)).is_none());
        }

        #[test]
        fn should_resolve_a_reference_on_a_component_node_to_the_component() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<MyComp #foo/>", true);
            let res = bind(nodes, make_selectorless_matcher(vec![Registered::Single(comp("MyComp"))]));
            let node = component(&nodes[0]);
            let reference = &node.references[0];
            let Some(ReferenceTarget::Directive { directive, node: result_node }) = res.get_reference_target(reference)
            else {
                panic!("not a directive")
            };
            assert_eq!(result_node, DirectiveOwner::Component(node));
            assert_eq!(directive.name, "MyComp");
        }

        #[test]
        fn should_resolve_a_reference_on_a_directive_node_to_the_component() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div @Dir(#foo)></div>", true);
            let res = bind(nodes, make_selectorless_matcher(vec![Registered::Single(dir("Dir"))]));
            let node = element(&nodes[0]);
            let directive_node = &node.directives[0];
            let reference = &directive_node.references[0];
            let Some(ReferenceTarget::Directive { directive, node: result_node }) = res.get_reference_target(reference)
            else {
                panic!("not a directive")
            };
            assert_eq!(result_node, DirectiveOwner::Directive(directive_node));
            assert_eq!(directive.name, "Dir");
        }

        #[test]
        fn should_resolve_a_reference_on_an_element_when_using_a_selectorless_matcher() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div #foo></div>", true);
            let res = bind(nodes, make_selectorless_matcher(vec![]));
            let node = element(&nodes[0]);
            let reference = &node.references[0];
            let Some(ReferenceTarget::Element(result)) = res.get_reference_target(reference) else {
                panic!("not an Element")
            };
            assert_eq!(result.name.as_str(), "div");
        }

        #[test]
        fn should_get_consumer_of_component_bindings() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "<MyComp [input]=\"value\" static=\"value\" (output)=\"doStuff()\" [doesNotExist]=\"value\" [attr.input]=\"value\"/>",
                true,
            );
            let res = bind(
                nodes,
                make_selectorless_matcher(vec![Registered::Single(make_directive_meta(DirectiveConfig {
                    name: "MyComp",
                    selector: None,
                    is_component: true,
                    inputs: &[("input", "input"), ("static", "static")],
                    outputs: &[("output", "output")],
                    ..Default::default()
                }))]),
            );
            let node = component(&nodes[0]);
            let input = &node.inputs[0];
            let static_attr = &node.attributes[0];
            let output = &node.outputs[0];
            let does_not_exist = &node.inputs[1];
            // `node.attributes[1]`: there is none (an `[attr.input]` binding is an input), and the consumer of
            // `undefined` is null.
            let attr_binding = node.attributes.get(1);

            assert_eq!(
                consumer_name(res.get_consumer_of_binding(Binding::BoundAttribute(input))).as_deref(),
                Some("MyComp")
            );
            assert_eq!(
                consumer_name(res.get_consumer_of_binding(Binding::TextAttribute(static_attr))).as_deref(),
                Some("MyComp")
            );
            assert_eq!(
                consumer_name(res.get_consumer_of_binding(Binding::BoundEvent(output))).as_deref(),
                Some("MyComp")
            );
            assert!(res.get_consumer_of_binding(Binding::BoundAttribute(does_not_exist)).is_none());
            assert!(attr_binding.and_then(|attr| res.get_consumer_of_binding(Binding::TextAttribute(attr))).is_none());
        }

        #[test]
        fn should_get_consumer_of_directive_bindings() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "<div @Dir([input]=\"value\" static=\"value\" (output)=\"doStuff()\" [doesNotExist]=\"value\")></div>",
                true,
            );
            let res = bind(
                nodes,
                make_selectorless_matcher(vec![Registered::Single(make_directive_meta(DirectiveConfig {
                    name: "Dir",
                    selector: None,
                    inputs: &[("input", "input"), ("static", "static")],
                    outputs: &[("output", "output")],
                    ..Default::default()
                }))]),
            );
            let node = element(&nodes[0]);
            let directive = &node.directives[0];
            let input = &directive.inputs[0];
            let static_attr = &directive.attributes[0];
            let output = &directive.outputs[0];
            let does_not_exist = &directive.inputs[1];

            assert_eq!(
                consumer_name(res.get_consumer_of_binding(Binding::BoundAttribute(input))).as_deref(),
                Some("Dir")
            );
            assert_eq!(
                consumer_name(res.get_consumer_of_binding(Binding::TextAttribute(static_attr))).as_deref(),
                Some("Dir")
            );
            assert_eq!(consumer_name(res.get_consumer_of_binding(Binding::BoundEvent(output))).as_deref(), Some("Dir"));
            assert!(res.get_consumer_of_binding(Binding::BoundAttribute(does_not_exist)).is_none());
        }

        #[test]
        fn should_get_eagerly_used_selectorless_directives() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<MyComp @Dir @OtherDir/>", true);
            let res = bind(
                nodes,
                make_selectorless_matcher(vec![
                    Registered::Single(comp("MyComp")),
                    Registered::Single(dir("Dir")),
                    Registered::Single(dir("OtherDir")),
                    Registered::Single(dir("UnusedDir")),
                ]),
            );
            assert_eq!(names(&res.get_used_directives()), ["MyComp", "Dir", "OtherDir"]);
            assert_eq!(names(&res.get_eagerly_used_directives()), ["MyComp", "Dir", "OtherDir"]);
        }

        #[test]
        fn should_get_deferred_selectorless_directives() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "@defer {<MyComp @Dir @OtherDir/>}", true);
            let res = bind(
                nodes,
                make_selectorless_matcher(vec![
                    Registered::Single(comp("MyComp")),
                    Registered::Single(dir("Dir")),
                    Registered::Single(dir("OtherDir")),
                ]),
            );
            assert_eq!(names(&res.get_used_directives()), ["MyComp", "Dir", "OtherDir"]);
            assert!(res.get_eagerly_used_directives().is_empty());
        }

        #[test]
        fn should_get_selectorless_directives_nested_in_other_code() {
            let allocator = Allocator::default();
            let nodes = parse_template(
                &allocator,
                "
        <section>
          @if (someCond) {
            <MyComp>
              <div>
                <h1>
                  <span @Dir></span>
                </h1>
              </div>
            </MyComp>
          }
        </section>
      ",
                true,
            );
            let res = bind(
                nodes,
                make_selectorless_matcher(vec![
                    Registered::Single(comp("MyComp")),
                    Registered::Single(dir("Dir")),
                    Registered::Single(dir("UnusedDir")),
                ]),
            );
            assert_eq!(names(&res.get_used_directives()), ["MyComp", "Dir"]);
            assert_eq!(names(&res.get_eagerly_used_directives()), ["MyComp", "Dir"]);
        }

        #[test]
        fn should_check_whether_a_referenced_directive_exists() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<MyComp @MissingDir/><MissingComp @Dir/>", true);
            let res = bind(
                nodes,
                make_selectorless_matcher(vec![Registered::Single(comp("MyComp")), Registered::Single(dir("Dir"))]),
            );
            assert!(res.referenced_directive_exists("MyComp"));
            assert!(res.referenced_directive_exists("Dir"));
            assert!(!res.referenced_directive_exists("MissingDir"));
            assert!(!res.referenced_directive_exists("MissingComp"));
        }
    }

    mod directive_de_duplication {
        use super::*;

        fn format_matches(matches: &[Dir]) -> Vec<String> {
            matches.iter().map(|dir| format!("{}:{:?}", dir.name, dir.match_source)).collect()
        }

        fn host_dir() -> Dir {
            make_directive_meta(DirectiveConfig {
                name: "HostDir",
                selector: None,
                match_source: Some(MatchSource::HostDirective),
                ..Default::default()
            })
        }

        /// `{...dir, ...changes}`
        fn spread(dir: &Dir, change: impl FnOnce(&mut TestDirectiveMeta)) -> Dir {
            let mut copy = (**dir).clone();
            change(&mut copy);
            Rc::new(copy)
        }

        fn merged_host<'a>(res: &R3BoundTarget<'a, Dir>, element: &'a R3Element<'a>) -> Dir {
            res.get_directives_of_node(DirectiveOwner::Element(element))
                .unwrap()
                .into_iter()
                .find(|d| d.name == "HostDir")
                .unwrap()
        }

        fn direct_mapped(mapping: &ClassPropertyMapping<InputOrOutput>) -> Vec<(String, String)> {
            mapping.to_direct_mapped_object().into_iter().collect()
        }

        fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
            entries.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
        }

        /// The matcher of the merge and conflict tests: two directives on `[dir]`, each with the host directive and
        /// its own mappings.
        fn two_dirs_with_host_dir(
            change_one: impl FnOnce(&mut TestDirectiveMeta),
            change_two: impl FnOnce(&mut TestDirectiveMeta),
        ) -> SelectorMatcher<Vec<Dir>> {
            let mut matcher = SelectorMatcher::new();
            let host_dir = host_dir();
            let one_dir =
                make_directive_meta(DirectiveConfig { name: "OneDir", selector: Some("[dir]"), ..Default::default() });
            let two_dir =
                make_directive_meta(DirectiveConfig { name: "TwoDir", selector: Some("[dir]"), ..Default::default() });
            matcher.add_selectables(&CssSelector::parse("[dir]"), vec![one_dir, spread(&host_dir, change_one)]);
            matcher.add_selectables(&CssSelector::parse("[dir]"), vec![two_dir, spread(&host_dir, change_two)]);
            matcher
        }

        #[test]
        fn should_give_precedence_to_the_template_matched_directive_over_a_host_directive_based_match() {
            let mut matcher = SelectorMatcher::new();
            let host_dir = make_directive_meta(DirectiveConfig {
                name: "HostDir",
                selector: Some("[dir]"),
                match_source: Some(MatchSource::HostDirective),
                ..Default::default()
            });
            matcher.add_selectables(
                &CssSelector::parse("[dir]"),
                vec![spread(&host_dir, |d| d.match_source = MatchSource::Selector)],
            );
            matcher.add_selectables(
                &CssSelector::parse("[dir]"),
                vec![
                    make_directive_meta(DirectiveConfig { name: "Dir", selector: Some("[dir]"), ..Default::default() }),
                    host_dir,
                ],
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let directives = res.get_directives_of_node(DirectiveOwner::Element(element)).unwrap();

            assert_eq!(format_matches(&directives), ["HostDir:Selector", "Dir:Selector"]);
        }

        #[test]
        fn should_de_duplicate_directives_that_match_multiple_times_as_host_directives() {
            let mut matcher = SelectorMatcher::new();
            let host_dir = host_dir();
            let one_dir =
                make_directive_meta(DirectiveConfig { name: "OneDir", selector: Some("[dir]"), ..Default::default() });
            let two_dir =
                make_directive_meta(DirectiveConfig { name: "TwoDir", selector: Some("[dir]"), ..Default::default() });
            matcher.add_selectables(&CssSelector::parse("[dir]"), vec![one_dir, Rc::clone(&host_dir)]);
            matcher.add_selectables(&CssSelector::parse("[dir]"), vec![two_dir, host_dir]);

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let directives = res.get_directives_of_node(DirectiveOwner::Element(element)).unwrap();

            assert_eq!(format_matches(&directives), ["OneDir:Selector", "HostDir:HostDirective", "TwoDir:Selector"]);
        }

        #[test]
        fn should_merge_the_inputs_of_duplicated_host_directives() {
            let matcher = two_dirs_with_host_dir(
                |d| d.inputs = mapping(&[("one", "one")]),
                |d| d.inputs = mapping(&[("two", "twoAlias")]),
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let merged_host = merged_host(&res, element);

            assert_eq!(merged_host.match_source, MatchSource::HostDirective);
            assert_eq!(direct_mapped(&merged_host.inputs), pairs(&[("one", "one"), ("two", "twoAlias")]));
            assert!(res.get_conflicting_host_directive_bindings(DirectiveOwner::Element(element)).is_none());
        }

        #[test]
        fn should_merge_the_outputs_of_duplicated_host_directives() {
            let matcher = two_dirs_with_host_dir(
                |d| d.outputs = mapping(&[("one", "one")]),
                |d| d.outputs = mapping(&[("two", "twoAlias")]),
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let merged_host = merged_host(&res, element);

            assert_eq!(merged_host.match_source, MatchSource::HostDirective);
            assert_eq!(direct_mapped(&merged_host.outputs), pairs(&[("one", "one"), ("two", "twoAlias")]));
            assert!(res.get_conflicting_host_directive_bindings(DirectiveOwner::Element(element)).is_none());
        }

        #[test]
        fn should_capture_conflicting_input_bindings_in_host_directives() {
            let matcher = two_dirs_with_host_dir(
                |d| d.inputs = mapping(&[("one", "one")]),
                |d| d.inputs = mapping(&[("one", "oneAlias")]),
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let merged_host = merged_host(&res, element);
            let conflict =
                res.get_conflicting_host_directive_bindings(DirectiveOwner::Element(element)).unwrap().remove(0);

            assert_eq!(merged_host.match_source, MatchSource::HostDirective);
            assert_eq!(direct_mapped(&merged_host.inputs), pairs(&[("one", "one")]));
            assert_eq!(conflict.kind, InputOrOutputKind::Input);
            assert_eq!(conflict.class_property_name, "one");
            assert_eq!(conflict.conflicting_aliases.into_iter().collect::<Vec<_>>(), ["one", "oneAlias"]);
        }

        #[test]
        fn should_not_capture_conflicting_input_bindings_if_they_are_equivalent() {
            let matcher = two_dirs_with_host_dir(
                |d| d.inputs = mapping(&[("one", "oneAlias")]),
                |d| d.inputs = mapping(&[("one", "oneAlias")]),
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let merged_host = merged_host(&res, element);

            assert_eq!(merged_host.match_source, MatchSource::HostDirective);
            assert_eq!(direct_mapped(&merged_host.inputs), pairs(&[("one", "oneAlias")]));
            assert!(res.get_conflicting_host_directive_bindings(DirectiveOwner::Element(element)).is_none());
        }

        #[test]
        fn should_capture_conflicting_output_bindings_in_host_directives() {
            let matcher = two_dirs_with_host_dir(
                |d| d.outputs = mapping(&[("one", "one")]),
                |d| d.outputs = mapping(&[("one", "oneAlias")]),
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let merged_host = merged_host(&res, element);
            let conflict =
                res.get_conflicting_host_directive_bindings(DirectiveOwner::Element(element)).unwrap().remove(0);

            assert_eq!(merged_host.match_source, MatchSource::HostDirective);
            assert_eq!(direct_mapped(&merged_host.outputs), pairs(&[("one", "one")]));
            assert_eq!(conflict.kind, InputOrOutputKind::Output);
            assert_eq!(conflict.class_property_name, "one");
            assert_eq!(conflict.conflicting_aliases.into_iter().collect::<Vec<_>>(), ["one", "oneAlias"]);
        }

        #[test]
        fn should_not_capture_conflicting_output_bindings_if_they_are_equivalent() {
            let matcher = two_dirs_with_host_dir(
                |d| d.outputs = mapping(&[("one", "oneAlias")]),
                |d| d.outputs = mapping(&[("one", "oneAlias")]),
            );

            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<div dir></div>", false);
            let res = bind(nodes, selector(matcher));
            let element = element(&nodes[0]);
            let merged_host = merged_host(&res, element);

            assert_eq!(merged_host.match_source, MatchSource::HostDirective);
            assert_eq!(direct_mapped(&merged_host.outputs), pairs(&[("one", "oneAlias")]));
            assert!(res.get_conflicting_host_directive_bindings(DirectiveOwner::Element(element)).is_none());
        }

        #[test]
        fn should_match_foreign_components_by_tag_name() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<FancyButton></FancyButton>", false);
            let mut registry: IndexMap<String, Vec<ForeignComponentMeta>, FxBuildHasher> = IndexMap::default();
            registry.insert("FancyButton".to_string(), vec![ForeignComponentMeta { name: "FancyButton".to_string() }]);
            let foreign_matcher = SelectorlessMatcher::new(registry);

            let binder =
                R3TargetBinder::with_foreign_component_matcher(selector(SelectorMatcher::new()), Some(foreign_matcher));
            let res = binder.bind(Target { template: Some(nodes), host: None });

            let el = element(&nodes[0]);
            let foreign_comp = res.get_foreign_component(el);
            assert_eq!(foreign_comp.map(|c| c.name).as_deref(), Some("FancyButton"));
        }

        #[test]
        #[should_panic(
            expected = "Conflict: Element 'comp' matches both an Angular directive and a foreign component."
        )]
        fn should_throw_an_error_when_tag_matches_both_directive_and_foreign_component() {
            let allocator = Allocator::default();
            let nodes = parse_template(&allocator, "<comp></comp>", false);
            let mut registry: IndexMap<String, Vec<ForeignComponentMeta>, FxBuildHasher> = IndexMap::default();
            registry.insert("comp".to_string(), vec![ForeignComponentMeta { name: "comp".to_string() }]);
            let foreign_matcher = SelectorlessMatcher::new(registry);

            let binder = R3TargetBinder::with_foreign_component_matcher(
                selector(make_selector_matcher()),
                Some(foreign_matcher),
            );

            binder.bind(Target { template: Some(nodes), host: None });
        }
    }
}
