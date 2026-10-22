//! `packages/compiler/src/typecheck/api.ts`

use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};
use rustc_hash::FxBuildHasher;

use crate::angular::core::SchemaMetadata;
use crate::angular::property_mapping::{AsInputOrOutput, ClassPropertyMapping, ClassPropertyName, InputOrOutput};
use crate::angular::t2_api::{BoundTarget, DirectiveMeta, LegacyAnimationTriggerNames, MatchSource};
use crate::angular::t2_binder::DirectiveSpread;
use crate::typecheck::ops::codegen::{SpanInfo, TcbExpr};

pub type Set<T> = IndexSet<T, FxBuildHasher>;
pub type Map<K, V> = IndexMap<K, V, FxBuildHasher>;

// api.ts:15 TypeCtorMetadata
#[derive(Debug, Clone)]
pub struct TypeCtorMetadata {
    pub fn_name: String,
    pub body: bool,
    /// `fields.inputs`
    pub inputs: ClassPropertyMapping<TcbInputMapping>,
    pub coerced_input_fields: Set<String>,
}

// api.ts:37 TcbReferenceMetadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcbReferenceMetadata {
    pub name: String,
    pub module_name: Option<String>,
    pub is_local: bool,
    pub unexported_diagnostic: Option<String>,
    pub key: TcbReferenceKey,
    pub node_name_span: Option<SpanInfo>,
    pub node_file_path: Option<String>,
}

// api.ts:60 TcbReferenceKey
pub type TcbReferenceKey = String;

// api.ts:63 TcbTypeParameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcbTypeParameter {
    pub name: String,
    pub representation: String,
    pub representation_with_default: String,
}

// api.ts:69 TcbInputMapping
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcbInputMapping {
    pub input: InputOrOutput,
    pub required: bool,
    pub transform_type: Option<String>,
}

impl AsInputOrOutput for TcbInputMapping {
    fn input_or_output(&self) -> &InputOrOutput {
        &self.input
    }
}

// api.ts:78 TcbPipeMetadata
#[derive(Debug, Clone)]
pub struct TcbPipeMetadata {
    pub name: String,
    pub r#ref: TcbReferenceMetadata,
    pub is_explicitly_deferred: bool,
    pub deferred_blocks: Option<Set<String>>,
}

// api.ts:88 TemplateGuardMeta
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateGuardMeta {
    pub input_name: String,
    pub r#type: TemplateGuardType,
}

/// `'invocation' | 'binding'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateGuardType {
    Invocation,
    Binding,
}

// api.ts:104 TcbDirectiveMetadata
#[derive(Debug, Clone)]
pub struct TcbDirectiveMetadata {
    pub r#ref: TcbReferenceMetadata,
    pub name: String,
    pub selector: Option<String>,
    pub is_component: bool,
    pub is_generic: bool,
    pub is_structural: bool,
    pub is_standalone: bool,
    pub is_explicitly_deferred: bool,
    pub deferred_blocks: Option<Set<String>>,
    pub preserve_whitespaces: bool,
    pub export_as: Option<Vec<String>>,
    pub match_source: MatchSource,
    pub type_parameters: Option<Vec<TcbTypeParameter>>,
    pub inputs: ClassPropertyMapping<TcbInputMapping>,
    pub outputs: ClassPropertyMapping<InputOrOutput>,
    pub requires_inline_type_ctor: bool,
    pub ng_template_guards: Vec<TemplateGuardMeta>,
    pub has_ng_template_context_guard: bool,
    pub has_ng_field_directive: bool,
    pub coerced_input_fields: Set<ClassPropertyName>,
    pub restricted_input_fields: Set<ClassPropertyName>,
    pub string_literal_input_fields: Set<ClassPropertyName>,
    pub undeclared_input_fields: Set<ClassPropertyName>,
    pub public_methods: Set<String>,
    pub ng_content_selectors: Option<Vec<String>>,
    pub animation_trigger_names: Option<LegacyAnimationTriggerNames>,
}

/// The directive type of the generator's bound targets: shared, compared by identity (`Rc::ptr_eq`).
pub type DirectiveRef = Rc<TcbDirectiveMetadata>;

/// A directive as a map key: equal and hashed by identity, as a JavaScript `Map` keys an object.
#[derive(Debug, Clone)]
pub struct DirectiveKey(pub DirectiveRef);

impl PartialEq for DirectiveKey {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for DirectiveKey {}

impl std::hash::Hash for DirectiveKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Rc::as_ptr(&self.0), state);
    }
}

impl DirectiveMeta for TcbDirectiveMetadata {
    type Input = TcbInputMapping;

    fn name(&self) -> &str {
        &self.name
    }
    fn ref_key(&self) -> &str {
        &self.r#ref.key
    }
    fn selector(&self) -> Option<&str> {
        self.selector.as_deref()
    }
    fn is_component(&self) -> bool {
        self.is_component
    }
    fn inputs(&self) -> &ClassPropertyMapping<TcbInputMapping> {
        &self.inputs
    }
    fn outputs(&self) -> &ClassPropertyMapping<InputOrOutput> {
        &self.outputs
    }
    fn export_as(&self) -> Option<&[String]> {
        self.export_as.as_deref()
    }
    fn is_structural(&self) -> bool {
        self.is_structural
    }
    fn ng_content_selectors(&self) -> Option<&[String]> {
        self.ng_content_selectors.as_deref()
    }
    fn preserve_whitespaces(&self) -> bool {
        self.preserve_whitespaces
    }
    fn animation_trigger_names(&self) -> Option<&LegacyAnimationTriggerNames> {
        self.animation_trigger_names.as_ref()
    }
    fn match_source(&self) -> MatchSource {
        self.match_source
    }
}

/// `{...dir, inputs, outputs}` (t2_binder.ts:818), for the binder's host directive merging.
impl DirectiveSpread for TcbDirectiveMetadata {
    fn with_mappings(
        &self,
        inputs: ClassPropertyMapping<TcbInputMapping>,
        outputs: ClassPropertyMapping<InputOrOutput>,
    ) -> Self {
        Self { inputs, outputs, ..self.clone() }
    }
}

// api.ts:135 TcbComponentMetadata
#[derive(Debug, Clone)]
pub struct TcbComponentMetadata {
    pub r#ref: TcbReferenceMetadata,
    pub type_parameters: Option<Vec<TcbTypeParameter>>,
    pub type_arguments: Option<Vec<String>>,
}

// api.ts:141 TcbTypeCheckBlockMetadata
pub struct TcbTypeCheckBlockMetadata<'a, 'b> {
    pub id: TypeCheckId,
    pub bound_target: &'b dyn BoundTarget<'a, DirectiveRef>,
    pub pipes: Option<Map<String, TcbPipeMetadata>>,
    pub schemas: Vec<SchemaMetadata>,
    pub is_standalone: bool,
    pub preserve_whitespaces: bool,
}

// api.ts:150 TypeCheckId
pub type TypeCheckId = String;

// api.ts:157 TcbEnvironment: the methods take `&self` (an implementation keeps its imports in a cell)
pub trait TcbEnvironment {
    fn config(&self) -> &TypeCheckingConfig;
    fn reference_tcb_value(&self, r#ref: &TcbReferenceMetadata) -> TcbExpr;
    fn reference_external_symbol(&self, module_name: &str, name: &str) -> TcbExpr;
    fn pipe_inst(&self, pipe_meta: &TcbPipeMetadata) -> TcbExpr;
    fn type_ctor_for(&self, dir: &TcbDirectiveMetadata) -> TcbExpr;
    fn get_prelude_statements(&self) -> Vec<TcbExpr>;
}

/// `'error' | 'warning' | 'suppress'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCategoryLabel {
    Error,
    Warning,
    Suppress,
}

// api.ts:166 TypeCheckingConfig
#[derive(Debug, Clone)]
pub struct TypeCheckingConfig {
    pub check_type_of_input_bindings: bool,
    pub honor_access_modifiers_for_input_bindings: bool,
    pub strict_null_input_bindings: bool,
    pub check_type_of_attributes: bool,
    pub check_type_of_dom_bindings: bool,
    pub check_type_of_output_events: bool,
    pub check_type_of_animation_events: bool,
    pub check_type_of_dom_events: bool,
    pub check_unclaimed_event_names: bool,
    pub check_type_of_dom_references: bool,
    pub check_type_of_non_dom_references: bool,
    pub enable_template_type_checker: bool,
    pub check_type_of_pipes: bool,
    pub apply_template_context_guards: bool,
    pub strict_safe_navigation_types: bool,
    pub check_template_bodies: bool,
    pub always_check_schema_in_template_bodies: bool,
    pub control_flow_preventing_content_projection: DiagnosticCategoryLabel,
    pub unused_standalone_imports: DiagnosticCategoryLabel,
    pub use_context_generic_type: bool,
    pub strict_literal_types: bool,
    pub use_inline_type_constructors: bool,
    pub allow_signals_in_two_way_bindings: bool,
    pub allow_dom_event_assertion: bool,
    pub check_control_flow_bodies: bool,
    pub check_unknown_elements: bool,
}
