//! `packages/compiler/src/typecheck/ops/context.ts`

use std::cell::Cell;

use crate::angular::core::SchemaMetadata;
use crate::angular::t2_api::BoundTarget;
use crate::typecheck::api::{DirectiveRef, Map, TcbEnvironment, TcbPipeMetadata, TypeCheckId};
use crate::typecheck::oob::OutOfBandDiagnosticRecorder;
use crate::typecheck::schema::DomSchemaChecker;

// context.ts TcbGenericContextBehavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcbGenericContextBehavior {
    UseEmitter,
    CopyClassNodes,
    FallbackToAny,
}

// context.ts Context: overall generation context for one type check block. `'a` is the template AST's, `'b` the
// borrows of what generation uses.
pub struct Context<'a, 'b> {
    next_id: Cell<u32>,
    pub env: &'b dyn TcbEnvironment,
    pub dom_schema_checker: &'b dyn DomSchemaChecker<'a>,
    pub oob_recorder: &'b dyn OutOfBandDiagnosticRecorder<'a>,
    pub id: TypeCheckId,
    pub bound_target: &'b dyn BoundTarget<'a, DirectiveRef>,
    pipes: Option<&'b Map<String, TcbPipeMetadata>>,
    pub schemas: &'b [SchemaMetadata],
    pub host_is_standalone: bool,
    pub host_preserve_whitespaces: bool,
}

impl<'a, 'b> Context<'a, 'b> {
    #[expect(clippy::too_many_arguments, reason = "the TypeScript constructor's parameters")]
    pub fn new(
        env: &'b dyn TcbEnvironment,
        dom_schema_checker: &'b dyn DomSchemaChecker<'a>,
        oob_recorder: &'b dyn OutOfBandDiagnosticRecorder<'a>,
        id: TypeCheckId,
        bound_target: &'b dyn BoundTarget<'a, DirectiveRef>,
        pipes: Option<&'b Map<String, TcbPipeMetadata>>,
        schemas: &'b [SchemaMetadata],
        host_is_standalone: bool,
        host_preserve_whitespaces: bool,
    ) -> Self {
        Self {
            next_id: Cell::new(1),
            env,
            dom_schema_checker,
            oob_recorder,
            id,
            bound_target,
            pipes,
            schemas,
            host_is_standalone,
            host_preserve_whitespaces,
        }
    }

    // context.ts allocateId
    pub fn allocate_id(&self) -> String {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        format!("_t{id}")
    }

    // context.ts getPipeByName
    pub fn get_pipe_by_name(&self, name: &str) -> Option<&'b TcbPipeMetadata> {
        self.pipes?.get(name)
    }
}
