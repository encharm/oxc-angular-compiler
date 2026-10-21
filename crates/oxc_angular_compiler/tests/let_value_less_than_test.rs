//! A `@let` value can contain `<` (`@let small = count < 10;`): as in Angular's
//! lexer, only a `;` outside quotes ends it. `@angular/compiler-cli` 21.0.1
//! compiles the template below to `const small_r1 = ctx.count < 10;` and
//! `const both_r2 = ctx.a < ctx.b && ctx.b < ctx.c;`.

use oxc_allocator::Allocator;
use oxc_angular_compiler::{TransformOptions, transform_angular_file};

#[test]
fn let_value_with_less_than_compiles_as_a_comparison() {
    let source = r"
import { Component } from '@angular/core';
@Component({
  selector: 'b-cmp',
  template: `@let small = count < 10; @let both = a < b && b < c;
<span>{{ small }} {{ both }}</span>`,
})
export class BCmp { count = 1; a = 1; b = 2; c = 3; }
";
    let allocator = Allocator::default();
    let result = transform_angular_file(
        &allocator,
        "test.ts",
        source,
        Some(&TransformOptions::default()),
        None,
    );
    assert!(!result.has_errors(), "Should not have errors: {:?}", result.diagnostics);
    let code: String = result.code.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(code.contains("constsmall_r1=(ctx.count<10);"), "Got:\n{}", result.code);
    assert!(code.contains("constboth_r2=((ctx.a<ctx.b)&&(ctx.b<ctx.c));"), "Got:\n{}", result.code);
}
