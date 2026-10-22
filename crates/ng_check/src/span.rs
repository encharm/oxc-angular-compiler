//! Offsets in a type-check block's span comments are JavaScript string (UTF-16) indices, as Angular writes them;
//! the parser's are UTF-8 byte offsets. Generation runs with the text the spans index into set
//! (`with_source_text`), and span comments convert through `utf16`.

use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// For the text being generated for: the UTF-16 index of each byte offset where they differ, or none when the
    /// text is ASCII (and every offset is its own index).
    static TABLE: RefCell<Option<Vec<u32>>> = const { RefCell::new(None) };

    /// The text being generated for (`with_current_text`).
    static TEXT: RefCell<Option<Rc<str>>> = const { RefCell::new(None) };
}

/// Runs `f` with span comments converted for `text`.
pub fn with_source_text<R>(text: &str, f: impl FnOnce() -> R) -> R {
    let table = if text.is_ascii() {
        None
    } else {
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut index = 0u32;
        for c in text.chars() {
            for _ in 0..c.len_utf8() {
                table.push(index);
            }
            index += c.len_utf16() as u32;
        }
        table.push(index);
        Some(table)
    };
    let previous = TABLE.with(|t| t.replace(table));
    let previous_text = TEXT.with(|t| t.replace(Some(Rc::from(text))));
    let result = f();
    TABLE.with(|t| t.replace(previous));
    TEXT.with(|t| t.replace(previous_text));
    result
}

/// The UTF-16 index of a byte offset in the current text.
pub fn utf16(offset: u32) -> u32 {
    TABLE.with(|t| match &*t.borrow() {
        Some(table) => table.get(offset as usize).copied().unwrap_or(offset),
        None => offset,
    })
}

/// Runs `f` with the text set by `with_source_text` (none outside of it), for spans the fork's AST does not keep and
/// that are recovered from the text (`typecheck::expression`'s literal map keys).
pub fn with_current_text<R>(f: impl FnOnce(Option<&str>) -> R) -> R {
    let text = TEXT.with(|t| t.borrow().clone());
    f(text.as_deref())
}
