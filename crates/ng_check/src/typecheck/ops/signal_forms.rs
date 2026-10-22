//! `packages/compiler/src/typecheck/ops/signal_forms.ts`: STUB by `tcb-core` (what `scope.rs` and
//! `directive_constructor.rs` use), to be replaced by `tcb-ops`'s port.

use oxc_angular_compiler::ast::r3::R3Element;

use crate::angular::t2_api::DirectiveOwner;
use crate::typecheck::api::{DirectiveRef, TcbDirectiveMetadata};
use crate::typecheck::ops::base::{ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::bindings::TcbBoundAttribute;
use crate::typecheck::ops::codegen::TcbExpr;

// signal_forms.ts:30 CustomFormControlType (stub): `'value' | 'checkbox'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomFormControlType {
    Value,
    Checkbox,
}

// signal_forms.ts:74 TcbNativeFieldOp (stub)
pub struct TcbNativeFieldOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _node: &'a R3Element<'a>,
    _input_type: Option<String>,
}

impl<'a, 'b> TcbNativeFieldOp<'a, 'b> {
    pub fn new(
        tcb: TcbRef<'a, 'b>,
        scope: ScopeRef<'a, 'b>,
        node: &'a R3Element<'a>,
        input_type: Option<String>,
    ) -> Self {
        Self { _tcb: tcb, _scope: scope, _node: node, _input_type: input_type }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbNativeFieldOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}

// signal_forms.ts:221 TcbNativeRadioButtonFieldOp (stub)
pub struct TcbNativeRadioButtonFieldOp<'a, 'b> {
    _tcb: TcbRef<'a, 'b>,
    _scope: ScopeRef<'a, 'b>,
    _node: &'a R3Element<'a>,
}

impl<'a, 'b> TcbNativeRadioButtonFieldOp<'a, 'b> {
    pub fn new(tcb: TcbRef<'a, 'b>, scope: ScopeRef<'a, 'b>, node: &'a R3Element<'a>) -> Self {
        Self { _tcb: tcb, _scope: scope, _node: node }
    }
}

impl<'a, 'b> TcbOp<'a, 'b> for TcbNativeRadioButtonFieldOp<'a, 'b> {
    fn optional(&self) -> bool {
        todo!("ported by tcb-ops")
    }
    fn execute(&self) -> Option<TcbExpr> {
        todo!("ported by tcb-ops")
    }
}

// signal_forms.ts:249 expandBoundAttributesForField (stub)
pub fn expand_bound_attributes_for_field<'a>(
    _directive: &TcbDirectiveMetadata,
    _node: DirectiveOwner<'a>,
    _custom_form_control_type: Option<CustomFormControlType>,
) -> Option<Vec<TcbBoundAttribute<'a>>> {
    todo!("ported by tcb-ops")
}

// signal_forms.ts:368 getCustomFieldDirectiveType (stub)
pub fn get_custom_field_directive_type(_meta: &TcbDirectiveMetadata) -> Option<CustomFormControlType> {
    todo!("ported by tcb-ops")
}

// signal_forms.ts:380 isNativeField (stub): true only for an `Element` (`node is Element & {name: ...}`)
pub fn is_native_field(_dir: &TcbDirectiveMetadata, _node: DirectiveOwner<'_>, _all_directive_matches: &[DirectiveRef]) -> bool {
    todo!("ported by tcb-ops")
}

// signal_forms.ts:487 isFormControl (stub)
pub fn is_form_control(_all_directive_matches: &[DirectiveRef]) -> bool {
    todo!("ported by tcb-ops")
}
