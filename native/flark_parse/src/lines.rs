//! Line table and byte → UTF-16 prefix table for one source text.
//!
//! Line endings follow CommonMark and comrak: `\n`, `\r\n`, or a bare `\r`.

pub struct LineIndex {
    /// Byte offset where each line starts.
    line_starts: Vec<usize>,
    /// Byte offset where each line's content ends, excluding its terminator.
    line_ends: Vec<usize>,
    /// utf16[i] = number of UTF-16 code units in src[..i]; len = src.len() + 1.
    utf16: Vec<u32>,
}

impl LineIndex {
    pub fn new(src: &str) -> Self {
        let bytes = src.as_bytes();
        let mut line_starts = vec![0usize];
        let mut line_ends = Vec::new();
        let mut utf16 = Vec::with_capacity(bytes.len() + 1);
        let mut count: u32 = 0;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            let width = if b < 0x80 { 1 } else if b < 0xE0 { 2 } else if b < 0xF0 { 3 } else { 4 };
            let units = if width == 4 { 2 } else { 1 };
            for _ in 0..width { utf16.push(count); }
            count += units;
            if b == b'\n' {
                line_ends.push(i);
                line_starts.push(i + 1);
            } else if b == b'\r' {
                line_ends.push(i);
                if bytes.get(i + 1) == Some(&b'\n') { utf16.push(count); count += 1; i += 1; }
                line_starts.push(i + 1);
            }
            i += width;
        }
        utf16.push(count);
        line_ends.push(bytes.len());
        LineIndex { line_starts, line_ends, utf16 }
    }
    pub fn line_count(&self) -> usize { self.line_starts.len() }
    pub fn line_start(&self, line0: usize) -> usize { self.line_starts[line0.min(self.line_starts.len() - 1)] }
    /// End of the line's content, excluding `\n`, `\r`, or `\r\n`.
    pub fn line_end(&self, line0: usize, _src_len: usize) -> usize { self.line_ends[line0.min(self.line_ends.len() - 1)] }
    /// Start of the next line, i.e. the end of this line including its terminator.
    pub fn line_end_with_break(&self, line0: usize, src_len: usize) -> usize {
        if line0 + 1 < self.line_starts.len() { self.line_starts[line0 + 1] } else { src_len }
    }
    pub fn line_of(&self, byte: usize) -> usize {
        match self.line_starts.binary_search(&byte) { Ok(i) => i, Err(i) => i.saturating_sub(1) }
    }
    pub fn u16(&self, byte: usize) -> u32 { self.utf16[byte.min(self.utf16.len() - 1)] }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bare_cr_and_crlf_are_line_endings() {
        let li = LineIndex::new("a\rb\r\nc\nd");
        assert_eq!(li.line_count(), 4);
        assert_eq!((li.line_start(1), li.line_end(1, 8)), (2, 3));
        assert_eq!(li.line_end_with_break(1, 8), 5);
        assert_eq!(li.line_of(4), 1);
        assert_eq!(li.u16(8), 8);
    }
    #[test]
    fn utf16_counts_supplementary_pairs() {
        let li = LineIndex::new("a😀b");
        assert_eq!(li.u16(1), 1); assert_eq!(li.u16(5), 3); assert_eq!(li.u16(6), 4);
    }
}
