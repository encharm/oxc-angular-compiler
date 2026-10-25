//! A template's structure as JSON: the R3 tree with its source text, for tools that read templates without
//! compiling them.
//!
//! The template is parsed as [`crate::component::transform`] compiles it (block and `@let` syntax, ICU expansion
//! forms, whitespace removed with significant whitespace kept), so a tool sees the elements, attributes and blocks the
//! compiled component has. Expressions are given as their source text; offsets are byte offsets into the template,
//! lines are 1-based.
//!
//! Node shapes, by `type`:
//! - `element`: `name`, `attrs` (`name`, `value`), `inputs` (`name`, `binding`, `expr`), `outputs` (`name`,
//!   `handler`), `refs` (`name`, `value`), `selfClosing`, `children`
//! - `template`: an `<ng-template>` or a structural directive's template: `tag`, `attrs`, `inputs`, `outputs`,
//!   `refs`, `templateAttrs` (`name` with `value` or `expr`), `vars` (`name`, `value`), `children`
//! - `if`: `branches` (`expr`, `null` for `@else`; `alias`; `children`)
//! - `for`: `expr`, `item`, `track`, `children`, `empty`
//! - `switch`: `expr`, `groups` (`cases`, each an expression or `null` for `@default`; `children`)
//! - `defer`: `children`, `placeholder`, `loading`, `error`
//! - `let`: `name`, `expr`
//! - `text`: `value`; `boundText`: `expr`, the text with its interpolations
//! - `content`: `selector`; `icu`; `unknownBlock`: `name`; `selectorless`
//!
//! Every node has `start` and `line`. An annotation hook adds a field to elements and templates (ng-check adds the
//! directives each one matches).

use oxc_allocator::Allocator;
use oxc_span::Span;
use serde_json::{Map, Value, json};

use crate::ast::expression::{AngularExpression, BindingType};
use crate::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Element, R3Node, R3Reference, R3Template, R3TemplateAttr,
    R3TextAttribute,
};
use crate::parser::ParseTemplateOptions;
use crate::parser::html::{HtmlParser, remove_whitespaces};
use crate::transform::HtmlToR3Transform;
use crate::transform::html_to_r3::TransformOptions as R3TransformOptions;

/// An element or a template, as handed to an annotation hook.
pub enum AnnotatedNode<'n, 'a> {
    /// An element.
    Element(&'n R3Element<'a>),
    /// An `<ng-template>` or a structural directive's template.
    Template(&'n R3Template<'a>),
}

/// Adds a field to an element or a template: its name and value, or nothing.
pub type Annotate<'h, 'a> = dyn Fn(AnnotatedNode<'_, 'a>) -> Option<(&'static str, Value)> + 'h;

/// Serializes R3 nodes over the template source they were parsed from.
pub struct StructureWriter<'s, 'h, 'a> {
    source: &'s str,
    line_starts: Vec<u32>,
    annotate: Option<&'h Annotate<'h, 'a>>,
}

impl<'s, 'h, 'a> StructureWriter<'s, 'h, 'a> {
    /// A writer over `source`, the text the nodes' spans point into.
    pub fn new(source: &'s str, annotate: Option<&'h Annotate<'h, 'a>>) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source.match_indices('\n').map(|(i, _)| u32::try_from(i + 1).unwrap_or(u32::MAX)),
        );
        Self { source, line_starts, annotate }
    }

    /// The 1-based line of a byte offset.
    pub fn line(&self, offset: u32) -> usize {
        self.line_starts.partition_point(|&start| start <= offset)
    }

    fn text(&self, start: u32, end: u32) -> &'s str {
        self.source.get(start as usize..end as usize).unwrap_or("")
    }

    fn expr(&self, expr: &AngularExpression<'_>) -> Value {
        let span = expr.source_span();
        json!(self.text(span.start, span.end).trim())
    }

    fn located(&self, kind: &str, span: Span) -> Map<String, Value> {
        let mut node = Map::new();
        node.insert("type".into(), json!(kind));
        node.insert("start".into(), json!(span.start));
        node.insert("line".into(), json!(self.line(span.start)));
        node
    }

    /// The nodes as a JSON array.
    pub fn nodes(&self, nodes: &[R3Node<'a>]) -> Value {
        Value::Array(nodes.iter().filter_map(|node| self.node(node)).collect())
    }

    fn node(&self, node: &R3Node<'a>) -> Option<Value> {
        let value = match node {
            R3Node::Comment(_)
            | R3Node::Variable(_)
            | R3Node::Reference(_)
            | R3Node::HostElement(_) => return None,
            R3Node::Text(t) => {
                let mut n = self.located("text", t.source_span);
                n.insert("value".into(), json!(t.value.as_str()));
                n
            }
            R3Node::BoundText(t) => {
                let mut n = self.located("boundText", t.source_span);
                n.insert("expr".into(), json!(self.text(t.source_span.start, t.source_span.end)));
                n
            }
            R3Node::Element(e) => self.element(e),
            R3Node::Template(t) => self.template(t),
            R3Node::Content(c) => {
                let mut n = self.located("content", c.source_span);
                n.insert("selector".into(), json!(c.selector.as_str()));
                n
            }
            R3Node::Icu(i) => self.located("icu", i.source_span),
            R3Node::IfBlock(block) => {
                let mut n = self.located("if", block.source_span);
                let branches: Vec<Value> = block
                    .branches
                    .iter()
                    .map(|b| {
                        json!({
                            "expr": b.expression.as_ref().map(|e| self.expr(e)),
                            "alias": b.expression_alias.as_ref().map(|a| a.name.as_str()),
                            "start": b.source_span.start,
                            "line": self.line(b.source_span.start),
                            "children": self.nodes(&b.children),
                        })
                    })
                    .collect();
                n.insert("branches".into(), Value::Array(branches));
                n
            }
            R3Node::IfBlockBranch(b) => {
                let mut n = self.located("if", b.source_span);
                n.insert(
                    "branches".into(),
                    json!([{ "expr": b.expression.as_ref().map(|e| self.expr(e)), "children": self.nodes(&b.children) }]),
                );
                n
            }
            R3Node::ForLoopBlock(block) => {
                let mut n = self.located("for", block.source_span);
                let source = |s: &crate::ast::expression::ASTWithSource<'_>| match &s.source {
                    Some(text) => json!(text.as_str().trim()),
                    None => self.expr(&s.ast),
                };
                n.insert("expr".into(), source(&block.expression));
                n.insert("item".into(), json!(block.item.name.as_str()));
                n.insert("track".into(), source(&block.track_by));
                n.insert("children".into(), self.nodes(&block.children));
                n.insert(
                    "empty".into(),
                    block.empty.as_ref().map_or(Value::Null, |e| self.nodes(&e.children)),
                );
                n
            }
            R3Node::ForLoopBlockEmpty(e) => {
                let mut n = self.located("for", e.source_span);
                n.insert("children".into(), json!([]));
                n.insert("empty".into(), self.nodes(&e.children));
                n
            }
            R3Node::SwitchBlock(block) => {
                let mut n = self.located("switch", block.source_span);
                n.insert("expr".into(), self.expr(&block.expression));
                let groups: Vec<Value> = block
                    .groups
                    .iter()
                    .map(|g| {
                        let cases: Vec<Value> = g
                            .cases
                            .iter()
                            .map(|c| c.expression.as_ref().map_or(Value::Null, |e| self.expr(e)))
                            .collect();
                        json!({ "cases": cases, "children": self.nodes(&g.children) })
                    })
                    .collect();
                n.insert("groups".into(), Value::Array(groups));
                n
            }
            R3Node::SwitchBlockCaseGroup(g) => {
                let mut n = self.located("switch", g.source_span);
                n.insert("expr".into(), Value::Null);
                n.insert(
                    "groups".into(),
                    json!([{ "cases": [], "children": self.nodes(&g.children) }]),
                );
                n
            }
            R3Node::DeferredBlock(block) => {
                let mut n = self.located("defer", block.source_span);
                n.insert("children".into(), self.nodes(&block.children));
                n.insert(
                    "placeholder".into(),
                    block.placeholder.as_ref().map_or(Value::Null, |b| self.nodes(&b.children)),
                );
                n.insert(
                    "loading".into(),
                    block.loading.as_ref().map_or(Value::Null, |b| self.nodes(&b.children)),
                );
                n.insert(
                    "error".into(),
                    block.error.as_ref().map_or(Value::Null, |b| self.nodes(&b.children)),
                );
                n
            }
            R3Node::DeferredBlockPlaceholder(b) => self.defer_part(b.source_span, &b.children),
            R3Node::DeferredBlockLoading(b) => self.defer_part(b.source_span, &b.children),
            R3Node::DeferredBlockError(b) => self.defer_part(b.source_span, &b.children),
            R3Node::LetDeclaration(l) => {
                let mut n = self.located("let", l.source_span);
                n.insert("name".into(), json!(l.name.as_str()));
                n.insert(
                    "expr".into(),
                    json!(self.text(l.value_span.start, l.value_span.end).trim()),
                );
                n
            }
            R3Node::UnknownBlock(b) => {
                let mut n = self.located("unknownBlock", b.source_span);
                n.insert("name".into(), json!(b.name.as_str()));
                n
            }
            R3Node::Component(c) => self.located("selectorless", c.source_span),
            R3Node::Directive(d) => self.located("selectorless", d.source_span),
        };
        Some(Value::Object(value))
    }

    fn defer_part(&self, span: Span, children: &[R3Node<'a>]) -> Map<String, Value> {
        let mut n = self.located("defer", span);
        n.insert("children".into(), self.nodes(children));
        n
    }

    fn element(&self, e: &R3Element<'a>) -> Map<String, Value> {
        let mut n = self.located("element", e.source_span);
        n.insert("name".into(), json!(e.name.as_str()));
        n.insert("attrs".into(), Self::attrs(&e.attributes));
        n.insert("inputs".into(), self.inputs(&e.inputs));
        n.insert("outputs".into(), self.outputs(&e.outputs));
        n.insert("refs".into(), Self::refs(&e.references));
        n.insert("selfClosing".into(), json!(e.is_self_closing || e.is_void));
        if let Some((key, value)) = self.annotate.and_then(|f| f(AnnotatedNode::Element(e))) {
            n.insert(key.into(), value);
        }
        n.insert("children".into(), self.nodes(&e.children));
        n
    }

    fn template(&self, t: &R3Template<'a>) -> Map<String, Value> {
        let mut n = self.located("template", t.source_span);
        n.insert("tag".into(), t.tag_name.as_ref().map_or(Value::Null, |tag| json!(tag.as_str())));
        n.insert("attrs".into(), Self::attrs(&t.attributes));
        n.insert("inputs".into(), self.inputs(&t.inputs));
        n.insert("outputs".into(), self.outputs(&t.outputs));
        n.insert("refs".into(), Self::refs(&t.references));
        let template_attrs: Vec<Value> = t
            .template_attrs
            .iter()
            .map(|a| match a {
                R3TemplateAttr::Bound(b) => {
                    json!({ "name": b.name.as_str(), "expr": self.bound_value(b) })
                }
                R3TemplateAttr::Text(t) => {
                    json!({ "name": t.name.as_str(), "value": t.value.as_str() })
                }
            })
            .collect();
        n.insert("templateAttrs".into(), Value::Array(template_attrs));
        let vars: Vec<Value> = t
            .variables
            .iter()
            .map(|v| json!({ "name": v.name.as_str(), "value": v.value.as_str() }))
            .collect();
        n.insert("vars".into(), Value::Array(vars));
        if let Some((key, value)) = self.annotate.and_then(|f| f(AnnotatedNode::Template(t))) {
            n.insert(key.into(), value);
        }
        n.insert("children".into(), self.nodes(&t.children));
        n
    }

    fn attrs(attrs: &[R3TextAttribute<'a>]) -> Value {
        Value::Array(
            attrs
                .iter()
                .map(|a| json!({ "name": a.name.as_str(), "value": a.value.as_str() }))
                .collect(),
        )
    }

    fn bound_value(&self, b: &R3BoundAttribute<'a>) -> Value {
        match b.value_span {
            Some(span) => json!(self.text(span.start, span.end).trim()),
            None => self.expr(&b.value),
        }
    }

    fn inputs(&self, inputs: &[R3BoundAttribute<'a>]) -> Value {
        Value::Array(
            inputs
                .iter()
                .map(|b| {
                    let binding = match b.binding_type {
                        BindingType::Property => "property",
                        BindingType::Attribute => "attribute",
                        BindingType::Class => "class",
                        BindingType::Style => "style",
                        BindingType::TwoWay => "twoWay",
                        BindingType::Animation | BindingType::LegacyAnimation => "animation",
                    };
                    json!({ "name": b.name.as_str(), "binding": binding, "expr": self.bound_value(b) })
                })
                .collect(),
        )
    }

    fn outputs(&self, outputs: &[R3BoundEvent<'a>]) -> Value {
        Value::Array(
            outputs
                .iter()
                .map(|o| {
                    json!({ "name": o.name.as_str(), "handler": self.text(o.handler_span.start, o.handler_span.end).trim() })
                })
                .collect(),
        )
    }

    fn refs(refs: &[R3Reference<'a>]) -> Value {
        Value::Array(
            refs.iter()
                .map(|r| json!({ "name": r.name.as_str(), "value": r.value.as_str() }))
                .collect(),
        )
    }
}

/// Parses `template` as the compiler does and returns `{ nodes, errors }`; `errors` holds the parse errors (`msg`,
/// `line`), and `nodes` is empty when there are any.
pub fn parse_template_structure(template: &str, file_path: &str) -> Value {
    let allocator = Allocator::default();
    let options = ParseTemplateOptions {
        enable_block_syntax: true,
        enable_let_syntax: true,
        tokenize_expansion_forms: true,
        ..Default::default()
    };
    let html = HtmlParser::with_options(&allocator, template, file_path, &options).parse();
    let writer = StructureWriter::new(template, None);
    let error = |msg: &str, offset: u32| json!({ "msg": msg, "line": writer.line(offset) });
    if !html.errors.is_empty() {
        let errors: Vec<Value> =
            html.errors.iter().map(|e| error(&e.msg, e.span.start.offset)).collect();
        return json!({ "nodes": [], "errors": errors });
    }
    let nodes = remove_whitespaces(&allocator, &html.nodes, true);
    let r3 = HtmlToR3Transform::new(&allocator, template, R3TransformOptions::default())
        .transform(&nodes);
    if !r3.errors.is_empty() {
        let errors: Vec<Value> =
            r3.errors.iter().map(|e| error(&e.msg, e.span.start.offset)).collect();
        return json!({ "nodes": [], "errors": errors });
    }
    json!({ "nodes": writer.nodes(&r3.nodes), "errors": [] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(template: &str) -> Value {
        let result = parse_template_structure(template, "test.html");
        assert_eq!(result["errors"], json!([]), "{result}");
        result["nodes"].clone()
    }

    #[test]
    fn element_attributes_inputs_outputs_and_refs() {
        let n = nodes(
            r#"<button command="copy" [attr.data-hint-value]="tool.size" (click)="go()" #b>Copy</button>"#,
        );
        let button = &n[0];
        assert_eq!(button["type"], "element");
        assert_eq!(button["name"], "button");
        assert_eq!(button["attrs"], json!([{ "name": "command", "value": "copy" }]));
        assert_eq!(
            button["inputs"],
            json!([{ "name": "data-hint-value", "binding": "attribute", "expr": "tool.size" }])
        );
        assert_eq!(button["outputs"], json!([{ "name": "click", "handler": "go()" }]));
        assert_eq!(button["refs"], json!([{ "name": "b", "value": "" }]));
        assert_eq!(
            button["children"][0],
            json!({ "type": "text", "value": "Copy", "start": 76, "line": 1 })
        );
    }

    #[test]
    fn apostrophes_in_text_are_text() {
        let n = nodes("<p>Don't worry</p>\n<button command=\"copy\">It's fine</button>");
        assert_eq!(n[1]["attrs"][0]["value"], "copy");
        assert_eq!(n[1]["line"], 2);
        assert_eq!(n[1]["children"][0]["value"], "It's fine");
    }

    #[test]
    fn if_branches_keep_their_conditions_and_alias() {
        let n = nodes(
            "@if (editor.selectedTool; as tool) {<i></i>} @else if (a || b) {<b></b>} @else {<u></u>}",
        );
        let branches = &n[0]["branches"];
        assert_eq!(branches[0]["expr"], "editor.selectedTool");
        assert_eq!(branches[0]["alias"], "tool");
        assert_eq!(branches[1]["expr"], "a || b");
        assert_eq!(branches[2]["expr"], Value::Null);
        assert_eq!(branches[2]["children"][0]["name"], "u");
    }

    #[test]
    fn templates_structural_directives_for_and_switch() {
        let n = nodes(concat!(
            r#"<ng-template #menu><a command="paste"></a></ng-template>"#,
            r#"<ng-container *ngTemplateOutlet="menu"></ng-container>"#,
            "@for (item of items; track item.id) {<i></i>} @empty {<b></b>}",
            "@switch (mode) { @case ('a') {<i></i>} @default {<b></b>} }",
        ));
        assert_eq!(n[0]["type"], "template");
        assert_eq!(n[0]["refs"][0]["name"], "menu");
        assert_eq!(n[1]["type"], "template");
        assert_eq!(n[1]["templateAttrs"][0], json!({ "name": "ngTemplateOutlet", "expr": "menu" }));
        assert_eq!(n[2]["expr"], "items");
        assert_eq!(n[2]["track"], "item.id");
        assert_eq!(n[2]["empty"][0]["name"], "b");
        assert_eq!(n[3]["expr"], "mode");
        assert_eq!(n[3]["groups"][0]["cases"][0], "'a'");
        assert_eq!(n[3]["groups"][1]["cases"][0], Value::Null);
    }

    #[test]
    fn a_parse_error_is_reported_with_its_line() {
        let result = parse_template_structure("<div>\n</span>", "test.html");
        assert_eq!(result["nodes"], json!([]));
        assert_eq!(result["errors"][0]["line"], 2);
    }
}
