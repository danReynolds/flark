use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use comrak::block_spine_facade::FacadeError;
use flark_engine::{ParserProfileId, SourceVersion};

use crate::contract::{
    M11ExactController, M11LineEnding, M11PhysicalLineFacts, M11SourceLinePollReceipt,
    M11SourceLinePollStatus, M11SourceLineSource, SourceLineIdentity,
};
use crate::segmented_lexical::{
    SegmentedAtxHeadingFacts, SegmentedBlockQuoteFacts, SegmentedIndentedCodeLineFacts,
    SegmentedLineFacts, SegmentedLineScanner, SegmentedListItemFacts, SegmentedListMarker,
    SegmentedReferenceDefinition, SegmentedReferenceError, SegmentedReferencePrefix,
    SegmentedReferenceTerminal, SegmentedSetextHeadingFacts, SegmentedThematicBreakFacts,
    SEGMENTED_LINE_PREFIX_BYTES,
};

/// Hard bound on raw physical-line bytes retained by the segmented lexer.
///
/// This is a memory bound, never an admission or document-size limit.
pub const M11_SEGMENTED_LINE_PREFIX_BYTES: usize = SEGMENTED_LINE_PREFIX_BYTES;

/// Grammar partition bound into leading-reference restart checkpoints.
pub const M11_GRAMMAR_REVISION: u32 = 9;
/// Minimum source-byte distance between ordinary Paragraph restart checkpoints.
///
/// A checkpoint is emitted only at a completely committed physical-line
/// boundary. A physical line that crosses several stride boundaries therefore
/// still contributes at most one checkpoint.
pub const M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES: u32 = 4 * 1024;

static CONTROLLER_IDS: AtomicU64 = AtomicU64::new(1);

/// Exact parser profile and grammar partition for one restart attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ParserBinding {
    syntax_profile: ParserProfileId,
    grammar_revision: u32,
}

impl M11ParserBinding {
    #[must_use]
    pub const fn new(syntax_profile: ParserProfileId, grammar_revision: u32) -> Self {
        Self {
            syntax_profile,
            grammar_revision,
        }
    }

    #[must_use]
    pub const fn current(syntax_profile: ParserProfileId) -> Self {
        Self::new(syntax_profile, M11_GRAMMAR_REVISION)
    }

    #[must_use]
    pub const fn syntax_profile(self) -> ParserProfileId {
        self.syntax_profile
    }

    #[must_use]
    pub const fn grammar_revision(self) -> u32 {
        self.grammar_revision
    }
}

/// Typed parser state at the exact end of a complete leading-definition run.
///
/// This tag owns no definitions or source. Its presence prevents a crop caller
/// from substituting an ordinary paragraph-open state.
pub struct LeadingReferencesAwaitingRemainder {
    _private: (),
}

/// Move-only restart authority minted by one eligible exact clean parse.
///
/// The checkpoint deliberately owns no definition vector, cooked value,
/// source lease, role identity, or publication root.
///
/// ```compile_fail
/// use flark_parser::LeadingReferencesRestartCheckpoint;
///
/// fn duplicate(checkpoint: LeadingReferencesRestartCheckpoint) {
///     let _copy = checkpoint.clone();
/// }
/// ```
#[must_use = "a leading-reference restart checkpoint must be consumed or deliberately dropped"]
pub struct LeadingReferencesRestartCheckpoint {
    source: SourceVersion,
    binding: M11ParserBinding,
    paragraph_content_start: u32,
    prefix_end_byte: u32,
    prefix_end_utf16: u32,
    next_physical_line_ordinal: u32,
    definition_count: usize,
    state: LeadingReferencesAwaitingRemainder,
}

impl fmt::Debug for LeadingReferencesRestartCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LeadingReferencesRestartCheckpoint")
            .field("source", &self.source)
            .field("binding", &self.binding)
            .field("paragraph_content_start", &self.paragraph_content_start)
            .field("prefix_end_byte", &self.prefix_end_byte)
            .field("prefix_end_utf16", &self.prefix_end_utf16)
            .field(
                "next_physical_line_ordinal",
                &self.next_physical_line_ordinal,
            )
            .field("definition_count", &self.definition_count)
            .finish()
    }
}

impl LeadingReferencesRestartCheckpoint {
    pub(crate) fn for_target(&self, target: SourceVersion) -> Self {
        Self {
            source: target,
            binding: self.binding,
            paragraph_content_start: self.paragraph_content_start,
            prefix_end_byte: self.prefix_end_byte,
            prefix_end_utf16: self.prefix_end_utf16,
            next_physical_line_ordinal: self.next_physical_line_ordinal,
            definition_count: self.definition_count,
            state: LeadingReferencesAwaitingRemainder { _private: () },
        }
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn paragraph_content_start(&self) -> u32 {
        self.paragraph_content_start
    }

    #[must_use]
    pub const fn prefix_end_byte(&self) -> u32 {
        self.prefix_end_byte
    }

    #[must_use]
    pub const fn prefix_end_utf16(&self) -> u32 {
        self.prefix_end_utf16
    }

    #[must_use]
    pub const fn next_physical_line_ordinal(&self) -> u32 {
        self.next_physical_line_ordinal
    }

    #[must_use]
    pub const fn definition_count(&self) -> usize {
        self.definition_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeadingReferencesCheckpointError {
    Ineligible,
    AlreadyTaken,
}

/// Move-only block-parser state after one committed line of an ordinary
/// definition-free Paragraph.
///
/// This checkpoint carries no inline-parser state or inline authority.
/// Emphasis, code, links, and other inline facts must still be derived over the
/// complete enclosing Paragraph.
#[must_use = "an ordinary Paragraph restart checkpoint must be consumed or deliberately dropped"]
pub struct M11OrdinaryParagraphRestartCheckpoint {
    source: SourceVersion,
    binding: M11ParserBinding,
    frozen_reference_definition_count: usize,
    paragraph_source_start_byte: u32,
    paragraph_source_start_utf16: u32,
    paragraph_content_start: u32,
    block_entry_ordinal: u64,
    preceding_line_start_byte: u32,
    preceding_line_start_utf16: u32,
    preceding_line_content_bytes: u32,
    preceding_line_content_utf16: u32,
    preceding_line_physical_bytes: u32,
    preceding_line_physical_utf16: u32,
    prefix_end_byte: u32,
    prefix_end_utf16: u32,
    next_physical_line_ordinal: u32,
    state: OrdinaryParagraphAwaitingContinuation,
}

impl fmt::Debug for M11OrdinaryParagraphRestartCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11OrdinaryParagraphRestartCheckpoint")
            .field("source", &self.source)
            .field("binding", &self.binding)
            .field(
                "frozen_reference_definition_count",
                &self.frozen_reference_definition_count,
            )
            .field(
                "paragraph_source_start_byte",
                &self.paragraph_source_start_byte,
            )
            .field(
                "paragraph_source_start_utf16",
                &self.paragraph_source_start_utf16,
            )
            .field("paragraph_content_start", &self.paragraph_content_start)
            .field("block_entry_ordinal", &self.block_entry_ordinal)
            .field("preceding_line_start_byte", &self.preceding_line_start_byte)
            .field(
                "preceding_line_start_utf16",
                &self.preceding_line_start_utf16,
            )
            .field(
                "preceding_line_content_bytes",
                &self.preceding_line_content_bytes,
            )
            .field(
                "preceding_line_content_utf16",
                &self.preceding_line_content_utf16,
            )
            .field(
                "preceding_line_physical_bytes",
                &self.preceding_line_physical_bytes,
            )
            .field(
                "preceding_line_physical_utf16",
                &self.preceding_line_physical_utf16,
            )
            .field("prefix_end_byte", &self.prefix_end_byte)
            .field("prefix_end_utf16", &self.prefix_end_utf16)
            .field(
                "next_physical_line_ordinal",
                &self.next_physical_line_ordinal,
            )
            .finish()
    }
}

impl M11OrdinaryParagraphRestartCheckpoint {
    pub(crate) fn for_target(&self, target: SourceVersion) -> Self {
        Self {
            source: target,
            binding: self.binding,
            frozen_reference_definition_count: self.frozen_reference_definition_count,
            paragraph_source_start_byte: self.paragraph_source_start_byte,
            paragraph_source_start_utf16: self.paragraph_source_start_utf16,
            paragraph_content_start: self.paragraph_content_start,
            block_entry_ordinal: self.block_entry_ordinal,
            preceding_line_start_byte: self.preceding_line_start_byte,
            preceding_line_start_utf16: self.preceding_line_start_utf16,
            preceding_line_content_bytes: self.preceding_line_content_bytes,
            preceding_line_content_utf16: self.preceding_line_content_utf16,
            preceding_line_physical_bytes: self.preceding_line_physical_bytes,
            preceding_line_physical_utf16: self.preceding_line_physical_utf16,
            prefix_end_byte: self.prefix_end_byte,
            prefix_end_utf16: self.prefix_end_utf16,
            next_physical_line_ordinal: self.next_physical_line_ordinal,
            state: OrdinaryParagraphAwaitingContinuation { _private: () },
        }
    }

    pub(crate) fn shifted_copy_for_target(
        &self,
        target: SourceVersion,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
    ) -> Option<Self> {
        self.shifted_copy_for_target_with_paragraph_start(
            target,
            self.paragraph_content_start,
            byte_delta,
            utf16_delta,
            ordinal_delta,
            0,
        )
    }

    pub(crate) fn shifted_copy_for_target_with_block_delta(
        &self,
        target: SourceVersion,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
        block_ordinal_delta: i64,
    ) -> Option<Self> {
        self.shifted_copy_for_target_with_paragraph_geometry(
            target,
            shift_u32(self.paragraph_source_start_byte, byte_delta)?,
            shift_u32(self.paragraph_source_start_utf16, utf16_delta)?,
            shift_u32(self.paragraph_content_start, byte_delta)?,
            byte_delta,
            utf16_delta,
            ordinal_delta,
            block_ordinal_delta,
        )
    }

    pub(crate) fn shifted_copy_for_target_with_paragraph_start(
        &self,
        target: SourceVersion,
        paragraph_content_start: u32,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
        block_ordinal_delta: i64,
    ) -> Option<Self> {
        self.shifted_copy_for_target_with_paragraph_geometry(
            target,
            self.paragraph_source_start_byte,
            self.paragraph_source_start_utf16,
            paragraph_content_start,
            byte_delta,
            utf16_delta,
            ordinal_delta,
            block_ordinal_delta,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn shifted_copy_for_target_with_paragraph_geometry(
        &self,
        target: SourceVersion,
        paragraph_source_start_byte: u32,
        paragraph_source_start_utf16: u32,
        paragraph_content_start: u32,
        byte_delta: i64,
        utf16_delta: i64,
        ordinal_delta: i64,
        block_ordinal_delta: i64,
    ) -> Option<Self> {
        Some(Self {
            source: target,
            binding: self.binding,
            frozen_reference_definition_count: self.frozen_reference_definition_count,
            paragraph_source_start_byte,
            paragraph_source_start_utf16,
            paragraph_content_start,
            block_entry_ordinal: shift_u64(self.block_entry_ordinal, block_ordinal_delta)?,
            preceding_line_start_byte: shift_u32(self.preceding_line_start_byte, byte_delta)?,
            preceding_line_start_utf16: shift_u32(self.preceding_line_start_utf16, utf16_delta)?,
            preceding_line_content_bytes: self.preceding_line_content_bytes,
            preceding_line_content_utf16: self.preceding_line_content_utf16,
            preceding_line_physical_bytes: self.preceding_line_physical_bytes,
            preceding_line_physical_utf16: self.preceding_line_physical_utf16,
            prefix_end_byte: shift_u32(self.prefix_end_byte, byte_delta)?,
            prefix_end_utf16: shift_u32(self.prefix_end_utf16, utf16_delta)?,
            next_physical_line_ordinal: shift_u32(self.next_physical_line_ordinal, ordinal_delta)?,
            state: OrdinaryParagraphAwaitingContinuation { _private: () },
        })
    }

    pub(crate) fn metrics_are_consistent(&self) -> bool {
        checkpoint_metrics_are_consistent(self)
    }

    pub(crate) fn same_boundary_state(&self, other: &Self) -> bool {
        self.source == other.source
            && self.binding == other.binding
            && self.frozen_reference_definition_count == other.frozen_reference_definition_count
            && self.paragraph_source_start_byte == other.paragraph_source_start_byte
            && self.paragraph_source_start_utf16 == other.paragraph_source_start_utf16
            && self.paragraph_content_start == other.paragraph_content_start
            && self.block_entry_ordinal == other.block_entry_ordinal
            && self.preceding_line_start_byte == other.preceding_line_start_byte
            && self.preceding_line_start_utf16 == other.preceding_line_start_utf16
            && self.preceding_line_content_bytes == other.preceding_line_content_bytes
            && self.preceding_line_content_utf16 == other.preceding_line_content_utf16
            && self.preceding_line_physical_bytes == other.preceding_line_physical_bytes
            && self.preceding_line_physical_utf16 == other.preceding_line_physical_utf16
            && self.prefix_end_byte == other.prefix_end_byte
            && self.prefix_end_utf16 == other.prefix_end_utf16
            && self.next_physical_line_ordinal == other.next_physical_line_ordinal
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    /// Number of exact reference definitions wholly contained in the
    /// authenticated unchanged prefix before this restart.
    ///
    /// A nonzero value does not authorize editing those definitions. It
    /// authorizes retaining their existing semantic root only while this
    /// checkpoint's exact prefix remains unchanged.
    #[must_use]
    pub const fn frozen_reference_definition_count(&self) -> usize {
        self.frozen_reference_definition_count
    }

    #[must_use]
    pub const fn paragraph_content_start(&self) -> u32 {
        self.paragraph_content_start
    }

    #[must_use]
    pub const fn paragraph_source_start_byte(&self) -> u32 {
        self.paragraph_source_start_byte
    }

    #[must_use]
    pub const fn paragraph_source_start_utf16(&self) -> u32 {
        self.paragraph_source_start_utf16
    }

    #[must_use]
    pub const fn block_entry_ordinal(&self) -> u64 {
        self.block_entry_ordinal
    }

    #[must_use]
    pub const fn preceding_line_start_byte(&self) -> u32 {
        self.preceding_line_start_byte
    }

    #[must_use]
    pub const fn preceding_line_start_utf16(&self) -> u32 {
        self.preceding_line_start_utf16
    }

    #[must_use]
    pub const fn preceding_line_content_bytes(&self) -> u32 {
        self.preceding_line_content_bytes
    }

    #[must_use]
    pub const fn preceding_line_content_utf16(&self) -> u32 {
        self.preceding_line_content_utf16
    }

    #[must_use]
    pub const fn preceding_line_physical_bytes(&self) -> u32 {
        self.preceding_line_physical_bytes
    }

    #[must_use]
    pub const fn preceding_line_physical_utf16(&self) -> u32 {
        self.preceding_line_physical_utf16
    }

    #[must_use]
    pub const fn prefix_end_byte(&self) -> u32 {
        self.prefix_end_byte
    }

    #[must_use]
    pub const fn prefix_end_utf16(&self) -> u32 {
        self.prefix_end_utf16
    }

    #[must_use]
    pub const fn next_physical_line_ordinal(&self) -> u32 {
        self.next_physical_line_ordinal
    }
}

/// One-take collection of sparse ordinary Paragraph restart checkpoints.
///
/// The collection and every checkpoint are bound to the exact clean source and
/// parser binding supplied when the terminal result authorizes the take.
#[must_use = "ordinary Paragraph restart checkpoints must be consumed or deliberately dropped"]
#[derive(Debug)]
pub struct M11OrdinaryParagraphRestartCheckpoints {
    source: SourceVersion,
    binding: M11ParserBinding,
    checkpoints: Vec<M11OrdinaryParagraphRestartCheckpoint>,
    top_level_block_count: u64,
}

impl M11OrdinaryParagraphRestartCheckpoints {
    pub(crate) const fn from_checkpoints(
        source: SourceVersion,
        binding: M11ParserBinding,
        checkpoints: Vec<M11OrdinaryParagraphRestartCheckpoint>,
        top_level_block_count: u64,
    ) -> Self {
        Self {
            source,
            binding,
            checkpoints,
            top_level_block_count,
        }
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    /// Exact reference-definition count frozen before every restart in this
    /// collection.
    ///
    /// An empty collection carries no usable restart authority.
    #[must_use]
    pub fn frozen_reference_definition_count(&self) -> Option<usize> {
        self.checkpoints
            .first()
            .map(M11OrdinaryParagraphRestartCheckpoint::frozen_reference_definition_count)
    }

    #[must_use]
    pub fn is_segmented_top_level(&self) -> bool {
        self.top_level_block_count > 1
    }

    /// Exact number of top-level structural leaves in the authenticated base.
    #[must_use]
    pub const fn top_level_block_count(&self) -> u64 {
        self.top_level_block_count
    }

    #[must_use = "ordinary Paragraph restart checkpoints must be consumed or deliberately dropped"]
    pub fn checkpoints(&self) -> &[M11OrdinaryParagraphRestartCheckpoint] {
        &self.checkpoints
    }

    #[must_use]
    pub fn into_checkpoints(self) -> Vec<M11OrdinaryParagraphRestartCheckpoint> {
        self.checkpoints
    }

    /// Selects the widest authenticated crop around one base-source edit.
    ///
    /// This is a borrow-only preflight. The consuming crop plan re-runs the
    /// selection before accepting checkpoint authority.
    pub fn select_crop(
        &self,
        changed_base_bytes: Range<usize>,
    ) -> Result<M11OrdinaryParagraphCropSelection, M11OrdinaryParagraphCropPlanError> {
        if changed_base_bytes.start > changed_base_bytes.end
            || changed_base_bytes.end > self.source.byte_len()
        {
            return Err(M11OrdinaryParagraphCropPlanError::InvalidChangedRange);
        }
        let changed_start = u32::try_from(changed_base_bytes.start)
            .map_err(|_| M11OrdinaryParagraphCropPlanError::InvalidChangedRange)?;
        let changed_end = u32::try_from(changed_base_bytes.end)
            .map_err(|_| M11OrdinaryParagraphCropPlanError::InvalidChangedRange)?;
        // Clean terminals and every checkpoint merge preserve strict source
        // order. Admission therefore probes logarithmically instead of
        // walking a document-sized checkpoint collection on the caller's
        // synchronous path.
        let restart_index = checkpoint_partition_point(&self.checkpoints, |checkpoint| {
            checkpoint.prefix_end_byte <= changed_start
        })
        .checked_sub(1)
        .ok_or(M11OrdinaryParagraphCropPlanError::NoRestartCheckpoint)?;
        let segmented_top_level = self.is_segmented_top_level();
        let convergence_offset =
            checkpoint_partition_point(&self.checkpoints[restart_index + 1..], |checkpoint| {
                if segmented_top_level {
                    checkpoint.paragraph_source_start_byte <= changed_end
                } else {
                    checkpoint.preceding_line_start_byte < changed_end
                }
            });
        let convergence_index = restart_index
            .checked_add(1)
            .and_then(|index| index.checked_add(convergence_offset))
            .filter(|index| *index < self.checkpoints.len())
            .ok_or(M11OrdinaryParagraphCropPlanError::NoConvergenceCheckpoint)?;
        let restart = &self.checkpoints[restart_index];
        let convergence = &self.checkpoints[convergence_index];
        if convergence.preceding_line_start_byte < restart.prefix_end_byte
            || convergence.preceding_line_physical_bytes == 0
            || segmented_top_level
                && (convergence.paragraph_source_start_byte < restart.prefix_end_byte
                    || convergence.block_entry_ordinal <= restart.block_entry_ordinal)
            || restart.block_entry_ordinal >= self.top_level_block_count
            || convergence.block_entry_ordinal >= self.top_level_block_count
            || !checkpoint_metrics_are_consistent(restart)
            || !checkpoint_metrics_are_consistent(convergence)
        {
            return Err(M11OrdinaryParagraphCropPlanError::InvalidCheckpoint);
        }
        Ok(M11OrdinaryParagraphCropSelection {
            source: self.source,
            binding: self.binding,
            changed_start,
            changed_end,
            restart_index,
            convergence_index,
            restart_prefix_end_byte: restart.prefix_end_byte,
            restart_prefix_end_utf16: restart.prefix_end_utf16,
            convergence_line_start_byte: convergence.preceding_line_start_byte,
            convergence_line_start_utf16: convergence.preceding_line_start_utf16,
            convergence_suffix_start_byte: if segmented_top_level {
                convergence.paragraph_source_start_byte
            } else {
                convergence.preceding_line_start_byte
            },
            convergence_suffix_start_utf16: if segmented_top_level {
                convergence.paragraph_source_start_utf16
            } else {
                convergence.preceding_line_start_utf16
            },
            restart_block_entry_ordinal: restart.block_entry_ordinal,
            convergence_block_entry_ordinal: convergence.block_entry_ordinal,
            base_block_entry_count: self.top_level_block_count,
            segmented_top_level,
        })
    }

    /// Selects a BOF crop ending after the first authenticated unchanged
    /// convergence line. A whole-source change remains ineligible so the
    /// clean lane retains authority.
    pub fn select_bof_crop(
        &self,
        changed_base_bytes: Range<usize>,
    ) -> Result<M11OrdinaryParagraphBofCropSelection, M11OrdinaryParagraphBoundaryCropPlanError>
    {
        if changed_base_bytes.start > changed_base_bytes.end
            || changed_base_bytes.end > self.source.byte_len()
        {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange);
        }
        if changed_base_bytes.start != 0 {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::NotBofBoundary);
        }
        if changed_base_bytes.end == self.source.byte_len() {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible);
        }
        if self.is_segmented_top_level()
            && self
                .frozen_reference_definition_count()
                .is_some_and(|count| count != 0)
        {
            // References retain absolute source coordinates. A BOF edit may
            // shift definitions before the first surviving ordinary restart,
            // so only a definition-free segmented collection may retain the
            // base References root.
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::FrozenReferencesIneligible);
        }
        let changed_end = u32::try_from(changed_base_bytes.end)
            .map_err(|_| M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange)?;
        let segmented_top_level = self.is_segmented_top_level();
        let convergence_index = checkpoint_partition_point(&self.checkpoints, |checkpoint| {
            if segmented_top_level {
                checkpoint.paragraph_source_start_byte <= changed_end
            } else {
                checkpoint.preceding_line_start_byte < changed_end
            }
        });
        if convergence_index == self.checkpoints.len() {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::NoConvergenceCheckpoint);
        }
        let convergence = &self.checkpoints[convergence_index];
        if convergence.preceding_line_physical_bytes == 0
            || segmented_top_level
                && (convergence.paragraph_source_start_byte > convergence.preceding_line_start_byte
                    || convergence.block_entry_ordinal == 0)
            || convergence.block_entry_ordinal >= self.top_level_block_count
            || !checkpoint_metrics_are_consistent(convergence)
        {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidCheckpoint);
        }
        Ok(M11OrdinaryParagraphBofCropSelection {
            source: self.source,
            binding: self.binding,
            changed_end,
            convergence_index,
            convergence_line_start_byte: convergence.preceding_line_start_byte,
            convergence_line_start_utf16: convergence.preceding_line_start_utf16,
            convergence_suffix_start_byte: if segmented_top_level {
                convergence.paragraph_source_start_byte
            } else {
                convergence.preceding_line_start_byte
            },
            convergence_suffix_start_utf16: if segmented_top_level {
                convergence.paragraph_source_start_utf16
            } else {
                convergence.preceding_line_start_utf16
            },
            convergence_block_entry_ordinal: convergence.block_entry_ordinal,
            base_block_entry_count: self.top_level_block_count,
            segmented_top_level,
        })
    }

    /// Selects an authenticated restart for a crop that extends through EOF.
    /// A whole-source change remains ineligible for the existing clean lane.
    pub fn select_eof_crop(
        &self,
        changed_base_bytes: Range<usize>,
    ) -> Result<M11OrdinaryParagraphEofCropSelection, M11OrdinaryParagraphBoundaryCropPlanError>
    {
        if changed_base_bytes.start > changed_base_bytes.end
            || changed_base_bytes.end > self.source.byte_len()
        {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange);
        }
        if changed_base_bytes.end != self.source.byte_len() {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::NotEofBoundary);
        }
        if changed_base_bytes.start == 0 {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible);
        }
        let changed_start = u32::try_from(changed_base_bytes.start)
            .map_err(|_| M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange)?;
        let restart_index = checkpoint_partition_point(&self.checkpoints, |checkpoint| {
            checkpoint.prefix_end_byte <= changed_start
        })
        .checked_sub(1)
        .ok_or(M11OrdinaryParagraphBoundaryCropPlanError::NoRestartCheckpoint)?;
        let restart = &self.checkpoints[restart_index];
        if !checkpoint_metrics_are_consistent(restart)
            || restart.block_entry_ordinal >= self.top_level_block_count
        {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidCheckpoint);
        }
        Ok(M11OrdinaryParagraphEofCropSelection {
            source: self.source,
            binding: self.binding,
            changed_start,
            restart_index,
            restart_prefix_end_byte: restart.prefix_end_byte,
            restart_prefix_end_utf16: restart.prefix_end_utf16,
            restart_block_entry_ordinal: restart.block_entry_ordinal,
            base_block_entry_count: self.top_level_block_count,
            segmented_top_level: self.is_segmented_top_level(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11OrdinaryParagraphBofCropSelection {
    source: SourceVersion,
    binding: M11ParserBinding,
    changed_end: u32,
    convergence_index: usize,
    convergence_line_start_byte: u32,
    convergence_line_start_utf16: u32,
    convergence_suffix_start_byte: u32,
    convergence_suffix_start_utf16: u32,
    convergence_block_entry_ordinal: u64,
    base_block_entry_count: u64,
    segmented_top_level: bool,
}

impl M11OrdinaryParagraphBofCropSelection {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn changed_base_end(&self) -> u32 {
        self.changed_end
    }

    #[must_use]
    pub const fn convergence_index(&self) -> usize {
        self.convergence_index
    }

    #[must_use]
    pub const fn convergence_line_start_byte(&self) -> u32 {
        self.convergence_line_start_byte
    }

    #[must_use]
    pub const fn convergence_line_start_utf16(&self) -> u32 {
        self.convergence_line_start_utf16
    }

    #[must_use]
    pub const fn convergence_suffix_start_byte(&self) -> u32 {
        self.convergence_suffix_start_byte
    }

    #[must_use]
    pub const fn convergence_suffix_start_utf16(&self) -> u32 {
        self.convergence_suffix_start_utf16
    }

    #[must_use]
    pub const fn convergence_block_entry_ordinal(&self) -> u64 {
        self.convergence_block_entry_ordinal
    }

    #[must_use]
    pub const fn base_block_entry_count(&self) -> u64 {
        self.base_block_entry_count
    }

    #[must_use]
    pub const fn is_segmented_top_level(&self) -> bool {
        self.segmented_top_level
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11OrdinaryParagraphEofCropSelection {
    source: SourceVersion,
    binding: M11ParserBinding,
    changed_start: u32,
    restart_index: usize,
    restart_prefix_end_byte: u32,
    restart_prefix_end_utf16: u32,
    restart_block_entry_ordinal: u64,
    base_block_entry_count: u64,
    segmented_top_level: bool,
}

impl M11OrdinaryParagraphEofCropSelection {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn changed_base_start(&self) -> u32 {
        self.changed_start
    }

    #[must_use]
    pub const fn restart_index(&self) -> usize {
        self.restart_index
    }

    #[must_use]
    pub const fn restart_prefix_end_byte(&self) -> u32 {
        self.restart_prefix_end_byte
    }

    #[must_use]
    pub const fn restart_prefix_end_utf16(&self) -> u32 {
        self.restart_prefix_end_utf16
    }

    #[must_use]
    pub const fn restart_block_entry_ordinal(&self) -> u64 {
        self.restart_block_entry_ordinal
    }

    #[must_use]
    pub const fn base_block_entry_count(&self) -> u64 {
        self.base_block_entry_count
    }

    #[must_use]
    pub const fn is_segmented_top_level(&self) -> bool {
        self.segmented_top_level
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11OrdinaryParagraphBoundaryCropPlanError {
    InvalidChangedRange,
    SegmentedTopLevelIneligible,
    FrozenReferencesIneligible,
    NotBofBoundary,
    NotEofBoundary,
    WholeSourceIneligible,
    NoRestartCheckpoint,
    NoConvergenceCheckpoint,
    InvalidCheckpoint,
    SelectionMismatch,
}

/// Move-only parser-owned base collection for a BOF convergence crop.
#[must_use = "a BOF crop plan must be consumed or deliberately dropped"]
pub struct M11OrdinaryParagraphBofCropPlan {
    selection: M11OrdinaryParagraphBofCropSelection,
    base_checkpoints: Vec<M11OrdinaryParagraphRestartCheckpoint>,
}

impl M11OrdinaryParagraphBofCropPlan {
    pub fn new(
        checkpoints: M11OrdinaryParagraphRestartCheckpoints,
        selection: M11OrdinaryParagraphBofCropSelection,
    ) -> Result<Self, M11OrdinaryParagraphBoundaryCropPlanError> {
        if checkpoints.source != selection.source || checkpoints.binding != selection.binding {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch);
        }
        let changed = 0..usize::try_from(selection.changed_end)
            .map_err(|_| M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch)?;
        if checkpoints.select_bof_crop(changed)? != selection {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch);
        }
        Ok(Self {
            selection,
            base_checkpoints: checkpoints.checkpoints,
        })
    }

    #[must_use]
    pub const fn selection(&self) -> M11OrdinaryParagraphBofCropSelection {
        self.selection
    }

    pub(crate) fn convergence(
        &self,
    ) -> Result<&M11OrdinaryParagraphRestartCheckpoint, M11OrdinaryParagraphBoundaryCropPlanError>
    {
        self.base_checkpoints
            .get(self.selection.convergence_index)
            .filter(|checkpoint| {
                checkpoint.preceding_line_start_byte == self.selection.convergence_line_start_byte
                    && checkpoint.preceding_line_start_utf16
                        == self.selection.convergence_line_start_utf16
                    && (if self.selection.segmented_top_level {
                        checkpoint.paragraph_source_start_byte
                            == self.selection.convergence_suffix_start_byte
                            && checkpoint.paragraph_source_start_utf16
                                == self.selection.convergence_suffix_start_utf16
                            && checkpoint.block_entry_ordinal
                                == self.selection.convergence_block_entry_ordinal
                    } else {
                        self.selection.convergence_suffix_start_byte
                            == self.selection.convergence_line_start_byte
                            && self.selection.convergence_suffix_start_utf16
                                == self.selection.convergence_line_start_utf16
                    })
            })
            .ok_or(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch)
    }

    pub(crate) fn into_base_checkpoints(self) -> Vec<M11OrdinaryParagraphRestartCheckpoint> {
        self.base_checkpoints
    }
}

/// Move-only parser-owned base collection for an EOF tail crop.
#[must_use = "an EOF crop plan must be consumed or deliberately dropped"]
pub struct M11OrdinaryParagraphEofCropPlan {
    selection: M11OrdinaryParagraphEofCropSelection,
    restart: Option<M11OrdinaryParagraphRestartCheckpoint>,
    remaining_base_checkpoints: Vec<M11OrdinaryParagraphRestartCheckpoint>,
}

impl M11OrdinaryParagraphEofCropPlan {
    pub fn new(
        checkpoints: M11OrdinaryParagraphRestartCheckpoints,
        selection: M11OrdinaryParagraphEofCropSelection,
    ) -> Result<Self, M11OrdinaryParagraphBoundaryCropPlanError> {
        if checkpoints.source != selection.source || checkpoints.binding != selection.binding {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch);
        }
        let changed = usize::try_from(selection.changed_start)
            .map_err(|_| M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch)?
            ..checkpoints.source.byte_len();
        if checkpoints.select_eof_crop(changed)? != selection {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch);
        }
        let mut remaining_base_checkpoints = checkpoints.checkpoints;
        // Move-only restart authority leaves the collection in O(1). Terminal
        // publication sorts the consumed remainder before minting the next
        // ordered collection.
        let restart = remaining_base_checkpoints.swap_remove(selection.restart_index);
        Ok(Self {
            selection,
            restart: Some(restart),
            remaining_base_checkpoints,
        })
    }

    #[must_use]
    pub const fn selection(&self) -> M11OrdinaryParagraphEofCropSelection {
        self.selection
    }

    pub(crate) fn take_restart(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoint, M11OrdinaryParagraphBoundaryCropPlanError>
    {
        self.restart
            .take()
            .ok_or(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch)
    }

    pub(crate) fn restore_base_checkpoints(
        mut self,
        restart: M11OrdinaryParagraphRestartCheckpoint,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphBoundaryCropPlanError>
    {
        if restart.source() != self.selection.source
            || restart.binding() != self.selection.binding
            || restart.prefix_end_byte() != self.selection.restart_prefix_end_byte
            || restart.prefix_end_utf16() != self.selection.restart_prefix_end_utf16
        {
            return Err(M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch);
        }
        restore_swap_removed_checkpoint(
            &mut self.remaining_base_checkpoints,
            self.selection.restart_index,
            restart,
        )
        .map_err(|()| M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch)?;
        Ok(M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(
            self.selection.source,
            self.selection.binding,
            self.remaining_base_checkpoints,
            self.selection.base_block_entry_count,
        ))
    }
}

/// Borrow-only ordinary-Paragraph crop selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11OrdinaryParagraphCropSelection {
    source: SourceVersion,
    binding: M11ParserBinding,
    changed_start: u32,
    changed_end: u32,
    restart_index: usize,
    convergence_index: usize,
    restart_prefix_end_byte: u32,
    restart_prefix_end_utf16: u32,
    convergence_line_start_byte: u32,
    convergence_line_start_utf16: u32,
    convergence_suffix_start_byte: u32,
    convergence_suffix_start_utf16: u32,
    restart_block_entry_ordinal: u64,
    convergence_block_entry_ordinal: u64,
    base_block_entry_count: u64,
    segmented_top_level: bool,
}

impl M11OrdinaryParagraphCropSelection {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub fn changed_base_bytes(&self) -> Range<u32> {
        self.changed_start..self.changed_end
    }

    #[must_use]
    pub const fn restart_index(&self) -> usize {
        self.restart_index
    }

    #[must_use]
    pub const fn convergence_index(&self) -> usize {
        self.convergence_index
    }

    #[must_use]
    pub const fn restart_prefix_end_byte(&self) -> u32 {
        self.restart_prefix_end_byte
    }

    #[must_use]
    pub const fn restart_prefix_end_utf16(&self) -> u32 {
        self.restart_prefix_end_utf16
    }

    #[must_use]
    pub const fn convergence_line_start_byte(&self) -> u32 {
        self.convergence_line_start_byte
    }

    #[must_use]
    pub const fn convergence_line_start_utf16(&self) -> u32 {
        self.convergence_line_start_utf16
    }

    #[must_use]
    pub const fn convergence_suffix_start_byte(&self) -> u32 {
        self.convergence_suffix_start_byte
    }

    #[must_use]
    pub const fn convergence_suffix_start_utf16(&self) -> u32 {
        self.convergence_suffix_start_utf16
    }

    #[must_use]
    pub const fn restart_block_entry_ordinal(&self) -> u64 {
        self.restart_block_entry_ordinal
    }

    #[must_use]
    pub const fn convergence_block_entry_ordinal(&self) -> u64 {
        self.convergence_block_entry_ordinal
    }

    #[must_use]
    pub const fn base_block_entry_count(&self) -> u64 {
        self.base_block_entry_count
    }

    #[must_use]
    pub const fn is_segmented_top_level(&self) -> bool {
        self.segmented_top_level
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11OrdinaryParagraphCropPlanError {
    InvalidChangedRange,
    NoRestartCheckpoint,
    NoConvergenceCheckpoint,
    InvalidCheckpoint,
    SelectionMismatch,
}

/// Move-only parser-owned partition of one base checkpoint collection.
pub struct M11OrdinaryParagraphCropPlan {
    selection: M11OrdinaryParagraphCropSelection,
    restart: Option<M11OrdinaryParagraphRestartCheckpoint>,
    remaining_base_checkpoints: Vec<M11OrdinaryParagraphRestartCheckpoint>,
}

impl M11OrdinaryParagraphCropPlan {
    /// Consumes and revalidates the collection selected during preflight.
    pub fn new(
        checkpoints: M11OrdinaryParagraphRestartCheckpoints,
        selection: M11OrdinaryParagraphCropSelection,
    ) -> Result<Self, M11OrdinaryParagraphCropPlanError> {
        if checkpoints.source != selection.source || checkpoints.binding != selection.binding {
            return Err(M11OrdinaryParagraphCropPlanError::SelectionMismatch);
        }
        let changed = usize::try_from(selection.changed_start)
            .ok()
            .zip(usize::try_from(selection.changed_end).ok())
            .map(|(start, end)| start..end)
            .ok_or(M11OrdinaryParagraphCropPlanError::SelectionMismatch)?;
        if checkpoints.select_crop(changed)? != selection {
            return Err(M11OrdinaryParagraphCropPlanError::SelectionMismatch);
        }
        let mut remaining_base_checkpoints = checkpoints.checkpoints;
        // Move-only restart authority leaves the collection in O(1). The
        // removed slot receives the former last checkpoint; convergence()
        // maps that one index explicitly, and terminal publication sorts the
        // consumed remainder before minting the next ordered collection.
        let restart = remaining_base_checkpoints.swap_remove(selection.restart_index);
        Ok(Self {
            selection,
            restart: Some(restart),
            remaining_base_checkpoints,
        })
    }

    #[must_use]
    pub const fn selection(&self) -> M11OrdinaryParagraphCropSelection {
        self.selection
    }

    pub(crate) fn take_restart(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoint, M11OrdinaryParagraphCropPlanError> {
        self.restart
            .take()
            .ok_or(M11OrdinaryParagraphCropPlanError::SelectionMismatch)
    }

    pub(crate) fn convergence(
        &self,
    ) -> Result<&M11OrdinaryParagraphRestartCheckpoint, M11OrdinaryParagraphCropPlanError> {
        // swap_remove places the old final checkpoint in the restart slot. All
        // other original indexes are unchanged.
        let convergence_index =
            if self.selection.convergence_index == self.remaining_base_checkpoints.len() {
                self.selection.restart_index
            } else {
                self.selection.convergence_index
            };
        self.remaining_base_checkpoints
            .get(convergence_index)
            .filter(|checkpoint| {
                checkpoint.preceding_line_start_byte == self.selection.convergence_line_start_byte
                    && checkpoint.preceding_line_start_utf16
                        == self.selection.convergence_line_start_utf16
            })
            .ok_or(M11OrdinaryParagraphCropPlanError::SelectionMismatch)
    }

    pub(crate) fn restore_base_checkpoints(
        mut self,
        restart: M11OrdinaryParagraphRestartCheckpoint,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCropPlanError> {
        if restart.source() != self.selection.source
            || restart.binding() != self.selection.binding
            || restart.prefix_end_byte() != self.selection.restart_prefix_end_byte
            || restart.prefix_end_utf16() != self.selection.restart_prefix_end_utf16
        {
            return Err(M11OrdinaryParagraphCropPlanError::SelectionMismatch);
        }
        restore_swap_removed_checkpoint(
            &mut self.remaining_base_checkpoints,
            self.selection.restart_index,
            restart,
        )
        .map_err(|()| M11OrdinaryParagraphCropPlanError::SelectionMismatch)?;
        Ok(M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(
            self.selection.source,
            self.selection.binding,
            self.remaining_base_checkpoints,
            self.selection.base_block_entry_count,
        ))
    }
}

fn restore_swap_removed_checkpoint(
    checkpoints: &mut Vec<M11OrdinaryParagraphRestartCheckpoint>,
    removed_index: usize,
    restart: M11OrdinaryParagraphRestartCheckpoint,
) -> Result<(), ()> {
    if removed_index > checkpoints.len() || checkpoints.len() == checkpoints.capacity() {
        return Err(());
    }
    if removed_index == checkpoints.len() {
        checkpoints.push(restart);
        return Ok(());
    }
    let moved_final = std::mem::replace(&mut checkpoints[removed_index], restart);
    checkpoints.push(moved_final);
    Ok(())
}

#[cfg(test)]
std::thread_local! {
    static CHECKPOINT_PARTITION_PROBES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

fn checkpoint_partition_point<T>(
    checkpoints: &[T],
    mut predicate: impl FnMut(&T) -> bool,
) -> usize {
    checkpoints.partition_point(|checkpoint| {
        #[cfg(test)]
        CHECKPOINT_PARTITION_PROBES.with(|probes| probes.set(probes.get() + 1));
        predicate(checkpoint)
    })
}

#[cfg(test)]
fn reset_checkpoint_partition_probes() {
    CHECKPOINT_PARTITION_PROBES.with(|probes| probes.set(0));
}

#[cfg(test)]
fn checkpoint_partition_probes() -> usize {
    CHECKPOINT_PARTITION_PROBES.with(std::cell::Cell::get)
}

fn checkpoint_metrics_are_consistent(checkpoint: &M11OrdinaryParagraphRestartCheckpoint) -> bool {
    let ending_bytes = checkpoint
        .preceding_line_physical_bytes
        .checked_sub(checkpoint.preceding_line_content_bytes);
    let ending_utf16 = checkpoint
        .preceding_line_physical_utf16
        .checked_sub(checkpoint.preceding_line_content_utf16);
    checkpoint.paragraph_source_start_byte <= checkpoint.paragraph_content_start
        && checkpoint.paragraph_content_start <= checkpoint.preceding_line_start_byte
        && checkpoint.paragraph_source_start_utf16 <= checkpoint.preceding_line_start_utf16
        && ending_bytes.is_some_and(|ending| ending <= 2)
        && ending_bytes == ending_utf16
        && checkpoint
            .preceding_line_start_byte
            .checked_add(checkpoint.preceding_line_physical_bytes)
            == Some(checkpoint.prefix_end_byte)
        && checkpoint
            .preceding_line_start_utf16
            .checked_add(checkpoint.preceding_line_physical_utf16)
            == Some(checkpoint.prefix_end_utf16)
        && checkpoint.next_physical_line_ordinal > 0
}

fn shift_u32(value: u32, delta: i64) -> Option<u32> {
    i64::from(value)
        .checked_add(delta)
        .and_then(|shifted| u32::try_from(shifted).ok())
}

fn shift_u64(value: u64, delta: i64) -> Option<u64> {
    i128::from(value)
        .checked_add(i128::from(delta))
        .and_then(|shifted| u64::try_from(shifted).ok())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11OrdinaryParagraphCheckpointError {
    Ineligible,
    AlreadyTaken,
    AllocationFailed,
}

/// One unsupported winner in Comrak's normative root-opener order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11UnsupportedOpener {
    BlockQuote,
    AtxHeading,
    FencedCode,
    HtmlBlock,
    SetextHeading,
    ThematicBreak,
    List,
    IndentedCode,
    TableCandidate,
}

/// Exact reason the narrow M1.1 result is source-backed `Unknown`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11UnknownReason {
    UnsupportedOpener(M11UnsupportedOpener),
    UnsupportedList(M11ListUnsupportedReason),
    /// Reserved for schema-v1 compatibility. The partitioned controller never
    /// emits this reason: blank source is represented by an exact `Blank` leaf.
    BlankBoundary,
}

/// Why a donor-recognized list was withheld from the exact tight-bullet
/// vertical.
///
/// These are grammar outcomes rather than host failures. The controller keeps
/// the affected source literal and never asks a renderer to infer list
/// structure from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ListUnsupportedReason {
    Ordered,
    Task,
    LazyOrMultiline,
    Loose,
    Nested,
    BlockChild,
    TabPadded,
    ExcessivePadding,
    NonTerminalEmptyItem,
}

/// Why the depth-1 block-quote slice withheld semantic certification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockQuoteUnsupportedReason {
    MarkerOnlyOrBlank,
    PartialTabMarker,
    NestedBlockQuote,
    AtxHeading,
    FencedCode,
    HtmlBlock,
    SetextHeading,
    ThematicBreak,
    List,
    IndentedCode,
    TableCandidate,
    PotentialReferenceDefinition,
    MultipleParagraphChildren,
}

/// Certification state for one parser-owned depth-1 block quote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockQuoteDisposition {
    ExactSingleParagraph,
    Unsupported(M11BlockQuoteUnsupportedReason),
}

/// Role of one exact physical-line map inside a block quote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockQuoteLineKind {
    MarkedParagraph,
    LazyParagraphContinuation,
    MarkedUnsupported,
}

/// Exact source-to-child map for one physical line owned by a block quote.
///
/// A lazy continuation has no opening marker or hidden prefix. Source ranges
/// remain physical-source coordinates; consumers must use the line sequence
/// rather than pretending the child paragraph is one contiguous source span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockQuoteLineMapping {
    pub source: Range<u32>,
    pub source_utf16: Range<u32>,
    pub opening_marker: Option<Range<u32>>,
    pub hidden_prefix: Option<Range<u32>>,
    pub content_source: Range<u32>,
    pub content_source_utf16: Range<u32>,
    pub line_ending: Range<u32>,
    pub line_ending_utf16: Range<u32>,
    pub residual_tab_columns: u8,
    pub kind: M11BlockQuoteLineKind,
}

/// The only child shape certified by this first container slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockQuoteParagraphMapping {
    pub line_indices: Range<u32>,
    pub projected_utf8_length: u32,
    pub projected_utf16_length: u32,
}

/// Exact one-line Paragraph child of one tight bullet-list Item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BulletListParagraphMapping {
    pub source: Range<u32>,
    pub source_utf16: Range<u32>,
    pub inline_source: Range<u32>,
    pub inline_source_utf16: Range<u32>,
}

/// Exact source-to-projection map for one physical bullet-list Item.
///
/// `continuation_prefix_source` excludes a possible BOF BOM while retaining
/// the certified indentation, marker, and padding spelling. It is the exact
/// item-prefix authority for both continuation and start-Backspace removal;
/// consumers must not remove the larger `hidden_prefix` when it contains BOM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BulletListItemMapping {
    pub ordinal: u32,
    pub source: Range<u32>,
    pub source_utf16: Range<u32>,
    pub opening_marker: Range<u32>,
    pub hidden_prefix: Range<u32>,
    pub hidden_prefix_utf16: Range<u32>,
    pub continuation_prefix_source: Range<u32>,
    pub continuation_prefix_source_utf16: Range<u32>,
    pub content_source: Range<u32>,
    pub content_source_utf16: Range<u32>,
    pub line_ending: Range<u32>,
    pub line_ending_utf16: Range<u32>,
    pub marker: u8,
    pub paragraph: Option<M11BulletListParagraphMapping>,
}

/// Exact one-line Paragraph child of one tight ordered-list Item.
///
/// Ordered and bullet items project their child Paragraph identically.  The
/// alias keeps that shared geometry explicit without making the established
/// bullet API source-incompatible.
pub type M11OrderedListParagraphMapping = M11BulletListParagraphMapping;

/// Exact source-to-projection map for one physical ordered-list Item.
///
/// `marker_value` is the donor-certified value of this item's authored
/// marker, not the semantic list ordinal.  CommonMark permits nonsequential
/// and zero-padded markers, so consumers need both this value and the exact
/// `opening_marker` span to implement continuation editing without reparsing
/// source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11OrderedListItemMapping {
    pub ordinal: u32,
    pub source: Range<u32>,
    pub source_utf16: Range<u32>,
    pub opening_marker: Range<u32>,
    pub hidden_prefix: Range<u32>,
    pub hidden_prefix_utf16: Range<u32>,
    pub continuation_prefix_source: Range<u32>,
    pub continuation_prefix_source_utf16: Range<u32>,
    pub content_source: Range<u32>,
    pub content_source_utf16: Range<u32>,
    pub line_ending: Range<u32>,
    pub line_ending_utf16: Range<u32>,
    pub marker_value: u32,
    pub delimiter: u8,
    pub paragraph: Option<M11OrderedListParagraphMapping>,
}

struct TightListItemGeometry {
    ordinal: u32,
    source: Range<u32>,
    source_utf16: Range<u32>,
    opening_marker: Range<u32>,
    hidden_prefix: Range<u32>,
    hidden_prefix_utf16: Range<u32>,
    continuation_prefix_source: Range<u32>,
    continuation_prefix_source_utf16: Range<u32>,
    content_source: Range<u32>,
    content_source_utf16: Range<u32>,
    line_ending: Range<u32>,
    line_ending_utf16: Range<u32>,
    paragraph: Option<M11BulletListParagraphMapping>,
}

/// One exact, source-ordered coverage leaf minted by the block controller.
///
/// Every nonempty source byte belongs to exactly one leaf. Paragraph source
/// ranges include their physical line endings but never include a following
/// blank separator. Unsupported coverage deliberately remains literal and may
/// conservatively absorb the unparsed suffix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11CleanLeaf {
    Paragraph {
        source: Range<u32>,
        source_utf16: Range<u32>,
        inline_source: Range<u32>,
        reference_definition_count: usize,
    },
    FencedCode {
        source: Range<u32>,
        source_utf16: Range<u32>,
        opening_marker: Range<u32>,
        raw_info_source: Range<u32>,
        body_source: Range<u32>,
        closing_marker: Option<Range<u32>>,
        marker: u8,
        opening_indent: u8,
    },
    IndentedCode {
        source: Range<u32>,
        source_utf16: Range<u32>,
        line_count: u32,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        terminal_eol_bytes: u32,
        has_bof_bom: bool,
    },
    BlockQuote {
        source: Range<u32>,
        source_utf16: Range<u32>,
        lines: Box<[M11BlockQuoteLineMapping]>,
        child_paragraph: Option<M11BlockQuoteParagraphMapping>,
        disposition: M11BlockQuoteDisposition,
    },
    BulletList {
        source: Range<u32>,
        source_utf16: Range<u32>,
        marker: u8,
        items: Box<[M11BulletListItemMapping]>,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        tight: bool,
    },
    OrderedList {
        source: Range<u32>,
        source_utf16: Range<u32>,
        start: u32,
        delimiter: u8,
        items: Box<[M11OrderedListItemMapping]>,
        projected_utf8_length: u32,
        projected_utf16_length: u32,
        tight: bool,
    },
    AtxHeading {
        source: Range<u32>,
        source_utf16: Range<u32>,
        opening_marker: Range<u32>,
        inline_source: Range<u32>,
        closing_marker: Option<Range<u32>>,
        line_ending: Range<u32>,
        level: u8,
        opening_indent: u8,
        has_bof_bom: bool,
    },
    SetextHeading {
        source: Range<u32>,
        source_utf16: Range<u32>,
        inline_source: Range<u32>,
        underline_marker: Range<u32>,
        underline_line_ending: Range<u32>,
        level: u8,
        opening_indent: u8,
        reference_definition_count: usize,
    },
    ThematicBreak {
        source: Range<u32>,
        source_utf16: Range<u32>,
        marker: u8,
        marker_count: u32,
        marker_envelope: Range<u32>,
        line_ending: Range<u32>,
        opening_indent: u8,
        has_bof_bom: bool,
    },
    Blank {
        source: Range<u32>,
        source_utf16: Range<u32>,
    },
    DefinitionsOnly {
        source: Range<u32>,
        source_utf16: Range<u32>,
        reference_definition_count: usize,
    },
    Unsupported {
        source: Range<u32>,
        source_utf16: Range<u32>,
        reason: M11UnknownReason,
    },
}

impl M11CleanLeaf {
    #[must_use]
    pub fn source_range(&self) -> Range<u32> {
        match self {
            Self::Paragraph { source, .. }
            | Self::FencedCode { source, .. }
            | Self::IndentedCode { source, .. }
            | Self::BlockQuote { source, .. }
            | Self::BulletList { source, .. }
            | Self::OrderedList { source, .. }
            | Self::AtxHeading { source, .. }
            | Self::SetextHeading { source, .. }
            | Self::ThematicBreak { source, .. }
            | Self::Blank { source, .. }
            | Self::DefinitionsOnly { source, .. }
            | Self::Unsupported { source, .. } => source.clone(),
        }
    }

    #[must_use]
    pub fn source_utf16_range(&self) -> Range<u32> {
        match self {
            Self::Paragraph { source_utf16, .. }
            | Self::FencedCode { source_utf16, .. }
            | Self::IndentedCode { source_utf16, .. }
            | Self::BlockQuote { source_utf16, .. }
            | Self::BulletList { source_utf16, .. }
            | Self::OrderedList { source_utf16, .. }
            | Self::AtxHeading { source_utf16, .. }
            | Self::SetextHeading { source_utf16, .. }
            | Self::ThematicBreak { source_utf16, .. }
            | Self::Blank { source_utf16, .. }
            | Self::DefinitionsOnly { source_utf16, .. }
            | Self::Unsupported { source_utf16, .. } => source_utf16.clone(),
        }
    }

    #[must_use]
    pub fn inline_source(&self) -> Option<Range<u32>> {
        match self {
            Self::Paragraph { inline_source, .. }
            | Self::AtxHeading { inline_source, .. }
            | Self::SetextHeading { inline_source, .. } => Some(inline_source.clone()),
            Self::FencedCode { .. }
            | Self::IndentedCode { .. }
            | Self::BlockQuote { .. }
            | Self::BulletList { .. }
            | Self::OrderedList { .. }
            | Self::ThematicBreak { .. }
            | Self::Blank { .. }
            | Self::DefinitionsOnly { .. }
            | Self::Unsupported { .. } => None,
        }
    }

    #[must_use]
    pub const fn reference_definition_count(&self) -> usize {
        match self {
            Self::Paragraph {
                reference_definition_count,
                ..
            }
            | Self::SetextHeading {
                reference_definition_count,
                ..
            }
            | Self::DefinitionsOnly {
                reference_definition_count,
                ..
            } => *reference_definition_count,
            Self::FencedCode { .. }
            | Self::IndentedCode { .. }
            | Self::BlockQuote { .. }
            | Self::BulletList { .. }
            | Self::OrderedList { .. }
            | Self::AtxHeading { .. }
            | Self::ThematicBreak { .. }
            | Self::Blank { .. }
            | Self::Unsupported { .. } => 0,
        }
    }

    pub(crate) fn is_definition_free_local_crop_leaf(&self) -> bool {
        self.reference_definition_count() == 0
            && matches!(
                self,
                Self::Paragraph { .. }
                    | Self::Blank { .. }
                    | Self::FencedCode { .. }
                    | Self::IndentedCode { .. }
                    | Self::BlockQuote {
                        disposition: M11BlockQuoteDisposition::ExactSingleParagraph,
                        ..
                    }
                    | Self::BulletList { .. }
                    | Self::OrderedList { .. }
                    | Self::AtxHeading { .. }
                    | Self::SetextHeading { .. }
                    | Self::ThematicBreak { .. }
            )
    }
}

/// Source-backed reference-definition cuts produced by Comrak's lexical
/// finalizer. Destination and title values never cross this boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11ReferenceDefinition {
    pub source: Range<u32>,
    pub label_source: Range<u32>,
    pub destination_source: Range<u32>,
    pub title_source: Option<Range<u32>>,
    pub normalized_label: String,
}

/// Public classification of one parser-minted exact-clean result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11CleanDocumentKind {
    Empty,
    Paragraph,
    Segmented,
    Unknown(M11UnknownReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum M11CleanDocumentOutcome {
    Empty {
        definitions: DefinitionAuthority,
    },
    Paragraph {
        visible_source: Range<u32>,
        definitions: DefinitionAuthority,
    },
    Segmented {
        definitions: DefinitionAuthority,
    },
    Unknown {
        reason: M11UnknownReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DefinitionAuthority {
    Exact(Vec<M11ReferenceDefinition>),
    ReusedLeading { count: usize },
}

impl DefinitionAuthority {
    const fn count(&self) -> usize {
        match self {
            Self::Exact(definitions) => definitions.len(),
            Self::ReusedLeading { count } => *count,
        }
    }

    fn exact(&self) -> Option<&[M11ReferenceDefinition]> {
        match self {
            Self::Exact(definitions) => Some(definitions),
            Self::ReusedLeading { .. } => None,
        }
    }

    fn append_exact(
        &mut self,
        definitions: &mut Vec<M11ReferenceDefinition>,
    ) -> Result<(), M11CleanControllerFault> {
        match self {
            Self::Exact(existing) => {
                existing.append(definitions);
                Ok(())
            }
            Self::ReusedLeading { .. } if definitions.is_empty() => Ok(()),
            Self::ReusedLeading { .. } => Err(M11CleanControllerFault::CropAcceptedDefinition),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum RestartAvailability {
    Ineligible,
    Eligible(LeadingReferencesRestartSeed),
    Taken,
}

#[derive(Debug, Eq, PartialEq)]
enum OrdinaryParagraphRestartAvailability {
    Ineligible,
    Eligible {
        seeds: Vec<OrdinaryParagraphRestartSeed>,
        top_level_block_count: u64,
    },
    Taken,
}

#[derive(Debug, Eq, PartialEq)]
struct LeadingReferencesRestartSeed {
    paragraph_content_start: u32,
    prefix_end_byte: u32,
    prefix_end_utf16: u32,
    next_physical_line_ordinal: u32,
    definition_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OrdinaryParagraphRestartSeed {
    frozen_reference_definition_count: usize,
    paragraph_source_start_byte: u32,
    paragraph_source_start_utf16: u32,
    paragraph_content_start: u32,
    block_entry_ordinal: u64,
    preceding_line_start_byte: u32,
    preceding_line_start_utf16: u32,
    preceding_line_content_bytes: u32,
    preceding_line_content_utf16: u32,
    preceding_line_physical_bytes: u32,
    preceding_line_physical_utf16: u32,
    prefix_end_byte: u32,
    prefix_end_utf16: u32,
    next_physical_line_ordinal: u32,
}

/// Move-only, allocation-free materializer for parser-authenticated restart
/// seeds accumulated by a bounded crop.
///
/// Terminal publication advances this iterator under its own fuel instead of
/// converting a crop-sized seed vector in one parser poll.
pub(crate) struct M11OrdinaryParagraphCheckpointSeedCursor {
    source: SourceVersion,
    binding: M11ParserBinding,
    seeds: std::vec::IntoIter<OrdinaryParagraphRestartSeed>,
}

impl M11OrdinaryParagraphCheckpointSeedCursor {
    fn new(
        source: SourceVersion,
        binding: M11ParserBinding,
        seeds: Vec<OrdinaryParagraphRestartSeed>,
    ) -> Self {
        Self {
            source,
            binding,
            seeds: seeds.into_iter(),
        }
    }
}

impl Iterator for M11OrdinaryParagraphCheckpointSeedCursor {
    type Item = M11OrdinaryParagraphRestartCheckpoint;

    fn next(&mut self) -> Option<Self::Item> {
        let seed = self.seeds.next()?;
        Some(M11OrdinaryParagraphRestartCheckpoint {
            source: self.source,
            binding: self.binding,
            frozen_reference_definition_count: seed.frozen_reference_definition_count,
            paragraph_source_start_byte: seed.paragraph_source_start_byte,
            paragraph_source_start_utf16: seed.paragraph_source_start_utf16,
            paragraph_content_start: seed.paragraph_content_start,
            block_entry_ordinal: seed.block_entry_ordinal,
            preceding_line_start_byte: seed.preceding_line_start_byte,
            preceding_line_start_utf16: seed.preceding_line_start_utf16,
            preceding_line_content_bytes: seed.preceding_line_content_bytes,
            preceding_line_content_utf16: seed.preceding_line_content_utf16,
            preceding_line_physical_bytes: seed.preceding_line_physical_bytes,
            preceding_line_physical_utf16: seed.preceding_line_physical_utf16,
            prefix_end_byte: seed.prefix_end_byte,
            prefix_end_utf16: seed.prefix_end_utf16,
            next_physical_line_ordinal: seed.next_physical_line_ordinal,
            state: OrdinaryParagraphAwaitingContinuation { _private: () },
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.seeds.size_hint()
    }
}

impl ExactSizeIterator for M11OrdinaryParagraphCheckpointSeedCursor {}

#[cfg(test)]
pub(crate) fn synthetic_ordinary_paragraph_restart_checkpoints(
    source: SourceVersion,
    binding: M11ParserBinding,
    count: usize,
) -> M11OrdinaryParagraphRestartCheckpoints {
    assert!(count <= source.byte_len());
    let checkpoints = (0..count)
        .map(|index| {
            let preceding = u32::try_from(index).expect("synthetic checkpoint coordinate");
            let prefix = preceding
                .checked_add(1)
                .expect("synthetic checkpoint prefix");
            M11OrdinaryParagraphRestartCheckpoint {
                source,
                binding,
                frozen_reference_definition_count: 0,
                paragraph_source_start_byte: 0,
                paragraph_source_start_utf16: 0,
                paragraph_content_start: 0,
                block_entry_ordinal: 0,
                preceding_line_start_byte: preceding,
                preceding_line_start_utf16: preceding,
                preceding_line_content_bytes: 1,
                preceding_line_content_utf16: 1,
                preceding_line_physical_bytes: 1,
                preceding_line_physical_utf16: 1,
                prefix_end_byte: prefix,
                prefix_end_utf16: prefix,
                next_physical_line_ordinal: prefix,
                state: OrdinaryParagraphAwaitingContinuation { _private: () },
            }
        })
        .collect();
    M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(source, binding, checkpoints, 1)
}

#[cfg(test)]
pub(crate) fn empty_ordinary_paragraph_checkpoint_seed_cursor(
    source: SourceVersion,
    binding: M11ParserBinding,
) -> M11OrdinaryParagraphCheckpointSeedCursor {
    M11OrdinaryParagraphCheckpointSeedCursor::new(source, binding, Vec::new())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11CommittedPhysicalLine {
    pub(crate) ordinal: u32,
    pub(crate) start_byte: u32,
    pub(crate) start_utf16: u32,
    pub(crate) content_bytes: u32,
    pub(crate) content_utf16: u32,
    pub(crate) physical_bytes: u32,
    pub(crate) physical_utf16: u32,
}

pub(crate) struct M11OrdinaryParagraphCropTerminal {
    pub(crate) source: SourceVersion,
    pub(crate) paragraph_source_start_byte: u32,
    pub(crate) paragraph_source_start_utf16: u32,
    pub(crate) paragraph_content_start: u32,
    pub(crate) replacement_start_byte: u32,
    pub(crate) replacement_start_utf16: u32,
    pub(crate) replacement_leaves: Vec<M11CleanLeaf>,
    pub(crate) crop_end_byte: u32,
    pub(crate) crop_end_utf16: u32,
    pub(crate) next_physical_line_ordinal: u32,
    pub(crate) last_committed_line: M11CommittedPhysicalLine,
    fresh_seeds: Vec<OrdinaryParagraphRestartSeed>,
}

impl M11OrdinaryParagraphCropTerminal {
    pub(crate) fn take_fresh_checkpoint_cursor(
        &mut self,
        binding: M11ParserBinding,
    ) -> M11OrdinaryParagraphCheckpointSeedCursor {
        M11OrdinaryParagraphCheckpointSeedCursor::new(
            self.source,
            binding,
            std::mem::take(&mut self.fresh_seeds),
        )
    }
}

pub(crate) struct M11OrdinaryParagraphEofCropTerminal {
    pub(crate) source: SourceVersion,
    pub(crate) replacement_start_byte: u32,
    pub(crate) replacement_start_utf16: u32,
    pub(crate) replacement_leaves: Vec<M11CleanLeaf>,
    fresh_seeds: Vec<OrdinaryParagraphRestartSeed>,
}

impl M11OrdinaryParagraphEofCropTerminal {
    pub(crate) fn take_fresh_checkpoint_cursor(
        &mut self,
        binding: M11ParserBinding,
    ) -> M11OrdinaryParagraphCheckpointSeedCursor {
        M11OrdinaryParagraphCheckpointSeedCursor::new(
            self.source,
            binding,
            std::mem::take(&mut self.fresh_seeds),
        )
    }
}

struct OrdinaryParagraphAwaitingContinuation {
    _private: (),
}

/// Parser-minted terminal result for the deliberately narrow M1.1 grammar.
///
/// Fields are intentionally opaque: arbitrary ranges or definitions cannot be
/// presented as authoritative parser output. Only a controller that consumed
/// the exact source version can construct this capability.
#[derive(Debug, Eq, PartialEq)]
pub struct M11CleanDocumentResult {
    source: SourceVersion,
    source_range: Range<u32>,
    leaves: Vec<M11CleanLeaf>,
    outcome: M11CleanDocumentOutcome,
    restart: RestartAvailability,
    ordinary_paragraph_restarts: OrdinaryParagraphRestartAvailability,
}

impl M11CleanDocumentResult {
    pub(crate) fn from_ordinary_paragraph_crop(
        source: SourceVersion,
        paragraph_content_start: u32,
    ) -> Result<Self, M11CleanControllerFault> {
        let end = u32::try_from(source.byte_len())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let end_utf16 = u32::try_from(source.utf16_len())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        if paragraph_content_start >= end {
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        }
        let mut leaves = Vec::new();
        leaves
            .try_reserve_exact(1)
            .map_err(|_| M11CleanControllerFault::LeafAllocationFailed)?;
        leaves.push(M11CleanLeaf::Paragraph {
            source: 0..end,
            source_utf16: 0..end_utf16,
            inline_source: paragraph_content_start..end,
            reference_definition_count: 0,
        });
        Ok(Self {
            source,
            source_range: 0..end,
            leaves,
            outcome: M11CleanDocumentOutcome::Paragraph {
                visible_source: paragraph_content_start..end,
                definitions: DefinitionAuthority::Exact(Vec::new()),
            },
            restart: RestartAvailability::Ineligible,
            // The crop result owns the one target-bound merged collection.
            ordinary_paragraph_restarts: OrdinaryParagraphRestartAvailability::Ineligible,
        })
    }

    #[must_use]
    pub const fn source_version(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    #[must_use]
    pub fn kind(&self) -> M11CleanDocumentKind {
        match self.outcome {
            M11CleanDocumentOutcome::Empty { .. } => M11CleanDocumentKind::Empty,
            M11CleanDocumentOutcome::Paragraph { .. } => M11CleanDocumentKind::Paragraph,
            M11CleanDocumentOutcome::Segmented { .. } => M11CleanDocumentKind::Segmented,
            M11CleanDocumentOutcome::Unknown { reason } => M11CleanDocumentKind::Unknown(reason),
        }
    }

    #[must_use]
    pub fn leaves(&self) -> &[M11CleanLeaf] {
        &self.leaves
    }

    #[must_use]
    pub fn sole_paragraph(&self) -> Option<&M11CleanLeaf> {
        match self.leaves.as_slice() {
            [leaf @ M11CleanLeaf::Paragraph { .. }] => Some(leaf),
            _ => None,
        }
    }

    #[must_use]
    pub fn has_unknown_coverage(&self) -> bool {
        self.leaves.iter().any(|leaf| {
            matches!(
                leaf,
                M11CleanLeaf::Unsupported { .. }
                    | M11CleanLeaf::BlockQuote {
                        disposition: M11BlockQuoteDisposition::Unsupported(_),
                        ..
                    }
            )
        })
    }

    /// Transfers exact block coverage to the incremental publication writer.
    ///
    /// This is deliberately crate-private: callers that only inspect parser
    /// output keep borrowing [`Self::leaves`], while the owning candidate path
    /// moves the vector without cloning document-sized coverage.
    pub(crate) fn into_publication_leaves(self) -> Vec<M11CleanLeaf> {
        self.leaves
    }

    #[must_use]
    pub fn visible_source(&self) -> Option<Range<u32>> {
        self.sole_paragraph().and_then(M11CleanLeaf::inline_source)
    }

    #[must_use]
    pub fn definitions(&self) -> &[M11ReferenceDefinition] {
        match &self.outcome {
            M11CleanDocumentOutcome::Empty { definitions }
            | M11CleanDocumentOutcome::Paragraph { definitions, .. }
            | M11CleanDocumentOutcome::Segmented { definitions } => {
                definitions.exact().unwrap_or(&[])
            }
            M11CleanDocumentOutcome::Unknown { .. } => &[],
        }
    }

    #[must_use]
    pub const fn definition_count(&self) -> usize {
        match &self.outcome {
            M11CleanDocumentOutcome::Empty { definitions }
            | M11CleanDocumentOutcome::Paragraph { definitions, .. }
            | M11CleanDocumentOutcome::Segmented { definitions } => definitions.count(),
            M11CleanDocumentOutcome::Unknown { .. } => 0,
        }
    }

    #[must_use]
    pub(crate) const fn reuses_leading_references(&self) -> bool {
        matches!(
            &self.outcome,
            M11CleanDocumentOutcome::Empty {
                definitions: DefinitionAuthority::ReusedLeading { .. },
            } | M11CleanDocumentOutcome::Paragraph {
                definitions: DefinitionAuthority::ReusedLeading { .. },
                ..
            } | M11CleanDocumentOutcome::Segmented {
                definitions: DefinitionAuthority::ReusedLeading { .. },
            }
        )
    }

    /// Mints the one move-only restart checkpoint retained by this result.
    pub fn take_leading_references_restart_checkpoint(
        &mut self,
        binding: M11ParserBinding,
    ) -> Result<LeadingReferencesRestartCheckpoint, LeadingReferencesCheckpointError> {
        let availability = std::mem::replace(&mut self.restart, RestartAvailability::Taken);
        match availability {
            RestartAvailability::Eligible(seed) => Ok(LeadingReferencesRestartCheckpoint {
                source: self.source,
                binding,
                paragraph_content_start: seed.paragraph_content_start,
                prefix_end_byte: seed.prefix_end_byte,
                prefix_end_utf16: seed.prefix_end_utf16,
                next_physical_line_ordinal: seed.next_physical_line_ordinal,
                definition_count: seed.definition_count,
                state: LeadingReferencesAwaitingRemainder { _private: () },
            }),
            RestartAvailability::Ineligible => {
                self.restart = RestartAvailability::Ineligible;
                Err(LeadingReferencesCheckpointError::Ineligible)
            }
            RestartAvailability::Taken => Err(LeadingReferencesCheckpointError::AlreadyTaken),
        }
    }

    /// Takes the sparse block-parser checkpoints authorized by this exact
    /// definition-free Paragraph terminal.
    ///
    /// The checkpoints do not authorize reuse of inline facts. Allocation for
    /// the binding-bearing move-only collection is fallible; an allocation
    /// failure leaves the collection eligible for a later retry.
    pub fn take_ordinary_paragraph_restart_checkpoints(
        &mut self,
        binding: M11ParserBinding,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        let availability = std::mem::replace(
            &mut self.ordinary_paragraph_restarts,
            OrdinaryParagraphRestartAvailability::Taken,
        );
        match availability {
            OrdinaryParagraphRestartAvailability::Eligible {
                seeds,
                top_level_block_count,
            } => {
                let mut checkpoints = Vec::new();
                if checkpoints.try_reserve_exact(seeds.len()).is_err() {
                    self.ordinary_paragraph_restarts =
                        OrdinaryParagraphRestartAvailability::Eligible {
                            seeds,
                            top_level_block_count,
                        };
                    return Err(M11OrdinaryParagraphCheckpointError::AllocationFailed);
                }
                checkpoints.extend(seeds.into_iter().map(|seed| {
                    M11OrdinaryParagraphRestartCheckpoint {
                        source: self.source,
                        binding,
                        frozen_reference_definition_count: seed.frozen_reference_definition_count,
                        paragraph_source_start_byte: seed.paragraph_source_start_byte,
                        paragraph_source_start_utf16: seed.paragraph_source_start_utf16,
                        paragraph_content_start: seed.paragraph_content_start,
                        block_entry_ordinal: seed.block_entry_ordinal,
                        preceding_line_start_byte: seed.preceding_line_start_byte,
                        preceding_line_start_utf16: seed.preceding_line_start_utf16,
                        preceding_line_content_bytes: seed.preceding_line_content_bytes,
                        preceding_line_content_utf16: seed.preceding_line_content_utf16,
                        preceding_line_physical_bytes: seed.preceding_line_physical_bytes,
                        preceding_line_physical_utf16: seed.preceding_line_physical_utf16,
                        prefix_end_byte: seed.prefix_end_byte,
                        prefix_end_utf16: seed.prefix_end_utf16,
                        next_physical_line_ordinal: seed.next_physical_line_ordinal,
                        state: OrdinaryParagraphAwaitingContinuation { _private: () },
                    }
                }));
                Ok(M11OrdinaryParagraphRestartCheckpoints {
                    source: self.source,
                    binding,
                    checkpoints,
                    top_level_block_count,
                })
            }
            OrdinaryParagraphRestartAvailability::Ineligible => {
                self.ordinary_paragraph_restarts = OrdinaryParagraphRestartAvailability::Ineligible;
                Err(M11OrdinaryParagraphCheckpointError::Ineligible)
            }
            OrdinaryParagraphRestartAvailability::Taken => {
                Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
            }
        }
    }

    pub(crate) fn take_ordinary_paragraph_checkpoint_seed_cursor(
        &mut self,
        binding: M11ParserBinding,
    ) -> Result<M11OrdinaryParagraphCheckpointSeedCursor, M11OrdinaryParagraphCheckpointError> {
        let availability = std::mem::replace(
            &mut self.ordinary_paragraph_restarts,
            OrdinaryParagraphRestartAvailability::Taken,
        );
        match availability {
            OrdinaryParagraphRestartAvailability::Eligible { seeds, .. } => Ok(
                M11OrdinaryParagraphCheckpointSeedCursor::new(self.source, binding, seeds),
            ),
            OrdinaryParagraphRestartAvailability::Ineligible => {
                self.ordinary_paragraph_restarts = OrdinaryParagraphRestartAvailability::Ineligible;
                Err(M11OrdinaryParagraphCheckpointError::Ineligible)
            }
            OrdinaryParagraphRestartAvailability::Taken => {
                Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
            }
        }
    }

    pub(crate) const fn outcome(&self) -> &M11CleanDocumentOutcome {
        &self.outcome
    }
}

/// Controller-owned failure. Grammar misses are not errors; they become a
/// typed [`M11CleanDocumentKind::Unknown`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11CleanControllerFault {
    AdmissionAlreadyActive,
    WrongController,
    StaleAdmission,
    SourceChanged {
        expected: SourceVersion,
        actual: SourceVersion,
    },
    LineOutOfSequence {
        expected_ordinal: u32,
        actual_ordinal: u32,
        expected_start: u32,
        actual_start: u32,
    },
    SourceLengthMismatch {
        expected: usize,
        actual: usize,
    },
    ZeroFuel,
    PollAfterComplete,
    PollAfterFailure,
    IncompleteAdmission,
    FactsMismatch,
    InvalidUtf8,
    MetricOverflow,
    OrdinalExhausted,
    LeafAllocationFailed,
    CheckpointAllocationFailed,
    FinishWithActiveAdmission,
    SourceUnbound,
    SourceIncomplete {
        expected: usize,
        actual: usize,
    },
    SourceUtf16Incomplete {
        expected: usize,
        actual: usize,
    },
    CropAcceptedDefinition,
    OrdinaryParagraphCropDiverged,
    DonorOverCap {
        bytes: usize,
        cap: usize,
    },
    UnsupportedDonorHtmlBlockType(u8),
}

impl fmt::Display for M11CleanControllerFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AdmissionAlreadyActive => formatter.write_str("an M1.1 admission is active"),
            Self::WrongController => formatter.write_str("admission belongs to another controller"),
            Self::StaleAdmission => formatter.write_str("admission is no longer active"),
            Self::SourceChanged { expected, actual } => {
                write!(formatter, "source changed from {expected:?} to {actual:?}")
            }
            Self::LineOutOfSequence {
                expected_ordinal,
                actual_ordinal,
                expected_start,
                actual_start,
            } => write!(
                formatter,
                "line sequence expected ordinal {expected_ordinal} at {expected_start}, received ordinal {actual_ordinal} at {actual_start}"
            ),
            Self::SourceLengthMismatch { expected, actual } => write!(
                formatter,
                "source-line length expected {expected} bytes, received {actual}"
            ),
            Self::ZeroFuel => formatter.write_str("exact-controller poll requires nonzero fuel"),
            Self::PollAfterComplete => formatter.write_str("source line was already classified"),
            Self::PollAfterFailure => formatter.write_str("source-line admission already failed"),
            Self::IncompleteAdmission => formatter.write_str("source line is not terminal"),
            Self::FactsMismatch => {
                formatter.write_str("source facts do not match classified bytes")
            }
            Self::InvalidUtf8 => formatter.write_str("source line is not valid UTF-8"),
            Self::MetricOverflow => formatter.write_str("M1.1 source metric overflow"),
            Self::OrdinalExhausted => formatter.write_str("M1.1 physical-line ordinal exhausted"),
            Self::LeafAllocationFailed => {
                formatter.write_str("clean coverage leaf allocation failed")
            }
            Self::CheckpointAllocationFailed => {
                formatter.write_str("ordinary Paragraph checkpoint allocation failed")
            }
            Self::FinishWithActiveAdmission => {
                formatter.write_str("cannot finish with an active source-line admission")
            }
            Self::SourceUnbound => {
                formatter.write_str("cannot finish before binding an exact source version")
            }
            Self::SourceIncomplete { expected, actual } => write!(
                formatter,
                "cannot finish after {actual} of {expected} immutable source bytes"
            ),
            Self::SourceUtf16Incomplete { expected, actual } => write!(
                formatter,
                "cannot finish after {actual} of {expected} immutable source UTF-16 units"
            ),
            Self::CropAcceptedDefinition => {
                formatter.write_str("crop accepted a definition after its authenticated prefix")
            }
            Self::OrdinaryParagraphCropDiverged => formatter
                .write_str("ordinary Paragraph crop did not converge in the same block state"),
            Self::DonorOverCap { bytes, cap } => {
                write!(
                    formatter,
                    "lexical donor received {bytes} bytes above its {cap}-byte cap"
                )
            }
            Self::UnsupportedDonorHtmlBlockType(kind) => {
                write!(
                    formatter,
                    "lexical donor returned unsupported HTML block type {kind}"
                )
            }
        }
    }
}

impl std::error::Error for M11CleanControllerFault {}

impl From<FacadeError> for M11CleanControllerFault {
    fn from(error: FacadeError) -> Self {
        match error {
            FacadeError::OverCap { bytes, cap } => Self::DonorOverCap { bytes, cap },
            FacadeError::UnsupportedHtmlBlockType(kind) => {
                Self::UnsupportedDonorHtmlBlockType(kind)
            }
        }
    }
}

impl From<SegmentedReferenceError> for M11CleanControllerFault {
    fn from(error: SegmentedReferenceError) -> Self {
        match error {
            SegmentedReferenceError::NonSequential { .. } => Self::FactsMismatch,
            SegmentedReferenceError::InvalidUtf8 => Self::InvalidUtf8,
            SegmentedReferenceError::MetricOverflow => Self::MetricOverflow,
        }
    }
}

/// A source error or a controller-owned failure from one poll lifecycle.
#[derive(Debug, Eq, PartialEq)]
pub enum M11CleanControllerError<E> {
    Controller(M11CleanControllerFault),
    Source(E),
}

impl<E> From<M11CleanControllerFault> for M11CleanControllerError<E> {
    fn from(error: M11CleanControllerFault) -> Self {
        Self::Controller(error)
    }
}

impl<E: fmt::Display> fmt::Display for M11CleanControllerError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => error.fmt(formatter),
            Self::Source(error) => write!(formatter, "source-line read failed: {error}"),
        }
    }
}

impl<E> std::error::Error for M11CleanControllerError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Controller(error) => Some(error),
            Self::Source(error) => Some(error),
        }
    }
}

/// Flark-owned resumable controller for the first exact-clean grammar slice.
///
/// Comrak 0.54 supplies lexical decisions only. This controller owns source
/// identity, bounded polling, normative opener order, definition chronology,
/// and the terminal admitted/Unknown result.
pub struct M11CleanBlockController {
    controller_id: u64,
    source: Option<SourceVersion>,
    next_ordinal: u32,
    next_start: u32,
    next_utf16: u32,
    next_admission: u64,
    active_admission: Option<u64>,
    state: DocumentState,
    leaves: Vec<M11CleanLeaf>,
    leaf_coverage_start: SourceCut,
    block_entry_ordinal_base: u64,
    inherited_ordinary_restart_block_entry_ordinal: Option<u64>,
    definitions: DefinitionAuthority,
    frozen_reference_definition_count: usize,
    ordinary_paragraph_restart_seeds: Vec<OrdinaryParagraphRestartSeed>,
    next_ordinary_paragraph_checkpoint_byte: u32,
    last_committed_line: Option<M11CommittedPhysicalLine>,
}

impl Default for M11CleanBlockController {
    fn default() -> Self {
        Self::new()
    }
}

impl M11CleanBlockController {
    #[must_use]
    pub fn new() -> Self {
        Self {
            controller_id: CONTROLLER_IDS.fetch_add(1, Ordering::Relaxed),
            source: None,
            next_ordinal: 0,
            next_start: 0,
            next_utf16: 0,
            next_admission: 1,
            active_admission: None,
            state: DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
            leaves: Vec::new(),
            leaf_coverage_start: SourceCut { byte: 0, utf16: 0 },
            block_entry_ordinal_base: 0,
            inherited_ordinary_restart_block_entry_ordinal: None,
            definitions: DefinitionAuthority::Exact(Vec::new()),
            frozen_reference_definition_count: 0,
            ordinary_paragraph_restart_seeds: Vec::new(),
            next_ordinary_paragraph_checkpoint_byte: M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES,
            last_committed_line: None,
        }
    }

    /// Creates a controller pre-bound to the immutable source version.
    ///
    /// Pre-binding is required for empty documents, which have no physical
    /// line identity from which the controller could otherwise learn source
    /// authority.
    #[must_use]
    pub fn new_for_source(source: SourceVersion) -> Self {
        let mut controller = Self::new();
        controller.source = Some(source);
        controller
    }

    pub(crate) fn new_for_leading_references_remainder(
        target: SourceVersion,
        checkpoint: LeadingReferencesRestartCheckpoint,
    ) -> Self {
        let LeadingReferencesRestartCheckpoint {
            paragraph_content_start,
            prefix_end_byte,
            prefix_end_utf16,
            next_physical_line_ordinal,
            definition_count,
            state,
            ..
        } = checkpoint;
        let LeadingReferencesAwaitingRemainder { _private: () } = state;
        Self {
            controller_id: CONTROLLER_IDS.fetch_add(1, Ordering::Relaxed),
            source: Some(target),
            next_ordinal: next_physical_line_ordinal,
            next_start: prefix_end_byte,
            next_utf16: prefix_end_utf16,
            next_admission: 1,
            active_admission: None,
            state: DocumentState::Paragraph(ParagraphState {
                source_start: SourceCut { byte: 0, utf16: 0 },
                content_start: paragraph_content_start,
                reference: Some(SegmentedReferencePrefix::awaiting_remainder(
                    prefix_end_byte,
                    definition_count,
                )),
                definitions: DefinitionAuthority::ReusedLeading {
                    count: definition_count,
                },
                visible_start: None,
                leading_restart_cut: None,
            }),
            leaves: Vec::new(),
            leaf_coverage_start: SourceCut { byte: 0, utf16: 0 },
            block_entry_ordinal_base: 0,
            inherited_ordinary_restart_block_entry_ordinal: None,
            definitions: DefinitionAuthority::Exact(Vec::new()),
            frozen_reference_definition_count: definition_count,
            ordinary_paragraph_restart_seeds: Vec::new(),
            next_ordinary_paragraph_checkpoint_byte: prefix_end_byte
                .saturating_add(M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES),
            last_committed_line: None,
        }
    }

    /// Seeds only the block-level state of one exact definition-free Paragraph.
    ///
    /// The caller must independently prove that the checkpoint prefix is exact
    /// for `target`. No inline state is carried across this boundary.
    pub(crate) fn new_for_ordinary_paragraph_remainder(
        target: SourceVersion,
        checkpoint: M11OrdinaryParagraphRestartCheckpoint,
    ) -> Self {
        let M11OrdinaryParagraphRestartCheckpoint {
            paragraph_source_start_byte,
            paragraph_source_start_utf16,
            paragraph_content_start,
            block_entry_ordinal,
            frozen_reference_definition_count,
            preceding_line_start_byte,
            preceding_line_start_utf16,
            preceding_line_content_bytes,
            preceding_line_content_utf16,
            preceding_line_physical_bytes,
            preceding_line_physical_utf16,
            prefix_end_byte,
            prefix_end_utf16,
            next_physical_line_ordinal,
            state,
            ..
        } = checkpoint;
        let OrdinaryParagraphAwaitingContinuation { _private: () } = state;
        Self {
            controller_id: CONTROLLER_IDS.fetch_add(1, Ordering::Relaxed),
            source: Some(target),
            next_ordinal: next_physical_line_ordinal,
            next_start: prefix_end_byte,
            next_utf16: prefix_end_utf16,
            next_admission: 1,
            active_admission: None,
            state: DocumentState::Paragraph(ParagraphState {
                source_start: SourceCut {
                    byte: paragraph_source_start_byte,
                    utf16: paragraph_source_start_utf16,
                },
                content_start: paragraph_content_start,
                reference: None,
                definitions: DefinitionAuthority::Exact(Vec::new()),
                visible_start: Some(paragraph_content_start),
                leading_restart_cut: None,
            }),
            leaves: Vec::new(),
            leaf_coverage_start: SourceCut {
                byte: paragraph_source_start_byte,
                utf16: paragraph_source_start_utf16,
            },
            block_entry_ordinal_base: block_entry_ordinal,
            inherited_ordinary_restart_block_entry_ordinal: Some(block_entry_ordinal),
            definitions: DefinitionAuthority::Exact(Vec::new()),
            frozen_reference_definition_count,
            ordinary_paragraph_restart_seeds: Vec::new(),
            next_ordinary_paragraph_checkpoint_byte: prefix_end_byte
                .saturating_add(M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES),
            last_committed_line: Some(M11CommittedPhysicalLine {
                ordinal: next_physical_line_ordinal.saturating_sub(1),
                start_byte: preceding_line_start_byte,
                start_utf16: preceding_line_start_utf16,
                content_bytes: preceding_line_content_bytes,
                content_utf16: preceding_line_content_utf16,
                physical_bytes: preceding_line_physical_bytes,
                physical_utf16: preceding_line_physical_utf16,
            }),
        }
    }

    pub(crate) fn finish_ordinary_paragraph_crop(
        self,
        expected_end_byte: u32,
        expected_end_utf16: u32,
    ) -> Result<M11OrdinaryParagraphCropTerminal, M11CleanControllerFault> {
        if self.active_admission.is_some() {
            return Err(M11CleanControllerFault::FinishWithActiveAdmission);
        }
        if self.next_start != expected_end_byte || self.next_utf16 != expected_end_utf16 {
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        }
        let source = self.source.ok_or(M11CleanControllerFault::SourceUnbound)?;
        let DocumentState::Paragraph(paragraph) = self.state else {
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        };
        if !paragraph_is_ordinary_definition_free(&paragraph)
            || !matches!(&self.definitions, DefinitionAuthority::Exact(definitions) if definitions.is_empty())
        {
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        }
        let replacement_end = paragraph.source_start;
        if !leaves_partition_range(&self.leaves, self.leaf_coverage_start, replacement_end) {
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        }
        Ok(M11OrdinaryParagraphCropTerminal {
            source,
            paragraph_source_start_byte: paragraph.source_start.byte,
            paragraph_source_start_utf16: paragraph.source_start.utf16,
            paragraph_content_start: paragraph.content_start,
            replacement_start_byte: self.leaf_coverage_start.byte,
            replacement_start_utf16: self.leaf_coverage_start.utf16,
            replacement_leaves: self.leaves,
            crop_end_byte: self.next_start,
            crop_end_utf16: self.next_utf16,
            next_physical_line_ordinal: self.next_ordinal,
            last_committed_line: self
                .last_committed_line
                .ok_or(M11CleanControllerFault::OrdinaryParagraphCropDiverged)?,
            fresh_seeds: self.ordinary_paragraph_restart_seeds,
        })
    }

    pub(crate) fn finish_ordinary_paragraph_eof_crop(
        mut self,
    ) -> Result<M11OrdinaryParagraphEofCropTerminal, M11CleanControllerFault> {
        if self.active_admission.is_some() {
            return Err(M11CleanControllerFault::FinishWithActiveAdmission);
        }
        let source = self.source.ok_or(M11CleanControllerFault::SourceUnbound)?;
        let actual = usize::try_from(self.next_start)
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        if actual != source.byte_len() {
            return Err(M11CleanControllerFault::SourceIncomplete {
                expected: source.byte_len(),
                actual,
            });
        }
        let actual_utf16 = usize::try_from(self.next_utf16)
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        if actual_utf16 != source.utf16_len() {
            return Err(M11CleanControllerFault::SourceUtf16Incomplete {
                expected: source.utf16_len(),
                actual: actual_utf16,
            });
        }

        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..self.next_start,
                        source_utf16: start.utf16..self.next_utf16,
                    })?;
                }
            }
            DocumentState::UnknownSuffix { .. } => {
                return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
            }
            DocumentState::BlockQuote(quote) => {
                self.push_finished_block_quote(
                    quote,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                )?;
            }
            DocumentState::TightList(list) => {
                let pending = self.push_finished_tight_list(list)?;
                if let Some(start) = pending {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..self.next_start,
                        source_utf16: start.utf16..self.next_utf16,
                    })?;
                }
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                    self.next_ordinal,
                    true,
                )?;
                self.push_finished_paragraph(
                    paragraph,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                )?;
            }
            DocumentState::FencedCode(fence) => {
                self.push_finished_fenced_code(
                    fence,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                    self.next_start,
                    None,
                )?;
            }
            DocumentState::IndentedCode(code) => {
                self.push_finished_indented_code(
                    code,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                )?;
            }
        }

        if !matches!(&self.definitions, DefinitionAuthority::Exact(definitions) if definitions.is_empty())
            || self
                .leaves
                .iter()
                .any(|leaf| !leaf.is_definition_free_local_crop_leaf())
            || !leaves_partition_range(
                &self.leaves,
                self.leaf_coverage_start,
                SourceCut {
                    byte: self.next_start,
                    utf16: self.next_utf16,
                },
            )
        {
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        }

        Ok(M11OrdinaryParagraphEofCropTerminal {
            source,
            replacement_start_byte: self.leaf_coverage_start.byte,
            replacement_start_utf16: self.leaf_coverage_start.utf16,
            replacement_leaves: self.leaves,
            fresh_seeds: self.ordinary_paragraph_restart_seeds,
        })
    }

    /// Finalizes the exact-clean result after the source scanner reports
    /// `Complete`.
    ///
    /// # Errors
    ///
    /// Returns a controller fault if source-line work remains active or the
    /// bounded lexical donor violates its pinned contract.
    pub fn finish(mut self) -> Result<M11CleanDocumentResult, M11CleanControllerFault> {
        if self.active_admission.is_some() {
            return Err(M11CleanControllerFault::FinishWithActiveAdmission);
        }
        let actual = usize::try_from(self.next_start)
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let source_version = self.source.ok_or(M11CleanControllerFault::SourceUnbound)?;
        let expected = source_version.byte_len();
        if actual != expected {
            return Err(M11CleanControllerFault::SourceIncomplete { expected, actual });
        }
        let actual_utf16 = usize::try_from(self.next_utf16)
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let expected_utf16 = source_version.utf16_len();
        if actual_utf16 != expected_utf16 {
            return Err(M11CleanControllerFault::SourceUtf16Incomplete {
                expected: expected_utf16,
                actual: actual_utf16,
            });
        }
        let source = 0..self.next_start;
        let mut restart = RestartAvailability::Ineligible;
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..self.next_start,
                        source_utf16: start.utf16..self.next_utf16,
                    })?;
                }
            }
            DocumentState::UnknownSuffix {
                source_start,
                reason,
            } => {
                // An unparsed suffix may contain definitions that affect the
                // whole document. Prefix definitions are therefore not a
                // complete References authority.
                self.definitions = DefinitionAuthority::Exact(Vec::new());
                self.push_leaf(M11CleanLeaf::Unsupported {
                    source: source_start.byte..self.next_start,
                    source_utf16: source_start.utf16..self.next_utf16,
                    reason,
                })?;
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                    self.next_ordinal,
                    true,
                )?;
                if self.leaves.is_empty() && paragraph.source_start.byte == 0 {
                    restart = restart_availability(&paragraph);
                }
                self.push_finished_paragraph(
                    paragraph,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                )?;
            }
            DocumentState::FencedCode(fence) => {
                self.push_finished_fenced_code(
                    fence,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                    self.next_start,
                    None,
                )?;
            }
            DocumentState::IndentedCode(code) => {
                self.push_finished_indented_code(
                    code,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                )?;
            }
            DocumentState::BlockQuote(quote) => {
                self.push_finished_block_quote(
                    quote,
                    SourceCut {
                        byte: self.next_start,
                        utf16: self.next_utf16,
                    },
                )?;
            }
            DocumentState::TightList(list) => {
                let pending = self.push_finished_tight_list(list)?;
                if let Some(start) = pending {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..self.next_start,
                        source_utf16: start.utf16..self.next_utf16,
                    })?;
                }
            }
        }
        if !leaves_partition_source(&self.leaves, self.next_start, self.next_utf16) {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let ordinary_paragraph_restarts = ordinary_document_restart_availability(
            &self.definitions,
            &self.leaves,
            std::mem::take(&mut self.ordinary_paragraph_restart_seeds),
        );
        let outcome = match self.leaves.as_slice() {
            [] | [M11CleanLeaf::DefinitionsOnly { .. }] => M11CleanDocumentOutcome::Empty {
                definitions: self.definitions,
            },
            [M11CleanLeaf::Paragraph { inline_source, .. }] => M11CleanDocumentOutcome::Paragraph {
                visible_source: inline_source.clone(),
                definitions: self.definitions,
            },
            [M11CleanLeaf::Unsupported { reason, .. }] => {
                M11CleanDocumentOutcome::Unknown { reason: *reason }
            }
            _ => M11CleanDocumentOutcome::Segmented {
                definitions: self.definitions,
            },
        };
        Ok(M11CleanDocumentResult {
            source: source_version,
            source_range: source,
            leaves: self.leaves,
            outcome,
            restart,
            ordinary_paragraph_restarts,
        })
    }

    fn push_leaf(&mut self, leaf: M11CleanLeaf) -> Result<(), M11CleanControllerFault> {
        let source = leaf.source_range();
        let source_utf16 = leaf.source_utf16_range();
        let (expected_start, expected_start_utf16) = self.leaves.last().map_or(
            (
                self.leaf_coverage_start.byte,
                self.leaf_coverage_start.utf16,
            ),
            |previous| {
                (
                    previous.source_range().end,
                    previous.source_utf16_range().end,
                )
            },
        );
        if source.start != expected_start
            || source_utf16.start != expected_start_utf16
            || source.start >= source.end
            || source_utf16.start >= source_utf16.end
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        self.leaves
            .try_reserve(1)
            .map_err(|_| M11CleanControllerFault::LeafAllocationFailed)?;
        self.leaves.push(leaf);
        Ok(())
    }

    fn finish_pending_gap(&mut self, end: SourceCut) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::BetweenBlocks { pending_gap_start } = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        if let Some(start) = pending_gap_start {
            self.push_leaf(M11CleanLeaf::Blank {
                source: start.byte..end.byte,
                source_utf16: start.utf16..end.utf16,
            })?;
        }
        Ok(())
    }

    fn apply_blank(&mut self, start: SourceCut) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                self.state = DocumentState::BetweenBlocks {
                    pending_gap_start: Some(pending_gap_start.unwrap_or(start)),
                };
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(&mut paragraph, start, self.next_ordinal, false)?;
                self.push_finished_paragraph(paragraph, start)?;
                self.state = DocumentState::BetweenBlocks {
                    pending_gap_start: Some(start),
                };
            }
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
            state @ DocumentState::UnknownSuffix { .. } => {
                self.state = state;
            }
        }
        Ok(())
    }

    fn apply_unsupported(
        &mut self,
        line_start: SourceCut,
        reason: M11UnknownReason,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let source_start = match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..line_start.byte,
                        source_utf16: start.utf16..line_start.utf16,
                    })?;
                }
                line_start
            }
            DocumentState::Paragraph(paragraph) => paragraph.source_start,
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
            state @ DocumentState::UnknownSuffix { .. } => {
                self.state = state;
                return Ok(());
            }
        };
        self.ordinary_paragraph_restart_seeds.clear();
        self.state = DocumentState::UnknownSuffix {
            source_start,
            reason,
        };
        Ok(())
    }

    fn apply_atx_heading(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        heading: SegmentedAtxHeadingFacts,
        opening_indent: u8,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..line_start.byte,
                        source_utf16: start.utf16..line_start.utf16,
                    })?;
                }
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    line_start,
                    self.next_ordinal,
                    false,
                )?;
                self.push_finished_paragraph(paragraph, line_start)?;
            }
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)
            | DocumentState::UnknownSuffix { .. }) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
        }
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let valid_span = |span: crate::segmented_lexical::SegmentedLineSpan| {
            span.start <= span.end && span.end <= physical_bytes
        };
        if !(1..=6).contains(&heading.level)
            || opening_indent > 3
            || has_bof_bom && line_start.byte != 0
            || !valid_span(heading.opening_marker)
            || !valid_span(heading.content)
            || !valid_span(heading.line_ending)
            || heading.opening_marker.start >= heading.opening_marker.end
            || heading.opening_marker.end - heading.opening_marker.start
                != usize::from(heading.level)
            || heading.opening_marker.start
                != usize::from(opening_indent) + if has_bof_bom { 3 } else { 0 }
            || heading.opening_marker.end > heading.content.start
            || heading.content.end > heading.line_ending.start
            || heading.line_ending.start != content_bytes
            || heading.line_ending.end != physical_bytes
            || heading.closing_marker.is_some_and(|closing| {
                !valid_span(closing)
                    || closing.start >= closing.end
                    || heading.content.end > closing.start
                    || closing.end > heading.line_ending.start
            })
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }

        let absolute = |span: crate::segmented_lexical::SegmentedLineSpan| {
            Ok::<_, M11CleanControllerFault>(
                add_u32_usize(line_start.byte, span.start)?
                    ..add_u32_usize(line_start.byte, span.end)?,
            )
        };
        let source_end_utf16 = line_start
            .utf16
            .checked_add(physical.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        self.push_leaf(M11CleanLeaf::AtxHeading {
            source: line_start.byte..physical.identity().end_byte(),
            source_utf16: line_start.utf16..source_end_utf16,
            opening_marker: absolute(heading.opening_marker)?,
            inline_source: absolute(heading.content)?,
            closing_marker: heading.closing_marker.map(absolute).transpose()?,
            line_ending: absolute(heading.line_ending)?,
            level: heading.level,
            opening_indent,
            has_bof_bom,
        })
    }

    fn apply_setext_heading(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        heading: SegmentedSetextHeadingFacts,
        opening_indent: u8,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::Paragraph(paragraph) = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        if paragraph.reference.is_some() {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        }
        let current_block_entry_ordinal = self
            .block_entry_ordinal_base
            .checked_add(
                u64::try_from(self.leaves.len())
                    .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
            )
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        if self.inherited_ordinary_restart_block_entry_ordinal == Some(current_block_entry_ordinal)
        {
            // A restart inside the promoted Paragraph is not valid authority
            // inside the target Setext leaf. Decline this crop so the clean
            // lane can rebuild both topology and restart authority.
            return Err(M11CleanControllerFault::OrdinaryParagraphCropDiverged);
        }

        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let valid_span = |span: crate::segmented_lexical::SegmentedLineSpan| {
            span.start <= span.end && span.end <= physical_bytes
        };
        if !matches!(heading.level, 1 | 2)
            || opening_indent > 3
            || !valid_span(heading.underline_marker)
            || !valid_span(heading.line_ending)
            || heading.underline_marker.start != usize::from(opening_indent)
            || heading.underline_marker.start >= heading.underline_marker.end
            || heading.underline_marker.end > heading.line_ending.start
            || heading.line_ending.start != content_bytes
            || heading.line_ending.end != physical_bytes
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }

        let inline_start = paragraph
            .visible_start
            .ok_or(M11CleanControllerFault::IncompleteAdmission)?;
        let content_line = self
            .last_committed_line
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        let content_line_end_byte = content_line
            .start_byte
            .checked_add(content_line.physical_bytes)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let content_line_end_utf16 = content_line
            .start_utf16
            .checked_add(content_line.physical_utf16)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let inline_end = content_line
            .start_byte
            .checked_add(content_line.content_bytes)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let inline_end_utf16 = content_line
            .start_utf16
            .checked_add(content_line.content_utf16)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let content_ending_bytes = content_line
            .physical_bytes
            .checked_sub(content_line.content_bytes)
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        let content_ending_utf16 = content_line
            .physical_utf16
            .checked_sub(content_line.content_utf16)
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        if content_line.ordinal.checked_add(1) != Some(physical.identity().ordinal())
            || content_line_end_byte != line_start.byte
            || content_line_end_utf16 != line_start.utf16
            || !matches!(content_ending_bytes, 1 | 2)
            || content_ending_bytes != content_ending_utf16
            || inline_end_utf16 >= line_start.utf16
            || inline_start >= inline_end
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let absolute = |span: crate::segmented_lexical::SegmentedLineSpan| {
            Ok::<_, M11CleanControllerFault>(
                add_u32_usize(line_start.byte, span.start)?
                    ..add_u32_usize(line_start.byte, span.end)?,
            )
        };
        let source_end_utf16 = line_start
            .utf16
            .checked_add(physical.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let reference_definition_count = paragraph.definitions.count();
        self.ordinary_paragraph_restart_seeds.retain(|seed| {
            seed.paragraph_source_start_byte != paragraph.source_start.byte
                || seed.paragraph_source_start_utf16 != paragraph.source_start.utf16
        });
        append_definition_authority(&mut self.definitions, paragraph.definitions)?;
        self.push_leaf(M11CleanLeaf::SetextHeading {
            source: paragraph.source_start.byte..physical.identity().end_byte(),
            source_utf16: paragraph.source_start.utf16..source_end_utf16,
            inline_source: inline_start..inline_end,
            underline_marker: absolute(heading.underline_marker)?,
            underline_line_ending: absolute(heading.line_ending)?,
            level: heading.level,
            opening_indent,
            reference_definition_count,
        })
    }

    fn apply_thematic_break(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        thematic: SegmentedThematicBreakFacts,
        opening_indent: u8,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..line_start.byte,
                        source_utf16: start.utf16..line_start.utf16,
                    })?;
                }
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    line_start,
                    self.next_ordinal,
                    false,
                )?;
                self.push_finished_paragraph(paragraph, line_start)?;
            }
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)
            | DocumentState::UnknownSuffix { .. }) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
        }

        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let marker_width = thematic
            .marker_envelope
            .end
            .checked_sub(thematic.marker_envelope.start)
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        if !matches!(thematic.marker, b'*' | b'-' | b'_')
            || thematic.marker_count < 3
            || thematic.marker_count > marker_width
            || opening_indent > 3
            || has_bof_bom && line_start.byte != 0
            || thematic.marker_envelope.start
                != usize::from(opening_indent) + if has_bof_bom { 3 } else { 0 }
            || thematic.marker_envelope.start >= thematic.marker_envelope.end
            || thematic.marker_envelope.end > thematic.line_ending.start
            || thematic.line_ending.start != content_bytes
            || thematic.line_ending.end != physical_bytes
            || thematic.line_ending.end - thematic.line_ending.start > 2
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }

        let absolute = |span: crate::segmented_lexical::SegmentedLineSpan| {
            Ok::<_, M11CleanControllerFault>(
                add_u32_usize(line_start.byte, span.start)?
                    ..add_u32_usize(line_start.byte, span.end)?,
            )
        };
        let source_end_utf16 = line_start
            .utf16
            .checked_add(physical.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        self.push_leaf(M11CleanLeaf::ThematicBreak {
            source: line_start.byte..physical.identity().end_byte(),
            source_utf16: line_start.utf16..source_end_utf16,
            marker: thematic.marker,
            marker_count: u32::try_from(thematic.marker_count)
                .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
            marker_envelope: absolute(thematic.marker_envelope)?,
            line_ending: absolute(thematic.line_ending)?,
            opening_indent,
            has_bof_bom,
        })
    }

    fn apply_start_fenced_code(
        &mut self,
        line_start: SourceCut,
        facts: M11PhysicalLineFacts,
        marker: u8,
        opening_run_length: u32,
        opening_indent: u8,
        opening_marker_start: u32,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..line_start.byte,
                        source_utf16: start.utf16..line_start.utf16,
                    })?;
                }
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    line_start,
                    self.next_ordinal,
                    false,
                )?;
                self.push_finished_paragraph(paragraph, line_start)?;
            }
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)
            | DocumentState::UnknownSuffix { .. }) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
        }
        let opening_marker_end = opening_marker_start
            .checked_add(opening_run_length)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let content_end = line_start
            .byte
            .checked_add(facts.content_bytes())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        if !matches!(marker, b'`' | b'~')
            || opening_indent > 3
            || opening_run_length < 3
            || opening_marker_start < line_start.byte
            || opening_marker_end > content_end
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        self.state = DocumentState::FencedCode(FencedCodeState {
            source_start: line_start,
            opening_marker: opening_marker_start..opening_marker_end,
            raw_info_source: opening_marker_end..content_end,
            body_start: facts.identity().end_byte(),
            marker,
            opening_indent,
        });
        Ok(())
    }

    fn apply_close_fenced_code(
        &mut self,
        line_start: SourceCut,
        facts: M11PhysicalLineFacts,
        closing_run_length: u32,
        closing_marker_start: u32,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::FencedCode(fence) = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let opening_run_length = fence.opening_marker.end - fence.opening_marker.start;
        let closing_marker_end = closing_marker_start
            .checked_add(closing_run_length)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let content_end = line_start
            .byte
            .checked_add(facts.content_bytes())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        if closing_run_length < opening_run_length
            || closing_marker_start < line_start.byte
            || closing_marker_end > content_end
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let source_end = SourceCut {
            byte: facts.identity().end_byte(),
            utf16: line_start
                .utf16
                .checked_add(facts.physical_utf16())
                .ok_or(M11CleanControllerFault::MetricOverflow)?,
        };
        self.push_finished_fenced_code(
            fence,
            source_end,
            line_start.byte,
            Some(closing_marker_start..closing_marker_end),
        )?;
        self.state = DocumentState::BetweenBlocks {
            pending_gap_start: None,
        };
        Ok(())
    }

    fn finish_paragraph_reference(
        paragraph: &mut ParagraphState,
        end: SourceCut,
        next_physical_line_ordinal: u32,
        permit_terminal_restart: bool,
    ) -> Result<(), M11CleanControllerFault> {
        let Some(mut reference) = paragraph.reference.take() else {
            return Ok(());
        };
        let mut completed = reference
            .finish_eof()?
            .into_iter()
            .map(map_segmented_definition)
            .collect();
        paragraph.definitions.append_exact(&mut completed)?;
        if reference.definition_count() != paragraph.definitions.count() {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let terminal = reference
            .terminal()
            .ok_or(M11CleanControllerFault::IncompleteAdmission)?;
        match terminal {
            SegmentedReferenceTerminal::ReferenceOnly { .. }
                if paragraph.definitions.count() != 0 =>
            {
                if permit_terminal_restart
                    && matches!(&paragraph.definitions, DefinitionAuthority::Exact(_))
                    && terminal.prefix_end(reference.base()) == end.byte
                {
                    paragraph.leading_restart_cut = Some(LeadingRestartCut {
                        byte_end: end.byte,
                        utf16_end: end.utf16,
                        next_physical_line_ordinal,
                        definition_count: paragraph.definitions.count(),
                    });
                }
            }
            terminal => {
                paragraph.visible_start = Some(reference_visible_start(
                    terminal,
                    reference.base(),
                    paragraph.content_start,
                    paragraph.definitions.count() == 0,
                ));
            }
        }
        Ok(())
    }

    fn push_finished_paragraph(
        &mut self,
        paragraph: ParagraphState,
        source_end: SourceCut,
    ) -> Result<(), M11CleanControllerFault> {
        let ParagraphState {
            source_start,
            content_start,
            reference,
            definitions,
            visible_start,
            ..
        } = paragraph;
        if reference.is_some()
            || source_start.byte >= source_end.byte
            || source_start.utf16 >= source_end.utf16
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let definition_count = definitions.count();
        let leaf = match visible_start {
            Some(start) => {
                if start >= source_end.byte {
                    return Err(M11CleanControllerFault::FactsMismatch);
                }
                M11CleanLeaf::Paragraph {
                    source: source_start.byte..source_end.byte,
                    source_utf16: source_start.utf16..source_end.utf16,
                    inline_source: start..source_end.byte,
                    reference_definition_count: definition_count,
                }
            }
            None if definition_count != 0 => M11CleanLeaf::DefinitionsOnly {
                source: source_start.byte..source_end.byte,
                source_utf16: source_start.utf16..source_end.utf16,
                reference_definition_count: definition_count,
            },
            None => {
                if content_start >= source_end.byte {
                    return Err(M11CleanControllerFault::FactsMismatch);
                }
                M11CleanLeaf::Paragraph {
                    source: source_start.byte..source_end.byte,
                    source_utf16: source_start.utf16..source_end.utf16,
                    inline_source: content_start..source_end.byte,
                    reference_definition_count: 0,
                }
            }
        };
        append_definition_authority(&mut self.definitions, definitions)?;
        self.push_leaf(leaf)
    }

    fn push_finished_fenced_code(
        &mut self,
        fence: FencedCodeState,
        source_end: SourceCut,
        body_end: u32,
        closing_marker: Option<Range<u32>>,
    ) -> Result<(), M11CleanControllerFault> {
        if fence.source_start.byte >= source_end.byte
            || fence.source_start.utf16 >= source_end.utf16
            || fence.body_start > body_end
            || body_end > source_end.byte
            || fence.opening_marker.start < fence.source_start.byte
            || fence.opening_marker.start >= fence.opening_marker.end
            || fence.opening_marker.end > fence.raw_info_source.start
            || fence.raw_info_source.start > fence.raw_info_source.end
            || fence.raw_info_source.end > fence.body_start
            || closing_marker.as_ref().is_some_and(|marker| {
                marker.start < body_end
                    || marker.start >= marker.end
                    || marker.end > source_end.byte
            })
            || !matches!(fence.marker, b'`' | b'~')
            || fence.opening_indent > 3
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        self.push_leaf(M11CleanLeaf::FencedCode {
            source: fence.source_start.byte..source_end.byte,
            source_utf16: fence.source_start.utf16..source_end.utf16,
            opening_marker: fence.opening_marker,
            raw_info_source: fence.raw_info_source,
            body_source: fence.body_start..body_end,
            closing_marker,
            marker: fence.marker,
            opening_indent: fence.opening_indent,
        })
    }

    fn summarize_indented_code_line(
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        code: SegmentedIndentedCodeLineFacts,
        has_bof_bom: bool,
    ) -> Result<IndentedLineSummary, M11CleanControllerFault> {
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let prefix_bytes = code.hidden_prefix.end;
        let eol_bytes = code
            .line_ending
            .end
            .checked_sub(code.line_ending.start)
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        if code.hidden_prefix.start != 0
            || code.hidden_prefix.end != code.content.start
            || code.content.start > code.content.end
            || code.content.end != code.line_ending.start
            || code.line_ending.start != content_bytes
            || code.line_ending.end != physical_bytes
            || eol_bytes > 2
            || has_bof_bom && (line_start.byte != 0 || prefix_bytes < 3)
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let prefix_utf16 = u32::try_from(prefix_bytes)
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?
            .checked_sub(if has_bof_bom { 2 } else { 0 })
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        let projected_utf8_length = physical
            .physical_bytes()
            .checked_sub(
                u32::try_from(prefix_bytes).map_err(|_| M11CleanControllerFault::MetricOverflow)?,
            )
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        let projected_utf16_length = physical
            .physical_utf16()
            .checked_sub(prefix_utf16)
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        Ok(IndentedLineSummary {
            source_end: SourceCut {
                byte: physical.identity().end_byte(),
                utf16: line_start
                    .utf16
                    .checked_add(physical.physical_utf16())
                    .ok_or(M11CleanControllerFault::MetricOverflow)?,
            },
            projected_utf8_length,
            projected_utf16_length,
            terminal_eol_bytes: u32::try_from(eol_bytes)
                .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
        })
    }

    fn apply_start_indented_code(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        code: SegmentedIndentedCodeLineFacts,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        self.finish_pending_gap(line_start)?;
        let summary = Self::summarize_indented_code_line(line_start, physical, code, has_bof_bom)?;
        if code.content.start >= code.content.end {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        self.state = DocumentState::IndentedCode(IndentedCodeState {
            source_start: line_start,
            source_end: summary.source_end,
            line_count: 1,
            projected_utf8_length: summary.projected_utf8_length,
            projected_utf16_length: summary.projected_utf16_length,
            terminal_eol_bytes: summary.terminal_eol_bytes,
            has_bof_bom,
            pending_blanks: None,
        });
        Ok(())
    }

    fn apply_continue_indented_code(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        code: SegmentedIndentedCodeLineFacts,
    ) -> Result<(), M11CleanControllerFault> {
        let summary = Self::summarize_indented_code_line(line_start, physical, code, false)?;
        if code.content.start >= code.content.end {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let DocumentState::IndentedCode(state) = &mut self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let pending = state.pending_blanks.take();
        if pending
            .as_ref()
            .map_or(state.source_end != line_start, |blank| {
                blank.source_start != state.source_end
            })
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let pending_line_count = pending.as_ref().map_or(0, |blank| blank.line_count);
        let pending_utf8 = pending
            .as_ref()
            .map_or(0, |blank| blank.projected_utf8_length);
        let pending_utf16 = pending
            .as_ref()
            .map_or(0, |blank| blank.projected_utf16_length);
        state.line_count = state
            .line_count
            .checked_add(pending_line_count)
            .and_then(|count| count.checked_add(1))
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        state.projected_utf8_length = state
            .projected_utf8_length
            .checked_add(pending_utf8)
            .and_then(|length| length.checked_add(summary.projected_utf8_length))
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        state.projected_utf16_length = state
            .projected_utf16_length
            .checked_add(pending_utf16)
            .and_then(|length| length.checked_add(summary.projected_utf16_length))
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        state.source_end = summary.source_end;
        state.terminal_eol_bytes = summary.terminal_eol_bytes;
        Ok(())
    }

    fn apply_pending_indented_blank(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        code: SegmentedIndentedCodeLineFacts,
    ) -> Result<(), M11CleanControllerFault> {
        let summary = Self::summarize_indented_code_line(line_start, physical, code, false)?;
        let DocumentState::IndentedCode(state) = &mut self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let pending = state.pending_blanks.get_or_insert(PendingIndentedBlanks {
            source_start: line_start,
            line_count: 0,
            projected_utf8_length: 0,
            projected_utf16_length: 0,
        });
        if pending.line_count == 0 && pending.source_start != state.source_end {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        pending.line_count = pending
            .line_count
            .checked_add(1)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        pending.projected_utf8_length = pending
            .projected_utf8_length
            .checked_add(summary.projected_utf8_length)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        pending.projected_utf16_length = pending
            .projected_utf16_length
            .checked_add(summary.projected_utf16_length)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        Ok(())
    }

    fn push_finished_indented_code(
        &mut self,
        code: IndentedCodeState,
        trailing_blank_end: SourceCut,
    ) -> Result<(), M11CleanControllerFault> {
        let IndentedCodeState {
            source_start,
            source_end,
            line_count,
            projected_utf8_length,
            projected_utf16_length,
            terminal_eol_bytes,
            has_bof_bom,
            pending_blanks,
        } = code;
        if source_start.byte >= source_end.byte
            || source_start.utf16 >= source_end.utf16
            || line_count == 0
            || terminal_eol_bytes > 2
            || projected_utf8_length > source_end.byte - source_start.byte
            || projected_utf16_length > source_end.utf16 - source_start.utf16
            || has_bof_bom && source_start.byte != 0
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        self.push_leaf(M11CleanLeaf::IndentedCode {
            source: source_start.byte..source_end.byte,
            source_utf16: source_start.utf16..source_end.utf16,
            line_count,
            projected_utf8_length,
            projected_utf16_length,
            terminal_eol_bytes,
            has_bof_bom,
        })?;
        if let Some(blank) = pending_blanks {
            if blank.source_start != source_end
                || blank.source_start.byte >= trailing_blank_end.byte
                || blank.source_start.utf16 >= trailing_blank_end.utf16
            {
                return Err(M11CleanControllerFault::FactsMismatch);
            }
            self.push_leaf(M11CleanLeaf::Blank {
                source: blank.source_start.byte..trailing_blank_end.byte,
                source_utf16: blank.source_start.utf16..trailing_blank_end.utf16,
            })?;
        } else if source_end != trailing_blank_end {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        Ok(())
    }

    fn finish_indented_code_before(
        &mut self,
        end: SourceCut,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::IndentedCode(code) = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        self.push_finished_indented_code(code, end)
    }

    fn marked_block_quote_line(
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        quote: SegmentedBlockQuoteFacts,
        has_bof_bom: bool,
        kind: M11BlockQuoteLineKind,
    ) -> Result<M11BlockQuoteLineMapping, M11CleanControllerFault> {
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        if quote.hidden_prefix.start != 0
            || quote.hidden_prefix.end != quote.content.start
            || quote.opening_marker.start >= quote.opening_marker.end
            || quote.opening_marker.end - quote.opening_marker.start != 1
            || quote.opening_marker.start < quote.hidden_prefix.start
            || quote.opening_marker.end > quote.hidden_prefix.end
            || quote.content.start > quote.content.end
            || quote.content.end != quote.line_ending.start
            || quote.line_ending.start != content_bytes
            || quote.line_ending.end != physical_bytes
            || quote.residual_tab_columns > 3
            || has_bof_bom && line_start.byte != 0
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let absolute = |span: crate::segmented_lexical::SegmentedLineSpan| {
            Ok::<_, M11CleanControllerFault>(
                add_u32_usize(line_start.byte, span.start)?
                    ..add_u32_usize(line_start.byte, span.end)?,
            )
        };
        let prefix_utf16 = |byte_offset: usize| {
            let adjusted = if has_bof_bom {
                byte_offset
                    .checked_sub(2)
                    .ok_or(M11CleanControllerFault::FactsMismatch)?
            } else {
                byte_offset
            };
            line_start
                .utf16
                .checked_add(
                    u32::try_from(adjusted).map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                )
                .ok_or(M11CleanControllerFault::MetricOverflow)
        };
        let content_utf16_end = line_start
            .utf16
            .checked_add(physical.content_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let source_utf16_end = line_start
            .utf16
            .checked_add(physical.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        Ok(M11BlockQuoteLineMapping {
            source: line_start.byte..physical.identity().end_byte(),
            source_utf16: line_start.utf16..source_utf16_end,
            opening_marker: Some(absolute(quote.opening_marker)?),
            hidden_prefix: Some(absolute(quote.hidden_prefix)?),
            content_source: absolute(quote.content)?,
            content_source_utf16: prefix_utf16(quote.content.start)?..content_utf16_end,
            line_ending: absolute(quote.line_ending)?,
            line_ending_utf16: content_utf16_end..source_utf16_end,
            residual_tab_columns: quote.residual_tab_columns,
            kind,
        })
    }

    fn lazy_block_quote_line(
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
    ) -> Result<M11BlockQuoteLineMapping, M11CleanControllerFault> {
        let content_end = line_start
            .byte
            .checked_add(physical.content_bytes())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let content_utf16_end = line_start
            .utf16
            .checked_add(physical.content_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let source_utf16_end = line_start
            .utf16
            .checked_add(physical.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        Ok(M11BlockQuoteLineMapping {
            source: line_start.byte..physical.identity().end_byte(),
            source_utf16: line_start.utf16..source_utf16_end,
            opening_marker: None,
            hidden_prefix: None,
            content_source: line_start.byte..content_end,
            content_source_utf16: line_start.utf16..content_utf16_end,
            line_ending: content_end..physical.identity().end_byte(),
            line_ending_utf16: content_utf16_end..source_utf16_end,
            residual_tab_columns: 0,
            kind: M11BlockQuoteLineKind::LazyParagraphContinuation,
        })
    }

    fn block_quote_unsupported_reason(
        facts: SegmentedBlockQuoteFacts,
        paragraph_open: bool,
    ) -> Option<M11BlockQuoteUnsupportedReason> {
        let residual = facts.residual;
        if facts.residual_tab_columns != 0 {
            Some(M11BlockQuoteUnsupportedReason::PartialTabMarker)
        } else if residual.blank {
            Some(M11BlockQuoteUnsupportedReason::MarkerOnlyOrBlank)
        } else if residual.block_quote {
            Some(M11BlockQuoteUnsupportedReason::NestedBlockQuote)
        } else if residual.atx_heading {
            Some(M11BlockQuoteUnsupportedReason::AtxHeading)
        } else if residual.fence {
            Some(M11BlockQuoteUnsupportedReason::FencedCode)
        } else if residual.html_block_1_to_6 || (!paragraph_open && residual.html_block_7) {
            Some(M11BlockQuoteUnsupportedReason::HtmlBlock)
        } else if paragraph_open && residual.setext {
            Some(M11BlockQuoteUnsupportedReason::SetextHeading)
        } else if residual.thematic_break {
            Some(M11BlockQuoteUnsupportedReason::ThematicBreak)
        } else if if paragraph_open {
            residual.interrupting_list
        } else {
            residual.list
        } {
            Some(M11BlockQuoteUnsupportedReason::List)
        } else if !paragraph_open && residual.indented_code {
            Some(M11BlockQuoteUnsupportedReason::IndentedCode)
        } else if residual.table_delimiter_candidate {
            Some(M11BlockQuoteUnsupportedReason::TableCandidate)
        } else if residual.potential_reference_definition {
            Some(M11BlockQuoteUnsupportedReason::PotentialReferenceDefinition)
        } else {
            None
        }
    }

    fn append_block_quote_mapping(
        quote: &mut BlockQuoteState,
        mapping: M11BlockQuoteLineMapping,
        reason: Option<M11BlockQuoteUnsupportedReason>,
        container_paragraph_open_after: bool,
    ) -> Result<(), M11CleanControllerFault> {
        if mapping.source.start != quote.source_end.byte
            || mapping.source_utf16.start != quote.source_end.utf16
            || mapping.source.start >= mapping.source.end
            || mapping.source_utf16.start >= mapping.source_utf16.end
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let line_index = u32::try_from(quote.lines.len())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let next_line_index = line_index
            .checked_add(1)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let projected_utf8_length = (mapping.content_source.end - mapping.content_source.start)
            .checked_add(mapping.line_ending.end - mapping.line_ending.start)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let projected_utf16_length = (mapping.content_source_utf16.end
            - mapping.content_source_utf16.start)
            .checked_add(mapping.line_ending_utf16.end - mapping.line_ending_utf16.start)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        quote
            .lines
            .try_reserve(1)
            .map_err(|_| M11CleanControllerFault::LeafAllocationFailed)?;
        quote.source_end = SourceCut {
            byte: mapping.source.end,
            utf16: mapping.source_utf16.end,
        };
        quote.lines.push(mapping);
        quote.container_paragraph_open = container_paragraph_open_after;

        if let Some(reason) = reason {
            if reason == M11BlockQuoteUnsupportedReason::MarkerOnlyOrBlank
                && quote.paragraph.is_some()
            {
                quote.paragraph_closed = true;
            }
            quote.unsupported.get_or_insert(reason);
            return Ok(());
        }
        if quote.paragraph_closed {
            quote.unsupported = Some(M11BlockQuoteUnsupportedReason::MultipleParagraphChildren);
            return Ok(());
        }
        if quote.unsupported.is_some() {
            return Ok(());
        }
        match &mut quote.paragraph {
            Some(paragraph) => {
                if paragraph.line_indices.end != line_index {
                    return Err(M11CleanControllerFault::FactsMismatch);
                }
                paragraph.line_indices.end = next_line_index;
                paragraph.projected_utf8_length = paragraph
                    .projected_utf8_length
                    .checked_add(projected_utf8_length)
                    .ok_or(M11CleanControllerFault::MetricOverflow)?;
                paragraph.projected_utf16_length = paragraph
                    .projected_utf16_length
                    .checked_add(projected_utf16_length)
                    .ok_or(M11CleanControllerFault::MetricOverflow)?;
            }
            None => {
                quote.paragraph = Some(BlockQuoteParagraphState {
                    line_indices: line_index..next_line_index,
                    projected_utf8_length,
                    projected_utf16_length,
                });
            }
        }
        Ok(())
    }

    fn block_quote_container_paragraph_open_after(
        facts: SegmentedBlockQuoteFacts,
        reason: Option<M11BlockQuoteUnsupportedReason>,
    ) -> bool {
        match reason {
            None | Some(M11BlockQuoteUnsupportedReason::PotentialReferenceDefinition) => true,
            Some(M11BlockQuoteUnsupportedReason::PartialTabMarker) => !facts.residual.blank,
            Some(
                M11BlockQuoteUnsupportedReason::MarkerOnlyOrBlank
                | M11BlockQuoteUnsupportedReason::NestedBlockQuote
                | M11BlockQuoteUnsupportedReason::AtxHeading
                | M11BlockQuoteUnsupportedReason::FencedCode
                | M11BlockQuoteUnsupportedReason::HtmlBlock
                | M11BlockQuoteUnsupportedReason::SetextHeading
                | M11BlockQuoteUnsupportedReason::ThematicBreak
                | M11BlockQuoteUnsupportedReason::List
                | M11BlockQuoteUnsupportedReason::IndentedCode
                | M11BlockQuoteUnsupportedReason::TableCandidate
                | M11BlockQuoteUnsupportedReason::MultipleParagraphChildren,
            ) => false,
        }
    }

    fn apply_start_block_quote(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedBlockQuoteFacts,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..line_start.byte,
                        source_utf16: start.utf16..line_start.utf16,
                    })?;
                }
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    line_start,
                    self.next_ordinal,
                    false,
                )?;
                self.push_finished_paragraph(paragraph, line_start)?;
            }
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)
            | DocumentState::UnknownSuffix { .. }) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
        }
        let paragraph_open = false;
        let reason = Self::block_quote_unsupported_reason(facts, paragraph_open);
        let container_paragraph_open_after =
            Self::block_quote_container_paragraph_open_after(facts, reason);
        let kind = if reason.is_some() {
            M11BlockQuoteLineKind::MarkedUnsupported
        } else {
            M11BlockQuoteLineKind::MarkedParagraph
        };
        let mapping =
            Self::marked_block_quote_line(line_start, physical, facts, has_bof_bom, kind)?;
        let mut quote = BlockQuoteState {
            source_start: line_start,
            source_end: line_start,
            lines: Vec::new(),
            paragraph: None,
            paragraph_closed: false,
            container_paragraph_open: false,
            unsupported: None,
        };
        Self::append_block_quote_mapping(
            &mut quote,
            mapping,
            reason,
            container_paragraph_open_after,
        )?;
        self.state = DocumentState::BlockQuote(quote);
        Ok(())
    }

    fn apply_continue_block_quote(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedBlockQuoteFacts,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        let DocumentState::BlockQuote(quote) = &mut self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let paragraph_open = quote.container_paragraph_open;
        let reason = Self::block_quote_unsupported_reason(facts, paragraph_open);
        let container_paragraph_open_after =
            Self::block_quote_container_paragraph_open_after(facts, reason);
        let kind = if reason.is_some() || quote.unsupported.is_some() {
            M11BlockQuoteLineKind::MarkedUnsupported
        } else {
            M11BlockQuoteLineKind::MarkedParagraph
        };
        let mapping =
            Self::marked_block_quote_line(line_start, physical, facts, has_bof_bom, kind)?;
        Self::append_block_quote_mapping(quote, mapping, reason, container_paragraph_open_after)
    }

    fn apply_continue_block_quote_lazy(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
    ) -> Result<(), M11CleanControllerFault> {
        let DocumentState::BlockQuote(quote) = &mut self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        if !quote.container_paragraph_open {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        }
        let mapping = Self::lazy_block_quote_line(line_start, physical)?;
        Self::append_block_quote_mapping(quote, mapping, None, true)
    }

    fn push_finished_block_quote(
        &mut self,
        quote: BlockQuoteState,
        end: SourceCut,
    ) -> Result<(), M11CleanControllerFault> {
        if quote.source_start.byte >= quote.source_end.byte
            || quote.source_start.utf16 >= quote.source_end.utf16
            || quote.source_end != end
            || quote.lines.is_empty()
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let (disposition, child_paragraph) = match (quote.unsupported, quote.paragraph) {
            (None, Some(paragraph)) => (
                M11BlockQuoteDisposition::ExactSingleParagraph,
                Some(M11BlockQuoteParagraphMapping {
                    line_indices: paragraph.line_indices,
                    projected_utf8_length: paragraph.projected_utf8_length,
                    projected_utf16_length: paragraph.projected_utf16_length,
                }),
            ),
            (Some(reason), _) => (M11BlockQuoteDisposition::Unsupported(reason), None),
            (None, None) => (
                M11BlockQuoteDisposition::Unsupported(
                    M11BlockQuoteUnsupportedReason::MarkerOnlyOrBlank,
                ),
                None,
            ),
        };
        self.push_leaf(M11CleanLeaf::BlockQuote {
            source: quote.source_start.byte..quote.source_end.byte,
            source_utf16: quote.source_start.utf16..quote.source_end.utf16,
            lines: quote.lines.into_boxed_slice(),
            child_paragraph,
            disposition,
        })
    }

    fn finish_block_quote_before(&mut self, end: SourceCut) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::BlockQuote(quote) = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        self.push_finished_block_quote(quote, end)
    }

    pub(crate) fn list_item_unsupported_reason(
        facts: SegmentedListItemFacts,
    ) -> Option<M11ListUnsupportedReason> {
        if facts.tab_padded {
            Some(M11ListUnsupportedReason::TabPadded)
        } else if !facts.empty && !(1..=4).contains(&facts.padding_columns) {
            Some(M11ListUnsupportedReason::ExcessivePadding)
        } else if facts.child.task {
            Some(M11ListUnsupportedReason::Task)
        } else if facts.child.thematic_break {
            // CommonMark gives a thematic break precedence over the same
            // leading marker bytes being interpreted as a nested list.
            Some(M11ListUnsupportedReason::BlockChild)
        } else if facts.child.list {
            Some(M11ListUnsupportedReason::Nested)
        } else if facts.child.block_quote
            || facts.child.atx_heading
            || facts.child.fence
            || facts.child.html_block_1_to_6
            || facts.child.html_block_7
            || facts.child.setext
            || facts.child.table_delimiter_candidate
            || facts.child.potential_reference_definition
        {
            Some(M11ListUnsupportedReason::BlockChild)
        } else {
            None
        }
    }

    pub(crate) fn bullet_list_item_mapping(
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
        ordinal: u32,
    ) -> Result<M11BulletListItemMapping, M11CleanControllerFault> {
        let SegmentedListMarker::Bullet(marker) = facts.marker else {
            return Err(M11CleanControllerFault::FactsMismatch);
        };
        if !matches!(marker, b'-' | b'+' | b'*') {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let geometry =
            Self::tight_list_item_geometry(line_start, physical, facts, has_bof_bom, ordinal, 1)?;
        Ok(M11BulletListItemMapping {
            ordinal: geometry.ordinal,
            source: geometry.source,
            source_utf16: geometry.source_utf16,
            opening_marker: geometry.opening_marker,
            hidden_prefix: geometry.hidden_prefix,
            hidden_prefix_utf16: geometry.hidden_prefix_utf16,
            continuation_prefix_source: geometry.continuation_prefix_source,
            continuation_prefix_source_utf16: geometry.continuation_prefix_source_utf16,
            content_source: geometry.content_source,
            content_source_utf16: geometry.content_source_utf16,
            line_ending: geometry.line_ending,
            line_ending_utf16: geometry.line_ending_utf16,
            marker,
            paragraph: geometry.paragraph,
        })
    }

    pub(crate) fn ordered_list_item_mapping(
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
        ordinal: u32,
    ) -> Result<M11OrderedListItemMapping, M11CleanControllerFault> {
        let SegmentedListMarker::Ordered { start, delimiter } = facts.marker else {
            return Err(M11CleanControllerFault::FactsMismatch);
        };
        let marker_value =
            u32::try_from(start).map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        if !matches!(delimiter, b'.' | b')') {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let marker_bytes = facts
            .opening_marker
            .end
            .checked_sub(facts.opening_marker.start)
            .ok_or(M11CleanControllerFault::FactsMismatch)?;
        if !(2..=10).contains(&marker_bytes) {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let geometry = Self::tight_list_item_geometry(
            line_start,
            physical,
            facts,
            has_bof_bom,
            ordinal,
            marker_bytes,
        )?;
        Ok(M11OrderedListItemMapping {
            ordinal: geometry.ordinal,
            source: geometry.source,
            source_utf16: geometry.source_utf16,
            opening_marker: geometry.opening_marker,
            hidden_prefix: geometry.hidden_prefix,
            hidden_prefix_utf16: geometry.hidden_prefix_utf16,
            continuation_prefix_source: geometry.continuation_prefix_source,
            continuation_prefix_source_utf16: geometry.continuation_prefix_source_utf16,
            content_source: geometry.content_source,
            content_source_utf16: geometry.content_source_utf16,
            line_ending: geometry.line_ending,
            line_ending_utf16: geometry.line_ending_utf16,
            marker_value,
            delimiter,
            paragraph: geometry.paragraph,
        })
    }

    fn tight_list_item_geometry(
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
        ordinal: u32,
        marker_bytes: usize,
    ) -> Result<TightListItemGeometry, M11CleanControllerFault> {
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let valid_span = |span: crate::segmented_lexical::SegmentedLineSpan| {
            span.start <= span.end && span.end <= physical_bytes
        };
        let bom_bytes = usize::from(has_bof_bom) * 3;
        if facts.opening_indent > 3
            || facts.tab_padded
            || !facts.empty && !(1..=4).contains(&facts.padding_columns)
            || has_bof_bom && line_start.byte != 0
            || !valid_span(facts.hidden_prefix)
            || !valid_span(facts.continuation_prefix)
            || !valid_span(facts.opening_marker)
            || !valid_span(facts.content)
            || !valid_span(facts.line_ending)
            || facts.hidden_prefix.start != 0
            || facts.continuation_prefix.start != 0
            || facts.opening_marker.start != bom_bytes + facts.opening_indent
            || facts.opening_marker.end - facts.opening_marker.start != marker_bytes
            || facts.opening_marker.end > facts.continuation_prefix.end
            || facts.continuation_prefix.end > facts.hidden_prefix.end
            || facts.hidden_prefix.end != facts.content.start
            || facts.content.end != facts.line_ending.start
            || facts.line_ending.start != content_bytes
            || facts.line_ending.end != physical_bytes
            || facts.empty != (facts.content.start == facts.content.end)
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let absolute = |span: crate::segmented_lexical::SegmentedLineSpan| {
            Ok::<_, M11CleanControllerFault>(
                add_u32_usize(line_start.byte, span.start)?
                    ..add_u32_usize(line_start.byte, span.end)?,
            )
        };
        let prefix_utf16 = |byte_offset: usize| {
            let adjusted = if has_bof_bom {
                byte_offset
                    .checked_sub(2)
                    .ok_or(M11CleanControllerFault::FactsMismatch)?
            } else {
                byte_offset
            };
            line_start
                .utf16
                .checked_add(
                    u32::try_from(adjusted).map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                )
                .ok_or(M11CleanControllerFault::MetricOverflow)
        };
        let content_utf16_end = line_start
            .utf16
            .checked_add(physical.content_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let source_utf16_end = line_start
            .utf16
            .checked_add(physical.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let hidden_prefix = absolute(facts.hidden_prefix)?;
        let continuation_prefix = absolute(facts.continuation_prefix)?;
        let content_source = absolute(facts.content)?;
        let line_ending = absolute(facts.line_ending)?;
        let hidden_prefix_utf16_end = prefix_utf16(facts.hidden_prefix.end)?;
        let continuation_prefix_utf16_end = prefix_utf16(facts.continuation_prefix.end)?;
        let content_source_utf16 = hidden_prefix_utf16_end..content_utf16_end;
        let paragraph = (!facts.empty).then(|| M11BulletListParagraphMapping {
            source: content_source.clone(),
            source_utf16: content_source_utf16.clone(),
            inline_source: content_source.clone(),
            inline_source_utf16: content_source_utf16.clone(),
        });
        let continuation_start_byte = line_start
            .byte
            .checked_add(u32::try_from(bom_bytes).expect("BOF BOM byte width"))
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let continuation_start_utf16 = line_start
            .utf16
            .checked_add(u32::from(has_bof_bom))
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        Ok(TightListItemGeometry {
            ordinal,
            source: line_start.byte..physical.identity().end_byte(),
            source_utf16: line_start.utf16..source_utf16_end,
            opening_marker: absolute(facts.opening_marker)?,
            hidden_prefix,
            hidden_prefix_utf16: line_start.utf16..hidden_prefix_utf16_end,
            continuation_prefix_source: continuation_start_byte..continuation_prefix.end,
            continuation_prefix_source_utf16: continuation_start_utf16
                ..continuation_prefix_utf16_end,
            content_source,
            content_source_utf16,
            line_ending,
            line_ending_utf16: content_utf16_end..source_utf16_end,
            paragraph,
        })
    }

    fn append_tight_list_item(
        list: &mut TightListState,
        mapping: TightListItemMapping,
        opening_indent: usize,
        opening_marker_bytes: usize,
        padding_columns: usize,
    ) -> Result<(), M11CleanControllerFault> {
        let expected_ordinal =
            u32::try_from(list.items.len()).map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        if mapping.ordinal() != expected_ordinal
            || mapping.source().start != list.source_end.byte
            || mapping.source_utf16().start != list.source_end.utf16
            || mapping.source().start >= mapping.source().end
            || mapping.source_utf16().start >= mapping.source_utf16().end
            || !mapping.matches_kind(list.kind)
            || list.pending_blank_start.is_some()
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let projected_utf8_length = (mapping.content_source().end - mapping.content_source().start)
            .checked_add(mapping.line_ending().end - mapping.line_ending().start)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        let projected_utf16_length = (mapping.content_source_utf16().end
            - mapping.content_source_utf16().start)
            .checked_add(mapping.line_ending_utf16().end - mapping.line_ending_utf16().start)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        list.items
            .try_reserve(1)
            .map_err(|_| M11CleanControllerFault::LeafAllocationFailed)?;
        list.projected_utf8_length = list
            .projected_utf8_length
            .checked_add(projected_utf8_length)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        list.projected_utf16_length = list
            .projected_utf16_length
            .checked_add(projected_utf16_length)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        list.source_end = SourceCut {
            byte: mapping.source().end,
            utf16: mapping.source_utf16().end,
        };
        list.current_content_indent = opening_indent
            .checked_add(opening_marker_bytes)
            .and_then(|value| value.checked_add(padding_columns.max(1)))
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        list.terminal_empty = !mapping.has_paragraph();
        list.items.push(mapping);
        Ok(())
    }

    fn tight_list_kind(
        marker: SegmentedListMarker,
    ) -> Result<TightListKind, M11CleanControllerFault> {
        match marker {
            SegmentedListMarker::Bullet(marker) if matches!(marker, b'-' | b'+' | b'*') => {
                Ok(TightListKind::Bullet { marker })
            }
            SegmentedListMarker::Ordered { start, delimiter }
                if matches!(delimiter, b'.' | b')') =>
            {
                Ok(TightListKind::Ordered {
                    start: u32::try_from(start)
                        .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                    delimiter,
                })
            }
            _ => Err(M11CleanControllerFault::FactsMismatch),
        }
    }

    const fn tight_list_marker_matches(kind: TightListKind, marker: SegmentedListMarker) -> bool {
        match (kind, marker) {
            (TightListKind::Bullet { marker: expected }, SegmentedListMarker::Bullet(actual)) => {
                expected == actual
            }
            (
                TightListKind::Ordered {
                    delimiter: expected,
                    start: _,
                },
                SegmentedListMarker::Ordered {
                    delimiter: actual,
                    start: _,
                },
            ) => expected == actual,
            _ => false,
        }
    }

    fn tight_list_item_mapping(
        kind: TightListKind,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
        ordinal: u32,
    ) -> Result<TightListItemMapping, M11CleanControllerFault> {
        if !Self::tight_list_marker_matches(kind, facts.marker) {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        match kind {
            TightListKind::Bullet { .. } => Ok(TightListItemMapping::Bullet(
                Self::bullet_list_item_mapping(line_start, physical, facts, has_bof_bom, ordinal)?,
            )),
            TightListKind::Ordered { .. } => Ok(TightListItemMapping::Ordered(
                Self::ordered_list_item_mapping(line_start, physical, facts, has_bof_bom, ordinal)?,
            )),
        }
    }

    fn apply_start_tight_list(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        if Self::list_item_unsupported_reason(facts).is_some() {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let kind = Self::tight_list_kind(facts.marker)?;
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        match state {
            DocumentState::BetweenBlocks { pending_gap_start } => {
                if let Some(start) = pending_gap_start {
                    self.push_leaf(M11CleanLeaf::Blank {
                        source: start.byte..line_start.byte,
                        source_utf16: start.utf16..line_start.utf16,
                    })?;
                }
            }
            DocumentState::Paragraph(mut paragraph) => {
                Self::finish_paragraph_reference(
                    &mut paragraph,
                    line_start,
                    self.next_ordinal,
                    false,
                )?;
                self.push_finished_paragraph(paragraph, line_start)?;
            }
            state @ (DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)
            | DocumentState::UnknownSuffix { .. }) => {
                self.state = state;
                return Err(M11CleanControllerFault::IncompleteAdmission);
            }
        }
        let marker_bytes = facts.opening_marker.end - facts.opening_marker.start;
        let mapping =
            Self::tight_list_item_mapping(kind, line_start, physical, facts, has_bof_bom, 0)?;
        let mut list = TightListState {
            source_start: line_start,
            source_end: line_start,
            kind,
            items: Vec::new(),
            projected_utf8_length: 0,
            projected_utf16_length: 0,
            current_content_indent: 0,
            terminal_empty: false,
            pending_blank_start: None,
        };
        Self::append_tight_list_item(
            &mut list,
            mapping,
            facts.opening_indent,
            marker_bytes,
            facts.padding_columns,
        )?;
        self.state = DocumentState::TightList(list);
        Ok(())
    }

    fn apply_continue_tight_list(
        &mut self,
        line_start: SourceCut,
        physical: M11PhysicalLineFacts,
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
    ) -> Result<(), M11CleanControllerFault> {
        if Self::list_item_unsupported_reason(facts).is_some() {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let DocumentState::TightList(list) = &mut self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let ordinal =
            u32::try_from(list.items.len()).map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let marker_bytes = facts.opening_marker.end - facts.opening_marker.start;
        let mapping = Self::tight_list_item_mapping(
            list.kind,
            line_start,
            physical,
            facts,
            has_bof_bom,
            ordinal,
        )?;
        Self::append_tight_list_item(
            list,
            mapping,
            facts.opening_indent,
            marker_bytes,
            facts.padding_columns,
        )
    }

    fn apply_pending_tight_list_blank(
        &mut self,
        line_start: SourceCut,
    ) -> Result<(), M11CleanControllerFault> {
        let DocumentState::TightList(list) = &mut self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        if list.pending_blank_start.is_none() {
            if list.source_end != line_start {
                return Err(M11CleanControllerFault::FactsMismatch);
            }
            list.pending_blank_start = Some(line_start);
        }
        Ok(())
    }

    fn apply_reject_open_tight_list(
        &mut self,
        reason: M11ListUnsupportedReason,
    ) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::TightList(list) = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        self.ordinary_paragraph_restart_seeds.clear();
        self.state = DocumentState::UnknownSuffix {
            source_start: list.source_start,
            reason: M11UnknownReason::UnsupportedList(reason),
        };
        Ok(())
    }

    fn push_finished_tight_list(
        &mut self,
        list: TightListState,
    ) -> Result<Option<SourceCut>, M11CleanControllerFault> {
        if list.source_start.byte >= list.source_end.byte
            || list.source_start.utf16 >= list.source_end.utf16
            || list.items.is_empty()
            || list.items.last().is_none_or(|item| {
                item.source().end != list.source_end.byte
                    || item.source_utf16().end != list.source_end.utf16
            })
            || list.items.iter().enumerate().any(|(ordinal, item)| {
                item.ordinal() != u32::try_from(ordinal).unwrap_or(u32::MAX)
                    || !item.matches_kind(list.kind)
            })
            || list
                .pending_blank_start
                .is_some_and(|start| start != list.source_end)
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let pending_blank_start = list.pending_blank_start;
        let source = list.source_start.byte..list.source_end.byte;
        let source_utf16 = list.source_start.utf16..list.source_end.utf16;
        match list.kind {
            TightListKind::Bullet { marker } => {
                let mut items = Vec::new();
                items
                    .try_reserve_exact(list.items.len())
                    .map_err(|_| M11CleanControllerFault::LeafAllocationFailed)?;
                for item in list.items {
                    let TightListItemMapping::Bullet(item) = item else {
                        return Err(M11CleanControllerFault::FactsMismatch);
                    };
                    items.push(item);
                }
                self.push_leaf(M11CleanLeaf::BulletList {
                    source,
                    source_utf16,
                    marker,
                    items: items.into_boxed_slice(),
                    projected_utf8_length: list.projected_utf8_length,
                    projected_utf16_length: list.projected_utf16_length,
                    tight: true,
                })?;
            }
            TightListKind::Ordered { start, delimiter } => {
                let mut items = Vec::new();
                items
                    .try_reserve_exact(list.items.len())
                    .map_err(|_| M11CleanControllerFault::LeafAllocationFailed)?;
                for item in list.items {
                    let TightListItemMapping::Ordered(item) = item else {
                        return Err(M11CleanControllerFault::FactsMismatch);
                    };
                    items.push(item);
                }
                self.push_leaf(M11CleanLeaf::OrderedList {
                    source,
                    source_utf16,
                    start,
                    delimiter,
                    items: items.into_boxed_slice(),
                    projected_utf8_length: list.projected_utf8_length,
                    projected_utf16_length: list.projected_utf16_length,
                    tight: true,
                })?;
            }
        }
        Ok(pending_blank_start)
    }

    fn finish_tight_list_before(&mut self, end: SourceCut) -> Result<(), M11CleanControllerFault> {
        let state = std::mem::replace(
            &mut self.state,
            DocumentState::BetweenBlocks {
                pending_gap_start: None,
            },
        );
        let DocumentState::TightList(list) = state else {
            self.state = state;
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        if list.pending_blank_start.is_none() && list.source_end != end
            || list
                .pending_blank_start
                .is_some_and(|start| start.byte >= end.byte || start.utf16 >= end.utf16)
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let pending_gap_start = self.push_finished_tight_list(list)?;
        self.state = DocumentState::BetweenBlocks { pending_gap_start };
        Ok(())
    }

    fn validate_admission(
        &self,
        admission: &M11CleanLineAdmission,
    ) -> Result<(), M11CleanControllerFault> {
        if admission.controller_id != self.controller_id {
            return Err(M11CleanControllerFault::WrongController);
        }
        if self.active_admission != Some(admission.admission_id) {
            return Err(M11CleanControllerFault::StaleAdmission);
        }
        Ok(())
    }

    fn prepare_classifier(
        &self,
        admission: &mut M11CleanLineAdmission,
    ) -> Result<(), M11CleanControllerFault> {
        admission.finish_segmented_read()?;
        admission.stage = AdmissionStage::Classifying(Classifier {
            stage: OpenerStage::Prepare,
            first_nonspace: 0,
            first_nonspace_source: admission.identity.start_byte(),
            paragraph_open: matches!(self.state, DocumentState::Paragraph(_)),
            finish_indented_before_apply: false,
            finish_block_quote_before_apply: false,
            finish_tight_list_before_apply: false,
        });
        Ok(())
    }

    fn advance_classifier(
        &self,
        admission: &mut M11CleanLineAdmission,
    ) -> Result<(), M11CleanControllerFault> {
        let AdmissionStage::Classifying(mut classifier) = admission.stage else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let outcome = match classifier.stage {
            OpenerStage::Prepare => self.prepare_line(admission, &mut classifier)?,
            OpenerStage::BlockQuote => Self::run_block_quote(admission, &mut classifier)?,
            OpenerStage::AtxHeading => Self::run_atx(admission, &mut classifier)?,
            OpenerStage::FencedCode => Self::run_fence(admission, &mut classifier)?,
            OpenerStage::HtmlBlock => Self::run_html(admission, &mut classifier)?,
            OpenerStage::SetextHeading => self.run_setext(admission, &mut classifier)?,
            OpenerStage::SetextReference => {
                self.run_setext_reference(admission, &mut classifier)?
            }
            OpenerStage::ThematicBreak => Self::run_thematic(admission, &mut classifier)?,
            OpenerStage::List => Self::run_list(admission, &mut classifier)?,
            OpenerStage::IndentedCode => {
                classifier.stage = OpenerStage::TableCandidate;
                None
            }
            OpenerStage::TableCandidate => Self::run_table(admission, &mut classifier)?,
            OpenerStage::Paragraph => Some(self.paragraph_decision(&classifier)),
        };
        admission.stage = outcome.map_or_else(
            || AdmissionStage::Classifying(classifier),
            |line| {
                AdmissionStage::Matched(PreparedAdmission {
                    line,
                    finish_indented_before_apply: classifier.finish_indented_before_apply,
                    finish_block_quote_before_apply: classifier.finish_block_quote_before_apply,
                    finish_tight_list_before_apply: classifier.finish_tight_list_before_apply,
                })
            },
        );
        Ok(())
    }

    fn prepare_line(
        &self,
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        if matches!(self.state, DocumentState::UnknownSuffix { .. }) {
            return Ok(Some(PreparedLine::Noop));
        }
        let facts = admission.segmented_facts()?;
        if let DocumentState::FencedCode(fence) = &self.state {
            let opening_run_length = fence.opening_marker.end - fence.opening_marker.start;
            let closes = facts.indent <= 3
                && facts.fence.marker == Some(fence.marker)
                && facts.fence.opening_run_length
                    >= usize::try_from(opening_run_length)
                        .map_err(|_| M11CleanControllerFault::MetricOverflow)?
                && facts.fence.tail_horizontal_whitespace_only;
            return if closes {
                Ok(Some(PreparedLine::CloseFencedCode {
                    closing_run_length: u32::try_from(facts.fence.opening_run_length)
                        .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                    closing_marker_start: add_u32_usize(
                        admission.identity.start_byte(),
                        facts.first_nonspace,
                    )?,
                }))
            } else {
                Ok(Some(PreparedLine::ContinueFencedCode))
            };
        }
        if let DocumentState::BlockQuote(quote) = &self.state {
            if let Some(quote_facts) = facts.block_quote_source {
                return Ok(Some(PreparedLine::ContinueBlockQuote {
                    facts: quote_facts,
                    has_bof_bom: facts.has_bof_bom,
                }));
            }
            if facts.blank {
                classifier.finish_block_quote_before_apply = true;
                classifier.paragraph_open = false;
                return Ok(Some(if admission.identity.physical_bytes() == 0 {
                    PreparedLine::Noop
                } else {
                    PreparedLine::Blank
                }));
            }
            if quote.container_paragraph_open && is_block_quote_lazy_paragraph_continuation(&facts)
            {
                return Ok(Some(PreparedLine::ContinueBlockQuoteLazy));
            }
            classifier.finish_block_quote_before_apply = true;
            classifier.paragraph_open = false;
        }
        if matches!(self.state, DocumentState::TightList(_)) {
            if let Some(prepared) = self.prepare_open_tight_list_line(&facts, classifier)? {
                return Ok(Some(prepared));
            }
        }
        if matches!(self.state, DocumentState::IndentedCode(_)) {
            if facts.blank {
                if admission.identity.physical_bytes() == 0 {
                    return Ok(Some(PreparedLine::Noop));
                }
                return Ok(Some(PreparedLine::PendingIndentedBlank {
                    facts: facts
                        .indented_code
                        .ok_or(M11CleanControllerFault::FactsMismatch)?,
                }));
            }
            if let Some(code) = facts.indented_code {
                return Ok(Some(PreparedLine::ContinueIndentedCode { facts: code }));
            }
            classifier.finish_indented_before_apply = true;
            classifier.paragraph_open = false;
        }
        if facts.blank {
            return Ok(Some(PreparedLine::Blank));
        }

        classifier.first_nonspace = facts.first_nonspace;
        classifier.first_nonspace_source =
            add_u32_usize(admission.identity.start_byte(), facts.first_nonspace)?;

        if facts.indent >= 4 {
            return Ok(Some(if classifier.paragraph_open {
                self.paragraph_decision(classifier)
            } else {
                PreparedLine::StartIndentedCode {
                    facts: facts
                        .indented_code
                        .ok_or(M11CleanControllerFault::FactsMismatch)?,
                    has_bof_bom: facts.has_bof_bom,
                }
            }));
        }

        classifier.stage = OpenerStage::BlockQuote;
        Ok(None)
    }

    fn prepare_open_tight_list_line(
        &self,
        facts: &SegmentedLineFacts,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let DocumentState::TightList(list) = &self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        classifier.paragraph_open = false;
        if facts.blank {
            return Ok(Some(PreparedLine::PendingTightListBlank));
        }

        if list.pending_blank_start.is_some() {
            let continues_same_list = facts.list_item.is_some_and(|item| {
                item.opening_indent >= list.current_content_indent
                    || Self::tight_list_marker_matches(list.kind, item.marker)
            }) || facts.indent >= list.current_content_indent;
            if continues_same_list {
                return Ok(Some(PreparedLine::RejectOpenTightList {
                    reason: M11ListUnsupportedReason::Loose,
                }));
            }
            classifier.finish_tight_list_before_apply = true;
            return Ok(None);
        }

        if let Some(item) = facts.list_item {
            if item.opening_indent >= list.current_content_indent {
                return Ok(Some(PreparedLine::RejectOpenTightList {
                    reason: M11ListUnsupportedReason::Nested,
                }));
            }
            let same_marker = Self::tight_list_marker_matches(list.kind, item.marker);
            if same_marker {
                if list.terminal_empty {
                    return Ok(Some(PreparedLine::RejectOpenTightList {
                        reason: M11ListUnsupportedReason::NonTerminalEmptyItem,
                    }));
                }
                if let Some(reason) = Self::list_item_unsupported_reason(item) {
                    return Ok(Some(PreparedLine::RejectOpenTightList { reason }));
                }
                return Ok(Some(PreparedLine::ContinueTightList {
                    facts: item,
                    has_bof_bom: facts.has_bof_bom,
                }));
            }
            classifier.finish_tight_list_before_apply = true;
            return Ok(None);
        }

        if facts.indent >= list.current_content_indent {
            let reason = if facts.list {
                M11ListUnsupportedReason::Nested
            } else if line_starts_block_child(facts) {
                M11ListUnsupportedReason::BlockChild
            } else {
                M11ListUnsupportedReason::LazyOrMultiline
            };
            return Ok(Some(PreparedLine::RejectOpenTightList { reason }));
        }
        if list.terminal_empty {
            classifier.finish_tight_list_before_apply = true;
            return Ok(None);
        }
        if line_interrupts_open_list_paragraph(facts) {
            classifier.finish_tight_list_before_apply = true;
            return Ok(None);
        }
        if facts.setext.is_some()
            || facts.table_delimiter_candidate
            || facts.first_significant_byte == Some(b'[')
        {
            return Ok(Some(PreparedLine::RejectOpenTightList {
                reason: M11ListUnsupportedReason::BlockChild,
            }));
        }
        Ok(Some(PreparedLine::RejectOpenTightList {
            reason: M11ListUnsupportedReason::LazyOrMultiline,
        }))
    }

    fn run_block_quote(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        if admission.segmented_facts()?.block_quote {
            Ok(Some(PreparedLine::StartBlockQuote {
                facts: admission
                    .segmented_facts()?
                    .block_quote_source
                    .ok_or(M11CleanControllerFault::FactsMismatch)?,
                has_bof_bom: admission.segmented_facts()?.has_bof_bom,
            }))
        } else {
            classifier.stage = OpenerStage::AtxHeading;
            Ok(None)
        }
    }

    fn run_atx(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let line = admission.segmented_facts()?;
        if let Some(facts) = line.atx_heading {
            Ok(Some(PreparedLine::AtxHeading {
                facts,
                opening_indent: u8::try_from(line.indent)
                    .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                has_bof_bom: line.has_bof_bom,
            }))
        } else {
            classifier.stage = OpenerStage::FencedCode;
            Ok(None)
        }
    }

    fn run_fence(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let facts = admission.segmented_facts()?;
        if facts.fence.opener_valid {
            Ok(Some(PreparedLine::StartFencedCode {
                marker: facts
                    .fence
                    .marker
                    .ok_or(M11CleanControllerFault::FactsMismatch)?,
                opening_run_length: u32::try_from(facts.fence.opening_run_length)
                    .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                opening_indent: u8::try_from(facts.indent)
                    .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                opening_marker_start: classifier.first_nonspace_source,
            }))
        } else {
            classifier.stage = OpenerStage::HtmlBlock;
            Ok(None)
        }
    }

    fn run_html(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let facts = admission.segmented_facts()?;
        if facts.html_block_1_to_6.is_some() || (!classifier.paragraph_open && facts.html_block_7) {
            Ok(Some(unsupported(M11UnsupportedOpener::HtmlBlock)))
        } else {
            classifier.stage = OpenerStage::SetextHeading;
            Ok(None)
        }
    }

    fn run_setext(
        &self,
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        if !classifier.paragraph_open {
            classifier.stage = OpenerStage::ThematicBreak;
            return Ok(None);
        }
        if let Some(facts) = admission.segmented_facts()?.setext {
            if self.paragraph_has_reference_work() {
                classifier.stage = OpenerStage::SetextReference;
                Ok(None)
            } else {
                Ok(Some(PreparedLine::SetextHeading {
                    facts,
                    opening_indent: u8::try_from(admission.segmented_facts()?.indent)
                        .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                }))
            }
        } else {
            classifier.stage = OpenerStage::ThematicBreak;
            Ok(None)
        }
    }

    fn run_setext_reference(
        &self,
        admission: &mut M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let DocumentState::Paragraph(paragraph) = &self.state else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let reference = admission
            .reference
            .as_ref()
            .ok_or(M11CleanControllerFault::IncompleteAdmission)?;
        let definitions = paragraph.definitions.count() + admission.pending_definitions.len();
        if definitions > 0
            && reference.terminal().is_some_and(|terminal| {
                terminal.prefix_end(reference.base()) == admission.identity.start_byte()
            })
        {
            Ok(Some(PreparedLine::ReferenceOnlySetext {
                visible_start: classifier.first_nonspace_source,
            }))
        } else {
            Ok(Some(unsupported(M11UnsupportedOpener::SetextHeading)))
        }
    }

    fn run_thematic(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let line = admission.segmented_facts()?;
        if let Some(facts) = line.thematic_break {
            Ok(Some(PreparedLine::ThematicBreak {
                facts,
                opening_indent: u8::try_from(line.indent)
                    .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
                has_bof_bom: line.has_bof_bom,
            }))
        } else {
            classifier.stage = OpenerStage::List;
            Ok(None)
        }
    }

    fn run_list(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        let facts = admission.segmented_facts()?;
        if if classifier.paragraph_open {
            facts.interrupting_list
        } else {
            facts.list
        } {
            let item = facts
                .list_item
                .ok_or(M11CleanControllerFault::FactsMismatch)?;
            if let Some(reason) = Self::list_item_unsupported_reason(item) {
                Ok(Some(PreparedLine::Unsupported(
                    M11UnknownReason::UnsupportedList(reason),
                )))
            } else {
                Ok(Some(PreparedLine::StartTightList {
                    facts: item,
                    has_bof_bom: facts.has_bof_bom,
                }))
            }
        } else {
            classifier.stage = OpenerStage::IndentedCode;
            Ok(None)
        }
    }

    fn run_table(
        admission: &M11CleanLineAdmission,
        classifier: &mut Classifier,
    ) -> Result<Option<PreparedLine>, M11CleanControllerFault> {
        if admission.segmented_facts()?.table_delimiter_candidate {
            Ok(Some(unsupported(M11UnsupportedOpener::TableCandidate)))
        } else {
            classifier.stage = OpenerStage::Paragraph;
            Ok(None)
        }
    }

    fn paragraph_has_reference_work(&self) -> bool {
        matches!(
            &self.state,
            DocumentState::Paragraph(ParagraphState {
                reference: Some(_),
                ..
            })
        )
    }

    fn paragraph_decision(&self, classifier: &Classifier) -> PreparedLine {
        if classifier.finish_indented_before_apply
            || classifier.finish_block_quote_before_apply
            || classifier.finish_tight_list_before_apply
        {
            return PreparedLine::StartParagraph {
                content_start: classifier.first_nonspace_source,
            };
        }
        match &self.state {
            DocumentState::BetweenBlocks { .. } => PreparedLine::StartParagraph {
                content_start: classifier.first_nonspace_source,
            },
            DocumentState::Paragraph(_) => PreparedLine::ContinueParagraph,
            DocumentState::BlockQuote(_) => PreparedLine::Noop,
            DocumentState::TightList(_) => PreparedLine::Noop,
            DocumentState::FencedCode(_) => PreparedLine::ContinueFencedCode,
            DocumentState::IndentedCode(_) => PreparedLine::Noop,
            DocumentState::UnknownSuffix { .. } => PreparedLine::Noop,
        }
    }

    fn apply_prepared(
        &mut self,
        mut admission: M11CleanLineAdmission,
        facts: M11PhysicalLineFacts,
    ) -> Result<(), M11CleanControllerFault> {
        let AdmissionStage::Matched(prepared) = admission.stage else {
            return Err(M11CleanControllerFault::IncompleteAdmission);
        };
        let PreparedAdmission {
            line: prepared,
            finish_indented_before_apply,
            finish_block_quote_before_apply,
            finish_tight_list_before_apply,
        } = prepared;
        let restart_cut = self.leading_restart_cut_for_admission(&admission, facts)?;
        let line_start = SourceCut {
            byte: admission.identity.start_byte(),
            utf16: self.next_utf16,
        };
        if finish_indented_before_apply {
            self.finish_indented_code_before(line_start)?;
        }
        if finish_block_quote_before_apply {
            self.finish_block_quote_before(line_start)?;
        }
        if finish_tight_list_before_apply {
            self.finish_tight_list_before(line_start)?;
        }
        match prepared {
            PreparedLine::Noop => {}
            PreparedLine::Blank if admission.identity.physical_bytes() == 0 => {}
            PreparedLine::Blank => self.apply_blank(line_start)?,
            PreparedLine::Unsupported(reason) => {
                self.apply_unsupported(line_start, reason)?;
            }
            PreparedLine::StartParagraph { content_start } => {
                self.finish_pending_gap(line_start)?;
                let has_reference = admission.reference.is_some();
                let mut paragraph = ParagraphState {
                    source_start: line_start,
                    content_start,
                    reference: None,
                    definitions: DefinitionAuthority::Exact(Vec::new()),
                    visible_start: (!has_reference).then_some(content_start),
                    leading_restart_cut: None,
                };
                apply_reference_admission(&mut paragraph, &mut admission)?;
                self.state = DocumentState::Paragraph(paragraph);
            }
            PreparedLine::ContinueParagraph => {
                let DocumentState::Paragraph(paragraph) = &mut self.state else {
                    return Err(M11CleanControllerFault::IncompleteAdmission);
                };
                apply_reference_admission(paragraph, &mut admission)?;
            }
            PreparedLine::StartBlockQuote {
                facts: quote_facts,
                has_bof_bom,
            } => {
                self.apply_start_block_quote(line_start, facts, quote_facts, has_bof_bom)?;
            }
            PreparedLine::ContinueBlockQuote {
                facts: quote_facts,
                has_bof_bom,
            } => {
                self.apply_continue_block_quote(line_start, facts, quote_facts, has_bof_bom)?;
            }
            PreparedLine::ContinueBlockQuoteLazy => {
                self.apply_continue_block_quote_lazy(line_start, facts)?;
            }
            PreparedLine::StartTightList {
                facts: list_facts,
                has_bof_bom,
            } => {
                self.apply_start_tight_list(line_start, facts, list_facts, has_bof_bom)?;
            }
            PreparedLine::ContinueTightList {
                facts: list_facts,
                has_bof_bom,
            } => {
                self.apply_continue_tight_list(line_start, facts, list_facts, has_bof_bom)?;
            }
            PreparedLine::PendingTightListBlank => {
                self.apply_pending_tight_list_blank(line_start)?;
            }
            PreparedLine::RejectOpenTightList { reason } => {
                self.apply_reject_open_tight_list(reason)?;
            }
            PreparedLine::AtxHeading {
                facts: heading_facts,
                opening_indent,
                has_bof_bom,
            } => {
                self.apply_atx_heading(
                    line_start,
                    facts,
                    heading_facts,
                    opening_indent,
                    has_bof_bom,
                )?;
            }
            PreparedLine::SetextHeading {
                facts: heading_facts,
                opening_indent,
            } => {
                if admission.reference.is_some() || !admission.pending_definitions.is_empty() {
                    return Err(M11CleanControllerFault::IncompleteAdmission);
                }
                self.apply_setext_heading(line_start, facts, heading_facts, opening_indent)?;
            }
            PreparedLine::ThematicBreak {
                facts: thematic_facts,
                opening_indent,
                has_bof_bom,
            } => {
                self.apply_thematic_break(
                    line_start,
                    facts,
                    thematic_facts,
                    opening_indent,
                    has_bof_bom,
                )?;
            }
            PreparedLine::StartFencedCode {
                marker,
                opening_run_length,
                opening_indent,
                opening_marker_start,
            } => {
                self.apply_start_fenced_code(
                    line_start,
                    facts,
                    marker,
                    opening_run_length,
                    opening_indent,
                    opening_marker_start,
                )?;
            }
            PreparedLine::ContinueFencedCode => {
                if !matches!(self.state, DocumentState::FencedCode(_)) {
                    return Err(M11CleanControllerFault::IncompleteAdmission);
                }
            }
            PreparedLine::CloseFencedCode {
                closing_run_length,
                closing_marker_start,
            } => {
                self.apply_close_fenced_code(
                    line_start,
                    facts,
                    closing_run_length,
                    closing_marker_start,
                )?;
            }
            PreparedLine::StartIndentedCode {
                facts: code_facts,
                has_bof_bom,
            } => {
                self.apply_start_indented_code(line_start, facts, code_facts, has_bof_bom)?;
            }
            PreparedLine::ContinueIndentedCode { facts: code_facts } => {
                self.apply_continue_indented_code(line_start, facts, code_facts)?;
            }
            PreparedLine::PendingIndentedBlank { facts: code_facts } => {
                self.apply_pending_indented_blank(line_start, facts, code_facts)?;
            }
            PreparedLine::ReferenceOnlySetext { visible_start } => {
                let DocumentState::Paragraph(paragraph) = &mut self.state else {
                    return Err(M11CleanControllerFault::IncompleteAdmission);
                };
                paragraph.reference = None;
                paragraph
                    .definitions
                    .append_exact(&mut admission.pending_definitions)?;
                paragraph.visible_start = Some(visible_start);
            }
        }
        if let (Some(restart_cut), DocumentState::Paragraph(paragraph)) =
            (restart_cut, &mut self.state)
        {
            paragraph.leading_restart_cut = Some(restart_cut);
        }
        Ok(())
    }

    fn leading_restart_cut_for_admission(
        &self,
        admission: &M11CleanLineAdmission,
        facts: M11PhysicalLineFacts,
    ) -> Result<Option<LeadingRestartCut>, M11CleanControllerFault> {
        let Some(reference) = admission.reference.as_ref() else {
            return Ok(None);
        };
        let Some(terminal) = reference.terminal() else {
            return Ok(None);
        };
        if matches!(terminal, SegmentedReferenceTerminal::NoDefinitions { .. }) {
            return Ok(None);
        }
        let committed = match &self.state {
            DocumentState::Paragraph(paragraph) => paragraph.definitions.count(),
            DocumentState::BetweenBlocks { .. }
            | DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::IndentedCode(_)
            | DocumentState::UnknownSuffix { .. } => 0,
        };
        let definition_count = committed
            .checked_add(admission.pending_definitions.len())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        if definition_count == 0 || reference.definition_count() != definition_count {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let byte_end = terminal.prefix_end(reference.base());
        if byte_end == admission.identity.start_byte() {
            return Ok(Some(LeadingRestartCut {
                byte_end,
                utf16_end: self.next_utf16,
                next_physical_line_ordinal: self.next_ordinal,
                definition_count,
            }));
        }
        if byte_end == admission.identity.end_byte() {
            return Ok(Some(LeadingRestartCut {
                byte_end,
                utf16_end: self
                    .next_utf16
                    .checked_add(facts.physical_utf16())
                    .ok_or(M11CleanControllerFault::MetricOverflow)?,
                next_physical_line_ordinal: self
                    .next_ordinal
                    .checked_add(1)
                    .ok_or(M11CleanControllerFault::OrdinalExhausted)?,
                definition_count,
            }));
        }
        Ok(None)
    }

    fn maybe_record_ordinary_paragraph_restart(
        &mut self,
        committed: M11CommittedPhysicalLine,
    ) -> Result<(), M11CleanControllerFault> {
        if self.next_start < self.next_ordinary_paragraph_checkpoint_byte {
            return Ok(());
        }
        let DocumentState::Paragraph(paragraph) = &self.state else {
            return Ok(());
        };
        if !paragraph_is_ordinary_definition_free(paragraph) {
            return Ok(());
        }
        let block_entry_ordinal = self
            .block_entry_ordinal_base
            .checked_add(
                u64::try_from(self.leaves.len())
                    .map_err(|_| M11CleanControllerFault::MetricOverflow)?,
            )
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        self.ordinary_paragraph_restart_seeds
            .try_reserve(1)
            .map_err(|_| M11CleanControllerFault::CheckpointAllocationFailed)?;
        self.ordinary_paragraph_restart_seeds
            .push(OrdinaryParagraphRestartSeed {
                frozen_reference_definition_count: self.frozen_reference_definition_count,
                paragraph_source_start_byte: paragraph.source_start.byte,
                paragraph_source_start_utf16: paragraph.source_start.utf16,
                paragraph_content_start: paragraph.content_start,
                block_entry_ordinal,
                preceding_line_start_byte: committed.start_byte,
                preceding_line_start_utf16: committed.start_utf16,
                preceding_line_content_bytes: committed.content_bytes,
                preceding_line_content_utf16: committed.content_utf16,
                preceding_line_physical_bytes: committed.physical_bytes,
                preceding_line_physical_utf16: committed.physical_utf16,
                prefix_end_byte: self.next_start,
                prefix_end_utf16: self.next_utf16,
                next_physical_line_ordinal: self.next_ordinal,
            });
        self.next_ordinary_paragraph_checkpoint_byte = self
            .next_start
            .checked_add(M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES)
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        Ok(())
    }
}

impl<S> M11ExactController<S> for M11CleanBlockController
where
    S: M11SourceLineSource<Identity = SourceLineIdentity>,
{
    type Admission = M11CleanLineAdmission;
    type Error = M11CleanControllerError<S::Error>;

    fn begin_source_line(
        &mut self,
        identity: SourceLineIdentity,
    ) -> Result<Self::Admission, Self::Error> {
        if self.active_admission.is_some() {
            return Err(M11CleanControllerFault::AdmissionAlreadyActive.into());
        }
        if identity.ordinal() != self.next_ordinal || identity.start_byte() != self.next_start {
            return Err(M11CleanControllerFault::LineOutOfSequence {
                expected_ordinal: self.next_ordinal,
                actual_ordinal: identity.ordinal(),
                expected_start: self.next_start,
                actual_start: identity.start_byte(),
            }
            .into());
        }
        if let Some(source) = self.source {
            if identity.source() != source {
                return Err(M11CleanControllerFault::SourceChanged {
                    expected: source,
                    actual: identity.source(),
                }
                .into());
            }
        } else {
            self.source = Some(identity.source());
        }
        let admission_id = self.next_admission;
        self.next_admission = self.next_admission.wrapping_add(1).max(1);
        self.active_admission = Some(admission_id);
        let expected_len = usize::try_from(identity.physical_bytes())
            .map_err(|_| M11CleanControllerFault::MetricOverflow)?;
        let reference_seed = match &self.state {
            DocumentState::BetweenBlocks { .. } | DocumentState::IndentedCode(_) => {
                ReferenceSeed::Potential
            }
            DocumentState::Paragraph(ParagraphState {
                reference: Some(reference),
                ..
            }) => ReferenceSeed::Active(reference.clone()),
            DocumentState::Paragraph(_)
            | DocumentState::BlockQuote(_)
            | DocumentState::TightList(_)
            | DocumentState::FencedCode(_)
            | DocumentState::UnknownSuffix { .. } => ReferenceSeed::None,
        };
        Ok(M11CleanLineAdmission::new(
            self.controller_id,
            admission_id,
            identity,
            expected_len,
            reference_seed,
        ))
    }

    fn poll_source_line(
        &mut self,
        admission: &mut Self::Admission,
        source: &mut S,
        fuel: usize,
    ) -> Result<M11SourceLinePollReceipt, Self::Error> {
        self.validate_admission(admission)?;
        if fuel == 0 {
            return Err(M11CleanControllerFault::ZeroFuel.into());
        }
        match admission.stage {
            AdmissionStage::Matched(_) => {
                return Err(M11CleanControllerFault::PollAfterComplete.into());
            }
            AdmissionStage::Failed => {
                return Err(M11CleanControllerFault::PollAfterFailure.into());
            }
            AdmissionStage::Reading | AdmissionStage::Classifying(_) => {}
        }
        if source.identity() != admission.identity {
            admission.stage = AdmissionStage::Failed;
            return Err(M11CleanControllerFault::FactsMismatch.into());
        }
        if source.len() != admission.expected_len {
            admission.stage = AdmissionStage::Failed;
            return Err(M11CleanControllerFault::SourceLengthMismatch {
                expected: admission.expected_len,
                actual: source.len(),
            }
            .into());
        }

        let mut work = 0;
        let mut source_reads = 0;
        while work < fuel {
            match admission.stage {
                AdmissionStage::Reading if admission.cursor < admission.expected_len => {
                    if source.access_budget() == 0 {
                        break;
                    }
                    let offset = admission.cursor;
                    let byte = match source.read_byte(offset) {
                        Ok(byte) => byte,
                        Err(error) => {
                            admission.stage = AdmissionStage::Failed;
                            return Err(M11CleanControllerError::Source(error));
                        }
                    };
                    if let Err(error) = admission.observe_byte(offset, byte) {
                        admission.stage = AdmissionStage::Failed;
                        return Err(error.into());
                    }
                    admission.cursor += 1;
                    source_reads += 1;
                    work += 1;
                }
                AdmissionStage::Reading => {
                    if let Err(error) = self.prepare_classifier(admission) {
                        admission.stage = AdmissionStage::Failed;
                        return Err(error.into());
                    }
                }
                AdmissionStage::Classifying(_) => {
                    if let Err(error) = self.advance_classifier(admission) {
                        admission.stage = AdmissionStage::Failed;
                        return Err(error.into());
                    }
                    work += 1;
                }
                AdmissionStage::Matched(_) => break,
                AdmissionStage::Failed => {
                    return Err(M11CleanControllerFault::PollAfterFailure.into());
                }
            }
        }

        let status = if matches!(admission.stage, AdmissionStage::Matched(_)) {
            M11SourceLinePollStatus::Matched
        } else {
            M11SourceLinePollStatus::NeedMore
        };
        Ok(M11SourceLinePollReceipt {
            status,
            lexical_work_units: work,
            source_first_reads: source_reads,
            physical_high_water: admission.cursor,
            retained_source_bytes: admission.retained_len(),
            source_budget_exhausted: matches!(admission.stage, AdmissionStage::Reading)
                && admission.cursor < admission.expected_len
                && source.access_budget() == 0,
            maximum_source_request_rewind_bytes: 0,
        })
    }

    fn commit_source_line(
        &mut self,
        admission: Self::Admission,
        facts: M11PhysicalLineFacts,
    ) -> Result<(), Self::Error> {
        self.validate_admission(&admission)?;
        if !matches!(admission.stage, AdmissionStage::Matched(_)) {
            return Err(M11CleanControllerFault::IncompleteAdmission.into());
        }
        if facts.identity() != admission.identity || !admission.matches_facts(facts) {
            return Err(M11CleanControllerFault::FactsMismatch.into());
        }
        let next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(M11CleanControllerFault::OrdinalExhausted)?;
        let next_start = admission.identity.end_byte();
        let committed = M11CommittedPhysicalLine {
            ordinal: admission.identity.ordinal(),
            start_byte: admission.identity.start_byte(),
            start_utf16: self.next_utf16,
            content_bytes: facts.content_bytes(),
            content_utf16: facts.content_utf16(),
            physical_bytes: facts.physical_bytes(),
            physical_utf16: facts.physical_utf16(),
        };
        let next_utf16 = self
            .next_utf16
            .checked_add(facts.physical_utf16())
            .ok_or(M11CleanControllerFault::MetricOverflow)?;
        self.apply_prepared(admission, facts)?;
        self.next_ordinal = next_ordinal;
        self.next_start = next_start;
        self.next_utf16 = next_utf16;
        self.last_committed_line = Some(committed);
        self.active_admission = None;
        self.maybe_record_ordinary_paragraph_restart(committed)?;
        Ok(())
    }

    fn cancel_source_line(&mut self, admission: Self::Admission) -> Result<(), Self::Error> {
        self.validate_admission(&admission)?;
        self.active_admission = None;
        Ok(())
    }
}

/// Opaque, move-only work for one physical line.
pub struct M11CleanLineAdmission {
    controller_id: u64,
    admission_id: u64,
    identity: SourceLineIdentity,
    expected_len: usize,
    cursor: usize,
    retained: Vec<u8>,
    line_scanner: Option<SegmentedLineScanner>,
    segmented_facts: Option<SegmentedLineFacts>,
    reference: Option<SegmentedReferencePrefix>,
    reference_mode: ReferenceFeedMode,
    reference_next_relative: usize,
    tail: [u8; 2],
    tail_len: usize,
    pending_definitions: Vec<M11ReferenceDefinition>,
    stage: AdmissionStage,
}

enum ReferenceSeed {
    None,
    Potential,
    Active(SegmentedReferencePrefix),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReferenceFeedMode {
    None,
    Potential,
    Feeding,
}

impl M11CleanLineAdmission {
    fn new(
        controller_id: u64,
        admission_id: u64,
        identity: SourceLineIdentity,
        expected_len: usize,
        reference_seed: ReferenceSeed,
    ) -> Self {
        let (reference, reference_mode) = match reference_seed {
            ReferenceSeed::None => (None, ReferenceFeedMode::None),
            ReferenceSeed::Potential => (None, ReferenceFeedMode::Potential),
            ReferenceSeed::Active(reference) => (Some(reference), ReferenceFeedMode::Feeding),
        };
        Self {
            controller_id,
            admission_id,
            identity,
            expected_len,
            cursor: 0,
            retained: Vec::with_capacity(expected_len.min(SEGMENTED_LINE_PREFIX_BYTES)),
            line_scanner: Some(SegmentedLineScanner::new(identity.ordinal() == 0)),
            segmented_facts: None,
            reference,
            reference_mode,
            reference_next_relative: 0,
            tail: [0; 2],
            tail_len: 0,
            pending_definitions: Vec::new(),
            stage: AdmissionStage::Reading,
        }
    }

    fn observe_byte(&mut self, offset: usize, byte: u8) -> Result<(), M11CleanControllerFault> {
        if self.reference_mode == ReferenceFeedMode::Potential
            && self.retained.len() < SEGMENTED_LINE_PREFIX_BYTES
        {
            self.retained.push(byte);
        }
        self.line_scanner
            .as_mut()
            .ok_or(M11CleanControllerFault::IncompleteAdmission)?
            .push(byte);
        if self.tail_len < 2 {
            self.tail[self.tail_len] = byte;
            self.tail_len += 1;
        } else {
            self.tail[0] = self.tail[1];
            self.tail[1] = byte;
        }
        match self.reference_mode {
            ReferenceFeedMode::None => {}
            ReferenceFeedMode::Potential => self.try_start_reference()?,
            ReferenceFeedMode::Feeding => self.feed_reference_byte(offset, byte)?,
        }
        Ok(())
    }

    fn try_start_reference(&mut self) -> Result<(), M11CleanControllerFault> {
        let first_nonspace = self
            .line_scanner
            .as_ref()
            .and_then(SegmentedLineScanner::first_nonspace)
            .or_else(|| {
                self.segmented_facts
                    .filter(|facts| !facts.blank)
                    .map(|facts| facts.first_nonspace)
            });
        let Some(first_nonspace) = first_nonspace else {
            return Ok(());
        };
        if self.retained.get(first_nonspace) != Some(&b'[') {
            self.reference_mode = ReferenceFeedMode::None;
            self.retained.clear();
            return Ok(());
        }
        let base = add_u32_usize(self.identity.start_byte(), first_nonspace)?;
        self.reference = Some(SegmentedReferencePrefix::new(base));
        self.reference_next_relative = first_nonspace;
        for relative in first_nonspace..self.retained.len() {
            let byte = self.retained[relative];
            self.feed_reference_byte(relative, byte)?;
        }
        self.reference_next_relative = self.retained.len();
        self.reference_mode = ReferenceFeedMode::Feeding;
        self.retained.clear();
        Ok(())
    }

    fn feed_reference_byte(
        &mut self,
        relative: usize,
        byte: u8,
    ) -> Result<(), M11CleanControllerFault> {
        if relative != self.reference_next_relative {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        let absolute = add_u32_usize(self.identity.start_byte(), relative)?;
        if let Some(definition) = self
            .reference
            .as_mut()
            .ok_or(M11CleanControllerFault::IncompleteAdmission)?
            .push(absolute, byte)?
        {
            self.pending_definitions
                .push(map_segmented_definition(definition));
        }
        self.reference_next_relative += 1;
        Ok(())
    }

    fn finish_segmented_read(&mut self) -> Result<(), M11CleanControllerFault> {
        let scanner = self
            .line_scanner
            .take()
            .ok_or(M11CleanControllerFault::IncompleteAdmission)?;
        let facts = scanner.finish()?;
        self.segmented_facts = Some(facts);
        if self.reference_mode == ReferenceFeedMode::Potential {
            self.try_start_reference()?;
            if self.reference_mode == ReferenceFeedMode::Potential {
                self.reference_mode = ReferenceFeedMode::None;
                self.retained.clear();
            }
        }
        if self.reference_mode == ReferenceFeedMode::Feeding
            && self.reference_next_relative != self.expected_len
        {
            return Err(M11CleanControllerFault::FactsMismatch);
        }
        if !facts.had_ending {
            if let Some(reference) = &mut self.reference {
                self.pending_definitions.extend(
                    reference
                        .finish_eof()?
                        .into_iter()
                        .map(map_segmented_definition),
                );
            }
        }
        Ok(())
    }

    fn segmented_facts(&self) -> Result<SegmentedLineFacts, M11CleanControllerFault> {
        self.segmented_facts
            .ok_or(M11CleanControllerFault::IncompleteAdmission)
    }

    fn retained_len(&self) -> usize {
        self.retained.len()
            + self
                .line_scanner
                .as_ref()
                .map_or(0, SegmentedLineScanner::retained_source_bytes)
            + self
                .reference
                .as_ref()
                .map_or(0, SegmentedReferencePrefix::retained_source_bytes)
    }

    fn matches_facts(&self, facts: M11PhysicalLineFacts) -> bool {
        if facts.physical_bytes() != self.identity.physical_bytes() {
            return false;
        }
        let terminator_bytes = match facts.ending() {
            M11LineEnding::Eof => 0,
            M11LineEnding::Lf | M11LineEnding::Cr => 1,
            M11LineEnding::CrLf => 2,
        };
        if facts.content_bytes().checked_add(terminator_bytes) != Some(facts.physical_bytes()) {
            return false;
        }
        match facts.ending() {
            M11LineEnding::Eof => true,
            M11LineEnding::Lf => self.tail_len >= 1 && self.tail[self.tail_len - 1] == b'\n',
            M11LineEnding::Cr => self.tail_len >= 1 && self.tail[self.tail_len - 1] == b'\r',
            M11LineEnding::CrLf => self.tail_len == 2 && self.tail == [b'\r', b'\n'],
        }
    }
}

#[derive(Clone, Copy)]
enum AdmissionStage {
    Reading,
    Classifying(Classifier),
    Matched(PreparedAdmission),
    Failed,
}

#[derive(Clone, Copy)]
struct PreparedAdmission {
    line: PreparedLine,
    finish_indented_before_apply: bool,
    finish_block_quote_before_apply: bool,
    finish_tight_list_before_apply: bool,
}

#[derive(Clone, Copy)]
struct Classifier {
    stage: OpenerStage,
    first_nonspace: usize,
    first_nonspace_source: u32,
    paragraph_open: bool,
    finish_indented_before_apply: bool,
    finish_block_quote_before_apply: bool,
    finish_tight_list_before_apply: bool,
}

#[derive(Clone, Copy)]
enum OpenerStage {
    Prepare,
    BlockQuote,
    AtxHeading,
    FencedCode,
    HtmlBlock,
    SetextHeading,
    SetextReference,
    ThematicBreak,
    List,
    IndentedCode,
    TableCandidate,
    Paragraph,
}

#[derive(Clone, Copy)]
enum PreparedLine {
    Noop,
    Blank,
    StartParagraph {
        content_start: u32,
    },
    ContinueParagraph,
    StartBlockQuote {
        facts: SegmentedBlockQuoteFacts,
        has_bof_bom: bool,
    },
    ContinueBlockQuote {
        facts: SegmentedBlockQuoteFacts,
        has_bof_bom: bool,
    },
    ContinueBlockQuoteLazy,
    StartTightList {
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
    },
    ContinueTightList {
        facts: SegmentedListItemFacts,
        has_bof_bom: bool,
    },
    PendingTightListBlank,
    RejectOpenTightList {
        reason: M11ListUnsupportedReason,
    },
    AtxHeading {
        facts: SegmentedAtxHeadingFacts,
        opening_indent: u8,
        has_bof_bom: bool,
    },
    SetextHeading {
        facts: SegmentedSetextHeadingFacts,
        opening_indent: u8,
    },
    ThematicBreak {
        facts: SegmentedThematicBreakFacts,
        opening_indent: u8,
        has_bof_bom: bool,
    },
    StartFencedCode {
        marker: u8,
        opening_run_length: u32,
        opening_indent: u8,
        opening_marker_start: u32,
    },
    ContinueFencedCode,
    CloseFencedCode {
        closing_run_length: u32,
        closing_marker_start: u32,
    },
    StartIndentedCode {
        facts: SegmentedIndentedCodeLineFacts,
        has_bof_bom: bool,
    },
    ContinueIndentedCode {
        facts: SegmentedIndentedCodeLineFacts,
    },
    PendingIndentedBlank {
        facts: SegmentedIndentedCodeLineFacts,
    },
    ReferenceOnlySetext {
        visible_start: u32,
    },
    Unsupported(M11UnknownReason),
}

enum DocumentState {
    BetweenBlocks {
        pending_gap_start: Option<SourceCut>,
    },
    Paragraph(ParagraphState),
    BlockQuote(BlockQuoteState),
    TightList(TightListState),
    FencedCode(FencedCodeState),
    IndentedCode(IndentedCodeState),
    UnknownSuffix {
        source_start: SourceCut,
        reason: M11UnknownReason,
    },
}

struct BlockQuoteState {
    source_start: SourceCut,
    source_end: SourceCut,
    lines: Vec<M11BlockQuoteLineMapping>,
    paragraph: Option<BlockQuoteParagraphState>,
    paragraph_closed: bool,
    container_paragraph_open: bool,
    unsupported: Option<M11BlockQuoteUnsupportedReason>,
}

struct BlockQuoteParagraphState {
    line_indices: Range<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TightListKind {
    Bullet { marker: u8 },
    Ordered { start: u32, delimiter: u8 },
}

enum TightListItemMapping {
    Bullet(M11BulletListItemMapping),
    Ordered(M11OrderedListItemMapping),
}

impl TightListItemMapping {
    const fn ordinal(&self) -> u32 {
        match self {
            Self::Bullet(item) => item.ordinal,
            Self::Ordered(item) => item.ordinal,
        }
    }

    const fn source(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.source,
            Self::Ordered(item) => &item.source,
        }
    }

    const fn source_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.source_utf16,
            Self::Ordered(item) => &item.source_utf16,
        }
    }

    const fn content_source(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.content_source,
            Self::Ordered(item) => &item.content_source,
        }
    }

    const fn content_source_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.content_source_utf16,
            Self::Ordered(item) => &item.content_source_utf16,
        }
    }

    const fn line_ending(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.line_ending,
            Self::Ordered(item) => &item.line_ending,
        }
    }

    const fn line_ending_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.line_ending_utf16,
            Self::Ordered(item) => &item.line_ending_utf16,
        }
    }

    const fn has_paragraph(&self) -> bool {
        match self {
            Self::Bullet(item) => item.paragraph.is_some(),
            Self::Ordered(item) => item.paragraph.is_some(),
        }
    }

    const fn matches_kind(&self, kind: TightListKind) -> bool {
        match (self, kind) {
            (Self::Bullet(item), TightListKind::Bullet { marker }) => item.marker == marker,
            (
                Self::Ordered(item),
                TightListKind::Ordered {
                    delimiter,
                    start: _,
                },
            ) => item.delimiter == delimiter,
            _ => false,
        }
    }
}

struct TightListState {
    source_start: SourceCut,
    source_end: SourceCut,
    kind: TightListKind,
    items: Vec<TightListItemMapping>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    current_content_indent: usize,
    terminal_empty: bool,
    pending_blank_start: Option<SourceCut>,
}

struct ParagraphState {
    source_start: SourceCut,
    content_start: u32,
    reference: Option<SegmentedReferencePrefix>,
    definitions: DefinitionAuthority,
    visible_start: Option<u32>,
    leading_restart_cut: Option<LeadingRestartCut>,
}

struct FencedCodeState {
    source_start: SourceCut,
    opening_marker: Range<u32>,
    raw_info_source: Range<u32>,
    body_start: u32,
    marker: u8,
    opening_indent: u8,
}

struct IndentedCodeState {
    source_start: SourceCut,
    source_end: SourceCut,
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    terminal_eol_bytes: u32,
    has_bof_bom: bool,
    pending_blanks: Option<PendingIndentedBlanks>,
}

struct PendingIndentedBlanks {
    source_start: SourceCut,
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

#[derive(Clone, Copy)]
struct IndentedLineSummary {
    source_end: SourceCut,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    terminal_eol_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceCut {
    pub(crate) byte: u32,
    pub(crate) utf16: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LeadingRestartCut {
    byte_end: u32,
    utf16_end: u32,
    next_physical_line_ordinal: u32,
    definition_count: usize,
}

fn apply_reference_admission(
    paragraph: &mut ParagraphState,
    admission: &mut M11CleanLineAdmission,
) -> Result<(), M11CleanControllerFault> {
    paragraph
        .definitions
        .append_exact(&mut admission.pending_definitions)?;
    let Some(reference) = admission.reference.take() else {
        return Ok(());
    };
    if let Some(terminal) = reference.terminal() {
        paragraph.visible_start = Some(reference_visible_start(
            terminal,
            reference.base(),
            paragraph.content_start,
            paragraph.definitions.count() == 0,
        ));
        paragraph.reference = matches!(terminal, SegmentedReferenceTerminal::ReferenceOnly { .. })
            .then_some(reference);
    } else {
        paragraph.reference = Some(reference);
    }
    Ok(())
}

fn restart_availability(paragraph: &ParagraphState) -> RestartAvailability {
    let Some(cut) = paragraph.leading_restart_cut else {
        return RestartAvailability::Ineligible;
    };
    if !matches!(&paragraph.definitions, DefinitionAuthority::Exact(_))
        || paragraph.definitions.count() == 0
        || paragraph.definitions.count() != cut.definition_count
    {
        return RestartAvailability::Ineligible;
    }
    RestartAvailability::Eligible(LeadingReferencesRestartSeed {
        paragraph_content_start: paragraph.content_start,
        prefix_end_byte: cut.byte_end,
        prefix_end_utf16: cut.utf16_end,
        next_physical_line_ordinal: cut.next_physical_line_ordinal,
        definition_count: cut.definition_count,
    })
}

fn paragraph_is_ordinary_definition_free(paragraph: &ParagraphState) -> bool {
    paragraph.reference.is_none()
        && matches!(
            &paragraph.definitions,
            DefinitionAuthority::Exact(definitions) if definitions.is_empty()
        )
        && paragraph.visible_start == Some(paragraph.content_start)
}

fn ordinary_document_restart_availability(
    definitions: &DefinitionAuthority,
    leaves: &[M11CleanLeaf],
    mut seeds: Vec<OrdinaryParagraphRestartSeed>,
) -> OrdinaryParagraphRestartAvailability {
    let DefinitionAuthority::Exact(definitions) = definitions else {
        return OrdinaryParagraphRestartAvailability::Ineligible;
    };
    let Ok(top_level_block_count) = u64::try_from(leaves.len()) else {
        return OrdinaryParagraphRestartAvailability::Ineligible;
    };
    if leaves
        .iter()
        .any(|leaf| matches!(leaf, M11CleanLeaf::Unsupported { .. }))
        || seeds.iter().any(|seed| {
            seed.paragraph_source_start_byte > seed.paragraph_content_start
                || seed.paragraph_content_start > seed.preceding_line_start_byte
                || seed.block_entry_ordinal >= top_level_block_count
        })
    {
        return OrdinaryParagraphRestartAvailability::Ineligible;
    }

    let mut observed_definition_count = 0_usize;
    let mut last_definition_leaf_ordinal = None;
    for (ordinal, leaf) in leaves.iter().enumerate() {
        let count = leaf.reference_definition_count();
        let Some(next_count) = observed_definition_count.checked_add(count) else {
            return OrdinaryParagraphRestartAvailability::Ineligible;
        };
        observed_definition_count = next_count;
        if count != 0 {
            let Ok(ordinal) = u64::try_from(ordinal) else {
                return OrdinaryParagraphRestartAvailability::Ineligible;
            };
            last_definition_leaf_ordinal = Some(ordinal);
        }
    }
    if observed_definition_count != definitions.len() {
        return OrdinaryParagraphRestartAvailability::Ineligible;
    }

    if let Some(last_definition_leaf_ordinal) = last_definition_leaf_ordinal {
        seeds.retain(|seed| seed.block_entry_ordinal > last_definition_leaf_ordinal);
        if seeds.is_empty() {
            return OrdinaryParagraphRestartAvailability::Ineligible;
        }
    }
    for seed in &mut seeds {
        seed.frozen_reference_definition_count = definitions.len();
    }

    OrdinaryParagraphRestartAvailability::Eligible {
        seeds,
        top_level_block_count,
    }
}

fn append_definition_authority(
    target: &mut DefinitionAuthority,
    source: DefinitionAuthority,
) -> Result<(), M11CleanControllerFault> {
    match source {
        DefinitionAuthority::Exact(mut definitions) => target.append_exact(&mut definitions),
        reused @ DefinitionAuthority::ReusedLeading { .. } => match target {
            DefinitionAuthority::Exact(existing) if existing.is_empty() => {
                *target = reused;
                Ok(())
            }
            DefinitionAuthority::Exact(_) | DefinitionAuthority::ReusedLeading { .. } => {
                Err(M11CleanControllerFault::CropAcceptedDefinition)
            }
        },
    }
}

fn leaves_partition_source(
    leaves: &[M11CleanLeaf],
    source_end: u32,
    source_end_utf16: u32,
) -> bool {
    leaves_partition_range(
        leaves,
        SourceCut { byte: 0, utf16: 0 },
        SourceCut {
            byte: source_end,
            utf16: source_end_utf16,
        },
    )
}

fn leaves_partition_range(leaves: &[M11CleanLeaf], start: SourceCut, end: SourceCut) -> bool {
    if start == end {
        return leaves.is_empty();
    }
    let mut next = start.byte;
    let mut next_utf16 = start.utf16;
    for leaf in leaves {
        let source = leaf.source_range();
        let source_utf16 = leaf.source_utf16_range();
        if source.start != next
            || source_utf16.start != next_utf16
            || source.start >= source.end
            || source_utf16.start >= source_utf16.end
        {
            return false;
        }
        next = source.end;
        next_utf16 = source_utf16.end;
    }
    next == end.byte && next_utf16 == end.utf16
}

const fn reference_visible_start(
    terminal: SegmentedReferenceTerminal,
    base: u32,
    content_start: u32,
    definitions_empty: bool,
) -> u32 {
    if definitions_empty {
        return content_start;
    }
    terminal.prefix_end(base)
}

fn map_segmented_definition(definition: SegmentedReferenceDefinition) -> M11ReferenceDefinition {
    M11ReferenceDefinition {
        source: definition.source,
        label_source: definition.label_source,
        destination_source: definition.destination_source,
        title_source: definition.title_source,
        normalized_label: definition.normalized_label,
    }
}

fn add_u32_usize(base: u32, offset: usize) -> Result<u32, M11CleanControllerFault> {
    base.checked_add(u32::try_from(offset).map_err(|_| M11CleanControllerFault::MetricOverflow)?)
        .ok_or(M11CleanControllerFault::MetricOverflow)
}

fn unsupported(opener: M11UnsupportedOpener) -> PreparedLine {
    PreparedLine::Unsupported(M11UnknownReason::UnsupportedOpener(opener))
}

fn is_block_quote_lazy_paragraph_continuation(facts: &SegmentedLineFacts) -> bool {
    facts.indent >= 4
        || (facts.atx_heading.is_none()
            && !facts.fence.opener_valid
            && facts.html_block_1_to_6.is_none()
            && facts.setext.is_none()
            && facts.thematic_break.is_none()
            && !facts.interrupting_list
            && !facts.table_delimiter_candidate)
}

fn line_starts_block_child(facts: &SegmentedLineFacts) -> bool {
    facts.block_quote
        || facts.atx_heading.is_some()
        || facts.fence.opener_valid
        || facts.html_block_1_to_6.is_some()
        || facts.html_block_7
        || facts.setext.is_some()
        || facts.thematic_break.is_some()
        || facts.list
        || facts.indented_code.is_some()
        || facts.table_delimiter_candidate
        || facts.first_significant_byte == Some(b'[')
}

fn line_interrupts_open_list_paragraph(facts: &SegmentedLineFacts) -> bool {
    facts.block_quote
        || facts.atx_heading.is_some()
        || facts.fence.opener_valid
        || facts.html_block_1_to_6.is_some()
        || facts.thematic_break.is_some()
        || facts.interrupting_list
}

#[cfg(test)]
mod checkpoint_selection_tests {
    use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, ParserProfileId};

    use super::{
        checkpoint_partition_probes, reset_checkpoint_partition_probes,
        M11OrdinaryParagraphCropPlan, M11OrdinaryParagraphRestartCheckpoint,
        M11OrdinaryParagraphRestartCheckpoints, M11ParserBinding,
        OrdinaryParagraphAwaitingContinuation,
    };

    const LARGE_CHECKPOINT_COUNT: usize = 1 << 18;

    fn large_checkpoint_collection() -> (DocumentRuntime, M11OrdinaryParagraphRestartCheckpoints) {
        let source = "x".repeat(LARGE_CHECKPOINT_COUNT + 1);
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let version = runtime.current_source_version().expect("source version");
        let binding =
            M11ParserBinding::current(ParserProfileId::new(73).expect("nonzero parser profile"));
        let checkpoints = (0..LARGE_CHECKPOINT_COUNT)
            .map(|index| {
                let preceding = u32::try_from(index).expect("test coordinate");
                let prefix = preceding.checked_add(1).expect("test prefix");
                M11OrdinaryParagraphRestartCheckpoint {
                    source: version,
                    binding,
                    frozen_reference_definition_count: 0,
                    paragraph_source_start_byte: 0,
                    paragraph_source_start_utf16: 0,
                    paragraph_content_start: 0,
                    block_entry_ordinal: 0,
                    preceding_line_start_byte: preceding,
                    preceding_line_start_utf16: preceding,
                    preceding_line_content_bytes: 1,
                    preceding_line_content_utf16: 1,
                    preceding_line_physical_bytes: 1,
                    preceding_line_physical_utf16: 1,
                    prefix_end_byte: prefix,
                    prefix_end_utf16: prefix,
                    next_physical_line_ordinal: prefix,
                    state: OrdinaryParagraphAwaitingContinuation { _private: () },
                }
            })
            .collect();
        (
            runtime,
            M11OrdinaryParagraphRestartCheckpoints::from_checkpoints(
                version,
                binding,
                checkpoints,
                1,
            ),
        )
    }

    fn close(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close poll").complete {}
    }

    #[test]
    fn large_checkpoint_admission_probes_logarithmically_for_every_crop_route() {
        let (runtime, checkpoints) = large_checkpoint_collection();
        let middle = LARGE_CHECKPOINT_COUNT / 2;
        let maximum_binary_probes =
            (usize::BITS - LARGE_CHECKPOINT_COUNT.leading_zeros()) as usize + 1;

        reset_checkpoint_partition_probes();
        let interior = checkpoints
            .select_crop(middle..middle + 1)
            .expect("interior selection");
        assert!(
            checkpoint_partition_probes() <= maximum_binary_probes * 2,
            "interior admission must use two binary boundary searches"
        );
        assert_eq!(interior.restart_index(), middle - 1);
        assert_eq!(interior.convergence_index(), middle + 1);

        reset_checkpoint_partition_probes();
        let bof = checkpoints
            .select_bof_crop(0..middle)
            .expect("BOF selection");
        assert!(
            checkpoint_partition_probes() <= maximum_binary_probes,
            "BOF admission must use one binary boundary search"
        );
        assert_eq!(bof.convergence_index(), middle);

        reset_checkpoint_partition_probes();
        let eof = checkpoints
            .select_eof_crop(middle..LARGE_CHECKPOINT_COUNT + 1)
            .expect("EOF selection");
        assert!(
            checkpoint_partition_probes() <= maximum_binary_probes,
            "EOF admission must use one binary boundary search"
        );
        assert_eq!(eof.restart_index(), middle - 1);

        close(runtime);
    }

    #[test]
    fn constant_time_restart_take_preserves_a_last_checkpoint_convergence() {
        let (runtime, checkpoints) = large_checkpoint_collection();
        let changed = LARGE_CHECKPOINT_COUNT - 2..LARGE_CHECKPOINT_COUNT - 1;
        let selection = checkpoints
            .select_crop(changed)
            .expect("selection converging at the old final checkpoint");
        assert_eq!(selection.convergence_index(), LARGE_CHECKPOINT_COUNT - 1);

        reset_checkpoint_partition_probes();
        let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");
        let maximum_binary_probes =
            (usize::BITS - LARGE_CHECKPOINT_COUNT.leading_zeros()) as usize + 1;
        assert!(
            checkpoint_partition_probes() <= maximum_binary_probes * 2,
            "consuming revalidation must remain logarithmic"
        );
        let convergence = plan.convergence().expect("mapped convergence");
        assert_eq!(
            convergence.preceding_line_start_byte() as usize,
            LARGE_CHECKPOINT_COUNT - 1
        );
        assert_eq!(
            convergence.prefix_end_byte() as usize,
            LARGE_CHECKPOINT_COUNT
        );

        close(runtime);
    }
}
