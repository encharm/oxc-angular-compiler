//! The error guide links ngtsc appends to its diagnostics: `packages/compiler-cli/src/ngtsc/diagnostics/src/`
//! `error_details_base_url.ts`, the guide codes of `error_code.ts`, and `addDiagnosticDetails`/`appendMessageText` of
//! `util.ts` (applied by `NgCompiler.addMessageTextDetails`, `core/src/compiler.ts`).

// error_code.ts: the codes with a guide page are the negative members of `ErrorCode`
const CODES_WITH_GUIDES: &[u32] = &[1001, 2003, 2009, 3003, 6100, 8001, 8002, 8003, 8011, 8023, 8024];

// error_details_base_url.ts DOC_PAGE_BASE_URL, for the Angular version `full` (`VERSION.full` of the project's
// @angular/compiler)
pub fn doc_page_base_url(full: &str) -> String {
    let is_pre_release = full.contains("-next") || full.contains("-rc");
    let prefix = if is_pre_release {
        "next".to_string()
    } else {
        format!("v{}", full.split('.').next().unwrap_or(full))
    };
    format!("https://{prefix}.angular.dev")
}

// compiler.ts addMessageTextDetails, for one diagnostic: `NG<code>`'s message with its guide link when it has one
pub fn add_message_text_details(code: u32, message: &str, doc_page_base_url: &str) -> String {
    if !CODES_WITH_GUIDES.contains(&code) {
        return message.to_string();
    }
    // util.ts addDiagnosticDetails
    let details = format!("Find more at {doc_page_base_url}/errors/NG{code}");
    append_message_text(message, &details)
}

// util.ts appendMessageText
fn append_message_text(message_text: &str, text_to_append: &str) -> String {
    if message_text.is_empty() {
        return text_to_append.to_string();
    }
    let separator = if message_text.ends_with(['.', ',', ';', '!', '?', '\n']) { " " } else { ". " };
    format!("{message_text}{separator}{text_to_append}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_links() {
        let next = doc_page_base_url("22.3.0-next.2+sha-3cc1edf");
        assert_eq!(next, "https://next.angular.dev");
        assert_eq!(doc_page_base_url("21.0.1"), "https://v21.angular.dev");
        assert_eq!(
            add_message_text_details(8002, "Can't bind to 'x' since it isn't a known property of 'div'.", &next),
            "Can't bind to 'x' since it isn't a known property of 'div'. Find more at https://next.angular.dev/errors/NG8002"
        );
        assert_eq!(add_message_text_details(8004, "No pipe found", &next), "No pipe found");
        assert_eq!(add_message_text_details(8001, "Unknown", &next), "Unknown. Find more at https://next.angular.dev/errors/NG8001");
    }
}
