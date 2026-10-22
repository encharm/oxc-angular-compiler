//! `packages/compiler/src/typecheck/ops/content_projection.ts`: STUB by `tcb-core` (what `scope.rs` uses), to be
//! replaced by `tcb-ops`'s port.

use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::ops::base::{TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;

// content_projection.ts:42 TcbControlFlowContentProjectionOp (stub)
pub struct TcbControlFlowContentProjectionOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    /// `Element | Component`
    _element: DirectiveOwner<'a>,
    _ng_content_selectors: Vec<String>,
    _component_name: String,
}

impl<'a, 'b> TcbControlFlowContentProjectionOp<'a, 'b> {
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        element: DirectiveOwner<'a>,
        ng_content_selectors: Vec<String>,
        component_name: String,
    ) -> Self {
        Self { _tcb: tcb, _element: element, _ng_content_selectors: ng_content_selectors, _component_name: component_name }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbControlFlowContentProjectionOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}
