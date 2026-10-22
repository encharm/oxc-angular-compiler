//! `packages/compiler/src/typecheck/ops/events.ts`: STUB by `tcb-core` (what `scope.rs` uses), to be replaced by
//! `tcb-ops`'s port.

use oxc_angular_compiler::ast::r3::{R3BoundAttribute, R3BoundEvent};

use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::api::{DirectiveRef, Set};
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::references::LocalSymbol;

// events.ts:48 TcbDirectiveOutputsOp (stub)
pub struct TcbDirectiveOutputsOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _node: DirectiveOwner<'a>,
    _inputs: Option<&'a [R3BoundAttribute<'a>]>,
    _outputs: &'a [R3BoundEvent<'a>],
    _dir: DirectiveRef,
}

impl<'a, 'b> TcbDirectiveOutputsOp<'a, 'b> {
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        node: DirectiveOwner<'a>,
        inputs: Option<&'a [R3BoundAttribute<'a>]>,
        outputs: &'a [R3BoundEvent<'a>],
        dir: DirectiveRef,
    ) -> Self {
        Self { _tcb: tcb, _scope: scope, _node: node, _inputs: inputs, _outputs: outputs, _dir: dir }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbDirectiveOutputsOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}

// events.ts:125 TcbUnclaimedOutputsOp (stub)
pub struct TcbUnclaimedOutputsOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _target: LocalSymbol<'a>,
    _outputs: &'a [R3BoundEvent<'a>],
    _inputs: Option<&'a [R3BoundAttribute<'a>]>,
    _claimed_outputs: Option<Set<String>>,
    _has_directives: bool,
    _has_component: bool,
}

impl<'a, 'b> TcbUnclaimedOutputsOp<'a, 'b> {
    #[expect(clippy::too_many_arguments, reason = "the TypeScript constructor's parameters")]
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        target: LocalSymbol<'a>,
        outputs: &'a [R3BoundEvent<'a>],
        inputs: Option<&'a [R3BoundAttribute<'a>]>,
        claimed_outputs: Option<Set<String>>,
        has_directives: bool,
        has_component: bool,
    ) -> Self {
        Self {
            _tcb: tcb,
            _scope: scope,
            _target: target,
            _outputs: outputs,
            _inputs: inputs,
            _claimed_outputs: claimed_outputs,
            _has_directives: has_directives,
            _has_component: has_component,
        }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbUnclaimedOutputsOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}
