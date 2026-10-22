//! `packages/compiler/src/typecheck/ops/for_block.ts`: STUB by `tcb-core` (what `scope.rs` uses), to be replaced by
//! `tcb-ops`'s port.

use oxc_angular_compiler::ast::r3::R3ForLoopBlock;

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;

// for_block.ts:22 TcbForOfOp (stub)
pub struct TcbForOfOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _block: &'a R3ForLoopBlock<'a>,
}

impl<'a, 'b> TcbForOfOp<'a, 'b> {
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, block: &'a R3ForLoopBlock<'a>) -> Self {
        Self { _tcb: tcb, _scope: scope, _block: block }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbForOfOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}
