//! `packages/compiler/src/directive_matching.ts`: not ported yet (PORTING.md).
//!
//! STUB (tcb-ops): the signatures of the port in progress (`ngc-matching`) that `typecheck/ops/content_projection.rs`
//! calls; the real port replaces this file.

// directive_matching.ts CssSelector (STUB)
#[derive(Debug, Clone, Default)]
pub struct CssSelector;

impl CssSelector {
    // directive_matching.ts CssSelector.parse (STUB)
    pub fn parse(_selector: &str) -> Vec<CssSelector> {
        todo!()
    }
}

/// `matchedCallback` of `SelectorMatcher.match`.
pub type MatchedCallback<'c, T> = Option<&'c mut dyn FnMut(&CssSelector, &T)>;

// directive_matching.ts:222 SelectorMatcher (STUB)
#[derive(Debug)]
pub struct SelectorMatcher<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> SelectorMatcher<T> {
    #[expect(clippy::new_without_default, reason = "stub")]
    pub fn new() -> Self {
        todo!()
    }

    // directive_matching.ts:237 addSelectables (STUB)
    pub fn add_selectables(&mut self, _css_selectors: &[CssSelector], _callback_ctxt: T)
    where
        T: Clone,
    {
        todo!()
    }

    // directive_matching.ts:340 match (STUB)
    pub fn match_(
        &self,
        _css_selector: &CssSelector,
        _matched_callback: MatchedCallback<'_, T>,
    ) -> bool {
        todo!()
    }
}
