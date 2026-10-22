//! `packages/compiler/src/schema/element_schema_registry.ts`: not ported yet (PORTING.md).
//!
//! STUB (ngc-glue): the methods `ngp/dom_schema_checker.rs` calls. The merge drops this in favour of the port.

use crate::angular::core::SchemaMetadata;

// element_schema_registry.ts:11 ElementSchemaRegistry (stub: the methods the DOM schema checker calls)
pub trait ElementSchemaRegistry {
    fn has_property(&self, tag_name: &str, prop_name: &str, schema_metas: &[SchemaMetadata]) -> bool;
    fn has_element(&self, tag_name: &str, schema_metas: &[SchemaMetadata]) -> bool;
}
