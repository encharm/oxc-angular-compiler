//! Implicit `@defer` dependencies (#313): a standalone component or directive
//! listed in `@Component.imports` and matched only inside `@defer` blocks is
//! lazy-loaded through a dynamic `import()`, the way ngc's full compilation
//! does, when cross-file analysis can read its selector
//! (`TransformOptions::resolve_imported_values`).

#![cfg(feature = "cross_file_elision")]

use oxc_allocator::Allocator;
use oxc_angular_compiler::{TransformOptions, TransformResult, transform_angular_file};
use tempfile::TempDir;

fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
    path
}

fn options() -> TransformOptions {
    TransformOptions { resolve_imported_values: true, ..Default::default() }
}

fn transform_with(dir: &TempDir, source: &str, options: &TransformOptions) -> TransformResult {
    let path = create_test_file(dir.path(), "app/parent.component.ts", source);
    let allocator = Allocator::default();
    let result =
        transform_angular_file(&allocator, path.to_str().unwrap(), source, Some(options), None);
    assert!(!result.has_errors(), "Should not have errors: {:?}", result.diagnostics);
    result
}

fn transform(dir: &TempDir, source: &str) -> TransformResult {
    transform_with(dir, source, &options())
}

fn compact(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

fn lazy_cmp(dir: &TempDir) {
    create_test_file(
        dir.path(),
        "app/lazy.ts",
        r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-lazy', template: 'lazy' })
export class LazyCmp {}
"#,
    );
}

fn assert_lazy(code: &str, module: &str, member: &str) {
    let compact = compact(code);
    assert!(
        compact.contains(&format!("import(\"{module}\").then((m)=>m.{member})"))
            || compact.contains(&format!("import(\"{module}\").then(m=>m.{member})")),
        "Expected `import('{module}').then(m => m.{member})`. Output:\n{code}"
    );
}

fn assert_eager(code: &str, module: &str) {
    assert!(
        !code.contains(&format!("import(\"{module}\")")),
        "`{module}` must stay an eager dependency. Output:\n{code}"
    );
}

#[test]
fn component_used_only_inside_defer_is_lazy_loaded() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    let compact = compact(code);

    assert_lazy(code, "./lazy", "LazyCmp");
    assert_eq!(
        defer_blocks(code),
        [block("Parent_Defer_1_DepsFn", &["./lazy"])],
        "The block's resolver must be wired into `ɵɵdefer`. Output:\n{code}"
    );
    assert!(
        compact.contains("ɵɵgetComponentDepsFactory(Parent,[])"),
        "The lazy component must leave the eager dependencies. Output:\n{code}"
    );
    assert!(
        !code.contains("import { LazyCmp }"),
        "The static import of the lazy component must be removed. Output:\n{code}"
    );
    assert!(code.contains("setClassMetadataAsync"), "Output:\n{code}");
}

#[test]
fn component_also_used_outside_defer_stays_eager() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '<app-lazy/> @defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    assert_eager(code, "./lazy");
    assert!(compact(code).contains("ɵɵgetComponentDepsFactory(Parent,[LazyCmp])"), "{code}");
    assert!(code.contains("import { LazyCmp }"), "{code}");
}

#[test]
fn component_used_in_a_placeholder_stays_eager() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> } @placeholder { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    assert_eager(&result.code, "./lazy");
}

#[test]
fn component_used_in_nested_control_flow_inside_defer_is_lazy_loaded() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { @switch (mode) { @case (1) { <div><app-lazy/></div> } } }',
})
export class Parent { mode = 1; }
"#,
    );
    assert_lazy(&result.code, "./lazy", "LazyCmp");
}

#[test]
fn aliased_import_reads_the_exported_name() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp as Lazy } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [Lazy],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    assert_lazy(&result.code, "./lazy", "LazyCmp");
}

#[test]
fn default_import_reads_default() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/lazy.ts",
        r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-lazy', template: 'lazy' })
export default class LazyCmp {}
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import LazyCmp from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    assert_lazy(&result.code, "./lazy", "default");
}

#[test]
fn import_referenced_elsewhere_in_the_file_stays_eager() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

export const EXPOSED = LazyCmp;

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    assert_eager(&result.code, "./lazy");
}

#[test]
fn attribute_directive_used_only_inside_defer_is_lazy_loaded() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/tooltip.ts",
        r#"
import { Directive } from '@angular/core';

@Directive({ selector: '[appTooltip]' })
export class Tooltip {}
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { Tooltip } from './tooltip';

@Component({
    selector: 'app-parent',
    imports: [Tooltip],
    template: '<button>eager</button> @defer { <button appTooltip>lazy</button> }',
})
export class Parent {}
"#,
    );
    assert_lazy(&result.code, "./tooltip", "Tooltip");
}

#[test]
fn bound_input_matches_an_attribute_selector() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/tooltip.ts",
        r#"
import { Directive, input } from '@angular/core';

@Directive({ selector: '[appTooltip]' })
export class Tooltip { appTooltip = input(''); }
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { Tooltip } from './tooltip';

@Component({
    selector: 'app-parent',
    imports: [Tooltip],
    template: '<button [appTooltip]="text">eager</button> @defer { <span appTooltip></span> }',
})
export class Parent { text = ''; }
"#,
    );
    assert_eager(&result.code, "./tooltip");
}

#[test]
fn not_standalone_component_stays_eager() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/lazy.ts",
        r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-lazy', template: 'lazy', standalone: false })
export class LazyCmp {}
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    assert_eager(&result.code, "./lazy");
}

#[test]
fn pipe_stays_eager() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/shout.ts",
        r#"
import { Pipe } from '@angular/core';

@Pipe({ name: 'shout' })
export class ShoutPipe { transform(v: string) { return v; } }
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { ShoutPipe } from './shout';

@Component({
    selector: 'app-parent',
    imports: [ShoutPipe],
    template: '@defer { {{ name | shout }} }',
})
export class Parent { name = ''; }
"#,
    );
    assert_eager(&result.code, "./shout");
}

#[test]
fn implicit_and_explicit_deferred_imports_share_the_resolver() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';
import { Other } from './other';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    deferredImports: [Other],
    template: '@defer { <app-lazy/> <app-other/> }',
})
export class Parent {}
"#,
    );
    assert_lazy(&result.code, "./lazy", "LazyCmp");
    assert_lazy(&result.code, "./other", "Other");
}

// --- One resolver per `@defer` block, as in ngc's PerBlock emit ---

/// What each `ɵɵdefer(...)` in `code` loads, in output order: the name of the
/// constant holding its resolver (empty for an inline resolver or none) and
/// the modules the resolver imports.
fn defer_blocks(code: &str) -> Vec<(String, Vec<String>)> {
    fn bracketed(s: &str) -> &str {
        let mut depth = 0;
        for (index, c) in s.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return &s[..=index];
                    }
                }
                _ => {}
            }
        }
        panic!("No closing `]` in {s}");
    }

    let compact = compact(code);
    let mut blocks = Vec::new();
    for (index, call) in compact.match_indices("ɵɵdefer(") {
        // `ɵɵdefer(slot, mainSlot, resolver, ...)`
        let args = &compact[index + call.len()..];
        let resolver = &args[args.find(|c: char| !c.is_ascii_digit() && c != ',').unwrap()..];
        let (name, body) = if resolver.starts_with("()=>[") {
            (String::new(), bracketed(resolver))
        } else if resolver.starts_with(')') || resolver.starts_with("null") {
            (String::new(), "")
        } else {
            let name = &resolver[..resolver.find([',', ')']).unwrap()];
            let declaration = format!("const{name}=");
            let start = compact
                .find(&declaration)
                .unwrap_or_else(|| panic!("No `const {name}` in the output:\n{code}"));
            (name.to_string(), bracketed(&compact[start + declaration.len()..]))
        };
        let modules = body
            .match_indices("import(\"")
            .map(|(index, import)| {
                let module = &body[index + import.len()..];
                module[..module.find('"').unwrap()].to_string()
            })
            .collect();
        blocks.push((name, modules));
    }
    blocks
}

fn block(name: &str, modules: &[&str]) -> (String, Vec<String>) {
    (name.to_string(), modules.iter().map(ToString::to_string).collect())
}

/// `app/lazy.ts` with `app-lazy` and `app/other.ts` with `app-other`.
fn lazy_and_other_cmps(dir: &TempDir) {
    lazy_cmp(dir);
    create_test_file(
        dir.path(),
        "app/other.ts",
        r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-other', template: 'other' })
export class OtherCmp {}
"#,
    );
}

/// Compiles a parent component that imports `LazyCmp` and `OtherCmp` and
/// uses `template`.
fn lazy_and_other_transform(dir: &TempDir, template: &str) -> TransformResult {
    lazy_and_other_cmps(dir);
    transform(
        dir,
        &format!(
            r#"
import {{ Component }} from '@angular/core';
import {{ LazyCmp }} from './lazy';
import {{ OtherCmp }} from './other';

@Component({{
    selector: 'app-parent',
    imports: [LazyCmp, OtherCmp],
    template: `{template}`,
}})
export class Parent {{ show = true; }}
"#
        ),
    )
}

#[test]
fn each_block_loads_only_the_imports_it_uses() {
    let dir = TempDir::new().unwrap();
    let result =
        lazy_and_other_transform(&dir, "@defer { <app-lazy/> } @defer (on idle) { <app-other/> }");
    let code = &result.code;
    assert_eq!(
        defer_blocks(code),
        [block("Parent_Defer_1_DepsFn", &["./lazy"]), block("Parent_Defer_4_DepsFn", &["./other"])],
        "Output:\n{code}"
    );
    assert!(compact(code).contains("ɵɵgetComponentDepsFactory(Parent,[])"), "Output:\n{code}");
    // `setClassMetadataAsync` loads every deferred import
    assert!(
        compact(code).contains(
            "ɵsetClassMetadataAsync(Parent,()=>[import(\"./lazy\").then((m)=>m.LazyCmp),import(\"./other\").then((m)=>m.OtherCmp)]"
        ),
        "Output:\n{code}"
    );
}

#[test]
fn nested_block_loads_its_own_imports() {
    let dir = TempDir::new().unwrap();
    let result = lazy_and_other_transform(&dir, "@defer { <app-lazy/> @defer { <app-other/> } }");
    let code = &result.code;
    let mut blocks = defer_blocks(code);
    blocks.sort();
    assert_eq!(
        blocks,
        [
            block("Parent_Defer_0_Defer_2_DepsFn", &["./other"]),
            block("Parent_Defer_1_DepsFn", &["./lazy"]),
        ],
        "Output:\n{code}"
    );
}

#[test]
fn nested_placeholder_loads_with_the_outer_block() {
    let dir = TempDir::new().unwrap();
    let result = lazy_and_other_transform(
        &dir,
        "@defer { @defer { <app-other/> } @placeholder { <app-lazy/> } }",
    );
    let code = &result.code;
    let mut blocks = defer_blocks(code);
    blocks.sort();
    assert_eq!(
        blocks,
        [
            block("Parent_Defer_0_Defer_2_DepsFn", &["./other"]),
            block("Parent_Defer_1_DepsFn", &["./lazy"]),
        ],
        "Output:\n{code}"
    );
}

#[test]
fn blocks_using_the_same_imports_share_one_function() {
    let dir = TempDir::new().unwrap();
    let result =
        lazy_and_other_transform(&dir, "@defer { <app-lazy/> } @defer (on idle) { <app-lazy/> }");
    let code = &result.code;
    assert_eq!(
        defer_blocks(code),
        [block("Parent_Defer_1_DepsFn", &["./lazy"]), block("Parent_Defer_1_DepsFn", &["./lazy"])],
        "Output:\n{code}"
    );
    assert_eq!(code.matches("_DepsFn = ").count(), 1, "Output:\n{code}");
}

#[test]
fn block_without_lazy_imports_has_no_resolver() {
    let dir = TempDir::new().unwrap();
    let result =
        lazy_and_other_transform(&dir, "@defer { <app-lazy/> } @defer (on idle) { <p>text</p> }");
    let code = &result.code;
    assert_eq!(
        defer_blocks(code),
        [block("Parent_Defer_1_DepsFn", &["./lazy"]), block("", &[])],
        "Output:\n{code}"
    );
}

#[test]
fn block_inside_control_flow_has_its_own_function() {
    let dir = TempDir::new().unwrap();
    let result = lazy_and_other_transform(
        &dir,
        "@if (show) { @defer { <app-lazy/> } } @defer { <app-other/> }",
    );
    let code = &result.code;
    let mut blocks = defer_blocks(code);
    blocks.sort();
    assert_eq!(
        blocks,
        [
            block("Parent_Conditional_0_Defer_1_DepsFn", &["./lazy"]),
            block("Parent_Defer_2_DepsFn", &["./other"]),
        ],
        "Output:\n{code}"
    );
}

#[test]
fn explicit_deferred_imports_share_one_resolver_across_blocks() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';
import { Other } from './other';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    deferredImports: [Other],
    template: '@defer { <app-lazy/> } @defer (on idle) { <app-other/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    assert_eq!(
        defer_blocks(code),
        [block("", &["./other", "./lazy"]), block("", &["./other", "./lazy"])],
        "Output:\n{code}"
    );
}

#[test]
fn hmr_keeps_imports_eager() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform_with(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
        &TransformOptions { hmr: true, ..options() },
    );
    assert_eager(&result.code, "./lazy");
}

// ---------------------------------------------------------------------------
// Selector matching (`pipeline/selector.rs`, `component/defer_resolver.rs`):
// elements are matched the way ngc's `createCssSelectorFromNode` and
// `SelectorMatcher` match them.
// ---------------------------------------------------------------------------

/// Writes `app/dep.ts`, a standalone directive `Dep` with `selector`.
fn selector_matching_dep(dir: &TempDir, selector: &str) {
    create_test_file(
        dir.path(),
        "app/dep.ts",
        &format!(
            r#"
import {{ Directive }} from '@angular/core';

@Directive({{ selector: '{selector}' }})
export class Dep {{}}
"#
        ),
    );
}

/// Compiles a parent component that imports `Dep` and uses `template`.
fn selector_matching_transform(dir: &TempDir, template: &str) -> TransformResult {
    transform(
        dir,
        &format!(
            r#"
import {{ Component }} from '@angular/core';
import {{ Dep }} from './dep';

@Component({{
    selector: 'app-parent',
    imports: [Dep],
    template: `{template}`,
}})
export class Parent {{ x = ''; }}
"#
        ),
    )
}

#[test]
fn selector_matching_svg_element_used_eagerly_stays_eager() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "svg[appIcon], i[appIcon]");
    let result =
        selector_matching_transform(&dir, "<svg appIcon></svg> @defer { <i appIcon></i> }");
    assert_eager(&result.code, "./dep");
}

#[test]
fn selector_matching_element_inside_svg_used_eagerly_stays_eager() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "a[appLink]");
    let result =
        selector_matching_transform(&dir, "<svg><a appLink></a></svg> @defer { <a appLink></a> }");
    assert_eager(&result.code, "./dep");
}

#[test]
fn selector_matching_element_inside_svg_used_only_inside_defer_is_lazy_loaded() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "g[appDep]");
    let result = selector_matching_transform(&dir, "@defer { <svg><g appDep></g></svg> }");
    assert_lazy(&result.code, "./dep", "Dep");
}

#[test]
fn selector_matching_namespaced_attribute_used_eagerly_stays_eager() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "[href]");
    let result = selector_matching_transform(
        &dir,
        r#"<svg><a xlink:href="x"></a></svg> @defer { <a href="x"></a> }"#,
    );
    assert_eager(&result.code, "./dep");
}

#[test]
fn selector_matching_only_i18n_attributes_are_skipped() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "[i18nTip]");
    let result =
        selector_matching_transform(&dir, r#"<b i18nTip></b> @defer { <b [i18nTip]="x"></b> }"#);
    assert_eager(&result.code, "./dep");
}

#[test]
fn selector_matching_class_attribute_name_is_case_insensitive() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, ".foo");
    let result =
        selector_matching_transform(&dir, r#"<b CLASS="foo"></b> @defer { <b class="foo"></b> }"#);
    assert_eager(&result.code, "./dep");
}

#[test]
fn selector_matching_not_only_selector_matches_any_element() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, ":not(.plain)");
    let result = selector_matching_transform(&dir, "@defer { <span></span> }");
    assert_lazy(&result.code, "./dep", "Dep");

    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "[appDep], :not([appDep]):not(span)");
    let result = selector_matching_transform(&dir, "<div></div> @defer { <span appDep></span> }");
    assert_eager(&result.code, "./dep");
}

#[test]
fn selector_matching_empty_selector_in_a_list_matches_nothing() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "[appDep],");
    let result = selector_matching_transform(&dir, "<div></div> @defer { <span appDep></span> }");
    assert_lazy(&result.code, "./dep", "Dep");
}

#[test]
fn selector_matching_binding_overrides_static_attribute_value() {
    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, "[type=a]");
    let result = selector_matching_transform(
        &dir,
        r#"<input type="a" [type]="x"/> @defer { <input type="a"/> }"#,
    );
    assert_lazy(&result.code, "./dep", "Dep");

    let dir = TempDir::new().unwrap();
    selector_matching_dep(&dir, ".foo");
    let result = selector_matching_transform(
        &dir,
        r#"<div class="foo" [class]="x"></div> @defer { <div class="foo"></div> }"#,
    );
    assert_lazy(&result.code, "./dep", "Dep");
}

#[test]
fn selector_matching_placeholder_of_a_nested_defer_is_lazy_loaded() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { @defer { <p>x</p> } @placeholder { <app-lazy/> } }',
})
export class Parent {}
"#,
    );
    assert_lazy(&result.code, "./lazy", "LazyCmp");
}

// --- Import removal: names other emitted code needs, and ngc's
// per-declaration `canDefer` rule ---

/// `app/cmps.ts` with two standalone components, `app-lazy` and `app-eager`,
/// a `TOKEN` value and a `CmpOptions` interface.
fn two_cmps(dir: &TempDir) {
    create_test_file(
        dir.path(),
        "app/cmps.ts",
        r#"
import { Component, InjectionToken } from '@angular/core';

@Component({ selector: 'app-lazy', template: 'lazy' })
export class LazyCmp {}

@Component({ selector: 'app-eager', template: 'eager' })
export class EagerCmp {}

export const TOKEN = new InjectionToken<string>('token');

export interface CmpOptions {}
"#,
    );
}

/// A parent deferring `LazyCmp` next to a `jit: true` class that injects it.
fn jit_sibling_source(jit_first: bool) -> String {
    let jit = r#"
@Component({ selector: 'app-jit', template: '', jit: true })
export class JitCmp {
    constructor(lazy: LazyCmp) {}
}
"#;
    let parent = r#"
@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#;
    let (first, second) = if jit_first { (jit, parent) } else { (parent, jit) };
    format!(
        "import {{ Component }} from '@angular/core';\nimport {{ LazyCmp }} from './lazy';\n{first}{second}"
    )
}

#[test]
fn jit_sibling_ctor_parameter_keeps_import_eager_when_declared_first() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(&dir, &jit_sibling_source(true));
    let code = &result.code;
    assert!(compact(code).contains("{type:LazyCmp}"), "{code}");
    assert!(code.contains("import { LazyCmp }"), "{code}");
    assert_eager(code, "./lazy");
}

#[test]
fn jit_sibling_ctor_parameter_keeps_import_eager_when_declared_last() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(&dir, &jit_sibling_source(false));
    let code = &result.code;
    assert!(compact(code).contains("{type:LazyCmp}"), "{code}");
    assert!(code.contains("import { LazyCmp }"), "{code}");
    assert_eager(code, "./lazy");
}

#[test]
fn default_import_in_sibling_ctor_metadata_stays_eager() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/lazy.ts",
        r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-lazy', template: 'lazy' })
export default class LazyCmp {}
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component, Injectable } from '@angular/core';
import LazyCmp from './lazy';

@Injectable()
export class Svc {
    constructor(lazy: LazyCmp) {}
}

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    assert!(compact(code).contains("{type:LazyCmp}"), "{code}");
    assert!(code.contains("import LazyCmp from"), "{code}");
    assert_eager(code, "./lazy");
}

#[test]
fn deferred_component_injected_by_a_sibling_uses_a_namespace_import() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component, Injectable } from '@angular/core';
import { LazyCmp } from './lazy';

@Injectable()
export class Svc {
    constructor(lazy: LazyCmp) {}
}

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    // Like ngc: the token goes through a namespace import, so the removed
    // named import leaves no dangling reference.
    assert_lazy(code, "./lazy", "LazyCmp");
    assert!(!code.contains("import { LazyCmp }"), "{code}");
    assert!(code.contains("import * as i1 from './lazy'"), "{code}");
    assert!(compact(code).contains("ɵɵinject(i1.LazyCmp)"), "{code}");
}

#[test]
fn sibling_specifier_used_eagerly_keeps_the_declaration_eager() {
    let dir = TempDir::new().unwrap();
    two_cmps(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp, EagerCmp } from './cmps';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp, EagerCmp],
    template: '<app-eager/> @defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    assert_eager(code, "./cmps");
    assert!(code.contains("import { LazyCmp, EagerCmp }"), "{code}");
    assert!(
        compact(code).contains("ɵɵgetComponentDepsFactory(Parent,[LazyCmp,EagerCmp])"),
        "{code}"
    );
}

#[test]
fn sibling_value_specifier_keeps_the_declaration_eager() {
    let dir = TempDir::new().unwrap();
    two_cmps(&dir);
    let result = transform(
        &dir,
        r#"
import { Component, inject } from '@angular/core';
import { LazyCmp, TOKEN } from './cmps';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {
    token = inject(TOKEN);
}
"#,
    );
    let code = &result.code;
    assert_eager(code, "./cmps");
    assert!(code.contains("import { LazyCmp, TOKEN }"), "{code}");
}

#[test]
fn inline_type_specifier_does_not_block_deferral() {
    let dir = TempDir::new().unwrap();
    two_cmps(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { type EagerCmp, LazyCmp } from './cmps';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {
    other?: EagerCmp;
}
"#,
    );
    let code = &result.code;
    assert_lazy(code, "./cmps", "LazyCmp");
    assert!(!code.contains("from \"./cmps\""), "{code}");
}

#[test]
fn all_specifiers_deferred_removes_the_declaration() {
    let dir = TempDir::new().unwrap();
    two_cmps(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp, EagerCmp } from './cmps';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp, EagerCmp],
    template: '@defer { <app-lazy/> <app-eager/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    assert_lazy(code, "./cmps", "LazyCmp");
    assert_lazy(code, "./cmps", "EagerCmp");
    assert!(compact(code).contains("ɵɵgetComponentDepsFactory(Parent,[])"), "{code}");
    assert!(!code.contains("from \"./cmps\""), "{code}");
}

#[test]
fn sibling_specifier_in_deferred_imports_keeps_the_declaration_eager() {
    // The explicit entry's static import stays, so the declaration does too.
    let dir = TempDir::new().unwrap();
    two_cmps(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp, EagerCmp } from './cmps';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    deferredImports: [EagerCmp],
    template: '@defer { <app-lazy/> <app-eager/> }',
})
export class Parent {}
"#,
    );
    let code = &result.code;
    assert!(code.contains("import { LazyCmp, EagerCmp }"), "{code}");
    assert!(compact(code).contains("ɵɵgetComponentDepsFactory(Parent,[LazyCmp])"), "{code}");
    assert!(!code.contains("m.LazyCmp"), "{code}");
}

#[test]
fn sibling_interface_without_type_keeps_the_declaration_eager() {
    // Like ngc, which can't tell an interface from a value here (`AssumeEager`).
    let dir = TempDir::new().unwrap();
    two_cmps(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp, CmpOptions } from './cmps';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {
    options?: CmpOptions;
}
"#,
    );
    let code = &result.code;
    assert_eager(code, "./cmps");
    assert!(compact(code).contains("ɵɵgetComponentDepsFactory(Parent,[LazyCmp])"), "{code}");
}

#[test]
fn computed_type_key_keeps_import_eager() {
    let dir = TempDir::new().unwrap();
    lazy_cmp(&dir);
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {
    keyed?: { [LazyCmp]: string };
}
"#,
    );
    let code = &result.code;
    assert!(code.contains("import { LazyCmp }"), "{code}");
    assert_eager(code, "./lazy");
}

// ---------------------------------------------------------------------------
// Cross-file selector lookup (`cross_file_elision.rs`)
// ---------------------------------------------------------------------------

#[test]
fn namespace_member_does_not_select_an_unexported_class() {
    // `NS.LazyCmp` is recorded as the barrel's `LazyCmp` export, but the
    // top-level `LazyCmp` class is not exported as `LazyCmp` (at most under
    // another name). The `LazyCmp` the barrel exports comes from `./real`
    // (`app-real`), which is used eagerly.
    for export_list in ["", "export { LazyCmp as Alias };"] {
        let dir = TempDir::new().unwrap();
        create_test_file(
            dir.path(),
            "app/real.ts",
            r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-real', template: 'real' })
export class LazyCmp {}
"#,
        );
        create_test_file(
            dir.path(),
            "app/barrel.ts",
            &format!(
                r#"
import {{ Component }} from '@angular/core';

export * from './real';

namespace NS {{
    export class LazyCmp {{}}
}}

@Component({{ selector: 'app-lazy', template: 'lazy' }})
class LazyCmp {{}}

{export_list}
"#
            ),
        );
        let result = transform(
            &dir,
            r#"
import { Component } from '@angular/core';
import { LazyCmp } from './barrel';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '<app-real/> @defer { <app-lazy/> }',
})
export class Parent {}
"#,
        );
        assert_eager(&result.code, "./barrel");
    }
}

#[test]
fn class_exported_by_a_local_export_list_is_lazy_loaded() {
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/lazy.ts",
        r#"
import { Component } from '@angular/core';

@Component({ selector: 'app-lazy', template: 'lazy' })
class Impl {}

export { Impl as LazyCmp };
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '@defer { <app-lazy/> }',
})
export class Parent {}
"#,
    );
    assert_lazy(&result.code, "./lazy", "LazyCmp");
}

#[test]
fn computed_selector_key_stays_eager() {
    // The computed `[S]` key overrides `selector` with `app-b`, which is
    // used eagerly.
    let dir = TempDir::new().unwrap();
    create_test_file(
        dir.path(),
        "app/lazy.ts",
        r#"
import { Component } from '@angular/core';

const S = 'selector';

@Component({ selector: 'app-a', [S]: 'app-b', template: 'lazy' })
export class LazyCmp {}
"#,
    );
    let result = transform(
        &dir,
        r#"
import { Component } from '@angular/core';
import { LazyCmp } from './lazy';

@Component({
    selector: 'app-parent',
    imports: [LazyCmp],
    template: '<app-b/> @defer { <app-a/> }',
})
export class Parent {}
"#,
    );
    assert_eager(&result.code, "./lazy");
}
