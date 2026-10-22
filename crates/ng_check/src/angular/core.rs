//! `packages/compiler/src/core.ts`, the part the type checker uses: schema metadata.

// core.ts:65 SchemaMetadata
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchemaMetadata {
    pub name: String,
}

// core.ts:69 CUSTOM_ELEMENTS_SCHEMA
pub const CUSTOM_ELEMENTS_SCHEMA: &str = "custom-elements";

// core.ts:73 NO_ERRORS_SCHEMA
pub const NO_ERRORS_SCHEMA: &str = "no-errors-schema";

impl SchemaMetadata {
    pub fn is_custom_elements(&self) -> bool {
        self.name == CUSTOM_ELEMENTS_SCHEMA
    }

    pub fn is_no_errors(&self) -> bool {
        self.name == NO_ERRORS_SCHEMA
    }
}
