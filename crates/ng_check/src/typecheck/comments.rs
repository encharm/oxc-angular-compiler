//! `packages/compiler/src/typecheck/comments.ts`

// comments.ts CommentTriviaType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentTriviaType {
    Diagnostic,
    ExpressionTypeIdentifier,
}

impl CommentTriviaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diagnostic => "D",
            Self::ExpressionTypeIdentifier => "T",
        }
    }
}

// comments.ts ExpressionIdentifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionIdentifier {
    Directive,
    HostDirective,
    ComponentCompletion,
    EventParameter,
    VariableAsExpression,
}

impl ExpressionIdentifier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Directive => "DIR",
            Self::HostDirective => "HOSTDIR",
            Self::ComponentCompletion => "COMPCOMP",
            Self::EventParameter => "EP",
            Self::VariableAsExpression => "VAE",
        }
    }
}
