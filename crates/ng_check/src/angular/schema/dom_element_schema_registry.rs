//! `packages/compiler/src/schema/dom_element_schema_registry.ts`: STUB by `tcb-core` (what `typecheck` uses), to be
//! replaced by `matching`'s port.

use crate::angular::schema::element_schema_registry::ElementSchemaRegistry;

// dom_element_schema_registry.ts DomElementSchemaRegistry (stub)
#[derive(Debug, Default)]
pub struct DomElementSchemaRegistry;

impl DomElementSchemaRegistry {
    pub fn new() -> Self {
        Self
    }
}

impl ElementSchemaRegistry for DomElementSchemaRegistry {
    fn get_mapped_prop_name(&self, _prop_name: &str) -> String {
        todo!("ported by matching")
    }
}
