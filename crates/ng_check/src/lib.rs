//! Angular template type checking without Node (`PORTING.md`): Angular's type-check block generator and binder,
//! ported from TypeScript, over the metadata of `ng-analyze`; `tsrs` checks the blocks.

pub mod angular;
pub mod ngp;
pub mod ngtsc;
pub mod span;
pub mod typecheck;
