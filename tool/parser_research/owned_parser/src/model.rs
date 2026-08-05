use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

impl SourceRange {
    pub fn new(start: usize, end: usize) -> Self {
        assert!(start <= end);
        Self { start, end }
    }

    pub fn as_range(self) -> Range<usize> {
        self.start..self.end
    }

    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkerKind {
    HeadingOpen,
    HeadingClose,
    FenceOpen,
    FenceClose,
    Quote,
    Emphasis,
    Strong,
    Code,
    HardBreak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub kind: MarkerKind,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading { level: u8 },
    ThematicBreak,
    IndentedCode,
    FencedCode { info: String },
    BlockQuote,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineKind {
    Text(String),
    SoftBreak,
    HardBreak,
    Code(String),
    Emphasis,
    Strong,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline {
    pub kind: InlineKind,
    pub range: SourceRange,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: u64,
    pub kind: BlockKind,
    pub range: SourceRange,
    pub content_range: SourceRange,
    pub markers: Vec<Marker>,
    pub inlines: Vec<Inline>,
    pub children: Vec<Block>,
    pub literal: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageLeaf {
    pub range: SourceRange,
    pub owner: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub source_len: usize,
    pub blocks: Vec<Block>,
    pub coverage: Vec<CoverageLeaf>,
}
