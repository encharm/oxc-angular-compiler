//! `packages/compiler/src/typecheck/ops/content_projection.ts`

use oxc_angular_compiler::ast::r3::{R3Component, R3Element, R3Node};

use crate::angular::directive_matching::{CssSelector, SelectorMatcher};
use crate::angular::t2_binder::create_css_selector_from_node;
use crate::typecheck::api::DiagnosticCategoryLabel;
use crate::typecheck::oob::{ControlFlowNode, ElementOrTemplate, OutOfBandDiagnosticCategory};
use crate::typecheck::ops::base::{TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;

/// `Element | Component`: the node whose children are checked
#[derive(Debug, Clone, Copy)]
pub enum ElementOrComponent<'a> {
    Element(&'a R3Element<'a>),
    Component(&'a R3Component<'a>),
}

impl<'a> ElementOrComponent<'a> {
    /// `children`
    fn children(&self) -> &'a [R3Node<'a>] {
        match self {
            Self::Element(n) => &n.children,
            Self::Component(n) => &n.children,
        }
    }
}

// content_projection.ts:42 TcbControlFlowContentProjectionOp: a `TcbOp` that finds and flags control flow nodes that
// interfere with content projection.
//
// Context:
// Control flow blocks try to emulate the content projection behavior of `*ngIf` and `*ngFor`
// in order to reduce breakages when moving from one syntax to the other (see #52414), however the
// approach only works if there's only one element at the root of the control flow expression.
// This means that a stray sibling node (e.g. text) can prevent an element from being projected
// into the right slot. The purpose of the `TcbOp` is to find any places where a node at the root
// of a control flow expression *would have been projected* into a specific slot, if the control
// flow node didn't exist.
pub struct TcbControlFlowContentProjectionOp<'a, 'b> {
    category: OutOfBandDiagnosticCategory,
    tcb: TcbRef<'a, 'b>,
    element: ElementOrComponent<'a>,
    ng_content_selectors: Vec<String>,
    component_name: String,
}

impl<'a, 'b> TcbControlFlowContentProjectionOp<'a, 'b> {
    // content_projection.ts:45 constructor
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        element: ElementOrComponent<'a>,
        ng_content_selectors: Vec<String>,
        component_name: String,
    ) -> Self {
        // We only need to account for `error` and `warning` since
        // this check won't be enabled for `suppress`.
        let category = if tcb.env.config().control_flow_preventing_content_projection
            == DiagnosticCategoryLabel::Error
        {
            OutOfBandDiagnosticCategory::Error
        } else {
            OutOfBandDiagnosticCategory::Warning
        };
        Self { category, tcb, element, ng_content_selectors, component_name }
    }

    // content_projection.ts:98 findPotentialControlFlowNodes
    fn find_potential_control_flow_nodes(&self) -> Vec<(ControlFlowNode<'a>, &'a [R3Node<'a>])> {
        let mut result: Vec<(ControlFlowNode<'a>, &'a [R3Node<'a>])> = Vec::new();

        for child in self.element.children() {
            match child {
                R3Node::ForLoopBlock(child) => {
                    if self.should_check(&child.children) {
                        result.push((ControlFlowNode::ForLoopBlock(child), &child.children));
                    }
                    if let Some(empty) = &child.empty
                        && self.should_check(&empty.children)
                    {
                        result.push((ControlFlowNode::ForLoopBlockEmpty(empty), &empty.children));
                    }
                }
                R3Node::IfBlock(child) => {
                    for branch in &child.branches {
                        if self.should_check(&branch.children) {
                            result.push((ControlFlowNode::IfBlockBranch(branch), &branch.children));
                        }
                    }
                }
                R3Node::SwitchBlock(child) => {
                    for current in &child.groups {
                        if self.should_check(&current.children) {
                            result.push((
                                ControlFlowNode::SwitchBlockCaseGroup(current),
                                &current.children,
                            ));
                        }
                    }
                }
                // `else if (child instanceof BoundaryBlock)`: Angular's `BoundaryBlock` is not in the fork's AST.
                // (`@error` blocks aren't checked, because they don't copy the tag name and attributes of their
                // root node for content projection.)
                _ => {}
            }
        }

        result
    }

    // content_projection.ts:135 shouldCheck
    fn should_check(&self, children: &[R3Node<'a>]) -> bool {
        // Skip nodes with less than two children since it's impossible
        // for them to run into the issue that we're checking for.
        if children.len() < 2 {
            return false;
        }

        let mut has_seen_root_node = false;

        // Check the number of root nodes while skipping empty text where relevant.
        for child in children {
            // `@let` declarations don't render anything so they don't affect content projection.
            if matches!(child, R3Node::LetDeclaration(_)) {
                continue;
            }

            // Normally `preserveWhitspaces` would have been accounted for during parsing, however
            // in `ngtsc/annotations/component/src/resources.ts#parseExtractedTemplate` we enable
            // `preserveWhitespaces` to preserve the accuracy of source maps diagnostics. This means
            // that we have to account for it here since the presence of text nodes affects the
            // content projection behavior.
            let is_text_with_content = |text: &str| text.chars().any(|c| !is_js_whitespace(c));
            if !matches!(child, R3Node::Text(_))
                || self.tcb.host_preserve_whitespaces
                || matches!(child, R3Node::Text(text) if is_text_with_content(&text.value))
            {
                // Content projection will be affected if there's more than one root node.
                if has_seen_root_node {
                    return true;
                }
                has_seen_root_node = true;
            }
        }

        false
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbControlFlowContentProjectionOp<'a, 'b> {
    // content_projection.ts:61 optional
    fn optional(&self) -> bool {
        false
    }

    // content_projection.ts:63 execute
    fn execute(&self) -> Option<TcbExpr> {
        let control_flow_to_check = self.find_potential_control_flow_nodes();

        if !control_flow_to_check.is_empty() {
            let mut matcher = SelectorMatcher::<String>::new();

            for selector in &self.ng_content_selectors {
                // `*` is a special selector for the catch-all slot.
                if selector != "*" {
                    matcher.add_selectables(&CssSelector::parse(selector), selector.clone());
                }
            }

            for (root, children) in control_flow_to_check {
                for child in children {
                    let child = match child {
                        R3Node::Element(child) => ElementOrTemplate::Element(child),
                        R3Node::Template(child) => ElementOrTemplate::Template(child),
                        _ => continue,
                    };
                    let mut callback = |_: &CssSelector, original_selector: &String| {
                        self.tcb.oob_recorder.control_flow_preventing_content_projection(
                            &self.tcb.id,
                            self.category,
                            child,
                            &self.component_name,
                            original_selector,
                            root,
                            self.tcb.host_preserve_whitespaces,
                        );
                    };
                    matcher.match_(&create_css_selector_from_node(child), Some(&mut callback));
                }
            }
        }

        None
    }
}

/// Whether `String.prototype.trim` removes `c`: ECMAScript's WhiteSpace and LineTerminator
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{FEFF}'
            | '\u{000A}'
            | '\u{000D}'
            | '\u{2028}'
            | '\u{2029}'
    ) || (c != '\u{0085}' && c.is_whitespace() && !c.is_ascii())
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;

    use super::*;
    use crate::typecheck::ops::bindings::test_support::{Parts, config, element, parse};

    fn control_flow_nodes(template: &str, preserve_whitespaces: bool) -> Vec<String> {
        let allocator = Allocator::default();
        let nodes = parse(&allocator, allocator.alloc_str(template));
        let mut parts = Parts::new(config());
        parts.preserve_whitespaces = preserve_whitespaces;
        let op = TcbControlFlowContentProjectionOp::new(
            parts.context(),
            ElementOrComponent::Element(element(nodes)),
            vec!["foo".to_string()],
            "Comp".to_string(),
        );
        op.find_potential_control_flow_nodes()
            .into_iter()
            .map(|(node, children)| {
                let name = match node {
                    ControlFlowNode::IfBlockBranch(_) => "IfBlockBranch",
                    ControlFlowNode::SwitchBlockCase(_) => "SwitchBlockCase",
                    ControlFlowNode::SwitchBlockCaseGroup(_) => "SwitchBlockCaseGroup",
                    ControlFlowNode::ForLoopBlock(_) => "ForLoopBlock",
                    ControlFlowNode::ForLoopBlockEmpty(_) => "ForLoopBlockEmpty",
                };
                format!("{name}({})", children.len())
            })
            .collect()
    }

    #[test]
    fn control_flow_nodes_with_several_roots() {
        assert_eq!(
            control_flow_nodes(
                "<comp>@if (a) {<foo/> text} @else {<foo/>} \
                 @for (x of xs; track x) {<foo/><bar/>} @empty {<foo/>@let y = 1;} \
                 @switch (a) { @case (1) {<foo/> <foo/>} @default {<foo/>} }</comp>",
                false,
            ),
            ["IfBlockBranch(2)", "ForLoopBlock(2)", "SwitchBlockCaseGroup(3)"]
        );
    }

    #[test]
    fn whitespace_text_is_a_root_only_when_preserved() {
        let template = "<comp>@if (a) {<foo/>\u{a0}} @if (b) {<foo/>\n} </comp>";
        assert_eq!(control_flow_nodes(template, false), Vec::<String>::new());
        assert_eq!(control_flow_nodes(template, true), ["IfBlockBranch(2)", "IfBlockBranch(2)"]);
    }

    #[test]
    fn js_whitespace() {
        assert!(is_js_whitespace('\u{feff}'));
        assert!(is_js_whitespace('\u{3000}'));
        assert!(!is_js_whitespace('\u{85}'));
        assert!(!is_js_whitespace('a'));
    }
}
