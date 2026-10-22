//! `packages/compiler/src/directive_matching.ts`: STUB by `tcb-core` (what `typecheck` uses), to be replaced by
//! `matching`'s port.

// directive_matching.ts:41 CssSelector (stub)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssSelector {
    pub element: Option<String>,
    pub class_names: Vec<String>,
    pub attrs: Vec<String>,
    pub not_selectors: Vec<CssSelector>,
}

impl CssSelector {
    // directive_matching.ts:58 parse (stub): panics where TypeScript throws
    pub fn parse(_selector: &str) -> Vec<CssSelector> {
        todo!("ported by matching")
    }

    /// `parse` for a `try { CssSelector.parse(...) } catch`: the error instead of the panic (requested from
    /// `matching`).
    pub fn try_parse(_selector: &str) -> Result<Vec<CssSelector>, String> {
        todo!("ported by matching")
    }
}
