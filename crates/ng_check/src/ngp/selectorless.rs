//! `packages/compiler-cli/preprocessor/src/selectorless.ts`: whether a template uses selectorless syntax (components,
//! directives or pipes referenced by their class name).
//!
//! The two visitors are TypeScript subclasses of `TmplAstRecursiveVisitor` and `RecursiveAstVisitor`; here they are
//! walks that visit what those visit, in the same order.

use oxc_angular_compiler::ast::expression::{AngularExpression, BindingPipeType};
use oxc_angular_compiler::ast::r3::{R3BoundAttribute, R3BoundEvent, R3Directive, R3Node};

use crate::typecheck::api::Set;

// selectorless.ts:22 SelectorlessPipeAnalyzer: tracks the pipes referenced directly (`type === 1`)
fn visit_pipes(ast: &AngularExpression<'_>, symbols: &mut Option<Set<String>>) {
    use AngularExpression as E;
    let mut visit = |ast: &AngularExpression<'_>| visit_pipes(ast, symbols);
    match ast {
        E::BindingPipe(ast) => {
            if ast.pipe_type == BindingPipeType::ReferencedDirectly {
                track_symbol(symbols, &ast.name);
            }
            visit_pipes(&ast.exp, symbols);
            for arg in &ast.args {
                visit_pipes(arg, symbols);
            }
        }
        E::Binary(ast) => {
            visit(&ast.left);
            visit(&ast.right);
        }
        E::Unary(ast) => visit(&ast.expr),
        E::Chain(ast) => ast.expressions.iter().for_each(visit),
        E::Conditional(ast) => {
            visit(&ast.condition);
            visit(&ast.true_exp);
            visit(&ast.false_exp);
        }
        E::Interpolation(ast) => ast.expressions.iter().for_each(visit),
        E::KeyedRead(ast) => {
            visit(&ast.receiver);
            visit(&ast.key);
        }
        E::SafeKeyedRead(ast) => {
            visit(&ast.receiver);
            visit(&ast.key);
        }
        E::LiteralArray(ast) => ast.expressions.iter().for_each(visit),
        E::LiteralMap(ast) => ast.values.iter().for_each(visit),
        E::PrefixNot(ast) => visit(&ast.expression),
        E::TypeofExpression(ast) => visit(&ast.expression),
        E::VoidExpression(ast) => visit(&ast.expression),
        E::NonNullAssert(ast) => visit(&ast.expression),
        E::PropertyRead(ast) => visit(&ast.receiver),
        E::SafePropertyRead(ast) => visit(&ast.receiver),
        E::Call(ast) => {
            visit(&ast.receiver);
            ast.args.iter().for_each(visit);
        }
        E::SafeCall(ast) => {
            visit(&ast.receiver);
            ast.args.iter().for_each(visit);
        }
        E::TemplateLiteral(ast) => ast.expressions.iter().for_each(visit),
        E::TaggedTemplateLiteral(ast) => {
            visit(&ast.tag);
            ast.template.expressions.iter().for_each(visit);
        }
        E::ParenthesizedExpression(ast) => visit(&ast.expression),
        E::ArrowFunction(ast) => visit(&ast.body),
        E::SpreadElement(ast) => visit(&ast.expression),
        E::Empty(_)
        | E::ImplicitReceiver(_)
        | E::ThisReceiver(_)
        | E::LiteralPrimitive(_)
        | E::RegularExpressionLiteral(_) => {}
    }
}

// selectorless.ts:86 trackSymbol
fn track_symbol(symbols: &mut Option<Set<String>>, name: &str) {
    symbols.get_or_insert_with(Set::default).insert(name.to_string());
}

// selectorless.ts:35 SelectorlessDirectivesAnalyzer
#[derive(Default)]
struct SelectorlessDirectivesAnalyzer {
    symbols: Option<Set<String>>,
}

impl SelectorlessDirectivesAnalyzer {
    // selectorless.ts:56 visitBoundAttribute
    fn visit_bound_attributes(&mut self, attributes: &[R3BoundAttribute<'_>]) {
        for attribute in attributes {
            visit_pipes(&attribute.value, &mut self.symbols);
        }
    }

    // selectorless.ts:61 visitBoundEvent
    fn visit_bound_events(&mut self, events: &[R3BoundEvent<'_>]) {
        for event in events {
            visit_pipes(&event.handler, &mut self.symbols);
        }
    }

    // selectorless.ts:71 visitDirective
    fn visit_directives(&mut self, directives: &[R3Directive<'_>]) {
        for directive in directives {
            track_symbol(&mut self.symbols, &directive.name);
            self.visit_bound_attributes(&directive.inputs);
            self.visit_bound_events(&directive.outputs);
        }
    }

    fn visit_all(&mut self, nodes: &[R3Node<'_>]) {
        for node in nodes {
            self.visit(node);
        }
    }

    /// r3_ast.ts RecursiveVisitor, with the overridden visits
    fn visit(&mut self, node: &R3Node<'_>) {
        match node {
            // selectorless.ts:39 visitElement
            R3Node::Element(element) => {
                for input in &element.inputs {
                    visit_pipes(&input.value, &mut self.symbols);
                }
                for output in &element.outputs {
                    visit_pipes(&output.handler, &mut self.symbols);
                }
                self.visit_bound_attributes(&element.inputs);
                self.visit_bound_events(&element.outputs);
                self.visit_directives(&element.directives);
                self.visit_all(&element.children);
            }
            R3Node::Template(template) => {
                self.visit_bound_attributes(&template.inputs);
                self.visit_bound_events(&template.outputs);
                self.visit_directives(&template.directives);
                self.visit_all(&template.children);
            }
            // selectorless.ts:51 visitBoundText
            R3Node::BoundText(text) => visit_pipes(&text.value, &mut self.symbols),
            // selectorless.ts:66 visitComponent
            R3Node::Component(component) => {
                track_symbol(&mut self.symbols, &component.component_name);
                self.visit_bound_attributes(&component.inputs);
                self.visit_bound_events(&component.outputs);
                self.visit_directives(&component.directives);
                self.visit_all(&component.children);
            }
            R3Node::Directive(directive) => self.visit_directives(std::slice::from_ref(&**directive)),
            R3Node::Content(content) => self.visit_all(&content.children),
            R3Node::DeferredBlock(block) => {
                self.visit_all(&block.children);
                if let Some(placeholder) = &block.placeholder {
                    self.visit_all(&placeholder.children);
                }
                if let Some(loading) = &block.loading {
                    self.visit_all(&loading.children);
                }
                if let Some(error) = &block.error {
                    self.visit_all(&error.children);
                }
            }
            R3Node::DeferredBlockPlaceholder(block) => self.visit_all(&block.children),
            R3Node::DeferredBlockLoading(block) => self.visit_all(&block.children),
            R3Node::DeferredBlockError(block) => self.visit_all(&block.children),
            R3Node::SwitchBlock(block) => {
                for group in &block.groups {
                    self.visit_all(&group.children);
                }
            }
            R3Node::SwitchBlockCaseGroup(group) => self.visit_all(&group.children),
            R3Node::ForLoopBlock(block) => {
                self.visit_all(&block.children);
                if let Some(empty) = &block.empty {
                    self.visit_all(&empty.children);
                }
            }
            R3Node::ForLoopBlockEmpty(block) => self.visit_all(&block.children),
            R3Node::IfBlock(block) => {
                for branch in &block.branches {
                    self.visit_all(&branch.children);
                }
            }
            R3Node::IfBlockBranch(branch) => self.visit_all(&branch.children),
            R3Node::Comment(_)
            | R3Node::Text(_)
            | R3Node::Variable(_)
            | R3Node::Reference(_)
            | R3Node::Icu(_)
            | R3Node::UnknownBlock(_)
            | R3Node::LetDeclaration(_)
            | R3Node::HostElement(_) => {}
        }
    }
}

/// analyzeTemplateForSelectorless's result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorlessAnalysis {
    pub is_selectorless: bool,
    pub local_referenced_symbols: Option<Set<String>>,
}

// selectorless.ts:93 analyzeTemplateForSelectorless
pub fn analyze_template_for_selectorless(template: &[R3Node<'_>]) -> SelectorlessAnalysis {
    let mut analyzer = SelectorlessDirectivesAnalyzer::default();
    analyzer.visit_all(template);
    let is_selectorless = analyzer.symbols.as_ref().is_some_and(|symbols| !symbols.is_empty());
    SelectorlessAnalysis { is_selectorless, local_referenced_symbols: analyzer.symbols }
}
