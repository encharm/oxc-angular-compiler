//! `packages/compiler/src/typecheck/ops/inputs.ts`: STUB by `tcb-core` (what `scope.rs` and `directive_constructor.rs`
//! use), to be replaced by `tcb-ops`'s port.

use oxc_angular_compiler::ast::r3::R3BoundAttribute;

use crate::typecheck::oob::DirectiveHost;
use crate::typecheck::api::{DirectiveRef, Set};
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::bindings::TcbBindingValue;
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::references::LocalSymbol;
use crate::typecheck::ops::signal_forms::CustomFormControlType;

// inputs.ts:34 translateInput (stub)
pub fn translate_input<'a, 'b>(
    _value: TcbBindingValue<'a>,
    _tcb: &TcbRef<'a, 'b>,
    _scope: &ScopeRef<'a, 'b>,
) -> TcbExpr {
    todo!("inputs.ts translateInput: ported by tcb-ops")
}

// inputs.ts:50 TcbDirectiveInputsOp (stub)
pub struct TcbDirectiveInputsOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _node: DirectiveHost<'a>,
    _dir: DirectiveRef,
    _is_form_control: bool,
    _custom_form_control_type: Option<CustomFormControlType>,
}

impl<'a, 'b> TcbDirectiveInputsOp<'a, 'b> {
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        node: DirectiveHost<'a>,
        dir: DirectiveRef,
        is_form_control: bool,
        custom_form_control_type: Option<CustomFormControlType>,
    ) -> Self {
        Self {
            _tcb: tcb,
            _scope: scope,
            _node: node,
            _dir: dir,
            _is_form_control: is_form_control,
            _custom_form_control_type: custom_form_control_type,
        }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbDirectiveInputsOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}

// inputs.ts:242 TcbUnclaimedInputsOp (stub)
pub struct TcbUnclaimedInputsOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _inputs: &'a [R3BoundAttribute<'a>],
    _target: LocalSymbol<'a>,
    _claimed_inputs: Option<Set<String>>,
}

impl<'a, 'b> TcbUnclaimedInputsOp<'a, 'b> {
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        inputs: &'a [R3BoundAttribute<'a>],
        target: LocalSymbol<'a>,
        claimed_inputs: Option<Set<String>>,
    ) -> Self {
        Self { _tcb: tcb, _scope: scope, _inputs: inputs, _target: target, _claimed_inputs: claimed_inputs }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbUnclaimedInputsOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}
