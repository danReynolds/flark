//! Line starts and a byte → UTF-16 prefix table for one source text.

pub struct LineIndex {
    line_starts: Vec<usize>,
    /// utf16[i] = number of UTF-16 code units in src[..i]; len = src.len() + 1.
    utf16: Vec<u32>,
}

impl LineIndex {
    pub fn new(src: &str) -> Self {
        let bytes = src.as_bytes();
        let mut line_starts = vec![0usize];
        let mut utf16 = Vec::with_capacity(bytes.len() + 1);
        let mut count: u32 = 0;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            let width = if b < 0x80 { 1 } else if b < 0xE0 { 2 } else if b < 0xF0 { 3 } else { 4 };
            let units = if width == 4 { 2 } else { 1 };
            for _ in 0..width { utf16.push(count); }
            count += units;
            if b == b'\n' { line_starts.push(i + 1); }
            i += width;
        }
        utf16.push(count);
        LineIndex { line_starts, utf16 }
    }
    pub fn line_count(&self) -> usize { self.line_starts.len() }
    pub fn line_start(&self, line0: usize) -> usize { self.line_starts[line0.min(self.line_starts.len() - 1)] }
    /// End of the line's content, excluding the newline.
    pub fn line_end(&self, line0: usize, src_len: usize) -> usize {
        if line0 + 1 < self.line_starts.len() { self.line_starts[line0 + 1] - 1 } else { src_len }
    }
    /// End of the line including its newline, if any.
    pub fn line_end_with_break(&self, line0: usize, src_len: usize) -> usize {
        if line0 + 1 < self.line_starts.len() { self.line_starts[line0 + 1] } else { src_len }
    }
    pub fn line_of(&self, byte: usize) -> usize {
        match self.line_starts.binary_search(&byte) { Ok(i) => i, Err(i) => i.saturating_sub(1) }
    }
    pub fn u16(&self, byte: usize) -> u32 { self.utf16[byte.min(self.utf16.len() - 1)] }
}
