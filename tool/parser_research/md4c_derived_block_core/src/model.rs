use crate::OriginSpan;
use comrak::block_spine_facade::FacadeAlignment;
use im::{OrdMap, Vector};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlClass {
    ScriptPreStyleTextarea = 1,
    Comment = 2,
    ProcessingInstruction = 3,
    Declaration = 4,
    Cdata = 5,
    BlockTag = 6,
    CompleteTag = 7,
}

impl HtmlClass {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::ScriptPreStyleTextarea),
            2 => Some(Self::Comment),
            3 => Some(Self::ProcessingInstruction),
            4 => Some(Self::Declaration),
            5 => Some(Self::Cdata),
            6 => Some(Self::BlockTag),
            7 => Some(Self::CompleteTag),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    Quote,
    Unordered { marker: u8 },
    Ordered { delimiter: u8, start: u64 },
    Item { task: Option<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeafKind {
    ThematicBreak,
    Heading { level: u8, setext: bool },
    Code { fenced: bool },
    Html { class: HtmlClass },
    Paragraph,
    Table { alignments: Vec<FacadeAlignment> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafLine {
    /// Inline/logical bytes after block/container prefixes. This may include
    /// the physical ending in raw coverage; the inline service rtrims the
    /// terminal suffix and emits breaks only for interior line endings.
    pub logical: OriginSpan,
    /// Exact visible content before the physical line ending.
    pub content: OriginSpan,
    /// Hidden indentation/container/list marker bytes.
    pub hidden_prefix: OriginSpan,
    pub indent: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafBlock {
    pub kind: LeafKind,
    pub lines: Vec<LeafLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockRecord {
    Enter {
        kind: ContainerKind,
        marker: OriginSpan,
        tight: bool,
    },
    Exit {
        kind: ContainerKind,
    },
    Leaf(LeafBlock),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceOccurrence {
    pub normalized_label: String,
    pub source: OriginSpan,
    pub label: OriginSpan,
    pub destination: OriginSpan,
    pub title: Option<OriginSpan>,
    pub url: String,
    pub clean_title: String,
}

#[derive(Clone, Debug, Default)]
pub struct BlockOutput {
    pub records: Vector<BlockRecord>,
    /// Ordered recognized occurrences are output, not continuation state.
    pub reference_occurrences: Vector<ReferenceOccurrence>,
    /// Convenience aggregate for inline resolution. It is deliberately kept
    /// out of checkpoint equality/convergence.
    pub first_definitions: OrdMap<String, ReferenceOccurrence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureKind {
    Quote,
    List {
        ordered: bool,
        start: u64,
        tight: bool,
    },
    Item,
    ThematicBreak,
    Heading {
        level: u8,
        setext: bool,
    },
    Code {
        fenced: bool,
    },
    Html {
        class: HtmlClass,
    },
    Paragraph,
    Table {
        columns: usize,
    },
    TableRow {
        header: bool,
    },
    TableCell {
        column: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureEvent {
    Enter(SignatureKind),
    Exit(SignatureKind),
}
