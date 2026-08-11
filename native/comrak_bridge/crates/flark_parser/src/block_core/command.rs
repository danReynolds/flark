// SPDX-License-Identifier: BSD-2-Clause
// SPDX-FileCopyrightText: 2017-2026 Comrak contributors
// SPDX-FileCopyrightText: 2026 Flark contributors
//
// Mechanically adapted from the Comrak 0.54.0-correspondent direct value seam
// in `tool/parser_research/comrak_value_block_core/src/parser.rs`. The pinned
// donor commit is 172c2ee7d2c5c262a28be3e407aadf705daea2b7. The complete
// license notice is in `vendor/comrak/COPYING`.

use super::ClosedChild;

/// Exact physical dimensions in the two coordinate systems used by Flark.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceMetric {
    bytes: u64,
    utf16: u64,
}

impl SourceMetric {
    #[must_use]
    pub const fn new(bytes: u64, utf16: u64) -> Option<Self> {
        if (bytes == 0) != (utf16 == 0) || bytes < utf16 {
            return None;
        }
        Some(Self { bytes, utf16 })
    }

    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn utf16(self) -> u64 {
        self.utf16
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bytes == 0 && self.utf16 == 0
    }

    #[must_use]
    pub const fn checked_add(self, suffix: Self) -> Option<Self> {
        let Some(bytes) = self.bytes.checked_add(suffix.bytes) else {
            return None;
        };
        let Some(utf16) = self.utf16.checked_add(suffix.utf16) else {
            return None;
        };
        Some(Self { bytes, utf16 })
    }
}

/// One exact position relative to the active physical line.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineSourcePosition {
    byte: u64,
    utf16: u64,
}

impl LineSourcePosition {
    #[must_use]
    pub const fn new(byte: u64, utf16: u64) -> Self {
        Self { byte, utf16 }
    }

    #[must_use]
    pub const fn byte(self) -> u64 {
        self.byte
    }

    #[must_use]
    pub const fn utf16(self) -> u64 {
        self.utf16
    }
}

/// One nonempty, exact source cut relative to the active physical line.
///
/// Both axes must advance. This matches a nonempty range in valid UTF-8 source
/// and prevents a byte-only or UTF-16-only claim from crossing the writer seam.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineSourceRange {
    start: LineSourcePosition,
    end: LineSourcePosition,
}

impl LineSourceRange {
    #[must_use]
    pub const fn new(start: LineSourcePosition, end: LineSourcePosition) -> Option<Self> {
        if end.byte <= start.byte
            || end.utf16 <= start.utf16
            || end.byte - start.byte < end.utf16 - start.utf16
        {
            return None;
        }
        Some(Self { start, end })
    }

    #[must_use]
    pub const fn start(self) -> LineSourcePosition {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> LineSourcePosition {
        self.end
    }

    #[must_use]
    pub const fn metric(self) -> SourceMetric {
        SourceMetric {
            bytes: self.end.byte - self.start.byte,
            utf16: self.end.utf16 - self.start.utf16,
        }
    }
}

/// Heading syntax whose level has already been certified by the grammar.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HeadingStyle {
    Atx,
    Setext,
}

/// Definitive heading properties stored next to an Enter record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HeadingFacts {
    level: u8,
    style: HeadingStyle,
}

impl HeadingFacts {
    #[must_use]
    pub const fn new(level: u8, style: HeadingStyle) -> Option<Self> {
        let maximum = match style {
            HeadingStyle::Atx => 6,
            HeadingStyle::Setext => 2,
        };
        if level == 0 || level > maximum {
            return None;
        }
        Some(Self { level, style })
    }

    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }

    #[must_use]
    pub const fn style(self) -> HeadingStyle {
        self.style
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FenceCharacter {
    Backtick,
    Tilde,
}

impl FenceCharacter {
    #[must_use]
    pub const fn marker(self) -> u8 {
        match self {
            Self::Backtick => b'`',
            Self::Tilde => b'~',
        }
    }
}

/// Definitive fenced-code properties known at Enter time.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FencedCodeFacts {
    fence: FenceCharacter,
    minimum_closing_length: u64,
    fence_offset_columns: u8,
}

impl FencedCodeFacts {
    #[must_use]
    pub const fn new(
        fence: FenceCharacter,
        minimum_closing_length: u64,
        fence_offset_columns: u8,
    ) -> Option<Self> {
        if minimum_closing_length < 3 || fence_offset_columns > 3 {
            return None;
        }
        Some(Self {
            fence,
            minimum_closing_length,
            fence_offset_columns,
        })
    }

    #[must_use]
    pub const fn fence(self) -> FenceCharacter {
        self.fence
    }

    #[must_use]
    pub const fn minimum_closing_length(self) -> u64 {
        self.minimum_closing_length
    }

    #[must_use]
    pub const fn fence_offset_columns(self) -> u8 {
        self.fence_offset_columns
    }
}

/// Fenced-code truth known only when the block closes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FencedCodeCloseFacts {
    closed: bool,
}

/// The seven block-level HTML start families defined by CommonMark.
///
/// Keeping the grammar's family as a value lets the writer retain exact
/// continuation/termination semantics without retaining an HTML recognizer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HtmlBlockType {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
}

impl HtmlBlockType {
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            4 => Some(Self::Four),
            5 => Some(Self::Five),
            6 => Some(Self::Six),
            7 => Some(Self::Seven),
            _ => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
        }
    }
}

/// Definitive continuation family for one CommonMark HTML block.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HtmlBlockFacts {
    block_type: HtmlBlockType,
}

impl HtmlBlockFacts {
    #[must_use]
    pub const fn new(block_type: HtmlBlockType) -> Self {
        Self { block_type }
    }

    #[must_use]
    pub const fn block_type(self) -> HtmlBlockType {
        self.block_type
    }
}

impl FencedCodeCloseFacts {
    #[must_use]
    pub const fn new(closed: bool) -> Self {
        Self { closed }
    }

    #[must_use]
    pub const fn closed(self) -> bool {
        self.closed
    }
}

/// A parser-certified logical cut captured by the writer's own metric fold.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FencedCodeBoundary {
    InfoEnd,
    LiteralStart,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BulletMarker {
    Hyphen,
    Plus,
    Asterisk,
}

impl BulletMarker {
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Hyphen => b'-',
            Self::Plus => b'+',
            Self::Asterisk => b'*',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ListDelimiter {
    Period,
    Parenthesis,
}

/// The two reachable list-property shapes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ListStyle {
    Bullet {
        marker: BulletMarker,
    },
    Ordered {
        start: u32,
        delimiter: ListDelimiter,
    },
}

/// Definitive list properties known at Enter time.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ListFacts {
    style: ListStyle,
}

impl ListFacts {
    pub const MAX_ORDERED_START: u32 = 999_999_999;

    #[must_use]
    pub const fn bullet(marker: BulletMarker) -> Self {
        Self {
            style: ListStyle::Bullet { marker },
        }
    }

    #[must_use]
    pub const fn ordered(start: u32, delimiter: ListDelimiter) -> Option<Self> {
        if start > Self::MAX_ORDERED_START {
            return None;
        }
        Some(Self {
            style: ListStyle::Ordered { start, delimiter },
        })
    }

    #[must_use]
    pub const fn style(self) -> ListStyle {
        self.style
    }
}

/// Definitive list-item indentation properties known at Enter time.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemFacts {
    marker_offset: u16,
    /// Donor `ListData::padding`: marker width plus the selected following
    /// whitespace, in columns. Ordered markers can therefore reach fourteen.
    padding: u16,
    task_checked: Option<bool>,
}

impl ItemFacts {
    #[must_use]
    pub const fn new(marker_offset: u16, padding: u16) -> Option<Self> {
        Self::new_with_task(marker_offset, padding, None)
    }

    #[must_use]
    pub const fn new_with_task(
        marker_offset: u16,
        padding: u16,
        task_checked: Option<bool>,
    ) -> Option<Self> {
        if marker_offset > 3 || padding < 2 || padding > 14 {
            return None;
        }
        Some(Self {
            marker_offset,
            padding,
            task_checked,
        })
    }

    #[must_use]
    pub const fn marker_offset(self) -> u16 {
        self.marker_offset
    }

    #[must_use]
    pub const fn padding(self) -> u16 {
        self.padding
    }

    #[must_use]
    pub const fn effective_content_indent(self) -> u16 {
        self.marker_offset + self.padding
    }

    #[must_use]
    pub const fn task_checked(self) -> Option<bool> {
        self.task_checked
    }
}

/// Final structural kind for one generic Enter command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BlockKind {
    Document,
    BlockQuote,
    List(ListFacts),
    Item(ItemFacts),
    Paragraph,
    Heading(HeadingFacts),
    IndentedCode,
    FencedCode(FencedCodeFacts),
    HtmlBlock(HtmlBlockFacts),
    ThematicBreak,
}

/// Ephemeral selection of an ancestor on the writer's currently open stack.
///
/// The value is valid only for the command being acknowledged. It is not a
/// persistent block identity or a random-access handle.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StackOwner {
    generations_from_top: u32,
}

impl StackOwner {
    pub const TOP: Self = Self::ancestor(0);
    pub const PARENT_OF_TOP: Self = Self::ancestor(1);

    #[must_use]
    pub const fn ancestor(generations_from_top: u32) -> Self {
        Self {
            generations_from_top,
        }
    }

    #[must_use]
    pub const fn generations_from_top(self) -> u32 {
        self.generations_from_top
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CoveragePart {
    Content,
    ContainerMarker,
    BlockMarker,
    Gap,
    Terminal,
}

/// Logical spaces left after an enclosing prefix consumes part of one tab.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PartialTab {
    logical_target: StackOwner,
    remaining_spaces: u8,
}

impl PartialTab {
    #[must_use]
    pub const fn new(logical_target: StackOwner, remaining_spaces: u8) -> Option<Self> {
        if remaining_spaces == 0 || remaining_spaces > 3 {
            return None;
        }
        Some(Self {
            logical_target,
            remaining_spaces,
        })
    }

    #[must_use]
    pub const fn logical_target(self) -> StackOwner {
        self.logical_target
    }

    #[must_use]
    pub const fn remaining_spaces(self) -> u8 {
        self.remaining_spaces
    }
}

/// Source-to-logical projection selected by the grammar.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LogicalAction {
    Identity,
    CanonicalText,
    PartialTab(PartialTab),
    HiddenUpstream,
    CanonicalNewline,
    None,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineEnding {
    Lf,
    Cr,
    CrLf,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminatorResolution {
    ContinueCanonicalNewline,
    CloseNone,
}

/// The only two levels reachable through Setext promotion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SetextHeadingLevel {
    One,
    Two,
}

impl SetextHeadingLevel {
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

/// A typed retroactive outcome selected by Paragraph grammar.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ParagraphOutcome {
    SetextHeading { level: SetextHeadingLevel },
}

impl ParagraphOutcome {
    #[must_use]
    pub const fn setext_heading(level: u8) -> Option<Self> {
        let level = match level {
            1 => SetextHeadingLevel::One,
            2 => SetextHeadingLevel::Two,
            _ => return None,
        };
        Some(Self::SetextHeading { level })
    }
}

/// Semantic properties which become definitive only at Close time.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FinalFacts {
    #[default]
    None,
    List {
        tight: bool,
    },
    FencedCode(FencedCodeCloseFacts),
}

/// Storage-neutral output protocol for the correspondent block grammar.
///
/// The consumer owns one open stack and acknowledges one command at a time.
/// Structural commands never contain a consumer identity. Source-bearing
/// commands contain exact line-relative byte and UTF-16 cuts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockCommand {
    Enter {
        kind: BlockKind,
    },
    Coverage {
        owner: StackOwner,
        part: CoveragePart,
        source: LineSourceRange,
        logical: LogicalAction,
    },
    StageTerminator {
        source: LineSourceRange,
        ending: LineEnding,
    },
    ResolveTerminator {
        resolution: TerminatorResolution,
    },
    StageBlankGap {
        source: LineSourceRange,
    },
    ResolveBlankGap {
        owner: StackOwner,
    },
    FinalizeParagraph {
        outcome: ParagraphOutcome,
    },
    MarkFencedCodeBoundary {
        boundary: FencedCodeBoundary,
    },
    Close {
        kind: BlockKind,
        final_facts: FinalFacts,
        last_line_blank: bool,
        child: ClosedChild,
    },
    FinishLine {
        physical: SourceMetric,
    },
    FinishDocument,
}
