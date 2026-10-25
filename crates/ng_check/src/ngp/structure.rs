//! `ngcheck --structure` (not a port): the project's components as `oxc_angular_structure::schema` writes them, for
//! tools that read templates without compiling them.
//!
//! Each template is the tree type checking binds (the `diagNodes` parse), written through a [`Binder`] over ngtsc's
//! bound target: the directives and components matched on each element and template, from the component's own
//! imports; what each reference and template name resolves to; and which directive input or output each binding
//! sets. The component's metadata comes from ng-analyze, its members and their JSDoc from its file.

use std::rc::Rc;

use ng_analyze::{AnalysisResult, ClassMetadata, ComponentMetadata};
use oxc_angular_compiler::ast::expression::AngularExpression;
use oxc_angular_compiler::ast::r3::R3Reference;
use oxc_angular_structure::schema::{
    ClassBinding, ComponentStructure, Consumer, CssSelector, DirectiveMatch, HostBinding,
    HostBindingKind, TemplateError, TemplateStructure,
};
use oxc_angular_structure::{
    Binder, BindingRef, BoundEntity, BoundReference, Owner, Positions, SCHEMA_VERSION,
    WriteOptions, Writer, class_members,
};

use crate::angular::directive_matching::CssSelector as ParsedSelector;
use crate::angular::property_mapping::AsInputOrOutput;
use crate::angular::t2_api::{
    Binding, BindingConsumer, BoundTarget, DirectiveMeta, DirectiveOwner, ReferenceTarget,
    TemplateEntity,
};
use crate::ngp::compiler_utils::get_or_parse_component_template;
use crate::ngp::dependency_templates::DependencyTemplates;
use crate::ngp::file_analysis::get_or_create_file_analysis;
use crate::ngp::processor::{TypeCheckContext, build_tcb_targets};
use crate::ngp::tcb::prepare_tcb_targets;
use crate::typecheck::api::DirectiveRef;

/// ngtsc's bound target, as the structure's [`Binder`].
struct BoundBinder<'a, 'b> {
    bound: &'b Rc<dyn BoundTarget<'a, DirectiveRef> + 'a>,
}

fn owner(owner: Owner<'_>) -> DirectiveOwner<'_> {
    match owner {
        Owner::Element(e) => DirectiveOwner::Element(e),
        Owner::Template(t) => DirectiveOwner::Template(t),
    }
}

impl<'a> Binder<'a> for BoundBinder<'a, '_> {
    fn matches(&self, node: Owner<'a>) -> Option<Vec<DirectiveMatch>> {
        let directives = self.bound.get_directives_of_node(owner(node)).unwrap_or_default();
        Some(
            directives
                .iter()
                .map(|d| DirectiveMatch {
                    name: d.name().to_string(),
                    selector: d.selector().map(str::to_string),
                    component: d.is_component(),
                    reference: d.ref_key().to_string(),
                })
                .collect(),
        )
    }

    fn reference(&self, reference: &'a R3Reference<'a>) -> Option<BoundReference<'a>> {
        Some(match self.bound.get_reference_target(reference)? {
            ReferenceTarget::Element(e) => BoundReference::Element(e),
            ReferenceTarget::Template(t) => BoundReference::Template(t),
            ReferenceTarget::Directive { directive, node } => {
                let owner = match node {
                    DirectiveOwner::Element(e) => Owner::Element(e),
                    DirectiveOwner::Template(t) => Owner::Template(t),
                    _ => return None,
                };
                BoundReference::Directive { owner, directive: directive.name().to_string() }
            }
        })
    }

    fn expression_target(&self, expr: &'a AngularExpression<'a>) -> Option<BoundEntity<'a>> {
        Some(match self.bound.get_expression_target(expr)? {
            TemplateEntity::Reference(r) => BoundEntity::Reference(r),
            TemplateEntity::Variable(v) => BoundEntity::Variable(v),
            TemplateEntity::LetDeclaration(l) => BoundEntity::Let(l),
        })
    }

    fn consumer(&self, binding: BindingRef<'a>) -> Option<Consumer> {
        let (key, name, is_output) = match binding {
            BindingRef::Attribute(a) => (Binding::TextAttribute(a), a.name.as_str(), false),
            BindingRef::Input(b) => (Binding::BoundAttribute(b), b.name.as_str(), false),
            BindingRef::Output(o) => (Binding::BoundEvent(o), o.name.as_str(), true),
        };
        let BindingConsumer::Directive(directive) = self.bound.get_consumer_of_binding(key)? else {
            return None;
        };
        let property = if is_output {
            directive
                .outputs()
                .get_by_binding_property_name(name)
                .and_then(<[_]>::first)
                .map(|o| o.class_property_name.clone())
        } else {
            directive
                .inputs()
                .get_by_binding_property_name(name)
                .and_then(<[_]>::first)
                .map(|i| i.input_or_output().class_property_name.clone())
        };
        Some(Consumer { directive: directive.name().to_string(), property })
    }
}

fn selectors(selector: Option<&str>) -> Vec<CssSelector> {
    fn convert(s: &ParsedSelector) -> CssSelector {
        CssSelector {
            element: s.element.clone(),
            attrs: s
                .attrs
                .chunks(2)
                .map(|pair| (pair[0].clone(), pair.get(1).cloned().unwrap_or_default()))
                .collect(),
            classes: s.class_names.clone(),
            not: s.not_selectors.iter().map(convert).collect(),
        }
    }
    selector.map(|s| ParsedSelector::parse(s).iter().map(convert).collect()).unwrap_or_default()
}

/// The `host` metadata's entries and the `@HostBinding` / `@HostListener` members.
fn host_bindings(cls: &ClassMetadata, component: &ComponentMetadata) -> Vec<HostBinding> {
    let mut out = vec![];
    for entry in component.host_metadata.iter().flatten() {
        let key = entry.key.as_str();
        let source = entry.expression.clone().or_else(|| entry.value.clone()).unwrap_or_default();
        let (kind, name) =
            if let Some(name) = key.strip_prefix('[').and_then(|k| k.strip_suffix(']')) {
                (HostBindingKind::Property, name)
            } else if let Some(name) = key.strip_prefix('(').and_then(|k| k.strip_suffix(')')) {
                (HostBindingKind::Listener, name)
            } else {
                (HostBindingKind::Attribute, key)
            };
        out.push(HostBinding { kind, name: name.to_string(), source });
    }
    for binding in &cls.host_bindings {
        let member = binding.member_name.text.clone().unwrap_or_default();
        let name = binding.host_property_name.clone().unwrap_or_else(|| member.clone());
        out.push(HostBinding { kind: HostBindingKind::Property, name, source: member });
    }
    for listener in &cls.host_listeners {
        let method = listener.method_name.text.clone().unwrap_or_default();
        let args: Vec<String> = listener.args.iter().filter_map(|a| a.text.clone()).collect();
        let name = listener
            .resolved_event_name
            .clone()
            .or_else(|| listener.event_name.as_ref().and_then(|e| e.text.clone()))
            .unwrap_or_default();
        out.push(HostBinding {
            kind: HostBindingKind::Listener,
            name,
            source: format!("{method}({})", args.join(", ")),
        });
    }
    out
}

fn import_name(reference: &ng_analyze::ReferenceMetadata) -> Option<String> {
    reference.local_alias.clone().or_else(|| {
        reference
            .consumer_import
            .as_ref()
            .or(reference.typecheck_import.as_ref())
            .map(|i| i.symbol.clone())
    })
}

/// The components of one file.
pub fn file_structure<'a>(
    ctx: &TypeCheckContext<'a, '_>,
    file_path: &str,
    content: &str,
    metadata: &AnalysisResult,
) -> Vec<ComponentStructure> {
    let content: &'a str = ctx.allocator.alloc_str(content);
    let classes = &metadata.classes;
    if !classes.iter().any(|c| c.component.is_some()) {
        return vec![];
    }
    let file_positions = Positions::new(content);
    let templates_by_class = Rc::clone(
        &get_or_create_file_analysis(&mut ctx.file_cache.borrow_mut(), file_path).parsed_templates,
    );
    // Every component's template parsed up front, with its parse errors: one that fails is still written, with them
    let mut parse_errors = vec![];
    for class_meta in classes {
        let Some(component) = &class_meta.component else { continue };
        let Some(class_name) = class_meta.class_name.as_ref().filter(|name| !name.is_empty())
        else {
            continue;
        };
        let parsed = get_or_parse_component_template(
            ctx.allocator,
            &templates_by_class,
            class_meta,
            component,
            file_path,
            content,
            ctx.template_parse_options,
        );
        let positions = Positions::new(parsed.parsed.source);
        let errors: Vec<TemplateError> = parsed
            .parsed
            .errors
            .iter()
            .flatten()
            .map(|e| TemplateError {
                message: e.msg.clone(),
                position: positions.of(e.span.start, e.span.end),
            })
            .collect();
        parse_errors.push((class_name.clone(), errors));
    }
    let dependencies = DependencyTemplates::new(
        ctx.allocator,
        ctx.analyzer,
        ctx.file_cache,
        ctx.template_parse_options,
        ctx.metadata_map,
    );
    dependencies.prefetch(metadata);
    let targets = build_tcb_targets(
        ctx.allocator,
        file_path,
        content,
        classes,
        &templates_by_class,
        ctx.template_parse_options,
        ctx.tcb_config,
        &dependencies,
    );
    let prepared = if targets.is_empty() {
        None
    } else {
        prepare_tcb_targets(
            ctx.allocator,
            file_path,
            targets,
            content,
            ctx.tcb_config,
            Some(classes),
        )
    };
    let bound: Vec<_> = prepared.iter().flat_map(|p| &p.targets).collect();

    let mut components = vec![];
    for (class_name, errors) in parse_errors {
        let Some(cls) =
            classes.iter().find(|c| c.class_name.as_deref() == Some(class_name.as_str()))
        else {
            continue;
        };
        let Some(component) = &cls.component else { continue };
        let template = match bound.iter().find(|t| t.comp.class_name == class_name) {
            Some(target) => {
                let binder = BoundBinder { bound: &target.tcb_meta.bound_target };
                let nodes =
                    Writer::new(target.comp.template_source, &binder, WriteOptions::default())
                        .write(target.comp.template_nodes);
                TemplateStructure { version: SCHEMA_VERSION, nodes, errors }
            }
            // a template that failed to parse, or one type checking skips (JIT)
            None => TemplateStructure { version: SCHEMA_VERSION, nodes: vec![], errors },
        };
        let template_url = component.template_url.as_ref().map(|url| url.resolved_path.clone());
        components.push(ComponentStructure {
            file: file_path.to_string(),
            class_name: class_name.clone(),
            class_position: file_positions.of_utf16(cls.span.start, cls.span.end),
            selector: component.selector.clone(),
            selectors: selectors(component.selector.as_deref()),
            standalone: component.standalone,
            inline_template: template_url.is_none(),
            template_file: template_url.unwrap_or_else(|| file_path.to_string()),
            imports: component.imports.iter().flatten().filter_map(import_name).collect(),
            inputs: cls
                .inputs
                .iter()
                .map(|i| ClassBinding {
                    property: i.name.clone(),
                    binding_name: i.alias.clone().unwrap_or_else(|| i.name.clone()),
                    required: i.required,
                })
                .collect(),
            outputs: cls
                .outputs
                .iter()
                .map(|o| ClassBinding {
                    property: o.name.clone(),
                    binding_name: o.alias.clone().unwrap_or_else(|| o.name.clone()),
                    required: false,
                })
                .collect(),
            host: host_bindings(cls, component),
            members: class_members(content, &class_name),
            template,
        });
    }
    components
}
