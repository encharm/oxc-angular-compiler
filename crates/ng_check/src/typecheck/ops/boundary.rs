//! `packages/compiler/src/typecheck/ops/boundary.ts`: `TcbBoundaryOp`, which renders a `@boundary` block, is not
//! ported: the fork's template AST has no `BoundaryBlock` (or `BoundaryErrorBlock`), its parser does not produce one,
//! and `Scope` never creates the op (`scope.rs` notes where it would).
//!
//! For when the fork gains the nodes, the TypeScript (boundary.ts:19-101), whose text is:
//!
//! ```text
//! execute():
//!   checkBody = config.checkControlFlowBodies
//!   tryBodyScope = scope.createChildScope(scope, null, checkBody ? block.children : [], null)
//!   tryBlockStr = `{\n${getStatementsBlock(tryBodyScope.render())}}`
//!   if block.errorBlocks.length > 0:
//!     catchBlockStr = `{\n${getStatementsBlock(generateCatchBranch(0))}}`
//!     scope.addStatement(`try ${tryBlockStr} catch (err) ${catchBlockStr}`)
//!   else:
//!     scope.addStatement(tryBlockStr)
//!   return null
//!
//! generateCatchBranch(index):
//!   errorBlock = block.errorBlocks[index]; if none: return []
//!   outerScope = scope.createChildScope(scope, errorBlock, [], null)
//!   resultStatements = [...outerScope.render()]
//!   expression = errorBlock.expression ? tcbExpression(errorBlock.expression, tcb, outerScope) : null
//!   bodyScope = scope.createChildScope(outerScope, null, checkBody ? errorBlock.children : [], null)
//!   bodyBlockStr = `{\n${getStatementsBlock(bodyScope.render())}}`
//!   next = generateCatchBranch(index + 1)
//!   if expression:
//!     resultStatements.push(`if (${expression.print()}) ${bodyBlockStr}` +
//!                           (next.length > 0 ? ` else {\n${getStatementsBlock(next)}}` : ''))
//!   else:
//!     resultStatements.push(bodyBlockStr)
//!   return resultStatements
//! ```
