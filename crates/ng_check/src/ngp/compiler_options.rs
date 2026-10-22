//! `packages/compiler-cli/preprocessor/src/compiler_options.ts`: the compiler options the preprocessor reads, from a
//! tsconfig's merged `angularCompilerOptions` (deserialized from its JSON).

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;
use serde::Deserialize;

use crate::typecheck::api;

// compiler_options.ts:13 DiagnosticCategoryLabel: `'warning' | 'error' | 'suppress'`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticCategoryLabel {
    Warning,
    Error,
    Suppress,
}

impl From<DiagnosticCategoryLabel> for api::DiagnosticCategoryLabel {
    fn from(label: DiagnosticCategoryLabel) -> Self {
        match label {
            DiagnosticCategoryLabel::Warning => Self::Warning,
            DiagnosticCategoryLabel::Error => Self::Error,
            DiagnosticCategoryLabel::Suppress => Self::Suppress,
        }
    }
}

// compiler_options.ts:19 ExtendedDiagnosticsOptions
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedDiagnosticsOptions {
    pub default_category: Option<DiagnosticCategoryLabel>,
    pub checks: Option<ExtendedDiagnosticsChecks>,
}

/// `ExtendedDiagnosticsOptions['checks']`
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedDiagnosticsChecks {
    pub control_flow_preventing_content_projection: Option<DiagnosticCategoryLabel>,
    pub unused_standalone_imports: Option<DiagnosticCategoryLabel>,
    #[serde(flatten)]
    pub others: IndexMap<String, DiagnosticCategoryLabel, FxBuildHasher>,
}

// compiler_options.ts:34 NgpCompilerOptions
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NgpCompilerOptions {
    // Type-checking (`TypeCheckingOptions`).
    pub strict_templates: Option<bool>,
    pub strict_input_types: Option<bool>,
    pub strict_input_access_modifiers: Option<bool>,
    pub strict_null_input_types: Option<bool>,
    pub strict_output_event_types: Option<bool>,
    pub strict_dom_event_types: Option<bool>,
    pub strict_safe_navigation_types: Option<bool>,
    pub strict_dom_local_ref_types: Option<bool>,
    pub strict_attribute_types: Option<bool>,
    pub strict_context_generics: Option<bool>,
    pub strict_literal_types: Option<bool>,
    pub type_check_host_bindings: Option<bool>,

    // Diagnostics (`DiagnosticOptions`).
    pub extended_diagnostics: Option<ExtendedDiagnosticsOptions>,

    // Emit and Bazel/google3 (`BazelAndG3Options`, `InternalOptions`).
    pub annotate_for_closure_compiler: Option<bool>,
    #[serde(rename = "_experimentalAllowEmitDeclarationOnly")]
    pub experimental_allow_emit_declaration_only: Option<bool>,
    pub only_publish_public_typings_for_ng_modules: Option<bool>,
    pub workspace_name: Option<String>,
    #[serde(rename = "_useHostForImportGeneration")]
    pub use_host_for_import_generation: Option<bool>,
    #[serde(rename = "_useHostForImportAndAliasGeneration")]
    pub use_host_for_import_and_alias_generation: Option<bool>,

    // Compilation (`MiscOptions`, `TargetOptions`, `InternalOptions`).
    pub legacy_optional_chaining: Option<bool>,
    pub only_explicit_defer_dependency_imports: Option<bool>,
    pub enable_template_source_locations: Option<bool>,
    pub forbid_orphan_components: Option<bool>,
    pub support_jit_mode: Option<bool>,
    pub support_test_bed: Option<bool>,
    pub preserve_whitespaces: Option<bool>,
    #[serde(rename = "_enableHmr")]
    pub enable_hmr: Option<bool>,
    #[serde(rename = "_compilePoisonedComponents")]
    pub compile_poisoned_components: Option<bool>,

    // i18n (`I18nOptions`).
    pub enable_i18n_legacy_message_id_format: Option<bool>,
    pub i18n_use_external_ids: Option<bool>,
    #[serde(rename = "i18nNormalizeLineEndingsInICUs")]
    pub i18n_normalize_line_endings_in_icus: Option<bool>,
    pub i18n_preserve_whitespace_for_legacy_extraction: Option<bool>,

    // TypeScript `compilerOptions` that ngtsc reads off the same merged options object.
    pub emit_declaration_only: Option<bool>,
    pub root_dirs: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_angular_compiler_options() {
        let options: NgpCompilerOptions = serde_json::from_str(
            r#"{"strictTemplates": false, "_compilePoisonedComponents": true,
                "extendedDiagnostics": {"defaultCategory": "error", "checks": {"nullishCoalescingNotNullable": "suppress"}}}"#,
        )
        .unwrap();
        assert_eq!(options.strict_templates, Some(false));
        assert_eq!(options.compile_poisoned_components, Some(true));
        let extended = options.extended_diagnostics.unwrap();
        assert_eq!(extended.default_category, Some(DiagnosticCategoryLabel::Error));
        let checks = extended.checks.unwrap();
        assert_eq!(checks.others.get("nullishCoalescingNotNullable"), Some(&DiagnosticCategoryLabel::Suppress));
    }
}
