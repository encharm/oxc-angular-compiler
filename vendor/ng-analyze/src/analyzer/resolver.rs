use crate::{ClassInfo, DeclarationTuple, ResourceResolverFs};
use futures::future::{BoxFuture, FutureExt};
use oxc_resolver::ResolverGeneric;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Try to resolve a specifier to a .d.ts file.
/// Handles cases where imports reference .js files but actual files are .d.ts.
pub fn resolve_to_dts<Fs: ResourceResolverFs>(
    resolver: &ResolverGeneric<Fs>,
    file_path: &Path,
    source: &str,
) -> Option<PathBuf> {
    resolver
        .resolve_dts(file_path, source)
        .ok()
        .map(|res| res.into_path_buf())
}

/// Asynchronously resolves a component, directive, or pipe declaration from its import specifier.
///
/// This is the main entry point for cross-file symbol resolution. It resolves the import path, ensures
/// the target file has been parsed (running lazy Stage 1 analysis for TS files or definition loading for DTS
/// files), and traces the symbol through re-exports and wildcard chains recursively to find its origin.
pub async fn resolve_declaration_async<Fs: ResourceResolverFs + Clone + 'static>(
    decl: &DeclarationTuple,
    module_file: &Path,
    ctx: &crate::QueryCtx<Fs>,
) -> Option<ClassInfo> {
    let mut visited = HashSet::new();
    let Ok(Some(resolved)) = crate::evaluator::cross_file::cross_file_resolve(
        ctx,
        module_file,
        decl.import_source.as_deref(),
        decl.export_name().to_owned(),
        &mut visited,
    )
    .await
    else {
        return None;
    };
    let decl_file_id = ctx.engine.intern_path(&resolved.file_path);
    let evaluated = ctx.analyze_file_evaluated(decl_file_id).await;
    let parsed = ctx.parse_file(decl_file_id).await;
    let guard = parsed.lock().unwrap();
    let symbol_name = guard
        .borrow_dependent()
        .semantic
        .scoping()
        .symbol_name(resolved.symbol_id);
    evaluated.class_index.get(symbol_name).cloned()
}

fn field_key(field: &crate::AngularFieldMetadata) -> Option<(&str, &str)> {
    let name = match field.kind.as_str() {
        "input" => field.input.as_ref().map(|i| i.name.as_str()),
        "output" => field.output.as_ref().map(|o| o.name.as_str()),
        "query" => field.query.as_ref().map(|q| q.property_name.as_str()),
        "coercion" => field.coercion.as_deref(),
        _ => None,
    }?;
    Some((field.kind.as_str(), name))
}

fn field_matches(a: &crate::AngularFieldMetadata, b: &crate::AngularFieldMetadata) -> bool {
    match (field_key(a), field_key(b)) {
        (Some(ka), Some(kb)) => ka == kb,
        _ => false,
    }
}

// ngcheck: merges as ngtsc's `flattenInheritedDirectiveMetadata` does, the base class first: `addMetadata` recurses
// into the base before merging the class's own metadata, and `ClassPropertyMapping.merge` keeps an inherited field's
// position while the derived class's definition replaces it. The type-check blocks list inputs in that order.
fn merge_derived_class_info(merged: &mut ClassInfo, derived: &ClassInfo) {
    if let Some(derived_fields) = &derived.fields {
        let fields = merged.fields.get_or_insert_with(Vec::new);
        for df in derived_fields {
            match fields.iter_mut().find(|f| field_matches(df, f)) {
                Some(field) => *field = df.clone(),
                None => fields.push(df.clone()),
            }
        }
    }
    merged.is_structural = merged.is_structural || derived.is_structural;

    for name in &derived.public_methods {
        if !merged.public_methods.contains(name) {
            merged.public_methods.push(name.clone());
        }
    }

    if let Some(derived_host_dirs) = &derived.host_directives {
        let host_dirs = merged.host_directives.get_or_insert_with(Vec::new);
        for dhd in derived_host_dirs {
            if !host_dirs.iter().any(|hd| hd.directive == dhd.directive) {
                host_dirs.push(dhd.clone());
            }
        }
    }
}

pub fn flatten_class_info<'a, Fs: ResourceResolverFs + Clone + 'static>(
    mut info: ClassInfo,
    ctx: &'a crate::QueryCtx<Fs>,
) -> BoxFuture<'a, ClassInfo> {
    async move {
        let mut current_super = info.super_class.clone();
        let mut current_file = info.file_path.clone();
        let mut visited = HashSet::new();
        visited.insert(info.reference_id);

        // the base classes, nearest first
        let mut bases = vec![];
        while let Some(super_decl) = current_super {
            let resolved = resolve_declaration_async(&super_decl, &current_file, ctx).await;
            let Some(super_info) = resolved else {
                break;
            };

            if !visited.insert(super_info.reference_id) {
                break;
            }

            current_super = super_info.super_class.clone();
            current_file = super_info.file_path.clone();
            bases.push(super_info);
        }

        // ngcheck: fold from the base-most class down, as ngtsc does (`merge_derived_class_info`)
        let Some(mut merged) = bases.pop() else { return info };
        for base in bases.iter().rev() {
            merge_derived_class_info(&mut merged, base);
        }
        merge_derived_class_info(&mut merged, &info);
        info.fields = merged.fields;
        info.is_structural = merged.is_structural;
        info.public_methods = merged.public_methods;
        info.host_directives = merged.host_directives;
        info
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AngularFieldMetadata, InputMetadata, OutputMetadata};

    fn make_input(name: &str) -> AngularFieldMetadata {
        AngularFieldMetadata {
            kind: "input".to_string(),
            input: Some(InputMetadata {
                name: name.to_string(),
                alias: None,
                required: false,
                is_signal: true,
                decorator_span: None,
                property_span: None,
                transform: None,
                is_restricted: false,
                is_literal: false,
                is_coerced: false,
            }),
            output: None,
            query: None,
            coercion: None,
        }
    }

    fn make_output(name: &str) -> AngularFieldMetadata {
        AngularFieldMetadata {
            kind: "output".to_string(),
            input: None,
            output: Some(OutputMetadata {
                name: name.to_string(),
                alias: Some(format!("{name}Change")),
                decorator_span: None,
                property_span: None,
                is_signal: true,
            }),
            query: None,
            coercion: None,
        }
    }

    #[test]
    fn test_field_matches_distinguishes_input_and_output() {
        let input_val = make_input("value");
        let output_val = make_output("value");

        assert!(!field_matches(&input_val, &output_val));
        assert!(field_matches(&input_val, &input_val));
        assert!(field_matches(&output_val, &output_val));
    }

    #[test]
    fn test_field_matches_same_kind_different_name() {
        let input_a = make_input("a");
        let input_b = make_input("b");

        assert!(!field_matches(&input_a, &input_b));
        assert!(field_matches(&input_a, &input_a));
    }
}
