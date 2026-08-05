use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub(crate) fn new(start: usize, end: usize) -> Result<Self, ParseError> {
        Ok(Self {
            start: u32::try_from(start).map_err(|_| ParseError::DocumentTooLarge)?,
            end: u32::try_from(end).map_err(|_| ParseError::DocumentTooLarge)?,
        })
    }

    pub(crate) fn shifted(self, delta: isize) -> Self {
        let delta = i32::try_from(delta).expect("32-bit spike shift");
        Self {
            start: self
                .start
                .checked_add_signed(delta)
                .expect("validated edit shift"),
            end: self
                .end
                .checked_add_signed(delta)
                .expect("validated edit shift"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HeadingLevel {
    H1,
    H2,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Container {
    BlockQuote,
    BulletItem {
        marker: u8,
        indent: u8,
    },
    OrderedItem {
        delimiter: u8,
        start: u32,
        indent: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ancestry(pub Vec<Container>);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChunkKind {
    Paragraph,
    Heading(HeadingLevel),
    Blank,
    FenceOpen { marker: u8, len: u32 },
    CodeLine,
    FenceClose { marker: u8, len: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    pub id: u64,
    pub kind: ChunkKind,
    pub source: Span,
    /// Exact for single-line chunks and fence info/code lines. For a
    /// multi-line paragraph this is the enclosing content interval; marker
    /// exclusions on continuation lines live in `MarkerFact`.
    pub content: Span,
    pub ancestry: u32,
}

impl Chunk {
    pub(crate) fn shifted(mut self, delta: isize) -> Self {
        self.source = self.source.shifted(delta);
        self.content = self.content.shifted(delta);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MarkerKind {
    BlockQuote,
    Bullet(u8),
    Ordered { delimiter: u8, start: u32 },
    FenceOpen(u8),
    FenceClose(u8),
    Setext(HeadingLevel),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkerFact {
    pub chunk_id: u64,
    pub kind: MarkerKind,
    pub span: Span,
}

impl MarkerFact {
    pub(crate) fn shifted(mut self, delta: isize) -> Self {
        self.span = self.span.shifted(delta);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fuel {
    pub bytes: usize,
}

impl Fuel {
    pub const fn bytes(bytes: usize) -> Self {
        Self { bytes }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdvanceReceipt {
    /// Source bytes examined by this call. This is always `<= Fuel::bytes`.
    pub source_bytes: usize,
    pub lines_completed: usize,
    pub chunks_emitted: usize,
    pub facts_emitted: usize,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputDelta {
    pub old_chunks: Range<usize>,
    pub new_chunks: Range<usize>,
    pub old_facts: Range<usize>,
    pub new_facts: Range<usize>,
    /// A production persistent tree would apply this lazily to the reused
    /// suffix segment. The spike's `Vec` backend applies it eagerly and
    /// reports that limitation through `ProductionGap`.
    pub reused_suffix_shift: isize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReceipt {
    pub restart: usize,
    pub reparsed_end: usize,
    pub reparsed_bytes: usize,
    pub advance_calls: usize,
    pub converged: bool,
    pub reused_suffix_chunks: usize,
    pub delta: OutputDelta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticChunk {
    pub kind: ChunkKind,
    pub source: Span,
    pub content: Span,
    pub ancestry: Ancestry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticFact {
    pub kind: MarkerKind,
    pub span: Span,
    pub chunk: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticSnapshot {
    pub chunks: Vec<SemanticChunk>,
    pub facts: Vec<SemanticFact>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MemoryReceipt {
    pub source_bytes: usize,
    pub chunk_bytes: usize,
    pub fact_bytes: usize,
    pub checkpoint_bytes: usize,
    pub checkpoint_container_bytes: usize,
    pub ancestry_bytes: usize,
    pub transient_state_bytes: usize,
    pub checkpoints: usize,
    pub chunks: usize,
    pub facts: usize,
    pub max_advance_source_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    DocumentTooLarge,
    InvalidEditRange,
    ZeroFuel,
    PrefixLimit {
        line_start: usize,
    },
    UnsupportedSyntax {
        offset: usize,
        feature: &'static str,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentTooLarge => formatter.write_str("the spike uses 32-bit source offsets"),
            Self::InvalidEditRange => formatter.write_str("edit range is not on UTF-8 boundaries"),
            Self::ZeroFuel => formatter.write_str("advance requires non-zero byte fuel"),
            Self::PrefixLimit { line_start } => write!(
                formatter,
                "syntactic prefix exceeds the bounded spike limit at byte {line_start}"
            ),
            Self::UnsupportedSyntax { offset, feature } => {
                write!(formatter, "unsupported {feature} at byte {offset}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionGap {
    PersistentChunkedSource,
    PersistentOutputTreeAndLazySuffixShift,
    IntraLineEditCheckpoint,
    CompleteCommonMarkBlockGrammar,
    InlineGrammarAndReferenceDependencies,
    ListTightnessAndContainerRangeFacts,
    GfmAutolinksAndTagfilter,
    NativeWasmParity,
    StableOrderKeyStress,
}

pub const PRODUCTION_GAPS: &[ProductionGap] = &[
    ProductionGap::PersistentChunkedSource,
    ProductionGap::PersistentOutputTreeAndLazySuffixShift,
    ProductionGap::IntraLineEditCheckpoint,
    ProductionGap::CompleteCommonMarkBlockGrammar,
    ProductionGap::InlineGrammarAndReferenceDependencies,
    ProductionGap::ListTightnessAndContainerRangeFacts,
    ProductionGap::GfmAutolinksAndTagfilter,
    ProductionGap::NativeWasmParity,
    ProductionGap::StableOrderKeyStress,
];
