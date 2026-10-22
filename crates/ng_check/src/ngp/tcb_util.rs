//! `packages/compiler-cli/preprocessor/src/tcb_util.ts`
//!
//! Offsets here are JavaScript string (UTF-16) indices, as in TypeScript and in ng-analyze's metadata (`SpanMetadata`).
//! The conversions at the end (not in the TypeScript, where strings are UTF-16) map them to and from the byte offsets
//! of Rust strings.

use std::cell::RefCell;
use std::rc::Rc;

use ng_analyze::ClassMetadata;

use crate::angular::t2_api::BoundTarget;
use crate::ngp::tcb::{PipeMeta, TcbTargetInput};
use crate::typecheck::api::{DirectiveRef, Map};

thread_local! {
    // tcb_util.ts:13 lineStartsCache
    static LINE_STARTS_CACHE: RefCell<Map<String, Rc<Vec<u32>>>> = RefCell::new(Map::default());
}

// tcb_util.ts:14 MAX_CACHE_SIZE
const MAX_CACHE_SIZE: usize = 10;

// tcb_util.ts:16 getLineStarts
pub fn get_line_starts(content: &str) -> Rc<Vec<u32>> {
    let cached = LINE_STARTS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // LRU: delete and re-set to move to end
        let line_starts = cache.shift_remove(content)?;
        cache.insert(content.to_string(), Rc::clone(&line_starts));
        Some(line_starts)
    });
    if let Some(line_starts) = cached {
        return line_starts;
    }

    let mut line_starts = vec![0];
    let mut pos = 0u32;
    for c in content.chars() {
        pos += c.len_utf16() as u32;
        if c == '\n' {
            line_starts.push(pos);
        }
    }
    let line_starts = Rc::new(line_starts);

    LINE_STARTS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= MAX_CACHE_SIZE {
            cache.shift_remove_index(0);
        }
        cache.insert(content.to_string(), Rc::clone(&line_starts));
    });

    line_starts
}

/// `{line: number; character: number}`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

// tcb_util.ts:43 positionToOffset
pub fn position_to_offset(content: &str, position: Position) -> u32 {
    let line_starts = get_line_starts(content);
    let content_length = utf16_len(content);
    let line = position.line as usize;
    if line >= line_starts.len() {
        return content_length; // Fallback
    }
    let line_start = line_starts[line];
    let next_line_start = line_starts.get(line + 1).copied().unwrap_or(content_length);

    let mut line_length = next_line_start - line_start;
    if line_length > 0 && char_code_at(content, next_line_start - 1) == Some(u16::from(b'\n')) {
        line_length -= 1;
        if line_length > 0 && char_code_at(content, next_line_start - 2) == Some(u16::from(b'\r')) {
            line_length -= 1;
        }
    }

    let character = position.character.min(line_length); // Clamp character

    line_start + character
}

// tcb_util.ts:71 lineNumberAtOffset: the 1-based line number containing `offset`
pub fn line_number_at_offset(content: &str, offset: u32) -> u32 {
    offset_to_position(content, offset).line + 1
}

// tcb_util.ts:75 offsetToPosition
pub fn offset_to_position(content: &str, offset: u32) -> Position {
    let offset = offset.min(utf16_len(content)); // Clamp offset to content length
    let line_starts = get_line_starts(content);
    // the first line start is 0, so the line is the last start at or before `offset`
    let line = line_starts.partition_point(|&start| start <= offset) - 1;
    let line_start = line_starts[line];
    Position { line: line as u32, character: offset - line_start }
}

/// What `requiresInlineDeclaration` reads: `{isExported?: boolean; hasNonExportedBounds?: boolean}`
pub trait InlineDeclarationMeta {
    fn is_exported(&self) -> Option<bool>;
    fn has_non_exported_bounds(&self) -> Option<bool>;
}

impl InlineDeclarationMeta for ClassMetadata {
    fn is_exported(&self) -> Option<bool> {
        Some(self.is_exported)
    }
    fn has_non_exported_bounds(&self) -> Option<bool> {
        Some(self.has_non_exported_bounds)
    }
}

impl InlineDeclarationMeta for PipeMeta {
    fn is_exported(&self) -> Option<bool> {
        self.is_exported
    }
    fn has_non_exported_bounds(&self) -> Option<bool> {
        self.has_non_exported_bounds
    }
}

// tcb_util.ts:103 requiresInlineDeclaration: whether a declaration requires inline handling due to not being
// exported or having non-exported generic type parameter bounds
pub fn requires_inline_declaration<M: InlineDeclarationMeta + ?Sized>(meta: Option<&M>) -> bool {
    let Some(meta) = meta else { return false };
    meta.is_exported() == Some(false) || meta.has_non_exported_bounds() == Some(true)
}

// tcb_util.ts:116 requiresInlineTypeCheckBlock: whether a component target requires an inline type-check block
// https://github.com/angular/angular/blob/18d899e/packages/compiler-cli/src/ngtsc/typecheck/src/tcb_util.ts#L102-L127
pub fn requires_inline_type_check_block<'a>(
    target: &TcbTargetInput<'a>,
    bound_target: &dyn BoundTarget<'a, DirectiveRef>,
    local_classes: &Map<String, &ClassMetadata>,
) -> bool {
    if requires_inline_declaration(Some(&target.class_meta)) {
        return true;
    }

    if let Some(pipe_registry) = &target.pipe_registry {
        for pipe_name in bound_target.get_used_pipes() {
            let pipe_meta = pipe_registry.get(&pipe_name);
            if let Some(pipe_meta) = pipe_meta
                && (requires_inline_declaration(Some(pipe_meta)) || pipe_meta.import_path.is_none())
            {
                return true;
            }
        }
    }

    for dir in bound_target.get_used_directives() {
        let dir_class = local_classes.get(&dir.name).copied();
        if dir_class.is_none() && dir.r#ref.is_local && dir.r#ref.module_name.is_none() {
            return true;
        }
        if requires_inline_declaration(dir_class) {
            return true;
        }
    }

    false
}

// Not in the TypeScript: JavaScript string indices and Rust byte offsets.

/// `content.length`: the length of `content` in UTF-16 code units
pub fn utf16_len(content: &str) -> u32 {
    if content.is_ascii() { content.len() as u32 } else { content.encode_utf16().count() as u32 }
}

/// `content.charCodeAt(index)`
pub fn char_code_at(content: &str, index: u32) -> Option<u16> {
    content.encode_utf16().nth(index as usize)
}

/// The byte offset in `content` of the UTF-16 index `offset` (clamped to the end; an index inside a character maps to
/// the character's start)
pub fn utf16_to_byte_offset(content: &str, offset: u32) -> u32 {
    if content.is_ascii() {
        return offset.min(content.len() as u32);
    }
    let mut index = 0u32;
    for (byte, c) in content.char_indices() {
        let next = index + c.len_utf16() as u32;
        if next > offset {
            return byte as u32;
        }
        index = next;
    }
    content.len() as u32
}

/// The UTF-16 index in `content` of the byte offset `offset`
pub fn byte_to_utf16_offset(content: &str, offset: u32) -> u32 {
    if content.is_ascii() {
        return offset.min(content.len() as u32);
    }
    let offset = (offset as usize).min(content.len());
    let mut end = offset;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    utf16_len(&content[..end])
}

/// `content.substring(start, end)` with UTF-16 indices
pub fn substring(content: &str, start: u32, end: u32) -> &str {
    let (start, end) = if start <= end { (start, end) } else { (end, start) };
    let start = utf16_to_byte_offset(content, start) as usize;
    let end = utf16_to_byte_offset(content, end) as usize;
    &content[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_and_offsets() {
        let content = "ab\r\ncd\nef";
        assert_eq!(*get_line_starts(content), vec![0, 4, 7]);
        assert_eq!(offset_to_position(content, 5), Position { line: 1, character: 1 });
        assert_eq!(offset_to_position(content, 100), Position { line: 2, character: 2 });
        assert_eq!(line_number_at_offset(content, 0), 1);
        // the character is clamped to the line's length without its line ending
        assert_eq!(position_to_offset(content, Position { line: 0, character: 10 }), 2);
        assert_eq!(position_to_offset(content, Position { line: 1, character: 1 }), 5);
        assert_eq!(position_to_offset(content, Position { line: 9, character: 0 }), 9);
    }

    #[test]
    fn utf16_offsets() {
        let content = "é𝒳x";
        assert_eq!(utf16_len(content), 4);
        assert_eq!(utf16_to_byte_offset(content, 1), 2);
        assert_eq!(utf16_to_byte_offset(content, 3), 6);
        assert_eq!(byte_to_utf16_offset(content, 6), 3);
        assert_eq!(substring(content, 3, 4), "x");
        assert_eq!(offset_to_position("é\nx", 2), Position { line: 1, character: 0 });
    }

    #[test]
    fn inline_declarations() {
        let pipe = PipeMeta { is_exported: Some(false), ..PipeMeta::default() };
        assert!(requires_inline_declaration(Some(&pipe)));
        let pipe = PipeMeta { has_non_exported_bounds: Some(true), ..PipeMeta::default() };
        assert!(requires_inline_declaration(Some(&pipe)));
        assert!(!requires_inline_declaration(Some(&PipeMeta::default())));
        assert!(!requires_inline_declaration::<PipeMeta>(None));
    }
}
