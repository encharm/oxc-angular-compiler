//! `structure.d.ts`, the schema's TypeScript declarations, generated from `src/schema.rs`; the committed file must be
//! current (`UPDATE_DECLARATIONS=1 cargo test -p oxc_angular_structure --test declarations` rewrites it).

use oxc_angular_structure::schema::*;
use ts_rs::{Config, TS};

fn declarations() -> String {
    let cfg = Config::new();
    let decls = [
        ProjectStructure::decl(&cfg),
        ComponentStructure::decl(&cfg),
        CssSelector::decl(&cfg),
        ClassBinding::decl(&cfg),
        HostBinding::decl(&cfg),
        HostBindingKind::decl(&cfg),
        ClassMember::decl(&cfg),
        MemberKind::decl(&cfg),
        TemplateStructure::decl(&cfg),
        TemplateError::decl(&cfg),
        Position::decl(&cfg),
        Node::decl(&cfg),
        Element::decl(&cfg),
        Template::decl(&cfg),
        Attr::decl(&cfg),
        Input::decl(&cfg),
        BindingKind::decl(&cfg),
        Output::decl(&cfg),
        Reference::decl(&cfg),
        ReferenceTarget::decl(&cfg),
        TemplateAttr::decl(&cfg),
        Variable::decl(&cfg),
        Consumer::decl(&cfg),
        DirectiveMatch::decl(&cfg),
        IfBlock::decl(&cfg),
        IfBranch::decl(&cfg),
        ForBlock::decl(&cfg),
        SwitchBlock::decl(&cfg),
        SwitchGroup::decl(&cfg),
        DeferBlock::decl(&cfg),
        LetDeclaration::decl(&cfg),
        Text::decl(&cfg),
        BoundText::decl(&cfg),
        Content::decl(&cfg),
        UnknownBlock::decl(&cfg),
        Leaf::decl(&cfg),
        Expr::decl(&cfg),
        ExprKind::decl(&cfg),
        ObjectEntry::decl(&cfg),
        ExprTarget::decl(&cfg),
        serde_json::Value::decl(&cfg),
    ];
    let mut out = format!(
        "// Generated from crates/oxc_angular_structure/src/schema.rs by its tests/declarations.rs: do not edit.\n// Schema version {SCHEMA_VERSION}.\n\nexport type NodeId = number;\n"
    );
    for decl in decls {
        out.push_str("\nexport ");
        out.push_str(&decl);
        out.push('\n');
    }
    out
}

#[test]
fn declarations_are_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/structure.d.ts");
    let generated = declarations();
    if std::env::var_os("UPDATE_DECLARATIONS").is_some() {
        std::fs::write(path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == generated,
        "structure.d.ts is stale: UPDATE_DECLARATIONS=1 cargo test -p oxc_angular_structure --test declarations"
    );
}
