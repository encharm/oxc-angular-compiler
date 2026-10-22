//! `packages/compiler-cli/src/ngtsc/typecheck/src/diagnostics.ts`: which diagnostics of a type-check block are
//! reported, and where in the template.

use crate::ngtsc::tcb_util::{SourceLocation, TcbFile};

// diagnostics.ts:18 isAccessOnThis
fn is_access_on_this(text: &str, start: usize) -> bool {
    let ends_with = |suffix: &str| start >= suffix.len() && text.is_char_boundary(start - suffix.len()) && &text[start - suffix.len()..start] == suffix;
    ends_with("this.") || ends_with("(this).")
}

// diagnostics.ts:29 shouldReportDiagnostic: some diagnostics come from the way type-check blocks are generated, not
// from the template. `start` is the diagnostic's byte offset in the block's text.
pub fn should_report_diagnostic(code: u32, text: &str, start: usize) -> bool {
    match code {
        // $var is declared but its value is never read.
        6133 => false,
        // All variables are unused.
        6199 => false,
        // Left side of comma operator is unused and has no side effects.
        2695 => false,
        // Parameter '$event' implicitly has an 'any' type.
        7006 => false,
        // Property 'X' is private and only accessible within class: private property reads are allowed in templates
        2341 => !is_access_on_this(text, start),
        _ => true,
    }
}

// diagnostics.ts:59 translateDiagnostic: where in the template a diagnostic at `start` (a byte offset in the block)
// comes from, or none when it should not be reported
pub fn translate_diagnostic(file: &TcbFile, start: usize) -> Option<SourceLocation> {
    file.find_source_location(start, /* is_diagnostics_request */ true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_reads_on_this_are_allowed() {
        let text = "\"\" + (this).secret /*1,2*/;";
        let start = text.find("secret").unwrap();
        assert!(!should_report_diagnostic(2341, text, start));
        assert!(should_report_diagnostic(2341, "other.secret", 6));
        assert!(!should_report_diagnostic(6133, text, 0));
    }
}
