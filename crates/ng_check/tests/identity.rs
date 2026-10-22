//! Node identity is the node's address (`t2_api::node_key`). A node stored inline in its parent (not boxed) would
//! share its parent's address at offset 0, and the two would be one node to every map keyed by it. These are the
//! nodes keyed that way; their addresses must differ from their parents'.

use ng_check::angular::t2_api::node_key;
use ng_check::ngp::compiler_utils::{TemplateParseOptions, parse_template};
use oxc_allocator::Allocator;
use oxc_angular_compiler::ast::r3::R3Node;

fn first_node<'a>(allocator: &'a Allocator, template: &str) -> &'a R3Node<'a> {
    let parsed = parse_template(allocator, allocator.alloc_str(template), "t.html", &TemplateParseOptions::default());
    assert!(parsed.errors.is_none(), "{:?}", parsed.errors);
    parsed.nodes.iter().find(|n| !matches!(n, R3Node::Text(_))).expect("a node")
}

#[test]
fn for_block_children_have_their_own_address() {
    let allocator = Allocator::default();
    let R3Node::ForLoopBlock(block) = first_node(&allocator, "@for (x of xs; track x) {a} @empty {b}") else {
        panic!("a @for block")
    };
    let parent = node_key(&**block);
    assert_ne!(parent, node_key(&block.item), "the item variable");
    assert_ne!(parent, node_key(block.empty.as_ref().unwrap()), "the @empty block");
}

#[test]
fn defer_block_children_have_their_own_address() {
    let allocator = Allocator::default();
    let template = "<p #t></p> @defer (on viewport(t); on hover(t); on interaction(t)) {a} @placeholder {b} \
                    @loading {c} @error {d}";
    let parsed = parse_template(&allocator, allocator.alloc_str(template), "t.html", &TemplateParseOptions::default());
    assert!(parsed.errors.is_none(), "{:?}", parsed.errors);
    let block = parsed
        .nodes
        .iter()
        .find_map(|n| if let R3Node::DeferredBlock(b) = n { Some(b) } else { None })
        .expect("a @defer block");
    let parent = node_key(&**block);
    let triggers = &block.triggers;
    for (what, key) in [
        ("@placeholder", node_key(block.placeholder.as_ref().unwrap())),
        ("@loading", node_key(block.loading.as_ref().unwrap())),
        ("@error", node_key(block.error.as_ref().unwrap())),
        ("the viewport trigger", node_key(triggers.viewport.as_ref().unwrap())),
        ("the hover trigger", node_key(triggers.hover.as_ref().unwrap())),
        ("the interaction trigger", node_key(triggers.interaction.as_ref().unwrap())),
    ] {
        assert_ne!(parent, key, "{what}");
    }
}
