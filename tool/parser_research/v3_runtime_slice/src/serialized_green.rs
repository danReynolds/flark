//! Packed source-ordered green document candidate.
//!
//! This is the shared-arena port of the serialized-green challenger. Retained
//! state is packed bytes plus arena ownership edges only: typed events exist as
//! bounded builder/query scratch and are never mirrored beside encoded pages.

mod active_leaf_transaction;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Private active-Paragraph cursor proof; writer join remains a later gate.
pub(crate) mod active_paragraph_projection_cursor;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Captured by the next atomic green/checkpoint splice gate.
mod builder_prefix_snapshot;
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Mechanism proof remains unpublishable until the actor consumes matched C.
mod green_journal_suffix_splice;
mod restart_output_query;
#[allow(dead_code)]
// Focused cross-build Setext storage proof; writer/source joins remain separate gates.
pub(crate) mod setext_retained_restart;
mod suffix_adoption;

pub use active_leaf_transaction::*;
#[cfg(feature = "exact-parser")]
#[allow(unused_imports)] // Crate-private handoff for the pending splice integrator.
pub(crate) use builder_prefix_snapshot::*;
#[cfg(feature = "exact-parser")]
#[allow(unused_imports)] // Crate-private handoff for the later actor rendezvous.
pub(crate) use green_journal_suffix_splice::*;
pub use restart_output_query::*;
pub use suffix_adoption::*;

/// Lexical friend token for the parent-selected retained-green output handoff
/// into CandidateWriter. Descendant storage modules can construct it after
/// consuming their complete output wrapper; sibling crate modules can only
/// name it, so raw lease/builder/cut parts are not a forgeable writer seam.
#[cfg(feature = "exact-parser")]
pub(crate) struct ParentSelectedCandidateGreenReadyMint(());

/// Lexical friend token proving retained-prefix provenance was minted while a
/// canonical restart still owned the old manifest and exact retained cut.
/// Construct-specific parser code cannot synthesize this token through a
/// sibling module; Direct, Setext, and future storage normalizations all feed
/// the same monotone builder provenance.
#[cfg(feature = "host-mirror-probe")]
pub(crate) struct HostRetainedPrefixMint(());

use std::fmt;
use std::ops::Range;

use crate::arena::{
    ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession, ArenaBuildTicket,
    ArenaBuildTransaction, ArenaOwnerHandle,
};
use crate::persistent_sequence::{
    BaseLeafReplacement, ResumableSequenceProgress, ResumableSequenceSplice,
    ResumableSequenceSplitProgress, ResumableStreamingSequenceBuilder, SequenceMutationReceipt,
    SequenceNodeKind, SequenceSpec, StreamingSequenceBuilder, replace_leaf_batch_in_transaction,
    retain_sequence_range_in_transaction, sequence_node, splice_owned_root_in_transaction,
};
use crate::record_forest::{ChildSequenceAggregate, ClosedChildAggregate, ContainerFoldSemantics};
use crate::{
    ARENA_PAGE_BYTES, ArenaError, ArenaId, ArenaScopedId, BlockId, CoverageId, GrammarRevision,
    MAX_PACKED_ARENA_CHILDREN, OwnedArenaRef, PageArena, ParseGeneration, SourceRevision,
    SourceRootId,
};

pub(crate) mod source_boundary_resolver;
use source_boundary_resolver::{
    SerializedGreenAdjacentCoverageSide, SerializedGreenCoverageSideObservation,
};

// Version 10 appends an additive logical bytes/UTF-16 metric to every sequence
// summary and manifest. It retains version 9's intrinsic `last_line_blank` Exit
// tags unchanged. Older readers reject the new version and shorter summary,
// while v10 readers reject old roots before interpreting their layout.
const FORMAT_VERSION: u8 = 10;
const LEAF_TAG: u8 = 0xc1;
const BRANCH_TAG: u8 = 0xc2;
const MANIFEST_TAG: u8 = 0xc3;
const PROJECTION_PROGRAM_TAG: u8 = 0xd1;
// Version 2 admits physically anchored Programs whose total logical metric is
// zero (for example, alternating hidden pieces with distinct affinities).
const PROJECTION_PROGRAM_VERSION: u8 = 2;
const SUMMARY_BYTES: usize = 96;
const LEAF_HEADER_BYTES: usize = SUMMARY_BYTES;
const MANIFEST_BYTES: usize = 144;
const MAX_INLINE_FACT_BYTES: usize = 256;
const ENTER_NO_FACTS_TAG: u8 = 0x10;
const ENTER_WITH_FACTS_TAG: u8 = 0x11;
const EXIT_TAG: u8 = 0x20;
const EXIT_LIST_LOOSE_TAG: u8 = 0x28;
const EXIT_LIST_TIGHT_TAG: u8 = 0x30;
const EXIT_FENCED_CODE_TAG: u8 = 0x38;
const EXIT_LAST_LINE_BLANK_TAG: u8 = 0x60;
const EXIT_LIST_LOOSE_LAST_LINE_BLANK_TAG: u8 = 0x68;
const EXIT_LIST_TIGHT_LAST_LINE_BLANK_TAG: u8 = 0x70;
const EXIT_FENCED_CODE_LAST_LINE_BLANK_TAG: u8 = 0x78;
const COVERAGE_TAG: u8 = 0x40;
const COVERAGE_PART_MASK: u8 = 0x07;
const COVERAGE_SAME_METRIC: u8 = 0x08;
const LOGICAL_NONE_TAG: u8 = 0;
const LOGICAL_IDENTITY_TAG: u8 = 1;
const LOGICAL_ATOMIC_TAG: u8 = 2;
const LOGICAL_PROGRAM_TAG: u8 = 3;
const LOGICAL_HIDDEN_UPSTREAM_TAG: u8 = 4;
const LOGICAL_HIDDEN_DOWNSTREAM_TAG: u8 = 5;
const LOGICAL_KIND_MASK: u8 = 0x07;
const LOGICAL_PROJECTION_RESET_AFTER: u8 = 0x08;
const LOGICAL_RESERVED_MASK: u8 = 0xf0;
const ATOMIC_EVENT_TAB_TAG: u8 = 1;
const ATOMIC_EVENT_CRLF_TAG: u8 = 2;
const ATOMIC_EVENT_LONE_CR_TAG: u8 = 3;
const ATOMIC_EVENT_NUL_TAG: u8 = 4;
const EVENT_METRIC_SAME: u8 = 0x80;
const PROGRAM_IDENTITY_TAG: u8 = 0x10;
const PROGRAM_HIDDEN_UPSTREAM_TAG: u8 = 0x20;
const PROGRAM_HIDDEN_DOWNSTREAM_TAG: u8 = 0x24;
const PROGRAM_ATOMIC_TAB_TAG: u8 = 0x30;
const PROGRAM_ATOMIC_CRLF_TAG: u8 = 0x34;
const PROGRAM_ATOMIC_LONE_CR_TAG: u8 = 0x38;
const PROGRAM_ATOMIC_NUL_TAG: u8 = 0x3c;
const PROGRAM_VIRTUAL_LINE_FEED_TAG: u8 = 0x40;
const PROGRAM_SAME_PHYSICAL_METRIC: u8 = 0x01;
const PROGRAM_SAME_LOGICAL_METRIC: u8 = 0x02;
// Fixed header + three maximum-width u64 varints for count/physical metrics
// + two maximum-width u64 varints for logical metrics.
const PROJECTION_PROGRAM_MAX_HEADER_BYTES: usize = 54;
/// Codec-versioned payload limit for one projection Program page.
pub const PROJECTION_PROGRAM_PAGE_BYTES: usize = 4 * 1024;
const _: () = assert!(PROJECTION_PROGRAM_PAGE_BYTES <= ARENA_PAGE_BYTES);
const NO_MINIMUM_CLOSED: i64 = i64::MIN;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SerializedMetric {
    pub bytes: u64,
    pub utf16: u64,
}

impl SerializedMetric {
    fn checked_add(self, other: Self) -> Result<Self, SerializedGreenError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(SerializedGreenError::Overflow("source bytes"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(SerializedGreenError::Overflow("source UTF-16"))?,
        })
    }

    fn checked_add_logical(self, other: Self) -> Result<Self, SerializedGreenError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(SerializedGreenError::Overflow("logical bytes"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(SerializedGreenError::Overflow("logical UTF-16"))?,
        })
    }

    fn checked_sub(self, other: Self) -> Result<Self, SerializedGreenError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_sub(other.bytes)
                .ok_or(SerializedGreenError::Corrupt("source byte metric order"))?,
            utf16: self
                .utf16
                .checked_sub(other.utf16)
                .ok_or(SerializedGreenError::Corrupt("source UTF-16 metric order"))?,
        })
    }

    const fn is_zero(self) -> bool {
        self.bytes == 0 && self.utf16 == 0
    }

    const fn is_partially_zero(self) -> bool {
        (self.bytes == 0) != (self.utf16 == 0)
    }

    const fn coordinate(self, coordinate: GreenCoordinate) -> u64 {
        match coordinate {
            GreenCoordinate::Bytes => self.bytes,
            GreenCoordinate::Utf16 => self.utf16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenCoordinate {
    Bytes,
    Utf16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenAffinity {
    Upstream,
    Downstream,
}

/// Codec-stable kind tag. New kinds require a schema registry update but not a
/// new storage mechanism.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GreenKind(pub u8);

impl GreenKind {
    pub const DOCUMENT: Self = Self(1);
    pub const BLOCK_QUOTE: Self = Self(2);
    pub const LIST: Self = Self(3);
    pub const ITEM: Self = Self(4);
    pub const PARAGRAPH: Self = Self(5);
    pub const INDENTED_CODE: Self = Self(6);
    pub const FENCED_CODE: Self = Self(7);
    pub const HTML_BLOCK: Self = Self(8);
    pub const TABLE: Self = Self(9);
    pub const TABLE_ROW: Self = Self(10);
    pub const TABLE_CELL: Self = Self(11);
    pub const HEADING: Self = Self(12);
    pub const THEMATIC_BREAK: Self = Self(13);

    const fn logical_channel(self) -> Option<LogicalChannel> {
        if matches!(self, Self::PARAGRAPH | Self::HEADING | Self::TABLE_CELL) {
            Some(LogicalChannel::Inline)
        } else if matches!(
            self,
            Self::INDENTED_CODE | Self::FENCED_CODE | Self::HTML_BLOCK
        ) {
            Some(LogicalChannel::Literal)
        } else {
            None
        }
    }

    pub(crate) const fn is_logical_terminal(self) -> bool {
        self.logical_channel().is_some()
    }

    pub(crate) const fn shares_logical_channel(self, other: Self) -> bool {
        matches!(
            (self.logical_channel(), other.logical_channel()),
            (Some(left), Some(right)) if left as u8 == right as u8
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoveragePart(pub u8);

impl CoveragePart {
    pub const CONTENT: Self = Self(1);
    pub const CONTAINER_MARKER: Self = Self(2);
    pub const BLOCK_MARKER: Self = Self(3);
    pub const GAP: Self = Self(4);
    pub const TERMINAL: Self = Self(5);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct FactId(pub u16);

impl FactId {
    pub const LIST: Self = Self(1);
    pub const ITEM: Self = Self(2);
    pub const HEADING: Self = Self(3);
    pub const CODE: Self = Self(4);
    pub const HTML: Self = Self(5);
    pub const TABLE: Self = Self(6);
    pub const TABLE_ALIGNMENTS: Self = Self(7);
    pub const TABLE_ROW: Self = Self(8);
    pub const TABLE_CELL: Self = Self(9);
    pub const THEMATIC_BREAK: Self = Self(10);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactField {
    pub id: FactId,
    pub critical: bool,
    pub value: Vec<u8>,
}

impl FactField {
    #[must_use]
    pub fn critical(id: FactId, value: impl Into<Vec<u8>>) -> Self {
        Self {
            id,
            critical: true,
            value: value.into(),
        }
    }

    #[must_use]
    pub fn optional(id: FactId, value: impl Into<Vec<u8>>) -> Self {
        Self {
            id,
            critical: false,
            value: value.into(),
        }
    }
}

/// Canonical bounded facts physically owned by one Enter. Unbounded semantic
/// vectors use a typed external fact root in the production codec; they may
/// not be smuggled into this envelope or copied from source text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactsEnvelope {
    pub schema_version: u16,
    pub fields: Vec<FactField>,
}

impl FactsEnvelope {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            schema_version: 1,
            fields: Vec::new(),
        }
    }

    pub fn new(fields: Vec<FactField>) -> Result<Self, SerializedGreenError> {
        let value = Self {
            schema_version: 1,
            fields,
        };
        value.validate_canonical()?;
        Ok(value)
    }

    fn validate_canonical(&self) -> Result<(), SerializedGreenError> {
        if self.schema_version != 1 {
            return Err(SerializedGreenError::Invalid(
                "unsupported facts schema version",
            ));
        }
        let mut previous = None;
        for field in &self.fields {
            if field.id.0 == 0 {
                return Err(SerializedGreenError::Invalid("fact ID must be nonzero"));
            }
            if previous.is_some_and(|previous| field.id <= previous) {
                return Err(SerializedGreenError::Invalid(
                    "facts must be strictly ordered and unique",
                ));
            }
            previous = Some(field.id);
        }
        let encoded = encode_facts(self)?;
        if encoded.len() > MAX_INLINE_FACT_BYTES {
            return Err(SerializedGreenError::Invalid(
                "inline facts exceed bounded envelope",
            ));
        }
        Ok(())
    }
}

/// Normalized list family. Display-only list facts are stored on the List
/// Enter; close-derived tightness is deliberately absent from this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenListStyle {
    Bullet {
        marker: GreenListBullet,
    },
    Ordered {
        start: u32,
        delimiter: GreenListDelimiter,
    },
}

/// Canonical ordered-list delimiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenListDelimiter {
    Period,
    Parenthesis,
}

impl GreenListDelimiter {
    #[must_use]
    pub const fn marker(self) -> u8 {
        match self {
            Self::Period => b'.',
            Self::Parenthesis => b')',
        }
    }
}

impl TryFrom<u8> for GreenListDelimiter {
    type Error = SerializedGreenError;

    fn try_from(marker: u8) -> Result<Self, Self::Error> {
        match marker {
            b'.' => Ok(Self::Period),
            b')' => Ok(Self::Parenthesis),
            _ => Err(SerializedGreenError::Invalid(
                "ordered List delimiter must be '.' or ')'",
            )),
        }
    }
}

/// Canonical bullet-list marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenListBullet {
    Asterisk,
    Plus,
    Dash,
}

impl GreenListBullet {
    #[must_use]
    pub const fn marker(self) -> u8 {
        match self {
            Self::Asterisk => b'*',
            Self::Plus => b'+',
            Self::Dash => b'-',
        }
    }
}

impl TryFrom<u8> for GreenListBullet {
    type Error = SerializedGreenError;

    fn try_from(marker: u8) -> Result<Self, Self::Error> {
        match marker {
            b'*' => Ok(Self::Asterisk),
            b'+' => Ok(Self::Plus),
            b'-' => Ok(Self::Dash),
            _ => Err(SerializedGreenError::Invalid(
                "List bullet marker must be '*', '+', or '-'",
            )),
        }
    }
}

/// Canonical List Enter facts. Its typed style carries exactly the fields valid
/// for that style, so anonymous or contradictory combinations cannot escape the
/// parser/storage seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenListOpenFacts {
    style: GreenListStyle,
}

impl GreenListOpenFacts {
    const PAYLOAD_BYTES: usize = 8;
    const BULLET_TAG: u8 = 1;
    const ORDERED_TAG: u8 = 2;
    const MAX_ORDERED_START: u32 = 999_999_999;

    #[must_use]
    pub const fn bullet(bullet: GreenListBullet) -> Self {
        Self {
            style: GreenListStyle::Bullet { marker: bullet },
        }
    }

    pub fn ordered(
        start: u32,
        delimiter: GreenListDelimiter,
    ) -> Result<Self, SerializedGreenError> {
        if start > Self::MAX_ORDERED_START {
            return Err(SerializedGreenError::Invalid(
                "ordered List start must fit the CommonMark nine-digit marker",
            ));
        }
        Ok(Self {
            style: GreenListStyle::Ordered { start, delimiter },
        })
    }

    #[must_use]
    pub const fn style(self) -> GreenListStyle {
        self.style
    }

    #[must_use]
    pub const fn start(self) -> Option<u32> {
        match self.style {
            GreenListStyle::Bullet { .. } => None,
            GreenListStyle::Ordered { start, .. } => Some(start),
        }
    }

    #[must_use]
    pub const fn delimiter(self) -> Option<GreenListDelimiter> {
        match self.style {
            GreenListStyle::Bullet { .. } => None,
            GreenListStyle::Ordered { delimiter, .. } => Some(delimiter),
        }
    }

    #[must_use]
    pub const fn bullet_marker(self) -> Option<GreenListBullet> {
        match self.style {
            GreenListStyle::Bullet { marker } => Some(marker),
            GreenListStyle::Ordered { .. } => None,
        }
    }

    /// Converts typed parser output into the one canonical storage envelope.
    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(FactId::LIST, self.encode_payload())],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::LIST, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::LIST)
            .ok_or(SerializedGreenError::Invalid(
                "required List fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        let mut payload = [0_u8; Self::PAYLOAD_BYTES];
        match self.style {
            GreenListStyle::Bullet { marker } => {
                payload[0] = Self::BULLET_TAG;
                payload[1] = marker.marker();
                payload[4..8].copy_from_slice(&1_u32.to_le_bytes());
            }
            GreenListStyle::Ordered { start, delimiter } => {
                payload[0] = Self::ORDERED_TAG;
                payload[2] = delimiter.marker();
                payload[4..8].copy_from_slice(&start.to_le_bytes());
            }
        }
        payload
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        if payload.len() != Self::PAYLOAD_BYTES || payload[3] != 0 {
            return Err(SerializedGreenError::Invalid(
                "List fact payload has invalid length or reserved byte",
            ));
        }
        let start = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        match payload[0] {
            Self::BULLET_TAG if payload[2] == 0 && start == 1 => {
                Ok(Self::bullet(GreenListBullet::try_from(payload[1])?))
            }
            Self::ORDERED_TAG
                if payload[1] == 0
                    && start <= Self::MAX_ORDERED_START
                    && matches!(payload[2], b'.' | b')') =>
            {
                Self::ordered(start, GreenListDelimiter::try_from(payload[2])?)
            }
            _ => Err(SerializedGreenError::Invalid(
                "List fact payload is not canonical",
            )),
        }
    }
}

/// Canonical Item Enter facts. Values are columns after tab expansion, not raw
/// source-byte offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenItemOpenFacts {
    marker_offset_columns: u16,
    padding_columns: u16,
}

impl GreenItemOpenFacts {
    const PAYLOAD_BYTES: usize = 4;

    pub fn new(
        marker_offset_columns: u16,
        padding_columns: u16,
    ) -> Result<Self, SerializedGreenError> {
        if marker_offset_columns > 3 {
            return Err(SerializedGreenError::Invalid(
                "Item marker offset must be at most three columns",
            ));
        }
        if !(2..=14).contains(&padding_columns) {
            return Err(SerializedGreenError::Invalid(
                "Item padding must include a valid marker and selected whitespace",
            ));
        }
        Ok(Self {
            marker_offset_columns,
            padding_columns,
        })
    }

    #[must_use]
    pub const fn marker_offset_columns(self) -> u16 {
        self.marker_offset_columns
    }

    #[must_use]
    pub const fn padding_columns(self) -> u16 {
        self.padding_columns
    }

    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(FactId::ITEM, self.encode_payload())],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::ITEM, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::ITEM)
            .ok_or(SerializedGreenError::Invalid(
                "required Item fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        let mut payload = [0_u8; Self::PAYLOAD_BYTES];
        payload[0..2].copy_from_slice(&self.marker_offset_columns.to_le_bytes());
        payload[2..4].copy_from_slice(&self.padding_columns.to_le_bytes());
        payload
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        if payload.len() != Self::PAYLOAD_BYTES {
            return Err(SerializedGreenError::Invalid(
                "Item fact payload has invalid length",
            ));
        }
        Self::new(
            u16::from_le_bytes([payload[0], payload[1]]),
            u16::from_le_bytes([payload[2], payload[3]]),
        )
    }
}

/// The source form that established a Heading block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenHeadingStyle {
    Atx,
    Setext,
}

/// Canonical Heading Enter facts. Keeping the style typed is important for
/// Setext promotion: storage records the parser's decision without retaining
/// a second inference path over source bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenHeadingOpenFacts {
    level: u8,
    style: GreenHeadingStyle,
}

impl GreenHeadingOpenFacts {
    const PAYLOAD_BYTES: usize = 2;
    const ATX_TAG: u8 = 0;
    const SETEXT_TAG: u8 = 1;

    pub fn new(level: u8, style: GreenHeadingStyle) -> Result<Self, SerializedGreenError> {
        if !(1..=6).contains(&level) {
            return Err(SerializedGreenError::Invalid(
                "Heading level must be between one and six",
            ));
        }
        if style == GreenHeadingStyle::Setext && level > 2 {
            return Err(SerializedGreenError::Invalid(
                "Setext Heading level must be one or two",
            ));
        }
        Ok(Self { level, style })
    }

    pub fn atx(level: u8) -> Result<Self, SerializedGreenError> {
        Self::new(level, GreenHeadingStyle::Atx)
    }

    pub fn setext(level: u8) -> Result<Self, SerializedGreenError> {
        Self::new(level, GreenHeadingStyle::Setext)
    }

    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }

    #[must_use]
    pub const fn style(self) -> GreenHeadingStyle {
        self.style
    }

    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(FactId::HEADING, self.encode_payload())],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::HEADING, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::HEADING)
            .ok_or(SerializedGreenError::Invalid(
                "required Heading fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        [
            self.level,
            match self.style {
                GreenHeadingStyle::Atx => Self::ATX_TAG,
                GreenHeadingStyle::Setext => Self::SETEXT_TAG,
            },
        ]
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        if payload.len() != Self::PAYLOAD_BYTES {
            return Err(SerializedGreenError::Invalid(
                "Heading fact payload has invalid length",
            ));
        }
        let style = match payload[1] {
            Self::ATX_TAG => GreenHeadingStyle::Atx,
            Self::SETEXT_TAG => GreenHeadingStyle::Setext,
            _ => {
                return Err(SerializedGreenError::Invalid(
                    "Heading fact payload has an unknown style",
                ));
            }
        };
        Self::new(payload[0], style)
    }
}

/// Canonical Table Enter facts.
///
/// Only the exact header width belongs inline. Per-column alignment is stored
/// once on each resumably emitted header-cell Enter, never copied into the
/// Table envelope or repeated on body cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenTableOpenFacts {
    column_count: u32,
}

impl GreenTableOpenFacts {
    const PAYLOAD_BYTES: usize = 4;

    pub fn new(column_count: u32) -> Result<Self, SerializedGreenError> {
        if column_count == 0 {
            return Err(SerializedGreenError::Invalid(
                "Table column count must be nonzero",
            ));
        }
        Ok(Self { column_count })
    }

    #[must_use]
    pub const fn column_count(self) -> u32 {
        self.column_count
    }

    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(FactId::TABLE, self.encode_payload())],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::TABLE, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::TABLE)
            .ok_or(SerializedGreenError::Invalid(
                "required Table fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        self.column_count.to_le_bytes()
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        if payload.len() != Self::PAYLOAD_BYTES {
            return Err(SerializedGreenError::Invalid(
                "Table fact payload has invalid length",
            ));
        }
        Self::new(u32::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3],
        ]))
    }
}

/// Canonical TableRow Enter facts. Header/body is intrinsic row structure;
/// only header cells carry one bounded alignment fact each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenTableRowOpenFacts {
    header: bool,
}

impl GreenTableRowOpenFacts {
    const PAYLOAD_BYTES: usize = 1;

    #[must_use]
    pub const fn header() -> Self {
        Self { header: true }
    }

    #[must_use]
    pub const fn body() -> Self {
        Self { header: false }
    }

    #[must_use]
    pub const fn is_header(self) -> bool {
        self.header
    }

    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(
                FactId::TABLE_ROW,
                self.encode_payload(),
            )],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::TABLE_ROW, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::TABLE_ROW)
            .ok_or(SerializedGreenError::Invalid(
                "required TableRow fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        [u8::from(self.header)]
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        match payload {
            [0] => Ok(Self::body()),
            [1] => Ok(Self::header()),
            _ => Err(SerializedGreenError::Invalid(
                "TableRow fact payload is not canonical",
            )),
        }
    }
}

/// One GFM table-column alignment. `Unspecified` is the delimiter form with no
/// leading or trailing colon; it is still an explicit header-cell alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenTableAlignment {
    Unspecified,
    Left,
    Center,
    Right,
}

/// Canonical TableCell Enter facts. The zero-based index is a full `u32`, so
/// the storage schema does not inherit the current bounded scanner's 65,535
/// cell implementation limit. Header cells carry their alignment exactly
/// once; body cells inherit it through their column index. Parent validation
/// checks both roles and the index bound when the canonical fragment seals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenTableCellOpenFacts {
    column_index: u32,
    header_alignment: Option<GreenTableAlignment>,
}

impl GreenTableCellOpenFacts {
    const PAYLOAD_BYTES: usize = 5;
    const BODY_TAG: u8 = 0;
    const HEADER_UNSPECIFIED_TAG: u8 = 1;
    const HEADER_LEFT_TAG: u8 = 2;
    const HEADER_CENTER_TAG: u8 = 3;
    const HEADER_RIGHT_TAG: u8 = 4;

    #[must_use]
    pub const fn header(column_index: u32, alignment: GreenTableAlignment) -> Self {
        Self {
            column_index,
            header_alignment: Some(alignment),
        }
    }

    #[must_use]
    pub const fn body(column_index: u32) -> Self {
        Self {
            column_index,
            header_alignment: None,
        }
    }

    #[must_use]
    pub const fn column_index(self) -> u32 {
        self.column_index
    }

    #[must_use]
    pub const fn header_alignment(self) -> Option<GreenTableAlignment> {
        self.header_alignment
    }

    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(
                FactId::TABLE_CELL,
                self.encode_payload(),
            )],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::TABLE_CELL, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::TABLE_CELL)
            .ok_or(SerializedGreenError::Invalid(
                "required TableCell fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        let mut payload = [0_u8; Self::PAYLOAD_BYTES];
        payload[..4].copy_from_slice(&self.column_index.to_le_bytes());
        payload[4] = match self.header_alignment {
            None => Self::BODY_TAG,
            Some(GreenTableAlignment::Unspecified) => Self::HEADER_UNSPECIFIED_TAG,
            Some(GreenTableAlignment::Left) => Self::HEADER_LEFT_TAG,
            Some(GreenTableAlignment::Center) => Self::HEADER_CENTER_TAG,
            Some(GreenTableAlignment::Right) => Self::HEADER_RIGHT_TAG,
        };
        payload
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        if payload.len() != Self::PAYLOAD_BYTES {
            return Err(SerializedGreenError::Invalid(
                "TableCell fact payload has invalid length",
            ));
        }
        let column_index = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        match payload[4] {
            Self::BODY_TAG => Ok(Self::body(column_index)),
            Self::HEADER_UNSPECIFIED_TAG => {
                Ok(Self::header(column_index, GreenTableAlignment::Unspecified))
            }
            Self::HEADER_LEFT_TAG => Ok(Self::header(column_index, GreenTableAlignment::Left)),
            Self::HEADER_CENTER_TAG => Ok(Self::header(column_index, GreenTableAlignment::Center)),
            Self::HEADER_RIGHT_TAG => Ok(Self::header(column_index, GreenTableAlignment::Right)),
            _ => Err(SerializedGreenError::Invalid(
                "TableCell fact payload has an unknown role or alignment",
            )),
        }
    }
}

/// Delimiter character used by one fenced code block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenFenceCharacter {
    Backtick,
    Tilde,
}

impl GreenFenceCharacter {
    #[must_use]
    pub const fn marker(self) -> u8 {
        match self {
            Self::Backtick => b'`',
            Self::Tilde => b'~',
        }
    }
}

impl TryFrom<u8> for GreenFenceCharacter {
    type Error = SerializedGreenError;

    fn try_from(marker: u8) -> Result<Self, Self::Error> {
        match marker {
            b'`' => Ok(Self::Backtick),
            b'~' => Ok(Self::Tilde),
            _ => Err(SerializedGreenError::Invalid(
                "FencedCode fence must be a backtick or tilde",
            )),
        }
    }
}

/// Canonical `FencedCode` Enter facts. The fence length is deliberately `u64`:
/// valid giant-line runs must not be capped to a display-oriented integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenFencedCodeOpenFacts {
    fence: GreenFenceCharacter,
    minimum_closing_length: u64,
    fence_offset_columns: u8,
}

impl GreenFencedCodeOpenFacts {
    const PAYLOAD_BYTES: usize = 10;

    pub fn new(
        fence: GreenFenceCharacter,
        minimum_closing_length: u64,
        fence_offset_columns: u8,
    ) -> Result<Self, SerializedGreenError> {
        if minimum_closing_length < 3 {
            return Err(SerializedGreenError::Invalid(
                "FencedCode fence length must be at least three",
            ));
        }
        if fence_offset_columns > 3 {
            return Err(SerializedGreenError::Invalid(
                "FencedCode fence offset must be at most three columns",
            ));
        }
        Ok(Self {
            fence,
            minimum_closing_length,
            fence_offset_columns,
        })
    }

    #[must_use]
    pub const fn fence(self) -> GreenFenceCharacter {
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

    #[must_use]
    pub fn into_envelope(self) -> FactsEnvelope {
        FactsEnvelope {
            schema_version: 1,
            fields: vec![FactField::critical(FactId::CODE, self.encode_payload())],
        }
    }

    pub fn try_from_envelope(facts: &FactsEnvelope) -> Result<Self, SerializedGreenError> {
        validate_facts_for_kind(GreenKind::FENCED_CODE, facts)?;
        let field = facts
            .fields
            .iter()
            .find(|field| field.id == FactId::CODE)
            .ok_or(SerializedGreenError::Invalid(
                "required FencedCode fact is missing",
            ))?;
        Self::decode_payload(&field.value)
    }

    fn encode_payload(self) -> [u8; Self::PAYLOAD_BYTES] {
        let mut payload = [0_u8; Self::PAYLOAD_BYTES];
        payload[0] = self.fence.marker();
        payload[1] = self.fence_offset_columns;
        payload[2..].copy_from_slice(&self.minimum_closing_length.to_le_bytes());
        payload
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, SerializedGreenError> {
        if payload.len() != Self::PAYLOAD_BYTES {
            return Err(SerializedGreenError::Invalid(
                "FencedCode fact payload has invalid length",
            ));
        }
        Self::new(
            GreenFenceCharacter::try_from(payload[0])?,
            u64::from_le_bytes([
                payload[2], payload[3], payload[4], payload[5], payload[6], payload[7], payload[8],
                payload[9],
            ]),
            payload[1],
        )
    }
}

/// One bounded logical slice relative to the owning terminal's logical stream.
/// Both coordinate ranges must describe the same UTF-8 boundary interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenRelativeLogicalSlice {
    start: SerializedMetric,
    end: SerializedMetric,
}

impl GreenRelativeLogicalSlice {
    pub fn new(bytes: Range<u64>, utf16: Range<u64>) -> Result<Self, SerializedGreenError> {
        let start = SerializedMetric {
            bytes: bytes.start,
            utf16: utf16.start,
        };
        let end = SerializedMetric {
            bytes: bytes.end,
            utf16: utf16.end,
        };
        Self::from_metrics(start, end)
    }

    fn from_metrics(
        start: SerializedMetric,
        end: SerializedMetric,
    ) -> Result<Self, SerializedGreenError> {
        if start.bytes > end.bytes || start.utf16 > end.utf16 {
            return Err(SerializedGreenError::Invalid(
                "relative logical slice is reversed",
            ));
        }
        if start.bytes < start.utf16 || end.bytes < end.utf16 {
            return Err(SerializedGreenError::Invalid(
                "relative logical slice has impossible UTF-8/UTF-16 offsets",
            ));
        }
        let byte_length = end.bytes - start.bytes;
        let utf16_length = end.utf16 - start.utf16;
        if (byte_length == 0) != (utf16_length == 0) || byte_length < utf16_length {
            return Err(SerializedGreenError::Invalid(
                "relative logical slice has incompatible byte and UTF-16 bounds",
            ));
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub const fn start(self) -> SerializedMetric {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> SerializedMetric {
        self.end
    }

    #[must_use]
    pub const fn bytes(self) -> Range<u64> {
        self.start.bytes..self.end.bytes
    }

    #[must_use]
    pub const fn utf16(self) -> Range<u64> {
        self.start.utf16..self.end.utf16
    }
}

/// Semantic `FencedCode` facts that become definitive only at close time.
/// Slices are relative offsets, so retained storage remains constant-size even
/// for giant info strings or literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenFencedCodeCloseFacts {
    closed: bool,
    info: GreenRelativeLogicalSlice,
    literal: GreenRelativeLogicalSlice,
}

impl GreenFencedCodeCloseFacts {
    pub fn new(
        closed: bool,
        info: GreenRelativeLogicalSlice,
        literal: GreenRelativeLogicalSlice,
    ) -> Result<Self, SerializedGreenError> {
        if info.end.bytes > literal.start.bytes || info.end.utf16 > literal.start.utf16 {
            return Err(SerializedGreenError::Invalid(
                "FencedCode info slice must precede its literal slice",
            ));
        }
        let gap_bytes = literal.start.bytes - info.end.bytes;
        let gap_utf16 = literal.start.utf16 - info.end.utf16;
        if (gap_bytes == 0) != (gap_utf16 == 0) || gap_bytes < gap_utf16 {
            return Err(SerializedGreenError::Invalid(
                "FencedCode slice gap has incompatible byte and UTF-16 bounds",
            ));
        }
        Ok(Self {
            closed,
            info,
            literal,
        })
    }

    #[must_use]
    pub const fn closed(self) -> bool {
        self.closed
    }

    #[must_use]
    pub const fn info(self) -> GreenRelativeLogicalSlice {
        self.info
    }

    #[must_use]
    pub const fn literal(self) -> GreenRelativeLogicalSlice {
        self.literal
    }

    fn encode_payload(self, output: &mut Vec<u8>) {
        output.push(u8::from(self.closed));
        for metric in [
            self.info.start,
            self.info.end,
            self.literal.start,
            self.literal.end,
        ] {
            push_varint(metric.bytes, output);
            push_varint(metric.utf16, output);
        }
    }

    fn decode_payload(decoder: &mut Decoder<'_>) -> Result<Self, SerializedGreenError> {
        let closed = match decoder.u8()? {
            0 => false,
            1 => true,
            _ => {
                return Err(SerializedGreenError::Corrupt(
                    "invalid FencedCode closed flag",
                ));
            }
        };
        let mut metric = || -> Result<SerializedMetric, SerializedGreenError> {
            Ok(SerializedMetric {
                bytes: decoder.varint()?,
                utf16: decoder.varint()?,
            })
        };
        let info = GreenRelativeLogicalSlice::from_metrics(metric()?, metric()?)
            .map_err(|_| SerializedGreenError::Corrupt("invalid FencedCode info slice"))?;
        let literal = GreenRelativeLogicalSlice::from_metrics(metric()?, metric()?)
            .map_err(|_| SerializedGreenError::Corrupt("invalid FencedCode literal slice"))?;
        Self::new(closed, info, literal)
            .map_err(|_| SerializedGreenError::Corrupt("invalid FencedCode slice ordering"))
    }
}

/// Semantic facts that become definitive only when a block closes. These are
/// independent of `ClosedChildAggregate`, which remains the structural fold
/// contribution consumed by the parent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GreenCloseFacts {
    #[default]
    None,
    List {
        tight: bool,
    },
    FencedCode(GreenFencedCodeCloseFacts),
}

impl GreenCloseFacts {
    pub(crate) fn validate_for_kind(self, kind: GreenKind) -> Result<(), SerializedGreenError> {
        match (kind, self) {
            (GreenKind::LIST, Self::None) => Err(SerializedGreenError::Invalid(
                "List Exit is missing its close-time tightness fact",
            )),
            (GreenKind::FENCED_CODE, Self::None) => Err(SerializedGreenError::Invalid(
                "FencedCode Exit is missing its close-time projection facts",
            )),
            (GreenKind::LIST, Self::List { .. })
            | (GreenKind::FENCED_CODE, Self::FencedCode(_))
            | (_, Self::None) => Ok(()),
            (_, Self::List { .. }) => Err(SerializedGreenError::Invalid(
                "List close-time facts require a List binding",
            )),
            (_, Self::FencedCode(_)) => Err(SerializedGreenError::Invalid(
                "FencedCode close-time facts require a FencedCode binding",
            )),
        }
    }
}

/// One terminal logical-input channel. This is deliberately separate from
/// physical ownership: an ancestor may own an indivisible source byte whose
/// residual transform contributes to an open descendant terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalChannel {
    Inline = 1,
    Literal = 2,
}

/// Typed, indivisible source-to-logical replacement. No consumer may guess an
/// interior coordinate; only the two boundaries map exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicProjectionKind {
    TabToSpaces { spaces: u8 },
    CrLfToLf,
    LoneCrToLf,
    NulToReplacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtomicProjection {
    pub kind: AtomicProjectionKind,
    pub logical_metric: SerializedMetric,
}

impl AtomicProjection {
    pub fn new(
        kind: AtomicProjectionKind,
        logical_metric: SerializedMetric,
    ) -> Result<Self, SerializedGreenError> {
        if logical_metric.is_zero() || logical_metric.is_partially_zero() {
            return Err(SerializedGreenError::Invalid(
                "atomic logical metric must be nonzero in both coordinates",
            ));
        }
        let value = Self {
            kind,
            logical_metric,
        };
        value.validate_kind()?;
        Ok(value)
    }

    pub fn tab_to_spaces(spaces: u8) -> Result<Self, SerializedGreenError> {
        Self::new(
            AtomicProjectionKind::TabToSpaces { spaces },
            SerializedMetric {
                bytes: u64::from(spaces),
                utf16: u64::from(spaces),
            },
        )
    }

    #[must_use]
    pub fn crlf_to_lf() -> Self {
        Self {
            kind: AtomicProjectionKind::CrLfToLf,
            logical_metric: SerializedMetric { bytes: 1, utf16: 1 },
        }
    }

    #[must_use]
    pub fn lone_cr_to_lf() -> Self {
        Self {
            kind: AtomicProjectionKind::LoneCrToLf,
            logical_metric: SerializedMetric { bytes: 1, utf16: 1 },
        }
    }

    /// `CommonMark` replaces one embedded NUL source byte with U+FFFD.
    #[must_use]
    pub const fn nul_to_replacement() -> Self {
        Self {
            kind: AtomicProjectionKind::NulToReplacement,
            logical_metric: SerializedMetric { bytes: 3, utf16: 1 },
        }
    }

    fn validate_kind(self) -> Result<(), SerializedGreenError> {
        let valid = match self.kind {
            AtomicProjectionKind::TabToSpaces { spaces } => {
                (1..=4).contains(&spaces)
                    && self.logical_metric
                        == (SerializedMetric {
                            bytes: u64::from(spaces),
                            utf16: u64::from(spaces),
                        })
            }
            AtomicProjectionKind::CrLfToLf | AtomicProjectionKind::LoneCrToLf => {
                self.logical_metric == (SerializedMetric { bytes: 1, utf16: 1 })
            }
            AtomicProjectionKind::NulToReplacement => {
                self.logical_metric == (SerializedMetric { bytes: 3, utf16: 1 })
            }
        };
        if valid {
            Ok(())
        } else {
            Err(SerializedGreenError::Invalid(
                "atomic transform metric does not match its typed output",
            ))
        }
    }

    fn validate_physical(self, physical: SerializedMetric) -> Result<(), SerializedGreenError> {
        let valid = match self.kind {
            AtomicProjectionKind::TabToSpaces { .. }
            | AtomicProjectionKind::LoneCrToLf
            | AtomicProjectionKind::NulToReplacement => {
                physical == (SerializedMetric { bytes: 1, utf16: 1 })
            }
            AtomicProjectionKind::CrLfToLf => physical == (SerializedMetric { bytes: 2, utf16: 2 }),
        };
        if valid {
            Ok(())
        } else {
            Err(SerializedGreenError::Invalid(
                "atomic transform physical metric does not match its typed input",
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualProjectionKind {
    LineFeed,
}

impl VirtualProjectionKind {
    const fn logical_metric(self) -> SerializedMetric {
        match self {
            Self::LineFeed => SerializedMetric { bytes: 1, utf16: 1 },
        }
    }
}

/// One relative piece in a compound projection program. Physical and logical
/// positions are implicit prefix sums; no source text or absolute coordinate
/// can enter the retained program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionPiece {
    Identity {
        metric: SerializedMetric,
    },
    Hidden {
        metric: SerializedMetric,
        affinity: GreenAffinity,
    },
    Atomic {
        physical_metric: SerializedMetric,
        projection: AtomicProjection,
    },
    Virtual {
        kind: VirtualProjectionKind,
    },
}

impl ProjectionPiece {
    fn metrics(&self) -> (SerializedMetric, SerializedMetric) {
        match self {
            Self::Identity { metric } => (*metric, *metric),
            Self::Hidden { metric, .. } => (*metric, SerializedMetric::default()),
            Self::Atomic {
                physical_metric,
                projection,
            } => (*physical_metric, projection.logical_metric),
            Self::Virtual { kind } => (SerializedMetric::default(), kind.logical_metric()),
        }
    }

    fn validate(&self) -> Result<(), SerializedGreenError> {
        match self {
            Self::Identity { metric } | Self::Hidden { metric, .. } => {
                if metric.is_zero() || metric.is_partially_zero() {
                    return Err(SerializedGreenError::Invalid(
                        "identity/hidden program metric must be nonzero in both coordinates",
                    ));
                }
            }
            Self::Atomic {
                physical_metric,
                projection,
            } => {
                if physical_metric.is_zero() || physical_metric.is_partially_zero() {
                    return Err(SerializedGreenError::Invalid(
                        "atomic physical metric must be nonzero in both coordinates",
                    ));
                }
                projection.validate_kind()?;
                projection.validate_physical(*physical_metric)?;
            }
            Self::Virtual { .. } => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionProgram {
    payload: Vec<u8>,
    piece_count: usize,
    physical_metric: SerializedMetric,
    logical_metric: SerializedMetric,
}

impl ProjectionProgram {
    /// Proof/test convenience for a projection already known to fit one page.
    /// Production parser output should use [`ProjectionProgramChunker`] so a
    /// dense transform never has to assemble an unbounded piece vector.
    pub fn new(pieces: Vec<ProjectionPiece>) -> Result<Self, SerializedGreenError> {
        if pieces.is_empty() {
            return Err(SerializedGreenError::Invalid("empty projection program"));
        }
        let mut encoder = ProjectionPageEncoder::default();
        for piece in pieces {
            if !encoder.try_push(piece)? {
                return Err(SerializedGreenError::Invalid(
                    "projection program exceeds one bounded arena page",
                ));
            }
        }
        encoder.into_program()
    }

    #[must_use]
    pub const fn piece_count(&self) -> usize {
        self.piece_count
    }

    #[must_use]
    pub const fn physical_metric(&self) -> SerializedMetric {
        self.physical_metric
    }

    #[must_use]
    pub const fn logical_metric(&self) -> SerializedMetric {
        self.logical_metric
    }

    #[must_use]
    pub fn encoded_bytes(&self) -> usize {
        self.payload.len()
    }
}

/// One page-bounded logical projection emitted by the streaming chunker.
/// The caller immediately binds its physical metric to the next exact source
/// capability and mints that run's identity under the active build epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionChunk {
    /// Stable source-order fragment number within this one envelope. Identity
    /// minting happens later in the source-bound composer, never here.
    pub fragment_ordinal: u64,
    pub physical_metric: SerializedMetric,
    pub logical_contribution: LogicalContribution,
}

/// Allocation receipt for a streaming projection producer. Buffered payload
/// excludes the fixed-size encoder state and is hard-capped to one arena page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectionChunkerReceipt {
    pub pieces_accepted: u64,
    pub chunks_emitted: u64,
    pub maximum_buffered_payload_bytes: usize,
    /// Largest capacity of the chunker's single reusable encoded-body buffer.
    /// This reports allocator reality, not only the compact sealed-page size.
    pub maximum_buffer_capacity_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionChunkerFinish {
    ChunkPending,
    Complete(ProjectionChunkerReceipt),
}

#[derive(Clone, Debug)]
struct ProjectionTailPacket {
    leading_virtual: Option<VirtualProjectionKind>,
    physical: ProjectionPiece,
    trailing_virtual: Option<VirtualProjectionKind>,
}

impl ProjectionTailPacket {
    fn fixed_pieces(&self) -> [Option<ProjectionPiece>; 3] {
        [
            self.leading_virtual
                .map(|kind| ProjectionPiece::Virtual { kind }),
            Some(self.physical.clone()),
            self.trailing_virtual
                .map(|kind| ProjectionPiece::Virtual { kind }),
        ]
    }
}

/// Deterministically coalesces and splits an arbitrary projection stream into
/// independently source-anchored runs. `push` accepts exactly one piece and
/// emits at most one complete chunk, so callers can yield between every input
/// and output without retaining a document-sized action vector.
#[derive(Debug)]
pub struct ProjectionProgramChunker {
    encoder: ProjectionPageEncoder,
    expected_physical: SerializedMetric,
    source_bound: bool,
    accepted_physical: SerializedMetric,
    tail: Option<ProjectionTailPacket>,
    pending_virtual: Option<VirtualProjectionKind>,
    finishing: bool,
    complete: bool,
    receipt: ProjectionChunkerReceipt,
}

impl ProjectionProgramChunker {
    /// A checkpoint may close the current source envelope only when no
    /// zero-width projection is still waiting to choose a physical anchor on
    /// its right. Finishing with such a pending Virtual would make checkpoint
    /// placement observable by attaching it to the preceding run.
    pub(crate) const fn checkpoint_cut_is_affinity_neutral(&self) -> bool {
        !self.finishing && !self.complete && self.pending_virtual.is_none()
    }

    pub fn new(expected_physical: SerializedMetric) -> Result<Self, SerializedGreenError> {
        if expected_physical.is_zero() || expected_physical.is_partially_zero() {
            return Err(SerializedGreenError::Invalid(
                "projection envelope metric must be nonzero in both coordinates",
            ));
        }
        let mut chunker = Self {
            encoder: ProjectionPageEncoder::default(),
            expected_physical,
            source_bound: false,
            accepted_physical: SerializedMetric::default(),
            tail: None,
            pending_virtual: None,
            finishing: false,
            complete: false,
            receipt: ProjectionChunkerReceipt::default(),
        };
        chunker.observe_buffer();
        Ok(chunker)
    }

    /// Candidate-composer mode: the expected envelope metric is closed from
    /// the exact admitted `ConsumedSourcePiece`s, never supplied as a scalar
    /// by parser or codec callers.
    pub(crate) fn new_source_bound() -> Self {
        let mut chunker = Self {
            encoder: ProjectionPageEncoder::default(),
            expected_physical: SerializedMetric::default(),
            source_bound: true,
            accepted_physical: SerializedMetric::default(),
            tail: None,
            pending_virtual: None,
            finishing: false,
            complete: false,
            receipt: ProjectionChunkerReceipt::default(),
        };
        chunker.observe_buffer();
        chunker
    }

    pub fn push(
        &mut self,
        piece: ProjectionPiece,
    ) -> Result<Option<ProjectionChunk>, SerializedGreenError> {
        if self.finishing || self.complete {
            return Err(SerializedGreenError::Invalid(
                "projection envelope already entered finish",
            ));
        }
        piece.validate()?;
        if let ProjectionPiece::Virtual { kind } = &piece {
            if self.pending_virtual.is_some() {
                return Err(SerializedGreenError::Invalid(
                    "adjacent virtual pieces require a bounded typed repeat transform",
                ));
            }
            self.pending_virtual = Some(*kind);
            self.receipt.pieces_accepted = self
                .receipt
                .pieces_accepted
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow("projection piece receipt"))?;
            return Ok(None);
        }

        let (physical, _) = piece.metrics();
        debug_assert!(!physical.is_zero(), "every non-Virtual piece is physical");
        let accepted_physical = self.accepted_physical.checked_add(physical)?;
        if !self.source_bound
            && (accepted_physical.bytes > self.expected_physical.bytes
                || accepted_physical.utf16 > self.expected_physical.utf16)
        {
            return Err(SerializedGreenError::Invalid(
                "projection pieces exceed their source envelope",
            ));
        }
        self.accepted_physical = accepted_physical;
        self.receipt.pieces_accepted = self
            .receipt
            .pieces_accepted
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow("projection piece receipt"))?;

        let boundary_virtual = self.pending_virtual.take();
        if let Some(tail) = self.tail.as_mut()
            && boundary_virtual.is_none()
            && let Some(merged) = merge_projection_piece(&tail.physical, &piece)?
        {
            tail.physical = merged;
            return Ok(None);
        }

        let emitted = if let Some(tail) = self.tail.take() {
            self.commit_packet(&tail)?
        } else {
            None
        };
        self.tail = Some(ProjectionTailPacket {
            leading_virtual: boundary_virtual,
            physical: piece,
            trailing_virtual: None,
        });
        self.observe_buffer();
        Ok(emitted)
    }

    /// Advances finalization by at most one emitted chunk. Call until
    /// [`ProjectionChunkerFinish::Complete`] is returned. This keeps EOF
    /// virtual anchoring and a full preceding page cancellable between pages.
    pub fn finish(
        &mut self,
    ) -> Result<(Option<ProjectionChunk>, ProjectionChunkerFinish), SerializedGreenError> {
        if self.source_bound {
            return Err(SerializedGreenError::Invalid(
                "source-bound projection chunker requires composer finish",
            ));
        }
        self.finish_inner()
    }

    pub(crate) fn finish_source_bound(
        &mut self,
    ) -> Result<(Option<ProjectionChunk>, ProjectionChunkerFinish), SerializedGreenError> {
        if !self.source_bound {
            return Err(SerializedGreenError::Invalid(
                "caller-metric chunker cannot enter source-bound finish",
            ));
        }
        if !self.finishing && !self.complete {
            self.expected_physical = self.accepted_physical;
        }
        self.finish_inner()
    }

    fn finish_inner(
        &mut self,
    ) -> Result<(Option<ProjectionChunk>, ProjectionChunkerFinish), SerializedGreenError> {
        if self.complete {
            return Ok((None, ProjectionChunkerFinish::Complete(self.receipt)));
        }
        if !self.finishing {
            if self.tail.is_none() && self.pending_virtual.is_some() {
                return Err(SerializedGreenError::Invalid(
                    "virtual-only projection envelope has no physical anchor",
                ));
            }
            if self.accepted_physical != self.expected_physical {
                return Err(SerializedGreenError::Invalid(
                    "projection pieces do not partition their source envelope",
                ));
            }
            let mut tail = self.tail.take().ok_or(SerializedGreenError::Invalid(
                "virtual-only projection envelope has no physical anchor",
            ))?;
            tail.trailing_virtual = self.pending_virtual.take();
            self.finishing = true;
            if let Some(chunk) = self.commit_packet(&tail)? {
                return Ok((Some(chunk), ProjectionChunkerFinish::ChunkPending));
            }
        }
        if self.encoder.is_empty() {
            self.complete = true;
            return Ok((None, ProjectionChunkerFinish::Complete(self.receipt)));
        }
        let chunk = self.emit_encoder()?;
        self.complete = true;
        Ok((Some(chunk), ProjectionChunkerFinish::ChunkPending))
    }

    #[must_use]
    pub const fn receipt(&self) -> ProjectionChunkerReceipt {
        self.receipt
    }

    fn commit_packet(
        &mut self,
        packet: &ProjectionTailPacket,
    ) -> Result<Option<ProjectionChunk>, SerializedGreenError> {
        let pieces = packet.fixed_pieces();
        if self.encoder.fits_sequence(pieces.iter().flatten())? {
            self.encoder.push_sequence(pieces.into_iter().flatten())?;
            self.observe_buffer();
            return Ok(None);
        }
        if self.encoder.is_empty() {
            return Err(SerializedGreenError::Invalid(
                "one indivisible projection packet exceeds the codec page",
            ));
        }
        let chunk = self.emit_encoder()?;
        if !self.encoder.fits_sequence(pieces.iter().flatten())? {
            return Err(SerializedGreenError::Invalid(
                "one indivisible projection packet exceeds the codec page",
            ));
        }
        self.encoder.push_sequence(pieces.into_iter().flatten())?;
        self.observe_buffer();
        Ok(Some(chunk))
    }

    fn emit_encoder(&mut self) -> Result<ProjectionChunk, SerializedGreenError> {
        let mut chunk = std::mem::take(&mut self.encoder).into_chunk()?;
        chunk.fragment_ordinal = self.receipt.chunks_emitted;
        self.receipt.chunks_emitted = self
            .receipt
            .chunks_emitted
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow("projection chunk receipt"))?;
        Ok(chunk)
    }

    fn observe_buffer(&mut self) {
        self.receipt.maximum_buffered_payload_bytes = self
            .receipt
            .maximum_buffered_payload_bytes
            .max(self.encoder.buffered_payload_bytes());
        self.receipt.maximum_buffer_capacity_bytes = self
            .receipt
            .maximum_buffer_capacity_bytes
            .max(self.encoder.buffer_capacity_bytes());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogicalContribution {
    None,
    Identity,
    Hidden { affinity: GreenAffinity },
    Atomic(AtomicProjection),
    Program(ProjectionProgram),
}

impl LogicalContribution {
    fn summary_metric(&self, physical_metric: SerializedMetric) -> SerializedMetric {
        match self {
            Self::None | Self::Hidden { .. } => SerializedMetric::default(),
            Self::Identity => physical_metric,
            Self::Atomic(projection) => projection.logical_metric,
            Self::Program(program) => program.logical_metric,
        }
    }
}

/// The one retained source/projection record. `owner_relative_depth` answers
/// physical editing/range ownership. A non-None contribution feeds the
/// innermost currently open terminal; its channel derives from that kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceProjectionRun {
    pub id: CoverageId,
    pub metric: SerializedMetric,
    /// Zero owns the innermost open block, one its parent, and so on.
    pub owner_relative_depth: u32,
    pub part: CoveragePart,
    pub logical_contribution: LogicalContribution,
    /// Stable storage boundary after this exact physical run. Production code
    /// can set it only through the source-bound projection composer; it is not
    /// a semantic-envelope end or parser-restart checkpoint by itself.
    projection_reset_after: bool,
    /// Builder-only assertion checked against the active terminal and omitted
    /// from packed bytes. The production parser passes a stronger open-binding
    /// capability at this seam; a `BlockId` is sufficient for this codec gate.
    transient_logical_target: Option<BlockId>,
}

impl SourceProjectionRun {
    /// Physical-only constructor retained for structural proof callers. It
    /// never infers logical meaning from `CoveragePart`; callers that know the
    /// parser result must use `with_logical` explicitly.
    pub fn new(
        id: CoverageId,
        bytes: u64,
        utf16: u64,
        owner_relative_depth: u32,
        part: CoveragePart,
    ) -> Result<Self, SerializedGreenError> {
        if id.0 == 0 || bytes == 0 || utf16 == 0 {
            return Err(SerializedGreenError::Invalid(
                "coverage identity and metrics must be nonzero",
            ));
        }
        if part.0 == 0 || part.0 > COVERAGE_PART_MASK {
            return Err(SerializedGreenError::Invalid(
                "coverage part is out of range",
            ));
        }
        Ok(Self {
            id,
            metric: SerializedMetric { bytes, utf16 },
            owner_relative_depth,
            part,
            logical_contribution: LogicalContribution::None,
            projection_reset_after: false,
            transient_logical_target: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_logical(
        id: CoverageId,
        bytes: u64,
        utf16: u64,
        owner_relative_depth: u32,
        part: CoveragePart,
        logical_target: BlockId,
        logical_contribution: LogicalContribution,
    ) -> Result<Self, SerializedGreenError> {
        if matches!(logical_contribution, LogicalContribution::None) {
            return Err(SerializedGreenError::Invalid(
                "None contribution cannot name a logical consumer",
            ));
        }
        let mut value = Self::new(id, bytes, utf16, owner_relative_depth, part)?;
        value.logical_contribution = logical_contribution;
        value.transient_logical_target = Some(logical_target);
        value.validate_builder_projection()?;
        Ok(value)
    }

    fn validate_codec_projection(&self) -> Result<(), SerializedGreenError> {
        match &self.logical_contribution {
            LogicalContribution::None
            | LogicalContribution::Identity
            | LogicalContribution::Hidden { .. } => Ok(()),
            LogicalContribution::Atomic(projection) => {
                projection.validate_kind()?;
                projection.validate_physical(self.metric)
            }
            LogicalContribution::Program(program) => {
                if program.physical_metric == self.metric {
                    Ok(())
                } else {
                    Err(SerializedGreenError::Invalid(
                        "projection program does not partition its physical run",
                    ))
                }
            }
        }
    }

    fn validate_builder_projection(&self) -> Result<(), SerializedGreenError> {
        self.validate_codec_projection()?;
        match (&self.logical_contribution, self.transient_logical_target) {
            (LogicalContribution::None, None) => Ok(()),
            (LogicalContribution::None, Some(_)) => Err(SerializedGreenError::Invalid(
                "None contribution cannot assert a logical target",
            )),
            (_, Some(target)) if target.0 != 0 => Ok(()),
            (_, Some(_)) => Err(SerializedGreenError::Invalid(
                "logical target identity must be nonzero",
            )),
            (_, None) => Err(SerializedGreenError::Invalid(
                "logical contribution is missing its transient target assertion",
            )),
        }
    }

    #[must_use]
    pub const fn has_projection_reset_after(&self) -> bool {
        self.projection_reset_after
    }

    /// This is deliberately crate-private: raw codec callers cannot promote a
    /// source offset or arbitrary run into retained reset authority.
    pub(crate) fn mark_projection_reset_after(&mut self) {
        self.projection_reset_after = true;
    }
}

/// Compatibility name for physical-only proof callers. It is the same unified
/// record and does not restore a parallel coverage model.
pub type CoverageRun = SourceProjectionRun;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GreenEvent {
    Enter {
        block: BlockId,
        kind: GreenKind,
        facts: FactsEnvelope,
    },
    Coverage(SourceProjectionRun),
    Exit {
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenCloseFacts,
    },
}

impl GreenEvent {
    #[must_use]
    pub fn enter(block: BlockId, kind: GreenKind, facts: FactsEnvelope) -> Self {
        Self::Enter { block, kind, facts }
    }

    #[must_use]
    pub const fn exit(closed: ClosedChildAggregate) -> Self {
        Self::Exit {
            closed,
            last_line_blank: false,
            facts: GreenCloseFacts::None,
        }
    }

    #[must_use]
    pub const fn exit_with_facts(closed: ClosedChildAggregate, facts: GreenCloseFacts) -> Self {
        Self::Exit {
            closed,
            last_line_blank: false,
            facts,
        }
    }

    /// Preserves both the derived parent contribution and the intrinsic blank
    /// state needed to recompute that contribution after suffix adoption.
    #[must_use]
    pub const fn exit_with_state(
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenCloseFacts,
    ) -> Self {
        Self::Exit {
            closed,
            last_line_blank,
            facts,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GreenSummary {
    leaves: u64,
    tokens: u64,
    blocks: u64,
    height: u16,
    metric: SerializedMetric,
    logical_metric: SerializedMetric,
    balance: i64,
    minimum_prefix: i64,
    minimum_closed_depth: Option<i64>,
    outermost: ChildSequenceAggregate,
}

impl GreenSummary {
    fn event(event: &GreenEvent) -> Self {
        match event {
            GreenEvent::Enter { .. } => Self {
                tokens: 1,
                blocks: 1,
                balance: 1,
                ..Self::default()
            },
            GreenEvent::Coverage(run) => Self {
                tokens: 1,
                metric: run.metric,
                logical_metric: run.logical_contribution.summary_metric(run.metric),
                ..Self::default()
            },
            GreenEvent::Exit { closed, .. } => Self {
                tokens: 1,
                balance: -1,
                minimum_prefix: -1,
                minimum_closed_depth: Some(-1),
                outermost: ChildSequenceAggregate::singleton(*closed),
                ..Self::default()
            },
        }
    }

    fn followed_by(self, suffix: Self) -> Result<Self, SerializedGreenError> {
        let shifted_right = suffix
            .minimum_closed_depth
            .map(|depth| self.balance + depth);
        let minimum_closed_depth = match (self.minimum_closed_depth, shifted_right) {
            (None, None) => None,
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (Some(left), Some(right)) => Some(left.min(right)),
        };
        let left_is_minimum = self.minimum_closed_depth == minimum_closed_depth;
        let right_is_minimum = shifted_right == minimum_closed_depth;
        let outermost = match (left_is_minimum, right_is_minimum) {
            (true, true) => self.outermost.followed_by(suffix.outermost),
            (true, false) => self.outermost,
            (false, true) => suffix.outermost,
            (false, false) => ChildSequenceAggregate::default(),
        };
        Ok(Self {
            leaves: self
                .leaves
                .checked_add(suffix.leaves)
                .ok_or(SerializedGreenError::Overflow("leaf count"))?,
            tokens: self
                .tokens
                .checked_add(suffix.tokens)
                .ok_or(SerializedGreenError::Overflow("token count"))?,
            blocks: self
                .blocks
                .checked_add(suffix.blocks)
                .ok_or(SerializedGreenError::Overflow("block count"))?,
            height: match (self.height, suffix.height) {
                (0, right) => right,
                (left, 0) => left,
                (left, right) => left
                    .max(right)
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow("sequence height"))?,
            },
            metric: self.metric.checked_add(suffix.metric)?,
            logical_metric: self
                .logical_metric
                .checked_add_logical(suffix.logical_metric)?,
            balance: self
                .balance
                .checked_add(suffix.balance)
                .ok_or(SerializedGreenError::Overflow("structural balance"))?,
            minimum_prefix: self
                .minimum_prefix
                .min(self.balance + suffix.minimum_prefix),
            minimum_closed_depth,
            outermost,
        })
    }

    fn unmatched(self) -> Result<(u64, u64), SerializedGreenError> {
        let closes = u64::try_from(self.minimum_prefix.saturating_neg())
            .map_err(|_| SerializedGreenError::Corrupt("negative close count"))?;
        let opens = self
            .balance
            .checked_add(
                i64::try_from(closes)
                    .map_err(|_| SerializedGreenError::Overflow("unmatched structural depth"))?,
            )
            .ok_or(SerializedGreenError::Overflow("unmatched structural depth"))?;
        Ok((
            u64::try_from(opens)
                .map_err(|_| SerializedGreenError::Corrupt("negative open count"))?,
            closes,
        ))
    }

    fn same_semantics(self, other: Self) -> bool {
        self.tokens == other.tokens
            && self.blocks == other.blocks
            && self.metric == other.metric
            && self.logical_metric == other.logical_metric
            && self.balance == other.balance
            && self.minimum_prefix == other.minimum_prefix
            && self.minimum_closed_depth == other.minimum_closed_depth
            && self.outermost == other.outermost
    }

    fn coverage_runs_for_valid_prefix(self) -> Result<u64, SerializedGreenError> {
        if self.minimum_prefix < 0 || self.balance < 0 {
            return Err(SerializedGreenError::Corrupt(
                "green prefix has invalid structural balance",
            ));
        }
        let open_depth = u64::try_from(self.balance)
            .map_err(|_| SerializedGreenError::Corrupt("green prefix depth is negative"))?;
        let exits = self
            .blocks
            .checked_sub(open_depth)
            .ok_or(SerializedGreenError::Corrupt(
                "green prefix depth exceeds Enter count",
            ))?;
        let structural_tokens =
            self.blocks
                .checked_add(exits)
                .ok_or(SerializedGreenError::Overflow(
                    "green prefix structural token count",
                ))?;
        self.tokens
            .checked_sub(structural_tokens)
            .ok_or(SerializedGreenError::Corrupt(
                "green prefix structural count exceeds event count",
            ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerializedGreenError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    SourceOutOfBounds,
    StaleCursor,
}

impl From<ArenaError> for SerializedGreenError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for SerializedGreenError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl fmt::Display for SerializedGreenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Invalid(message) => write!(formatter, "invalid serialized green: {message}"),
            Self::Corrupt(message) => write!(formatter, "corrupt serialized green: {message}"),
            Self::Overflow(field) => write!(formatter, "serialized green {field} overflow"),
            Self::SourceOutOfBounds => formatter.write_str("serialized green source out of bounds"),
            Self::StaleCursor => formatter.write_str("stale serialized green cursor"),
        }
    }
}

impl std::error::Error for SerializedGreenError {}

#[derive(Debug)]
struct SerializedGreenSpec;

impl SequenceSpec for SerializedGreenSpec {
    type Summary = GreenSummary;
    type Error = SerializedGreenError;
    type BranchPayload = [u8; SUMMARY_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(LEAF_TAG) {
            return Ok(None);
        }
        decode_summary(payload, LEAF_TAG).map(Some)
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(BRANCH_TAG) {
            return Ok(None);
        }
        decode_summary(payload, BRANCH_TAG).map(Some)
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_summary(BRANCH_TAG, summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.followed_by(right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        SerializedGreenError::Corrupt(message)
    }
}

fn encode_summary(tag: u8, summary: GreenSummary) -> [u8; SUMMARY_BYTES] {
    let mut output = [0_u8; SUMMARY_BYTES];
    output[0] = tag;
    output[1] = FORMAT_VERSION;
    output[2..4].copy_from_slice(&summary.height.to_le_bytes());
    output[8..16].copy_from_slice(&summary.leaves.to_le_bytes());
    output[16..24].copy_from_slice(&summary.tokens.to_le_bytes());
    output[24..32].copy_from_slice(&summary.blocks.to_le_bytes());
    output[32..40].copy_from_slice(&summary.metric.bytes.to_le_bytes());
    output[40..48].copy_from_slice(&summary.metric.utf16.to_le_bytes());
    output[48..56].copy_from_slice(&summary.balance.to_le_bytes());
    output[56..64].copy_from_slice(&summary.minimum_prefix.to_le_bytes());
    output[64..72].copy_from_slice(
        &summary
            .minimum_closed_depth
            .unwrap_or(NO_MINIMUM_CLOSED)
            .to_le_bytes(),
    );
    output[72] = encode_fold(summary.outermost);
    output[80..88].copy_from_slice(&summary.logical_metric.bytes.to_le_bytes());
    output[88..96].copy_from_slice(&summary.logical_metric.utf16.to_le_bytes());
    output
}

fn decode_summary(payload: &[u8], tag: u8) -> Result<GreenSummary, SerializedGreenError> {
    if payload.len() < SUMMARY_BYTES
        || payload[0] != tag
        || payload[1] != FORMAT_VERSION
        || payload[4..8] != [0; 4]
        || payload[73..80] != [0; 7]
    {
        return Err(SerializedGreenError::Corrupt("invalid summary header"));
    }
    let height = read_u16(&payload[2..4]);
    let minimum_closed = read_i64(&payload[64..72]);
    let summary = GreenSummary {
        height,
        leaves: read_u64(&payload[8..16]),
        tokens: read_u64(&payload[16..24]),
        blocks: read_u64(&payload[24..32]),
        metric: SerializedMetric {
            bytes: read_u64(&payload[32..40]),
            utf16: read_u64(&payload[40..48]),
        },
        logical_metric: SerializedMetric {
            bytes: read_u64(&payload[80..88]),
            utf16: read_u64(&payload[88..96]),
        },
        balance: read_i64(&payload[48..56]),
        minimum_prefix: read_i64(&payload[56..64]),
        minimum_closed_depth: (minimum_closed != NO_MINIMUM_CLOSED).then_some(minimum_closed),
        outermost: decode_fold(payload[72])?,
    };
    if summary.leaves == 0
        || summary.tokens == 0
        || summary.logical_metric.is_partially_zero()
        || (tag == LEAF_TAG && (summary.leaves != 1 || summary.height != 1))
        || (tag == BRANCH_TAG && (summary.leaves < 2 || summary.height < 2))
    {
        return Err(SerializedGreenError::Corrupt("invalid summary values"));
    }
    Ok(summary)
}

fn encode_fold(value: ChildSequenceAggregate) -> u8 {
    u8::from(value.had_child)
        | (u8::from(value.any_nonlast_child_ends_blank) << 1)
        | (u8::from(value.last_child_ends_blank) << 2)
        | (u8::from(value.list_loose_before_last) << 3)
        | (u8::from(value.last_item_loose_if_nonlast) << 4)
        | (u8::from(value.last_item_loose_if_last) << 5)
}

fn decode_fold(value: u8) -> Result<ChildSequenceAggregate, SerializedGreenError> {
    if value & !0x3f != 0 {
        return Err(SerializedGreenError::Corrupt("invalid child fold bits"));
    }
    Ok(ChildSequenceAggregate {
        had_child: value & 1 != 0,
        any_nonlast_child_ends_blank: value & 2 != 0,
        last_child_ends_blank: value & 4 != 0,
        list_loose_before_last: value & 8 != 0,
        last_item_loose_if_nonlast: value & 16 != 0,
        last_item_loose_if_last: value & 32 != 0,
    })
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().expect("fixed u16"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("fixed u64"))
}

fn read_i64(bytes: &[u8]) -> i64 {
    i64::from_le_bytes(bytes.try_into().expect("fixed i64"))
}

fn push_varint(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn push_metric(metric: SerializedMetric, same_flag: u8, descriptor: &mut u8, output: &mut Vec<u8>) {
    if metric.bytes == metric.utf16 {
        *descriptor |= same_flag;
    }
    push_varint(metric.bytes, output);
    if metric.bytes != metric.utf16 {
        push_varint(metric.utf16, output);
    }
}

fn varint_encoded_len(value: u64) -> usize {
    let significant_bits = usize::try_from(u64::BITS - value.leading_zeros()).unwrap_or(64);
    significant_bits.max(1).div_ceil(7)
}

fn metric_encoded_len(metric: SerializedMetric) -> usize {
    varint_encoded_len(metric.bytes)
        + if metric.bytes == metric.utf16 {
            0
        } else {
            varint_encoded_len(metric.utf16)
        }
}

fn projection_program_header_len(
    piece_count: usize,
    physical_metric: SerializedMetric,
    logical_metric: SerializedMetric,
) -> Result<usize, SerializedGreenError> {
    let piece_count = u64::try_from(piece_count)
        .map_err(|_| SerializedGreenError::Overflow("projection piece count"))?;
    Ok(4 + varint_encoded_len(piece_count)
        + metric_encoded_len(physical_metric)
        + metric_encoded_len(logical_metric))
}

fn encode_projection_piece(
    piece: &ProjectionPiece,
    output: &mut Vec<u8>,
) -> Result<(), SerializedGreenError> {
    piece.validate()?;
    let descriptor_index = output.len();
    output.push(0);
    let mut descriptor;
    match piece {
        ProjectionPiece::Identity { metric } => {
            descriptor = PROGRAM_IDENTITY_TAG;
            push_metric(
                *metric,
                PROGRAM_SAME_PHYSICAL_METRIC,
                &mut descriptor,
                output,
            );
        }
        ProjectionPiece::Hidden { metric, affinity } => {
            descriptor = match affinity {
                GreenAffinity::Upstream => PROGRAM_HIDDEN_UPSTREAM_TAG,
                GreenAffinity::Downstream => PROGRAM_HIDDEN_DOWNSTREAM_TAG,
            };
            push_metric(
                *metric,
                PROGRAM_SAME_PHYSICAL_METRIC,
                &mut descriptor,
                output,
            );
        }
        ProjectionPiece::Atomic {
            physical_metric,
            projection,
        } => {
            descriptor = match projection.kind {
                AtomicProjectionKind::TabToSpaces { spaces } => {
                    output.push(spaces);
                    PROGRAM_ATOMIC_TAB_TAG
                }
                AtomicProjectionKind::CrLfToLf => PROGRAM_ATOMIC_CRLF_TAG,
                AtomicProjectionKind::LoneCrToLf => PROGRAM_ATOMIC_LONE_CR_TAG,
                AtomicProjectionKind::NulToReplacement => PROGRAM_ATOMIC_NUL_TAG,
            };
            push_metric(
                *physical_metric,
                PROGRAM_SAME_PHYSICAL_METRIC,
                &mut descriptor,
                output,
            );
            push_metric(
                projection.logical_metric,
                PROGRAM_SAME_LOGICAL_METRIC,
                &mut descriptor,
                output,
            );
        }
        ProjectionPiece::Virtual { kind } => {
            descriptor = match kind {
                VirtualProjectionKind::LineFeed => PROGRAM_VIRTUAL_LINE_FEED_TAG,
            };
            push_metric(
                kind.logical_metric(),
                PROGRAM_SAME_LOGICAL_METRIC,
                &mut descriptor,
                output,
            );
        }
    }
    output[descriptor_index] = descriptor;
    Ok(())
}

fn projection_piece_encoded_len(piece: &ProjectionPiece) -> Result<usize, SerializedGreenError> {
    piece.validate()?;
    Ok(match piece {
        ProjectionPiece::Identity { metric } | ProjectionPiece::Hidden { metric, .. } => {
            1 + metric_encoded_len(*metric)
        }
        ProjectionPiece::Atomic {
            physical_metric,
            projection,
        } => {
            1 + usize::from(matches!(
                projection.kind,
                AtomicProjectionKind::TabToSpaces { .. }
            )) + metric_encoded_len(*physical_metric)
                + metric_encoded_len(projection.logical_metric)
        }
        ProjectionPiece::Virtual { kind } => 1 + metric_encoded_len(kind.logical_metric()),
    })
}

fn merge_projection_piece(
    left: &ProjectionPiece,
    right: &ProjectionPiece,
) -> Result<Option<ProjectionPiece>, SerializedGreenError> {
    let merged = match (left, right) {
        (
            ProjectionPiece::Identity { metric: left },
            ProjectionPiece::Identity { metric: right },
        ) => ProjectionPiece::Identity {
            metric: left.checked_add(*right)?,
        },
        (
            ProjectionPiece::Hidden {
                metric: left,
                affinity: left_affinity,
            },
            ProjectionPiece::Hidden {
                metric: right,
                affinity: right_affinity,
            },
        ) if left_affinity == right_affinity => ProjectionPiece::Hidden {
            metric: left.checked_add(*right)?,
            affinity: *left_affinity,
        },
        _ => return Ok(None),
    };
    Ok(Some(merged))
}

#[derive(Clone, Debug)]
struct EncodedProjectionTail {
    piece: ProjectionPiece,
    start: usize,
    encoded_len: usize,
}

#[derive(Debug)]
struct ProjectionPageEncoder {
    payload: Vec<u8>,
    piece_count: usize,
    physical_metric: SerializedMetric,
    logical_metric: SerializedMetric,
    last: Option<EncodedProjectionTail>,
}

impl Default for ProjectionPageEncoder {
    fn default() -> Self {
        Self {
            // The body is encoded from offset zero. Sealing shifts it once by
            // the exact header length inside this same fixed-capacity page.
            payload: Vec::with_capacity(PROJECTION_PROGRAM_PAGE_BYTES),
            piece_count: 0,
            physical_metric: SerializedMetric::default(),
            logical_metric: SerializedMetric::default(),
            last: None,
        }
    }
}

impl ProjectionPageEncoder {
    fn is_empty(&self) -> bool {
        self.piece_count == 0
    }

    fn buffered_payload_bytes(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            projection_program_header_len(
                self.piece_count,
                self.physical_metric,
                self.logical_metric,
            )
            .unwrap_or(PROJECTION_PROGRAM_MAX_HEADER_BYTES)
                + self.payload.len()
        }
    }

    fn buffer_capacity_bytes(&self) -> usize {
        self.payload.capacity()
    }

    fn fits_sequence<'a>(
        &self,
        pieces: impl IntoIterator<Item = &'a ProjectionPiece>,
    ) -> Result<bool, SerializedGreenError> {
        let mut piece_count = self.piece_count;
        let mut physical_metric = self.physical_metric;
        let mut logical_metric = self.logical_metric;
        let mut body_len = self.payload.len();
        let mut last = self
            .last
            .as_ref()
            .map(|tail| (tail.piece.clone(), tail.encoded_len));
        for piece in pieces {
            piece.validate()?;
            let (physical, logical) = piece.metrics();
            physical_metric = physical_metric.checked_add(physical)?;
            logical_metric = logical_metric.checked_add(logical)?;
            if let Some((last_piece, last_len)) = last.as_ref()
                && let Some(merged) = merge_projection_piece(last_piece, piece)?
            {
                let merged_len = projection_piece_encoded_len(&merged)?;
                body_len = body_len - *last_len + merged_len;
                last = Some((merged, merged_len));
            } else {
                let encoded_len = projection_piece_encoded_len(piece)?;
                piece_count = piece_count
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow("projection piece count"))?;
                body_len = body_len
                    .checked_add(encoded_len)
                    .ok_or(SerializedGreenError::Overflow("projection page bytes"))?;
                last = Some((piece.clone(), encoded_len));
            }
        }
        Ok(
            projection_program_header_len(piece_count, physical_metric, logical_metric)? + body_len
                <= PROJECTION_PROGRAM_PAGE_BYTES,
        )
    }

    fn push_sequence(
        &mut self,
        pieces: impl IntoIterator<Item = ProjectionPiece>,
    ) -> Result<(), SerializedGreenError> {
        for piece in pieces {
            if !self.try_push(piece)? {
                return Err(SerializedGreenError::Invalid(
                    "projection packet changed after its exact fit preflight",
                ));
            }
        }
        Ok(())
    }

    fn try_push(&mut self, piece: ProjectionPiece) -> Result<bool, SerializedGreenError> {
        piece.validate()?;
        let (piece_physical, piece_logical) = piece.metrics();
        let physical_metric = self.physical_metric.checked_add(piece_physical)?;
        let logical_metric = self.logical_metric.checked_add(piece_logical)?;
        let merged = self
            .last
            .as_ref()
            .map(|last| merge_projection_piece(&last.piece, &piece))
            .transpose()?
            .flatten();
        let (piece_count, next_body_len) =
            if let (Some(last), Some(merged)) = (self.last.as_ref(), merged.as_ref()) {
                (
                    self.piece_count,
                    self.payload.len() - last.encoded_len + projection_piece_encoded_len(merged)?,
                )
            } else {
                (
                    self.piece_count
                        .checked_add(1)
                        .ok_or(SerializedGreenError::Overflow("projection piece count"))?,
                    self.payload.len() + projection_piece_encoded_len(&piece)?,
                )
            };
        let header_len =
            projection_program_header_len(piece_count, physical_metric, logical_metric)?;
        if header_len + next_body_len > PROJECTION_PROGRAM_PAGE_BYTES {
            return Ok(false);
        }

        if let Some(merged) = merged {
            let last = self
                .last
                .as_mut()
                .expect("a merged piece has a predecessor");
            self.payload.truncate(last.start);
            encode_projection_piece(&merged, &mut self.payload)?;
            last.encoded_len = self.payload.len() - last.start;
            last.piece = merged;
        } else {
            let start = self.payload.len();
            encode_projection_piece(&piece, &mut self.payload)?;
            self.last = Some(EncodedProjectionTail {
                piece,
                start,
                encoded_len: self.payload.len() - start,
            });
            self.piece_count = piece_count;
        }
        self.physical_metric = physical_metric;
        self.logical_metric = logical_metric;
        Ok(true)
    }

    fn into_chunk(self) -> Result<ProjectionChunk, SerializedGreenError> {
        if self.piece_count == 1 {
            let piece = self
                .last
                .as_ref()
                .ok_or(SerializedGreenError::Invalid("empty projection program"))?
                .piece
                .clone();
            let logical_contribution = match piece {
                ProjectionPiece::Identity { .. } => LogicalContribution::Identity,
                ProjectionPiece::Hidden { affinity, .. } => {
                    LogicalContribution::Hidden { affinity }
                }
                ProjectionPiece::Atomic { projection, .. } => {
                    LogicalContribution::Atomic(projection)
                }
                ProjectionPiece::Virtual { .. } => {
                    return Err(SerializedGreenError::Invalid(
                        "virtual projection is missing a physical anchor",
                    ));
                }
            };
            return Ok(ProjectionChunk {
                fragment_ordinal: 0,
                physical_metric: self.physical_metric,
                logical_contribution,
            });
        }
        let physical_metric = self.physical_metric;
        Ok(ProjectionChunk {
            fragment_ordinal: 0,
            physical_metric,
            logical_contribution: LogicalContribution::Program(self.into_program()?),
        })
    }

    fn into_program(mut self) -> Result<ProjectionProgram, SerializedGreenError> {
        if self.piece_count == 0 {
            return Err(SerializedGreenError::Invalid("empty projection program"));
        }
        if self.physical_metric.is_zero() || self.logical_metric.is_partially_zero() {
            return Err(SerializedGreenError::Invalid(
                "projection program must anchor physical input and a valid logical metric",
            ));
        }
        let mut header = ProjectionProgramHeaderEncoder::default();
        header.push(PROJECTION_PROGRAM_TAG)?;
        header.push(PROJECTION_PROGRAM_VERSION)?;
        header.push(0)?;
        header.push(0)?;
        header.push_varint(
            u64::try_from(self.piece_count)
                .map_err(|_| SerializedGreenError::Overflow("projection piece count"))?,
        )?;
        let mut metric_descriptor = 0_u8;
        header.push_metric(
            self.physical_metric,
            PROGRAM_SAME_PHYSICAL_METRIC,
            &mut metric_descriptor,
        )?;
        header.push_metric(
            self.logical_metric,
            PROGRAM_SAME_LOGICAL_METRIC,
            &mut metric_descriptor,
        )?;
        header.bytes[2] = metric_descriptor;
        debug_assert_eq!(
            header.len(),
            projection_program_header_len(
                self.piece_count,
                self.physical_metric,
                self.logical_metric
            )?
        );
        let body_end = self.payload.len();
        let sealed_len = header
            .len()
            .checked_add(body_end)
            .ok_or(SerializedGreenError::Overflow("projection page bytes"))?;
        if sealed_len > PROJECTION_PROGRAM_PAGE_BYTES {
            return Err(SerializedGreenError::Invalid(
                "projection program exceeds one bounded arena page",
            ));
        }
        self.payload.resize(sealed_len, 0);
        self.payload.copy_within(0..body_end, header.len());
        self.payload[..header.len()].copy_from_slice(header.as_slice());
        debug_assert!(self.payload.len() <= PROJECTION_PROGRAM_PAGE_BYTES);
        debug_assert!(self.payload.capacity() <= PROJECTION_PROGRAM_PAGE_BYTES);
        Ok(ProjectionProgram {
            payload: self.payload,
            piece_count: self.piece_count,
            physical_metric: self.physical_metric,
            logical_metric: self.logical_metric,
        })
    }
}

/// Stack-only encoder for the codec's bounded page header. Keeping this
/// separate from the body means a near-full page cannot trigger allocator
/// growth merely because the eventual compact header is not known yet.
struct ProjectionProgramHeaderEncoder {
    bytes: [u8; PROJECTION_PROGRAM_MAX_HEADER_BYTES],
    len: usize,
}

impl Default for ProjectionProgramHeaderEncoder {
    fn default() -> Self {
        Self {
            bytes: [0; PROJECTION_PROGRAM_MAX_HEADER_BYTES],
            len: 0,
        }
    }
}

impl ProjectionProgramHeaderEncoder {
    fn push(&mut self, byte: u8) -> Result<(), SerializedGreenError> {
        let slot = self
            .bytes
            .get_mut(self.len)
            .ok_or(SerializedGreenError::Overflow("projection program header"))?;
        *slot = byte;
        self.len += 1;
        Ok(())
    }

    fn push_varint(&mut self, mut value: u64) -> Result<(), SerializedGreenError> {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            self.push(byte)?;
            if value == 0 {
                return Ok(());
            }
        }
    }

    fn push_metric(
        &mut self,
        metric: SerializedMetric,
        same_flag: u8,
        descriptor: &mut u8,
    ) -> Result<(), SerializedGreenError> {
        if metric.bytes == metric.utf16 {
            *descriptor |= same_flag;
        }
        self.push_varint(metric.bytes)?;
        if metric.bytes != metric.utf16 {
            self.push_varint(metric.utf16)?;
        }
        Ok(())
    }

    const fn len(&self) -> usize {
        self.len
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

fn encode_projection_program(program: &ProjectionProgram) -> Result<Vec<u8>, SerializedGreenError> {
    if program.payload.len() > PROJECTION_PROGRAM_PAGE_BYTES {
        return Err(SerializedGreenError::Invalid(
            "projection program exceeds one bounded arena page",
        ));
    }
    Ok(program.payload.clone())
}

fn decode_metric(
    decoder: &mut Decoder<'_>,
    same: bool,
) -> Result<SerializedMetric, SerializedGreenError> {
    let bytes = decoder.varint()?;
    let utf16 = if same { bytes } else { decoder.varint()? };
    Ok(SerializedMetric { bytes, utf16 })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectionProgramHeader {
    piece_count: usize,
    physical_metric: SerializedMetric,
    logical_metric: SerializedMetric,
}

fn decode_projection_program_header(
    decoder: &mut Decoder<'_>,
) -> Result<ProjectionProgramHeader, SerializedGreenError> {
    if decoder.u8()? != PROJECTION_PROGRAM_TAG || decoder.u8()? != PROJECTION_PROGRAM_VERSION {
        return Err(SerializedGreenError::Corrupt(
            "projection edge has the wrong page type or version",
        ));
    }
    let metric_descriptor = decoder.u8()?;
    if metric_descriptor & !(PROGRAM_SAME_PHYSICAL_METRIC | PROGRAM_SAME_LOGICAL_METRIC) != 0
        || decoder.u8()? != 0
    {
        return Err(SerializedGreenError::Corrupt(
            "invalid projection program header",
        ));
    }
    let piece_count = usize::try_from(decoder.varint()?)
        .map_err(|_| SerializedGreenError::Corrupt("projection piece count exceeds usize"))?;
    if piece_count == 0 {
        return Err(SerializedGreenError::Corrupt("empty projection program"));
    }
    let physical_metric = decode_metric(
        decoder,
        metric_descriptor & PROGRAM_SAME_PHYSICAL_METRIC != 0,
    )?;
    let logical_metric = decode_metric(
        decoder,
        metric_descriptor & PROGRAM_SAME_LOGICAL_METRIC != 0,
    )?;
    if physical_metric.is_zero()
        || physical_metric.is_partially_zero()
        || logical_metric.is_partially_zero()
    {
        return Err(SerializedGreenError::Corrupt(
            "invalid projection program summary",
        ));
    }
    // Every projection piece has at least a descriptor and one metric varint.
    if piece_count > decoder.remaining() / 2 {
        return Err(SerializedGreenError::Corrupt(
            "projection piece count exceeds remaining page",
        ));
    }
    Ok(ProjectionProgramHeader {
        piece_count,
        physical_metric,
        logical_metric,
    })
}

fn decode_projection_piece(
    decoder: &mut Decoder<'_>,
) -> Result<ProjectionPiece, SerializedGreenError> {
    let descriptor = decoder.u8()?;
    let kind = descriptor & !(PROGRAM_SAME_PHYSICAL_METRIC | PROGRAM_SAME_LOGICAL_METRIC);
    let physical_same = descriptor & PROGRAM_SAME_PHYSICAL_METRIC != 0;
    let logical_same = descriptor & PROGRAM_SAME_LOGICAL_METRIC != 0;
    let piece = match kind {
        PROGRAM_IDENTITY_TAG => {
            if logical_same {
                return Err(SerializedGreenError::Corrupt(
                    "identity program piece has an invalid logical flag",
                ));
            }
            ProjectionPiece::Identity {
                metric: decode_metric(decoder, physical_same)?,
            }
        }
        PROGRAM_HIDDEN_UPSTREAM_TAG | PROGRAM_HIDDEN_DOWNSTREAM_TAG => {
            if logical_same {
                return Err(SerializedGreenError::Corrupt(
                    "hidden program piece has an invalid logical flag",
                ));
            }
            ProjectionPiece::Hidden {
                metric: decode_metric(decoder, physical_same)?,
                affinity: if kind == PROGRAM_HIDDEN_UPSTREAM_TAG {
                    GreenAffinity::Upstream
                } else {
                    GreenAffinity::Downstream
                },
            }
        }
        PROGRAM_ATOMIC_TAB_TAG
        | PROGRAM_ATOMIC_CRLF_TAG
        | PROGRAM_ATOMIC_LONE_CR_TAG
        | PROGRAM_ATOMIC_NUL_TAG => {
            let transform = if kind == PROGRAM_ATOMIC_TAB_TAG {
                AtomicProjectionKind::TabToSpaces {
                    spaces: decoder.u8()?,
                }
            } else if kind == PROGRAM_ATOMIC_CRLF_TAG {
                AtomicProjectionKind::CrLfToLf
            } else if kind == PROGRAM_ATOMIC_NUL_TAG {
                AtomicProjectionKind::NulToReplacement
            } else {
                AtomicProjectionKind::LoneCrToLf
            };
            let physical_metric = decode_metric(decoder, physical_same)?;
            let logical_metric = decode_metric(decoder, logical_same)?;
            ProjectionPiece::Atomic {
                physical_metric,
                projection: AtomicProjection::new(transform, logical_metric)
                    .map_err(|_| SerializedGreenError::Corrupt("invalid atomic program piece"))?,
            }
        }
        PROGRAM_VIRTUAL_LINE_FEED_TAG => {
            if physical_same {
                return Err(SerializedGreenError::Corrupt(
                    "virtual program piece has an invalid physical flag",
                ));
            }
            let metric = decode_metric(decoder, logical_same)?;
            let kind = VirtualProjectionKind::LineFeed;
            if metric != kind.logical_metric() {
                return Err(SerializedGreenError::Corrupt(
                    "virtual program metric does not match its typed output",
                ));
            }
            ProjectionPiece::Virtual { kind }
        }
        _ => {
            return Err(SerializedGreenError::Corrupt(
                "unknown projection program piece type",
            ));
        }
    };
    piece
        .validate()
        .map_err(|_| SerializedGreenError::Corrupt("invalid projection program piece"))?;
    Ok(piece)
}

#[cfg(test)]
fn decode_projection_program_payload(
    payload: &[u8],
) -> Result<ProjectionProgram, SerializedGreenError> {
    let mut decoder = Decoder::new(payload);
    let header = decode_projection_program_header(&mut decoder)?;
    let mut pieces = Vec::with_capacity(header.piece_count);
    for _ in 0..header.piece_count {
        pieces.push(decode_projection_piece(&mut decoder)?);
    }
    if !decoder.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "trailing projection program bytes",
        ));
    }
    let program = ProjectionProgram::new(pieces)
        .map_err(|_| SerializedGreenError::Corrupt("invalid projection program partition"))?;
    if program.physical_metric != header.physical_metric
        || program.logical_metric != header.logical_metric
    {
        return Err(SerializedGreenError::Corrupt(
            "projection program summary mismatch",
        ));
    }
    Ok(program)
}

#[cfg(test)]
fn validate_projection_program_edge_header(
    arena: &PageArena,
    page: ArenaId,
    expected_piece_count: usize,
    expected_physical: SerializedMetric,
    expected_logical: SerializedMetric,
) -> Result<(), SerializedGreenError> {
    if arena.packed_child_count(page)? != 0 {
        return Err(SerializedGreenError::Corrupt(
            "projection program page has ownership edges",
        ));
    }
    let mut decoder = Decoder::new(arena.payload(page)?);
    let header = decode_projection_program_header(&mut decoder)?;
    if header.piece_count != expected_piece_count
        || header.physical_metric != expected_physical
        || header.logical_metric != expected_logical
    {
        return Err(SerializedGreenError::Corrupt(
            "projection edge count or partition mismatch",
        ));
    }
    Ok(())
}

/// Validates one selected Program page without allocating or materializing its
/// pieces. Returns the byte offset of the first piece for lazy traversal.
fn validate_projection_program_edge_payload(
    arena: &PageArena,
    page: ArenaId,
    expected_piece_count: usize,
    expected_physical: SerializedMetric,
    expected_logical: SerializedMetric,
) -> Result<usize, SerializedGreenError> {
    if arena.packed_child_count(page)? != 0 {
        return Err(SerializedGreenError::Corrupt(
            "projection program page has ownership edges",
        ));
    }
    let payload = arena.payload(page)?;
    let mut decoder = Decoder::new(payload);
    let header = decode_projection_program_header(&mut decoder)?;
    if header.piece_count != expected_piece_count
        || header.physical_metric != expected_physical
        || header.logical_metric != expected_logical
    {
        return Err(SerializedGreenError::Corrupt(
            "projection edge count or partition mismatch",
        ));
    }
    let first_piece = decoder.cursor;
    let mut physical = SerializedMetric::default();
    let mut logical = SerializedMetric::default();
    for _ in 0..header.piece_count {
        let piece = decode_projection_piece(&mut decoder)?;
        let (piece_physical, piece_logical) = piece.metrics();
        physical = physical.checked_add(piece_physical).map_err(|_| {
            SerializedGreenError::Corrupt("projection program physical metric overflow")
        })?;
        logical = logical.checked_add(piece_logical).map_err(|_| {
            SerializedGreenError::Corrupt("projection program logical metric overflow")
        })?;
        if physical.bytes > expected_physical.bytes
            || physical.utf16 > expected_physical.utf16
            || logical.bytes > expected_logical.bytes
            || logical.utf16 > expected_logical.utf16
        {
            return Err(SerializedGreenError::Corrupt(
                "projection program prefix exceeds its declared partition",
            ));
        }
    }
    if !decoder.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "trailing projection program bytes",
        ));
    }
    if physical != expected_physical || logical != expected_logical {
        return Err(SerializedGreenError::Corrupt(
            "projection program summary mismatch",
        ));
    }
    Ok(first_piece)
}

fn encode_facts(facts: &FactsEnvelope) -> Result<Vec<u8>, SerializedGreenError> {
    let mut output = Vec::new();
    push_varint(u64::from(facts.schema_version), &mut output);
    push_varint(
        u64::try_from(facts.fields.len())
            .map_err(|_| SerializedGreenError::Overflow("fact field count"))?,
        &mut output,
    );
    for field in &facts.fields {
        push_varint(
            (u64::from(field.id.0) << 1) | u64::from(field.critical),
            &mut output,
        );
        push_varint(
            u64::try_from(field.value.len())
                .map_err(|_| SerializedGreenError::Overflow("fact value length"))?,
            &mut output,
        );
        output.extend_from_slice(&field.value);
    }
    Ok(output)
}

fn validate_facts_for_kind(
    kind: GreenKind,
    facts: &FactsEnvelope,
) -> Result<(), SerializedGreenError> {
    facts.validate_canonical()?;
    let required = match kind {
        GreenKind::LIST => Some(FactId::LIST),
        GreenKind::ITEM => Some(FactId::ITEM),
        GreenKind::HEADING => Some(FactId::HEADING),
        GreenKind::FENCED_CODE => Some(FactId::CODE),
        GreenKind::HTML_BLOCK => Some(FactId::HTML),
        GreenKind::TABLE => Some(FactId::TABLE),
        GreenKind::TABLE_ROW => Some(FactId::TABLE_ROW),
        GreenKind::TABLE_CELL => Some(FactId::TABLE_CELL),
        GreenKind::THEMATIC_BREAK => Some(FactId::THEMATIC_BREAK),
        _ => None,
    };
    if let Some(required) = required
        && !facts.fields.iter().any(|field| field.id == required)
    {
        return Err(SerializedGreenError::Invalid(
            "required kind fact is missing",
        ));
    }
    for field in &facts.fields {
        let known = matches!(
            field.id,
            FactId::LIST
                | FactId::ITEM
                | FactId::HEADING
                | FactId::CODE
                | FactId::HTML
                | FactId::TABLE
                | FactId::TABLE_ALIGNMENTS
                | FactId::TABLE_ROW
                | FactId::TABLE_CELL
                | FactId::THEMATIC_BREAK
        );
        if !known && field.critical {
            return Err(SerializedGreenError::Invalid("unknown critical fact"));
        }
        let allowed = match field.id {
            FactId::LIST => {
                kind == GreenKind::LIST && GreenListOpenFacts::decode_payload(&field.value).is_ok()
            }
            FactId::ITEM => {
                kind == GreenKind::ITEM && GreenItemOpenFacts::decode_payload(&field.value).is_ok()
            }
            FactId::HEADING => {
                kind == GreenKind::HEADING
                    && GreenHeadingOpenFacts::decode_payload(&field.value).is_ok()
            }
            FactId::CODE => {
                kind == GreenKind::FENCED_CODE
                    && GreenFencedCodeOpenFacts::decode_payload(&field.value).is_ok()
            }
            FactId::HTML => kind == GreenKind::HTML_BLOCK && field.value.len() == 1,
            FactId::TABLE => {
                kind == GreenKind::TABLE
                    && GreenTableOpenFacts::decode_payload(&field.value).is_ok()
            }
            // Alignment is distributed once across the typed header-cell
            // Enter facts. This codec-stable aggregate tag remains reserved;
            // accepting it would create a second vector representation or a
            // hidden table-width limit.
            FactId::TABLE_ALIGNMENTS => false,
            FactId::TABLE_ROW => {
                kind == GreenKind::TABLE_ROW
                    && GreenTableRowOpenFacts::decode_payload(&field.value).is_ok()
            }
            FactId::TABLE_CELL => {
                kind == GreenKind::TABLE_CELL
                    && GreenTableCellOpenFacts::decode_payload(&field.value).is_ok()
            }
            FactId::THEMATIC_BREAK => kind == GreenKind::THEMATIC_BREAK && field.value.len() == 1,
            _ => !field.critical,
        };
        if !allowed {
            return Err(SerializedGreenError::Invalid(
                "fact is incompatible with block kind",
            ));
        }
    }
    Ok(())
}

enum PendingProjectionProgram {
    New(Vec<u8>),
    Retained(ArenaId),
}

struct EncodedGreenEvent {
    bytes: Vec<u8>,
    program: Option<PendingProjectionProgram>,
    program_ordinal_offset: Option<usize>,
}

#[allow(clippy::too_many_lines)] // One exhaustive codec match keeps event tags centralized.
fn encode_event(
    event: &GreenEvent,
    program_ordinal: usize,
) -> Result<EncodedGreenEvent, SerializedGreenError> {
    encode_event_inner(event, program_ordinal, true)
}

#[allow(clippy::too_many_lines)] // One exhaustive codec match keeps event tags centralized.
fn encode_event_inner(
    event: &GreenEvent,
    program_ordinal: usize,
    copy_program_payload: bool,
) -> Result<EncodedGreenEvent, SerializedGreenError> {
    let mut output = Vec::new();
    let mut pending_program = None;
    let mut program_ordinal_offset = None;
    match event {
        GreenEvent::Enter { block, kind, facts } => {
            if block.0 == 0 || kind.0 == 0 || kind.0 > 31 {
                return Err(SerializedGreenError::Invalid(
                    "block identity and kind must fit the codec",
                ));
            }
            validate_facts_for_kind(*kind, facts)?;
            let encoded_facts = encode_facts(facts)?;
            output.push(if facts.fields.is_empty() {
                ENTER_NO_FACTS_TAG
            } else {
                ENTER_WITH_FACTS_TAG
            });
            output.push(kind.0);
            output.extend_from_slice(&block.0.to_le_bytes());
            if !facts.fields.is_empty() {
                push_varint(
                    u64::try_from(encoded_facts.len())
                        .map_err(|_| SerializedGreenError::Overflow("facts length"))?,
                    &mut output,
                );
                output.extend_from_slice(&encoded_facts);
            }
        }
        GreenEvent::Coverage(run) => {
            if run.id.0 == 0
                || run.metric.bytes == 0
                || run.metric.utf16 == 0
                || run.part.0 == 0
                || run.part.0 > COVERAGE_PART_MASK
            {
                return Err(SerializedGreenError::Invalid("invalid coverage run"));
            }
            run.validate_codec_projection()?;
            let same_metric = run.metric.bytes == run.metric.utf16;
            output.push(
                COVERAGE_TAG | run.part.0 | if same_metric { COVERAGE_SAME_METRIC } else { 0 },
            );
            output.extend_from_slice(&run.id.0.to_le_bytes());
            push_varint(u64::from(run.owner_relative_depth), &mut output);
            push_varint(run.metric.bytes, &mut output);
            if !same_metric {
                push_varint(run.metric.utf16, &mut output);
            }
            let logical_descriptor = match &run.logical_contribution {
                LogicalContribution::None => LOGICAL_NONE_TAG,
                LogicalContribution::Identity => LOGICAL_IDENTITY_TAG,
                LogicalContribution::Hidden {
                    affinity: GreenAffinity::Upstream,
                } => LOGICAL_HIDDEN_UPSTREAM_TAG,
                LogicalContribution::Hidden {
                    affinity: GreenAffinity::Downstream,
                } => LOGICAL_HIDDEN_DOWNSTREAM_TAG,
                LogicalContribution::Atomic(_) => LOGICAL_ATOMIC_TAG,
                LogicalContribution::Program(_) => LOGICAL_PROGRAM_TAG,
            } | if run.projection_reset_after {
                LOGICAL_PROJECTION_RESET_AFTER
            } else {
                0
            };
            output.push(logical_descriptor);
            match &run.logical_contribution {
                LogicalContribution::None
                | LogicalContribution::Identity
                | LogicalContribution::Hidden { .. } => {}
                LogicalContribution::Atomic(projection) => {
                    let mut descriptor = match projection.kind {
                        AtomicProjectionKind::TabToSpaces { .. } => ATOMIC_EVENT_TAB_TAG,
                        AtomicProjectionKind::CrLfToLf => ATOMIC_EVENT_CRLF_TAG,
                        AtomicProjectionKind::LoneCrToLf => ATOMIC_EVENT_LONE_CR_TAG,
                        AtomicProjectionKind::NulToReplacement => ATOMIC_EVENT_NUL_TAG,
                    };
                    if projection.logical_metric.bytes == projection.logical_metric.utf16 {
                        descriptor |= EVENT_METRIC_SAME;
                    }
                    output.push(descriptor);
                    if let AtomicProjectionKind::TabToSpaces { spaces } = projection.kind {
                        output.push(spaces);
                    }
                    push_varint(projection.logical_metric.bytes, &mut output);
                    if projection.logical_metric.bytes != projection.logical_metric.utf16 {
                        push_varint(projection.logical_metric.utf16, &mut output);
                    }
                }
                LogicalContribution::Program(program) => {
                    let mut metric_descriptor = 0_u8;
                    if program.logical_metric.bytes == program.logical_metric.utf16 {
                        metric_descriptor |= EVENT_METRIC_SAME;
                    }
                    output.push(metric_descriptor);
                    push_varint(program.logical_metric.bytes, &mut output);
                    if program.logical_metric.bytes != program.logical_metric.utf16 {
                        push_varint(program.logical_metric.utf16, &mut output);
                    }
                    program_ordinal_offset = Some(output.len());
                    push_varint(
                        u64::try_from(program_ordinal)
                            .map_err(|_| SerializedGreenError::Overflow("program edge ordinal"))?,
                        &mut output,
                    );
                    push_varint(
                        u64::try_from(program.piece_count)
                            .map_err(|_| SerializedGreenError::Overflow("program piece count"))?,
                        &mut output,
                    );
                    if copy_program_payload {
                        pending_program = Some(PendingProjectionProgram::New(
                            encode_projection_program(program)?,
                        ));
                    }
                }
            }
        }
        GreenEvent::Exit {
            closed,
            last_line_blank,
            facts,
        } => {
            let tag = match (*last_line_blank, facts) {
                (false, GreenCloseFacts::None) => EXIT_TAG,
                (false, GreenCloseFacts::List { tight: false }) => EXIT_LIST_LOOSE_TAG,
                (false, GreenCloseFacts::List { tight: true }) => EXIT_LIST_TIGHT_TAG,
                (false, GreenCloseFacts::FencedCode(_)) => EXIT_FENCED_CODE_TAG,
                (true, GreenCloseFacts::None) => EXIT_LAST_LINE_BLANK_TAG,
                (true, GreenCloseFacts::List { tight: false }) => {
                    EXIT_LIST_LOOSE_LAST_LINE_BLANK_TAG
                }
                (true, GreenCloseFacts::List { tight: true }) => {
                    EXIT_LIST_TIGHT_LAST_LINE_BLANK_TAG
                }
                (true, GreenCloseFacts::FencedCode(_)) => EXIT_FENCED_CODE_LAST_LINE_BLANK_TAG,
            };
            output.push(tag | encode_closed(*closed));
            if let GreenCloseFacts::FencedCode(facts) = facts {
                facts.encode_payload(&mut output);
            }
        }
    }
    Ok(EncodedGreenEvent {
        bytes: output,
        program: pending_program,
        program_ordinal_offset,
    })
}

fn encode_closed(value: ClosedChildAggregate) -> u8 {
    u8::from(value.ends_blank)
        | (u8::from(value.item_loose_if_nonlast) << 1)
        | (u8::from(value.item_loose_if_last) << 2)
}

fn decode_closed(value: u8) -> ClosedChildAggregate {
    ClosedChildAggregate {
        ends_blank: value & 1 != 0,
        item_loose_if_nonlast: value & 2 != 0,
        item_loose_if_last: value & 4 != 0,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Manifest {
    syntax_profile: u64,
    source_revision: SourceRevision,
    source_root: SourceRootId,
    source_bytes: u64,
    source_utf16: u64,
    grammar_revision: GrammarRevision,
    parse_generation: ParseGeneration,
    semantic_epoch: u64,
    known_bytes: Range<u64>,
    summary: GreenSummary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SerializedGreenBuildReceipt {
    pub leaf_pages_allocated: usize,
    pub projection_program_pages_allocated: usize,
    pub branch_nodes_allocated: usize,
    pub manifest_nodes_allocated: usize,
    pub payload_bytes_copied: usize,
    pub edge_bytes_copied: usize,
    pub sequence_nodes_visited: usize,
    pub sequence_leaves_reused: usize,
    pub maximum_encoded_page_buffer_bytes: usize,
    pub maximum_projection_program_bytes: usize,
    pub maximum_pending_program_payload_bytes: usize,
    pub maximum_decoded_page_buffer_bytes: usize,
    pub maximum_streaming_roots: usize,
    /// Allocator-returned bin capacity plus the fixed optional carry scratch.
    pub maximum_streaming_bin_bytes: usize,
    /// Hard bin-index limit derived from the `u64` leaf-count domain.
    pub maximum_sequence_bin_logical_slots: usize,
    /// Bytes requested up front for the logical bin envelope.
    pub maximum_sequence_bin_requested_bytes: usize,
    /// Maximum live tasks, always checked against the logical height bound.
    pub maximum_sequence_join_tasks: usize,
    /// Bytes requested from `max(left.height, right.height) + 4` task slots.
    pub maximum_sequence_join_task_requested_bytes: usize,
    /// Actual allocator-returned task-vector capacity in bytes.
    pub maximum_sequence_join_task_capacity_bytes: usize,
    /// Maximum live join values, checked against the logical limit of two.
    pub maximum_sequence_join_values: usize,
    /// Bytes requested for exactly two join values.
    pub maximum_sequence_join_value_requested_bytes: usize,
    /// Actual allocator-returned value-vector capacity in bytes.
    pub maximum_sequence_join_value_capacity_bytes: usize,
    pub maximum_live_owner_handles: usize,
    pub owner_journal_capacity: usize,
    pub owner_journal_bytes: usize,
    /// Largest compact Program payload copied into an arena page.
    pub maximum_projection_program_payload_len: usize,
    /// Largest temporary Program buffer capacity presented by one event.
    pub maximum_projection_program_scratch_capacity: usize,
    pub maximum_partial_leaf_payload_len: usize,
    pub maximum_partial_leaf_payload_capacity: usize,
    /// Bytes requested before streaming for the fixed leaf payload envelope.
    pub partial_leaf_payload_requested_bytes: usize,
    pub maximum_partial_leaf_program_owners: usize,
    pub maximum_partial_leaf_program_owner_capacity_bytes: usize,
    /// Hard packed-child limit, independent of allocator overcapacity.
    pub partial_leaf_program_owner_logical_slots: usize,
    /// Bytes requested before streaming for the logical owner envelope.
    pub partial_leaf_program_owner_requested_bytes: usize,
    pub maximum_pending_event_payload_len: usize,
    pub maximum_pending_event_payload_capacity: usize,
    /// Maximum structural depth retained by the one exact open-frame stack.
    pub maximum_validator_frame_depth: usize,
    /// Actual allocator-returned storage used by the validator frame stack.
    pub maximum_validator_frame_capacity_bytes: usize,
    /// One fresh descriptor `Vec` is currently created per offered event.
    pub offer_event_descriptor_buffers_created: usize,
    /// Enter events with facts currently create a second temporary `Vec`.
    pub offer_event_facts_buffers_created: usize,
    pub fixed_leaf_child_id_scratch_bytes: usize,
    pub resumable_polls: usize,
    pub resumable_arena_allocations: usize,
    /// Completed normalization reductions, including typed no-ops when no
    /// packed page has naturally sealed since the previous reduction.
    pub working_prefix_reductions_completed: u64,
    /// Reductions that preserved only the active partial page and therefore
    /// needed no sequence-root work.
    pub working_prefix_noop_reductions: u64,
    /// Maximum installed working-prefix capabilities owned at once.
    pub maximum_working_prefixes: usize,
    pub resumable_sequence_splice_polls: usize,
    pub maximum_sequence_splice_requested_bytes: usize,
    pub maximum_sequence_splice_scratch_bytes: usize,
    /// Sparse exact leaf boundaries explicitly forced by the checkpoint lane.
    pub leaf_barriers_completed: u64,
    /// Paragraph Enter records offered through the single-use Setext seam.
    pub provisional_paragraph_enters_offered: u64,
    /// Setext promotions completed by shifting the active partial page in
    /// place. These allocate no arena page or branch.
    pub setext_partial_promotions_completed: u64,
    /// Partial pages force-sealed because the canonical Heading descriptor's
    /// exact seven-byte expansion did not fit beside packed child edges.
    pub setext_capacity_cliff_force_seals: u64,
    /// Previously sealed leaves decoded and replaced through the owned splice
    /// path. One source leaf produces at most two replacement leaves.
    pub setext_sealed_promotions_completed: u64,
    pub setext_replacement_leaf_pages_allocated: u64,
    /// Largest fixed pair of page payload/edge buffers reserved before input
    /// for a Setext one-leaf repack.
    pub maximum_setext_repack_scratch_bytes: usize,
    /// Height of the one sealed source-ordered sequence root.
    pub final_sequence_height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializedGreenRootSpec {
    pub syntax_profile: u64,
    pub source_revision: SourceRevision,
    pub source_root: SourceRootId,
    pub source_bytes: u64,
    /// Exact UTF-16 length of the same immutable source snapshot. Byte length
    /// alone cannot certify caret coordinates for non-ASCII input.
    pub source_utf16: u64,
    pub grammar_revision: GrammarRevision,
    pub parse_generation: ParseGeneration,
    pub semantic_epoch: u64,
    pub known_bytes: Range<u64>,
}

#[derive(Debug)]
pub struct SerializedGreenDocument {
    owner: OwnedArenaRef,
    manifest: SerializedGreenManifestId,
}

/// Arena-bound identity of a validated serialized-green manifest.
///
/// The constructor is intentionally private to the document builder. Local
/// leaf and projection-page IDs are meaningful only together with this typed
/// root capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SerializedGreenManifestId(ArenaScopedId);

impl SerializedGreenManifestId {
    const fn new(id: ArenaScopedId) -> Self {
        Self(id)
    }

    const fn scoped(self) -> ArenaScopedId {
        self.0
    }
}

/// A failed document release returns the complete linear document owner.
#[must_use = "recover the document owner before handling the release failure"]
#[derive(Debug)]
pub struct SerializedGreenReleaseError {
    pub error: ArenaError,
    pub document: SerializedGreenDocument,
}

impl fmt::Display for SerializedGreenReleaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for SerializedGreenReleaseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StructuralOpenFrame {
    block: BlockId,
    kind: GreenKind,
    facts: FactsEnvelope,
    closed_children: ChildSequenceAggregate,
}

#[derive(Debug, Default)]
struct StructuralValidator {
    open_frames: Vec<StructuralOpenFrame>,
    coverage_runs: u64,
    active_terminal: Option<BlockId>,
    saw_root: bool,
    finished_root: bool,
}

impl StructuralValidator {
    fn validate_stack_shape(&self) -> Result<(), SerializedGreenError> {
        if self.active_terminal.is_some_and(|terminal| {
            self.open_frames.last().is_none_or(|frame| {
                frame.block != terminal || frame.kind.logical_channel().is_none()
            })
        }) {
            return Err(SerializedGreenError::Invalid(
                "structural validator terminal is not the deepest open frame",
            ));
        }
        Ok(())
    }

    /// Retypes the one currently-open terminal after an in-build event
    /// rewrite has changed its canonical Enter descriptor. This is purposely
    /// narrow: Setext promotion is the only admitted structural retype, and
    /// the terminal identity plus top-of-stack kind must still agree.
    #[allow(
        dead_code,
        reason = "consumed by the next typed Setext leaf-repack job"
    )]
    fn retype_active_terminal(
        &mut self,
        block: BlockId,
        replacement_block: BlockId,
        from: GreenKind,
        to: GreenKind,
        facts: FactsEnvelope,
    ) -> Result<(), SerializedGreenError> {
        if from != GreenKind::PARAGRAPH || to != GreenKind::HEADING {
            return Err(SerializedGreenError::Invalid(
                "unsupported active terminal retype",
            ));
        }
        if replacement_block.0 == 0 || self.active_terminal != Some(block) {
            return Err(SerializedGreenError::Invalid(
                "active terminal identity does not match retype",
            ));
        }
        let frame = self
            .open_frames
            .last_mut()
            .ok_or(SerializedGreenError::Invalid(
                "active terminal retype has no open kind",
            ))?;
        if frame.block != block || frame.kind != from {
            return Err(SerializedGreenError::Invalid(
                "active terminal kind does not match retype",
            ));
        }
        validate_facts_for_kind(to, &facts)?;
        frame.block = replacement_block;
        frame.kind = to;
        frame.facts = facts;
        self.active_terminal = Some(replacement_block);
        Ok(())
    }

    /// Removes one still-open provisional terminal from the live validation
    /// path without folding a child into its parent.  The caller must stream
    /// an exhaustive canonical replacement and later prove its declared root
    /// is the new deepest open frame.  This is the validator half of the
    /// grammar-neutral fragment-replacement transaction.
    fn begin_active_terminal_fragment_replacement(
        &mut self,
        block: BlockId,
    ) -> Result<(), SerializedGreenError> {
        if self.active_terminal != Some(block) {
            return Err(SerializedGreenError::Invalid(
                "active fragment replacement targets another terminal",
            ));
        }
        let frame = self
            .open_frames
            .last()
            .ok_or(SerializedGreenError::Invalid(
                "active fragment replacement has no open terminal",
            ))?;
        if frame.block != block || frame.kind != GreenKind::PARAGRAPH {
            return Err(SerializedGreenError::Invalid(
                "active fragment replacement requires a provisional Paragraph",
            ));
        }
        self.open_frames.pop();
        self.active_terminal = None;
        self.validate_stack_shape()
    }

    /// Checks the empty-fragment destination before the active Paragraph is
    /// removed. A rejected begin must not partially mutate the validator;
    /// deterministic candidate abort still depends on the original stack.
    fn validate_active_terminal_fragment_parent(
        &self,
        terminal_block: BlockId,
        parent_block: BlockId,
        parent_kind: GreenKind,
    ) -> Result<(), SerializedGreenError> {
        self.validate_stack_shape()?;
        if self.active_terminal != Some(terminal_block) || parent_kind.logical_channel().is_some() {
            return Err(SerializedGreenError::Invalid(
                "removed fragment parent does not match the active terminal path",
            ));
        }
        let mut frames = self.open_frames.iter().rev();
        let terminal = frames.next().ok_or(SerializedGreenError::Invalid(
            "removed fragment has no active terminal frame",
        ))?;
        let parent = frames.next().ok_or(SerializedGreenError::Invalid(
            "removed fragment has no parent frame",
        ))?;
        if terminal.block != terminal_block
            || terminal.kind != GreenKind::PARAGRAPH
            || parent.block != parent_block
            || parent.kind != parent_kind
        {
            return Err(SerializedGreenError::Invalid(
                "removed fragment parent does not match the active terminal path",
            ));
        }
        Ok(())
    }

    fn validate_open_fragment_root(
        &self,
        block: BlockId,
        kind: GreenKind,
    ) -> Result<(), SerializedGreenError> {
        self.validate_stack_shape()?;
        let replacement_is_terminal = kind.logical_channel().is_some();
        if (replacement_is_terminal && self.active_terminal != Some(block))
            || (!replacement_is_terminal && self.active_terminal.is_some())
        {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment ended with an open terminal descendant",
            ));
        }
        let frame = self
            .open_frames
            .last()
            .ok_or(SerializedGreenError::Invalid(
                "canonical fragment replacement closed its ancestor path",
            ))?;
        if frame.block != block || frame.kind != kind || kind == GreenKind::DOCUMENT {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment left a different replacement root open",
            ));
        }
        Ok(())
    }

    /// Validates the distinct empty-fragment outcome: the provisional
    /// Paragraph is gone and the exact pre-existing parent is again deepest.
    /// Unlike a replacement root, that parent may be the Document itself.
    fn validate_fragment_parent(
        &self,
        block: BlockId,
        kind: GreenKind,
    ) -> Result<(), SerializedGreenError> {
        self.validate_stack_shape()?;
        if self.active_terminal.is_some() || kind.logical_channel().is_some() {
            return Err(SerializedGreenError::Invalid(
                "removed fragment left an open terminal descendant",
            ));
        }
        let frame = self
            .open_frames
            .last()
            .ok_or(SerializedGreenError::Invalid(
                "removed fragment closed its parent path",
            ))?;
        if frame.block != block || frame.kind != kind {
            return Err(SerializedGreenError::Invalid(
                "removed fragment returned to a different parent",
            ));
        }
        Ok(())
    }

    fn push(&mut self, event: &GreenEvent) -> Result<(), SerializedGreenError> {
        self.validate_stack_shape()?;
        if self.finished_root {
            return Err(SerializedGreenError::Invalid("event follows document root"));
        }
        match event {
            GreenEvent::Enter { block, kind, facts } => {
                self.push_enter(*block, *kind, facts.clone())?;
            }
            GreenEvent::Coverage(run) => {
                self.push_coverage(run)?;
                self.coverage_runs =
                    self.coverage_runs
                        .checked_add(1)
                        .ok_or(SerializedGreenError::Overflow(
                            "structural validator coverage count",
                        ))?;
            }
            GreenEvent::Exit {
                closed,
                last_line_blank,
                facts,
            } => self.push_exit(*closed, *last_line_blank, *facts)?,
        }
        self.validate_stack_shape()
    }

    fn push_enter(
        &mut self,
        block: BlockId,
        kind: GreenKind,
        facts: FactsEnvelope,
    ) -> Result<(), SerializedGreenError> {
        if self.open_frames.is_empty() {
            if self.saw_root || kind != GreenKind::DOCUMENT {
                return Err(SerializedGreenError::Invalid(
                    "document must have exactly one Document root",
                ));
            }
            self.saw_root = true;
        } else if kind == GreenKind::DOCUMENT {
            return Err(SerializedGreenError::Invalid("nested Document block"));
        }
        if self.active_terminal.is_some() {
            return Err(SerializedGreenError::Invalid(
                "terminal block cannot contain another block",
            ));
        }
        self.open_frames.try_reserve(1).map_err(|_| {
            SerializedGreenError::Invalid("structural frame stack reservation failed")
        })?;
        if kind.logical_channel().is_some() {
            self.active_terminal = Some(block);
        }
        self.open_frames.push(StructuralOpenFrame {
            block,
            kind,
            facts,
            closed_children: ChildSequenceAggregate::default(),
        });
        Ok(())
    }

    fn push_coverage(&self, run: &SourceProjectionRun) -> Result<(), SerializedGreenError> {
        run.validate_builder_projection()?;
        let depth = usize::try_from(run.owner_relative_depth)
            .map_err(|_| SerializedGreenError::Overflow("coverage owner depth"))?;
        if depth >= self.open_frames.len() {
            return Err(SerializedGreenError::Invalid(
                "coverage owner depth escapes open path",
            ));
        }
        if !matches!(run.logical_contribution, LogicalContribution::None) {
            let Some(terminal) = self.active_terminal else {
                return Err(SerializedGreenError::Invalid(
                    "logical contribution has no open terminal",
                ));
            };
            if run.transient_logical_target != Some(terminal) {
                return Err(SerializedGreenError::Invalid(
                    "logical target assertion does not match the active terminal",
                ));
            }
        }
        Ok(())
    }

    fn push_exit(
        &mut self,
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenCloseFacts,
    ) -> Result<(), SerializedGreenError> {
        let frame = self
            .open_frames
            .last()
            .cloned()
            .ok_or(SerializedGreenError::Invalid("unmatched Exit"))?;
        let kind = frame.kind;
        let children = frame.closed_children;
        facts.validate_for_kind(kind)?;
        let semantics = ContainerFoldSemantics {
            descends_through_last_child: matches!(kind, GreenKind::LIST | GreenKind::ITEM),
            is_item: kind == GreenKind::ITEM,
            last_line_blank,
        };
        if semantics.closed_summary(children) != closed {
            return Err(SerializedGreenError::Invalid(
                "Exit closed summary disagrees with children and last_line_blank",
            ));
        }
        if let GreenCloseFacts::List { tight } = facts
            && tight != children.list_is_tight()
        {
            return Err(SerializedGreenError::Invalid(
                "List Exit tightness disagrees with finalized children",
            ));
        }
        self.open_frames.pop();
        if kind.logical_channel().is_some() {
            self.active_terminal = None;
        }
        if let Some(parent) = self.open_frames.last_mut() {
            parent.closed_children = parent
                .closed_children
                .followed_by(ChildSequenceAggregate::singleton(closed));
        }
        if self.open_frames.is_empty() {
            self.finished_root = true;
        }
        Ok(())
    }

    fn finish(self) -> Result<(), SerializedGreenError> {
        if !self.saw_root
            || !self.finished_root
            || !self.open_frames.is_empty()
            || self.active_terminal.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "document root is not complete",
            ));
        }
        Ok(())
    }
}

fn record_validator_scratch(
    receipt: &mut SerializedGreenBuildReceipt,
    validator: &StructuralValidator,
) {
    receipt.maximum_validator_frame_depth = receipt
        .maximum_validator_frame_depth
        .max(validator.open_frames.len());
    receipt.maximum_validator_frame_capacity_bytes =
        receipt.maximum_validator_frame_capacity_bytes.max(
            validator
                .open_frames
                .capacity()
                .saturating_mul(std::mem::size_of::<StructuralOpenFrame>()),
        );
}

struct LeafEncoder {
    bytes: Vec<u8>,
    summary: GreenSummary,
    programs: Vec<PendingProjectionProgram>,
    pending_new_program_bytes: usize,
}

impl Default for LeafEncoder {
    fn default() -> Self {
        let mut bytes = Vec::with_capacity(ARENA_PAGE_BYTES);
        bytes.resize(LEAF_HEADER_BYTES, 0);
        Self {
            bytes,
            summary: GreenSummary::default(),
            programs: Vec::new(),
            pending_new_program_bytes: 0,
        }
    }
}

impl LeafEncoder {
    fn is_empty(&self) -> bool {
        self.summary.tokens == 0
    }

    fn can_fit(&self, encoded: &EncodedGreenEvent) -> bool {
        let next_programs = self.programs.len() + usize::from(encoded.program.is_some());
        let next_program_bytes = self.pending_new_program_bytes
            + match encoded.program.as_ref() {
                Some(PendingProjectionProgram::New(payload)) => payload.capacity(),
                Some(PendingProjectionProgram::Retained(_)) | None => 0,
            };
        next_programs <= MAX_PACKED_ARENA_CHILDREN
            && next_program_bytes <= ARENA_PAGE_BYTES
            && self.bytes.len()
                + encoded.bytes.len()
                + next_programs * std::mem::size_of::<ArenaId>()
                <= ARENA_PAGE_BYTES
    }

    fn push(
        &mut self,
        event: &GreenEvent,
        encoded: EncodedGreenEvent,
    ) -> Result<(), SerializedGreenError> {
        if !self.can_fit(&encoded) {
            return Err(SerializedGreenError::Invalid("event exceeds green leaf"));
        }
        self.bytes.extend_from_slice(&encoded.bytes);
        if let Some(program) = encoded.program {
            if let PendingProjectionProgram::New(payload) = &program {
                self.pending_new_program_bytes += payload.capacity();
            }
            self.programs.push(program);
        }
        self.summary = self.summary.followed_by(GreenSummary::event(event))?;
        Ok(())
    }

    fn push_decoded(
        &mut self,
        event: &DecodedGreenEventKind,
        encoded: EncodedGreenEvent,
    ) -> Result<(), SerializedGreenError> {
        if !self.can_fit(&encoded) {
            return Err(SerializedGreenError::Invalid("event exceeds green leaf"));
        }
        self.bytes.extend_from_slice(&encoded.bytes);
        if let Some(program) = encoded.program {
            if let PendingProjectionProgram::New(payload) = &program {
                self.pending_new_program_bytes += payload.capacity();
            }
            self.programs.push(program);
        }
        self.summary = self
            .summary
            .followed_by(GreenSummary::decoded_event(event))?;
        Ok(())
    }

    fn seal(
        mut self,
    ) -> Result<(Vec<u8>, GreenSummary, Vec<PendingProjectionProgram>), SerializedGreenError> {
        if self.is_empty() {
            return Err(SerializedGreenError::Invalid("empty green leaf"));
        }
        self.summary.leaves = 1;
        self.summary.height = 1;
        let header = encode_summary(LEAF_TAG, self.summary);
        self.bytes[..LEAF_HEADER_BYTES].copy_from_slice(&header);
        Ok((self.bytes, self.summary, self.programs))
    }
}

/// Progress of the allocation-granular packed-green mechanism builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerializedGreenStreamProgress {
    /// The previous event is encoded into the one partial leaf; another event
    /// may be offered.
    ReadyForEvent,
    /// More bounded work remains. One poll performs at most one arena page or
    /// branch allocation. Leaf, bin, reusable tail-join, and working-splice
    /// storage are all fallibly reserved before input; polls cannot grow them.
    Pending,
    /// Exact physical coverage and structure are sealed in one manifest owner.
    ManifestReady,
}

/// One exact event-sequence cut after a force-sealed packed leaf.
///
/// Fields are private so a parser can retain or move the cut but cannot forge
/// source metrics, event ordinals, leaf ordinals, or build provenance. This is
/// deliberately only build-local mechanism state: it becomes durable restart
/// authority only after a candidate writer consumes it into a composite
/// manifest entry while the matching arena session remains live.
#[must_use = "a build-local green cut must be consumed into its candidate checkpoint entry"]
#[derive(Debug, PartialEq, Eq)]
pub struct SerializedGreenLeafCut {
    build: ArenaBuildId,
    leaves_before: u64,
    events_before: u64,
    source_before: SerializedMetric,
}

impl SerializedGreenLeafCut {
    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn leaves_before(&self) -> u64 {
        self.leaves_before
    }

    #[must_use]
    pub const fn events_before(&self) -> u64 {
        self.events_before
    }

    #[must_use]
    pub const fn source_before(&self) -> SerializedMetric {
        self.source_before
    }
}

/// Exact input cut after all naturally sealed packed pages have been folded
/// into the builder's sole working prefix. The active partial page deliberately
/// remains buffered so frequent normalization cuts do not fragment packing.
/// No arena root or local ID can escape through this capability.
#[must_use = "a build-local working cut must be consumed by its normalization action"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SerializedGreenWorkingCut {
    build: ArenaBuildId,
    installed_leaves_before: u64,
    events_before: u64,
    source_before: SerializedMetric,
}

#[cfg_attr(not(test), allow(dead_code))]
impl SerializedGreenWorkingCut {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub(crate) const fn installed_leaves_before(&self) -> u64 {
        self.installed_leaves_before
    }

    #[must_use]
    pub(crate) const fn events_before(&self) -> u64 {
        self.events_before
    }

    #[must_use]
    pub(crate) const fn source_before(&self) -> SerializedMetric {
        self.source_before
    }
}

/// Typed build-local ownership of the one installed source-ordered prefix.
/// The owner and decoded summary stay private so later leaf-repack jobs can
/// consume/replace this capability without exposing a forgeable raw root.
#[derive(Debug)]
pub(crate) struct SerializedGreenWorkingPrefix {
    build: ArenaBuildId,
    owner: ArenaBuildOwner,
    summary: GreenSummary,
}

/// Single-use authority for the exact Paragraph Enter most recently accepted
/// through the typed builder seam. The capability carries only logical build
/// coordinates; its physical leaf and byte location remain private inside the
/// builder and are updated when a partial page naturally seals.
#[must_use = "a provisional Paragraph Enter must be promoted or allowed to close"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProvisionalParagraphEnter {
    build: ArenaBuildId,
    block: BlockId,
    generation: u64,
    event_ordinal: u64,
    source_before: SerializedMetric,
}

impl ProvisionalParagraphEnter {
    pub(crate) const fn source_before(&self) -> SerializedMetric {
        self.source_before
    }
}

/// Green-coordinate portion of one already joined in-memory Setext sample.
/// Construction borrows the live provisional Paragraph token and the exact
/// force-sealed line cut; no caller can supply event/source ordinals.
#[must_use = "a retained Setext green draft must be validated against the committed Heading"]
#[derive(Debug)]
pub(crate) struct RetainedSetextGreenCheckpointDraft {
    old_build: ArenaBuildId,
    block: BlockId,
    target_event_ordinal: u64,
    target_source_before: SerializedMetric,
    accepted_event_cut: u64,
    accepted_source_cut: SerializedMetric,
}

impl RetainedSetextGreenCheckpointDraft {
    pub(crate) const fn old_build(&self) -> ArenaBuildId {
        self.old_build
    }

    pub(crate) const fn block(&self) -> BlockId {
        self.block
    }

    pub(crate) const fn accepted_source_cut(&self) -> SerializedMetric {
        self.accepted_source_cut
    }

    pub(crate) const fn accepted_event_cut(&self) -> u64 {
        self.accepted_event_cut
    }
}

/// Typed acknowledgement that the packed Enter descriptor and structural
/// validator now both name the same canonical Setext Heading.
#[must_use = "a Setext promotion acknowledgement must be joined by its writer group"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SetextPromotion {
    build: ArenaBuildId,
    retired_block: BlockId,
    block: BlockId,
    event_ordinal: u64,
    source_before: SerializedMetric,
    facts: GreenHeadingOpenFacts,
    storage: ProvisionalParagraphStorage,
}

impl SetextPromotion {
    pub(crate) const fn retired_block(&self) -> BlockId {
        self.retired_block
    }

    pub(crate) const fn replacement_block(&self) -> BlockId {
        self.block
    }

    #[cfg(test)]
    pub(crate) fn cross_identity_for_test(&mut self) {
        std::mem::swap(&mut self.retired_block, &mut self.block);
    }
}

/// Storage acknowledgement for one grammar-neutral replacement of the active
/// provisional terminal suffix.  The parser chooses the replacement events;
/// storage only proves that they replaced the exact provisional range,
/// preserved its complete physical metric, and left the declared replacement
/// root open.  Construct-specific meaning never enters the persistent splice.
#[must_use = "a canonical fragment replacement acknowledgement must be joined by its writer"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CanonicalFragmentReplacement {
    build: ArenaBuildId,
    retired_block: BlockId,
    replacement_block: BlockId,
    replacement_kind: GreenKind,
    removed_terminal: bool,
    physical_metric: SerializedMetric,
    retired_coverage_runs: u64,
    replacement_coverage_runs: u64,
}

impl CanonicalFragmentReplacement {
    #[cfg(test)]
    pub(crate) const fn mechanism_only_for_projection_rebase(
        build: ArenaBuildId,
        physical_metric: SerializedMetric,
        retired_coverage_runs: u64,
        replacement_coverage_runs: u64,
    ) -> Self {
        Self {
            build,
            retired_block: BlockId(1),
            replacement_block: BlockId(2),
            replacement_kind: GreenKind::TABLE,
            removed_terminal: false,
            physical_metric,
            retired_coverage_runs,
            replacement_coverage_runs,
        }
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn retired_block(&self) -> BlockId {
        self.retired_block
    }

    pub(crate) const fn replacement_block(&self) -> BlockId {
        self.replacement_block
    }

    pub(crate) const fn replacement_kind(&self) -> GreenKind {
        self.replacement_kind
    }

    /// True when the canonical fragment removed the provisional Paragraph
    /// and left its already-open parent as the structural continuation.
    pub(crate) const fn removed_terminal(&self) -> bool {
        self.removed_terminal
    }

    pub(crate) const fn physical_metric(&self) -> SerializedMetric {
        self.physical_metric
    }

    pub(crate) const fn retired_coverage_runs(&self) -> u64 {
        self.retired_coverage_runs
    }

    pub(crate) const fn replacement_coverage_runs(&self) -> u64 {
        self.replacement_coverage_runs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingProvisionalParagraph {
    build: ArenaBuildId,
    block: BlockId,
    generation: u64,
    event_ordinal: u64,
    source_before: SerializedMetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProvisionalParagraphStorage {
    Partial {
        byte_offset: u16,
        event_ordinal_in_leaf: u64,
        source_before_in_leaf: SerializedMetric,
    },
    Sealed {
        leaf_index: u64,
        byte_offset: u16,
        event_ordinal_in_leaf: u64,
        source_before_in_leaf: SerializedMetric,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveProvisionalParagraph {
    build: ArenaBuildId,
    block: BlockId,
    generation: u64,
    event_ordinal: u64,
    source_before: SerializedMetric,
    storage: ProvisionalParagraphStorage,
}

#[derive(Debug)]
struct SetextPromotionJob {
    active: ActiveProvisionalParagraph,
    replacement_block: BlockId,
    facts: GreenHeadingOpenFacts,
    encoded_enter: Vec<u8>,
    replacement_page_count: usize,
    next_replacement_page: usize,
    base_prefix_summary: Option<GreenSummary>,
    expected_prefix_leaves: Option<u64>,
    replacement_target: Option<ProvisionalParagraphStorage>,
}

/// Storage-private locator retained only while a restart-crossing Setext
/// promotion may still resolve either as one whole Heading or as a Heading
/// followed by the retired Paragraph residual.  The writer owns the linear
/// semantic authorities; this copy is merely the exact packed-page witness
/// needed to consume either outcome without searching the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeferredNormalizationGreenTarget {
    build: ArenaBuildId,
    retired_block: BlockId,
    replacement_block: BlockId,
    event_ordinal: u64,
    source_before: SerializedMetric,
    facts: GreenHeadingOpenFacts,
    storage: ProvisionalParagraphStorage,
}

#[derive(Debug)]
struct WholeNormalizationReidentityJob {
    authority: crate::ResolvedWholeNormalizationIdentity,
    storage: SetextPromotion,
    target: ProvisionalParagraphStorage,
    encoded_enter: Vec<u8>,
    base_prefix_summary: Option<GreenSummary>,
}

#[must_use = "the writer must consume completed whole-normalization storage authority"]
#[derive(Debug)]
pub(crate) struct WholeNormalizationReidentity {
    build: ArenaBuildId,
}

/// One streamed canonical-fragment replacement.  The old range is the suffix
/// beginning at the leaf containing `active`; the untouched prefix inside that
/// first leaf is copied into the replacement stream before parser events are
/// accepted.  No event vector or per-cell descriptor is retained.
#[derive(Debug)]
struct CanonicalFragmentReplacementJob {
    active: ActiveProvisionalParagraph,
    replacement_block: BlockId,
    replacement_kind: GreenKind,
    removed_terminal: bool,
    expected_physical: SerializedMetric,
    replacement_range: Option<Range<u64>>,
    base_prefix_summary: Option<GreenSummary>,
    untouched_summary: Option<GreenSummary>,
    replacement_pages: u64,
    replacement_summary: GreenSummary,
    replacement_events_offered: u64,
    replacement_metric_offered: SerializedMetric,
    surviving_paragraph: Option<FragmentSurvivingParagraph>,
    input_finished: bool,
}

/// Build-local coordinates of the surviving Paragraph Enter in a split
/// canonical-fragment replacement. The relative leaf location is captured
/// when the typed Enter is acknowledged and becomes a global installed-leaf
/// location only after the replacement splice succeeds.
#[derive(Debug)]
struct FragmentSurvivingParagraph {
    pending: PendingProvisionalParagraph,
    replacement_leaf_index: u64,
    byte_offset: u16,
    event_ordinal_in_leaf: u64,
    source_before_in_leaf: SerializedMetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SerializedGreenStreamPhase {
    Accepting,
    PushingForPendingEvent,
    FlushingBarrierLeaf,
    PushingBarrierLeaf,
    FlushingFinalLeaf,
    PushingFinalLeaf,
    ReducingWorkingTail,
    SplicingWorkingTail,
    FlushingSetextLeaf,
    PushingSetextLeaf,
    ReducingSetextTail,
    SplicingSetextTail,
    PreparingSetextRepack,
    AllocatingSetextReplacementLeaf,
    PushingSetextReplacementLeaf,
    ReducingSetextReplacement,
    SplicingSetextReplacement,
    ReducingWholeNormalizationTail,
    SplicingWholeNormalizationTail,
    PreparingWholeNormalizationRepack,
    AllocatingWholeNormalizationLeaf,
    PushingWholeNormalizationLeaf,
    ReducingWholeNormalizationReplacement,
    SplicingWholeNormalizationReplacement,
    FlushingFragmentBaseLeaf,
    PushingFragmentBaseLeaf,
    ReducingFragmentBaseTail,
    SplicingFragmentBaseTail,
    PreparingFragmentReplacement,
    AcceptingFragmentEvent,
    PushingFragmentEventLeaf,
    FlushingFinalFragmentLeaf,
    PushingFinalFragmentLeaf,
    ReducingFragmentReplacement,
    SplicingFragmentReplacement,
    FragmentReplacementReady,
    ReducingFinalTail,
    SplicingFinalTail,
    AllocatingManifest,
    ManifestReady,
    Failed,
}

#[derive(Debug)]
struct JournaledGreenEvent {
    bytes: Vec<u8>,
    program: Option<ArenaBuildOwner>,
    program_ordinal_offset: Option<usize>,
    summary: GreenSummary,
}

#[derive(Debug)]
struct JournaledLeafEncoder {
    bytes: Vec<u8>,
    byte_capacity: usize,
    summary: GreenSummary,
    programs: Vec<ArenaBuildOwner>,
    program_slot_limit: usize,
    program_capacity: usize,
}

impl JournaledLeafEncoder {
    fn try_new() -> Result<Self, SerializedGreenError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| SerializedGreenError::Invalid("green leaf reservation failed"))?;
        let byte_capacity = bytes.capacity();
        bytes.resize(LEAF_HEADER_BYTES, 0);
        let program_slot_limit = MAX_PACKED_ARENA_CHILDREN;
        let mut programs = Vec::new();
        programs
            .try_reserve_exact(program_slot_limit)
            .map_err(|_| SerializedGreenError::Invalid("green leaf owner reservation failed"))?;
        let program_capacity = programs.capacity();
        Ok(Self {
            bytes,
            byte_capacity,
            summary: GreenSummary::default(),
            programs,
            program_slot_limit,
            program_capacity,
        })
    }
    fn is_empty(&self) -> bool {
        self.summary.tokens == 0
    }

    fn can_fit(&self, event: &JournaledGreenEvent) -> bool {
        let program_count = self.programs.len() + usize::from(event.program.is_some());
        program_count <= MAX_PACKED_ARENA_CHILDREN
            && self.bytes.len() + event.bytes.len() + program_count * std::mem::size_of::<ArenaId>()
                <= ARENA_PAGE_BYTES
    }

    fn push(&mut self, mut event: JournaledGreenEvent) -> Result<(), SerializedGreenError> {
        self.require_fixed_capacity()?;
        if !self.can_fit(&event) {
            return Err(SerializedGreenError::Invalid("event exceeds green leaf"));
        }
        match (event.program.as_ref(), event.program_ordinal_offset) {
            (Some(_), Some(offset)) => {
                let ordinal = u8::try_from(self.programs.len()).map_err(|_| {
                    SerializedGreenError::Overflow("projection program edge ordinal")
                })?;
                let encoded = event
                    .bytes
                    .get_mut(offset)
                    .ok_or(SerializedGreenError::Corrupt(
                        "program ordinal offset escapes event",
                    ))?;
                // A leaf has at most 128 child edges, so the ordinal is always
                // a canonical one-byte varint and patching cannot change fit.
                *encoded = ordinal;
            }
            (None, None) => {}
            _ => {
                return Err(SerializedGreenError::Corrupt(
                    "program owner and ordinal offset disagree",
                ));
            }
        }
        self.bytes.extend_from_slice(&event.bytes);
        if let Some(program) = event.program.take() {
            if self.programs.len() >= self.program_slot_limit {
                return Err(SerializedGreenError::Invalid(
                    "green leaf exceeded logical owner bound",
                ));
            }
            self.programs.push(program);
        }
        self.summary = self.summary.followed_by(event.summary)?;
        self.require_fixed_capacity()?;
        Ok(())
    }

    fn seal_in_place(&mut self) -> Result<(), SerializedGreenError> {
        if self.is_empty() {
            return Err(SerializedGreenError::Invalid("empty green leaf"));
        }
        self.summary.leaves = 1;
        self.summary.height = 1;
        let header = encode_summary(LEAF_TAG, self.summary);
        self.bytes[..LEAF_HEADER_BYTES].copy_from_slice(&header);
        Ok(())
    }

    fn reset_after_allocation(&mut self) {
        self.bytes.truncate(LEAF_HEADER_BYTES);
        self.bytes[..LEAF_HEADER_BYTES].fill(0);
        self.summary = GreenSummary::default();
        debug_assert!(self.programs.is_empty());
        debug_assert_eq!(self.bytes.capacity(), self.byte_capacity);
        debug_assert_eq!(self.programs.capacity(), self.program_capacity);
    }

    fn require_fixed_capacity(&self) -> Result<(), SerializedGreenError> {
        if self.bytes.capacity() != self.byte_capacity
            || self.programs.capacity() != self.program_capacity
        {
            return Err(SerializedGreenError::Corrupt(
                "green leaf scratch capacity changed",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct PreparedSetextLeaf {
    bytes: Vec<u8>,
    byte_capacity: usize,
    summary: GreenSummary,
    programs: Vec<ArenaId>,
    program_capacity: usize,
    sealed: bool,
}

impl PreparedSetextLeaf {
    fn try_new() -> Result<Self, SerializedGreenError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| SerializedGreenError::Invalid("Setext page reservation failed"))?;
        let byte_capacity = bytes.capacity();
        bytes.resize(LEAF_HEADER_BYTES, 0);
        let mut programs = Vec::new();
        programs
            .try_reserve_exact(MAX_PACKED_ARENA_CHILDREN)
            .map_err(|_| SerializedGreenError::Invalid("Setext edge reservation failed"))?;
        let program_capacity = programs.capacity();
        Ok(Self {
            bytes,
            byte_capacity,
            summary: GreenSummary::default(),
            programs,
            program_capacity,
            sealed: false,
        })
    }

    fn reset(&mut self) -> Result<(), SerializedGreenError> {
        self.require_fixed_capacity()?;
        self.bytes.truncate(LEAF_HEADER_BYTES);
        self.bytes[..LEAF_HEADER_BYTES].fill(0);
        self.summary = GreenSummary::default();
        self.programs.clear();
        self.sealed = false;
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.summary.tokens == 0
    }

    fn can_fit(&self, event_bytes: usize, has_program: bool) -> bool {
        let program_count = self.programs.len() + usize::from(has_program);
        !self.sealed
            && program_count <= MAX_PACKED_ARENA_CHILDREN
            && self.bytes.len() + event_bytes + program_count * std::mem::size_of::<ArenaId>()
                <= ARENA_PAGE_BYTES
    }

    fn push_raw(
        &mut self,
        raw: &[u8],
        event_summary: GreenSummary,
        program: Option<(ArenaId, usize)>,
    ) -> Result<(), SerializedGreenError> {
        self.require_fixed_capacity()?;
        if !self.can_fit(raw.len(), program.is_some()) {
            return Err(SerializedGreenError::Invalid(
                "Setext replacement event exceeds green leaf",
            ));
        }
        let start = self.bytes.len();
        self.bytes.extend_from_slice(raw);
        if let Some((program, ordinal_offset)) = program {
            let ordinal = u8::try_from(self.programs.len())
                .map_err(|_| SerializedGreenError::Overflow("Setext projection edge ordinal"))?;
            let offset =
                start
                    .checked_add(ordinal_offset)
                    .ok_or(SerializedGreenError::Overflow(
                        "Setext program ordinal offset",
                    ))?;
            *self
                .bytes
                .get_mut(offset)
                .ok_or(SerializedGreenError::Corrupt(
                    "Setext Program ordinal escapes event",
                ))? = ordinal;
            self.programs.push(program);
        }
        self.summary = self.summary.followed_by(event_summary)?;
        self.require_fixed_capacity()?;
        Ok(())
    }

    fn seal(&mut self) -> Result<(), SerializedGreenError> {
        self.require_fixed_capacity()?;
        if self.is_empty() || self.sealed {
            return Err(SerializedGreenError::Invalid(
                "Setext replacement leaf cannot be sealed",
            ));
        }
        self.summary.leaves = 1;
        self.summary.height = 1;
        let header = encode_summary(LEAF_TAG, self.summary);
        self.bytes[..LEAF_HEADER_BYTES].copy_from_slice(&header);
        self.sealed = true;
        Ok(())
    }

    fn require_fixed_capacity(&self) -> Result<(), SerializedGreenError> {
        if self.bytes.capacity() != self.byte_capacity
            || self.programs.capacity() != self.program_capacity
        {
            return Err(SerializedGreenError::Corrupt(
                "Setext repack scratch capacity changed",
            ));
        }
        Ok(())
    }

    fn scratch_bytes(&self) -> usize {
        self.bytes.capacity() + self.programs.capacity() * std::mem::size_of::<ArenaId>()
    }
}

#[derive(Debug)]
struct SetextRepackScratch {
    pages: [PreparedSetextLeaf; 2],
    page_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SetextRepackTargetLocation {
    page_index: usize,
    byte_offset: u16,
    event_ordinal_in_leaf: u64,
    source_before_in_leaf: SerializedMetric,
}

impl SetextRepackScratch {
    fn try_new() -> Result<Self, SerializedGreenError> {
        Ok(Self {
            pages: [
                PreparedSetextLeaf::try_new()?,
                PreparedSetextLeaf::try_new()?,
            ],
            page_count: 0,
        })
    }

    fn reset(&mut self) -> Result<(), SerializedGreenError> {
        for page in &mut self.pages {
            page.reset()?;
        }
        self.page_count = 0;
        Ok(())
    }

    fn scratch_bytes(&self) -> usize {
        self.pages
            .iter()
            .map(PreparedSetextLeaf::scratch_bytes)
            .sum()
    }
}

/// Mechanism-level, source-spec-driven proof builder for packed green output.
///
/// This is intentionally not the production parser authority seam: callers
/// can provide a raw [`SerializedGreenRootSpec`]. Production composition must
/// wrap it in the source-bound candidate writer that derives the spec from its
/// private source lease. The builder itself proves the storage mechanism: it
/// accepts one event at a time, is bound to one arena build generation, and
/// retains no typed or mirrored document-sized event representation. Polling
/// uses preflighted leaf/bin/join scratch; `offer_event` remains a separately
/// accounted input kernel with fresh descriptor/facts buffers.
#[derive(Debug)]
pub struct ResumableSerializedGreenBuild {
    build: ArenaBuildId,
    spec: SerializedGreenRootSpec,
    phase: SerializedGreenStreamPhase,
    validator: StructuralValidator,
    leaf: JournaledLeafEncoder,
    pending_event: Option<JournaledGreenEvent>,
    pending_provisional_paragraph: Option<PendingProvisionalParagraph>,
    ready_provisional_paragraph: Option<ProvisionalParagraphEnter>,
    active_provisional_paragraph: Option<ActiveProvisionalParagraph>,
    next_provisional_generation: u64,
    setext_job: Option<SetextPromotionJob>,
    ready_setext_promotion: Option<SetextPromotion>,
    deferred_normalization_target: Option<DeferredNormalizationGreenTarget>,
    whole_normalization_job: Option<WholeNormalizationReidentityJob>,
    ready_whole_normalization: Option<WholeNormalizationReidentity>,
    fragment_job: Option<CanonicalFragmentReplacementJob>,
    ready_fragment_replacement: Option<CanonicalFragmentReplacement>,
    setext_scratch: SetextRepackScratch,
    pending_barrier_cut: Option<SerializedGreenLeafCut>,
    ready_barrier_cut: Option<SerializedGreenLeafCut>,
    ready_working_cut: Option<SerializedGreenWorkingCut>,
    sealed_leaves: u64,
    sealed_events: u64,
    sealed_metric: SerializedMetric,
    tail_sealed_leaves: u64,
    tail_summary: GreenSummary,
    working_prefix: Option<SerializedGreenWorkingPrefix>,
    pending_splice_summary: Option<GreenSummary>,
    sequence: ResumableStreamingSequenceBuilder<SerializedGreenSpec>,
    splice: ResumableSequenceSplice<SerializedGreenSpec>,
    sequence_receipt: SequenceMutationReceipt,
    manifest: Option<ArenaBuildOwner>,
    #[cfg(feature = "host-mirror-probe")]
    retained_host_prefix: Option<crate::host_mirror::CanonicalRetainedGreenPrefixSeed>,
    receipt: SerializedGreenBuildReceipt,
}

impl ResumableSerializedGreenBuild {
    /// Binds mechanism state to the exact build generation named by `ticket`.
    /// The ticket remains linear and must subsequently be consumed by
    /// [`PageArena::resume_build`].
    pub fn new(
        ticket: &ArenaBuildTicket,
        spec: SerializedGreenRootSpec,
    ) -> Result<Self, SerializedGreenError> {
        validate_root_spec(&spec)?;
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let sequence = ResumableStreamingSequenceBuilder::try_new(&mut sequence_receipt)?;
        let splice = ResumableSequenceSplice::try_preallocated_for_build(
            ticket.id(),
            &mut sequence_receipt,
        )?;
        let mut build = Self {
            build: ticket.id(),
            spec,
            phase: SerializedGreenStreamPhase::Accepting,
            validator: StructuralValidator::default(),
            leaf: JournaledLeafEncoder::try_new()?,
            pending_event: None,
            pending_provisional_paragraph: None,
            ready_provisional_paragraph: None,
            active_provisional_paragraph: None,
            next_provisional_generation: 1,
            setext_job: None,
            ready_setext_promotion: None,
            deferred_normalization_target: None,
            whole_normalization_job: None,
            ready_whole_normalization: None,
            fragment_job: None,
            ready_fragment_replacement: None,
            setext_scratch: SetextRepackScratch::try_new()?,
            pending_barrier_cut: None,
            ready_barrier_cut: None,
            ready_working_cut: None,
            sealed_leaves: 0,
            sealed_events: 0,
            sealed_metric: SerializedMetric::default(),
            tail_sealed_leaves: 0,
            tail_summary: GreenSummary::default(),
            working_prefix: None,
            pending_splice_summary: None,
            sequence,
            splice,
            sequence_receipt,
            manifest: None,
            #[cfg(feature = "host-mirror-probe")]
            retained_host_prefix: None,
            receipt: SerializedGreenBuildReceipt::default(),
        };
        build.record_scratch();
        Ok(build)
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    /// Returns an honest snapshot, including allocated journal capacity rather
    /// than only its logical live-owner count.
    #[must_use]
    pub fn receipt(&self) -> SerializedGreenBuildReceipt {
        let mut receipt = self.receipt;
        merge_sequence_receipt(&mut receipt, self.sequence_receipt);
        receipt.resumable_arena_allocations += self.sequence_receipt.branches_allocated;
        receipt
    }

    /// Validates and encodes exactly one event. A Program payload, if present,
    /// is copied directly into the active build journal in this call; the
    /// partial leaf retains only its typed arena owner.
    ///
    /// This input kernel is not yet heap-preflighted: it creates one encoded
    /// descriptor `Vec`, plus a temporary facts `Vec` for an Enter with facts.
    /// Those buffers are fully created here and never grow in [`Self::poll`].
    pub fn offer_event(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        self.leaf.require_fixed_capacity()?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "builder is not ready for another event",
            ));
        }
        let result = self.encode_and_journal_event(session, event);
        if result.is_err() {
            self.phase = SerializedGreenStreamPhase::Failed;
        }
        self.sync_journal_receipt(session)?;
        result
    }

    /// Offers the one Paragraph Enter that may later be retyped to Setext.
    /// The ordinary sink acknowledgement must arrive before the matching
    /// capability can be taken, so no token can name an event that failed to
    /// enter the active partial page.
    pub(crate) fn offer_provisional_paragraph_enter(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        block: BlockId,
        facts: FactsEnvelope,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        self.leaf.require_fixed_capacity()?;
        if !facts.fields.is_empty() {
            return Err(SerializedGreenError::Invalid(
                "provisional Paragraph Enter facts must be empty",
            ));
        }
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.active_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "builder is not ready for a provisional Paragraph Enter",
            ));
        }
        let event_ordinal = self
            .sealed_events
            .checked_add(self.leaf.summary.tokens)
            .ok_or(SerializedGreenError::Overflow(
                "provisional Paragraph event ordinal",
            ))?;
        let source_before = self.sealed_metric.checked_add(self.leaf.summary.metric)?;
        let generation = self.next_provisional_generation;
        let next_generation = generation
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "provisional Paragraph generation",
            ))?;
        let pending = PendingProvisionalParagraph {
            build: self.build,
            block,
            generation,
            event_ordinal,
            source_before,
        };
        let result = self.encode_and_journal_event(
            session,
            GreenEvent::enter(block, GreenKind::PARAGRAPH, facts),
        );
        if result.is_ok() {
            self.pending_provisional_paragraph = Some(pending);
            self.next_provisional_generation = next_generation;
            self.receipt.provisional_paragraph_enters_offered = self
                .receipt
                .provisional_paragraph_enters_offered
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "provisional Paragraph offer count",
                ))?;
        } else {
            self.phase = SerializedGreenStreamPhase::Failed;
        }
        self.sync_journal_receipt(session)?;
        result
    }

    /// Takes the linear capability only after `poll` acknowledged the typed
    /// Enter. Requiring the matching live session prevents a useful authority
    /// token from being extracted after cancellation consumed the build lease.
    pub(crate) fn take_provisional_paragraph_enter(
        &mut self,
        session: &ArenaBuildSession<'_>,
        block: BlockId,
    ) -> Result<ProvisionalParagraphEnter, SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting || self.pending_event.is_some() {
            return Err(SerializedGreenError::Invalid(
                "provisional Paragraph Enter is not acknowledged",
            ));
        }
        let ready =
            self.ready_provisional_paragraph
                .as_ref()
                .ok_or(SerializedGreenError::Invalid(
                    "no provisional Paragraph Enter is ready",
                ))?;
        let active =
            self.active_provisional_paragraph
                .as_ref()
                .ok_or(SerializedGreenError::Corrupt(
                    "ready provisional Paragraph lost its storage record",
                ))?;
        if ready.block != block
            || ready.build != self.build
            || active.block != ready.block
            || active.generation != ready.generation
            || active.event_ordinal != ready.event_ordinal
            || active.source_before != ready.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        self.ready_provisional_paragraph
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "ready provisional Paragraph disappeared",
            ))
    }

    /// Begins the exact Paragraph-to-Setext storage transition. A seven-byte
    /// in-place expansion completes immediately when the active page has room;
    /// otherwise ordinary `poll` calls force-seal, repack one leaf into at
    /// most two leaves, and install the result through the owned splice.
    #[allow(clippy::needless_pass_by_value, clippy::too_many_lines)] // Consuming the token is the replay boundary; one visible admission transaction.
    pub(crate) fn begin_setext_promotion(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), SerializedGreenError> {
        let replacement_block = token.block;
        self.begin_setext_promotion_replacing(session, token, replacement_block, facts)
    }

    /// Same bounded Paragraph-to-Heading Enter repack, but the Heading
    /// identity comes from a writer-minted fresh permit. The retired Paragraph
    /// identity is never accepted as a raw caller value and can survive only
    /// through the source-ledger's matching deferred-normalization token.
    pub(crate) fn begin_reidentified_setext_promotion(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        replacement_block: BlockId,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), SerializedGreenError> {
        if replacement_block.0 == 0 || replacement_block == token.block {
            return Err(SerializedGreenError::Invalid(
                "reidentified Setext promotion requires a distinct fresh identity",
            ));
        }
        self.begin_setext_promotion_replacing(session, token, replacement_block, facts)
    }

    #[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
    fn begin_setext_promotion_replacing(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        replacement_block: BlockId,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if facts.style() != GreenHeadingStyle::Setext {
            return Err(SerializedGreenError::Invalid(
                "Setext promotion requires Setext Heading facts",
            ));
        }
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.pending_barrier_cut.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "builder is not ready for Setext promotion",
            ));
        }
        let active = self
            .active_provisional_paragraph
            .as_ref()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if token.build != self.build
            || token.block != active.block
            || token.generation != active.generation
            || token.event_ordinal != active.event_ordinal
            || token.source_before != active.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }

        let encoded = encode_event(
            &GreenEvent::enter(replacement_block, GreenKind::HEADING, facts.into_envelope()),
            0,
        )?;
        if encoded.program.is_some() || encoded.program_ordinal_offset.is_some() {
            return Err(SerializedGreenError::Corrupt(
                "Heading Enter unexpectedly owns a Program edge",
            ));
        }
        let paragraph = encode_event(
            &GreenEvent::enter(token.block, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            0,
        )?;
        if encoded.bytes.len() != paragraph.bytes.len() + 7 {
            return Err(SerializedGreenError::Corrupt(
                "canonical Setext Enter is not an exact seven-byte expansion",
            ));
        }
        self.setext_scratch.reset()?;

        let active = self
            .active_provisional_paragraph
            .take()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if matches!(active.storage, ProvisionalParagraphStorage::Partial { .. })
            && self.partial_setext_can_fit(encoded.bytes.len() - paragraph.bytes.len())
        {
            let result = self
                .rewrite_partial_setext(active, &encoded.bytes)
                .and_then(|()| {
                    self.validator.retype_active_terminal(
                        active.block,
                        replacement_block,
                        GreenKind::PARAGRAPH,
                        GreenKind::HEADING,
                        facts.into_envelope(),
                    )
                });
            if let Err(error) = result {
                self.phase = SerializedGreenStreamPhase::Failed;
                return Err(error);
            }
            self.ready_setext_promotion = Some(SetextPromotion {
                build: self.build,
                retired_block: active.block,
                block: replacement_block,
                event_ordinal: active.event_ordinal,
                source_before: active.source_before,
                facts,
                storage: active.storage,
            });
            self.receipt.setext_partial_promotions_completed = self
                .receipt
                .setext_partial_promotions_completed
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "partial Setext promotion count",
                ))?;
            self.record_scratch();
            self.sync_journal_receipt(session)?;
            return Ok(());
        }

        let storage = active.storage;
        self.setext_job = Some(SetextPromotionJob {
            active,
            replacement_block,
            facts,
            encoded_enter: encoded.bytes,
            replacement_page_count: 0,
            next_replacement_page: 0,
            base_prefix_summary: None,
            expected_prefix_leaves: None,
            replacement_target: None,
        });
        match storage {
            ProvisionalParagraphStorage::Partial { .. } => {
                self.receipt.setext_capacity_cliff_force_seals = self
                    .receipt
                    .setext_capacity_cliff_force_seals
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow(
                        "Setext capacity-cliff seal count",
                    ))?;
                self.phase = SerializedGreenStreamPhase::FlushingSetextLeaf;
            }
            ProvisionalParagraphStorage::Sealed { .. } if self.tail_sealed_leaves != 0 => {
                self.sequence.begin_finish(&mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::ReducingSetextTail;
            }
            ProvisionalParagraphStorage::Sealed { .. } => {
                if self.working_prefix.is_none() {
                    self.phase = SerializedGreenStreamPhase::Failed;
                    return Err(SerializedGreenError::Corrupt(
                        "sealed Setext target has no working prefix or tail",
                    ));
                }
                self.phase = SerializedGreenStreamPhase::PreparingSetextRepack;
            }
        }
        self.record_scratch();
        self.sync_journal_receipt(session)?;
        Ok(())
    }

    /// Consumes the storage acknowledgement after `poll` returns
    /// `ReadyForEvent`. The builder remains blocked until this typed result is
    /// joined by the writer's ledger/composer transition.
    pub(crate) fn take_setext_promotion(
        &mut self,
        session: &ArenaBuildSession<'_>,
        block: BlockId,
    ) -> Result<SetextPromotion, SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting {
            return Err(SerializedGreenError::Invalid(
                "Setext promotion is not ready",
            ));
        }
        let ready = self
            .ready_setext_promotion
            .as_ref()
            .ok_or(SerializedGreenError::Invalid(
                "no Setext promotion acknowledgement is ready",
            ))?;
        if ready.build != self.build || ready.block != block {
            return Err(SerializedGreenError::StaleCursor);
        }
        self.ready_setext_promotion
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "ready Setext promotion disappeared",
            ))
    }

    /// Retains only storage's exact packed-page locator after the writer has
    /// joined a restart-crossing Setext promotion to the ledger's linear
    /// deferred identity.  The semantic tokens stay writer-owned; this copy
    /// cannot authorize either outcome by itself.
    pub(crate) fn retain_deferred_normalization_target(
        &mut self,
        session: &ArenaBuildSession<'_>,
        storage: &SetextPromotion,
        identity: &crate::DeferredNormalizationIdentity,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.ready_setext_promotion.is_some()
            || self.deferred_normalization_target.is_some()
            || storage.build != self.build
            || identity.build_id() != self.build
            || storage.retired_block != identity.retired_block()
            || storage.block != identity.replacement_block()
            || storage.block == storage.retired_block
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        if let ProvisionalParagraphStorage::Partial {
            event_ordinal_in_leaf,
            source_before_in_leaf,
            ..
        } = storage.storage
            && (self
                .sealed_events
                .checked_add(event_ordinal_in_leaf)
                .ok_or(SerializedGreenError::Overflow(
                    "deferred normalization event ordinal",
                ))?
                != storage.event_ordinal
                || self.sealed_metric.checked_add(source_before_in_leaf)? != storage.source_before)
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        self.deferred_normalization_target = Some(DeferredNormalizationGreenTarget {
            build: storage.build,
            retired_block: storage.retired_block,
            replacement_block: storage.block,
            event_ordinal: storage.event_ordinal,
            source_before: storage.source_before,
            facts: storage.facts,
            storage: storage.storage,
        });
        Ok(())
    }

    /// Consumes storage's pending-whole locator when lookahead instead reopens
    /// the retired Paragraph residual.  The new Paragraph Enter remains an
    /// ordinary provisional event; no Heading identity is rewritten.
    pub(crate) fn offer_deferred_normalization_paragraph_enter(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        identity: &crate::DeferredNormalizationIdentity,
        storage: &SetextPromotion,
    ) -> Result<(), SerializedGreenError> {
        let target = self
            .deferred_normalization_target
            .as_ref()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if target.build != self.build
            || identity.build_id() != self.build
            || storage.build != self.build
            || identity.survivor_kind() != GreenKind::PARAGRAPH
            || target.retired_block != identity.retired_block()
            || target.replacement_block != identity.replacement_block()
            || storage.retired_block != target.retired_block
            || storage.block != target.replacement_block
            || storage.event_ordinal != target.event_ordinal
            || storage.source_before != target.source_before
            || storage.facts != target.facts
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        self.deferred_normalization_target
            .take()
            .ok_or(SerializedGreenError::StaleCursor)?;
        self.offer_provisional_paragraph_enter(
            session,
            identity.retired_block(),
            FactsEnvelope::empty(),
        )
    }

    /// Consumes the ledger's resolved-whole identity together with the exact
    /// Setext storage acknowledgement.  A partial-page target is rewritten in
    /// this bounded call; a sealed target starts the ordinary one-leaf,
    /// journalled persistent splice and completes through `poll`.
    pub(crate) fn begin_whole_normalization_reidentity(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        authority: crate::ResolvedWholeNormalizationIdentity,
        storage: SetextPromotion,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.active_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "builder is not ready for whole normalization reidentity",
            ));
        }
        let target = self
            .deferred_normalization_target
            .as_ref()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if authority.build_id() != self.build
            || authority.kind() != GreenKind::HEADING
            || storage.build != self.build
            || authority.retired_block() != storage.retired_block
            || authority.replacement_block() != storage.block
            || target.build != self.build
            || target.retired_block != storage.retired_block
            || target.replacement_block != storage.block
            || target.event_ordinal != storage.event_ordinal
            || target.source_before != storage.source_before
            || target.facts != storage.facts
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let encoded_enter = encode_event(
            &GreenEvent::enter(
                authority.retired_block(),
                GreenKind::HEADING,
                storage.facts.into_envelope(),
            ),
            0,
        )?;
        let encoded_replacement = encode_event(
            &GreenEvent::enter(
                authority.replacement_block(),
                GreenKind::HEADING,
                storage.facts.into_envelope(),
            ),
            0,
        )?;
        if encoded_enter.bytes.len() != encoded_replacement.bytes.len()
            || encoded_enter.program.is_some()
            || encoded_replacement.program.is_some()
        {
            return Err(SerializedGreenError::Corrupt(
                "whole normalization changed the Heading descriptor width",
            ));
        }
        let target = self
            .deferred_normalization_target
            .take()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if matches!(target.storage, ProvisionalParagraphStorage::Partial { .. }) {
            self.rewrite_partial_whole_normalization(
                target,
                &encoded_replacement.bytes,
                &encoded_enter.bytes,
            )?;
            self.ready_whole_normalization =
                Some(WholeNormalizationReidentity { build: self.build });
            self.sync_journal_receipt(session)?;
            return Ok(());
        }
        self.setext_scratch.reset()?;
        self.whole_normalization_job = Some(WholeNormalizationReidentityJob {
            authority,
            storage,
            target: target.storage,
            encoded_enter: encoded_enter.bytes,
            base_prefix_summary: None,
        });
        if self.tail_sealed_leaves != 0 {
            self.sequence.begin_finish(&mut self.sequence_receipt)?;
            self.phase = SerializedGreenStreamPhase::ReducingWholeNormalizationTail;
        } else if self.working_prefix.is_some() {
            self.phase = SerializedGreenStreamPhase::PreparingWholeNormalizationRepack;
        } else {
            return Err(SerializedGreenError::Corrupt(
                "sealed whole-normalization target has no working prefix or tail",
            ));
        }
        self.record_scratch();
        self.sync_journal_receipt(session)?;
        Ok(())
    }

    pub(crate) fn take_whole_normalization_reidentity(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<WholeNormalizationReidentity, SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.whole_normalization_job.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "whole normalization reidentity is not ready",
            ));
        }
        let ready = self
            .ready_whole_normalization
            .take()
            .ok_or(SerializedGreenError::Invalid(
                "no whole normalization reidentity is ready",
            ))?;
        if ready.build != self.build {
            return Err(SerializedGreenError::StaleCursor);
        }
        Ok(ready)
    }

    /// Begins a grammar-neutral replacement of the still-open provisional
    /// Paragraph suffix.  The replacement root identity/kind and complete
    /// physical extent are typed writer facts; canonical events arrive later
    /// one at a time.  Storage never receives a table/reference/etc. outcome
    /// tag and ultimately installs through `begin_canonical_leaf_replacement`.
    pub(crate) fn begin_canonical_fragment_replacement(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        replacement_block: BlockId,
        replacement_kind: GreenKind,
        expected_physical: SerializedMetric,
    ) -> Result<(), SerializedGreenError> {
        self.begin_canonical_fragment_rewrite(
            session,
            token,
            replacement_block,
            replacement_kind,
            expected_physical,
            false,
        )
    }

    /// Removes the provisional Paragraph while preserving its complete source
    /// as canonical parent-owned coverage. This is the grammar-neutral empty
    /// counterpart to [`Self::begin_canonical_fragment_replacement`]; the
    /// parser-selected reference terminal policy remains outside storage.
    pub(crate) fn begin_canonical_fragment_removal(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        parent_block: BlockId,
        parent_kind: GreenKind,
        expected_physical: SerializedMetric,
    ) -> Result<(), SerializedGreenError> {
        self.begin_canonical_fragment_rewrite(
            session,
            token,
            parent_block,
            parent_kind,
            expected_physical,
            true,
        )
    }

    fn begin_canonical_fragment_rewrite(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        replacement_block: BlockId,
        replacement_kind: GreenKind,
        expected_physical: SerializedMetric,
        removed_terminal: bool,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.pending_barrier_cut.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
            || replacement_block.0 == 0
            || (!removed_terminal && replacement_kind == GreenKind::DOCUMENT)
            || (removed_terminal && replacement_kind.logical_channel().is_some())
            || (!removed_terminal
                && replacement_kind.logical_channel().is_some()
                && (replacement_kind != GreenKind::PARAGRAPH || replacement_block != token.block))
            || expected_physical.is_zero()
            || expected_physical.is_partially_zero()
        {
            return Err(SerializedGreenError::Invalid(
                "builder is not ready for canonical fragment replacement",
            ));
        }
        let active = self
            .active_provisional_paragraph
            .as_ref()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if token.build != self.build
            || token.block != active.block
            || token.generation != active.generation
            || token.event_ordinal != active.event_ordinal
            || token.source_before != active.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let current_source = self.sealed_metric.checked_add(self.leaf.summary.metric)?;
        if current_source.checked_sub(active.source_before)? != expected_physical {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment physical extent disagrees with accepted source",
            ));
        }

        if removed_terminal {
            self.validator.validate_active_terminal_fragment_parent(
                active.block,
                replacement_block,
                replacement_kind,
            )?;
        }
        self.validator
            .begin_active_terminal_fragment_replacement(active.block)?;
        if removed_terminal {
            self.validator
                .validate_fragment_parent(replacement_block, replacement_kind)?;
        }
        let active = self
            .active_provisional_paragraph
            .take()
            .ok_or(SerializedGreenError::StaleCursor)?;
        let storage = active.storage;
        self.fragment_job = Some(CanonicalFragmentReplacementJob {
            active,
            replacement_block,
            replacement_kind,
            removed_terminal,
            expected_physical,
            replacement_range: None,
            base_prefix_summary: None,
            untouched_summary: None,
            replacement_pages: 0,
            replacement_summary: GreenSummary::default(),
            replacement_events_offered: 0,
            replacement_metric_offered: SerializedMetric::default(),
            surviving_paragraph: None,
            input_finished: false,
        });
        match storage {
            ProvisionalParagraphStorage::Partial { .. } => {
                self.phase = SerializedGreenStreamPhase::FlushingFragmentBaseLeaf;
            }
            ProvisionalParagraphStorage::Sealed { .. } if self.tail_sealed_leaves != 0 => {
                self.sequence.begin_finish(&mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::ReducingFragmentBaseTail;
            }
            ProvisionalParagraphStorage::Sealed { .. } => {
                if self.working_prefix.is_none() {
                    self.phase = SerializedGreenStreamPhase::Failed;
                    return Err(SerializedGreenError::Corrupt(
                        "sealed fragment target has no working prefix or tail",
                    ));
                }
                self.phase = SerializedGreenStreamPhase::PreparingFragmentReplacement;
            }
        }
        self.record_scratch();
        self.sync_journal_receipt(session)
    }

    /// Accepts exactly one parser-authored canonical event for the active
    /// fragment.  The ordinary structural validator and Program-page journal
    /// are reused; only the destination sequence differs until atomic splice.
    pub(crate) fn offer_canonical_fragment_event(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        self.leaf.require_fixed_capacity()?;
        if self.phase != SerializedGreenStreamPhase::AcceptingFragmentEvent
            || self.fragment_job.is_none()
            || self.pending_event.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment is not ready for another event",
            ));
        }
        let job = self
            .fragment_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment offer lost its job",
            ))?;
        if job.removed_terminal
            && !matches!(
                &event,
                GreenEvent::Coverage(run)
                    if matches!(run.logical_contribution, LogicalContribution::None)
            )
        {
            return Err(SerializedGreenError::Invalid(
                "removed fragment accepts only physical parent-owned coverage",
            ));
        }
        if (job.surviving_paragraph.is_some() && !matches!(&event, GreenEvent::Coverage(_)))
            || self.pending_provisional_paragraph.is_some()
            || (job.replacement_kind == GreenKind::PARAGRAPH
                && matches!(
                    &event,
                    GreenEvent::Enter { block, kind, .. }
                        if *block == job.replacement_block && *kind == GreenKind::PARAGRAPH
                ))
        {
            return Err(SerializedGreenError::Invalid(
                "surviving fragment Paragraph requires its typed Enter seam",
            ));
        }
        let summary = GreenSummary::event(&event);
        let result = self.encode_and_journal_event(session, event);
        if result.is_ok() {
            let job = self
                .fragment_job
                .as_mut()
                .ok_or(SerializedGreenError::Corrupt(
                    "canonical fragment offer lost its job",
                ))?;
            job.replacement_events_offered = job.replacement_events_offered.checked_add(1).ok_or(
                SerializedGreenError::Overflow("canonical fragment offered event count"),
            )?;
            job.replacement_metric_offered =
                job.replacement_metric_offered.checked_add(summary.metric)?;
        }
        if result.is_err() {
            self.phase = SerializedGreenStreamPhase::Failed;
        }
        self.sync_journal_receipt(session)?;
        result
    }

    /// Offers the sole surviving Paragraph Enter for a split canonical
    /// fragment. The identity is derived from the consumed provisional
    /// Paragraph rather than accepted from the caller. The resulting token is
    /// minted only after the replacement splice installs this exact Enter.
    pub(crate) fn offer_canonical_fragment_surviving_paragraph_enter(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        self.leaf.require_fixed_capacity()?;
        if self.phase != SerializedGreenStreamPhase::AcceptingFragmentEvent
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment is not ready for a surviving Paragraph",
            ));
        }
        let job = self
            .fragment_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "surviving fragment Paragraph lost its job",
            ))?;
        if job.replacement_kind != GreenKind::PARAGRAPH
            || job.replacement_block != job.active.block
            || job.surviving_paragraph.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment does not preserve its Paragraph identity",
            ));
        }
        let event_ordinal = job
            .active
            .event_ordinal
            .checked_add(job.replacement_events_offered)
            .ok_or(SerializedGreenError::Overflow(
                "surviving fragment Paragraph event ordinal",
            ))?;
        let source_before = job
            .active
            .source_before
            .checked_add(job.replacement_metric_offered)?;
        let generation = self.next_provisional_generation;
        let next_generation = generation
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "surviving provisional Paragraph generation",
            ))?;
        let pending = PendingProvisionalParagraph {
            build: self.build,
            block: job.active.block,
            generation,
            event_ordinal,
            source_before,
        };
        let event = GreenEvent::enter(
            job.active.block,
            GreenKind::PARAGRAPH,
            FactsEnvelope::empty(),
        );
        let result = self.encode_and_journal_event(session, event);
        if result.is_ok() {
            self.pending_provisional_paragraph = Some(pending);
            self.next_provisional_generation = next_generation;
            let job = self
                .fragment_job
                .as_mut()
                .ok_or(SerializedGreenError::Corrupt(
                    "surviving fragment Paragraph lost its job",
                ))?;
            job.replacement_events_offered = job.replacement_events_offered.checked_add(1).ok_or(
                SerializedGreenError::Overflow("canonical fragment offered event count"),
            )?;
            self.receipt.provisional_paragraph_enters_offered = self
                .receipt
                .provisional_paragraph_enters_offered
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "provisional Paragraph offer count",
                ))?;
        } else {
            self.phase = SerializedGreenStreamPhase::Failed;
        }
        self.sync_journal_receipt(session)?;
        result
    }

    /// Closes parser input while deliberately leaving the replacement root
    /// open for later source-ledger continuation (for example a Table awaiting
    /// its delimiter/body rows).
    pub(crate) fn finish_canonical_fragment_replacement(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::AcceptingFragmentEvent
            || self.pending_event.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment still has unacknowledged input",
            ));
        }
        let job = self
            .fragment_job
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment finish lost its job",
            ))?;
        if (job.replacement_kind == GreenKind::PARAGRAPH) != job.surviving_paragraph.is_some()
            || self.pending_provisional_paragraph.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment survivor is not acknowledged",
            ));
        }
        if job.removed_terminal {
            self.validator
                .validate_fragment_parent(job.replacement_block, job.replacement_kind)?;
        } else {
            self.validator
                .validate_open_fragment_root(job.replacement_block, job.replacement_kind)?;
        }
        job.input_finished = true;
        if self.leaf.is_empty() {
            self.sequence.begin_finish(&mut self.sequence_receipt)?;
            self.phase = SerializedGreenStreamPhase::ReducingFragmentReplacement;
        } else {
            self.phase = SerializedGreenStreamPhase::FlushingFinalFragmentLeaf;
        }
        Ok(())
    }

    pub(crate) fn take_canonical_fragment_replacement(
        &mut self,
        session: &ArenaBuildSession<'_>,
        replacement_block: BlockId,
    ) -> Result<CanonicalFragmentReplacement, SerializedGreenError> {
        self.take_canonical_fragment_rewrite(session, replacement_block, false)
    }

    pub(crate) fn take_canonical_fragment_removal(
        &mut self,
        session: &ArenaBuildSession<'_>,
        parent_block: BlockId,
    ) -> Result<CanonicalFragmentReplacement, SerializedGreenError> {
        self.take_canonical_fragment_rewrite(session, parent_block, true)
    }

    fn take_canonical_fragment_rewrite(
        &mut self,
        session: &ArenaBuildSession<'_>,
        replacement_block: BlockId,
        removed_terminal: bool,
    ) -> Result<CanonicalFragmentReplacement, SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::FragmentReplacementReady {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment replacement is not ready",
            ));
        }
        let ready =
            self.ready_fragment_replacement
                .as_ref()
                .ok_or(SerializedGreenError::Corrupt(
                    "ready canonical fragment replacement disappeared",
                ))?;
        if ready.build != self.build
            || ready.replacement_block != replacement_block
            || ready.removed_terminal != removed_terminal
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let ready = self
            .ready_fragment_replacement
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "ready canonical fragment replacement disappeared",
            ))?;
        self.phase = SerializedGreenStreamPhase::Accepting;
        Ok(ready)
    }

    /// Performs bounded builder work. This method allocates at most one arena
    /// page (Program allocation happens in `offer_event`) or one sequence
    /// branch, so the session may be suspended after every call.
    pub fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        self.ensure_session(session)?;
        self.leaf.require_fixed_capacity()?;
        self.receipt.resumable_polls += 1;
        let result = self.poll_inner(session);
        if result.is_err() {
            self.phase = SerializedGreenStreamPhase::Failed;
        }
        self.leaf.require_fixed_capacity()?;
        self.sync_journal_receipt(session)?;
        result
    }

    /// Closes the event stream. Late structure failures leave every allocated
    /// page in the same journal, ready for constant-time abort and fuelled
    /// cleanup.
    pub fn finish_input(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.deferred_normalization_target.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "cannot finish while an event or leaf carry is pending",
            ));
        }
        let validator = std::mem::take(&mut self.validator);
        if let Err(error) = validator.finish() {
            self.phase = SerializedGreenStreamPhase::Failed;
            return Err(error);
        }
        if self.leaf.is_empty() && self.tail_sealed_leaves == 0 && self.working_prefix.is_none() {
            self.phase = SerializedGreenStreamPhase::Failed;
            return Err(SerializedGreenError::Invalid("empty green document"));
        }
        if !self.leaf.is_empty() {
            self.phase = SerializedGreenStreamPhase::FlushingFinalLeaf;
        } else if self.tail_sealed_leaves != 0 {
            if let Err(error) = self.sequence.begin_finish(&mut self.sequence_receipt) {
                self.phase = SerializedGreenStreamPhase::Failed;
                return Err(error);
            }
            self.phase = SerializedGreenStreamPhase::ReducingFinalTail;
        } else {
            self.phase = SerializedGreenStreamPhase::AllocatingManifest;
        }
        self.sync_journal_receipt(session)?;
        Ok(())
    }

    /// Starts one sparse checkpoint barrier at the current exact event cut.
    /// The active partial leaf must contain at least one event. Subsequent
    /// calls to [`Self::poll`] allocate/push at most one page or branch each;
    /// no new event may be offered until [`Self::take_leaf_barrier_cut`]
    /// consumes the resulting capability.
    pub fn begin_leaf_barrier(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.pending_barrier_cut.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
            || self.leaf.is_empty()
        {
            return Err(SerializedGreenError::Invalid(
                "green leaf barrier requires one idle nonempty partial leaf",
            ));
        }
        self.phase = SerializedGreenStreamPhase::FlushingBarrierLeaf;
        self.sync_journal_receipt(session)?;
        Ok(())
    }

    /// True only when a previously minted exact cut is still the builder's
    /// complete current sealed boundary. This is the no-new-event reuse path;
    /// it never compares a caller-provided coordinate because the cut itself
    /// came from this build's live session.
    pub(crate) fn line_boundary_cut_is_current(&self, cut: &SerializedGreenLeafCut) -> bool {
        self.phase == SerializedGreenStreamPhase::Accepting
            && self.pending_event.is_none()
            && self.pending_provisional_paragraph.is_none()
            && self.ready_provisional_paragraph.is_none()
            && self.setext_job.is_none()
            && self.ready_setext_promotion.is_none()
            && self.pending_barrier_cut.is_none()
            && self.ready_barrier_cut.is_none()
            && self.ready_working_cut.is_none()
            && self.leaf.is_empty()
            && cut == &self.current_leaf_cut()
    }

    /// Seals the provisional Paragraph coordinates owned by the exact current
    /// line barrier. This is an observation draft only; the finalized old
    /// document must later prove that the same event became a Setext Heading.
    pub(crate) fn seal_retained_setext_green_checkpoint(
        &self,
        provisional: &ProvisionalParagraphEnter,
        cut: &SerializedGreenLeafCut,
    ) -> Result<RetainedSetextGreenCheckpointDraft, SerializedGreenError> {
        let active =
            self.active_provisional_paragraph
                .as_ref()
                .ok_or(SerializedGreenError::Invalid(
                    "retained Setext checkpoint has no provisional Paragraph",
                ))?;
        if !self.line_boundary_cut_is_current(cut)
            || provisional.build != self.build
            || provisional.build != cut.build
            || active.build != provisional.build
            || active.block != provisional.block
            || active.generation != provisional.generation
            || active.event_ordinal != provisional.event_ordinal
            || active.source_before != provisional.source_before
            || provisional.block.0 == 0
            || provisional.event_ordinal >= cut.events_before
            || provisional.source_before.bytes > cut.source_before.bytes
            || provisional.source_before.utf16 > cut.source_before.utf16
        {
            return Err(SerializedGreenError::Invalid(
                "provisional Paragraph and retained line cut disagree",
            ));
        }
        Ok(RetainedSetextGreenCheckpointDraft {
            old_build: self.build,
            block: provisional.block,
            target_event_ordinal: provisional.event_ordinal,
            target_source_before: provisional.source_before,
            accepted_event_cut: cut.events_before,
            accepted_source_cut: cut.source_before,
        })
    }

    /// Rechecks that a restored provisional token and the source-ledger
    /// terminal binding name the identical retained semantic owner.
    pub(crate) fn retained_provisional_matches(
        &self,
        provisional: &ProvisionalParagraphEnter,
        block: BlockId,
    ) -> bool {
        self.active_provisional_paragraph
            .as_ref()
            .is_some_and(|active| {
                provisional.build == self.build
                    && provisional.block == block
                    && active.build == provisional.build
                    && active.block == provisional.block
                    && active.generation == provisional.generation
                    && active.event_ordinal == provisional.event_ordinal
                    && active.source_before == provisional.source_before
            })
    }

    /// Mints an exact cut when input has naturally ended on a sealed-page
    /// boundary. The active partial leaf is empty, so forcing another leaf
    /// would fabricate storage. A matching prior cut must instead use
    /// [`Self::line_boundary_cut_is_current`].
    pub(crate) fn take_natural_line_boundary_cut(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenLeafCut, SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.pending_barrier_cut.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
            || !self.leaf.is_empty()
            || self.sealed_leaves == 0
        {
            return Err(SerializedGreenError::Invalid(
                "natural green line boundary requires an idle nonempty sealed prefix",
            ));
        }
        Ok(self.current_leaf_cut())
    }

    pub(crate) fn has_partial_line_boundary_events(&self) -> bool {
        !self.leaf.is_empty()
    }

    /// Folds every naturally sealed packed page into the sole typed working
    /// prefix while preserving the active partial page for continued packing.
    /// This is the normalization boundary primitive: calling it per small
    /// block does not force a page or expose the prefix root.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn begin_working_prefix_reduction(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.pending_barrier_cut.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
        {
            return Err(SerializedGreenError::Invalid(
                "working-prefix reduction requires an idle event boundary",
            ));
        }
        if self.tail_sealed_leaves == 0 {
            self.ready_working_cut = Some(self.current_working_cut()?);
            self.record_working_reduction(true)?;
        } else {
            if let Err(error) = self.sequence.begin_finish(&mut self.sequence_receipt) {
                self.phase = SerializedGreenStreamPhase::Failed;
                return Err(error);
            }
            self.phase = SerializedGreenStreamPhase::ReducingWorkingTail;
        }
        self.sync_journal_receipt(session)?;
        Ok(())
    }

    /// Consumes the exact build-local acknowledgement for one completed
    /// working-prefix reduction. The root remains private inside the builder.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn take_working_prefix_cut(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenWorkingCut, SerializedGreenError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting {
            return Err(SerializedGreenError::Invalid(
                "working-prefix reduction is not ready",
            ));
        }
        self.ready_working_cut
            .take()
            .ok_or(SerializedGreenError::Invalid(
                "working-prefix reduction has no unconsumed cut",
            ))
    }

    /// Consumes the exact build-local cut after barrier polling has returned
    /// [`SerializedGreenStreamProgress::ReadyForEvent`]. The matching live
    /// session is mandatory; after cancellation there is no lease with which
    /// this observation can escape and masquerade as checkpoint authority.
    pub fn take_leaf_barrier_cut(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenLeafCut, SerializedGreenError> {
        // A build-local cut is authority only while its exact arena generation
        // is live. Requiring the session here prevents a ready cut from
        // escaping after cancellation has already consumed the build lease.
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting {
            return Err(SerializedGreenError::Invalid(
                "green leaf barrier is not ready",
            ));
        }
        self.ready_barrier_cut
            .take()
            .ok_or(SerializedGreenError::Invalid(
                "green leaf barrier has no unconsumed cut",
            ))
    }

    /// Extracts the sole source-complete manifest owner after `poll` reports
    /// [`SerializedGreenStreamProgress::ManifestReady`].
    pub fn take_manifest(mut self) -> Result<SerializedGreenBuildManifest, SerializedGreenError> {
        if self.phase != SerializedGreenStreamPhase::ManifestReady {
            return Err(SerializedGreenError::Invalid(
                "serialized green manifest is not ready",
            ));
        }
        let owner = self.manifest.take().ok_or(SerializedGreenError::Corrupt(
            "ready build lost manifest owner",
        ))?;
        let receipt = self.receipt();
        Ok(SerializedGreenBuildManifest {
            build: self.build,
            owner,
            receipt,
        })
    }

    #[allow(clippy::needless_pass_by_value)] // Consuming input prevents the builder from retaining a typed mirror.
    fn encode_and_journal_event(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) -> Result<(), SerializedGreenError> {
        self.receipt.offer_event_descriptor_buffers_created += 1;
        if matches!(&event, GreenEvent::Enter { facts, .. } if !facts.fields.is_empty()) {
            self.receipt.offer_event_facts_buffers_created += 1;
        }
        let closes_active_provisional = matches!(event, GreenEvent::Exit { .. })
            && self
                .active_provisional_paragraph
                .as_ref()
                .is_some_and(|active| self.validator.active_terminal == Some(active.block));
        self.validator.push(&event)?;
        let summary = GreenSummary::event(&event);
        // Encode the descriptor while borrowing the typed event, then consume
        // the event to move its already-sealed Program payload. The resumable
        // path therefore never holds both a typed Program and a cloned encoded
        // Program buffer.
        let encoded = encode_event_inner(&event, 0, false)?;
        let program_payload = match event {
            GreenEvent::Coverage(SourceProjectionRun {
                logical_contribution: LogicalContribution::Program(program),
                ..
            }) => Some(program.payload),
            GreenEvent::Enter { .. } | GreenEvent::Coverage(_) | GreenEvent::Exit { .. } => None,
        };
        let program = match program_payload {
            Some(payload) => {
                let payload_len = payload.len();
                let payload_capacity = payload.capacity();
                let (owner, allocation) = session.allocate(&payload, &[])?;
                self.receipt.projection_program_pages_allocated += 1;
                self.receipt.resumable_arena_allocations += 1;
                self.receipt.payload_bytes_copied += allocation.payload_bytes_copied;
                self.receipt.edge_bytes_copied += allocation.edge_bytes_copied;
                self.receipt.maximum_projection_program_bytes = self
                    .receipt
                    .maximum_projection_program_bytes
                    .max(payload_len);
                self.receipt.maximum_projection_program_payload_len = self
                    .receipt
                    .maximum_projection_program_payload_len
                    .max(payload_len);
                self.receipt.maximum_projection_program_scratch_capacity = self
                    .receipt
                    .maximum_projection_program_scratch_capacity
                    .max(payload_capacity);
                self.receipt.maximum_pending_program_payload_bytes = self
                    .receipt
                    .maximum_pending_program_payload_bytes
                    .max(payload_capacity);
                Some(owner)
            }
            None => None,
        };
        if program.is_some() != encoded.program_ordinal_offset.is_some()
            || encoded.program.is_some()
        {
            return Err(SerializedGreenError::Corrupt(
                "owned Program descriptor and payload disagree",
            ));
        }
        self.pending_event = Some(JournaledGreenEvent {
            bytes: encoded.bytes,
            program,
            program_ordinal_offset: encoded.program_ordinal_offset,
            summary,
        });
        if closes_active_provisional {
            self.active_provisional_paragraph = None;
        }
        self.record_scratch();
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Exhaustive allocation-granular phase table stays centralized.
    fn poll_inner(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        match self.phase {
            SerializedGreenStreamPhase::Accepting => self.poll_pending_event(session),
            SerializedGreenStreamPhase::PushingForPendingEvent => {
                match self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                {
                    ResumableSequenceProgress::Pending => {
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                    ResumableSequenceProgress::Complete => {
                        self.phase = SerializedGreenStreamPhase::Accepting;
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                }
            }
            SerializedGreenStreamPhase::FlushingBarrierLeaf => {
                let leaf = self.allocate_partial_leaf(session)?;
                self.sequence
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.pending_barrier_cut = Some(self.current_leaf_cut());
                self.phase = SerializedGreenStreamPhase::PushingBarrierLeaf;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingBarrierLeaf => {
                match self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                {
                    ResumableSequenceProgress::Pending => {
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                    ResumableSequenceProgress::Complete => {
                        self.ready_barrier_cut = self.pending_barrier_cut.take();
                        self.receipt.leaf_barriers_completed = self
                            .receipt
                            .leaf_barriers_completed
                            .checked_add(1)
                            .ok_or(SerializedGreenError::Overflow("green leaf barriers"))?;
                        self.phase = SerializedGreenStreamPhase::Accepting;
                        Ok(SerializedGreenStreamProgress::ReadyForEvent)
                    }
                }
            }
            SerializedGreenStreamPhase::FlushingFinalLeaf => {
                let leaf = self.allocate_partial_leaf(session)?;
                self.sequence
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::PushingFinalLeaf;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingFinalLeaf => {
                match self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                {
                    ResumableSequenceProgress::Pending => {
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                    ResumableSequenceProgress::Complete => {
                        self.sequence.begin_finish(&mut self.sequence_receipt)?;
                        self.phase = SerializedGreenStreamPhase::ReducingFinalTail;
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                }
            }
            SerializedGreenStreamPhase::ReducingWorkingTail => {
                self.poll_tail_reduction(session, false)
            }
            SerializedGreenStreamPhase::SplicingWorkingTail => {
                self.poll_tail_splice(session, false)
            }
            SerializedGreenStreamPhase::FlushingSetextLeaf => {
                let leaf = self.allocate_partial_leaf(session)?;
                self.sequence
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::PushingSetextLeaf;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingSetextLeaf => {
                match self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                {
                    ResumableSequenceProgress::Pending => {
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                    ResumableSequenceProgress::Complete => {
                        self.sequence.begin_finish(&mut self.sequence_receipt)?;
                        self.phase = SerializedGreenStreamPhase::ReducingSetextTail;
                        Ok(SerializedGreenStreamProgress::Pending)
                    }
                }
            }
            SerializedGreenStreamPhase::ReducingSetextTail => {
                self.poll_setext_tail_reduction(session)
            }
            SerializedGreenStreamPhase::SplicingSetextTail => self.poll_setext_tail_splice(session),
            SerializedGreenStreamPhase::PreparingSetextRepack => {
                self.prepare_setext_repack(session)?;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::AllocatingSetextReplacementLeaf => {
                self.allocate_setext_replacement_leaf(session)?;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingSetextReplacementLeaf => {
                self.poll_setext_replacement_push(session)
            }
            SerializedGreenStreamPhase::ReducingSetextReplacement => {
                self.poll_setext_replacement_reduction(session)
            }
            SerializedGreenStreamPhase::SplicingSetextReplacement => {
                self.poll_setext_replacement_splice(session)
            }
            SerializedGreenStreamPhase::ReducingWholeNormalizationTail => {
                self.poll_whole_normalization_tail_reduction(session)
            }
            SerializedGreenStreamPhase::SplicingWholeNormalizationTail => {
                self.poll_whole_normalization_tail_splice(session)
            }
            SerializedGreenStreamPhase::PreparingWholeNormalizationRepack => {
                self.prepare_whole_normalization_repack(session)?;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::AllocatingWholeNormalizationLeaf => {
                self.allocate_whole_normalization_leaf(session)?;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingWholeNormalizationLeaf => {
                self.poll_whole_normalization_leaf_push(session)
            }
            SerializedGreenStreamPhase::ReducingWholeNormalizationReplacement => {
                self.poll_whole_normalization_replacement_reduction(session)
            }
            SerializedGreenStreamPhase::SplicingWholeNormalizationReplacement => {
                self.poll_whole_normalization_replacement_splice(session)
            }
            SerializedGreenStreamPhase::FlushingFragmentBaseLeaf => {
                let leaf = self.allocate_partial_leaf(session)?;
                self.sequence
                    .begin_push(session, leaf, &mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::PushingFragmentBaseLeaf;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingFragmentBaseLeaf => {
                if self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Pending
                {
                    return Ok(SerializedGreenStreamProgress::Pending);
                }
                self.sequence.begin_finish(&mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::ReducingFragmentBaseTail;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::ReducingFragmentBaseTail => {
                self.poll_fragment_base_tail_reduction(session)
            }
            SerializedGreenStreamPhase::SplicingFragmentBaseTail => {
                self.poll_fragment_base_tail_splice(session)
            }
            SerializedGreenStreamPhase::PreparingFragmentReplacement => {
                self.prepare_fragment_replacement(session)?;
                Ok(SerializedGreenStreamProgress::ReadyForEvent)
            }
            SerializedGreenStreamPhase::AcceptingFragmentEvent => {
                self.poll_pending_fragment_event(session)
            }
            SerializedGreenStreamPhase::PushingFragmentEventLeaf => {
                self.poll_fragment_event_leaf_push(session)
            }
            SerializedGreenStreamPhase::FlushingFinalFragmentLeaf => {
                self.flush_fragment_leaf(session)?;
                self.phase = SerializedGreenStreamPhase::PushingFinalFragmentLeaf;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::PushingFinalFragmentLeaf => {
                if self
                    .sequence
                    .poll_push(session, &mut self.sequence_receipt)?
                    == ResumableSequenceProgress::Pending
                {
                    return Ok(SerializedGreenStreamProgress::Pending);
                }
                self.sequence.begin_finish(&mut self.sequence_receipt)?;
                self.phase = SerializedGreenStreamPhase::ReducingFragmentReplacement;
                Ok(SerializedGreenStreamProgress::Pending)
            }
            SerializedGreenStreamPhase::ReducingFragmentReplacement => {
                self.poll_fragment_replacement_reduction(session)
            }
            SerializedGreenStreamPhase::SplicingFragmentReplacement => {
                self.poll_fragment_replacement_splice(session)
            }
            SerializedGreenStreamPhase::FragmentReplacementReady => {
                Ok(SerializedGreenStreamProgress::ReadyForEvent)
            }
            SerializedGreenStreamPhase::ReducingFinalTail => {
                self.poll_tail_reduction(session, true)
            }
            SerializedGreenStreamPhase::SplicingFinalTail => self.poll_tail_splice(session, true),
            SerializedGreenStreamPhase::AllocatingManifest => {
                let prefix = self
                    .working_prefix
                    .take()
                    .ok_or(SerializedGreenError::Corrupt(
                        "finished build lost working prefix",
                    ))?;
                self.allocate_manifest(session, prefix)?;
                Ok(SerializedGreenStreamProgress::ManifestReady)
            }
            SerializedGreenStreamPhase::ManifestReady => {
                Ok(SerializedGreenStreamProgress::ManifestReady)
            }
            SerializedGreenStreamPhase::Failed => Err(SerializedGreenError::Invalid(
                "serialized green build is terminally failed",
            )),
        }
    }

    // Kept as individually named transitions so every enum arm compiles while
    // the generic reducer is implemented incrementally; none is reachable
    // before `begin_canonical_fragment_replacement` installs its typed job.
    fn poll_fragment_base_tail_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.fragment_job.is_none() {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment base reduction lost its job",
            ));
        }
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let tail = self.sequence.take_root()?;
        let tail_summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&tail)?)?.0;
        if tail_summary.leaves != self.tail_sealed_leaves
            || !tail_summary.same_semantics(self.tail_summary)
        {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment base reduction changed its tail",
            ));
        }
        let Some(prefix) = self.working_prefix.take() else {
            self.install_working_prefix(session, tail, self.tail_summary)?;
            self.tail_sealed_leaves = 0;
            self.tail_summary = GreenSummary::default();
            self.phase = SerializedGreenStreamPhase::PreparingFragmentReplacement;
            return Ok(SerializedGreenStreamProgress::Pending);
        };
        if prefix.build != self.build {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment base prefix belongs to another build",
            ));
        }
        let expected = prefix.summary.followed_by(self.tail_summary)?;
        let insertion = prefix.summary.leaves;
        self.begin_canonical_leaf_insertion(session, prefix.owner, insertion, tail)?;
        self.pending_splice_summary = Some(expected);
        self.phase = SerializedGreenStreamPhase::SplicingFragmentBaseTail;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_fragment_base_tail_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment base splice produced an empty prefix",
            ))?;
        let expected = self
            .pending_splice_summary
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment base splice lost its summary",
            ))?;
        self.install_working_prefix(session, root, expected)?;
        self.tail_sealed_leaves = 0;
        self.tail_summary = GreenSummary::default();
        self.phase = SerializedGreenStreamPhase::PreparingFragmentReplacement;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn prepare_fragment_replacement(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        if !self.leaf.is_empty() || self.tail_sealed_leaves != 0 {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment preparation retained an active base leaf",
            ));
        }
        let prefix = self
            .working_prefix
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment preparation has no complete base prefix",
            ))?;
        if prefix.build != self.build
            || prefix.summary.leaves != self.sealed_leaves
            || prefix.summary.tokens != self.sealed_events
            || prefix.summary.metric != self.sealed_metric
        {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment base prefix is not the complete sealed stream",
            ));
        }
        let job = self
            .fragment_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment preparation lost its job",
            ))?;
        let ProvisionalParagraphStorage::Sealed {
            leaf_index,
            byte_offset,
            event_ordinal_in_leaf,
            source_before_in_leaf,
        } = job.active.storage
        else {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment preparation target was not sealed",
            ));
        };
        let root = session.owner_id(&prefix.owner)?;
        let (leaf, prefix_before_leaf) =
            locate_green_leaf_with_prefix(session.arena(), root, leaf_index)?;
        if prefix_before_leaf
            .tokens
            .checked_add(event_ordinal_in_leaf)
            .ok_or(SerializedGreenError::Overflow(
                "canonical fragment target event ordinal",
            ))?
            != job.active.event_ordinal
            || prefix_before_leaf
                .metric
                .checked_add(source_before_in_leaf)?
                != job.active.source_before
            || prefix
                .summary
                .metric
                .checked_sub(job.active.source_before)?
                != job.expected_physical
        {
            return Err(SerializedGreenError::StaleCursor);
        }

        // A leaf is page-bounded.  Copying its payload here is therefore a
        // fixed 4 KiB decode scratch, not a fragment/document event tape, and
        // lets Program owners be retained one at a time through the mutable
        // build session.
        let child_count = session.arena().packed_child_count(leaf)?;
        let payload = session.arena().payload(leaf)?.to_vec();
        self.receipt.maximum_decoded_page_buffer_bytes = self
            .receipt
            .maximum_decoded_page_buffer_bytes
            .max(payload.capacity());
        let expected_leaf = decode_summary(&payload, LEAF_TAG)?;
        let mut decoder = Decoder::new(&payload[LEAF_HEADER_BYTES..]);
        let mut actual_leaf = GreenSummary::default();
        let mut next_program_ordinal = 0_usize;
        let mut event_ordinal = 0_u64;
        let mut found_target = false;
        while !decoder.is_empty() {
            let start = LEAF_HEADER_BYTES.checked_add(decoder.cursor).ok_or(
                SerializedGreenError::Overflow("canonical fragment decoded event offset"),
            )?;
            let event = decode_event(
                &mut decoder,
                session.arena(),
                leaf,
                &mut next_program_ordinal,
            )?;
            let end = LEAF_HEADER_BYTES.checked_add(decoder.cursor).ok_or(
                SerializedGreenError::Overflow("canonical fragment decoded event end"),
            )?;
            let event_summary = GreenSummary::decoded_event(&event);
            actual_leaf = actual_leaf.followed_by(event_summary)?;
            if event_ordinal < event_ordinal_in_leaf {
                let raw = payload
                    .get(start..end)
                    .ok_or(SerializedGreenError::Corrupt(
                        "canonical fragment prefix event escapes its leaf",
                    ))?
                    .to_vec();
                let (program, program_ordinal_offset) = match &event {
                    DecodedGreenEventKind::Coverage(DecodedSourceProjectionRun {
                        logical_contribution: DecodedLogicalContribution::Program(program),
                        ..
                    }) => (
                        Some(session.retain(program.retained_page()?)?),
                        Some(usize::from(program.encoded_ordinal_offset)),
                    ),
                    DecodedGreenEventKind::Enter { .. }
                    | DecodedGreenEventKind::Coverage(_)
                    | DecodedGreenEventKind::Exit { .. } => (None, None),
                };
                self.leaf.push(JournaledGreenEvent {
                    bytes: raw,
                    program,
                    program_ordinal_offset,
                    summary: event_summary,
                })?;
            } else if event_ordinal == event_ordinal_in_leaf {
                let offset = u16::try_from(start).map_err(|_| {
                    SerializedGreenError::Corrupt("canonical fragment target offset exceeds u16")
                })?;
                let DecodedGreenEventKind::Enter { block, kind, facts } = &event else {
                    return Err(SerializedGreenError::StaleCursor);
                };
                if offset != byte_offset
                    || *block != job.active.block
                    || *kind != GreenKind::PARAGRAPH
                    || !facts.fields.is_empty()
                {
                    return Err(SerializedGreenError::StaleCursor);
                }
                found_target = true;
            }
            event_ordinal = event_ordinal
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "canonical fragment event ordinal",
                ))?;
        }
        if next_program_ordinal != child_count {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment target leaf has an unreferenced Program edge",
            ));
        }
        actual_leaf.leaves = 1;
        actual_leaf.height = 1;
        if actual_leaf != expected_leaf || !found_target || self.leaf.is_empty() {
            return Err(SerializedGreenError::StaleCursor);
        }
        let replacement_range = leaf_index..prefix.summary.leaves;
        if replacement_range.is_empty() {
            return Err(SerializedGreenError::StaleCursor);
        }
        let job = self
            .fragment_job
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment preparation lost its job",
            ))?;
        job.replacement_range = Some(replacement_range);
        job.base_prefix_summary = Some(prefix.summary);
        job.untouched_summary = Some(prefix_before_leaf.followed_by(self.leaf.summary)?);
        self.phase = SerializedGreenStreamPhase::AcceptingFragmentEvent;
        self.record_scratch();
        Ok(())
    }

    fn poll_pending_fragment_event(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        let Some(event) = self.pending_event.take() else {
            return Ok(SerializedGreenStreamProgress::ReadyForEvent);
        };
        if self.leaf.can_fit(&event) {
            let byte_offset = u16::try_from(self.leaf.bytes.len())
                .map_err(|_| SerializedGreenError::Overflow("fragment survivor event offset"))?;
            let event_ordinal_in_leaf = self.leaf.summary.tokens;
            let source_before_in_leaf = self.leaf.summary.metric;
            self.leaf.push(event)?;
            if let Some(pending) = self.pending_provisional_paragraph.take() {
                let job = self
                    .fragment_job
                    .as_mut()
                    .ok_or(SerializedGreenError::Corrupt(
                        "fragment survivor acknowledgement lost its job",
                    ))?;
                if job.replacement_kind != GreenKind::PARAGRAPH
                    || job.replacement_block != pending.block
                    || job.active.block != pending.block
                    || job.surviving_paragraph.is_some()
                {
                    return Err(SerializedGreenError::Corrupt(
                        "fragment survivor acknowledgement crossed its recipe",
                    ));
                }
                job.surviving_paragraph = Some(FragmentSurvivingParagraph {
                    pending,
                    replacement_leaf_index: job.replacement_pages,
                    byte_offset,
                    event_ordinal_in_leaf,
                    source_before_in_leaf,
                });
            }
            self.record_scratch();
            return Ok(SerializedGreenStreamProgress::ReadyForEvent);
        }
        self.pending_event = Some(event);
        if self.leaf.is_empty() {
            return Err(SerializedGreenError::Invalid(
                "canonical fragment event exceeds an empty leaf",
            ));
        }
        self.flush_fragment_leaf(session)?;
        self.phase = SerializedGreenStreamPhase::PushingFragmentEventLeaf;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_fragment_event_leaf_push(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self
            .sequence
            .poll_push(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        self.phase = SerializedGreenStreamPhase::AcceptingFragmentEvent;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn flush_fragment_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.leaf.seal_in_place()?;
        let sealed_summary = self.leaf.summary;
        let mut ids = [ArenaId::default(); MAX_PACKED_ARENA_CHILDREN];
        let program_count = self.leaf.programs.len();
        if program_count > ids.len() {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment leaf exceeds packed child capacity",
            ));
        }
        for (slot, program) in ids.iter_mut().zip(&self.leaf.programs) {
            *slot = session.owner_id(program)?;
        }
        let (leaf, allocation) =
            session.allocate_packed(&self.leaf.bytes, &ids[..program_count])?;
        self.receipt.leaf_pages_allocated =
            self.receipt.leaf_pages_allocated.checked_add(1).ok_or(
                SerializedGreenError::Overflow("canonical fragment leaf allocation count"),
            )?;
        self.receipt.resumable_arena_allocations = self
            .receipt
            .resumable_arena_allocations
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "canonical fragment arena allocation count",
            ))?;
        self.receipt.payload_bytes_copied = self
            .receipt
            .payload_bytes_copied
            .checked_add(allocation.payload_bytes_copied)
            .ok_or(SerializedGreenError::Overflow(
                "canonical fragment payload copy count",
            ))?;
        self.receipt.edge_bytes_copied = self
            .receipt
            .edge_bytes_copied
            .checked_add(allocation.edge_bytes_copied)
            .ok_or(SerializedGreenError::Overflow(
                "canonical fragment edge copy count",
            ))?;
        while let Some(program) = self.leaf.programs.pop() {
            session.release(program)?;
        }
        let job = self
            .fragment_job
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment leaf flush lost its job",
            ))?;
        job.replacement_pages =
            job.replacement_pages
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "canonical fragment replacement leaf count",
                ))?;
        job.replacement_summary = job.replacement_summary.followed_by(sealed_summary)?;
        self.leaf.reset_after_allocation();
        self.sequence
            .begin_push(session, leaf, &mut self.sequence_receipt)?;
        self.record_scratch();
        Ok(())
    }

    fn poll_fragment_replacement_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        let job = self
            .fragment_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement reduction lost its job",
            ))?;
        if !job.input_finished || job.replacement_pages == 0 {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment replacement reduced before input finish",
            ));
        }
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let replacement = self.sequence.take_root()?;
        let replacement_summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&replacement)?)?
                .0;
        let job = self
            .fragment_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement reduction lost its job",
            ))?;
        if replacement_summary.leaves != job.replacement_pages
            || !replacement_summary.same_semantics(job.replacement_summary)
        {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment replacement reduction changed packed pages",
            ));
        }
        let range = job
            .replacement_range
            .clone()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement lost its exact range",
            ))?;
        let prefix = self
            .working_prefix
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement lost its base prefix",
            ))?;
        self.begin_canonical_leaf_replacement(session, prefix.owner, range, replacement)?;
        self.phase = SerializedGreenStreamPhase::SplicingFragmentReplacement;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_fragment_replacement_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement produced an empty prefix",
            ))?;
        let summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&root)?)?.0;
        let job = self
            .fragment_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement splice lost its job",
            ))?;
        let base = job
            .base_prefix_summary
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement splice lost its base summary",
            ))?;
        let range = job
            .replacement_range
            .clone()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement splice lost its range",
            ))?;
        let removed = range
            .end
            .checked_sub(range.start)
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement range is reversed",
            ))?;
        let expected_leaves = base
            .leaves
            .checked_sub(removed)
            .and_then(|leaves| leaves.checked_add(job.replacement_pages))
            .ok_or(SerializedGreenError::Overflow(
                "canonical fragment installed leaf count",
            ))?;
        let expected_balance = if job.removed_terminal {
            base.balance
                .checked_sub(1)
                .ok_or(SerializedGreenError::Overflow(
                    "removed fragment structural balance",
                ))?
        } else {
            base.balance
        };
        if summary.leaves != expected_leaves
            || summary.metric != base.metric
            || summary.balance != expected_balance
            || summary.minimum_prefix != base.minimum_prefix
            || summary.minimum_prefix < 0
        {
            return Err(SerializedGreenError::Corrupt(
                "canonical fragment replacement changed physical or ancestor continuity",
            ));
        }
        let job = self
            .fragment_job
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement job disappeared after splice",
            ))?;
        let untouched = job.untouched_summary.ok_or(SerializedGreenError::Corrupt(
            "canonical fragment replacement lost its untouched summary",
        ))?;
        let untouched_coverage = untouched.coverage_runs_for_valid_prefix()?;
        let retired_coverage_runs = base
            .coverage_runs_for_valid_prefix()?
            .checked_sub(untouched_coverage)
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment retired coverage underflow",
            ))?;
        let replacement_coverage_runs = summary
            .coverage_runs_for_valid_prefix()?
            .checked_sub(untouched_coverage)
            .ok_or(SerializedGreenError::Corrupt(
                "canonical fragment replacement coverage underflow",
            ))?;
        let restored_survivor = match job.surviving_paragraph {
            Some(survivor) => {
                if job.replacement_kind != GreenKind::PARAGRAPH
                    || job.replacement_block != job.active.block
                    || survivor.pending.block != job.active.block
                {
                    return Err(SerializedGreenError::Corrupt(
                        "fragment survivor disagrees with its replacement recipe",
                    ));
                }
                let global_leaf_index = range
                    .start
                    .checked_add(survivor.replacement_leaf_index)
                    .ok_or(SerializedGreenError::Overflow(
                        "fragment survivor installed leaf index",
                    ))?;
                if global_leaf_index >= summary.leaves {
                    return Err(SerializedGreenError::Corrupt(
                        "fragment survivor lies beyond the installed prefix",
                    ));
                }
                let root_id = session.owner_id(&root)?;
                let (leaf, prefix_before_leaf) =
                    locate_green_leaf_with_prefix(session.arena(), root_id, global_leaf_index)?;
                let expected_event = prefix_before_leaf
                    .tokens
                    .checked_add(survivor.event_ordinal_in_leaf)
                    .ok_or(SerializedGreenError::Overflow(
                        "fragment survivor global event ordinal",
                    ))?;
                let expected_source = prefix_before_leaf
                    .metric
                    .checked_add(survivor.source_before_in_leaf)?;
                if expected_event != survivor.pending.event_ordinal
                    || expected_source != survivor.pending.source_before
                {
                    return Err(SerializedGreenError::Corrupt(
                        "fragment survivor moved during canonical splice",
                    ));
                }
                let mut matched = false;
                visit_decoded_leaf_events(session.arena(), leaf, |offset, event| {
                    if offset == survivor.byte_offset {
                        if matched
                            || !matches!(
                                event,
                                DecodedGreenEventKind::Enter { block, kind, ref facts }
                                    if block == survivor.pending.block
                                        && kind == GreenKind::PARAGRAPH
                                        && facts.fields.is_empty()
                            )
                        {
                            return Err(SerializedGreenError::Corrupt(
                                "fragment survivor Enter descriptor changed",
                            ));
                        }
                        matched = true;
                    }
                    Ok(())
                })?;
                if !matched {
                    return Err(SerializedGreenError::Corrupt(
                        "fragment survivor Enter disappeared",
                    ));
                }
                Some(ActiveProvisionalParagraph {
                    build: survivor.pending.build,
                    block: survivor.pending.block,
                    generation: survivor.pending.generation,
                    event_ordinal: survivor.pending.event_ordinal,
                    source_before: survivor.pending.source_before,
                    storage: ProvisionalParagraphStorage::Sealed {
                        leaf_index: global_leaf_index,
                        byte_offset: survivor.byte_offset,
                        event_ordinal_in_leaf: survivor.event_ordinal_in_leaf,
                        source_before_in_leaf: survivor.source_before_in_leaf,
                    },
                })
            }
            None if job.replacement_kind.logical_channel().is_none() => None,
            None => {
                return Err(SerializedGreenError::Corrupt(
                    "logical fragment replacement lost its surviving terminal",
                ));
            }
        };
        self.sealed_leaves = summary.leaves;
        self.sealed_events = summary.tokens;
        self.sealed_metric = summary.metric;
        self.install_working_prefix(session, root, summary)?;
        if let Some(active) = restored_survivor {
            let ready = ProvisionalParagraphEnter {
                build: active.build,
                block: active.block,
                generation: active.generation,
                event_ordinal: active.event_ordinal,
                source_before: active.source_before,
            };
            self.active_provisional_paragraph = Some(active);
            self.ready_provisional_paragraph = Some(ready);
        }
        self.ready_fragment_replacement = Some(CanonicalFragmentReplacement {
            build: self.build,
            retired_block: job.active.block,
            replacement_block: job.replacement_block,
            replacement_kind: job.replacement_kind,
            removed_terminal: job.removed_terminal,
            physical_metric: job.expected_physical,
            retired_coverage_runs,
            replacement_coverage_runs,
        });
        self.phase = SerializedGreenStreamPhase::FragmentReplacementReady;
        self.record_scratch();
        Ok(SerializedGreenStreamProgress::ReadyForEvent)
    }

    fn poll_tail_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        finalizing: bool,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let tail = self.sequence.take_root()?;
        let tail_id = session.owner_id(&tail)?;
        let tail_summary = sequence_node::<SerializedGreenSpec>(session.arena(), tail_id)?.0;
        if tail_summary.leaves != self.tail_sealed_leaves
            || !tail_summary.same_semantics(self.tail_summary)
        {
            return Err(SerializedGreenError::Corrupt(
                "reduced green tail summary changed",
            ));
        }

        let Some(prefix) = self.working_prefix.take() else {
            self.install_working_prefix(session, tail, self.tail_summary)?;
            return self.complete_tail_reduction(finalizing);
        };
        if prefix.build != self.build {
            return Err(SerializedGreenError::Corrupt(
                "working prefix belongs to another build generation",
            ));
        }
        let expected = prefix.summary.followed_by(self.tail_summary)?;
        let insertion = prefix.summary.leaves;
        self.begin_canonical_leaf_insertion(session, prefix.owner, insertion, tail)?;
        self.pending_splice_summary = Some(expected);
        self.phase = if finalizing {
            SerializedGreenStreamPhase::SplicingFinalTail
        } else {
            SerializedGreenStreamPhase::SplicingWorkingTail
        };
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_tail_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        finalizing: bool,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "working-prefix splice produced an empty root",
            ))?;
        let expected = self
            .pending_splice_summary
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "working-prefix splice lost its expected summary",
            ))?;
        self.install_working_prefix(session, root, expected)?;
        self.complete_tail_reduction(finalizing)
    }

    fn poll_setext_tail_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.setext_job.is_none() {
            return Err(SerializedGreenError::Corrupt(
                "Setext tail reduction lost its job",
            ));
        }
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let tail = self.sequence.take_root()?;
        let tail_id = session.owner_id(&tail)?;
        let tail_summary = sequence_node::<SerializedGreenSpec>(session.arena(), tail_id)?.0;
        if tail_summary.leaves != self.tail_sealed_leaves
            || !tail_summary.same_semantics(self.tail_summary)
        {
            return Err(SerializedGreenError::Corrupt(
                "Setext tail reduction changed its summary",
            ));
        }
        let Some(prefix) = self.working_prefix.take() else {
            self.install_working_prefix(session, tail, self.tail_summary)?;
            return self.complete_setext_tail_install();
        };
        if prefix.build != self.build {
            return Err(SerializedGreenError::Corrupt(
                "Setext working prefix belongs to another build",
            ));
        }
        let expected = prefix.summary.followed_by(self.tail_summary)?;
        let insertion = prefix.summary.leaves;
        self.begin_canonical_leaf_insertion(session, prefix.owner, insertion, tail)?;
        self.pending_splice_summary = Some(expected);
        self.phase = SerializedGreenStreamPhase::SplicingSetextTail;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_setext_tail_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "Setext tail splice produced an empty prefix",
            ))?;
        let expected = self
            .pending_splice_summary
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext tail splice lost its summary",
            ))?;
        self.install_working_prefix(session, root, expected)?;
        self.complete_setext_tail_install()
    }

    fn complete_setext_tail_install(
        &mut self,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.setext_job.is_none() || self.working_prefix.is_none() {
            return Err(SerializedGreenError::Corrupt(
                "Setext tail install lost its job or prefix",
            ));
        }
        self.tail_sealed_leaves = 0;
        self.tail_summary = GreenSummary::default();
        self.phase = SerializedGreenStreamPhase::PreparingSetextRepack;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn prepare_setext_repack(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        let prefix = self
            .working_prefix
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext repack has no installed working prefix",
            ))?;
        if prefix.build != self.build
            || prefix.summary.leaves != self.sealed_leaves
            || self.tail_sealed_leaves != 0
        {
            return Err(SerializedGreenError::Corrupt(
                "Setext repack prefix is not the complete sealed stream",
            ));
        }
        let job = self
            .setext_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt("Setext repack lost its job"))?;
        let ProvisionalParagraphStorage::Sealed {
            leaf_index,
            byte_offset,
            event_ordinal_in_leaf,
            source_before_in_leaf,
        } = job.active.storage
        else {
            return Err(SerializedGreenError::Corrupt(
                "Setext repack target was not sealed",
            ));
        };
        let root = session.owner_id(&prefix.owner)?;
        let (leaf, prefix_before_leaf) =
            locate_green_leaf_with_prefix(session.arena(), root, leaf_index)?;
        if prefix_before_leaf
            .tokens
            .checked_add(event_ordinal_in_leaf)
            .ok_or(SerializedGreenError::Overflow(
                "Setext target event ordinal",
            ))?
            != job.active.event_ordinal
            || prefix_before_leaf
                .metric
                .checked_add(source_before_in_leaf)?
                != job.active.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let (old_leaf_summary, replacement_summary, page_count, replacement_target) =
            prepare_setext_leaf_repack(
                session.arena(),
                leaf,
                event_ordinal_in_leaf,
                byte_offset,
                job.active.block,
                &job.encoded_enter,
                &mut self.setext_scratch,
            )?;
        if !old_leaf_summary.same_semantics(replacement_summary)
            || replacement_summary.leaves
                != u64::try_from(page_count)
                    .map_err(|_| SerializedGreenError::Overflow("Setext replacement leaf count"))?
        {
            return Err(SerializedGreenError::Corrupt(
                "Setext repack changed leaf semantics",
            ));
        }
        let expected_prefix_leaves = prefix
            .summary
            .leaves
            .checked_sub(1)
            .and_then(|leaves| leaves.checked_add(replacement_summary.leaves))
            .ok_or(SerializedGreenError::Overflow(
                "Setext working-prefix leaf count",
            ))?;
        let job = self
            .setext_job
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt("Setext repack lost its job"))?;
        job.replacement_page_count = page_count;
        job.next_replacement_page = 0;
        job.base_prefix_summary = Some(prefix.summary);
        job.expected_prefix_leaves = Some(expected_prefix_leaves);
        job.replacement_target = Some(ProvisionalParagraphStorage::Sealed {
            leaf_index: leaf_index
                .checked_add(u64::try_from(replacement_target.page_index).map_err(|_| {
                    SerializedGreenError::Overflow("Setext replacement target page index")
                })?)
                .ok_or(SerializedGreenError::Overflow(
                    "Setext replacement target leaf index",
                ))?,
            byte_offset: replacement_target.byte_offset,
            event_ordinal_in_leaf: replacement_target.event_ordinal_in_leaf,
            source_before_in_leaf: replacement_target.source_before_in_leaf,
        });
        self.phase = SerializedGreenStreamPhase::AllocatingSetextReplacementLeaf;
        self.record_scratch();
        Ok(())
    }

    fn allocate_setext_replacement_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        let job = self
            .setext_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement allocation lost its job",
            ))?;
        let index = job.next_replacement_page;
        if index >= job.replacement_page_count
            || job.replacement_page_count == 0
            || job.replacement_page_count > self.setext_scratch.pages.len()
        {
            return Err(SerializedGreenError::Corrupt(
                "Setext replacement page index is invalid",
            ));
        }
        let page = &self.setext_scratch.pages[index];
        page.require_fixed_capacity()?;
        if !page.sealed || page.is_empty() {
            return Err(SerializedGreenError::Corrupt(
                "Setext replacement page is not sealed",
            ));
        }
        let (leaf, allocation) = session.allocate_packed(&page.bytes, &page.programs)?;
        self.receipt.leaf_pages_allocated += 1;
        self.receipt.setext_replacement_leaf_pages_allocated = self
            .receipt
            .setext_replacement_leaf_pages_allocated
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "Setext replacement allocation count",
            ))?;
        self.receipt.resumable_arena_allocations += 1;
        self.receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        self.receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        self.sequence
            .begin_push(session, leaf, &mut self.sequence_receipt)?;
        self.setext_job
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement allocation lost its job",
            ))?
            .next_replacement_page = index + 1;
        self.phase = SerializedGreenStreamPhase::PushingSetextReplacementLeaf;
        Ok(())
    }

    fn poll_setext_replacement_push(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self
            .sequence
            .poll_push(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let job = self
            .setext_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement push lost its job",
            ))?;
        if job.next_replacement_page < job.replacement_page_count {
            self.phase = SerializedGreenStreamPhase::AllocatingSetextReplacementLeaf;
        } else {
            self.sequence.begin_finish(&mut self.sequence_receipt)?;
            self.phase = SerializedGreenStreamPhase::ReducingSetextReplacement;
        }
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_setext_replacement_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let replacement = self.sequence.take_root()?;
        let replacement_summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&replacement)?)?
                .0;
        let job = self
            .setext_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement reduction lost its job",
            ))?;
        let expected_replacement_leaves = u64::try_from(job.replacement_page_count)
            .map_err(|_| SerializedGreenError::Overflow("Setext replacement leaf count"))?;
        let prepared_summary = self.setext_scratch.pages[..job.replacement_page_count]
            .iter()
            .try_fold(GreenSummary::default(), |summary, page| {
                summary.followed_by(page.summary)
            })?;
        if replacement_summary.leaves != expected_replacement_leaves
            || !replacement_summary.same_semantics(prepared_summary)
        {
            return Err(SerializedGreenError::Corrupt(
                "Setext replacement reduction changed packed pages",
            ));
        }
        let ProvisionalParagraphStorage::Sealed { leaf_index, .. } = job.active.storage else {
            return Err(SerializedGreenError::Corrupt(
                "Setext replacement lost its sealed target",
            ));
        };
        let prefix = self
            .working_prefix
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement splice lost its working prefix",
            ))?;
        let leaf_end = leaf_index
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "Setext replacement leaf range",
            ))?;
        self.begin_canonical_leaf_replacement(
            session,
            prefix.owner,
            leaf_index..leaf_end,
            replacement,
        )?;
        self.phase = SerializedGreenStreamPhase::SplicingSetextReplacement;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    /// Parser-builder seam for append-only insertion. The empty range makes
    /// the retained identity prefix invariant explicit and cannot shrink its
    /// provenance.
    fn begin_canonical_leaf_insertion(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        root: ArenaBuildOwner,
        insertion: u64,
        suffix: ArenaBuildOwner,
    ) -> Result<(), SerializedGreenError> {
        self.splice.begin_from_owned(
            session,
            Some(root),
            insertion..insertion,
            Some(suffix),
            &mut self.sequence_receipt,
        )
    }

    /// Parser-builder seam for a retroactive replacement of already
    /// sealed leaves. Retained-prefix identity provenance is monotone: any
    /// rewrite caps it at the first touched leaf before the persistent splice
    /// begins. Appends use the separate empty-range paths and cannot shrink
    /// the retained prefix.
    ///
    /// Keeping this boundary construct-neutral is important. Setext is the
    /// first caller, but a future GFM normalization must report only its exact
    /// replacement range here; host authority never needs to understand the
    /// grammar construct that caused it.
    fn begin_canonical_leaf_replacement(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        root: ArenaBuildOwner,
        range: Range<u64>,
        replacement: ArenaBuildOwner,
    ) -> Result<(), SerializedGreenError> {
        if range.is_empty() {
            return Err(SerializedGreenError::Invalid(
                "canonical leaf replacement requires a nonempty range",
            ));
        }
        #[cfg(feature = "host-mirror-probe")]
        if let Some(prefix) = self.retained_host_prefix.as_mut() {
            prefix.cap_before_rewrite(self.build, range.start)?;
        }
        self.splice.begin_from_owned(
            session,
            Some(root),
            range,
            Some(replacement),
            &mut self.sequence_receipt,
        )
    }

    fn poll_setext_replacement_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement splice produced an empty prefix",
            ))?;
        let summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&root)?)?.0;
        let job = self
            .setext_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement splice lost its job",
            ))?;
        let base = job
            .base_prefix_summary
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement splice lost its base summary",
            ))?;
        let expected_leaves = job
            .expected_prefix_leaves
            .ok_or(SerializedGreenError::Corrupt(
                "Setext replacement splice lost its expected leaf count",
            ))?;
        let active = job.active;
        let replacement_block = job.replacement_block;
        let facts = job.facts;
        let replacement_target = job.replacement_target.ok_or(SerializedGreenError::Corrupt(
            "Setext replacement lost its promoted Enter locator",
        ))?;
        if summary.leaves != expected_leaves || !summary.same_semantics(base) {
            return Err(SerializedGreenError::Corrupt(
                "Setext replacement splice changed distant semantics",
            ));
        }
        self.sealed_leaves = expected_leaves;
        self.install_working_prefix(session, root, summary)?;
        self.validator.retype_active_terminal(
            active.block,
            replacement_block,
            GreenKind::PARAGRAPH,
            GreenKind::HEADING,
            facts.into_envelope(),
        )?;
        let _ = self.setext_job.take().ok_or(SerializedGreenError::Corrupt(
            "Setext replacement job disappeared after install",
        ))?;
        self.ready_setext_promotion = Some(SetextPromotion {
            build: self.build,
            retired_block: active.block,
            block: replacement_block,
            event_ordinal: active.event_ordinal,
            source_before: active.source_before,
            facts,
            storage: replacement_target,
        });
        self.receipt.setext_sealed_promotions_completed = self
            .receipt
            .setext_sealed_promotions_completed
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "sealed Setext promotion count",
            ))?;
        self.setext_scratch.reset()?;
        self.phase = SerializedGreenStreamPhase::Accepting;
        self.record_scratch();
        Ok(SerializedGreenStreamProgress::ReadyForEvent)
    }

    fn poll_whole_normalization_tail_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.whole_normalization_job.is_none() {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization tail reduction lost its job",
            ));
        }
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let tail = self.sequence.take_root()?;
        let tail_summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&tail)?)?.0;
        if tail_summary.leaves != self.tail_sealed_leaves
            || !tail_summary.same_semantics(self.tail_summary)
        {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization tail reduction changed its summary",
            ));
        }
        let Some(prefix) = self.working_prefix.take() else {
            self.install_working_prefix(session, tail, self.tail_summary)?;
            self.tail_sealed_leaves = 0;
            self.tail_summary = GreenSummary::default();
            self.phase = SerializedGreenStreamPhase::PreparingWholeNormalizationRepack;
            return Ok(SerializedGreenStreamProgress::Pending);
        };
        if prefix.build != self.build {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization prefix belongs to another build",
            ));
        }
        let expected = prefix.summary.followed_by(self.tail_summary)?;
        let insertion = prefix.summary.leaves;
        self.begin_canonical_leaf_insertion(session, prefix.owner, insertion, tail)?;
        self.pending_splice_summary = Some(expected);
        self.phase = SerializedGreenStreamPhase::SplicingWholeNormalizationTail;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_whole_normalization_tail_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization tail splice produced no prefix",
            ))?;
        let expected = self
            .pending_splice_summary
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization tail splice lost its summary",
            ))?;
        self.install_working_prefix(session, root, expected)?;
        self.tail_sealed_leaves = 0;
        self.tail_summary = GreenSummary::default();
        self.phase = SerializedGreenStreamPhase::PreparingWholeNormalizationRepack;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn prepare_whole_normalization_repack(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        let prefix = self
            .working_prefix
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "whole normalization has no installed prefix",
            ))?;
        if prefix.build != self.build
            || prefix.summary.leaves != self.sealed_leaves
            || self.tail_sealed_leaves != 0
        {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization prefix is not the complete sealed stream",
            ));
        }
        let job = self
            .whole_normalization_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization repack lost its job",
            ))?;
        let ProvisionalParagraphStorage::Sealed {
            leaf_index,
            byte_offset,
            event_ordinal_in_leaf,
            source_before_in_leaf,
        } = job.target
        else {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization repack target was not sealed",
            ));
        };
        let root = session.owner_id(&prefix.owner)?;
        let (leaf, prefix_before_leaf) =
            locate_green_leaf_with_prefix(session.arena(), root, leaf_index)?;
        if prefix_before_leaf
            .tokens
            .checked_add(event_ordinal_in_leaf)
            .ok_or(SerializedGreenError::Overflow(
                "whole-normalization target event ordinal",
            ))?
            != job.storage.event_ordinal
            || prefix_before_leaf
                .metric
                .checked_add(source_before_in_leaf)?
                != job.storage.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let (old_summary, replacement_summary) = prepare_whole_normalization_leaf_repack(
            session.arena(),
            leaf,
            event_ordinal_in_leaf,
            byte_offset,
            job.authority.replacement_block(),
            job.storage.facts,
            &job.encoded_enter,
            &mut self.setext_scratch,
        )?;
        if old_summary != replacement_summary || replacement_summary.leaves != 1 {
            return Err(SerializedGreenError::Corrupt(
                "whole normalization changed leaf semantics or cardinality",
            ));
        }
        self.whole_normalization_job
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization repack lost its job",
            ))?
            .base_prefix_summary = Some(prefix.summary);
        self.phase = SerializedGreenStreamPhase::AllocatingWholeNormalizationLeaf;
        self.record_scratch();
        Ok(())
    }

    fn allocate_whole_normalization_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        if self.whole_normalization_job.is_none() || self.setext_scratch.page_count != 1 {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization allocation lost its one-leaf recipe",
            ));
        }
        let page = &self.setext_scratch.pages[0];
        page.require_fixed_capacity()?;
        if !page.sealed || page.is_empty() {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization replacement leaf is not sealed",
            ));
        }
        let (leaf, allocation) = session.allocate_packed(&page.bytes, &page.programs)?;
        self.receipt.leaf_pages_allocated =
            self.receipt.leaf_pages_allocated.checked_add(1).ok_or(
                SerializedGreenError::Overflow("whole-normalization leaf allocation count"),
            )?;
        self.receipt.resumable_arena_allocations = self
            .receipt
            .resumable_arena_allocations
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "whole-normalization arena allocation count",
            ))?;
        self.receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        self.receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        self.sequence
            .begin_push(session, leaf, &mut self.sequence_receipt)?;
        self.phase = SerializedGreenStreamPhase::PushingWholeNormalizationLeaf;
        Ok(())
    }

    fn poll_whole_normalization_leaf_push(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self
            .sequence
            .poll_push(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        self.sequence.begin_finish(&mut self.sequence_receipt)?;
        self.phase = SerializedGreenStreamPhase::ReducingWholeNormalizationReplacement;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_whole_normalization_replacement_reduction(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self
            .sequence
            .poll_finish(session, &mut self.sequence_receipt)?
            == ResumableSequenceProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let replacement = self.sequence.take_root()?;
        let replacement_summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&replacement)?)?
                .0;
        if replacement_summary != self.setext_scratch.pages[0].summary {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization replacement reduction changed its leaf",
            ));
        }
        let job = self
            .whole_normalization_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization replacement lost its job",
            ))?;
        let ProvisionalParagraphStorage::Sealed { leaf_index, .. } = job.target else {
            return Err(SerializedGreenError::Corrupt(
                "whole-normalization replacement lost its sealed target",
            ));
        };
        let prefix = self
            .working_prefix
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization replacement lost its prefix",
            ))?;
        let leaf_end = leaf_index
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "whole-normalization replacement leaf range",
            ))?;
        self.begin_canonical_leaf_replacement(
            session,
            prefix.owner,
            leaf_index..leaf_end,
            replacement,
        )?;
        self.phase = SerializedGreenStreamPhase::SplicingWholeNormalizationReplacement;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn poll_whole_normalization_replacement_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        if self.splice.poll(session, &mut self.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok(SerializedGreenStreamProgress::Pending);
        }
        let root = self
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization splice produced no prefix",
            ))?;
        let summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&root)?)?.0;
        let job = self
            .whole_normalization_job
            .as_ref()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization splice lost its job",
            ))?;
        let base = job
            .base_prefix_summary
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization splice lost its base summary",
            ))?;
        if summary != base {
            return Err(SerializedGreenError::Corrupt(
                "whole normalization changed distant semantics",
            ));
        }
        if job.authority.build_id() != self.build
            || job.storage.build != self.build
            || job.authority.retired_block() != job.storage.retired_block
            || job.authority.replacement_block() != job.storage.block
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        self.install_working_prefix(session, root, summary)?;
        self.whole_normalization_job
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization job disappeared after install",
            ))?;
        self.ready_whole_normalization = Some(WholeNormalizationReidentity { build: self.build });
        self.setext_scratch.reset()?;
        self.phase = SerializedGreenStreamPhase::Accepting;
        self.record_scratch();
        Ok(SerializedGreenStreamProgress::ReadyForEvent)
    }

    fn install_working_prefix(
        &mut self,
        session: &ArenaBuildSession<'_>,
        owner: ArenaBuildOwner,
        expected: GreenSummary,
    ) -> Result<(), SerializedGreenError> {
        if self.working_prefix.is_some() {
            return Err(SerializedGreenError::Corrupt(
                "working-prefix install would create a root chain",
            ));
        }
        let summary =
            sequence_node::<SerializedGreenSpec>(session.arena(), session.owner_id(&owner)?)?.0;
        if summary.leaves != expected.leaves
            || !summary.same_semantics(expected)
            || summary.leaves != self.sealed_leaves
            || summary.tokens != self.sealed_events
            || summary.metric != self.sealed_metric
        {
            return Err(SerializedGreenError::Corrupt(
                "working-prefix install changed exact green continuity",
            ));
        }
        self.working_prefix = Some(SerializedGreenWorkingPrefix {
            build: self.build,
            owner,
            summary,
        });
        self.receipt.maximum_working_prefixes = self.receipt.maximum_working_prefixes.max(1);
        Ok(())
    }

    fn complete_tail_reduction(
        &mut self,
        finalizing: bool,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        self.tail_sealed_leaves = 0;
        self.tail_summary = GreenSummary::default();
        if finalizing {
            self.phase = SerializedGreenStreamPhase::AllocatingManifest;
            Ok(SerializedGreenStreamProgress::Pending)
        } else {
            self.ready_working_cut = Some(self.current_working_cut()?);
            self.record_working_reduction(false)?;
            self.phase = SerializedGreenStreamPhase::Accepting;
            Ok(SerializedGreenStreamProgress::ReadyForEvent)
        }
    }

    fn poll_pending_event(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenStreamProgress, SerializedGreenError> {
        let Some(event) = self.pending_event.take() else {
            return Ok(SerializedGreenStreamProgress::ReadyForEvent);
        };
        if self.leaf.can_fit(&event) {
            let byte_offset = u16::try_from(self.leaf.bytes.len())
                .map_err(|_| SerializedGreenError::Overflow("green leaf event offset"))?;
            let event_ordinal_in_leaf = self.leaf.summary.tokens;
            let source_before_in_leaf = self.leaf.summary.metric;
            self.leaf.push(event)?;
            if let Some(pending) = self.pending_provisional_paragraph.take() {
                let expected_event = self
                    .sealed_events
                    .checked_add(event_ordinal_in_leaf)
                    .ok_or(SerializedGreenError::Overflow(
                        "provisional Paragraph event ordinal",
                    ))?;
                let expected_source = self.sealed_metric.checked_add(source_before_in_leaf)?;
                if pending.build != self.build
                    || pending.event_ordinal != expected_event
                    || pending.source_before != expected_source
                {
                    return Err(SerializedGreenError::Corrupt(
                        "provisional Paragraph acknowledgement moved its logical cut",
                    ));
                }
                let active = ActiveProvisionalParagraph {
                    build: pending.build,
                    block: pending.block,
                    generation: pending.generation,
                    event_ordinal: pending.event_ordinal,
                    source_before: pending.source_before,
                    storage: ProvisionalParagraphStorage::Partial {
                        byte_offset,
                        event_ordinal_in_leaf,
                        source_before_in_leaf,
                    },
                };
                self.active_provisional_paragraph = Some(active);
                self.ready_provisional_paragraph = Some(ProvisionalParagraphEnter {
                    build: pending.build,
                    block: pending.block,
                    generation: pending.generation,
                    event_ordinal: pending.event_ordinal,
                    source_before: pending.source_before,
                });
            }
            self.record_scratch();
            return Ok(SerializedGreenStreamProgress::ReadyForEvent);
        }
        self.pending_event = Some(event);
        if self.leaf.is_empty() {
            return Err(SerializedGreenError::Invalid("event exceeds green leaf"));
        }
        let leaf = self.allocate_partial_leaf(session)?;
        self.sequence
            .begin_push(session, leaf, &mut self.sequence_receipt)?;
        self.phase = SerializedGreenStreamPhase::PushingForPendingEvent;
        Ok(SerializedGreenStreamProgress::Pending)
    }

    fn partial_setext_can_fit(&self, extra_bytes: usize) -> bool {
        self.leaf.bytes.len()
            + extra_bytes
            + self.leaf.programs.len() * std::mem::size_of::<ArenaId>()
            <= ARENA_PAGE_BYTES
            && self
                .leaf
                .bytes
                .len()
                .checked_add(extra_bytes)
                .is_some_and(|length| length <= self.leaf.bytes.capacity())
    }

    fn rewrite_partial_setext(
        &mut self,
        active: ActiveProvisionalParagraph,
        encoded_heading: &[u8],
    ) -> Result<(), SerializedGreenError> {
        let ProvisionalParagraphStorage::Partial {
            byte_offset,
            event_ordinal_in_leaf,
            source_before_in_leaf,
        } = active.storage
        else {
            return Err(SerializedGreenError::Corrupt(
                "partial Setext rewrite targets a sealed leaf",
            ));
        };
        if active.build != self.build
            || self
                .sealed_events
                .checked_add(event_ordinal_in_leaf)
                .ok_or(SerializedGreenError::Overflow(
                    "partial Setext event ordinal",
                ))?
                != active.event_ordinal
            || self.sealed_metric.checked_add(source_before_in_leaf)? != active.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let old_len = encoded_heading
            .len()
            .checked_sub(7)
            .ok_or(SerializedGreenError::Corrupt(
                "Setext Heading descriptor is shorter than Paragraph",
            ))?;
        let offset = usize::from(byte_offset);
        let old_end = offset
            .checked_add(old_len)
            .ok_or(SerializedGreenError::Overflow(
                "partial Setext descriptor end",
            ))?;
        let old = self
            .leaf
            .bytes
            .get(offset..old_end)
            .ok_or(SerializedGreenError::StaleCursor)?;
        let mut expected = [0_u8; 10];
        if old_len != expected.len() {
            return Err(SerializedGreenError::Corrupt(
                "Paragraph Enter descriptor has unexpected length",
            ));
        }
        expected[0] = ENTER_NO_FACTS_TAG;
        expected[1] = GreenKind::PARAGRAPH.0;
        expected[2..10].copy_from_slice(&active.block.0.to_le_bytes());
        if old != expected {
            return Err(SerializedGreenError::StaleCursor);
        }
        if !self.partial_setext_can_fit(7) {
            return Err(SerializedGreenError::Invalid(
                "partial Setext expansion does not fit",
            ));
        }
        let previous_len = self.leaf.bytes.len();
        let next_len = previous_len
            .checked_add(7)
            .ok_or(SerializedGreenError::Overflow(
                "partial Setext payload length",
            ))?;
        self.leaf.bytes.resize(next_len, 0);
        self.leaf
            .bytes
            .copy_within(old_end..previous_len, old_end + 7);
        self.leaf.bytes[offset..offset + encoded_heading.len()].copy_from_slice(encoded_heading);
        self.leaf.require_fixed_capacity()?;
        self.record_scratch();
        Ok(())
    }

    fn rewrite_partial_whole_normalization(
        &mut self,
        target: DeferredNormalizationGreenTarget,
        expected_heading: &[u8],
        reidentified_heading: &[u8],
    ) -> Result<(), SerializedGreenError> {
        let ProvisionalParagraphStorage::Partial {
            byte_offset,
            event_ordinal_in_leaf,
            source_before_in_leaf,
        } = target.storage
        else {
            return Err(SerializedGreenError::Corrupt(
                "partial whole normalization targets a sealed leaf",
            ));
        };
        if target.build != self.build
            || expected_heading.len() != reidentified_heading.len()
            || self
                .sealed_events
                .checked_add(event_ordinal_in_leaf)
                .ok_or(SerializedGreenError::Overflow(
                    "whole normalization event ordinal",
                ))?
                != target.event_ordinal
            || self.sealed_metric.checked_add(source_before_in_leaf)? != target.source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let start = usize::from(byte_offset);
        let end =
            start
                .checked_add(expected_heading.len())
                .ok_or(SerializedGreenError::Overflow(
                    "whole normalization descriptor end",
                ))?;
        if self
            .leaf
            .bytes
            .get(start..end)
            .ok_or(SerializedGreenError::StaleCursor)?
            != expected_heading
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        self.leaf.bytes[start..end].copy_from_slice(reidentified_heading);
        self.leaf.require_fixed_capacity()?;
        self.record_scratch();
        Ok(())
    }

    fn allocate_partial_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ArenaBuildOwner, SerializedGreenError> {
        self.leaf.seal_in_place()?;
        let sealed_summary = self.leaf.summary;
        let mut ids = [ArenaId::default(); MAX_PACKED_ARENA_CHILDREN];
        let program_count = self.leaf.programs.len();
        if program_count > ids.len() {
            return Err(SerializedGreenError::Corrupt(
                "partial leaf exceeds packed child capacity",
            ));
        }
        for (slot, program) in ids.iter_mut().zip(&self.leaf.programs) {
            *slot = session.owner_id(program)?;
        }
        let (leaf, allocation) =
            session.allocate_packed(&self.leaf.bytes, &ids[..program_count])?;
        self.receipt.leaf_pages_allocated += 1;
        self.receipt.resumable_arena_allocations += 1;
        self.receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        self.receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        self.receipt.fixed_leaf_child_id_scratch_bytes = self
            .receipt
            .fixed_leaf_child_id_scratch_bytes
            .max(std::mem::size_of_val(&ids));
        while let Some(program) = self.leaf.programs.pop() {
            session.release(program)?;
        }
        self.mark_provisional_paragraph_sealed(self.sealed_leaves)?;
        self.sealed_leaves = self
            .sealed_leaves
            .checked_add(sealed_summary.leaves)
            .ok_or(SerializedGreenError::Overflow("sealed green leaves"))?;
        self.sealed_events = self
            .sealed_events
            .checked_add(sealed_summary.tokens)
            .ok_or(SerializedGreenError::Overflow("sealed green events"))?;
        self.sealed_metric = self.sealed_metric.checked_add(sealed_summary.metric)?;
        self.tail_sealed_leaves = self
            .tail_sealed_leaves
            .checked_add(sealed_summary.leaves)
            .ok_or(SerializedGreenError::Overflow("sealed tail leaves"))?;
        self.tail_summary = self.tail_summary.followed_by(sealed_summary)?;
        self.record_scratch();
        self.leaf.reset_after_allocation();
        Ok(leaf)
    }

    fn mark_provisional_paragraph_sealed(
        &mut self,
        leaf_index: u64,
    ) -> Result<(), SerializedGreenError> {
        let active_records = usize::from(self.active_provisional_paragraph.is_some())
            + usize::from(self.setext_job.is_some())
            + usize::from(self.fragment_job.is_some());
        if active_records > 1 {
            return Err(SerializedGreenError::Corrupt(
                "normalization storage has two active Paragraph records",
            ));
        }
        let sealed_events = self.sealed_events;
        let sealed_metric = self.sealed_metric;
        let active = self
            .active_provisional_paragraph
            .as_mut()
            .or_else(|| self.setext_job.as_mut().map(|job| &mut job.active))
            .or_else(|| self.fragment_job.as_mut().map(|job| &mut job.active));
        if let Some(active) = active
            && let ProvisionalParagraphStorage::Partial {
                byte_offset,
                event_ordinal_in_leaf,
                source_before_in_leaf,
            } = active.storage
        {
            if active.build != self.build
                || sealed_events.checked_add(event_ordinal_in_leaf).ok_or(
                    SerializedGreenError::Overflow("sealed provisional event ordinal"),
                )? != active.event_ordinal
                || sealed_metric.checked_add(source_before_in_leaf)? != active.source_before
            {
                return Err(SerializedGreenError::Corrupt(
                    "provisional Paragraph changed logical position while sealing",
                ));
            }
            active.storage = ProvisionalParagraphStorage::Sealed {
                leaf_index,
                byte_offset,
                event_ordinal_in_leaf,
                source_before_in_leaf,
            };
        }
        if let Some(target) = self.deferred_normalization_target.as_mut()
            && let ProvisionalParagraphStorage::Partial {
                byte_offset,
                event_ordinal_in_leaf,
                source_before_in_leaf,
            } = target.storage
        {
            if target.build != self.build
                || sealed_events.checked_add(event_ordinal_in_leaf).ok_or(
                    SerializedGreenError::Overflow("sealed deferred-normalization event ordinal"),
                )? != target.event_ordinal
                || sealed_metric.checked_add(source_before_in_leaf)? != target.source_before
            {
                return Err(SerializedGreenError::Corrupt(
                    "deferred normalization changed logical position while sealing",
                ));
            }
            target.storage = ProvisionalParagraphStorage::Sealed {
                leaf_index,
                byte_offset,
                event_ordinal_in_leaf,
                source_before_in_leaf,
            };
        }
        Ok(())
    }

    fn current_leaf_cut(&self) -> SerializedGreenLeafCut {
        SerializedGreenLeafCut {
            build: self.build,
            leaves_before: self.sealed_leaves,
            events_before: self.sealed_events,
            source_before: self.sealed_metric,
        }
    }

    fn current_working_cut(&self) -> Result<SerializedGreenWorkingCut, SerializedGreenError> {
        let installed_leaves_before = self
            .working_prefix
            .as_ref()
            .map_or(0, |prefix| prefix.summary.leaves);
        if self.tail_sealed_leaves != 0 || installed_leaves_before != self.sealed_leaves {
            return Err(SerializedGreenError::Corrupt(
                "working cut was minted before sealed tail installation",
            ));
        }
        Ok(SerializedGreenWorkingCut {
            build: self.build,
            installed_leaves_before,
            events_before: self
                .sealed_events
                .checked_add(self.leaf.summary.tokens)
                .ok_or(SerializedGreenError::Overflow("working-cut event count"))?,
            source_before: self.sealed_metric.checked_add(self.leaf.summary.metric)?,
        })
    }

    fn record_working_reduction(&mut self, noop: bool) -> Result<(), SerializedGreenError> {
        self.receipt.working_prefix_reductions_completed = self
            .receipt
            .working_prefix_reductions_completed
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "working-prefix reduction count",
            ))?;
        if noop {
            self.receipt.working_prefix_noop_reductions = self
                .receipt
                .working_prefix_noop_reductions
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow("working-prefix no-op count"))?;
        }
        Ok(())
    }

    fn allocate_manifest(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        root: SerializedGreenWorkingPrefix,
    ) -> Result<(), SerializedGreenError> {
        if root.build != self.build {
            return Err(SerializedGreenError::Corrupt(
                "manifest prefix belongs to another build generation",
            ));
        }
        let root_id = session.owner_id(&root.owner)?;
        let summary = sequence_node::<SerializedGreenSpec>(session.arena(), root_id)?.0;
        if summary != root.summary {
            return Err(SerializedGreenError::Corrupt(
                "manifest prefix summary changed before publication",
            ));
        }
        if summary.balance != 0 || summary.minimum_prefix < 0 || summary.blocks == 0 {
            return Err(SerializedGreenError::Invalid(
                "green document is structurally unbalanced",
            ));
        }
        if self.spec.source_bytes != summary.metric.bytes
            || self.spec.source_utf16 != summary.metric.utf16
            || self.spec.known_bytes.end > summary.metric.bytes
        {
            return Err(SerializedGreenError::Invalid(
                "green coverage does not match bound source length",
            ));
        }
        let manifest = Manifest {
            syntax_profile: self.spec.syntax_profile,
            source_revision: self.spec.source_revision,
            source_root: self.spec.source_root,
            source_bytes: self.spec.source_bytes,
            source_utf16: self.spec.source_utf16,
            grammar_revision: self.spec.grammar_revision,
            parse_generation: self.spec.parse_generation,
            semantic_epoch: self.spec.semantic_epoch,
            known_bytes: self.spec.known_bytes.clone(),
            summary,
        };
        self.receipt.final_sequence_height = summary.height;
        let payload = encode_manifest(&manifest);
        let (manifest_owner, allocation) = session.allocate(&payload, &[root_id])?;
        self.receipt.manifest_nodes_allocated += 1;
        self.receipt.resumable_arena_allocations += 1;
        self.receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        self.receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        session.release(root.owner)?;
        // This is a sub-builder handoff invariant, not the document commit
        // gate. A checkpoint-index sibling may already be owned by the same
        // journal, so counting the whole journal here would couple otherwise
        // independent children to build order. Re-decode the exact manifest
        // owner after the linear root transfer instead; the composite builder
        // is responsible for reducing all typed children to one journal root.
        let manifest_id = session.owner_id(&manifest_owner)?;
        decode_document(session.arena(), manifest_id)?;
        self.manifest = Some(manifest_owner);
        self.phase = SerializedGreenStreamPhase::ManifestReady;
        Ok(())
    }

    fn ensure_session(&self, session: &ArenaBuildSession<'_>) -> Result<(), SerializedGreenError> {
        if session.id() != self.build {
            return Err(SerializedGreenError::Invalid(
                "arena session belongs to another build generation",
            ));
        }
        session.live_owners()?;
        Ok(())
    }

    fn record_scratch(&mut self) {
        record_validator_scratch(&mut self.receipt, &self.validator);
        self.receipt.maximum_partial_leaf_payload_len = self
            .receipt
            .maximum_partial_leaf_payload_len
            .max(self.leaf.bytes.len());
        self.receipt.maximum_partial_leaf_payload_capacity = self
            .receipt
            .maximum_partial_leaf_payload_capacity
            .max(self.leaf.bytes.capacity());
        self.receipt.partial_leaf_payload_requested_bytes = ARENA_PAGE_BYTES;
        self.receipt.maximum_partial_leaf_program_owners = self
            .receipt
            .maximum_partial_leaf_program_owners
            .max(self.leaf.programs.len());
        self.receipt
            .maximum_partial_leaf_program_owner_capacity_bytes = self
            .receipt
            .maximum_partial_leaf_program_owner_capacity_bytes
            .max(self.leaf.programs.capacity() * std::mem::size_of::<ArenaBuildOwner>());
        self.receipt.partial_leaf_program_owner_logical_slots = self.leaf.program_slot_limit;
        self.receipt.partial_leaf_program_owner_requested_bytes = self
            .leaf
            .program_slot_limit
            .saturating_mul(std::mem::size_of::<ArenaBuildOwner>());
        if let Some(event) = &self.pending_event {
            self.receipt.maximum_pending_event_payload_len = self
                .receipt
                .maximum_pending_event_payload_len
                .max(event.bytes.len());
            self.receipt.maximum_pending_event_payload_capacity = self
                .receipt
                .maximum_pending_event_payload_capacity
                .max(event.bytes.capacity());
        }
        self.receipt.maximum_encoded_page_buffer_bytes =
            self.receipt.maximum_encoded_page_buffer_bytes.max(
                self.leaf.bytes.capacity()
                    + self.leaf.programs.capacity() * std::mem::size_of::<ArenaBuildOwner>(),
            );
        self.receipt.maximum_setext_repack_scratch_bytes = self
            .receipt
            .maximum_setext_repack_scratch_bytes
            .max(self.setext_scratch.scratch_bytes());
    }

    fn sync_journal_receipt(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        let metrics = session
            .arena()
            .build_journal_metrics(self.build)
            .map_err(SerializedGreenError::ArenaBuild)?;
        self.receipt.maximum_live_owner_handles = self
            .receipt
            .maximum_live_owner_handles
            .max(metrics.maximum_live_owners);
        self.receipt.owner_journal_capacity = self
            .receipt
            .owner_journal_capacity
            .max(metrics.slot_capacity);
        self.receipt.owner_journal_bytes =
            self.receipt.owner_journal_bytes.max(metrics.storage_bytes);
        Ok(())
    }
}

/// The sole build-owned, physically source-complete manifest produced by the
/// resumable mechanism. It can only be published by consuming its exact arena
/// session through `commit`.
#[derive(Debug)]
pub struct SerializedGreenBuildManifest {
    build: ArenaBuildId,
    owner: ArenaBuildOwner,
    receipt: SerializedGreenBuildReceipt,
}

impl SerializedGreenBuildManifest {
    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn receipt(&self) -> SerializedGreenBuildReceipt {
        self.receipt
    }

    /// Validates this typed build-local manifest before a composite parent
    /// adopts it through an arena child edge.
    pub(crate) fn validate_composite_child(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ArenaId, SerializedGreenError> {
        Ok(self.composite_descriptor(session)?.manifest)
    }

    /// Revalidates the exact build-owned manifest and returns the same typed
    /// identity/summary descriptor used by the committed parent read path.
    pub(crate) fn composite_descriptor(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenCompositeDescriptor, SerializedGreenError> {
        if session.id() != self.build {
            return Err(SerializedGreenError::Invalid(
                "manifest and arena session build generations differ",
            ));
        }
        let manifest = session.owner_id(&self.owner)?;
        validate_serialized_green_composite_child(session.arena(), manifest)
    }

    /// Linear transfer into a parent builder. The raw owner remains
    /// crate-private and cannot escape the typed composite seam.
    pub(crate) fn into_composite_parts(self) -> (ArenaBuildOwner, SerializedGreenBuildReceipt) {
        (self.owner, self.receipt)
    }

    pub fn commit(
        self,
        session: ArenaBuildSession<'_>,
    ) -> Result<(SerializedGreenDocument, SerializedGreenBuildReceipt), SerializedGreenError> {
        if session.id() != self.build {
            return Err(SerializedGreenError::Invalid(
                "manifest and arena session build generations differ",
            ));
        }
        let owner = session.commit(self.owner)?;
        let manifest = SerializedGreenManifestId::new(owner.scoped_id());
        Ok((SerializedGreenDocument { owner, manifest }, self.receipt))
    }
}

fn validate_root_spec(spec: &SerializedGreenRootSpec) -> Result<(), SerializedGreenError> {
    if spec.syntax_profile == 0
        || spec.source_root.0 == 0
        || spec.grammar_revision.0 == 0
        || spec.parse_generation.0 == 0
        || spec.semantic_epoch == 0
        || spec.known_bytes.start > spec.known_bytes.end
        || spec.known_bytes.end > spec.source_bytes
    {
        return Err(SerializedGreenError::Invalid("invalid root generations"));
    }
    Ok(())
}

fn allocate_leaf_page(
    transaction: &mut ArenaBuildTransaction<'_>,
    page: LeafEncoder,
    receipt: &mut SerializedGreenBuildReceipt,
) -> Result<ArenaOwnerHandle, SerializedGreenError> {
    receipt.maximum_pending_program_payload_bytes = receipt
        .maximum_pending_program_payload_bytes
        .max(page.pending_new_program_bytes);
    let (payload, _, programs) = page.seal()?;
    let mut program_handles = Vec::with_capacity(programs.len());
    for program in programs {
        match program {
            PendingProjectionProgram::New(program_payload) => {
                let program_capacity = program_payload.capacity();
                let (program, allocation) = transaction.allocate(&program_payload, &[])?;
                receipt.projection_program_pages_allocated += 1;
                receipt.payload_bytes_copied += allocation.payload_bytes_copied;
                receipt.edge_bytes_copied += allocation.edge_bytes_copied;
                receipt.maximum_projection_program_bytes = receipt
                    .maximum_projection_program_bytes
                    .max(program_capacity);
                program_handles.push(program);
            }
            PendingProjectionProgram::Retained(program) => {
                program_handles.push(transaction.retain(program)?);
            }
        }
    }
    let program_ids = program_handles
        .iter()
        .map(|program| transaction.id(program))
        .collect::<Vec<_>>();
    let (leaf, allocation) = transaction.allocate_packed(&payload, &program_ids)?;
    for program in program_handles {
        transaction.release(program)?;
    }
    receipt.leaf_pages_allocated += 1;
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.edge_bytes_copied += allocation.edge_bytes_copied;
    receipt.maximum_encoded_page_buffer_bytes = receipt
        .maximum_encoded_page_buffer_bytes
        .max(payload.capacity() + program_ids.capacity() * std::mem::size_of::<ArenaId>());
    Ok(leaf)
}

fn flush_leaf(
    transaction: &mut ArenaBuildTransaction<'_>,
    sequence: &mut StreamingSequenceBuilder<SerializedGreenSpec>,
    sequence_receipt: &mut SequenceMutationReceipt,
    page: LeafEncoder,
    receipt: &mut SerializedGreenBuildReceipt,
) -> Result<(), SerializedGreenError> {
    let leaf = allocate_leaf_page(transaction, page, receipt)?;
    sequence.push_handle(transaction, leaf, sequence_receipt)?;
    Ok(())
}

impl SerializedGreenDocument {
    pub fn build(
        arena: &mut PageArena,
        spec: SerializedGreenRootSpec,
        events: impl IntoIterator<Item = GreenEvent>,
        receipt: &mut SerializedGreenBuildReceipt,
    ) -> Result<Self, SerializedGreenError> {
        if spec.syntax_profile == 0
            || spec.source_root.0 == 0
            || spec.grammar_revision.0 == 0
            || spec.parse_generation.0 == 0
            || spec.semantic_epoch == 0
            || spec.known_bytes.start > spec.known_bytes.end
            || spec.known_bytes.end > spec.source_bytes
        {
            return Err(SerializedGreenError::Invalid("invalid root generations"));
        }
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut sequence = StreamingSequenceBuilder::<SerializedGreenSpec>::default();
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let mut page = LeafEncoder::default();
        let mut validator = StructuralValidator::default();
        for event in events {
            validator.push(&event)?;
            record_validator_scratch(receipt, &validator);
            let mut encoded = encode_event(&event, page.programs.len())?;
            if !page.is_empty() && !page.can_fit(&encoded) {
                flush_leaf(
                    &mut transaction,
                    &mut sequence,
                    &mut sequence_receipt,
                    page,
                    receipt,
                )?;
                page = LeafEncoder::default();
                encoded = encode_event(&event, 0)?;
            }
            page.push(&event, encoded)?;
        }
        validator.finish()?;
        if !page.is_empty() {
            flush_leaf(
                &mut transaction,
                &mut sequence,
                &mut sequence_receipt,
                page,
                receipt,
            )?;
        }
        let root = sequence
            .finish(&mut transaction, &mut sequence_receipt)?
            .ok_or(SerializedGreenError::Invalid("empty green document"))?;
        let summary =
            sequence_node::<SerializedGreenSpec>(transaction.arena(), transaction.id(&root))?.0;
        if summary.balance != 0 || summary.minimum_prefix < 0 || summary.blocks == 0 {
            return Err(SerializedGreenError::Invalid(
                "green document is structurally unbalanced",
            ));
        }
        if spec.source_bytes != summary.metric.bytes
            || spec.source_utf16 != summary.metric.utf16
            || spec.known_bytes.end > summary.metric.bytes
        {
            return Err(SerializedGreenError::Invalid(
                "green coverage does not match bound source length",
            ));
        }
        let manifest = Manifest {
            syntax_profile: spec.syntax_profile,
            source_revision: spec.source_revision,
            source_root: spec.source_root,
            source_bytes: spec.source_bytes,
            source_utf16: spec.source_utf16,
            grammar_revision: spec.grammar_revision,
            parse_generation: spec.parse_generation,
            semantic_epoch: spec.semantic_epoch,
            known_bytes: spec.known_bytes,
            summary,
        };
        receipt.final_sequence_height = summary.height;
        let payload = encode_manifest(&manifest);
        let (manifest_owner, allocation) =
            transaction.allocate(&payload, &[transaction.id(&root)])?;
        transaction.release(root)?;
        receipt.manifest_nodes_allocated += 1;
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        merge_sequence_receipt(receipt, sequence_receipt);
        sync_transaction_receipt(receipt, &transaction);
        debug_assert_eq!(transaction.live_owners(), 1);
        let owner = transaction.take(manifest_owner);
        let manifest = SerializedGreenManifestId::new(owner.scoped_id());
        Ok(Self { owner, manifest })
    }

    /// Constructs the Green query view owned by a terminal composite parent.
    /// The linear owner is the parent root while `manifest` is its validated
    /// Green child. Only CandidateWriter can mint this pairing; ordinary
    /// documents continue to bind both identities to the same root.
    pub(crate) fn from_candidate_writer_composite_parent(
        owner: OwnedArenaRef,
        manifest: ArenaScopedId,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Self {
        debug_assert_eq!(owner.scoped_id().arena(), manifest.arena());
        Self {
            owner,
            manifest: SerializedGreenManifestId::new(manifest),
        }
    }

    #[must_use]
    pub const fn manifest_id(&self) -> SerializedGreenManifestId {
        self.manifest
    }

    pub(crate) fn manifest_descriptor(
        &self,
        arena: &PageArena,
    ) -> Result<SerializedGreenManifestDescriptor, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let (manifest, _) = decode_document(arena, manifest_id)?;
        Ok(SerializedGreenManifestDescriptor::new(
            self.manifest_id(),
            &manifest,
        ))
    }

    fn local_manifest_id(&self, arena: &PageArena) -> Result<ArenaId, SerializedGreenError> {
        Ok(arena.local_id(self.manifest_id().scoped())?)
    }

    pub fn metric(&self, arena: &PageArena) -> Result<SerializedMetric, SerializedGreenError> {
        Ok(decode_document(arena, self.local_manifest_id(arena)?)?
            .0
            .summary
            .metric)
    }

    pub fn block_count(&self, arena: &PageArena) -> Result<u64, SerializedGreenError> {
        Ok(decode_document(arena, self.local_manifest_id(arena)?)?
            .0
            .summary
            .blocks)
    }

    pub fn leaf_count(&self, arena: &PageArena) -> Result<u64, SerializedGreenError> {
        Ok(decode_document(arena, self.local_manifest_id(arena)?)?
            .0
            .summary
            .leaves)
    }

    pub fn leaf_at(
        &self,
        arena: &PageArena,
        leaf_index: u64,
    ) -> Result<Option<ArenaId>, SerializedGreenError> {
        let (_, root) = decode_document(arena, self.local_manifest_id(arena)?)?;
        locate_leaf_in_arena(arena, root, leaf_index)
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), SerializedGreenReleaseError> {
        let manifest = self.manifest;
        match arena.release_later(self.owner) {
            Ok(()) => Ok(()),
            Err(transfer) => Err(SerializedGreenReleaseError {
                error: transfer.error,
                document: Self {
                    owner: transfer.owner,
                    manifest,
                },
            }),
        }
    }

    /// Rewrites complete Enter+facts records through capabilities obtained
    /// from one immutable base manifest. Every target is resolved before the
    /// candidate root exists; later targets therefore cannot be invalidated by
    /// earlier path copies.
    #[allow(clippy::too_many_lines)] // One visible transaction from capability validation to manifest transfer.
    pub fn rewrite_enters(
        &self,
        arena: &mut PageArena,
        next_parse_generation: ParseGeneration,
        next_semantic_epoch: u64,
        mut rewrites: Vec<GreenEnterRewrite>,
        receipt: &mut SerializedGreenBuildReceipt,
    ) -> Result<Self, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let manifest_capability = self.manifest_id();
        let (base_manifest, base_root) = decode_document(arena, manifest_id)?;
        if next_parse_generation.0 <= base_manifest.parse_generation.0
            || next_semantic_epoch <= base_manifest.semantic_epoch
        {
            return Err(SerializedGreenError::Invalid(
                "rewrite generation must advance",
            ));
        }
        if rewrites.is_empty() {
            return Err(SerializedGreenError::Invalid("empty Enter rewrite batch"));
        }
        rewrites
            .sort_by_key(|rewrite| (rewrite.target.base_leaf_index, rewrite.target.byte_offset));
        if rewrites.windows(2).any(|pair| {
            pair[0].target.base_leaf_index == pair[1].target.base_leaf_index
                && pair[0].target.byte_offset == pair[1].target.byte_offset
        }) {
            return Err(SerializedGreenError::Invalid(
                "duplicate Enter rewrite target",
            ));
        }
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut replacements = Vec::new();
        let mut cursor = 0;
        while cursor < rewrites.len() {
            let first = &rewrites[cursor].target;
            if first.manifest != manifest_capability {
                return Err(SerializedGreenError::StaleCursor);
            }
            let leaf_index = first.base_leaf_index;
            let expected_leaf = first.leaf;
            let end = cursor
                + rewrites[cursor..]
                    .iter()
                    .take_while(|rewrite| rewrite.target.base_leaf_index == leaf_index)
                    .count();
            if rewrites[cursor..end].iter().any(|rewrite| {
                rewrite.target.manifest != manifest_capability
                    || rewrite.target.leaf != expected_leaf
            }) {
                return Err(SerializedGreenError::StaleCursor);
            }
            let actual_leaf = locate_leaf_in_arena(transaction.arena(), base_root, leaf_index)?
                .ok_or(SerializedGreenError::StaleCursor)?;
            if actual_leaf != expected_leaf {
                return Err(SerializedGreenError::StaleCursor);
            }
            let payload_bytes = transaction.arena().payload(expected_leaf)?.len();
            let (_, decoded) = decode_leaf(transaction.arena(), expected_leaf)?;
            receipt.maximum_decoded_page_buffer_bytes = receipt
                .maximum_decoded_page_buffer_bytes
                .max(payload_bytes + decoded.capacity() * std::mem::size_of::<DecodedLeafEvent>());
            let mut events = decoded
                .into_iter()
                .map(|decoded| (decoded.byte_offset, decoded.event))
                .collect::<Vec<_>>();
            for rewrite in &rewrites[cursor..end] {
                let event = events
                    .iter_mut()
                    .find(|(offset, _)| *offset == rewrite.target.byte_offset)
                    .ok_or(SerializedGreenError::StaleCursor)?;
                let DecodedGreenEventKind::Enter { block, kind, .. } = &event.1 else {
                    return Err(SerializedGreenError::StaleCursor);
                };
                if *block != rewrite.target.block || *kind != rewrite.target.kind {
                    return Err(SerializedGreenError::StaleCursor);
                }
                if *kind != rewrite.kind {
                    return Err(SerializedGreenError::Invalid(
                        "Enter rewrite may only replace facts for the same block kind",
                    ));
                }
                validate_facts_for_kind(rewrite.kind, &rewrite.facts)?;
                event.1 = DecodedGreenEventKind::Enter {
                    block: *block,
                    kind: rewrite.kind,
                    facts: rewrite.facts.clone(),
                };
            }
            let handles = allocate_event_pages(
                &mut transaction,
                events.into_iter().map(|(_, event)| event),
                receipt,
            )?;
            replacements.push(BaseLeafReplacement {
                leaf_index,
                expected_leaf,
                replacements: handles,
            });
            cursor = end;
        }
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let next_root = replace_leaf_batch_in_transaction::<SerializedGreenSpec>(
            &mut transaction,
            Some(base_root),
            replacements,
            &mut sequence_receipt,
        )?
        .ok_or(SerializedGreenError::Corrupt("rewrite removed green root"))?;
        let next_summary =
            sequence_node::<SerializedGreenSpec>(transaction.arena(), transaction.id(&next_root))?
                .0;
        if !next_summary.same_semantics(base_manifest.summary) {
            return Err(SerializedGreenError::Corrupt(
                "Enter rewrite changed structural/source summary",
            ));
        }
        let next_manifest = Manifest {
            parse_generation: next_parse_generation,
            semantic_epoch: next_semantic_epoch,
            summary: next_summary,
            ..base_manifest
        };
        receipt.final_sequence_height = next_summary.height;
        let payload = encode_manifest(&next_manifest);
        let (manifest_owner, allocation) =
            transaction.allocate(&payload, &[transaction.id(&next_root)])?;
        transaction.release(next_root)?;
        receipt.manifest_nodes_allocated += 1;
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        merge_sequence_receipt(receipt, sequence_receipt);
        sync_transaction_receipt(receipt, &transaction);
        debug_assert_eq!(transaction.live_owners(), 1);
        let owner = transaction.take(manifest_owner);
        let manifest = SerializedGreenManifestId::new(owner.scoped_id());
        Ok(Self { owner, manifest })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenEnterRewrite {
    pub target: GreenEnterCapability,
    pub kind: GreenKind,
    pub facts: FactsEnvelope,
}

fn locate_leaf_in_arena(
    arena: &PageArena,
    root: ArenaId,
    leaf_index: u64,
) -> Result<Option<ArenaId>, SerializedGreenError> {
    let summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if leaf_index >= summary.leaves {
        return Ok(None);
    }
    let mut node = root;
    let mut index = leaf_index;
    loop {
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => return Ok(Some(node)),
            SequenceNodeKind::Branch { left, right } => {
                let left_leaves = sequence_node::<SerializedGreenSpec>(arena, left)?.0.leaves;
                if index < left_leaves {
                    node = left;
                } else {
                    index -= left_leaves;
                    node = right;
                }
            }
        }
    }
}

/// Worker-side exporter hook for a typed manifest already authenticated by a
/// publication proof. Raw local IDs never cross the copied-object envelope;
/// this helper only selects the exact leaf that will be copied while the
/// worker arena is still alive.
pub(crate) fn serialized_green_leaf_at_scoped_manifest(
    arena: &PageArena,
    manifest: ArenaScopedId,
    leaf_index: u64,
) -> Result<Option<ArenaId>, SerializedGreenError> {
    let manifest = arena.local_id(manifest)?;
    let (_, root) = decode_document(arena, manifest)?;
    locate_leaf_in_arena(arena, root, leaf_index)
}

/// Returns the exact metric and final leaf identity for a nonempty prefix of a
/// typed manifest. The caller supplies an already-authorized prefix length;
/// this logarithmic observation validates that claim but never searches for a
/// larger common range.
#[cfg(feature = "host-mirror-probe")]
pub(crate) fn serialized_green_prefix_metric_and_last_leaf_at_scoped_manifest(
    arena: &PageArena,
    manifest: ArenaScopedId,
    prefix_leaves: u64,
) -> Result<Option<(SerializedMetric, ArenaId)>, SerializedGreenError> {
    if prefix_leaves == 0 {
        return Ok(None);
    }
    let manifest = arena.local_id(manifest)?;
    let (_, root) = decode_document(arena, manifest)?;
    let last_index = prefix_leaves
        .checked_sub(1)
        .ok_or(SerializedGreenError::Corrupt(
            "nonempty green prefix lost its final leaf index",
        ))?;
    let (last_leaf, prefix_before_last) = locate_green_leaf_with_prefix(arena, root, last_index)?;
    let (last_summary, kind) = sequence_node::<SerializedGreenSpec>(arena, last_leaf)?;
    if !matches!(kind, SequenceNodeKind::Leaf) {
        return Err(SerializedGreenError::Corrupt(
            "green prefix final identity is not a leaf",
        ));
    }
    let prefix = prefix_before_last.followed_by(last_summary)?;
    if prefix.leaves != prefix_leaves {
        return Err(SerializedGreenError::Corrupt(
            "green prefix metric observation changed its leaf count",
        ));
    }
    Ok(Some((prefix.metric, last_leaf)))
}

fn locate_green_leaf_with_prefix(
    arena: &PageArena,
    root: ArenaId,
    leaf_index: u64,
) -> Result<(ArenaId, GreenSummary), SerializedGreenError> {
    let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if leaf_index >= root_summary.leaves {
        return Err(SerializedGreenError::StaleCursor);
    }
    let mut node = root;
    let mut index = leaf_index;
    let mut prefix = GreenSummary::default();
    loop {
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => return Ok((node, prefix)),
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                if index < left_summary.leaves {
                    node = left;
                } else {
                    index -= left_summary.leaves;
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

fn encode_decoded_event(
    arena: &PageArena,
    event: &DecodedGreenEventKind,
    program_ordinal: usize,
) -> Result<EncodedGreenEvent, SerializedGreenError> {
    match event {
        DecodedGreenEventKind::Enter { block, kind, facts } => encode_event(
            &GreenEvent::enter(*block, *kind, facts.clone()),
            program_ordinal,
        ),
        DecodedGreenEventKind::Exit {
            closed,
            last_line_blank,
            facts,
        } => encode_event(
            &GreenEvent::exit_with_state(*closed, *last_line_blank, *facts),
            program_ordinal,
        ),
        DecodedGreenEventKind::Coverage(run) => match &run.logical_contribution {
            DecodedLogicalContribution::Program(program) => {
                if program.physical_metric != run.metric || program.piece_count == 0 {
                    return Err(SerializedGreenError::Corrupt(
                        "retained Program reference does not match its run",
                    ));
                }
                validate_projection_program_edge_payload(
                    arena,
                    program.retained_page()?,
                    usize::from(program.piece_count),
                    program.physical_metric,
                    program.logical_metric,
                )?;
                let same_metric = run.metric.bytes == run.metric.utf16;
                let mut output = vec![
                    COVERAGE_TAG | run.part.0 | if same_metric { COVERAGE_SAME_METRIC } else { 0 },
                ];
                output.extend_from_slice(&run.id.0.to_le_bytes());
                push_varint(u64::from(run.owner_relative_depth), &mut output);
                push_varint(run.metric.bytes, &mut output);
                if !same_metric {
                    push_varint(run.metric.utf16, &mut output);
                }
                output.push(
                    LOGICAL_PROGRAM_TAG
                        | if run.projection_reset_after {
                            LOGICAL_PROJECTION_RESET_AFTER
                        } else {
                            0
                        },
                );
                let mut metric_descriptor = 0_u8;
                if program.logical_metric.bytes == program.logical_metric.utf16 {
                    metric_descriptor |= EVENT_METRIC_SAME;
                }
                output.push(metric_descriptor);
                push_varint(program.logical_metric.bytes, &mut output);
                if program.logical_metric.bytes != program.logical_metric.utf16 {
                    push_varint(program.logical_metric.utf16, &mut output);
                }
                let program_ordinal_offset = output.len();
                push_varint(
                    u64::try_from(program_ordinal)
                        .map_err(|_| SerializedGreenError::Overflow("program edge ordinal"))?,
                    &mut output,
                );
                push_varint(u64::from(program.piece_count), &mut output);
                Ok(EncodedGreenEvent {
                    bytes: output,
                    program: Some(PendingProjectionProgram::Retained(program.retained_page()?)),
                    program_ordinal_offset: Some(program_ordinal_offset),
                })
            }
            logical => {
                let contribution = match logical {
                    DecodedLogicalContribution::None => LogicalContribution::None,
                    DecodedLogicalContribution::Identity => LogicalContribution::Identity,
                    DecodedLogicalContribution::Hidden { affinity } => {
                        LogicalContribution::Hidden {
                            affinity: *affinity,
                        }
                    }
                    DecodedLogicalContribution::Atomic(projection) => {
                        LogicalContribution::Atomic(*projection)
                    }
                    DecodedLogicalContribution::Program(_) => unreachable!("matched above"),
                };
                encode_event(
                    &GreenEvent::Coverage(SourceProjectionRun {
                        id: run.id,
                        metric: run.metric,
                        owner_relative_depth: run.owner_relative_depth,
                        part: run.part,
                        logical_contribution: contribution,
                        projection_reset_after: run.projection_reset_after,
                        transient_logical_target: None,
                    }),
                    program_ordinal,
                )
            }
        },
    }
}

fn allocate_event_pages(
    transaction: &mut ArenaBuildTransaction<'_>,
    events: impl IntoIterator<Item = DecodedGreenEventKind>,
    receipt: &mut SerializedGreenBuildReceipt,
) -> Result<Vec<ArenaOwnerHandle>, SerializedGreenError> {
    let mut handles = Vec::new();
    let mut page = LeafEncoder::default();
    for event in events {
        let mut encoded = encode_decoded_event(transaction.arena(), &event, page.programs.len())?;
        if !page.is_empty() && !page.can_fit(&encoded) {
            let handle = allocate_leaf_page(transaction, page, receipt)?;
            handles.push(handle);
            page = LeafEncoder::default();
            encoded = encode_decoded_event(transaction.arena(), &event, 0)?;
        }
        page.push_decoded(&event, encoded)?;
    }
    if !page.is_empty() {
        let handle = allocate_leaf_page(transaction, page, receipt)?;
        handles.push(handle);
    }
    if handles.is_empty() {
        return Err(SerializedGreenError::Invalid(
            "Enter rewrite produced no leaf",
        ));
    }
    Ok(handles)
}

fn encode_manifest(manifest: &Manifest) -> [u8; MANIFEST_BYTES] {
    let mut output = [0_u8; MANIFEST_BYTES];
    output[0] = MANIFEST_TAG;
    output[1] = FORMAT_VERSION;
    output[8..16].copy_from_slice(&manifest.syntax_profile.to_le_bytes());
    output[16..24].copy_from_slice(&manifest.source_revision.0.to_le_bytes());
    output[24..32].copy_from_slice(&manifest.source_root.0.to_le_bytes());
    output[32..40].copy_from_slice(&manifest.source_bytes.to_le_bytes());
    output[40..48].copy_from_slice(&manifest.source_utf16.to_le_bytes());
    output[48..56].copy_from_slice(&manifest.grammar_revision.0.to_le_bytes());
    output[56..64].copy_from_slice(&manifest.parse_generation.0.to_le_bytes());
    output[64..72].copy_from_slice(&manifest.semantic_epoch.to_le_bytes());
    output[72..80].copy_from_slice(&manifest.known_bytes.start.to_le_bytes());
    output[80..88].copy_from_slice(&manifest.known_bytes.end.to_le_bytes());
    output[88..96].copy_from_slice(&manifest.summary.blocks.to_le_bytes());
    output[96..104].copy_from_slice(&manifest.summary.tokens.to_le_bytes());
    output[104..112].copy_from_slice(&manifest.summary.leaves.to_le_bytes());
    output[112..120].copy_from_slice(&manifest.summary.metric.bytes.to_le_bytes());
    output[120..128].copy_from_slice(&manifest.summary.metric.utf16.to_le_bytes());
    output[128..136].copy_from_slice(&manifest.summary.logical_metric.bytes.to_le_bytes());
    output[136..144].copy_from_slice(&manifest.summary.logical_metric.utf16.to_le_bytes());
    output
}

fn decode_manifest(payload: &[u8]) -> Result<Manifest, SerializedGreenError> {
    if payload.len() != MANIFEST_BYTES
        || payload[0] != MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2..8] != [0; 6]
    {
        return Err(SerializedGreenError::Corrupt("invalid green manifest"));
    }
    let manifest = Manifest {
        syntax_profile: read_u64(&payload[8..16]),
        source_revision: SourceRevision(read_u64(&payload[16..24])),
        source_root: SourceRootId(read_u64(&payload[24..32])),
        source_bytes: read_u64(&payload[32..40]),
        source_utf16: read_u64(&payload[40..48]),
        grammar_revision: GrammarRevision(read_u64(&payload[48..56])),
        parse_generation: ParseGeneration(read_u64(&payload[56..64])),
        semantic_epoch: read_u64(&payload[64..72]),
        known_bytes: read_u64(&payload[72..80])..read_u64(&payload[80..88]),
        summary: GreenSummary {
            blocks: read_u64(&payload[88..96]),
            tokens: read_u64(&payload[96..104]),
            leaves: read_u64(&payload[104..112]),
            metric: SerializedMetric {
                bytes: read_u64(&payload[112..120]),
                utf16: read_u64(&payload[120..128]),
            },
            logical_metric: SerializedMetric {
                bytes: read_u64(&payload[128..136]),
                utf16: read_u64(&payload[136..144]),
            },
            ..GreenSummary::default()
        },
    };
    if manifest.syntax_profile == 0
        || manifest.source_root.0 == 0
        || manifest.grammar_revision.0 == 0
        || manifest.parse_generation.0 == 0
        || manifest.semantic_epoch == 0
        || manifest.summary.logical_metric.is_partially_zero()
        || manifest.known_bytes.start > manifest.known_bytes.end
        || manifest.known_bytes.end > manifest.source_bytes
    {
        return Err(SerializedGreenError::Corrupt(
            "invalid green manifest values",
        ));
    }
    Ok(manifest)
}

fn decode_document(
    arena: &PageArena,
    manifest_id: ArenaId,
) -> Result<(Manifest, ArenaId), SerializedGreenError> {
    let mut manifest = decode_manifest(arena.payload(manifest_id)?)?;
    if arena.packed_child_count(manifest_id)? != 1 {
        return Err(SerializedGreenError::Corrupt(
            "green manifest must own exactly one root",
        ));
    }
    let root = arena.children(manifest_id)?[0]
        .ok_or(SerializedGreenError::Corrupt("green manifest has no root"))?;
    let summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if manifest.summary.blocks != summary.blocks
        || manifest.summary.tokens != summary.tokens
        || manifest.summary.leaves != summary.leaves
        || manifest.summary.metric != summary.metric
        || manifest.summary.logical_metric != summary.logical_metric
        || manifest.source_bytes != summary.metric.bytes
        || manifest.source_utf16 != summary.metric.utf16
        || summary.balance != 0
        || summary.minimum_prefix < 0
        || manifest.known_bytes.end > summary.metric.bytes
    {
        return Err(SerializedGreenError::Corrupt(
            "green manifest summary mismatch",
        ));
    }
    manifest.summary = summary;
    Ok((manifest, root))
}

fn merge_sequence_receipt(
    receipt: &mut SerializedGreenBuildReceipt,
    sequence: SequenceMutationReceipt,
) {
    receipt.branch_nodes_allocated += sequence.branches_allocated;
    receipt.payload_bytes_copied += sequence.branch_payload_bytes_copied;
    receipt.edge_bytes_copied += sequence.child_references_added * 8;
    receipt.sequence_nodes_visited += sequence.nodes_visited;
    receipt.sequence_leaves_reused += sequence.leaves_reused;
    receipt.maximum_streaming_roots = receipt
        .maximum_streaming_roots
        .max(sequence.maximum_streaming_roots);
    receipt.maximum_streaming_bin_bytes = receipt
        .maximum_streaming_bin_bytes
        .max(sequence.maximum_streaming_bin_bytes);
    receipt.maximum_sequence_bin_logical_slots = receipt
        .maximum_sequence_bin_logical_slots
        .max(sequence.maximum_resumable_bin_logical_slots);
    receipt.maximum_sequence_bin_requested_bytes = receipt
        .maximum_sequence_bin_requested_bytes
        .max(sequence.maximum_resumable_bin_requested_bytes);
    receipt.maximum_sequence_join_tasks = receipt
        .maximum_sequence_join_tasks
        .max(sequence.maximum_resumable_join_tasks);
    receipt.maximum_sequence_join_task_requested_bytes = receipt
        .maximum_sequence_join_task_requested_bytes
        .max(sequence.maximum_resumable_join_task_requested_bytes);
    receipt.maximum_sequence_join_task_capacity_bytes = receipt
        .maximum_sequence_join_task_capacity_bytes
        .max(sequence.maximum_resumable_join_task_bytes);
    receipt.maximum_sequence_join_values = receipt
        .maximum_sequence_join_values
        .max(sequence.maximum_resumable_join_values);
    receipt.maximum_sequence_join_value_requested_bytes = receipt
        .maximum_sequence_join_value_requested_bytes
        .max(sequence.maximum_resumable_join_value_requested_bytes);
    receipt.maximum_sequence_join_value_capacity_bytes = receipt
        .maximum_sequence_join_value_capacity_bytes
        .max(sequence.maximum_resumable_join_value_bytes);
    receipt.resumable_sequence_splice_polls += sequence.resumable_splice_polls;
    receipt.maximum_sequence_splice_requested_bytes = receipt
        .maximum_sequence_splice_requested_bytes
        .max(sequence.maximum_resumable_splice_total_requested_bytes);
    receipt.maximum_sequence_splice_scratch_bytes = receipt
        .maximum_sequence_splice_scratch_bytes
        .max(sequence.maximum_resumable_splice_total_scratch_bytes);
}

fn sync_transaction_receipt(
    receipt: &mut SerializedGreenBuildReceipt,
    transaction: &ArenaBuildTransaction<'_>,
) {
    receipt.maximum_live_owner_handles = receipt
        .maximum_live_owner_handles
        .max(transaction.maximum_live_owners());
    receipt.owner_journal_capacity = receipt
        .owner_journal_capacity
        .max(transaction.owner_journal_capacity());
    receipt.owner_journal_bytes = receipt
        .owner_journal_bytes
        .max(transaction.owner_journal_bytes());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SerializedGreenRetainedReceipt {
    pub live_nodes: usize,
    pub live_payload_bytes: usize,
    pub live_edge_bytes: usize,
    pub slot_capacity: usize,
    pub slot_storage_bytes: usize,
    pub modeled_allocator_bytes: usize,
    pub root_handle_bytes: usize,
    pub accounted_retained_bytes: usize,
}

#[must_use]
pub fn serialized_green_retained_receipt(
    arena: &PageArena,
    retained_roots: usize,
) -> SerializedGreenRetainedReceipt {
    let metrics = arena.metrics();
    let allocator = metrics.live_nodes * 16;
    let root_handles = retained_roots * std::mem::size_of::<ArenaId>();
    SerializedGreenRetainedReceipt {
        live_nodes: metrics.live_nodes,
        live_payload_bytes: metrics.live_payload_bytes,
        live_edge_bytes: metrics.live_edge_bytes,
        slot_capacity: metrics.slot_capacity,
        slot_storage_bytes: metrics.slot_storage_bytes,
        modeled_allocator_bytes: allocator,
        root_handle_bytes: root_handles,
        accounted_retained_bytes: metrics.live_storage_bytes
            + metrics.slot_storage_bytes
            + allocator
            + root_handles,
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    fn u8(&mut self) -> Result<u8, SerializedGreenError> {
        let value = *self
            .bytes
            .get(self.cursor)
            .ok_or(SerializedGreenError::Corrupt("truncated packed event"))?;
        self.cursor += 1;
        Ok(value)
    }

    fn u64(&mut self) -> Result<u64, SerializedGreenError> {
        let bytes = self.take(8)?;
        Ok(read_u64(bytes))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SerializedGreenError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(SerializedGreenError::Corrupt(
                "packed event length overflow",
            ))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(SerializedGreenError::Corrupt("truncated packed event"))?;
        self.cursor = end;
        Ok(value)
    }

    fn varint(&mut self) -> Result<u64, SerializedGreenError> {
        let start = self.cursor;
        let mut value = 0_u64;
        for shift in (0..=63).step_by(7) {
            let byte = self.u8()?;
            if shift == 63 && byte > 1 {
                return Err(SerializedGreenError::Corrupt("varint overflow"));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                if self.cursor - start != varint_length(value) {
                    return Err(SerializedGreenError::Corrupt("nonminimal varint"));
                }
                return Ok(value);
            }
        }
        Err(SerializedGreenError::Corrupt("varint overflow"))
    }
}

fn varint_length(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        value >>= 7;
        length += 1;
    }
    length
}

fn decode_facts(bytes: &[u8]) -> Result<FactsEnvelope, SerializedGreenError> {
    let mut decoder = Decoder::new(bytes);
    let schema_version = u16::try_from(decoder.varint()?)
        .map_err(|_| SerializedGreenError::Corrupt("facts schema exceeds u16"))?;
    let fields = usize::try_from(decoder.varint()?)
        .map_err(|_| SerializedGreenError::Corrupt("fact count exceeds usize"))?;
    // Every canonical field has at least a descriptor and a length varint.
    // Bound an untrusted count before reserving scratch storage.
    if fields > decoder.remaining() / 2 {
        return Err(SerializedGreenError::Corrupt(
            "fact count exceeds remaining envelope",
        ));
    }
    let mut output = Vec::with_capacity(fields);
    for _ in 0..fields {
        let descriptor = decoder.varint()?;
        let id = u16::try_from(descriptor >> 1)
            .map_err(|_| SerializedGreenError::Corrupt("fact ID exceeds u16"))?;
        let length = usize::try_from(decoder.varint()?)
            .map_err(|_| SerializedGreenError::Corrupt("fact length exceeds usize"))?;
        output.push(FactField {
            id: FactId(id),
            critical: descriptor & 1 != 0,
            value: decoder.take(length)?.to_vec(),
        });
    }
    if !decoder.is_empty() {
        return Err(SerializedGreenError::Corrupt("trailing facts bytes"));
    }
    let envelope = FactsEnvelope {
        schema_version,
        fields: output,
    };
    envelope
        .validate_canonical()
        .map_err(|_| SerializedGreenError::Corrupt("noncanonical facts envelope"))?;
    Ok(envelope)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetainedProgramRef {
    /// Present for arena-backed leaf decoding and deliberately absent while a
    /// copied closure is being validated before admission. Keeping the
    /// storage capability separate from the canonical event grammar lets the
    /// same decoder validate transport bytes without forging an `ArenaId`.
    page: Option<ArenaId>,
    edge_ordinal: u16,
    encoded_ordinal_offset: u16,
    piece_count: u16,
    physical_metric: SerializedMetric,
    logical_metric: SerializedMetric,
}

impl RetainedProgramRef {
    fn retained_page(&self) -> Result<ArenaId, SerializedGreenError> {
        self.page.ok_or(SerializedGreenError::Corrupt(
            "decoded Program is not bound to retained storage",
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DecodedLogicalContribution {
    None,
    Identity,
    Hidden { affinity: GreenAffinity },
    Atomic(AtomicProjection),
    Program(RetainedProgramRef),
}

impl DecodedLogicalContribution {
    fn summary_metric(&self, physical_metric: SerializedMetric) -> SerializedMetric {
        match self {
            Self::None | Self::Hidden { .. } => SerializedMetric::default(),
            Self::Identity => physical_metric,
            Self::Atomic(projection) => projection.logical_metric,
            Self::Program(program) => program.logical_metric,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DecodedSourceProjectionRun {
    id: CoverageId,
    metric: SerializedMetric,
    owner_relative_depth: u32,
    part: CoveragePart,
    logical_contribution: DecodedLogicalContribution,
    projection_reset_after: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DecodedGreenEventKind {
    Enter {
        block: BlockId,
        kind: GreenKind,
        facts: FactsEnvelope,
    },
    Coverage(DecodedSourceProjectionRun),
    Exit {
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenCloseFacts,
    },
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SerializedGreenTestLogical {
    None,
    Identity,
    Hidden(GreenAffinity),
    Atomic(AtomicProjectionKind),
    Program { piece_count: u16 },
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SerializedGreenTestEvent {
    Enter {
        block: BlockId,
        kind: GreenKind,
    },
    Coverage {
        coverage: CoverageId,
        metric: SerializedMetric,
        owner_relative_depth: u32,
        part: CoveragePart,
        logical: SerializedGreenTestLogical,
    },
    Exit,
}

/// Canonical semantic/physical trace used when storage barriers are allowed to
/// split one coalesced coverage envelope. It deliberately excludes
/// history-specific `CoverageIds` and projection packing, while preserving block
/// identity, structure, physical ownership, part, and exact metrics.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SerializedGreenTestCanonicalEvent {
    Enter {
        block: BlockId,
        kind: GreenKind,
    },
    Coverage {
        metric: SerializedMetric,
        owner_relative_depth: u32,
        part: CoveragePart,
    },
    Exit,
}

/// Fully decoded terminal projection piece with every storage capability and
/// `CoverageId` removed. This catches logical/mapping drift even when an
/// otherwise harmless checkpoint barrier changes run packing.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SerializedGreenTestLogicalSegment {
    pub target_block: BlockId,
    pub target_kind: GreenKind,
    pub part: CoveragePart,
    pub physical_owner_block: BlockId,
    pub physical_owner_kind: GreenKind,
    pub consumer_block: BlockId,
    pub consumer_kind: GreenKind,
    pub channel: LogicalChannel,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub logical_byte_range: Range<u64>,
    pub logical_utf16_range: Range<u64>,
    pub mapping: LogicalSegmentMapping,
}

impl GreenSummary {
    fn decoded_event(event: &DecodedGreenEventKind) -> Self {
        match event {
            DecodedGreenEventKind::Enter { .. } => Self {
                tokens: 1,
                blocks: 1,
                balance: 1,
                ..Self::default()
            },
            DecodedGreenEventKind::Coverage(run) => Self {
                tokens: 1,
                metric: run.metric,
                logical_metric: run.logical_contribution.summary_metric(run.metric),
                ..Self::default()
            },
            DecodedGreenEventKind::Exit { closed, .. } => Self {
                tokens: 1,
                balance: -1,
                minimum_prefix: -1,
                minimum_closed_depth: Some(-1),
                outermost: ChildSequenceAggregate::singleton(*closed),
                ..Self::default()
            },
        }
    }
}

#[allow(clippy::too_many_lines)] // Decoder mirrors the centralized event-tag table above.
fn decode_event_with_program_resolver(
    decoder: &mut Decoder<'_>,
    next_program_ordinal: &mut usize,
    mut resolve_program: impl FnMut(usize) -> Result<Option<ArenaId>, SerializedGreenError>,
) -> Result<DecodedGreenEventKind, SerializedGreenError> {
    let event_start = decoder.cursor;
    let tag = decoder.u8()?;
    match tag {
        ENTER_NO_FACTS_TAG | ENTER_WITH_FACTS_TAG => {
            let descriptor = decoder.u8()?;
            if descriptor == 0 || descriptor > 31 {
                return Err(SerializedGreenError::Corrupt("invalid Enter kind"));
            }
            let block = BlockId(decoder.u64()?);
            if block.0 == 0 {
                return Err(SerializedGreenError::Corrupt("zero block identity"));
            }
            let facts = if tag == ENTER_WITH_FACTS_TAG {
                let length = usize::try_from(decoder.varint()?)
                    .map_err(|_| SerializedGreenError::Corrupt("facts length exceeds usize"))?;
                if length == 0 || length > MAX_INLINE_FACT_BYTES {
                    return Err(SerializedGreenError::Corrupt("invalid facts length"));
                }
                decode_facts(decoder.take(length)?)?
            } else {
                FactsEnvelope::empty()
            };
            let kind = GreenKind(descriptor);
            validate_facts_for_kind(kind, &facts)
                .map_err(|_| SerializedGreenError::Corrupt("facts do not match Enter kind"))?;
            Ok(DecodedGreenEventKind::Enter { block, kind, facts })
        }
        value
            if matches!(
                value & 0xf8,
                EXIT_TAG
                    | EXIT_LIST_LOOSE_TAG
                    | EXIT_LIST_TIGHT_TAG
                    | EXIT_FENCED_CODE_TAG
                    | EXIT_LAST_LINE_BLANK_TAG
                    | EXIT_LIST_LOOSE_LAST_LINE_BLANK_TAG
                    | EXIT_LIST_TIGHT_LAST_LINE_BLANK_TAG
                    | EXIT_FENCED_CODE_LAST_LINE_BLANK_TAG
            ) =>
        {
            let descriptor = value & 0xf8;
            let last_line_blank = descriptor >= EXIT_LAST_LINE_BLANK_TAG;
            let facts = match descriptor {
                EXIT_TAG | EXIT_LAST_LINE_BLANK_TAG => GreenCloseFacts::None,
                EXIT_LIST_LOOSE_TAG | EXIT_LIST_LOOSE_LAST_LINE_BLANK_TAG => {
                    GreenCloseFacts::List { tight: false }
                }
                EXIT_LIST_TIGHT_TAG | EXIT_LIST_TIGHT_LAST_LINE_BLANK_TAG => {
                    GreenCloseFacts::List { tight: true }
                }
                EXIT_FENCED_CODE_TAG | EXIT_FENCED_CODE_LAST_LINE_BLANK_TAG => {
                    GreenCloseFacts::FencedCode(GreenFencedCodeCloseFacts::decode_payload(decoder)?)
                }
                _ => return Err(SerializedGreenError::Corrupt("unknown packed event tag")),
            };
            Ok(DecodedGreenEventKind::Exit {
                closed: decode_closed(value & 0x07),
                last_line_blank,
                facts,
            })
        }
        value if value & 0xf0 == COVERAGE_TAG => {
            let part = CoveragePart(value & COVERAGE_PART_MASK);
            if part.0 == 0 {
                return Err(SerializedGreenError::Corrupt("zero coverage part"));
            }
            let id = CoverageId(decoder.u64()?);
            let owner_relative_depth = u32::try_from(decoder.varint()?)
                .map_err(|_| SerializedGreenError::Corrupt("coverage owner depth exceeds u32"))?;
            let bytes = decoder.varint()?;
            let utf16 = if value & COVERAGE_SAME_METRIC != 0 {
                bytes
            } else {
                decoder.varint()?
            };
            let metric = SerializedMetric { bytes, utf16 };
            if id.0 == 0 || metric.is_zero() || metric.is_partially_zero() {
                return Err(SerializedGreenError::Corrupt("invalid coverage run"));
            }
            let logical_descriptor = decoder.u8()?;
            if logical_descriptor & LOGICAL_RESERVED_MASK != 0 {
                return Err(SerializedGreenError::Corrupt(
                    "logical descriptor has reserved bits",
                ));
            }
            let logical_kind = logical_descriptor & LOGICAL_KIND_MASK;
            let projection_reset_after = logical_descriptor & LOGICAL_PROJECTION_RESET_AFTER != 0;
            let logical_contribution = match logical_kind {
                LOGICAL_NONE_TAG => DecodedLogicalContribution::None,
                LOGICAL_IDENTITY_TAG => DecodedLogicalContribution::Identity,
                LOGICAL_HIDDEN_UPSTREAM_TAG | LOGICAL_HIDDEN_DOWNSTREAM_TAG => {
                    DecodedLogicalContribution::Hidden {
                        affinity: if logical_kind == LOGICAL_HIDDEN_UPSTREAM_TAG {
                            GreenAffinity::Upstream
                        } else {
                            GreenAffinity::Downstream
                        },
                    }
                }
                LOGICAL_ATOMIC_TAG => {
                    let descriptor = decoder.u8()?;
                    let kind = match descriptor & !EVENT_METRIC_SAME {
                        ATOMIC_EVENT_TAB_TAG => AtomicProjectionKind::TabToSpaces {
                            spaces: decoder.u8()?,
                        },
                        ATOMIC_EVENT_CRLF_TAG => AtomicProjectionKind::CrLfToLf,
                        ATOMIC_EVENT_LONE_CR_TAG => AtomicProjectionKind::LoneCrToLf,
                        ATOMIC_EVENT_NUL_TAG => AtomicProjectionKind::NulToReplacement,
                        _ => {
                            return Err(SerializedGreenError::Corrupt(
                                "unknown atomic projection type",
                            ));
                        }
                    };
                    let logical_metric =
                        decode_metric(decoder, descriptor & EVENT_METRIC_SAME != 0)?;
                    let projection = AtomicProjection::new(kind, logical_metric)
                        .and_then(|projection| {
                            projection.validate_physical(metric)?;
                            Ok(projection)
                        })
                        .map_err(|_| SerializedGreenError::Corrupt("invalid atomic projection"))?;
                    DecodedLogicalContribution::Atomic(projection)
                }
                LOGICAL_PROGRAM_TAG => {
                    let metric_descriptor = decoder.u8()?;
                    if metric_descriptor & !EVENT_METRIC_SAME != 0 {
                        return Err(SerializedGreenError::Corrupt(
                            "invalid Program logical metric descriptor",
                        ));
                    }
                    let logical_metric =
                        decode_metric(decoder, metric_descriptor & EVENT_METRIC_SAME != 0)?;
                    let encoded_ordinal_offset = u16::try_from(decoder.cursor - event_start)
                        .map_err(|_| {
                            SerializedGreenError::Corrupt("program ordinal offset exceeds u16")
                        })?;
                    let edge_ordinal = usize::try_from(decoder.varint()?).map_err(|_| {
                        SerializedGreenError::Corrupt("program edge ordinal exceeds usize")
                    })?;
                    let piece_count = usize::try_from(decoder.varint()?).map_err(|_| {
                        SerializedGreenError::Corrupt("program piece count exceeds usize")
                    })?;
                    if logical_metric.is_partially_zero() || piece_count == 0 {
                        return Err(SerializedGreenError::Corrupt(
                            "invalid Program event summary",
                        ));
                    }
                    if edge_ordinal != *next_program_ordinal {
                        return Err(SerializedGreenError::Corrupt(
                            "program edge ordinals are not canonical",
                        ));
                    }
                    let page = resolve_program(edge_ordinal)?;
                    *next_program_ordinal = next_program_ordinal
                        .checked_add(1)
                        .ok_or(SerializedGreenError::Overflow("program edge ordinal"))?;
                    DecodedLogicalContribution::Program(RetainedProgramRef {
                        page,
                        edge_ordinal: u16::try_from(edge_ordinal).map_err(|_| {
                            SerializedGreenError::Corrupt("program edge ordinal exceeds u16")
                        })?,
                        encoded_ordinal_offset,
                        piece_count: u16::try_from(piece_count).map_err(|_| {
                            SerializedGreenError::Corrupt("program piece count exceeds u16")
                        })?,
                        physical_metric: metric,
                        logical_metric,
                    })
                }
                _ => {
                    return Err(SerializedGreenError::Corrupt(
                        "unknown logical contribution type",
                    ));
                }
            };
            Ok(DecodedGreenEventKind::Coverage(
                DecodedSourceProjectionRun {
                    id,
                    metric,
                    owner_relative_depth,
                    part,
                    logical_contribution,
                    projection_reset_after,
                },
            ))
        }
        _ => Err(SerializedGreenError::Corrupt("unknown packed event tag")),
    }
}

fn decode_event(
    decoder: &mut Decoder<'_>,
    arena: &PageArena,
    leaf: ArenaId,
    next_program_ordinal: &mut usize,
) -> Result<DecodedGreenEventKind, SerializedGreenError> {
    decode_event_with_program_resolver(decoder, next_program_ordinal, |edge_ordinal| {
        arena
            .packed_child_at(leaf, edge_ordinal)
            .map(Some)
            .map_err(SerializedGreenError::from)
    })
}

#[derive(Clone, Debug)]
struct DecodedLeafEvent {
    byte_offset: u16,
    event: DecodedGreenEventKind,
}

/// Runs the one canonical event decoder over a leaf while retaining only the
/// current decoded event. Callers that need an owned event page can collect in
/// the visitor; scalar storage queries can inspect and immediately discard.
fn visit_decoded_leaf_events(
    arena: &PageArena,
    leaf: ArenaId,
    mut visitor: impl FnMut(u16, DecodedGreenEventKind) -> Result<(), SerializedGreenError>,
) -> Result<GreenSummary, SerializedGreenError> {
    let child_count = arena.packed_child_count(leaf)?;
    let payload = arena.payload(leaf)?;
    let expected = decode_summary(payload, LEAF_TAG)?;
    let mut decoder = Decoder::new(&payload[LEAF_HEADER_BYTES..]);
    let mut actual = GreenSummary::default();
    let mut next_program_ordinal = 0;
    while !decoder.is_empty() {
        let offset = u16::try_from(LEAF_HEADER_BYTES + decoder.cursor)
            .map_err(|_| SerializedGreenError::Corrupt("leaf offset exceeds u16"))?;
        let event = decode_event(&mut decoder, arena, leaf, &mut next_program_ordinal)?;
        actual = actual.followed_by(GreenSummary::decoded_event(&event))?;
        visitor(offset, event)?;
    }
    actual.leaves = 1;
    actual.height = 1;
    if next_program_ordinal != child_count {
        return Err(SerializedGreenError::Corrupt(
            "green leaf has an unreferenced projection edge",
        ));
    }
    if actual != expected {
        return Err(SerializedGreenError::Corrupt("green leaf summary mismatch"));
    }
    Ok(expected)
}

fn decode_leaf(
    arena: &PageArena,
    leaf: ArenaId,
) -> Result<(GreenSummary, Vec<DecodedLeafEvent>), SerializedGreenError> {
    let mut events = Vec::new();
    let expected = visit_decoded_leaf_events(arena, leaf, |offset, event| {
        events.push(DecodedLeafEvent {
            byte_offset: offset,
            event,
        });
        Ok(())
    })?;
    Ok((expected, events))
}

/// Composable structural header copied out of one canonically decoded green
/// leaf. It is intentionally smaller than `GreenSummary`: these are the exact
/// fields the host page tree needs to skip balanced subtrees while recovering
/// viewport open context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopiedGreenLeafSummary {
    pub(crate) metric: SerializedMetric,
    pub(crate) balance: i64,
    pub(crate) minimum_prefix: i64,
}

/// Host-owned structural event retained only while a contributing copied leaf
/// is decoded. Coverage and Program details stay in the leaf closure itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CopiedGreenStructuralEvent {
    Enter {
        block: BlockId,
        kind: GreenKind,
        facts: FactsEnvelope,
    },
    Exit {
        facts: GreenCloseFacts,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CopiedGreenLeafDecoded {
    pub(crate) summary: CopiedGreenLeafSummary,
    pub(crate) structural_events: Vec<CopiedGreenStructuralEvent>,
}

/// Maximum credited transport closure admitted for one immutable green leaf.
/// The leaf and each Program remain arena-page bounded; this larger envelope
/// only accounts for the ordered set of Program payloads owned by the leaf.
pub(crate) const COPIED_GREEN_CLOSURE_MAX_BYTES: usize = 256 * 1024;

const MAX_CANONICAL_EVENT_INSPECT_BYTES: usize = ARENA_PAGE_BYTES;
const MAX_PROGRAM_HEADER_INSPECT_BYTES: usize = 64;
const MAX_PROGRAM_PIECE_INSPECT_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopiedGreenValidationFuel {
    pub(crate) inspect_bytes: usize,
    pub(crate) copy_bytes: usize,
    pub(crate) transitions: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CopiedGreenValidationReceipt {
    pub(crate) inspected_bytes: usize,
    pub(crate) copied_bytes: usize,
    pub(crate) transitions: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopiedGreenValidationProgress {
    Pending,
    Complete,
}

#[derive(Clone, Debug)]
struct CopiedProgramValidation {
    program: RetainedProgramRef,
    cursor: usize,
    pieces_remaining: usize,
    physical: SerializedMetric,
    logical: SerializedMetric,
    header_decoded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopiedGreenValidationPhase {
    Header,
    Events,
    Program,
    Complete,
}

/// Continuation for the one canonical serialized-green leaf and Program
/// decoder. It keeps no borrowed transport state, so a caller can pause after
/// any header, event, or Program-piece transition while retaining exactly one
/// credited closure buffer.
#[derive(Clone, Debug)]
pub(crate) struct CopiedGreenClosureValidator {
    leaf_payload_bytes: usize,
    program_count: usize,
    collect_structural_events: bool,
    expected: Option<GreenSummary>,
    actual: GreenSummary,
    event_cursor: usize,
    next_program_ordinal: usize,
    structural_events: Vec<CopiedGreenStructuralEvent>,
    program: Option<CopiedProgramValidation>,
    decoded: Option<CopiedGreenLeafDecoded>,
    phase: CopiedGreenValidationPhase,
}

impl CopiedGreenClosureValidator {
    /// Staged-publication mode: validate structure and folds without retaining
    /// an event vector. Facts decoding remains canonical and is charged to the
    /// copy-fuel axis, but every decoded event is discarded immediately.
    pub(crate) fn try_new_summary_only(
        leaf_payload_bytes: usize,
        program_count: usize,
        closure_bytes: usize,
    ) -> Result<Self, SerializedGreenError> {
        Self::try_new(leaf_payload_bytes, program_count, closure_bytes, false)
    }

    fn try_new_collecting(
        leaf_payload_bytes: usize,
        program_count: usize,
        closure_bytes: usize,
    ) -> Result<Self, SerializedGreenError> {
        Self::try_new(leaf_payload_bytes, program_count, closure_bytes, true)
    }

    fn try_new(
        leaf_payload_bytes: usize,
        program_count: usize,
        closure_bytes: usize,
        collect_structural_events: bool,
    ) -> Result<Self, SerializedGreenError> {
        if leaf_payload_bytes > ARENA_PAGE_BYTES
            || program_count > MAX_PACKED_ARENA_CHILDREN
            || closure_bytes > COPIED_GREEN_CLOSURE_MAX_BYTES
            || closure_bytes < leaf_payload_bytes
        {
            return Err(SerializedGreenError::Corrupt(
                "copied green closure exceeds its admitted bounds",
            ));
        }
        let packed_bytes = leaf_payload_bytes
            .checked_add(
                program_count
                    .checked_mul(std::mem::size_of::<ArenaId>())
                    .ok_or(SerializedGreenError::Overflow(
                        "copied green Program edge bytes",
                    ))?,
            )
            .ok_or(SerializedGreenError::Overflow(
                "copied green packed leaf bytes",
            ))?;
        if packed_bytes > ARENA_PAGE_BYTES {
            return Err(SerializedGreenError::Corrupt(
                "copied green packed leaf exceeds one arena page",
            ));
        }
        Ok(Self {
            leaf_payload_bytes,
            program_count,
            collect_structural_events,
            expected: None,
            actual: GreenSummary::default(),
            event_cursor: 0,
            next_program_ordinal: 0,
            structural_events: Vec::new(),
            program: None,
            decoded: None,
            phase: CopiedGreenValidationPhase::Header,
        })
    }

    pub(crate) fn take_decoded(&mut self) -> Result<CopiedGreenLeafDecoded, SerializedGreenError> {
        if self.phase != CopiedGreenValidationPhase::Complete {
            return Err(SerializedGreenError::Invalid(
                "copied green validation is not complete",
            ));
        }
        self.decoded.take().ok_or(SerializedGreenError::Invalid(
            "copied green validation result already taken",
        ))
    }

    #[allow(clippy::too_many_lines)] // Explicit phases make every untrusted decode step fuelled.
    pub(crate) fn poll<'a>(
        &mut self,
        leaf_payload: &'a [u8],
        mut program_payload: impl FnMut(usize) -> Result<&'a [u8], SerializedGreenError>,
        mut fuel: CopiedGreenValidationFuel,
    ) -> Result<(CopiedGreenValidationProgress, CopiedGreenValidationReceipt), SerializedGreenError>
    {
        if leaf_payload.len() != self.leaf_payload_bytes {
            return Err(SerializedGreenError::Corrupt(
                "copied green leaf payload changed during validation",
            ));
        }
        let mut receipt = CopiedGreenValidationReceipt::default();
        loop {
            match self.phase {
                CopiedGreenValidationPhase::Header => {
                    if fuel.transitions == 0 || fuel.inspect_bytes < LEAF_HEADER_BYTES {
                        return Ok((CopiedGreenValidationProgress::Pending, receipt));
                    }
                    let expected = decode_summary(leaf_payload, LEAF_TAG)?;
                    self.expected = Some(expected);
                    self.event_cursor = LEAF_HEADER_BYTES;
                    self.phase = CopiedGreenValidationPhase::Events;
                    fuel.transitions -= 1;
                    fuel.inspect_bytes -= LEAF_HEADER_BYTES;
                    receipt.transitions += 1;
                    receipt.inspected_bytes += LEAF_HEADER_BYTES;
                }
                CopiedGreenValidationPhase::Events => {
                    if self.event_cursor == leaf_payload.len() {
                        if fuel.transitions == 0 {
                            return Ok((CopiedGreenValidationProgress::Pending, receipt));
                        }
                        if self.next_program_ordinal != self.program_count {
                            return Err(SerializedGreenError::Corrupt(
                                "green leaf has an unreferenced projection edge",
                            ));
                        }
                        self.actual.leaves = 1;
                        self.actual.height = 1;
                        let expected = self.expected.ok_or(SerializedGreenError::Corrupt(
                            "copied green validation lost its leaf summary",
                        ))?;
                        if self.actual != expected {
                            return Err(SerializedGreenError::Corrupt(
                                "green leaf summary mismatch",
                            ));
                        }
                        self.decoded = Some(CopiedGreenLeafDecoded {
                            summary: CopiedGreenLeafSummary {
                                metric: expected.metric,
                                balance: expected.balance,
                                minimum_prefix: expected.minimum_prefix,
                            },
                            structural_events: std::mem::take(&mut self.structural_events),
                        });
                        self.phase = CopiedGreenValidationPhase::Complete;
                        receipt.transitions += 1;
                        return Ok((CopiedGreenValidationProgress::Complete, receipt));
                    }
                    let remaining = leaf_payload.len().checked_sub(self.event_cursor).ok_or(
                        SerializedGreenError::Corrupt("copied green event cursor escaped leaf"),
                    )?;
                    let inspect_reservation = remaining.min(MAX_CANONICAL_EVENT_INSPECT_BYTES);
                    let copies_facts = leaf_payload[self.event_cursor] == ENTER_WITH_FACTS_TAG;
                    let copy_reservation = if copies_facts {
                        MAX_INLINE_FACT_BYTES
                    } else {
                        0
                    };
                    if fuel.transitions == 0
                        || fuel.inspect_bytes < inspect_reservation
                        || fuel.copy_bytes < copy_reservation
                    {
                        return Ok((CopiedGreenValidationProgress::Pending, receipt));
                    }
                    fuel.transitions -= 1;
                    fuel.inspect_bytes -= inspect_reservation;
                    fuel.copy_bytes -= copy_reservation;
                    let start = self.event_cursor;
                    let mut decoder = Decoder {
                        bytes: leaf_payload,
                        cursor: start,
                    };
                    let program_count = self.program_count;
                    let event = decode_event_with_program_resolver(
                        &mut decoder,
                        &mut self.next_program_ordinal,
                        |edge_ordinal| {
                            if edge_ordinal >= program_count {
                                return Err(SerializedGreenError::Corrupt(
                                    "program edge ordinal exceeds copied children",
                                ));
                            }
                            Ok(None)
                        },
                    )?;
                    let inspected =
                        decoder
                            .cursor
                            .checked_sub(start)
                            .ok_or(SerializedGreenError::Corrupt(
                                "copied green event cursor reversed",
                            ))?;
                    if inspected > inspect_reservation {
                        return Err(SerializedGreenError::Corrupt(
                            "canonical event exceeded its inspection bound",
                        ));
                    }
                    fuel.inspect_bytes += inspect_reservation - inspected;
                    receipt.inspected_bytes += inspected;
                    receipt.copied_bytes += copy_reservation;
                    receipt.transitions += 1;
                    self.event_cursor = decoder.cursor;
                    self.actual = self
                        .actual
                        .followed_by(GreenSummary::decoded_event(&event))?;
                    match event {
                        DecodedGreenEventKind::Enter { block, kind, facts }
                            if self.collect_structural_events =>
                        {
                            self.structural_events.try_reserve(1).map_err(|_| {
                                SerializedGreenError::Invalid(
                                    "copied structural event reservation failed",
                                )
                            })?;
                            self.structural_events
                                .push(CopiedGreenStructuralEvent::Enter { block, kind, facts });
                        }
                        DecodedGreenEventKind::Coverage(DecodedSourceProjectionRun {
                            logical_contribution: DecodedLogicalContribution::Program(program),
                            ..
                        }) => {
                            self.program = Some(CopiedProgramValidation {
                                program,
                                cursor: 0,
                                pieces_remaining: 0,
                                physical: SerializedMetric::default(),
                                logical: SerializedMetric::default(),
                                header_decoded: false,
                            });
                            self.phase = CopiedGreenValidationPhase::Program;
                        }
                        DecodedGreenEventKind::Coverage(_) => {}
                        DecodedGreenEventKind::Enter { .. } => {}
                        DecodedGreenEventKind::Exit { facts, .. }
                            if self.collect_structural_events =>
                        {
                            self.structural_events.try_reserve(1).map_err(|_| {
                                SerializedGreenError::Invalid(
                                    "copied structural event reservation failed",
                                )
                            })?;
                            self.structural_events
                                .push(CopiedGreenStructuralEvent::Exit { facts });
                        }
                        DecodedGreenEventKind::Exit { .. } => {}
                    }
                }
                CopiedGreenValidationPhase::Program => {
                    let pending = self.program.as_mut().ok_or(SerializedGreenError::Corrupt(
                        "copied green Program phase lost its continuation",
                    ))?;
                    let ordinal = usize::from(pending.program.edge_ordinal);
                    let payload = program_payload(ordinal)?;
                    if payload.len() > PROJECTION_PROGRAM_PAGE_BYTES {
                        return Err(SerializedGreenError::Corrupt(
                            "copied projection Program exceeds one arena page",
                        ));
                    }
                    if !pending.header_decoded {
                        let inspect_reservation =
                            payload.len().min(MAX_PROGRAM_HEADER_INSPECT_BYTES);
                        if fuel.transitions == 0 || fuel.inspect_bytes < inspect_reservation {
                            return Ok((CopiedGreenValidationProgress::Pending, receipt));
                        }
                        fuel.transitions -= 1;
                        fuel.inspect_bytes -= inspect_reservation;
                        let mut decoder = Decoder::new(payload);
                        let header = decode_projection_program_header(&mut decoder)?;
                        if header.piece_count != usize::from(pending.program.piece_count)
                            || header.physical_metric != pending.program.physical_metric
                            || header.logical_metric != pending.program.logical_metric
                        {
                            return Err(SerializedGreenError::Corrupt(
                                "projection edge count or partition mismatch",
                            ));
                        }
                        if decoder.cursor > inspect_reservation {
                            return Err(SerializedGreenError::Corrupt(
                                "projection Program header exceeded its inspection bound",
                            ));
                        }
                        fuel.inspect_bytes += inspect_reservation - decoder.cursor;
                        receipt.inspected_bytes += decoder.cursor;
                        receipt.transitions += 1;
                        pending.cursor = decoder.cursor;
                        pending.pieces_remaining = header.piece_count;
                        pending.header_decoded = true;
                        continue;
                    }
                    if pending.pieces_remaining == 0 {
                        if pending.cursor != payload.len() {
                            return Err(SerializedGreenError::Corrupt(
                                "trailing projection program bytes",
                            ));
                        }
                        if pending.physical != pending.program.physical_metric
                            || pending.logical != pending.program.logical_metric
                        {
                            return Err(SerializedGreenError::Corrupt(
                                "projection program summary mismatch",
                            ));
                        }
                        self.program = None;
                        self.phase = CopiedGreenValidationPhase::Events;
                        continue;
                    }
                    let remaining = payload.len().checked_sub(pending.cursor).ok_or(
                        SerializedGreenError::Corrupt("projection Program cursor escaped its page"),
                    )?;
                    let inspect_reservation = remaining.min(MAX_PROGRAM_PIECE_INSPECT_BYTES);
                    if fuel.transitions == 0 || fuel.inspect_bytes < inspect_reservation {
                        return Ok((CopiedGreenValidationProgress::Pending, receipt));
                    }
                    fuel.transitions -= 1;
                    fuel.inspect_bytes -= inspect_reservation;
                    let start = pending.cursor;
                    let mut decoder = Decoder {
                        bytes: payload,
                        cursor: start,
                    };
                    let piece = decode_projection_piece(&mut decoder)?;
                    let inspected =
                        decoder
                            .cursor
                            .checked_sub(start)
                            .ok_or(SerializedGreenError::Corrupt(
                                "projection Program piece cursor reversed",
                            ))?;
                    if inspected > inspect_reservation {
                        return Err(SerializedGreenError::Corrupt(
                            "projection Program piece exceeded its inspection bound",
                        ));
                    }
                    fuel.inspect_bytes += inspect_reservation - inspected;
                    receipt.inspected_bytes += inspected;
                    receipt.transitions += 1;
                    pending.cursor = decoder.cursor;
                    pending.pieces_remaining -= 1;
                    let (physical, logical) = piece.metrics();
                    pending.physical = pending.physical.checked_add(physical).map_err(|_| {
                        SerializedGreenError::Corrupt("projection program physical metric overflow")
                    })?;
                    pending.logical = pending.logical.checked_add(logical).map_err(|_| {
                        SerializedGreenError::Corrupt("projection program logical metric overflow")
                    })?;
                    if pending.physical.bytes > pending.program.physical_metric.bytes
                        || pending.physical.utf16 > pending.program.physical_metric.utf16
                        || pending.logical.bytes > pending.program.logical_metric.bytes
                        || pending.logical.utf16 > pending.program.logical_metric.utf16
                    {
                        return Err(SerializedGreenError::Corrupt(
                            "projection program prefix exceeds its declared partition",
                        ));
                    }
                }
                CopiedGreenValidationPhase::Complete => {
                    return Ok((CopiedGreenValidationProgress::Complete, receipt));
                }
            }
        }
    }
}

/// Compatibility driver for callers that already own a complete copied
/// closure. It uses the same resumable continuation as staged publication and
/// no longer allocates a scratch arena or materializes Program pieces.
///
/// This is deliberately narrower than a document decoder. A host may retain
/// independently copied leaves and Program pages after the worker retires,
/// while branches and manifests remain worker-local publication machinery.
pub(crate) fn validate_copied_green_leaf_closure(
    leaf_payload: &[u8],
    program_payloads: &[&[u8]],
) -> Result<CopiedGreenLeafDecoded, SerializedGreenError> {
    let closure_bytes =
        program_payloads
            .iter()
            .try_fold(leaf_payload.len(), |total, payload| {
                total
                    .checked_add(payload.len())
                    .ok_or(SerializedGreenError::Overflow("copied green closure bytes"))
            })?;
    let mut validator = CopiedGreenClosureValidator::try_new_collecting(
        leaf_payload.len(),
        program_payloads.len(),
        closure_bytes,
    )?;
    loop {
        let (progress, _) = validator.poll(
            leaf_payload,
            |ordinal| {
                program_payloads
                    .get(ordinal)
                    .copied()
                    .ok_or(SerializedGreenError::Corrupt(
                        "copied Program ordinal is out of range",
                    ))
            },
            CopiedGreenValidationFuel {
                inspect_bytes: usize::MAX,
                copy_bytes: usize::MAX,
                transitions: usize::MAX,
            },
        )?;
        if progress == CopiedGreenValidationProgress::Complete {
            return validator.take_decoded();
        }
    }
}

/// Near-maximum, structurally balanced canonical closure used by the 100 MiB
/// staged-publication receipt. Every padded Enter is closed in the same leaf,
/// so repeating the fixture remains a complete green document.
#[cfg(all(test, feature = "host-publication-staging-probe"))]
pub(crate) fn serialized_green_staging_test_closure(
    metric: SerializedMetric,
) -> (Vec<u8>, Vec<Vec<u8>>) {
    let program = ProjectionProgram::new(vec![ProjectionPiece::Identity { metric }])
        .expect("test projection is valid");
    let coverage = GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(1),
            metric.bytes,
            metric.utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(1),
            LogicalContribution::Program(program),
        )
        .expect("test coverage is valid"),
    );
    let mut leaf = LeafEncoder::default();
    let encoded = encode_event(&coverage, 0).expect("test coverage encodes");
    leaf.push(&coverage, encoded)
        .expect("test coverage fits an empty leaf");
    let padded_enter = GreenEvent::enter(
        BlockId(2),
        GreenKind::DOCUMENT,
        FactsEnvelope::new(vec![FactField::optional(FactId(u16::MAX), vec![0; 240])])
            .expect("test padding facts are canonical"),
    );
    let exit = GreenEvent::exit(ClosedChildAggregate::default());
    loop {
        let encoded_enter =
            encode_event(&padded_enter, leaf.programs.len()).expect("test Enter encodes");
        let encoded_exit = encode_event(&exit, leaf.programs.len()).expect("test Exit encodes");
        let packed_bytes = leaf.bytes.len()
            + encoded_enter.bytes.len()
            + encoded_exit.bytes.len()
            + leaf.programs.len() * std::mem::size_of::<ArenaId>();
        if packed_bytes > ARENA_PAGE_BYTES {
            break;
        }
        leaf.push(&padded_enter, encoded_enter)
            .expect("preflighted test Enter fits");
        leaf.push(&exit, encoded_exit)
            .expect("preflighted test Exit fits");
    }
    let (payload, summary, programs) = leaf.seal().expect("balanced test leaf seals");
    assert_eq!(summary.metric, metric);
    assert_eq!(summary.balance, 0);
    assert_eq!(summary.minimum_prefix, 0);
    assert!(payload.len() > 3_800);
    let programs = programs
        .into_iter()
        .map(|program| match program {
            PendingProjectionProgram::New(payload) => payload,
            PendingProjectionProgram::Retained(_) => panic!("test Program must be copied"),
        })
        .collect();
    (payload, programs)
}

/// Exact packed-page edge case. Unlike the repeated document fixture above,
/// this closure is intentionally only leaf-canonical: trailing one-byte Exit
/// events fill the page and exercise summary-only admission at the hard cap.
#[cfg(all(test, feature = "host-publication-staging-probe"))]
pub(crate) fn serialized_green_staging_test_max_page_closure(
    metric: SerializedMetric,
) -> (Vec<u8>, Vec<Vec<u8>>) {
    let program = ProjectionProgram::new(vec![ProjectionPiece::Identity { metric }])
        .expect("test projection is valid");
    let coverage = GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(1),
            metric.bytes,
            metric.utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(1),
            LogicalContribution::Program(program),
        )
        .expect("test coverage is valid"),
    );
    let mut leaf = LeafEncoder::default();
    let encoded = encode_event(&coverage, 0).expect("test coverage encodes");
    leaf.push(&coverage, encoded)
        .expect("test coverage fits an empty leaf");
    let exit = GreenEvent::exit(ClosedChildAggregate::default());
    loop {
        let encoded = encode_event(&exit, leaf.programs.len()).expect("test Exit encodes");
        if !leaf.can_fit(&encoded) {
            break;
        }
        leaf.push(&exit, encoded)
            .expect("preflighted test Exit fits");
    }
    let (payload, summary, programs) = leaf.seal().expect("dense test leaf seals");
    assert_eq!(summary.metric, metric);
    assert_eq!(
        payload.len() + programs.len() * std::mem::size_of::<ArenaId>(),
        ARENA_PAGE_BYTES
    );
    let programs = programs
        .into_iter()
        .map(|program| match program {
            PendingProjectionProgram::New(payload) => payload,
            PendingProjectionProgram::Retained(_) => panic!("test Program must be copied"),
        })
        .collect();
    (payload, programs)
}

#[cfg(all(test, feature = "host-publication-staging-probe"))]
pub(crate) fn serialized_green_staging_test_structural_closure(
    metric: SerializedMetric,
    enter: bool,
) -> Vec<u8> {
    let coverage = GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(1),
            metric.bytes,
            metric.utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(1),
            LogicalContribution::Identity,
        )
        .expect("test coverage is valid"),
    );
    let structural = if enter {
        GreenEvent::enter(BlockId(2), GreenKind::DOCUMENT, FactsEnvelope::empty())
    } else {
        GreenEvent::exit(ClosedChildAggregate::default())
    };
    let events = if enter {
        [&coverage, &structural]
    } else {
        [&structural, &coverage]
    };
    let mut leaf = LeafEncoder::default();
    for event in events {
        let encoded = encode_event(event, 0).expect("test structural event encodes");
        leaf.push(event, encoded)
            .expect("test structural event fits");
    }
    let (payload, summary, programs) = leaf.seal().expect("structural test leaf seals");
    assert!(programs.is_empty());
    assert_eq!(summary.balance, if enter { 1 } else { -1 });
    payload
}

#[cfg(all(test, feature = "host-publication-staging-probe"))]
pub(crate) fn serialized_green_staging_test_zero_program_closure(
    metric: SerializedMetric,
) -> Vec<u8> {
    let coverage = GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(1),
            metric.bytes,
            metric.utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(1),
            LogicalContribution::Identity,
        )
        .expect("test coverage is valid"),
    );
    let mut leaf = LeafEncoder::default();
    let encoded = encode_event(&coverage, 0).expect("test coverage encodes");
    leaf.push(&coverage, encoded)
        .expect("test coverage fits an empty leaf");
    let padded_enter = GreenEvent::enter(
        BlockId(2),
        GreenKind::DOCUMENT,
        FactsEnvelope::new(vec![FactField::optional(FactId(u16::MAX), vec![0; 240])])
            .expect("test padding facts are canonical"),
    );
    loop {
        let encoded = encode_event(&padded_enter, 0).expect("test Enter encodes");
        if !leaf.can_fit(&encoded) {
            break;
        }
        leaf.push(&padded_enter, encoded)
            .expect("preflighted test Enter fits");
    }
    let exit = GreenEvent::exit(ClosedChildAggregate::default());
    loop {
        let encoded = encode_event(&exit, 0).expect("test Exit encodes");
        if !leaf.can_fit(&encoded) {
            break;
        }
        leaf.push(&exit, encoded)
            .expect("preflighted test Exit fits");
    }
    let (payload, summary, programs) = leaf.seal().expect("dense test leaf seals");
    assert_eq!(summary.metric, metric);
    assert!(programs.is_empty());
    assert_eq!(payload.len(), ARENA_PAGE_BYTES);
    payload
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // One bounded decode/repack pass keeps raw event and Program-edge handling adjacent.
fn prepare_setext_leaf_repack(
    arena: &PageArena,
    leaf: ArenaId,
    target_event_ordinal: u64,
    target_byte_offset: u16,
    target_block: BlockId,
    encoded_heading: &[u8],
    scratch: &mut SetextRepackScratch,
) -> Result<
    (
        GreenSummary,
        GreenSummary,
        usize,
        SetextRepackTargetLocation,
    ),
    SerializedGreenError,
> {
    scratch.reset()?;
    let child_count = arena.packed_child_count(leaf)?;
    let payload = arena.payload(leaf)?;
    let expected = decode_summary(payload, LEAF_TAG)?;
    let mut decoder = Decoder::new(&payload[LEAF_HEADER_BYTES..]);
    let mut actual = GreenSummary::default();
    let mut next_program_ordinal = 0_usize;
    let mut event_ordinal = 0_u64;
    let mut output_page = 0_usize;
    let mut found_target = false;
    let mut replacement_target = None;

    while !decoder.is_empty() {
        let start =
            LEAF_HEADER_BYTES
                .checked_add(decoder.cursor)
                .ok_or(SerializedGreenError::Overflow(
                    "Setext decoded event offset",
                ))?;
        let event = decode_event(&mut decoder, arena, leaf, &mut next_program_ordinal)?;
        let end = LEAF_HEADER_BYTES
            .checked_add(decoder.cursor)
            .ok_or(SerializedGreenError::Overflow("Setext decoded event end"))?;
        let raw = payload
            .get(start..end)
            .ok_or(SerializedGreenError::Corrupt(
                "Setext decoded event escapes leaf",
            ))?;
        let offset = u16::try_from(start)
            .map_err(|_| SerializedGreenError::Corrupt("Setext leaf offset exceeds u16"))?;
        let at_target = event_ordinal == target_event_ordinal;
        let (output, program) = if at_target {
            if offset != target_byte_offset || found_target {
                return Err(SerializedGreenError::StaleCursor);
            }
            let DecodedGreenEventKind::Enter { block, kind, facts } = &event else {
                return Err(SerializedGreenError::StaleCursor);
            };
            if *block != target_block
                || *kind != GreenKind::PARAGRAPH
                || !facts.fields.is_empty()
                || encoded_heading.len() != raw.len() + 7
            {
                return Err(SerializedGreenError::StaleCursor);
            }
            found_target = true;
            (encoded_heading, None)
        } else {
            let program = match &event {
                DecodedGreenEventKind::Coverage(DecodedSourceProjectionRun {
                    logical_contribution: DecodedLogicalContribution::Program(program),
                    ..
                }) => Some((
                    program.retained_page()?,
                    usize::from(program.encoded_ordinal_offset),
                )),
                DecodedGreenEventKind::Enter { .. }
                | DecodedGreenEventKind::Coverage(_)
                | DecodedGreenEventKind::Exit { .. } => None,
            };
            (raw, program)
        };
        let event_summary = GreenSummary::decoded_event(&event);
        actual = actual.followed_by(event_summary)?;
        if !scratch.pages[output_page].can_fit(output.len(), program.is_some()) {
            if scratch.pages[output_page].is_empty() {
                return Err(SerializedGreenError::Invalid(
                    "Setext replacement event exceeds an empty leaf",
                ));
            }
            scratch.pages[output_page].seal()?;
            output_page = output_page
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "Setext replacement page index",
                ))?;
            if output_page >= scratch.pages.len() {
                return Err(SerializedGreenError::Corrupt(
                    "one Setext leaf expanded beyond two replacement leaves",
                ));
            }
        }
        if at_target {
            replacement_target = Some(SetextRepackTargetLocation {
                page_index: output_page,
                byte_offset: u16::try_from(scratch.pages[output_page].bytes.len()).map_err(
                    |_| SerializedGreenError::Corrupt("Setext replacement offset exceeds u16"),
                )?,
                event_ordinal_in_leaf: scratch.pages[output_page].summary.tokens,
                source_before_in_leaf: scratch.pages[output_page].summary.metric,
            });
        }
        scratch.pages[output_page].push_raw(output, event_summary, program)?;
        event_ordinal = event_ordinal
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow("Setext leaf event ordinal"))?;
    }
    if next_program_ordinal != child_count {
        return Err(SerializedGreenError::Corrupt(
            "Setext target leaf has an unreferenced Program edge",
        ));
    }
    actual.leaves = 1;
    actual.height = 1;
    if actual != expected {
        return Err(SerializedGreenError::Corrupt(
            "Setext target leaf summary mismatch",
        ));
    }
    if !found_target || scratch.pages[output_page].is_empty() {
        return Err(SerializedGreenError::StaleCursor);
    }
    scratch.pages[output_page].seal()?;
    let page_count = output_page + 1;
    if !(1..=2).contains(&page_count) {
        return Err(SerializedGreenError::Corrupt(
            "Setext replacement leaf count exceeds its bound",
        ));
    }
    scratch.page_count = page_count;
    let replacement = scratch.pages[..page_count]
        .iter()
        .try_fold(GreenSummary::default(), |summary, page| {
            summary.followed_by(page.summary)
        })?;
    Ok((
        expected,
        replacement,
        page_count,
        replacement_target.ok_or(SerializedGreenError::StaleCursor)?,
    ))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn prepare_whole_normalization_leaf_repack(
    arena: &PageArena,
    leaf: ArenaId,
    target_event_ordinal: u64,
    target_byte_offset: u16,
    replacement_block: BlockId,
    facts: GreenHeadingOpenFacts,
    encoded_reidentified_heading: &[u8],
    scratch: &mut SetextRepackScratch,
) -> Result<(GreenSummary, GreenSummary), SerializedGreenError> {
    scratch.reset()?;
    let child_count = arena.packed_child_count(leaf)?;
    let payload = arena.payload(leaf)?;
    let expected = decode_summary(payload, LEAF_TAG)?;
    let mut decoder = Decoder::new(&payload[LEAF_HEADER_BYTES..]);
    let mut actual = GreenSummary::default();
    let mut next_program_ordinal = 0_usize;
    let mut event_ordinal = 0_u64;
    let mut found_target = false;

    while !decoder.is_empty() {
        let start =
            LEAF_HEADER_BYTES
                .checked_add(decoder.cursor)
                .ok_or(SerializedGreenError::Overflow(
                    "whole-normalization decoded event offset",
                ))?;
        let event = decode_event(&mut decoder, arena, leaf, &mut next_program_ordinal)?;
        let end =
            LEAF_HEADER_BYTES
                .checked_add(decoder.cursor)
                .ok_or(SerializedGreenError::Overflow(
                    "whole-normalization decoded event end",
                ))?;
        let raw = payload
            .get(start..end)
            .ok_or(SerializedGreenError::Corrupt(
                "whole-normalization decoded event escapes leaf",
            ))?;
        let at_target = event_ordinal == target_event_ordinal;
        let (output, program) = if at_target {
            let offset = u16::try_from(start).map_err(|_| {
                SerializedGreenError::Corrupt("whole-normalization leaf offset exceeds u16")
            })?;
            let DecodedGreenEventKind::Enter {
                block,
                kind,
                facts: stored_facts,
            } = &event
            else {
                return Err(SerializedGreenError::StaleCursor);
            };
            if found_target
                || offset != target_byte_offset
                || *block != replacement_block
                || *kind != GreenKind::HEADING
                || GreenHeadingOpenFacts::try_from_envelope(stored_facts) != Ok(facts)
                || raw.len() != encoded_reidentified_heading.len()
            {
                return Err(SerializedGreenError::StaleCursor);
            }
            found_target = true;
            (encoded_reidentified_heading, None)
        } else {
            let program = match &event {
                DecodedGreenEventKind::Coverage(DecodedSourceProjectionRun {
                    logical_contribution: DecodedLogicalContribution::Program(program),
                    ..
                }) => Some((
                    program.retained_page()?,
                    usize::from(program.encoded_ordinal_offset),
                )),
                DecodedGreenEventKind::Enter { .. }
                | DecodedGreenEventKind::Coverage(_)
                | DecodedGreenEventKind::Exit { .. } => None,
            };
            (raw, program)
        };
        let event_summary = GreenSummary::decoded_event(&event);
        actual = actual.followed_by(event_summary)?;
        if !scratch.pages[0].can_fit(output.len(), program.is_some()) {
            return Err(SerializedGreenError::Corrupt(
                "same-width whole normalization changed leaf cardinality",
            ));
        }
        scratch.pages[0].push_raw(output, event_summary, program)?;
        event_ordinal = event_ordinal
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "whole-normalization leaf event ordinal",
            ))?;
    }
    if next_program_ordinal != child_count {
        return Err(SerializedGreenError::Corrupt(
            "whole-normalization target leaf has an unreferenced Program edge",
        ));
    }
    actual.leaves = 1;
    actual.height = 1;
    if actual != expected || !found_target || scratch.pages[0].is_empty() {
        return Err(SerializedGreenError::StaleCursor);
    }
    scratch.pages[0].seal()?;
    scratch.page_count = 1;
    Ok((expected, scratch.pages[0].summary))
}

#[cfg(test)]
pub(crate) fn serialized_green_test_trace(
    document: &SerializedGreenDocument,
    arena: &PageArena,
) -> Result<Vec<SerializedGreenTestEvent>, SerializedGreenError> {
    let mut trace = Vec::new();
    let mut open_kinds = Vec::new();
    for leaf_index in 0..document.leaf_count(arena)? {
        let leaf = document
            .leaf_at(arena, leaf_index)?
            .ok_or(SerializedGreenError::Corrupt("test trace leaf missing"))?;
        visit_decoded_leaf_events(arena, leaf, |_, event| {
            let event = match event {
                DecodedGreenEventKind::Enter { block, kind, .. } => {
                    open_kinds.push(kind);
                    SerializedGreenTestEvent::Enter { block, kind }
                }
                DecodedGreenEventKind::Coverage(run) => {
                    let logical = match run.logical_contribution {
                        DecodedLogicalContribution::None => SerializedGreenTestLogical::None,
                        DecodedLogicalContribution::Identity => {
                            SerializedGreenTestLogical::Identity
                        }
                        DecodedLogicalContribution::Hidden { affinity } => {
                            SerializedGreenTestLogical::Hidden(affinity)
                        }
                        DecodedLogicalContribution::Atomic(projection) => {
                            SerializedGreenTestLogical::Atomic(projection.kind)
                        }
                        DecodedLogicalContribution::Program(program) => {
                            SerializedGreenTestLogical::Program {
                                piece_count: program.piece_count,
                            }
                        }
                    };
                    SerializedGreenTestEvent::Coverage {
                        coverage: run.id,
                        metric: run.metric,
                        owner_relative_depth: run.owner_relative_depth,
                        part: run.part,
                        logical,
                    }
                }
                DecodedGreenEventKind::Exit { facts, .. } => {
                    let kind = open_kinds
                        .pop()
                        .ok_or(SerializedGreenError::Corrupt("test trace stack underflow"))?;
                    facts.validate_for_kind(kind).map_err(|_| {
                        SerializedGreenError::Corrupt(
                            "close-time facts do not match the closing block kind",
                        )
                    })?;
                    SerializedGreenTestEvent::Exit
                }
            };
            trace.push(event);
            Ok(())
        })?;
    }
    if !open_kinds.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "test trace ends with open blocks",
        ));
    }
    Ok(trace)
}

#[cfg(test)]
pub(crate) fn serialized_green_test_canonical_trace(
    document: &SerializedGreenDocument,
    arena: &PageArena,
) -> Result<Vec<SerializedGreenTestCanonicalEvent>, SerializedGreenError> {
    let mut trace = Vec::new();
    let mut open_kinds = Vec::new();
    for leaf_index in 0..document.leaf_count(arena)? {
        let leaf = document
            .leaf_at(arena, leaf_index)?
            .ok_or(SerializedGreenError::Corrupt("test trace leaf missing"))?;
        visit_decoded_leaf_events(arena, leaf, |_, event| {
            match event {
                DecodedGreenEventKind::Enter { block, kind, .. } => {
                    open_kinds.push(kind);
                    trace.push(SerializedGreenTestCanonicalEvent::Enter { block, kind });
                }
                DecodedGreenEventKind::Coverage(run) => {
                    let can_merge = matches!(
                        trace.last(),
                        Some(SerializedGreenTestCanonicalEvent::Coverage {
                            owner_relative_depth,
                            part,
                            ..
                        }) if *owner_relative_depth == run.owner_relative_depth && *part == run.part
                    );
                    if can_merge {
                        let Some(SerializedGreenTestCanonicalEvent::Coverage { metric, .. }) =
                            trace.last_mut()
                        else {
                            unreachable!("coverage merge predicate matched the last event")
                        };
                        *metric = metric.checked_add(run.metric)?;
                    } else {
                        trace.push(SerializedGreenTestCanonicalEvent::Coverage {
                            metric: run.metric,
                            owner_relative_depth: run.owner_relative_depth,
                            part: run.part,
                        });
                    }
                }
                DecodedGreenEventKind::Exit { facts, .. } => {
                    let kind = open_kinds
                        .pop()
                        .ok_or(SerializedGreenError::Corrupt("test trace stack underflow"))?;
                    facts.validate_for_kind(kind).map_err(|_| {
                        SerializedGreenError::Corrupt(
                            "close-time facts do not match the closing block kind",
                        )
                    })?;
                    trace.push(SerializedGreenTestCanonicalEvent::Exit);
                }
            }
            Ok(())
        })?;
    }
    if !open_kinds.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "test trace ends with open blocks",
        ));
    }
    Ok(trace)
}

/// Test-only semantic decoder for a green child retained behind a composite
/// parent. Production deliberately exposes no independently ownable child
/// document; this helper revalidates the descriptor and borrows its persistent
/// sequence only long enough to compare a restart result with a clean parse.
#[cfg(all(test, feature = "exact-parser"))]
pub(crate) fn serialized_green_test_composite_canonical_trace(
    descriptor: SerializedGreenCompositeDescriptor,
    arena: &PageArena,
) -> Result<Vec<SerializedGreenTestCanonicalEvent>, SerializedGreenError> {
    if validate_serialized_green_composite_child(arena, descriptor.manifest)? != descriptor {
        return Err(SerializedGreenError::Corrupt(
            "test composite descriptor changed before canonical decoding",
        ));
    }
    let mut trace = Vec::new();
    let mut open_kinds = Vec::new();
    for leaf_index in 0..descriptor.summary.leaves {
        let leaf = locate_leaf_in_arena(arena, descriptor.sequence_root, leaf_index)?.ok_or(
            SerializedGreenError::Corrupt("test composite trace leaf missing"),
        )?;
        visit_decoded_leaf_events(arena, leaf, |_, event| {
            match event {
                DecodedGreenEventKind::Enter { block, kind, .. } => {
                    open_kinds.push(kind);
                    trace.push(SerializedGreenTestCanonicalEvent::Enter { block, kind });
                }
                DecodedGreenEventKind::Coverage(run) => {
                    let can_merge = matches!(
                        trace.last(),
                        Some(SerializedGreenTestCanonicalEvent::Coverage {
                            owner_relative_depth,
                            part,
                            ..
                        }) if *owner_relative_depth == run.owner_relative_depth && *part == run.part
                    );
                    if can_merge {
                        let Some(SerializedGreenTestCanonicalEvent::Coverage { metric, .. }) =
                            trace.last_mut()
                        else {
                            unreachable!("coverage merge predicate matched the last event")
                        };
                        *metric = metric.checked_add(run.metric)?;
                    } else {
                        trace.push(SerializedGreenTestCanonicalEvent::Coverage {
                            metric: run.metric,
                            owner_relative_depth: run.owner_relative_depth,
                            part: run.part,
                        });
                    }
                }
                DecodedGreenEventKind::Exit { facts, .. } => {
                    let kind = open_kinds.pop().ok_or(SerializedGreenError::Corrupt(
                        "test composite trace stack underflow",
                    ))?;
                    facts.validate_for_kind(kind).map_err(|_| {
                        SerializedGreenError::Corrupt(
                            "test composite close facts do not match block kind",
                        )
                    })?;
                    trace.push(SerializedGreenTestCanonicalEvent::Exit);
                }
            }
            Ok(())
        })?;
    }
    if !open_kinds.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "test composite trace ends with open blocks",
        ));
    }
    Ok(trace)
}

#[cfg(test)]
pub(crate) fn serialized_green_test_logical_segments(
    document: &SerializedGreenDocument,
    arena: &PageArena,
) -> Result<Vec<SerializedGreenTestLogicalSegment>, SerializedGreenError> {
    let mut targets = Vec::new();
    for leaf_index in 0..document.leaf_count(arena)? {
        let leaf = document
            .leaf_at(arena, leaf_index)?
            .ok_or(SerializedGreenError::Corrupt("test trace leaf missing"))?;
        visit_decoded_leaf_events(arena, leaf, |byte_offset, event| {
            if let DecodedGreenEventKind::Enter { block, kind, .. } = event
                && kind.logical_channel().is_some()
            {
                targets.push(GreenEnterCapability {
                    manifest: document.manifest_id(),
                    leaf,
                    base_leaf_index: leaf_index,
                    byte_offset,
                    block,
                    kind,
                });
            }
            Ok(())
        })?;
    }

    let mut output = Vec::new();
    for target in targets {
        let mut logical = document.logical_cursor(arena, target)?;
        while let Some(segment) = logical.next_segment(document, arena)? {
            output.push(SerializedGreenTestLogicalSegment {
                target_block: target.block,
                target_kind: target.kind,
                part: segment.part,
                physical_owner_block: segment.physical_owner.block,
                physical_owner_kind: segment.physical_owner.kind,
                consumer_block: segment.consumer.block,
                consumer_kind: segment.consumer.kind,
                channel: segment.channel,
                byte_range: segment.byte_range,
                utf16_range: segment.utf16_range,
                logical_byte_range: segment.logical_byte_range,
                logical_utf16_range: segment.logical_utf16_range,
                mapping: segment.mapping,
            });
        }
    }
    Ok(output)
}

#[cfg(test)]
pub(crate) fn serialized_green_test_close_facts(
    document: &SerializedGreenDocument,
    arena: &PageArena,
) -> Result<Vec<(GreenKind, GreenCloseFacts)>, SerializedGreenError> {
    let mut trace = Vec::new();
    let mut open_kinds = Vec::new();
    for leaf_index in 0..document.leaf_count(arena)? {
        let leaf = document
            .leaf_at(arena, leaf_index)?
            .ok_or(SerializedGreenError::Corrupt("test trace leaf missing"))?;
        visit_decoded_leaf_events(arena, leaf, |_, event| {
            match event {
                DecodedGreenEventKind::Enter { kind, .. } => open_kinds.push(kind),
                DecodedGreenEventKind::Coverage(_) => {}
                DecodedGreenEventKind::Exit { facts, .. } => {
                    let kind = open_kinds
                        .pop()
                        .ok_or(SerializedGreenError::Corrupt("test trace stack underflow"))?;
                    facts.validate_for_kind(kind).map_err(|_| {
                        SerializedGreenError::Corrupt(
                            "close-time facts do not match the closing block kind",
                        )
                    })?;
                    trace.push((kind, facts));
                }
            }
            Ok(())
        })?;
    }
    if !open_kinds.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "test trace ends with open blocks",
        ));
    }
    Ok(trace)
}

#[cfg(test)]
pub(crate) fn serialized_green_test_close_states(
    document: &SerializedGreenDocument,
    arena: &PageArena,
) -> Result<Vec<(GreenKind, ClosedChildAggregate, bool, GreenCloseFacts)>, SerializedGreenError> {
    let mut trace = Vec::new();
    let mut open_kinds = Vec::new();
    for leaf_index in 0..document.leaf_count(arena)? {
        let leaf = document
            .leaf_at(arena, leaf_index)?
            .ok_or(SerializedGreenError::Corrupt("test trace leaf missing"))?;
        visit_decoded_leaf_events(arena, leaf, |_, event| {
            match event {
                DecodedGreenEventKind::Enter { kind, .. } => open_kinds.push(kind),
                DecodedGreenEventKind::Coverage(_) => {}
                DecodedGreenEventKind::Exit {
                    closed,
                    last_line_blank,
                    facts,
                } => {
                    let kind = open_kinds
                        .pop()
                        .ok_or(SerializedGreenError::Corrupt("test trace stack underflow"))?;
                    trace.push((kind, closed, last_line_blank, facts));
                }
            }
            Ok(())
        })?;
    }
    if !open_kinds.is_empty() {
        return Err(SerializedGreenError::Corrupt(
            "test trace ends with open blocks",
        ));
    }
    Ok(trace)
}

#[cfg(test)]
pub(crate) fn serialized_green_test_open_facts(
    document: &SerializedGreenDocument,
    arena: &PageArena,
) -> Result<Vec<(GreenKind, FactsEnvelope)>, SerializedGreenError> {
    let mut trace = Vec::new();
    for leaf_index in 0..document.leaf_count(arena)? {
        let leaf = document
            .leaf_at(arena, leaf_index)?
            .ok_or(SerializedGreenError::Corrupt("test trace leaf missing"))?;
        visit_decoded_leaf_events(arena, leaf, |_, event| {
            if let DecodedGreenEventKind::Enter { kind, facts, .. } = event {
                validate_facts_for_kind(kind, &facts).map_err(|_| {
                    SerializedGreenError::Corrupt(
                        "open-time facts do not match the opening block kind",
                    )
                })?;
                trace.push((kind, facts));
            }
            Ok(())
        })?;
    }
    Ok(trace)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenEnterCapability {
    pub manifest: SerializedGreenManifestId,
    pub leaf: ArenaId,
    pub base_leaf_index: u64,
    pub byte_offset: u16,
    pub block: BlockId,
    pub kind: GreenKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenOpenFrame {
    pub block: BlockId,
    pub kind: GreenKind,
    pub facts: FactsEnvelope,
    pub enter: GreenEnterCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenOpenCapability {
    pub block: BlockId,
    pub kind: GreenKind,
    pub enter: GreenEnterCapability,
    pub path_index: u32,
}

impl GreenOpenCapability {
    fn from_frame(frame: &GreenOpenFrame, path_index: usize) -> Result<Self, SerializedGreenError> {
        Ok(Self {
            block: frame.block,
            kind: frame.kind,
            enter: frame.enter,
            path_index: u32::try_from(path_index)
                .map_err(|_| SerializedGreenError::Overflow("open path index"))?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenCoverageCapability {
    pub manifest: SerializedGreenManifestId,
    pub leaf: ArenaId,
    pub base_leaf_index: u64,
    pub byte_offset: u16,
    pub coverage: CoverageId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SerializedGreenManifestDescriptor {
    pub(crate) manifest: SerializedGreenManifestId,
    pub(crate) syntax_profile: u64,
    pub(crate) source_revision: SourceRevision,
    pub(crate) source_root: SourceRootId,
    pub(crate) source_bytes: u64,
    pub(crate) source_utf16: u64,
    pub(crate) grammar_revision: GrammarRevision,
    pub(crate) parse_generation: ParseGeneration,
    pub(crate) semantic_epoch: u64,
    pub(crate) known_bytes_start: u64,
    pub(crate) known_bytes_end: u64,
}

/// Revalidated identity, root binding, and complete folded summary for the
/// serialized-green child accepted by a same-arena composite parent.
///
/// Both arena IDs remain private so this descriptor cannot become an
/// independently ownable raw child handle. They still participate in exact
/// descriptor equality, allowing a parent to prove that its decoded child is
/// the same manifest and sequence root it adopted during the build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SerializedGreenCompositeDescriptor {
    manifest: ArenaId,
    sequence_root: ArenaId,
    syntax_profile: u64,
    source_revision: SourceRevision,
    source_root: SourceRootId,
    source_metric: SerializedMetric,
    grammar_revision: GrammarRevision,
    parse_generation: ParseGeneration,
    semantic_epoch: u64,
    known_bytes_start: u64,
    known_bytes_end: u64,
    summary: GreenSummary,
    coverage_count: u64,
}

/// Typed, parent-derived borrow of the green child already retained by a
/// fresh adoption journal. It is deliberately non-Clone and exposes neither
/// `SerializedGreenDocument` nor either manifest/sequence `ArenaId`.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentRetainedGreenLease<'lease> {
    build: ArenaBuildId,
    parent_activation: ArenaScopedId,
    owner: &'lease ArenaBuildOwner,
    descriptor: SerializedGreenCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl<'lease> ParentRetainedGreenLease<'lease> {
    pub(crate) fn from_parent_mint(
        mint: crate::storage_only_composite_document::RestartGreenLeaseMint<'lease>,
    ) -> Self {
        let (build, parent_activation, owner, descriptor) = mint.into_green_lease_parts();
        Self {
            build,
            parent_activation,
            owner,
            descriptor,
        }
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) fn validate_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        self.validated_manifest(session).map(|_| ())
    }

    /// Available to serialized-green child modules implementing retained
    /// prefix/suffix operations, but never returned through the parent API.
    fn validated_manifest(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ArenaId, SerializedGreenError> {
        if session.id() != self.build {
            return Err(SerializedGreenError::Invalid(
                "parent-retained green lease and arena build differ",
            ));
        }
        session.arena().local_id(self.parent_activation)?;
        let manifest = session.owner_id(self.owner)?;
        let descriptor = validate_serialized_green_composite_child(session.arena(), manifest)?;
        if descriptor != self.descriptor {
            return Err(SerializedGreenError::Corrupt(
                "parent-retained green lease descriptor changed",
            ));
        }
        Ok(manifest)
    }

    /// Suspended read sibling used when parser and writer are jointly parked
    /// at R or C. It authenticates the exact retained parent child without
    /// resuming or mutating the candidate journal.
    fn validated_suspended_manifest(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<ArenaId, SerializedGreenError> {
        if ticket.id() != self.build {
            return Err(SerializedGreenError::Invalid(
                "parent-retained green lease and suspended build differ",
            ));
        }
        arena.local_id(self.parent_activation)?;
        let manifest = arena.suspended_owner_id(ticket, self.owner)?;
        let descriptor = validate_serialized_green_composite_child(arena, manifest)?;
        if descriptor != self.descriptor {
            return Err(SerializedGreenError::Corrupt(
                "parent-retained green lease descriptor changed while suspended",
            ));
        }
        Ok(manifest)
    }
}

impl SerializedGreenCompositeDescriptor {
    fn new(
        manifest: ArenaId,
        sequence_root: ArenaId,
        value: &Manifest,
    ) -> Result<Self, SerializedGreenError> {
        let structural_tokens =
            value
                .summary
                .blocks
                .checked_mul(2)
                .ok_or(SerializedGreenError::Corrupt(
                    "green block count exceeds event count domain",
                ))?;
        let coverage_count = value.summary.tokens.checked_sub(structural_tokens).ok_or(
            SerializedGreenError::Corrupt("green event count is smaller than Enter/Exit count"),
        )?;
        if value.summary.balance != 0 {
            return Err(SerializedGreenError::Corrupt(
                "composite green child is structurally unbalanced",
            ));
        }
        Ok(Self {
            manifest,
            sequence_root,
            syntax_profile: value.syntax_profile,
            source_revision: value.source_revision,
            source_root: value.source_root,
            source_metric: SerializedMetric {
                bytes: value.source_bytes,
                utf16: value.source_utf16,
            },
            grammar_revision: value.grammar_revision,
            parse_generation: value.parse_generation,
            semantic_epoch: value.semantic_epoch,
            known_bytes_start: value.known_bytes.start,
            known_bytes_end: value.known_bytes.end,
            summary: value.summary,
            coverage_count,
        })
    }

    pub(crate) const fn syntax_profile(self) -> u64 {
        self.syntax_profile
    }

    pub(crate) const fn source_revision(self) -> SourceRevision {
        self.source_revision
    }

    pub(crate) const fn source_root(self) -> SourceRootId {
        self.source_root
    }

    pub(crate) const fn source_metric(self) -> SerializedMetric {
        self.source_metric
    }

    pub(crate) const fn grammar_revision(self) -> GrammarRevision {
        self.grammar_revision
    }

    pub(crate) const fn parse_generation(self) -> ParseGeneration {
        self.parse_generation
    }

    pub(crate) const fn semantic_epoch(self) -> u64 {
        self.semantic_epoch
    }

    pub(crate) const fn known_bytes_start(self) -> u64 {
        self.known_bytes_start
    }

    pub(crate) const fn known_bytes_end(self) -> u64 {
        self.known_bytes_end
    }

    pub(crate) const fn leaf_pages(self) -> u64 {
        self.summary.leaves
    }

    pub(crate) const fn tokens(self) -> u64 {
        self.summary.tokens
    }

    pub(crate) const fn blocks(self) -> u64 {
        self.summary.blocks
    }

    pub(crate) const fn height(self) -> u16 {
        self.summary.height
    }

    pub(crate) const fn physical_metric(self) -> SerializedMetric {
        self.summary.metric
    }

    pub(crate) const fn logical_metric(self) -> SerializedMetric {
        self.summary.logical_metric
    }

    pub(crate) const fn balance(self) -> i64 {
        self.summary.balance
    }

    pub(crate) const fn minimum_prefix(self) -> i64 {
        self.summary.minimum_prefix
    }

    pub(crate) const fn minimum_closed_depth(self) -> Option<i64> {
        self.summary.minimum_closed_depth
    }

    pub(crate) const fn coverage_count(self) -> u64 {
        self.coverage_count
    }

    /// Typed host snapshot seam for a composite-owned green child. The host
    /// receives one arena-branded manifest identity, never a detachable child
    /// owner or a caller-supplied leaf coordinate.
    #[cfg(feature = "host-mirror-probe")]
    pub(crate) fn scoped_manifest_for_host_snapshot(
        self,
        arena: &PageArena,
    ) -> Result<ArenaScopedId, SerializedGreenError> {
        arena
            .scoped_query_id(self.manifest)
            .map_err(SerializedGreenError::from)
    }
}

/// Read-side composite validation. The selected parent edge must decode as an
/// exact green manifest; its child sequence is revalidated and folded before
/// any root metadata or summary total is exposed.
pub(crate) fn validate_serialized_green_composite_child(
    arena: &PageArena,
    manifest: ArenaId,
) -> Result<SerializedGreenCompositeDescriptor, SerializedGreenError> {
    let (value, sequence_root) = decode_document(arena, manifest)?;
    SerializedGreenCompositeDescriptor::new(manifest, sequence_root, &value)
}

impl SerializedGreenManifestDescriptor {
    fn new(manifest: SerializedGreenManifestId, value: &Manifest) -> Self {
        Self {
            manifest,
            syntax_profile: value.syntax_profile,
            source_revision: value.source_revision,
            source_root: value.source_root,
            source_bytes: value.source_bytes,
            source_utf16: value.source_utf16,
            grammar_revision: value.grammar_revision,
            parse_generation: value.parse_generation,
            semantic_epoch: value.semantic_epoch,
            known_bytes_start: value.known_bytes.start,
            known_bytes_end: value.known_bytes.end,
        }
    }

    fn matches(self, manifest: SerializedGreenManifestId, value: &Manifest) -> bool {
        self == Self::new(manifest, value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoredProjectionResetLocation {
    ImplicitZero,
    Run {
        leaf: ArenaId,
        leaf_index: u64,
        event_byte_offset: u16,
        coverage: CoverageId,
    },
}

/// Storage/query authority for one reset embedded in the flat source-ordered
/// run sequence. This capability never certifies a parser restart state or a
/// semantic-envelope end. It must be joined with those distinct capabilities
/// before a suffix can be adopted.
#[must_use = "stored reset authority must be consumed by a storage join or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct StoredProjectionResetCapability {
    binding: SerializedGreenManifestDescriptor,
    location: StoredProjectionResetLocation,
    source_end: SerializedMetric,
}

impl StoredProjectionResetCapability {
    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.binding.manifest
    }

    #[must_use]
    pub const fn source_revision(&self) -> SourceRevision {
        self.binding.source_revision
    }

    #[must_use]
    pub const fn source_root(&self) -> SourceRootId {
        self.binding.source_root
    }

    #[must_use]
    pub const fn parse_generation(&self) -> ParseGeneration {
        self.binding.parse_generation
    }

    #[must_use]
    pub const fn source_end(&self) -> SerializedMetric {
        self.source_end
    }

    #[must_use]
    pub const fn is_implicit_zero(&self) -> bool {
        matches!(self.location, StoredProjectionResetLocation::ImplicitZero)
    }

    #[must_use]
    pub const fn coverage(&self) -> Option<CoverageId> {
        match self.location {
            StoredProjectionResetLocation::ImplicitZero => None,
            StoredProjectionResetLocation::Run { coverage, .. } => Some(coverage),
        }
    }

    #[must_use]
    pub const fn leaf_index(&self) -> Option<u64> {
        match self.location {
            StoredProjectionResetLocation::ImplicitZero => None,
            StoredProjectionResetLocation::Run { leaf_index, .. } => Some(leaf_index),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectionResetSeekReceipt {
    pub maximum_pages: usize,
    pub pages_scanned: usize,
    pub predecessor_pages: usize,
    pub sequence_nodes_visited: usize,
    pub events_inspected: usize,
    pub decoded_page_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProjectionResetSeekOutcome {
    Found {
        reset: StoredProjectionResetCapability,
        receipt: ProjectionResetSeekReceipt,
    },
    ImplicitZero {
        reset: StoredProjectionResetCapability,
        receipt: ProjectionResetSeekReceipt,
    },
    NotFoundWithinBound(ProjectionResetSeekReceipt),
}

impl ProjectionResetSeekOutcome {
    #[must_use]
    pub const fn receipt(&self) -> ProjectionResetSeekReceipt {
        match self {
            Self::Found { receipt, .. }
            | Self::ImplicitZero { receipt, .. }
            | Self::NotFoundWithinBound(receipt) => *receipt,
        }
    }
}

/// Revalidated query data. The originating capability remains storage-only;
/// this copyable view is observation data and carries no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedProjectionReset {
    pub source_end: SerializedMetric,
    pub coverage: Option<CoverageId>,
    pub implicit_zero: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionProgramCapability {
    pub manifest: SerializedGreenManifestId,
    pub leaf: ArenaId,
    pub page: ArenaId,
    pub edge_ordinal: u16,
    pub piece_count: u16,
    pub physical_metric: SerializedMetric,
    pub logical_metric: SerializedMetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalContributionView {
    None,
    Identity {
        logical_metric: SerializedMetric,
    },
    Hidden {
        affinity: GreenAffinity,
    },
    Atomic {
        projection: AtomicProjection,
    },
    Program {
        logical_metric: SerializedMetric,
        program: ProjectionProgramCapability,
    },
}

impl LogicalContributionView {
    #[must_use]
    pub const fn logical_metric(self) -> SerializedMetric {
        match self {
            Self::None | Self::Hidden { .. } => SerializedMetric { bytes: 0, utf16: 0 },
            Self::Identity { logical_metric } | Self::Program { logical_metric, .. } => {
                logical_metric
            }
            Self::Atomic { projection } => projection.logical_metric,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenCoverageView {
    pub coverage: CoverageId,
    pub part: CoveragePart,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub owner: GreenOpenCapability,
    pub logical_consumer: Option<GreenOpenCapability>,
    pub logical_channel: Option<LogicalChannel>,
    pub logical_contribution: LogicalContributionView,
    /// Query observation of the storage reset after this exact physical run.
    /// This flag is not a parser-restart or semantic-envelope capability.
    pub projection_reset_after: bool,
    pub cursor: GreenCoverageCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalSegmentMapping {
    ExactIdentity,
    Hidden { affinity: GreenAffinity },
    AtomicAmbiguity { transform: AtomicProjectionKind },
    Virtual { kind: VirtualProjectionKind },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenLogicalSegment {
    pub coverage: CoverageId,
    pub part: CoveragePart,
    pub physical_owner: GreenOpenCapability,
    pub consumer: GreenOpenCapability,
    pub channel: LogicalChannel,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub logical_byte_range: Range<u64>,
    pub logical_utf16_range: Range<u64>,
    pub mapping: LogicalSegmentMapping,
    pub source: GreenCoverageCapability,
    pub program: Option<ProjectionProgramCapability>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GreenProjectionPosition {
    Exact {
        physical: u64,
        logical: u64,
    },
    Hidden {
        physical: Range<u64>,
        logical_boundary: u64,
        affinity: GreenAffinity,
    },
    AtomicAmbiguity {
        physical: Range<u64>,
        logical: Range<u64>,
        transform: AtomicProjectionKind,
    },
    Virtual {
        physical_boundary: u64,
        logical: Range<u64>,
        kind: VirtualProjectionKind,
    },
}

impl GreenLogicalSegment {
    fn ranges(&self, coordinate: GreenCoordinate) -> (&Range<u64>, &Range<u64>) {
        match coordinate {
            GreenCoordinate::Bytes => (&self.byte_range, &self.logical_byte_range),
            GreenCoordinate::Utf16 => (&self.utf16_range, &self.logical_utf16_range),
        }
    }

    #[must_use]
    pub fn map_physical(
        &self,
        coordinate: GreenCoordinate,
        offset: u64,
    ) -> Option<GreenProjectionPosition> {
        let (physical, logical) = self.ranges(coordinate);
        if offset < physical.start || offset > physical.end {
            return None;
        }
        match self.mapping {
            LogicalSegmentMapping::ExactIdentity => Some(GreenProjectionPosition::Exact {
                physical: offset,
                logical: logical.start + (offset - physical.start),
            }),
            LogicalSegmentMapping::Hidden { affinity } => {
                if offset == physical.start || offset == physical.end {
                    Some(GreenProjectionPosition::Exact {
                        physical: offset,
                        logical: logical.start,
                    })
                } else {
                    Some(GreenProjectionPosition::Hidden {
                        physical: physical.clone(),
                        logical_boundary: logical.start,
                        affinity,
                    })
                }
            }
            LogicalSegmentMapping::AtomicAmbiguity { transform } => {
                if offset == physical.start {
                    Some(GreenProjectionPosition::Exact {
                        physical: offset,
                        logical: logical.start,
                    })
                } else if offset == physical.end {
                    Some(GreenProjectionPosition::Exact {
                        physical: offset,
                        logical: logical.end,
                    })
                } else {
                    Some(GreenProjectionPosition::AtomicAmbiguity {
                        physical: physical.clone(),
                        logical: logical.clone(),
                        transform,
                    })
                }
            }
            LogicalSegmentMapping::Virtual { kind } => {
                (offset == physical.start).then(|| GreenProjectionPosition::Virtual {
                    physical_boundary: physical.start,
                    logical: logical.clone(),
                    kind,
                })
            }
        }
    }

    #[must_use]
    pub fn map_logical(
        &self,
        coordinate: GreenCoordinate,
        offset: u64,
    ) -> Option<GreenProjectionPosition> {
        let (physical, logical) = self.ranges(coordinate);
        if offset < logical.start || offset > logical.end {
            return None;
        }
        match self.mapping {
            LogicalSegmentMapping::ExactIdentity => Some(GreenProjectionPosition::Exact {
                physical: physical.start + (offset - logical.start),
                logical: offset,
            }),
            LogicalSegmentMapping::Hidden { affinity } => {
                (offset == logical.start).then(|| GreenProjectionPosition::Hidden {
                    physical: physical.clone(),
                    logical_boundary: logical.start,
                    affinity,
                })
            }
            LogicalSegmentMapping::AtomicAmbiguity { transform } => {
                if offset == logical.start {
                    Some(GreenProjectionPosition::Exact {
                        physical: physical.start,
                        logical: offset,
                    })
                } else if offset == logical.end {
                    Some(GreenProjectionPosition::Exact {
                        physical: physical.end,
                        logical: offset,
                    })
                } else {
                    Some(GreenProjectionPosition::AtomicAmbiguity {
                        physical: physical.clone(),
                        logical: logical.clone(),
                        transform,
                    })
                }
            }
            LogicalSegmentMapping::Virtual { kind } => {
                if offset == logical.start || offset == logical.end {
                    Some(GreenProjectionPosition::Exact {
                        physical: physical.start,
                        logical: offset,
                    })
                } else {
                    Some(GreenProjectionPosition::Virtual {
                        physical_boundary: physical.start,
                        logical: logical.clone(),
                        kind,
                    })
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenLogicalReceipt {
    pub coverage_runs_visited: usize,
    pub projection_program_pages_decoded: usize,
    pub projection_program_bytes_validated: usize,
    pub projection_pieces_yielded: usize,
    pub maximum_program_scratch_bytes: usize,
    pub stream: GreenStreamReceipt,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenStreamReceipt {
    pub root_descents: usize,
    pub successor_root_descents: usize,
    pub sequence_nodes_visited: usize,
    pub summary_nodes_skipped: usize,
    pub leaf_pages_decoded: usize,
    pub events_decoded: usize,
    pub coverage_runs_yielded: usize,
    pub maximum_route_depth: usize,
    pub maximum_open_depth: usize,
    pub maximum_decoded_page_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
struct RouteFrame {
    branch: ArenaId,
    base_leaf_index: u64,
    went_right: bool,
}

#[derive(Debug)]
pub struct GreenStreamCursor {
    manifest: SerializedGreenManifestId,
    route: Vec<RouteFrame>,
    leaf: ArenaId,
    leaf_index: u64,
    events: Vec<DecodedLeafEvent>,
    next_event: usize,
    position: SerializedMetric,
    open: Vec<GreenOpenFrame>,
    active_terminal: Option<usize>,
    receipt: GreenStreamReceipt,
}

#[derive(Debug)]
struct PendingLogicalProgram {
    coverage: GreenCoverageView,
    capability: ProjectionProgramCapability,
    next_byte: usize,
    pieces_remaining: u16,
    physical_position: SerializedMetric,
    expected_physical_end: SerializedMetric,
    expected_logical_end: SerializedMetric,
}

#[derive(Debug)]
pub struct GreenLogicalCursor {
    target: GreenEnterCapability,
    target_frame: GreenOpenFrame,
    channel: LogicalChannel,
    stream: GreenStreamCursor,
    logical_position: SerializedMetric,
    pending: Option<PendingLogicalProgram>,
    receipt: GreenLogicalReceipt,
}

fn derive_active_terminal(open: &[GreenOpenFrame]) -> Result<Option<usize>, SerializedGreenError> {
    let mut active = None;
    for (index, frame) in open.iter().enumerate() {
        if frame.kind.logical_channel().is_some() {
            if active.is_some() {
                return Err(SerializedGreenError::Corrupt(
                    "terminal block contains another terminal",
                ));
            }
            if index + 1 != open.len() {
                return Err(SerializedGreenError::Corrupt(
                    "terminal block contains another block",
                ));
            }
            active = Some(index);
        }
    }
    Ok(active)
}

impl GreenLogicalCursor {
    #[must_use]
    pub fn receipt(&self) -> GreenLogicalReceipt {
        GreenLogicalReceipt {
            stream: self.stream.receipt(),
            ..self.receipt
        }
    }

    /// The exact terminal frame revalidated while this logical cursor was
    /// constructed. Consumers derive inline kind/facts from this frame instead
    /// of reclassifying source text or accepting caller-supplied metadata.
    pub fn target_frame(&self) -> Result<&GreenOpenFrame, SerializedGreenError> {
        let frame = &self.target_frame;
        if frame.enter != self.target || frame.kind.logical_channel() != Some(self.channel) {
            return Err(SerializedGreenError::Corrupt(
                "logical cursor terminal frame differs from its target",
            ));
        }
        Ok(frame)
    }

    fn pending_segment(
        &mut self,
        arena: &PageArena,
    ) -> Result<Option<GreenLogicalSegment>, SerializedGreenError> {
        let Some(mut pending) = self.pending.take() else {
            return Ok(None);
        };
        if pending.pieces_remaining == 0 {
            return Err(SerializedGreenError::Corrupt(
                "projection program piece cursor escaped page",
            ));
        }
        let payload = arena.payload(pending.capability.page)?;
        let mut decoder = Decoder::new(payload);
        if pending.next_byte > payload.len() {
            return Err(SerializedGreenError::Corrupt(
                "projection program byte cursor escaped page",
            ));
        }
        decoder.cursor = pending.next_byte;
        let piece = decode_projection_piece(&mut decoder)?;
        pending.next_byte = decoder.cursor;
        pending.pieces_remaining -= 1;
        let (physical_metric, logical_metric) = piece.metrics();
        let physical_start = pending.physical_position;
        let physical_end = physical_start.checked_add(physical_metric).map_err(|_| {
            SerializedGreenError::Corrupt("projection program physical prefix overflow")
        })?;
        let logical_start = self.logical_position;
        let logical_end = logical_start.checked_add(logical_metric).map_err(|_| {
            SerializedGreenError::Corrupt("projection program logical prefix overflow")
        })?;
        if physical_end.bytes > pending.expected_physical_end.bytes
            || physical_end.utf16 > pending.expected_physical_end.utf16
            || logical_end.bytes > pending.expected_logical_end.bytes
            || logical_end.utf16 > pending.expected_logical_end.utf16
        {
            return Err(SerializedGreenError::Corrupt(
                "projection program prefix exceeds its declared partition",
            ));
        }
        pending.physical_position = physical_end;
        self.logical_position = logical_end;
        let mapping = match piece {
            ProjectionPiece::Identity { .. } => LogicalSegmentMapping::ExactIdentity,
            ProjectionPiece::Hidden { affinity, .. } => LogicalSegmentMapping::Hidden { affinity },
            ProjectionPiece::Atomic { projection, .. } => LogicalSegmentMapping::AtomicAmbiguity {
                transform: projection.kind,
            },
            ProjectionPiece::Virtual { kind } => LogicalSegmentMapping::Virtual { kind },
        };
        let consumer = pending
            .coverage
            .logical_consumer
            .ok_or(SerializedGreenError::Corrupt(
                "Program coverage lost its logical consumer",
            ))?;
        let segment = GreenLogicalSegment {
            coverage: pending.coverage.coverage,
            part: pending.coverage.part,
            physical_owner: pending.coverage.owner,
            consumer,
            channel: self.channel,
            byte_range: physical_start.bytes..physical_end.bytes,
            utf16_range: physical_start.utf16..physical_end.utf16,
            logical_byte_range: logical_start.bytes..logical_end.bytes,
            logical_utf16_range: logical_start.utf16..logical_end.utf16,
            mapping,
            source: pending.coverage.cursor,
            program: Some(pending.capability),
        };
        self.receipt.projection_pieces_yielded += 1;
        if pending.pieces_remaining == 0 {
            if !decoder.is_empty() {
                return Err(SerializedGreenError::Corrupt(
                    "trailing projection program bytes",
                ));
            }
            if physical_end != pending.expected_physical_end
                || logical_end != pending.expected_logical_end
            {
                return Err(SerializedGreenError::Corrupt(
                    "projection Program cursor ended on the wrong partition boundary",
                ));
            }
        } else if decoder.is_empty() {
            return Err(SerializedGreenError::Corrupt(
                "projection program has fewer pieces than declared",
            ));
        } else {
            self.pending = Some(pending);
        }
        Ok(Some(segment))
    }

    #[allow(clippy::too_many_lines)] // One resumable state machine covers inline and page-backed runs.
    pub fn next_segment(
        &mut self,
        document: &SerializedGreenDocument,
        arena: &PageArena,
    ) -> Result<Option<GreenLogicalSegment>, SerializedGreenError> {
        if let Some(segment) = self.pending_segment(arena)? {
            return Ok(Some(segment));
        }
        loop {
            let Some(coverage) = self.stream.next_coverage(document, arena)? else {
                return Ok(None);
            };
            self.receipt.coverage_runs_visited += 1;
            if self.stream.active_terminal_frame().map(|frame| frame.enter) != Some(self.target) {
                return Ok(None);
            }
            if coverage
                .logical_consumer
                .as_ref()
                .is_none_or(|consumer| consumer.enter != self.target)
                || coverage.logical_channel != Some(self.channel)
            {
                continue;
            }
            let consumer = coverage
                .logical_consumer
                .ok_or(SerializedGreenError::Corrupt(
                    "logical contribution is missing its consumer",
                ))?;
            let logical_start = self.logical_position;
            match coverage.logical_contribution {
                LogicalContributionView::None => {
                    return Err(SerializedGreenError::Corrupt(
                        "None contribution unexpectedly names a consumer",
                    ));
                }
                LogicalContributionView::Hidden { affinity } => {
                    return Ok(Some(GreenLogicalSegment {
                        coverage: coverage.coverage,
                        part: coverage.part,
                        physical_owner: coverage.owner,
                        consumer,
                        channel: self.channel,
                        byte_range: coverage.byte_range,
                        utf16_range: coverage.utf16_range,
                        logical_byte_range: logical_start.bytes..logical_start.bytes,
                        logical_utf16_range: logical_start.utf16..logical_start.utf16,
                        mapping: LogicalSegmentMapping::Hidden { affinity },
                        source: coverage.cursor,
                        program: None,
                    }));
                }
                LogicalContributionView::Identity { logical_metric } => {
                    let logical_end = logical_start.checked_add(logical_metric)?;
                    self.logical_position = logical_end;
                    return Ok(Some(GreenLogicalSegment {
                        coverage: coverage.coverage,
                        part: coverage.part,
                        physical_owner: coverage.owner,
                        consumer,
                        channel: self.channel,
                        byte_range: coverage.byte_range,
                        utf16_range: coverage.utf16_range,
                        logical_byte_range: logical_start.bytes..logical_end.bytes,
                        logical_utf16_range: logical_start.utf16..logical_end.utf16,
                        mapping: LogicalSegmentMapping::ExactIdentity,
                        source: coverage.cursor,
                        program: None,
                    }));
                }
                LogicalContributionView::Atomic { projection } => {
                    let logical_end = logical_start.checked_add(projection.logical_metric)?;
                    self.logical_position = logical_end;
                    return Ok(Some(GreenLogicalSegment {
                        coverage: coverage.coverage,
                        part: coverage.part,
                        physical_owner: coverage.owner,
                        consumer,
                        channel: self.channel,
                        byte_range: coverage.byte_range,
                        utf16_range: coverage.utf16_range,
                        logical_byte_range: logical_start.bytes..logical_end.bytes,
                        logical_utf16_range: logical_start.utf16..logical_end.utf16,
                        mapping: LogicalSegmentMapping::AtomicAmbiguity {
                            transform: projection.kind,
                        },
                        source: coverage.cursor,
                        program: None,
                    }));
                }
                LogicalContributionView::Program {
                    logical_metric,
                    program,
                } => {
                    if program.manifest != document.manifest_id()
                        || program.leaf != coverage.cursor.leaf
                        || arena.packed_child_at(program.leaf, usize::from(program.edge_ordinal))?
                            != program.page
                    {
                        return Err(SerializedGreenError::StaleCursor);
                    }
                    let next_byte = validate_projection_program_edge_payload(
                        arena,
                        program.page,
                        usize::from(program.piece_count),
                        program.physical_metric,
                        program.logical_metric,
                    )?;
                    self.receipt.projection_program_pages_decoded += 1;
                    self.receipt.projection_program_bytes_validated +=
                        arena.payload(program.page)?.len();
                    self.receipt.maximum_program_scratch_bytes =
                        self.receipt.maximum_program_scratch_bytes.max(
                            std::mem::size_of::<PendingLogicalProgram>()
                                + std::mem::size_of::<ProjectionPiece>()
                                + std::mem::size_of::<Decoder<'_>>(),
                        );
                    let physical_start = SerializedMetric {
                        bytes: coverage.byte_range.start,
                        utf16: coverage.utf16_range.start,
                    };
                    let expected_physical_end = SerializedMetric {
                        bytes: coverage.byte_range.end,
                        utf16: coverage.utf16_range.end,
                    };
                    let expected_logical_end = logical_start.checked_add(logical_metric)?;
                    self.pending = Some(PendingLogicalProgram {
                        coverage,
                        capability: program,
                        next_byte,
                        pieces_remaining: program.piece_count,
                        physical_position: physical_start,
                        expected_physical_end,
                        expected_logical_end,
                    });
                    return self.pending_segment(arena);
                }
            }
        }
    }
}

impl GreenStreamCursor {
    #[must_use]
    pub const fn receipt(&self) -> GreenStreamReceipt {
        self.receipt
    }

    #[must_use]
    pub fn open_path(&self) -> &[GreenOpenFrame] {
        &self.open
    }

    fn active_terminal_frame(&self) -> Option<&GreenOpenFrame> {
        self.active_terminal.and_then(|index| self.open.get(index))
    }

    #[allow(clippy::too_many_lines)] // One cursor transition validates structure, ownership, and projections.
    pub fn next_coverage(
        &mut self,
        document: &SerializedGreenDocument,
        arena: &PageArena,
    ) -> Result<Option<GreenCoverageView>, SerializedGreenError> {
        self.next_coverage_at_bound_manifest(document.manifest_id(), arena)
    }

    /// Bound-root adapter used by composite-held green children. It preserves
    /// the same manifest validation and transition kernel as the owning
    /// document API without manufacturing a second owner.
    fn next_coverage_at_bound_manifest(
        &mut self,
        manifest: SerializedGreenManifestId,
        arena: &PageArena,
    ) -> Result<Option<GreenCoverageView>, SerializedGreenError> {
        if self.manifest != manifest {
            return Err(SerializedGreenError::StaleCursor);
        }
        arena.local_id(self.manifest.scoped())?;
        loop {
            if self.next_event == self.events.len() && !self.advance_leaf(arena)? {
                return Ok(None);
            }
            let decoded = self.events[self.next_event].clone();
            self.next_event += 1;
            match decoded.event {
                DecodedGreenEventKind::Enter { block, kind, facts } => {
                    if self.active_terminal.is_some() {
                        return Err(SerializedGreenError::Corrupt(
                            "terminal block contains another block",
                        ));
                    }
                    let path_index = self.open.len();
                    self.open.push(GreenOpenFrame {
                        block,
                        kind,
                        facts,
                        enter: GreenEnterCapability {
                            manifest: self.manifest,
                            leaf: self.leaf,
                            base_leaf_index: self.leaf_index,
                            byte_offset: decoded.byte_offset,
                            block,
                            kind,
                        },
                    });
                    if kind.logical_channel().is_some() {
                        self.active_terminal = Some(path_index);
                    }
                    self.receipt.maximum_open_depth =
                        self.receipt.maximum_open_depth.max(self.open.len());
                }
                DecodedGreenEventKind::Exit { facts, .. } => {
                    let path_index = self
                        .open
                        .len()
                        .checked_sub(1)
                        .ok_or(SerializedGreenError::Corrupt("viewport stack underflow"))?;
                    let closing_kind = self
                        .open
                        .last()
                        .ok_or(SerializedGreenError::Corrupt("viewport stack underflow"))?
                        .kind;
                    facts.validate_for_kind(closing_kind).map_err(|_| {
                        SerializedGreenError::Corrupt(
                            "close-time facts do not match the closing block kind",
                        )
                    })?;
                    let frame = self
                        .open
                        .pop()
                        .ok_or(SerializedGreenError::Corrupt("viewport stack underflow"))?;
                    if frame.kind.logical_channel().is_some() {
                        if self.active_terminal != Some(path_index) {
                            return Err(SerializedGreenError::Corrupt(
                                "active terminal stack is inconsistent",
                            ));
                        }
                        self.active_terminal = None;
                    }
                }
                DecodedGreenEventKind::Coverage(run) => {
                    let owner_depth = usize::try_from(run.owner_relative_depth)
                        .map_err(|_| SerializedGreenError::Overflow("coverage owner depth"))?;
                    if owner_depth >= self.open.len() {
                        return Err(SerializedGreenError::Corrupt(
                            "coverage owner escapes viewport stack",
                        ));
                    }
                    let owner_index = self.open.len() - 1 - owner_depth;
                    let logical_contribution = match run.logical_contribution {
                        DecodedLogicalContribution::None => LogicalContributionView::None,
                        DecodedLogicalContribution::Identity => LogicalContributionView::Identity {
                            logical_metric: run.metric,
                        },
                        DecodedLogicalContribution::Hidden { affinity } => {
                            LogicalContributionView::Hidden { affinity }
                        }
                        DecodedLogicalContribution::Atomic(projection) => {
                            LogicalContributionView::Atomic { projection }
                        }
                        DecodedLogicalContribution::Program(program) => {
                            LogicalContributionView::Program {
                                logical_metric: program.logical_metric,
                                program: ProjectionProgramCapability {
                                    manifest: self.manifest,
                                    leaf: self.leaf,
                                    page: program.retained_page()?,
                                    edge_ordinal: program.edge_ordinal,
                                    piece_count: program.piece_count,
                                    physical_metric: program.physical_metric,
                                    logical_metric: program.logical_metric,
                                },
                            }
                        }
                    };
                    let (logical_consumer, logical_channel) =
                        if matches!(logical_contribution, LogicalContributionView::None) {
                            (None, None)
                        } else {
                            let terminal_index =
                                self.active_terminal.ok_or(SerializedGreenError::Corrupt(
                                    "logical contribution has no open terminal",
                                ))?;
                            let frame = self.open.get(terminal_index).ok_or(
                                SerializedGreenError::Corrupt(
                                    "active terminal index escapes open path",
                                ),
                            )?;
                            let channel = frame.kind.logical_channel().ok_or(
                                SerializedGreenError::Corrupt(
                                    "active logical consumer is not terminal",
                                ),
                            )?;
                            (
                                Some(GreenOpenCapability::from_frame(frame, terminal_index)?),
                                Some(channel),
                            )
                        };
                    let start = self.position;
                    let end = start.checked_add(run.metric)?;
                    self.position = end;
                    self.receipt.coverage_runs_yielded += 1;
                    return Ok(Some(GreenCoverageView {
                        coverage: run.id,
                        part: run.part,
                        byte_range: start.bytes..end.bytes,
                        utf16_range: start.utf16..end.utf16,
                        owner: GreenOpenCapability::from_frame(
                            &self.open[owner_index],
                            owner_index,
                        )?,
                        logical_consumer,
                        logical_channel,
                        logical_contribution,
                        projection_reset_after: run.projection_reset_after,
                        cursor: GreenCoverageCapability {
                            manifest: self.manifest,
                            leaf: self.leaf,
                            base_leaf_index: self.leaf_index,
                            byte_offset: decoded.byte_offset,
                            coverage: run.id,
                        },
                    }));
                }
            }
        }
    }

    fn advance_leaf(&mut self, arena: &PageArena) -> Result<bool, SerializedGreenError> {
        while let Some(mut frame) = self.route.pop() {
            if frame.went_right {
                continue;
            }
            let (_, SequenceNodeKind::Branch { left, right }) =
                sequence_node::<SerializedGreenSpec>(arena, frame.branch)?
            else {
                return Err(SerializedGreenError::Corrupt("route branch became leaf"));
            };
            let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
            frame.went_right = true;
            let leaf_index = frame
                .base_leaf_index
                .checked_add(left_summary.leaves)
                .ok_or(SerializedGreenError::Overflow("leaf index"))?;
            self.route.push(frame);
            let mut node = right;
            loop {
                self.receipt.sequence_nodes_visited += 1;
                match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
                    SequenceNodeKind::Leaf => {
                        let payload_bytes = arena.payload(node)?.len();
                        let (_, events) = decode_leaf(arena, node)?;
                        self.receipt.leaf_pages_decoded += 1;
                        self.receipt.events_decoded += events.len();
                        self.receipt.maximum_decoded_page_bytes =
                            self.receipt.maximum_decoded_page_bytes.max(
                                payload_bytes
                                    + events.capacity() * std::mem::size_of::<DecodedLeafEvent>(),
                            );
                        self.leaf = node;
                        self.leaf_index = leaf_index;
                        self.events = events;
                        self.next_event = 0;
                        self.receipt.maximum_route_depth =
                            self.receipt.maximum_route_depth.max(self.route.len());
                        return Ok(true);
                    }
                    SequenceNodeKind::Branch { left, .. } => {
                        self.route.push(RouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: false,
                        });
                        node = left;
                    }
                }
            }
        }
        Ok(false)
    }
}

fn retreat_projection_leaf(
    arena: &PageArena,
    route: &mut Vec<RouteFrame>,
    expected_predecessor_index: u64,
    receipt: &mut ProjectionResetSeekReceipt,
) -> Result<Option<(ArenaId, u64, Vec<DecodedLeafEvent>)>, SerializedGreenError> {
    while let Some(mut frame) = route.pop() {
        receipt.sequence_nodes_visited += 1;
        if !frame.went_right {
            continue;
        }
        let (_, SequenceNodeKind::Branch { left, .. }) =
            sequence_node::<SerializedGreenSpec>(arena, frame.branch)?
        else {
            return Err(SerializedGreenError::Corrupt("route branch became leaf"));
        };
        frame.went_right = false;
        let mut node = left;
        let mut leaf_index = frame.base_leaf_index;
        route.push(frame);
        loop {
            receipt.sequence_nodes_visited += 1;
            match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
                SequenceNodeKind::Leaf => {
                    if leaf_index != expected_predecessor_index {
                        return Err(SerializedGreenError::Corrupt(
                            "projection predecessor route changed leaf order",
                        ));
                    }
                    let payload_bytes = arena.payload(node)?.len();
                    let (_, events) = decode_leaf(arena, node)?;
                    receipt.decoded_page_bytes = receipt
                        .decoded_page_bytes
                        .checked_add(payload_bytes)
                        .ok_or(SerializedGreenError::Overflow(
                            "projection reset decoded page bytes",
                        ))?;
                    return Ok(Some((node, leaf_index, events)));
                }
                SequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                    route.push(RouteFrame {
                        branch: node,
                        base_leaf_index: leaf_index,
                        went_right: true,
                    });
                    leaf_index = leaf_index
                        .checked_add(left_summary.leaves)
                        .ok_or(SerializedGreenError::Overflow("leaf index"))?;
                    node = right;
                }
            }
        }
    }
    Ok(None)
}

fn projection_leaf_at_index(
    arena: &PageArena,
    root: ArenaId,
    target_leaf_index: u64,
    receipt: &mut ProjectionResetSeekReceipt,
) -> Result<
    (
        ArenaId,
        SerializedMetric,
        GreenSummary,
        Vec<DecodedLeafEvent>,
    ),
    SerializedGreenError,
> {
    let mut node = root;
    let mut remaining = target_leaf_index;
    let mut base = SerializedMetric::default();
    loop {
        receipt.sequence_nodes_visited =
            receipt
                .sequence_nodes_visited
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "projection reset sequence-node receipt",
                ))?;
        let (summary, kind) = sequence_node::<SerializedGreenSpec>(arena, node)?;
        match kind {
            SequenceNodeKind::Leaf => {
                if remaining != 0 {
                    return Err(SerializedGreenError::StaleCursor);
                }
                let (_, events) = decode_leaf(arena, node)?;
                return Ok((node, base, summary, events));
            }
            SequenceNodeKind::Branch { left, right } => {
                receipt.sequence_nodes_visited =
                    receipt.sequence_nodes_visited.checked_add(1).ok_or(
                        SerializedGreenError::Overflow("projection reset sequence-node receipt"),
                    )?;
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                if remaining < left_summary.leaves {
                    node = left;
                } else {
                    remaining -= left_summary.leaves;
                    base = base.checked_add(left_summary.metric)?;
                    node = right;
                }
            }
        }
    }
}

impl SerializedGreenDocument {
    /// Finds the prior stable projection reset from one exact, storage-derived
    /// adjacent-Coverage observation. This is the restart-path mechanism:
    /// unlike
    /// [`Self::previous_projection_reset`], it never constructs or consumes a
    /// structural green open path.
    ///
    /// The current leaf counts toward `maximum_pages`. Each bounded
    /// predecessor step performs a scalar persistent-sequence descent by leaf
    /// index, decodes one leaf, and retains no route. The caller must expose
    /// this crate-private mechanism only through a role-typed checkpoint
    /// capability. The observation cannot choose among intervening zero-metric
    /// structural events and is neither sequence-cut nor parser-continuation
    /// authority.
    #[allow(clippy::too_many_lines)] // Exact side validation and bounded reverse scan are one audit.
    pub(crate) fn previous_projection_reset_from_observation(
        &self,
        arena: &PageArena,
        observation: &SerializedGreenCoverageSideObservation,
        maximum_pages: usize,
    ) -> Result<ProjectionResetSeekOutcome, SerializedGreenError> {
        if maximum_pages == 0 {
            return Err(SerializedGreenError::Invalid(
                "projection reset search requires a nonzero page bound",
            ));
        }
        let manifest_id = self.local_manifest_id(arena)?;
        let manifest_capability = self.manifest_id();
        let (manifest, root) = decode_document(arena, manifest_id)?;
        let binding = SerializedGreenManifestDescriptor::new(manifest_capability, &manifest);
        if observation.manifest != binding {
            return Err(SerializedGreenError::StaleCursor);
        }

        let mut receipt = ProjectionResetSeekReceipt {
            maximum_pages,
            ..ProjectionResetSeekReceipt::default()
        };
        let (capability, include_coverage) = match observation.adjacent {
            SerializedGreenAdjacentCoverageSide::EmptyDocument {
                manifest: sequence_manifest,
            } => {
                if sequence_manifest != manifest_capability
                    || observation.source_cut != SerializedMetric::default()
                    || manifest.source_bytes != 0
                    || manifest.source_utf16 != 0
                {
                    return Err(SerializedGreenError::StaleCursor);
                }
                return Ok(ProjectionResetSeekOutcome::ImplicitZero {
                    reset: StoredProjectionResetCapability {
                        binding,
                        location: StoredProjectionResetLocation::ImplicitZero,
                        source_end: SerializedMetric::default(),
                    },
                    receipt,
                });
            }
            SerializedGreenAdjacentCoverageSide::BeforeFollowing(capability) => (capability, false),
            SerializedGreenAdjacentCoverageSide::AfterPreceding(capability) => (capability, true),
        };
        if capability.manifest != manifest_capability {
            return Err(SerializedGreenError::StaleCursor);
        }

        let (mut leaf, leaf_base, _leaf_summary, mut events) =
            projection_leaf_at_index(arena, root, capability.base_leaf_index, &mut receipt)?;
        if leaf != capability.leaf {
            return Err(SerializedGreenError::StaleCursor);
        }
        receipt.pages_scanned = 1;
        receipt.decoded_page_bytes = arena.payload(leaf)?.len();

        let mut within = leaf_base;
        let mut event_end = None;
        for (index, decoded) in events.iter().enumerate() {
            receipt.events_inspected =
                receipt
                    .events_inspected
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow(
                        "projection reset event receipt",
                    ))?;
            match &decoded.event {
                DecodedGreenEventKind::Coverage(run) => {
                    let end = within.checked_add(run.metric)?;
                    if decoded.byte_offset == capability.byte_offset {
                        if run.id != capability.coverage {
                            return Err(SerializedGreenError::StaleCursor);
                        }
                        let expected_cut = if include_coverage { end } else { within };
                        if observation.source_cut != expected_cut {
                            return Err(SerializedGreenError::StaleCursor);
                        }
                        event_end = Some(if include_coverage { index + 1 } else { index });
                        break;
                    }
                    within = end;
                }
                DecodedGreenEventKind::Enter { .. } | DecodedGreenEventKind::Exit { .. } => {
                    if decoded.byte_offset == capability.byte_offset {
                        return Err(SerializedGreenError::Corrupt(
                            "storage boundary points to a structural event",
                        ));
                    }
                }
            }
        }
        let mut event_end = event_end.ok_or(SerializedGreenError::StaleCursor)?;
        let mut leaf_index = capability.base_leaf_index;
        let mut source_end = observation.source_cut;

        loop {
            for decoded in events[..event_end].iter().rev() {
                receipt.events_inspected = receipt.events_inspected.checked_add(1).ok_or(
                    SerializedGreenError::Overflow("projection reset event receipt"),
                )?;
                if let DecodedGreenEventKind::Coverage(run) = &decoded.event {
                    if run.projection_reset_after {
                        return Ok(ProjectionResetSeekOutcome::Found {
                            reset: StoredProjectionResetCapability {
                                binding,
                                location: StoredProjectionResetLocation::Run {
                                    leaf,
                                    leaf_index,
                                    event_byte_offset: decoded.byte_offset,
                                    coverage: run.id,
                                },
                                source_end,
                            },
                            receipt,
                        });
                    }
                    source_end = source_end.checked_sub(run.metric)?;
                }
            }

            if leaf_index == 0 {
                if source_end != SerializedMetric::default() {
                    return Err(SerializedGreenError::Corrupt(
                        "projection reset reverse scan did not reach source zero",
                    ));
                }
                return Ok(ProjectionResetSeekOutcome::ImplicitZero {
                    reset: StoredProjectionResetCapability {
                        binding,
                        location: StoredProjectionResetLocation::ImplicitZero,
                        source_end,
                    },
                    receipt,
                });
            }
            if receipt.pages_scanned == maximum_pages {
                return Ok(ProjectionResetSeekOutcome::NotFoundWithinBound(receipt));
            }

            leaf_index -= 1;
            let (previous_leaf, previous_base, previous_summary, previous_events) =
                projection_leaf_at_index(arena, root, leaf_index, &mut receipt)?;
            if previous_base.checked_add(previous_summary.metric)? != source_end {
                return Err(SerializedGreenError::Corrupt(
                    "projection predecessor leaf does not meet the source cut",
                ));
            }
            leaf = previous_leaf;
            events = previous_events;
            event_end = events.len();
            receipt.pages_scanned += 1;
            receipt.predecessor_pages += 1;
            receipt.decoded_page_bytes = receipt
                .decoded_page_bytes
                .checked_add(arena.payload(leaf)?.len())
                .ok_or(SerializedGreenError::Overflow(
                    "projection reset decoded page bytes",
                ))?;
        }
    }

    /// Finds the prior stable projection reset at or before this cursor's
    /// exact source boundary. The cursor is produced by `seek`/streaming and
    /// consumed here, so callers cannot substitute a raw source offset.
    ///
    /// The current page counts toward `maximum_pages`. Further work walks only
    /// predecessor leaves through the existing persistent-sequence route. It
    /// never builds or consults a document-wide reset directory.
    pub fn previous_projection_reset(
        &self,
        arena: &PageArena,
        cursor: GreenStreamCursor,
        maximum_pages: usize,
    ) -> Result<ProjectionResetSeekOutcome, SerializedGreenError> {
        if maximum_pages == 0 {
            return Err(SerializedGreenError::Invalid(
                "projection reset search requires a nonzero page bound",
            ));
        }
        let manifest_id = self.local_manifest_id(arena)?;
        let manifest_capability = self.manifest_id();
        if cursor.manifest != manifest_capability {
            return Err(SerializedGreenError::StaleCursor);
        }
        let (manifest, _) = decode_document(arena, manifest_id)?;
        let binding = SerializedGreenManifestDescriptor::new(manifest_capability, &manifest);
        if cursor.next_event > cursor.events.len() {
            return Err(SerializedGreenError::Corrupt(
                "projection cursor event position exceeds leaf",
            ));
        }

        let mut receipt = ProjectionResetSeekReceipt {
            maximum_pages,
            pages_scanned: 1,
            decoded_page_bytes: arena.payload(cursor.leaf)?.len(),
            ..ProjectionResetSeekReceipt::default()
        };
        let mut route = cursor.route;
        let mut leaf = cursor.leaf;
        let mut leaf_index = cursor.leaf_index;
        let mut events = cursor.events;
        let mut event_end = cursor.next_event;
        let mut source_end = cursor.position;

        loop {
            for decoded in events[..event_end].iter().rev() {
                receipt.events_inspected = receipt.events_inspected.checked_add(1).ok_or(
                    SerializedGreenError::Overflow("projection reset event receipt"),
                )?;
                if let DecodedGreenEventKind::Coverage(run) = &decoded.event {
                    if run.projection_reset_after {
                        return Ok(ProjectionResetSeekOutcome::Found {
                            reset: StoredProjectionResetCapability {
                                binding,
                                location: StoredProjectionResetLocation::Run {
                                    leaf,
                                    leaf_index,
                                    event_byte_offset: decoded.byte_offset,
                                    coverage: run.id,
                                },
                                source_end,
                            },
                            receipt,
                        });
                    }
                    source_end = source_end.checked_sub(run.metric)?;
                }
            }

            if leaf_index == 0 {
                if source_end != SerializedMetric::default() {
                    return Err(SerializedGreenError::Corrupt(
                        "projection reset reverse scan did not reach source zero",
                    ));
                }
                return Ok(ProjectionResetSeekOutcome::ImplicitZero {
                    reset: StoredProjectionResetCapability {
                        binding,
                        location: StoredProjectionResetLocation::ImplicitZero,
                        source_end,
                    },
                    receipt,
                });
            }
            if receipt.pages_scanned == maximum_pages {
                return Ok(ProjectionResetSeekOutcome::NotFoundWithinBound(receipt));
            }

            let predecessor_index = leaf_index - 1;
            let Some((previous_leaf, previous_index, previous_events)) =
                retreat_projection_leaf(arena, &mut route, predecessor_index, &mut receipt)?
            else {
                return Err(SerializedGreenError::Corrupt(
                    "projection route lost an existing predecessor",
                ));
            };
            leaf = previous_leaf;
            leaf_index = previous_index;
            events = previous_events;
            event_end = events.len();
            receipt.pages_scanned += 1;
            receipt.predecessor_pages += 1;
        }
    }

    /// Revalidates every manifest/source/leaf/event/coverage field carried by
    /// stored reset authority. A valid reset remains storage authority only;
    /// this operation does not manufacture parser continuation state.
    pub fn resolve_projection_reset(
        &self,
        arena: &PageArena,
        reset: &StoredProjectionResetCapability,
    ) -> Result<ResolvedProjectionReset, SerializedGreenError> {
        let manifest_capability = self.manifest_id();
        if reset.binding.manifest != manifest_capability {
            return Err(SerializedGreenError::StaleCursor);
        }
        let manifest_id = self.local_manifest_id(arena)?;
        let (manifest, root) = decode_document(arena, manifest_id)?;
        if !reset.binding.matches(manifest_capability, &manifest) {
            return Err(SerializedGreenError::StaleCursor);
        }
        match reset.location {
            StoredProjectionResetLocation::ImplicitZero => {
                if reset.source_end != SerializedMetric::default() {
                    return Err(SerializedGreenError::Corrupt(
                        "implicit projection reset is not source zero",
                    ));
                }
                Ok(ResolvedProjectionReset {
                    source_end: reset.source_end,
                    coverage: None,
                    implicit_zero: true,
                })
            }
            StoredProjectionResetLocation::Run {
                leaf,
                leaf_index,
                event_byte_offset,
                coverage,
            } => {
                let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
                if leaf_index >= root_summary.leaves {
                    return Err(SerializedGreenError::StaleCursor);
                }
                let mut node = root;
                let mut remaining = leaf_index;
                let mut base = SerializedMetric::default();
                loop {
                    match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
                        SequenceNodeKind::Leaf => break,
                        SequenceNodeKind::Branch { left, right } => {
                            let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                            if remaining < left_summary.leaves {
                                node = left;
                            } else {
                                remaining -= left_summary.leaves;
                                base = base.checked_add(left_summary.metric)?;
                                node = right;
                            }
                        }
                    }
                }
                if node != leaf || remaining != 0 {
                    return Err(SerializedGreenError::StaleCursor);
                }
                let (_, events) = decode_leaf(arena, node)?;
                let mut position = base;
                for decoded in events {
                    match decoded.event {
                        DecodedGreenEventKind::Coverage(run) => {
                            let end = position.checked_add(run.metric)?;
                            if decoded.byte_offset == event_byte_offset {
                                if run.id != coverage
                                    || !run.projection_reset_after
                                    || end != reset.source_end
                                {
                                    return Err(SerializedGreenError::Corrupt(
                                        "stored projection reset event changed",
                                    ));
                                }
                                return Ok(ResolvedProjectionReset {
                                    source_end: end,
                                    coverage: Some(coverage),
                                    implicit_zero: false,
                                });
                            }
                            position = end;
                        }
                        DecodedGreenEventKind::Enter { .. }
                        | DecodedGreenEventKind::Exit { .. } => {
                            if decoded.byte_offset == event_byte_offset {
                                return Err(SerializedGreenError::Corrupt(
                                    "stored projection reset points to a structural event",
                                ));
                            }
                        }
                    }
                }
                Err(SerializedGreenError::Corrupt(
                    "stored projection reset event is missing",
                ))
            }
        }
    }

    pub fn seek(
        &self,
        arena: &PageArena,
        coordinate: GreenCoordinate,
        offset: u64,
        affinity: GreenAffinity,
    ) -> Result<GreenStreamCursor, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let manifest_capability = self.manifest_id();
        let (manifest, root) = decode_document(arena, manifest_id)?;
        stream_at_bound_root(
            arena,
            manifest_id,
            manifest_capability,
            &manifest,
            root,
            coordinate,
            offset,
            affinity,
        )
    }

    pub fn logical_cursor(
        &self,
        arena: &PageArena,
        target: GreenEnterCapability,
    ) -> Result<GreenLogicalCursor, SerializedGreenError> {
        let channel = target
            .kind
            .logical_channel()
            .ok_or(SerializedGreenError::Invalid(
                "logical cursor target is not a terminal kind",
            ))?;
        let stream = self.stream_from_enter(arena, target)?;
        let target_frame = match stream
            .events
            .get(stream.next_event)
            .map(|event| &event.event)
        {
            Some(DecodedGreenEventKind::Enter { block, kind, facts })
                if *block == target.block && *kind == target.kind =>
            {
                GreenOpenFrame {
                    block: *block,
                    kind: *kind,
                    facts: facts.clone(),
                    enter: target,
                }
            }
            Some(
                DecodedGreenEventKind::Enter { .. }
                | DecodedGreenEventKind::Exit { .. }
                | DecodedGreenEventKind::Coverage(_),
            )
            | None => {
                return Err(SerializedGreenError::Corrupt(
                    "logical cursor target event changed after validation",
                ));
            }
        };
        Ok(GreenLogicalCursor {
            target,
            target_frame,
            channel,
            stream,
            logical_position: SerializedMetric::default(),
            pending: None,
            receipt: GreenLogicalReceipt::default(),
        })
    }

    #[allow(clippy::too_many_lines)] // Exact capability routing and reverse path recovery are one audit unit.
    fn stream_from_enter(
        &self,
        arena: &PageArena,
        target: GreenEnterCapability,
    ) -> Result<GreenStreamCursor, SerializedGreenError> {
        if target.manifest != self.manifest_id() {
            return Err(SerializedGreenError::StaleCursor);
        }
        let manifest_id = arena.local_id(target.manifest.scoped())?;
        let (_, root) = decode_document(arena, manifest_id)?;
        let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
        if target.base_leaf_index >= root_summary.leaves {
            return Err(SerializedGreenError::StaleCursor);
        }
        let mut receipt = GreenStreamReceipt {
            root_descents: 1,
            ..GreenStreamReceipt::default()
        };
        let mut node = root;
        let mut remaining_leaf_index = target.base_leaf_index;
        let mut leaf_index = 0_u64;
        let mut base = SerializedMetric::default();
        let mut route = Vec::new();
        loop {
            receipt.sequence_nodes_visited += 1;
            match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
                SequenceNodeKind::Leaf => break,
                SequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                    if remaining_leaf_index < left_summary.leaves {
                        route.push(RouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: false,
                        });
                        node = left;
                    } else {
                        route.push(RouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: true,
                        });
                        remaining_leaf_index -= left_summary.leaves;
                        leaf_index = leaf_index
                            .checked_add(left_summary.leaves)
                            .ok_or(SerializedGreenError::Overflow("leaf index"))?;
                        base = base.checked_add(left_summary.metric)?;
                        node = right;
                    }
                }
            }
        }
        if node != target.leaf || leaf_index != target.base_leaf_index {
            return Err(SerializedGreenError::StaleCursor);
        }
        let payload_bytes = arena.payload(node)?.len();
        let (_, events) = decode_leaf(arena, node)?;
        receipt.leaf_pages_decoded += 1;
        receipt.events_decoded += events.len();
        receipt.maximum_decoded_page_bytes =
            payload_bytes + events.capacity() * std::mem::size_of::<DecodedLeafEvent>();
        receipt.maximum_route_depth = route.len();
        let selected = events
            .iter()
            .position(|decoded| {
                decoded.byte_offset == target.byte_offset
                    && matches!(
                        decoded.event,
                        DecodedGreenEventKind::Enter { block, kind, .. }
                            if block == target.block && kind == target.kind
                    )
            })
            .ok_or(SerializedGreenError::StaleCursor)?;
        let within = events[..selected].iter().try_fold(
            SerializedMetric::default(),
            |metric, decoded| match &decoded.event {
                DecodedGreenEventKind::Coverage(run) => metric.checked_add(run.metric),
                DecodedGreenEventKind::Enter { .. } | DecodedGreenEventKind::Exit { .. } => {
                    Ok(metric)
                }
            },
        )?;
        let position = base.checked_add(within)?;
        let mut inner_first = Vec::new();
        let mut unmatched_exits = 0_u64;
        scan_events_reverse(
            target.manifest,
            node,
            leaf_index,
            &events[..selected],
            &mut unmatched_exits,
            &mut inner_first,
            &mut receipt,
        )?;
        for frame in route.iter().rev() {
            if frame.went_right {
                let (_, SequenceNodeKind::Branch { left, .. }) =
                    sequence_node::<SerializedGreenSpec>(arena, frame.branch)?
                else {
                    return Err(SerializedGreenError::Corrupt("route branch became leaf"));
                };
                scan_node_reverse(
                    arena,
                    target.manifest,
                    left,
                    frame.base_leaf_index,
                    &mut unmatched_exits,
                    &mut inner_first,
                    &mut receipt,
                )?;
            }
        }
        if unmatched_exits != 0 {
            return Err(SerializedGreenError::Corrupt(
                "terminal Enter follows unmatched Exit",
            ));
        }
        inner_first.reverse();
        let active_terminal = derive_active_terminal(&inner_first)?;
        receipt.maximum_open_depth = inner_first.len();
        Ok(GreenStreamCursor {
            manifest: target.manifest,
            route,
            leaf: node,
            leaf_index,
            events,
            next_event: selected,
            position,
            open: inner_first,
            active_terminal,
            receipt,
        })
    }
}

/// The source-coordinate stream primitive shared by an owning green document
/// and an authenticated composite-held green root. The latter must not forge
/// a temporary `SerializedGreenDocument` merely to traverse a borrowed child.
/// Descent is logarithmic in leaf count; reverse structural reconstruction
/// summary-skips closed subtrees and decodes only the open-path frontier.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn stream_at_bound_root(
    arena: &PageArena,
    manifest_id: ArenaId,
    manifest_capability: SerializedGreenManifestId,
    manifest: &Manifest,
    root: ArenaId,
    coordinate: GreenCoordinate,
    offset: u64,
    affinity: GreenAffinity,
) -> Result<GreenStreamCursor, SerializedGreenError> {
    if arena.scoped_query_id(manifest_id)? != manifest_capability.scoped() {
        return Err(SerializedGreenError::StaleCursor);
    }
    let total = manifest.summary.metric.coordinate(coordinate);
    if total == 0 || offset > total {
        return Err(SerializedGreenError::SourceOutOfBounds);
    }
    let probe = match affinity {
        GreenAffinity::Upstream if offset > 0 => offset - 1,
        GreenAffinity::Downstream if offset == total => total - 1,
        GreenAffinity::Upstream | GreenAffinity::Downstream => offset,
    };
    let mut receipt = GreenStreamReceipt {
        root_descents: 1,
        ..GreenStreamReceipt::default()
    };
    let mut node = root;
    let mut local_probe = probe;
    let mut base = SerializedMetric::default();
    let mut leaf_index = 0_u64;
    let mut route = Vec::new();
    loop {
        receipt.sequence_nodes_visited += 1;
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => break,
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                let left_length = left_summary.metric.coordinate(coordinate);
                if local_probe < left_length {
                    route.push(RouteFrame {
                        branch: node,
                        base_leaf_index: leaf_index,
                        went_right: false,
                    });
                    node = left;
                } else {
                    route.push(RouteFrame {
                        branch: node,
                        base_leaf_index: leaf_index,
                        went_right: true,
                    });
                    local_probe -= left_length;
                    base = base.checked_add(left_summary.metric)?;
                    leaf_index = leaf_index
                        .checked_add(left_summary.leaves)
                        .ok_or(SerializedGreenError::Overflow("leaf index"))?;
                    node = right;
                }
            }
        }
    }
    let payload_bytes = arena.payload(node)?.len();
    let (_, events) = decode_leaf(arena, node)?;
    receipt.leaf_pages_decoded += 1;
    receipt.events_decoded += events.len();
    receipt.maximum_decoded_page_bytes =
        payload_bytes + events.capacity() * std::mem::size_of::<DecodedLeafEvent>();
    receipt.maximum_route_depth = route.len();
    let mut within = SerializedMetric::default();
    let mut selected = None;
    for (index, decoded) in events.iter().enumerate() {
        if let DecodedGreenEventKind::Coverage(run) = &decoded.event {
            let end = within.checked_add(run.metric)?;
            if local_probe < end.coordinate(coordinate) {
                selected = Some((index, base.checked_add(within)?));
                break;
            }
            within = end;
        }
    }
    let (selected, position) = selected.ok_or(SerializedGreenError::Corrupt(
        "source descent leaf has no matching coverage",
    ))?;
    let mut inner_first = Vec::new();
    let mut unmatched_exits = 0_u64;
    scan_events_reverse(
        manifest_capability,
        node,
        leaf_index,
        &events[..selected],
        &mut unmatched_exits,
        &mut inner_first,
        &mut receipt,
    )?;
    for frame in route.iter().rev() {
        if frame.went_right {
            let (_, SequenceNodeKind::Branch { left, .. }) =
                sequence_node::<SerializedGreenSpec>(arena, frame.branch)?
            else {
                return Err(SerializedGreenError::Corrupt("route branch became leaf"));
            };
            scan_node_reverse(
                arena,
                manifest_capability,
                left,
                frame.base_leaf_index,
                &mut unmatched_exits,
                &mut inner_first,
                &mut receipt,
            )?;
        }
    }
    if unmatched_exits != 0 {
        return Err(SerializedGreenError::Corrupt(
            "source position follows unmatched Exit",
        ));
    }
    inner_first.reverse();
    let active_terminal = derive_active_terminal(&inner_first)?;
    receipt.maximum_open_depth = inner_first.len();
    Ok(GreenStreamCursor {
        manifest: manifest_capability,
        route,
        leaf: node,
        leaf_index,
        events,
        next_event: selected,
        position,
        open: inner_first,
        active_terminal,
        receipt,
    })
}

fn scan_events_reverse(
    manifest: SerializedGreenManifestId,
    leaf: ArenaId,
    leaf_index: u64,
    events: &[DecodedLeafEvent],
    unmatched_exits: &mut u64,
    output: &mut Vec<GreenOpenFrame>,
    receipt: &mut GreenStreamReceipt,
) -> Result<(), SerializedGreenError> {
    for decoded in events.iter().rev() {
        match &decoded.event {
            DecodedGreenEventKind::Exit { .. } => {
                *unmatched_exits = unmatched_exits
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow("reverse Exit count"))?;
            }
            DecodedGreenEventKind::Enter { block, kind, facts } => {
                if *unmatched_exits != 0 {
                    *unmatched_exits -= 1;
                } else {
                    output.push(GreenOpenFrame {
                        block: *block,
                        kind: *kind,
                        facts: facts.clone(),
                        enter: GreenEnterCapability {
                            manifest,
                            leaf,
                            base_leaf_index: leaf_index,
                            byte_offset: decoded.byte_offset,
                            block: *block,
                            kind: *kind,
                        },
                    });
                }
            }
            DecodedGreenEventKind::Coverage(_) => {}
        }
    }
    receipt.maximum_open_depth = receipt.maximum_open_depth.max(output.len());
    Ok(())
}

fn scan_node_reverse(
    arena: &PageArena,
    manifest: SerializedGreenManifestId,
    node: ArenaId,
    base_leaf_index: u64,
    unmatched_exits: &mut u64,
    output: &mut Vec<GreenOpenFrame>,
    receipt: &mut GreenStreamReceipt,
) -> Result<(), SerializedGreenError> {
    receipt.sequence_nodes_visited += 1;
    let (summary, kind) = sequence_node::<SerializedGreenSpec>(arena, node)?;
    let (opens, closes) = summary.unmatched()?;
    if opens <= *unmatched_exits {
        *unmatched_exits = unmatched_exits
            .checked_sub(opens)
            .and_then(|remaining| remaining.checked_add(closes))
            .ok_or(SerializedGreenError::Overflow("reverse structural count"))?;
        receipt.summary_nodes_skipped += 1;
        return Ok(());
    }
    match kind {
        SequenceNodeKind::Leaf => {
            let payload_bytes = arena.payload(node)?.len();
            let (_, events) = decode_leaf(arena, node)?;
            receipt.leaf_pages_decoded += 1;
            receipt.events_decoded += events.len();
            receipt.maximum_decoded_page_bytes = receipt
                .maximum_decoded_page_bytes
                .max(payload_bytes + events.capacity() * std::mem::size_of::<DecodedLeafEvent>());
            scan_events_reverse(
                manifest,
                node,
                base_leaf_index,
                &events,
                unmatched_exits,
                output,
                receipt,
            )
        }
        SequenceNodeKind::Branch { left, right } => {
            let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
            let right_base = base_leaf_index
                .checked_add(left_summary.leaves)
                .ok_or(SerializedGreenError::Overflow("leaf index"))?;
            scan_node_reverse(
                arena,
                manifest,
                right,
                right_base,
                unmatched_exits,
                output,
                receipt,
            )?;
            scan_node_reverse(
                arena,
                manifest,
                left,
                base_leaf_index,
                unmatched_exits,
                output,
                receipt,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold_events(events: &[GreenEvent]) -> GreenSummary {
        events
            .iter()
            .try_fold(GreenSummary::default(), |summary, event| {
                summary.followed_by(GreenSummary::event(event))
            })
            .unwrap()
    }

    fn closed(bits: u8) -> ClosedChildAggregate {
        decode_closed(bits)
    }

    #[test]
    fn table_enter_facts_are_typed_scalable_and_keep_alignment_on_header_cells() {
        let table = GreenTableOpenFacts::new(3).unwrap();
        assert_eq!(table.column_count(), 3);
        assert_eq!(
            GreenTableOpenFacts::try_from_envelope(&table.into_envelope()),
            Ok(table)
        );
        assert_eq!(
            GreenTableOpenFacts::new(0),
            Err(SerializedGreenError::Invalid(
                "Table column count must be nonzero"
            ))
        );

        let header = GreenTableRowOpenFacts::header();
        let body = GreenTableRowOpenFacts::body();
        assert!(header.is_header());
        assert!(!body.is_header());
        assert_eq!(
            GreenTableRowOpenFacts::try_from_envelope(&header.into_envelope()),
            Ok(header)
        );
        assert_eq!(
            GreenTableRowOpenFacts::try_from_envelope(&body.into_envelope()),
            Ok(body)
        );

        let far_cell = GreenTableCellOpenFacts::body(u32::MAX);
        assert_eq!(far_cell.column_index(), u32::MAX);
        assert_eq!(far_cell.header_alignment(), None);
        assert_eq!(
            GreenTableCellOpenFacts::try_from_envelope(&far_cell.into_envelope()),
            Ok(far_cell)
        );
        for alignment in [
            GreenTableAlignment::Unspecified,
            GreenTableAlignment::Left,
            GreenTableAlignment::Center,
            GreenTableAlignment::Right,
        ] {
            let header_cell = GreenTableCellOpenFacts::header(2, alignment);
            assert_eq!(header_cell.header_alignment(), Some(alignment));
            assert_eq!(
                GreenTableCellOpenFacts::try_from_envelope(&header_cell.into_envelope()),
                Ok(header_cell)
            );
        }

        let inline_alignments = FactsEnvelope::new(vec![
            FactField::critical(FactId::TABLE, 3_u32.to_le_bytes()),
            FactField::critical(FactId::TABLE_ALIGNMENTS, [1_u8, 2, 3]),
        ])
        .unwrap();
        assert_eq!(
            validate_facts_for_kind(GreenKind::TABLE, &inline_alignments),
            Err(SerializedGreenError::Invalid(
                "fact is incompatible with block kind"
            ))
        );
    }

    #[test]
    fn close_time_facts_and_intrinsic_blank_use_distinct_one_byte_exit_tags() {
        let closed = closed(0b101);
        let cases = [
            (GreenCloseFacts::None, EXIT_TAG, EXIT_LAST_LINE_BLANK_TAG),
            (
                GreenCloseFacts::List { tight: false },
                EXIT_LIST_LOOSE_TAG,
                EXIT_LIST_LOOSE_LAST_LINE_BLANK_TAG,
            ),
            (
                GreenCloseFacts::List { tight: true },
                EXIT_LIST_TIGHT_TAG,
                EXIT_LIST_TIGHT_LAST_LINE_BLANK_TAG,
            ),
        ];
        let mut arena = PageArena::new();
        let dummy = arena.allocate(b"Exit-codec-dummy", &[]).unwrap();
        for (facts, false_tag, true_tag) in cases {
            for (last_line_blank, expected_tag) in [(false, false_tag), (true, true_tag)] {
                let event = GreenEvent::exit_with_state(closed, last_line_blank, facts);
                let encoded = encode_event(&event, 0).unwrap();
                assert_eq!(encoded.bytes, [expected_tag | encode_closed(closed)]);

                let mut decoder = Decoder::new(&encoded.bytes);
                let mut next_program_ordinal = 0;
                assert_eq!(
                    decode_event(
                        &mut decoder,
                        &arena,
                        dummy.owner.id(),
                        &mut next_program_ordinal,
                    )
                    .unwrap(),
                    DecodedGreenEventKind::Exit {
                        closed,
                        last_line_blank,
                        facts,
                    }
                );
                assert!(decoder.is_empty());
            }
        }

        let loose = GreenSummary::event(&GreenEvent::exit_with_facts(
            closed,
            GreenCloseFacts::List { tight: false },
        ));
        let tight = GreenSummary::event(&GreenEvent::exit_with_facts(
            closed,
            GreenCloseFacts::List { tight: true },
        ));
        assert_eq!(loose, tight, "tightness must not enter structural folds");
        assert_eq!(
            loose,
            GreenSummary::event(&GreenEvent::exit_with_state(
                closed,
                true,
                GreenCloseFacts::List { tight: false },
            )),
            "intrinsic blank truth must not replace the derived structural fold",
        );

        let mut unknown = Decoder::new(&[0x58]);
        let mut next_program_ordinal = 0;
        assert_eq!(
            decode_event(
                &mut unknown,
                &arena,
                dummy.owner.id(),
                &mut next_program_ordinal,
            ),
            Err(SerializedGreenError::Corrupt("unknown packed event tag"))
        );
        arena.release_later(dummy.owner).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn fenced_code_close_facts_have_a_canonical_bounded_exit_payload() {
        let closed_children = closed(0b101);
        let facts = GreenFencedCodeCloseFacts::new(
            true,
            GreenRelativeLogicalSlice::new(2..6, 2..6).unwrap(),
            GreenRelativeLogicalSlice::new(7..u64::MAX, 7..u64::MAX).unwrap(),
        )
        .unwrap();
        let event =
            GreenEvent::exit_with_facts(closed_children, GreenCloseFacts::FencedCode(facts));
        let encoded = encode_event(&event, 0).unwrap();
        assert_eq!(
            encoded.bytes[0],
            EXIT_FENCED_CODE_TAG | encode_closed(closed_children)
        );

        let mut arena = PageArena::new();
        let dummy = arena.allocate(b"fence-exit-codec-dummy", &[]).unwrap();
        let mut decoder = Decoder::new(&encoded.bytes);
        let mut next_program_ordinal = 0;
        assert_eq!(
            decode_event(
                &mut decoder,
                &arena,
                dummy.owner.id(),
                &mut next_program_ordinal,
            )
            .unwrap(),
            DecodedGreenEventKind::Exit {
                closed: closed_children,
                last_line_blank: false,
                facts: GreenCloseFacts::FencedCode(facts),
            }
        );
        assert!(decoder.is_empty());

        let structural = GreenSummary::event(&GreenEvent::exit(closed_children));
        assert_eq!(
            structural,
            GreenSummary::event(&event),
            "FencedCode projection facts must not enter structural folds"
        );

        let corrupt_cases = [
            (
                vec![EXIT_FENCED_CODE_TAG, 2],
                SerializedGreenError::Corrupt("invalid FencedCode closed flag"),
            ),
            (
                vec![EXIT_FENCED_CODE_TAG, 1, 0x80, 0],
                SerializedGreenError::Corrupt("nonminimal varint"),
            ),
            (
                vec![EXIT_FENCED_CODE_TAG, 1, 0],
                SerializedGreenError::Corrupt("truncated packed event"),
            ),
        ];
        for (bytes, expected) in corrupt_cases {
            let mut decoder = Decoder::new(&bytes);
            let mut next_program_ordinal = 0;
            assert_eq!(
                decode_event(
                    &mut decoder,
                    &arena,
                    dummy.owner.id(),
                    &mut next_program_ordinal,
                ),
                Err(expected)
            );
        }

        // Canonical metrics, but info end is after literal start.
        let reversed_order = [EXIT_FENCED_CODE_TAG, 1, 0, 0, 4, 4, 3, 3, 5, 5];
        let mut decoder = Decoder::new(&reversed_order);
        let mut next_program_ordinal = 0;
        assert_eq!(
            decode_event(
                &mut decoder,
                &arena,
                dummy.owner.id(),
                &mut next_program_ordinal,
            ),
            Err(SerializedGreenError::Corrupt(
                "invalid FencedCode slice ordering"
            ))
        );

        arena.release_later(dummy.owner).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn fenced_code_intrinsic_blank_uses_the_mirrored_payload_tag() {
        let closed_children = closed(0b101);
        let facts = GreenFencedCodeCloseFacts::new(
            true,
            GreenRelativeLogicalSlice::new(2..6, 2..6).unwrap(),
            GreenRelativeLogicalSlice::new(7..11, 7..11).unwrap(),
        )
        .unwrap();
        let event =
            GreenEvent::exit_with_state(closed_children, true, GreenCloseFacts::FencedCode(facts));
        let encoded = encode_event(&event, 0).unwrap();
        assert_eq!(
            encoded.bytes[0],
            EXIT_FENCED_CODE_LAST_LINE_BLANK_TAG | encode_closed(closed_children),
        );

        let mut arena = PageArena::new();
        let dummy = arena
            .allocate(b"blank-fence-exit-codec-dummy", &[])
            .unwrap();
        let mut decoder = Decoder::new(&encoded.bytes);
        let mut next_program_ordinal = 0;
        assert_eq!(
            decode_event(
                &mut decoder,
                &arena,
                dummy.owner.id(),
                &mut next_program_ordinal,
            )
            .unwrap(),
            DecodedGreenEventKind::Exit {
                closed: closed_children,
                last_line_blank: true,
                facts: GreenCloseFacts::FencedCode(facts),
            },
        );
        assert!(decoder.is_empty());
        arena.release_later(dummy.owner).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn close_time_fold_selects_only_direct_children_across_every_split() {
        for first in 0..8 {
            for second in 0..8 {
                for third in 0..8 {
                    let contributions = [closed(first), closed(second), closed(third)];
                    let mut events = Vec::new();
                    for (index, contribution) in contributions.into_iter().enumerate() {
                        events.push(GreenEvent::enter(
                            BlockId(u64::try_from(index * 2 + 1).unwrap()),
                            GreenKind::BLOCK_QUOTE,
                            FactsEnvelope::empty(),
                        ));
                        events.push(GreenEvent::enter(
                            BlockId(u64::try_from(index * 2 + 2).unwrap()),
                            GreenKind::PARAGRAPH,
                            FactsEnvelope::empty(),
                        ));
                        events.push(GreenEvent::exit(ClosedChildAggregate::default()));
                        events.push(GreenEvent::exit(contribution));
                    }
                    let expected = contributions.into_iter().fold(
                        ChildSequenceAggregate::default(),
                        |summary, contribution| {
                            summary.followed_by(ChildSequenceAggregate::singleton(contribution))
                        },
                    );
                    let complete = fold_events(&events);
                    assert_eq!(complete.balance, 0);
                    assert_eq!(complete.minimum_prefix, 0);
                    assert_eq!(complete.minimum_closed_depth, Some(0));
                    assert_eq!(complete.outermost, expected);
                    for split in 0..=events.len() {
                        let recomposed = fold_events(&events[..split])
                            .followed_by(fold_events(&events[split..]))
                            .unwrap();
                        assert_eq!(
                            recomposed, complete,
                            "bits={first}/{second}/{third} split={split}"
                        );
                    }
                }
            }
        }
    }

    fn one_piece_program() -> ProjectionProgram {
        ProjectionProgram::new(vec![ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        }])
        .unwrap()
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1_000).unwrap();
        }
    }

    fn resumable_spec(bytes: u64) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(1),
            source_root: SourceRootId(1),
            source_bytes: bytes,
            source_utf16: bytes,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(1),
            semantic_epoch: 1,
            known_bytes: 0..bytes,
        }
    }

    const CAPACITY_CLIFF_RUNS: u64 = 331;
    const CAPACITY_CLIFF_WIDE_RUNS: u64 = 8;

    fn capacity_cliff_metric() -> SerializedMetric {
        SerializedMetric {
            bytes: CAPACITY_CLIFF_RUNS + CAPACITY_CLIFF_WIDE_RUNS,
            utf16: CAPACITY_CLIFF_RUNS,
        }
    }

    fn capacity_cliff_spec() -> SerializedGreenRootSpec {
        let metric = capacity_cliff_metric();
        let mut spec = resumable_spec(metric.bytes);
        spec.source_utf16 = metric.utf16;
        spec
    }

    fn test_coverage(id: u64, target: BlockId) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(id),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                target,
                LogicalContribution::Identity,
            )
            .unwrap(),
        )
    }

    fn capacity_cliff_coverage(id: u64, target: BlockId) -> GreenEvent {
        if id <= CAPACITY_CLIFF_WIDE_RUNS {
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(id),
                    2,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    target,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            )
        } else {
            test_coverage(id, target)
        }
    }

    fn poll_builder_to_event_boundary(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        loop {
            let before = build.receipt().resumable_arena_allocations;
            let progress = build.poll(session).unwrap();
            let after = build.receipt().resumable_arena_allocations;
            assert!(
                after - before <= 1,
                "one green poll allocated more than once"
            );
            match progress {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("event boundary unexpectedly finalized the manifest")
                }
            }
        }
    }

    fn offer_test_event(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        build.offer_event(session, event).unwrap();
        poll_builder_to_event_boundary(build, session);
    }

    fn offer_provisional_test_paragraph(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        block: BlockId,
    ) -> ProvisionalParagraphEnter {
        build
            .offer_provisional_paragraph_enter(session, block, FactsEnvelope::empty())
            .unwrap();
        poll_builder_to_event_boundary(build, session);
        build
            .take_provisional_paragraph_enter(session, block)
            .unwrap()
    }

    fn promote_test_setext(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        token: ProvisionalParagraphEnter,
        block: BlockId,
        level: u8,
    ) -> SetextPromotion {
        build
            .begin_setext_promotion(
                session,
                token,
                GreenHeadingOpenFacts::setext(level).unwrap(),
            )
            .unwrap();
        poll_builder_to_event_boundary(build, session);
        build.take_setext_promotion(session, block).unwrap()
    }

    fn reduce_working_prefix(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) -> SerializedGreenWorkingCut {
        build.begin_working_prefix_reduction(session).unwrap();
        poll_builder_to_event_boundary(build, session);
        build.take_working_prefix_cut(session).unwrap()
    }

    fn force_test_leaf_barrier(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        build.begin_leaf_barrier(session).unwrap();
        poll_builder_to_event_boundary(build, session);
        let _ = build.take_leaf_barrier_cut(session).unwrap();
    }

    fn collect_green_leaf_ids(arena: &PageArena, root: ArenaId, output: &mut Vec<ArenaId>) {
        match sequence_node::<SerializedGreenSpec>(arena, root).unwrap().1 {
            SequenceNodeKind::Leaf => output.push(root),
            SequenceNodeKind::Branch { left, right } => {
                collect_green_leaf_ids(arena, left, output);
                collect_green_leaf_ids(arena, right, output);
            }
        }
    }

    fn test_maximum_avl_height(leaves: u64) -> u16 {
        let mut height = 1_u16;
        let mut minimum_at_height = 1_u64;
        let mut minimum_at_next_height = 2_u64;
        while minimum_at_next_height <= leaves {
            height += 1;
            let next = minimum_at_height.saturating_add(minimum_at_next_height);
            minimum_at_height = minimum_at_next_height;
            minimum_at_next_height = next;
        }
        height
    }

    fn finish_test_builder(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        build.finish_input(session).unwrap();
        loop {
            let before = build.receipt().resumable_arena_allocations;
            let progress = build.poll(session).unwrap();
            let after = build.receipt().resumable_arena_allocations;
            assert!(
                after - before <= 1,
                "one green poll allocated more than once"
            );
            match progress {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => return,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("finished green builder became writable")
                }
            }
        }
    }

    #[test]
    fn structural_validator_retypes_only_the_matching_active_setext_terminal() {
        let mut validator = StructuralValidator::default();
        validator
            .push(&GreenEvent::enter(
                BlockId(1),
                GreenKind::DOCUMENT,
                FactsEnvelope::empty(),
            ))
            .unwrap();
        validator
            .push(&GreenEvent::enter(
                BlockId(2),
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ))
            .unwrap();
        assert_eq!(
            validator.retype_active_terminal(
                BlockId(3),
                BlockId(3),
                GreenKind::PARAGRAPH,
                GreenKind::HEADING,
                GreenHeadingOpenFacts::new(1, GreenHeadingStyle::Setext)
                    .unwrap()
                    .into_envelope(),
            ),
            Err(SerializedGreenError::Invalid(
                "active terminal identity does not match retype"
            ))
        );
        validator
            .retype_active_terminal(
                BlockId(2),
                BlockId(2),
                GreenKind::PARAGRAPH,
                GreenKind::HEADING,
                GreenHeadingOpenFacts::new(1, GreenHeadingStyle::Setext)
                    .unwrap()
                    .into_envelope(),
            )
            .unwrap();
        assert_eq!(
            validator.open_frames.last().map(|frame| frame.kind),
            Some(GreenKind::HEADING)
        );
        validator
            .push(&GreenEvent::exit(ClosedChildAggregate::default()))
            .unwrap();
        validator
            .push(&GreenEvent::exit(ClosedChildAggregate::default()))
            .unwrap();
        validator.finish().unwrap();
    }

    #[test]
    fn intrinsic_blank_disambiguates_equal_item_summaries_and_is_structurally_checked() {
        let blank_child = ClosedChildAggregate {
            ends_blank: true,
            ..ClosedChildAggregate::default()
        };
        let children = ChildSequenceAggregate::singleton(blank_child);
        let without_intrinsic_blank = ContainerFoldSemantics {
            descends_through_last_child: true,
            is_item: true,
            last_line_blank: false,
        }
        .closed_summary(children);
        let with_intrinsic_blank = ContainerFoldSemantics {
            descends_through_last_child: true,
            is_item: true,
            last_line_blank: true,
        }
        .closed_summary(children);
        assert_eq!(
            without_intrinsic_blank, with_intrinsic_blank,
            "the derived parent contribution alone cannot recover intrinsic blank truth",
        );

        let false_bytes = encode_event(
            &GreenEvent::exit_with_state(without_intrinsic_blank, false, GreenCloseFacts::None),
            0,
        )
        .unwrap()
        .bytes;
        let true_bytes = encode_event(
            &GreenEvent::exit_with_state(with_intrinsic_blank, true, GreenCloseFacts::None),
            0,
        )
        .unwrap()
        .bytes;
        assert_ne!(false_bytes, true_bytes);
        assert_eq!(false_bytes[0] & 0x07, true_bytes[0] & 0x07);

        let mut validator = StructuralValidator::default();
        validator
            .push(&GreenEvent::enter(
                BlockId(1),
                GreenKind::DOCUMENT,
                FactsEnvelope::empty(),
            ))
            .unwrap();
        validator
            .push(&GreenEvent::enter(
                BlockId(2),
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ))
            .unwrap();
        assert_eq!(
            validator.push(&GreenEvent::exit_with_state(
                ClosedChildAggregate::default(),
                true,
                GreenCloseFacts::None,
            )),
            Err(SerializedGreenError::Invalid(
                "Exit closed summary disagrees with children and last_line_blank",
            )),
        );

        let mut validator = StructuralValidator::default();
        validator
            .push(&GreenEvent::enter(
                BlockId(1),
                GreenKind::DOCUMENT,
                FactsEnvelope::empty(),
            ))
            .unwrap();
        validator
            .push(&GreenEvent::enter(
                BlockId(2),
                GreenKind::LIST,
                GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope(),
            ))
            .unwrap();
        assert_eq!(
            validator.push(&GreenEvent::exit_with_state(
                ClosedChildAggregate::default(),
                false,
                GreenCloseFacts::List { tight: false },
            )),
            Err(SerializedGreenError::Invalid(
                "List Exit tightness disagrees with finalized children",
            )),
        );
    }

    fn assert_validator_stack_receipt_is_depth_bounded(
        receipt: &SerializedGreenBuildReceipt,
        expected_depth: usize,
    ) {
        assert_eq!(receipt.maximum_validator_frame_depth, expected_depth);
        let minimum = expected_depth.saturating_mul(std::mem::size_of::<StructuralOpenFrame>());
        let maximum = minimum.saturating_mul(4);
        assert!(receipt.maximum_validator_frame_capacity_bytes >= minimum);
        assert!(
            receipt.maximum_validator_frame_capacity_bytes <= maximum,
            "validator frame scratch must remain O(open depth)",
        );
    }

    #[test]
    fn one_shot_and_resumable_builds_receipt_depth_bounded_validator_frame_scratch() {
        const DEPTH: usize = 257;
        let mut events = Vec::with_capacity(DEPTH * 2);
        events.push(GreenEvent::enter(
            BlockId(1),
            GreenKind::DOCUMENT,
            FactsEnvelope::empty(),
        ));
        for index in 1..DEPTH {
            events.push(GreenEvent::enter(
                BlockId(u64::try_from(index + 1).unwrap()),
                GreenKind::BLOCK_QUOTE,
                FactsEnvelope::empty(),
            ));
        }
        events.extend(
            std::iter::repeat_with(|| GreenEvent::exit(ClosedChildAggregate::default()))
                .take(DEPTH),
        );

        let mut arena = PageArena::new();
        let mut one_shot_receipt = SerializedGreenBuildReceipt::default();
        let document = SerializedGreenDocument::build(
            &mut arena,
            resumable_spec(0),
            events.clone(),
            &mut one_shot_receipt,
        )
        .unwrap();
        assert_validator_stack_receipt_is_depth_bounded(&one_shot_receipt, DEPTH);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(0)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        for event in events {
            offer_test_event(&mut build, &mut session, event);
        }
        assert_validator_stack_receipt_is_depth_bounded(&build.receipt(), DEPTH);
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn composite_descriptor_binds_build_and_read_identity_with_complete_v10_totals() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer_test_event(&mut build, &mut session, test_coverage(1, BlockId(2)));
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let build_descriptor = manifest.composite_descriptor(&session).unwrap();
        let manifest_id = manifest.validate_composite_child(&session).unwrap();
        let read_descriptor =
            validate_serialized_green_composite_child(session.arena(), manifest_id).unwrap();
        assert_eq!(build_descriptor, read_descriptor);
        assert_eq!(build_descriptor.syntax_profile(), 1);
        assert_eq!(build_descriptor.source_revision(), SourceRevision(1));
        assert_eq!(build_descriptor.source_root(), SourceRootId(1));
        assert_eq!(
            build_descriptor.source_metric(),
            SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(build_descriptor.grammar_revision(), GrammarRevision(1));
        assert_eq!(build_descriptor.parse_generation(), ParseGeneration(1));
        assert_eq!(build_descriptor.semantic_epoch(), 1);
        assert_eq!(build_descriptor.known_bytes_start(), 0);
        assert_eq!(build_descriptor.known_bytes_end(), 1);
        assert_eq!(build_descriptor.leaf_pages(), 1);
        assert_eq!(build_descriptor.tokens(), 5);
        assert_eq!(build_descriptor.blocks(), 2);
        assert_eq!(build_descriptor.height(), 1);
        assert_eq!(
            build_descriptor.physical_metric(),
            SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(
            build_descriptor.logical_metric(),
            SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(build_descriptor.balance(), 0);
        assert_eq!(build_descriptor.minimum_prefix(), 0);
        assert_eq!(build_descriptor.minimum_closed_depth(), Some(0));
        assert_eq!(build_descriptor.coverage_count(), 1);

        let document = manifest.commit(session).unwrap().0;
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn provisional_paragraph_promotes_in_place_by_exactly_seven_bytes() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let token = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        offer_test_event(&mut build, &mut session, test_coverage(1, BlockId(2)));
        let before_len = build.leaf.bytes.len();
        let before_allocations = build.receipt().resumable_arena_allocations;
        let acknowledgement = promote_test_setext(&mut build, &mut session, token, BlockId(2), 1);
        assert_eq!(acknowledgement.block, BlockId(2));
        assert_eq!(acknowledgement.event_ordinal, 1);
        assert_eq!(acknowledgement.source_before, SerializedMetric::default());
        assert_eq!(build.leaf.bytes.len(), before_len + 7);
        assert_eq!(
            build.receipt().resumable_arena_allocations,
            before_allocations,
            "the partial fast path must allocate no arena page"
        );
        assert_eq!(build.receipt().setext_partial_promotions_completed, 1);
        assert_eq!(build.receipt().setext_sealed_promotions_completed, 0);
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        assert_eq!(
            serialized_green_test_trace(&document, &arena).unwrap()[1],
            SerializedGreenTestEvent::Enter {
                block: BlockId(2),
                kind: GreenKind::HEADING,
            }
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn canonical_fragment_replacement_streams_a_paragraph_into_an_open_table() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(2)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    2,
                    2,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );

        let expected = SerializedMetric { bytes: 2, utf16: 2 };
        build
            .begin_canonical_fragment_replacement(
                &mut session,
                paragraph,
                BlockId(3),
                GreenKind::TABLE,
                expected,
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);

        for event in [
            GreenEvent::enter(
                BlockId(3),
                GreenKind::TABLE,
                GreenTableOpenFacts::new(1).unwrap().into_envelope(),
            ),
            GreenEvent::enter(
                BlockId(4),
                GreenKind::TABLE_ROW,
                GreenTableRowOpenFacts::header().into_envelope(),
            ),
            GreenEvent::enter(
                BlockId(5),
                GreenKind::TABLE_CELL,
                GreenTableCellOpenFacts::header(0, GreenTableAlignment::Center).into_envelope(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(5),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(3), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ] {
            build
                .offer_canonical_fragment_event(&mut session, event)
                .unwrap();
            poll_builder_to_event_boundary(&mut build, &mut session);
        }
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        let replacement = build
            .take_canonical_fragment_replacement(&session, BlockId(3))
            .unwrap();
        assert_eq!(replacement.build, session.id());
        assert_eq!(replacement.retired_block, BlockId(2));
        assert_eq!(replacement.replacement_block(), BlockId(3));
        assert_eq!(replacement.replacement_kind(), GreenKind::TABLE);
        assert_eq!(replacement.physical_metric, expected);
        assert_eq!(replacement.retired_coverage_runs(), 1);
        assert_eq!(replacement.replacement_coverage_runs(), 2);

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        let trace = serialized_green_test_trace(&document, &arena).unwrap();
        assert_eq!(
            trace,
            vec![
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(3),
                    kind: GreenKind::TABLE,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(4),
                    kind: GreenKind::TABLE_ROW,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(5),
                    kind: GreenKind::TABLE_CELL,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(2),
                    metric: SerializedMetric { bytes: 1, utf16: 1 },
                    owner_relative_depth: 0,
                    part: CoveragePart::CONTENT,
                    logical: SerializedGreenTestLogical::Identity,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(3),
                    metric: SerializedMetric { bytes: 1, utf16: 1 },
                    owner_relative_depth: 0,
                    part: CoveragePart::TERMINAL,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
            ]
        );
        assert_eq!(
            trace
                .iter()
                .fold(SerializedMetric::default(), |sum, event| {
                    if let SerializedGreenTestEvent::Coverage { metric, .. } = event {
                        sum.checked_add(*metric).unwrap()
                    } else {
                        sum
                    }
                }),
            expected,
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn canonical_fragment_removal_preserves_nested_source_as_parent_owned_gaps() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(12)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(2), GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        );
        let paragraph = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(3));
        for event in [
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(1), 2, 2, 1, CoveragePart::CONTAINER_MARKER)
                    .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    9,
                    9,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(3),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(3), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
            ),
        ] {
            offer_test_event(&mut build, &mut session, event);
        }

        let expected = SerializedMetric {
            bytes: 12,
            utf16: 12,
        };
        build
            .begin_canonical_fragment_removal(
                &mut session,
                paragraph,
                BlockId(2),
                GreenKind::BLOCK_QUOTE,
                expected,
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        for event in [
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(4), 2, 2, 0, CoveragePart::CONTAINER_MARKER)
                    .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(5), 9, 9, 0, CoveragePart::GAP).unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(6), 1, 1, 0, CoveragePart::GAP).unwrap(),
            ),
        ] {
            build
                .offer_canonical_fragment_event(&mut session, event)
                .unwrap();
            poll_builder_to_event_boundary(&mut build, &mut session);
        }
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        assert!(matches!(
            build.take_canonical_fragment_replacement(&session, BlockId(2)),
            Err(SerializedGreenError::StaleCursor)
        ));
        let removal = build
            .take_canonical_fragment_removal(&session, BlockId(2))
            .unwrap();
        assert!(removal.removed_terminal());
        assert_eq!(removal.retired_block(), BlockId(3));
        assert_eq!(removal.replacement_block(), BlockId(2));
        assert_eq!(removal.replacement_kind(), GreenKind::BLOCK_QUOTE);
        assert_eq!(removal.physical_metric(), expected);
        assert_eq!(removal.retired_coverage_runs(), 3);
        assert_eq!(removal.replacement_coverage_runs(), 3);

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        assert_eq!(
            serialized_green_test_trace(&document, &arena).unwrap(),
            vec![
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(2),
                    kind: GreenKind::BLOCK_QUOTE,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(4),
                    metric: SerializedMetric { bytes: 2, utf16: 2 },
                    owner_relative_depth: 0,
                    part: CoveragePart::CONTAINER_MARKER,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(5),
                    metric: SerializedMetric { bytes: 9, utf16: 9 },
                    owner_relative_depth: 0,
                    part: CoveragePart::GAP,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(6),
                    metric: SerializedMetric { bytes: 1, utf16: 1 },
                    owner_relative_depth: 0,
                    part: CoveragePart::GAP,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
            ]
        );
        assert_eq!(document.metric(&arena).unwrap(), expected);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn canonical_fragment_removal_may_return_directly_to_document() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(3)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    3,
                    3,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );

        let expected = SerializedMetric { bytes: 3, utf16: 3 };
        build
            .begin_canonical_fragment_removal(
                &mut session,
                paragraph,
                BlockId(1),
                GreenKind::DOCUMENT,
                expected,
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        build
            .offer_canonical_fragment_event(
                &mut session,
                GreenEvent::Coverage(
                    SourceProjectionRun::new(CoverageId(2), 3, 3, 0, CoveragePart::GAP).unwrap(),
                ),
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        let removal = build
            .take_canonical_fragment_removal(&session, BlockId(1))
            .unwrap();
        assert!(removal.removed_terminal());

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let document = build.take_manifest().unwrap().commit(session).unwrap().0;
        assert_eq!(
            serialized_green_test_trace(&document, &arena).unwrap(),
            vec![
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(2),
                    metric: expected,
                    owner_relative_depth: 0,
                    part: CoveragePart::GAP,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Exit,
            ]
        );
        assert_eq!(document.metric(&arena).unwrap(), expected);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn canonical_fragment_split_setext_mints_heading_and_restores_old_paragraph_enter() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(15)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    10,
                    10,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );

        let rewritten = SerializedMetric {
            bytes: 10,
            utf16: 10,
        };
        build
            .begin_canonical_fragment_replacement(
                &mut session,
                paragraph,
                BlockId(2),
                GreenKind::PARAGRAPH,
                rewritten,
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        for event in [
            GreenEvent::enter(
                BlockId(3),
                GreenKind::HEADING,
                GreenHeadingOpenFacts::setext(1).unwrap().into_envelope(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    6,
                    6,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(3),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(3), 4, 4, 0, CoveragePart::TERMINAL).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ] {
            build
                .offer_canonical_fragment_event(&mut session, event)
                .unwrap();
            poll_builder_to_event_boundary(&mut build, &mut session);
        }
        build
            .offer_canonical_fragment_surviving_paragraph_enter(&mut session)
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        let replacement = build
            .take_canonical_fragment_replacement(&session, BlockId(2))
            .unwrap();
        let survivor = build
            .take_provisional_paragraph_enter(&session, BlockId(2))
            .unwrap();
        assert_eq!(replacement.retired_block, BlockId(2));
        assert_eq!(replacement.replacement_block(), BlockId(2));
        assert_eq!(replacement.replacement_kind(), GreenKind::PARAGRAPH);
        assert_eq!(replacement.physical_metric(), rewritten);
        assert_eq!(replacement.retired_coverage_runs(), 1);
        assert_eq!(replacement.replacement_coverage_runs(), 2);
        assert_eq!(survivor.block, BlockId(2));
        assert_eq!(survivor.source_before, rewritten);
        assert_eq!(survivor.event_ordinal, 5);

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(4),
                    5,
                    5,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let document = build.take_manifest().unwrap().commit(session).unwrap().0;
        let trace = serialized_green_test_trace(&document, &arena).unwrap();
        let enters = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Enter { block, kind } => Some((*block, *kind)),
                SerializedGreenTestEvent::Coverage { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            enters,
            vec![
                (BlockId(1), GreenKind::DOCUMENT),
                (BlockId(3), GreenKind::HEADING),
                (BlockId(2), GreenKind::PARAGRAPH),
            ],
            "the split outcome must mint the Heading and move the old identity to the survivor",
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn canonical_fragment_replacement_streams_a_retained_preface_before_an_open_table() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(5)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    5,
                    5,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );

        let expected = SerializedMetric { bytes: 5, utf16: 5 };
        build
            .begin_canonical_fragment_replacement(
                &mut session,
                paragraph,
                BlockId(3),
                GreenKind::TABLE,
                expected,
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);

        // Split-table identity policy is explicit in the producer: the old
        // Paragraph identity survives on the closed preface, while the open
        // Table and all of its descendants receive fresh identities.
        for event in [
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    2,
                    2,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::enter(
                BlockId(3),
                GreenKind::TABLE,
                GreenTableOpenFacts::new(1).unwrap().into_envelope(),
            ),
            GreenEvent::enter(
                BlockId(4),
                GreenKind::TABLE_ROW,
                GreenTableRowOpenFacts::header().into_envelope(),
            ),
            GreenEvent::enter(
                BlockId(5),
                GreenKind::TABLE_CELL,
                GreenTableCellOpenFacts::header(0, GreenTableAlignment::Left).into_envelope(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(3),
                    2,
                    2,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(5),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(4), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ] {
            build
                .offer_canonical_fragment_event(&mut session, event)
                .unwrap();
            poll_builder_to_event_boundary(&mut build, &mut session);
        }
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_event_boundary(&mut build, &mut session);
        let replacement = build
            .take_canonical_fragment_replacement(&session, BlockId(3))
            .unwrap();
        assert_eq!(replacement.retired_block, BlockId(2));
        assert_eq!(replacement.replacement_block(), BlockId(3));
        assert_eq!(replacement.replacement_kind(), GreenKind::TABLE);
        assert_eq!(replacement.physical_metric, expected);
        assert_eq!(replacement.retired_coverage_runs(), 1);
        assert_eq!(replacement.replacement_coverage_runs(), 3);

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        let trace = serialized_green_test_trace(&document, &arena).unwrap();
        assert_eq!(
            trace,
            vec![
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(2),
                    kind: GreenKind::PARAGRAPH,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(2),
                    metric: SerializedMetric { bytes: 2, utf16: 2 },
                    owner_relative_depth: 0,
                    part: CoveragePart::CONTENT,
                    logical: SerializedGreenTestLogical::Identity,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Enter {
                    block: BlockId(3),
                    kind: GreenKind::TABLE,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(4),
                    kind: GreenKind::TABLE_ROW,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(5),
                    kind: GreenKind::TABLE_CELL,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(3),
                    metric: SerializedMetric { bytes: 2, utf16: 2 },
                    owner_relative_depth: 0,
                    part: CoveragePart::CONTENT,
                    logical: SerializedGreenTestLogical::Identity,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(4),
                    metric: SerializedMetric { bytes: 1, utf16: 1 },
                    owner_relative_depth: 0,
                    part: CoveragePart::TERMINAL,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
            ]
        );
        assert_eq!(
            trace
                .iter()
                .fold(SerializedMetric::default(), |sum, event| {
                    if let SerializedGreenTestEvent::Coverage { metric, .. } = event {
                        sum.checked_add(*metric).unwrap()
                    } else {
                        sum
                    }
                }),
            expected,
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn sealed_middle_setext_repack_preserves_distant_leaf_and_program_ids() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(2)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        force_test_leaf_barrier(&mut build, &mut session);

        let token = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Program(one_piece_program()),
                )
                .unwrap(),
            ),
        );
        force_test_leaf_barrier(&mut build, &mut session);
        offer_test_event(&mut build, &mut session, test_coverage(2, BlockId(2)));
        force_test_leaf_barrier(&mut build, &mut session);
        let _ = reduce_working_prefix(&mut build, &mut session);

        let prefix = build.working_prefix.as_ref().unwrap();
        let mut before = Vec::new();
        collect_green_leaf_ids(
            session.arena(),
            session.owner_id(&prefix.owner).unwrap(),
            &mut before,
        );
        assert_eq!(before.len(), 3);
        let program_before = session.arena().packed_child_at(before[1], 0).unwrap();

        let promotion_before = build.receipt();
        let _ = promote_test_setext(&mut build, &mut session, token, BlockId(2), 2);
        let promotion_after = build.receipt();
        let prefix = build.working_prefix.as_ref().unwrap();
        let mut after = Vec::new();
        collect_green_leaf_ids(
            session.arena(),
            session.owner_id(&prefix.owner).unwrap(),
            &mut after,
        );
        assert_eq!(after.len(), 3);
        assert_eq!(after[0], before[0], "prefix leaf identity must survive");
        assert_ne!(after[1], before[1], "the target leaf must be replaced");
        assert_eq!(after[2], before[2], "suffix leaf identity must survive");
        assert_eq!(
            session.arena().packed_child_at(after[1], 0).unwrap(),
            program_before,
            "the target leaf's Program edge must be retained, not recopied"
        );
        assert_eq!(build.receipt().setext_partial_promotions_completed, 0);
        assert_eq!(build.receipt().setext_sealed_promotions_completed, 1);
        assert_eq!(build.receipt().setext_replacement_leaf_pages_allocated, 1);
        assert!(promotion_after.resumable_polls > promotion_before.resumable_polls);
        assert!(
            promotion_after.resumable_arena_allocations
                > promotion_before.resumable_arena_allocations
        );

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        assert_eq!(
            serialized_green_test_trace(&document, &arena).unwrap()[1],
            SerializedGreenTestEvent::Enter {
                block: BlockId(2),
                kind: GreenKind::HEADING,
            }
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn sealed_setext_repack_resolves_first_middle_and_last_leaf_positions() {
        #[derive(Clone, Copy, Debug)]
        enum Position {
            First,
            Middle,
            Last,
        }

        for position in [Position::First, Position::Middle, Position::Last] {
            let suffix_runs = match position {
                Position::First => 2_u64,
                Position::Middle => 1,
                Position::Last => 0,
            };
            let source_bytes = 1 + suffix_runs;
            let mut arena = PageArena::new();
            let ticket = arena.begin_build().unwrap();
            let mut build =
                ResumableSerializedGreenBuild::new(&ticket, resumable_spec(source_bytes)).unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            );
            if !matches!(position, Position::First) {
                force_test_leaf_barrier(&mut build, &mut session);
            }
            let token = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::Coverage(
                    SourceProjectionRun::with_logical(
                        CoverageId(1),
                        1,
                        1,
                        0,
                        CoveragePart::CONTENT,
                        BlockId(2),
                        LogicalContribution::Program(one_piece_program()),
                    )
                    .unwrap(),
                ),
            );
            force_test_leaf_barrier(&mut build, &mut session);
            for offset in 0..suffix_runs {
                offer_test_event(
                    &mut build,
                    &mut session,
                    test_coverage(offset + 2, BlockId(2)),
                );
                force_test_leaf_barrier(&mut build, &mut session);
            }
            let _ = reduce_working_prefix(&mut build, &mut session);
            let target = match position {
                Position::First => 0,
                Position::Middle | Position::Last => 1,
            };
            let mut before = Vec::new();
            let prefix = build.working_prefix.as_ref().unwrap();
            collect_green_leaf_ids(
                session.arena(),
                session.owner_id(&prefix.owner).unwrap(),
                &mut before,
            );
            assert_eq!(
                target,
                match position {
                    Position::First => 0,
                    Position::Middle => before.len() / 2,
                    Position::Last => before.len() - 1,
                }
            );
            let program = session.arena().packed_child_at(before[target], 0).unwrap();

            let _ = promote_test_setext(&mut build, &mut session, token, BlockId(2), 1);
            let mut after = Vec::new();
            let prefix = build.working_prefix.as_ref().unwrap();
            collect_green_leaf_ids(
                session.arena(),
                session.owner_id(&prefix.owner).unwrap(),
                &mut after,
            );
            assert_eq!(after.len(), before.len());
            for index in 0..before.len() {
                if index == target {
                    assert_ne!(after[index], before[index], "{position:?}");
                } else {
                    assert_eq!(after[index], before[index], "{position:?}");
                }
            }
            assert_eq!(
                session.arena().packed_child_at(after[target], 0).unwrap(),
                program,
                "{position:?}"
            );
            assert!(prefix.summary.height <= test_maximum_avl_height(prefix.summary.leaves));

            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::exit(ClosedChildAggregate::default()),
            );
            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::exit(ClosedChildAggregate::default()),
            );
            finish_test_builder(&mut build, &mut session);
            let manifest = build.take_manifest().unwrap();
            let document = manifest.commit(session).unwrap().0;
            document.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0, "{position:?}");
        }
    }

    #[test]
    fn partial_capacity_cliff_force_seals_and_repacks_one_leaf_into_two() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, capacity_cliff_spec()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let token = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
        for coverage in 1..=CAPACITY_CLIFF_RUNS {
            offer_test_event(
                &mut build,
                &mut session,
                capacity_cliff_coverage(coverage, BlockId(2)),
            );
        }
        assert_eq!(
            build.leaf.bytes.len(),
            ARENA_PAGE_BYTES,
            "the fixture must hit the packed payload cliff exactly"
        );
        assert!(!build.partial_setext_can_fit(7));

        let promotion_before = build.receipt();
        let _ = promote_test_setext(&mut build, &mut session, token, BlockId(2), 1);
        let promotion_after = build.receipt();
        assert_eq!(build.receipt().setext_capacity_cliff_force_seals, 1);
        assert_eq!(build.receipt().setext_sealed_promotions_completed, 1);
        assert_eq!(build.receipt().setext_replacement_leaf_pages_allocated, 2);
        let prefix = build.working_prefix.as_ref().unwrap();
        assert_eq!(prefix.summary.leaves, 2);
        assert_eq!(prefix.summary.height, 2);
        let mut leaves = Vec::new();
        collect_green_leaf_ids(
            session.arena(),
            session.owner_id(&prefix.owner).unwrap(),
            &mut leaves,
        );
        assert_eq!(leaves.len(), 2);
        assert_eq!(session.arena().payload(leaves[0]).unwrap().len(), 4_091);
        assert_eq!(session.arena().payload(leaves[1]).unwrap().len(), 108);
        assert_eq!(
            promotion_after.resumable_polls - promotion_before.resumable_polls,
            11
        );
        assert_eq!(
            promotion_after.resumable_arena_allocations
                - promotion_before.resumable_arena_allocations,
            4
        );

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        let trace = serialized_green_test_trace(&document, &arena).unwrap();
        assert_eq!(
            trace.len(),
            usize::try_from(CAPACITY_CLIFF_RUNS).unwrap() + 4
        );
        assert_eq!(
            trace[1],
            SerializedGreenTestEvent::Enter {
                block: BlockId(2),
                kind: GreenKind::HEADING,
            }
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn provisional_setext_authority_rejects_wrong_block_build_and_replay() {
        let mut arena = PageArena::new();

        let first_ticket = arena.begin_build().unwrap();
        let mut first =
            ResumableSerializedGreenBuild::new(&first_ticket, resumable_spec(0)).unwrap();
        let mut first_session = arena.resume_build(first_ticket).unwrap();
        offer_test_event(
            &mut first,
            &mut first_session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        first
            .offer_provisional_paragraph_enter(
                &mut first_session,
                BlockId(2),
                FactsEnvelope::empty(),
            )
            .unwrap();
        poll_builder_to_event_boundary(&mut first, &mut first_session);
        assert_eq!(
            first.take_provisional_paragraph_enter(&first_session, BlockId(3)),
            Err(SerializedGreenError::StaleCursor)
        );
        let first_token = first
            .take_provisional_paragraph_enter(&first_session, BlockId(2))
            .unwrap();
        let first_ticket = first_session.suspend().unwrap();

        let second_ticket = arena.begin_build().unwrap();
        let mut second =
            ResumableSerializedGreenBuild::new(&second_ticket, resumable_spec(0)).unwrap();
        let mut second_session = arena.resume_build(second_ticket).unwrap();
        offer_test_event(
            &mut second,
            &mut second_session,
            GreenEvent::enter(BlockId(10), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let second_token =
            offer_provisional_test_paragraph(&mut second, &mut second_session, BlockId(11));
        assert_eq!(
            second.begin_setext_promotion(
                &mut second_session,
                first_token,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            ),
            Err(SerializedGreenError::StaleCursor)
        );
        let wrong_ordinal = ProvisionalParagraphEnter {
            build: second_token.build,
            block: second_token.block,
            generation: second_token.generation,
            event_ordinal: second_token.event_ordinal + 1,
            source_before: second_token.source_before,
        };
        assert_eq!(
            second.begin_setext_promotion(
                &mut second_session,
                wrong_ordinal,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            ),
            Err(SerializedGreenError::StaleCursor)
        );
        let wrong_source = ProvisionalParagraphEnter {
            build: second_token.build,
            block: second_token.block,
            generation: second_token.generation,
            event_ordinal: second_token.event_ordinal,
            source_before: SerializedMetric {
                bytes: second_token.source_before.bytes + 1,
                utf16: second_token.source_before.utf16 + 1,
            },
        };
        assert_eq!(
            second.begin_setext_promotion(
                &mut second_session,
                wrong_source,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            ),
            Err(SerializedGreenError::StaleCursor)
        );
        let replay = ProvisionalParagraphEnter {
            build: second_token.build,
            block: second_token.block,
            generation: second_token.generation,
            event_ordinal: second_token.event_ordinal,
            source_before: second_token.source_before,
        };
        let _ = promote_test_setext(
            &mut second,
            &mut second_session,
            second_token,
            BlockId(11),
            1,
        );
        assert_eq!(
            second.begin_setext_promotion(
                &mut second_session,
                replay,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            ),
            Err(SerializedGreenError::StaleCursor)
        );

        let second_abort = second_session.begin_abort().unwrap();
        while !arena.poll_build_abort(second_abort, 1).unwrap().complete {}
        drop(second);
        let first_session = arena.resume_build(first_ticket).unwrap();
        let first_abort = first_session.begin_abort().unwrap();
        while !arena.poll_build_abort(first_abort, 1).unwrap().complete {}
        drop(first);
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn setext_promotion_can_cancel_after_every_force_seal_and_middle_splice_poll() {
        for middle_splice in [false, true] {
            let mut reached_complete = false;
            let mut completion_polls = 0_usize;
            for polls_before_abort in 0..128 {
                let mut arena = PageArena::new();
                let ticket = arena.begin_build().unwrap();
                let spec = if middle_splice {
                    resumable_spec(2)
                } else {
                    capacity_cliff_spec()
                };
                let mut build = ResumableSerializedGreenBuild::new(&ticket, spec).unwrap();
                let mut session = arena.resume_build(ticket).unwrap();
                offer_test_event(
                    &mut build,
                    &mut session,
                    GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
                );
                if middle_splice {
                    force_test_leaf_barrier(&mut build, &mut session);
                }
                let token = offer_provisional_test_paragraph(&mut build, &mut session, BlockId(2));
                if middle_splice {
                    offer_test_event(&mut build, &mut session, test_coverage(1, BlockId(2)));
                    force_test_leaf_barrier(&mut build, &mut session);
                    offer_test_event(&mut build, &mut session, test_coverage(2, BlockId(2)));
                    force_test_leaf_barrier(&mut build, &mut session);
                    let _ = reduce_working_prefix(&mut build, &mut session);
                } else {
                    for coverage in 1..=CAPACITY_CLIFF_RUNS {
                        offer_test_event(
                            &mut build,
                            &mut session,
                            capacity_cliff_coverage(coverage, BlockId(2)),
                        );
                    }
                    assert!(!build.partial_setext_can_fit(7));
                }
                build
                    .begin_setext_promotion(
                        &mut session,
                        token,
                        GreenHeadingOpenFacts::setext(1).unwrap(),
                    )
                    .unwrap();

                let mut complete = false;
                for poll_index in 0..polls_before_abort {
                    let before = build.receipt().resumable_arena_allocations;
                    let progress = build.poll(&mut session).unwrap();
                    let after = build.receipt().resumable_arena_allocations;
                    assert!(after - before <= 1, "poll {poll_index} allocated twice");
                    match progress {
                        SerializedGreenStreamProgress::Pending => {}
                        SerializedGreenStreamProgress::ReadyForEvent => {
                            complete = true;
                            completion_polls = poll_index + 1;
                            break;
                        }
                        SerializedGreenStreamProgress::ManifestReady => {
                            panic!("Setext cancellation fixture finalized a manifest")
                        }
                    }
                }
                let abort = session.begin_abort().unwrap();
                while !arena.poll_build_abort(abort, 1).unwrap().complete {}
                drop(build);
                settle(&mut arena);
                assert_eq!(arena.metrics().live_nodes, 0);
                if complete {
                    reached_complete = true;
                    break;
                }
            }
            assert!(
                reached_complete,
                "Setext cancellation sweep never completed (middle={middle_splice})"
            );
            assert!(
                completion_polls >= 8,
                "fixture did not traverse a meaningful resumable path"
            );
        }
    }

    #[test]
    fn working_reduction_handles_empty_prefix_and_tail_without_fragmenting_the_partial_leaf() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(0)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );

        let empty = reduce_working_prefix(&mut build, &mut session);
        assert_eq!(empty.build_id(), session.id());
        assert_eq!(empty.installed_leaves_before(), 0);
        assert_eq!(empty.events_before(), 1);
        assert_eq!(empty.source_before(), SerializedMetric::default());
        assert!(build.working_prefix.is_none());
        assert_eq!(build.receipt().leaf_pages_allocated, 0);

        let repeated_empty = reduce_working_prefix(&mut build, &mut session);
        assert_eq!(repeated_empty.installed_leaves_before(), 0);
        assert_eq!(build.receipt().leaf_pages_allocated, 0);

        force_test_leaf_barrier(&mut build, &mut session);
        assert!(build.working_prefix.is_none());
        assert_eq!(build.tail_sealed_leaves, 1);
        let installed = reduce_working_prefix(&mut build, &mut session);
        assert_eq!(installed.installed_leaves_before(), 1);
        assert_eq!(installed.events_before(), 1);
        assert!(build.working_prefix.is_some());
        assert_eq!(build.tail_sealed_leaves, 0);

        let prefix_only = reduce_working_prefix(&mut build, &mut session);
        assert_eq!(prefix_only.installed_leaves_before(), 1);
        assert_eq!(build.receipt().working_prefix_reductions_completed, 4);
        assert_eq!(build.receipt().working_prefix_noop_reductions, 3);
        assert_eq!(build.receipt().maximum_working_prefixes, 1);

        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let manifest = build.take_manifest().unwrap();
        let document = manifest.commit(session).unwrap().0;
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    fn build_paragraph_stream(
        blocks: u64,
        reduce_after_each_block: bool,
    ) -> (
        PageArena,
        SerializedGreenDocument,
        SerializedGreenBuildReceipt,
    ) {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build =
            ResumableSerializedGreenBuild::new(&ticket, resumable_spec(blocks)).unwrap();
        if reduce_after_each_block {
            // An unrelated historical split peak must not contaminate the
            // reusable splice job's own scratch receipt.
            build
                .sequence_receipt
                .maximum_resumable_split_total_requested_bytes = usize::MAX;
            build
                .sequence_receipt
                .maximum_resumable_split_total_scratch_bytes = usize::MAX;
        }
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let mut previous_prefix_leaves = Vec::new();
        for index in 0..blocks {
            let block = BlockId(index + 2);
            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::enter(block, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            );
            offer_test_event(&mut build, &mut session, test_coverage(index + 1, block));
            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::exit(ClosedChildAggregate::default()),
            );
            if reduce_after_each_block {
                let cut = reduce_working_prefix(&mut build, &mut session);
                assert_eq!(cut.events_before(), 1 + (index + 1) * 3);
                assert_eq!(cut.source_before().bytes, index + 1);
                assert_eq!(cut.source_before().utf16, index + 1);
                if let Some(prefix) = &build.working_prefix {
                    let mut current = Vec::new();
                    collect_green_leaf_ids(
                        session.arena(),
                        session.owner_id(&prefix.owner).unwrap(),
                        &mut current,
                    );
                    assert!(current.starts_with(&previous_prefix_leaves));
                    previous_prefix_leaves = current;
                }
            }
        }
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_test_builder(&mut build, &mut session);
        let receipt = build.receipt();
        if reduce_after_each_block {
            assert_eq!(
                build.sequence_receipt.resumable_join_scratch_reservations, 2,
                "tail and splice joins must each reserve exactly once"
            );
            assert_eq!(build.sequence_receipt.resumable_split_frame_reservations, 1);
            assert_ne!(receipt.maximum_sequence_splice_requested_bytes, usize::MAX);
            assert_ne!(receipt.maximum_sequence_splice_scratch_bytes, usize::MAX);
            assert!(
                receipt.maximum_sequence_splice_scratch_bytes
                    >= receipt.maximum_sequence_splice_requested_bytes
            );
            assert!(receipt.resumable_sequence_splice_polls > 0);
        }
        let manifest = build.take_manifest().unwrap();
        let manifest_id = session.owner_id(&manifest.owner).unwrap();
        let (decoded, root) = decode_document(session.arena(), manifest_id).unwrap();
        assert_eq!(decoded.summary.tokens, blocks * 3 + 2);
        assert_eq!(decoded.summary.metric.bytes, blocks);
        assert_eq!(decoded.summary.metric.utf16, blocks);
        assert!(decoded.summary.height <= test_maximum_avl_height(decoded.summary.leaves));
        let mut final_leaves = Vec::new();
        collect_green_leaf_ids(session.arena(), root, &mut final_leaves);
        assert!(final_leaves.starts_with(&previous_prefix_leaves));
        let document = manifest.commit(session).unwrap().0;
        (arena, document, receipt)
    }

    #[test]
    fn thousands_of_working_reductions_preserve_packing_identity_and_logarithmic_shape() {
        const BLOCKS: u64 = 2_000;
        let (mut baseline_arena, baseline, baseline_receipt) =
            build_paragraph_stream(BLOCKS, false);
        let (mut reduced_arena, reduced, reduced_receipt) = build_paragraph_stream(BLOCKS, true);

        assert_eq!(
            reduced_receipt.leaf_pages_allocated, baseline_receipt.leaf_pages_allocated,
            "normalization reductions must not fragment the active packed page"
        );
        assert!(
            reduced_receipt.branch_nodes_allocated
                <= baseline_receipt.branch_nodes_allocated
                    + reduced_receipt.leaf_pages_allocated * 4,
            "branch work must scale with naturally sealed pages, not small-block cuts"
        );
        assert_eq!(reduced_receipt.working_prefix_reductions_completed, BLOCKS);
        assert!(reduced_receipt.working_prefix_noop_reductions < BLOCKS);
        assert_eq!(reduced_receipt.maximum_working_prefixes, 1);

        baseline.release_later(&mut baseline_arena).unwrap();
        reduced.release_later(&mut reduced_arena).unwrap();
        settle(&mut baseline_arena);
        settle(&mut reduced_arena);
        assert_eq!(baseline_arena.metrics().live_nodes, 0);
        assert_eq!(reduced_arena.metrics().live_nodes, 0);
    }

    #[test]
    fn working_prefix_reduction_can_cancel_after_every_poll_boundary() {
        let mut reached_complete = false;
        for polls_before_abort in 0..128 {
            let mut arena = PageArena::new();
            let ticket = arena.begin_build().unwrap();
            let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(2)).unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            offer_test_event(
                &mut build,
                &mut session,
                GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            );
            for (index, block) in [BlockId(2), BlockId(3)].into_iter().enumerate() {
                offer_test_event(
                    &mut build,
                    &mut session,
                    GreenEvent::enter(block, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
                );
                offer_test_event(
                    &mut build,
                    &mut session,
                    test_coverage(u64::try_from(index + 1).unwrap(), block),
                );
                offer_test_event(
                    &mut build,
                    &mut session,
                    GreenEvent::exit(ClosedChildAggregate::default()),
                );
                force_test_leaf_barrier(&mut build, &mut session);
                if index == 0 {
                    let _ = reduce_working_prefix(&mut build, &mut session);
                }
            }
            assert!(build.working_prefix.is_some());
            assert_eq!(build.tail_sealed_leaves, 1);
            build.begin_working_prefix_reduction(&session).unwrap();

            let mut complete = false;
            for _ in 0..polls_before_abort {
                let before = build.receipt().resumable_arena_allocations;
                let progress = build.poll(&mut session).unwrap();
                let after = build.receipt().resumable_arena_allocations;
                assert!(after - before <= 1);
                if progress == SerializedGreenStreamProgress::ReadyForEvent {
                    let _ = build.take_working_prefix_cut(&session).unwrap();
                    complete = true;
                    break;
                }
                assert_eq!(progress, SerializedGreenStreamProgress::Pending);
            }
            let abort = session.begin_abort().unwrap();
            while !arena.poll_build_abort(abort, 1).unwrap().complete {}
            drop(build);
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
            if complete {
                reached_complete = true;
                break;
            }
        }
        assert!(
            reached_complete,
            "phase-by-phase cancellation never reached installed output"
        );
    }

    #[test]
    fn working_cut_rejects_the_wrong_build_generation_without_poisoning_the_right_one() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, resumable_spec(0)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_test_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        build.begin_working_prefix_reduction(&session).unwrap();
        let ticket = session.suspend().unwrap();

        let wrong_ticket = arena.begin_build().unwrap();
        let wrong_session = arena.resume_build(wrong_ticket).unwrap();
        assert_eq!(
            build.take_working_prefix_cut(&wrong_session),
            Err(SerializedGreenError::Invalid(
                "arena session belongs to another build generation"
            ))
        );
        let wrong_abort = wrong_session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(wrong_abort, 0).unwrap().complete);

        let session = arena.resume_build(ticket).unwrap();
        let cut = build.take_working_prefix_cut(&session).unwrap();
        assert_eq!(cut.build_id(), session.id());
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(build);
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn common_logical_runs_use_one_compact_descriptor_without_a_consumer_field() {
        let none = GreenEvent::Coverage(
            SourceProjectionRun::new(CoverageId(1), 1, 1, 0, CoveragePart::CONTENT).unwrap(),
        );
        let identity = GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(1),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                BlockId(2),
                LogicalContribution::Identity,
            )
            .unwrap(),
        );
        let hidden = GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(1),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                BlockId(2),
                LogicalContribution::Hidden {
                    affinity: GreenAffinity::Downstream,
                },
            )
            .unwrap(),
        );
        let none = encode_event(&none, 0).unwrap();
        let identity = encode_event(&identity, 0).unwrap();
        let hidden = encode_event(&hidden, 0).unwrap();
        assert_eq!(none.bytes.len(), identity.bytes.len());
        assert_eq!(none.bytes.len(), hidden.bytes.len());
        assert!(none.program.is_none());
        assert!(identity.program.is_none());
        assert!(hidden.program.is_none());
        assert_eq!(*none.bytes.last().unwrap(), LOGICAL_NONE_TAG);
        assert_eq!(*identity.bytes.last().unwrap(), LOGICAL_IDENTITY_TAG);
        assert_eq!(*hidden.bytes.last().unwrap(), LOGICAL_HIDDEN_DOWNSTREAM_TAG);
    }

    #[test]
    fn v10_retains_v9_reset_bit_while_v9_and_unknown_bits_fail_closed() {
        let mut run = SourceProjectionRun::with_logical(
            CoverageId(9),
            1,
            1,
            0,
            CoveragePart::CONTENT,
            BlockId(2),
            LogicalContribution::Identity,
        )
        .unwrap();
        run.mark_projection_reset_after();
        let encoded = encode_event(&GreenEvent::Coverage(run), 0).unwrap();
        assert_eq!(
            *encoded.bytes.last().unwrap(),
            LOGICAL_IDENTITY_TAG | LOGICAL_PROJECTION_RESET_AFTER
        );

        let mut arena = PageArena::new();
        let dummy = arena.allocate(b"reset-codec-dummy", &[]).unwrap();
        let mut decoder = Decoder::new(&encoded.bytes);
        let mut next_program_ordinal = 0;
        let decoded_event = decode_event(
            &mut decoder,
            &arena,
            dummy.owner.id(),
            &mut next_program_ordinal,
        )
        .unwrap();
        assert!(matches!(
            decoded_event,
            DecodedGreenEventKind::Coverage(DecodedSourceProjectionRun {
                projection_reset_after: true,
                ..
            })
        ));
        assert!(decoder.is_empty());

        let mut unknown = encoded.bytes.clone();
        *unknown.last_mut().unwrap() |= 0x10;
        let mut decoder = Decoder::new(&unknown);
        assert_eq!(
            decode_event(
                &mut decoder,
                &arena,
                dummy.owner.id(),
                &mut next_program_ordinal,
            ),
            Err(SerializedGreenError::Corrupt(
                "logical descriptor has reserved bits"
            ))
        );

        let summary = GreenSummary {
            leaves: 1,
            tokens: 1,
            height: 1,
            ..GreenSummary::default()
        };
        let v10 = encode_summary(LEAF_TAG, summary);
        assert_eq!(v10[1], FORMAT_VERSION);
        assert!(decode_summary(&v10, LEAF_TAG).is_ok());
        let mut v9 = v10;
        v9[1] = 9;
        assert_eq!(
            decode_summary(&v9, LEAF_TAG),
            Err(SerializedGreenError::Corrupt("invalid summary header"))
        );

        arena.release_later(dummy.owner).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn v10_logical_summary_and_manifest_round_trip_and_fail_closed() {
        let summary = GreenSummary {
            leaves: 1,
            tokens: 1,
            height: 1,
            metric: SerializedMetric { bytes: 5, utf16: 3 },
            logical_metric: SerializedMetric { bytes: 4, utf16: 2 },
            ..GreenSummary::default()
        };
        let encoded = encode_summary(LEAF_TAG, summary);
        assert_eq!(encoded.len(), SUMMARY_BYTES);
        assert_eq!(decode_summary(&encoded, LEAF_TAG), Ok(summary));

        let mut partial_logical = encoded;
        partial_logical[88..96].copy_from_slice(&0_u64.to_le_bytes());
        assert_eq!(
            decode_summary(&partial_logical, LEAF_TAG),
            Err(SerializedGreenError::Corrupt("invalid summary values")),
        );

        let manifest = Manifest {
            syntax_profile: 1,
            source_revision: SourceRevision(1),
            source_root: SourceRootId(1),
            source_bytes: summary.metric.bytes,
            source_utf16: summary.metric.utf16,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(1),
            semantic_epoch: 1,
            known_bytes: 0..summary.metric.bytes,
            summary,
        };
        let encoded_manifest = encode_manifest(&manifest);
        assert_eq!(encoded_manifest.len(), MANIFEST_BYTES);
        let decoded_manifest = decode_manifest(&encoded_manifest).unwrap();
        assert_eq!(decoded_manifest.syntax_profile, manifest.syntax_profile);
        assert_eq!(decoded_manifest.source_revision, manifest.source_revision);
        assert_eq!(decoded_manifest.source_root, manifest.source_root);
        assert_eq!(decoded_manifest.source_bytes, manifest.source_bytes);
        assert_eq!(decoded_manifest.source_utf16, manifest.source_utf16);
        assert_eq!(decoded_manifest.grammar_revision, manifest.grammar_revision);
        assert_eq!(decoded_manifest.parse_generation, manifest.parse_generation);
        assert_eq!(decoded_manifest.semantic_epoch, manifest.semantic_epoch);
        assert_eq!(decoded_manifest.known_bytes, manifest.known_bytes);
        assert_eq!(decoded_manifest.summary.blocks, summary.blocks);
        assert_eq!(decoded_manifest.summary.tokens, summary.tokens);
        assert_eq!(decoded_manifest.summary.leaves, summary.leaves);
        assert_eq!(decoded_manifest.summary.metric, summary.metric);
        assert_eq!(
            decoded_manifest.summary.logical_metric,
            summary.logical_metric
        );
        let mut partial_manifest = encoded_manifest;
        partial_manifest[136..144].copy_from_slice(&0_u64.to_le_bytes());
        assert_eq!(
            decode_manifest(&partial_manifest),
            Err(SerializedGreenError::Corrupt(
                "invalid green manifest values"
            )),
        );

        let logical_bytes_overflow = GreenSummary {
            logical_metric: SerializedMetric {
                bytes: u64::MAX,
                utf16: 1,
            },
            ..GreenSummary::default()
        }
        .followed_by(GreenSummary {
            logical_metric: SerializedMetric { bytes: 1, utf16: 1 },
            ..GreenSummary::default()
        });
        assert_eq!(
            logical_bytes_overflow,
            Err(SerializedGreenError::Overflow("logical bytes")),
        );

        let logical_utf16_overflow = GreenSummary {
            logical_metric: SerializedMetric {
                bytes: 1,
                utf16: u64::MAX,
            },
            ..GreenSummary::default()
        }
        .followed_by(GreenSummary {
            logical_metric: SerializedMetric { bytes: 1, utf16: 1 },
            ..GreenSummary::default()
        });
        assert_eq!(
            logical_utf16_overflow,
            Err(SerializedGreenError::Overflow("logical UTF-16")),
        );
    }

    #[test]
    fn v10_leaf_and_manifest_reject_corrupt_logical_summaries() {
        let paragraph = BlockId(2);
        let events = vec![
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(paragraph, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    4,
                    2,
                    0,
                    CoveragePart::CONTENT,
                    paragraph,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let mut arena = PageArena::new();
        let mut receipt = SerializedGreenBuildReceipt::default();
        let document = SerializedGreenDocument::build(
            &mut arena,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: SourceRevision(1),
                source_root: SourceRootId(1),
                source_bytes: 4,
                source_utf16: 2,
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(1),
                semantic_epoch: 1,
                known_bytes: 0..4,
            },
            events,
            &mut receipt,
        )
        .unwrap();

        let leaf = document.leaf_at(&arena, 0).unwrap().unwrap();
        let mut corrupt_leaf_payload = arena.payload(leaf).unwrap().to_vec();
        corrupt_leaf_payload[80..88].copy_from_slice(&5_u64.to_le_bytes());
        corrupt_leaf_payload[88..96].copy_from_slice(&3_u64.to_le_bytes());
        let corrupt_leaf = arena.allocate(&corrupt_leaf_payload, &[]).unwrap();
        assert_eq!(
            visit_decoded_leaf_events(&arena, corrupt_leaf.owner.id(), |_, _| Ok(())),
            Err(SerializedGreenError::Corrupt("green leaf summary mismatch")),
        );

        let manifest_id = document.local_manifest_id(&arena).unwrap();
        let root = arena.children(manifest_id).unwrap()[0].unwrap();
        let mut corrupt_manifest_payload = arena.payload(manifest_id).unwrap().to_vec();
        corrupt_manifest_payload[128..136].copy_from_slice(&5_u64.to_le_bytes());
        corrupt_manifest_payload[136..144].copy_from_slice(&3_u64.to_le_bytes());
        let corrupt_manifest = arena.allocate(&corrupt_manifest_payload, &[root]).unwrap();
        assert_eq!(
            decode_document(&arena, corrupt_manifest.owner.id()),
            Err(SerializedGreenError::Corrupt(
                "green manifest summary mismatch"
            )),
        );

        arena.release_later(corrupt_leaf.owner).unwrap();
        arena.release_later(corrupt_manifest.owner).unwrap();
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn corrupt_declared_counts_are_bounded_before_scratch_reservation() {
        let mut program = encode_projection_program(&one_piece_program()).unwrap();
        program[4] = 127;
        assert_eq!(
            decode_projection_program_payload(&program),
            Err(SerializedGreenError::Corrupt(
                "projection piece count exceeds remaining page"
            ))
        );

        let facts = FactsEnvelope::new(vec![FactField::optional(FactId(100), [1])]).unwrap();
        let mut facts = encode_facts(&facts).unwrap();
        facts[1] = 127;
        assert_eq!(
            decode_facts(&facts),
            Err(SerializedGreenError::Corrupt(
                "fact count exceeds remaining envelope"
            ))
        );
    }

    #[test]
    fn program_decoder_rejects_unknown_piece_type_and_wrong_edge_type_count_or_generation() {
        let program = one_piece_program();
        let mut payload = encode_projection_program(&program).unwrap();
        let mut decoder = Decoder::new(&payload);
        decode_projection_program_header(&mut decoder).unwrap();
        let piece_descriptor = decoder.cursor;
        payload[piece_descriptor] = 0x70;
        assert_eq!(
            decode_projection_program_payload(&payload),
            Err(SerializedGreenError::Corrupt(
                "unknown projection program piece type"
            ))
        );

        let valid_payload = encode_projection_program(&program).unwrap();
        let mut arena = PageArena::new();
        let wrong_type = arena.allocate(b"not-a-program", &[]).unwrap();
        assert_eq!(
            validate_projection_program_edge_header(
                &arena,
                wrong_type.owner.id(),
                1,
                program.physical_metric(),
                program.logical_metric(),
            ),
            Err(SerializedGreenError::Corrupt(
                "projection edge has the wrong page type or version"
            ))
        );
        arena.release_later(wrong_type.owner).unwrap();
        settle(&mut arena);

        let live = arena.allocate(&valid_payload, &[]).unwrap();
        let live_id = live.owner.id();
        assert_eq!(
            validate_projection_program_edge_header(
                &arena,
                live_id,
                2,
                program.physical_metric(),
                program.logical_metric(),
            ),
            Err(SerializedGreenError::Corrupt(
                "projection edge count or partition mismatch"
            ))
        );
        arena.release_later(live.owner).unwrap();
        settle(&mut arena);
        assert_eq!(
            validate_projection_program_edge_header(
                &arena,
                live_id,
                1,
                program.physical_metric(),
                program.logical_metric(),
            ),
            Err(SerializedGreenError::Arena(ArenaError::StaleId(live_id)))
        );
    }

    #[test]
    fn leaf_decoder_rejects_noncanonical_program_edge_ordinal() {
        let program = one_piece_program();
        let events = [
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Program(program),
                )
                .unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let mut page = LeafEncoder::default();
        for event in &events {
            let ordinal = usize::from(matches!(event, GreenEvent::Coverage(_)));
            let encoded = encode_event(event, ordinal).unwrap();
            page.push(event, encoded).unwrap();
        }
        let (leaf_payload, _, programs) = page.seal().unwrap();
        let [PendingProjectionProgram::New(program_payload)] = programs.as_slice() else {
            panic!("one new Program payload");
        };
        let mut arena = PageArena::new();
        let program_page = arena.allocate(program_payload, &[]).unwrap();
        let leaf = arena
            .allocate_packed(&leaf_payload, &[program_page.owner.id()])
            .unwrap();
        assert_eq!(
            decode_leaf(&arena, leaf.owner.id()).unwrap_err(),
            SerializedGreenError::Corrupt("program edge ordinals are not canonical")
        );
        arena.release_later(leaf.owner).unwrap();
        arena.release_later(program_page.owner).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn physical_leaf_decode_defers_program_page_validation_until_logical_selection() {
        let events = [
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Program(one_piece_program()),
                )
                .unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let mut page = LeafEncoder::default();
        for event in &events {
            let encoded = encode_event(event, page.programs.len()).unwrap();
            page.push(event, encoded).unwrap();
        }
        let (leaf_payload, summary, _) = page.seal().unwrap();
        let mut arena = PageArena::new();
        let wrong_program = arena.allocate(b"not-a-program", &[]).unwrap();
        let leaf = arena
            .allocate_packed(&leaf_payload, &[wrong_program.owner.id()])
            .unwrap();
        let manifest = Manifest {
            syntax_profile: 1,
            source_revision: SourceRevision(1),
            source_root: SourceRootId(1),
            source_bytes: 1,
            source_utf16: 1,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(1),
            semantic_epoch: 1,
            known_bytes: 0..1,
            summary,
        };
        let manifest_allocation = arena
            .allocate(&encode_manifest(&manifest), &[leaf.owner.id()])
            .unwrap();
        let owner = manifest_allocation.owner;
        let document = SerializedGreenDocument {
            manifest: SerializedGreenManifestId::new(owner.scoped_id()),
            owner,
        };
        arena.release_later(leaf.owner).unwrap();
        arena.release_later(wrong_program.owner).unwrap();

        let mut source = document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        let paragraph = source.open_path().last().unwrap().enter;
        let coverage = source.next_coverage(&document, &arena).unwrap().unwrap();
        assert!(matches!(
            coverage.logical_contribution,
            LogicalContributionView::Program { .. }
        ));
        assert_eq!(source.receipt().leaf_pages_decoded, 1);

        let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
        assert_eq!(
            logical.next_segment(&document, &arena),
            Err(SerializedGreenError::Corrupt(
                "projection edge has the wrong page type or version"
            ))
        );

        let (_, root) = decode_document(&arena, document.owner.id()).unwrap();
        let extra_edge_manifest = arena
            .allocate(&encode_manifest(&manifest), &[root, root])
            .unwrap();
        assert_eq!(
            decode_document(&arena, extra_edge_manifest.owner.id()),
            Err(SerializedGreenError::Corrupt(
                "green manifest must own exactly one root"
            ))
        );
        arena.release_later(extra_edge_manifest.owner).unwrap();
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}
