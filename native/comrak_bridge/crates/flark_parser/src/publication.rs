//! Exact-clean parser output joined to the five-role candidate owner.
//!
//! `Green` and `Projection` use deliberately narrow M1.1 records, while
//! `SourceFacts` publication retains the runtime-owned persistent measured
//! sequence directly. Every role is joined to the exact certified source and
//! terminal parse result; no predictive substitute or digest-only fact record
//! can enter publication.

use std::collections::VecDeque;
use std::fmt;

use flark_engine::parser_internal::{
    M11BlockRoleRecord, M11BlockSequenceBuild, M11BlockSequenceBuildStatus, M11BlockSequenceEntry,
    M11BlockSequenceEntryKind, M11BlockSequenceError, M11BlockSequenceLocation,
    M11BlockSequencePoint, M11BlockSequenceQueryReceipt, M11BlockSequenceRoot,
    M11BlockSequenceSpliceReceipt, M11BlockSequenceSpliceSelection, M11BlockUnsupportedReason,
    M11CandidateBuild, M11CandidateBuildPoll, M11CandidateDescriptor, M11CandidatePublication,
    M11InlineProjectionDescriptor, M11InlineProjectionRoot, M11ParserSourceRangeAuthority,
    M11PublicationError, M11ReferenceRange, M11RetainedBlockVisitControl,
    M11RetainedBlockVisitDisposition, M11RetainedBlockVisitReceipt, M11RetainedBlockVisitStart,
    M11RetainedCandidatePublication, M11RoleRecords, M11_SINGLE_RECORD_MAX_BYTES,
};
use flark_engine::{
    CertifiedSource, DocumentRuntime, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness,
    ParserProfileId, PersistentCertifiedSource, SourceBoundaryAffinity, SourceFactsCoverage,
    SourceFactsScanProfile, SourceSnapshotLease, SourceVersion, SOURCE_CURSOR_WINDOW_BYTES,
};

use crate::bullet_list_local_delta::M11BulletListLocalDeltaTerminal;
use crate::exact_clean::M11BlockQuoteDisposition;
use crate::exact_clean::M11BlockQuoteLineKind;
use crate::exact_clean::M11BlockQuoteLineMapping;
use crate::exact_clean::M11BlockQuoteParagraphMapping;
use crate::exact_clean::M11BlockQuoteUnsupportedReason;
use crate::exact_clean::M11BulletListItemMapping;
use crate::exact_clean::M11CleanDocumentOutcome;
use crate::exact_clean::M11ListUnsupportedReason;
use crate::exact_clean::M11OrderedListItemMapping;
use crate::exact_clean::M11OrdinaryParagraphCheckpointSeedCursor;
use crate::exact_clean::M11OrdinaryParagraphCropTerminal;
use crate::exact_clean::M11OrdinaryParagraphEofCropTerminal;
use crate::exact_clean::SourceCut;
use crate::inline_projection_job::M11InlineProjectionUnsupportedRecord;
use crate::persistent_recursive_green_session::{
    M11PersistentRecursiveGreenSession, M11PersistentRecursiveGreenSessionError,
    M11PersistentRecursiveGreenUpdate,
};
use crate::reference_cook::{
    CookReferencePlan, M11ReferenceCookReceipt, ReferenceCookError, ReferenceCookPoll,
    ReferenceCooker,
};
use crate::segmented_lexical::SegmentedLineScanner;
use crate::source_adapter::SnapshotLineRetainedPoll;
use crate::{
    M11CleanBlockController, M11CleanControllerError, M11CleanControllerFault,
    M11CleanDocumentResult, M11CleanLeaf, M11CleanLineAdmission, M11ExactController, M11LineEnding,
    M11OrdinaryParagraphBofCropPlan, M11OrdinaryParagraphBoundaryCropPlanError,
    M11OrdinaryParagraphCheckpointError, M11OrdinaryParagraphCropPlan,
    M11OrdinaryParagraphCropPlanError, M11OrdinaryParagraphEofCropPlan,
    M11OrdinaryParagraphEofCropSelection, M11OrdinaryParagraphRestartCheckpoint,
    M11OrdinaryParagraphRestartCheckpoints, M11ParserBinding, M11PhysicalLineFacts,
    M11SourceLinePollStatus, M11SourceLineSource, M11UnknownReason, M11UnsupportedOpener,
    SnapshotLineScanner, SnapshotLineSource, SnapshotPhysicalLine, SourceAdapterError,
    M11_GRAMMAR_REVISION,
};

const GREEN_MAGIC: &[u8; 8] = b"FLKGR001";
const PROJECTION_MAGIC: &[u8; 8] = b"FLKPR001";
pub const M11_INLINE_META_MAGIC: &[u8; 8] = b"FLKIN002";
pub const M11_INLINE_PAGE_MAGIC: &[u8; 8] = b"FLKIP002";
pub const M11_INLINE_SCHEMA: u32 = 2;
const ROLE_SCHEMA_V1: u32 = 1;
const FENCED_CODE_ROLE_VARIANT: u8 = 3;
const FENCED_CODE_CLOSED_FLAG: u64 = 1 << 16;
const FENCED_CODE_ABSENT_CUT: u32 = u32::MAX;
const ATX_HEADING_ROLE_VARIANT: u8 = 4;
const ATX_HEADING_CLOSED_FLAG: u64 = 1 << 8;
const ATX_HEADING_OPENING_INDENT_SHIFT: u32 = 9;
const ATX_HEADING_BOF_BOM_FLAG: u64 = 1 << 11;
const ATX_HEADING_ABSENT_CUT: u32 = u32::MAX;
const SETEXT_HEADING_ROLE_VARIANT: u8 = 5;
const SETEXT_HEADING_OPENING_INDENT_SHIFT: u32 = 8;
const THEMATIC_BREAK_ROLE_VARIANT: u8 = 6;
const THEMATIC_BREAK_OPENING_INDENT_SHIFT: u32 = 8;
const THEMATIC_BREAK_BOF_BOM_FLAG: u64 = 1 << 10;
const INDENTED_CODE_ROLE_VARIANT: u8 = 7;
const INDENTED_CODE_DEINDENT_COLUMNS: u8 = 4;
const INDENTED_CODE_BOF_BOM_FLAG: u64 = 1 << 8;
const BLOCK_QUOTE_ROLE_VARIANT: u8 = 8;
const BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION: u64 = 1;
const BULLET_LIST_ROLE_VARIANT: u8 = 9;
const BULLET_LIST_EXACT_DISPOSITION: u64 = 1;
const BULLET_LIST_MARKER_SHIFT: u32 = 8;
const BULLET_LIST_TIGHT_FLAG: u64 = 1 << 16;
const BULLET_LIST_ABSENT_TERMINAL_EMPTY: u32 = u32::MAX;
const SELECTED_LIST_ITEM_INLINE_MAX_BYTES: usize = 8 * 1024;
const SELECTED_LIST_ITEM_PHYSICAL_WINDOW_MAX_BYTES: usize =
    SELECTED_LIST_ITEM_INLINE_MAX_BYTES + 16;
const ORDERED_LIST_ROLE_VARIANT: u8 = 10;
const ORDERED_LIST_EXACT_DISPOSITION: u64 = 1;
const ORDERED_LIST_DELIMITER_SHIFT: u32 = 8;
const ORDERED_LIST_TIGHT_FLAG: u64 = 1 << 16;
const ORDERED_LIST_ABSENT_TERMINAL_EMPTY: u32 = u32::MAX;
pub const M11_GREEN_RECORD_BYTES: usize = 80;
pub const M11_PROJECTION_RECORD_BYTES: usize = 56;
pub const M11_INLINE_META_RECORD_BYTES: usize = 48;
pub const M11_INLINE_PAGE_HEADER_BYTES: usize = 24;
/// Canonical bytes in one schema-v2 viewport inline fact.
pub const M11_INLINE_FACT_RECORD_BYTES: usize = 20;
pub const M11_INLINE_FACTS_PER_PAGE: usize =
    (M11_SINGLE_RECORD_MAX_BYTES - M11_INLINE_PAGE_HEADER_BYTES) / M11_INLINE_FACT_RECORD_BYTES;
pub const M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES: usize = 64 * 1024;

/// Resource or representation failure while admitting or encoding production
/// inline publication state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlinePublicationError {
    CoordinateOverflow,
    OverCap { bytes: usize, cap: usize },
    AllocationFailed,
}

impl fmt::Display for M11InlinePublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CoordinateOverflow => {
                formatter.write_str("inline publication coordinate exceeds the packed u32 schema")
            }
            Self::OverCap { bytes, cap } => {
                write!(
                    formatter,
                    "inline leaf has {bytes} bytes above the {cap}-byte admission cap"
                )
            }
            Self::AllocationFailed => formatter.write_str("inline publication allocation failed"),
        }
    }
}

impl std::error::Error for M11InlinePublicationError {}

/// Resource or authority failure while deriving a real M1.1 candidate.
#[derive(Debug)]
pub enum M11CandidateDerivationError {
    SourceAuthorityMismatch,
    ResultRangeMismatch,
    ParserProfileOverflow,
    ReusedReferencesRequireExactBase,
    ExactBaseReferencesRequired,
    InlinePublicationMismatch,
    PublishedInlineLeafFenceCorrupt(&'static str),
    PublishedIndentedCodeLeafFenceNotIndentedCode,
    PublishedIndentedCodeLeafFenceCorrupt(&'static str),
    PublishedBlockQuoteLeafFenceNotBlockQuote,
    PublishedBlockQuoteLeafFenceCorrupt(&'static str),
    PublishedBulletListLeafFenceNotBulletList,
    PublishedBulletListLeafFenceCorrupt(&'static str),
    PublishedOrderedListLeafFenceNotOrderedList,
    PublishedOrderedListLeafFenceCorrupt(&'static str),
    MetricOverflow,
    AllocationFailed,
    SegmentedCandidateRequiresOwningPath,
    RecursiveGreenPublicationMismatch,
    ReferenceCook(ReferenceCookError),
    PersistentRecursiveGreen(M11PersistentRecursiveGreenSessionError),
    InlinePublication(M11InlinePublicationError),
    BlockSequence(M11BlockSequenceError),
    Publication(M11PublicationError),
}

impl fmt::Display for M11CandidateDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceAuthorityMismatch => {
                formatter.write_str("clean parse and certification cross source authority")
            }
            Self::ResultRangeMismatch => {
                formatter.write_str("clean parse result does not cover the certified source")
            }
            Self::ParserProfileOverflow => {
                formatter.write_str("parser profile does not fit the M1.1 candidate schema")
            }
            Self::ReusedReferencesRequireExactBase => formatter
                .write_str("reused leading references require the exact-base candidate path"),
            Self::ExactBaseReferencesRequired => {
                formatter.write_str("exact-base candidate path requires reused reference authority")
            }
            Self::InlinePublicationMismatch => formatter
                .write_str("inline publication does not match the exact Paragraph authority"),
            Self::PublishedInlineLeafFenceCorrupt(message) => {
                write!(
                    formatter,
                    "published inline-leaf fence is corrupt: {message}"
                )
            }
            Self::PublishedIndentedCodeLeafFenceNotIndentedCode => {
                formatter.write_str("published block is not an indented-code leaf")
            }
            Self::PublishedIndentedCodeLeafFenceCorrupt(message) => {
                write!(
                    formatter,
                    "published indented-code leaf fence is corrupt: {message}"
                )
            }
            Self::PublishedBlockQuoteLeafFenceNotBlockQuote => {
                formatter.write_str("published block is not an exact block-quote leaf")
            }
            Self::PublishedBlockQuoteLeafFenceCorrupt(message) => {
                write!(
                    formatter,
                    "published block-quote leaf fence is corrupt: {message}"
                )
            }
            Self::PublishedBulletListLeafFenceNotBulletList => {
                formatter.write_str("published block is not an exact bullet-list leaf")
            }
            Self::PublishedBulletListLeafFenceCorrupt(message) => {
                write!(
                    formatter,
                    "published bullet-list leaf fence is corrupt: {message}"
                )
            }
            Self::PublishedOrderedListLeafFenceNotOrderedList => {
                formatter.write_str("published block is not an exact ordered-list leaf")
            }
            Self::PublishedOrderedListLeafFenceCorrupt(message) => {
                write!(
                    formatter,
                    "published ordered-list leaf fence is corrupt: {message}"
                )
            }
            Self::MetricOverflow => formatter.write_str("candidate metric overflowed"),
            Self::AllocationFailed => formatter.write_str("candidate record allocation failed"),
            Self::SegmentedCandidateRequiresOwningPath => formatter
                .write_str("segmented parser output requires the owning block publication path"),
            Self::RecursiveGreenPublicationMismatch => formatter.write_str(
                "recursive-Green candidate requires its dedicated persistent-root writer path",
            ),
            Self::ReferenceCook(error) => error.fmt(formatter),
            Self::PersistentRecursiveGreen(error) => error.fmt(formatter),
            Self::InlinePublication(error) => error.fmt(formatter),
            Self::BlockSequence(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11CandidateDerivationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReferenceCook(error) => Some(error),
            Self::PersistentRecursiveGreen(error) => Some(error),
            Self::InlinePublication(error) => Some(error),
            Self::BlockSequence(error) => Some(error),
            Self::Publication(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11PublicationError> for M11CandidateDerivationError {
    fn from(error: M11PublicationError) -> Self {
        Self::Publication(error)
    }
}

impl From<ReferenceCookError> for M11CandidateDerivationError {
    fn from(error: ReferenceCookError) -> Self {
        Self::ReferenceCook(error)
    }
}

impl From<M11PersistentRecursiveGreenSessionError> for M11CandidateDerivationError {
    fn from(error: M11PersistentRecursiveGreenSessionError) -> Self {
        Self::PersistentRecursiveGreen(error)
    }
}

impl From<M11InlinePublicationError> for M11CandidateDerivationError {
    fn from(error: M11InlinePublicationError) -> Self {
        Self::InlinePublication(error)
    }
}

impl From<M11BlockSequenceError> for M11CandidateDerivationError {
    fn from(error: M11BlockSequenceError) -> Self {
        Self::BlockSequence(error)
    }
}

/// Parser-owned role payload selector for equality and diagnostic queries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11CandidateRoleBytes {
    Green,
    Projection,
}

/// Definitive inline input for the parser-owned candidate publication seam.
///
/// `Authoritative` borrows the typed root only long enough to bind its exact
/// descriptor into the candidate. The endpoint keeps owning that root and
/// later supplies the same descriptor to
/// [`M11ParserCandidate::into_writer_with_inline_projection`].
pub enum M11ParserInlinePublication<'root> {
    NoInline,
    Authoritative(&'root M11InlineProjectionRoot),
    Unsupported(M11InlineProjectionUnsupportedRecord),
}

/// Fresh parser-derived terminal role facts.
#[derive(Debug, Eq, PartialEq)]
pub struct M11ParserTerminalFacts {
    green: Box<[u8]>,
    projection: Box<[u8]>,
}

impl M11ParserTerminalFacts {
    /// Encodes fresh Green and Projection facts from one exact terminal.
    pub fn derive(result: &M11CleanDocumentResult) -> Result<Self, M11CandidateDerivationError> {
        Ok(Self {
            green: encode_green(result)?,
            projection: encode_projection(result)?,
        })
    }

    #[must_use]
    pub fn green(&self) -> &[u8] {
        &self.green
    }

    #[must_use]
    pub fn projection(&self) -> &[u8] {
        &self.projection
    }

    pub(crate) fn into_parts(self) -> (Box<[u8]>, Box<[u8]>) {
        (self.green, self.projection)
    }
}

/// Complete parser-derived inputs for one candidate build.
pub struct M11ParserCandidate {
    source: SourceVersion,
    syntax_profile: u32,
    source_facts_profile: SourceFactsScanProfile,
    roles: M11CandidateRolePlan,
    references: Option<ReferenceCooker>,
    reuse_references: bool,
}

enum M11CandidateRolePlan {
    Flat {
        green: Box<[u8]>,
        projection: Vec<Box<[u8]>>,
        persistent_inline_projection: Option<M11InlineProjectionDescriptor>,
    },
    RecursiveGreen {
        projection: Vec<Box<[u8]>>,
    },
    Segmented {
        source_lease: SourceSnapshotLease,
        leaves: Vec<M11CleanLeaf>,
        prepared_replacement_entries: Vec<M11BlockSequenceEntry>,
        block_splice: Option<M11BlockSequenceSpliceSelection>,
        leaves_are_replacement: bool,
    },
}

/// Move-only proof that a successful parser crop may retain the exact base's
/// canonical References while rebuilding target block coverage.
///
/// Only authenticated crop result types can mint this value. A generic clean
/// terminal plus a merely similar source lease cannot opt into reference
/// reuse.
pub struct M11ExactSegmentedCandidateInput {
    source_lease: SourceSnapshotLease,
    leaves: Vec<M11CleanLeaf>,
    prepared_replacement_entries: Vec<M11BlockSequenceEntry>,
    block_splice: Option<M11BlockSequenceSpliceSelection>,
    leaves_are_replacement: bool,
}

impl M11ExactSegmentedCandidateInput {
    fn from_crop(
        source_lease: SourceSnapshotLease,
        terminal: M11CleanDocumentResult,
    ) -> Result<Self, M11CandidateDerivationError> {
        if source_lease.version() != terminal.source_version() {
            return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
        }
        validate_whole_source_result(source_lease.version(), &terminal)?;
        Ok(Self {
            source_lease,
            leaves: terminal.into_publication_leaves(),
            prepared_replacement_entries: Vec::new(),
            block_splice: None,
            leaves_are_replacement: false,
        })
    }

    fn from_ordinary_crop(
        source_lease: SourceSnapshotLease,
        terminal: M11CleanDocumentResult,
    ) -> Result<Self, M11CandidateDerivationError> {
        let mut input = Self::from_crop(source_lease, terminal)?;
        if input.leaves.len() != 1
            || !matches!(input.leaves.first(), Some(M11CleanLeaf::Paragraph { .. }))
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }
        input.block_splice = Some(M11BlockSequenceSpliceSelection::new(0..1, 0..1)?);
        Ok(input)
    }

    fn from_segmented_crop(
        source_lease: SourceSnapshotLease,
        leaves: Vec<M11CleanLeaf>,
        block_splice: M11BlockSequenceSpliceSelection,
    ) -> Result<Self, M11CandidateDerivationError> {
        let target = block_splice.target_entry_range();
        if target.end - target.start
            != u64::try_from(leaves.len())
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
            || leaves
                .iter()
                .any(|leaf| !leaf.is_definition_free_local_crop_leaf())
            || !leaves.windows(2).all(|pair| {
                pair[0].source_range().end == pair[1].source_range().start
                    && pair[0].source_utf16_range().end == pair[1].source_utf16_range().start
            })
            || leaves.last().is_some_and(|leaf| {
                leaf.source_range().end as usize > source_lease.version().byte_len()
                    || leaf.source_utf16_range().end as usize > source_lease.version().utf16_len()
            })
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }
        Ok(Self {
            source_lease,
            leaves,
            prepared_replacement_entries: Vec::new(),
            block_splice: Some(block_splice),
            leaves_are_replacement: true,
        })
    }

    /// Authenticates one checkpoint-free tight bullet-list summary and turns
    /// it into the existing exact-base block-splice input.
    ///
    /// The caller cannot supply role bytes. Both variant-9 records are encoded
    /// here from the parser terminal after its source ranges, scalar mapping,
    /// and compact structural invariants have been checked against the
    /// move-only target lease.
    pub fn from_bullet_list_local_delta(
        target_source: SourceSnapshotLease,
        terminal: &M11BulletListLocalDeltaTerminal,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = target_source.version();
        if source != terminal.source {
            return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
        }

        let byte_start = usize::try_from(terminal.list_source.start)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        let byte_end = usize::try_from(terminal.list_source.end)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        let utf16_start = usize::try_from(terminal.list_source_utf16.start)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        let utf16_end = usize::try_from(terminal.list_source_utf16.end)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        if byte_start >= byte_end
            || utf16_start >= utf16_end
            || byte_end > source.byte_len()
            || utf16_end > source.utf16_len()
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }

        let mapped_utf16_start = target_source
            .utf16_offset_for_byte(byte_start)
            .map_err(|_| M11CandidateDerivationError::ResultRangeMismatch)?;
        let mapped_utf16_end = target_source
            .utf16_offset_for_byte(byte_end)
            .map_err(|_| M11CandidateDerivationError::ResultRangeMismatch)?;
        let mapped_byte_start = target_source
            .byte_offset_for_utf16(utf16_start)
            .map_err(|_| M11CandidateDerivationError::ResultRangeMismatch)?;
        let mapped_byte_end = target_source
            .byte_offset_for_utf16(utf16_end)
            .map_err(|_| M11CandidateDerivationError::ResultRangeMismatch)?;
        if mapped_utf16_start != utf16_start
            || mapped_utf16_end != utf16_end
            || mapped_byte_start != byte_start
            || mapped_byte_end != byte_end
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }

        let source_bytes = terminal
            .list_source
            .end
            .checked_sub(terminal.list_source.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let source_utf16 = terminal
            .list_source_utf16
            .end
            .checked_sub(terminal.list_source_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let expected_paragraph_count = if terminal.terminal_empty_relative_start.is_some() {
            terminal.item_count.checked_sub(1)
        } else {
            Some(terminal.item_count)
        };
        if !matches!(terminal.marker, b'-' | b'+' | b'*')
            || terminal.item_count == 0
            || expected_paragraph_count != Some(terminal.paragraph_count)
            || terminal
                .terminal_empty_relative_start
                .is_some_and(|start| start >= source_bytes)
            || terminal.projected_utf8_length >= source_bytes
            || terminal.projected_utf16_length >= source_utf16
            || terminal.projected_utf16_length > terminal.projected_utf8_length
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }
        if let Some(relative) = terminal.terminal_empty_relative_start {
            let empty_start = byte_start
                .checked_add(
                    usize::try_from(relative)
                        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
                )
                .ok_or(M11CandidateDerivationError::MetricOverflow)?;
            if target_source.utf16_offset_for_byte(empty_start).is_err()
                || !target_source
                    .is_physical_line_start(empty_start)
                    .map_err(|_| M11CandidateDerivationError::ResultRangeMismatch)?
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
        }

        let terminal_empty_relative_start = terminal
            .terminal_empty_relative_start
            .unwrap_or(BULLET_LIST_ABSENT_TERMINAL_EMPTY);
        let green = encode_block_bullet_list_green(
            source_bytes,
            terminal.marker,
            terminal.item_count,
            terminal_empty_relative_start,
            terminal.paragraph_count,
            terminal.projected_utf8_length,
            terminal.projected_utf16_length,
        )?;
        let projection = encode_block_bullet_list_projection(source_bytes, terminal.item_count)?;
        let entry = M11BlockSequenceEntry::structured(
            usize::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
            usize::try_from(source_utf16)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
            0,
            green,
            projection,
        )?;
        let entry_end = terminal
            .block_entry_ordinal
            .checked_add(1)
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        let block_splice = M11BlockSequenceSpliceSelection::new(
            terminal.block_entry_ordinal..entry_end,
            terminal.block_entry_ordinal..entry_end,
        )?;
        let mut prepared_replacement_entries = Vec::new();
        prepared_replacement_entries
            .try_reserve_exact(1)
            .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
        prepared_replacement_entries.push(entry);
        Ok(Self {
            source_lease: target_source,
            leaves: Vec::new(),
            prepared_replacement_entries,
            block_splice: Some(block_splice),
            leaves_are_replacement: true,
        })
    }

    #[must_use]
    pub fn source(&self) -> SourceVersion {
        self.source_lease.version()
    }
}

/// Move-only exact authority for one inline-bearing leaf selected from the
/// persistent segmented block publication.
///
/// The parser mints this value only from a sealed retained candidate queried
/// with an exact byte/UTF-16 point. Its inline authority therefore cannot be
/// widened or redirected by a caller before
/// [`M11InlineProjectionJob`](crate::M11InlineProjectionJob) consumes it.
#[must_use = "published inline-leaf fences must be consumed by exact inline work or deliberately dropped"]
pub struct M11PublishedInlineLeafFence {
    source: SourceVersion,
    kind: M11BlockSequenceEntryKind,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    inline_source: std::ops::Range<u32>,
    inline_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedInlineLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedInlineLeafFence")
            .field("source", &self.source)
            .field("kind", &self.kind)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("inline_source", &self.inline_source)
            .field("inline_source_utf16", &self.inline_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .finish_non_exhaustive()
    }
}

impl M11PublishedInlineLeafFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn kind(&self) -> M11BlockSequenceEntryKind {
        self.kind
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> std::ops::Range<u32> {
        self.inline_source.clone()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.inline_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    pub(crate) fn into_inline_authority(
        self,
    ) -> (
        M11ParserSourceRangeAuthority,
        M11ParserBinding,
        std::ops::Range<u32>,
    ) {
        (self.authority, self.binding, self.inline_source)
    }
}

/// Explicit admission bounds for one consecutive retained-publication inline
/// range selection.
///
/// Structural entries and inline-bearing leaves are intentionally independent:
/// blank and unsupported entries consume structural work without consuming an
/// inline-leaf slot. Storage pages bound topology work, while inline source
/// bytes bound the exact parser workload admitted by the resulting fences.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PublishedInlineRangeLimits {
    maximum_structural_entries: u32,
    maximum_storage_pages: u32,
    maximum_inline_leaves: u32,
    maximum_inline_source_bytes: u64,
}

impl M11PublishedInlineRangeLimits {
    #[must_use]
    pub const fn new(
        maximum_structural_entries: u32,
        maximum_storage_pages: u32,
        maximum_inline_leaves: u32,
        maximum_inline_source_bytes: u64,
    ) -> Option<Self> {
        if maximum_structural_entries == 0
            || maximum_storage_pages == 0
            || maximum_inline_leaves == 0
            || maximum_inline_source_bytes == 0
        {
            return None;
        }
        Some(Self {
            maximum_structural_entries,
            maximum_storage_pages,
            maximum_inline_leaves,
            maximum_inline_source_bytes,
        })
    }

    #[must_use]
    pub const fn maximum_structural_entries(self) -> u32 {
        self.maximum_structural_entries
    }

    #[must_use]
    pub const fn maximum_storage_pages(self) -> u32 {
        self.maximum_storage_pages
    }

    #[must_use]
    pub const fn maximum_inline_leaves(self) -> u32 {
        self.maximum_inline_leaves
    }

    #[must_use]
    pub const fn maximum_inline_source_bytes(self) -> u64 {
        self.maximum_inline_source_bytes
    }
}

/// Exact terminal source cut for one bounded retained-publication inline
/// range.
///
/// Both coordinates form one parser-authenticated claim. The resolver stops
/// only after visiting the structural entry whose end matches both values;
/// it never infers a terminal cut from an inline-leaf count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PublishedInlineRangeEnd {
    byte_offset: u64,
    utf16_offset: u64,
}

impl M11PublishedInlineRangeEnd {
    #[must_use]
    pub const fn new(byte_offset: u64, utf16_offset: u64) -> Self {
        Self {
            byte_offset,
            utf16_offset,
        }
    }

    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.byte_offset
    }

    #[must_use]
    pub const fn utf16_offset(self) -> u64 {
        self.utf16_offset
    }
}

/// Move-only exact inline authority minted by one authenticated consecutive
/// retained-publication visit rather than one point query.
///
/// Keeping this distinct from [`M11PublishedInlineLeafFence`] prevents a
/// range receipt from masquerading as a per-leaf point-query receipt while
/// preserving the same exact source-range parser authority.
#[must_use = "published inline range fences must be consumed by exact inline work or deliberately dropped"]
pub struct M11PublishedInlineRangeLeafFence {
    source: SourceVersion,
    kind: M11BlockSequenceEntryKind,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    inline_source: std::ops::Range<u32>,
    inline_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    authority: M11ParserSourceRangeAuthority,
}

impl M11PublishedInlineRangeLeafFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn kind(&self) -> M11BlockSequenceEntryKind {
        self.kind
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> std::ops::Range<u32> {
        self.inline_source.clone()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.inline_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    pub(crate) fn into_inline_authority(
        self,
    ) -> (
        M11ParserSourceRangeAuthority,
        M11ParserBinding,
        std::ops::Range<u32>,
    ) {
        (self.authority, self.binding, self.inline_source)
    }
}

/// One bounded set of exact inline leaf authorities selected by a single
/// authenticated structural range walk.
///
/// `descriptor` is the sealed producer identity that the bridge must compare
/// with its exact installed structural acknowledgement before starting or
/// accepting this sibling presentation work. The range receipt carries the
/// exact continuation cut; no packed-page cursor escapes the engine.
#[must_use = "published inline range batches contain move-only exact source authorities"]
pub struct M11PublishedInlineRangeBatch {
    descriptor: M11CandidateDescriptor,
    limits: M11PublishedInlineRangeLimits,
    receipt: M11RetainedBlockVisitReceipt,
    total_inline_source_bytes: u64,
    fences: Vec<M11PublishedInlineRangeLeafFence>,
}

impl M11PublishedInlineRangeBatch {
    #[must_use]
    pub const fn descriptor(&self) -> M11CandidateDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn limits(&self) -> M11PublishedInlineRangeLimits {
        self.limits
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RetainedBlockVisitReceipt {
        self.receipt
    }

    #[must_use]
    pub const fn total_inline_source_bytes(&self) -> u64 {
        self.total_inline_source_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.fences.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fences.is_empty()
    }

    #[must_use]
    pub fn into_fences(self) -> Vec<M11PublishedInlineRangeLeafFence> {
        self.fences
    }
}

#[derive(Debug)]
pub enum M11PublishedInlineRangeError {
    Derivation(M11CandidateDerivationError),
    EndCutMismatch {
        expected_byte_offset: u64,
        expected_utf16_offset: u64,
        actual_byte_offset: u64,
        actual_utf16_offset: u64,
    },
    StructuralEntryLimitExceeded {
        maximum: u32,
    },
    StoragePageLimitExceeded {
        maximum: u32,
    },
    InlineLeafLimitExceeded {
        maximum: u32,
        required_through_leaf: u64,
    },
    InlineSourceByteLimitExceeded {
        maximum: u64,
        required_through_leaf: u64,
    },
}

impl fmt::Display for M11PublishedInlineRangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Derivation(error) => error.fmt(formatter),
            Self::EndCutMismatch {
                expected_byte_offset,
                expected_utf16_offset,
                actual_byte_offset,
                actual_utf16_offset,
            } => write!(
                formatter,
                "published inline range ended at byte/UTF-16 \
                 {actual_byte_offset}/{actual_utf16_offset}, not the exact \
                 requested cut {expected_byte_offset}/{expected_utf16_offset}"
            ),
            Self::StructuralEntryLimitExceeded { maximum } => write!(
                formatter,
                "published inline range exceeded its admitted {maximum} \
                 structural entries before the exact end cut"
            ),
            Self::StoragePageLimitExceeded { maximum } => write!(
                formatter,
                "published inline range exceeded its admitted {maximum} \
                 storage pages before the exact end cut"
            ),
            Self::InlineLeafLimitExceeded {
                maximum,
                required_through_leaf,
            } => write!(
                formatter,
                "published inline range requires {required_through_leaf} \
                 inline leaves, exceeding the admitted {maximum}"
            ),
            Self::InlineSourceByteLimitExceeded {
                maximum,
                required_through_leaf,
            } => write!(
                formatter,
                "published inline range requires {required_through_leaf} source bytes through \
                 the current leaf, exceeding the admitted {maximum}"
            ),
        }
    }
}

impl std::error::Error for M11PublishedInlineRangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Derivation(error) => Some(error),
            Self::EndCutMismatch { .. }
            | Self::StructuralEntryLimitExceeded { .. }
            | Self::StoragePageLimitExceeded { .. }
            | Self::InlineLeafLimitExceeded { .. }
            | Self::InlineSourceByteLimitExceeded { .. } => None,
        }
    }
}

impl From<M11CandidateDerivationError> for M11PublishedInlineRangeError {
    fn from(error: M11CandidateDerivationError) -> Self {
        Self::Derivation(error)
    }
}

/// Result of resolving one late hot point against retained exact block
/// coverage.
#[derive(Debug)]
pub enum M11PublishedInlineLeafFenceResolution {
    InlineLeaf(M11PublishedInlineLeafFence),
    NotInlineLeaf {
        kind: M11BlockSequenceEntryKind,
        entry_ordinal: u64,
        source: std::ops::Range<u32>,
        source_utf16: std::ops::Range<u32>,
        query_receipt: M11BlockSequenceQueryReceipt,
    },
}

/// Move-only exact authority for one published top-level indented-code leaf.
///
/// The source range and structural summary are decoded from the retained
/// variant-7 Green and Projection records. Callers cannot widen the private
/// range authority before handing this fence to
/// [`M11IndentedCodeProjectionJob`](crate::M11IndentedCodeProjectionJob).
#[must_use = "published indented-code fences must be consumed by exact projection work or deliberately dropped"]
pub struct M11PublishedIndentedCodeLeafFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    terminal_eol_bytes: u32,
    has_bof_bom: bool,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedIndentedCodeLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedIndentedCodeLeafFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .field("line_count", &self.line_count)
            .field("projected_utf8_length", &self.projected_utf8_length)
            .field("projected_utf16_length", &self.projected_utf16_length)
            .field("terminal_eol_bytes", &self.terminal_eol_bytes)
            .field("has_bof_bom", &self.has_bof_bom)
            .finish_non_exhaustive()
    }
}

impl M11PublishedIndentedCodeLeafFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn line_count(&self) -> u32 {
        self.line_count
    }

    #[must_use]
    pub const fn projected_utf8_length(&self) -> u32 {
        self.projected_utf8_length
    }

    #[must_use]
    pub const fn projected_utf16_length(&self) -> u32 {
        self.projected_utf16_length
    }

    #[must_use]
    pub const fn terminal_eol_bytes(&self) -> u32 {
        self.terminal_eol_bytes
    }

    #[must_use]
    pub const fn has_bof_bom(&self) -> bool {
        self.has_bof_bom
    }

    pub(crate) fn into_projection_authority(self) -> PublishedIndentedCodeProjectionAuthority {
        PublishedIndentedCodeProjectionAuthority {
            source: self.source,
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            binding: self.binding,
            line_count: self.line_count,
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
            terminal_eol_bytes: self.terminal_eol_bytes,
            has_bof_bom: self.has_bof_bom,
            authority: self.authority,
        }
    }
}

pub(crate) struct PublishedIndentedCodeProjectionAuthority {
    pub(crate) source: SourceVersion,
    pub(crate) block_source: std::ops::Range<u32>,
    pub(crate) block_source_utf16: std::ops::Range<u32>,
    pub(crate) binding: M11ParserBinding,
    pub(crate) line_count: u32,
    pub(crate) projected_utf8_length: u32,
    pub(crate) projected_utf16_length: u32,
    pub(crate) terminal_eol_bytes: u32,
    pub(crate) has_bof_bom: bool,
    pub(crate) authority: M11ParserSourceRangeAuthority,
}

/// Move-only exact authority for one published top-level block-quote leaf.
///
/// The source range and single-paragraph path summary are decoded from the
/// retained variant-8 Green and Projection records. The private source-range
/// authority prevents callers from widening the selected block before a later
/// parser-owned projection job consumes the fence.
#[must_use = "published block-quote fences must be consumed by exact projection work or deliberately dropped"]
pub struct M11PublishedBlockQuoteLeafFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedBlockQuoteLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedBlockQuoteLeafFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .field("line_count", &self.line_count)
            .field("projected_utf8_length", &self.projected_utf8_length)
            .field("projected_utf16_length", &self.projected_utf16_length)
            .finish_non_exhaustive()
    }
}

impl M11PublishedBlockQuoteLeafFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn line_count(&self) -> u32 {
        self.line_count
    }

    #[must_use]
    pub const fn projected_utf8_length(&self) -> u32 {
        self.projected_utf8_length
    }

    #[must_use]
    pub const fn projected_utf16_length(&self) -> u32 {
        self.projected_utf16_length
    }

    pub(crate) fn into_projection_authority(self) -> PublishedBlockQuoteProjectionAuthority {
        PublishedBlockQuoteProjectionAuthority {
            source: self.source,
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            binding: self.binding,
            line_count: self.line_count,
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
            authority: self.authority,
        }
    }
}

pub(crate) struct PublishedBlockQuoteProjectionAuthority {
    pub(crate) source: SourceVersion,
    pub(crate) block_source: std::ops::Range<u32>,
    pub(crate) block_source_utf16: std::ops::Range<u32>,
    pub(crate) binding: M11ParserBinding,
    pub(crate) line_count: u32,
    pub(crate) projected_utf8_length: u32,
    pub(crate) projected_utf16_length: u32,
    pub(crate) authority: M11ParserSourceRangeAuthority,
}

/// Move-only exact authority for one published top-level tight bullet list.
///
/// The complete list envelope and its structural summary are decoded from the
/// retained variant-9 Green and Projection records. The private source-range
/// authority prevents a caller from widening or redirecting the selected list
/// before parser-owned item projection consumes it.
#[must_use = "published bullet-list fences must be consumed by exact projection work or deliberately dropped"]
pub struct M11PublishedBulletListLeafFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    item_count: u32,
    paragraph_count: u32,
    marker: u8,
    terminal_empty_relative_start: Option<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedBulletListLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedBulletListLeafFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .field("item_count", &self.item_count)
            .field("paragraph_count", &self.paragraph_count)
            .field("marker", &char::from(self.marker))
            .field(
                "terminal_empty_relative_start",
                &self.terminal_empty_relative_start,
            )
            .field("projected_utf8_length", &self.projected_utf8_length)
            .field("projected_utf16_length", &self.projected_utf16_length)
            .finish_non_exhaustive()
    }
}

impl M11PublishedBulletListLeafFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn item_count(&self) -> u32 {
        self.item_count
    }

    #[must_use]
    pub const fn paragraph_count(&self) -> u32 {
        self.paragraph_count
    }

    #[must_use]
    pub const fn marker(&self) -> u8 {
        self.marker
    }

    #[must_use]
    pub const fn terminal_empty_relative_start(&self) -> Option<u32> {
        self.terminal_empty_relative_start
    }

    #[must_use]
    pub const fn projected_utf8_length(&self) -> u32 {
        self.projected_utf8_length
    }

    #[must_use]
    pub const fn projected_utf16_length(&self) -> u32 {
        self.projected_utf16_length
    }

    pub(crate) fn into_projection_authority(self) -> PublishedBulletListProjectionAuthority {
        PublishedBulletListProjectionAuthority {
            source: self.source,
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            entry_ordinal: self.entry_ordinal,
            binding: self.binding,
            item_count: self.item_count,
            paragraph_count: self.paragraph_count,
            marker: self.marker,
            terminal_empty_relative_start: self.terminal_empty_relative_start,
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
            authority: self.authority,
        }
    }
}

/// Exact selected-item metadata paired with the existing move-only inline
/// projection fence.
///
/// The top-level block range remains the complete published list while the
/// private inline authority is narrowed to this item's nonempty content. That
/// keeps the result joinable with list-level projection caches without
/// allowing callers to widen the inline parse.
#[must_use = "published list-item inline fences must be consumed by inline projection or deliberately dropped"]
pub struct M11PublishedBulletListItemInlineFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    item_ordinal: u32,
    item_source: std::ops::Range<u32>,
    item_source_utf16: std::ops::Range<u32>,
    content_source: std::ops::Range<u32>,
    content_source_utf16: std::ops::Range<u32>,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    projection_fence: M11PublishedBulletListItemProjectionFence,
    inline_leaf_fence: M11PublishedInlineLeafFence,
}

impl fmt::Debug for M11PublishedBulletListItemInlineFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedBulletListItemInlineFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("item_ordinal", &self.item_ordinal)
            .field("item_source", &self.item_source)
            .field("item_source_utf16", &self.item_source_utf16)
            .field("content_source", &self.content_source)
            .field("content_source_utf16", &self.content_source_utf16)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .finish_non_exhaustive()
    }
}

impl M11PublishedBulletListItemInlineFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn item_ordinal(&self) -> u32 {
        self.item_ordinal
    }

    #[must_use]
    pub fn item_source_range(&self) -> std::ops::Range<u32> {
        self.item_source.clone()
    }

    #[must_use]
    pub fn item_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.item_source_utf16.clone()
    }

    #[must_use]
    pub fn content_source_range(&self) -> std::ops::Range<u32> {
        self.content_source.clone()
    }

    #[must_use]
    pub fn content_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.content_source_utf16.clone()
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    /// Splits the one parser resolution into compact item-structure authority
    /// and the existing content-only inline authority.
    #[must_use]
    pub fn into_projection_and_inline_fences(
        self,
    ) -> (
        M11PublishedBulletListItemProjectionFence,
        M11PublishedInlineLeafFence,
    ) {
        (self.projection_fence, self.inline_leaf_fence)
    }

    #[must_use]
    pub fn into_inline_leaf_fence(self) -> M11PublishedInlineLeafFence {
        self.inline_leaf_fence
    }
}

/// Exact published location of the one terminal marker-only item admitted by
/// the current tight-list subset.
///
/// This is deliberately a no-fence outcome: there are no content bytes to
/// hand to the inline parser.
pub struct M11PublishedBulletListTerminalEmpty {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    item_ordinal: u32,
    item_source: std::ops::Range<u32>,
    item_source_utf16: std::ops::Range<u32>,
    content_source: std::ops::Range<u32>,
    content_source_utf16: std::ops::Range<u32>,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    projection_fence: M11PublishedBulletListItemProjectionFence,
}

impl fmt::Debug for M11PublishedBulletListTerminalEmpty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedBulletListTerminalEmpty")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("item_ordinal", &self.item_ordinal)
            .field("item_source", &self.item_source)
            .field("item_source_utf16", &self.item_source_utf16)
            .field("content_source", &self.content_source)
            .field("content_source_utf16", &self.content_source_utf16)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .finish_non_exhaustive()
    }
}

impl M11PublishedBulletListTerminalEmpty {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn item_ordinal(&self) -> u32 {
        self.item_ordinal
    }

    #[must_use]
    pub fn item_source_range(&self) -> std::ops::Range<u32> {
        self.item_source.clone()
    }

    #[must_use]
    pub fn item_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.item_source_utf16.clone()
    }

    #[must_use]
    pub fn content_source_range(&self) -> std::ops::Range<u32> {
        self.content_source.clone()
    }

    #[must_use]
    pub fn content_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.content_source_utf16.clone()
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub fn into_projection_fence(self) -> M11PublishedBulletListItemProjectionFence {
        self.projection_fence
    }
}

/// Selected-item inline resolution for one exact published tight bullet list.
#[derive(Debug)]
pub enum M11PublishedBulletListItemInlineFenceOutcome {
    Inline(M11PublishedBulletListItemInlineFence),
    TerminalEmpty(M11PublishedBulletListTerminalEmpty),
}

pub(crate) struct PublishedBulletListProjectionAuthority {
    pub(crate) source: SourceVersion,
    pub(crate) block_source: std::ops::Range<u32>,
    pub(crate) block_source_utf16: std::ops::Range<u32>,
    pub(crate) entry_ordinal: u64,
    pub(crate) binding: M11ParserBinding,
    pub(crate) item_count: u32,
    pub(crate) paragraph_count: u32,
    pub(crate) marker: u8,
    pub(crate) terminal_empty_relative_start: Option<u32>,
    pub(crate) projected_utf8_length: u32,
    pub(crate) projected_utf16_length: u32,
    pub(crate) authority: M11ParserSourceRangeAuthority,
}

/// Move-only exact authority for one published top-level tight ordered list.
///
/// Variant 10 stays distinct from the established bullet-list schema. The
/// list-level start and delimiter are authenticated here; literal per-item
/// marker spelling remains parser-owned selected-item geometry.
#[must_use = "published ordered-list fences must be consumed by exact projection work or deliberately dropped"]
pub struct M11PublishedOrderedListLeafFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    item_count: u32,
    paragraph_count: u32,
    start: u32,
    delimiter: u8,
    terminal_empty_relative_start: Option<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedOrderedListLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedOrderedListLeafFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .field("item_count", &self.item_count)
            .field("paragraph_count", &self.paragraph_count)
            .field("start", &self.start)
            .field("delimiter", &char::from(self.delimiter))
            .field(
                "terminal_empty_relative_start",
                &self.terminal_empty_relative_start,
            )
            .field("projected_utf8_length", &self.projected_utf8_length)
            .field("projected_utf16_length", &self.projected_utf16_length)
            .finish_non_exhaustive()
    }
}

impl M11PublishedOrderedListLeafFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn item_count(&self) -> u32 {
        self.item_count
    }

    #[must_use]
    pub const fn paragraph_count(&self) -> u32 {
        self.paragraph_count
    }

    #[must_use]
    pub const fn start(&self) -> u32 {
        self.start
    }

    #[must_use]
    pub const fn delimiter(&self) -> u8 {
        self.delimiter
    }

    #[must_use]
    pub const fn terminal_empty_relative_start(&self) -> Option<u32> {
        self.terminal_empty_relative_start
    }

    #[must_use]
    pub const fn projected_utf8_length(&self) -> u32 {
        self.projected_utf8_length
    }

    #[must_use]
    pub const fn projected_utf16_length(&self) -> u32 {
        self.projected_utf16_length
    }

    pub(crate) fn into_projection_authority(self) -> PublishedOrderedListProjectionAuthority {
        PublishedOrderedListProjectionAuthority {
            source: self.source,
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            entry_ordinal: self.entry_ordinal,
            binding: self.binding,
            item_count: self.item_count,
            paragraph_count: self.paragraph_count,
            start: self.start,
            delimiter: self.delimiter,
            terminal_empty_relative_start: self.terminal_empty_relative_start,
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
            authority: self.authority,
        }
    }
}

pub(crate) struct PublishedOrderedListProjectionAuthority {
    pub(crate) source: SourceVersion,
    pub(crate) block_source: std::ops::Range<u32>,
    pub(crate) block_source_utf16: std::ops::Range<u32>,
    pub(crate) entry_ordinal: u64,
    pub(crate) binding: M11ParserBinding,
    pub(crate) item_count: u32,
    pub(crate) paragraph_count: u32,
    pub(crate) start: u32,
    pub(crate) delimiter: u8,
    pub(crate) terminal_empty_relative_start: Option<u32>,
    pub(crate) projected_utf8_length: u32,
    pub(crate) projected_utf16_length: u32,
    pub(crate) authority: M11ParserSourceRangeAuthority,
}

/// Move-only parser-certified geometry for one selected item inside a
/// published top-level tight bullet list.
///
/// The full list remains the physical structural authority while
/// [M11PublishedBulletListItemProjectionFence::item_source_range] is the only
/// requested projection window. The fence owns source authority for exactly
/// that item, so a projection job cannot widen the selected work back to the
/// enclosing list.
#[must_use = "published list-item projection fences must be consumed by compact projection work or deliberately dropped"]
pub struct M11PublishedBulletListItemProjectionFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    item: M11BulletListItemMapping,
    physical_line_ending: M11LineEnding,
    canonical_line_ending: M11LineEnding,
    source_discovery_bytes: u32,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedBulletListItemProjectionFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedBulletListItemProjectionFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .field("item", &self.item)
            .field("physical_line_ending", &self.physical_line_ending)
            .field("canonical_line_ending", &self.canonical_line_ending)
            .field("source_discovery_bytes", &self.source_discovery_bytes)
            .finish_non_exhaustive()
    }
}

impl M11PublishedBulletListItemProjectionFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn item_ordinal(&self) -> u32 {
        self.item.ordinal
    }

    #[must_use]
    pub fn item_source_range(&self) -> std::ops::Range<u32> {
        self.item.source.clone()
    }

    #[must_use]
    pub fn item_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.item.source_utf16.clone()
    }

    #[must_use]
    pub fn content_source_range(&self) -> std::ops::Range<u32> {
        self.item.content_source.clone()
    }

    #[must_use]
    pub fn content_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.item.content_source_utf16.clone()
    }

    #[must_use]
    pub const fn physical_line_ending(&self) -> M11LineEnding {
        self.physical_line_ending
    }

    /// Parser-authored line ending for future continuation edits.
    ///
    /// A selected physical EOF line inherits the immediate predecessor's
    /// authenticated terminator when one exists; only a first-item EOF falls
    /// back to LF. Non-EOF LF/CRLF/CR spellings remain exact.
    #[must_use]
    pub const fn canonical_line_ending(&self) -> M11LineEnding {
        self.canonical_line_ending
    }

    #[must_use]
    pub const fn terminal_empty(&self) -> bool {
        self.item.paragraph.is_none()
    }

    /// Bytes inspected by physical-line discovery for this resolution.
    ///
    /// This equals exactly the selected physical item width. EOF continuation
    /// policy uses the predecessor rank/select receipt's O(1) terminator fact,
    /// so discovery remains independent of both the predecessor and
    /// enclosing-list lengths.
    #[must_use]
    pub const fn source_discovery_bytes(&self) -> u32 {
        self.source_discovery_bytes
    }

    pub(crate) fn into_projection_authority(self) -> PublishedBulletListItemProjectionAuthority {
        PublishedBulletListItemProjectionAuthority {
            source: self.source,
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            binding: self.binding,
            item: self.item,
            physical_line_ending: self.physical_line_ending,
            canonical_line_ending: self.canonical_line_ending,
            authority: self.authority,
        }
    }
}

pub(crate) struct PublishedBulletListItemProjectionAuthority {
    pub(crate) source: SourceVersion,
    pub(crate) block_source: std::ops::Range<u32>,
    pub(crate) block_source_utf16: std::ops::Range<u32>,
    pub(crate) binding: M11ParserBinding,
    pub(crate) item: M11BulletListItemMapping,
    pub(crate) physical_line_ending: M11LineEnding,
    pub(crate) canonical_line_ending: M11LineEnding,
    pub(crate) authority: M11ParserSourceRangeAuthority,
}

/// Move-only parser-certified geometry for one selected ordered-list item.
#[must_use = "published ordered-list item projection fences must be consumed or deliberately dropped"]
pub struct M11PublishedOrderedListItemProjectionFence {
    source: SourceVersion,
    block_source: std::ops::Range<u32>,
    block_source_utf16: std::ops::Range<u32>,
    entry_ordinal: u64,
    binding: M11ParserBinding,
    query_receipt: M11BlockSequenceQueryReceipt,
    item: M11OrderedListItemMapping,
    physical_line_ending: M11LineEnding,
    canonical_line_ending: M11LineEnding,
    source_discovery_bytes: u32,
    authority: M11ParserSourceRangeAuthority,
}

impl fmt::Debug for M11PublishedOrderedListItemProjectionFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedOrderedListItemProjectionFence")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("entry_ordinal", &self.entry_ordinal)
            .field("binding", &self.binding)
            .field("query_receipt", &self.query_receipt)
            .field("item", &self.item)
            .field("physical_line_ending", &self.physical_line_ending)
            .field("canonical_line_ending", &self.canonical_line_ending)
            .field("source_discovery_bytes", &self.source_discovery_bytes)
            .finish_non_exhaustive()
    }
}

impl M11PublishedOrderedListItemProjectionFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn block_source_range(&self) -> std::ops::Range<u32> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn query_receipt(&self) -> M11BlockSequenceQueryReceipt {
        self.query_receipt
    }

    #[must_use]
    pub const fn item_ordinal(&self) -> u32 {
        self.item.ordinal
    }

    #[must_use]
    pub fn item_source_range(&self) -> std::ops::Range<u32> {
        self.item.source.clone()
    }

    #[must_use]
    pub fn item_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.item.source_utf16.clone()
    }

    #[must_use]
    pub fn content_source_range(&self) -> std::ops::Range<u32> {
        self.item.content_source.clone()
    }

    #[must_use]
    pub fn content_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.item.content_source_utf16.clone()
    }

    #[must_use]
    pub fn opening_marker_range(&self) -> std::ops::Range<u32> {
        self.item.opening_marker.clone()
    }

    #[must_use]
    pub const fn marker_value(&self) -> u32 {
        self.item.marker_value
    }

    #[must_use]
    pub const fn delimiter(&self) -> u8 {
        self.item.delimiter
    }

    #[must_use]
    pub const fn physical_line_ending(&self) -> M11LineEnding {
        self.physical_line_ending
    }

    #[must_use]
    pub const fn canonical_line_ending(&self) -> M11LineEnding {
        self.canonical_line_ending
    }

    #[must_use]
    pub const fn terminal_empty(&self) -> bool {
        self.item.paragraph.is_none()
    }

    #[must_use]
    pub const fn source_discovery_bytes(&self) -> u32 {
        self.source_discovery_bytes
    }

    pub(crate) fn into_projection_authority(self) -> PublishedOrderedListItemProjectionAuthority {
        PublishedOrderedListItemProjectionAuthority {
            source: self.source,
            block_source: self.block_source,
            block_source_utf16: self.block_source_utf16,
            binding: self.binding,
            item: self.item,
            physical_line_ending: self.physical_line_ending,
            canonical_line_ending: self.canonical_line_ending,
            authority: self.authority,
        }
    }
}

pub(crate) struct PublishedOrderedListItemProjectionAuthority {
    pub(crate) source: SourceVersion,
    pub(crate) block_source: std::ops::Range<u32>,
    pub(crate) block_source_utf16: std::ops::Range<u32>,
    pub(crate) binding: M11ParserBinding,
    pub(crate) item: M11OrderedListItemMapping,
    pub(crate) physical_line_ending: M11LineEnding,
    pub(crate) canonical_line_ending: M11LineEnding,
    pub(crate) authority: M11ParserSourceRangeAuthority,
}

#[must_use = "published ordered-list item inline fences must be consumed or deliberately dropped"]
pub struct M11PublishedOrderedListItemInlineFence {
    projection_fence: M11PublishedOrderedListItemProjectionFence,
    inline_leaf_fence: M11PublishedInlineLeafFence,
}

impl fmt::Debug for M11PublishedOrderedListItemInlineFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedOrderedListItemInlineFence")
            .field("projection_fence", &self.projection_fence)
            .finish_non_exhaustive()
    }
}

impl M11PublishedOrderedListItemInlineFence {
    #[must_use]
    pub fn into_projection_and_inline_fences(
        self,
    ) -> (
        M11PublishedOrderedListItemProjectionFence,
        M11PublishedInlineLeafFence,
    ) {
        (self.projection_fence, self.inline_leaf_fence)
    }

    #[must_use]
    pub fn into_inline_leaf_fence(self) -> M11PublishedInlineLeafFence {
        self.inline_leaf_fence
    }
}

pub struct M11PublishedOrderedListTerminalEmpty {
    projection_fence: M11PublishedOrderedListItemProjectionFence,
}

impl fmt::Debug for M11PublishedOrderedListTerminalEmpty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PublishedOrderedListTerminalEmpty")
            .field("projection_fence", &self.projection_fence)
            .finish_non_exhaustive()
    }
}

impl M11PublishedOrderedListTerminalEmpty {
    #[must_use]
    pub fn into_projection_fence(self) -> M11PublishedOrderedListItemProjectionFence {
        self.projection_fence
    }
}

#[derive(Debug)]
pub enum M11PublishedOrderedListItemInlineFenceOutcome {
    Inline(M11PublishedOrderedListItemInlineFence),
    TerminalEmpty(M11PublishedOrderedListTerminalEmpty),
}

impl M11ParserCandidate {
    /// Derives ordinary Projection and References records for a candidate
    /// whose canonical Green role will retain a session-owned recursive tree.
    ///
    /// No materialized Green summary is encoded. The resulting candidate can
    /// only be transferred through [`Self::into_writer_with_recursive_green`].
    pub fn derive_with_recursive_green(
        certified: CertifiedSource,
        result: &M11CleanDocumentResult,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = certified.source();
        validate_whole_source_result(source, result)?;
        if certified.coverage() != SourceFactsCoverage::CleanEof
            || certified.facts().source() != source
        {
            return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
        }
        if result.reuses_leading_references() {
            return Err(M11CandidateDerivationError::ReusedReferencesRequireExactBase);
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        let source_facts_profile = certified.facts().profile();
        let projection = encode_recursive_green_projection(source)?;
        let definitions = result_definitions(result);
        let (lease, certified_profile, _facts) = certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        let plans = derive_reference_plans(definitions, &lease)?;
        let references = ReferenceCooker::new(lease, plans);
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::RecursiveGreen {
                projection: vec![projection],
            },
            references: Some(references),
            reuse_references: false,
        })
    }

    /// Persistent-SourceFacts counterpart to
    /// [`Self::derive_with_recursive_green`].
    ///
    /// This is the definitive clean escape hatch after local recursive-Green
    /// adoption declines. The caller still supplies the independently built
    /// target Green session when constructing the writer.
    pub fn derive_with_recursive_green_from_persistent(
        certified: PersistentCertifiedSource,
        result: &M11CleanDocumentResult,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = certified.source();
        validate_whole_source_result(source, result)?;
        if result.reuses_leading_references() {
            return Err(M11CandidateDerivationError::ReusedReferencesRequireExactBase);
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        let source_facts_profile = certified.source_facts_profile();
        let projection = encode_recursive_green_projection(source)?;
        let definitions = result_definitions(result);
        let (lease, certified_profile, certified_source_facts_profile) = certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        debug_assert_eq!(certified_source_facts_profile, source_facts_profile);
        let plans = derive_reference_plans(definitions, &lease)?;
        let references = ReferenceCooker::new(lease, plans);
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::RecursiveGreen {
                projection: vec![projection],
            },
            references: Some(references),
            reuse_references: false,
        })
    }

    /// Derives an exact target whose Green role will retain the completed
    /// update's recursive tree while References remain owned by the retained
    /// exact base.
    ///
    /// A completed recursive-Green update is the unforgeable parser proof that
    /// both structural sides and adopted reference authority are still live.
    /// The update remains borrowed so the endpoint can install it only after
    /// delivery commits. The resulting candidate can only be transferred via
    /// [`Self::into_writer_with_recursive_green_reusing_references`].
    pub fn derive_with_recursive_green_reusing_references(
        certified: PersistentCertifiedSource,
        update: &M11PersistentRecursiveGreenUpdate,
    ) -> Result<Self, M11CandidateDerivationError> {
        let exact = update.exact_publication()?;
        let source = certified.source();
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        if exact.target_session().source() != source
            || exact.target_session().syntax_profile() != syntax_profile
            || exact.base_session().source() == source
            || exact.base_session().syntax_profile() != syntax_profile
        {
            return Err(M11CandidateDerivationError::RecursiveGreenPublicationMismatch);
        }
        // Minting the exact-publication borrow also authenticates the parser's
        // structural selection. Delta transport consumes that selection at a
        // separate ownership seam; candidate construction must not infer it.
        let _ = exact.recursive_green_splice_selection();
        let source_facts_profile = certified.source_facts_profile();
        let projection = encode_recursive_green_projection(source)?;
        let (_lease, certified_profile, certified_source_facts_profile) = certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        debug_assert_eq!(certified_source_facts_profile, source_facts_profile);
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::RecursiveGreen {
                projection: vec![projection],
            },
            references: None,
            reuse_references: true,
        })
    }

    /// Joins clean-EOF certification to the resumable inline publication
    /// result for the same exact terminal.
    ///
    /// This explicit integration seam never invokes a synchronous inline
    /// encoder.
    pub fn derive_with_inline_publication(
        certified: CertifiedSource,
        result: &M11CleanDocumentResult,
        inline: M11ParserInlinePublication<'_>,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = certified.source();
        validate_whole_source_result(source, result)?;
        if certified.coverage() != SourceFactsCoverage::CleanEof
            || certified.facts().source() != source
        {
            return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
        }
        if result.reuses_leading_references() {
            return Err(M11CandidateDerivationError::ReusedReferencesRequireExactBase);
        }
        if result.sole_paragraph().is_none() {
            return Err(M11CandidateDerivationError::SegmentedCandidateRequiresOwningPath);
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        let source_facts_profile = certified.facts().profile();
        let (green, projection, persistent_inline_projection) =
            derive_inline_publication_records(source, syntax_profile, result, inline)?;
        let definitions = result_definitions(result);
        let (lease, certified_profile, _facts) = certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        let plans = derive_reference_plans(definitions, &lease)?;
        let references = ReferenceCooker::new(lease, plans);
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::Flat {
                green,
                projection,
                persistent_inline_projection,
            },
            references: Some(references),
            reuse_references: false,
        })
    }

    /// Moves one exact terminal into an incremental block publication plan.
    ///
    /// No leaf record is encoded here. The writer consumes the moved coverage
    /// one leaf at a time under caller fuel, then retains the completed block
    /// root into the candidate manifest.
    pub fn derive_segmented(
        certified: CertifiedSource,
        result: M11CleanDocumentResult,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = certified.source();
        validate_whole_source_result(source, &result)?;
        if certified.coverage() != SourceFactsCoverage::CleanEof
            || certified.facts().source() != source
        {
            return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
        }
        if result.reuses_leading_references() {
            return Err(M11CandidateDerivationError::ReusedReferencesRequireExactBase);
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        let source_facts_profile = certified.facts().profile();
        let source_lease = certified.exact_parse_lease();
        let (reference_lease, certified_profile, _facts) = certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        let plans = derive_reference_plans(result_definitions(&result), &reference_lease)?;
        let references = ReferenceCooker::new(reference_lease, plans);
        let leaves = result.into_publication_leaves();
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::Segmented {
                source_lease,
                leaves,
                prepared_replacement_entries: Vec::new(),
                block_splice: None,
                leaves_are_replacement: false,
            },
            references: Some(references),
            reuse_references: false,
        })
    }

    /// Persistent-SourceFacts counterpart to [`Self::derive_segmented`].
    pub fn derive_segmented_from_persistent(
        certified: PersistentCertifiedSource,
        result: M11CleanDocumentResult,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = certified.source();
        validate_whole_source_result(source, &result)?;
        if result.reuses_leading_references() {
            return Err(M11CandidateDerivationError::ReusedReferencesRequireExactBase);
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        let source_facts_profile = certified.source_facts_profile();
        let source_lease = certified.exact_parse_lease();
        let (reference_lease, certified_profile, certified_source_facts_profile) =
            certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        debug_assert_eq!(certified_source_facts_profile, source_facts_profile);
        let plans = derive_reference_plans(result_definitions(&result), &reference_lease)?;
        let references = ReferenceCooker::new(reference_lease, plans);
        let leaves = result.into_publication_leaves();
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::Segmented {
                source_lease,
                leaves,
                prepared_replacement_entries: Vec::new(),
                block_splice: None,
                leaves_are_replacement: false,
            },
            references: Some(references),
            reuse_references: false,
        })
    }

    /// Exact-clean counterpart to the crop-only persistent block splice path.
    ///
    /// The caller must first prove that the selected base and target block
    /// ranges contain no reference definitions and that the surrounding
    /// source cuts survived authenticated edit lineage. This constructor keeps
    /// the definitive clean target leaves while reusing the exact base's
    /// References and packed block pages.
    pub fn derive_segmented_from_persistent_reusing_references(
        certified: PersistentCertifiedSource,
        result: M11CleanDocumentResult,
        block_splice: M11BlockSequenceSpliceSelection,
    ) -> Result<Self, M11CandidateDerivationError> {
        let source = certified.source();
        validate_whole_source_result(source, &result)?;
        if result.reuses_leading_references() {
            return Err(M11CandidateDerivationError::ReusedReferencesRequireExactBase);
        }
        let target_range = block_splice.target_entry_range();
        if target_range.end
            > u64::try_from(result.leaves().len())
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        {
            return Err(M11BlockSequenceError::InvalidPoint.into());
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        let source_facts_profile = certified.source_facts_profile();
        let source_lease = certified.exact_parse_lease();
        let (_reference_lease, certified_profile, certified_source_facts_profile) =
            certified.into_parts();
        debug_assert_eq!(certified_profile.get(), u64::from(syntax_profile));
        debug_assert_eq!(certified_source_facts_profile, source_facts_profile);
        let leaves = result.into_publication_leaves();
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::Segmented {
                source_lease,
                leaves,
                prepared_replacement_entries: Vec::new(),
                block_splice: Some(block_splice),
                leaves_are_replacement: false,
            },
            references: None,
            reuse_references: true,
        })
    }

    /// Moves one successful exact-crop terminal into the same fuelled
    /// persistent block publication plan as a clean parse while preserving the
    /// base publication as the sole References authority.
    ///
    /// The crop's move-only target lease authenticates every block range. No
    /// reference is recooked or copied here; the writer can only be driven
    /// through [`M11ParserCandidateWriter::poll_reusing_references`], which
    /// requires the retained exact base on every poll.
    pub fn derive_segmented_reusing_references(
        input: M11ExactSegmentedCandidateInput,
        parser_profile: ParserProfileId,
        source_facts_profile: SourceFactsScanProfile,
    ) -> Result<Self, M11CandidateDerivationError> {
        let M11ExactSegmentedCandidateInput {
            source_lease,
            leaves,
            prepared_replacement_entries,
            block_splice,
            leaves_are_replacement,
        } = input;
        let source = source_lease.version();
        let syntax_profile = u32::try_from(parser_profile.get())
            .map_err(|_| M11CandidateDerivationError::ParserProfileOverflow)?;
        Ok(Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles: M11CandidateRolePlan::Segmented {
                source_lease,
                leaves,
                prepared_replacement_entries,
                block_splice,
                leaves_are_replacement,
            },
            references: None,
            reuse_references: true,
        })
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn syntax_profile(&self) -> u32 {
        self.syntax_profile
    }

    #[must_use]
    pub fn role_record_count(&self, role: M11CandidateRoleBytes) -> usize {
        match (&self.roles, role) {
            (M11CandidateRolePlan::Flat { .. }, M11CandidateRoleBytes::Green) => 1,
            (M11CandidateRolePlan::Flat { projection, .. }, M11CandidateRoleBytes::Projection) => {
                projection.len()
            }
            (M11CandidateRolePlan::RecursiveGreen { .. }, M11CandidateRoleBytes::Green) => 0,
            (
                M11CandidateRolePlan::RecursiveGreen { projection },
                M11CandidateRoleBytes::Projection,
            ) => projection.len(),
            (M11CandidateRolePlan::Segmented { .. }, _) => 0,
        }
    }

    #[must_use]
    pub fn role_record(&self, role: M11CandidateRoleBytes, ordinal: usize) -> Option<&[u8]> {
        match (&self.roles, role) {
            (M11CandidateRolePlan::Flat { green, .. }, M11CandidateRoleBytes::Green) => {
                (ordinal == 0).then_some(green)
            }
            (M11CandidateRolePlan::Flat { projection, .. }, M11CandidateRoleBytes::Projection) => {
                projection.get(ordinal).map(std::convert::AsRef::as_ref)
            }
            (M11CandidateRolePlan::RecursiveGreen { .. }, M11CandidateRoleBytes::Green) => None,
            (
                M11CandidateRolePlan::RecursiveGreen { projection },
                M11CandidateRoleBytes::Projection,
            ) => projection.get(ordinal).map(std::convert::AsRef::as_ref),
            (M11CandidateRolePlan::Segmented { .. }, _) => None,
        }
    }

    /// Transfers the derived role records into the fuelled manifest owner.
    ///
    /// # Errors
    ///
    /// Returns an authority, allocation, or publication-envelope failure.
    pub fn into_writer(
        self,
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        parse_generation: u64,
    ) -> Result<M11ParserCandidateWriter, M11CandidateDerivationError> {
        let Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles,
            references,
            reuse_references,
        } = self;
        if reuse_references && !matches!(&roles, M11CandidateRolePlan::Segmented { .. }) {
            return Err(M11CandidateDerivationError::ExactBaseReferencesRequired);
        }
        let mut block_splice_selection = None;
        let (build, segmented) = match roles {
            M11CandidateRolePlan::Flat {
                green,
                projection,
                persistent_inline_projection: None,
            } => {
                let records = M11RoleRecords::persistent_projection_records(green, projection)?;
                let build = M11CandidateBuild::new_with_persistent_source_facts(
                    runtime,
                    document,
                    publication,
                    source,
                    parse_generation,
                    syntax_profile,
                    source_facts_profile,
                    records,
                )?;
                (Some(build), None)
            }
            M11CandidateRolePlan::Flat {
                persistent_inline_projection: Some(_),
                ..
            } => return Err(M11CandidateDerivationError::InlinePublicationMismatch),
            M11CandidateRolePlan::RecursiveGreen { .. } => {
                return Err(M11CandidateDerivationError::RecursiveGreenPublicationMismatch);
            }
            M11CandidateRolePlan::Segmented {
                source_lease,
                mut leaves,
                prepared_replacement_entries,
                block_splice,
                leaves_are_replacement,
            } => {
                if !prepared_replacement_entries.is_empty()
                    && (!leaves.is_empty() || !leaves_are_replacement || block_splice.is_none())
                {
                    return Err(M11BlockSequenceError::InvalidPoint.into());
                }
                let block_splice = block_splice.filter(|selection| {
                    let target = selection.target_entry_range();
                    target.start <= target.end
                        && if !prepared_replacement_entries.is_empty() {
                            u64::try_from(prepared_replacement_entries.len()).ok()
                                == Some(target.end - target.start)
                        } else if leaves_are_replacement {
                            u64::try_from(leaves.len()).ok() == Some(target.end - target.start)
                        } else {
                            usize::try_from(target.end).is_ok_and(|end| end <= leaves.len())
                        }
                        && reuse_references
                });
                let (block_build, target_source_lease) =
                    if let Some(selection) = block_splice.as_ref() {
                        block_splice_selection = Some(selection.clone());
                        let target = selection.target_entry_range();
                        if !leaves_are_replacement {
                            let start = usize::try_from(target.start)
                                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
                            let end = usize::try_from(target.end)
                                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
                            let mut replacement_leaves = Vec::new();
                            replacement_leaves
                                .try_reserve_exact(end - start)
                                .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
                            replacement_leaves.extend(leaves.drain(start..end));
                            leaves = replacement_leaves;
                        }
                        (None, Some(source_lease))
                    } else {
                        (
                            Some(M11BlockSequenceBuild::new(runtime, source_lease)?),
                            None,
                        )
                    };
                (
                    None,
                    Some(M11SegmentedCandidateWriter {
                        leaves: leaves.into_iter(),
                        block_build,
                        target_source_lease,
                        block_splice,
                        replacement_entries: prepared_replacement_entries,
                        input_finished: false,
                        block_root: None,
                        root_release_started: false,
                        manifest: M11SegmentedManifestInputs {
                            document,
                            publication,
                            source,
                            parse_generation,
                            syntax_profile,
                            source_facts_profile,
                        },
                    }),
                )
            }
        };
        Ok(M11ParserCandidateWriter {
            build,
            segmented,
            references,
            reuse_references,
            references_finished: reuse_references,
            block_splice_selection,
            block_splice_receipt: None,
            aborting: false,
        })
    }

    /// Transfers a projection-only candidate beside the current session-owned
    /// recursive Green root.
    ///
    /// The session keeps owning its root. The engine journals a retained,
    /// authority-bound wrapper into the candidate before this call returns.
    #[allow(clippy::too_many_arguments)]
    pub fn into_writer_with_recursive_green(
        self,
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        parse_generation: u64,
        session: &M11PersistentRecursiveGreenSession,
    ) -> Result<M11ParserCandidateWriter, M11CandidateDerivationError> {
        let Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles,
            references,
            reuse_references,
        } = self;
        if reuse_references
            || session.source() != source
            || session.syntax_profile() != syntax_profile
        {
            return Err(M11CandidateDerivationError::RecursiveGreenPublicationMismatch);
        }
        let M11CandidateRolePlan::RecursiveGreen { projection } = roles else {
            return Err(M11CandidateDerivationError::RecursiveGreenPublicationMismatch);
        };
        let records = M11RoleRecords::persistent_recursive_green_projection_records(projection)?;
        let recursive_green = session.current_green_root(runtime)?;
        let build = M11CandidateBuild::new_with_persistent_source_facts_and_recursive_green(
            runtime,
            document,
            publication,
            source,
            parse_generation,
            syntax_profile,
            source_facts_profile,
            records,
            recursive_green,
        )?;
        Ok(M11ParserCandidateWriter {
            build: Some(build),
            segmented: None,
            references,
            reuse_references: false,
            references_finished: false,
            block_splice_selection: None,
            block_splice_receipt: None,
            aborting: false,
        })
    }

    /// Transfers an exact recursive-Green candidate while retaining the
    /// canonical References root of `base`.
    ///
    /// Both the completed parser update and retained base are borrowed. The
    /// base manifest must name the update's exact base source, document, and
    /// syntax profile; the target tree must name this candidate's source.
    /// Polling remains restricted to
    /// [`M11ParserCandidateWriter::poll_reusing_references`].
    #[allow(clippy::too_many_arguments)]
    pub fn into_writer_with_recursive_green_reusing_references(
        self,
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        parse_generation: u64,
        update: &M11PersistentRecursiveGreenUpdate,
        base: &M11RetainedCandidatePublication,
    ) -> Result<M11ParserCandidateWriter, M11CandidateDerivationError> {
        let Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles,
            references,
            reuse_references,
        } = self;
        if !reuse_references || references.is_some() {
            return Err(M11CandidateDerivationError::ExactBaseReferencesRequired);
        }
        let M11CandidateRolePlan::RecursiveGreen { projection } = roles else {
            return Err(M11CandidateDerivationError::RecursiveGreenPublicationMismatch);
        };
        let exact = update.exact_publication()?;
        let target_session = exact.target_session();
        if target_session.source() != source || target_session.syntax_profile() != syntax_profile {
            return Err(M11CandidateDerivationError::RecursiveGreenPublicationMismatch);
        }
        let exact_base_source = exact.base_session().source();
        let exact_base_descriptor = base.descriptor(runtime)?;
        if exact_base_descriptor.document != document
            || exact_base_descriptor.source_root != exact_base_source.root().get()
            || exact_base_descriptor.source_revision != exact_base_source.revision().get()
            || exact_base_descriptor.source_bytes
                != u64::try_from(exact_base_source.byte_len())
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
            || exact_base_descriptor.source_utf16
                != u64::try_from(exact_base_source.utf16_len())
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
            || exact_base_descriptor.syntax_profile != exact.base_session().syntax_profile()
            || exact_base_descriptor.parse_generation >= parse_generation
        {
            return Err(M11CandidateDerivationError::ExactBaseReferencesRequired);
        }
        let records = M11RoleRecords::persistent_recursive_green_projection_records(projection)?;
        let recursive_green = target_session.current_green_root(runtime)?;
        let build = M11CandidateBuild::
            new_with_persistent_source_facts_and_recursive_green_reusing_references(
                runtime,
                document,
                publication,
                source,
                parse_generation,
                syntax_profile,
                source_facts_profile,
                records,
                recursive_green,
                base,
            )?;
        Ok(M11ParserCandidateWriter {
            build: Some(build),
            segmented: None,
            references: None,
            reuse_references: true,
            references_finished: true,
            block_splice_selection: None,
            block_splice_receipt: None,
            aborting: false,
        })
    }

    /// Transfers an authoritative-inline candidate into the schema-v2
    /// manifest owner.
    ///
    /// The engine retains `inline_projection` into the candidate journal. The
    /// caller continues to own the original root and must explicitly release
    /// it with fuel after this constructor succeeds.
    pub fn into_writer_with_inline_projection(
        self,
        runtime: &mut DocumentRuntime,
        document: [u8; 16],
        publication: [u8; 16],
        parse_generation: u64,
        inline_projection: &M11InlineProjectionRoot,
    ) -> Result<M11ParserCandidateWriter, M11CandidateDerivationError> {
        let Self {
            source,
            syntax_profile,
            source_facts_profile,
            roles,
            references,
            reuse_references,
        } = self;
        if reuse_references {
            return Err(M11CandidateDerivationError::ExactBaseReferencesRequired);
        }
        let M11CandidateRolePlan::Flat {
            green,
            projection,
            persistent_inline_projection: Some(expected),
        } = roles
        else {
            return Err(M11CandidateDerivationError::InlinePublicationMismatch);
        };
        if inline_projection.descriptor() != &expected {
            return Err(M11CandidateDerivationError::InlinePublicationMismatch);
        }
        let records = M11RoleRecords::persistent_projection_records(green, projection)?;
        let build = M11CandidateBuild::new_with_persistent_source_facts_and_inline_projection(
            runtime,
            document,
            publication,
            source,
            parse_generation,
            syntax_profile,
            source_facts_profile,
            records,
            inline_projection,
        )?;
        Ok(M11ParserCandidateWriter {
            build: Some(build),
            segmented: None,
            references,
            reuse_references: false,
            references_finished: false,
            block_splice_selection: None,
            block_splice_receipt: None,
            aborting: false,
        })
    }
}

fn validate_whole_source_result(
    source: SourceVersion,
    result: &M11CleanDocumentResult,
) -> Result<(), M11CandidateDerivationError> {
    if result.source_version() != source
        || result.source_range()
            != (0..u32::try_from(source.byte_len())
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    Ok(())
}

type InlinePublicationRecords = (
    Box<[u8]>,
    Vec<Box<[u8]>>,
    Option<M11InlineProjectionDescriptor>,
);

fn derive_inline_publication_records(
    source: SourceVersion,
    syntax_profile: u32,
    result: &M11CleanDocumentResult,
    inline: M11ParserInlinePublication<'_>,
) -> Result<InlinePublicationRecords, M11CandidateDerivationError> {
    let terminal = M11ParserTerminalFacts::derive(result)?;
    let (green, projection) = terminal.into_parts();
    let expected_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    let visible_source = match result.outcome() {
        M11CleanDocumentOutcome::Paragraph { visible_source, .. } => Some(visible_source.clone()),
        M11CleanDocumentOutcome::Empty { .. }
        | M11CleanDocumentOutcome::Segmented { .. }
        | M11CleanDocumentOutcome::Unknown { .. } => None,
    };
    let (inline_record, persistent_inline_projection) = match inline {
        M11ParserInlinePublication::NoInline if visible_source.is_none() => (None, None),
        M11ParserInlinePublication::NoInline => {
            return Err(M11CandidateDerivationError::InlinePublicationMismatch);
        }
        M11ParserInlinePublication::Authoritative(root) => {
            let visible_source = visible_source
                .as_ref()
                .ok_or(M11CandidateDerivationError::InlinePublicationMismatch)?;
            let descriptor = root.descriptor();
            if descriptor.source() != source
                || descriptor.parser_profile() != expected_profile
                || descriptor.source_range() != visible_source
            {
                return Err(M11CandidateDerivationError::InlinePublicationMismatch);
            }
            (None, Some(descriptor.clone()))
        }
        M11ParserInlinePublication::Unsupported(record) => {
            let visible_source = visible_source
                .as_ref()
                .ok_or(M11CandidateDerivationError::InlinePublicationMismatch)?;
            if record.source() != source
                || record.parser_profile() != expected_profile
                || record.source_range() != *visible_source
            {
                return Err(M11CandidateDerivationError::InlinePublicationMismatch);
            }
            (Some(record.into_encoded()), None)
        }
    };
    let mut projection_records = Vec::new();
    projection_records
        .try_reserve_exact(1 + usize::from(inline_record.is_some()))
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    projection_records.push(projection);
    projection_records.extend(inline_record);
    Ok((green, projection_records, persistent_inline_projection))
}

struct M11SegmentedManifestInputs {
    document: [u8; 16],
    publication: [u8; 16],
    source: SourceVersion,
    parse_generation: u64,
    syntax_profile: u32,
    source_facts_profile: SourceFactsScanProfile,
}

struct M11SegmentedCandidateWriter {
    leaves: std::vec::IntoIter<M11CleanLeaf>,
    block_build: Option<M11BlockSequenceBuild>,
    target_source_lease: Option<SourceSnapshotLease>,
    block_splice: Option<M11BlockSequenceSpliceSelection>,
    replacement_entries: Vec<M11BlockSequenceEntry>,
    input_finished: bool,
    block_root: Option<M11BlockSequenceRoot>,
    root_release_started: bool,
    manifest: M11SegmentedManifestInputs,
}

/// Parser-side owner that feeds block coverage and exact references, then
/// drives manifest sealing.
pub struct M11ParserCandidateWriter {
    build: Option<M11CandidateBuild>,
    segmented: Option<M11SegmentedCandidateWriter>,
    references: Option<ReferenceCooker>,
    reuse_references: bool,
    references_finished: bool,
    block_splice_selection: Option<M11BlockSequenceSpliceSelection>,
    block_splice_receipt: Option<M11BlockSequenceSpliceReceipt>,
    aborting: bool,
}

impl M11ParserCandidateWriter {
    /// Advances reference paging and manifest sealing within `fuel`.
    ///
    /// # Errors
    ///
    /// Returns a typed state, fuel, allocation, or validation failure.
    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserCandidateWriterPoll, M11CandidateDerivationError> {
        if self.reuse_references {
            return Err(M11CandidateDerivationError::ExactBaseReferencesRequired);
        }
        self.poll_internal(runtime, fuel, None)
    }

    /// Advances an exact-crop block publication while retaining References
    /// from `base`.
    ///
    /// The base is borrowed only for the single bounded transition that
    /// journals its canonical References root into the target manifest. The
    /// endpoint continues to own the move-only base capability for exact-delta
    /// transport.
    pub fn poll_reusing_references(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        base: &M11RetainedCandidatePublication,
    ) -> Result<M11ParserCandidateWriterPoll, M11CandidateDerivationError> {
        if !self.reuse_references {
            return Err(M11CandidateDerivationError::ExactBaseReferencesRequired);
        }
        self.poll_internal(runtime, fuel, Some(base))
    }

    fn poll_internal(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        exact_base: Option<&M11RetainedCandidatePublication>,
    ) -> Result<M11ParserCandidateWriterPoll, M11CandidateDerivationError> {
        if fuel == 0 {
            return Err(M11CandidateDerivationError::Publication(
                M11PublicationError::zero_fuel(),
            ));
        }
        if self.aborting {
            return Err(M11CandidateDerivationError::Publication(
                M11PublicationError::invalid_state(),
            ));
        }
        let mut transitions = 0;
        while transitions < fuel {
            if self.segmented.is_some() {
                let consumed = self.poll_segmented_one(runtime, exact_base)?;
                if consumed == 0 || consumed > fuel - transitions {
                    return Err(M11CandidateDerivationError::MetricOverflow);
                }
                transitions = transitions
                    .checked_add(consumed)
                    .ok_or(M11CandidateDerivationError::MetricOverflow)?;
                continue;
            }
            let build = self.build.as_mut().ok_or_else(|| {
                M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
            })?;
            if !self.references_finished {
                let references = self
                    .references
                    .as_mut()
                    .ok_or(M11CandidateDerivationError::ExactBaseReferencesRequired)?;
                match references.poll_one(runtime, build)? {
                    ReferenceCookPoll::Progress => {
                        transitions += 1;
                        continue;
                    }
                    ReferenceCookPoll::Complete => {
                        build.finish_references(runtime)?;
                        self.references_finished = true;
                        transitions += 1;
                        continue;
                    }
                }
            }

            let remaining = fuel - transitions;
            match build.poll(runtime, remaining)? {
                M11CandidateBuildPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11CandidateDerivationError::MetricOverflow)?;
                    if consumed == 0 {
                        return Ok(M11ParserCandidateWriterPoll::Pending { transitions });
                    }
                }
                M11CandidateBuildPoll::Published {
                    transitions: consumed,
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11CandidateDerivationError::MetricOverflow)?;
                    let build = self.build.take().ok_or_else(|| {
                        M11CandidateDerivationError::Publication(
                            M11PublicationError::invalid_state(),
                        )
                    })?;
                    let mut publication = build.into_publication()?;
                    if let Some(selection) = self.block_splice_selection.take() {
                        let receipt = self
                            .block_splice_receipt
                            .take()
                            .ok_or(M11BlockSequenceError::InvalidState)?;
                        publication.attach_exact_block_splice(selection, receipt)?;
                    } else if self.block_splice_receipt.is_some() {
                        return Err(M11BlockSequenceError::InvalidState.into());
                    }
                    return Ok(M11ParserCandidateWriterPoll::Published {
                        transitions,
                        publication: Box::new(publication),
                    });
                }
            }
        }
        Ok(M11ParserCandidateWriterPoll::Pending { transitions })
    }

    fn poll_segmented_one(
        &mut self,
        runtime: &mut DocumentRuntime,
        exact_base: Option<&M11RetainedCandidatePublication>,
    ) -> Result<usize, M11CandidateDerivationError> {
        let has_root = self
            .segmented
            .as_ref()
            .is_some_and(|segmented| segmented.block_root.is_some());
        if has_root {
            if self.build.is_none() {
                let segmented = self.segmented.as_ref().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                let root = segmented.block_root.as_ref().ok_or(
                    M11CandidateDerivationError::BlockSequence(M11BlockSequenceError::InvalidState),
                )?;
                let manifest = &segmented.manifest;
                self.build = Some(if self.reuse_references {
                    M11CandidateBuild::
                        new_with_persistent_source_facts_and_blocks_reusing_references(
                            runtime,
                            manifest.document,
                            manifest.publication,
                            manifest.source,
                            manifest.parse_generation,
                            manifest.syntax_profile,
                            manifest.source_facts_profile,
                            root,
                            exact_base.ok_or(
                                M11CandidateDerivationError::ExactBaseReferencesRequired,
                            )?,
                        )?
                } else {
                    M11CandidateBuild::new_with_persistent_source_facts_and_blocks(
                        runtime,
                        manifest.document,
                        manifest.publication,
                        manifest.source,
                        manifest.parse_generation,
                        manifest.syntax_profile,
                        manifest.source_facts_profile,
                        root,
                    )?
                });
                return Ok(1);
            }

            let release_started = self
                .segmented
                .as_ref()
                .is_some_and(|segmented| segmented.root_release_started);
            if !release_started {
                let segmented = self.segmented.as_mut().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                segmented
                    .block_root
                    .as_mut()
                    .ok_or(M11BlockSequenceError::InvalidState)?
                    .begin_release(runtime)?;
                segmented.root_release_started = true;
                return Ok(1);
            }

            let poll = self
                .segmented
                .as_mut()
                .and_then(|segmented| segmented.block_root.as_mut())
                .ok_or(M11BlockSequenceError::InvalidState)?
                .poll_release(runtime, 1)?;
            let complete = poll.complete();
            let consumed = poll.receipt().transitions;
            if complete {
                let segmented = self.segmented.as_mut().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                segmented.block_root.take();
                self.segmented.take();
            }
            return Ok(consumed.max(usize::from(complete)));
        }

        let has_block_splice = self
            .segmented
            .as_ref()
            .is_some_and(|segmented| segmented.block_splice.is_some());
        if has_block_splice {
            let next_leaf = self
                .segmented
                .as_mut()
                .ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?
                .leaves
                .next();
            if let Some(leaf) = next_leaf {
                let entry = encode_block_sequence_entry(leaf)?;
                let segmented = self.segmented.as_mut().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                segmented
                    .replacement_entries
                    .try_reserve(1)
                    .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
                segmented.replacement_entries.push(entry);
                return Ok(1);
            }

            let (target_lease, selection, replacement) = {
                let segmented = self.segmented.as_mut().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                let selection = segmented
                    .block_splice
                    .take()
                    .ok_or(M11BlockSequenceError::InvalidState)?;
                let target_len =
                    selection.target_entry_range().end - selection.target_entry_range().start;
                if u64::try_from(segmented.replacement_entries.len())
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
                    != target_len
                {
                    return Err(M11BlockSequenceError::InvalidPoint.into());
                }
                let target_lease = segmented
                    .target_source_lease
                    .take()
                    .ok_or(M11BlockSequenceError::InvalidState)?;
                (
                    target_lease,
                    selection,
                    std::mem::take(&mut segmented.replacement_entries),
                )
            };
            let (root, receipt) = exact_base
                .ok_or(M11CandidateDerivationError::ExactBaseReferencesRequired)?
                .splice_block_sequence_atomic(runtime, target_lease, &selection, &replacement)?;
            self.block_splice_receipt = Some(receipt);
            self.segmented
                .as_mut()
                .ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?
                .block_root = Some(root);
            return Ok(1);
        }

        let poll = self
            .segmented
            .as_mut()
            .and_then(|segmented| segmented.block_build.as_mut())
            .ok_or(M11BlockSequenceError::InvalidState)?
            .poll(runtime, 1)?;
        if poll.transitions() > 0 {
            return Ok(poll.transitions());
        }
        match poll.status() {
            M11BlockSequenceBuildStatus::NeedsInput => {
                let segmented = self.segmented.as_mut().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                if segmented.input_finished {
                    return Err(M11BlockSequenceError::InvalidState.into());
                }
                if let Some(leaf) = segmented.leaves.next() {
                    let entry = encode_block_sequence_entry(leaf)?;
                    segmented
                        .block_build
                        .as_mut()
                        .ok_or(M11BlockSequenceError::InvalidState)?
                        .offer_entry(entry)?;
                } else {
                    segmented
                        .block_build
                        .as_mut()
                        .ok_or(M11BlockSequenceError::InvalidState)?
                        .finish_input()?;
                    segmented.input_finished = true;
                }
                Ok(1)
            }
            M11BlockSequenceBuildStatus::Complete => {
                let segmented = self.segmented.as_mut().ok_or_else(|| {
                    M11CandidateDerivationError::Publication(M11PublicationError::invalid_state())
                })?;
                let root = segmented
                    .block_build
                    .as_mut()
                    .and_then(M11BlockSequenceBuild::take_root)
                    .ok_or(M11BlockSequenceError::InvalidState)?;
                segmented.block_build.take();
                segmented.block_root = Some(root);
                Ok(1)
            }
            M11BlockSequenceBuildStatus::Pending | M11BlockSequenceBuildStatus::Cancelled => {
                Err(M11BlockSequenceError::InvalidState.into())
            }
        }
    }

    /// Returns the bounded-work and maximum-retention receipt accumulated by
    /// exact reference cooking so far.
    #[must_use]
    pub fn reference_cook_receipt(&self) -> M11ReferenceCookReceipt {
        self.references
            .as_ref()
            .map_or_else(M11ReferenceCookReceipt::default, ReferenceCooker::receipt)
    }

    /// Transfers an incomplete build to the engine's fuelled abort queue.
    ///
    /// # Errors
    ///
    /// Returns a typed state or arena failure.
    pub fn begin_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11CandidateDerivationError> {
        if self.aborting {
            return Err(M11CandidateDerivationError::Publication(
                M11PublicationError::invalid_state(),
            ));
        }
        let mut owned_work = false;
        if let Some(build) = self.build.as_mut() {
            build.begin_abort(runtime)?;
            owned_work = true;
        }
        if let Some(segmented) = self.segmented.as_mut() {
            if let Some(build) = segmented.block_build.as_mut() {
                build.begin_cancel(runtime)?;
                owned_work = true;
            }
            if let Some(root) = segmented.block_root.as_mut() {
                if !segmented.root_release_started {
                    root.begin_release(runtime)?;
                    segmented.root_release_started = true;
                }
                owned_work = true;
            }
            if segmented.block_root.is_none()
                && (segmented.block_splice.is_some()
                    || segmented.target_source_lease.is_some()
                    || !segmented.replacement_entries.is_empty())
            {
                segmented.block_splice.take();
                segmented.target_source_lease.take();
                segmented.replacement_entries.clear();
                segmented.leaves = Vec::new().into_iter();
                owned_work = true;
            }
        }
        if self.block_splice_selection.take().is_some() {
            self.block_splice_receipt.take();
            owned_work = true;
        }
        if !owned_work {
            return Err(M11CandidateDerivationError::Publication(
                M11PublicationError::invalid_state(),
            ));
        }
        if let Some(references) = self.references.as_mut() {
            references.cancel();
        }
        self.aborting = true;
        Ok(())
    }

    /// Reclaims an aborted build within `fuel`.
    ///
    /// # Errors
    ///
    /// Returns a typed state, fuel, or arena failure.
    pub fn poll_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11CandidateDerivationError> {
        if !self.aborting {
            return Err(M11CandidateDerivationError::Publication(
                M11PublicationError::invalid_state(),
            ));
        }
        if fuel == 0 {
            return Err(M11CandidateDerivationError::Publication(
                M11PublicationError::zero_fuel(),
            ));
        }
        let mut remaining = fuel;
        while remaining > 0 {
            let mut segmented_complete = false;
            if let Some(segmented) = self.segmented.as_mut() {
                if let Some(build) = segmented.block_build.as_mut() {
                    let poll = build.poll_cancel(runtime, 1)?;
                    remaining -= 1;
                    if poll.complete() {
                        segmented.block_build.take();
                    }
                    if remaining == 0 {
                        return Ok(false);
                    }
                    continue;
                }
                if let Some(root) = segmented.block_root.as_mut() {
                    let poll = root.poll_release(runtime, 1)?;
                    remaining -= 1;
                    if poll.complete() {
                        segmented.block_root.take();
                    }
                    if remaining == 0 {
                        return Ok(false);
                    }
                    continue;
                }
                segmented_complete = true;
            }
            if segmented_complete {
                self.segmented.take();
            }

            if let Some(build) = self.build.as_mut() {
                let complete = build.poll_abort(runtime, remaining)?;
                if complete {
                    self.build.take();
                }
                return Ok(complete && self.segmented.is_none());
            }
            return Ok(self.segmented.is_none());
        }
        Ok(false)
    }
}

pub enum M11ParserCandidateWriterPoll {
    Pending {
        transitions: usize,
    },
    Published {
        transitions: usize,
        publication: Box<M11CandidatePublication>,
    },
}

/// A caller-fuelled exact-clean parse over one immutable source lease.
pub struct M11CleanParseJob {
    scanner: Option<SnapshotLineScanner>,
    completed_source: Option<SourceSnapshotLease>,
    controller: Option<M11CleanBlockController>,
    pending_line: Option<SnapshotPhysicalLine>,
    active: Option<ActiveLine>,
    work: CleanParseWork,
    finish: Option<CleanParseFinish>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CleanParseWork {
    source_bytes_discovered: usize,
    source_bytes_read: usize,
    physical_lines_discovered: usize,
    parser_transitions: usize,
}

struct ActiveLine {
    facts: M11PhysicalLineFacts,
    source: SnapshotLineSource,
    admission: M11CleanLineAdmission,
    matched: bool,
}

enum CleanParseFinish {
    WholeSource,
    OrdinaryParagraphEofCrop,
    OrdinaryParagraphCrop {
        expected_end_byte: u32,
        expected_end_utf16: u32,
    },
}

enum CleanParseTerminal {
    WholeSource(M11CleanDocumentResult),
    OrdinaryParagraphEofCrop(M11OrdinaryParagraphEofCropTerminal),
    OrdinaryParagraphCrop(M11OrdinaryParagraphCropTerminal),
}

#[derive(Debug)]
pub enum M11CleanParseJobError {
    ZeroFuel,
    Complete,
    Source(SourceAdapterError),
    Controller(M11CleanControllerError<SourceAdapterError>),
    Finish(M11CleanControllerFault),
    WorkAccounting,
}

impl fmt::Display for M11CleanParseJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFuel => formatter.write_str("clean parse job requires nonzero fuel"),
            Self::Complete => formatter.write_str("clean parse job is already complete"),
            Self::Source(error) => error.fmt(formatter),
            Self::Controller(error) => error.fmt(formatter),
            Self::Finish(error) => error.fmt(formatter),
            Self::WorkAccounting => formatter.write_str("clean parse work accounting diverged"),
        }
    }
}

impl std::error::Error for M11CleanParseJobError {}

#[derive(Debug)]
pub enum M11OrdinaryParagraphRestartError {
    AuthorityMismatch,
    BindingMismatch,
    UnsupportedGrammarRevision { actual: u32 },
    CutMismatch,
    Source(SourceAdapterError),
}

impl fmt::Display for M11OrdinaryParagraphRestartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthorityMismatch => {
                formatter.write_str("ordinary Paragraph restart crossed source authority")
            }
            Self::BindingMismatch => {
                formatter.write_str("ordinary Paragraph restart crossed parser binding")
            }
            Self::UnsupportedGrammarRevision { actual } => {
                write!(
                    formatter,
                    "unsupported ordinary Paragraph restart grammar revision {actual}"
                )
            }
            Self::CutMismatch => {
                formatter.write_str("ordinary Paragraph restart cut does not match its witness")
            }
            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11OrdinaryParagraphRestartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl M11CleanParseJob {
    /// Binds a parse job to one immutable source lease.
    ///
    /// # Errors
    ///
    /// Returns a source-adapter failure when the source dimensions or cursor
    /// cannot satisfy the M1.1 contract.
    pub fn new(lease: flark_engine::SourceSnapshotLease) -> Result<Self, M11CleanParseJobError> {
        let source = lease.version();
        Ok(Self {
            scanner: Some(SnapshotLineScanner::new(lease).map_err(M11CleanParseJobError::Source)?),
            completed_source: None,
            controller: Some(M11CleanBlockController::new_for_source(source)),
            pending_line: None,
            active: None,
            work: CleanParseWork::default(),
            finish: Some(CleanParseFinish::WholeSource),
        })
    }

    /// Resumes only the block-level state of one exact definition-free
    /// Paragraph after an authenticated unchanged prefix.
    ///
    /// The checkpoint contains no inline state. A successful terminal result
    /// therefore does not authorize incremental inline reuse; callers must
    /// derive inline facts over the complete enclosing Paragraph.
    pub fn new_for_ordinary_paragraph_remainder(
        checkpoint: M11OrdinaryParagraphRestartCheckpoint,
        witness: ExactUnchangedPrefixWitness,
        target: SourceSnapshotLease,
        binding: M11ParserBinding,
    ) -> Result<Self, M11OrdinaryParagraphRestartError> {
        if binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(
                M11OrdinaryParagraphRestartError::UnsupportedGrammarRevision {
                    actual: binding.grammar_revision(),
                },
            );
        }
        if checkpoint.binding() != binding {
            return Err(M11OrdinaryParagraphRestartError::BindingMismatch);
        }
        if checkpoint.source() != witness.base() || target.version() != witness.target() {
            return Err(M11OrdinaryParagraphRestartError::AuthorityMismatch);
        }
        let crop_start = usize::try_from(checkpoint.prefix_end_byte())
            .map_err(|_| M11OrdinaryParagraphRestartError::CutMismatch)?;
        let prefix_utf16 = usize::try_from(checkpoint.prefix_end_utf16())
            .map_err(|_| M11OrdinaryParagraphRestartError::CutMismatch)?;
        if witness.byte_end() != crop_start
            || witness.utf16_end() != prefix_utf16
            || crop_start > witness.target().byte_len()
            || prefix_utf16 > witness.target().utf16_len()
            || usize::try_from(checkpoint.paragraph_content_start())
                .ok()
                .is_none_or(|start| start >= crop_start)
            || checkpoint.next_physical_line_ordinal() == 0
        {
            return Err(M11OrdinaryParagraphRestartError::CutMismatch);
        }
        let observed_utf16 = target
            .utf16_offset_for_byte(crop_start)
            .map_err(SourceAdapterError::from)
            .map_err(M11OrdinaryParagraphRestartError::Source)?;
        if observed_utf16 != prefix_utf16 {
            return Err(M11OrdinaryParagraphRestartError::CutMismatch);
        }

        let source = target.version();
        let (scanner, completed_source) = if crop_start == source.byte_len() {
            (None, Some(target))
        } else {
            (
                Some(
                    SnapshotLineScanner::new_at(
                        target,
                        crop_start,
                        checkpoint.next_physical_line_ordinal(),
                    )
                    .map_err(M11OrdinaryParagraphRestartError::Source)?,
                ),
                None,
            )
        };
        let checkpoint = checkpoint.for_target(source);
        Ok(Self {
            scanner,
            completed_source,
            controller: Some(
                M11CleanBlockController::new_for_ordinary_paragraph_remainder(source, checkpoint),
            ),
            pending_line: None,
            active: None,
            work: CleanParseWork::default(),
            finish: Some(CleanParseFinish::WholeSource),
        })
    }

    /// Advances exact line discovery and grammar work within `fuel`.
    ///
    /// One transition spends at most one engine source window of aggregate
    /// accounted work across line discovery, admission/commit boundaries, and
    /// lexical/classifier work. The fixed 4 KiB quantum keeps endpoint fuel
    /// independent of physical-line shape without hiding zero-byte state
    /// transitions.
    ///
    /// # Errors
    ///
    /// Returns a typed source, controller, lifecycle, or zero-fuel failure.
    pub fn poll(&mut self, fuel: usize) -> Result<M11CleanParsePoll, M11CleanParseJobError> {
        match self.poll_internal(fuel)? {
            CleanParseInternalPoll::Pending { transitions } => {
                Ok(M11CleanParsePoll::Pending { transitions })
            }
            CleanParseInternalPoll::Complete {
                transitions,
                terminal: CleanParseTerminal::WholeSource(result),
            } => Ok(M11CleanParsePoll::Complete {
                transitions,
                result,
            }),
            CleanParseInternalPoll::Complete {
                terminal:
                    CleanParseTerminal::OrdinaryParagraphCrop(_)
                    | CleanParseTerminal::OrdinaryParagraphEofCrop(_),
                ..
            } => unreachable!("ordinary Paragraph crops use their typed crop poll"),
        }
    }

    fn poll_internal(
        &mut self,
        fuel: usize,
    ) -> Result<CleanParseInternalPoll, M11CleanParseJobError> {
        if fuel == 0 {
            return Err(M11CleanParseJobError::ZeroFuel);
        }
        if self.controller.is_none() {
            return Err(M11CleanParseJobError::Complete);
        }
        let mut transitions = 0;
        while transitions < fuel {
            let quantum = self.poll_quantum()?;
            self.work.parser_transitions = self
                .work
                .parser_transitions
                .checked_add(1)
                .ok_or(M11CleanParseJobError::WorkAccounting)?;
            match quantum {
                CleanParseQuantumPoll::Pending => transitions += 1,
                CleanParseQuantumPoll::Complete(terminal) => {
                    transitions += 1;
                    return Ok(CleanParseInternalPoll::Complete {
                        transitions,
                        terminal,
                    });
                }
            }
        }
        Ok(CleanParseInternalPoll::Pending { transitions })
    }

    #[allow(clippy::too_many_lines)] // One linear state machine keeps the shared work ledger auditable.
    fn poll_quantum(&mut self) -> Result<CleanParseQuantumPoll, M11CleanParseJobError> {
        const STATE_WORK: usize = 1;
        let mut remaining = SOURCE_CURSOR_WINDOW_BYTES;
        loop {
            if let Some(mut active) = self.active.take() {
                if active.matched {
                    remaining = remaining
                        .checked_sub(STATE_WORK)
                        .ok_or(M11CleanParseJobError::WorkAccounting)?;
                    let controller = self
                        .controller
                        .as_mut()
                        .ok_or(M11CleanParseJobError::Complete)?;
                    <M11CleanBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
                        controller,
                        active.admission,
                        active.facts,
                    )
                    .map_err(M11CleanParseJobError::Controller)?;
                    self.scanner = Some(
                        active
                            .source
                            .finish()
                            .map_err(M11CleanParseJobError::Source)?,
                    );
                    if remaining == 0 {
                        return Ok(CleanParseQuantumPoll::Pending);
                    }
                    continue;
                }

                if active.source.access_budget() == 0
                    && active.source.position() < active.source.len()
                {
                    active
                        .source
                        .replenish_access_budget(remaining)
                        .map_err(M11CleanParseJobError::Source)?;
                }
                let receipt = self
                    .controller
                    .as_mut()
                    .ok_or(M11CleanParseJobError::Complete)?
                    .poll_source_line(&mut active.admission, &mut active.source, remaining)
                    .map_err(M11CleanParseJobError::Controller)?;
                if receipt.lexical_work_units == 0 || receipt.lexical_work_units > remaining {
                    self.active = Some(active);
                    return Err(M11CleanParseJobError::WorkAccounting);
                }
                self.work.source_bytes_read = self
                    .work
                    .source_bytes_read
                    .checked_add(receipt.source_first_reads)
                    .ok_or(M11CleanParseJobError::WorkAccounting)?;
                remaining -= receipt.lexical_work_units;
                active.matched = receipt.status == M11SourceLinePollStatus::Matched;
                self.active = Some(active);
                if remaining == 0 {
                    return Ok(CleanParseQuantumPoll::Pending);
                }
                continue;
            }

            if let Some(line) = self.pending_line.take() {
                remaining = remaining
                    .checked_sub(STATE_WORK)
                    .ok_or(M11CleanParseJobError::WorkAccounting)?;
                let facts = line.facts();
                let source = line.into_source().map_err(M11CleanParseJobError::Source)?;
                let controller = self
                    .controller
                    .as_mut()
                    .ok_or(M11CleanParseJobError::Complete)?;
                let admission = <M11CleanBlockController as M11ExactController<
                    SnapshotLineSource,
                >>::begin_source_line(controller, facts.identity())
                .map_err(M11CleanParseJobError::Controller)?;
                self.active = Some(ActiveLine {
                    facts,
                    source,
                    admission,
                    matched: false,
                });
                if remaining == 0 {
                    return Ok(CleanParseQuantumPoll::Pending);
                }
                continue;
            }

            if remaining <= STATE_WORK {
                return Ok(CleanParseQuantumPoll::Pending);
            }
            let Some(scanner) = self.scanner.take() else {
                return Ok(CleanParseQuantumPoll::Complete(self.finish_terminal()?));
            };
            let discovery_grant = remaining - STATE_WORK;
            let (poll, inspected) = scanner
                .poll_counted_retaining_complete(discovery_grant)
                .map_err(M11CleanParseJobError::Source)?;
            if inspected > discovery_grant {
                return Err(M11CleanParseJobError::WorkAccounting);
            }
            self.work.source_bytes_discovered = self
                .work
                .source_bytes_discovered
                .checked_add(inspected)
                .ok_or(M11CleanParseJobError::WorkAccounting)?;
            remaining -= inspected;
            match poll {
                SnapshotLineRetainedPoll::Pending(scanner) => {
                    if inspected == 0 {
                        self.scanner = Some(scanner);
                        return Err(M11CleanParseJobError::WorkAccounting);
                    }
                    self.scanner = Some(scanner);
                    return Ok(CleanParseQuantumPoll::Pending);
                }
                SnapshotLineRetainedPoll::Line(line) => {
                    self.work.physical_lines_discovered = self
                        .work
                        .physical_lines_discovered
                        .checked_add(1)
                        .ok_or(M11CleanParseJobError::WorkAccounting)?;
                    remaining = remaining
                        .checked_sub(STATE_WORK)
                        .ok_or(M11CleanParseJobError::WorkAccounting)?;
                    self.pending_line = Some(line);
                    if remaining == 0 {
                        return Ok(CleanParseQuantumPoll::Pending);
                    }
                }
                SnapshotLineRetainedPoll::Complete(scanner) => {
                    let _finish_work = remaining
                        .checked_sub(STATE_WORK)
                        .ok_or(M11CleanParseJobError::WorkAccounting)?;
                    self.completed_source = Some(scanner.into_source_lease());
                    return Ok(CleanParseQuantumPoll::Complete(self.finish_terminal()?));
                }
            }
        }
    }

    fn take_completed_source(&mut self) -> Option<SourceSnapshotLease> {
        self.completed_source.take()
    }

    fn finish_terminal(&mut self) -> Result<CleanParseTerminal, M11CleanParseJobError> {
        let controller = self
            .controller
            .take()
            .ok_or(M11CleanParseJobError::Complete)?;
        let finish = self.finish.take().ok_or(M11CleanParseJobError::Complete)?;
        match finish {
            CleanParseFinish::WholeSource => Ok(CleanParseTerminal::WholeSource(
                controller.finish().map_err(M11CleanParseJobError::Finish)?,
            )),
            CleanParseFinish::OrdinaryParagraphEofCrop => {
                Ok(CleanParseTerminal::OrdinaryParagraphEofCrop(
                    controller
                        .finish_ordinary_paragraph_eof_crop()
                        .map_err(M11CleanParseJobError::Finish)?,
                ))
            }
            CleanParseFinish::OrdinaryParagraphCrop {
                expected_end_byte,
                expected_end_utf16,
            } => Ok(CleanParseTerminal::OrdinaryParagraphCrop(
                controller
                    .finish_ordinary_paragraph_crop(expected_end_byte, expected_end_utf16)
                    .map_err(M11CleanParseJobError::Finish)?,
            )),
        }
    }
}

enum CleanParseQuantumPoll {
    Pending,
    Complete(CleanParseTerminal),
}

enum CleanParseInternalPoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        terminal: CleanParseTerminal,
    },
}

pub enum M11CleanParsePoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: M11CleanDocumentResult,
    },
}

/// Hard per-transition bound for target checkpoint materialization.
///
/// Candidate poll fuel therefore bounds terminal remapping as strictly as it
/// bounds source parsing; a large base collection cannot disappear inside one
/// nominal parser transition.
pub const M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION: usize = 32;

#[derive(Clone, Copy)]
enum OrderedCheckpointReuse {
    Prefix,
    Suffix,
    Fresh,
}

#[derive(Clone, Copy)]
struct OrderedCheckpointTransform {
    byte_delta: i64,
    utf16_delta: i64,
    ordinal_delta: i64,
    block_ordinal_delta: i64,
    shift_paragraph_geometry: bool,
    paragraph_content_start: Option<u32>,
}

enum OrderedCheckpointSegment {
    Base {
        next: usize,
        end: usize,
        transform: OrderedCheckpointTransform,
        reuse: OrderedCheckpointReuse,
    },
    Owned {
        checkpoint: Option<M11OrdinaryParagraphRestartCheckpoint>,
        reuse: OrderedCheckpointReuse,
    },
    Fresh(M11OrdinaryParagraphCheckpointSeedCursor),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OrderedCheckpointMergeWork {
    reused_prefix_checkpoints: usize,
    fresh_crop_checkpoints: usize,
    reused_suffix_checkpoints: usize,
    transitions: usize,
    maximum_records_per_transition: usize,
}

struct OrderedCheckpointMergeResult {
    base: M11OrdinaryParagraphRestartCheckpoints,
    target: M11OrdinaryParagraphRestartCheckpoints,
    work: OrderedCheckpointMergeWork,
}

enum OrderedCheckpointMergePoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: OrderedCheckpointMergeResult,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrderedCheckpointMergeError {
    Complete,
    InvalidBoundary,
    AllocationFailed,
}

fn shift_top_level_block_count(base: u64, delta: i64) -> Option<u64> {
    if delta >= 0 {
        base.checked_add(u64::try_from(delta).ok()?)
    } else {
        base.checked_sub(delta.unsigned_abs())
    }
}

/// Fuelled ordered splice of parser restart authority.
///
/// The base collection stays intact and move-only until the surrounding target
/// transaction commits or cancels. Target checkpoints are derived in already
/// sorted segments, so terminal publication needs no document-wide sort.
struct OrderedCheckpointMerge {
    source: SourceVersion,
    binding: M11ParserBinding,
    base: Option<M11OrdinaryParagraphRestartCheckpoints>,
    target_top_level_block_count: u64,
    segments: VecDeque<OrderedCheckpointSegment>,
    target: Vec<M11OrdinaryParagraphRestartCheckpoint>,
    work: OrderedCheckpointMergeWork,
    complete: bool,
}

impl OrderedCheckpointMerge {
    // These explicit seam indices and coordinate deltas are the complete
    // private invariant boundary; a parameter object would have one caller.
    #[allow(clippy::too_many_arguments)]
    fn interior(
        source: SourceVersion,
        binding: M11ParserBinding,
        base: M11OrdinaryParagraphRestartCheckpoints,
        target_restart: M11OrdinaryParagraphRestartCheckpoint,
        fresh: M11OrdinaryParagraphCheckpointSeedCursor,
        restart_index: usize,
        convergence_index: usize,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
        block_ordinal_delta: i64,
        shift_paragraph_geometry: bool,
        target_top_level_block_count: u64,
    ) -> Result<Self, OrderedCheckpointMergeError> {
        if restart_index >= base.len()
            || convergence_index <= restart_index
            || convergence_index >= base.len()
        {
            return Err(OrderedCheckpointMergeError::InvalidBoundary);
        }
        let fresh_count = fresh.len();
        let expected = restart_index
            .checked_add(1)
            .and_then(|count| count.checked_add(fresh_count))
            .and_then(|count| count.checked_add(base.len() - convergence_index))
            .ok_or(OrderedCheckpointMergeError::AllocationFailed)?;
        let mut segments = VecDeque::with_capacity(4);
        segments.push_back(OrderedCheckpointSegment::Base {
            next: 0,
            end: restart_index,
            transform: OrderedCheckpointTransform {
                byte_delta: 0,
                utf16_delta: 0,
                ordinal_delta: 0,
                block_ordinal_delta: 0,
                shift_paragraph_geometry: false,
                paragraph_content_start: None,
            },
            reuse: OrderedCheckpointReuse::Prefix,
        });
        segments.push_back(OrderedCheckpointSegment::Owned {
            checkpoint: Some(target_restart),
            reuse: OrderedCheckpointReuse::Prefix,
        });
        segments.push_back(OrderedCheckpointSegment::Fresh(fresh));
        segments.push_back(OrderedCheckpointSegment::Base {
            next: convergence_index,
            end: base.len(),
            transform: OrderedCheckpointTransform {
                byte_delta,
                utf16_delta,
                ordinal_delta,
                block_ordinal_delta,
                shift_paragraph_geometry,
                paragraph_content_start: None,
            },
            reuse: OrderedCheckpointReuse::Suffix,
        });
        Self::new(
            source,
            binding,
            base,
            segments,
            expected,
            target_top_level_block_count,
        )
    }

    // BOF uses the same explicit merge boundary plus its paragraph rebasing
    // coordinate; wrapping them would only move this private call signature.
    #[allow(clippy::too_many_arguments)]
    fn from_bof(
        source: SourceVersion,
        binding: M11ParserBinding,
        base: M11OrdinaryParagraphRestartCheckpoints,
        fresh: M11OrdinaryParagraphCheckpointSeedCursor,
        convergence_index: usize,
        paragraph_content_start: u32,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
    ) -> Result<Self, OrderedCheckpointMergeError> {
        if convergence_index >= base.len() {
            return Err(OrderedCheckpointMergeError::InvalidBoundary);
        }
        let expected = fresh
            .len()
            .checked_add(base.len() - convergence_index)
            .ok_or(OrderedCheckpointMergeError::AllocationFailed)?;
        let mut segments = VecDeque::with_capacity(2);
        segments.push_back(OrderedCheckpointSegment::Fresh(fresh));
        segments.push_back(OrderedCheckpointSegment::Base {
            next: convergence_index,
            end: base.len(),
            transform: OrderedCheckpointTransform {
                byte_delta,
                utf16_delta,
                ordinal_delta,
                block_ordinal_delta: 0,
                shift_paragraph_geometry: false,
                paragraph_content_start: Some(paragraph_content_start),
            },
            reuse: OrderedCheckpointReuse::Suffix,
        });
        Self::new(source, binding, base, segments, expected, 1)
    }

    #[allow(clippy::too_many_arguments)]
    fn from_segmented_bof(
        source: SourceVersion,
        binding: M11ParserBinding,
        base: M11OrdinaryParagraphRestartCheckpoints,
        fresh: M11OrdinaryParagraphCheckpointSeedCursor,
        convergence_index: usize,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
        block_ordinal_delta: i64,
        target_top_level_block_count: u64,
    ) -> Result<Self, OrderedCheckpointMergeError> {
        if convergence_index >= base.len() {
            return Err(OrderedCheckpointMergeError::InvalidBoundary);
        }
        let expected = fresh
            .len()
            .checked_add(base.len() - convergence_index)
            .ok_or(OrderedCheckpointMergeError::AllocationFailed)?;
        let mut segments = VecDeque::with_capacity(2);
        segments.push_back(OrderedCheckpointSegment::Fresh(fresh));
        segments.push_back(OrderedCheckpointSegment::Base {
            next: convergence_index,
            end: base.len(),
            transform: OrderedCheckpointTransform {
                byte_delta,
                utf16_delta,
                ordinal_delta,
                block_ordinal_delta,
                shift_paragraph_geometry: true,
                paragraph_content_start: None,
            },
            reuse: OrderedCheckpointReuse::Suffix,
        });
        Self::new(
            source,
            binding,
            base,
            segments,
            expected,
            target_top_level_block_count,
        )
    }

    fn to_eof(
        source: SourceVersion,
        binding: M11ParserBinding,
        base: M11OrdinaryParagraphRestartCheckpoints,
        target_restart: M11OrdinaryParagraphRestartCheckpoint,
        fresh: M11OrdinaryParagraphCheckpointSeedCursor,
        restart_index: usize,
        target_top_level_block_count: u64,
    ) -> Result<Self, OrderedCheckpointMergeError> {
        if restart_index >= base.len() {
            return Err(OrderedCheckpointMergeError::InvalidBoundary);
        }
        let expected = restart_index
            .checked_add(1)
            .and_then(|count| count.checked_add(fresh.len()))
            .ok_or(OrderedCheckpointMergeError::AllocationFailed)?;
        let mut segments = VecDeque::with_capacity(3);
        segments.push_back(OrderedCheckpointSegment::Base {
            next: 0,
            end: restart_index,
            transform: OrderedCheckpointTransform {
                byte_delta: 0,
                utf16_delta: 0,
                ordinal_delta: 0,
                block_ordinal_delta: 0,
                shift_paragraph_geometry: false,
                paragraph_content_start: None,
            },
            reuse: OrderedCheckpointReuse::Prefix,
        });
        segments.push_back(OrderedCheckpointSegment::Owned {
            checkpoint: Some(target_restart),
            reuse: OrderedCheckpointReuse::Prefix,
        });
        segments.push_back(OrderedCheckpointSegment::Fresh(fresh));
        Self::new(
            source,
            binding,
            base,
            segments,
            expected,
            target_top_level_block_count,
        )
    }

    fn new(
        source: SourceVersion,
        binding: M11ParserBinding,
        base: M11OrdinaryParagraphRestartCheckpoints,
        segments: VecDeque<OrderedCheckpointSegment>,
        expected: usize,
        target_top_level_block_count: u64,
    ) -> Result<Self, OrderedCheckpointMergeError> {
        if target_top_level_block_count == 0
            || base.top_level_block_count() == 0
            || base
                .checkpoints()
                .iter()
                .any(|checkpoint| checkpoint.block_entry_ordinal() >= base.top_level_block_count())
        {
            return Err(OrderedCheckpointMergeError::InvalidBoundary);
        }
        let mut target = Vec::new();
        target
            .try_reserve_exact(expected)
            .map_err(|_| OrderedCheckpointMergeError::AllocationFailed)?;
        Ok(Self {
            source,
            binding,
            base: Some(base),
            target_top_level_block_count,
            segments,
            target,
            work: OrderedCheckpointMergeWork::default(),
            complete: false,
        })
    }

    fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<OrderedCheckpointMergePoll, OrderedCheckpointMergeError> {
        if self.complete {
            return Err(OrderedCheckpointMergeError::Complete);
        }
        if fuel == 0 {
            return Ok(OrderedCheckpointMergePoll::Pending { transitions: 0 });
        }
        let mut transitions = 0;
        while transitions < fuel {
            let mut records = 0;
            while records < M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION {
                let Some((checkpoint, reuse)) = self.next_checkpoint()? else {
                    break;
                };
                self.append(checkpoint)?;
                match reuse {
                    OrderedCheckpointReuse::Prefix => {
                        self.work.reused_prefix_checkpoints += 1;
                    }
                    OrderedCheckpointReuse::Suffix => {
                        self.work.reused_suffix_checkpoints += 1;
                    }
                    OrderedCheckpointReuse::Fresh => {
                        self.work.fresh_crop_checkpoints += 1;
                    }
                }
                records += 1;
            }
            transitions += 1;
            self.work.transitions += 1;
            self.work.maximum_records_per_transition =
                self.work.maximum_records_per_transition.max(records);
            self.discard_empty_segments();
            if self.segments.is_empty() {
                self.complete = true;
                let base = self
                    .base
                    .take()
                    .ok_or(OrderedCheckpointMergeError::Complete)?;
                let target = M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(
                    self.source,
                    self.binding,
                    std::mem::take(&mut self.target),
                    self.target_top_level_block_count,
                );
                return Ok(OrderedCheckpointMergePoll::Complete {
                    transitions,
                    result: OrderedCheckpointMergeResult {
                        base,
                        target,
                        work: self.work,
                    },
                });
            }
        }
        Ok(OrderedCheckpointMergePoll::Pending { transitions })
    }

    fn cancel_into_base(
        mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, OrderedCheckpointMergeError> {
        self.base
            .take()
            .ok_or(OrderedCheckpointMergeError::Complete)
    }

    fn next_checkpoint(
        &mut self,
    ) -> Result<
        Option<(
            M11OrdinaryParagraphRestartCheckpoint,
            OrderedCheckpointReuse,
        )>,
        OrderedCheckpointMergeError,
    > {
        loop {
            let Some(segment) = self.segments.front_mut() else {
                return Ok(None);
            };
            match segment {
                OrderedCheckpointSegment::Base {
                    next,
                    end,
                    transform,
                    reuse,
                } => {
                    if *next >= *end {
                        self.segments.pop_front();
                        continue;
                    }
                    let checkpoint = self
                        .base
                        .as_ref()
                        .and_then(|base| base.checkpoints().get(*next))
                        .ok_or(OrderedCheckpointMergeError::InvalidBoundary)?;
                    *next += 1;
                    let shifted = match transform.paragraph_content_start {
                        Some(paragraph_content_start) => checkpoint
                            .shifted_copy_for_target_with_paragraph_start(
                                self.source,
                                paragraph_content_start,
                                transform.byte_delta,
                                transform.utf16_delta,
                                transform.ordinal_delta,
                                transform.block_ordinal_delta,
                            ),
                        None if transform.shift_paragraph_geometry => checkpoint
                            .shifted_copy_for_target_with_block_delta(
                                self.source,
                                transform.byte_delta,
                                transform.utf16_delta,
                                transform.ordinal_delta,
                                transform.block_ordinal_delta,
                            ),
                        None => checkpoint.shifted_copy_for_target(
                            self.source,
                            transform.byte_delta,
                            transform.utf16_delta,
                            transform.ordinal_delta,
                        ),
                    };
                    let shifted = shifted.ok_or(OrderedCheckpointMergeError::InvalidBoundary)?;
                    return Ok(Some((shifted, *reuse)));
                }
                OrderedCheckpointSegment::Owned { checkpoint, reuse } => {
                    if let Some(checkpoint) = checkpoint.take() {
                        return Ok(Some((checkpoint, *reuse)));
                    }
                    self.segments.pop_front();
                }
                OrderedCheckpointSegment::Fresh(cursor) => {
                    if let Some(checkpoint) = cursor.next() {
                        return Ok(Some((checkpoint, OrderedCheckpointReuse::Fresh)));
                    }
                    self.segments.pop_front();
                }
            }
        }
    }

    fn append(
        &mut self,
        checkpoint: M11OrdinaryParagraphRestartCheckpoint,
    ) -> Result<(), OrderedCheckpointMergeError> {
        if checkpoint.source() != self.source
            || checkpoint.binding() != self.binding
            || !checkpoint.metrics_are_consistent()
            || checkpoint.prefix_end_byte() as usize > self.source.byte_len()
            || checkpoint.prefix_end_utf16() as usize > self.source.utf16_len()
            || checkpoint.block_entry_ordinal() >= self.target_top_level_block_count
        {
            return Err(OrderedCheckpointMergeError::InvalidBoundary);
        }
        if let Some(previous) = self.target.last() {
            if previous.frozen_reference_definition_count()
                != checkpoint.frozen_reference_definition_count()
            {
                return Err(OrderedCheckpointMergeError::InvalidBoundary);
            }
            if previous.prefix_end_byte() == checkpoint.prefix_end_byte() {
                if previous.same_boundary_state(&checkpoint) {
                    return Ok(());
                }
                return Err(OrderedCheckpointMergeError::InvalidBoundary);
            }
            if previous.prefix_end_byte() >= checkpoint.prefix_end_byte()
                || previous.prefix_end_utf16() >= checkpoint.prefix_end_utf16()
                || previous.next_physical_line_ordinal() >= checkpoint.next_physical_line_ordinal()
                || previous.block_entry_ordinal() > checkpoint.block_entry_ordinal()
            {
                return Err(OrderedCheckpointMergeError::InvalidBoundary);
            }
        }
        self.target.push(checkpoint);
        Ok(())
    }

    fn discard_empty_segments(&mut self) {
        loop {
            let empty = match self.segments.front() {
                Some(OrderedCheckpointSegment::Base { next, end, .. }) => next >= end,
                Some(OrderedCheckpointSegment::Owned { checkpoint, .. }) => checkpoint.is_none(),
                Some(OrderedCheckpointSegment::Fresh(cursor)) => cursor.len() == 0,
                None => false,
            };
            if !empty {
                break;
            }
            self.segments.pop_front();
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11OrdinaryParagraphCropWork {
    target_crop_bytes: std::ops::Range<usize>,
    crop_source_bytes_discovered: usize,
    crop_source_bytes_read: usize,
    crop_physical_lines_discovered: usize,
    crop_parser_transitions: usize,
    reused_prefix_checkpoints: usize,
    fresh_crop_checkpoints: usize,
    reused_suffix_checkpoints: usize,
    convergence_ordinal_delta: i64,
    checkpoint_merge_transitions: usize,
    maximum_checkpoint_records_per_transition: usize,
}

impl M11OrdinaryParagraphCropWork {
    #[must_use]
    pub fn target_crop_bytes(&self) -> std::ops::Range<usize> {
        self.target_crop_bytes.clone()
    }

    #[must_use]
    pub const fn crop_source_bytes_discovered(&self) -> usize {
        self.crop_source_bytes_discovered
    }

    #[must_use]
    pub const fn crop_source_bytes_read(&self) -> usize {
        self.crop_source_bytes_read
    }

    #[must_use]
    pub const fn crop_physical_lines_discovered(&self) -> usize {
        self.crop_physical_lines_discovered
    }

    #[must_use]
    pub const fn crop_parser_transitions(&self) -> usize {
        self.crop_parser_transitions
    }

    #[must_use]
    pub const fn reused_prefix_checkpoints(&self) -> usize {
        self.reused_prefix_checkpoints
    }

    #[must_use]
    pub const fn fresh_crop_checkpoints(&self) -> usize {
        self.fresh_crop_checkpoints
    }

    #[must_use]
    pub const fn reused_suffix_checkpoints(&self) -> usize {
        self.reused_suffix_checkpoints
    }

    #[must_use]
    pub const fn convergence_ordinal_delta(&self) -> i64 {
        self.convergence_ordinal_delta
    }

    #[must_use]
    pub const fn checkpoint_merge_transitions(&self) -> usize {
        self.checkpoint_merge_transitions
    }

    #[must_use]
    pub const fn maximum_checkpoint_records_per_transition(&self) -> usize {
        self.maximum_checkpoint_records_per_transition
    }
}

pub struct M11OrdinaryParagraphCropResult {
    output: M11OrdinaryParagraphCropOutput,
    work: M11OrdinaryParagraphCropWork,
    base_checkpoints: Option<M11OrdinaryParagraphRestartCheckpoints>,
    next_checkpoints: Option<M11OrdinaryParagraphRestartCheckpoints>,
    target_source: Option<SourceSnapshotLease>,
}

enum M11OrdinaryParagraphCropOutput {
    Whole(M11CleanDocumentResult),
    Segmented {
        replacement_leaves: Vec<M11CleanLeaf>,
        block_splice: M11BlockSequenceSpliceSelection,
    },
}

impl M11OrdinaryParagraphCropResult {
    #[must_use]
    pub fn terminal(&self) -> &M11CleanDocumentResult {
        match &self.output {
            M11OrdinaryParagraphCropOutput::Whole(terminal) => terminal,
            M11OrdinaryParagraphCropOutput::Segmented { .. } => {
                panic!("a segmented top-level crop has replacement leaves, not a whole terminal")
            }
        }
    }

    #[must_use]
    pub const fn work(&self) -> &M11OrdinaryParagraphCropWork {
        &self.work
    }

    /// Takes the move-only base collection retained until the target commits.
    pub fn take_base_restart_checkpoints(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        self.base_checkpoints
            .take()
            .ok_or(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
    }

    /// Takes the merged, target-bound restart collection exactly once.
    pub fn take_next_restart_checkpoints(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        self.next_checkpoints
            .take()
            .ok_or(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
    }

    /// Takes the exact target lease consumed by this crop exactly once.
    #[must_use]
    pub fn take_target_source_lease(&mut self) -> Option<SourceSnapshotLease> {
        self.target_source.take()
    }

    /// Consumes this authenticated crop into the only parser input that may
    /// retain exact-base References while rebuilding segmented target blocks.
    pub fn into_exact_segmented_candidate_input(
        mut self,
    ) -> Result<M11ExactSegmentedCandidateInput, M11CandidateDerivationError> {
        let source_lease = self
            .target_source
            .take()
            .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
        match self.output {
            M11OrdinaryParagraphCropOutput::Whole(terminal) => {
                M11ExactSegmentedCandidateInput::from_ordinary_crop(source_lease, terminal)
            }
            M11OrdinaryParagraphCropOutput::Segmented {
                replacement_leaves,
                block_splice,
            } => M11ExactSegmentedCandidateInput::from_segmented_crop(
                source_lease,
                replacement_leaves,
                block_splice,
            ),
        }
    }

    #[must_use]
    pub fn into_terminal(self) -> M11CleanDocumentResult {
        match self.output {
            M11OrdinaryParagraphCropOutput::Whole(terminal) => terminal,
            M11OrdinaryParagraphCropOutput::Segmented { .. } => {
                panic!("a segmented top-level crop has no whole terminal")
            }
        }
    }
}

// The large terminal is produced once and transfers several move-only
// authorities through this public poll API. Boxing would add a completion
// allocation and change the public `Complete` field solely to shrink Pending.
#[allow(clippy::large_enum_variant)]
pub enum M11OrdinaryParagraphCropPoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: M11OrdinaryParagraphCropResult,
    },
}

#[derive(Debug)]
pub enum M11OrdinaryParagraphCropError {
    Complete,
    Plan(M11OrdinaryParagraphCropPlanError),
    AuthorityMismatch,
    BindingMismatch,
    UnsupportedGrammarRevision { actual: u32 },
    PrefixCutMismatch,
    SuffixCutMismatch,
    CropDiverged,
    ConvergenceMismatch,
    CheckpointAllocationFailed,
    Source(SourceAdapterError),
    Parse(M11CleanParseJobError),
}

impl fmt::Display for M11OrdinaryParagraphCropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => formatter.write_str("ordinary Paragraph crop is already complete"),
            Self::Plan(error) => {
                write!(formatter, "ordinary Paragraph crop plan failed: {error:?}")
            }
            Self::AuthorityMismatch => {
                formatter.write_str("ordinary Paragraph crop crossed source authority")
            }
            Self::BindingMismatch => {
                formatter.write_str("ordinary Paragraph crop crossed parser binding")
            }
            Self::UnsupportedGrammarRevision { actual } => {
                write!(
                    formatter,
                    "unsupported ordinary Paragraph crop grammar revision {actual}"
                )
            }
            Self::PrefixCutMismatch => {
                formatter.write_str("ordinary Paragraph crop restart cut is not exact")
            }
            Self::SuffixCutMismatch => {
                formatter.write_str("ordinary Paragraph crop suffix cut is not exact")
            }
            Self::CropDiverged => {
                formatter.write_str("ordinary Paragraph crop changed block semantics")
            }
            Self::ConvergenceMismatch => formatter
                .write_str("ordinary Paragraph crop did not converge at the authenticated line"),
            Self::CheckpointAllocationFailed => {
                formatter.write_str("ordinary Paragraph crop checkpoint allocation failed")
            }
            Self::Source(error) => error.fmt(formatter),
            Self::Parse(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11OrdinaryParagraphCropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct OrdinaryParagraphConvergence {
    base_paragraph_start_byte: u32,
    base_paragraph_start_utf16: u32,
    base_paragraph_content_start: u32,
    base_line_start_byte: u32,
    base_line_start_utf16: u32,
    base_line_physical_bytes: u32,
    base_line_physical_utf16: u32,
    base_next_physical_line_ordinal: u32,
    base_block_entry_ordinal: u64,
    target_paragraph_start_byte: u32,
    target_paragraph_start_utf16: u32,
    target_line_start_byte: u32,
    target_line_start_utf16: u32,
    target_line_end_byte: u32,
    target_line_end_utf16: u32,
}

struct OrdinaryParagraphCropMergeCompletion {
    output: M11OrdinaryParagraphCropOutput,
    parse_work: CleanParseWork,
    target_source: SourceSnapshotLease,
    ordinal_delta: i64,
}

/// Authenticated bounded reparse between one exact prefix restart and one
/// exact unchanged successor line.
pub struct M11OrdinaryParagraphCropParseJob {
    parse: Option<M11CleanParseJob>,
    plan: Option<M11OrdinaryParagraphCropPlan>,
    target_restart: Option<M11OrdinaryParagraphRestartCheckpoint>,
    checkpoint_merge: Option<OrderedCheckpointMerge>,
    merge_completion: Option<OrdinaryParagraphCropMergeCompletion>,
    source: SourceVersion,
    binding: M11ParserBinding,
    crop_start: usize,
    convergence: OrdinaryParagraphConvergence,
}

impl M11OrdinaryParagraphCropParseJob {
    /// Consumes parser checkpoints plus runtime-minted prefix and suffix
    /// lineage authority. The suffix witness alone is insufficient: the
    /// bounded scan must observe its mapped start as the expected physical
    /// successor line before this job can complete.
    pub fn new(
        mut plan: M11OrdinaryParagraphCropPlan,
        prefix: ExactUnchangedPrefixWitness,
        suffix: ExactUnchangedSuffixWitness,
        target: SourceSnapshotLease,
        binding: M11ParserBinding,
    ) -> Result<Self, M11OrdinaryParagraphCropError> {
        if binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(M11OrdinaryParagraphCropError::UnsupportedGrammarRevision {
                actual: binding.grammar_revision(),
            });
        }
        let selection = plan.selection();
        if selection.binding() != binding {
            return Err(M11OrdinaryParagraphCropError::BindingMismatch);
        }
        if selection.source() != prefix.base()
            || selection.source() != suffix.base()
            || target.version() != prefix.target()
            || target.version() != suffix.target()
        {
            return Err(M11OrdinaryParagraphCropError::AuthorityMismatch);
        }

        let convergence = plan
            .convergence()
            .map_err(M11OrdinaryParagraphCropError::Plan)?;
        if !convergence.metrics_are_consistent()
            || convergence.source() != selection.source()
            || convergence.binding() != binding
            || convergence.preceding_line_start_byte() != selection.convergence_line_start_byte()
            || convergence.preceding_line_start_utf16() != selection.convergence_line_start_utf16()
        {
            return Err(M11OrdinaryParagraphCropError::ConvergenceMismatch);
        }
        let base_line_offset_bytes = if selection.is_segmented_top_level() {
            convergence
                .preceding_line_start_byte()
                .checked_sub(convergence.paragraph_source_start_byte())
                .ok_or(M11OrdinaryParagraphCropError::SuffixCutMismatch)? as usize
        } else {
            0
        };
        let base_line_offset_utf16 = if selection.is_segmented_top_level() {
            convergence
                .preceding_line_start_utf16()
                .checked_sub(convergence.paragraph_source_start_utf16())
                .ok_or(M11OrdinaryParagraphCropError::SuffixCutMismatch)? as usize
        } else {
            0
        };
        let target_line_start = suffix
            .target_byte_start()
            .checked_add(base_line_offset_bytes)
            .ok_or(M11OrdinaryParagraphCropError::SuffixCutMismatch)?;
        let target_line_start_utf16 = suffix
            .target_utf16_start()
            .checked_add(base_line_offset_utf16)
            .ok_or(M11OrdinaryParagraphCropError::SuffixCutMismatch)?;
        let target_line_end = target_line_start
            .checked_add(convergence.preceding_line_physical_bytes() as usize)
            .ok_or(M11OrdinaryParagraphCropError::SuffixCutMismatch)?;
        let target_line_end_utf16 = target_line_start_utf16
            .checked_add(convergence.preceding_line_physical_utf16() as usize)
            .ok_or(M11OrdinaryParagraphCropError::SuffixCutMismatch)?;
        let convergence = OrdinaryParagraphConvergence {
            base_paragraph_start_byte: convergence.paragraph_source_start_byte(),
            base_paragraph_start_utf16: convergence.paragraph_source_start_utf16(),
            base_paragraph_content_start: convergence.paragraph_content_start(),
            base_line_start_byte: convergence.preceding_line_start_byte(),
            base_line_start_utf16: convergence.preceding_line_start_utf16(),
            base_line_physical_bytes: convergence.preceding_line_physical_bytes(),
            base_line_physical_utf16: convergence.preceding_line_physical_utf16(),
            base_next_physical_line_ordinal: convergence.next_physical_line_ordinal(),
            base_block_entry_ordinal: convergence.block_entry_ordinal(),
            target_paragraph_start_byte: u32::try_from(suffix.target_byte_start())
                .map_err(|_| M11OrdinaryParagraphCropError::SuffixCutMismatch)?,
            target_paragraph_start_utf16: u32::try_from(suffix.target_utf16_start())
                .map_err(|_| M11OrdinaryParagraphCropError::SuffixCutMismatch)?,
            target_line_start_byte: u32::try_from(target_line_start)
                .map_err(|_| M11OrdinaryParagraphCropError::SuffixCutMismatch)?,
            target_line_start_utf16: u32::try_from(target_line_start_utf16)
                .map_err(|_| M11OrdinaryParagraphCropError::SuffixCutMismatch)?,
            target_line_end_byte: u32::try_from(target_line_end)
                .map_err(|_| M11OrdinaryParagraphCropError::SuffixCutMismatch)?,
            target_line_end_utf16: u32::try_from(target_line_end_utf16)
                .map_err(|_| M11OrdinaryParagraphCropError::SuffixCutMismatch)?,
        };
        if suffix.base_byte_start() != selection.convergence_suffix_start_byte() as usize
            || suffix.base_utf16_start() != selection.convergence_suffix_start_utf16() as usize
        {
            return Err(M11OrdinaryParagraphCropError::SuffixCutMismatch);
        }

        let restart = plan
            .take_restart()
            .map_err(M11OrdinaryParagraphCropError::Plan)?;
        let crop_start = restart.prefix_end_byte() as usize;
        let crop_start_utf16 = restart.prefix_end_utf16() as usize;
        if !restart.metrics_are_consistent()
            || restart.source() != selection.source()
            || restart.binding() != binding
            || restart.prefix_end_byte() != selection.restart_prefix_end_byte()
            || restart.prefix_end_utf16() != selection.restart_prefix_end_utf16()
            || prefix.byte_end() != crop_start
            || prefix.utf16_end() != crop_start_utf16
            || convergence.base_next_physical_line_ordinal <= restart.next_physical_line_ordinal()
            || convergence.target_line_start_byte < restart.prefix_end_byte()
            || convergence.target_line_end_byte <= convergence.target_line_start_byte
            || selection.is_segmented_top_level()
                && convergence.target_line_end_byte as usize - crop_start
                    > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES
        {
            return Err(M11OrdinaryParagraphCropError::PrefixCutMismatch);
        }
        let source = target.version();
        let target_end = convergence.target_line_end_byte as usize;
        let target_end_utf16 = convergence.target_line_end_utf16 as usize;
        if target_end > source.byte_len()
            || target_end_utf16 > source.utf16_len()
            || target
                .utf16_offset_for_byte(crop_start)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphCropError::Source)?
                != crop_start_utf16
            || target
                .utf16_offset_for_byte(convergence.target_line_start_byte as usize)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphCropError::Source)?
                != convergence.target_line_start_utf16 as usize
            || target
                .byte_offset_for_utf16(convergence.target_line_start_utf16 as usize)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphCropError::Source)?
                != convergence.target_line_start_byte as usize
            || target
                .utf16_offset_for_byte(target_end)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphCropError::Source)?
                != target_end_utf16
        {
            return Err(M11OrdinaryParagraphCropError::SuffixCutMismatch);
        }

        let target_restart = restart.for_target(source);
        let next_ordinal = restart.next_physical_line_ordinal();
        let controller =
            M11CleanBlockController::new_for_ordinary_paragraph_remainder(source, restart);
        let scanner = SnapshotLineScanner::new_in(target, crop_start..target_end, next_ordinal)
            .map_err(M11OrdinaryParagraphCropError::Source)?;
        Ok(Self {
            parse: Some(M11CleanParseJob {
                scanner: Some(scanner),
                completed_source: None,
                controller: Some(controller),
                pending_line: None,
                active: None,
                work: CleanParseWork::default(),
                finish: Some(CleanParseFinish::OrdinaryParagraphCrop {
                    expected_end_byte: convergence.target_line_end_byte,
                    expected_end_utf16: convergence.target_line_end_utf16,
                }),
            }),
            plan: Some(plan),
            target_restart: Some(target_restart),
            checkpoint_merge: None,
            merge_completion: None,
            source,
            binding,
            crop_start,
            convergence,
        })
    }

    pub fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<M11OrdinaryParagraphCropPoll, M11OrdinaryParagraphCropError> {
        if fuel == 0 {
            return Ok(M11OrdinaryParagraphCropPoll::Pending { transitions: 0 });
        }
        let mut transitions = 0;
        loop {
            if let Some(merge) = self.checkpoint_merge.as_mut() {
                let merge_poll = merge
                    .poll(fuel - transitions)
                    .map_err(Self::map_checkpoint_merge_error)?;
                match merge_poll {
                    OrderedCheckpointMergePoll::Pending {
                        transitions: consumed,
                    } => {
                        transitions = transitions
                            .checked_add(consumed)
                            .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
                        return Ok(M11OrdinaryParagraphCropPoll::Pending { transitions });
                    }
                    OrderedCheckpointMergePoll::Complete {
                        transitions: consumed,
                        result: merged,
                    } => {
                        transitions = transitions
                            .checked_add(consumed)
                            .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
                        self.checkpoint_merge.take();
                        let completion = self
                            .merge_completion
                            .take()
                            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
                        return Ok(M11OrdinaryParagraphCropPoll::Complete {
                            transitions,
                            result: M11OrdinaryParagraphCropResult {
                                output: completion.output,
                                work: M11OrdinaryParagraphCropWork {
                                    target_crop_bytes: self.crop_start
                                        ..self.convergence.target_line_end_byte as usize,
                                    crop_source_bytes_discovered: completion
                                        .parse_work
                                        .source_bytes_discovered,
                                    crop_source_bytes_read: completion.parse_work.source_bytes_read,
                                    crop_physical_lines_discovered: completion
                                        .parse_work
                                        .physical_lines_discovered,
                                    crop_parser_transitions: completion
                                        .parse_work
                                        .parser_transitions,
                                    reused_prefix_checkpoints: merged
                                        .work
                                        .reused_prefix_checkpoints,
                                    fresh_crop_checkpoints: merged.work.fresh_crop_checkpoints,
                                    reused_suffix_checkpoints: merged
                                        .work
                                        .reused_suffix_checkpoints,
                                    convergence_ordinal_delta: completion.ordinal_delta,
                                    checkpoint_merge_transitions: merged.work.transitions,
                                    maximum_checkpoint_records_per_transition: merged
                                        .work
                                        .maximum_records_per_transition,
                                },
                                base_checkpoints: Some(merged.base),
                                next_checkpoints: Some(merged.target),
                                target_source: Some(completion.target_source),
                            },
                        });
                    }
                }
            }
            if transitions == fuel {
                return Ok(M11OrdinaryParagraphCropPoll::Pending { transitions });
            }
            let poll = {
                let parse = self
                    .parse
                    .as_mut()
                    .ok_or(M11OrdinaryParagraphCropError::Complete)?;
                parse.poll_internal(fuel - transitions)
            };
            let poll = match poll {
                Ok(poll) => poll,
                Err(
                    M11CleanParseJobError::Controller(M11CleanControllerError::Controller(
                        M11CleanControllerFault::OrdinaryParagraphCropDiverged,
                    ))
                    | M11CleanParseJobError::Finish(
                        M11CleanControllerFault::OrdinaryParagraphCropDiverged,
                    ),
                ) => {
                    self.parse.take();
                    return Err(M11OrdinaryParagraphCropError::CropDiverged);
                }
                Err(error) => {
                    self.parse.take();
                    return Err(M11OrdinaryParagraphCropError::Parse(error));
                }
            };
            match poll {
                CleanParseInternalPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
                    return Ok(M11OrdinaryParagraphCropPoll::Pending { transitions });
                }
                CleanParseInternalPoll::Complete {
                    transitions: consumed,
                    terminal: CleanParseTerminal::OrdinaryParagraphCrop(terminal),
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
                    let mut parse = self
                        .parse
                        .take()
                        .ok_or(M11OrdinaryParagraphCropError::Complete)?;
                    let target_source = parse
                        .take_completed_source()
                        .filter(|lease| lease.version() == self.source)
                        .ok_or(M11OrdinaryParagraphCropError::AuthorityMismatch)?;
                    self.begin_checkpoint_merge(terminal, parse.work, target_source)?;
                }
                CleanParseInternalPoll::Complete {
                    terminal:
                        CleanParseTerminal::WholeSource(_)
                        | CleanParseTerminal::OrdinaryParagraphEofCrop(_),
                    ..
                } => unreachable!("ordinary Paragraph crop has a typed finish mode"),
            }
        }
    }

    /// Cancels the bounded target parse and reconstructs the original
    /// move-only base collection without copying the document-sized vector.
    pub fn cancel_into_base_restart_checkpoints(
        mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCropError> {
        if let Some(merge) = self.checkpoint_merge.take() {
            drop(self.parse.take());
            self.merge_completion.take();
            return merge
                .cancel_into_base()
                .map_err(Self::map_checkpoint_merge_error);
        }
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
        let target_restart = self
            .target_restart
            .take()
            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
        let base_restart = target_restart.for_target(plan.selection().source());
        drop(self.parse.take());
        plan.restore_base_checkpoints(base_restart)
            .map_err(M11OrdinaryParagraphCropError::Plan)
    }

    fn begin_checkpoint_merge(
        &mut self,
        mut terminal: M11OrdinaryParagraphCropTerminal,
        parse_work: CleanParseWork,
        target_source: SourceSnapshotLease,
    ) -> Result<(), M11OrdinaryParagraphCropError> {
        let last = terminal.last_committed_line;
        if terminal.source != self.source
            || terminal.crop_end_byte != self.convergence.target_line_end_byte
            || terminal.crop_end_utf16 != self.convergence.target_line_end_utf16
            || terminal.next_physical_line_ordinal
                != last
                    .ordinal
                    .checked_add(1)
                    .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?
            || last.start_byte != self.convergence.target_line_start_byte
            || last.start_utf16 != self.convergence.target_line_start_utf16
            || last.physical_bytes != self.convergence.base_line_physical_bytes
            || last.physical_utf16 != self.convergence.base_line_physical_utf16
        {
            return Err(M11OrdinaryParagraphCropError::ConvergenceMismatch);
        }
        let base_convergence_ordinal = self
            .convergence
            .base_next_physical_line_ordinal
            .checked_sub(1)
            .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
        let ordinal_delta = i64::from(last.ordinal) - i64::from(base_convergence_ordinal);
        let byte_delta = i64::from(self.convergence.target_line_start_byte)
            - i64::from(self.convergence.base_line_start_byte);
        let utf16_delta = i64::from(self.convergence.target_line_start_utf16)
            - i64::from(self.convergence.base_line_start_utf16);
        let paragraph_content_start = terminal.paragraph_content_start;
        let selection = self
            .plan
            .as_ref()
            .map(M11OrdinaryParagraphCropPlan::selection)
            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
        let target_restart = self
            .target_restart
            .as_ref()
            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
        let (output, block_ordinal_delta, target_top_level_block_count) = if selection
            .is_segmented_top_level()
        {
            let paragraph_content_offset = self
                .convergence
                .base_paragraph_content_start
                .checked_sub(self.convergence.base_paragraph_start_byte)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let expected_target_content_start = self
                .convergence
                .target_paragraph_start_byte
                .checked_add(paragraph_content_offset)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let base_line_offset_bytes = self
                .convergence
                .base_line_start_byte
                .checked_sub(self.convergence.base_paragraph_start_byte)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let target_line_offset_bytes = self
                .convergence
                .target_line_start_byte
                .checked_sub(self.convergence.target_paragraph_start_byte)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let base_line_offset_utf16 = self
                .convergence
                .base_line_start_utf16
                .checked_sub(self.convergence.base_paragraph_start_utf16)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let target_line_offset_utf16 = self
                .convergence
                .target_line_start_utf16
                .checked_sub(self.convergence.target_paragraph_start_utf16)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            if terminal.paragraph_source_start_byte != self.convergence.target_paragraph_start_byte
                || terminal.paragraph_source_start_utf16
                    != self.convergence.target_paragraph_start_utf16
                || terminal.paragraph_content_start != expected_target_content_start
                || terminal.replacement_start_byte != target_restart.paragraph_source_start_byte()
                || terminal.replacement_start_utf16 != target_restart.paragraph_source_start_utf16()
                || terminal.replacement_start_byte >= self.convergence.target_paragraph_start_byte
                || terminal.replacement_start_utf16 >= self.convergence.target_paragraph_start_utf16
                || target_restart.block_entry_ordinal() != selection.restart_block_entry_ordinal()
                || self.convergence.base_block_entry_ordinal
                    != selection.convergence_block_entry_ordinal()
                || base_line_offset_bytes != target_line_offset_bytes
                || base_line_offset_utf16 != target_line_offset_utf16
            {
                return Err(M11OrdinaryParagraphCropError::ConvergenceMismatch);
            }
            let replacement_count = u64::try_from(terminal.replacement_leaves.len())
                .map_err(|_| M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let removed_count = selection
                .convergence_block_entry_ordinal()
                .checked_sub(selection.restart_block_entry_ordinal())
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let block_ordinal_delta = i64::try_from(replacement_count)
                .ok()
                .and_then(|replacement| {
                    i64::try_from(removed_count)
                        .ok()
                        .and_then(|removed| replacement.checked_sub(removed))
                })
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let target_end = selection
                .restart_block_entry_ordinal()
                .checked_add(replacement_count)
                .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let block_splice = M11BlockSequenceSpliceSelection::new(
                selection.restart_block_entry_ordinal()
                    ..selection.convergence_block_entry_ordinal(),
                selection.restart_block_entry_ordinal()..target_end,
            )
            .map_err(|_| M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            let target_top_level_block_count = shift_top_level_block_count(
                selection.base_block_entry_count(),
                block_ordinal_delta,
            )
            .filter(|count| *count != 0)
            .ok_or(M11OrdinaryParagraphCropError::ConvergenceMismatch)?;
            (
                M11OrdinaryParagraphCropOutput::Segmented {
                    replacement_leaves: std::mem::take(&mut terminal.replacement_leaves),
                    block_splice,
                },
                block_ordinal_delta,
                target_top_level_block_count,
            )
        } else {
            if terminal.paragraph_source_start_byte != 0
                || terminal.paragraph_source_start_utf16 != 0
                || terminal.replacement_start_byte != 0
                || terminal.replacement_start_utf16 != 0
                || !terminal.replacement_leaves.is_empty()
            {
                return Err(M11OrdinaryParagraphCropError::CropDiverged);
            }
            (
                M11OrdinaryParagraphCropOutput::Whole(
                    M11CleanDocumentResult::from_ordinary_paragraph_crop(
                        self.source,
                        paragraph_content_start,
                    )
                    .map_err(M11CleanParseJobError::Finish)
                    .map_err(M11OrdinaryParagraphCropError::Parse)?,
                ),
                0,
                1,
            )
        };
        let fresh = terminal.take_fresh_checkpoint_cursor(self.binding);
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
        let target_restart = self
            .target_restart
            .take()
            .ok_or(M11OrdinaryParagraphCropError::Complete)?;
        let base_restart = target_restart.for_target(selection.source());
        let base_checkpoints = plan
            .restore_base_checkpoints(base_restart)
            .map_err(M11OrdinaryParagraphCropError::Plan)?;
        let merge = OrderedCheckpointMerge::interior(
            self.source,
            self.binding,
            base_checkpoints,
            target_restart,
            fresh,
            selection.restart_index(),
            selection.convergence_index(),
            byte_delta,
            utf16_delta,
            ordinal_delta,
            block_ordinal_delta,
            selection.is_segmented_top_level(),
            target_top_level_block_count,
        )
        .map_err(Self::map_checkpoint_merge_error)?;
        self.checkpoint_merge = Some(merge);
        self.merge_completion = Some(OrdinaryParagraphCropMergeCompletion {
            output,
            parse_work,
            target_source,
            ordinal_delta,
        });
        Ok(())
    }

    fn map_checkpoint_merge_error(
        error: OrderedCheckpointMergeError,
    ) -> M11OrdinaryParagraphCropError {
        match error {
            OrderedCheckpointMergeError::Complete => M11OrdinaryParagraphCropError::Complete,
            OrderedCheckpointMergeError::InvalidBoundary => {
                M11OrdinaryParagraphCropError::ConvergenceMismatch
            }
            OrderedCheckpointMergeError::AllocationFailed => {
                M11OrdinaryParagraphCropError::CheckpointAllocationFailed
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11OrdinaryParagraphBoundaryCropWork {
    target_crop_bytes: std::ops::Range<usize>,
    crop_source_bytes_discovered: usize,
    crop_source_bytes_read: usize,
    reused_prefix_checkpoints: usize,
    fresh_crop_checkpoints: usize,
    reused_suffix_checkpoints: usize,
    convergence_ordinal_delta: Option<i64>,
    checkpoint_merge_transitions: usize,
    maximum_checkpoint_records_per_transition: usize,
}

impl M11OrdinaryParagraphBoundaryCropWork {
    #[must_use]
    pub fn target_crop_bytes(&self) -> std::ops::Range<usize> {
        self.target_crop_bytes.clone()
    }

    #[must_use]
    pub const fn crop_source_bytes_discovered(&self) -> usize {
        self.crop_source_bytes_discovered
    }

    #[must_use]
    pub const fn crop_source_bytes_read(&self) -> usize {
        self.crop_source_bytes_read
    }

    #[must_use]
    pub const fn reused_prefix_checkpoints(&self) -> usize {
        self.reused_prefix_checkpoints
    }

    #[must_use]
    pub const fn fresh_crop_checkpoints(&self) -> usize {
        self.fresh_crop_checkpoints
    }

    #[must_use]
    pub const fn reused_suffix_checkpoints(&self) -> usize {
        self.reused_suffix_checkpoints
    }

    #[must_use]
    pub const fn convergence_ordinal_delta(&self) -> Option<i64> {
        self.convergence_ordinal_delta
    }

    #[must_use]
    pub const fn checkpoint_merge_transitions(&self) -> usize {
        self.checkpoint_merge_transitions
    }

    #[must_use]
    pub const fn maximum_checkpoint_records_per_transition(&self) -> usize {
        self.maximum_checkpoint_records_per_transition
    }
}

pub struct M11OrdinaryParagraphBoundaryCropResult {
    output: M11OrdinaryParagraphCropOutput,
    work: M11OrdinaryParagraphBoundaryCropWork,
    base_checkpoints: Option<M11OrdinaryParagraphRestartCheckpoints>,
    next_checkpoints: Option<M11OrdinaryParagraphRestartCheckpoints>,
    target_source: Option<SourceSnapshotLease>,
}

impl M11OrdinaryParagraphBoundaryCropResult {
    #[must_use]
    pub fn terminal(&self) -> &M11CleanDocumentResult {
        match &self.output {
            M11OrdinaryParagraphCropOutput::Whole(terminal) => terminal,
            M11OrdinaryParagraphCropOutput::Segmented { .. } => {
                panic!("a segmented top-level boundary crop has replacement leaves")
            }
        }
    }

    #[must_use]
    pub const fn work(&self) -> &M11OrdinaryParagraphBoundaryCropWork {
        &self.work
    }

    /// Takes the move-only base collection retained until the target commits.
    pub fn take_base_restart_checkpoints(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        self.base_checkpoints
            .take()
            .ok_or(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
    }

    pub fn take_next_restart_checkpoints(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        self.next_checkpoints
            .take()
            .ok_or(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
    }

    /// Takes the exact target lease consumed by this crop exactly once.
    #[must_use]
    pub fn take_target_source_lease(&mut self) -> Option<SourceSnapshotLease> {
        self.target_source.take()
    }

    /// Consumes this authenticated boundary crop into the only parser input
    /// that may retain exact-base References while rebuilding target blocks.
    pub fn into_exact_segmented_candidate_input(
        mut self,
    ) -> Result<M11ExactSegmentedCandidateInput, M11CandidateDerivationError> {
        let source_lease = self
            .target_source
            .take()
            .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
        match self.output {
            M11OrdinaryParagraphCropOutput::Whole(terminal) => {
                M11ExactSegmentedCandidateInput::from_ordinary_crop(source_lease, terminal)
            }
            M11OrdinaryParagraphCropOutput::Segmented {
                replacement_leaves,
                block_splice,
            } => M11ExactSegmentedCandidateInput::from_segmented_crop(
                source_lease,
                replacement_leaves,
                block_splice,
            ),
        }
    }

    #[must_use]
    pub fn into_terminal(self) -> M11CleanDocumentResult {
        match self.output {
            M11OrdinaryParagraphCropOutput::Whole(terminal) => terminal,
            M11OrdinaryParagraphCropOutput::Segmented { .. } => {
                panic!("a segmented top-level boundary crop has no whole terminal")
            }
        }
    }
}

// The large terminal is produced once and transfers several move-only
// authorities through this public poll API. Boxing would add a completion
// allocation and change the public `Complete` field solely to shrink Pending.
#[allow(clippy::large_enum_variant)]
pub enum M11OrdinaryParagraphBoundaryCropPoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: M11OrdinaryParagraphBoundaryCropResult,
    },
}

#[derive(Debug)]
pub enum M11OrdinaryParagraphBoundaryCropError {
    Complete,
    Plan(M11OrdinaryParagraphBoundaryCropPlanError),
    Restart(M11OrdinaryParagraphRestartError),
    AuthorityMismatch,
    BindingMismatch,
    UnsupportedGrammarRevision { actual: u32 },
    SuffixCutMismatch,
    CropDiverged,
    ConvergenceMismatch,
    CheckpointAllocationFailed,
    Source(SourceAdapterError),
    Parse(M11CleanParseJobError),
}

impl fmt::Display for M11OrdinaryParagraphBoundaryCropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => formatter.write_str("ordinary Paragraph edge crop is complete"),
            Self::Plan(error) => {
                write!(
                    formatter,
                    "ordinary Paragraph edge crop plan failed: {error:?}"
                )
            }
            Self::Restart(error) => error.fmt(formatter),
            Self::AuthorityMismatch => {
                formatter.write_str("ordinary Paragraph edge crop crossed source authority")
            }
            Self::BindingMismatch => {
                formatter.write_str("ordinary Paragraph edge crop crossed parser binding")
            }
            Self::UnsupportedGrammarRevision { actual } => {
                write!(
                    formatter,
                    "unsupported ordinary Paragraph edge crop grammar revision {actual}"
                )
            }
            Self::SuffixCutMismatch => {
                formatter.write_str("ordinary Paragraph BOF crop suffix cut is not exact")
            }
            Self::CropDiverged => {
                formatter.write_str("ordinary Paragraph edge crop changed block semantics")
            }
            Self::ConvergenceMismatch => {
                formatter.write_str("ordinary Paragraph edge crop did not converge")
            }
            Self::CheckpointAllocationFailed => {
                formatter.write_str("ordinary Paragraph edge checkpoint allocation failed")
            }
            Self::Source(error) => error.fmt(formatter),
            Self::Parse(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11OrdinaryParagraphBoundaryCropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Restart(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

struct OrdinaryParagraphBoundaryCropMergeCompletion {
    output: M11OrdinaryParagraphCropOutput,
    parse_work: CleanParseWork,
    target_source: SourceSnapshotLease,
    target_crop_bytes: std::ops::Range<usize>,
    convergence_ordinal_delta: Option<i64>,
}

fn map_boundary_checkpoint_merge_error(
    error: OrderedCheckpointMergeError,
) -> M11OrdinaryParagraphBoundaryCropError {
    match error {
        OrderedCheckpointMergeError::Complete => M11OrdinaryParagraphBoundaryCropError::Complete,
        OrderedCheckpointMergeError::InvalidBoundary => {
            M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch
        }
        OrderedCheckpointMergeError::AllocationFailed => {
            M11OrdinaryParagraphBoundaryCropError::CheckpointAllocationFailed
        }
    }
}

fn finish_boundary_crop_merge(
    completion: OrdinaryParagraphBoundaryCropMergeCompletion,
    merged: OrderedCheckpointMergeResult,
) -> M11OrdinaryParagraphBoundaryCropResult {
    M11OrdinaryParagraphBoundaryCropResult {
        output: completion.output,
        work: M11OrdinaryParagraphBoundaryCropWork {
            target_crop_bytes: completion.target_crop_bytes,
            crop_source_bytes_discovered: completion.parse_work.source_bytes_discovered,
            crop_source_bytes_read: completion.parse_work.source_bytes_read,
            reused_prefix_checkpoints: merged.work.reused_prefix_checkpoints,
            fresh_crop_checkpoints: merged.work.fresh_crop_checkpoints,
            reused_suffix_checkpoints: merged.work.reused_suffix_checkpoints,
            convergence_ordinal_delta: completion.convergence_ordinal_delta,
            checkpoint_merge_transitions: merged.work.transitions,
            maximum_checkpoint_records_per_transition: merged.work.maximum_records_per_transition,
        },
        base_checkpoints: Some(merged.base),
        next_checkpoints: Some(merged.target),
        target_source: Some(completion.target_source),
    }
}

/// BOF-to-convergence parse authenticated only by an exact unchanged suffix.
pub struct M11OrdinaryParagraphBofCropParseJob {
    parse: Option<M11CleanParseJob>,
    plan: Option<M11OrdinaryParagraphBofCropPlan>,
    checkpoint_merge: Option<OrderedCheckpointMerge>,
    merge_completion: Option<OrdinaryParagraphBoundaryCropMergeCompletion>,
    source: SourceVersion,
    binding: M11ParserBinding,
    convergence: OrdinaryParagraphConvergence,
}

impl M11OrdinaryParagraphBofCropParseJob {
    pub fn new(
        plan: M11OrdinaryParagraphBofCropPlan,
        suffix: ExactUnchangedSuffixWitness,
        target: SourceSnapshotLease,
        binding: M11ParserBinding,
    ) -> Result<Self, M11OrdinaryParagraphBoundaryCropError> {
        if binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(
                M11OrdinaryParagraphBoundaryCropError::UnsupportedGrammarRevision {
                    actual: binding.grammar_revision(),
                },
            );
        }
        let selection = plan.selection();
        if selection.binding() != binding {
            return Err(M11OrdinaryParagraphBoundaryCropError::BindingMismatch);
        }
        if selection.source() != suffix.base() || target.version() != suffix.target() {
            return Err(M11OrdinaryParagraphBoundaryCropError::AuthorityMismatch);
        }
        let base_convergence = plan
            .convergence()
            .map_err(M11OrdinaryParagraphBoundaryCropError::Plan)?;
        if !base_convergence.metrics_are_consistent()
            || base_convergence.source() != selection.source()
            || base_convergence.binding() != binding
            || base_convergence.preceding_line_start_byte()
                != selection.convergence_line_start_byte()
            || base_convergence.preceding_line_start_utf16()
                != selection.convergence_line_start_utf16()
            || suffix.base_byte_start() != selection.convergence_suffix_start_byte() as usize
            || suffix.base_utf16_start() != selection.convergence_suffix_start_utf16() as usize
            || selection.is_segmented_top_level()
                && (base_convergence.paragraph_source_start_byte()
                    != selection.convergence_suffix_start_byte()
                    || base_convergence.paragraph_source_start_utf16()
                        != selection.convergence_suffix_start_utf16()
                    || base_convergence.block_entry_ordinal()
                        != selection.convergence_block_entry_ordinal())
        {
            return Err(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch);
        }
        let target_suffix_start_byte = u32::try_from(suffix.target_byte_start())
            .map_err(|_| M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?;
        let target_suffix_start_utf16 = u32::try_from(suffix.target_utf16_start())
            .map_err(|_| M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?;
        let (target_paragraph_start_byte, target_paragraph_start_utf16) =
            if selection.is_segmented_top_level() {
                (target_suffix_start_byte, target_suffix_start_utf16)
            } else {
                (0, 0)
            };
        let target_line_start_byte = if selection.is_segmented_top_level() {
            target_paragraph_start_byte
                .checked_add(
                    base_convergence
                        .preceding_line_start_byte()
                        .checked_sub(base_convergence.paragraph_source_start_byte())
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?,
                )
                .ok_or(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?
        } else {
            target_suffix_start_byte
        };
        let target_line_start_utf16 = if selection.is_segmented_top_level() {
            target_paragraph_start_utf16
                .checked_add(
                    base_convergence
                        .preceding_line_start_utf16()
                        .checked_sub(base_convergence.paragraph_source_start_utf16())
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?,
                )
                .ok_or(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?
        } else {
            target_suffix_start_utf16
        };
        let target_line_end_byte = target_line_start_byte
            .checked_add(base_convergence.preceding_line_physical_bytes())
            .ok_or(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?;
        let target_line_end_utf16 = target_line_start_utf16
            .checked_add(base_convergence.preceding_line_physical_utf16())
            .ok_or(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch)?;
        let convergence = OrdinaryParagraphConvergence {
            base_paragraph_start_byte: base_convergence.paragraph_source_start_byte(),
            base_paragraph_start_utf16: base_convergence.paragraph_source_start_utf16(),
            base_paragraph_content_start: base_convergence.paragraph_content_start(),
            base_line_start_byte: base_convergence.preceding_line_start_byte(),
            base_line_start_utf16: base_convergence.preceding_line_start_utf16(),
            base_line_physical_bytes: base_convergence.preceding_line_physical_bytes(),
            base_line_physical_utf16: base_convergence.preceding_line_physical_utf16(),
            base_next_physical_line_ordinal: base_convergence.next_physical_line_ordinal(),
            base_block_entry_ordinal: base_convergence.block_entry_ordinal(),
            target_paragraph_start_byte,
            target_paragraph_start_utf16,
            target_line_start_byte,
            target_line_start_utf16,
            target_line_end_byte,
            target_line_end_utf16,
        };
        let source = target.version();
        if target_line_end_byte as usize > source.byte_len()
            || target_line_end_utf16 as usize > source.utf16_len()
            || target
                .utf16_offset_for_byte(target_line_start_byte as usize)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphBoundaryCropError::Source)?
                != target_line_start_utf16 as usize
            || target
                .byte_offset_for_utf16(target_line_start_utf16 as usize)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphBoundaryCropError::Source)?
                != target_line_start_byte as usize
            || target
                .utf16_offset_for_byte(target_line_end_byte as usize)
                .map_err(SourceAdapterError::from)
                .map_err(M11OrdinaryParagraphBoundaryCropError::Source)?
                != target_line_end_utf16 as usize
        {
            return Err(M11OrdinaryParagraphBoundaryCropError::SuffixCutMismatch);
        }
        let scanner = SnapshotLineScanner::new_in(target, 0..target_line_end_byte as usize, 0)
            .map_err(M11OrdinaryParagraphBoundaryCropError::Source)?;
        Ok(Self {
            parse: Some(M11CleanParseJob {
                scanner: Some(scanner),
                completed_source: None,
                controller: Some(M11CleanBlockController::new_for_source(source)),
                pending_line: None,
                active: None,
                work: CleanParseWork::default(),
                finish: Some(CleanParseFinish::OrdinaryParagraphCrop {
                    expected_end_byte: target_line_end_byte,
                    expected_end_utf16: target_line_end_utf16,
                }),
            }),
            plan: Some(plan),
            checkpoint_merge: None,
            merge_completion: None,
            source,
            binding,
            convergence,
        })
    }

    pub fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<M11OrdinaryParagraphBoundaryCropPoll, M11OrdinaryParagraphBoundaryCropError> {
        if fuel == 0 {
            return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions: 0 });
        }
        let mut transitions = 0;
        loop {
            if let Some(merge) = self.checkpoint_merge.as_mut() {
                let merge_poll = merge
                    .poll(fuel - transitions)
                    .map_err(map_boundary_checkpoint_merge_error)?;
                match merge_poll {
                    OrderedCheckpointMergePoll::Pending {
                        transitions: consumed,
                    } => {
                        transitions = transitions
                            .checked_add(consumed)
                            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                        return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions });
                    }
                    OrderedCheckpointMergePoll::Complete {
                        transitions: consumed,
                        result: merged,
                    } => {
                        transitions = transitions
                            .checked_add(consumed)
                            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                        self.checkpoint_merge.take();
                        let completion = self
                            .merge_completion
                            .take()
                            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                        return Ok(M11OrdinaryParagraphBoundaryCropPoll::Complete {
                            transitions,
                            result: finish_boundary_crop_merge(completion, merged),
                        });
                    }
                }
            }
            if transitions == fuel {
                return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions });
            }
            let poll = {
                let parse = self
                    .parse
                    .as_mut()
                    .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                parse.poll_internal(fuel - transitions)
            };
            let poll = match poll {
                Ok(poll) => poll,
                Err(
                    M11CleanParseJobError::Controller(M11CleanControllerError::Controller(
                        M11CleanControllerFault::OrdinaryParagraphCropDiverged,
                    ))
                    | M11CleanParseJobError::Finish(
                        M11CleanControllerFault::OrdinaryParagraphCropDiverged,
                    ),
                ) => {
                    self.parse.take();
                    return Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged);
                }
                Err(error) => {
                    self.parse.take();
                    return Err(M11OrdinaryParagraphBoundaryCropError::Parse(error));
                }
            };
            match poll {
                CleanParseInternalPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                    return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions });
                }
                CleanParseInternalPoll::Complete {
                    transitions: consumed,
                    terminal: CleanParseTerminal::OrdinaryParagraphCrop(terminal),
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                    let mut parse = self
                        .parse
                        .take()
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                    let target_source = parse
                        .take_completed_source()
                        .filter(|lease| lease.version() == self.source)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::AuthorityMismatch)?;
                    self.begin_checkpoint_merge(terminal, parse.work, target_source)?;
                }
                CleanParseInternalPoll::Complete {
                    terminal:
                        CleanParseTerminal::WholeSource(_)
                        | CleanParseTerminal::OrdinaryParagraphEofCrop(_),
                    ..
                } => unreachable!("BOF crop has a typed bounded finish"),
            }
        }
    }

    /// Cancels the bounded target parse and hands the untouched base collection
    /// back to the endpoint.
    pub fn cancel_into_base_restart_checkpoints(
        mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphBoundaryCropError> {
        if let Some(merge) = self.checkpoint_merge.take() {
            drop(self.parse.take());
            self.merge_completion.take();
            return merge
                .cancel_into_base()
                .map_err(map_boundary_checkpoint_merge_error);
        }
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let selection = plan.selection();
        drop(self.parse.take());
        Ok(M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(
            selection.source(),
            selection.binding(),
            plan.into_base_checkpoints(),
            selection.base_block_entry_count(),
        ))
    }

    fn begin_checkpoint_merge(
        &mut self,
        mut terminal: M11OrdinaryParagraphCropTerminal,
        parse_work: CleanParseWork,
        target_source: SourceSnapshotLease,
    ) -> Result<(), M11OrdinaryParagraphBoundaryCropError> {
        let last = terminal.last_committed_line;
        if terminal.source != self.source
            || terminal.crop_end_byte != self.convergence.target_line_end_byte
            || terminal.crop_end_utf16 != self.convergence.target_line_end_utf16
            || terminal.next_physical_line_ordinal
                != last
                    .ordinal
                    .checked_add(1)
                    .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?
            || last.start_byte != self.convergence.target_line_start_byte
            || last.start_utf16 != self.convergence.target_line_start_utf16
            || last.physical_bytes != self.convergence.base_line_physical_bytes
            || last.physical_utf16 != self.convergence.base_line_physical_utf16
        {
            return Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch);
        }
        let base_ordinal = self
            .convergence
            .base_next_physical_line_ordinal
            .checked_sub(1)
            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
        let ordinal_delta = i64::from(last.ordinal) - i64::from(base_ordinal);
        let byte_delta = i64::from(self.convergence.target_line_start_byte)
            - i64::from(self.convergence.base_line_start_byte);
        let utf16_delta = i64::from(self.convergence.target_line_start_utf16)
            - i64::from(self.convergence.base_line_start_utf16);
        let paragraph_content_start = terminal.paragraph_content_start;
        let selection = self
            .plan
            .as_ref()
            .map(M11OrdinaryParagraphBofCropPlan::selection)
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let (output, block_ordinal_delta, target_top_level_block_count) = if selection
            .is_segmented_top_level()
        {
            let paragraph_content_offset = self
                .convergence
                .base_paragraph_content_start
                .checked_sub(self.convergence.base_paragraph_start_byte)
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let expected_target_content_start = self
                .convergence
                .target_paragraph_start_byte
                .checked_add(paragraph_content_offset)
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let base_line_offset_bytes = self
                .convergence
                .base_line_start_byte
                .checked_sub(self.convergence.base_paragraph_start_byte)
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let target_line_offset_bytes = self
                .convergence
                .target_line_start_byte
                .checked_sub(self.convergence.target_paragraph_start_byte)
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let base_line_offset_utf16 = self
                .convergence
                .base_line_start_utf16
                .checked_sub(self.convergence.base_paragraph_start_utf16)
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let target_line_offset_utf16 = self
                .convergence
                .target_line_start_utf16
                .checked_sub(self.convergence.target_paragraph_start_utf16)
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            if terminal.paragraph_source_start_byte != self.convergence.target_paragraph_start_byte
                || terminal.paragraph_source_start_utf16
                    != self.convergence.target_paragraph_start_utf16
                || terminal.paragraph_content_start != expected_target_content_start
                || terminal.replacement_start_byte != 0
                || terminal.replacement_start_utf16 != 0
                || self.convergence.base_block_entry_ordinal
                    != selection.convergence_block_entry_ordinal()
                || base_line_offset_bytes != target_line_offset_bytes
                || base_line_offset_utf16 != target_line_offset_utf16
            {
                return Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch);
            }
            let replacement_count = u64::try_from(terminal.replacement_leaves.len())
                .map_err(|_| M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let removed_count = selection.convergence_block_entry_ordinal();
            let block_ordinal_delta = i64::try_from(replacement_count)
                .ok()
                .and_then(|replacement| {
                    i64::try_from(removed_count)
                        .ok()
                        .and_then(|removed| replacement.checked_sub(removed))
                })
                .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let block_splice = M11BlockSequenceSpliceSelection::new(
                0..selection.convergence_block_entry_ordinal(),
                0..replacement_count,
            )
            .map_err(|_| M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            let target_top_level_block_count = shift_top_level_block_count(
                selection.base_block_entry_count(),
                block_ordinal_delta,
            )
            .filter(|count| *count != 0)
            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
            (
                M11OrdinaryParagraphCropOutput::Segmented {
                    replacement_leaves: std::mem::take(&mut terminal.replacement_leaves),
                    block_splice,
                },
                block_ordinal_delta,
                target_top_level_block_count,
            )
        } else {
            if terminal.paragraph_source_start_byte != 0
                || terminal.paragraph_source_start_utf16 != 0
                || terminal.replacement_start_byte != 0
                || terminal.replacement_start_utf16 != 0
                || !terminal.replacement_leaves.is_empty()
            {
                return Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged);
            }
            (
                M11OrdinaryParagraphCropOutput::Whole(
                    M11CleanDocumentResult::from_ordinary_paragraph_crop(
                        self.source,
                        paragraph_content_start,
                    )
                    .map_err(M11CleanParseJobError::Finish)
                    .map_err(M11OrdinaryParagraphBoundaryCropError::Parse)?,
                ),
                0,
                1,
            )
        };
        let fresh = terminal.take_fresh_checkpoint_cursor(self.binding);
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let base_checkpoints = M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(
            selection.source(),
            selection.binding(),
            plan.into_base_checkpoints(),
            selection.base_block_entry_count(),
        );
        let merge = if selection.is_segmented_top_level() {
            OrderedCheckpointMerge::from_segmented_bof(
                self.source,
                self.binding,
                base_checkpoints,
                fresh,
                selection.convergence_index(),
                byte_delta,
                utf16_delta,
                ordinal_delta,
                block_ordinal_delta,
                target_top_level_block_count,
            )
        } else {
            OrderedCheckpointMerge::from_bof(
                self.source,
                self.binding,
                base_checkpoints,
                fresh,
                selection.convergence_index(),
                paragraph_content_start,
                byte_delta,
                utf16_delta,
                ordinal_delta,
            )
        }
        .map_err(map_boundary_checkpoint_merge_error)?;
        self.checkpoint_merge = Some(merge);
        self.merge_completion = Some(OrdinaryParagraphBoundaryCropMergeCompletion {
            output,
            target_source,
            parse_work,
            target_crop_bytes: 0..self.convergence.target_line_end_byte as usize,
            convergence_ordinal_delta: Some(ordinal_delta),
        });
        Ok(())
    }
}

/// Authenticated restart-to-EOF parse requiring no suffix witness.
pub struct M11OrdinaryParagraphEofCropParseJob {
    parse: Option<M11CleanParseJob>,
    plan: Option<M11OrdinaryParagraphEofCropPlan>,
    target_restart: Option<M11OrdinaryParagraphRestartCheckpoint>,
    checkpoint_merge: Option<OrderedCheckpointMerge>,
    merge_completion: Option<OrdinaryParagraphBoundaryCropMergeCompletion>,
    source: SourceVersion,
    binding: M11ParserBinding,
    crop_start: usize,
    paragraph_content_start: u32,
}

impl M11OrdinaryParagraphEofCropParseJob {
    pub fn new(
        mut plan: M11OrdinaryParagraphEofCropPlan,
        prefix: ExactUnchangedPrefixWitness,
        target: SourceSnapshotLease,
        binding: M11ParserBinding,
    ) -> Result<Self, M11OrdinaryParagraphBoundaryCropError> {
        if binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(
                M11OrdinaryParagraphBoundaryCropError::UnsupportedGrammarRevision {
                    actual: binding.grammar_revision(),
                },
            );
        }
        let selection = plan.selection();
        if selection.binding() != binding {
            return Err(M11OrdinaryParagraphBoundaryCropError::BindingMismatch);
        }
        if selection.source() != prefix.base() || target.version() != prefix.target() {
            return Err(M11OrdinaryParagraphBoundaryCropError::AuthorityMismatch);
        }
        let restart = plan
            .take_restart()
            .map_err(M11OrdinaryParagraphBoundaryCropError::Plan)?;
        if !restart.metrics_are_consistent()
            || restart.source() != selection.source()
            || restart.binding() != binding
            || restart.prefix_end_byte() != selection.restart_prefix_end_byte()
            || restart.prefix_end_utf16() != selection.restart_prefix_end_utf16()
        {
            return Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch);
        }
        let source = target.version();
        let crop_start = restart.prefix_end_byte() as usize;
        let paragraph_content_start = restart.paragraph_content_start();
        let target_restart = restart.for_target(source);
        let mut parse = M11CleanParseJob::new_for_ordinary_paragraph_remainder(
            restart, prefix, target, binding,
        )
        .map_err(M11OrdinaryParagraphBoundaryCropError::Restart)?;
        if selection.is_segmented_top_level() {
            parse.finish = Some(CleanParseFinish::OrdinaryParagraphEofCrop);
        }
        Ok(Self {
            parse: Some(parse),
            plan: Some(plan),
            target_restart: Some(target_restart),
            checkpoint_merge: None,
            merge_completion: None,
            source,
            binding,
            crop_start,
            paragraph_content_start,
        })
    }

    pub fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<M11OrdinaryParagraphBoundaryCropPoll, M11OrdinaryParagraphBoundaryCropError> {
        if fuel == 0 {
            return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions: 0 });
        }
        let mut transitions = 0;
        loop {
            if let Some(merge) = self.checkpoint_merge.as_mut() {
                let merge_poll = merge
                    .poll(fuel - transitions)
                    .map_err(map_boundary_checkpoint_merge_error)?;
                match merge_poll {
                    OrderedCheckpointMergePoll::Pending {
                        transitions: consumed,
                    } => {
                        transitions = transitions
                            .checked_add(consumed)
                            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                        return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions });
                    }
                    OrderedCheckpointMergePoll::Complete {
                        transitions: consumed,
                        result: merged,
                    } => {
                        transitions = transitions
                            .checked_add(consumed)
                            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                        self.checkpoint_merge.take();
                        let completion = self
                            .merge_completion
                            .take()
                            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                        return Ok(M11OrdinaryParagraphBoundaryCropPoll::Complete {
                            transitions,
                            result: finish_boundary_crop_merge(completion, merged),
                        });
                    }
                }
            }
            if transitions == fuel {
                return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions });
            }
            let poll = {
                let parse = self
                    .parse
                    .as_mut()
                    .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                parse.poll_internal(fuel - transitions)
            };
            let poll = match poll {
                Ok(poll) => poll,
                Err(
                    M11CleanParseJobError::Controller(M11CleanControllerError::Controller(
                        M11CleanControllerFault::OrdinaryParagraphCropDiverged,
                    ))
                    | M11CleanParseJobError::Finish(
                        M11CleanControllerFault::OrdinaryParagraphCropDiverged,
                    ),
                ) => {
                    self.parse.take();
                    return Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged);
                }
                Err(error) => {
                    self.parse.take();
                    return Err(M11OrdinaryParagraphBoundaryCropError::Parse(error));
                }
            };
            match poll {
                CleanParseInternalPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                    return Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions });
                }
                CleanParseInternalPoll::Complete {
                    transitions: consumed,
                    terminal: CleanParseTerminal::WholeSource(result),
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                    let mut parse = self
                        .parse
                        .take()
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                    let target_source = parse
                        .take_completed_source()
                        .filter(|lease| lease.version() == self.source)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::AuthorityMismatch)?;
                    self.begin_checkpoint_merge(result, parse.work, target_source)?;
                }
                CleanParseInternalPoll::Complete {
                    transitions: consumed,
                    terminal: CleanParseTerminal::OrdinaryParagraphEofCrop(terminal),
                } => {
                    transitions = transitions
                        .checked_add(consumed)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
                    let mut parse = self
                        .parse
                        .take()
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
                    let target_source = parse
                        .take_completed_source()
                        .filter(|lease| lease.version() == self.source)
                        .ok_or(M11OrdinaryParagraphBoundaryCropError::AuthorityMismatch)?;
                    self.begin_segmented_checkpoint_merge(terminal, parse.work, target_source)?;
                }
                CleanParseInternalPoll::Complete {
                    terminal: CleanParseTerminal::OrdinaryParagraphCrop(_),
                    ..
                } => unreachable!("EOF crop has its own typed finish"),
            }
        }
    }

    /// Cancels the bounded target parse and reconstructs the original
    /// move-only base collection without copying the document-sized vector.
    pub fn cancel_into_base_restart_checkpoints(
        mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphBoundaryCropError> {
        if let Some(merge) = self.checkpoint_merge.take() {
            drop(self.parse.take());
            self.merge_completion.take();
            return merge
                .cancel_into_base()
                .map_err(map_boundary_checkpoint_merge_error);
        }
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let target_restart = self
            .target_restart
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let base_restart = target_restart.for_target(plan.selection().source());
        drop(self.parse.take());
        plan.restore_base_checkpoints(base_restart)
            .map_err(M11OrdinaryParagraphBoundaryCropError::Plan)
    }

    fn begin_checkpoint_merge(
        &mut self,
        mut result: M11CleanDocumentResult,
        parse_work: CleanParseWork,
        target_source: SourceSnapshotLease,
    ) -> Result<(), M11OrdinaryParagraphBoundaryCropError> {
        if result.source_version() != self.source {
            return Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch);
        }
        if result.kind() != crate::M11CleanDocumentKind::Paragraph
            || result.definition_count() != 0
            || result.visible_source().as_ref().map(|range| range.start)
                != Some(self.paragraph_content_start)
        {
            return Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged);
        }
        let fresh = result
            .take_ordinary_paragraph_checkpoint_seed_cursor(self.binding)
            .map_err(|error| match error {
                M11OrdinaryParagraphCheckpointError::AllocationFailed => {
                    M11OrdinaryParagraphBoundaryCropError::CheckpointAllocationFailed
                }
                M11OrdinaryParagraphCheckpointError::Ineligible
                | M11OrdinaryParagraphCheckpointError::AlreadyTaken => {
                    M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch
                }
            })?;
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let selection = plan.selection();
        let target_restart = self
            .target_restart
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let base_restart = target_restart.for_target(selection.source());
        let base_checkpoints = plan
            .restore_base_checkpoints(base_restart)
            .map_err(M11OrdinaryParagraphBoundaryCropError::Plan)?;
        let merge = OrderedCheckpointMerge::to_eof(
            self.source,
            self.binding,
            base_checkpoints,
            target_restart,
            fresh,
            selection.restart_index(),
            1,
        )
        .map_err(map_boundary_checkpoint_merge_error)?;
        self.checkpoint_merge = Some(merge);
        self.merge_completion = Some(OrdinaryParagraphBoundaryCropMergeCompletion {
            output: M11OrdinaryParagraphCropOutput::Whole(result),
            parse_work,
            target_source,
            target_crop_bytes: self.crop_start..self.source.byte_len(),
            convergence_ordinal_delta: None,
        });
        Ok(())
    }

    fn begin_segmented_checkpoint_merge(
        &mut self,
        mut terminal: M11OrdinaryParagraphEofCropTerminal,
        parse_work: CleanParseWork,
        target_source: SourceSnapshotLease,
    ) -> Result<(), M11OrdinaryParagraphBoundaryCropError> {
        if terminal.source != self.source {
            return Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch);
        }
        let selection = self
            .plan
            .as_ref()
            .map(M11OrdinaryParagraphEofCropPlan::selection)
            .filter(M11OrdinaryParagraphEofCropSelection::is_segmented_top_level)
            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
        let target_restart = self
            .target_restart
            .as_ref()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        if terminal.replacement_start_byte != target_restart.paragraph_source_start_byte()
            || terminal.replacement_start_utf16 != target_restart.paragraph_source_start_utf16()
            || target_restart.block_entry_ordinal() != selection.restart_block_entry_ordinal()
            || selection.restart_block_entry_ordinal() >= selection.base_block_entry_count()
        {
            return Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch);
        }
        let replacement_count = u64::try_from(terminal.replacement_leaves.len())
            .map_err(|_| M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
        let target_end = selection
            .restart_block_entry_ordinal()
            .checked_add(replacement_count)
            .filter(|count| *count != 0)
            .ok_or(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
        let block_splice = M11BlockSequenceSpliceSelection::new(
            selection.restart_block_entry_ordinal()..selection.base_block_entry_count(),
            selection.restart_block_entry_ordinal()..target_end,
        )
        .map_err(|_| M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)?;
        let output = M11OrdinaryParagraphCropOutput::Segmented {
            replacement_leaves: std::mem::take(&mut terminal.replacement_leaves),
            block_splice,
        };
        let fresh = terminal.take_fresh_checkpoint_cursor(self.binding);
        let plan = self
            .plan
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let target_restart = self
            .target_restart
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropError::Complete)?;
        let base_restart = target_restart.for_target(selection.source());
        let base_checkpoints = plan
            .restore_base_checkpoints(base_restart)
            .map_err(M11OrdinaryParagraphBoundaryCropError::Plan)?;
        let merge = OrderedCheckpointMerge::to_eof(
            self.source,
            self.binding,
            base_checkpoints,
            target_restart,
            fresh,
            selection.restart_index(),
            target_end,
        )
        .map_err(map_boundary_checkpoint_merge_error)?;
        self.checkpoint_merge = Some(merge);
        self.merge_completion = Some(OrdinaryParagraphBoundaryCropMergeCompletion {
            output,
            parse_work,
            target_source,
            target_crop_bytes: self.crop_start..self.source.byte_len(),
            convergence_ordinal_delta: None,
        });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11LeadingReferencesCropWork {
    prefix_source_bytes_scanned: usize,
    crop_source_bytes_discovered: usize,
    crop_source_bytes_read: usize,
    reused_definitions: usize,
    definitions_enumerated: usize,
    definitions_cooked: usize,
}

impl M11LeadingReferencesCropWork {
    #[must_use]
    pub const fn prefix_source_bytes_scanned(self) -> usize {
        self.prefix_source_bytes_scanned
    }

    #[must_use]
    pub const fn crop_source_bytes_discovered(self) -> usize {
        self.crop_source_bytes_discovered
    }

    #[must_use]
    pub const fn crop_source_bytes_read(self) -> usize {
        self.crop_source_bytes_read
    }

    #[must_use]
    pub const fn reused_definitions(self) -> usize {
        self.reused_definitions
    }

    #[must_use]
    pub const fn definitions_enumerated(self) -> usize {
        self.definitions_enumerated
    }

    #[must_use]
    pub const fn definitions_cooked(self) -> usize {
        self.definitions_cooked
    }
}

pub struct M11LeadingReferencesCropResult {
    terminal: M11CleanDocumentResult,
    facts: M11ParserTerminalFacts,
    work: M11LeadingReferencesCropWork,
    base_restart: Option<crate::LeadingReferencesRestartCheckpoint>,
    next_restart: Option<crate::LeadingReferencesRestartCheckpoint>,
    target_source: Option<SourceSnapshotLease>,
}

impl M11LeadingReferencesCropResult {
    #[must_use]
    pub const fn terminal(&self) -> &M11CleanDocumentResult {
        &self.terminal
    }

    #[must_use]
    pub const fn facts(&self) -> &M11ParserTerminalFacts {
        &self.facts
    }

    #[must_use]
    pub const fn work(&self) -> M11LeadingReferencesCropWork {
        self.work
    }

    /// Takes the move-only base checkpoint retained until the target commits.
    pub fn take_base_restart_checkpoint(
        &mut self,
    ) -> Result<crate::LeadingReferencesRestartCheckpoint, crate::LeadingReferencesCheckpointError>
    {
        self.base_restart
            .take()
            .ok_or(crate::LeadingReferencesCheckpointError::AlreadyTaken)
    }

    /// Takes the move-only restart checkpoint for the next tail edit.
    pub fn take_next_restart_checkpoint(
        &mut self,
    ) -> Result<crate::LeadingReferencesRestartCheckpoint, crate::LeadingReferencesCheckpointError>
    {
        self.next_restart
            .take()
            .ok_or(crate::LeadingReferencesCheckpointError::AlreadyTaken)
    }

    /// Takes the exact target lease consumed by this crop exactly once.
    #[must_use]
    pub fn take_target_source_lease(&mut self) -> Option<SourceSnapshotLease> {
        self.target_source.take()
    }

    /// Consumes this authenticated leading-reference crop into the only parser
    /// input that may retain exact-base References while rebuilding target
    /// blocks.
    pub fn into_exact_segmented_candidate_input(
        mut self,
    ) -> Result<M11ExactSegmentedCandidateInput, M11CandidateDerivationError> {
        let source_lease = self
            .target_source
            .take()
            .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
        M11ExactSegmentedCandidateInput::from_crop(source_lease, self.terminal)
    }

    #[must_use]
    pub fn into_terminal(self) -> M11CleanDocumentResult {
        self.terminal
    }
}

// The large terminal is produced once and transfers move-only source and
// restart authority through this public poll API. Boxing would add a
// completion allocation and change the public `Complete` field solely to
// shrink Pending.
#[allow(clippy::large_enum_variant)]
pub enum M11LeadingReferencesCropPoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: M11LeadingReferencesCropResult,
    },
}

#[derive(Debug)]
pub enum M11LeadingReferencesCropError {
    Complete,
    AuthorityMismatch,
    BindingMismatch,
    UnsupportedGrammarRevision { actual: u32 },
    CutMismatch,
    CropAcceptedDefinition,
    Unknown(M11UnknownReason),
    TerminalMismatch,
    Source(SourceAdapterError),
    Parse(M11CleanParseJobError),
    Derivation(M11CandidateDerivationError),
}

impl fmt::Display for M11LeadingReferencesCropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => formatter.write_str("leading-reference crop is already complete"),
            Self::AuthorityMismatch => {
                formatter.write_str("leading-reference crop crossed source authority")
            }
            Self::BindingMismatch => {
                formatter.write_str("leading-reference crop crossed parser binding")
            }
            Self::UnsupportedGrammarRevision { actual } => {
                write!(formatter, "unsupported crop grammar revision {actual}")
            }
            Self::CutMismatch => {
                formatter.write_str("leading-reference crop cut does not match its witness")
            }
            Self::CropAcceptedDefinition => {
                formatter.write_str("crop accepted a new definition after its reused prefix")
            }
            Self::Unknown(reason) => write!(formatter, "crop reached Unknown: {reason:?}"),
            Self::TerminalMismatch => {
                formatter.write_str("crop could not reproduce an admitted exact terminal")
            }
            Self::Source(error) => error.fmt(formatter),
            Self::Parse(error) => error.fmt(formatter),
            Self::Derivation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11LeadingReferencesCropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Derivation(error) => Some(error),
            _ => None,
        }
    }
}

/// Exact tail parse seeded by one clean leading-reference checkpoint.
pub struct M11LeadingReferencesCropParseJob {
    parse: Option<M11CleanParseJob>,
    base_source: SourceVersion,
    source: SourceVersion,
    crop_start: usize,
    definition_count: usize,
    next_restart: Option<crate::LeadingReferencesRestartCheckpoint>,
}

impl M11LeadingReferencesCropParseJob {
    /// Consumes parser and lineage authority and starts at the exact cut.
    pub fn new(
        checkpoint: crate::LeadingReferencesRestartCheckpoint,
        witness: ExactUnchangedPrefixWitness,
        target: SourceSnapshotLease,
        binding: M11ParserBinding,
    ) -> Result<Self, M11LeadingReferencesCropError> {
        if binding.grammar_revision() != crate::M11_GRAMMAR_REVISION {
            return Err(M11LeadingReferencesCropError::UnsupportedGrammarRevision {
                actual: binding.grammar_revision(),
            });
        }
        if checkpoint.binding() != binding {
            return Err(M11LeadingReferencesCropError::BindingMismatch);
        }
        if checkpoint.source() != witness.base() || target.version() != witness.target() {
            return Err(M11LeadingReferencesCropError::AuthorityMismatch);
        }
        let crop_start = usize::try_from(checkpoint.prefix_end_byte())
            .map_err(|_| M11LeadingReferencesCropError::CutMismatch)?;
        let prefix_utf16 = usize::try_from(checkpoint.prefix_end_utf16())
            .map_err(|_| M11LeadingReferencesCropError::CutMismatch)?;
        if witness.byte_end() != crop_start
            || witness.utf16_end() != prefix_utf16
            || crop_start > witness.target().byte_len()
            || prefix_utf16 > witness.target().utf16_len()
            || usize::try_from(checkpoint.paragraph_content_start())
                .ok()
                .is_none_or(|start| start > crop_start)
            || checkpoint.definition_count() == 0
            || checkpoint.next_physical_line_ordinal() == 0
        {
            return Err(M11LeadingReferencesCropError::CutMismatch);
        }
        let observed_utf16 = target
            .utf16_offset_for_byte(crop_start)
            .map_err(SourceAdapterError::from)
            .map_err(M11LeadingReferencesCropError::Source)?;
        if observed_utf16 != prefix_utf16 {
            return Err(M11LeadingReferencesCropError::CutMismatch);
        }

        let source = target.version();
        let base_source = checkpoint.source();
        let definition_count = checkpoint.definition_count();
        let next_restart = Some(checkpoint.for_target(source));
        let (scanner, completed_source) = if crop_start == source.byte_len() {
            (None, Some(target))
        } else {
            (
                Some(
                    SnapshotLineScanner::new_at(
                        target,
                        crop_start,
                        checkpoint.next_physical_line_ordinal(),
                    )
                    .map_err(M11LeadingReferencesCropError::Source)?,
                ),
                None,
            )
        };
        let controller =
            M11CleanBlockController::new_for_leading_references_remainder(source, checkpoint);
        Ok(Self {
            parse: Some(M11CleanParseJob {
                scanner,
                completed_source,
                controller: Some(controller),
                pending_line: None,
                active: None,
                work: CleanParseWork::default(),
                finish: Some(CleanParseFinish::WholeSource),
            }),
            base_source,
            source,
            crop_start,
            definition_count,
            next_restart,
        })
    }

    /// Advances only the target tail within caller fuel.
    pub fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<M11LeadingReferencesCropPoll, M11LeadingReferencesCropError> {
        let poll = {
            let parse = self
                .parse
                .as_mut()
                .ok_or(M11LeadingReferencesCropError::Complete)?;
            parse.poll(fuel)
        };
        let poll = match poll {
            Ok(poll) => poll,
            Err(
                M11CleanParseJobError::Controller(M11CleanControllerError::Controller(
                    M11CleanControllerFault::CropAcceptedDefinition,
                ))
                | M11CleanParseJobError::Finish(M11CleanControllerFault::CropAcceptedDefinition),
            ) => {
                self.parse.take();
                return Err(M11LeadingReferencesCropError::CropAcceptedDefinition);
            }
            Err(error) => {
                self.parse.take();
                return Err(M11LeadingReferencesCropError::Parse(error));
            }
        };
        match poll {
            M11CleanParsePoll::Pending { transitions } => {
                Ok(M11LeadingReferencesCropPoll::Pending { transitions })
            }
            M11CleanParsePoll::Complete {
                transitions,
                result,
            } => {
                let mut parse = self
                    .parse
                    .take()
                    .ok_or(M11LeadingReferencesCropError::Complete)?;
                let target_source = parse
                    .take_completed_source()
                    .filter(|lease| lease.version() == self.source)
                    .ok_or(M11LeadingReferencesCropError::AuthorityMismatch)?;
                if result.source_version() != self.source {
                    return Err(M11LeadingReferencesCropError::TerminalMismatch);
                }
                if let M11CleanDocumentOutcome::Unknown { reason } = result.outcome() {
                    return Err(M11LeadingReferencesCropError::Unknown(*reason));
                }
                if result.definition_count() != self.definition_count
                    || !result.reuses_leading_references()
                {
                    return Err(M11LeadingReferencesCropError::TerminalMismatch);
                }
                match result.outcome() {
                    M11CleanDocumentOutcome::Empty { .. }
                        if self.crop_start != self.source.byte_len() =>
                    {
                        return Err(M11LeadingReferencesCropError::TerminalMismatch);
                    }
                    M11CleanDocumentOutcome::Paragraph { visible_source, .. }
                        if usize::try_from(visible_source.start)
                            .ok()
                            .is_none_or(|start| start < self.crop_start)
                            || usize::try_from(visible_source.end).ok()
                                != Some(self.source.byte_len()) =>
                    {
                        return Err(M11LeadingReferencesCropError::TerminalMismatch);
                    }
                    M11CleanDocumentOutcome::Empty { .. }
                    | M11CleanDocumentOutcome::Paragraph { .. } => {}
                    M11CleanDocumentOutcome::Segmented { .. } => {
                        return Err(M11LeadingReferencesCropError::TerminalMismatch);
                    }
                    M11CleanDocumentOutcome::Unknown { .. } => {
                        unreachable!("Unknown crop terminals returned before reuse validation")
                    }
                }
                let facts = M11ParserTerminalFacts::derive(&result)
                    .map_err(M11LeadingReferencesCropError::Derivation)?;
                let base_restart = self
                    .next_restart
                    .as_ref()
                    .map(|checkpoint| checkpoint.for_target(self.base_source));
                Ok(M11LeadingReferencesCropPoll::Complete {
                    transitions,
                    result: M11LeadingReferencesCropResult {
                        terminal: result,
                        facts,
                        work: M11LeadingReferencesCropWork {
                            prefix_source_bytes_scanned: 0,
                            crop_source_bytes_discovered: parse.work.source_bytes_discovered,
                            crop_source_bytes_read: parse.work.source_bytes_read,
                            reused_definitions: self.definition_count,
                            definitions_enumerated: 0,
                            definitions_cooked: 0,
                        },
                        base_restart,
                        next_restart: self.next_restart.take(),
                        target_source: Some(target_source),
                    },
                })
            }
        }
    }

    /// Cancels the target parse and hands the original base checkpoint back.
    pub fn cancel_into_base_restart_checkpoint(
        mut self,
    ) -> Result<crate::LeadingReferencesRestartCheckpoint, M11LeadingReferencesCropError> {
        let next_restart = self
            .next_restart
            .take()
            .ok_or(M11LeadingReferencesCropError::Complete)?;
        drop(self.parse.take());
        Ok(next_restart.for_target(self.base_source))
    }
}

pub(crate) fn encode_inline_projection_metadata(
    disposition: u8,
    profile_partition: u32,
    fact_count: usize,
    source_range: &std::ops::Range<u32>,
) -> Result<Box<[u8]>, M11InlinePublicationError> {
    let mut metadata = Vec::new();
    metadata
        .try_reserve_exact(M11_INLINE_META_RECORD_BYTES)
        .map_err(|_| M11InlinePublicationError::AllocationFailed)?;
    metadata.extend_from_slice(M11_INLINE_META_MAGIC);
    push_u32(&mut metadata, M11_INLINE_SCHEMA);
    metadata.push(disposition);
    metadata.extend_from_slice(&[0; 3]);
    push_u32(&mut metadata, profile_partition);
    push_u32(
        &mut metadata,
        u32::try_from(fact_count).map_err(|_| M11InlinePublicationError::CoordinateOverflow)?,
    );
    push_u64(&mut metadata, u64::from(source_range.start));
    push_u64(&mut metadata, u64::from(source_range.end));
    push_u32(
        &mut metadata,
        u32::try_from(M11_INLINE_FACT_RECORD_BYTES)
            .map_err(|_| M11InlinePublicationError::CoordinateOverflow)?,
    );
    push_u32(&mut metadata, 0);
    debug_assert_eq!(metadata.len(), M11_INLINE_META_RECORD_BYTES);
    Ok(metadata.into_boxed_slice())
}

fn encode_block_sequence_entry(
    leaf: M11CleanLeaf,
) -> Result<M11BlockSequenceEntry, M11CandidateDerivationError> {
    match leaf {
        M11CleanLeaf::Paragraph {
            source,
            source_utf16,
            inline_source,
            reference_definition_count,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16 = range_len(&source_utf16)?;
            if inline_source.start < source.start
                || inline_source.end > source.end
                || inline_source.start >= inline_source.end
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
            let inline_relative =
                inline_source.start - source.start..inline_source.end - source.start;
            let green = encode_block_paragraph_green(
                u32::try_from(source_bytes)
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
                inline_relative.clone(),
                reference_definition_count,
            )?;
            let projection = encode_block_paragraph_projection(
                u32::try_from(source_bytes)
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
                inline_relative,
            )?;
            M11BlockSequenceEntry::paragraph(
                source_bytes,
                source_utf16,
                u64::try_from(reference_definition_count)
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
                green,
                projection,
            )
            .map_err(Into::into)
        }
        M11CleanLeaf::FencedCode {
            source,
            source_utf16,
            opening_marker,
            raw_info_source,
            body_source,
            closing_marker,
            marker,
            opening_indent,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16 = range_len(&source_utf16)?;
            if !matches!(marker, b'`' | b'~')
                || opening_indent > 3
                || opening_marker.start < source.start
                || opening_marker.start >= opening_marker.end
                || opening_marker.end - opening_marker.start < 3
                || opening_marker.end > source.end
                || raw_info_source.start != opening_marker.end
                || raw_info_source.start > raw_info_source.end
                || raw_info_source.end > body_source.start
                || body_source.start > body_source.end
                || body_source.end > source.end
                || body_source.start - raw_info_source.end > 2
                || closing_marker.as_ref().is_some_and(|closing| {
                    closing.start < body_source.end
                        || closing.start - body_source.end > 3
                        || closing.start >= closing.end
                        || closing.end > source.end
                        || closing.end - closing.start < opening_marker.end - opening_marker.start
                })
                || closing_marker.is_none() && body_source.end != source.end
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
            let relative =
                |range: std::ops::Range<u32>| range.start - source.start..range.end - source.start;
            let opening_marker = relative(opening_marker);
            let raw_info_source = relative(raw_info_source);
            let body_source = relative(body_source);
            let closing_marker = closing_marker.map(relative);
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_fenced_code_green(
                source_bytes_u32,
                body_source.clone(),
                opening_marker,
                raw_info_source,
                closing_marker,
                marker,
                opening_indent,
            )?;
            let projection = encode_block_fenced_code_projection(source_bytes_u32, body_source)?;
            M11BlockSequenceEntry::structured(source_bytes, source_utf16, 0, green, projection)
                .map_err(Into::into)
        }
        M11CleanLeaf::IndentedCode {
            source,
            source_utf16,
            line_count,
            projected_utf8_length,
            projected_utf16_length,
            terminal_eol_bytes,
            has_bof_bom,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16 = range_len(&source_utf16)?;
            if line_count == 0
                || projected_utf8_length == 0
                || projected_utf16_length == 0
                || usize::try_from(projected_utf8_length)
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
                    >= source_bytes
                || usize::try_from(projected_utf16_length)
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
                    >= source_utf16
                || terminal_eol_bytes > 2
                || has_bof_bom && source.start != 0
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_indented_code_green(
                source_bytes_u32,
                line_count,
                projected_utf8_length,
                projected_utf16_length,
                terminal_eol_bytes,
                has_bof_bom,
            )?;
            let projection = encode_block_indented_code_projection(source_bytes_u32, line_count)?;
            M11BlockSequenceEntry::structured(source_bytes, source_utf16, 0, green, projection)
                .map_err(Into::into)
        }
        M11CleanLeaf::BlockQuote {
            source,
            source_utf16,
            lines,
            child_paragraph,
            disposition,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16_units = range_len(&source_utf16)?;
            match disposition {
                M11BlockQuoteDisposition::ExactSingleParagraph => {
                    let child =
                        child_paragraph.ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
                    validate_exact_block_quote(&source, &source_utf16, &lines, &child)?;
                    let source_bytes_u32 = u32::try_from(source_bytes)
                        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
                    let line_count = u32::try_from(lines.len())
                        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
                    let green = encode_block_quote_green(source_bytes_u32, line_count, &child)?;
                    let projection = encode_block_quote_projection(source_bytes_u32, line_count)?;
                    M11BlockSequenceEntry::structured(
                        source_bytes,
                        source_utf16_units,
                        0,
                        green,
                        projection,
                    )
                    .map_err(Into::into)
                }
                M11BlockQuoteDisposition::Unsupported(reason) => {
                    if child_paragraph.is_some() || lines.is_empty() {
                        return Err(M11CandidateDerivationError::ResultRangeMismatch);
                    }
                    M11BlockSequenceEntry::unsupported(
                        source_bytes,
                        source_utf16_units,
                        M11BlockUnsupportedReason::new(encode_block_quote_unsupported_reason(
                            reason,
                        ))?,
                    )
                    .map_err(Into::into)
                }
            }
        }
        M11CleanLeaf::BulletList {
            source,
            source_utf16,
            marker,
            items,
            projected_utf8_length,
            projected_utf16_length,
            tight,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16_units = range_len(&source_utf16)?;
            let (terminal_empty_relative_start, paragraph_count) = validate_exact_bullet_list(
                &source,
                &source_utf16,
                marker,
                &items,
                projected_utf8_length,
                projected_utf16_length,
                tight,
            )?;
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let item_count = u32::try_from(items.len())
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_bullet_list_green(
                source_bytes_u32,
                marker,
                item_count,
                terminal_empty_relative_start,
                paragraph_count,
                projected_utf8_length,
                projected_utf16_length,
            )?;
            let projection = encode_block_bullet_list_projection(source_bytes_u32, item_count)?;
            M11BlockSequenceEntry::structured(
                source_bytes,
                source_utf16_units,
                0,
                green,
                projection,
            )
            .map_err(Into::into)
        }
        M11CleanLeaf::OrderedList {
            source,
            source_utf16,
            start,
            delimiter,
            items,
            projected_utf8_length,
            projected_utf16_length,
            tight,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16_units = range_len(&source_utf16)?;
            let (terminal_empty_relative_start, paragraph_count) = validate_exact_ordered_list(
                &source,
                &source_utf16,
                start,
                delimiter,
                &items,
                projected_utf8_length,
                projected_utf16_length,
                tight,
            )?;
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let item_count = u32::try_from(items.len())
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_ordered_list_green(
                source_bytes_u32,
                start,
                delimiter,
                item_count,
                terminal_empty_relative_start,
                paragraph_count,
                projected_utf8_length,
                projected_utf16_length,
            )?;
            let projection = encode_block_ordered_list_projection(source_bytes_u32, item_count)?;
            M11BlockSequenceEntry::structured(
                source_bytes,
                source_utf16_units,
                0,
                green,
                projection,
            )
            .map_err(Into::into)
        }
        M11CleanLeaf::AtxHeading {
            source,
            source_utf16,
            opening_marker,
            inline_source,
            closing_marker,
            line_ending,
            level,
            opening_indent,
            has_bof_bom,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16 = range_len(&source_utf16)?;
            if !(1..=6).contains(&level)
                || opening_indent > 3
                || has_bof_bom && source.start != 0
                || opening_marker.start < source.start
                || opening_marker.start >= opening_marker.end
                || opening_marker.end - opening_marker.start != u32::from(level)
                || opening_marker.start - source.start
                    != u32::from(opening_indent) + if has_bof_bom { 3 } else { 0 }
                || opening_marker.end > inline_source.start
                || inline_source.start > inline_source.end
                || inline_source.end > line_ending.start
                || line_ending.start > line_ending.end
                || line_ending.end != source.end
                || line_ending.end - line_ending.start > 2
                || closing_marker.as_ref().is_some_and(|closing| {
                    inline_source.end > closing.start
                        || closing.start >= closing.end
                        || closing.end > line_ending.start
                })
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
            let relative =
                |range: std::ops::Range<u32>| range.start - source.start..range.end - source.start;
            let opening_marker = relative(opening_marker);
            let inline_source = relative(inline_source);
            let closing_marker = closing_marker.map(relative);
            let line_ending = relative(line_ending);
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_atx_heading_green(
                source_bytes_u32,
                inline_source.clone(),
                opening_marker,
                closing_marker,
                line_ending,
                level,
                opening_indent,
                has_bof_bom,
            )?;
            let projection = encode_block_atx_heading_projection(source_bytes_u32, inline_source)?;
            M11BlockSequenceEntry::structured(source_bytes, source_utf16, 0, green, projection)
                .map_err(Into::into)
        }
        M11CleanLeaf::SetextHeading {
            source,
            source_utf16,
            inline_source,
            underline_marker,
            underline_line_ending,
            level,
            opening_indent,
            reference_definition_count,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16 = range_len(&source_utf16)?;
            if !matches!(level, 1 | 2)
                || opening_indent > 3
                || inline_source.start < source.start
                || inline_source.start >= inline_source.end
                || inline_source.end > underline_marker.start
                || !(u32::from(opening_indent) + 1..=u32::from(opening_indent) + 2)
                    .contains(&(underline_marker.start - inline_source.end))
                || underline_marker.start >= underline_marker.end
                || underline_marker.end > underline_line_ending.start
                || underline_line_ending.start > underline_line_ending.end
                || underline_line_ending.end != source.end
                || underline_line_ending.end - underline_line_ending.start > 2
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
            let relative =
                |range: std::ops::Range<u32>| range.start - source.start..range.end - source.start;
            let inline_source = relative(inline_source);
            let underline_marker = relative(underline_marker);
            let underline_line_ending = relative(underline_line_ending);
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_setext_heading_green(
                source_bytes_u32,
                inline_source.clone(),
                underline_marker,
                underline_line_ending,
                level,
                opening_indent,
                reference_definition_count,
            )?;
            let projection =
                encode_block_setext_heading_projection(source_bytes_u32, inline_source)?;
            M11BlockSequenceEntry::structured(
                source_bytes,
                source_utf16,
                u64::try_from(reference_definition_count)
                    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
                green,
                projection,
            )
            .map_err(Into::into)
        }
        M11CleanLeaf::ThematicBreak {
            source,
            source_utf16,
            marker,
            marker_count,
            marker_envelope,
            line_ending,
            opening_indent,
            has_bof_bom,
        } => {
            let source_bytes = range_len(&source)?;
            let source_utf16 = range_len(&source_utf16)?;
            if !matches!(marker, b'*' | b'-' | b'_')
                || marker_count < 3
                || opening_indent > 3
                || has_bof_bom && source.start != 0
                || marker_envelope.start < source.start
                || marker_envelope.start >= marker_envelope.end
                || marker_envelope.start - source.start
                    != u32::from(opening_indent) + if has_bof_bom { 3 } else { 0 }
                || marker_count > marker_envelope.end - marker_envelope.start
                || marker_envelope.end > line_ending.start
                || line_ending.start > line_ending.end
                || line_ending.end != source.end
                || line_ending.end - line_ending.start > 2
            {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
            let relative =
                |range: std::ops::Range<u32>| range.start - source.start..range.end - source.start;
            let marker_envelope = relative(marker_envelope);
            let line_ending = relative(line_ending);
            let source_bytes_u32 = u32::try_from(source_bytes)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
            let green = encode_block_thematic_break_green(
                source_bytes_u32,
                marker_envelope,
                line_ending,
                marker,
                marker_count,
                opening_indent,
                has_bof_bom,
            )?;
            let projection = encode_block_thematic_break_projection(source_bytes_u32)?;
            M11BlockSequenceEntry::structured(source_bytes, source_utf16, 0, green, projection)
                .map_err(Into::into)
        }
        M11CleanLeaf::Blank {
            source,
            source_utf16,
        } => M11BlockSequenceEntry::blank(range_len(&source)?, range_len(&source_utf16)?)
            .map_err(Into::into),
        M11CleanLeaf::DefinitionsOnly {
            source,
            source_utf16,
            reference_definition_count,
        } => M11BlockSequenceEntry::definitions_only(
            range_len(&source)?,
            range_len(&source_utf16)?,
            u64::try_from(reference_definition_count)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
        )
        .map_err(Into::into),
        M11CleanLeaf::Unsupported {
            source,
            source_utf16,
            reason,
        } => M11BlockSequenceEntry::unsupported(
            range_len(&source)?,
            range_len(&source_utf16)?,
            M11BlockUnsupportedReason::new(encode_block_unsupported_reason(reason))?,
        )
        .map_err(Into::into),
    }
}

fn mint_published_inline_leaf_fence(
    runtime: &DocumentRuntime,
    source: SourceVersion,
    location: M11BlockSequenceLocation,
    syntax_profile: u32,
) -> Result<M11PublishedInlineLeafFenceResolution, M11CandidateDerivationError> {
    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let receipt = location.receipt();
    let kind = location.entry().kind();
    let Some(inline_relative) =
        decode_published_inline_relative(location.entry(), location.byte_range().start)?
    else {
        return Ok(M11PublishedInlineLeafFenceResolution::NotInlineLeaf {
            kind,
            entry_ordinal: location.entry_ordinal(),
            source: block_source,
            source_utf16: block_source_utf16,
            query_receipt: receipt,
        });
    };
    if inline_relative.start == inline_relative.end {
        return Ok(M11PublishedInlineLeafFenceResolution::NotInlineLeaf {
            kind,
            entry_ordinal: location.entry_ordinal(),
            source: block_source,
            source_utf16: block_source_utf16,
            query_receipt: receipt,
        });
    }

    let inline_source = block_source
        .start
        .checked_add(inline_relative.start)
        .ok_or(M11CandidateDerivationError::MetricOverflow)?
        ..block_source
            .start
            .checked_add(inline_relative.end)
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
    if inline_source.start < block_source.start
        || inline_source.end > block_source.end
        || inline_source.start >= inline_source.end
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "inline range escaped its selected block",
            ),
        );
    }

    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let inline_start = usize::try_from(inline_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let inline_end = usize::try_from(inline_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let inline_source_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(inline_start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        ..u32::try_from(
            lease
                .utf16_offset_for_byte(inline_end)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
        )
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if inline_source_utf16.start < block_source_utf16.start
        || inline_source_utf16.end > block_source_utf16.end
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "inline UTF-16 range escaped its selected block",
            ),
        );
    }
    let authority = M11ParserSourceRangeAuthority::new(runtime, lease, inline_start..inline_end)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let parser_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    Ok(M11PublishedInlineLeafFenceResolution::InlineLeaf(
        M11PublishedInlineLeafFence {
            source,
            kind,
            block_source,
            block_source_utf16,
            inline_source,
            inline_source_utf16,
            entry_ordinal: location.entry_ordinal(),
            binding: M11ParserBinding::current(parser_profile),
            query_receipt: receipt,
            authority,
        },
    ))
}

/// Resolves one late caret/viewport point against a sealed retained candidate
/// and mints exact inline authority only when the selected block is an
/// inline-bearing Paragraph, ATX Heading, or Setext Heading with nonempty
/// content.
///
/// Repeated calls do not alter canonical structure. Each call authenticates
/// one logarithmic block path and one bounded packed page through the retained
/// publication capability.
pub fn resolve_m11_published_inline_leaf_fence(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedInlineLeafFenceResolution, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    mint_published_inline_leaf_fence(runtime, source, location, descriptor.syntax_profile)
}

fn mint_published_inline_range_leaf_fence(
    runtime: &DocumentRuntime,
    source: SourceVersion,
    entry: &M11BlockSequenceEntry,
    entry_ordinal: u64,
    block_source: std::ops::Range<u64>,
    block_source_utf16: std::ops::Range<u64>,
    syntax_profile: u32,
) -> Result<Option<M11PublishedInlineRangeLeafFence>, M11CandidateDerivationError> {
    let block_byte_start = block_source.start;
    let block_source = u64_range_to_u32(block_source)?;
    let block_source_utf16 = u64_range_to_u32(block_source_utf16)?;
    let kind = entry.kind();
    let Some(inline_relative) = decode_published_inline_relative(entry, block_byte_start)? else {
        return Ok(None);
    };
    if inline_relative.start == inline_relative.end {
        return Ok(None);
    }
    let inline_source = block_source
        .start
        .checked_add(inline_relative.start)
        .ok_or(M11CandidateDerivationError::MetricOverflow)?
        ..block_source
            .start
            .checked_add(inline_relative.end)
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
    if inline_source.start < block_source.start
        || inline_source.end > block_source.end
        || inline_source.start >= inline_source.end
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "range-visited inline source escaped its structural block",
            ),
        );
    }

    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let inline_start = usize::try_from(inline_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let inline_end = usize::try_from(inline_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let inline_source_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(inline_start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        ..u32::try_from(
            lease
                .utf16_offset_for_byte(inline_end)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
        )
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if inline_source_utf16.start < block_source_utf16.start
        || inline_source_utf16.end > block_source_utf16.end
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "range-visited inline UTF-16 source escaped its structural block",
            ),
        );
    }
    let authority = M11ParserSourceRangeAuthority::new(runtime, lease, inline_start..inline_end)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let parser_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    Ok(Some(M11PublishedInlineRangeLeafFence {
        source,
        kind,
        block_source,
        block_source_utf16,
        inline_source,
        inline_source_utf16,
        entry_ordinal,
        binding: M11ParserBinding::current(parser_profile),
        authority,
    }))
}

/// Resolves one bounded consecutive structural range into exact inline leaf
/// authorities suitable for resumable viewport presentation work.
///
/// It authenticates the retained publication and source once, walks one
/// consecutive structural range, and mints no more than the admitted inline
/// leaf and source-byte limits. The bridge remains responsible for comparing
/// the returned descriptor with the exact structural acknowledgement that
/// authorized its request.
///
/// Inline source bytes are admitted atomically: exceeding the cap rejects the
/// whole batch rather than exposing a continuation advanced past an
/// unprocessed leaf.
pub fn resolve_m11_published_inline_leaf_range(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    start: M11RetainedBlockVisitStart,
    end: M11PublishedInlineRangeEnd,
    limits: M11PublishedInlineRangeLimits,
) -> Result<M11PublishedInlineRangeBatch, M11PublishedInlineRangeError> {
    let descriptor = publication
        .descriptor(runtime)
        .map_err(M11CandidateDerivationError::from)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch.into());
    }
    if end.byte_offset() <= start.byte_offset()
        || end.utf16_offset() <= start.utf16_offset()
        || end.byte_offset() > descriptor.source_bytes
        || end.utf16_offset() > descriptor.source_utf16
    {
        return Err(M11PublishedInlineRangeError::EndCutMismatch {
            expected_byte_offset: end.byte_offset(),
            expected_utf16_offset: end.utf16_offset(),
            actual_byte_offset: start.byte_offset(),
            actual_utf16_offset: start.utf16_offset(),
        });
    }

    let maximum_inline_leaves = usize::try_from(limits.maximum_inline_leaves())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let mut fences = Vec::new();
    fences
        .try_reserve_exact(maximum_inline_leaves)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    let mut total_inline_source_bytes = 0_u64;
    let mut callback_error = None;
    let mut reached_end = false;
    let receipt = publication
        .visit_blocks(
            runtime,
            start,
            limits.maximum_structural_entries(),
            limits.maximum_storage_pages(),
            |visited| {
                let visited_bytes = visited.byte_range();
                let visited_utf16 = visited.utf16_range();
                let byte_end_matches = visited_bytes.end == end.byte_offset();
                let utf16_end_matches = visited_utf16.end == end.utf16_offset();
                if visited_bytes.end > end.byte_offset()
                    || visited_utf16.end > end.utf16_offset()
                    || byte_end_matches != utf16_end_matches
                {
                    callback_error = Some(M11PublishedInlineRangeError::EndCutMismatch {
                        expected_byte_offset: end.byte_offset(),
                        expected_utf16_offset: end.utf16_offset(),
                        actual_byte_offset: visited_bytes.end,
                        actual_utf16_offset: visited_utf16.end,
                    });
                    return M11RetainedBlockVisitControl::Stop;
                }
                let minted = mint_published_inline_range_leaf_fence(
                    runtime,
                    source,
                    visited.entry(),
                    visited.entry_ordinal(),
                    visited_bytes,
                    visited_utf16,
                    descriptor.syntax_profile,
                );
                match minted {
                    Ok(Some(fence)) => {
                        if fences.len() == maximum_inline_leaves {
                            callback_error =
                                Some(M11PublishedInlineRangeError::InlineLeafLimitExceeded {
                                    maximum: limits.maximum_inline_leaves(),
                                    required_through_leaf: u64::try_from(fences.len())
                                        .unwrap_or(u64::MAX)
                                        .saturating_add(1),
                                });
                            return M11RetainedBlockVisitControl::Stop;
                        }
                        let range = fence.inline_source_range();
                        let bytes = u64::from(range.end - range.start);
                        let Some(next_total) = total_inline_source_bytes.checked_add(bytes) else {
                            callback_error = Some(M11PublishedInlineRangeError::Derivation(
                                M11CandidateDerivationError::MetricOverflow,
                            ));
                            return M11RetainedBlockVisitControl::Stop;
                        };
                        if next_total > limits.maximum_inline_source_bytes() {
                            callback_error = Some(
                                M11PublishedInlineRangeError::InlineSourceByteLimitExceeded {
                                    maximum: limits.maximum_inline_source_bytes(),
                                    required_through_leaf: next_total,
                                },
                            );
                            return M11RetainedBlockVisitControl::Stop;
                        }
                        total_inline_source_bytes = next_total;
                        fences.push(fence);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        callback_error = Some(M11PublishedInlineRangeError::Derivation(error));
                        return M11RetainedBlockVisitControl::Stop;
                    }
                }
                if byte_end_matches {
                    reached_end = true;
                    M11RetainedBlockVisitControl::Stop
                } else {
                    M11RetainedBlockVisitControl::Continue
                }
            },
        )
        .map_err(M11CandidateDerivationError::from)?;
    if let Some(error) = callback_error {
        return Err(error);
    }
    if !reached_end {
        return Err(match receipt.disposition() {
            M11RetainedBlockVisitDisposition::EntryLimit => {
                M11PublishedInlineRangeError::StructuralEntryLimitExceeded {
                    maximum: limits.maximum_structural_entries(),
                }
            }
            M11RetainedBlockVisitDisposition::StoragePageLimit => {
                M11PublishedInlineRangeError::StoragePageLimitExceeded {
                    maximum: limits.maximum_storage_pages(),
                }
            }
            M11RetainedBlockVisitDisposition::Complete
            | M11RetainedBlockVisitDisposition::VisitorStopped => {
                M11PublishedInlineRangeError::EndCutMismatch {
                    expected_byte_offset: end.byte_offset(),
                    expected_utf16_offset: end.utf16_offset(),
                    actual_byte_offset: receipt.next_byte_offset(),
                    actual_utf16_offset: receipt.next_utf16_offset(),
                }
            }
        });
    }
    if receipt.next_byte_offset() != end.byte_offset()
        || receipt.next_utf16_offset() != end.utf16_offset()
    {
        return Err(M11PublishedInlineRangeError::EndCutMismatch {
            expected_byte_offset: end.byte_offset(),
            expected_utf16_offset: end.utf16_offset(),
            actual_byte_offset: receipt.next_byte_offset(),
            actual_utf16_offset: receipt.next_utf16_offset(),
        });
    }
    Ok(M11PublishedInlineRangeBatch {
        descriptor,
        limits,
        receipt,
        total_inline_source_bytes,
        fences,
    })
}

#[derive(Clone, Copy)]
struct PublishedIndentedCodeSummary {
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    terminal_eol_bytes: u32,
    has_bof_bom: bool,
}

/// Resolves one exact retained variant-7 block and mints move-only authority
/// for its complete physical source window.
pub fn resolve_m11_published_indented_code_leaf_fence(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedIndentedCodeLeafFence, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    mint_published_indented_code_leaf_fence(runtime, source, location, descriptor.syntax_profile)
}

fn mint_published_indented_code_leaf_fence(
    runtime: &DocumentRuntime,
    source: SourceVersion,
    location: M11BlockSequenceLocation,
    syntax_profile: u32,
) -> Result<M11PublishedIndentedCodeLeafFence, M11CandidateDerivationError> {
    let entry = location.entry();
    let is_indented_code = entry.kind() == M11BlockSequenceEntryKind::Structured
        && entry
            .green()
            .is_some_and(|record| record.as_bytes().get(12) == Some(&INDENTED_CODE_ROLE_VARIANT));
    if !is_indented_code {
        return Err(M11CandidateDerivationError::PublishedIndentedCodeLeafFenceNotIndentedCode);
    }
    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let summary = decode_published_indented_code_summary(&location)?;
    if summary.has_bof_bom && block_source.start != 0 {
        return Err(
            M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
                "BOF BOM metadata belongs to a non-BOF block",
            ),
        );
    }
    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let block_start = usize::try_from(block_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let block_end = usize::try_from(block_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let measured_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(block_start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        ..u32::try_from(
            lease
                .utf16_offset_for_byte(block_end)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
        )
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if measured_utf16 != block_source_utf16 {
        return Err(
            M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
                "block UTF-16 authority disagrees with immutable source",
            ),
        );
    }
    let authority = M11ParserSourceRangeAuthority::new(runtime, lease, block_start..block_end)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let parser_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    Ok(M11PublishedIndentedCodeLeafFence {
        source,
        block_source,
        block_source_utf16,
        entry_ordinal: location.entry_ordinal(),
        binding: M11ParserBinding::current(parser_profile),
        query_receipt: location.receipt(),
        line_count: summary.line_count,
        projected_utf8_length: summary.projected_utf8_length,
        projected_utf16_length: summary.projected_utf16_length,
        terminal_eol_bytes: summary.terminal_eol_bytes,
        has_bof_bom: summary.has_bof_bom,
        authority,
    })
}

fn decode_published_indented_code_summary(
    location: &M11BlockSequenceLocation,
) -> Result<PublishedIndentedCodeSummary, M11CandidateDerivationError> {
    let entry = location.entry();
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
            "Indented Code Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
            "Indented Code Projection record is absent",
        ),
    )?;
    let green = green.as_bytes();
    let projection = projection.as_bytes();
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green.get(..8) != Some(GREEN_MAGIC.as_slice())
        || projection.get(..8) != Some(PROJECTION_MAGIC.as_slice())
        || green.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || projection.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || green.get(12) != Some(&INDENTED_CODE_ROLE_VARIANT)
        || projection.get(12) != Some(&INDENTED_CODE_ROLE_VARIANT)
        || green.get(13..16) != Some(&[0; 3])
        || projection.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
                "variant-7 role header is invalid",
            ),
        );
    }
    let source_bytes = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let source_utf16 = u32::try_from(entry.source_utf16_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let metadata = read_record_u64(green, 48)?;
    let line_count = read_record_u32(green, 56)?;
    let projected_utf8_length = read_record_u32(green, 60)?;
    let projected_utf16_length = read_record_u32(green, 64)?;
    let terminal_eol_bytes = read_record_u32(green, 68)?;
    let known_metadata = u64::from(INDENTED_CODE_DEINDENT_COLUMNS)
        | if metadata & INDENTED_CODE_BOF_BOM_FLAG != 0 {
            INDENTED_CODE_BOF_BOM_FLAG
        } else {
            0
        };
    if decode_record_range(green, 16)? != (0..source_bytes)
        || decode_record_range(projection, 16)? != (0..source_bytes)
        || decode_record_range(green, 32)? != (0..0)
        || decode_record_range(projection, 32)? != (0..0)
        || metadata != known_metadata
        || line_count == 0
        || projected_utf8_length > source_bytes
        || projected_utf16_length > source_utf16
        || terminal_eol_bytes > 2
        || read_record_u64(green, 72)? != 0
        || read_record_u64(projection, 48)? != u64::from(line_count)
        || entry.reference_definition_count() != 0
    {
        return Err(
            M11CandidateDerivationError::PublishedIndentedCodeLeafFenceCorrupt(
                "variant-7 structural summary disagrees with block coverage",
            ),
        );
    }
    Ok(PublishedIndentedCodeSummary {
        line_count,
        projected_utf8_length,
        projected_utf16_length,
        terminal_eol_bytes,
        has_bof_bom: metadata & INDENTED_CODE_BOF_BOM_FLAG != 0,
    })
}

#[derive(Clone, Copy)]
struct PublishedBlockQuoteSummary {
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

/// Resolves one exact retained variant-8 block and mints move-only authority
/// for its complete physical source window.
pub fn resolve_m11_published_block_quote_leaf_fence(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedBlockQuoteLeafFence, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedBlockQuoteLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    mint_published_block_quote_leaf_fence(runtime, source, location, descriptor.syntax_profile)
}

fn mint_published_block_quote_leaf_fence(
    runtime: &DocumentRuntime,
    source: SourceVersion,
    location: M11BlockSequenceLocation,
    syntax_profile: u32,
) -> Result<M11PublishedBlockQuoteLeafFence, M11CandidateDerivationError> {
    let entry = location.entry();
    let is_block_quote = entry.kind() == M11BlockSequenceEntryKind::Structured
        && entry
            .green()
            .is_some_and(|record| record.as_bytes().get(12) == Some(&BLOCK_QUOTE_ROLE_VARIANT));
    if !is_block_quote {
        return Err(M11CandidateDerivationError::PublishedBlockQuoteLeafFenceNotBlockQuote);
    }
    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let summary = decode_published_block_quote_summary(&location)?;
    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let block_start = usize::try_from(block_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let block_end = usize::try_from(block_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let measured_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(block_start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        ..u32::try_from(
            lease
                .utf16_offset_for_byte(block_end)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
        )
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if measured_utf16 != block_source_utf16 {
        return Err(
            M11CandidateDerivationError::PublishedBlockQuoteLeafFenceCorrupt(
                "block UTF-16 authority disagrees with immutable source",
            ),
        );
    }
    let authority = M11ParserSourceRangeAuthority::new(runtime, lease, block_start..block_end)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let parser_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    Ok(M11PublishedBlockQuoteLeafFence {
        source,
        block_source,
        block_source_utf16,
        entry_ordinal: location.entry_ordinal(),
        binding: M11ParserBinding::current(parser_profile),
        query_receipt: location.receipt(),
        line_count: summary.line_count,
        projected_utf8_length: summary.projected_utf8_length,
        projected_utf16_length: summary.projected_utf16_length,
        authority,
    })
}

fn decode_published_block_quote_summary(
    location: &M11BlockSequenceLocation,
) -> Result<PublishedBlockQuoteSummary, M11CandidateDerivationError> {
    let entry = location.entry();
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedBlockQuoteLeafFenceCorrupt(
            "Block Quote Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedBlockQuoteLeafFenceCorrupt(
            "Block Quote Projection record is absent",
        ),
    )?;
    let green = green.as_bytes();
    let projection = projection.as_bytes();
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green.get(..8) != Some(GREEN_MAGIC.as_slice())
        || projection.get(..8) != Some(PROJECTION_MAGIC.as_slice())
        || green.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || projection.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || green.get(12) != Some(&BLOCK_QUOTE_ROLE_VARIANT)
        || projection.get(12) != Some(&BLOCK_QUOTE_ROLE_VARIANT)
        || green.get(13..16) != Some(&[0; 3])
        || projection.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedBlockQuoteLeafFenceCorrupt(
                "variant-8 role header is invalid",
            ),
        );
    }
    let source_bytes = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let source_utf16 = u32::try_from(entry.source_utf16_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let disposition = read_record_u64(green, 48)?;
    let line_count = read_record_u32(green, 56)?;
    let child_first_line = read_record_u32(green, 60)?;
    let child_line_count = read_record_u32(green, 64)?;
    let projected_utf8_length = read_record_u32(green, 68)?;
    let projected_utf16_length = read_record_u32(green, 72)?;
    if decode_record_range(green, 16)? != (0..source_bytes)
        || decode_record_range(projection, 16)? != (0..source_bytes)
        || decode_record_range(green, 32)? != (0..0)
        || decode_record_range(projection, 32)? != (0..0)
        || disposition != BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION
        || line_count == 0
        || child_first_line != 0
        || child_line_count != line_count
        || projected_utf8_length == 0
        || projected_utf8_length >= source_bytes
        || projected_utf16_length == 0
        || projected_utf16_length >= source_utf16
        || read_record_u32(green, 76)? != 0
        || read_record_u64(projection, 48)? != u64::from(line_count)
        || entry.reference_definition_count() != 0
    {
        return Err(
            M11CandidateDerivationError::PublishedBlockQuoteLeafFenceCorrupt(
                "variant-8 structural summary disagrees with block coverage",
            ),
        );
    }
    Ok(PublishedBlockQuoteSummary {
        line_count,
        projected_utf8_length,
        projected_utf16_length,
    })
}

#[derive(Clone, Copy)]
struct PublishedBulletListSummary {
    item_count: u32,
    paragraph_count: u32,
    marker: u8,
    terminal_empty_relative_start: Option<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

#[derive(Clone, Copy)]
struct PublishedOrderedListSummary {
    item_count: u32,
    paragraph_count: u32,
    start: u32,
    delimiter: u8,
    terminal_empty_relative_start: Option<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

/// Resolves one exact retained variant-9 block and mints move-only authority
/// for its complete physical source window.
pub fn resolve_m11_published_bullet_list_leaf_fence(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedBulletListLeafFence, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    mint_published_bullet_list_leaf_fence(runtime, source, location, descriptor.syntax_profile)
}

/// Resolves one exact retained variant-10 block and mints move-only authority
/// for its complete physical source window.
pub fn resolve_m11_published_ordered_list_leaf_fence(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedOrderedListLeafFence, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    mint_published_ordered_list_leaf_fence(runtime, source, location, descriptor.syntax_profile)
}

/// Compatibility spelling for callers that consume only the content-inline
/// half of the selected item resolution.
pub fn resolve_m11_published_bullet_list_item_inline_fence(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedBulletListItemInlineFenceOutcome, M11CandidateDerivationError> {
    resolve_m11_published_bullet_list_item_fences(runtime, publication, point)
}

/// Resolves one late point inside an exact published tight bullet list to the
/// selected one-line item's compact structural and optional inline authority.
///
/// Physical-line rank/select comes from the same immutable source lease, so
/// list size does not affect discovery work. The selected line is then
/// reclassified through the parser's Comrak-derived segmented donor before an
/// item-window projection fence and existing inline projection fence are
/// minted together. Marker-only content returns a typed no-inline result with
/// the compact structural fence and is accepted only at the summary-certified
/// terminal-empty position.
pub fn resolve_m11_published_bullet_list_item_fences(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedBulletListItemInlineFenceOutcome, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    let entry = location.entry();
    if entry.kind() != M11BlockSequenceEntryKind::Structured
        || entry
            .green()
            .is_none_or(|record| record.as_bytes().get(12) != Some(&BULLET_LIST_ROLE_VARIANT))
    {
        return Err(M11CandidateDerivationError::PublishedBulletListLeafFenceNotBulletList);
    }

    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let summary = decode_published_bullet_list_summary(&location)?;
    let entry_ordinal = location.entry_ordinal();
    let query_receipt = location.receipt();
    let parser_profile = ParserProfileId::new(u64::from(descriptor.syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    let binding = M11ParserBinding::current(parser_profile);
    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }

    let block_start = usize::try_from(block_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let block_end = usize::try_from(block_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let first_line = lease
        .locate_physical_line(block_start, SourceBoundaryAffinity::After)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?
        .ok_or(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "published list has no first physical line",
            ),
        )?;
    let selected_line = lease
        .locate_physical_line(point.byte_offset(), point.affinity())
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?
        .ok_or(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "published list point has no physical line",
            ),
        )?;
    let selected_bytes = selected_line.byte_range();
    if first_line.source() != source
        || selected_line.source() != source
        || first_line.byte_range().start != block_start
        || selected_bytes.start < block_start
        || selected_bytes.end > block_end
        || selected_bytes.start >= selected_bytes.end
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "selected physical line escaped published list coverage",
            ),
        );
    }
    let item_ordinal = selected_line
        .ordinal()
        .checked_sub(first_line.ordinal())
        .and_then(|ordinal| u32::try_from(ordinal).ok())
        .ok_or(M11CandidateDerivationError::MetricOverflow)?;
    if item_ordinal >= summary.item_count {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "selected physical-line rank exceeds published item count",
            ),
        );
    }
    let physical_bytes = selected_bytes
        .end
        .checked_sub(selected_bytes.start)
        .ok_or(M11CandidateDerivationError::MetricOverflow)?;
    if physical_bytes > SELECTED_LIST_ITEM_PHYSICAL_WINDOW_MAX_BYTES {
        return Err(M11InlinePublicationError::OverCap {
            bytes: physical_bytes,
            cap: SELECTED_LIST_ITEM_PHYSICAL_WINDOW_MAX_BYTES,
        }
        .into());
    }
    let line_start_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(selected_bytes.start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let scanner = SnapshotLineScanner::new_in(lease, selected_bytes.clone(), item_ordinal)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let (poll, inspected) = scanner
        .poll_counted_retaining_complete(physical_bytes)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let SnapshotLineRetainedPoll::Line(line) = poll else {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "selected physical line did not complete within exact coverage",
            ),
        );
    };
    if inspected != physical_bytes
        || line.facts().identity().source() != source
        || line.facts().identity().start_byte() as usize != selected_bytes.start
        || line.facts().identity().end_byte() as usize != selected_bytes.end
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "selected physical-line facts disagree with rank/select",
            ),
        );
    }

    let physical = line.facts();
    let mut line_source = line
        .into_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let mut segmented = SegmentedLineScanner::new(selected_bytes.start == 0);
    while line_source.position() < line_source.len() {
        if line_source.access_budget() == 0 {
            let remaining = line_source.len() - line_source.position();
            let _ = line_source
                .replenish_access_budget(remaining)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
        }
        let offset = line_source.position();
        let byte = line_source
            .read_byte(offset)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
        segmented.push(byte);
    }
    let scanner = line_source
        .finish()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let lease = scanner.into_source_lease();
    let segmented = segmented.finish().map_err(|_| {
        M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "selected item donor classification failed",
        )
    })?;
    let item_facts = segmented.list_item.ok_or(
        M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "selected physical line has no parser-donor list item",
        ),
    )?;
    if segmented.blank
        || !segmented.list
        || M11CleanBlockController::list_item_unsupported_reason(item_facts).is_some()
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "selected physical line left the admitted tight-list subset",
            ),
        );
    }
    let item = M11CleanBlockController::bullet_list_item_mapping(
        SourceCut {
            byte: u32::try_from(selected_bytes.start)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
            utf16: line_start_utf16,
        },
        physical,
        item_facts,
        segmented.has_bof_bom,
        item_ordinal,
    )
    .map_err(|error| match error {
        M11CleanControllerFault::MetricOverflow | M11CleanControllerFault::OrdinalExhausted => {
            M11CandidateDerivationError::MetricOverflow
        }
        _ => M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "selected item mapping disagrees with donor facts",
        ),
    })?;
    if item.marker != summary.marker
        || item.source.start as usize != selected_bytes.start
        || item.source.end as usize != selected_bytes.end
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "selected item mapping disagrees with published list summary",
            ),
        );
    }

    let terminal_empty_start = summary
        .terminal_empty_relative_start
        .and_then(|relative| block_source.start.checked_add(relative));
    let terminal_empty = item.paragraph.is_none();
    if terminal_empty {
        if terminal_empty_start != Some(item.source.start)
            || item_ordinal.checked_add(1) != Some(summary.item_count)
            || item.content_source.start != item.content_source.end
            || item.content_source_utf16.start != item.content_source_utf16.end
        {
            return Err(
                M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                    "empty item is not the summary-certified terminal item",
                ),
            );
        }
    } else if terminal_empty_start == Some(item.source.start)
        || item.content_source.start >= item.content_source.end
        || item.content_source_utf16.start >= item.content_source_utf16.end
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "nonempty item disagrees with published terminal-empty summary",
            ),
        );
    }
    if !terminal_empty {
        let content_bytes = usize::try_from(item.content_source.end - item.content_source.start)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        if content_bytes > SELECTED_LIST_ITEM_INLINE_MAX_BYTES {
            return Err(M11InlinePublicationError::OverCap {
                bytes: content_bytes,
                cap: SELECTED_LIST_ITEM_INLINE_MAX_BYTES,
            }
            .into());
        }
    }

    let item_source = item.source.clone();
    let item_source_utf16 = item.source_utf16.clone();
    let content_source = item.content_source.clone();
    let content_source_utf16 = item.content_source_utf16.clone();
    let physical_line_ending = physical.ending();
    let source_discovery_bytes =
        u32::try_from(inspected).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let canonical_line_ending = if physical_line_ending != M11LineEnding::Eof {
        physical_line_ending
    } else if item_ordinal == 0 {
        M11LineEnding::Lf
    } else {
        let predecessor = lease
            .locate_physical_line(selected_bytes.start, SourceBoundaryAffinity::Before)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?
            .ok_or(
                M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                    "EOF list item has no predecessor physical line",
                ),
            )?;
        let predecessor_bytes = predecessor.byte_range();
        if predecessor.source() != source
            || predecessor.ordinal().checked_add(1) != Some(selected_line.ordinal())
            || predecessor_bytes.start < block_start
            || predecessor_bytes.end != selected_bytes.start
            || predecessor_bytes.start >= predecessor_bytes.end
        {
            return Err(
                M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                    "EOF list item predecessor escaped published list coverage",
                ),
            );
        }
        match predecessor.ending() {
            flark_engine::LineEnding::Lf => M11LineEnding::Lf,
            flark_engine::LineEnding::CrLf => M11LineEnding::CrLf,
            flark_engine::LineEnding::Cr => M11LineEnding::Cr,
            flark_engine::LineEnding::Eof => {
                return Err(
                    M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                        "EOF list item predecessor has no physical line ending",
                    ),
                );
            }
        }
    };
    let projection_authority = M11ParserSourceRangeAuthority::new(
        runtime,
        lease,
        item_source.start as usize..item_source.end as usize,
    )
    .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let projection_fence = M11PublishedBulletListItemProjectionFence {
        source,
        block_source: block_source.clone(),
        block_source_utf16: block_source_utf16.clone(),
        entry_ordinal,
        binding,
        query_receipt,
        item,
        physical_line_ending,
        canonical_line_ending,
        source_discovery_bytes,
        authority: projection_authority,
    };

    if terminal_empty {
        return Ok(M11PublishedBulletListItemInlineFenceOutcome::TerminalEmpty(
            M11PublishedBulletListTerminalEmpty {
                source,
                block_source,
                block_source_utf16,
                entry_ordinal,
                item_ordinal,
                item_source,
                item_source_utf16,
                content_source,
                content_source_utf16,
                binding,
                query_receipt,
                projection_fence,
            },
        ));
    }

    let inline_lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if inline_lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let inline_authority = M11ParserSourceRangeAuthority::new(
        runtime,
        inline_lease,
        content_source.start as usize..content_source.end as usize,
    )
    .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let inline_leaf_fence = M11PublishedInlineLeafFence {
        source,
        kind: M11BlockSequenceEntryKind::Structured,
        block_source: block_source.clone(),
        block_source_utf16: block_source_utf16.clone(),
        inline_source: content_source.clone(),
        inline_source_utf16: content_source_utf16.clone(),
        entry_ordinal,
        binding,
        query_receipt,
        authority: inline_authority,
    };
    Ok(M11PublishedBulletListItemInlineFenceOutcome::Inline(
        M11PublishedBulletListItemInlineFence {
            source,
            block_source,
            block_source_utf16,
            entry_ordinal,
            item_ordinal,
            item_source,
            item_source_utf16,
            content_source,
            content_source_utf16,
            binding,
            query_receipt,
            projection_fence,
            inline_leaf_fence,
        },
    ))
}

/// Resolves one late point inside an exact published tight ordered list to the
/// selected one-line item's compact structural and optional inline authority.
pub fn resolve_m11_published_ordered_list_item_fences(
    runtime: &DocumentRuntime,
    publication: &M11RetainedCandidatePublication,
    point: M11BlockSequencePoint,
) -> Result<M11PublishedOrderedListItemInlineFenceOutcome, M11CandidateDerivationError> {
    let descriptor = publication.descriptor(runtime)?;
    let source = runtime
        .current_source_version()
        .ok_or(M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if source.root().get() != descriptor.source_root
        || source.revision().get() != descriptor.source_revision
        || u64::try_from(source.byte_len()).ok() != Some(descriptor.source_bytes)
        || u64::try_from(source.utf16_len()).ok() != Some(descriptor.source_utf16)
    {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let location = publication.locate_block_point(runtime, point)?.ok_or(
        M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "a nonempty segmented source produced no point location",
        ),
    )?;
    let entry = location.entry();
    if entry.kind() != M11BlockSequenceEntryKind::Structured
        || entry
            .green()
            .is_none_or(|record| record.as_bytes().get(12) != Some(&ORDERED_LIST_ROLE_VARIANT))
    {
        return Err(M11CandidateDerivationError::PublishedOrderedListLeafFenceNotOrderedList);
    }

    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let summary = decode_published_ordered_list_summary(&location)?;
    let entry_ordinal = location.entry_ordinal();
    let query_receipt = location.receipt();
    let parser_profile = ParserProfileId::new(u64::from(descriptor.syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    let binding = M11ParserBinding::current(parser_profile);
    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }

    let block_start = usize::try_from(block_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let block_end = usize::try_from(block_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let first_line = lease
        .locate_physical_line(block_start, SourceBoundaryAffinity::After)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?
        .ok_or(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "published ordered list has no first physical line",
            ),
        )?;
    let selected_line = lease
        .locate_physical_line(point.byte_offset(), point.affinity())
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?
        .ok_or(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "published ordered-list point has no physical line",
            ),
        )?;
    let selected_bytes = selected_line.byte_range();
    if first_line.source() != source
        || selected_line.source() != source
        || first_line.byte_range().start != block_start
        || selected_bytes.start < block_start
        || selected_bytes.end > block_end
        || selected_bytes.start >= selected_bytes.end
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "selected physical line escaped published ordered-list coverage",
            ),
        );
    }
    let item_ordinal = selected_line
        .ordinal()
        .checked_sub(first_line.ordinal())
        .and_then(|ordinal| u32::try_from(ordinal).ok())
        .ok_or(M11CandidateDerivationError::MetricOverflow)?;
    if item_ordinal >= summary.item_count {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "selected physical-line rank exceeds published item count",
            ),
        );
    }
    let physical_bytes = selected_bytes
        .end
        .checked_sub(selected_bytes.start)
        .ok_or(M11CandidateDerivationError::MetricOverflow)?;
    if physical_bytes > SELECTED_LIST_ITEM_PHYSICAL_WINDOW_MAX_BYTES {
        return Err(M11InlinePublicationError::OverCap {
            bytes: physical_bytes,
            cap: SELECTED_LIST_ITEM_PHYSICAL_WINDOW_MAX_BYTES,
        }
        .into());
    }
    let line_start_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(selected_bytes.start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let scanner = SnapshotLineScanner::new_in(lease, selected_bytes.clone(), item_ordinal)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let (poll, inspected) = scanner
        .poll_counted_retaining_complete(physical_bytes)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let SnapshotLineRetainedPoll::Line(line) = poll else {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "selected physical line did not complete within exact coverage",
            ),
        );
    };
    if inspected != physical_bytes
        || line.facts().identity().source() != source
        || line.facts().identity().start_byte() as usize != selected_bytes.start
        || line.facts().identity().end_byte() as usize != selected_bytes.end
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "selected physical-line facts disagree with rank/select",
            ),
        );
    }

    let physical = line.facts();
    let mut line_source = line
        .into_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let mut segmented = SegmentedLineScanner::new(selected_bytes.start == 0);
    while line_source.position() < line_source.len() {
        if line_source.access_budget() == 0 {
            let remaining = line_source.len() - line_source.position();
            let _ = line_source
                .replenish_access_budget(remaining)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
        }
        let offset = line_source.position();
        let byte = line_source
            .read_byte(offset)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
        segmented.push(byte);
    }
    let scanner = line_source
        .finish()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let lease = scanner.into_source_lease();
    let segmented = segmented.finish().map_err(|_| {
        M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "selected item donor classification failed",
        )
    })?;
    let item_facts = segmented.list_item.ok_or(
        M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "selected physical line has no parser-donor list item",
        ),
    )?;
    if segmented.blank
        || !segmented.list
        || M11CleanBlockController::list_item_unsupported_reason(item_facts).is_some()
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "selected physical line left the admitted tight ordered-list subset",
            ),
        );
    }
    let item = M11CleanBlockController::ordered_list_item_mapping(
        SourceCut {
            byte: u32::try_from(selected_bytes.start)
                .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
            utf16: line_start_utf16,
        },
        physical,
        item_facts,
        segmented.has_bof_bom,
        item_ordinal,
    )
    .map_err(|error| match error {
        M11CleanControllerFault::MetricOverflow | M11CleanControllerFault::OrdinalExhausted => {
            M11CandidateDerivationError::MetricOverflow
        }
        _ => M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "selected ordered item mapping disagrees with donor facts",
        ),
    })?;
    if item.delimiter != summary.delimiter
        || item_ordinal == 0 && item.marker_value != summary.start
        || item.source.start as usize != selected_bytes.start
        || item.source.end as usize != selected_bytes.end
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "selected item mapping disagrees with published ordered-list summary",
            ),
        );
    }

    let terminal_empty_start = summary
        .terminal_empty_relative_start
        .and_then(|relative| block_source.start.checked_add(relative));
    let terminal_empty = item.paragraph.is_none();
    if terminal_empty {
        if terminal_empty_start != Some(item.source.start)
            || item_ordinal.checked_add(1) != Some(summary.item_count)
            || item.content_source.start != item.content_source.end
            || item.content_source_utf16.start != item.content_source_utf16.end
        {
            return Err(
                M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                    "empty item is not the summary-certified terminal item",
                ),
            );
        }
    } else if terminal_empty_start == Some(item.source.start)
        || item.content_source.start >= item.content_source.end
        || item.content_source_utf16.start >= item.content_source_utf16.end
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "nonempty item disagrees with published terminal-empty summary",
            ),
        );
    }
    if !terminal_empty {
        let content_bytes = usize::try_from(item.content_source.end - item.content_source.start)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        if content_bytes > SELECTED_LIST_ITEM_INLINE_MAX_BYTES {
            return Err(M11InlinePublicationError::OverCap {
                bytes: content_bytes,
                cap: SELECTED_LIST_ITEM_INLINE_MAX_BYTES,
            }
            .into());
        }
    }

    let item_source = item.source.clone();
    let content_source = item.content_source.clone();
    let content_source_utf16 = item.content_source_utf16.clone();
    let physical_line_ending = physical.ending();
    let source_discovery_bytes =
        u32::try_from(inspected).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let canonical_line_ending = if physical_line_ending != M11LineEnding::Eof {
        physical_line_ending
    } else if item_ordinal == 0 {
        M11LineEnding::Lf
    } else {
        let predecessor = lease
            .locate_physical_line(selected_bytes.start, SourceBoundaryAffinity::Before)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?
            .ok_or(
                M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                    "EOF ordered-list item has no predecessor physical line",
                ),
            )?;
        let predecessor_bytes = predecessor.byte_range();
        if predecessor.source() != source
            || predecessor.ordinal().checked_add(1) != Some(selected_line.ordinal())
            || predecessor_bytes.start < block_start
            || predecessor_bytes.end != selected_bytes.start
            || predecessor_bytes.start >= predecessor_bytes.end
        {
            return Err(
                M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                    "EOF ordered-list predecessor escaped published coverage",
                ),
            );
        }
        match predecessor.ending() {
            flark_engine::LineEnding::Lf => M11LineEnding::Lf,
            flark_engine::LineEnding::CrLf => M11LineEnding::CrLf,
            flark_engine::LineEnding::Cr => M11LineEnding::Cr,
            flark_engine::LineEnding::Eof => {
                return Err(
                    M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                        "EOF ordered-list predecessor has no line ending",
                    ),
                );
            }
        }
    };
    let projection_authority = M11ParserSourceRangeAuthority::new(
        runtime,
        lease,
        item_source.start as usize..item_source.end as usize,
    )
    .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let projection_fence = M11PublishedOrderedListItemProjectionFence {
        source,
        block_source: block_source.clone(),
        block_source_utf16: block_source_utf16.clone(),
        entry_ordinal,
        binding,
        query_receipt,
        item,
        physical_line_ending,
        canonical_line_ending,
        source_discovery_bytes,
        authority: projection_authority,
    };

    if terminal_empty {
        return Ok(
            M11PublishedOrderedListItemInlineFenceOutcome::TerminalEmpty(
                M11PublishedOrderedListTerminalEmpty { projection_fence },
            ),
        );
    }

    let inline_lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if inline_lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let inline_authority = M11ParserSourceRangeAuthority::new(
        runtime,
        inline_lease,
        content_source.start as usize..content_source.end as usize,
    )
    .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let inline_leaf_fence = M11PublishedInlineLeafFence {
        source,
        kind: M11BlockSequenceEntryKind::Structured,
        block_source,
        block_source_utf16,
        inline_source: content_source,
        inline_source_utf16: content_source_utf16,
        entry_ordinal,
        binding,
        query_receipt,
        authority: inline_authority,
    };
    Ok(M11PublishedOrderedListItemInlineFenceOutcome::Inline(
        M11PublishedOrderedListItemInlineFence {
            projection_fence,
            inline_leaf_fence,
        },
    ))
}

fn mint_published_bullet_list_leaf_fence(
    runtime: &DocumentRuntime,
    source: SourceVersion,
    location: M11BlockSequenceLocation,
    syntax_profile: u32,
) -> Result<M11PublishedBulletListLeafFence, M11CandidateDerivationError> {
    let entry = location.entry();
    let is_bullet_list = entry.kind() == M11BlockSequenceEntryKind::Structured
        && entry
            .green()
            .is_some_and(|record| record.as_bytes().get(12) == Some(&BULLET_LIST_ROLE_VARIANT));
    if !is_bullet_list {
        return Err(M11CandidateDerivationError::PublishedBulletListLeafFenceNotBulletList);
    }
    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let summary = decode_published_bullet_list_summary(&location)?;
    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let block_start = usize::try_from(block_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let block_end = usize::try_from(block_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let measured_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(block_start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        ..u32::try_from(
            lease
                .utf16_offset_for_byte(block_end)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
        )
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if measured_utf16 != block_source_utf16 {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "block UTF-16 authority disagrees with immutable source",
            ),
        );
    }
    let authority = M11ParserSourceRangeAuthority::new(runtime, lease, block_start..block_end)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let parser_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    Ok(M11PublishedBulletListLeafFence {
        source,
        block_source,
        block_source_utf16,
        entry_ordinal: location.entry_ordinal(),
        binding: M11ParserBinding::current(parser_profile),
        query_receipt: location.receipt(),
        item_count: summary.item_count,
        paragraph_count: summary.paragraph_count,
        marker: summary.marker,
        terminal_empty_relative_start: summary.terminal_empty_relative_start,
        projected_utf8_length: summary.projected_utf8_length,
        projected_utf16_length: summary.projected_utf16_length,
        authority,
    })
}

fn decode_published_bullet_list_summary(
    location: &M11BlockSequenceLocation,
) -> Result<PublishedBulletListSummary, M11CandidateDerivationError> {
    let entry = location.entry();
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "Bullet List Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
            "Bullet List Projection record is absent",
        ),
    )?;
    let green = green.as_bytes();
    let projection = projection.as_bytes();
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green.get(..8) != Some(GREEN_MAGIC.as_slice())
        || projection.get(..8) != Some(PROJECTION_MAGIC.as_slice())
        || green.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || projection.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || green.get(12) != Some(&BULLET_LIST_ROLE_VARIANT)
        || projection.get(12) != Some(&BULLET_LIST_ROLE_VARIANT)
        || green.get(13..16) != Some(&[0; 3])
        || projection.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "variant-9 role header is invalid",
            ),
        );
    }
    let source_bytes = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let source_utf16 = u32::try_from(entry.source_utf16_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let metadata = read_record_u64(green, 48)?;
    let marker = u8::try_from((metadata >> BULLET_LIST_MARKER_SHIFT) & 0xff)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let known_metadata = BULLET_LIST_EXACT_DISPOSITION
        | (u64::from(marker) << BULLET_LIST_MARKER_SHIFT)
        | BULLET_LIST_TIGHT_FLAG;
    let item_count = read_record_u32(green, 56)?;
    let terminal_empty = read_record_u32(green, 60)?;
    let paragraph_count = read_record_u32(green, 64)?;
    let projected_utf8_length = read_record_u32(green, 68)?;
    let projected_utf16_length = read_record_u32(green, 72)?;
    let terminal_empty_relative_start =
        (terminal_empty != BULLET_LIST_ABSENT_TERMINAL_EMPTY).then_some(terminal_empty);
    let expected_paragraph_count = if terminal_empty_relative_start.is_some() {
        item_count.checked_sub(1)
    } else {
        Some(item_count)
    };
    if decode_record_range(green, 16)? != (0..source_bytes)
        || decode_record_range(projection, 16)? != (0..source_bytes)
        || decode_record_range(green, 32)? != (0..0)
        || decode_record_range(projection, 32)? != (0..0)
        || !matches!(marker, b'-' | b'+' | b'*')
        || metadata != known_metadata
        || item_count == 0
        || expected_paragraph_count != Some(paragraph_count)
        || terminal_empty_relative_start.is_some_and(|start| start >= source_bytes)
        || projected_utf8_length >= source_bytes
        || projected_utf16_length >= source_utf16
        || read_record_u32(green, 76)? != 0
        || read_record_u64(projection, 48)? != u64::from(item_count)
        || entry.reference_definition_count() != 0
    {
        return Err(
            M11CandidateDerivationError::PublishedBulletListLeafFenceCorrupt(
                "variant-9 structural summary disagrees with block coverage",
            ),
        );
    }
    Ok(PublishedBulletListSummary {
        item_count,
        paragraph_count,
        marker,
        terminal_empty_relative_start,
        projected_utf8_length,
        projected_utf16_length,
    })
}

fn mint_published_ordered_list_leaf_fence(
    runtime: &DocumentRuntime,
    source: SourceVersion,
    location: M11BlockSequenceLocation,
    syntax_profile: u32,
) -> Result<M11PublishedOrderedListLeafFence, M11CandidateDerivationError> {
    let entry = location.entry();
    let is_ordered_list = entry.kind() == M11BlockSequenceEntryKind::Structured
        && entry
            .green()
            .is_some_and(|record| record.as_bytes().get(12) == Some(&ORDERED_LIST_ROLE_VARIANT));
    if !is_ordered_list {
        return Err(M11CandidateDerivationError::PublishedOrderedListLeafFenceNotOrderedList);
    }
    let block_source = u64_range_to_u32(location.byte_range())?;
    let block_source_utf16 = u64_range_to_u32(location.utf16_range())?;
    let summary = decode_published_ordered_list_summary(&location)?;
    let lease = runtime
        .snapshot_current_source()
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    if lease.version() != source {
        return Err(M11CandidateDerivationError::SourceAuthorityMismatch);
    }
    let block_start = usize::try_from(block_source.start)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let block_end = usize::try_from(block_source.end)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let measured_utf16 = u32::try_from(
        lease
            .utf16_offset_for_byte(block_start)
            .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)?
        ..u32::try_from(
            lease
                .utf16_offset_for_byte(block_end)
                .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?,
        )
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if measured_utf16 != block_source_utf16 {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "block UTF-16 authority disagrees with immutable source",
            ),
        );
    }
    let authority = M11ParserSourceRangeAuthority::new(runtime, lease, block_start..block_end)
        .map_err(|_| M11CandidateDerivationError::SourceAuthorityMismatch)?;
    let parser_profile = ParserProfileId::new(u64::from(syntax_profile))
        .ok_or(M11CandidateDerivationError::ParserProfileOverflow)?;
    Ok(M11PublishedOrderedListLeafFence {
        source,
        block_source,
        block_source_utf16,
        entry_ordinal: location.entry_ordinal(),
        binding: M11ParserBinding::current(parser_profile),
        query_receipt: location.receipt(),
        item_count: summary.item_count,
        paragraph_count: summary.paragraph_count,
        start: summary.start,
        delimiter: summary.delimiter,
        terminal_empty_relative_start: summary.terminal_empty_relative_start,
        projected_utf8_length: summary.projected_utf8_length,
        projected_utf16_length: summary.projected_utf16_length,
        authority,
    })
}

fn decode_published_ordered_list_summary(
    location: &M11BlockSequenceLocation,
) -> Result<PublishedOrderedListSummary, M11CandidateDerivationError> {
    let entry = location.entry();
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "Ordered List Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
            "Ordered List Projection record is absent",
        ),
    )?;
    let green = green.as_bytes();
    let projection = projection.as_bytes();
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green.get(..8) != Some(GREEN_MAGIC.as_slice())
        || projection.get(..8) != Some(PROJECTION_MAGIC.as_slice())
        || green.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || projection.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || green.get(12) != Some(&ORDERED_LIST_ROLE_VARIANT)
        || projection.get(12) != Some(&ORDERED_LIST_ROLE_VARIANT)
        || green.get(13..16) != Some(&[0; 3])
        || projection.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "variant-10 role header is invalid",
            ),
        );
    }
    let source_bytes = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let source_utf16 = u32::try_from(entry.source_utf16_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let metadata = read_record_u64(green, 48)?;
    let delimiter = u8::try_from((metadata >> ORDERED_LIST_DELIMITER_SHIFT) & 0xff)
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let known_metadata = ORDERED_LIST_EXACT_DISPOSITION
        | (u64::from(delimiter) << ORDERED_LIST_DELIMITER_SHIFT)
        | ORDERED_LIST_TIGHT_FLAG;
    let item_count = read_record_u32(green, 56)?;
    let terminal_empty = read_record_u32(green, 60)?;
    let paragraph_count = read_record_u32(green, 64)?;
    let projected_utf8_length = read_record_u32(green, 68)?;
    let projected_utf16_length = read_record_u32(green, 72)?;
    let start = read_record_u32(green, 76)?;
    let terminal_empty_relative_start =
        (terminal_empty != ORDERED_LIST_ABSENT_TERMINAL_EMPTY).then_some(terminal_empty);
    let expected_paragraph_count = if terminal_empty_relative_start.is_some() {
        item_count.checked_sub(1)
    } else {
        Some(item_count)
    };
    if decode_record_range(green, 16)? != (0..source_bytes)
        || decode_record_range(projection, 16)? != (0..source_bytes)
        || decode_record_range(green, 32)? != (0..0)
        || decode_record_range(projection, 32)? != (0..0)
        || !matches!(delimiter, b'.' | b')')
        || start > 999_999_999
        || metadata != known_metadata
        || item_count == 0
        || expected_paragraph_count != Some(paragraph_count)
        || terminal_empty_relative_start.is_some_and(|relative| relative >= source_bytes)
        || projected_utf8_length >= source_bytes
        || projected_utf16_length >= source_utf16
        || read_record_u64(projection, 48)? != u64::from(item_count)
        || entry.reference_definition_count() != 0
    {
        return Err(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceCorrupt(
                "variant-10 structural summary disagrees with block coverage",
            ),
        );
    }
    Ok(PublishedOrderedListSummary {
        item_count,
        paragraph_count,
        start,
        delimiter,
        terminal_empty_relative_start,
        projected_utf8_length,
        projected_utf16_length,
    })
}

fn decode_published_inline_relative(
    entry: &M11BlockSequenceEntry,
    block_byte_start: u64,
) -> Result<Option<std::ops::Range<u32>>, M11CandidateDerivationError> {
    match entry.kind() {
        M11BlockSequenceEntryKind::Paragraph => {
            decode_published_paragraph_inline_relative(entry).map(Some)
        }
        M11BlockSequenceEntryKind::Structured
            if entry.green().is_some_and(|record| {
                record.as_bytes().get(12) == Some(&ATX_HEADING_ROLE_VARIANT)
            }) =>
        {
            decode_published_atx_inline_relative(entry, block_byte_start).map(Some)
        }
        M11BlockSequenceEntryKind::Structured
            if entry.green().is_some_and(|record| {
                record.as_bytes().get(12) == Some(&SETEXT_HEADING_ROLE_VARIANT)
            }) =>
        {
            decode_published_setext_inline_relative(entry).map(Some)
        }
        _ => Ok(None),
    }
}

fn decode_published_paragraph_inline_relative(
    entry: &M11BlockSequenceEntry,
) -> Result<std::ops::Range<u32>, M11CandidateDerivationError> {
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "Paragraph Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "Paragraph Projection record is absent",
        ),
    )?;
    let green_inline = decode_published_paragraph_role_record(
        green.as_bytes(),
        GREEN_MAGIC,
        M11_GREEN_RECORD_BYTES,
    )?;
    let projection_inline = decode_published_paragraph_role_record(
        projection.as_bytes(),
        PROJECTION_MAGIC,
        M11_PROJECTION_RECORD_BYTES,
    )?;
    if green_inline != projection_inline {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "Paragraph Green and Projection ranges disagree",
            ),
        );
    }
    let expected_source_end = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let green_bytes = green.as_bytes();
    let projection_bytes = projection.as_bytes();
    if decode_record_range(green_bytes, 16)? != (0..expected_source_end)
        || decode_record_range(projection_bytes, 16)? != (0..expected_source_end)
        || read_record_u64(projection_bytes, 48)? != 1
        || read_record_u64(green_bytes, 48)? != entry.reference_definition_count()
        || green_bytes.get(56..80) != Some(&[0; 24])
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "Paragraph role metadata disagrees with block coverage",
            ),
        );
    }
    Ok(green_inline)
}

fn decode_published_atx_inline_relative(
    entry: &M11BlockSequenceEntry,
    block_byte_start: u64,
) -> Result<std::ops::Range<u32>, M11CandidateDerivationError> {
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "ATX Heading Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "ATX Heading Projection record is absent",
        ),
    )?;
    let green_bytes = green.as_bytes();
    let projection_bytes = projection.as_bytes();
    if green_bytes.len() != M11_GREEN_RECORD_BYTES
        || projection_bytes.len() != M11_PROJECTION_RECORD_BYTES
        || green_bytes.get(..8) != Some(GREEN_MAGIC.as_slice())
        || projection_bytes.get(..8) != Some(PROJECTION_MAGIC.as_slice())
        || green_bytes.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || projection_bytes.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || green_bytes.get(12) != Some(&ATX_HEADING_ROLE_VARIANT)
        || projection_bytes.get(12) != Some(&ATX_HEADING_ROLE_VARIANT)
        || green_bytes.get(13..16) != Some(&[0; 3])
        || projection_bytes.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "ATX Heading role header is invalid",
            ),
        );
    }
    let expected_source_end = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let inline = decode_record_range(green_bytes, 32)?;
    if decode_record_range(green_bytes, 16)? != (0..expected_source_end)
        || decode_record_range(projection_bytes, 16)? != (0..expected_source_end)
        || decode_record_range(projection_bytes, 32)? != inline
        || inline.start > inline.end
        || inline.end > expected_source_end
        || entry.reference_definition_count() != 0
        || read_record_u64(projection_bytes, 48)? != 1
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "ATX Heading ranges disagree with block coverage",
            ),
        );
    }

    let metadata = read_record_u64(green_bytes, 48)?;
    let level = u32::from(metadata as u8);
    let closed = metadata & ATX_HEADING_CLOSED_FLAG != 0;
    let opening_indent = (metadata >> ATX_HEADING_OPENING_INDENT_SHIFT) & 0x3;
    let has_bof_bom = metadata & ATX_HEADING_BOF_BOM_FLAG != 0;
    let opening_start = read_record_u32(green_bytes, 56)?;
    let opening_end = read_record_u32(green_bytes, 60)?;
    let closing_start = read_record_u32(green_bytes, 64)?;
    let closing_end = read_record_u32(green_bytes, 68)?;
    let line_ending_start = read_record_u32(green_bytes, 72)?;
    let line_ending_end = read_record_u32(green_bytes, 76)?;
    let closing_valid = if closed {
        closing_start != ATX_HEADING_ABSENT_CUT
            && closing_end != ATX_HEADING_ABSENT_CUT
            && closing_start < closing_end
            && inline.end <= closing_start
            && closing_end <= line_ending_start
    } else {
        closing_start == ATX_HEADING_ABSENT_CUT
            && closing_end == ATX_HEADING_ABSENT_CUT
            && inline.end <= line_ending_start
    };
    if metadata
        & !(ATX_HEADING_CLOSED_FLAG
            | (0x3 << ATX_HEADING_OPENING_INDENT_SHIFT)
            | ATX_HEADING_BOF_BOM_FLAG
            | 0xff)
        != 0
        || !(1..=6).contains(&level)
        || has_bof_bom && block_byte_start != 0
        || opening_start >= opening_end
        || opening_end - opening_start != level
        || u64::from(opening_start) != opening_indent + if has_bof_bom { 3 } else { 0 }
        || opening_end > inline.start
        || line_ending_start > line_ending_end
        || line_ending_end != expected_source_end
        || line_ending_end - line_ending_start > 2
        || !closing_valid
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "ATX Heading marker metadata is invalid",
            ),
        );
    }
    Ok(inline)
}

fn decode_published_setext_inline_relative(
    entry: &M11BlockSequenceEntry,
) -> Result<std::ops::Range<u32>, M11CandidateDerivationError> {
    let green = entry.green().ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "Setext Heading Green record is absent",
        ),
    )?;
    let projection = entry.projection().ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "Setext Heading Projection record is absent",
        ),
    )?;
    let green_bytes = green.as_bytes();
    let projection_bytes = projection.as_bytes();
    if green_bytes.len() != M11_GREEN_RECORD_BYTES
        || projection_bytes.len() != M11_PROJECTION_RECORD_BYTES
        || green_bytes.get(..8) != Some(GREEN_MAGIC.as_slice())
        || projection_bytes.get(..8) != Some(PROJECTION_MAGIC.as_slice())
        || green_bytes.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || projection_bytes.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || green_bytes.get(12) != Some(&SETEXT_HEADING_ROLE_VARIANT)
        || projection_bytes.get(12) != Some(&SETEXT_HEADING_ROLE_VARIANT)
        || green_bytes.get(13..16) != Some(&[0; 3])
        || projection_bytes.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "Setext Heading role header is invalid",
            ),
        );
    }
    let expected_source_end = u32::try_from(entry.source_byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let inline = decode_record_range(green_bytes, 32)?;
    let marker_start = read_record_u32(green_bytes, 56)?;
    let marker_end = read_record_u32(green_bytes, 60)?;
    let line_ending_start = read_record_u32(green_bytes, 64)?;
    let line_ending_end = read_record_u32(green_bytes, 68)?;
    let reference_definition_count = read_record_u64(green_bytes, 72)?;
    let metadata = read_record_u64(green_bytes, 48)?;
    let level = u32::from(metadata as u8);
    let opening_indent = (metadata >> SETEXT_HEADING_OPENING_INDENT_SHIFT) & 0x3;
    if decode_record_range(green_bytes, 16)? != (0..expected_source_end)
        || decode_record_range(projection_bytes, 16)? != (0..expected_source_end)
        || decode_record_range(projection_bytes, 32)? != inline
        || inline.start >= inline.end
        || inline.end > marker_start
        || !(opening_indent + 1..=opening_indent + 2)
            .contains(&u64::from(marker_start - inline.end))
        || marker_start >= marker_end
        || marker_end > line_ending_start
        || line_ending_start > line_ending_end
        || line_ending_end != expected_source_end
        || line_ending_end - line_ending_start > 2
        || !matches!(level, 1 | 2)
        || metadata & !((0x3 << SETEXT_HEADING_OPENING_INDENT_SHIFT) | 0xff) != 0
        || reference_definition_count != entry.reference_definition_count()
        || read_record_u64(projection_bytes, 48)? != 1
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "Setext Heading geometry disagrees with block coverage",
            ),
        );
    }
    Ok(inline)
}

fn decode_published_paragraph_role_record(
    input: &[u8],
    magic: &[u8; 8],
    expected_len: usize,
) -> Result<std::ops::Range<u32>, M11CandidateDerivationError> {
    if input.len() != expected_len
        || input.get(..8) != Some(magic.as_slice())
        || input.get(8..12) != Some(ROLE_SCHEMA_V1.to_le_bytes().as_slice())
        || input.get(12) != Some(&1)
        || input.get(13..16) != Some(&[0; 3])
    {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "Paragraph role header is invalid",
            ),
        );
    }
    let inline = decode_record_range(input, 32)?;
    if inline.start >= inline.end {
        return Err(
            M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
                "Paragraph inline range is empty or reversed",
            ),
        );
    }
    Ok(inline)
}

fn decode_record_range(
    input: &[u8],
    offset: usize,
) -> Result<std::ops::Range<u32>, M11CandidateDerivationError> {
    let start = read_record_u64(input, offset)?;
    let end = read_record_u64(input, offset + 8)?;
    Ok(
        u32::try_from(start).map_err(|_| M11CandidateDerivationError::MetricOverflow)?
            ..u32::try_from(end).map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
    )
}

fn read_record_u64(input: &[u8], offset: usize) -> Result<u64, M11CandidateDerivationError> {
    let bytes = input.get(offset..offset + 8).ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "inline-leaf role record is truncated",
        ),
    )?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "inline-leaf role integer is truncated",
        )
    })?))
}

fn read_record_u32(input: &[u8], offset: usize) -> Result<u32, M11CandidateDerivationError> {
    let bytes = input.get(offset..offset + 4).ok_or(
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "inline-leaf role record is truncated",
        ),
    )?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        M11CandidateDerivationError::PublishedInlineLeafFenceCorrupt(
            "inline-leaf role integer is truncated",
        )
    })?))
}

fn u64_range_to_u32(
    range: std::ops::Range<u64>,
) -> Result<std::ops::Range<u32>, M11CandidateDerivationError> {
    Ok(
        u32::try_from(range.start).map_err(|_| M11CandidateDerivationError::MetricOverflow)?
            ..u32::try_from(range.end).map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
    )
}

fn validate_exact_block_quote(
    source: &std::ops::Range<u32>,
    source_utf16: &std::ops::Range<u32>,
    lines: &[M11BlockQuoteLineMapping],
    child: &M11BlockQuoteParagraphMapping,
) -> Result<(), M11CandidateDerivationError> {
    let line_count =
        u32::try_from(lines.len()).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    if source.start >= source.end
        || source_utf16.start >= source_utf16.end
        || lines.is_empty()
        || child.line_indices != (0..line_count)
        || child.projected_utf8_length == 0
        || child.projected_utf16_length == 0
    {
        return Err(M11CandidateDerivationError::ResultRangeMismatch);
    }

    let mut next_byte = source.start;
    let mut next_utf16 = source_utf16.start;
    let mut projected_utf8 = 0_u32;
    let mut projected_utf16 = 0_u32;
    for line in lines {
        if line.source.start != next_byte
            || line.source_utf16.start != next_utf16
            || line.source.start >= line.source.end
            || line.source_utf16.start >= line.source_utf16.end
            || line.source.end > source.end
            || line.source_utf16.end > source_utf16.end
            || line.content_source.start > line.content_source.end
            || line.content_source.start < line.source.start
            || line.content_source.end != line.line_ending.start
            || line.line_ending.end != line.source.end
            || line.content_source_utf16.start > line.content_source_utf16.end
            || line.content_source_utf16.start < line.source_utf16.start
            || line.content_source_utf16.end != line.line_ending_utf16.start
            || line.line_ending_utf16.end != line.source_utf16.end
            || line.line_ending.end - line.line_ending.start > 2
            || line.line_ending_utf16.end - line.line_ending_utf16.start
                != line.line_ending.end - line.line_ending.start
            || line.residual_tab_columns != 0
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }
        match line.kind {
            M11BlockQuoteLineKind::MarkedParagraph => {
                let Some(opening_marker) = &line.opening_marker else {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                };
                let Some(hidden_prefix) = &line.hidden_prefix else {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                };
                if hidden_prefix.start != line.source.start
                    || hidden_prefix.end != line.content_source.start
                    || opening_marker.start < hidden_prefix.start
                    || opening_marker.end > hidden_prefix.end
                    || opening_marker.end - opening_marker.start != 1
                {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                }
            }
            M11BlockQuoteLineKind::LazyParagraphContinuation => {
                if line.opening_marker.is_some()
                    || line.hidden_prefix.is_some()
                    || line.content_source.start != line.source.start
                    || line.content_source_utf16.start != line.source_utf16.start
                {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                }
            }
            M11BlockQuoteLineKind::MarkedUnsupported => {
                return Err(M11CandidateDerivationError::ResultRangeMismatch);
            }
        }
        projected_utf8 = projected_utf8
            .checked_add(line.content_source.end - line.content_source.start)
            .and_then(|value| value.checked_add(line.line_ending.end - line.line_ending.start))
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        projected_utf16 = projected_utf16
            .checked_add(line.content_source_utf16.end - line.content_source_utf16.start)
            .and_then(|value| {
                value.checked_add(line.line_ending_utf16.end - line.line_ending_utf16.start)
            })
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        next_byte = line.source.end;
        next_utf16 = line.source_utf16.end;
    }
    if next_byte != source.end
        || next_utf16 != source_utf16.end
        || projected_utf8 != child.projected_utf8_length
        || projected_utf16 != child.projected_utf16_length
    {
        return Err(M11CandidateDerivationError::ResultRangeMismatch);
    }
    Ok(())
}

fn validate_exact_bullet_list(
    source: &std::ops::Range<u32>,
    source_utf16: &std::ops::Range<u32>,
    marker: u8,
    items: &[M11BulletListItemMapping],
    expected_projected_utf8: u32,
    expected_projected_utf16: u32,
    tight: bool,
) -> Result<(u32, u32), M11CandidateDerivationError> {
    if source.start >= source.end
        || source_utf16.start >= source_utf16.end
        || !matches!(marker, b'-' | b'+' | b'*')
        || items.is_empty()
        || !tight
    {
        return Err(M11CandidateDerivationError::ResultRangeMismatch);
    }
    let mut next_byte = source.start;
    let mut next_utf16 = source_utf16.start;
    let mut projected_utf8 = 0_u32;
    let mut projected_utf16 = 0_u32;
    let mut paragraph_count = 0_u32;
    let mut terminal_empty_relative_start = BULLET_LIST_ABSENT_TERMINAL_EMPTY;
    for (index, item) in items.iter().enumerate() {
        let ordinal =
            u32::try_from(index).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        let last = index + 1 == items.len();
        let prefix_source_bytes = item
            .hidden_prefix
            .end
            .checked_sub(item.hidden_prefix.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let prefix_source_utf16 = item
            .hidden_prefix_utf16
            .end
            .checked_sub(item.hidden_prefix_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let continuation_bytes = item
            .continuation_prefix_source
            .end
            .checked_sub(item.continuation_prefix_source.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let continuation_utf16 = item
            .continuation_prefix_source_utf16
            .end
            .checked_sub(item.continuation_prefix_source_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let bom_byte_delta = item
            .continuation_prefix_source
            .start
            .checked_sub(item.source.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let bom_utf16_delta = item
            .continuation_prefix_source_utf16
            .start
            .checked_sub(item.source_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let bom_prefix = (bom_byte_delta, bom_utf16_delta) == (3, 1);
        if item.ordinal != ordinal
            || item.marker != marker
            || item.source.start != next_byte
            || item.source_utf16.start != next_utf16
            || item.source.start >= item.source.end
            || item.source_utf16.start >= item.source_utf16.end
            || item.source.end > source.end
            || item.source_utf16.end > source_utf16.end
            || item.hidden_prefix.start != item.source.start
            || item.hidden_prefix_utf16.start != item.source_utf16.start
            || item.hidden_prefix.end != item.content_source.start
            || item.hidden_prefix_utf16.end != item.content_source_utf16.start
            || item.opening_marker.start < item.continuation_prefix_source.start
            || item.opening_marker.end > item.continuation_prefix_source.end
            || item.opening_marker.end - item.opening_marker.start != 1
            || item.continuation_prefix_source.end > item.hidden_prefix.end
            || item.continuation_prefix_source_utf16.end > item.hidden_prefix_utf16.end
            || !matches!((bom_byte_delta, bom_utf16_delta), (0, 0) | (3, 1))
            || bom_prefix && (index != 0 || item.source.start != 0)
            || prefix_source_bytes != prefix_source_utf16 + if bom_prefix { 2 } else { 0 }
            || continuation_bytes != continuation_utf16
            || item.content_source.start > item.content_source.end
            || item.content_source.end != item.line_ending.start
            || item.line_ending.end != item.source.end
            || item.content_source_utf16.start > item.content_source_utf16.end
            || item.content_source_utf16.end != item.line_ending_utf16.start
            || item.line_ending_utf16.end != item.source_utf16.end
            || item.line_ending.end - item.line_ending.start > 2
            || item.line_ending_utf16.end - item.line_ending_utf16.start
                != item.line_ending.end - item.line_ending.start
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }
        match &item.paragraph {
            Some(paragraph) => {
                if paragraph.source != item.content_source
                    || paragraph.source_utf16 != item.content_source_utf16
                    || paragraph.inline_source != item.content_source
                    || paragraph.inline_source_utf16 != item.content_source_utf16
                    || paragraph.source.start >= paragraph.source.end
                    || paragraph.source_utf16.start >= paragraph.source_utf16.end
                {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                }
                paragraph_count = paragraph_count
                    .checked_add(1)
                    .ok_or(M11CandidateDerivationError::MetricOverflow)?;
            }
            None => {
                if !last
                    || item.content_source.start != item.content_source.end
                    || item.content_source_utf16.start != item.content_source_utf16.end
                {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                }
                terminal_empty_relative_start = item
                    .source
                    .start
                    .checked_sub(source.start)
                    .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
            }
        }
        projected_utf8 = projected_utf8
            .checked_add(item.content_source.end - item.content_source.start)
            .and_then(|value| value.checked_add(item.line_ending.end - item.line_ending.start))
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        projected_utf16 = projected_utf16
            .checked_add(item.content_source_utf16.end - item.content_source_utf16.start)
            .and_then(|value| {
                value.checked_add(item.line_ending_utf16.end - item.line_ending_utf16.start)
            })
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        next_byte = item.source.end;
        next_utf16 = item.source_utf16.end;
    }
    if next_byte != source.end
        || next_utf16 != source_utf16.end
        || projected_utf8 != expected_projected_utf8
        || projected_utf16 != expected_projected_utf16
        || paragraph_count == 0
            && terminal_empty_relative_start == BULLET_LIST_ABSENT_TERMINAL_EMPTY
    {
        return Err(M11CandidateDerivationError::ResultRangeMismatch);
    }
    Ok((terminal_empty_relative_start, paragraph_count))
}

fn validate_exact_ordered_list(
    source: &std::ops::Range<u32>,
    source_utf16: &std::ops::Range<u32>,
    start: u32,
    delimiter: u8,
    items: &[M11OrderedListItemMapping],
    expected_projected_utf8: u32,
    expected_projected_utf16: u32,
    tight: bool,
) -> Result<(u32, u32), M11CandidateDerivationError> {
    if source.start >= source.end
        || source_utf16.start >= source_utf16.end
        || start > 999_999_999
        || !matches!(delimiter, b'.' | b')')
        || items.is_empty()
        || !tight
        || items.first().is_none_or(|item| item.marker_value != start)
    {
        return Err(M11CandidateDerivationError::ResultRangeMismatch);
    }
    let mut next_byte = source.start;
    let mut next_utf16 = source_utf16.start;
    let mut projected_utf8 = 0_u32;
    let mut projected_utf16 = 0_u32;
    let mut paragraph_count = 0_u32;
    let mut terminal_empty_relative_start = ORDERED_LIST_ABSENT_TERMINAL_EMPTY;
    for (index, item) in items.iter().enumerate() {
        let ordinal =
            u32::try_from(index).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
        let last = index + 1 == items.len();
        let prefix_source_bytes = item
            .hidden_prefix
            .end
            .checked_sub(item.hidden_prefix.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let prefix_source_utf16 = item
            .hidden_prefix_utf16
            .end
            .checked_sub(item.hidden_prefix_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let continuation_bytes = item
            .continuation_prefix_source
            .end
            .checked_sub(item.continuation_prefix_source.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let continuation_utf16 = item
            .continuation_prefix_source_utf16
            .end
            .checked_sub(item.continuation_prefix_source_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let opening_marker_bytes = item
            .opening_marker
            .end
            .checked_sub(item.opening_marker.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let bom_byte_delta = item
            .continuation_prefix_source
            .start
            .checked_sub(item.source.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let bom_utf16_delta = item
            .continuation_prefix_source_utf16
            .start
            .checked_sub(item.source_utf16.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
        let bom_prefix = (bom_byte_delta, bom_utf16_delta) == (3, 1);
        if item.ordinal != ordinal
            || item.delimiter != delimiter
            || item.marker_value > 999_999_999
            || item.source.start != next_byte
            || item.source_utf16.start != next_utf16
            || item.source.start >= item.source.end
            || item.source_utf16.start >= item.source_utf16.end
            || item.source.end > source.end
            || item.source_utf16.end > source_utf16.end
            || item.hidden_prefix.start != item.source.start
            || item.hidden_prefix_utf16.start != item.source_utf16.start
            || item.hidden_prefix.end != item.content_source.start
            || item.hidden_prefix_utf16.end != item.content_source_utf16.start
            || item.opening_marker.start < item.continuation_prefix_source.start
            || item.opening_marker.end > item.continuation_prefix_source.end
            || !(2..=10).contains(&opening_marker_bytes)
            || item.continuation_prefix_source.end > item.hidden_prefix.end
            || item.continuation_prefix_source_utf16.end > item.hidden_prefix_utf16.end
            || !matches!((bom_byte_delta, bom_utf16_delta), (0, 0) | (3, 1))
            || bom_prefix && (index != 0 || item.source.start != 0)
            || prefix_source_bytes != prefix_source_utf16 + if bom_prefix { 2 } else { 0 }
            || continuation_bytes != continuation_utf16
            || item.content_source.start > item.content_source.end
            || item.content_source.end != item.line_ending.start
            || item.line_ending.end != item.source.end
            || item.content_source_utf16.start > item.content_source_utf16.end
            || item.content_source_utf16.end != item.line_ending_utf16.start
            || item.line_ending_utf16.end != item.source_utf16.end
            || item.line_ending.end - item.line_ending.start > 2
            || item.line_ending_utf16.end - item.line_ending_utf16.start
                != item.line_ending.end - item.line_ending.start
        {
            return Err(M11CandidateDerivationError::ResultRangeMismatch);
        }
        match &item.paragraph {
            Some(paragraph) => {
                if paragraph.source != item.content_source
                    || paragraph.source_utf16 != item.content_source_utf16
                    || paragraph.inline_source != item.content_source
                    || paragraph.inline_source_utf16 != item.content_source_utf16
                    || paragraph.source.start >= paragraph.source.end
                    || paragraph.source_utf16.start >= paragraph.source_utf16.end
                {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                }
                paragraph_count = paragraph_count
                    .checked_add(1)
                    .ok_or(M11CandidateDerivationError::MetricOverflow)?;
            }
            None => {
                if !last
                    || item.content_source.start != item.content_source.end
                    || item.content_source_utf16.start != item.content_source_utf16.end
                {
                    return Err(M11CandidateDerivationError::ResultRangeMismatch);
                }
                terminal_empty_relative_start = item
                    .source
                    .start
                    .checked_sub(source.start)
                    .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?;
            }
        }
        projected_utf8 = projected_utf8
            .checked_add(item.content_source.end - item.content_source.start)
            .and_then(|value| value.checked_add(item.line_ending.end - item.line_ending.start))
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        projected_utf16 = projected_utf16
            .checked_add(item.content_source_utf16.end - item.content_source_utf16.start)
            .and_then(|value| {
                value.checked_add(item.line_ending_utf16.end - item.line_ending_utf16.start)
            })
            .ok_or(M11CandidateDerivationError::MetricOverflow)?;
        next_byte = item.source.end;
        next_utf16 = item.source_utf16.end;
    }
    if next_byte != source.end
        || next_utf16 != source_utf16.end
        || projected_utf8 != expected_projected_utf8
        || projected_utf16 != expected_projected_utf16
        || paragraph_count == 0
            && terminal_empty_relative_start == ORDERED_LIST_ABSENT_TERMINAL_EMPTY
    {
        return Err(M11CandidateDerivationError::ResultRangeMismatch);
    }
    Ok((terminal_empty_relative_start, paragraph_count))
}

fn range_len(range: &std::ops::Range<u32>) -> Result<usize, M11CandidateDerivationError> {
    usize::try_from(
        range
            .end
            .checked_sub(range.start)
            .ok_or(M11CandidateDerivationError::ResultRangeMismatch)?,
    )
    .map_err(|_| M11CandidateDerivationError::MetricOverflow)
}

fn encode_block_quote_green(
    source_bytes: u32,
    line_count: u32,
    child: &M11BlockQuoteParagraphMapping,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(BLOCK_QUOTE_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    // A quote paragraph is a sequence of per-line source runs, not one
    // contiguous visible span. The separately demanded path projection owns
    // those runs.
    push_range(&mut output, 0..0);
    push_u64(&mut output, BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION);
    push_u32(&mut output, line_count);
    push_u32(&mut output, child.line_indices.start);
    push_u32(
        &mut output,
        child.line_indices.end - child.line_indices.start,
    );
    push_u32(&mut output, child.projected_utf8_length);
    push_u32(&mut output, child.projected_utf16_length);
    push_u32(&mut output, 0);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_quote_projection(
    source_bytes: u32,
    line_count: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(BLOCK_QUOTE_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    push_u64(&mut output, u64::from(line_count));
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_bullet_list_green(
    source_bytes: u32,
    marker: u8,
    item_count: u32,
    terminal_empty_relative_start: u32,
    paragraph_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(BULLET_LIST_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    // A list is a sequence of item Paragraph runs. The separately demanded
    // projection owns those noncontiguous visible spans.
    push_range(&mut output, 0..0);
    let metadata = BULLET_LIST_EXACT_DISPOSITION
        | (u64::from(marker) << BULLET_LIST_MARKER_SHIFT)
        | BULLET_LIST_TIGHT_FLAG;
    push_u64(&mut output, metadata);
    push_u32(&mut output, item_count);
    push_u32(&mut output, terminal_empty_relative_start);
    push_u32(&mut output, paragraph_count);
    push_u32(&mut output, projected_utf8_length);
    push_u32(&mut output, projected_utf16_length);
    push_u32(&mut output, 0);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_bullet_list_projection(
    source_bytes: u32,
    item_count: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(BULLET_LIST_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    push_u64(&mut output, u64::from(item_count));
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_ordered_list_green(
    source_bytes: u32,
    start: u32,
    delimiter: u8,
    item_count: u32,
    terminal_empty_relative_start: u32,
    paragraph_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(ORDERED_LIST_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    // Ordered-list content is noncontiguous for the same reason as bullet-list
    // content. The selected-item sidecar owns exact marker and prefix spans.
    push_range(&mut output, 0..0);
    let metadata = ORDERED_LIST_EXACT_DISPOSITION
        | (u64::from(delimiter) << ORDERED_LIST_DELIMITER_SHIFT)
        | ORDERED_LIST_TIGHT_FLAG;
    push_u64(&mut output, metadata);
    push_u32(&mut output, item_count);
    push_u32(&mut output, terminal_empty_relative_start);
    push_u32(&mut output, paragraph_count);
    push_u32(&mut output, projected_utf8_length);
    push_u32(&mut output, projected_utf16_length);
    push_u32(&mut output, start);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_ordered_list_projection(
    source_bytes: u32,
    item_count: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(ORDERED_LIST_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    push_u64(&mut output, u64::from(item_count));
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_paragraph_green(
    source_bytes: u32,
    inline_relative: std::ops::Range<u32>,
    reference_definition_count: usize,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(1);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, inline_relative);
    push_u64(
        &mut output,
        u64::try_from(reference_definition_count)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
    );
    push_u32(&mut output, 0);
    push_u32(&mut output, 0);
    push_u64(&mut output, 0);
    push_u64(&mut output, 0);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_paragraph_projection(
    source_bytes: u32,
    inline_relative: std::ops::Range<u32>,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(1);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, inline_relative);
    push_u64(&mut output, 1);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_fenced_code_green(
    source_bytes: u32,
    body_relative: std::ops::Range<u32>,
    opening_marker_relative: std::ops::Range<u32>,
    raw_info_relative: std::ops::Range<u32>,
    closing_marker_relative: Option<std::ops::Range<u32>>,
    marker: u8,
    opening_indent: u8,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(FENCED_CODE_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, body_relative);
    let metadata = u64::from(marker)
        | (u64::from(opening_indent) << 8)
        | closing_marker_relative
            .as_ref()
            .map_or(0, |_| FENCED_CODE_CLOSED_FLAG);
    push_u64(&mut output, metadata);
    push_u32(&mut output, opening_marker_relative.start);
    push_u32(&mut output, opening_marker_relative.end);
    push_u32(&mut output, raw_info_relative.start);
    push_u32(&mut output, raw_info_relative.end);
    let closing = closing_marker_relative.unwrap_or(FENCED_CODE_ABSENT_CUT..FENCED_CODE_ABSENT_CUT);
    push_u32(&mut output, closing.start);
    push_u32(&mut output, closing.end);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_fenced_code_projection(
    source_bytes: u32,
    body_relative: std::ops::Range<u32>,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(FENCED_CODE_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, body_relative);
    push_u64(&mut output, 1);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_indented_code_green(
    source_bytes: u32,
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    terminal_eol_bytes: u32,
    has_bof_bom: bool,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(INDENTED_CODE_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    push_u64(
        &mut output,
        u64::from(INDENTED_CODE_DEINDENT_COLUMNS)
            | if has_bof_bom {
                INDENTED_CODE_BOF_BOM_FLAG
            } else {
                0
            },
    );
    push_u32(&mut output, line_count);
    push_u32(&mut output, projected_utf8_length);
    push_u32(&mut output, projected_utf16_length);
    push_u32(&mut output, terminal_eol_bytes);
    push_u64(&mut output, 0);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_indented_code_projection(
    source_bytes: u32,
    line_count: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(INDENTED_CODE_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    push_u64(&mut output, u64::from(line_count));
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn encode_block_atx_heading_green(
    source_bytes: u32,
    inline_relative: std::ops::Range<u32>,
    opening_marker_relative: std::ops::Range<u32>,
    closing_marker_relative: Option<std::ops::Range<u32>>,
    line_ending_relative: std::ops::Range<u32>,
    level: u8,
    opening_indent: u8,
    has_bof_bom: bool,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(ATX_HEADING_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, inline_relative);
    let metadata = u64::from(level)
        | closing_marker_relative
            .as_ref()
            .map_or(0, |_| ATX_HEADING_CLOSED_FLAG)
        | (u64::from(opening_indent) << ATX_HEADING_OPENING_INDENT_SHIFT)
        | if has_bof_bom {
            ATX_HEADING_BOF_BOM_FLAG
        } else {
            0
        };
    push_u64(&mut output, metadata);
    push_u32(&mut output, opening_marker_relative.start);
    push_u32(&mut output, opening_marker_relative.end);
    let closing = closing_marker_relative.unwrap_or(ATX_HEADING_ABSENT_CUT..ATX_HEADING_ABSENT_CUT);
    push_u32(&mut output, closing.start);
    push_u32(&mut output, closing.end);
    push_u32(&mut output, line_ending_relative.start);
    push_u32(&mut output, line_ending_relative.end);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_atx_heading_projection(
    source_bytes: u32,
    inline_relative: std::ops::Range<u32>,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(ATX_HEADING_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, inline_relative);
    push_u64(&mut output, 1);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn encode_block_setext_heading_green(
    source_bytes: u32,
    inline_relative: std::ops::Range<u32>,
    underline_marker_relative: std::ops::Range<u32>,
    underline_line_ending_relative: std::ops::Range<u32>,
    level: u8,
    opening_indent: u8,
    reference_definition_count: usize,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(SETEXT_HEADING_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, inline_relative);
    push_u64(
        &mut output,
        u64::from(level) | (u64::from(opening_indent) << SETEXT_HEADING_OPENING_INDENT_SHIFT),
    );
    push_u32(&mut output, underline_marker_relative.start);
    push_u32(&mut output, underline_marker_relative.end);
    push_u32(&mut output, underline_line_ending_relative.start);
    push_u32(&mut output, underline_line_ending_relative.end);
    push_u64(
        &mut output,
        u64::try_from(reference_definition_count)
            .map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
    );
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_setext_heading_projection(
    source_bytes: u32,
    inline_relative: std::ops::Range<u32>,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(SETEXT_HEADING_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, inline_relative);
    push_u64(&mut output, 1);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn encode_block_thematic_break_green(
    source_bytes: u32,
    marker_envelope_relative: std::ops::Range<u32>,
    line_ending_relative: std::ops::Range<u32>,
    marker: u8,
    marker_count: u32,
    opening_indent: u8,
    has_bof_bom: bool,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(THEMATIC_BREAK_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    let metadata = u64::from(marker)
        | (u64::from(opening_indent) << THEMATIC_BREAK_OPENING_INDENT_SHIFT)
        | if has_bof_bom {
            THEMATIC_BREAK_BOF_BOM_FLAG
        } else {
            0
        };
    push_u64(&mut output, metadata);
    push_u32(&mut output, marker_envelope_relative.start);
    push_u32(&mut output, marker_envelope_relative.end);
    push_u32(&mut output, line_ending_relative.start);
    push_u32(&mut output, line_ending_relative.end);
    push_u64(&mut output, u64::from(marker_count));
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

fn encode_block_thematic_break_projection(
    source_bytes: u32,
) -> Result<M11BlockRoleRecord, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(THEMATIC_BREAK_ROLE_VARIANT);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..0);
    push_u64(&mut output, 0);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    M11BlockRoleRecord::new(&output).map_err(Into::into)
}

const fn encode_block_unsupported_reason(reason: M11UnknownReason) -> u32 {
    match reason {
        M11UnknownReason::BlankBoundary => 0x0001_0000,
        M11UnknownReason::UnsupportedOpener(opener) => {
            let opener = match opener {
                M11UnsupportedOpener::BlockQuote => 1,
                M11UnsupportedOpener::AtxHeading => 2,
                M11UnsupportedOpener::FencedCode => 3,
                M11UnsupportedOpener::HtmlBlock => 4,
                M11UnsupportedOpener::SetextHeading => 5,
                M11UnsupportedOpener::ThematicBreak => 6,
                M11UnsupportedOpener::List => 7,
                M11UnsupportedOpener::IndentedCode => 8,
                M11UnsupportedOpener::TableCandidate => 9,
            };
            0x0002_0000 | opener
        }
        M11UnknownReason::UnsupportedList(reason) => {
            0x0004_0000 | encode_list_unsupported_reason_detail(reason)
        }
    }
}

const fn encode_list_unsupported_reason_detail(reason: M11ListUnsupportedReason) -> u32 {
    match reason {
        M11ListUnsupportedReason::Ordered => 1,
        M11ListUnsupportedReason::Task => 2,
        M11ListUnsupportedReason::LazyOrMultiline => 3,
        M11ListUnsupportedReason::Loose => 4,
        M11ListUnsupportedReason::Nested => 5,
        M11ListUnsupportedReason::BlockChild => 6,
        M11ListUnsupportedReason::TabPadded => 7,
        M11ListUnsupportedReason::ExcessivePadding => 8,
        M11ListUnsupportedReason::NonTerminalEmptyItem => 9,
    }
}

const fn encode_block_quote_unsupported_reason(reason: M11BlockQuoteUnsupportedReason) -> u32 {
    let detail = match reason {
        M11BlockQuoteUnsupportedReason::MarkerOnlyOrBlank => 1,
        M11BlockQuoteUnsupportedReason::PartialTabMarker => 2,
        M11BlockQuoteUnsupportedReason::NestedBlockQuote => 3,
        M11BlockQuoteUnsupportedReason::AtxHeading => 4,
        M11BlockQuoteUnsupportedReason::FencedCode => 5,
        M11BlockQuoteUnsupportedReason::HtmlBlock => 6,
        M11BlockQuoteUnsupportedReason::SetextHeading => 7,
        M11BlockQuoteUnsupportedReason::ThematicBreak => 8,
        M11BlockQuoteUnsupportedReason::List => 9,
        M11BlockQuoteUnsupportedReason::IndentedCode => 10,
        M11BlockQuoteUnsupportedReason::TableCandidate => 11,
        M11BlockQuoteUnsupportedReason::PotentialReferenceDefinition => 12,
        M11BlockQuoteUnsupportedReason::MultipleParagraphChildren => 13,
    };
    0x0003_0000 | detail
}

fn encode_green(result: &M11CleanDocumentResult) -> Result<Box<[u8]>, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_GREEN_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(GREEN_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    let source = result.source_range();
    let (variant, visible, definition_count, reason_tag, detail) = match result.outcome() {
        M11CleanDocumentOutcome::Empty { .. } => (0, 0..0, result.definition_count(), 0, (0, 0, 0)),
        M11CleanDocumentOutcome::Paragraph { visible_source, .. } => (
            1,
            visible_source.clone(),
            result.definition_count(),
            0,
            (0, 0, 0),
        ),
        M11CleanDocumentOutcome::Unknown { reason } => {
            let (tag, detail) = encode_unknown_reason(*reason);
            (2, 0..0, 0, tag, detail)
        }
        M11CleanDocumentOutcome::Segmented { .. } => (2, 0..0, 0, 0, (0, 0, 0)),
    };
    output.push(variant);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, source);
    push_range(&mut output, visible);
    push_u64(
        &mut output,
        u64::try_from(definition_count).map_err(|_| M11CandidateDerivationError::MetricOverflow)?,
    );
    push_u32(&mut output, reason_tag);
    push_u32(&mut output, detail.0);
    push_u64(&mut output, detail.1);
    push_u64(&mut output, detail.2);
    debug_assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
    Ok(output.into_boxed_slice())
}

fn encode_projection(
    result: &M11CleanDocumentResult,
) -> Result<Box<[u8]>, M11CandidateDerivationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    let source = result.source_range();
    let (variant, projected, runs) = match result.outcome() {
        M11CleanDocumentOutcome::Empty { .. } => (0, 0..0, 0),
        M11CleanDocumentOutcome::Paragraph { visible_source, .. } => (1, visible_source.clone(), 1),
        M11CleanDocumentOutcome::Segmented { .. } => (2, source.clone(), 1),
        M11CleanDocumentOutcome::Unknown { .. } => (2, source.clone(), 1),
    };
    output.push(variant);
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, source);
    push_range(&mut output, projected);
    push_u64(&mut output, runs);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    Ok(output.into_boxed_slice())
}

/// Stable coarse Projection companion for the canonical recursive Green role.
///
/// Recursive Green owns all structural and nested source geometry. Projection
/// schema v1 remains only a whole-document source-backed fallback, so clean
/// initialization and local exact updates publish identical semantics without
/// requiring a whole-document parse result after every edit.
fn encode_recursive_green_projection(
    source: SourceVersion,
) -> Result<Box<[u8]>, M11CandidateDerivationError> {
    let source_bytes = u32::try_from(source.byte_len())
        .map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(M11_PROJECTION_RECORD_BYTES)
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    output.extend_from_slice(PROJECTION_MAGIC);
    push_u32(&mut output, ROLE_SCHEMA_V1);
    output.push(2); // segmented/whole-source fallback
    output.extend_from_slice(&[0; 3]);
    push_range(&mut output, 0..source_bytes);
    push_range(&mut output, 0..source_bytes);
    push_u64(&mut output, 1);
    debug_assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
    Ok(output.into_boxed_slice())
}

fn encode_unknown_reason(reason: M11UnknownReason) -> (u32, (u32, u64, u64)) {
    match reason {
        M11UnknownReason::BlankBoundary => (1, (0, 0, 0)),
        M11UnknownReason::UnsupportedOpener(opener) => {
            let opener = match opener {
                M11UnsupportedOpener::BlockQuote => 1,
                M11UnsupportedOpener::AtxHeading => 2,
                M11UnsupportedOpener::FencedCode => 3,
                M11UnsupportedOpener::HtmlBlock => 4,
                M11UnsupportedOpener::SetextHeading => 5,
                M11UnsupportedOpener::ThematicBreak => 6,
                M11UnsupportedOpener::List => 7,
                M11UnsupportedOpener::IndentedCode => 8,
                M11UnsupportedOpener::TableCandidate => 9,
            };
            (2, (opener, 0, 0))
        }
        M11UnknownReason::UnsupportedList(reason) => {
            (3, (encode_list_unsupported_reason_detail(reason), 0, 0))
        }
    }
}

fn result_definitions(result: &M11CleanDocumentResult) -> &[crate::M11ReferenceDefinition] {
    result.definitions()
}

fn derive_reference_plans(
    definitions: &[crate::M11ReferenceDefinition],
    lease: &SourceSnapshotLease,
) -> Result<VecDeque<CookReferencePlan>, M11CandidateDerivationError> {
    let mut plans = VecDeque::new();
    plans
        .try_reserve_exact(definitions.len())
        .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
    for definition in definitions {
        let (source, _) = map_reference_range(lease, &definition.source)?;
        let (label_source, _) = map_reference_range(lease, &definition.label_source)?;
        let (destination_source, destination_bytes) =
            map_reference_range(lease, &definition.destination_source)?;
        let (title_source, title_bytes) = match &definition.title_source {
            Some(range) => {
                let (engine, bytes) = map_reference_range(lease, range)?;
                (Some(engine), Some(bytes))
            }
            None => (None, None),
        };
        let mut normalized_label = Vec::new();
        normalized_label
            .try_reserve_exact(definition.normalized_label.len())
            .map_err(|_| M11CandidateDerivationError::AllocationFailed)?;
        normalized_label.extend_from_slice(definition.normalized_label.as_bytes());
        plans.push_back(CookReferencePlan {
            source,
            label_source,
            destination_source,
            title_source,
            destination_bytes,
            title_bytes,
            normalized_label: normalized_label.into_boxed_slice(),
        });
    }
    Ok(plans)
}

fn map_reference_range(
    lease: &SourceSnapshotLease,
    range: &std::ops::Range<u32>,
) -> Result<(M11ReferenceRange, std::ops::Range<usize>), M11CandidateDerivationError> {
    let start =
        usize::try_from(range.start).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let end =
        usize::try_from(range.end).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let utf16_start = lease
        .utf16_offset_for_byte(start)
        .map_err(ReferenceCookError::Source)?;
    let utf16_end = lease
        .utf16_offset_for_byte(end)
        .map_err(ReferenceCookError::Source)?;
    let utf16_start =
        u64::try_from(utf16_start).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    let utf16_end =
        u64::try_from(utf16_end).map_err(|_| M11CandidateDerivationError::MetricOverflow)?;
    Ok((
        M11ReferenceRange::new(
            u64::from(range.start)..u64::from(range.end),
            utf16_start..utf16_end,
        ),
        start..end,
    ))
}

fn push_range(output: &mut Vec<u8>, range: std::ops::Range<u32>) {
    push_u64(output, u64::from(range.start));
    push_u64(output, u64::from(range.end));
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod retained_inline_range_tests {
    use super::*;
    use crate::inline_projection_job::{
        M11InlineProjectionJob, M11InlineProjectionJobPollStatus, M11InlineProjectionPublication,
    };
    use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
    use flark_engine::parser_internal::{
        M11InlineProjectionCursorPoll, M11OwnedSnapshotPoll, M11RetainedBlockVisitDisposition,
        M11SnapshotFrameKind, M11_MAX_ROLE_RECORDS,
    };
    use flark_engine::{
        ArenaLimits, DocumentRuntimeConfig, RuntimeSourceFactsPoll, SourceBoundaryAffinity,
        SourceFactsRootLimits,
    };

    const TEST_PROFILE: u64 = 0x1703;
    const PARAGRAPH_COUNT: usize = 24;
    const STRUCTURAL_ENTRY_COUNT: u64 = PARAGRAPH_COUNT as u64 * 2 - 1;
    const POLL_FUEL: usize = 8;

    fn binding() -> M11ParserBinding {
        M11ParserBinding::current(
            ParserProfileId::new(TEST_PROFILE).expect("nonzero parser profile"),
        )
    }

    fn parse(runtime: &DocumentRuntime) -> M11CleanDocumentResult {
        let mut job =
            M11CleanParseJob::new(runtime.snapshot_current_source().expect("parse lease"))
                .expect("clean parse job");
        loop {
            match job.poll(64).expect("clean parse poll") {
                M11CleanParsePoll::Pending { transitions } => {
                    assert!(transitions <= 64);
                }
                M11CleanParsePoll::Complete {
                    transitions,
                    result,
                } => {
                    assert!(transitions <= 64);
                    return result;
                }
            }
        }
    }

    fn prepare_source_facts(runtime: &mut DocumentRuntime) {
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        let expected = runtime
            .begin_source_facts(
                scan_profile,
                binding().syntax_profile(),
                SourceFactsRootLimits::default(),
            )
            .expect("begin source facts");
        loop {
            match runtime.poll_source_facts(17, 3).expect("source facts poll") {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { completion, .. } => {
                    assert_eq!(completion.source(), expected);
                    break;
                }
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean source-fact scan reported incremental progress")
                }
            }
        }
    }

    fn retain_segmented_candidate(
        runtime: &mut DocumentRuntime,
    ) -> M11RetainedCandidatePublication {
        prepare_source_facts(runtime);
        let result = parse(runtime);
        assert!(result.sole_paragraph().is_none());
        let certified = runtime.take_certified_source().expect("certified source");
        let candidate =
            M11ParserCandidate::derive_segmented(certified, result).expect("segmented candidate");
        let mut writer = candidate
            .into_writer(runtime, [0x91; 16], [0x92; 16], 1)
            .expect("candidate writer");
        let publication = loop {
            match writer.poll(runtime, 1).expect("candidate writer poll") {
                M11ParserCandidateWriterPoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                }
                M11ParserCandidateWriterPoll::Published {
                    transitions,
                    publication,
                } => {
                    assert!(transitions <= 1);
                    break publication;
                }
            }
        };
        drop(writer);
        let mut stream = publication
            .into_snapshot_stream(runtime)
            .expect("owned snapshot stream");
        assert_eq!(
            stream.begin_frame().expect("snapshot Begin").kind,
            M11SnapshotFrameKind::Begin
        );
        loop {
            match stream.poll(runtime, 17).expect("snapshot traversal") {
                M11OwnedSnapshotPoll::Pending { transitions } => {
                    assert!(transitions <= 17);
                }
                M11OwnedSnapshotPoll::Frame { transitions, frame } => {
                    assert!(transitions <= 17);
                    if frame.kind == M11SnapshotFrameKind::End {
                        break;
                    }
                }
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full segmented snapshot requested exact-base replay")
                }
            }
        }
        stream
            .into_retained_publication(runtime)
            .expect("retained candidate publication")
    }

    fn close_retained(
        retained: &mut M11RetainedCandidatePublication,
        runtime: &mut DocumentRuntime,
    ) {
        retained.begin_close(runtime).expect("begin retained close");
        while !retained.poll_close(runtime, 17).expect("retained close") {}
    }

    fn release_root(root: &mut M11InlineProjectionRoot, runtime: &mut DocumentRuntime) {
        root.begin_release(runtime).expect("begin root release");
        loop {
            let poll = root.poll_release(runtime, 1).expect("root release poll");
            assert!(poll.receipt().transitions <= 1);
            if poll.complete() {
                break;
            }
        }
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("runtime close").complete {}
        let metrics = runtime.arena_metrics();
        assert_eq!(metrics.reserved_external_payload_bytes, 0);
        assert_eq!(metrics.resident_nodes, 0);
        assert_eq!(metrics.live_builds, 0);
    }

    #[test]
    fn retained_inline_range_limits_reject_every_zero_dimension() {
        assert!(M11PublishedInlineRangeLimits::new(0, 1, 1, 1).is_none());
        assert!(M11PublishedInlineRangeLimits::new(1, 0, 1, 1).is_none());
        assert!(M11PublishedInlineRangeLimits::new(1, 1, 0, 1).is_none());
        assert!(M11PublishedInlineRangeLimits::new(1, 1, 1, 0).is_none());
        assert_eq!(
            M11PublishedInlineRangeLimits::new(47, 2, 24, 4_096)
                .expect("all dimensions are nonzero")
                .maximum_inline_leaves(),
            24
        );
    }

    #[test]
    fn retained_inline_range_stops_at_exact_end_cut_across_leaf_and_work_limits() {
        let mut source = String::new();
        let mut paragraph_starts = Vec::with_capacity(PARAGRAPH_COUNT);
        for ordinal in 0..PARAGRAPH_COUNT {
            if ordinal != 0 {
                source.push_str("\n\n");
            }
            paragraph_starts.push(source.len());
            source.push_str(&format!("paragraph {ordinal:02}"));
        }
        let mut runtime = DocumentRuntime::new(
            &source,
            DocumentRuntimeConfig {
                arena_limits: ArenaLimits {
                    max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                    max_live_payload_bytes: 64 * 1024 * 1024,
                    max_children_per_node: M11_MAX_ROLE_RECORDS,
                },
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect("segmented runtime");
        let mut retained = retain_segmented_candidate(&mut runtime);
        let end_at = |byte_offset: usize| {
            M11PublishedInlineRangeEnd::new(
                byte_offset as u64,
                source[..byte_offset].encode_utf16().count() as u64,
            )
        };
        let limits = |structural: u32, pages: u32, leaves: u32| {
            M11PublishedInlineRangeLimits::new(structural, pages, leaves, source.len() as u64)
                .expect("nonzero limits")
        };

        let bof_end = paragraph_starts[12];
        let bof = resolve_m11_published_inline_leaf_range(
            &runtime,
            &retained,
            M11RetainedBlockVisitStart::new(0, 0, 0),
            end_at(bof_end),
            limits(24, 2, 12),
        )
        .expect("BOF range ending after a blank entry");
        assert_eq!(bof.len(), 12);
        assert_eq!(bof.receipt().visited_entries(), 24);
        assert_eq!(bof.receipt().next_entry_ordinal(), 24);
        assert_eq!(bof.receipt().next_byte_offset(), bof_end as u64);
        drop(bof);

        let twelfth_paragraph_end = paragraph_starts[12] - 1;
        let exact_leaf_end = resolve_m11_published_inline_leaf_range(
            &runtime,
            &retained,
            M11RetainedBlockVisitStart::new(0, 0, 0),
            end_at(twelfth_paragraph_end),
            limits(23, 2, 12),
        )
        .expect("the admitted final inline leaf may also be the exact end");
        assert_eq!(exact_leaf_end.len(), 12);
        assert_eq!(exact_leaf_end.receipt().visited_entries(), 23);
        drop(exact_leaf_end);

        let thirteenth_paragraph_end = paragraph_starts[13] - 1;
        assert!(matches!(
            resolve_m11_published_inline_leaf_range(
                &runtime,
                &retained,
                M11RetainedBlockVisitStart::new(0, 0, 0),
                end_at(thirteenth_paragraph_end),
                limits(25, 2, 12),
            ),
            Err(M11PublishedInlineRangeError::InlineLeafLimitExceeded {
                maximum: 12,
                required_through_leaf: 13,
            })
        ));

        assert!(matches!(
            resolve_m11_published_inline_leaf_range(
                &runtime,
                &retained,
                M11RetainedBlockVisitStart::new(0, 0, 0),
                end_at(bof_end),
                limits(23, 2, 12),
            ),
            Err(M11PublishedInlineRangeError::StructuralEntryLimitExceeded { maximum: 23 })
        ));

        assert!(matches!(
            resolve_m11_published_inline_leaf_range(
                &runtime,
                &retained,
                M11RetainedBlockVisitStart::new(0, 0, 0),
                end_at(source.len()),
                limits(STRUCTURAL_ENTRY_COUNT as u32, 1, PARAGRAPH_COUNT as u32),
            ),
            Err(M11PublishedInlineRangeError::StoragePageLimitExceeded { maximum: 1 })
        ));

        assert!(matches!(
            resolve_m11_published_inline_leaf_range(
                &runtime,
                &retained,
                M11RetainedBlockVisitStart::new(0, 0, 0),
                M11PublishedInlineRangeEnd::new(1, 1),
                limits(1, 1, 1),
            ),
            Err(M11PublishedInlineRangeError::EndCutMismatch { .. })
        ));
        assert!(matches!(
            resolve_m11_published_inline_leaf_range(
                &runtime,
                &retained,
                M11RetainedBlockVisitStart::new(0, 0, 0),
                M11PublishedInlineRangeEnd::new(
                    bof_end as u64,
                    source[..bof_end].encode_utf16().count() as u64 - 1,
                ),
                limits(24, 2, 12),
            ),
            Err(M11PublishedInlineRangeError::EndCutMismatch { .. })
        ));

        let middle_start = paragraph_starts[6];
        let middle_end = paragraph_starts[18];
        let middle = resolve_m11_published_inline_leaf_range(
            &runtime,
            &retained,
            M11RetainedBlockVisitStart::new(
                12,
                middle_start as u64,
                source[..middle_start].encode_utf16().count() as u64,
            ),
            end_at(middle_end),
            limits(24, 2, 12),
        )
        .expect("middle exact range");
        assert_eq!(middle.len(), 12);
        assert_eq!(middle.receipt().visited_entries(), 24);
        assert_eq!(middle.receipt().next_entry_ordinal(), 36);
        assert_eq!(middle.receipt().next_byte_offset(), middle_end as u64);
        drop(middle);

        close_retained(&mut retained, &mut runtime);
        drop(retained);
        close_runtime(runtime);
    }

    #[test]
    fn one_retained_walk_drives_twenty_four_bounded_inline_jobs() {
        let mut source = String::new();
        let mut paragraph_starts = Vec::with_capacity(PARAGRAPH_COUNT);
        for ordinal in 0..PARAGRAPH_COUNT {
            if ordinal != 0 {
                source.push_str("\n\n");
            }
            paragraph_starts.push(source.len());
            source.push_str(&format!(
                "**bold{ordinal:02}** *em{ordinal:02}* `code{ordinal:02}`"
            ));
        }
        let mut runtime = DocumentRuntime::new(
            &source,
            DocumentRuntimeConfig {
                arena_limits: ArenaLimits {
                    max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                    max_live_payload_bytes: 64 * 1024 * 1024,
                    max_children_per_node: M11_MAX_ROLE_RECORDS,
                },
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect("segmented runtime");
        let mut retained = retain_segmented_candidate(&mut runtime);
        let retained_descriptor = retained.descriptor(&runtime).expect("retained descriptor");

        let mut point_node_headers = 0_u64;
        let mut point_entries_authenticated = 0_u64;
        for &byte_start in &paragraph_starts {
            let location = retained
                .locate_block_point(
                    &runtime,
                    M11BlockSequencePoint::new(
                        byte_start,
                        source[..byte_start].encode_utf16().count(),
                        SourceBoundaryAffinity::After,
                    ),
                )
                .expect("point query")
                .expect("paragraph location");
            assert_eq!(
                location.entry().kind(),
                M11BlockSequenceEntryKind::Paragraph
            );
            point_node_headers += location.receipt().node_headers_decoded();
            point_entries_authenticated += location.receipt().entries_authenticated();
        }
        let source_end = M11PublishedInlineRangeEnd::new(
            u64::try_from(source.len()).expect("small source"),
            u64::try_from(source.encode_utf16().count()).expect("small source"),
        );

        let undersized_limits = M11PublishedInlineRangeLimits::new(
            u32::try_from(STRUCTURAL_ENTRY_COUNT).expect("small structural cap"),
            2,
            u32::try_from(PARAGRAPH_COUNT).expect("small leaf cap"),
            1,
        )
        .expect("nonzero undersized limits");
        assert!(matches!(
            resolve_m11_published_inline_leaf_range(
                &runtime,
                &retained,
                M11RetainedBlockVisitStart::new(0, 0, 0),
                source_end,
                undersized_limits,
            ),
            Err(M11PublishedInlineRangeError::InlineSourceByteLimitExceeded {
                maximum: 1,
                required_through_leaf,
            }) if required_through_leaf > 1
        ));

        let limits = M11PublishedInlineRangeLimits::new(
            u32::try_from(STRUCTURAL_ENTRY_COUNT).expect("small structural cap"),
            2,
            u32::try_from(PARAGRAPH_COUNT).expect("small leaf cap"),
            u64::try_from(source.len()).expect("small source"),
        )
        .expect("nonzero range limits");
        let batch = resolve_m11_published_inline_leaf_range(
            &runtime,
            &retained,
            M11RetainedBlockVisitStart::new(0, 0, 0),
            source_end,
            limits,
        )
        .expect("one retained range batch");
        assert_eq!(batch.descriptor(), retained_descriptor);
        assert_eq!(batch.limits(), limits);
        assert_eq!(batch.len(), PARAGRAPH_COUNT);
        assert!(!batch.is_empty());
        let receipt = batch.receipt();
        assert_eq!(receipt.visited_entries(), STRUCTURAL_ENTRY_COUNT);
        assert!(receipt.storage_pages_visited() <= 2);
        assert_eq!(
            receipt.disposition(),
            M11RetainedBlockVisitDisposition::VisitorStopped
        );
        assert_eq!(receipt.next_entry_ordinal(), STRUCTURAL_ENTRY_COUNT);
        assert_eq!(
            receipt.next_byte_offset(),
            u64::try_from(source.len()).expect("small source")
        );
        assert_eq!(
            receipt.next_utf16_offset(),
            u64::try_from(source.encode_utf16().count()).expect("small source")
        );
        assert!(
            receipt.node_headers_decoded() < point_node_headers,
            "one range seek should decode fewer tree headers than 24 point seeks"
        );
        assert!(
            receipt.entries_authenticated() < point_entries_authenticated,
            "one range walk should authenticate fewer repeated packed entries"
        );
        assert!(batch.total_inline_source_bytes() <= source.len() as u64);
        let expected_inline_source_bytes = batch.total_inline_source_bytes();
        let fences = batch.into_fences();
        assert_eq!(fences.len(), PARAGRAPH_COUNT);

        let mut total_initial_lexical_bytes = 0_u64;
        let mut total_facts = 0_usize;
        for (index, fence) in fences.into_iter().enumerate() {
            assert_eq!(fence.source(), runtime.current_source_version().unwrap());
            assert_eq!(fence.kind(), M11BlockSequenceEntryKind::Paragraph);
            assert_eq!(fence.entry_ordinal(), (index * 2) as u64);
            assert_eq!(
                fence.block_source_range().start,
                paragraph_starts[index] as u32
            );
            assert_eq!(
                fence.block_source_utf16_range().start,
                source[..paragraph_starts[index]].encode_utf16().count() as u32
            );
            let inline_range = fence.inline_source_range();
            let inline_utf16_range = fence.inline_source_utf16_range();
            assert!(inline_range.start < inline_range.end);
            assert!(inline_utf16_range.start < inline_utf16_range.end);

            let mut job =
                M11InlineProjectionJob::new_for_published_inline_range_leaf(&runtime, fence)
                    .expect("range inline job");
            loop {
                let poll = job.poll(&mut runtime, POLL_FUEL).expect("inline poll");
                assert!(poll.transitions() <= POLL_FUEL);
                if poll.status() == M11InlineProjectionJobPollStatus::Complete {
                    break;
                }
            }
            total_initial_lexical_bytes += job.initial_lexical_source_bytes_read();
            let output = job.take_output().expect("inline output");
            let (_, _, profile, authority, publication) =
                output.into_publication_parts().into_parts();
            assert_eq!(profile, binding().syntax_profile());
            let M11InlineProjectionPublication::Authoritative(mut root) = publication else {
                panic!("styled paragraph must produce authoritative inline facts");
            };
            let mut cursor = root
                .cursor(
                    &runtime,
                    runtime.current_source_version().unwrap(),
                    binding().syntax_profile(),
                )
                .expect("inline cursor");
            let mut paragraph_facts = 0_usize;
            loop {
                match cursor.poll(&runtime).expect("cursor poll") {
                    M11InlineProjectionCursorPoll::Pending { .. } => {}
                    M11InlineProjectionCursorPoll::Fact { .. } => {
                        paragraph_facts += 1;
                    }
                    M11InlineProjectionCursorPoll::Complete { .. } => break,
                }
            }
            assert_eq!(paragraph_facts, 3);
            total_facts += paragraph_facts;
            drop(cursor);
            release_root(&mut root, &mut runtime);
            drop(root);
            drop(authority);
            drop(job);
        }
        assert_eq!(total_facts, PARAGRAPH_COUNT * 3);
        assert_eq!(total_initial_lexical_bytes, expected_inline_source_bytes);

        close_retained(&mut retained, &mut runtime);
        drop(retained);
        close_runtime(runtime);
    }
}

#[cfg(test)]
mod bullet_list_local_delta_publication_tests {
    use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, SourceSnapshotLease};

    use crate::M11BulletListLocalDeltaTerminal;

    use super::{
        read_record_u32, read_record_u64, M11CandidateDerivationError,
        M11ExactSegmentedCandidateInput, BULLET_LIST_ABSENT_TERMINAL_EMPTY,
        BULLET_LIST_EXACT_DISPOSITION, BULLET_LIST_MARKER_SHIFT, BULLET_LIST_ROLE_VARIANT,
        BULLET_LIST_TIGHT_FLAG,
    };

    const PREFIX: &str = "before\n";
    const LIST: &str = "- alpha\n- café 😀\n- omega\n";
    const PROJECTED: &str = "alpha\ncafé 😀\nomega\n";
    const SUFFIX: &str = "after\n";
    const ENTRY_ORDINAL: u64 = 7;

    fn utf16_len(value: &str) -> usize {
        value.encode_utf16().count()
    }

    fn fixture() -> (
        DocumentRuntime,
        SourceSnapshotLease,
        M11BulletListLocalDeltaTerminal,
    ) {
        let document = format!("{PREFIX}{LIST}{SUFFIX}");
        let runtime = DocumentRuntime::new(&document, DocumentRuntimeConfig::default())
            .expect("document runtime");
        let source = runtime.current_source_version().expect("source version");
        let lease = runtime.snapshot_current_source().expect("source lease");
        let list_start = u32::try_from(PREFIX.len()).expect("small byte start");
        let list_end = u32::try_from(PREFIX.len() + LIST.len()).expect("small byte end");
        let list_utf16_start = u32::try_from(utf16_len(PREFIX)).expect("small UTF-16 start");
        let list_utf16_end =
            u32::try_from(utf16_len(PREFIX) + utf16_len(LIST)).expect("small UTF-16 end");
        (
            runtime,
            lease,
            M11BulletListLocalDeltaTerminal {
                source,
                list_source: list_start..list_end,
                list_source_utf16: list_utf16_start..list_utf16_end,
                block_entry_ordinal: ENTRY_ORDINAL,
                marker: b'-',
                item_count: 3,
                paragraph_count: 3,
                terminal_empty_relative_start: None,
                projected_utf8_length: u32::try_from(PROJECTED.len())
                    .expect("small projected bytes"),
                projected_utf16_length: u32::try_from(utf16_len(PROJECTED))
                    .expect("small projected UTF-16"),
            },
        )
    }

    fn close(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close poll").complete {}
    }

    fn assert_summary_rejected(mutate: impl FnOnce(&mut M11BulletListLocalDeltaTerminal)) {
        let (runtime, lease, mut terminal) = fixture();
        mutate(&mut terminal);
        assert!(matches!(
            M11ExactSegmentedCandidateInput::from_bullet_list_local_delta(lease, &terminal),
            Err(M11CandidateDerivationError::ResultRangeMismatch)
        ));
        close(runtime);
    }

    #[test]
    fn authenticated_unicode_summary_encodes_one_private_variant_9_replacement() {
        let (runtime, lease, terminal) = fixture();
        let input = M11ExactSegmentedCandidateInput::from_bullet_list_local_delta(lease, &terminal)
            .expect("authenticated compact list replacement");

        assert_eq!(input.source(), terminal.source);
        assert!(input.leaves.is_empty());
        assert!(input.leaves_are_replacement);
        let selection = input.block_splice.as_ref().expect("block splice");
        assert_eq!(
            selection.base_entry_range(),
            ENTRY_ORDINAL..ENTRY_ORDINAL + 1
        );
        assert_eq!(
            selection.target_entry_range(),
            ENTRY_ORDINAL..ENTRY_ORDINAL + 1
        );
        assert_eq!(input.prepared_replacement_entries.len(), 1);
        let entry = &input.prepared_replacement_entries[0];
        assert_eq!(
            entry.source_byte_len(),
            u64::try_from(LIST.len()).expect("small list bytes")
        );
        assert_eq!(
            entry.source_utf16_len(),
            u64::try_from(utf16_len(LIST)).expect("small list UTF-16")
        );
        assert_eq!(entry.reference_definition_count(), 0);

        let green = entry.green().expect("variant-9 Green").as_bytes();
        let projection = entry.projection().expect("variant-9 Projection").as_bytes();
        assert_eq!(green[12], BULLET_LIST_ROLE_VARIANT);
        assert_eq!(projection[12], BULLET_LIST_ROLE_VARIANT);
        assert_eq!(
            read_record_u64(green, 48).expect("metadata"),
            BULLET_LIST_EXACT_DISPOSITION
                | (u64::from(b'-') << BULLET_LIST_MARKER_SHIFT)
                | BULLET_LIST_TIGHT_FLAG
        );
        assert_eq!(read_record_u32(green, 56).expect("item count"), 3);
        assert_eq!(
            read_record_u32(green, 60).expect("terminal empty"),
            BULLET_LIST_ABSENT_TERMINAL_EMPTY
        );
        assert_eq!(read_record_u32(green, 64).expect("paragraph count"), 3);
        assert_eq!(
            read_record_u32(green, 68).expect("projected bytes"),
            u32::try_from(PROJECTED.len()).expect("small projected bytes")
        );
        assert_eq!(
            read_record_u32(green, 72).expect("projected UTF-16"),
            u32::try_from(utf16_len(PROJECTED)).expect("small projected UTF-16")
        );
        assert_eq!(
            read_record_u64(projection, 48).expect("projection item count"),
            3
        );

        drop(input);
        close(runtime);
    }

    #[test]
    fn foreign_source_authority_is_rejected_before_summary_encoding() {
        let (mut runtime, lease, mut terminal) = fixture();
        let target = runtime
            .apply_edit(terminal.source, 0..0, "x")
            .expect("advance source")
            .source()
            .current();
        terminal.source = target;

        assert!(matches!(
            M11ExactSegmentedCandidateInput::from_bullet_list_local_delta(lease, &terminal),
            Err(M11CandidateDerivationError::SourceAuthorityMismatch)
        ));
        close(runtime);
    }

    #[test]
    fn malformed_summary_and_selection_authority_are_rejected() {
        assert_summary_rejected(|terminal| terminal.list_source.end = terminal.list_source.start);
        assert_summary_rejected(|terminal| {
            terminal.list_source_utf16.start = terminal.list_source_utf16.start.saturating_add(1);
        });
        assert_summary_rejected(|terminal| terminal.marker = b'1');
        assert_summary_rejected(|terminal| terminal.item_count = 0);
        assert_summary_rejected(|terminal| terminal.paragraph_count = 2);
        assert_summary_rejected(|terminal| {
            terminal.terminal_empty_relative_start = Some(1);
            terminal.paragraph_count = 2;
        });
        assert_summary_rejected(|terminal| {
            terminal.terminal_empty_relative_start =
                Some(terminal.list_source.end - terminal.list_source.start);
            terminal.paragraph_count = 2;
        });
        assert_summary_rejected(|terminal| {
            terminal.projected_utf8_length = terminal.list_source.end - terminal.list_source.start;
        });
        assert_summary_rejected(|terminal| {
            terminal.projected_utf16_length = terminal.projected_utf8_length.saturating_add(1);
        });

        let (runtime, lease, mut terminal) = fixture();
        terminal.block_entry_ordinal = u64::MAX;
        assert!(matches!(
            M11ExactSegmentedCandidateInput::from_bullet_list_local_delta(lease, &terminal),
            Err(M11CandidateDerivationError::MetricOverflow)
        ));
        close(runtime);
    }
}

#[cfg(test)]
mod ordered_checkpoint_merge_tests {
    use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, SourceVersion};

    use crate::exact_clean::{
        empty_ordinary_paragraph_checkpoint_seed_cursor,
        synthetic_ordinary_paragraph_restart_checkpoints,
    };
    use crate::{M11OrdinaryParagraphRestartCheckpoints, M11ParserBinding};

    use super::{
        OrderedCheckpointMerge, OrderedCheckpointMergeError, OrderedCheckpointMergePoll,
        OrderedCheckpointMergeResult, M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION,
    };

    // Deliberately not divisible by the 32-record quantum so segment seams
    // must be crossed within one transition.
    const LARGE_CHECKPOINT_COUNT: usize = (1 << 14) + 7;

    fn binding() -> M11ParserBinding {
        M11ParserBinding::current(ParserProfileId::new(79).expect("nonzero parser profile"))
    }

    fn fixture() -> (
        DocumentRuntime,
        SourceVersion,
        SourceVersion,
        M11ParserBinding,
    ) {
        let source = "x".repeat(LARGE_CHECKPOINT_COUNT + 1);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let base = runtime.current_source_version().expect("base source");
        let target = runtime
            .apply_edit(base, 0..1, "y")
            .expect("same-size target edit")
            .source()
            .current();
        (runtime, base, target, binding())
    }

    fn checkpoints(
        source: SourceVersion,
        binding: M11ParserBinding,
    ) -> M11OrdinaryParagraphRestartCheckpoints {
        synthetic_ordinary_paragraph_restart_checkpoints(source, binding, LARGE_CHECKPOINT_COUNT)
    }

    fn target_restart(
        base: &M11OrdinaryParagraphRestartCheckpoints,
        target: SourceVersion,
        index: usize,
    ) -> crate::M11OrdinaryParagraphRestartCheckpoint {
        base.checkpoints()[index]
            .shifted_copy_for_target(target, 0, 0, 0)
            .expect("target restart")
    }

    fn complete_with_unit_fuel(mut merge: OrderedCheckpointMerge) -> OrderedCheckpointMergeResult {
        loop {
            match merge.poll(1).expect("ordered merge poll") {
                OrderedCheckpointMergePoll::Pending { transitions } => {
                    assert_eq!(transitions, 1);
                }
                OrderedCheckpointMergePoll::Complete {
                    transitions,
                    result,
                } => {
                    assert_eq!(transitions, 1);
                    return result;
                }
            }
        }
    }

    fn assert_complete_large_merge(
        result: &OrderedCheckpointMergeResult,
        base: SourceVersion,
        target: SourceVersion,
        binding: M11ParserBinding,
    ) {
        assert_eq!(result.base.source(), base);
        assert_eq!(result.base.binding(), binding);
        assert_eq!(result.base.len(), LARGE_CHECKPOINT_COUNT);
        assert_eq!(result.target.source(), target);
        assert_eq!(result.target.binding(), binding);
        assert_eq!(result.target.len(), LARGE_CHECKPOINT_COUNT);
        assert_eq!(
            result.work.transitions,
            LARGE_CHECKPOINT_COUNT.div_ceil(M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION)
        );
        assert!(
            result.work.maximum_records_per_transition
                <= M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION
        );
        assert_eq!(
            result.work.reused_prefix_checkpoints
                + result.work.fresh_crop_checkpoints
                + result.work.reused_suffix_checkpoints,
            LARGE_CHECKPOINT_COUNT
        );
        for (index, checkpoint) in result.target.checkpoints().iter().enumerate() {
            assert_eq!(checkpoint.source(), target);
            assert_eq!(checkpoint.binding(), binding);
            assert_eq!(checkpoint.prefix_end_byte() as usize, index + 1);
            assert_eq!(checkpoint.prefix_end_utf16() as usize, index + 1);
            assert_eq!(checkpoint.next_physical_line_ordinal() as usize, index + 1);
        }
    }

    fn close(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close poll").complete {}
    }

    #[test]
    fn unit_fuel_bounds_every_large_crop_route_to_thirty_two_records() {
        let (runtime, base, target, binding) = fixture();
        let restart_index = LARGE_CHECKPOINT_COUNT / 2 - 1;
        let convergence_index = restart_index + 1;

        let interior_base = checkpoints(base, binding);
        let interior_restart = target_restart(&interior_base, target, restart_index);
        let interior = OrderedCheckpointMerge::interior(
            target,
            binding,
            interior_base,
            interior_restart,
            empty_ordinary_paragraph_checkpoint_seed_cursor(target, binding),
            restart_index,
            convergence_index,
            0,
            0,
            0,
            0,
            false,
            1,
        )
        .expect("interior merge");
        assert_complete_large_merge(&complete_with_unit_fuel(interior), base, target, binding);

        let bof = OrderedCheckpointMerge::from_bof(
            target,
            binding,
            checkpoints(base, binding),
            empty_ordinary_paragraph_checkpoint_seed_cursor(target, binding),
            0,
            0,
            0,
            0,
            0,
        )
        .expect("BOF merge");
        assert_complete_large_merge(&complete_with_unit_fuel(bof), base, target, binding);

        let eof_base = checkpoints(base, binding);
        let eof_restart =
            target_restart(&eof_base, target, LARGE_CHECKPOINT_COUNT.saturating_sub(1));
        let eof = OrderedCheckpointMerge::to_eof(
            target,
            binding,
            eof_base,
            eof_restart,
            empty_ordinary_paragraph_checkpoint_seed_cursor(target, binding),
            LARGE_CHECKPOINT_COUNT - 1,
            1,
        )
        .expect("EOF merge");
        assert_complete_large_merge(&complete_with_unit_fuel(eof), base, target, binding);

        close(runtime);
    }

    #[test]
    fn mid_merge_cancel_returns_the_untouched_large_base_collection() {
        let (runtime, base, target, binding) = fixture();
        let restart_index = LARGE_CHECKPOINT_COUNT / 2 - 1;
        let base_checkpoints = checkpoints(base, binding);
        let target_restart = target_restart(&base_checkpoints, target, restart_index);
        let mut merge = OrderedCheckpointMerge::interior(
            target,
            binding,
            base_checkpoints,
            target_restart,
            empty_ordinary_paragraph_checkpoint_seed_cursor(target, binding),
            restart_index,
            restart_index + 1,
            0,
            0,
            0,
            0,
            false,
            1,
        )
        .expect("interior merge");

        assert!(matches!(
            merge.poll(3).expect("partial merge"),
            OrderedCheckpointMergePoll::Pending { transitions: 3 }
        ));
        let restored = merge.cancel_into_base().expect("base authority");
        assert_eq!(restored.source(), base);
        assert_eq!(restored.binding(), binding);
        assert_eq!(restored.len(), LARGE_CHECKPOINT_COUNT);
        for (index, checkpoint) in restored.checkpoints().iter().enumerate() {
            assert_eq!(checkpoint.source(), base);
            assert_eq!(checkpoint.prefix_end_byte() as usize, index + 1);
            assert_eq!(checkpoint.prefix_end_utf16() as usize, index + 1);
            assert_eq!(checkpoint.next_physical_line_ordinal() as usize, index + 1);
        }

        close(runtime);
    }

    #[test]
    fn merge_rejects_a_shifted_checkpoint_outside_the_target_block_topology() {
        let (runtime, base, target, binding) = fixture();
        let base_checkpoints = checkpoints(base, binding);
        let target_restart = target_restart(&base_checkpoints, target, 0);
        let mut merge = OrderedCheckpointMerge::interior(
            target,
            binding,
            base_checkpoints,
            target_restart,
            empty_ordinary_paragraph_checkpoint_seed_cursor(target, binding),
            0,
            1,
            0,
            0,
            0,
            1,
            true,
            1,
        )
        .expect("merge construction validates only base topology");

        assert!(matches!(
            merge.poll(1),
            Err(OrderedCheckpointMergeError::InvalidBoundary)
        ));
        close(runtime);
    }
}
