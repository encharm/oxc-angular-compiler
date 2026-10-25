//! `ngcheck --structure` (not a port): the project's component templates as JSON, for tools that read templates
//! without compiling them.
//!
//! Each component gives its template's parsed tree (`oxc_angular_compiler::structure`) with, on each element and
//! template, the directives and components the binder matches there (`matches`: `name`, `selector`, `component`),
//! as type checking binds them: from the component's own imports. The tree is the type-check parse, with whitespace
//! kept; its `line`s are the template file's, or the component file's for an inline template literal.

use ng_analyze::AnalysisResult;
use oxc_angular_compiler::structure::{AnnotatedNode, StructureWriter};
use serde_json::{Value, json};

use crate::angular::t2_api::{DirectiveMeta, DirectiveOwner};
use crate::ngp::compiler_utils::get_or_parse_component_template;
use crate::ngp::dependency_templates::DependencyTemplates;
use crate::ngp::file_analysis::get_or_create_file_analysis;
use crate::ngp::processor::{TypeCheckContext, build_tcb_targets};
use crate::ngp::tcb::prepare_tcb_targets;

/// The components of one file: `file`, `className`, `classSpan` (`[start, end]` in the file, as JavaScript string
/// indices), `selector`, `templateFile` (none for an inline template), `nodes` and `errors` (the template's parse
/// errors: `msg`, `line`).
pub fn file_structure<'a>(
    ctx: &TypeCheckContext<'a, '_>,
    file_path: &str,
    content: &str,
    metadata: &AnalysisResult,
) -> Vec<Value> {
    let content: &'a str = ctx.allocator.alloc_str(content);
    let classes = &metadata.classes;
    if !classes.iter().any(|c| c.component.is_some()) {
        return vec![];
    }
    let templates_by_class = std::rc::Rc::clone(
        &get_or_create_file_analysis(&mut ctx.file_cache.borrow_mut(), file_path).parsed_templates,
    );
    let mut errors_by_class = vec![];
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
        let writer = StructureWriter::new(parsed.parsed.source, None);
        let errors: Vec<Value> = parsed
            .parsed
            .errors
            .iter()
            .flatten()
            .map(|e| json!({ "msg": e.msg, "line": writer.line(e.span.start) }))
            .collect();
        errors_by_class.push((class_name.clone(), errors));
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

    let mut components = vec![];
    for target in prepared.iter().flat_map(|prepared| &prepared.targets) {
        let Some(component) = &target.comp.class_meta.component else { continue };
        let bound = &target.tcb_meta.bound_target;
        let annotate = |node: AnnotatedNode<'a>| {
            let owner = match node {
                AnnotatedNode::Element(e) => DirectiveOwner::Element(e),
                AnnotatedNode::Template(t) => DirectiveOwner::Template(t),
            };
            let directives = bound.get_directives_of_node(owner).filter(|d| !d.is_empty())?;
            let matches: Vec<Value> = directives
                .iter()
                .map(|d| json!({ "name": d.name(), "selector": d.selector(), "component": d.is_component() }))
                .collect();
            Some(("matches", Value::Array(matches)))
        };
        let writer = StructureWriter::new(target.comp.template_source, Some(&annotate));
        let errors = errors_by_class
            .iter()
            .find(|(name, _)| *name == target.comp.class_name)
            .map_or_else(Vec::new, |(_, errors)| errors.clone());
        let span = &target.comp.class_meta.span;
        components.push(json!({
            "file": file_path,
            "className": target.comp.class_name,
            "classSpan": [span.start, span.end],
            "selector": component.selector,
            "templateFile": component.template_url.as_ref().map(|url| url.resolved_path.clone()),
            "nodes": writer.nodes(target.comp.template_nodes),
            "errors": errors,
        }));
    }
    // a component type checking skips (a template that failed to parse, a JIT one) still has its errors told
    for (class_name, errors) in errors_by_class {
        if !errors.is_empty() && !components.iter().any(|c| c["className"] == json!(class_name)) {
            let class =
                classes.iter().find(|c| c.class_name.as_deref() == Some(class_name.as_str()));
            let component = class.and_then(|c| c.component.as_ref());
            components.push(json!({
                "file": file_path,
                "className": class_name,
                "classSpan": class.map(|c| [c.span.start, c.span.end]),
                "selector": component.and_then(|c| c.selector.clone()),
                "templateFile": component.and_then(|c| c.template_url.as_ref()).map(|url| url.resolved_path.clone()),
                "nodes": [],
                "errors": errors,
            }));
        }
    }
    components
}
