//! `packages/compiler-cli/preprocessor/src/diagnostic.ts`, the part type-check block generation uses: `Diagnostic`.

use crate::typecheck::api::TypeCheckId;
use crate::typecheck::oob::OutOfBandDiagnosticCategory;

// diagnostic.ts:12 Diagnostic: a diagnostic found while generating a type-check block. `start` and `end` are
// JavaScript string (UTF-16) indices into the text the block's template spans index into (the template file for an
// external template, the component's file otherwise; see `TcbTemplateSource`), as in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub type_check_id: TypeCheckId,
    pub category: OutOfBandDiagnosticCategory,
    pub code: Option<u32>,
    pub message: String,
    pub start: u32,
    pub end: u32,
}
