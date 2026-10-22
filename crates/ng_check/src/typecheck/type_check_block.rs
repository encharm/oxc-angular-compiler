//! `packages/compiler/src/typecheck/type_check_block.ts`: not ported yet (PORTING.md).
//!
//! STUB (ngc-glue): the signature `ngp/tcb.rs` calls. The merge drops this in favour of the port.

use crate::typecheck::api::{TcbComponentMetadata, TcbEnvironment, TcbTypeCheckBlockMetadata};
use crate::typecheck::oob::OutOfBandDiagnosticRecorder;
use crate::typecheck::schema::DomSchemaChecker;

// type_check_block.ts generateTypeCheckBlock (stub)
pub fn generate_type_check_block<'a, 'b>(
    _env: &'b dyn TcbEnvironment,
    _component: &TcbComponentMetadata,
    _name: &str,
    _meta: &'b TcbTypeCheckBlockMetadata<'a, 'b>,
    _dom_schema_checker: &'b dyn DomSchemaChecker<'a>,
    _oob_recorder: &'b dyn OutOfBandDiagnosticRecorder<'a>,
) -> String {
    panic!("stub: generate_type_check_block is not ported yet")
}
