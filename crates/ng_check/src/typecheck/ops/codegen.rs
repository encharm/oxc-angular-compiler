//! `packages/compiler/src/typecheck/ops/codegen.ts`

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
#[derive(Debug, Clone)]
pub struct TcbExpr {
    source: String,
    span_comment: Option<String>,
    identifier_comment: Option<String>,
    ignore_comment: Option<String>,
}

impl TcbExpr {
    // codegen.ts:27 constructor
    pub fn new(source: impl Into<String>) -> Self {
        Self { source: source.into(), span_comment: None, identifier_comment: None, ignore_comment: None }
    }

    // codegen.ts:30 quoteAndEscape: `JSON.stringify(value)`
    pub fn quote_and_escape(value: &str) -> String {
        serde_json::to_string(value).expect("a string serializes")
    }

    // codegen.ts:40 print
    pub fn print(&self, ignore_comments: bool) -> String {
        if ignore_comments {
            return self.source.clone();
        }
        let mut out = self.source.clone();
        out.push_str(&format_comment(self.identifier_comment.as_deref()));
        out.push_str(&format_comment(self.ignore_comment.as_deref()));
        out.push_str(&format_comment(self.span_comment.as_deref()));
        out
    }

    /// `print()`
    pub fn p(&self) -> String {
        self.print(false)
    }

    // codegen.ts:57 addParseSpanInfo; offsets as JavaScript string indices (`crate::span`)
    pub fn add_parse_span_info(mut self, span: impl Into<SpanInfo>) -> Self {
        let span = span.into();
        self.span_comment =
            Some(format!("{},{}", crate::span::utf16(span.start), crate::span::utf16(span.end)));
        self
    }

    // codegen.ts:74 markIgnoreDiagnostics
    pub fn mark_ignore_diagnostics(mut self) -> Self {
        self.ignore_comment = Some(format!("{}:ignore", CommentTriviaType::Diagnostic.as_str()));
        self
    }

    // codegen.ts:83 wrapForTypeChecker
    pub fn wrap_for_type_checker(mut self) -> Self {
        self.source = format!("({})", self.p());
        self.span_comment = None;
        self.identifier_comment = None;
        self.ignore_comment = None;
        self
    }

    // codegen.ts:93 addExpressionIdentifier
    pub fn add_expression_identifier(mut self, identifier: ExpressionIdentifier, id: Option<u32>) -> Self {
        let suffix = id.map(|id| format!(":{id}")).unwrap_or_default();
        self.identifier_comment = Some(format!(
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
