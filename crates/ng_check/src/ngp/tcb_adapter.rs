//! `packages/compiler-cli/preprocessor/src/tcb_adapter.ts`: from ng-analyze's metadata (`nga.*`, the Rust types of
//! `vendor/ng-analyze`) to the type-check block generator's (`TcbDirectiveMetadata`, `TcbPipeMetadata`, the host
//! element), and the binding of a component's template.
//!
//! Differences forced by the types:
//! - TypeScript's `TcbTypeParameter`s are `ExtendedTcbTypeParameter`s at run time, and `tcb.ts` reads their `typeRefs`
//!   back off `TcbDirectiveMetadata.typeParameters`. The Rust `TcbTypeParameter` has no `typeRefs`: the extended type
//!   parameters of each directive are recorded in a `TypeParameterRefs` (by `ref.key`) as its metadata is made.
//! - `TcbTypeCheckBlockMetadata` borrows its bound target; `adapt_tcb_input` returns an `AdaptedTcbMeta` that owns it
//!   (`as_tcb_meta` borrows it back).
//! - ng-analyze's spans are JavaScript string (UTF-16) indices into `content`; the host element's are the parser's
//!   (byte offsets), converted by `create_span`. `content` is always given (TypeScript's is optional).

use std::rc::Rc;

use indexmap::IndexSet;
use ng_analyze::{
    DeclarationMetadata, ExpressionValueKindMetadata, ExpressionValueMetadata, HostDirectiveBinding, InputMetadata,
    OutputMetadata, ResolvedHostDirectiveMetadata, TypeParameterMetadata, TypeRefMetadata,
};
use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::r3::R3HostElement;
use rustc_hash::FxBuildHasher;

use crate::angular::core::SchemaMetadata;
use crate::angular::directive_matching::{CssSelector, SelectorMatcher, SelectorlessMatcher};
use crate::angular::property_mapping::{ClassPropertyMapping, InputOrOutput};
use crate::angular::t2_api::{BoundTarget, LegacyAnimationTriggerNames, MatchSource, Target, TargetHost};
use crate::angular::t2_binder::{DirectiveMatcher, R3TargetBinder};
use crate::ngp::compiler_utils::{create_foreign_component_matcher, create_span, is_static_source_node};
use crate::ngp::tcb::{NgpTypeCheckingConfig, TcbTargetInput};
use crate::ngp::tcb_util::substring;
use crate::typecheck::api::{
    DirectiveRef, Map, Set, TcbComponentMetadata, TcbDirectiveMetadata, TcbInputMapping, TcbPipeMetadata,
    TcbReferenceKey, TcbReferenceMetadata, TcbTypeCheckBlockMetadata, TcbTypeParameter, TemplateGuardMeta,
    TemplateGuardType, TypeCheckId,
};
use crate::typecheck::host_bindings::{
    HostBindingDecorator, HostListenerDecorator, HostObjectLiteralBinding, SourceNode,
    StaticSourceNode, StaticSourceNodeKind, create_host_element,
};
use crate::typecheck::ops::codegen::SpanInfo;

/// The extended type parameters (`ExtendedTcbTypeParameter[]`) of the directives whose metadata was made, by their
/// `ref.key`: what TypeScript reads off `TcbDirectiveMetadata.typeParameters` at run time.
pub type TypeParameterRefs = Map<TcbReferenceKey, Vec<ExtendedTcbTypeParameter>>;

// tcb_adapter.ts:36 createInputPropertyMapping
pub fn create_input_property_mapping(
    metadata: Option<&[&InputMetadata]>,
    content: Option<&str>,
) -> ClassPropertyMapping<TcbInputMapping> {
    let mut obj: Map<String, TcbInputMapping> = Map::default();

    if let Some(metadata) = metadata {
        for item in metadata {
            let class_prop = &item.name;
            let binding_name = item.alias.as_ref().filter(|alias| !alias.is_empty()).unwrap_or(class_prop);
            let mut mapping = TcbInputMapping {
                input: InputOrOutput {
                    class_property_name: class_prop.clone(),
                    binding_property_name: binding_name.clone(),
                    is_signal: item.is_signal,
                },
                required: item.required,
                transform_type: None,
            };

            if let Some(transform) = &item.transform
                && transform.kind == "type"
                && let Some(type_span) = transform.type_span
                && let Some(content) = content.filter(|content| !content.is_empty())
            {
                mapping.transform_type = Some(substring(content, type_span.start, type_span.end).to_string());
            }

            obj.insert(class_prop.clone(), mapping);
        }
    }

    ClassPropertyMapping::from_entries(obj)
}

// tcb_adapter.ts:72 createOutputPropertyMapping
pub fn create_output_property_mapping(metadata: Option<&[&OutputMetadata]>) -> ClassPropertyMapping<InputOrOutput> {
    let mut obj: Map<String, InputOrOutput> = Map::default();

    if let Some(metadata) = metadata {
        for item in metadata {
            let class_prop = &item.name;
            let binding_name = item.alias.as_ref().filter(|alias| !alias.is_empty()).unwrap_or(class_prop);

            obj.insert(
                class_prop.clone(),
                InputOrOutput {
                    class_property_name: class_prop.clone(),
                    binding_property_name: binding_name.clone(),
                    is_signal: false,
                },
            );
        }
    }

    ClassPropertyMapping::from_entries(obj)
}

// tcb_adapter.ts:96 filterInputMappings
fn filter_input_mappings(
    source: &ClassPropertyMapping<TcbInputMapping>,
    allowed: Option<&[HostDirectiveBinding]>,
) -> ClassPropertyMapping<TcbInputMapping> {
    let mut result: Map<String, TcbInputMapping> = Map::default();

    if let Some(allowed) = allowed.filter(|allowed| !allowed.is_empty()) {
        for HostDirectiveBinding { public_name, binding_name } in allowed {
            if let Some(bindings) = source.get_by_binding_property_name(public_name) {
                for binding in bindings {
                    result.insert(
                        binding.input.class_property_name.clone(),
                        TcbInputMapping {
                            input: InputOrOutput {
                                class_property_name: binding.input.class_property_name.clone(),
                                binding_property_name: binding_name.clone(),
                                is_signal: binding.input.is_signal,
                            },
                            required: binding.required,
                            transform_type: binding.transform_type.clone(),
                        },
                    );
                }
            }
        }
    }

    ClassPropertyMapping::from_entries(result)
}

// tcb_adapter.ts:122 filterOutputMappings
fn filter_output_mappings(
    source: &ClassPropertyMapping<InputOrOutput>,
    allowed: Option<&[HostDirectiveBinding]>,
) -> ClassPropertyMapping<InputOrOutput> {
    let mut result: Map<String, InputOrOutput> = Map::default();

    if let Some(allowed) = allowed.filter(|allowed| !allowed.is_empty()) {
        for HostDirectiveBinding { public_name, binding_name } in allowed {
            if let Some(bindings) = source.get_by_binding_property_name(public_name) {
                for binding in bindings {
                    result.insert(
                        binding.class_property_name.clone(),
                        InputOrOutput {
                            class_property_name: binding.class_property_name.clone(),
                            binding_property_name: binding_name.clone(),
                            is_signal: binding.is_signal,
                        },
                    );
                }
            }
        }
    }

    ClassPropertyMapping::from_entries(result)
}

// tcb_adapter.ts:157 resolveHostDirectives: the directives a host's `resolvedHostDirectives` put on every element the
// host matches, each chain's inner host directives first. Mirrors ngtsc's `HostDirectivesResolver.resolve`.
//
// The analyzer attaches these to the declaration that hosts them, not to the template scope. A
// host directive is not a member of the consumer's scope, so it is only ever registered alongside
// its host and never under its own selector.
pub fn resolve_host_directives(
    resolved_host_directives: Option<&[ResolvedHostDirectiveMetadata]>,
    file_path: &str,
    requires_inline: bool,
    content: Option<&str>,
    consumer_path: Option<&str>,
    ancestors: &mut IndexSet<String, FxBuildHasher>,
    type_parameter_refs: &mut TypeParameterRefs,
) -> Vec<DirectiveRef> {
    let Some(resolved_host_directives) = resolved_host_directives.filter(|hds| !hds.is_empty()) else {
        return vec![];
    };

    let mut results = vec![];

    for hd in resolved_host_directives {
        let host_decl = &hd.directive;
        let host_decl_path = host_decl.file_path.as_deref().filter(|path| !path.is_empty()).unwrap_or(file_path);
        let host_key = format!("{host_decl_path}#{}", host_decl.name);
        if ancestors.contains(&host_key) {
            continue;
        }

        // Recursively resolve chained host directives first
        ancestors.insert(host_key.clone());
        let chained = resolve_host_directives(
            host_decl.resolved_host_directives.as_deref(),
            host_decl_path,
            requires_inline,
            content,
            consumer_path,
            ancestors,
            type_parameter_refs,
        );
        ancestors.shift_remove(&host_key);
        results.extend(chained);

        let host_meta =
            declaration_to_metadata(host_decl, host_decl_path, requires_inline, content, consumer_path, type_parameter_refs);

        let mapped_inputs = filter_input_mappings(&host_meta.inputs, hd.inputs.as_deref());
        let mapped_outputs = filter_output_mappings(&host_meta.outputs, hd.outputs.as_deref());

        results.push(Rc::new(TcbDirectiveMetadata {
            match_source: MatchSource::HostDirective,
            inputs: mapped_inputs,
            outputs: mapped_outputs,
            ..host_meta
        }));
    }

    results
}

/// `TcbTypeCheckBlockMetadata`, owning its bound target (`as_tcb_meta` borrows the generator's view of it)
pub struct AdaptedTcbMeta<'a> {
    pub id: TypeCheckId,
    pub bound_target: Rc<dyn BoundTarget<'a, DirectiveRef> + 'a>,
    pub pipes: Option<Map<String, TcbPipeMetadata>>,
    pub schemas: Vec<SchemaMetadata>,
    pub is_standalone: bool,
    pub preserve_whitespaces: bool,
}

impl<'a> AdaptedTcbMeta<'a> {
    pub fn as_tcb_meta(&self) -> TcbTypeCheckBlockMetadata<'a, '_> {
        TcbTypeCheckBlockMetadata {
            id: self.id.clone(),
            bound_target: &*self.bound_target,
            pipes: self.pipes.clone(),
            schemas: self.schemas.clone(),
            is_standalone: self.is_standalone,
            preserve_whitespaces: self.preserve_whitespaces,
        }
    }
}

/// adaptTcbInput's result: `{tcbMeta, targetMeta, hostElement}`, with the extended type parameters TypeScript keeps
/// on `targetMeta.typeParameters` and on the directives' metadata (`TypeParameterRefs`)
pub struct AdaptedTcbInput<'a> {
    pub tcb_meta: AdaptedTcbMeta<'a>,
    pub target_meta: TcbComponentMetadata,
    pub host_element: Option<&'a R3HostElement<'a>>,
    /// `targetMeta.typeParameters` as `ExtendedTcbTypeParameter[]`
    pub target_type_parameters: Option<Vec<ExtendedTcbTypeParameter>>,
    pub type_parameter_refs: TypeParameterRefs,
}

// tcb_adapter.ts:214 adaptTcbInput. The host element lives in `allocator`.
pub fn adapt_tcb_input<'a>(
    allocator: &'a Allocator,
    target: &TcbTargetInput<'a>,
    type_check_id: &TypeCheckId,
    file_path: &str,
    config: &NgpTypeCheckingConfig,
    requires_inline: bool,
    content: &str,
) -> AdaptedTcbInput<'a> {
    let mut type_parameter_refs = TypeParameterRefs::default();
    let content_opt = Some(content);
    // `target.isStandalone ?? target.ownDeclaration.isStandalone ?? false`: always set
    let is_standalone = target.is_standalone;
    let is_selectorless = target.own_declaration.declaration_type == "component"
        && is_standalone
        && target.selectorless_enabled.unwrap_or(false);

    let own = &target.own_declaration;
    let declarations: Vec<&DeclarationMetadata> = if is_standalone {
        std::iter::once(own)
            .chain(target.declarations.iter().filter(|d| {
                !(d.name == own.name
                    && d.r#ref.local_alias.as_deref() == Some(own.name.as_str())
                    && d.r#ref.consumer_import.is_none())
            }))
            .collect()
    } else {
        target.declarations.iter().collect()
    };

    let matcher = if is_selectorless {
        let mut registry: Map<String, Vec<DirectiveRef>> = Map::default();
        for decl in &declarations {
            if decl.declaration_type != "directive" && decl.declaration_type != "component" {
                continue;
            }
            let decl_path = decl.file_path.as_deref().filter(|path| !path.is_empty()).unwrap_or(file_path);
            let meta = Rc::new(declaration_to_metadata(
                decl,
                decl_path,
                requires_inline,
                content_opt,
                Some(file_path),
                &mut type_parameter_refs,
            ));
            let host_directives = resolve_host_directives(
                decl.resolved_host_directives.as_deref(),
                decl_path,
                requires_inline,
                content_opt,
                Some(file_path),
                &mut IndexSet::default(),
                &mut type_parameter_refs,
            );
            let entry = registry.entry(decl.name.clone()).or_default();
            entry.extend(host_directives);
            entry.push(meta);
        }
        DirectiveMatcher::Selectorless(SelectorlessMatcher::new(registry))
    } else {
        let mut selector_matcher = SelectorMatcher::<Vec<DirectiveRef>>::new();
        for decl in &declarations {
            if decl.declaration_type != "directive" && decl.declaration_type != "component" {
                continue;
            }
            if let Some(selector) = decl.selector.as_deref().filter(|selector| !selector.is_empty()) {
                let decl_path = decl.file_path.as_deref().filter(|path| !path.is_empty()).unwrap_or(file_path);
                let mut host_directives = resolve_host_directives(
                    decl.resolved_host_directives.as_deref(),
                    decl_path,
                    requires_inline,
                    content_opt,
                    Some(file_path),
                    &mut IndexSet::default(),
                    &mut type_parameter_refs,
                );
                host_directives.push(Rc::new(declaration_to_metadata(
                    decl,
                    decl_path,
                    requires_inline,
                    content_opt,
                    Some(file_path),
                    &mut type_parameter_refs,
                )));
                selector_matcher.add_selectables(&CssSelector::parse(selector), host_directives);
            }
        }
        DirectiveMatcher::Selector(selector_matcher)
    };

    let host_directives_on_target = resolve_host_directives(
        target.own_declaration.resolved_host_directives.as_deref(),
        file_path,
        requires_inline,
        content_opt,
        Some(file_path),
        &mut IndexSet::default(),
        &mut type_parameter_refs,
    );

    let binder =
        R3TargetBinder::with_foreign_component_matcher(Some(matcher), create_foreign_component_matcher(target.foreign_imports.as_deref()));
    let host =
        if config.type_check_host_bindings == Some(false) { None } else { adapt_host_element(allocator, target, content) };
    let host = host.map(|node| {
        let mut directives = host_directives_on_target;
        directives.push(Rc::new(declaration_to_metadata(
            &target.own_declaration,
            file_path,
            requires_inline,
            content_opt,
            Some(file_path),
            &mut type_parameter_refs,
        )));
        TargetHost { node, directives }
    });
    let host_element = host.as_ref().map(|host| host.node);
    // (`template` is always set: `[]` for a directive, which JavaScript takes as present)
    let bound_target = binder.bind(Target { template: Some(target.template_nodes), host });
    // tcb_adapter.ts:324 "Monkey-patch missing method in reference compiler": `getConflictingHostDirectiveBindings`
    // is part of `BoundTarget` here.

    let mut pipes: Option<Map<String, TcbPipeMetadata>> = None;

    if let Some(pipe_registry) = &target.pipe_registry {
        for (name, meta) in pipe_registry {
            let import_path = meta.import_path.as_deref().filter(|path| !path.is_empty());
            let is_same_file = meta.file_path.as_deref() == Some(file_path);
            let is_local = (requires_inline && is_same_file) || import_path.is_none();
            let module_name = if is_local { None } else { import_path.map(str::to_string) };
            pipes.get_or_insert_with(Map::default).insert(
                name.clone(),
                TcbPipeMetadata {
                    name: name.clone(),
                    r#ref: TcbReferenceMetadata {
                        key: match import_path {
                            Some(import_path) => format!("{import_path}#{}", meta.class_name),
                            None => format!("{file_path}#{}", meta.class_name),
                        },
                        name: meta.class_name.clone(),
                        module_name,
                        is_local,
                        unexported_diagnostic: None,
                        node_name_span: None,
                        node_file_path: None,
                    },
                    is_explicitly_deferred: meta.is_explicitly_deferred.unwrap_or(false),
                    deferred_blocks: meta.deferred_blocks.as_ref().map(|blocks| blocks.iter().cloned().collect()),
                },
            );
        }
    }

    let type_parameters = map_type_parameters(target.type_parameters.as_deref());
    let type_arguments = type_parameters.as_ref().map(|type_parameters| {
        if config.use_context_generic_type {
            type_parameters.iter().map(|p| p.name.clone()).collect()
        } else {
            vec!["any".to_string(); type_parameters.len()]
        }
    });

    let typecheck_import = target.class_meta.r#ref.as_ref().and_then(|r| r.typecheck_import.as_ref());
    let is_local = requires_inline || typecheck_import.is_none();
    let module_name = if is_local { None } else { typecheck_import.map(|import| import.specifier.clone()) };

    let target_meta = TcbComponentMetadata {
        r#ref: TcbReferenceMetadata {
            key: match typecheck_import {
                Some(import) => format!("{}#{}", import.specifier, target.class_name),
                None => format!("{file_path}#{}", target.class_name),
            },
            name: target.class_name.clone(),
            module_name,
            is_local,
            unexported_diagnostic: None,
            node_name_span: None,
            node_file_path: None,
        },
        type_parameters: type_parameters
            .as_ref()
            .map(|params| params.iter().map(ExtendedTcbTypeParameter::to_tcb_type_parameter).collect()),
        type_arguments,
    };

    let tcb_meta = AdaptedTcbMeta {
        id: type_check_id.clone(),
        bound_target: Rc::new(bound_target),
        pipes,
        schemas: target.schemas.clone(),
        // `target.isStandalone ?? true`
        is_standalone: target.is_standalone,
        preserve_whitespaces: target.preserve_whitespaces.unwrap_or(false),
    };

    AdaptedTcbInput { tcb_meta, target_meta, host_element, target_type_parameters: type_parameters, type_parameter_refs }
}

// tcb_adapter.ts:388 adaptHostElement
fn adapt_host_element<'a>(
    allocator: &'a Allocator,
    target: &TcbTargetInput<'a>,
    content: &str,
) -> Option<&'a R3HostElement<'a>> {
    // (TypeScript gives its spans a dummy file name, `hostTcb.ts`, which the spans here do not carry.)
    let mut object_literal_bindings: Vec<HostObjectLiteralBinding> = vec![];
    let mut host_binding_decorators: Vec<HostBindingDecorator> = vec![];
    let mut host_listener_decorators: Vec<HostListenerDecorator> = vec![];

    for prop in &target.host_properties {
        if !is_static_source_node(&prop.key) || prop.value.kind != ExpressionValueKindMetadata::String {
            continue;
        }

        object_literal_bindings.push(HostObjectLiteralBinding {
            key: adapt_source_node(&prop.key, content),
            value: adapt_source_node(&prop.value, content),
            source_span: create_span(content, prop.key.source_span.start, prop.value.source_span.end),
        });
    }

    for decorator in &target.host_bindings {
        // ngtsc only type-checks `@HostBinding`s on property declarations; those on getters and
        // setters are emitted but never reach the type-check block.
        // https://github.com/angular/angular/blob/cda8962/packages/compiler-cli/src/ngtsc/annotations/directive/src/shared.ts#L681-L682
        if decorator.is_accessor || !is_static_source_node(&decorator.member_name) {
            continue;
        }

        host_binding_decorators.push(HostBindingDecorator {
            member_name: as_static_source_node(adapt_source_node(&decorator.member_name, content)),
            member_span: create_span(content, decorator.member_span.start, decorator.member_span.end),
            decorator_span: create_span(content, decorator.decorator_span.start, decorator.decorator_span.end),
            arguments: decorator.arguments.iter().map(|arg| adapt_source_node(arg, content)).collect(),
        });
    }

    for decorator in &target.host_listeners {
        let Some(event_name) =
            decorator.event_name.as_ref().filter(|event_name| event_name.kind == ExpressionValueKindMetadata::String)
        else {
            continue;
        };
        if !is_static_source_node(&decorator.method_name) {
            continue;
        }

        host_listener_decorators.push(HostListenerDecorator {
            event_name: Some(adapt_source_node(event_name, content)),
            member_name: as_static_source_node(adapt_source_node(&decorator.method_name, content)),
            member_span: create_span(content, decorator.member_span.start, decorator.member_span.end),
            decorator_span: create_span(content, decorator.decorator_span.start, decorator.decorator_span.end),
            arguments: decorator.args.iter().map(|arg| adapt_source_node(arg, content)).collect(),
        });
    }

    let own = &target.own_declaration;
    create_host_element(
        allocator,
        if own.declaration_type == "directive" { "directive" } else { "component" },
        own.selector.as_deref().filter(|selector| !selector.is_empty()),
        create_span(content, own.name_span.start, own.name_span.end),
        &object_literal_bindings,
        &host_binding_decorators,
        &host_listener_decorators,
    )
}

/// `adaptSourceNode(...) as StaticSourceNode`, for a node known to be static
fn as_static_source_node(node: SourceNode) -> StaticSourceNode {
    match node {
        SourceNode::Static(node) => node,
        SourceNode::Unspecified { .. } => panic!("a static source node is static"),
    }
}

// tcb_adapter.ts:457 adaptSourceNode
fn adapt_source_node(node: &ExpressionValueMetadata, content: &str) -> SourceNode {
    let source_span = create_span(content, node.source_span.start, node.source_span.end);
    let source = substring(content, node.source_span.start, node.source_span.end).to_string();

    if is_static_source_node(node) {
        return SourceNode::Static(StaticSourceNode {
            kind: if node.kind == ExpressionValueKindMetadata::String {
                StaticSourceNodeKind::String
            } else {
                StaticSourceNodeKind::Identifier
            },
            text: node.text.clone().unwrap_or_default(),
            source,
            source_span,
        });
    }

    SourceNode::Unspecified { source_span }
}

// tcb_adapter.ts:477 declarationToMetadata. Records the declaration's extended type parameters in
// `type_parameter_refs`. (TypeScript also marks the declaration's coerced inputs `isCoerced` in place, which only
// repeats what `coercedInputFields` computes.)
pub fn declaration_to_metadata(
    decl: &DeclarationMetadata,
    file_path: &str,
    requires_inline: bool,
    content: Option<&str>,
    consumer_path: Option<&str>,
    type_parameter_refs: &mut TypeParameterRefs,
) -> TcbDirectiveMetadata {
    let typecheck_import = decl.r#ref.typecheck_import.as_ref();
    let is_same_file = consumer_path == Some(file_path);
    let is_local = (requires_inline && is_same_file) || typecheck_import.is_none();
    let module_name = if is_local { None } else { typecheck_import.map(|import| import.specifier.clone()) };
    let symbol_name =
        if is_local { decl.name.clone() } else { typecheck_import.map_or_else(|| decl.name.clone(), |i| i.symbol.clone()) };
    let fields = decl.flattened_fields.as_deref().or(decl.fields.as_deref()).unwrap_or(&[]);

    let key = match typecheck_import {
        Some(import) => format!("{}#{symbol_name}", import.specifier),
        None => format!("{file_path}#{symbol_name}"),
    };
    let type_parameters = map_type_parameters(decl.type_parameters.as_deref());
    if let Some(type_parameters) = &type_parameters {
        type_parameter_refs.insert(key.clone(), type_parameters.clone());
    }

    let inputs_list: Vec<&InputMetadata> =
        fields.iter().filter(|f| f.kind == "input").filter_map(|f| f.input.as_ref()).collect();
    let coercions: Set<&str> =
        fields.iter().filter(|f| f.kind == "coercion").filter_map(|f| f.coercion.as_deref()).collect();

    TcbDirectiveMetadata {
        match_source: MatchSource::Selector,
        r#ref: TcbReferenceMetadata {
            key,
            name: symbol_name,
            module_name,
            is_local,
            unexported_diagnostic: None,
            node_file_path: Some(file_path.to_string()),
            node_name_span: Some(SpanInfo { start: decl.name_span.start, end: decl.name_span.end }),
        },
        name: decl.name.clone(),
        selector: decl.selector.clone().filter(|selector| !selector.is_empty()),
        is_component: decl.declaration_type == "component",
        is_generic: decl.type_parameters.as_ref().is_some_and(|params| !params.is_empty()),
        is_structural: decl.is_structural,
        is_standalone: decl.is_standalone.unwrap_or(false),
        is_explicitly_deferred: decl.is_explicitly_deferred,
        deferred_blocks: decl.deferred_blocks.as_ref().map(|blocks| blocks.iter().cloned().collect()),
        preserve_whitespaces: false,
        export_as: decl.export_as.clone(),
        type_parameters: type_parameters
            .as_ref()
            .map(|params| params.iter().map(ExtendedTcbTypeParameter::to_tcb_type_parameter).collect()),
        inputs: create_input_property_mapping(Some(&inputs_list), content),
        outputs: {
            let outputs_list: Vec<&OutputMetadata> =
                fields.iter().filter(|f| f.kind == "output").filter_map(|f| f.output.as_ref()).collect();
            create_output_property_mapping(Some(&outputs_list))
        },
        requires_inline_type_ctor: decl.has_non_exported_bounds,
        ng_template_guards: decl
            .ng_template_guards
            .iter()
            .map(|g| TemplateGuardMeta {
                input_name: g.input_name.clone(),
                r#type: if g.type_ == "binding" { TemplateGuardType::Binding } else { TemplateGuardType::Invocation },
            })
            .collect(),
        has_ng_template_context_guard: decl.has_ng_template_context_guard,
        has_ng_field_directive: decl.has_ng_field_directive,
        coerced_input_fields: inputs_list
            .iter()
            .filter(|i| !i.is_signal && (i.transform.is_some() || i.is_coerced || coercions.contains(i.name.as_str())))
            .map(|i| i.name.clone())
            .collect(),
        restricted_input_fields: fields
            .iter()
            .filter(|f| f.kind == "input")
            .filter_map(|f| f.input.as_ref())
            .filter(|i| i.is_restricted)
            .map(|i| i.name.clone())
            .collect(),
        string_literal_input_fields: fields
            .iter()
            .filter(|f| f.kind == "input")
            .filter_map(|f| f.input.as_ref())
            .filter(|i| i.is_literal)
            .map(|i| i.name.clone())
            .collect(),
        undeclared_input_fields: fields
            .iter()
            .filter(|f| f.kind == "input")
            .filter_map(|f| f.input.as_ref())
            .filter(|i| i.property_span.is_none())
            .map(|i| i.name.clone())
            .collect(),
        public_methods: decl.public_methods.iter().cloned().collect(),
        ng_content_selectors: decl.ng_content_selectors.clone(),
        animation_trigger_names: decl.animation_trigger_names.as_ref().map(|names| LegacyAnimationTriggerNames {
            includes_dynamic_animations: names.includes_dynamic_animations,
            static_trigger_names: names.static_trigger_names.clone(),
        }),
    }
}

// tcb_adapter.ts:585 ExtendedTcbTypeParameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedTcbTypeParameter {
    pub name: String,
    pub representation: String,
    pub representation_with_default: String,
    pub type_refs: Option<Vec<TypeRefMetadata>>,
}

impl ExtendedTcbTypeParameter {
    /// The `TcbTypeParameter` it extends
    pub fn to_tcb_type_parameter(&self) -> TcbTypeParameter {
        TcbTypeParameter {
            name: self.name.clone(),
            representation: self.representation.clone(),
            representation_with_default: self.representation_with_default.clone(),
        }
    }

    /// A `TcbTypeParameter` without type references
    pub fn from_tcb_type_parameter(param: &TcbTypeParameter) -> Self {
        Self {
            name: param.name.clone(),
            representation: param.representation.clone(),
            representation_with_default: param.representation_with_default.clone(),
            type_refs: None,
        }
    }
}

// tcb_adapter.ts:589 mapTypeParameters
fn map_type_parameters(params: Option<&[TypeParameterMetadata]>) -> Option<Vec<ExtendedTcbTypeParameter>> {
    let params = params?;
    Some(
        params
            .iter()
            .map(|p| ExtendedTcbTypeParameter {
                name: p.name.clone(),
                representation: p.representation.clone(),
                representation_with_default: p.representation_with_default.clone(),
                type_refs: p.type_refs.clone(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use ng_analyze::{AngularFieldMetadata, ImportableRef, ReferenceMetadata, SpanMetadata, TransformMetadata};

    use super::*;

    fn input(name: &str, alias: Option<&str>) -> InputMetadata {
        InputMetadata {
            name: name.to_string(),
            alias: alias.map(str::to_string),
            required: false,
            is_signal: false,
            decorator_span: None,
            property_span: Some(SpanMetadata::default()),
            transform: None,
            is_restricted: false,
            is_literal: false,
            is_coerced: false,
        }
    }

    fn field_input(input: InputMetadata) -> AngularFieldMetadata {
        AngularFieldMetadata { kind: "input".to_string(), input: Some(input), output: None, query: None, coercion: None }
    }

    fn declaration(name: &str, typecheck_import: Option<(&str, &str)>) -> DeclarationMetadata {
        DeclarationMetadata {
            name: name.to_string(),
            r#ref: ReferenceMetadata {
                consumer_import: None,
                typecheck_import: typecheck_import.map(|(specifier, symbol)| ImportableRef {
                    specifier: specifier.to_string(),
                    symbol: symbol.to_string(),
                }),
                local_alias: None,
            },
            ref_in_declaring_module: None,
            name_span: SpanMetadata { start: 3, end: 7 },
            declaration_type: "directive".to_string(),
            selector: Some("[dir]".to_string()),
            pipe_name: None,
            is_standalone: Some(true),
            export_as: None,
            file_id: 0,
            file_path: None,
            host_directives: None,
            fields: None,
            flattened_fields: None,
            ng_content_selectors: None,
            type_parameters: None,
            has_ng_template_context_guard: false,
            ng_template_guards: vec![],
            animation_trigger_names: None,
            has_ng_field_directive: false,
            public_methods: vec![],
            is_forward_ref: false,
            is_structural: false,
            cycle_prone: None,
            is_exported: true,
            has_non_exported_bounds: false,
            is_explicitly_deferred: false,
            deferred_blocks: None,
            resolved_host_directives: None,
        }
    }

    #[test]
    fn input_mappings_take_transform_types_from_the_source() {
        let content = "x: (v: é | number) => 1";
        let mut transformed = input("value", Some("val"));
        transformed.transform = Some(TransformMetadata {
            kind: "type".to_string(),
            span: SpanMetadata::default(),
            // `é | number` in UTF-16 indices
            type_span: Some(SpanMetadata { start: 7, end: 17 }),
        });
        let plain = input("other", None);
        let mapping = create_input_property_mapping(Some(&[&transformed, &plain]), Some(content));
        let value = mapping.get_by_class_property_name("value").unwrap();
        assert_eq!(value.input.binding_property_name, "val");
        assert_eq!(value.transform_type.as_deref(), Some("é | number"));
        assert_eq!(mapping.get_by_binding_property_name("other").unwrap()[0].input.class_property_name, "other");
    }

    #[test]
    fn declarations_to_metadata() {
        let mut decl = declaration("Dir", Some(("./dir", "Dir")));
        let mut coerced = input("coerced", None);
        coerced.property_span = None;
        decl.fields = Some(vec![
            field_input(input("plain", None)),
            field_input(coerced),
            AngularFieldMetadata {
                kind: "coercion".to_string(),
                input: None,
                output: None,
                query: None,
                coercion: Some("coerced".to_string()),
            },
        ]);
        decl.type_parameters = Some(vec![TypeParameterMetadata {
            name: "T".to_string(),
            representation: "T extends Foo".to_string(),
            representation_with_default: "T extends Foo = any".to_string(),
            has_default: true,
            type_refs: None,
        }]);
        let mut refs = TypeParameterRefs::default();
        let meta = declaration_to_metadata(&decl, "/a.ts", false, None, Some("/a.ts"), &mut refs);
        assert_eq!(meta.r#ref.key, "./dir#Dir");
        assert_eq!(meta.r#ref.module_name.as_deref(), Some("./dir"));
        assert!(!meta.r#ref.is_local);
        assert!(meta.is_generic);
        assert_eq!(meta.coerced_input_fields.iter().collect::<Vec<_>>(), ["coerced"]);
        assert_eq!(meta.undeclared_input_fields.iter().collect::<Vec<_>>(), ["coerced"]);
        assert_eq!(refs["./dir#Dir"][0].representation_with_default, "T extends Foo = any");

        // same file and inline: local
        let meta = declaration_to_metadata(&decl, "/a.ts", true, None, Some("/a.ts"), &mut refs);
        assert_eq!((meta.r#ref.key.as_str(), meta.r#ref.module_name.as_deref(), meta.r#ref.is_local), ("./dir#Dir", None, true));

        let local = declaration("Local", None);
        let meta = declaration_to_metadata(&local, "/b.ts", false, None, Some("/a.ts"), &mut refs);
        assert_eq!((meta.r#ref.key.as_str(), meta.r#ref.is_local), ("/b.ts#Local", true));
    }

    #[test]
    fn host_directives_expose_their_mapped_bindings_inner_first() {
        let mut inner = declaration("Inner", Some(("./inner", "Inner")));
        inner.fields = Some(vec![field_input(input("a", Some("aliasA")))]);
        let mut outer = declaration("Outer", Some(("./outer", "Outer")));
        outer.resolved_host_directives = Some(vec![ResolvedHostDirectiveMetadata {
            directive: inner,
            inputs: Some(vec![HostDirectiveBinding {
                public_name: "aliasA".to_string(),
                binding_name: "exposed".to_string(),
            }]),
            outputs: None,
        }]);
        let hds = vec![ResolvedHostDirectiveMetadata { directive: outer, inputs: None, outputs: None }];
        let mut refs = TypeParameterRefs::default();
        let resolved =
            resolve_host_directives(Some(&hds), "/a.ts", false, None, Some("/a.ts"), &mut IndexSet::default(), &mut refs);
        let names: Vec<_> = resolved.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Inner", "Outer"]);
        assert_eq!(resolved[0].match_source, MatchSource::HostDirective);
        assert_eq!(resolved[0].inputs.get_by_class_property_name("a").unwrap().input.binding_property_name, "exposed");
        // nothing exposed from the outer one
        assert!(resolved[1].inputs.class_property_names().is_empty());
    }
}
