//! Ports of `packages/compiler-cli/preprocessor/src` (`PORTING.md`): from ng-analyze's metadata to the type-check
//! block generator; and the command line.

pub mod cli;
pub mod compiler_utils;
pub mod dependency_templates;
pub mod dom_schema_checker;
pub mod oob;
pub mod processor;
pub mod tcb;
pub mod tcb_adapter;
pub mod tcb_util;
pub mod template_semantics;
