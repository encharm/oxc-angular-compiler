# ng_check: porting guide

`ng_check` type-checks Angular templates without Node: `ng-analyze` (vendored, Angular's Rust analyzer) finds each
component's directives, pipes and their types; this crate binds each template (`t2_binder`), generates its type-check
block (TCB) exactly as Angular does, and `tsrs` checks the TCBs; diagnostics map back to the templates as ngtsc maps
them.

Everything but the command line is a **faithful port** of Angular's TypeScript at the commit pinned in
`UPSTREAM.json`, in the style of tsrs: same algorithms, same function decomposition, same order of operations, same
output text. Behaviour that matches Angular is correct; "improvements" are bugs. Every Rust function should be
recognizable next to its TypeScript original, so that an Angular update is a diff to port, not a redesign.

## Layout

One Rust file per TypeScript file, same base name in snake_case, functions in the same order as the TypeScript
file. Above each ported function, type or method: `// <file>.ts:<line> <name>` (the line in the pinned commit).

| Angular (`angular/angular` at `UPSTREAM.json`'s commit) | Rust |
| --- | --- |
| `packages/compiler/src/core.ts`, `property_mapping.ts`, `directive_matching.ts` | `src/angular/` |
| `packages/compiler/src/render3/view/t2_api.ts`, `t2_binder.ts` | `src/angular/` |
| `packages/compiler/src/schema/element_schema_registry.ts`, `dom_element_schema_registry.ts` | `src/angular/schema/` |
| `packages/compiler/src/typecheck/**` (the TCB generator) | `src/typecheck/**` |
| `packages/compiler-cli/preprocessor/src/` (`tcb.ts`, `tcb_adapter.ts`, ...: from ng-analyze's metadata to the generator) | `src/ngp/` |
| `packages/compiler-cli/src/ngtsc/typecheck/src/diagnostics.ts`, `tcb_util.ts`, `comments.ts` (mapping TCB diagnostics back) | `src/ngtsc/` |
| `packages/compiler-cli/preprocessor/ng-analyze` | `vendor/ng-analyze`, with the patches in `patches/` |

`UPSTREAM.json` lists every ported file. `node tools/ng-check-upstream.mjs status <commit>` shows which of them
changed between the pinned commit and `<commit>`, with their diffs: port those, then update the pin and
`vendor/ng-analyze` (`node tools/ng-check-upstream.mjs vendor <commit>`, which also applies `patches/`).

`patches/` holds our changes to ng-analyze, each listed with its reason in `UPSTREAM.json` (`vendored[].patches`):
they make its metadata match ngtsc's where the type-check blocks depend on it. A patch that stops applying makes
`vendor` fail; port the change to the new code and rewrite the patch with `git diff -- vendor/ng-analyze`.

## Conventions

- **The template AST** is `oxc_angular_compiler`'s (`ast::r3`, `ast::expression`), Angular's `r3_ast.ts` and
  `expression_parser/ast.ts` with an `R3` prefix on template nodes (`Element` is `R3Element`). Nodes live in an
  `oxc_allocator` arena for `'a`; code borrows them as `&'a R3Element<'a>`.
- **Identity.** TypeScript keys maps by node object; Rust keys them by the node's address: `node_key(node)` for a
  template node (the struct inside its `Box`), `expr_key(&AngularExpression)` for an expression (the boxed struct of
  its variant). Both are in `src/angular/t2_api.rs`. The enums there (`DirectiveOwner`, `ScopedNode`,
  `TemplateEntity`, `ReferenceTarget`) compare and hash by that key.
- **Directives** are `Rc<TcbDirectiveMetadata>`, compared by `Rc::ptr_eq` (TypeScript compares the objects);
  `DirectiveKey` wraps one for maps.
- **Collections.** JavaScript `Map` and `Set` iterate in insertion order, and output order depends on it: use
  `IndexMap`/`IndexSet` for them (with `FxBuildHasher`), never `HashMap`/`HashSet`. Arrays are `Vec`.
- **Names**: Rust snake_case of the TypeScript name (`getDirectivesOfNode` is `get_directives_of_node`); types keep
  their names (`TcbExpr`, `Scope`); `R3` prefixed AST names come from the fork. A keyword gets a `_` suffix.
- **Classes with subclasses** (the TCB ops: `TcbOp` and its subclasses) become a trait plus one struct per subclass,
  each in the file of its TypeScript class.
- **`instanceof` chains** become `match` on the enum.
- **Errors.** A `throw new Error(...)` in the generator is an internal assertion: `panic!` with the same message.
- **Strings** are `String`; template literals become `format!`. Text written into a TCB must be byte-identical to
  Angular's.
- **Offsets.** Angular's offsets are JavaScript (UTF-16) string indices; the fork's are UTF-8 byte offsets. Span
  comments (`TcbExpr::add_parse_span_info`) convert through `crate::span::utf16` so TCB text matches Angular's.
- No `unsafe`.

## Testing

`cargo test -p ng_check` runs the unit tests: ported spec tests (the binder's `binding_spec.ts`, span and expression
tests from `type_check_block_spec.ts`), the generator end to end on hand-built bindings, and `tests/identity.rs`.

The differential test compares this crate with Angular's own compiler (ngtsc, at the commit `UPSTREAM.json` pins) on a
real project. It has two halves.

**Type-check blocks.** Generate both sides and compare them function by function:

```
node tools/ng-check-oracle.mjs <project>/tsconfig.json <oracle dir>     # ngtsc's blocks and diagnostics
ngcheck -p <project>/tsconfig.json --keep                               # ours, in <root>/../.ngcheck-<root>/
node tools/ng-check-diff.mjs <oracle dir> <root>/../.ngcheck-<root>/<root>
```

`ng-check-diff.mjs` matches functions by component and prints the first differing line of each. A function counts as
identical, as identical but for import numbering, or as different.

**Diagnostics.** `tests/probes/` is a project of components with one planted template error each (and two that must
pass). Link `node_modules` from an Angular project into it, then:

```
node tools/ng-check-oracle.mjs crates/ng_check/tests/probes/tsconfig.json <oracle dir>
(cd crates/ng_check/tests/probes && ngcheck -p tsconfig.json > <out file>)
node tools/ng-check-diagnostics.mjs crates/ng_check/tests/probes <oracle dir> <out file>
```

Positions are compared as `file:line:col` with the code and the message.

**Known differences.** On a large private monorepo (663 type-check functions), 634 functions are identical and 24 are
identical but for import numbering. Five differ, all for these reasons:

- **Import numbering.** Aliases (`i1.`) are numbered by each tool's environment: ngcheck follows the prototype's
  (`tcb.ts`), not ngtsc's.
- **Directive names.** A dependency directive is named as it is imported (`typecheckImport.symbol`, as the prototype
  does), so `NgFor` where ngtsc writes the declared `NgForOf`. Both name the same class, and TypeScript reports types
  by their declared name either way.
- **Inline blocks.** A component that cannot be imported gets an inline block. ngtsc inlines only that component's
  block, into the component's file (`_tcb_<pos>`). ngcheck, as the prototype, generates the whole file inline, with
  unqualified names.
- **Entity offsets.** In text holding a non-ASCII entity (`&#160;`), Angular counts expression offsets in the decoded
  text in UTF-16 units; the fork's parser counts UTF-8 bytes. A span after such an entity is off by the difference.
- **TypeScript 7.** tsrs places a few diagnostics (TS2769 "No overload matches this call") elsewhere than the
  TypeScript ngtsc runs.
- **Extended diagnostics.** ngtsc's type-based warnings (NG8102 nullish coalescing, NG8107 optional chain) need
  TypeScript's type checker. ngcheck runs tsrs as a separate program, so it does not report them.
- **Guide links.** "Find more at" links follow the project's `@angular/compiler` version, as its own ngc prints them.
