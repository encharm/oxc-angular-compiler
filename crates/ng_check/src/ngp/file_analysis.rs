//! `packages/compiler-cli/preprocessor/src/file_analysis.ts`: what is cached per file across the files of a
//! compilation (the parsed component templates).
//!
//! The parsed templates live in the `oxc_allocator::Allocator` (`'a`) that parsed them: whoever owns a `FileCache`
//! owns that allocator too, and keeps it alive as long as the cache (`processor.rs`).

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use oxc_angular_compiler::ast::r3::R3Node;

use crate::ngp::compiler_utils::ParsedTemplate;
use crate::typecheck::api::Map;

// file_analysis.ts:14 ParsedComponentTemplate
#[derive(Debug)]
pub struct ParsedComponentTemplate<'a> {
    /// `extends ParsedTemplate`
    pub parsed: ParsedTemplate<'a>,
    /// Nodes of the template parsed with whitespace, line endings, and leading trivia preserved,
    /// for accurate source spans in type-checking, diagnostics, and indexing.
    pub diag_nodes: &'a [R3Node<'a>],
    /// Start of the component class's name span, recorded when the template is cached in
    /// `FileAnalysis.parsedTemplates`. Lets a template be found from a `DeclarationMetadata`, which
    /// carries the class's name span but not the class span the cache is keyed by.
    pub class_name_start: Option<u32>,
}

impl<'a> Deref for ParsedComponentTemplate<'a> {
    type Target = ParsedTemplate<'a>;

    fn deref(&self) -> &ParsedTemplate<'a> {
        &self.parsed
    }
}

/// `FileAnalysis.parsedTemplates`: shared, so that the processing of a file and the dependency templates of other
/// files can both add to it (TypeScript shares the `Map`)
pub type TemplatesByClass<'a> = Rc<RefCell<Map<String, Rc<ParsedComponentTemplate<'a>>>>>;

// file_analysis.ts:28 FileAnalysis. (`preparedTcbData`, which the language service reads, is not kept.)
#[derive(Debug)]
pub struct FileAnalysis<'a> {
    pub file_path: String,
    pub tcb: Option<String>,
    pub parsed_templates: TemplatesByClass<'a>,
}

/// `Map<string, FileAnalysis>`, the cache `getOrCreateFileAnalysis` reads
pub type FileCache<'a> = Map<String, FileAnalysis<'a>>;

// file_analysis.ts:35 ChunkContext: emitting only (not ported).

// file_analysis.ts:47 getOrCreateFileAnalysis
pub fn get_or_create_file_analysis<'c, 'a>(cache: &'c mut FileCache<'a>, file_path: &str) -> &'c mut FileAnalysis<'a> {
    cache.entry(file_path.to_string()).or_insert_with(|| FileAnalysis {
        file_path: file_path.to_string(),
        tcb: None,
        parsed_templates: Rc::default(),
    })
}
