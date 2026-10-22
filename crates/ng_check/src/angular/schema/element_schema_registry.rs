//! `packages/compiler/src/schema/element_schema_registry.ts`

use crate::angular::core::SchemaMetadata;
use crate::angular::schema::dom_security_schema::SecurityContext;

/// `{error: boolean; msg?: string}`, the result of `validateProperty` and `validateAttribute`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub error: bool,
    pub msg: Option<String>,
}

/// `string | number`, the value `normalizeAnimationStyleValue` takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StyleValue<'s> {
    String(&'s str),
    Number(f64),
}

/// `{error: string; value: string}`, the result of `normalizeAnimationStyleValue` (`error` is `null` when there is
/// none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedStyleValue {
    pub error: Option<String>,
    pub value: String,
}

// element_schema_registry.ts:11 ElementSchemaRegistry
pub trait ElementSchemaRegistry {
    fn has_property(
        &self,
        tag_name: &str,
        prop_name: &str,
        schema_metas: &[SchemaMetadata],
    ) -> bool;
    fn has_element(&self, tag_name: &str, schema_metas: &[SchemaMetadata]) -> bool;
    fn security_context(
        &self,
        element_name: &str,
        prop_name: &str,
        is_attribute: bool,
    ) -> SecurityContext;
    fn all_known_element_names(&self) -> Vec<String>;
    fn get_mapped_prop_name(&self, prop_name: &str) -> String;
    fn get_default_component_element_name(&self) -> String;
    fn validate_property(&self, name: &str) -> ValidationResult;
    fn validate_attribute(&self, name: &str) -> ValidationResult;
    fn normalize_animation_style_property(&self, prop_name: &str) -> String;
    fn normalize_animation_style_value(
        &self,
        camel_case_prop: &str,
        user_provided_prop: &str,
        val: StyleValue<'_>,
    ) -> NormalizedStyleValue;
}
