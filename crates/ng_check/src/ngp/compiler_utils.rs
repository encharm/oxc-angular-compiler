//! `packages/compiler-cli/preprocessor/src/compiler-utils.ts`, the part type checking uses: parsing component
//! templates (`getOrParseComponentTemplate` and what it calls), `metadataToDeclaration`, `buildPipeRegistry`,
//! `makeClassKey`, `createForeignComponentMatcher`, `createSpan` and `isStaticSourceNode`. The rest builds compiled
//! output (not ported).
//!
//! Templates are parsed with the fork's parser (`oxc_angular_compiler::parser`) into an `oxc_allocator::Allocator`
//! (`'a`) the caller owns: the nodes, the template text and the text the spans index into all live in it. Spans are
//! byte offsets into that text; ng-analyze's spans (`SpanMetadata`) are JavaScript string (UTF-16) indices, converted
//! where they meet (`create_span`, `parse_direct_inline_template`).

use std::rc::Rc;

use ng_analyze::query::FileId;
use ng_analyze::{
    ClassMetadata, ComponentMetadata, DeclarationMetadata, DirectiveMetadata, ExpressionValueKindMetadata,
    ExpressionValueMetadata, ForeignImportMetadata, ReferenceMetadata, SpanMetadata,
};
use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::r3::R3Node;
use oxc_angular_compiler::parser::html::{HtmlParser, remove_whitespaces};
use oxc_angular_compiler::parser::{LexerRange, ParseTemplateOptions};
use oxc_angular_compiler::transform::HtmlToR3Transform;
use oxc_angular_compiler::transform::html_to_r3::TransformOptions;
use oxc_span::Span;

use crate::angular::directive_matching::SelectorlessMatcher;
use crate::angular::t2_api::ForeignComponentMeta;
use crate::ngp::file_analysis::{ParsedComponentTemplate, TemplatesByClass};
use crate::ngp::tcb::PipeMeta;
use crate::ngp::tcb_util::{offset_to_position, utf16_to_byte_offset};
use crate::typecheck::api::Map;

// template.ts:23 LEADING_TRIVIA_CHARS (`packages/compiler/src/render3/view/template.ts`)
pub const LEADING_TRIVIA_CHARS: [char; 4] = [' ', '\n', '\r', '\t'];

/// template.ts:28 ParseTemplateOptions, as the preprocessor passes them (`Partial<ParseTemplateOptions>`: a field
/// that is `None` is not set). `range` is in byte offsets, the parser's.
#[derive(Debug, Clone, Default)]
pub struct TemplateParseOptions {
    pub preserve_whitespaces: Option<bool>,
    pub preserve_line_endings: Option<bool>,
    pub preserve_significant_whitespace: Option<bool>,
    pub range: Option<LexerRange>,
    pub escaped_string: Option<bool>,
    pub leading_trivia_chars: Option<Vec<char>>,
    pub enable_i18n_legacy_message_id_format: Option<bool>,
    pub i18n_normalize_line_endings_in_icus: Option<bool>,
    pub collect_comment_nodes: Option<bool>,
    pub enable_block_syntax: Option<bool>,
    pub enable_let_syntax: Option<bool>,
    pub enable_selectorless: Option<bool>,
    pub interpolation: Option<(String, String)>,
}

/// A template parse error (`ParseError`, which the fork does not export): its message and span (byte offsets into
/// the template's `source`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateParseError {
    pub msg: String,
    pub span: Span,
}

/// template.ts ParsedTemplate: a parsed template and what was collected while parsing it. `source` and `url` are
/// the `ParseSourceFile` its spans point into (TypeScript reaches them through any span: `span.start.file`).
#[derive(Debug)]
pub struct ParsedTemplate<'a> {
    pub preserve_whitespaces: Option<bool>,
    pub errors: Option<Vec<TemplateParseError>>,
    pub nodes: &'a [R3Node<'a>],
    pub style_urls: Vec<String>,
    pub styles: Vec<String>,
    pub ng_content_selectors: Vec<String>,
    /// `ParseSourceFile.content`: the text the template's spans index into
    pub source: &'a str,
    /// `ParseSourceFile.url`
    pub url: String,
}

/// `ParseError` as `TemplateParseError` (a macro: the fork does not export the type)
macro_rules! template_parse_error {
    () => {
        |error| TemplateParseError {
            msg: error.msg.clone(),
            span: Span::new(error.span.start.offset, error.span.end.offset),
        }
    };
}

/// template.ts:140 parseTemplate (`packages/compiler/src/render3/view/template.ts`), over the fork's parser: HTML
/// parsing, whitespace removal and the transform to the template AST, as `component/transform.rs` runs them. The
/// fork's transform applies the i18n metadata itself (Angular's `I18nMetaVisitor` passes). The fork's lexer always
/// normalizes line endings in text (`preserveLineEndings`, `i18nNormalizeLineEndingsInICUs`) and has no
/// `retainEmptyTokens`.
pub fn parse_template<'a>(
    allocator: &'a Allocator,
    template: &'a str,
    template_url: &str,
    options: &TemplateParseOptions,
) -> ParsedTemplate<'a> {
    let preserve_whitespaces = options.preserve_whitespaces;
    let selectorless_enabled = options.enable_selectorless.unwrap_or(false);
    let parse_options = ParseTemplateOptions {
        preserve_whitespaces: preserve_whitespaces.unwrap_or(false),
        preserve_line_endings: options.preserve_line_endings.unwrap_or(false),
        preserve_significant_whitespace: options.preserve_significant_whitespace.unwrap_or(true),
        enable_block_syntax: options.enable_block_syntax.unwrap_or(true),
        enable_let_syntax: options.enable_let_syntax.unwrap_or(true),
        enable_selectorless: selectorless_enabled,
        tokenize_expansion_forms: true,
        collect_comment_nodes: options.collect_comment_nodes.unwrap_or(false),
        leading_trivia_chars: Some(
            options.leading_trivia_chars.clone().unwrap_or_else(|| LEADING_TRIVIA_CHARS.to_vec()),
        ),
        interpolation: options.interpolation.clone(),
        escaped_string: options.escaped_string.unwrap_or(false),
        range: options.range,
    };
    let parse_result = HtmlParser::with_options(allocator, template, template_url, &parse_options).parse();

    if !parse_result.errors.is_empty() {
        return ParsedTemplate {
            preserve_whitespaces,
            errors: Some(parse_result.errors.iter().map(template_parse_error!()).collect()),
            nodes: &[],
            style_urls: vec![],
            styles: vec![],
            ng_content_selectors: vec![],
            source: template,
            url: template_url.to_string(),
        };
    }

    let root_nodes: &'a [_] = if preserve_whitespaces.unwrap_or(false) {
        allocator.alloc(parse_result.nodes)
    } else {
        // Always preserve significant whitespace here because this is used to generate the `goog.getMsg`
        // and `$localize` calls which should retain significant whitespace in order to render the
        // correct output.
        allocator.alloc(remove_whitespaces(allocator, &parse_result.nodes, true))
    };

    let r3 = HtmlToR3Transform::new(
        allocator,
        template,
        TransformOptions { collect_comment_nodes: parse_options.collect_comment_nodes, angular_version: None },
    )
    .transform(root_nodes);
    let errors: Vec<TemplateParseError> = r3.errors.iter().map(template_parse_error!()).collect();

    ParsedTemplate {
        preserve_whitespaces,
        errors: if errors.is_empty() { None } else { Some(errors) },
        nodes: allocator.alloc(r3.nodes),
        style_urls: r3.style_urls.iter().map(ToString::to_string).collect(),
        styles: r3.styles.iter().map(ToString::to_string).collect(),
        ng_content_selectors: r3.ng_content_selectors.iter().map(ToString::to_string).collect(),
        source: template,
        url: template_url.to_string(),
    }
}

// compiler-utils.ts:127 createSpan: creates a source span from a known position. The position is ng-analyze's (UTF-16
// indices into `content`); the span is the parser's (byte offsets into `content`). (TypeScript's spans carry a file
// name it does not use for anything: `fileName` is not kept.)
pub fn create_span(content: &str, start: u32, end: u32) -> Span {
    Span::new(utf16_to_byte_offset(content, start), utf16_to_byte_offset(content, end))
}

// compiler-utils.ts:575 isStaticSourceNode: checks if a `SourceNode` is static
pub fn is_static_source_node(node: &ExpressionValueMetadata) -> bool {
    matches!(node.kind, ExpressionValueKindMetadata::Identifier | ExpressionValueKindMetadata::String)
}

// compiler-utils.ts:594 parseDirectInlineTemplate: parses an inline template declared as a string literal or a
// no-substitution template literal straight out of the component file's source text, as ngtsc's `extractTemplate`
// does for a `direct` source mapping.
//
// The lexer reads the literal's source text inside `contentSpan` (ngtsc's `getTemplateRange`) and decodes its escape
// sequences itself (`escapedString`), so every span it produces is an offset into `sourceText`. ngtsc always
// normalizes ICU line endings for such a template.
pub fn parse_direct_inline_template<'a>(
    allocator: &'a Allocator,
    source_text: &'a str,
    file_path: &str,
    content_span: SpanMetadata,
    options: &TemplateParseOptions,
) -> ParsedTemplate<'a> {
    let position = offset_to_position(source_text, content_span.start);
    parse_template(
        allocator,
        source_text,
        file_path,
        &TemplateParseOptions {
            escaped_string: Some(true),
            i18n_normalize_line_endings_in_icus: Some(true),
            range: Some(LexerRange {
                start_pos: utf16_to_byte_offset(source_text, content_span.start),
                end_pos: utf16_to_byte_offset(source_text, content_span.end),
                start_line: position.line,
                start_col: position.character,
            }),
            ..options.clone()
        },
    )
}

// compiler-utils.ts:614 parseComponentTemplate
pub fn parse_component_template<'a>(
    allocator: &'a Allocator,
    component: &ComponentMetadata,
    file_path: &str,
    content: &'a str,
    options: &TemplateParseOptions,
) -> ParsedComponentTemplate<'a> {
    let parse_options = TemplateParseOptions {
        preserve_whitespaces: Some(
            component.preserve_whitespaces.or(options.preserve_whitespaces).unwrap_or(false),
        ),
        ..options.clone()
    };
    let diag_parse_options = TemplateParseOptions {
        preserve_whitespaces: Some(true),
        preserve_line_endings: Some(true),
        preserve_significant_whitespace: Some(true),
        leading_trivia_chars: Some(vec![]),
        ..options.clone()
    };

    // A `template` literal is parsed out of this file's source text so its spans are offsets
    // into it. An external template, or an inline one computed by any other expression, is parsed
    // on its own: its spans are offsets into the template string (ngtsc's `external` and
    // `indirect` source mappings).
    // Mirroring ngtsc's `parseExtractedTemplate`, also parse a second, span-faithful copy
    // (`diagNodes`: whitespace, line endings and leading trivia preserved) for type-checking,
    // diagnostics, and indexing.
    let content_span = component.template_content_span;
    if component.template_url.is_none()
        && let Some(content_span) = content_span
    {
        let parsed = parse_direct_inline_template(allocator, content, file_path, content_span, &parse_options);
        let diag_nodes =
            parse_direct_inline_template(allocator, content, file_path, content_span, &diag_parse_options).nodes;
        return ParsedComponentTemplate { parsed, diag_nodes, class_name_start: None };
    }

    // The URL identifies the file the spans are offsets into, which for an external template is
    // the template file itself (as in ngtsc's `extractTemplate`).
    let template_str = allocator.alloc_str(component.template.as_deref().unwrap_or(""));
    let template_url = component.template_url.as_ref().map_or(file_path, |url| url.resolved_path.as_str());
    let parsed = parse_template(allocator, template_str, template_url, &parse_options);
    let diag_nodes = parse_template(allocator, template_str, template_url, &diag_parse_options).nodes;
    ParsedComponentTemplate { parsed, diag_nodes, class_name_start: None }
}

// compiler-utils.ts:665 getOrParseComponentTemplate: the parsed template of `component` from `templatesByClass` (a
// `FileAnalysis.parsedTemplates`), parsing and caching it on first use. Every consumer of a template in a file goes
// through this, so that it is parsed once whichever of them gets to it first. `content` is the file's text, in the
// allocator (an inline template is parsed in place in it).
pub fn get_or_parse_component_template<'a>(
    allocator: &'a Allocator,
    templates_by_class: &TemplatesByClass<'a>,
    class_meta: &ClassMetadata,
    component: &ComponentMetadata,
    file_path: &str,
    content: &'a str,
    options: Option<&TemplateParseOptions>,
) -> Rc<ParsedComponentTemplate<'a>> {
    let class_key = make_class_key(class_meta.class_name.as_deref(), class_meta.span.start);
    let parsed = templates_by_class.borrow().get(&class_key).cloned();
    if let Some(parsed) = parsed {
        return parsed;
    }
    let default_options = TemplateParseOptions::default();
    let parsed = Rc::new(ParsedComponentTemplate {
        class_name_start: class_meta.name_span.map(|span| span.start),
        ..parse_component_template(allocator, component, file_path, content, options.unwrap_or(&default_options))
    });
    templates_by_class.borrow_mut().insert(class_key, Rc::clone(&parsed));
    parsed
}

/// `nga.ComponentMetadata | nga.DirectiveMetadata`, what `metadataToDeclaration` reads a declaration from
#[derive(Debug, Clone, Copy)]
pub enum ComponentOrDirectiveMetadata<'m> {
    Component(&'m ComponentMetadata),
    Directive(&'m DirectiveMetadata),
}

// compiler-utils.ts:685 metadataToDeclaration
pub fn metadata_to_declaration(
    meta: ComponentOrDirectiveMetadata<'_>,
    class_meta: &ClassMetadata,
    declaration_type: &str,
    ng_content_selectors: Option<Vec<String>>,
    file_id: FileId,
) -> DeclarationMetadata {
    let (selector, standalone, export_as, host_directives, resolved_host_directives, is_structural) = match meta {
        ComponentOrDirectiveMetadata::Component(meta) => (
            &meta.selector,
            meta.standalone,
            &meta.export_as,
            &meta.host_directives,
            &meta.resolved_host_directives,
            // `'isStructural' in meta`: a component has none
            false,
        ),
        ComponentOrDirectiveMetadata::Directive(meta) => (
            &meta.selector,
            meta.standalone,
            &meta.export_as,
            &meta.host_directives,
            &meta.resolved_host_directives,
            meta.is_structural,
        ),
    };
    let class_name = class_meta.class_name.clone().unwrap_or_default();
    DeclarationMetadata {
        name: class_name.clone(),
        name_span: class_meta.name_span.unwrap_or_default(),
        declaration_type: declaration_type.to_string(),
        // The analyzer has already applied ngtsc's default selector (`ng-component`) to a component
        // without one, so this matches the `DeclarationMetadata` other components see.
        selector: selector.clone().filter(|selector| !selector.is_empty()),
        pipe_name: None,
        is_standalone: Some(standalone),
        export_as: export_as.clone(),
        file_id,
        file_path: None,
        r#ref: class_meta.r#ref.clone().unwrap_or(ReferenceMetadata {
            consumer_import: None,
            typecheck_import: None,
            local_alias: Some(class_name),
        }),
        ref_in_declaring_module: None,
        host_directives: host_directives.clone(),
        resolved_host_directives: resolved_host_directives.clone(),
        fields: Some(class_meta.fields.clone()),
        flattened_fields: class_meta.flattened_fields.clone(),
        ng_content_selectors,
        type_parameters: class_meta.type_parameters.clone(),

        has_ng_template_context_guard: class_meta.has_ng_template_context_guard,
        ng_template_guards: class_meta.ng_template_guards.clone(),
        animation_trigger_names: None,
        has_ng_field_directive: class_meta.has_ng_field_directive,
        public_methods: class_meta.public_methods.clone(),
        // `'isForwardRef' in meta`: neither has one
        is_forward_ref: false,
        is_structural,
        cycle_prone: None,
        is_exported: class_meta.is_exported,
        has_non_exported_bounds: class_meta.has_non_exported_bounds,
        is_explicitly_deferred: false,
        deferred_blocks: None,
    }
}

// compiler-utils.ts:730 buildPipeRegistry
pub fn build_pipe_registry(declarations: &[DeclarationMetadata]) -> Map<String, PipeMeta> {
    let mut registry = Map::default();
    for decl in declarations {
        if decl.declaration_type == "pipe"
            && let Some(pipe_name) = decl.pipe_name.as_ref().filter(|name| !name.is_empty())
        {
            registry.insert(
                pipe_name.clone(),
                PipeMeta {
                    name: pipe_name.clone(),
                    class_name: decl.name.clone(),
                    import_path: decl.r#ref.typecheck_import.as_ref().map(|import| import.specifier.clone()),
                    file_path: decl.file_path.clone(),
                    is_exported: Some(decl.is_exported),
                    has_non_exported_bounds: Some(decl.has_non_exported_bounds),
                    is_explicitly_deferred: Some(decl.is_explicitly_deferred),
                    deferred_blocks: decl.deferred_blocks.clone(),
                },
            );
        }
    }
    registry
}

// compiler-utils.ts:748 makeClassKey
pub fn make_class_key(class_name: Option<&str>, span_start: u32) -> String {
    format!("{}@{span_start}", class_name.unwrap_or(""))
}

// compiler-utils.ts:757 createForeignComponentMatcher: the selectorless matcher `R3TargetBinder` uses to recognize
// foreign components (one registry entry per `foreignImports` name), or `null` when there are none.
// https://github.com/angular/angular/blob/e3ac727/packages/compiler-cli/src/ngtsc/metadata/src/util.ts#L384-L396
pub fn create_foreign_component_matcher(
    foreign_imports: Option<&[ForeignImportMetadata]>,
) -> Option<SelectorlessMatcher<ForeignComponentMeta>> {
    let foreign_imports = foreign_imports.filter(|imports| !imports.is_empty())?;
    let mut registry: Map<String, Vec<ForeignComponentMeta>> = Map::default();
    for ForeignImportMetadata { name, .. } in foreign_imports {
        registry.insert(name.clone(), vec![ForeignComponentMeta { name: name.clone() }]);
    }
    Some(SelectorlessMatcher::new(registry))
}

#[cfg(test)]
mod tests {
    use oxc_angular_compiler::ast::r3::R3Node;

    use super::*;

    #[test]
    fn class_keys() {
        assert_eq!(make_class_key(Some("AppComponent"), 12), "AppComponent@12");
        assert_eq!(make_class_key(None, 0), "@0");
    }

    #[test]
    fn parses_an_inline_template_in_place() {
        let allocator = Allocator::default();
        // `template: '<b>{{ é }}</b>'`: the content span (UTF-16) is the literal's text
        let content = "const é = 1; @Component({template: '<b>{{x}}</b>'}) class C {}";
        let start = content.find("<b>").unwrap();
        let start16 = content[..start].encode_utf16().count() as u32;
        let span = SpanMetadata { start: start16, end: start16 + "<b>{{x}}</b>".len() as u32 };
        let content = allocator.alloc_str(content);
        let parsed = parse_direct_inline_template(
            &allocator,
            content,
            "c.ts",
            span,
            &TemplateParseOptions { preserve_whitespaces: Some(true), ..TemplateParseOptions::default() },
        );
        assert!(parsed.errors.is_none(), "{:?}", parsed.errors);
        let [R3Node::Element(element)] = parsed.nodes else { panic!("{:?}", parsed.nodes) };
        // spans are byte offsets into the file
        assert_eq!(element.source_span.start as usize, start);
        assert_eq!(&content[element.start_source_span.start as usize..element.start_source_span.end as usize], "<b>");
    }

    #[test]
    fn parse_errors_leave_no_nodes() {
        let allocator = Allocator::default();
        let parsed = parse_template(&allocator, allocator.alloc_str("<b></div>"), "t.html", &TemplateParseOptions::default());
        assert!(parsed.errors.is_some());
        assert!(parsed.nodes.is_empty());
    }
}
