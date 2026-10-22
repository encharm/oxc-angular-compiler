//! `packages/compiler/src/directive_matching.ts`: not ported yet (PORTING.md).
//!
//! STUB (branch `ngc-binder`): the items `t2_binder.rs` uses, with the signatures of the port on `ngc-matching`, so
//! that the binder compiles before the merge. The merge replaces this file with the port.

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssSelector {
    pub element: Option<String>,
    pub class_names: Vec<String>,
    pub attrs: Vec<String>,
    pub not_selectors: Vec<CssSelector>,
}

impl CssSelector {
    pub fn new() -> Self {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn parse(_selector: &str) -> Vec<CssSelector> {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn set_element(&mut self, _element: Option<&str>) {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn add_attribute(&mut self, _name: &str, _value: &str) {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn add_class_name(&mut self, _name: &str) {
        todo!("directive_matching.rs is a stub until the merge")
    }
}

/// `matchedCallback` of `SelectorMatcher.match`.
pub type MatchedCallback<'c, T> = Option<&'c mut dyn FnMut(&CssSelector, &T)>;

#[derive(Debug)]
pub struct SelectorMatcher<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> SelectorMatcher<T> {
    pub fn new() -> Self {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn add_selectables(&mut self, _css_selectors: &[CssSelector], _callback_ctxt: T)
    where
        T: Clone,
    {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn match_(&self, _css_selector: &CssSelector, _matched_callback: MatchedCallback<'_, T>) -> bool {
        todo!("directive_matching.rs is a stub until the merge")
    }
}

#[derive(Debug)]
pub struct SelectorlessMatcher<T> {
    _registry: IndexMap<String, Vec<T>, FxBuildHasher>,
}

impl<T: Clone> SelectorlessMatcher<T> {
    pub fn new(_registry: IndexMap<String, Vec<T>, FxBuildHasher>) -> Self {
        todo!("directive_matching.rs is a stub until the merge")
    }

    pub fn match_(&self, _name: &str) -> Vec<T> {
        todo!("directive_matching.rs is a stub until the merge")
    }
}
