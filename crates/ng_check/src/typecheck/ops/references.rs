//! `packages/compiler/src/typecheck/ops/references.ts`

use oxc_angular_compiler::ast::r3::{
    R3Component, R3Directive, R3Element, R3HostElement, R3LetDeclaration, R3Reference, R3Template, R3Variable,
};

use crate::angular::t2_api::node_key;

// references.ts:26 LocalSymbol
#[derive(Debug, Clone, Copy)]
pub enum LocalSymbol<'a> {
    Element(&'a R3Element<'a>),
    Template(&'a R3Template<'a>),
    Variable(&'a R3Variable<'a>),
    LetDeclaration(&'a R3LetDeclaration<'a>),
    Reference(&'a R3Reference<'a>),
    HostElement(&'a R3HostElement<'a>),
    Component(&'a R3Component<'a>),
    Directive(&'a R3Directive<'a>),
}

impl LocalSymbol<'_> {
    /// The node's identity (`t2_api::node_key`)
    pub fn key(&self) -> usize {
        match self {
            Self::Element(n) => node_key(*n),
            Self::Template(n) => node_key(*n),
            Self::Variable(n) => node_key(*n),
            Self::LetDeclaration(n) => node_key(*n),
            Self::Reference(n) => node_key(*n),
            Self::HostElement(n) => node_key(*n),
            Self::Component(n) => node_key(*n),
            Self::Directive(n) => node_key(*n),
        }
    }
}

// The rest of references.ts (TcbReferenceOp, TcbInvalidReferenceOp): not ported yet.
