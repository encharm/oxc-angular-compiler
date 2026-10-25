//! A TypeScript class's members, with the JSDoc written right before each.

use oxc_allocator::Allocator;
use oxc_ast::ast::{ClassElement, Comment, Decorator, Expression, MethodDefinitionKind, Statement};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};

use crate::schema::{ClassMember, MemberKind};

/// The members of the class `class_name` declared in `source`; none when the file has no such class.
pub fn class_members(source: &str, class_name: &str) -> Vec<ClassMember> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let comments = &parsed.program.comments;
    let Some(class) = parsed.program.body.iter().find_map(|statement| {
        let class = match statement {
            Statement::ClassDeclaration(c) => c,
            Statement::ExportDeclaration(e) => match &e.declaration {
                oxc_ast::ast::Declaration::ClassDeclaration(c) => c,
                _ => return None,
            },
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                oxc_ast::ast::ExportDefaultDeclarationKind::ClassDeclaration(c) => c,
                _ => return None,
            },
            _ => return None,
        };
        (class.id.as_ref().map(|id| id.name.as_str()) == Some(class_name)).then_some(class)
    }) else {
        return vec![];
    };

    let mut members = vec![];
    for element in &class.body.body {
        let (name, kind, is_static, decorators, initializer, span) = match element {
            ClassElement::MethodDefinition(m) => (
                m.key.static_name().and_then(|n| n.as_str().map(str::to_string)),
                match m.kind {
                    MethodDefinitionKind::Constructor => MemberKind::Constructor,
                    MethodDefinitionKind::Method => MemberKind::Method,
                    MethodDefinitionKind::Get => MemberKind::Getter,
                    MethodDefinitionKind::Set => MemberKind::Setter,
                },
                m.r#static,
                &m.decorators,
                None,
                m.span,
            ),
            ClassElement::PropertyDefinition(p) => (
                p.key.static_name().and_then(|n| n.as_str().map(str::to_string)),
                MemberKind::Field,
                p.r#static,
                &p.decorators,
                p.value.as_ref().and_then(|v| callee(v, source)),
                p.span,
            ),
            ClassElement::AccessorProperty(p) => (
                p.key.static_name().and_then(|n| n.as_str().map(str::to_string)),
                MemberKind::Field,
                p.r#static,
                &p.decorators,
                p.value.as_ref().and_then(|v| callee(v, source)),
                p.span,
            ),
            ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => continue,
        };
        let Some(name) = name else { continue };
        members.push(ClassMember {
            name,
            kind,
            is_static,
            doc: jsdoc_before(comments, source, span),
            initializer,
            decorators: decorators.iter().filter_map(|d| decorator_name(d, source)).collect(),
        });
    }
    members
}

/// The callee of a call that initializes a field: `input`, `input.required`, `computed`.
fn callee(value: &Expression<'_>, source: &str) -> Option<String> {
    match value.get_inner_expression() {
        Expression::CallExpression(call) => {
            Some(call.callee.span().source_text(source).to_string())
        }
        _ => None,
    }
}

fn decorator_name(decorator: &Decorator<'_>, source: &str) -> Option<String> {
    let expression = match &decorator.expression {
        Expression::CallExpression(call) => &call.callee,
        other => other,
    };
    Some(expression.span().source_text(source).to_string()).filter(|name| !name.is_empty())
}

/// The JSDoc that ends where only whitespace separates it from `span`, as text without its delimiters and the
/// leading `*` of each line.
fn jsdoc_before(comments: &[Comment], source: &str, span: Span) -> Option<String> {
    let comment = comments.iter().rev().find(|c| c.span.end <= span.start)?;
    let text = comment.span.source_text(source);
    let gap = source.get(comment.span.end as usize..span.start as usize)?;
    if !text.starts_with("/**") || text == "/**/" || !gap.trim().is_empty() {
        return None;
    }
    let inner = &text[3..text.len() - 2];
    let lines: Vec<&str> = inner
        .lines()
        .map(|line| {
            let line = line.trim();
            line.strip_prefix("* ").or_else(|| line.strip_prefix('*')).unwrap_or(line).trim_end()
        })
        .collect();
    let doc = lines.join("\n").trim().to_string();
    (!doc.is_empty()).then_some(doc)
}

#[cfg(test)]
mod tests {
    use super::class_members;
    use crate::schema::MemberKind;

    #[test]
    fn reads_members_with_their_jsdoc() {
        let source = r"
            import { Component, Input } from '@angular/core';
            export class Panel {
              /** Playback controls of the open video overlay. */
              get videoControls() { return 1; }
              /**
               * The open state.
               * Second line.
               */
              @Input() open = false;
              // not a JSDoc
              readonly size = input.required<number>();
              static make() {}
            }
        ";
        let members = class_members(source, "Panel");
        let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["videoControls", "open", "size", "make"]);
        assert!(matches!(members[0].kind, MemberKind::Getter));
        assert_eq!(members[0].doc.as_deref(), Some("Playback controls of the open video overlay."));
        assert_eq!(members[1].doc.as_deref(), Some("The open state.\nSecond line."));
        assert_eq!(members[1].decorators, vec!["Input"]);
        assert_eq!(members[2].doc, None);
        assert_eq!(members[2].initializer.as_deref(), Some("input.required"));
        assert!(members[3].is_static);
        assert!(class_members(source, "Missing").is_empty());
    }
}
