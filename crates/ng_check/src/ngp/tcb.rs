//! `packages/compiler-cli/preprocessor/src/tcb.ts`: the type-checking configuration, the environment the type-check
//! block generator writes imports, pipe instances and type constructors through, and the generation of a file's
//! type-check blocks (`prepareTcbTargets`, `generateTcbCode`, `combineTcbContent`).
//!
//! Every text written here into a type-check file is TypeScript's, byte for byte. Offsets in ng-analyze's metadata and
//! in diagnostics are JavaScript string (UTF-16) indices; template spans are the parser's byte offsets, and the
//! generator runs inside `span::with_source_text` for the text each template's spans index into
//! (`TcbTargetInput::template_source`).

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use ng_analyze::{
    ClassMetadata, ComponentMetadata, DeclarationMetadata, ForeignImportMetadata, HostBindingMetadata,
    HostListenerMetadata, HostPropertyMetadata, TypeParameterMetadata,
};
use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::r3::{R3HostElement, R3Node};

use crate::angular::core::SchemaMetadata;
use crate::angular::t2_api::BoundTarget;
use crate::ngp::compiler_options::NgpCompilerOptions;
use crate::ngp::compiler_utils::make_class_key;
use crate::ngp::diagnostic::Diagnostic;
use crate::ngp::dom_schema_checker::RegistryDomSchemaChecker;
use crate::ngp::oob::{AbsoluteSpan, OutOfBandDiagnosticRecorderImpl};
use crate::ngp::tcb_adapter::{AdaptedTcbMeta, ExtendedTcbTypeParameter, TypeParameterRefs, adapt_tcb_input};
use crate::ngp::tcb_util::{requires_inline_declaration, requires_inline_type_check_block, substring, utf16_len};
use crate::ngp::template_semantics::get_template_semantics_diagnostics;
use crate::typecheck::api::{
    DiagnosticCategoryLabel, DirectiveRef, Map, TcbComponentMetadata, TcbDirectiveMetadata, TcbEnvironment,
    TcbPipeMetadata, TcbReferenceKey, TcbReferenceMetadata, TcbTypeParameter, TypeCheckId, TypeCheckingConfig,
    TypeCtorMetadata,
};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::type_check_block::generate_type_check_block;

// tcb.ts:43 TcbTargetInput. `template_source` is not in the TypeScript: the text the template's spans index into
// (what TypeScript reaches through any span's `ParseSourceFile`).
#[derive(Debug)]
pub struct TcbTargetInput<'a> {
    pub class_meta: ClassMetadata,
    pub class_name: String,
    pub template: String,
    pub template_nodes: &'a [R3Node<'a>],
    pub template_source: &'a str,
    pub declarations: Vec<DeclarationMetadata>,
    pub own_declaration: DeclarationMetadata,
    pub host_properties: Vec<HostPropertyMetadata>,
    pub host_bindings: Vec<HostBindingMetadata>,
    pub host_listeners: Vec<HostListenerMetadata>,
    pub is_standalone: bool,
    pub preserve_whitespaces: Option<bool>,
    pub schemas: Vec<SchemaMetadata>,
    pub type_parameters: Option<Vec<TypeParameterMetadata>>,
    pub pipe_registry: Option<Map<String, PipeMeta>>,
    pub enable_selectorless: Option<bool>,
    pub selectorless_enabled: Option<bool>,
    pub foreign_imports: Option<Vec<ForeignImportMetadata>>,
}

// tcb.ts:63 PreparedTcbTarget, with the extended type parameters TypeScript keeps on `targetMeta` and on the
// directives' metadata (`tcb_adapter.rs`)
pub struct PreparedTcbTarget<'a> {
    pub comp: TcbTargetInput<'a>,
    pub type_check_id: TypeCheckId,
    pub tcb_meta: AdaptedTcbMeta<'a>,
    pub target_meta: TcbComponentMetadata,
    pub target_type_parameters: Option<Vec<ExtendedTcbTypeParameter>>,
    pub type_parameter_refs: TypeParameterRefs,
}

// tcb.ts:70 PreparedTcbData
pub struct PreparedTcbData<'a> {
    pub targets: Vec<PreparedTcbTarget<'a>>,
    pub bound_target_map: Map<String, Rc<dyn BoundTarget<'a, DirectiveRef> + 'a>>,
    pub host_elements_map: Map<String, Option<&'a R3HostElement<'a>>>,
    pub type_check_id_map: Map<String, TypeCheckId>,
    pub requires_inline: bool,
}

// tcb.ts:78 TcbResult
#[derive(Debug, Clone)]
pub struct TcbResult {
    pub code: String,
    pub imports: Option<String>,
    /// classKey -> TCB code
    pub tcb_map: Map<String, String>,
    /// TypeCheckId -> where the TCB's template spans point, for TCBs whose template spans don't index
    /// into the component's source file.
    pub template_sources: Map<TypeCheckId, TcbTemplateSource>,
    pub diagnostics: Vec<Diagnostic>,
    pub is_inline: bool,
}

// tcb.ts:108 TcbTemplateSource: which file the span comments (`/*start,end*/`) of a type-check block's template
// bindings index into, if not the component's source file, mirroring ngtsc's template source mappings. Spans are
// always absolute offsets into the text the template was parsed from; host binding spans always index into the
// component's own source file.
//
// - `TemplateFile`: an external template (`external`); spans index into that file, as resolved by the analyzer.
// - `TemplateExpression`: an inline template whose `template:` value is an expression rather than a string literal
//   (`indirect`). Spans index into the resolved template string, which is not any one file, so a diagnostic is
//   reported at this span (the `template:` expression, UTF-16 indices) of the component's source file instead.
//
// TCBs of a string-literal inline template (`direct`) or of no template have no `TcbTemplateSource`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcbTemplateSource {
    TemplateFile(String),
    TemplateExpression { start: u32, end: u32 },
}

// tcb.ts:119 TCB_TEMPLATE_SOURCES_COMMENT_PREFIX: prefix of the block comment appended to a generated
// `.ngtypecheck.ts` file, followed by the JSON of `TcbResult.templateSources`. It is omitted if `templateSources` is
// empty.
pub const TCB_TEMPLATE_SOURCES_COMMENT_PREFIX: &str = "ngp-tcb-template-sources:";

// tcb.ts:121 getTcbTemplateSource
fn get_tcb_template_source(component: Option<&ComponentMetadata>) -> Option<TcbTemplateSource> {
    // Same classification as the processor's `templateDiagnosticLocation`.
    if let Some(template_url) = component.and_then(|c| c.template_url.as_ref()) {
        return Some(TcbTemplateSource::TemplateFile(template_url.resolved_path.clone()));
    }
    let expression_span = component.and_then(|c| c.template_span);
    if component.and_then(|c| c.template_content_span).is_some() {
        return None;
    }
    let expression_span = expression_span?;
    Some(TcbTemplateSource::TemplateExpression { start: expression_span.start, end: expression_span.end })
}

/// `JSON.stringify(templateSources)`
fn template_sources_json(template_sources: &Map<TypeCheckId, TcbTemplateSource>) -> String {
    let mut json = String::from("{");
    for (index, (id, source)) in template_sources.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&TcbExpr::quote_and_escape(id));
        json.push(':');
        match source {
            TcbTemplateSource::TemplateFile(file) => {
                json.push_str(&format!("{{\"templateFile\":{}}}", TcbExpr::quote_and_escape(file)));
            }
            TcbTemplateSource::TemplateExpression { start, end } => {
                json.push_str(&format!("{{\"templateExpression\":{{\"start\":{start},\"end\":{end}}}}}"));
            }
        }
    }
    json.push('}');
    json
}

// tcb.ts:135 serializeTcbTemplateSources
fn serialize_tcb_template_sources(template_sources: &Map<TypeCheckId, TcbTemplateSource>) -> String {
    if template_sources.is_empty() {
        return String::new();
    }
    // `*/` would terminate the comment; `\/` is an equivalent JSON escape of `/`.
    let json = template_sources_json(template_sources).replace("*/", "*\\/");
    format!("\n/*{TCB_TEMPLATE_SOURCES_COMMENT_PREFIX}{json}*/\n")
}

// tcb.ts:144 PipeMeta
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipeMeta {
    pub name: String,
    pub class_name: String,
    pub import_path: Option<String>,
    pub file_path: Option<String>,
    pub is_exported: Option<bool>,
    pub has_non_exported_bounds: Option<bool>,
    pub is_explicitly_deferred: Option<bool>,
    pub deferred_blocks: Option<Vec<String>>,
}

// tcb.ts:155 NgpTypeCheckingConfig
#[derive(Debug, Clone)]
pub struct NgpTypeCheckingConfig {
    /// `extends TypeCheckingConfig`
    pub config: TypeCheckingConfig,
    pub type_check_host_bindings: Option<bool>,
    pub use_poisoned_data: Option<bool>,
}

impl std::ops::Deref for NgpTypeCheckingConfig {
    type Target = TypeCheckingConfig;

    fn deref(&self) -> &TypeCheckingConfig {
        &self.config
    }
}

// tcb.ts:160 buildTypeCheckingConfig
pub fn build_type_checking_config(
    options: &NgpCompilerOptions,
    enable_template_type_checker: bool,
    use_poisoned_data: bool,
) -> NgpTypeCheckingConfig {
    let strict_templates = options.strict_templates != Some(false);
    let extended = options.extended_diagnostics.as_ref();
    let check_category = |check: fn(&crate::ngp::compiler_options::ExtendedDiagnosticsChecks) -> _| {
        extended
            .and_then(|e| e.checks.as_ref())
            .and_then(check)
            .or_else(|| extended.and_then(|e| e.default_category))
            .map_or(DiagnosticCategoryLabel::Warning, Into::into)
    };
    let control_flow_preventing_content_projection =
        check_category(|checks| checks.control_flow_preventing_content_projection);
    let unused_standalone_imports = check_category(|checks| checks.unused_standalone_imports);

    let mut type_checking_config = if strict_templates {
        TypeCheckingConfig {
            apply_template_context_guards: true,
            check_template_bodies: true,
            always_check_schema_in_template_bodies: true,
            check_type_of_input_bindings: true,
            honor_access_modifiers_for_input_bindings: false,
            check_control_flow_bodies: true,
            strict_null_input_bindings: true,
            check_type_of_attributes: true,
            check_type_of_dom_bindings: false,
            check_type_of_output_events: true,
            check_type_of_animation_events: true,
            check_type_of_dom_events: true,
            check_type_of_dom_references: true,
            check_type_of_non_dom_references: true,
            check_type_of_pipes: true,
            strict_safe_navigation_types: true,
            use_context_generic_type: true,
            strict_literal_types: true,
            enable_template_type_checker,
            use_inline_type_constructors: false,
            control_flow_preventing_content_projection,
            unused_standalone_imports,
            // These are actually set via version check in ngtsc (ngtsc/core/src/compiler.ts).
            // Since we don't have any of that here we just default to true.
            allow_signals_in_two_way_bindings: true,
            allow_dom_event_assertion: true,
            check_unclaimed_event_names: false,
            check_unknown_elements: false,
        }
    } else {
        TypeCheckingConfig {
            apply_template_context_guards: false,
            check_template_bodies: false,
            check_control_flow_bodies: false,
            always_check_schema_in_template_bodies: options.annotate_for_closure_compiler.unwrap_or(false),
            check_type_of_input_bindings: false,
            strict_null_input_bindings: false,
            honor_access_modifiers_for_input_bindings: false,
            check_type_of_attributes: false,
            check_type_of_dom_bindings: false,
            check_type_of_output_events: false,
            check_type_of_animation_events: false,
            check_type_of_dom_events: false,
            check_type_of_dom_references: false,
            check_type_of_non_dom_references: false,
            check_type_of_pipes: false,
            strict_safe_navigation_types: false,
            use_context_generic_type: false,
            strict_literal_types: false,
            enable_template_type_checker,
            use_inline_type_constructors: false,
            control_flow_preventing_content_projection,
            unused_standalone_imports,
            // These are actually set via version check in ngtsc (ngtsc/core/src/compiler.ts).
            // Since we don't have any of that here we just default to true.
            allow_signals_in_two_way_bindings: true,
            allow_dom_event_assertion: true,
            check_unclaimed_event_names: false,
            check_unknown_elements: false,
        }
    };

    if let Some(strict_input_types) = options.strict_input_types {
        type_checking_config.check_type_of_input_bindings = strict_input_types;
        type_checking_config.apply_template_context_guards = strict_input_types;
    }
    if let Some(strict_input_access_modifiers) = options.strict_input_access_modifiers {
        type_checking_config.honor_access_modifiers_for_input_bindings = strict_input_access_modifiers;
    }
    if let Some(strict_null_input_types) = options.strict_null_input_types {
        type_checking_config.strict_null_input_bindings = strict_null_input_types;
    }
    if let Some(strict_output_event_types) = options.strict_output_event_types {
        type_checking_config.check_type_of_output_events = strict_output_event_types;
        type_checking_config.check_type_of_animation_events = strict_output_event_types;
    }
    if let Some(strict_dom_event_types) = options.strict_dom_event_types {
        type_checking_config.check_type_of_dom_events = strict_dom_event_types;
    }
    if let Some(strict_safe_navigation_types) = options.strict_safe_navigation_types {
        type_checking_config.strict_safe_navigation_types = strict_safe_navigation_types;
    }
    if let Some(strict_dom_local_ref_types) = options.strict_dom_local_ref_types {
        type_checking_config.check_type_of_dom_references = strict_dom_local_ref_types;
    }
    if let Some(strict_attribute_types) = options.strict_attribute_types {
        type_checking_config.check_type_of_attributes = strict_attribute_types;
    }
    if let Some(strict_context_generics) = options.strict_context_generics {
        type_checking_config.use_context_generic_type = strict_context_generics;
    }
    if let Some(strict_literal_types) = options.strict_literal_types {
        type_checking_config.strict_literal_types = strict_literal_types;
    }

    NgpTypeCheckingConfig {
        config: type_checking_config,
        type_check_host_bindings: Some(options.type_check_host_bindings.unwrap_or(true)),
        use_poisoned_data: Some(use_poisoned_data || options.compile_poisoned_components.unwrap_or(false)),
    }
}

// tcb.ts:283 printStatements
fn print_statements(expressions: &[TcbExpr]) -> String {
    let mut result = String::new();
    for expr in expressions {
        result.push_str(&format!("{};\n", expr.p()));
    }
    result
}

// tcb.ts:291 TcbImportManager. The methods take `&self` (the environments share it).
#[derive(Debug, Default)]
pub struct TcbImportManager {
    imports: RefCell<Map<String, String>>,
    next_index: Cell<u32>,
}

impl TcbImportManager {
    pub fn new() -> Self {
        Self::default()
    }

    // tcb.ts:295 addImport
    pub fn add_import(&self, module_specifier: &str) -> String {
        let mut imports = self.imports.borrow_mut();
        if let Some(alias) = imports.get(module_specifier) {
            return alias.clone();
        }
        let alias = format!("i{}", self.next_index.get());
        self.next_index.set(self.next_index.get() + 1);
        imports.insert(module_specifier.to_string(), alias.clone());
        alias
    }

    // tcb.ts:304 generateImportStatements
    pub fn generate_import_statements(&self) -> String {
        let mut result = String::new();
        for (module_specifier, alias) in self.imports.borrow().iter() {
            result.push_str(&format!("import * as {alias} from '{module_specifier}';\n"));
        }
        result
    }
}

// tcb.ts:313 qualifyTypeParameters. The type references' spans are JavaScript string indices into the
// representations.
pub fn qualify_type_parameters(
    params: Option<&[ExtendedTcbTypeParameter]>,
    env: &dyn TcbEnvironment,
    default_module_specifier: Option<&str>,
) -> Option<Vec<ExtendedTcbTypeParameter>> {
    let params = params?;
    if params.is_empty() {
        return Some(vec![]);
    }

    Some(
        params
            .iter()
            .map(|param| {
                let Some(type_refs) = param.type_refs.as_ref().filter(|refs| !refs.is_empty()) else {
                    return param.clone();
                };

                // Sort descending by start offset so replacing back-to-front preserves earlier offsets
                let mut sorted_refs: Vec<_> = type_refs.iter().collect();
                sorted_refs.sort_by_key(|r| std::cmp::Reverse(r.span.map_or(0, |span| span.start)));

                let mut representation = param.representation.clone();
                let mut representation_with_default = param.representation_with_default.clone();

                for r in sorted_refs {
                    let module = Some(r.module_specifier.as_str())
                        .filter(|module| !module.is_empty())
                        .or(default_module_specifier)
                        .filter(|module| !module.is_empty());
                    let (Some(module), Some(span)) = (module, r.span) else { continue };

                    let qualified = env.reference_external_symbol(module, &r.symbol).p();
                    let (start, end) = (span.start, span.end);

                    representation = replace_range(&representation, start, end, &qualified);
                    representation_with_default = replace_range(&representation_with_default, start, end, &qualified);
                }

                ExtendedTcbTypeParameter { representation, representation_with_default, ..param.clone() }
            })
            .collect(),
    )
}

/// `if (start <= s.length && end <= s.length) s = s.slice(0, start) + replacement + s.slice(end)`, in JavaScript
/// string indices
fn replace_range(s: &str, start: u32, end: u32, replacement: &str) -> String {
    let length = utf16_len(s);
    if start > length || end > length {
        return s.to_string();
    }
    format!("{}{replacement}{}", substring(s, 0, start), substring(s, end, length))
}

// tcb.ts:360 generateGenericArgs
fn generate_generic_args(type_parameters: Option<&[TcbTypeParameter]>) -> String {
    match type_parameters {
        Some(params) if !params.is_empty() => {
            format!("<{}>", params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>().join(", "))
        }
        _ => String::new(),
    }
}

// tcb.ts:367 typeParametersWithDefaultTypes
fn type_parameters_with_default_types(params: Option<&[TcbTypeParameter]>) -> String {
    match params {
        Some(params) if !params.is_empty() => format!(
            "<{}>",
            params.iter().map(|param| param.representation_with_default.as_str()).collect::<Vec<_>>().join(", ")
        ),
        _ => String::new(),
    }
}

// render3/util.ts:24 UNSAFE_OBJECT_KEY_NAME_REGEXP, render3/util.ts:179 isUnsafeObjectKey
fn is_unsafe_object_key(key: &str) -> bool {
    key.contains(['-', '.'])
}

// r3_identifiers.ts:506 R3Identifiers.UnwrapDirectiveSignalInputs
const UNWRAP_DIRECTIVE_SIGNAL_INPUTS_NAME: &str = "ɵUnwrapDirectiveSignalInputs";
const UNWRAP_DIRECTIVE_SIGNAL_INPUTS_MODULE: &str = "@angular/core";

// tcb.ts:376 constructTypeCtorParameter
fn construct_type_ctor_parameter(
    env: &dyn TcbEnvironment,
    meta: &TypeCtorMetadata,
    type_ref: &str,
    type_ref_with_generics: &str,
) -> String {
    let mut init_type: Option<String> = None;

    let mut plain_keys: Vec<String> = vec![];
    let mut coerced_keys: Vec<String> = vec![];
    let mut signal_input_keys: Vec<String> = vec![];

    for input in meta.inputs.iter() {
        let class_property_name = &input.input.class_property_name;
        if input.input.is_signal {
            signal_input_keys.push(TcbExpr::quote_and_escape(class_property_name));
        } else if !meta.coerced_input_fields.contains(class_property_name) {
            plain_keys.push(TcbExpr::quote_and_escape(class_property_name));
        } else {
            let prop_name = format!("ngAcceptInputType_{class_property_name}");
            let is_unsafe = is_unsafe_object_key(class_property_name);
            let access =
                if is_unsafe { format!("[{}]", TcbExpr::quote_and_escape(&prop_name)) } else { format!(".{prop_name}") };
            let coercion_type = match &input.transform_type {
                Some(transform_type) => transform_type.clone(),
                None => format!("typeof {type_ref}{access}"),
            };

            coerced_keys.push(format!(
                "{}: {coercion_type}",
                if is_unsafe { TcbExpr::quote_and_escape(class_property_name) } else { class_property_name.clone() }
            ));
        }
    }

    if !plain_keys.is_empty() {
        init_type = Some(format!("Pick<{type_ref_with_generics}, {}>", plain_keys.join(" | ")));
    }
    if !coerced_keys.is_empty() {
        let mut coerced_literal = String::from("{\n");
        for key in &coerced_keys {
            coerced_literal.push_str(&format!("{key};\n"));
        }
        coerced_literal.push('}');
        init_type = Some(match init_type {
            Some(init_type) => format!("{init_type} & {coerced_literal}"),
            None => coerced_literal,
        });
    }
    if !signal_input_keys.is_empty() {
        let key_type_union = signal_input_keys.join(" | ");

        let unwrap_ref =
            env.reference_external_symbol(UNWRAP_DIRECTIVE_SIGNAL_INPUTS_MODULE, UNWRAP_DIRECTIVE_SIGNAL_INPUTS_NAME);
        let unwrap_expr = format!("{}<{type_ref_with_generics}, {key_type_union}>", unwrap_ref.p());
        init_type = Some(match init_type {
            Some(init_type) => format!("{init_type} & {unwrap_expr}"),
            None => unwrap_expr,
        });
    }

    format!("init: {}", init_type.unwrap_or_else(|| "{}".to_string()))
}

// tcb.ts:435 generateTypeCtorDeclarationFn
pub fn generate_type_ctor_declaration_fn(
    env: &dyn TcbEnvironment,
    meta: &TypeCtorMetadata,
    node_type_ref: &TcbExpr,
    type_params: Option<&[TcbTypeParameter]>,
) -> TcbExpr {
    let type_args = generate_generic_args(type_params);
    let type_ref_with_generics = format!("{}{type_args}", node_type_ref.p());
    let init_param = construct_type_ctor_parameter(env, meta, &node_type_ref.p(), &type_ref_with_generics);
    let type_parameters = type_parameters_with_default_types(type_params);

    let source = if meta.body {
        let fn_type = format!("{type_parameters}({init_param}) => {type_ref_with_generics}");
        format!("const {}: {fn_type} = null!", meta.fn_name)
    } else {
        format!("declare function {}{type_parameters}({init_param}): {type_ref_with_generics}", meta.fn_name)
    };

    TcbExpr::new(source)
}

// tcb.ts:457 TcbEnvironmentImpl. The methods take `&self` (`TcbEnvironment`); the state is in cells.
// `type_parameter_refs` is not in the TypeScript: the directives' extended type parameters, which TypeScript reads
// off `dir.typeParameters` (`tcb_adapter.rs`).
pub struct TcbEnvironmentImpl<'e> {
    // tcb.ts:458 nextIds
    next_pipe_inst_id: Cell<u32>,
    next_type_ctor_id: Cell<u32>,

    type_ctors: RefCell<Map<TcbReferenceKey, String>>,
    type_ctor_statements: RefCell<Vec<TcbExpr>>,

    pipe_insts: RefCell<Map<TcbReferenceKey, String>>,
    pipe_inst_statements: RefCell<Vec<TcbExpr>>,

    config: &'e TypeCheckingConfig,
    import_manager: &'e TcbImportManager,
    is_inline: bool,
    type_parameter_refs: Option<&'e TypeParameterRefs>,
}

impl<'e> TcbEnvironmentImpl<'e> {
    // tcb.ts:469 constructor
    pub fn new(config: &'e TypeCheckingConfig, import_manager: &'e TcbImportManager, is_inline: bool) -> Self {
        Self {
            next_pipe_inst_id: Cell::new(1),
            next_type_ctor_id: Cell::new(1),
            type_ctors: RefCell::default(),
            type_ctor_statements: RefCell::default(),
            pipe_insts: RefCell::default(),
            pipe_inst_statements: RefCell::default(),
            config,
            import_manager,
            is_inline,
            type_parameter_refs: None,
        }
    }

    /// The directives' extended type parameters, by `ref.key` (`tcb_adapter.rs`)
    #[must_use]
    pub fn with_type_parameter_refs(mut self, type_parameter_refs: &'e TypeParameterRefs) -> Self {
        self.type_parameter_refs = Some(type_parameter_refs);
        self
    }

    /// `dir.typeParameters` as TypeScript sees them at run time: `ExtendedTcbTypeParameter[]`
    fn extended_type_parameters(&self, dir: &TcbDirectiveMetadata) -> Option<Vec<ExtendedTcbTypeParameter>> {
        let type_parameters = dir.type_parameters.as_ref()?;
        if let Some(extended) = self.type_parameter_refs.and_then(|refs| refs.get(&dir.r#ref.key))
            && extended.len() == type_parameters.len()
        {
            return Some(extended.clone());
        }
        Some(type_parameters.iter().map(ExtendedTcbTypeParameter::from_tcb_type_parameter).collect())
    }

    // tcb.ts:540 getFileLevelStatements
    pub fn get_file_level_statements(&self) -> Vec<TcbExpr> {
        let mut statements = self.pipe_inst_statements.borrow().clone();
        statements.extend(self.type_ctor_statements.borrow().iter().cloned());
        statements
    }
}

impl TcbEnvironment for TcbEnvironmentImpl<'_> {
    fn config(&self) -> &TypeCheckingConfig {
        self.config
    }

    // tcb.ts:475 referenceTcbValue
    fn reference_tcb_value(&self, r#ref: &TcbReferenceMetadata) -> TcbExpr {
        if r#ref.unexported_diagnostic.is_some() || r#ref.is_local || r#ref.module_name.is_none() {
            if let Some(unexported_diagnostic) = &r#ref.unexported_diagnostic {
                panic!("Unable to import symbol {}: {unexported_diagnostic}", r#ref.name);
            }
            return TcbExpr::new(r#ref.name.clone());
        }
        let module_name = r#ref.module_name.as_deref().unwrap_or_default();
        self.reference_external_symbol(module_name, &r#ref.name)
    }

    // tcb.ts:485 referenceExternalSymbol
    fn reference_external_symbol(&self, module_name: &str, name: &str) -> TcbExpr {
        let alias = self.import_manager.add_import(module_name);
        TcbExpr::new(format!("{alias}.{name}"))
    }

    // tcb.ts:490 typeCtorFor
    fn type_ctor_for(&self, dir: &TcbDirectiveMetadata) -> TcbExpr {
        if let Some(type_ctor) = self.type_ctors.borrow().get(&dir.r#ref.key) {
            return TcbExpr::new(type_ctor.clone());
        }

        if dir.requires_inline_type_ctor {
            let type_ctor_expr = format!("{}.ngTypeCtor", self.reference_tcb_value(&dir.r#ref).p());
            self.type_ctors.borrow_mut().insert(dir.r#ref.key.clone(), type_ctor_expr.clone());
            TcbExpr::new(type_ctor_expr)
        } else {
            let fn_name = format!("_ctor{}", self.next_type_ctor_id.get());
            self.next_type_ctor_id.set(self.next_type_ctor_id.get() + 1);
            let node_type_ref = self.reference_tcb_value(&dir.r#ref);
            let meta = TypeCtorMetadata {
                fn_name: fn_name.clone(),
                body: true,
                inputs: dir.inputs.clone(),
                coerced_input_fields: dir.coerced_input_fields.clone(),
            };

            let type_params = self.extended_type_parameters(dir).and_then(|type_parameters| {
                qualify_type_parameters(Some(&type_parameters), self, dir.r#ref.module_name.as_deref())
            });
            let type_params: Option<Vec<TcbTypeParameter>> = type_params
                .map(|params| params.iter().map(ExtendedTcbTypeParameter::to_tcb_type_parameter).collect());
            let type_ctor = generate_type_ctor_declaration_fn(self, &meta, &node_type_ref, type_params.as_deref());
            self.type_ctor_statements.borrow_mut().push(type_ctor);
            self.type_ctors.borrow_mut().insert(dir.r#ref.key.clone(), fn_name.clone());
            TcbExpr::new(fn_name)
        }
    }

    // tcb.ts:521 pipeInst
    fn pipe_inst(&self, pipe: &TcbPipeMetadata) -> TcbExpr {
        if let Some(pipe_inst) = self.pipe_insts.borrow().get(&pipe.r#ref.key) {
            return TcbExpr::new(pipe_inst.clone());
        }

        let pipe_type = self.reference_tcb_value(&pipe.r#ref);
        let pipe_inst_id = format!("_pipe{}", self.next_pipe_inst_id.get());
        self.next_pipe_inst_id.set(self.next_pipe_inst_id.get() + 1);
        self.pipe_insts.borrow_mut().insert(pipe.r#ref.key.clone(), pipe_inst_id.clone());
        self.pipe_inst_statements
            .borrow_mut()
            .push(TcbExpr::new(format!("var {pipe_inst_id} = null! as {}", pipe_type.p())));
        TcbExpr::new(pipe_inst_id)
    }

    // tcb.ts:533 getPreludeStatements
    fn get_prelude_statements(&self) -> Vec<TcbExpr> {
        if self.is_inline {
            return self.get_file_level_statements();
        }
        vec![]
    }
}

// tcb.ts:545 prepareTcbTargets. Templates and host elements live in `allocator`.
pub fn prepare_tcb_targets<'a>(
    allocator: &'a Allocator,
    file_path: &str,
    tcb_targets: Vec<TcbTargetInput<'a>>,
    content: &str,
    config: &NgpTypeCheckingConfig,
    classes: Option<&[ClassMetadata]>,
) -> Option<PreparedTcbData<'a>> {
    if tcb_targets.is_empty() {
        return None;
    }

    let mut local_classes: Map<String, &ClassMetadata> = Map::default();
    if let Some(classes) = classes {
        for cls in classes {
            if let Some(class_name) = cls.class_name.as_ref().filter(|name| !name.is_empty()) {
                local_classes.insert(class_name.clone(), cls);
            }
        }
    } else {
        for target in &tcb_targets {
            if !target.class_name.is_empty() {
                local_classes.insert(target.class_name.clone(), &target.class_meta);
            }
        }
    }

    let mut requires_inline = tcb_targets.iter().any(|target| requires_inline_declaration(Some(&target.class_meta)));

    let mut intermediate_targets = vec![];

    for (i, comp) in tcb_targets.iter().enumerate() {
        let type_check_id: TypeCheckId = format!("tcb{}", i + 1);
        let adapted = adapt_tcb_input(allocator, comp, &type_check_id, file_path, config, requires_inline, content);

        if !requires_inline
            && requires_inline_type_check_block(comp, &*adapted.tcb_meta.bound_target, &local_classes)
        {
            requires_inline = true;
        }
        intermediate_targets.push((type_check_id, adapted));
    }

    let mut bound_target_map: Map<String, Rc<dyn BoundTarget<'a, DirectiveRef> + 'a>> = Map::default();
    let mut host_elements_map: Map<String, Option<&'a R3HostElement<'a>>> = Map::default();
    let mut type_check_id_map: Map<String, TypeCheckId> = Map::default();
    let mut targets = vec![];

    drop(local_classes);
    for (comp, (type_check_id, intermediate)) in tcb_targets.into_iter().zip(intermediate_targets) {
        let adapted = if requires_inline {
            adapt_tcb_input(allocator, &comp, &type_check_id, file_path, config, true, content)
        } else {
            intermediate
        };

        let class_key = make_class_key(Some(&comp.class_name), comp.class_meta.span.start);
        bound_target_map.insert(class_key.clone(), Rc::clone(&adapted.tcb_meta.bound_target));
        host_elements_map.insert(class_key.clone(), adapted.host_element);
        type_check_id_map.insert(class_key, type_check_id.clone());
        targets.push(PreparedTcbTarget {
            comp,
            type_check_id,
            tcb_meta: adapted.tcb_meta,
            target_meta: adapted.target_meta,
            target_type_parameters: adapted.target_type_parameters,
            type_parameter_refs: adapted.type_parameter_refs,
        });
    }

    Some(PreparedTcbData { targets, bound_target_map, host_elements_map, type_check_id_map, requires_inline })
}

/// A panic's message: `e?.message || e`
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown error".to_string()
    }
}

// tcb.ts:610 generateTcbCode. A panic in the generator (TypeScript's `throw`) is caught, as TypeScript catches the
// error, and reported as a diagnostic (8000) on the class.
pub fn generate_tcb_code(
    _file_path: &str,
    prepared_data: &PreparedTcbData<'_>,
    config: &TypeCheckingConfig,
) -> Option<TcbResult> {
    let PreparedTcbData { targets, requires_inline, .. } = prepared_data;
    let requires_inline = *requires_inline;
    if targets.is_empty() {
        return None;
    }

    let import_manager = TcbImportManager::new();
    let oob_recorder = OutOfBandDiagnosticRecorderImpl::new();
    let dom_schema_checker = RegistryDomSchemaChecker::new();
    let mut semantics_diagnostics: Vec<Diagnostic> = vec![];

    let mut code = String::new();
    let mut function_strings = String::new();
    let mut tcb_map: Map<String, String> = Map::default();
    let mut template_sources: Map<TypeCheckId, TcbTemplateSource> = Map::default();

    // (every target's extended type parameters: one environment may serve them all)
    let mut type_parameter_refs = TypeParameterRefs::default();
    for target in targets {
        for (key, params) in &target.type_parameter_refs {
            type_parameter_refs.entry(key.clone()).or_insert_with(|| params.clone());
        }
    }

    let shared_env = (!requires_inline)
        .then(|| TcbEnvironmentImpl::new(config, &import_manager, false).with_type_parameter_refs(&type_parameter_refs));

    for target in targets {
        let PreparedTcbTarget { comp, type_check_id, tcb_meta, target_meta, target_type_parameters, .. } = target;
        let own_env;
        let env = if let Some(shared_env) = &shared_env {
            shared_env
        } else {
            own_env =
                TcbEnvironmentImpl::new(config, &import_manager, true).with_type_parameter_refs(&type_parameter_refs);
            &own_env
        };

        let target_type_params = target_type_parameters.as_ref().and_then(|params| {
            qualify_type_parameters(Some(params), env, target_meta.r#ref.module_name.as_deref())
        });
        let qualified_target_meta = match &target_type_params {
            Some(params) => TcbComponentMetadata {
                type_parameters: Some(params.iter().map(ExtendedTcbTypeParameter::to_tcb_type_parameter).collect()),
                ..target_meta.clone()
            },
            None => target_meta.clone(),
        };

        oob_recorder.set_source_text(comp.template_source);
        let tcb_meta = tcb_meta.as_tcb_meta();
        let generated = crate::span::with_source_text(comp.template_source, || {
            semantics_diagnostics.extend(get_template_semantics_diagnostics(
                type_check_id,
                comp.template_nodes,
                tcb_meta.bound_target,
            ));

            catch_unwind(AssertUnwindSafe(|| {
                generate_type_check_block(
                    env,
                    &qualified_target_meta,
                    &format!("_{type_check_id}"),
                    &tcb_meta,
                    &dom_schema_checker,
                    &oob_recorder,
                )
            }))
        });
        let tcb_string = match generated {
            Ok(tcb_string) => tcb_string,
            Err(e) => {
                oob_recorder.error(
                    type_check_id,
                    format!("Error generating TCB for {}: {}", comp.class_name, panic_message(&*e)),
                    AbsoluteSpan { start: comp.class_meta.span.start, end: comp.class_meta.span.end },
                );
                continue;
            }
        };

        function_strings.push_str(&tcb_string);
        function_strings.push_str("\n\n");
        let class_key = make_class_key(Some(&comp.class_name), comp.class_meta.span.start);
        tcb_map.insert(class_key, tcb_string);
        if let Some(template_source) = get_tcb_template_source(comp.class_meta.component.as_ref()) {
            template_sources.insert(type_check_id.clone(), template_source);
        }
    }

    if let Some(shared_env) = &shared_env {
        let prelude_statements = shared_env.get_file_level_statements();
        let prelude_str = print_statements(&prelude_statements);
        code.push_str(&prelude_str);
        code.push('\n');
        code.push_str(&function_strings);
    } else {
        code.push_str(&function_strings);
    }

    let imports_str = import_manager.generate_import_statements();

    let mut diagnostics: Vec<Diagnostic> = dom_schema_checker.diagnostics().clone();
    diagnostics.extend(oob_recorder.diagnostics().iter().cloned());
    diagnostics.extend(semantics_diagnostics);

    Some(TcbResult {
        code,
        imports: Some(imports_str),
        tcb_map,
        template_sources,
        diagnostics,
        is_inline: requires_inline,
    })
}

// tcb.ts:697 combineTcbContent. `inlineInfo`'s `sourceContent` and `classes` are its last two parameters.
pub fn combine_tcb_content(
    tcb_result: &TcbResult,
    file_path: &str,
    source_content: &str,
    classes: &[ClassMetadata],
) -> String {
    let generated_comment = format!("/**\n * TCB for {file_path}\n * @generated\n */\n\n");
    let imports_str = tcb_result.imports.as_deref().unwrap_or("");
    let separator = if imports_str.is_empty() { "" } else { "\n" };
    let tcb_header = format!("{generated_comment}{imports_str}{separator}");
    let tcb_footer = serialize_tcb_template_sources(&tcb_result.template_sources);

    if tcb_result.is_inline {
        // `MagicString.appendLeft(classMeta.span.end, ...)` for each class, in order
        let mut insertions: Vec<(usize, String)> = vec![];
        for class_meta in classes {
            let class_key = make_class_key(Some(class_meta.class_name.as_deref().unwrap_or("")), class_meta.span.start);
            if let Some(tcb_code) = tcb_result.tcb_map.get(&class_key).filter(|code| !code.is_empty()) {
                let position = crate::ngp::tcb_util::utf16_to_byte_offset(source_content, class_meta.span.end);
                insertions.push((position as usize, format!("\n\n{tcb_code}")));
            }
        }
        insertions.sort_by_key(|(position, _)| *position);
        let mut s = String::with_capacity(source_content.len());
        let mut last = 0;
        for (position, text) in insertions {
            s.push_str(&source_content[last..position]);
            s.push_str(&text);
            last = position;
        }
        s.push_str(&source_content[last..]);
        format!("{tcb_header}{s}{tcb_footer}")
    } else {
        format!("{tcb_header}{}{tcb_footer}", tcb_result.code)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use ng_analyze::{SpanMetadata, TypeRefMetadata};

    use super::*;
    use crate::angular::property_mapping::{ClassPropertyMapping, InputOrOutput};
    use crate::typecheck::api::{Set, TcbInputMapping};

    fn config() -> NgpTypeCheckingConfig {
        build_type_checking_config(&NgpCompilerOptions::default(), false, false)
    }

    #[test]
    fn type_checking_config_follows_the_options() {
        let strict = config();
        assert!(strict.check_type_of_input_bindings && strict.use_context_generic_type);
        assert_eq!(strict.control_flow_preventing_content_projection, DiagnosticCategoryLabel::Warning);
        assert_eq!(strict.type_check_host_bindings, Some(true));

        let options: NgpCompilerOptions = serde_json::from_str(
            r#"{"strictTemplates": false, "strictInputTypes": true, "annotateForClosureCompiler": true,
                "typeCheckHostBindings": false, "extendedDiagnostics": {"defaultCategory": "error",
                "checks": {"unusedStandaloneImports": "suppress"}}}"#,
        )
        .unwrap();
        let loose = build_type_checking_config(&options, true, false);
        assert!(loose.check_type_of_input_bindings && loose.apply_template_context_guards);
        assert!(!loose.check_template_bodies);
        assert!(loose.always_check_schema_in_template_bodies && loose.enable_template_type_checker);
        assert_eq!(loose.control_flow_preventing_content_projection, DiagnosticCategoryLabel::Error);
        assert_eq!(loose.unused_standalone_imports, DiagnosticCategoryLabel::Suppress);
        assert_eq!(loose.type_check_host_bindings, Some(false));
    }

    #[test]
    fn imports_are_numbered_once_per_module() {
        let manager = TcbImportManager::new();
        assert_eq!(manager.add_import("@angular/core"), "i0");
        assert_eq!(manager.add_import("./a"), "i1");
        assert_eq!(manager.add_import("@angular/core"), "i0");
        assert_eq!(
            manager.generate_import_statements(),
            "import * as i0 from '@angular/core';\nimport * as i1 from './a';\n"
        );
    }

    #[test]
    fn type_parameters_are_qualified_back_to_front() {
        let config = config();
        let manager = TcbImportManager::new();
        let env = TcbEnvironmentImpl::new(&config, &manager, false);
        let type_ref = |symbol: &str, start: u32, module: &str| TypeRefMetadata {
            name: symbol.to_string(),
            module_specifier: module.to_string(),
            symbol: symbol.to_string(),
            span: Some(SpanMetadata { start, end: start + symbol.len() as u32 }),
        };
        let params = vec![ExtendedTcbTypeParameter {
            name: "T".to_string(),
            representation: "T extends Foo<Bar>".to_string(),
            representation_with_default: "T extends Foo<Bar> = Foo<Bar>".to_string(),
            type_refs: Some(vec![type_ref("Foo", 10, ""), type_ref("Bar", 14, "./bar")]),
        }];
        let qualified = qualify_type_parameters(Some(&params), &env, Some("./foo")).unwrap();
        assert_eq!(qualified[0].representation, "T extends i1.Foo<i0.Bar>");
        assert_eq!(qualified[0].representation_with_default, "T extends i1.Foo<i0.Bar> = Foo<Bar>");
        assert_eq!(qualify_type_parameters(Some(&[]), &env, None), Some(vec![]));
        assert_eq!(qualify_type_parameters(None, &env, None), None);
    }

    fn input(name: &str, is_signal: bool, transform_type: Option<&str>) -> (String, TcbInputMapping) {
        (
            name.to_string(),
            TcbInputMapping {
                input: InputOrOutput {
                    class_property_name: name.to_string(),
                    binding_property_name: name.to_string(),
                    is_signal,
                },
                required: false,
                transform_type: transform_type.map(str::to_string),
            },
        )
    }

    #[test]
    fn type_constructors() {
        let config = config();
        let manager = TcbImportManager::new();
        let env = TcbEnvironmentImpl::new(&config, &manager, false);
        let meta = TypeCtorMetadata {
            fn_name: "_ctor1".to_string(),
            body: true,
            inputs: ClassPropertyMapping::from_entries([
                input("plain", false, None),
                input("coerced", false, None),
                input("data-x", false, Some("string | number")),
                input("sig", true, None),
            ]),
            coerced_input_fields: ["coerced".to_string(), "data-x".to_string()].into_iter().collect::<Set<_>>(),
        };
        let params = [TcbTypeParameter {
            name: "T".to_string(),
            representation: "T".to_string(),
            representation_with_default: "T = any".to_string(),
        }];
        let expr = generate_type_ctor_declaration_fn(&env, &meta, &TcbExpr::new("i1.Dir"), Some(&params));
        assert_eq!(
            expr.p(),
            "const _ctor1: <T = any>(init: Pick<i1.Dir<T>, \"plain\"> & {\ncoerced: typeof i1.Dir.ngAcceptInputType_coerced;\n\"data-x\": string | number;\n} & i0.ɵUnwrapDirectiveSignalInputs<i1.Dir<T>, \"sig\">) => i1.Dir<T> = null!"
        );
        let empty = TypeCtorMetadata {
            fn_name: "_ctor2".to_string(),
            body: false,
            inputs: ClassPropertyMapping::empty(),
            coerced_input_fields: Set::default(),
        };
        assert_eq!(
            generate_type_ctor_declaration_fn(&env, &empty, &TcbExpr::new("Dir"), None).p(),
            "declare function _ctor2(init: {}): Dir"
        );
    }

    #[test]
    fn environment_references_and_pipes() {
        let config = config();
        let manager = TcbImportManager::new();
        let env = TcbEnvironmentImpl::new(&config, &manager, true);
        let pipe = TcbPipeMetadata {
            name: "date".to_string(),
            r#ref: TcbReferenceMetadata {
                name: "DatePipe".to_string(),
                module_name: Some("@angular/common".to_string()),
                is_local: false,
                unexported_diagnostic: None,
                key: "@angular/common#DatePipe".to_string(),
                node_name_span: None,
                node_file_path: None,
            },
            is_explicitly_deferred: false,
            deferred_blocks: None,
        };
        assert_eq!(env.pipe_inst(&pipe).p(), "_pipe1");
        assert_eq!(env.pipe_inst(&pipe).p(), "_pipe1");
        let prelude: Vec<_> = env.get_prelude_statements().iter().map(TcbExpr::p).collect();
        assert_eq!(prelude, ["var _pipe1 = null! as i0.DatePipe"]);
        let local = TcbReferenceMetadata { is_local: true, ..pipe.r#ref.clone() };
        assert_eq!(env.reference_tcb_value(&local).p(), "DatePipe");
        let shared = TcbEnvironmentImpl::new(&config, &manager, false);
        assert!(shared.get_prelude_statements().is_empty());
    }

    #[test]
    #[should_panic(expected = "Unable to import symbol Foo: not exported")]
    fn unexported_references_throw() {
        let config = config();
        let manager = TcbImportManager::new();
        let env = TcbEnvironmentImpl::new(&config, &manager, false);
        env.reference_tcb_value(&TcbReferenceMetadata {
            name: "Foo".to_string(),
            module_name: None,
            is_local: true,
            unexported_diagnostic: Some("not exported".to_string()),
            key: "a#Foo".to_string(),
            node_name_span: None,
            node_file_path: None,
        });
    }

    fn result(is_inline: bool, template_sources: Map<TypeCheckId, TcbTemplateSource>) -> TcbResult {
        TcbResult {
            code: "var _pipe1 = null! as i0.P;\n\nfunction _tcb1() {}\n\n".to_string(),
            imports: Some("import * as i0 from './p';\n".to_string()),
            tcb_map: [("C@0".to_string(), "function _tcb1() {}".to_string())].into_iter().collect(),
            template_sources,
            diagnostics: vec![],
            is_inline,
        }
    }

    #[test]
    fn combined_content_has_a_header_and_the_template_sources_footer() {
        let sources: Map<TypeCheckId, TcbTemplateSource> = [
            ("tcb1".to_string(), TcbTemplateSource::TemplateFile("/a/*/b.html".to_string())),
            ("tcb2".to_string(), TcbTemplateSource::TemplateExpression { start: 4, end: 9 }),
        ]
        .into_iter()
        .collect();
        let text = combine_tcb_content(&result(false, sources), "/a/c.ts", "", &[]);
        assert_eq!(
            text,
            "/**\n * TCB for /a/c.ts\n * @generated\n */\n\nimport * as i0 from './p';\n\nvar _pipe1 = null! as i0.P;\n\nfunction _tcb1() {}\n\n\n/*ngp-tcb-template-sources:{\"tcb1\":{\"templateFile\":\"/a/*\\/b.html\"},\"tcb2\":{\"templateExpression\":{\"start\":4,\"end\":9}}}*/\n"
        );
    }

    /// A `ClassMetadata` with nothing but its name and span
    pub fn class_meta(name: &str, start: u32, end: u32) -> ClassMetadata {
        ClassMetadata {
            symbol_id: 0,
            span: SpanMetadata { start, end },
            decorated_span: SpanMetadata { start, end },
            name_span: Some(SpanMetadata { start: start + 6, end: start + 6 + name.len() as u32 }),
            class_name: Some(name.to_string()),
            r#ref: None,
            constructor_params: None,
            member_decorators: None,
            later_declaration_references: vec![],
            top_level_references: None,
            injectable: None,
            service: None,
            component: None,
            directive: None,
            pipe: None,
            ng_module: None,
            fields: vec![],
            flattened_fields: None,
            inputs: vec![],
            outputs: vec![],
            queries: vec![],
            view_queries: vec![],
            removal_spans: vec![],
            nocollapse_insertions: vec![],
            coercion_members: String::new(),
            host_bindings: vec![],
            host_listeners: vec![],
            type_parameters: None,
            has_ng_template_context_guard: false,
            ng_template_guards: vec![],
            has_ng_field_directive: false,
            public_methods: vec![],
            super_class: None,
            uses_inheritance: false,
            uses_on_changes: false,
            is_exported: true,
            has_non_exported_bounds: false,
        }
    }

    #[test]
    fn inline_blocks_follow_their_classes() {
        let source = "class C {}\nconst x = 1;\n";
        let classes = [class_meta("C", 0, 10)];
        let text = combine_tcb_content(&result(true, Map::default()), "/a/c.ts", source, &classes);
        assert_eq!(
            text,
            "/**\n * TCB for /a/c.ts\n * @generated\n */\n\nimport * as i0 from './p';\n\nclass C {}\n\nfunction _tcb1() {}\nconst x = 1;\n"
        );
    }
}
