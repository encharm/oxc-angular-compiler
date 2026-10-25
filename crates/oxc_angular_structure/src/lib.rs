//! Angular templates as data, for tools that read templates without compiling them: the R3 tree the compiler builds,
//! with every expression as a tree and as its source text, written as JSON (`schema`, declared for TypeScript in
//! `structure.d.ts`).
//!
//! A template is parsed as Angular parses it for type checking (whitespace, line endings and leading trivia kept, so
//! positions are exact). [`parse_template`] reads one template on its own; a caller that has bound the template to its
//! component's declarations (ng-check) writes the same tree through a [`Binder`], which adds what binding knows: the
//! directives matched on each element, what each reference and template name resolves to, and which directive input a
//! binding sets. [`class_members`] reads a component class's members and their JSDoc.

mod members;
mod positions;
pub mod schema;
mod writer;

use oxc_allocator::Allocator;
use oxc_angular_compiler::parser::ParseTemplateOptions;
use oxc_angular_compiler::parser::html::HtmlParser;
use oxc_angular_compiler::transform::HtmlToR3Transform;
use oxc_angular_compiler::transform::html_to_r3::TransformOptions;

pub use members::class_members;
pub use positions::Positions;
pub use schema::SCHEMA_VERSION;
pub use writer::{
    Binder, BindingRef, BoundEntity, BoundReference, NoBinder, Owner, WriteOptions, Writer,
};

use schema::{TemplateError, TemplateStructure};

/// The options Angular's type checking parses a template with (ngtsc's `diagNodes`): every character keeps its
/// position.
pub fn parse_options() -> ParseTemplateOptions {
    ParseTemplateOptions {
        preserve_whitespaces: true,
        preserve_line_endings: true,
        preserve_significant_whitespace: true,
        enable_block_syntax: true,
        enable_let_syntax: true,
        tokenize_expansion_forms: true,
        leading_trivia_chars: Some(vec![]),
        ..ParseTemplateOptions::default()
    }
}

/// One template, parsed on its own: no binder, so no matches and no resolved names.
pub fn parse_template(template: &str, file_path: &str, options: WriteOptions) -> TemplateStructure {
    let allocator = Allocator::default();
    let html = HtmlParser::with_options(&allocator, template, file_path, &parse_options()).parse();
    let positions = Positions::new(template);
    let error = |message: &str, start: u32, end: u32| TemplateError {
        message: message.to_string(),
        position: positions.of(start, end),
    };
    if !html.errors.is_empty() {
        let errors = html
            .errors
            .iter()
            .map(|e| error(&e.msg, e.span.start.offset, e.span.end.offset))
            .collect();
        return TemplateStructure { version: SCHEMA_VERSION, nodes: vec![], errors };
    }
    let r3 = HtmlToR3Transform::new(&allocator, template, TransformOptions::default())
        .transform(&html.nodes);
    if !r3.errors.is_empty() {
        let errors = r3
            .errors
            .iter()
            .map(|e| error(&e.msg, e.span.start.offset, e.span.end.offset))
            .collect();
        return TemplateStructure { version: SCHEMA_VERSION, nodes: vec![], errors };
    }
    // the nodes live as long as the allocator, as a binder's references do
    let nodes = allocator.alloc(r3.nodes);
    let nodes = Writer::new(template, &NoBinder, options).write(nodes);
    TemplateStructure { version: SCHEMA_VERSION, nodes, errors: vec![] }
}
