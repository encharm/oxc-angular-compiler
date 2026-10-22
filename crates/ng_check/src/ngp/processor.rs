//! `packages/compiler-cli/preprocessor/src/processor.ts`, the type-checking part: `buildTcbTargets`, the type-check
//! block step of `processFileSync` (targets, `prepareTcbTargets`, `generateTcbCode`, `combineTcbContent`, the
//! diagnostics, `templateDiagnosticLocation`), the template parse errors it reports, and what `processFile` reads
//! before it for type checking. Everything about emitting compiled code is not ported.
//!
//! # API
//!
//! [`type_check_file`] generates the type-check file of one source file:
//!
//! - in: a [`TypeCheckContext`] (the allocator, ng-analyze's `Analyzer` or another [`IAnalyzer`] to read other
//!   files' metadata and text, the type-checking config from [`build_type_checking_config`], the file cache, the
//!   template parse options), the file's path, its text, and ng-analyze's `AnalysisResult` for it;
//! - out: a [`TypeCheckFileResult`]: the type-check file's text as `combineTcbContent` makes it (header, imports,
//!   blocks, and the `ngp-tcb-template-sources` footer), or none when the file has nothing to check; its template
//!   sources (`TypeCheckId` to template file or `template:` expression span, `TcbTemplateSource`); and the
//!   diagnostics found while generating (template parse errors 5002, out-of-band, DOM schema and template semantics
//!   diagnostics, generation failures 8000) as ng-analyze `NgDiagnostic`s: category (0 warning, 1 error), code,
//!   message, file, and span.
//!
//! Offsets: every offset in and out is a JavaScript string (UTF-16) index, as in TypeScript and ng-analyze: the span
//! comments of the type-check blocks, the template sources, and the diagnostics' spans. A diagnostic's span indexes
//! into its `file_path` (the template file for an external template; for an inline template computed by an
//! expression, the span is that expression's in the component's file).
//!
//! Ownership: the parsed templates live in the caller's `oxc_allocator::Allocator` (`TypeCheckContext::allocator`),
//! cached in `TypeCheckContext::file_cache` (a template is parsed once, whichever file needs it first). Keep the
//! allocator and the cache together: one pair for a whole run lets files share their dependencies' templates; a fresh
//! pair per file frees each file's templates after it.

use std::cell::RefCell;

use ng_analyze::{AnalysisResult, ClassMetadata, ComponentMetadata, NgDiagnostic, SpanMetadata};
use oxc_allocator::Allocator;

use crate::angular::core::{CUSTOM_ELEMENTS_SCHEMA, NO_ERRORS_SCHEMA, SchemaMetadata};
use crate::ngp::compiler_utils::{
    ComponentOrDirectiveMetadata, TemplateParseOptions, build_pipe_registry, get_or_parse_component_template,
    make_class_key, metadata_to_declaration,
};
use crate::ngp::dependency_templates::{DependencyTemplateConsumer, DependencyTemplates, IAnalyzer};
use crate::ngp::selectorless::analyze_template_for_selectorless;
use crate::ngp::file_analysis::{FileCache, TemplatesByClass, get_or_create_file_analysis};
use crate::ngp::tcb::{
    NgpTypeCheckingConfig, TcbResult, TcbTargetInput, TcbTemplateSource, combine_tcb_content, generate_tcb_code,
    prepare_tcb_targets,
};
use crate::ngp::tcb_util::byte_to_utf16_offset;
use crate::typecheck::api::{Map, TypeCheckId};
use crate::typecheck::oob::OutOfBandDiagnosticCategory;

#[cfg(doc)]
use crate::ngp::tcb::build_type_checking_config;

/// processor.ts:104 HybridCompilerContext, the part type checking uses (`optimize` is on and `emitDeclarationOnly`
/// off: type checking is what is asked for)
pub struct TypeCheckContext<'a, 'c> {
    /// Where templates are parsed into
    pub allocator: &'a Allocator,
    pub analyzer: &'c dyn IAnalyzer,
    pub tcb_config: &'c NgpTypeCheckingConfig,
    pub file_cache: &'c RefCell<FileCache<'a>>,
    pub template_parse_options: Option<&'c TemplateParseOptions>,
    pub metadata_map: Option<&'c Map<String, AnalysisResult>>,
}

/// What `type_check_file` returns (see the top of this file)
#[derive(Debug, Clone)]
pub struct TypeCheckFileResult {
    /// The type-check file's text (`ProcessedFile.tcb.code`), none when the file has nothing to type-check
    pub text: Option<String>,
    /// `TcbResult.templateSources`
    pub template_sources: Map<TypeCheckId, TcbTemplateSource>,
    /// `ProcessedFile.diagnostics`, the type-checking ones
    pub diagnostics: Vec<NgDiagnostic>,
    /// The rest of the generation's result (`code` is the blocks without the header and footer)
    pub tcb: Option<TcbResult>,
}

// processor.ts:181 buildTcbTargets
pub fn build_tcb_targets<'a>(
    allocator: &'a Allocator,
    file_path: &str,
    content: &'a str,
    classes: &[ClassMetadata],
    templates_by_class: &TemplatesByClass<'a>,
    options: Option<&TemplateParseOptions>,
    tcb_config: &NgpTypeCheckingConfig,
    dependency_templates: &DependencyTemplates<'a, '_>,
) -> Vec<TcbTargetInput<'a>> {
    let mut tcb_targets = vec![];
    let consumer = DependencyTemplateConsumer {
        file_path,
        content,
        classes,
        templates_by_class: std::rc::Rc::clone(templates_by_class),
    };
    let default_options = TemplateParseOptions::default();
    let options_or_default = options.unwrap_or(&default_options);

    for cls in classes {
        let Some(class_name) = cls.class_name.as_ref().filter(|name| !name.is_empty()) else {
            continue;
        };

        if let Some(component) = &cls.component {
            if component.is_jit || (component.is_poisoned && tcb_config.use_poisoned_data != Some(true)) {
                continue;
            }

            // 1. Parse template (or reuse if already parsed in processFile)
            let parsed = get_or_parse_component_template(
                allocator,
                templates_by_class,
                cls,
                component,
                file_path,
                content,
                options,
            );

            let mut parsed_schemas: Vec<SchemaMetadata> = vec![];
            if let Some(schemas) = &component.schemas {
                for schema in schemas {
                    if schema == "CUSTOM_ELEMENTS_SCHEMA" {
                        parsed_schemas.push(SchemaMetadata { name: CUSTOM_ELEMENTS_SCHEMA.to_string() });
                    } else if schema == "NO_ERRORS_SCHEMA" {
                        parsed_schemas.push(SchemaMetadata { name: NO_ERRORS_SCHEMA.to_string() });
                    }
                }
            }

            let mut selectorless_enabled = false;
            if options_or_default.enable_selectorless.unwrap_or(false) {
                let analysis = analyze_template_for_selectorless(parsed.nodes);
                selectorless_enabled = analysis.is_selectorless;
            }

            let resolved_declarations = component.resolved_declarations.as_deref().unwrap_or(&[]);
            let host_bindings_checked = tcb_config.type_check_host_bindings != Some(false);
            // 9. Collect component for TCB generation (optimize mode only)
            tcb_targets.push(TcbTargetInput {
                class_meta: cls.clone(),
                class_name: class_name.clone(),
                template: component.template.clone().unwrap_or_default(),
                template_nodes: parsed.diag_nodes,
                template_source: parsed.source,
                declarations: dependency_templates.with_ng_content_selectors(resolved_declarations, &consumer),
                enable_selectorless: Some(options_or_default.enable_selectorless.unwrap_or(false)),
                selectorless_enabled: Some(selectorless_enabled),
                own_declaration: metadata_to_declaration(
                    ComponentOrDirectiveMetadata::Component(component),
                    cls,
                    "component",
                    Some(parsed.ng_content_selectors.clone()),
                    0,
                ),
                host_properties: if host_bindings_checked { component.host_properties.clone() } else { vec![] },
                host_bindings: if host_bindings_checked { cls.host_bindings.clone() } else { vec![] },
                host_listeners: if host_bindings_checked { cls.host_listeners.clone() } else { vec![] },
                is_standalone: component.standalone,
                preserve_whitespaces: parsed.preserve_whitespaces,
                schemas: parsed_schemas,
                type_parameters: cls.type_parameters.clone(),
                pipe_registry: Some(build_pipe_registry(resolved_declarations)),
                foreign_imports: component.foreign_imports.clone(),
            });
        } else if let Some(directive) = &cls.directive {
            if directive.is_jit {
                continue;
            }
            if tcb_config.type_check_host_bindings == Some(false) {
                continue;
            }
            let has_host_bindings = !directive.host_properties.is_empty()
                || !cls.host_bindings.is_empty()
                || !cls.host_listeners.is_empty();

            if has_host_bindings {
                tcb_targets.push(TcbTargetInput {
                    class_meta: cls.clone(),
                    class_name: class_name.clone(),
                    template: String::new(),
                    template_nodes: &[],
                    // a directive's spans (its host bindings') index into its file
                    template_source: content,
                    declarations: vec![],
                    own_declaration: metadata_to_declaration(
                        ComponentOrDirectiveMetadata::Directive(directive),
                        cls,
                        "directive",
                        None,
                        0,
                    ),
                    host_properties: directive.host_properties.clone(),
                    host_bindings: cls.host_bindings.clone(),
                    host_listeners: cls.host_listeners.clone(),
                    is_standalone: directive.standalone,
                    preserve_whitespaces: None,
                    schemas: vec![],
                    type_parameters: cls.type_parameters.clone(),
                    pipe_registry: None,
                    enable_selectorless: None,
                    selectorless_enabled: None,
                    foreign_imports: None,
                });
            }
        }
    }

    tcb_targets
}

/// processor.ts:312 processFile and processor.ts:356 processFileSync, their type-checking part: generates the
/// type-check file of `file_path` (see the top of this file).
pub fn type_check_file<'a>(
    ctx: &TypeCheckContext<'a, '_>,
    file_path: &str,
    content: &str,
    metadata: &AnalysisResult,
) -> TypeCheckFileResult {
    let content: &'a str = ctx.allocator.alloc_str(content);
    let classes = &metadata.classes;

    // processor.ts:323: parse every component's template up front (processFile also reads the stylesheets the
    // templates link, which type checking does not need)
    let templates_by_class =
        std::rc::Rc::clone(&get_or_create_file_analysis(&mut ctx.file_cache.borrow_mut(), file_path).parsed_templates);
    for class_meta in classes {
        let Some(component) = &class_meta.component else { continue };
        if class_meta.class_name.as_ref().is_none_or(String::is_empty) {
            continue;
        }
        get_or_parse_component_template(
            ctx.allocator,
            &templates_by_class,
            class_meta,
            component,
            file_path,
            content,
            ctx.template_parse_options,
        );
    }
    // processor.ts:342: templates of dependencies declared in other files are needed to bind this file's templates
    // for type-checking
    DependencyTemplates::new(ctx.allocator, ctx.analyzer, ctx.file_cache, ctx.template_parse_options, ctx.metadata_map)
        .prefetch(metadata);

    let mut file_diagnostics: Vec<NgDiagnostic> = vec![];

    // processor.ts:651: the template parse errors of the components processFileSync compiles
    for class_meta in classes {
        let Some(component) = &class_meta.component else { continue };
        if class_meta.class_name.as_ref().is_none_or(String::is_empty) {
            continue;
        }
        if component.is_jit || class_meta.directive.as_ref().is_some_and(|directive| directive.is_jit) {
            continue;
        }
        let parsed = get_or_parse_component_template(
            ctx.allocator,
            &templates_by_class,
            class_meta,
            component,
            file_path,
            content,
            ctx.template_parse_options,
        );
        if let Some(errors) = parsed.errors.as_ref().filter(|errors| !errors.is_empty()) {
            for error in errors {
                let span = &error.span;
                let start = byte_to_utf16_offset(parsed.source, span.start);
                let mut end = byte_to_utf16_offset(parsed.source, span.end);
                if start == end {
                    end += 1;
                }
                let (diagnostic_file_path, span) =
                    template_diagnostic_location(component, file_path, SpanMetadata { start, end });
                file_diagnostics.push(NgDiagnostic {
                    category: 1,
                    code: 5002,
                    message_text: error.msg.clone(),
                    file_path: Some(diagnostic_file_path),
                    span: Some(span),
                    next: vec![],
                    related_information: vec![],
                });
            }
        }
    }

    // processor.ts:1868
    let tcb_targets = build_tcb_targets(
        ctx.allocator,
        file_path,
        content,
        classes,
        &templates_by_class,
        ctx.template_parse_options,
        ctx.tcb_config,
        &DependencyTemplates::new(
            ctx.allocator,
            ctx.analyzer,
            ctx.file_cache,
            ctx.template_parse_options,
            ctx.metadata_map,
        ),
    );
    let prepared = if tcb_targets.is_empty() {
        None
    } else {
        prepare_tcb_targets(ctx.allocator, file_path, tcb_targets, content, ctx.tcb_config, Some(classes))
    };
    let tcb = prepared.as_ref().and_then(|prepared| generate_tcb_code(file_path, prepared, ctx.tcb_config));

    let text = tcb.as_ref().map(|tcb| combine_tcb_content(tcb, file_path, content, classes));
    if let Some(text) = &text {
        let mut cache = ctx.file_cache.borrow_mut();
        let analysis = get_or_create_file_analysis(&mut cache, file_path);
        analysis.tcb = Some(text.clone());
    }

    if let Some(tcb) = &tcb {
        for diag in &tcb.diagnostics {
            let mut component: Option<&ComponentMetadata> = None;
            if let Some(prepared) = &prepared {
                for (class_key, id) in &prepared.type_check_id_map {
                    if *id == diag.type_check_id {
                        let cls = classes
                            .iter()
                            .find(|c| make_class_key(c.class_name.as_deref(), c.span.start) == *class_key);
                        component = cls.and_then(|cls| cls.component.as_ref());
                        break;
                    }
                }
            }

            let span = SpanMetadata { start: diag.start, end: diag.end };
            let (diagnostic_file_path, span) = match component {
                Some(component) => template_diagnostic_location(component, file_path, span),
                None => (file_path.to_string(), span),
            };
            file_diagnostics.push(NgDiagnostic {
                category: u8::from(diag.category != OutOfBandDiagnosticCategory::Warning),
                code: diag.code.unwrap_or(8000),
                message_text: diag.message.clone(),
                file_path: Some(diagnostic_file_path),
                span: Some(span),
                next: vec![],
                related_information: vec![],
            });
        }
    }

    TypeCheckFileResult {
        text,
        template_sources: tcb.as_ref().map(|tcb| tcb.template_sources.clone()).unwrap_or_default(),
        diagnostics: file_diagnostics,
        tcb,
    }
}

// processor.ts:3221 templateDiagnosticLocation: where ngtsc's `makeTemplateDiagnostic` reports a template diagnostic
// for each kind of template source mapping: in the template file for an external template, and at the span itself
// for an inline literal (`direct`, whose spans are offsets into this file). An inline template computed by any other
// expression (`indirect`) has spans that are offsets into the resolved template string, not into any file on disk,
// so the diagnostic is anchored on the `template` expression instead.
fn template_diagnostic_location(
    component: &ComponentMetadata,
    file_path: &str,
    span: SpanMetadata,
) -> (String, SpanMetadata) {
    if let Some(template_url) = &component.template_url {
        return (template_url.resolved_path.clone(), span);
    }
    let expression_span = component.template_span;
    if component.template_content_span.is_some() {
        return (file_path.to_string(), span);
    }
    let Some(expression_span) = expression_span else {
        return (file_path.to_string(), span);
    };
    // TODO(parity): for an `indirect` template ngtsc reports the diagnostic at `span` inside a
    // synthetic `<file> (<ComponentName> template)` source file holding the resolved template, and
    // points related information ("Error occurs in the template of component …") at the
    // `template` expression. `NgDiagnostic` can carry neither a synthetic file nor related
    // information, so only the expression location is reported.
    (file_path.to_string(), expression_span)
}

#[cfg(test)]
mod tests {
    use ng_analyze::{DirectiveMetadata, UrlMetadata};

    use super::*;
    use crate::ngp::compiler_options::NgpCompilerOptions;
    use crate::ngp::tcb::build_type_checking_config;
    use crate::ngp::tcb::tests::class_meta;

    fn component(selector: &str, template: &str) -> ComponentMetadata {
        ComponentMetadata {
            decorator_name: None,
            selector: Some(selector.to_string()),
            imports: None,
            template: Some(template.to_string()),
            template_content_span: None,
            template_span: None,
            template_dynamic: false,
            template_url_span: None,
            template_url: None,
            styles: None,
            styles_from_urls: None,
            style_urls: None,
            standalone: true,
            signals: false,
            export_as: None,
            schemas: Some(vec!["CUSTOM_ELEMENTS_SCHEMA".to_string(), "OTHER".to_string()]),
            raw_imports_span: None,
            imports_factory_span: None,
            resolved_declarations: None,
            resolved_host_directives: None,
            local_compilation_extra_imports: None,
            host_properties: vec![],
            host_span: None,
            host_metadata: None,
            host_directives: None,
            providers_span: None,
            view_providers_span: None,
            encapsulation: None,
            change_detection: None,
            change_detection_span: None,
            preserve_whitespaces: None,
            animations_span: None,
            animation_trigger_names: None,
            args_span: None,
            preserved_decorator_properties: None,
            declaring_ng_module: None,
            foreign_imports: None,
            is_jit: false,
            is_poisoned: false,
            deferred_imports: None,
            deferred_imports_by_block: None,
            deferred_imports_span: None,
            resolved_deferred_declarations: None,
            resolved_deferred_declarations_by_block: None,
        }
    }

    fn analysis(file_path: &str, classes: Vec<ClassMetadata>) -> AnalysisResult {
        AnalysisResult {
            file_id: 0,
            file_path: file_path.to_string(),
            imports_end: 0,
            classes,
            imports: vec![],
            type_only_exports: vec![],
            diagnostics: vec![],
            signal_debug_names: vec![],
            r3_symbols_specifier: None,
            top_level_value_bindings: None,
        }
    }

    /// The files of a project, by path
    struct Files(Vec<(&'static str, &'static str, AnalysisResult)>);

    impl IAnalyzer for Files {
        fn get_metadata_for_file_sync(&self, file_path: &str) -> Result<Option<AnalysisResult>, String> {
            Ok(self.0.iter().find(|(path, ..)| *path == file_path).map(|(.., metadata)| metadata.clone()))
        }
        fn get_file_content_sync(&self, file_path: &str) -> Result<String, String> {
            self.0
                .iter()
                .find(|(path, ..)| *path == file_path)
                .map(|(_, content, _)| (*content).to_string())
                .ok_or_else(|| "missing".to_string())
        }
    }

    #[test]
    fn targets_of_components_and_host_bound_directives() {
        // a dependency in another file, whose `ngContentSelectors` come from its parsed template
        let dep_content = "class Dep {}";
        let mut dep_class = class_meta("Dep", 0, 12);
        dep_class.component = Some(component("dep", "<ng-content select=\"[slot]\"></ng-content>"));
        let dep_name_span = dep_class.name_span.unwrap();
        let files = Files(vec![("/dep.ts", dep_content, analysis("/dep.ts", vec![dep_class]))]);

        let content = "export class App {} export class Dir {} export class Jit {}";
        let mut dep_declaration = metadata_to_declaration(
            ComponentOrDirectiveMetadata::Component(&component("dep", "")),
            &class_meta("Dep", 0, 12),
            "component",
            None,
            0,
        );
        dep_declaration.file_path = Some("/dep.ts".to_string());
        dep_declaration.name_span = dep_name_span;
        let mut app = class_meta("App", 7, 19);
        let mut app_component = component("app-root", "<p>{{ a }}</p>\n<dep></dep>");
        app_component.template_url =
            Some(UrlMetadata { url: "./app.html".to_string(), resolved_path: "/app.html".to_string(), string_literal_span: None });
        app_component.resolved_declarations = Some(vec![dep_declaration]);
        app.component = Some(app_component);
        let mut dir = class_meta("Dir", 27, 39);
        dir.directive = Some(DirectiveMetadata {
            decorator_name: None,
            selector: Some("[dir]".to_string()),
            standalone: true,
            signals: false,
            is_structural: false,
            export_as: None,
            host_properties: vec![],
            host_span: None,
            host_metadata: None,
            host_directives: None,
            resolved_host_directives: None,
            providers_span: None,
            args_span: None,
            declaring_ng_module: None,
            is_jit: false,
        });
        dir.host_listeners = vec![];
        let mut jit = class_meta("Jit", 47, 59);
        jit.component = Some(ComponentMetadata { is_jit: true, ..component("jit", "") });
        let classes = vec![app, dir, jit];

        let allocator = Allocator::default();
        let file_cache = RefCell::new(FileCache::default());
        let config = build_type_checking_config(&NgpCompilerOptions::default(), false, false);
        let content = allocator.alloc_str(content);
        let templates_by_class =
            std::rc::Rc::clone(&get_or_create_file_analysis(&mut file_cache.borrow_mut(), "/app.ts").parsed_templates);
        let dependency_templates = DependencyTemplates::new(&allocator, &files, &file_cache, None, None);
        let targets = build_tcb_targets(
            &allocator,
            "/app.ts",
            content,
            &classes,
            &templates_by_class,
            None,
            &config,
            &dependency_templates,
        );

        // the directive has no host bindings and the JIT component is skipped
        assert_eq!(targets.len(), 1);
        let app = &targets[0];
        assert_eq!(app.class_name, "App");
        // an external template's spans index into it
        assert_eq!(app.template_source, "<p>{{ a }}</p>\n<dep></dep>");
        // the diagnostic copy keeps whitespace: `p`, the line break, `dep`
        assert_eq!(app.template_nodes.len(), 3);
        assert_eq!(app.schemas, [SchemaMetadata { name: CUSTOM_ELEMENTS_SCHEMA.to_string() }]);
        assert_eq!(app.own_declaration.selector.as_deref(), Some("app-root"));
        assert_eq!(app.own_declaration.ng_content_selectors.as_deref(), Some(&[][..]));
        assert_eq!(app.declarations[0].ng_content_selectors.as_deref(), Some(&["[slot]".to_string()][..]));
        // the dependency's template is cached under its file
        assert_eq!(file_cache.borrow()["/dep.ts"].parsed_templates.borrow().len(), 1);
    }

    #[test]
    fn template_diagnostics_point_where_ngtsc_reports_them() {
        let span = SpanMetadata { start: 3, end: 5 };
        let mut external = component("a", "");
        external.template_url =
            Some(UrlMetadata { url: "./a.html".to_string(), resolved_path: "/a.html".to_string(), string_literal_span: None });
        assert_eq!(template_diagnostic_location(&external, "/a.ts", span), ("/a.html".to_string(), span));
        let mut direct = component("a", "");
        direct.template_content_span = Some(SpanMetadata { start: 1, end: 9 });
        direct.template_span = Some(SpanMetadata { start: 0, end: 10 });
        assert_eq!(template_diagnostic_location(&direct, "/a.ts", span), ("/a.ts".to_string(), span));
        let mut indirect = component("a", "");
        indirect.template_span = Some(SpanMetadata { start: 20, end: 30 });
        assert_eq!(
            template_diagnostic_location(&indirect, "/a.ts", span),
            ("/a.ts".to_string(), SpanMetadata { start: 20, end: 30 })
        );
    }
}
