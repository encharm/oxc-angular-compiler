//! `packages/compiler/src/typecheck/ops/base.ts`
//!
//! Ownership, for every op: TypeScript's ops and scopes reference each other freely, and executing an op can resolve
//! (execute) another op of the same scope. Here the context and scopes are shared (`TcbRef`, `ScopeRef`); a scope
//! keeps its op queue in a `RefCell`, takes an op out of it while executing it (with its `circular_fallback` in its
//! place, as TypeScript does) and puts the result back; ops hold what they need by `Rc` and take `&self`.

use std::rc::Rc;

use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::context::Context;
use crate::typecheck::ops::scope::Scope;

/// The generation context, shared by the scopes and ops of one type-check block
pub type TcbRef<'a, 'b> = Rc<Context<'a, 'b>>;

/// A scope, shared by its ops and child scopes
pub type ScopeRef<'a, 'b> = Rc<Scope<'a, 'b>>;

/// An op in a scope's queue
pub type BoxedOp<'a, 'b> = Box<dyn TcbOp<'a, 'b> + 'b>;

// base.ts TcbOp: a code generation operation that's involved in the construction of a type-check block
pub trait TcbOp<'a, 'b> {
    /// `optional`: only executed when depended upon by other operations
    fn optional(&self) -> bool;

    fn execute(&self) -> Option<TcbExpr>;

    /// The replacement value or operation used while this op executes (to resolve circular references)
    fn circular_fallback(&self) -> CircularFallback<'a, 'b> {
        CircularFallback::Expr(TcbExpr::new("null!"))
    }
}

/// `TcbOp | TcbExpr`
pub enum CircularFallback<'a, 'b> {
    Op(BoxedOp<'a, 'b>),
    Expr(TcbExpr),
}
