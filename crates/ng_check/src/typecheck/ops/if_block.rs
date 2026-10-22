//! `packages/compiler/src/typecheck/ops/if_block.ts`: STUB by `tcb-core` (what `scope.rs` uses), to be replaced by
//! `tcb-ops`'s port.

use oxc_angular_compiler::ast::r3::R3IfBlock;

use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;

// if_block.ts:21 TcbIfBlockOp (stub)
pub struct TcbIfBlockOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _block: &'a R3IfBlock<'a>,
}

impl<'a, 'b> TcbIfBlockOp<'a, 'b> {
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, block: &'a R3IfBlock<'a>) -> Self {
        Self { _tcb: tcb, _scope: scope, _block: block }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbIfBlockOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}
