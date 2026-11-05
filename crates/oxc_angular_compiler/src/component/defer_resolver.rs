//! Build the dependency-resolver function for `@defer` blocks.
//!
//! In Angular's local compilation mode the compiler emits a single
//! per-component deferrable-dependencies function — a no-arg arrow that
//! returns an array of dynamic `import()` calls resolving each lazy
//! dependency. The runtime invokes it lazily, the first time a defer block
//! triggers, so the deferred chunks can be code-split by the bundler.
//!
//! Example shape:
//!
//! ```js
//! () => [
//!   import('./lazy').then(m => m.LazyCmp),
//!   import('./widget').then(m => m.HeavyWidget),
//!   import('./default-export').then(m => m.default),
//! ]
//! ```
//!
//! This module produces that expression from the component's
//! `@Component.deferredImports` array (mirrored as
//! [`ComponentMetadata::deferred_imports`]) combined with the file-level
//! import map. It is consumed by the main compile path in
//! [`super::transform::compile_component_full`].
//!
//! ## `deferredImports` and implicitly deferred `imports`
//!
//! Angular's full compilation mode also derives a deferrable set from
//! `@Component.imports` by matching template selectors to imported
//! directives/pipes/components (see `resolveAllDeferredDependencies` in
//! `packages/compiler-cli/src/ngtsc/annotations/component/src/handler.ts`).
//! That requires cross-file metadata — selectors live on the imported class,
//! not on the import declaration. In local compilation Angular falls back to
//! the `deferredImports` field as the only source of deferrable symbols.
//! OXC does the same unless cross-file analysis can read the imported
//! class's selector: then [`implicitly_deferred_imports`] picks the
//! standalone components and directives the template matches only inside
//! `@defer` blocks, and they join `deferredImports` (#313).
//!
//! Like ngc's full compilation (its PerBlock emit), each `@defer` block then
//! loads only the ones it uses: [`deferred_imports_per_block`] says which,
//! and [`build_defer_block_resolvers`] builds one function per block, which
//! the pipeline hoists to a `<View>_Defer_<slot>_DepsFn` constant. Explicit
//! `deferredImports` don't say what block uses them, so with any of those
//! every block shares the per-component function above.

use oxc_allocator::Allocator;

use crate::ast::expression::BindingType;
use crate::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Component, R3DeferredBlock, R3Directive, R3Element, R3Node,
    R3Template, R3TemplateAttr, R3TextAttribute, R3Visitor, visit_all,
};
use crate::class_metadata::{
    R3DeferPerComponentDependency, compile_component_metadata_async_resolver,
};
use crate::i18n::serializer::is_i18n_attribute;
use crate::output::ast::OutputExpression;
use crate::parser::html::split_ns_name;
use crate::pipeline::selector::CssSelector;
use oxc_span::Span;
use oxc_str::Ident;
use rustc_hash::FxHashMap;

use super::metadata::ComponentMetadata;
use super::transform::ImportMap;

/// Returns `true` if any `@defer` block exists anywhere in the template.
///
/// We need this so the compiler only generates a deferrable-dependencies
/// function when it would actually be wired into a `ɵɵdefer` call — otherwise
/// the resolver would be dead code.
pub fn template_has_defer_block<'a>(nodes: &[R3Node<'a>]) -> bool {
    struct DeferDetector {
        found: bool,
    }

    impl<'a> R3Visitor<'a> for DeferDetector {
        fn visit_deferred_block(&mut self, _block: &R3DeferredBlock<'a>) {
            // No need to recurse: a single hit is enough.
            self.found = true;
        }
    }

    let mut visitor = DeferDetector { found: false };
    visit_all(&mut visitor, nodes);
    visitor.found
}

/// Build the list of `R3DeferPerComponentDependency` describing the
/// `@Component.deferredImports` entries, used to emit `setClassMetadataAsync`
/// and the per-component `ɵɵdefer` resolver.
///
/// Each entry carries two names that serve distinct roles in the emitted
/// output:
///
/// - `param_name` — the local binding from the source file (e.g. `Heavy` for
///   `import { HeavyWidget as Heavy }`, `LazyCmp` for
///   `import LazyCmp from './lazy'`). Used as the parameter name on the
///   `setClassMetadataAsync` callback so the wrapped decorator metadata
///   literal's identifier references shadow the outer static import,
///   allowing bundlers to drop the eager declaration.
/// - `export_name` — the name under which the symbol is exported from its
///   source module (e.g. `HeavyWidget` or `default`). Used as the property
///   read in the dynamic-import resolver chain (`m.<export_name>`). For
///   default imports the resolver substitutes `m.default` based on
///   `is_default_import`, so the value here is informational in that case.
///
/// Angular collapses both into a single `symbolName` field set to the
/// exported name; that leaves the static `import { Foo as Bar }` pinned for
/// aliased deferrable imports because the callback parameter (`Foo`) doesn't
/// match the metadata body's reference (`Bar`). Splitting the fields here is
/// a deliberate improvement on Angular's emission.
///
/// Entries for unresolved symbols, namespace imports, and type-only imports
/// are skipped — they have no concrete runtime value to lazy-load and the
/// resolver would silently misfire.
pub(super) fn build_defer_per_component_deps<'a>(
    allocator: &'a Allocator,
    deferred_imports: &[Ident<'a>],
    import_map: &ImportMap<'a>,
) -> oxc_allocator::Vec<'a, R3DeferPerComponentDependency<'a>> {
    let mut deps = oxc_allocator::Vec::new_in(&allocator);
    for local_name in deferred_imports {
        let Some(info) = import_map.get(local_name) else { continue };
        if !info.is_named_import || info.is_type_only {
            continue;
        }
        let is_default_import = info.imported_name.as_deref() == Some("default");
        // `export_name` is `info.imported_name` when present (aliased named
        // imports or default imports), otherwise the local name (which
        // equals the exported name for plain `import { Foo }`).
        let export_name = info.imported_name.clone().unwrap_or_else(|| local_name.clone());
        deps.push(R3DeferPerComponentDependency {
            param_name: local_name.clone(),
            export_name,
            import_path: info.source_module.clone(),
            is_default_import,
        });
    }
    deps
}

/// Build the deferrable-dependencies resolver expression from the component's
/// `@Component.deferredImports` array.
///
/// For each local identifier in `metadata.deferred_imports` that resolves to
/// an entry in `import_map`, emit one
/// `import(<path>).then(m => m.<exportedName>)` entry. Returns `None` when the
/// component has no `deferredImports` declared (no resolver to wire) — in
/// that case callers leave `ɵɵdefer`'s resolver argument as `null`, matching
/// Angular's behavior for a defer block with no explicit deferrable
/// dependencies.
///
/// A deferred entry is silently skipped (with no compiler error) when:
/// - the symbol isn't in the file's import map (it might be a local binding;
///   Angular would have flagged this earlier, but we treat it as a no-op),
/// - the import is a namespace import (`import * as ns`) — there is no
///   single `m.X` to point at,
/// - the import is type-only (erased at runtime).
///
/// Unlike eager `imports`, we do NOT filter by source module: Angular's
/// `compileDeferResolverFunction` emits a dynamic `import(<path>)` for every
/// entry the user lists, including bare specifiers like `@angular/material`.
/// Filtering would silently drop intentional opt-ins.
///
/// Shares the emission code path with `setClassMetadataAsync` via
/// [`compile_component_metadata_async_resolver`] so both call sites stay in
/// lockstep — the resolver wired into `ɵɵdefer(...)` is byte-equivalent to
/// the one wired into `ɵsetClassMetadataAsync(...)`.
pub fn build_defer_resolver_expression<'a>(
    allocator: &'a Allocator,
    metadata: &ComponentMetadata<'a>,
    import_map: &ImportMap<'a>,
) -> Option<OutputExpression<'a>> {
    if metadata.deferred_imports.is_empty() {
        return None;
    }

    let deps = build_defer_per_component_deps(allocator, &metadata.deferred_imports, import_map);
    if deps.is_empty() {
        return None;
    }

    Some(compile_component_metadata_async_resolver(allocator, &deps))
}

/// The `candidates` matched only inside the main content of `@defer` blocks
/// (`resolveDeferBlocks` in ngc's component `handler.ts`). `@placeholder`,
/// `@loading` and `@error` count as eager. Selectorless templates return none.
pub(super) fn implicitly_deferred_imports<'a>(
    nodes: &[R3Node<'a>],
    candidates: &[(Ident<'a>, std::vec::Vec<CssSelector>)],
) -> std::vec::Vec<Ident<'a>> {
    let Some(matches) = match_candidates(nodes, candidates) else {
        return std::vec::Vec::new();
    };
    candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            !matches.eager[*index] && matches.blocks.iter().any(|(_, matched)| matched[*index])
        })
        .map(|(_, (name, _))| *name)
        .collect()
}

/// The `candidates` each `@defer` block loads, by the block's source span:
/// the ones matched in its main content and in the `@placeholder`, `@loading`
/// and `@error` of the `@defer` blocks inside it, whose main content loads its
/// own. ngc binds each block's `children` alike for its PerBlock emit
/// (`resolveDeferBlocks` in its component `handler.ts`). Every `@defer` block
/// has an entry, without names when it loads none; a selectorless template
/// returns none.
pub(super) fn deferred_imports_per_block<'a>(
    nodes: &[R3Node<'a>],
    candidates: &[(Ident<'a>, std::vec::Vec<CssSelector>)],
) -> std::vec::Vec<(Span, std::vec::Vec<Ident<'a>>)> {
    let Some(matches) = match_candidates(nodes, candidates) else {
        return std::vec::Vec::new();
    };
    matches
        .blocks
        .into_iter()
        .map(|(span, matched)| {
            let names = candidates
                .iter()
                .zip(matched)
                .filter(|(_, matched)| *matched)
                .map(|((name, _), _)| *name)
                .collect();
            (span, names)
        })
        .collect()
}

/// One dependency function per `@defer` block for the PerBlock emit, from
/// [`deferred_imports_per_block`]: `None` for a block that loads nothing.
pub(super) fn build_defer_block_resolvers<'a>(
    allocator: &'a Allocator,
    blocks: std::vec::Vec<(Span, std::vec::Vec<Ident<'a>>)>,
    import_map: &ImportMap<'a>,
) -> FxHashMap<Span, Option<OutputExpression<'a>>> {
    blocks
        .into_iter()
        .map(|(span, names)| {
            let deps = build_defer_per_component_deps(allocator, &names, import_map);
            let resolver = (!deps.is_empty())
                .then(|| compile_component_metadata_async_resolver(allocator, &deps));
            (span, resolver)
        })
        .collect()
}

/// Which candidates a template matches where.
struct Matches {
    /// Matched outside the main content of every `@defer` block
    eager: std::vec::Vec<bool>,
    /// Each `@defer` block's source span and the candidates it loads, as
    /// [`deferred_imports_per_block`] says
    blocks: std::vec::Vec<(Span, std::vec::Vec<bool>)>,
}

/// Matches the template's elements against `candidates`; `None` when the
/// template is selectorless.
fn match_candidates<'a>(
    nodes: &[R3Node<'a>],
    candidates: &[(Ident<'a>, std::vec::Vec<CssSelector>)],
) -> Option<Matches> {
    struct Matcher<'c, 'a> {
        candidates: &'c [(Ident<'a>, std::vec::Vec<CssSelector>)],
        /// The `@defer` block the visited nodes load with, an index into
        /// `matches.blocks`, or `None` outside every block
        block: Option<usize>,
        matches: Matches,
        selectorless: bool,
    }

    impl Matcher<'_, '_> {
        fn record(&mut self, element: &CssSelector) {
            let matched = match self.block {
                Some(block) => &mut self.matches.blocks[block].1,
                None => &mut self.matches.eager,
            };
            for (index, (_, selectors)) in self.candidates.iter().enumerate() {
                if selectors.iter().any(|selector| selector.matches(element)) {
                    matched[index] = true;
                }
            }
        }
    }

    impl<'a> R3Visitor<'a> for Matcher<'_, 'a> {
        fn visit_element(&mut self, element: &R3Element<'a>) {
            if !element.directives.is_empty() {
                self.selectorless = true;
            }
            self.record(&element_selector(
                &element.name,
                &element.attributes,
                &element.inputs,
                &element.outputs,
            ));
            self.visit_element_children(element);
        }

        fn visit_template(&mut self, template: &R3Template<'a>) {
            if !template.directives.is_empty() {
                self.selectorless = true;
            }
            self.record(&template_selector(template));
            self.visit_template_children(template);
        }

        fn visit_deferred_block(&mut self, block: &R3DeferredBlock<'a>) {
            let outer = self.block;
            self.block = Some(self.matches.blocks.len());
            self.matches.blocks.push((block.source_span, vec![false; self.candidates.len()]));
            for child in &block.children {
                child.visit(self);
            }
            // `@placeholder`, `@loading` and `@error` render before the chunk
            // loads, so they load with the outer `@defer`, or are eager outside
            // one (`visitDeferredBlock` in t2_binder's `DirectiveBinder`).
            self.block = outer;
            if let Some(placeholder) = &block.placeholder {
                self.visit_deferred_block_placeholder(placeholder);
            }
            if let Some(loading) = &block.loading {
                self.visit_deferred_block_loading(loading);
            }
            if let Some(error) = &block.error {
                self.visit_deferred_block_error(error);
            }
        }

        fn visit_component(&mut self, _component: &R3Component<'a>) {
            self.selectorless = true;
        }

        fn visit_directive(&mut self, _directive: &R3Directive<'a>) {
            self.selectorless = true;
        }
    }

    let mut matcher = Matcher {
        candidates,
        block: None,
        matches: Matches { eager: vec![false; candidates.len()], blocks: std::vec::Vec::new() },
        selectorless: false,
    };
    visit_all(&mut matcher, nodes);
    (!matcher.selectorless).then_some(matcher.matches)
}

/// An element as Angular matches directives against it
/// (`createCssSelectorFromNode` and `getAttrsForDirectiveMatching` in
/// `packages/compiler/src/render3/view/util.ts`): its static attributes with
/// their values, the names of its property and two-way bindings and of its
/// outputs, and its `class` attribute as classes. A binding replaces the value
/// of a static attribute with the same name, and namespace prefixes
/// (`:svg:`, `:xlink:`) are dropped.
fn element_selector(
    name: &str,
    attributes: &[R3TextAttribute<'_>],
    inputs: &[R3BoundAttribute<'_>],
    outputs: &[R3BoundEvent<'_>],
) -> CssSelector {
    let mut attrs = std::vec::Vec::new();
    for attribute in attributes {
        if !is_i18n_attribute(&attribute.name) {
            set_attribute(&mut attrs, &attribute.name, &attribute.value);
        }
    }
    for input in inputs {
        if matches!(input.binding_type, BindingType::Property | BindingType::TwoWay) {
            set_attribute(&mut attrs, &input.name, "");
        }
    }
    for output in outputs {
        set_attribute(&mut attrs, &output.name, "");
    }

    let mut selector = CssSelector::new();
    selector.set_element(split_ns_name(name).1);
    for (name, value) in attrs {
        selector.add_attribute(split_ns_name(name).1, Some(value));
        if name.eq_ignore_ascii_case("class") {
            for class in value.split_whitespace() {
                selector.add_class_name(class);
            }
        }
    }
    selector
}

/// Sets `name` to `value` in `attrs`, a name/value map kept in insertion order
/// like the object `getAttrsForDirectiveMatching` builds.
fn set_attribute<'s>(attrs: &mut std::vec::Vec<(&'s str, &'s str)>, name: &'s str, value: &'s str) {
    match attrs.iter_mut().find(|(existing, _)| *existing == name) {
        Some(entry) => entry.1 = value,
        None => attrs.push((name, value)),
    }
}

/// A template as Angular matches directives against it: an `<ng-template>`
/// like an element, a structural directive's template (`*ngIf`) by the names
/// of its template attributes.
fn template_selector(template: &R3Template<'_>) -> CssSelector {
    match &template.tag_name {
        Some(tag_name) if tag_name.as_str() == "ng-template" => element_selector(
            "ng-template",
            &template.attributes,
            &template.inputs,
            &template.outputs,
        ),
        _ => {
            let mut selector = CssSelector::new();
            selector.set_element("ng-template");
            for attribute in &template.template_attrs {
                let name = match attribute {
                    R3TemplateAttr::Bound(bound) => &bound.name,
                    R3TemplateAttr::Text(text) => &text.name,
                };
                selector.add_attribute(split_ns_name(name).1, None);
            }
            selector
        }
    }
}
