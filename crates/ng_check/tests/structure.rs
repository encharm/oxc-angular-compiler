//! `ngcheck --structure` over `tests/structure/`, a project of its own (`angular/` declares the part of Angular it
//! uses): what binding adds to the tree, asserted, and the whole output as a snapshot.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn structure() -> Value {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/structure");
    let output = Command::new(env!("CARGO_BIN_EXE_ngcheck"))
        .args(["-p", "tsconfig.json", "--structure", "-"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let root = std::fs::canonicalize(&dir).unwrap();
    let text = String::from_utf8(output.stdout)
        .unwrap()
        .replace(&root.to_string_lossy().into_owned(), "<fixture>");
    serde_json::from_str(&text).unwrap()
}

/// Every node of a tree, depth first, with the branches of `@if` blocks.
fn nodes<'v>(list: &'v Value, out: &mut Vec<&'v Value>) {
    for node in list.as_array().into_iter().flatten() {
        out.push(node);
        for key in ["children", "empty"] {
            nodes(&node[key], out);
        }
        for branch in node["branches"].as_array().into_iter().flatten() {
            out.push(branch);
            nodes(&branch["children"], out);
        }
    }
}

#[test]
fn binding_resolves_matches_targets_and_consumers() {
    let structure = structure();
    let panel = structure["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["className"] == "Panel")
        .unwrap();
    let mut all = vec![];
    nodes(&panel["template"]["nodes"], &mut all);
    let by_id = |id: &Value| all.iter().find(|n| n["id"] == *id).copied().unwrap();
    let element = |name: &str| {
        all.iter().find(|n| n["type"] == "element" && n["name"] == name).copied().unwrap()
    };
    let names = |matches: &Value| {
        matches
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };

    // a component by its selector or its alias, a directive by its attribute selector
    let child = element("x-child");
    assert_eq!(names(&child["matches"]), ["Child", "Hint"]);
    assert_eq!(names(&element("x-kid")["matches"]), ["Child"]);
    // a binding names the input it sets
    let label = child["inputs"].as_array().unwrap().iter().find(|i| i["name"] == "label").unwrap();
    assert_eq!(label["consumer"], serde_json::json!({ "directive": "Child", "property": "label" }));
    let hint =
        child["attrs"].as_array().unwrap().iter().find(|a| a["name"] == "data-hint").unwrap();
    assert_eq!(hint["consumer"], serde_json::json!({ "directive": "Hint", "property": "name" }));
    // `@if (…; as vc)`: the alias is the branch's variable
    assert_eq!(label["expr"]["target"]["kind"], "variable");
    assert!(by_id(&label["expr"]["target"]["node"])["condition"].is_object());

    // an outlet renders the template its expression references
    let outlet = all
        .iter()
        .find(|n| {
            n["templateAttrs"]
                .as_array()
                .is_some_and(|a| a.iter().any(|t| t["name"] == "ngTemplateOutlet"))
        })
        .unwrap();
    let rendered = &outlet["templateAttrs"][0]["expr"]["target"];
    assert_eq!(rendered["kind"], "reference");
    assert_eq!(by_id(&rendered["node"])["refs"][0]["name"], "menu");
    // `*ngIf="…; else closed"`: the else is NgIf's input, its value the `#closed` template
    let ng_if = all
        .iter()
        .find(|n| n["matches"].as_array().is_some_and(|m| m.iter().any(|d| d["name"] == "NgIf")))
        .unwrap();
    let else_attr = ng_if["templateAttrs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "ngIfElse")
        .unwrap();
    assert_eq!(else_attr["consumer"]["property"], "ngIfElse");
    assert_eq!(by_id(&else_attr["expr"]["target"]["node"])["refs"][0]["name"], "closed");

    // `@for`'s `i` and a `#note` reference
    let b = element("b");
    assert_eq!(by_id(&b["outputs"][0]["handler"]["args"][0]["target"]["node"])["type"], "for");
    let note = &b["children"][0]["expr"]["exprs"][0]["receiver"]["target"];
    assert_eq!(by_id(&note["node"])["name"], "i");

    // the component's metadata
    assert_eq!(panel["imports"], serde_json::json!(["Child", "Hint", "NgIf", "NgTemplateOutlet"]));
    assert!(
        panel["host"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["name"] == "attr.data-hint" && h["source"] == "variant")
    );
    let video =
        panel["members"].as_array().unwrap().iter().find(|m| m["name"] == "videoControls").unwrap();
    assert_eq!(video["kind"], "getter");
    assert_eq!(video["doc"], "Playback controls of the open video overlay.");
}

#[test]
fn project_structure() {
    insta::assert_json_snapshot!(structure());
}
