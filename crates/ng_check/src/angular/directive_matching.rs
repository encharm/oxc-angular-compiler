//! `packages/compiler/src/directive_matching.ts`: not ported yet (PORTING.md).
//!
//! STUB (ngc-glue): the signatures `ngp/tcb_adapter.rs` calls, so that it compiles before the port lands. The merge
//! drops this in favour of the port.

use std::marker::PhantomData;

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

// directive_matching.ts CssSelector (stub)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssSelector {
    pub element: Option<String>,
}

impl CssSelector {
    // directive_matching.ts CssSelector.parse (stub)
    pub fn parse(_selector: &str) -> Vec<CssSelector> {
        panic!("stub: CssSelector::parse is not ported yet")
    }
}

// directive_matching.ts SelectorMatcher (stub)
#[derive(Debug)]
pub struct SelectorMatcher<T> {
    _contexts: PhantomData<T>,
}

impl<T> Default for SelectorMatcher<T> {
    fn default() -> Self {
        Self { _contexts: PhantomData }
    }
}

impl<T> SelectorMatcher<T> {
    pub fn new() -> Self {
        Self::default()
    }

    // directive_matching.ts addSelectables (stub)
    pub fn add_selectables(&mut self, _css_selectors: &[CssSelector], _callback_ctxt: T)
    where
        T: Clone,
    {
        panic!("stub: SelectorMatcher::add_selectables is not ported yet")
    }
}

// directive_matching.ts SelectorlessMatcher (stub)
#[derive(Debug)]
pub struct SelectorlessMatcher<T> {
    registry: IndexMap<String, Vec<T>, FxBuildHasher>,
}

impl<T: Clone> SelectorlessMatcher<T> {
    pub fn new(registry: IndexMap<String, Vec<T>, FxBuildHasher>) -> Self {
        Self { registry }
    }

    // directive_matching.ts match
    pub fn match_(&self, name: &str) -> Vec<T> {
        self.registry.get(name).cloned().unwrap_or_default()
    }
}
