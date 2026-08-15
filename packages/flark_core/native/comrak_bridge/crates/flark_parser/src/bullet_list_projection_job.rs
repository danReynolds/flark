//! Exact demanded projection for one published top-level tight bullet list.
//!
//! Bullet lists deliberately reuse the bounded physical-line scanner and
//! persistent page lifecycle proven by the block-quote job. The shared engine
//! stream carries a distinct authenticated kind, and this wrapper keeps the
//! public job type honest about the grammar authority it consumes.

use std::fmt;

use flark_engine::parser_internal::{
    BlockQuoteLineV1, M11BlockQuoteProjectionBuild, M11BlockQuoteProjectionBuildStatus,
    M11BlockQuoteProjectionRoot, M11MarkedLineProjectionKind,
};
use flark_engine::DocumentRuntime;

use crate::block_quote_projection_job::{
    M11BlockQuoteProjectionJob, M11BlockQuoteProjectionJobError, M11BlockQuoteProjectionJobPoll,
    M11BlockQuoteProjectionJobReleasePoll, M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};
use crate::publication::{
    M11PublishedBulletListItemProjectionFence, M11PublishedBulletListLeafFence,
    M11PublishedOrderedListItemProjectionFence, PublishedBulletListItemProjectionAuthority,
    PublishedOrderedListItemProjectionAuthority,
};
use crate::{M11LineEnding, M11_GRAMMAR_REVISION};

pub const M11_BULLET_LIST_PROJECTION_JOB_MAX_POLL_TRANSITIONS: usize =
    M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS;

pub type M11BulletListProjectionJobError = M11BlockQuoteProjectionJobError;
pub type M11BulletListProjectionJobPoll = M11BlockQuoteProjectionJobPoll;
pub type M11BulletListProjectionJobReleasePoll = M11BlockQuoteProjectionJobReleasePoll;
pub type M11OrderedListItemProjectionJob = M11BulletListItemProjectionJob;

#[must_use = "bullet-list projection jobs require root transfer or explicit fuelled cancellation"]
pub struct M11BulletListProjectionJob {
    inner: M11BlockQuoteProjectionJob,
}

impl fmt::Debug for M11BulletListProjectionJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BulletListProjectionJob")
            .field("inner", &self.inner)
            .finish()
    }
}

impl M11BulletListProjectionJob {
    /// Starts exact item projection from one move-only retained variant-9
    /// authority fence.
    pub fn new(
        runtime: &DocumentRuntime,
        fence: M11PublishedBulletListLeafFence,
    ) -> Result<Self, M11BulletListProjectionJobError> {
        Ok(Self {
            inner: M11BlockQuoteProjectionJob::new_bullet_list(runtime, fence)?,
        })
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BulletListProjectionJobPoll, M11BulletListProjectionJobError> {
        self.inner.poll(runtime, fuel)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11BlockQuoteProjectionRoot> {
        self.inner.take_root()
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BulletListProjectionJobError> {
        self.inner.begin_cancel(runtime)
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BulletListProjectionJobReleasePoll, M11BulletListProjectionJobError> {
        self.inner.poll_cancel(runtime, fuel)
    }
}

/// Successful compact projection of exactly one parser-selected list item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11OrderedListItemProjectionMetadata {
    opening_marker_start: u32,
    opening_marker_end: u32,
    marker_value: u32,
}

impl M11OrderedListItemProjectionMetadata {
    #[must_use]
    pub const fn opening_marker_start(self) -> u32 {
        self.opening_marker_start
    }

    #[must_use]
    pub const fn opening_marker_end(self) -> u32 {
        self.opening_marker_end
    }

    #[must_use]
    pub const fn marker_value(self) -> u32 {
        self.marker_value
    }
}

/// Successful compact projection of exactly one parser-selected list item.
pub struct M11BulletListItemProjectionOutput {
    root: M11BlockQuoteProjectionRoot,
    selected_item_ordinal: u32,
    canonical_line_ending: M11LineEnding,
    terminal_empty: bool,
    ordered_item: Option<M11OrderedListItemProjectionMetadata>,
}

impl fmt::Debug for M11BulletListItemProjectionOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BulletListItemProjectionOutput")
            .field("selected_item_ordinal", &self.selected_item_ordinal)
            .field("canonical_line_ending", &self.canonical_line_ending)
            .field("terminal_empty", &self.terminal_empty)
            .field("ordered_item", &self.ordered_item)
            .finish_non_exhaustive()
    }
}

impl M11BulletListItemProjectionOutput {
    #[must_use]
    pub const fn selected_item_ordinal(&self) -> u32 {
        self.selected_item_ordinal
    }

    #[must_use]
    pub const fn canonical_line_ending(&self) -> M11LineEnding {
        self.canonical_line_ending
    }

    #[must_use]
    pub const fn terminal_empty(&self) -> bool {
        self.terminal_empty
    }

    #[must_use]
    pub const fn ordered_item(&self) -> Option<M11OrderedListItemProjectionMetadata> {
        self.ordered_item
    }

    #[must_use]
    pub const fn root(&self) -> &M11BlockQuoteProjectionRoot {
        &self.root
    }

    #[must_use]
    pub fn into_parts(self) -> (M11BlockQuoteProjectionRoot, u32, M11LineEnding, bool) {
        debug_assert!(self.ordered_item.is_none());
        (
            self.root,
            self.selected_item_ordinal,
            self.canonical_line_ending,
            self.terminal_empty,
        )
    }

    #[must_use]
    pub fn into_parts_with_metadata(
        self,
    ) -> (
        M11BlockQuoteProjectionRoot,
        u32,
        M11LineEnding,
        bool,
        Option<M11OrderedListItemProjectionMetadata>,
    ) {
        (
            self.root,
            self.selected_item_ordinal,
            self.canonical_line_ending,
            self.terminal_empty,
            self.ordered_item,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BulletListItemProjectionJobPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BulletListItemProjectionJobPoll {
    status: M11BulletListItemProjectionJobPollStatus,
    transitions: usize,
}

impl M11BulletListItemProjectionJobPoll {
    #[must_use]
    pub const fn status(self) -> M11BulletListItemProjectionJobPollStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BulletListItemProjectionJobReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11BulletListItemProjectionJobReleasePoll {
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemProjectionPhase {
    Offer,
    PollOfferedPage,
    FinishInput,
    Seal,
    Complete,
    Faulted,
    Cancelling,
    Cancelled,
    Transferred,
}

/// Fuelled persistent projection of exactly one parser-selected list item.
///
/// Item discovery and donor classification happen once while the publication
/// fence is minted. This job performs no source scan: it writes the one
/// certified mapping into a BQP2 root whose physical range is the complete
/// list and whose requested window is only the selected item.
#[must_use = "list-item projection jobs require output transfer or explicit fuelled cancellation"]
pub struct M11BulletListItemProjectionJob {
    source: flark_engine::SourceVersion,
    block_source: std::ops::Range<u32>,
    item_source: std::ops::Range<u32>,
    binding: crate::M11ParserBinding,
    selected_item_ordinal: u32,
    canonical_line_ending: M11LineEnding,
    terminal_empty: bool,
    projection_kind: M11MarkedLineProjectionKind,
    ordered_item: Option<M11OrderedListItemProjectionMetadata>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    record: BlockQuoteLineV1,
    authority: Option<flark_engine::parser_internal::M11ParserSourceRangeAuthority>,
    build: Option<M11BlockQuoteProjectionBuild>,
    root: Option<M11BlockQuoteProjectionRoot>,
    phase: ItemProjectionPhase,
    build_cancel_started: bool,
    root_release_started: bool,
}

impl fmt::Debug for M11BulletListItemProjectionJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BulletListItemProjectionJob")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("item_source", &self.item_source)
            .field("selected_item_ordinal", &self.selected_item_ordinal)
            .field("canonical_line_ending", &self.canonical_line_ending)
            .field("terminal_empty", &self.terminal_empty)
            .field("projection_kind", &self.projection_kind)
            .field("ordered_item", &self.ordered_item)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl M11BulletListItemProjectionJob {
    pub fn new(
        runtime: &DocumentRuntime,
        fence: M11PublishedBulletListItemProjectionFence,
    ) -> Result<Self, M11BulletListProjectionJobError> {
        let fenced = fence.into_projection_authority();
        Self::from_authority(runtime, fenced)
    }

    fn from_authority(
        runtime: &DocumentRuntime,
        fenced: PublishedBulletListItemProjectionAuthority,
    ) -> Result<Self, M11BulletListProjectionJobError> {
        if fenced.binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(
                M11BlockQuoteProjectionJobError::UnsupportedGrammarRevision {
                    expected: M11_GRAMMAR_REVISION,
                    actual: fenced.binding.grammar_revision(),
                },
            );
        }
        fenced
            .authority
            .validate(runtime)
            .map_err(|_| M11BlockQuoteProjectionJobError::SourceAuthorityMismatch)?;
        if fenced.authority.source() != fenced.source
            || fenced.authority.source_range()
                != (usize::try_from(fenced.item.source.start)
                    .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?
                    ..usize::try_from(fenced.item.source.end)
                        .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?)
        {
            return Err(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch);
        }
        let block = u32_range_as_usize(&fenced.block_source)?;
        let item = u32_range_as_usize(&fenced.item.source)?;
        if block.start >= block.end
            || item.start >= item.end
            || item.start < block.start
            || item.end > block.end
            || fenced.block_source_utf16.start >= fenced.block_source_utf16.end
            || fenced.item.source_utf16.start < fenced.block_source_utf16.start
            || fenced.item.source_utf16.end > fenced.block_source_utf16.end
            || fenced.item.source_utf16.start >= fenced.item.source_utf16.end
        {
            return Err(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch);
        }
        let item_bytes = item.end - item.start;
        if item_bytes > flark_engine::parser_internal::BLOCK_QUOTE_WINDOW_MAX_BYTES {
            return Err(M11BlockQuoteProjectionJobError::WindowTooLarge {
                bytes: item_bytes,
                cap: flark_engine::parser_internal::BLOCK_QUOTE_WINDOW_MAX_BYTES,
            });
        }

        let mapping = &fenced.item;
        let item_utf16 = checked_range_len(&mapping.source_utf16)?;
        let hidden_bytes = checked_range_len(&mapping.hidden_prefix)?;
        let hidden_utf16 = checked_range_len(&mapping.hidden_prefix_utf16)?;
        let content_bytes = checked_range_len(&mapping.content_source)?;
        let content_utf16 = checked_range_len(&mapping.content_source_utf16)?;
        let eol_bytes = checked_range_len(&mapping.line_ending)?;
        let eol_utf16 = checked_range_len(&mapping.line_ending_utf16)?;
        let continuation_start = mapping
            .continuation_prefix_source
            .start
            .checked_sub(mapping.source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let continuation_end = mapping
            .continuation_prefix_source
            .end
            .checked_sub(mapping.source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let physical_eol_bytes = match fenced.physical_line_ending {
            M11LineEnding::Lf | M11LineEnding::Cr => 1,
            M11LineEnding::CrLf => 2,
            M11LineEnding::Eof => 0,
        };
        let terminal_empty = mapping.paragraph.is_none();
        let mapping_is_valid = matches!(mapping.marker, b'-' | b'+' | b'*')
            && mapping.hidden_prefix.start == mapping.source.start
            && mapping.hidden_prefix.end == mapping.content_source.start
            && mapping.hidden_prefix_utf16.start == mapping.source_utf16.start
            && mapping.hidden_prefix_utf16.end == mapping.content_source_utf16.start
            && mapping.continuation_prefix_source.start >= mapping.source.start
            && mapping.continuation_prefix_source.end <= mapping.hidden_prefix.end
            && mapping.continuation_prefix_source.start < mapping.continuation_prefix_source.end
            && mapping.opening_marker.start >= mapping.continuation_prefix_source.start
            && mapping.opening_marker.end <= mapping.continuation_prefix_source.end
            && mapping
                .opening_marker
                .end
                .checked_sub(mapping.opening_marker.start)
                == Some(1)
            && mapping.content_source.end == mapping.line_ending.start
            && mapping.line_ending.end == mapping.source.end
            && mapping.content_source_utf16.end == mapping.line_ending_utf16.start
            && mapping.line_ending_utf16.end == mapping.source_utf16.end
            && eol_bytes == physical_eol_bytes
            && eol_utf16 == physical_eol_bytes
            && hidden_utf16
                .checked_add(content_utf16)
                .and_then(|length| length.checked_add(eol_utf16))
                == Some(item_utf16)
            && (content_bytes == 0) == (content_utf16 == 0)
            && terminal_empty == (content_bytes == 0);
        if !mapping_is_valid {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "selected list-item mapping is internally inconsistent",
            ));
        }

        let projected_utf8_length = content_bytes
            .checked_add(eol_bytes)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let projected_utf16_length = content_utf16
            .checked_add(eol_utf16)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let relative_start = mapping
            .source
            .start
            .checked_sub(fenced.block_source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let record = BlockQuoteLineV1::bullet_item(
            relative_start,
            u32::try_from(item_bytes)
                .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
            hidden_bytes,
            continuation_start,
            continuation_end,
            content_bytes,
            content_utf16,
        )?;

        let build_lease = runtime
            .snapshot_current_source()
            .map_err(|_| M11BlockQuoteProjectionJobError::SourceAuthorityMismatch)?;
        if build_lease.version() != fenced.source {
            return Err(M11BlockQuoteProjectionJobError::SourceAuthorityMismatch);
        }
        let build = M11BlockQuoteProjectionBuild::new_bullet_list(
            runtime,
            build_lease,
            block,
            item,
            projected_utf8_length,
            projected_utf16_length,
            fenced.binding.syntax_profile(),
        )?;
        Ok(Self {
            source: fenced.source,
            block_source: fenced.block_source,
            item_source: mapping.source.clone(),
            binding: fenced.binding,
            selected_item_ordinal: mapping.ordinal,
            canonical_line_ending: fenced.canonical_line_ending,
            terminal_empty,
            projection_kind: M11MarkedLineProjectionKind::BulletList,
            ordered_item: None,
            projected_utf8_length,
            projected_utf16_length,
            record,
            authority: Some(fenced.authority),
            build: Some(build),
            root: None,
            phase: ItemProjectionPhase::Offer,
            build_cancel_started: false,
            root_release_started: false,
        })
    }

    pub fn new_ordered(
        runtime: &DocumentRuntime,
        fence: M11PublishedOrderedListItemProjectionFence,
    ) -> Result<Self, M11BulletListProjectionJobError> {
        Self::from_ordered_authority(runtime, fence.into_projection_authority())
    }

    fn from_ordered_authority(
        runtime: &DocumentRuntime,
        fenced: PublishedOrderedListItemProjectionAuthority,
    ) -> Result<Self, M11BulletListProjectionJobError> {
        if fenced.binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(
                M11BlockQuoteProjectionJobError::UnsupportedGrammarRevision {
                    expected: M11_GRAMMAR_REVISION,
                    actual: fenced.binding.grammar_revision(),
                },
            );
        }
        fenced
            .authority
            .validate(runtime)
            .map_err(|_| M11BlockQuoteProjectionJobError::SourceAuthorityMismatch)?;
        if fenced.authority.source() != fenced.source
            || fenced.authority.source_range()
                != (usize::try_from(fenced.item.source.start)
                    .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?
                    ..usize::try_from(fenced.item.source.end)
                        .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?)
        {
            return Err(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch);
        }
        let block = u32_range_as_usize(&fenced.block_source)?;
        let item = u32_range_as_usize(&fenced.item.source)?;
        if block.start >= block.end
            || item.start >= item.end
            || item.start < block.start
            || item.end > block.end
            || fenced.block_source_utf16.start >= fenced.block_source_utf16.end
            || fenced.item.source_utf16.start < fenced.block_source_utf16.start
            || fenced.item.source_utf16.end > fenced.block_source_utf16.end
            || fenced.item.source_utf16.start >= fenced.item.source_utf16.end
        {
            return Err(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch);
        }
        let item_bytes = item.end - item.start;
        if item_bytes > flark_engine::parser_internal::BLOCK_QUOTE_WINDOW_MAX_BYTES {
            return Err(M11BlockQuoteProjectionJobError::WindowTooLarge {
                bytes: item_bytes,
                cap: flark_engine::parser_internal::BLOCK_QUOTE_WINDOW_MAX_BYTES,
            });
        }

        let mapping = &fenced.item;
        let item_utf16 = checked_range_len(&mapping.source_utf16)?;
        let hidden_bytes = checked_range_len(&mapping.hidden_prefix)?;
        let hidden_utf16 = checked_range_len(&mapping.hidden_prefix_utf16)?;
        let content_bytes = checked_range_len(&mapping.content_source)?;
        let content_utf16 = checked_range_len(&mapping.content_source_utf16)?;
        let eol_bytes = checked_range_len(&mapping.line_ending)?;
        let eol_utf16 = checked_range_len(&mapping.line_ending_utf16)?;
        let continuation_start = mapping
            .continuation_prefix_source
            .start
            .checked_sub(mapping.source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let continuation_end = mapping
            .continuation_prefix_source
            .end
            .checked_sub(mapping.source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let opening_marker_start = mapping
            .opening_marker
            .start
            .checked_sub(mapping.source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let opening_marker_end = mapping
            .opening_marker
            .end
            .checked_sub(mapping.source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let opening_marker_bytes = opening_marker_end
            .checked_sub(opening_marker_start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let physical_eol_bytes = match fenced.physical_line_ending {
            M11LineEnding::Lf | M11LineEnding::Cr => 1,
            M11LineEnding::CrLf => 2,
            M11LineEnding::Eof => 0,
        };
        let terminal_empty = mapping.paragraph.is_none();
        let mapping_is_valid = matches!(mapping.delimiter, b'.' | b')')
            && mapping.marker_value <= 999_999_999
            && (2..=10).contains(&opening_marker_bytes)
            && mapping.hidden_prefix.start == mapping.source.start
            && mapping.hidden_prefix.end == mapping.content_source.start
            && mapping.hidden_prefix_utf16.start == mapping.source_utf16.start
            && mapping.hidden_prefix_utf16.end == mapping.content_source_utf16.start
            && mapping.continuation_prefix_source.start >= mapping.source.start
            && mapping.continuation_prefix_source.end <= mapping.hidden_prefix.end
            && mapping.continuation_prefix_source.start < mapping.continuation_prefix_source.end
            && mapping.opening_marker.start >= mapping.continuation_prefix_source.start
            && mapping.opening_marker.end <= mapping.continuation_prefix_source.end
            && mapping.content_source.end == mapping.line_ending.start
            && mapping.line_ending.end == mapping.source.end
            && mapping.content_source_utf16.end == mapping.line_ending_utf16.start
            && mapping.line_ending_utf16.end == mapping.source_utf16.end
            && eol_bytes == physical_eol_bytes
            && eol_utf16 == physical_eol_bytes
            && hidden_utf16
                .checked_add(content_utf16)
                .and_then(|length| length.checked_add(eol_utf16))
                == Some(item_utf16)
            && (content_bytes == 0) == (content_utf16 == 0)
            && terminal_empty == (content_bytes == 0);
        if !mapping_is_valid {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "selected ordered-list item mapping is internally inconsistent",
            ));
        }

        let projected_utf8_length = content_bytes
            .checked_add(eol_bytes)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let projected_utf16_length = content_utf16
            .checked_add(eol_utf16)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let relative_start = mapping
            .source
            .start
            .checked_sub(fenced.block_source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let record = BlockQuoteLineV1::ordered_item(
            relative_start,
            u32::try_from(item_bytes)
                .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
            hidden_bytes,
            continuation_start,
            continuation_end,
            content_bytes,
            content_utf16,
        )?;
        let ordered_item = M11OrderedListItemProjectionMetadata {
            opening_marker_start,
            opening_marker_end,
            marker_value: mapping.marker_value,
        };

        let build_lease = runtime
            .snapshot_current_source()
            .map_err(|_| M11BlockQuoteProjectionJobError::SourceAuthorityMismatch)?;
        if build_lease.version() != fenced.source {
            return Err(M11BlockQuoteProjectionJobError::SourceAuthorityMismatch);
        }
        let build = M11BlockQuoteProjectionBuild::new_ordered_list(
            runtime,
            build_lease,
            block,
            item,
            projected_utf8_length,
            projected_utf16_length,
            fenced.binding.syntax_profile(),
        )?;
        Ok(Self {
            source: fenced.source,
            block_source: fenced.block_source,
            item_source: mapping.source.clone(),
            binding: fenced.binding,
            selected_item_ordinal: mapping.ordinal,
            canonical_line_ending: fenced.canonical_line_ending,
            terminal_empty,
            projection_kind: M11MarkedLineProjectionKind::OrderedList,
            ordered_item: Some(ordered_item),
            projected_utf8_length,
            projected_utf16_length,
            record,
            authority: Some(fenced.authority),
            build: Some(build),
            root: None,
            phase: ItemProjectionPhase::Offer,
            build_cancel_started: false,
            root_release_started: false,
        })
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BulletListItemProjectionJobPoll, M11BulletListProjectionJobError> {
        validate_item_fuel(fuel)?;
        if self.phase == ItemProjectionPhase::Complete {
            return Ok(M11BulletListItemProjectionJobPoll {
                status: M11BulletListItemProjectionJobPollStatus::Complete,
                transitions: 0,
            });
        }
        if matches!(
            self.phase,
            ItemProjectionPhase::Faulted
                | ItemProjectionPhase::Cancelling
                | ItemProjectionPhase::Cancelled
                | ItemProjectionPhase::Transferred
        ) {
            return Err(M11BlockQuoteProjectionJobError::InvalidState);
        }

        let mut transitions = 0;
        while transitions < fuel {
            let before = transitions;
            let phase_before = self.phase;
            let step = match self.phase {
                ItemProjectionPhase::Offer => self.offer(&mut transitions),
                ItemProjectionPhase::PollOfferedPage => {
                    self.poll_offered_page(runtime, fuel, &mut transitions)
                }
                ItemProjectionPhase::FinishInput => self.finish_input(&mut transitions),
                ItemProjectionPhase::Seal => self.poll_seal(runtime, fuel, &mut transitions),
                ItemProjectionPhase::Complete => break,
                ItemProjectionPhase::Faulted
                | ItemProjectionPhase::Cancelling
                | ItemProjectionPhase::Cancelled
                | ItemProjectionPhase::Transferred => {
                    Err(M11BlockQuoteProjectionJobError::InvalidState)
                }
            };
            if let Err(error) = step {
                self.phase = ItemProjectionPhase::Faulted;
                return Err(error);
            }
            if transitions == before && self.phase != phase_before {
                transitions = transitions
                    .checked_add(1)
                    .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
            }
            if self.phase == ItemProjectionPhase::Complete {
                return Ok(M11BulletListItemProjectionJobPoll {
                    status: M11BulletListItemProjectionJobPollStatus::Complete,
                    transitions,
                });
            }
            if transitions == before {
                break;
            }
        }
        Ok(M11BulletListItemProjectionJobPoll {
            status: M11BulletListItemProjectionJobPollStatus::Pending,
            transitions,
        })
    }

    fn offer(&mut self, transitions: &mut usize) -> Result<(), M11BulletListProjectionJobError> {
        self.build
            .as_mut()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .offer_page(&[self.record])?;
        self.phase = ItemProjectionPhase::PollOfferedPage;
        *transitions = transitions
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        Ok(())
    }

    fn poll_offered_page(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11BulletListProjectionJobError> {
        let poll = self
            .build
            .as_mut()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        match poll.status() {
            M11BlockQuoteProjectionBuildStatus::NeedsPage => {
                self.phase = ItemProjectionPhase::FinishInput;
            }
            M11BlockQuoteProjectionBuildStatus::Pending => {}
            M11BlockQuoteProjectionBuildStatus::Complete
            | M11BlockQuoteProjectionBuildStatus::Cancelled => {
                return Err(M11BlockQuoteProjectionJobError::InvalidState);
            }
        }
        Ok(())
    }

    fn finish_input(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11BulletListProjectionJobError> {
        self.build
            .as_mut()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .finish_input()?;
        self.phase = ItemProjectionPhase::Seal;
        *transitions = transitions
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        Ok(())
    }

    fn poll_seal(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11BulletListProjectionJobError> {
        let poll = self
            .build
            .as_mut()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        match poll.status() {
            M11BlockQuoteProjectionBuildStatus::Pending => {}
            M11BlockQuoteProjectionBuildStatus::Complete => {
                let root = self
                    .build
                    .as_mut()
                    .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
                    .take_root()
                    .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?;
                self.validate_root(&root)?;
                self.root = Some(root);
                drop(self.build.take());
                drop(self.authority.take());
                self.phase = ItemProjectionPhase::Complete;
            }
            M11BlockQuoteProjectionBuildStatus::NeedsPage
            | M11BlockQuoteProjectionBuildStatus::Cancelled => {
                return Err(M11BlockQuoteProjectionJobError::InvalidState);
            }
        }
        Ok(())
    }

    fn validate_root(
        &self,
        root: &M11BlockQuoteProjectionRoot,
    ) -> Result<(), M11BulletListProjectionJobError> {
        let descriptor = root.descriptor();
        if descriptor.source() != self.source
            || descriptor.parser_profile() != self.binding.syntax_profile()
            || descriptor.projection_kind() != self.projection_kind
            || descriptor.physical_block_range() != &self.block_source
            || descriptor.requested_window() != &self.item_source
            || descriptor.projected_utf8_length() != self.projected_utf8_length
            || descriptor.projected_utf16_length() != self.projected_utf16_length
            || descriptor.line_count() != 1
            || descriptor.logical_page_count() != 1
        {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "compact persistent root differs from selected item authority",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn take_output(&mut self) -> Option<M11BulletListItemProjectionOutput> {
        if self.phase != ItemProjectionPhase::Complete {
            return None;
        }
        let root = self.root.take()?;
        self.phase = ItemProjectionPhase::Transferred;
        Some(M11BulletListItemProjectionOutput {
            root,
            selected_item_ordinal: self.selected_item_ordinal,
            canonical_line_ending: self.canonical_line_ending,
            terminal_empty: self.terminal_empty,
            ordered_item: self.ordered_item,
        })
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BulletListProjectionJobError> {
        if matches!(
            self.phase,
            ItemProjectionPhase::Transferred | ItemProjectionPhase::Cancelled
        ) {
            return Err(M11BlockQuoteProjectionJobError::InvalidState);
        }
        self.phase = ItemProjectionPhase::Cancelling;
        if let Some(build) = self.build.as_mut() {
            if !self.build_cancel_started {
                build.begin_cancel(runtime)?;
                self.build_cancel_started = true;
            }
        }
        if let Some(root) = self.root.as_mut() {
            if !self.root_release_started {
                root.begin_release(runtime)?;
                self.root_release_started = true;
            }
        }
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BulletListItemProjectionJobReleasePoll, M11BulletListProjectionJobError> {
        validate_item_fuel(fuel)?;
        if self.phase != ItemProjectionPhase::Cancelling {
            return Err(M11BlockQuoteProjectionJobError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if let Some(build) = self.build.as_mut() {
                if !self.build_cancel_started {
                    return Err(M11BlockQuoteProjectionJobError::InvalidState);
                }
                let poll = build.poll_cancel(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.receipt().transitions)
                    .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.build.take());
                    continue;
                }
                return Ok(M11BulletListItemProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(root) = self.root.as_ref() {
                if !self.root_release_started {
                    return Err(M11BlockQuoteProjectionJobError::InvalidState);
                }
                let poll = root.poll_release(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.receipt().transitions)
                    .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.root.take());
                    continue;
                }
                return Ok(M11BulletListItemProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            drop(self.authority.take());
            self.phase = ItemProjectionPhase::Cancelled;
            return Ok(M11BulletListItemProjectionJobReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11BulletListItemProjectionJobReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11BulletListItemProjectionJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    ItemProjectionPhase::Cancelled | ItemProjectionPhase::Transferred
                ),
                "list-item projection jobs require output transfer or explicit fuelled cancellation"
            );
        }
    }
}

fn checked_range_len(range: &std::ops::Range<u32>) -> Result<u32, M11BulletListProjectionJobError> {
    range
        .end
        .checked_sub(range.start)
        .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)
}

fn u32_range_as_usize(
    range: &std::ops::Range<u32>,
) -> Result<std::ops::Range<usize>, M11BulletListProjectionJobError> {
    Ok(usize::try_from(range.start)
        .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?
        ..usize::try_from(range.end)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?)
}

fn validate_item_fuel(fuel: usize) -> Result<(), M11BulletListProjectionJobError> {
    if fuel == 0 {
        return Err(M11BlockQuoteProjectionJobError::ZeroFuel);
    }
    if fuel > M11_BULLET_LIST_PROJECTION_JOB_MAX_POLL_TRANSITIONS {
        return Err(M11BlockQuoteProjectionJobError::PollLimitExceeded);
    }
    Ok(())
}
