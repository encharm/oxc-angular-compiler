//! `packages/compiler-cli/src/ngtsc/typecheck/src/tcb_util.ts`, the part that maps a position in a type-check block
//! back to its template: `findSourceLocation` and `getTypeCheckId`, over oxc's parse of the block (TypeScript's
//! `getTokenAtPosition` and `node.parent` become the chain of nodes containing the position).

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use oxc_span::{GetSpan, SourceType};

use crate::ngtsc::comments::{has_ignore_for_diagnostics_marker, read_span_comment};
use crate::typecheck::ops::codegen::SpanInfo;

// tcb_util.ts SourceLocation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// The `TypeCheckId` (`tcb1`)
    pub id: String,
    /// The span comment's offsets, into the template's text (JavaScript string indices)
    pub span: SpanInfo,
}

/// A node of the chain: where it starts and ends (byte offsets into the block), and whether it is a function
/// declaration (the type-check function, where the search stops)
#[derive(Debug, Clone, Copy)]
pub struct ChainNode {
    pub start: usize,
    pub end: usize,
    pub is_function_declaration: bool,
}

/// A parsed type-check block, for locating diagnostics in it
pub struct TcbFile {
    pub text: String,
}

impl TcbFile {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    /// The nodes containing `pos` (a byte offset), from the outermost to the innermost: TypeScript's
    /// `getTokenAtPosition(file, pos)` and its parents, in reverse
    pub fn chain_at(&self, pos: usize) -> Vec<ChainNode> {
        let allocator = Allocator::default();
        let ret = oxc_parser::Parser::new(&allocator, &self.text, SourceType::ts()).parse();
        let mut collector = Collector { pos: pos as u32, nodes: vec![] };
        collector.visit_program(&ret.program);
        // outer before inner: longer spans first, and the earlier entered of equal spans
        collector.nodes.sort_by_key(|(order, start, end, _)| (std::cmp::Reverse(end - start), *order));
        collector
            .nodes
            .into_iter()
            .map(|(_, start, end, is_function_declaration)| ChainNode {
                start: start as usize,
                end: end as usize,
                is_function_declaration,
            })
            .collect()
    }

    // tcb_util.ts:204 findSourceLocation, from the token at `pos` (a byte offset into the block)
    pub fn find_source_location(&self, pos: usize, is_diagnostics_request: bool) -> Option<SourceLocation> {
        let chain = self.chain_at(pos);
        // innermost first, as TypeScript walks `node.parent`
        let mut index = chain.len().checked_sub(1)?;
        loop {
            let node = chain[index];
            if node.is_function_declaration {
                return None;
            }
            if has_ignore_for_diagnostics_marker(&self.text, node.end) && is_diagnostics_request {
                // There's an ignore marker on this node, so the diagnostic should not be reported.
                return None;
            }
            if let Some(span) = read_span_comment(&self.text, node.end) {
                // Once the positional information has been extracted, search further up the TCB to extract the
                // unique id that is attached with the TCB's function declaration.
                let id = self.get_type_check_id(&chain[..=index], is_diagnostics_request)?;
                return Some(SourceLocation { id, span });
            }
            index = index.checked_sub(1)?;
        }
    }

    // tcb_util.ts:233 getTypeCheckId: walks up to the function declaration of the TCB, whose leading comment is the
    // id; an ignore marker on the way drops the diagnostic
    fn get_type_check_id(&self, chain: &[ChainNode], is_diagnostic_request: bool) -> Option<String> {
        for node in chain.iter().rev() {
            if node.is_function_declaration {
                return preceding_comment(&self.text, node.start).map(str::to_string);
            }
            if has_ignore_for_diagnostics_marker(&self.text, node.end) && is_diagnostic_request {
                return None;
            }
        }
        None
    }
}

/// The text of the `/*...*/` comment just before `start` (TypeScript's `forEachLeadingCommentRange` at the
/// function's full start, whose first comment is the id)
fn preceding_comment(text: &str, start: usize) -> Option<&str> {
    let before = text[..start].trim_end();
    let body = before.strip_suffix("*/")?;
    let open = body.rfind("/*")?;
    Some(&body[open + 2..])
}

struct Collector {
    pos: u32,
    /// (order entered, start, end, is a function declaration)
    nodes: Vec<(usize, u32, u32, bool)>,
}

impl<'a> Visit<'a> for Collector {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        let span = kind.span();
        if span.start <= self.pos && self.pos < span.end {
            let is_function_declaration = matches!(kind, AstKind::Function(f) if f.is_declaration());
            self.nodes.push((self.nodes.len(), span.start, span.end, is_function_declaration));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCB: &str = "/*tcb1*/\nfunction _tcb1(this: i0.C) {\nif (true) {\n_t6.addEventListener /*415,420*/(\"click\", ($event /*T:EP*/): any => { if ((_t5 /*287,298*/) && ((((_t5 /*302,313*/() /*302,315*/).length /*316,322*/) /*302,322*/) > (0 /*325,326*/) /*302,326*/) /*D:ignore*/ /*287,326*/) {\n}});\n\"\" + (((this).count /*158,163*/() /*158,165*/).toUpperCase /*166,177*/() /*158,179*/);\n}\n}\n";

    #[test]
    fn a_span_under_an_ignored_expression_is_dropped() {
        let file = TcbFile::new(TCB.to_string());
        let pos = TCB.find("_t5 /*287").unwrap();
        assert_eq!(file.find_source_location(pos, true), None);
        // outside diagnostics requests, the span is found
        let found = file.find_source_location(pos, false).unwrap();
        assert_eq!((found.id.as_str(), found.span), ("tcb1", SpanInfo { start: 287, end: 298 }));
    }

    #[test]
    fn a_property_read_maps_to_its_span() {
        let file = TcbFile::new(TCB.to_string());
        let pos = TCB.find("toUpperCase").unwrap();
        let found = file.find_source_location(pos, true).unwrap();
        assert_eq!((found.id.as_str(), found.span), ("tcb1", SpanInfo { start: 166, end: 177 }));
    }
}
