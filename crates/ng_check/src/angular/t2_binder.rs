//! `packages/compiler/src/render3/view/t2_binder.ts`: not ported yet (PORTING.md).
//!
//! STUB (ngc-glue): the signatures `ngp/tcb_adapter.rs` calls (`R3TargetBinder::new(matcher, foreign_matcher)`,
//! `bind(target)`), so that it compiles before the port lands. The merge drops this in favour of the port.

use std::marker::PhantomData;

use crate::angular::directive_matching::{SelectorMatcher, SelectorlessMatcher};
use crate::angular::t2_api::{BoundTarget, ForeignComponentMeta, Target};

// t2_binder.ts:167 DirectiveMatcher: `SelectorMatcher<DirectiveT[]> | SelectorlessMatcher<DirectiveT>`
#[derive(Debug)]
pub enum DirectiveMatcher<D> {
    Selector(SelectorMatcher<Vec<D>>),
    Selectorless(SelectorlessMatcher<D>),
}

// t2_binder.ts:175 R3TargetBinder (stub)
pub struct R3TargetBinder<D> {
    _directive_matcher: Option<DirectiveMatcher<D>>,
    _foreign_component_matcher: Option<SelectorlessMatcher<ForeignComponentMeta>>,
}

impl<D: Clone> R3TargetBinder<D> {
    pub fn new(
        directive_matcher: Option<DirectiveMatcher<D>>,
        foreign_component_matcher: Option<SelectorlessMatcher<ForeignComponentMeta>>,
    ) -> Self {
        Self { _directive_matcher: directive_matcher, _foreign_component_matcher: foreign_component_matcher }
    }

    // t2_binder.ts:185 bind (stub)
    pub fn bind<'a>(&self, _target: Target<'a, D>) -> R3BoundTarget<'a, D> {
        panic!("stub: R3TargetBinder::bind is not ported yet")
    }
}

// t2_binder.ts R3BoundTarget (stub)
pub struct R3BoundTarget<'a, D> {
    target: Target<'a, D>,
    _directives: PhantomData<D>,
}

impl<'a, D: Clone> BoundTarget<'a, D> for R3BoundTarget<'a, D> {
    fn target(&self) -> &Target<'a, D> {
        &self.target
    }
    fn get_directives_of_node(&self, _node: crate::angular::t2_api::DirectiveOwner<'a>) -> Option<Vec<D>> {
        panic!("stub")
    }
    fn get_foreign_component(
        &self,
        _element: &'a oxc_angular_compiler::ast::r3::R3Element<'a>,
    ) -> Option<ForeignComponentMeta> {
        panic!("stub")
    }
    fn get_reference_target(
        &self,
        _ref: &'a oxc_angular_compiler::ast::r3::R3Reference<'a>,
    ) -> Option<crate::angular::t2_api::ReferenceTarget<'a, D>> {
        panic!("stub")
    }
    fn get_consumer_of_binding(
        &self,
        _binding: crate::angular::t2_api::Binding<'a>,
    ) -> Option<crate::angular::t2_api::BindingConsumer<'a, D>> {
        panic!("stub")
    }
    fn get_expression_target(
        &self,
        _expr: &oxc_angular_compiler::ast::expression::AngularExpression<'a>,
    ) -> Option<crate::angular::t2_api::TemplateEntity<'a>> {
        panic!("stub")
    }
    fn get_definition_node_of_symbol(
        &self,
        _symbol: crate::angular::t2_api::TemplateEntity<'a>,
    ) -> Option<crate::angular::t2_api::ScopedNode<'a>> {
        panic!("stub")
    }
    fn get_nesting_level(&self, _node: crate::angular::t2_api::ScopedNode<'a>) -> usize {
        panic!("stub")
    }
    fn get_entities_in_scope(
        &self,
        _node: Option<crate::angular::t2_api::ScopedNode<'a>>,
    ) -> Vec<crate::angular::t2_api::TemplateEntity<'a>> {
        panic!("stub")
    }
    fn get_used_directives(&self) -> Vec<D> {
        panic!("stub")
    }
    fn get_eagerly_used_directives(&self) -> Vec<D> {
        panic!("stub")
    }
    fn get_used_pipes(&self) -> Vec<String> {
        panic!("stub")
    }
    fn get_eagerly_used_pipes(&self) -> Vec<String> {
        panic!("stub")
    }
    fn get_defer_blocks(&self) -> Vec<&'a oxc_angular_compiler::ast::r3::R3DeferredBlock<'a>> {
        panic!("stub")
    }
    fn get_deferred_trigger_target(
        &self,
        _block: &'a oxc_angular_compiler::ast::r3::R3DeferredBlock<'a>,
        _trigger: &'a oxc_angular_compiler::ast::r3::R3DeferredTrigger<'a>,
    ) -> Option<&'a oxc_angular_compiler::ast::r3::R3Element<'a>> {
        panic!("stub")
    }
    fn is_deferred(&self, _node: crate::angular::t2_api::DirectiveOwner<'a>) -> bool {
        panic!("stub")
    }
    fn get_defer_blocks_of_node(
        &self,
        _node: crate::angular::t2_api::DirectiveOwner<'a>,
    ) -> Vec<&'a oxc_angular_compiler::ast::r3::R3DeferredBlock<'a>> {
        panic!("stub")
    }
    fn get_defer_blocks_of_pipe(
        &self,
        _ast: &'a oxc_angular_compiler::ast::expression::BindingPipe<'a>,
    ) -> Vec<&'a oxc_angular_compiler::ast::r3::R3DeferredBlock<'a>> {
        panic!("stub")
    }
    fn referenced_directive_exists(&self, _name: &str) -> bool {
        panic!("stub")
    }
    fn get_conflicting_host_directive_bindings(
        &self,
        _node: crate::angular::t2_api::DirectiveOwner<'a>,
    ) -> Option<Vec<crate::angular::t2_api::ConflictingHostDirectiveBinding<D>>> {
        panic!("stub")
    }
}
