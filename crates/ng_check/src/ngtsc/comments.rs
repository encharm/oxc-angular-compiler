//! `packages/compiler-cli/src/ngtsc/typecheck/src/comments.ts`, the part that reads diagnostics' locations:
//! `readSpanComment` and `hasIgnoreForDiagnosticsMarker`, over the text after a node (TypeScript's
//! `forEachTrailingCommentRange`).

use crate::typecheck::ops::codegen::SpanInfo;

/// TypeScript's `forEachTrailingCommentRange(text, pos)`: the comments after `pos` up to the end of its line, as the
/// texts between their delimiters (`/*` `*/` or `//`).
pub fn trailing_comments(text: &str, pos: usize) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut comments = vec![];
    let mut i = pos;
    loop {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\x0b' | b'\x0c') {
            i += 1;
        }
        if text[i..].starts_with("/*") {
            let Some(end) = text[i + 2..].find("*/") else { break };
            comments.push(&text[i + 2..i + 2 + end]);
            i += end + 4;
        } else if text[i..].starts_with("//") {
            let end = text[i..].find('\n').map_or(text.len(), |e| i + e);
            comments.push(&text[i + 2..end]);
            break;
        } else {
            break;
        }
    }
    comments
}

/// TypeScript's `forEachLeadingCommentRange(text, pos)` for multi-line comments: the `/*...*/` comments before the
/// token at `pos` (after any earlier code), in order
pub fn leading_comments(text: &str, pos: usize) -> Vec<&str> {
    let mut comments = vec![];
    let mut i = pos;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c' => i += 1,
            b'/' if text[i..].starts_with("/*") => {
                let Some(end) = text[i + 2..].find("*/") else { break };
                comments.push(&text[i + 2..i + 2 + end]);
                i += end + 4;
            }
            b'/' if text[i..].starts_with("//") => {
                i = text[i..].find('\n').map_or(text.len(), |e| i + e);
            }
            _ => break,
        }
    }
    comments
}

// comments.ts readSpanComment: the `/*start,end*/` comment after a node
pub fn read_span_comment(text: &str, node_end: usize) -> Option<SpanInfo> {
    trailing_comments(text, node_end).into_iter().find_map(|comment| {
        let (start, end) = comment.split_once(',')?;
        if start.is_empty() || end.is_empty() || !start.bytes().chain(end.bytes()).all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(SpanInfo { start: start.parse().ok()?, end: end.parse().ok()? })
    })
}

// comments.ts hasIgnoreForDiagnosticsMarker: a `/*D:ignore*/` comment after a node
pub fn has_ignore_for_diagnostics_marker(text: &str, node_end: usize) -> bool {
    trailing_comments(text, node_end).iter().any(|comment| comment.trim() == "D:ignore")
}
