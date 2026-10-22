//! `packages/compiler-cli/preprocessor/src/dependency_templates.ts`: fills in template-derived metadata of the
//! components a template depends on.
//!
//! The analyzer does not parse templates, so the only `DeclarationMetadata` that carry a
//! component's `ngContentSelectors` are those it read off a `.d.ts`. For a component declared in a source file of the
//! compilation they are missing, and the type-check block then cannot tell whether a control flow
//! block keeps a node out of the slot it targets (`controlFlowPreventingContentProjection`,
//! NG8011). They are recovered from the dependency's own parsed template, parsed at most once: it is cached in the
//! dependency file's `FileAnalysis.parsedTemplates` under the same key the processing of that file uses.
//!
//! Lookups are synchronous here (`IAnalyzer`, implemented by ng-analyze's `Analyzer`); `prefetch` is TypeScript's
//! asynchronous read-ahead, kept for its caching. Dependency files' contents and templates go into the allocator
//! (`'a`) of the file cache.

use std::cell::RefCell;
use std::rc::Rc;

use ng_analyze::{AnalysisResult, Analyzer, ClassMetadata, DeclarationMetadata};
use oxc_allocator::Allocator;

use crate::ngp::compiler_utils::{TemplateParseOptions, get_or_parse_component_template};
use crate::ngp::file_analysis::{FileCache, ParsedComponentTemplate, TemplatesByClass, get_or_create_file_analysis};
use crate::typecheck::api::Map;

// hybrid_compiler.ts IAnalyzer: the synchronous lookups `DependencyTemplates` makes. An `Err` is a lookup that threw.
pub trait IAnalyzer {
    fn get_metadata_for_file_sync(&self, file_path: &str) -> Result<Option<AnalysisResult>, String>;
    fn get_file_content_sync(&self, file_path: &str) -> Result<String, String>;
}

impl IAnalyzer for Analyzer {
    fn get_metadata_for_file_sync(&self, file_path: &str) -> Result<Option<AnalysisResult>, String> {
        Ok(self.get_metadata_for_file(file_path.to_string()))
    }

    fn get_file_content_sync(&self, file_path: &str) -> Result<String, String> {
        self.get_file_content_core(file_path.to_string())
    }
}

// dependency_templates.ts:21 DependencyTemplateConsumer: the file whose component's dependencies are being resolved.
// `content` is in the allocator.
pub struct DependencyTemplateConsumer<'a, 'c> {
    pub file_path: &'c str,
    pub content: &'a str,
    pub classes: &'c [ClassMetadata],
    pub templates_by_class: TemplatesByClass<'a>,
}

// dependency_templates.ts:33 ClassId: identifies a class within its declaring file. The name alone is not enough:
// same-named classes can be declared in different scopes of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ClassId {
    name: String,
    name_start: u32,
}

// dependency_templates.ts:42 MissingTemplates: component classes whose template is not parsed yet, grouped by the
// file declaring them and keyed by the start of their name span.
type MissingTemplates = Map<String, Map<u32, ClassId>>;

// dependency_templates.ts:62 DependencyTemplates
pub struct DependencyTemplates<'a, 'c> {
    allocator: &'a Allocator,
    analyzer: &'c dyn IAnalyzer,
    file_cache: &'c RefCell<FileCache<'a>>,
    template_parse_options: Option<&'c TemplateParseOptions>,
    /// Metadata already at hand (e.g. the files of the chunk being processed), preferred to a round trip to the
    /// analyzer.
    known_metadata: Option<&'c Map<String, AnalysisResult>>,
}

impl<'a, 'c> DependencyTemplates<'a, 'c> {
    // dependency_templates.ts:67 constructor (and the allocator the cache's templates live in)
    pub fn new(
        allocator: &'a Allocator,
        analyzer: &'c dyn IAnalyzer,
        file_cache: &'c RefCell<FileCache<'a>>,
        template_parse_options: Option<&'c TemplateParseOptions>,
        known_metadata: Option<&'c Map<String, AnalysisResult>>,
    ) -> Self {
        Self { allocator, analyzer, file_cache, template_parse_options, known_metadata }
    }

    // dependency_templates.ts:79 withNgContentSelectors: `declarations` with `ngContentSelectors` filled in for every
    // source-file component that lacks them.
    pub fn with_ng_content_selectors(
        &self,
        declarations: &[DeclarationMetadata],
        consumer: &DependencyTemplateConsumer<'a, '_>,
    ) -> Vec<DeclarationMetadata> {
        let mut missing = MissingTemplates::default();
        self.collect_missing(declarations, consumer.file_path, &mut missing);
        for (file_path, class_ids) in &missing {
            // A dependency whose lookup fails gets no selectors rather than failing the whole file.
            let metadata = match self.known_metadata.and_then(|known| known.get(file_path)) {
                Some(metadata) => Some(metadata.clone()),
                None => safe_lookup(|| self.analyzer.get_metadata_for_file_sync(file_path)).flatten(),
            };
            let content = metadata.as_ref().and_then(|_| safe_lookup(|| self.analyzer.get_file_content_sync(file_path)));
            if let (Some(metadata), Some(content)) = (metadata, content) {
                self.parse_templates(file_path, &metadata.classes, self.allocator.alloc_str(&content), class_ids.values());
            }
        }

        let mut result = declarations.to_vec();
        for (i, decl) in declarations.iter().enumerate() {
            if !needs_ng_content_selectors(decl) {
                continue;
            }
            let Some(parsed) = self.get_template(decl, consumer) else {
                continue;
            };
            result[i] = DeclarationMetadata { ng_content_selectors: Some(parsed.ng_content_selectors.clone()), ..decl.clone() };
        }
        result
    }

    // dependency_templates.ts:120 prefetch: parses and caches the templates of the cross-file component dependencies
    // of every component in `consumer` that `withNgContentSelectors` would otherwise have to read. (TypeScript reads
    // the files concurrently; here they are read in order.)
    pub fn prefetch(&self, consumer: &AnalysisResult) {
        let mut missing = MissingTemplates::default();
        for cls in &consumer.classes {
            let declarations =
                cls.component.as_ref().and_then(|c| c.resolved_declarations.as_deref()).unwrap_or(&[]);
            self.collect_missing(declarations, &consumer.file_path, &mut missing);
        }
        for (file_path, class_ids) in &missing {
            let metadata = match self.known_metadata.and_then(|known| known.get(file_path)) {
                Some(metadata) => Some(metadata.clone()),
                None => safe_lookup(|| self.analyzer.get_metadata_for_file_sync(file_path)).flatten(),
            };
            let Some(metadata) = metadata else { continue };
            let Some(content) = safe_lookup(|| self.analyzer.get_file_content_sync(file_path)) else { continue };
            self.parse_templates(file_path, &metadata.classes, self.allocator.alloc_str(&content), class_ids.values());
        }
    }

    // dependency_templates.ts:143 collectMissing: adds to `missing` the components among `declarations` that are
    // declared in a file other than `consumerPath` and whose template is not cached yet.
    fn collect_missing(&self, declarations: &[DeclarationMetadata], consumer_path: &str, missing: &mut MissingTemplates) {
        for decl in declarations {
            let file_path = decl.file_path.as_deref().unwrap_or(consumer_path);
            if !needs_ng_content_selectors(decl) || file_path == consumer_path {
                continue;
            }
            let id = class_id_of(decl);
            if find_parsed_template(&self.get_parsed_templates(file_path), &id).is_some() {
                continue;
            }
            missing.entry(file_path.to_string()).or_default().insert(id.name_start, id);
        }
    }

    // dependency_templates.ts:167 parseTemplates: parses the templates of `classIds` into `filePath`'s cache, if not
    // there already
    fn parse_templates<'i>(
        &self,
        file_path: &str,
        classes: &[ClassMetadata],
        content: &'a str,
        class_ids: impl Iterator<Item = &'i ClassId>,
    ) {
        let templates_by_class = self.get_parsed_templates(file_path);
        for id in class_ids {
            self.get_or_parse_template(id, classes, file_path, content, &templates_by_class);
        }
    }

    // dependency_templates.ts:179 getTemplate
    fn get_template(
        &self,
        decl: &DeclarationMetadata,
        consumer: &DependencyTemplateConsumer<'a, '_>,
    ) -> Option<Rc<ParsedComponentTemplate<'a>>> {
        let file_path = decl.file_path.as_deref().unwrap_or(consumer.file_path);
        let id = class_id_of(decl);
        if file_path == consumer.file_path {
            return self.get_or_parse_template(
                &id,
                consumer.classes,
                consumer.file_path,
                consumer.content,
                &consumer.templates_by_class,
            );
        }
        find_parsed_template(&self.get_parsed_templates(file_path), &id)
    }

    // dependency_templates.ts:197 getParsedTemplates
    fn get_parsed_templates(&self, file_path: &str) -> TemplatesByClass<'a> {
        Rc::clone(&get_or_create_file_analysis(&mut self.file_cache.borrow_mut(), file_path).parsed_templates)
    }

    // dependency_templates.ts:201 getOrParseTemplate
    fn get_or_parse_template(
        &self,
        id: &ClassId,
        classes: &[ClassMetadata],
        file_path: &str,
        content: &'a str,
        templates_by_class: &TemplatesByClass<'a>,
    ) -> Option<Rc<ParsedComponentTemplate<'a>>> {
        let class_meta = classes.iter().find(|c| {
            c.name_span.map(|span| span.start) == Some(id.name_start)
                && c.class_name.as_deref() == Some(id.name.as_str())
                && c.component.is_some()
        })?;
        let component = class_meta.component.as_ref()?;
        Some(get_or_parse_component_template(
            self.allocator,
            templates_by_class,
            class_meta,
            component,
            file_path,
            content,
            self.template_parse_options,
        ))
    }
}

// dependency_templates.ts:225 needsNgContentSelectors
fn needs_ng_content_selectors(decl: &DeclarationMetadata) -> bool {
    decl.declaration_type == "component" && decl.ng_content_selectors.is_none()
}

// dependency_templates.ts:229 classIdOf
fn class_id_of(decl: &DeclarationMetadata) -> ClassId {
    ClassId { name: decl.name.clone(), name_start: decl.name_span.start }
}

// dependency_templates.ts:239 findParsedTemplate: the cached template of the class `id`, matched on the name span
// start recorded when it was cached (the cache is keyed by the class span, not known before the declaring file's
// metadata is).
fn find_parsed_template<'a>(
    templates_by_class: &TemplatesByClass<'a>,
    id: &ClassId,
) -> Option<Rc<ParsedComponentTemplate<'a>>> {
    let prefix = format!("{}@", id.name);
    templates_by_class
        .borrow()
        .iter()
        .find(|(key, parsed)| key.starts_with(&prefix) && parsed.class_name_start == Some(id.name_start))
        .map(|(_, parsed)| Rc::clone(parsed))
}

// dependency_templates.ts:252 safeLookup
fn safe_lookup<T>(lookup: impl FnOnce() -> Result<T, String>) -> Option<T> {
    lookup().ok()
}
