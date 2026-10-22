//! `packages/compiler/src/schema/dom_element_schema_registry.ts`: not ported yet (PORTING.md).
//!
//! STUB (ngc-glue): the methods `ngp/dom_schema_checker.rs` calls. The merge drops this in favour of the port.

use crate::angular::core::SchemaMetadata;
use crate::angular::schema::element_schema_registry::ElementSchemaRegistry;

// dom_element_schema_registry.ts DomElementSchemaRegistry (stub)
#[derive(Debug, Default)]
pub struct DomElementSchemaRegistry;

impl DomElementSchemaRegistry {
    pub fn new() -> Self {
        Self
    }

    // dom_element_schema_registry.ts isKnownEventOfAnyElement (stub)
    pub fn is_known_event_of_any_element(&self, _event_name: &str) -> bool {
        panic!("stub: DomElementSchemaRegistry is not ported yet")
    }
}

impl ElementSchemaRegistry for DomElementSchemaRegistry {
    fn has_property(&self, _tag_name: &str, _prop_name: &str, _schema_metas: &[SchemaMetadata]) -> bool {
        panic!("stub: DomElementSchemaRegistry is not ported yet")
    }
    fn has_element(&self, _tag_name: &str, _schema_metas: &[SchemaMetadata]) -> bool {
        panic!("stub: DomElementSchemaRegistry is not ported yet")
    }
}
