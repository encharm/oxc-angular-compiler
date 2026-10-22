//! `packages/compiler/src/render3/view/t2_binder.ts`: not ported yet (PORTING.md).
//!
//! STUB (tcb-ops): `createCssSelectorFromNode` (`render3/view/util.ts:170`, which t2_binder.ts imports), called by
//! `typecheck/ops/content_projection.rs`; the real port replaces this.

use crate::angular::directive_matching::CssSelector;
use crate::typecheck::oob::ElementOrTemplate;

// render3/view/util.ts:170 createCssSelectorFromNode (STUB)
pub fn create_css_selector_from_node(_node: ElementOrTemplate<'_>) -> CssSelector {
    todo!()
}
