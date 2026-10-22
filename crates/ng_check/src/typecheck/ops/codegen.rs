//! `packages/compiler/src/typecheck/ops/codegen.ts`

use std::cell::RefCell;
use std::rc::Rc;

use oxc_angular_compiler::ast::expression::AbsoluteSourceSpan;
use oxc_span::Span;

use crate::typecheck::comments::{CommentTriviaType, ExpressionIdentifier};

/// A parse span, from either kind of node (`AbsoluteSourceSpan | ParseSourceSpan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanInfo {
    pub start: u32,
    pub end: u32,
}

impl From<AbsoluteSourceSpan> for SpanInfo {
    fn from(span: AbsoluteSourceSpan) -> Self {
        Self { start: span.start, end: span.end }
    }
}

impl From<Span> for SpanInfo {
    fn from(span: Span) -> Self {
        Self { start: span.start, end: span.end }
    }
}

// codegen.ts:14 TcbExpr: an expression generated within a type check block
///
/// A shared handle, as a TypeScript object is: a clone is the same expression, and the methods change it for every
/// holder. TypeScript relies on that: `Scope.resolve` hands out an op's cached result, which `addParseSpanInfo` and
/// `wrapForTypeChecker` then change, so a later use of the same variable prints what earlier ones did to it.
#[derive(Debug, Clone)]
pub struct TcbExpr(Rc<RefCell<TcbExprState>>);

#[derive(Debug)]
struct TcbExprState {
    source: String,
    span_comment: Option<String>,
    identifier_comment: Option<String>,
    ignore_comment: Option<String>,
}

impl TcbExpr {
    // codegen.ts:27 constructor
    pub fn new(source: impl Into<String>) -> Self {
        Self(Rc::new(RefCell::new(TcbExprState {
            source: source.into(),
            span_comment: None,
            identifier_comment: None,
            ignore_comment: None,
        })))
    }

    // codegen.ts:30 quoteAndEscape: `JSON.stringify(value)`
    pub fn quote_and_escape(value: &str) -> String {
        serde_json::to_string(value).expect("a string serializes")
    }

    // codegen.ts:40 print
    pub fn print(&self, ignore_comments: bool) -> String {
        let state = self.0.borrow();
        if ignore_comments {
            return state.source.clone();
        }
        let mut out = state.source.clone();
        out.push_str(&format_comment(state.identifier_comment.as_deref()));
        out.push_str(&format_comment(state.ignore_comment.as_deref()));
        out.push_str(&format_comment(state.span_comment.as_deref()));
        out
    }

    /// `print()`
    pub fn p(&self) -> String {
        self.print(false)
    }

    // codegen.ts:57 addParseSpanInfo; offsets as JavaScript string indices (`crate::span`)
    pub fn add_parse_span_info(self, span: impl Into<SpanInfo>) -> Self {
        let span = span.into();
        self.0.borrow_mut().span_comment =
            Some(format!("{},{}", crate::span::utf16(span.start), crate::span::utf16(span.end)));
        self
    }

    // codegen.ts:74 markIgnoreDiagnostics
    pub fn mark_ignore_diagnostics(self) -> Self {
        self.0.borrow_mut().ignore_comment = Some(format!("{}:ignore", CommentTriviaType::Diagnostic.as_str()));
        self
    }

    // codegen.ts:83 wrapForTypeChecker
    pub fn wrap_for_type_checker(self) -> Self {
        let source = format!("({})", self.p());
        let mut state = self.0.borrow_mut();
        state.source = source;
        state.span_comment = None;
        state.identifier_comment = None;
        state.ignore_comment = None;
        drop(state);
        self
    }

    // codegen.ts:93 addExpressionIdentifier
    pub fn add_expression_identifier(self, identifier: ExpressionIdentifier, id: Option<u32>) -> Self {
        let suffix = id.map(|id| format!(":{id}")).unwrap_or_default();
        self.0.borrow_mut().identifier_comment = Some(format!(
            "{}:{}{suffix}",
            CommentTriviaType::ExpressionTypeIdentifier.as_str(),
            identifier.as_str()
        ));
        self
    }
}

// codegen.ts:111 formatComment
fn format_comment(content: Option<&str>) -> String {
    match content {
        Some(content) if !content.is_empty() => format!(" /*{content}*/"),
        _ => String::new(),
    }
}

// codegen.ts:121 declareVariable
pub fn declare_variable(identifier: &TcbExpr, r#type: TcbExpr) -> TcbExpr {
    let r#type = r#type.add_expression_identifier(ExpressionIdentifier::VariableAsExpression, None);
    TcbExpr::new(format!("var {} = null! as {}", identifier.p(), r#type.p()))
}

// codegen.ts:131 getStatementsBlock
pub fn get_statements_block(expressions: &[TcbExpr], single_line: bool) -> String {
    let mut result = String::new();
    for expr in expressions {
        result.push_str(&expr.p());
        result.push(';');
        result.push_str(if single_line { " " } else { "\n" });
    }
    result
}
