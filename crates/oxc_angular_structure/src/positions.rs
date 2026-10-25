//! Byte offsets, as the parsers count, to the UTF-16 offsets, lines and columns of the schema.

use crate::schema::Position;

/// Offsets into one source text.
pub struct Positions<'s> {
    source: &'s str,
    /// The UTF-16 offset at each byte offset, the source's length included; a byte inside a character maps to the
    /// character's start
    utf16: Vec<u32>,
    /// The byte offset of each line's start
    line_starts: Vec<u32>,
}

impl<'s> Positions<'s> {
    pub fn new(source: &'s str) -> Self {
        let mut utf16 = Vec::with_capacity(source.len() + 1);
        let mut line_starts = vec![0];
        let mut units = 0u32;
        for (i, c) in source.char_indices() {
            while utf16.len() < i {
                utf16.push(units);
            }
            utf16.push(units);
            units += u32::try_from(c.len_utf16()).unwrap_or(2);
            if c == '\n' {
                line_starts.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
            }
        }
        while utf16.len() <= source.len() {
            utf16.push(units);
        }
        Self { source, utf16, line_starts }
    }

    pub fn source(&self) -> &'s str {
        self.source
    }

    /// The UTF-16 offset of a byte offset; past the end, the end.
    pub fn utf16(&self, byte: u32) -> u32 {
        self.utf16
            .get(byte as usize)
            .copied()
            .unwrap_or_else(|| self.utf16.last().copied().unwrap_or(0))
    }

    /// The position of the bytes `start..end`.
    pub fn of(&self, start: u32, end: u32) -> Position {
        let line_index = self.line_starts.partition_point(|&s| s <= start).saturating_sub(1);
        let line_start = self.line_starts[line_index];
        Position {
            start: self.utf16(start),
            end: self.utf16(end.max(start)),
            line: u32::try_from(line_index + 1).unwrap_or(u32::MAX),
            column: self.utf16(start) - self.utf16(line_start) + 1,
        }
    }

    /// The position of the UTF-16 offsets `start..end` (JavaScript string indices, as ng-analyze counts).
    pub fn of_utf16(&self, start: u32, end: u32) -> Position {
        let byte = |offset: u32| {
            u32::try_from(self.utf16.partition_point(|&u| u < offset)).unwrap_or(u32::MAX)
        };
        self.of(byte(start), byte(end))
    }

    /// The text of the bytes `start..end`; empty when they are not a character range of the source.
    pub fn text(&self, start: u32, end: u32) -> &'s str {
        self.source.get(start as usize..end as usize).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::Positions;

    #[test]
    fn counts_utf16_units_lines_and_columns() {
        // "é" is 2 bytes and 1 UTF-16 unit, "😀" 4 bytes and 2 units
        let p = Positions::new("é😀\nab");
        let b = u32::try_from("é😀\n".len()).unwrap() + 1;
        let pos = p.of(b, b + 1);
        assert_eq!((pos.start, pos.end, pos.line, pos.column), (5, 6, 2, 2));
        assert_eq!(p.of(2, 6).start, 1);
        let back = p.of_utf16(5, 6);
        assert_eq!((back.start, back.end, back.line, back.column), (5, 6, 2, 2));
        assert_eq!(p.of(2, 6).end, 3);
    }
}
