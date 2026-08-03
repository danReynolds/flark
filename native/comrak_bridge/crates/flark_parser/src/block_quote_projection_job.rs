//! Exact, resumable derivation of one published depth-one block-quote projection.
//!
//! The retained variant-8 fence supplies the complete source envelope and
//! structural summary. This job reuses the parser's shared segmented physical
//! line scanner to recover source-owned quote prefixes; any markerless physical
//! line inside that authenticated envelope is a lazy paragraph continuation.

use std::fmt;
use std::ops::Range;

use comrak::block_spine_facade::FacadeError;
use flark_engine::parser_internal::{
    BlockQuoteLineV1, M11BlockQuoteProjectionBuild, M11BlockQuoteProjectionBuildStatus,
    M11BlockQuoteProjectionError, M11BlockQuoteProjectionRoot, M11MarkedLineProjectionKind,
    M11ParserSourceRangeAuthority, BLOCK_QUOTE_LINES_PER_PAGE_MAX, BLOCK_QUOTE_WINDOW_MAX_BYTES,
    M11_PARSER_PAGE_MAX_POLL_TRANSITIONS,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::contract::{M11LineEnding, M11PhysicalLineFacts, M11SourceLineSource};
use crate::exact_clean::{M11ParserBinding, M11_GRAMMAR_REVISION};
use crate::publication::{
    M11PublishedBlockQuoteLeafFence, M11PublishedBulletListLeafFence,
    PublishedBlockQuoteProjectionAuthority, PublishedBulletListProjectionAuthority,
};
use crate::recursive_green_block_quote_projection::M11RecursiveGreenBlockQuoteProjectionFence;
use crate::segmented_lexical::{SegmentedLineFacts, SegmentedLineScanner, SegmentedListMarker};
use crate::source_adapter::{
    SnapshotLineRetainedPoll, SnapshotLineScanner, SnapshotLineSource, SourceAdapterError,
};

/// Maximum work accepted by one projection-job poll.
pub const M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS: usize =
    M11_PARSER_PAGE_MAX_POLL_TRANSITIONS;

/// Failure while deriving or explicitly reclaiming one exact projection root.
#[derive(Debug)]
pub enum M11BlockQuoteProjectionJobError {
    SourceAuthorityMismatch,
    BlockFenceRangeMismatch,
    UnsupportedGrammarRevision { expected: u32, actual: u32 },
    WindowTooLarge { bytes: usize, cap: usize },
    StructuralSummaryMismatch(&'static str),
    CoordinateOverflow,
    AllocationFailed,
    ZeroFuel,
    PollLimitExceeded,
    InvalidState,
    LexicalDonorOverCap { bytes: usize, cap: usize },
    UnsupportedDonorHtmlBlockType(u8),
    Source(SourceAdapterError),
    Projection(M11BlockQuoteProjectionError),
}

impl fmt::Display for M11BlockQuoteProjectionJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceAuthorityMismatch => {
                formatter.write_str("block-quote projection crossed source authority")
            }
            Self::BlockFenceRangeMismatch => {
                formatter.write_str("block-quote projection range differs from its exact fence")
            }
            Self::UnsupportedGrammarRevision { expected, actual } => write!(
                formatter,
                "block-quote projection requires grammar revision {expected}, received {actual}"
            ),
            Self::WindowTooLarge { bytes, cap } => write!(
                formatter,
                "block-quote projection window has {bytes} bytes above the {cap}-byte cap"
            ),
            Self::StructuralSummaryMismatch(message) => {
                write!(
                    formatter,
                    "block-quote structural summary mismatch: {message}"
                )
            }
            Self::CoordinateOverflow => {
                formatter.write_str("block-quote projection coordinate or counter overflow")
            }
            Self::AllocationFailed => {
                formatter.write_str("block-quote projection page allocation failed")
            }
            Self::ZeroFuel => {
                formatter.write_str("block-quote projection poll requires nonzero fuel")
            }
            Self::PollLimitExceeded => {
                formatter.write_str("block-quote projection poll exceeds its transition limit")
            }
            Self::InvalidState => {
                formatter.write_str("block-quote projection job is in an invalid state")
            }
            Self::LexicalDonorOverCap { bytes, cap } => write!(
                formatter,
                "block-quote segmented lexical donor received {bytes} bytes above its {cap}-byte cap"
            ),
            Self::UnsupportedDonorHtmlBlockType(kind) => write!(
                formatter,
                "block-quote segmented lexical donor returned unsupported HTML block type {kind}"
            ),
            Self::Source(error) => write!(formatter, "block-quote source scan failed: {error}"),
            Self::Projection(error) => {
                write!(
                    formatter,
                    "block-quote projection persistence failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for M11BlockQuoteProjectionJobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Projection(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SourceAdapterError> for M11BlockQuoteProjectionJobError {
    fn from(value: SourceAdapterError) -> Self {
        Self::Source(value)
    }
}

impl From<M11BlockQuoteProjectionError> for M11BlockQuoteProjectionJobError {
    fn from(value: M11BlockQuoteProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<FacadeError> for M11BlockQuoteProjectionJobError {
    fn from(value: FacadeError) -> Self {
        match value {
            FacadeError::OverCap { bytes, cap } => Self::LexicalDonorOverCap { bytes, cap },
            FacadeError::UnsupportedHtmlBlockType(kind) => {
                Self::UnsupportedDonorHtmlBlockType(kind)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockQuoteProjectionJobPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockQuoteProjectionJobPoll {
    status: M11BlockQuoteProjectionJobPollStatus,
    transitions: usize,
}

impl M11BlockQuoteProjectionJobPoll {
    #[must_use]
    pub const fn status(self) -> M11BlockQuoteProjectionJobPollStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockQuoteProjectionJobReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11BlockQuoteProjectionJobReleasePoll {
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
enum ProjectionJobPhase {
    DiscoverLine,
    ReadLine,
    OfferPage,
    PollOfferedPage,
    FinishInput,
    Seal,
    Complete,
    Faulted,
    Cancelling,
    Cancelled,
    Transferred,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionJobShape {
    BlockQuote,
    BulletList {
        marker: u8,
        paragraph_count: u32,
        terminal_empty_relative_start: Option<u32>,
    },
}

struct PublishedMarkedLineProjectionAuthority {
    source: SourceVersion,
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    binding: M11ParserBinding,
    line_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    authority: M11ParserSourceRangeAuthority,
}

/// Move-only parser work that turns one retained variant-8 quote or variant-9
/// tight bullet-list leaf into an authenticated marked-line projection root.
///
/// The two grammar shapes share physical-line scanning and persistent page
/// lifecycle, but carry a distinct authenticated projection kind and exact
/// per-line validation.
#[must_use = "projection jobs require root transfer or explicit fuelled cancellation"]
pub struct M11BlockQuoteProjectionJob {
    source: SourceVersion,
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    binding: M11ParserBinding,
    expected_line_count: u32,
    expected_projected_utf8_length: u32,
    expected_projected_utf16_length: u32,
    shape: ProjectionJobShape,
    authority: Option<M11ParserSourceRangeAuthority>,
    scanner: Option<SnapshotLineScanner>,
    line_source: Option<SnapshotLineSource>,
    line_scanner: Option<SegmentedLineScanner>,
    page: Vec<BlockQuoteLineV1>,
    build: Option<M11BlockQuoteProjectionBuild>,
    root: Option<M11BlockQuoteProjectionRoot>,
    phase: ProjectionJobPhase,
    scan_complete: bool,
    next_absolute_byte: u32,
    observed_line_count: u32,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
    observed_physical_utf16_length: u32,
    observed_paragraph_count: u32,
    observed_terminal_empty_relative_start: Option<u32>,
    build_cancel_started: bool,
    root_release_started: bool,
}

impl fmt::Debug for M11BlockQuoteProjectionJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockQuoteProjectionJob")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("binding", &self.binding)
            .field("phase", &self.phase)
            .field("observed_line_count", &self.observed_line_count)
            .finish_non_exhaustive()
    }
}

impl M11BlockQuoteProjectionJob {
    /// Starts exact projection work from a retained published block fence.
    ///
    /// The fence is consumed so neither its source range nor parser profile can
    /// be substituted after validation.
    pub fn new(
        runtime: &DocumentRuntime,
        fence: M11PublishedBlockQuoteLeafFence,
    ) -> Result<Self, M11BlockQuoteProjectionJobError> {
        Self::from_authority(runtime, fence.into_projection_authority())
    }

    /// Starts exact projection work from independently authenticated
    /// recursive-Green container authority.
    ///
    /// Green supplies the expected single-Paragraph shape and projected
    /// metrics. The existing marked-line scanner still derives every line
    /// record from source and rejects any disagreement before publication.
    pub fn new_for_recursive_green(
        runtime: &DocumentRuntime,
        fence: M11RecursiveGreenBlockQuoteProjectionFence,
        binding: M11ParserBinding,
    ) -> Result<Self, M11BlockQuoteProjectionJobError> {
        let source = fence.source();
        let block_source_u64 = fence.block_source_range();
        let block_source_utf16_u64 = fence.block_source_utf16_range();
        let line_count = u32::try_from(fence.line_count())
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let projected_utf8_length = u32::try_from(fence.projected_utf8_length())
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let projected_utf16_length = u32::try_from(fence.projected_utf16_length())
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let (authority, authority_range) = fence.into_source_authority();
        if authority_range != block_source_u64 {
            return Err(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch);
        }
        let block_source = u32::try_from(block_source_u64.start)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?
            ..u32::try_from(block_source_u64.end)
                .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let block_source_utf16 = u32::try_from(block_source_utf16_u64.start)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?
            ..u32::try_from(block_source_utf16_u64.end)
                .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        Self::from_marked_line_authority(
            runtime,
            PublishedMarkedLineProjectionAuthority {
                source,
                block_source,
                block_source_utf16,
                binding,
                line_count,
                projected_utf8_length,
                projected_utf16_length,
                authority,
            },
            ProjectionJobShape::BlockQuote,
        )
    }

    fn from_authority(
        runtime: &DocumentRuntime,
        fenced: PublishedBlockQuoteProjectionAuthority,
    ) -> Result<Self, M11BlockQuoteProjectionJobError> {
        Self::from_marked_line_authority(
            runtime,
            PublishedMarkedLineProjectionAuthority {
                source: fenced.source,
                block_source: fenced.block_source,
                block_source_utf16: fenced.block_source_utf16,
                binding: fenced.binding,
                line_count: fenced.line_count,
                projected_utf8_length: fenced.projected_utf8_length,
                projected_utf16_length: fenced.projected_utf16_length,
                authority: fenced.authority,
            },
            ProjectionJobShape::BlockQuote,
        )
    }

    pub(crate) fn new_bullet_list(
        runtime: &DocumentRuntime,
        fence: M11PublishedBulletListLeafFence,
    ) -> Result<Self, M11BlockQuoteProjectionJobError> {
        let fenced: PublishedBulletListProjectionAuthority = fence.into_projection_authority();
        Self::from_marked_line_authority(
            runtime,
            PublishedMarkedLineProjectionAuthority {
                source: fenced.source,
                block_source: fenced.block_source,
                block_source_utf16: fenced.block_source_utf16,
                binding: fenced.binding,
                line_count: fenced.item_count,
                projected_utf8_length: fenced.projected_utf8_length,
                projected_utf16_length: fenced.projected_utf16_length,
                authority: fenced.authority,
            },
            ProjectionJobShape::BulletList {
                marker: fenced.marker,
                paragraph_count: fenced.paragraph_count,
                terminal_empty_relative_start: fenced.terminal_empty_relative_start,
            },
        )
    }

    fn from_marked_line_authority(
        runtime: &DocumentRuntime,
        fenced: PublishedMarkedLineProjectionAuthority,
        shape: ProjectionJobShape,
    ) -> Result<Self, M11BlockQuoteProjectionJobError> {
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
        let block_start = usize::try_from(fenced.block_source.start)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let block_end = usize::try_from(fenced.block_source.end)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let block = block_start..block_end;
        if fenced.authority.source() != fenced.source || fenced.authority.source_range() != block {
            return Err(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch);
        }
        let bytes = block
            .end
            .checked_sub(block.start)
            .ok_or(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch)?;
        if bytes == 0 {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "variant-8 block is empty",
            ));
        }
        if bytes > BLOCK_QUOTE_WINDOW_MAX_BYTES {
            return Err(M11BlockQuoteProjectionJobError::WindowTooLarge {
                bytes,
                cap: BLOCK_QUOTE_WINDOW_MAX_BYTES,
            });
        }
        let block_utf16_length = fenced
            .block_source_utf16
            .end
            .checked_sub(fenced.block_source_utf16.start)
            .ok_or(M11BlockQuoteProjectionJobError::BlockFenceRangeMismatch)?;
        let summary_is_valid = match shape {
            ProjectionJobShape::BlockQuote => {
                fenced.line_count != 0
                    && fenced.projected_utf8_length != 0
                    && fenced.projected_utf16_length != 0
            }
            ProjectionJobShape::BulletList {
                marker,
                paragraph_count,
                terminal_empty_relative_start,
            } => {
                let paragraph_shape = terminal_empty_relative_start.map_or(
                    paragraph_count == fenced.line_count,
                    |terminal_start| {
                        paragraph_count.checked_add(1) == Some(fenced.line_count)
                            && terminal_start < u32::try_from(bytes).unwrap_or(u32::MAX)
                    },
                );
                matches!(marker, b'-' | b'+' | b'*')
                    && fenced.line_count != 0
                    && paragraph_shape
                    && (fenced.projected_utf8_length == 0) == (fenced.projected_utf16_length == 0)
                    && (fenced.projected_utf8_length != 0
                        || paragraph_count == 0 && terminal_empty_relative_start.is_some())
            }
        };
        if block_utf16_length == 0 || !summary_is_valid {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "published marked-line summary is empty or internally inconsistent",
            ));
        }

        let mut page = Vec::new();
        page.try_reserve_exact(BLOCK_QUOTE_LINES_PER_PAGE_MAX)
            .map_err(|_| M11BlockQuoteProjectionJobError::AllocationFailed)?;

        let scan_lease = runtime
            .snapshot_current_source()
            .map_err(|_| M11BlockQuoteProjectionJobError::SourceAuthorityMismatch)?;
        if scan_lease.version() != fenced.source {
            return Err(M11BlockQuoteProjectionJobError::SourceAuthorityMismatch);
        }
        let scanner = SnapshotLineScanner::new_in(scan_lease, block.clone(), 0)?;

        let build_lease = runtime
            .snapshot_current_source()
            .map_err(|_| M11BlockQuoteProjectionJobError::SourceAuthorityMismatch)?;
        if build_lease.version() != fenced.source {
            return Err(M11BlockQuoteProjectionJobError::SourceAuthorityMismatch);
        }
        let build = match shape {
            ProjectionJobShape::BlockQuote => M11BlockQuoteProjectionBuild::new(
                runtime,
                build_lease,
                block.clone(),
                block,
                fenced.projected_utf8_length,
                fenced.projected_utf16_length,
                fenced.binding.syntax_profile(),
            )?,
            ProjectionJobShape::BulletList { .. } => M11BlockQuoteProjectionBuild::new_bullet_list(
                runtime,
                build_lease,
                block.clone(),
                block,
                fenced.projected_utf8_length,
                fenced.projected_utf16_length,
                fenced.binding.syntax_profile(),
            )?,
        };

        Ok(Self {
            source: fenced.source,
            block_source: fenced.block_source.clone(),
            block_source_utf16: fenced.block_source_utf16,
            binding: fenced.binding,
            expected_line_count: fenced.line_count,
            expected_projected_utf8_length: fenced.projected_utf8_length,
            expected_projected_utf16_length: fenced.projected_utf16_length,
            shape,
            authority: Some(fenced.authority),
            scanner: Some(scanner),
            line_source: None,
            line_scanner: None,
            page,
            build: Some(build),
            root: None,
            phase: ProjectionJobPhase::DiscoverLine,
            scan_complete: false,
            next_absolute_byte: fenced.block_source.start,
            observed_line_count: 0,
            observed_projected_utf8_length: 0,
            observed_projected_utf16_length: 0,
            observed_physical_utf16_length: 0,
            observed_paragraph_count: 0,
            observed_terminal_empty_relative_start: None,
            build_cancel_started: false,
            root_release_started: false,
        })
    }

    /// Advances source discovery, exact line scanning, and persistent build by
    /// at most `fuel` transitions.
    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockQuoteProjectionJobPoll, M11BlockQuoteProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase == ProjectionJobPhase::Complete {
            return Ok(M11BlockQuoteProjectionJobPoll {
                status: M11BlockQuoteProjectionJobPollStatus::Complete,
                transitions: 0,
            });
        }
        if matches!(
            self.phase,
            ProjectionJobPhase::Faulted
                | ProjectionJobPhase::Cancelling
                | ProjectionJobPhase::Cancelled
                | ProjectionJobPhase::Transferred
        ) {
            return Err(M11BlockQuoteProjectionJobError::InvalidState);
        }

        let mut transitions = 0;
        while transitions < fuel {
            let before = transitions;
            let phase_before = self.phase;
            let step = match self.phase {
                ProjectionJobPhase::DiscoverLine => {
                    self.poll_line_discovery(fuel, &mut transitions)
                }
                ProjectionJobPhase::ReadLine => self.poll_line_read(fuel, &mut transitions),
                ProjectionJobPhase::OfferPage => self.offer_page(&mut transitions),
                ProjectionJobPhase::PollOfferedPage => {
                    self.poll_offered_page(runtime, fuel, &mut transitions)
                }
                ProjectionJobPhase::FinishInput => self.finish_input(&mut transitions),
                ProjectionJobPhase::Seal => self.poll_seal(runtime, fuel, &mut transitions),
                ProjectionJobPhase::Complete => break,
                ProjectionJobPhase::Faulted
                | ProjectionJobPhase::Cancelling
                | ProjectionJobPhase::Cancelled
                | ProjectionJobPhase::Transferred => {
                    Err(M11BlockQuoteProjectionJobError::InvalidState)
                }
            };
            if let Err(error) = step {
                self.phase = ProjectionJobPhase::Faulted;
                return Err(error);
            }
            if transitions == before && self.phase != phase_before {
                transitions = transitions
                    .checked_add(1)
                    .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
            }
            if self.phase == ProjectionJobPhase::Complete {
                return Ok(M11BlockQuoteProjectionJobPoll {
                    status: M11BlockQuoteProjectionJobPollStatus::Complete,
                    transitions,
                });
            }
            if transitions == before {
                break;
            }
        }
        Ok(M11BlockQuoteProjectionJobPoll {
            status: M11BlockQuoteProjectionJobPollStatus::Pending,
            transitions,
        })
    }

    fn poll_line_discovery(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
        let scanner = self
            .scanner
            .take()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?;
        let (poll, inspected) = scanner.poll_counted_retaining_complete(fuel - *transitions)?;
        *transitions = transitions
            .checked_add(inspected)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        match poll {
            SnapshotLineRetainedPoll::Pending(scanner) => {
                self.scanner = Some(scanner);
            }
            SnapshotLineRetainedPoll::Line(line) => {
                let facts = line.facts();
                if facts.identity().source() != self.source
                    || facts.identity().start_byte() != self.next_absolute_byte
                    || facts.identity().end_byte() > self.block_source.end
                {
                    return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                        "physical line discovery crossed fenced coverage",
                    ));
                }
                self.line_scanner = Some(SegmentedLineScanner::new(
                    facts.identity().start_byte() == 0,
                ));
                self.line_source = Some(line.into_source()?);
                self.phase = ProjectionJobPhase::ReadLine;
            }
            SnapshotLineRetainedPoll::Complete(scanner) => {
                drop(scanner.into_source_lease());
                return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                    "physical line discovery ended before fenced coverage",
                ));
            }
        }
        Ok(())
    }

    fn poll_line_read(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
        while *transitions < fuel {
            let complete = self
                .line_source
                .as_ref()
                .is_some_and(|source| source.position() == source.len());
            if complete {
                self.finish_line()?;
                return Ok(());
            }
            let remaining_fuel = fuel - *transitions;
            let source = self
                .line_source
                .as_mut()
                .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?;
            if source.access_budget() == 0 {
                let _ = source.replenish_access_budget(remaining_fuel)?;
            }
            let offset = source.position();
            let byte = source.read_byte(offset)?;
            self.line_scanner
                .as_mut()
                .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
                .push(byte);
            *transitions = transitions
                .checked_add(1)
                .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        }
        Ok(())
    }

    fn finish_line(&mut self) -> Result<(), M11BlockQuoteProjectionJobError> {
        let source = self
            .line_source
            .take()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?;
        let physical = source.facts();
        let scanner = source.finish()?;
        let segmented = self
            .line_scanner
            .take()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .finish()?;
        let record = self.record_line(physical, segmented)?;
        self.page.push(record);
        self.next_absolute_byte = physical.identity().end_byte();

        if self.next_absolute_byte == self.block_source.end {
            drop(scanner.into_source_lease());
            self.scan_complete = true;
            self.phase = ProjectionJobPhase::OfferPage;
        } else if self.next_absolute_byte > self.block_source.end {
            drop(scanner.into_source_lease());
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "physical line exceeded fenced coverage",
            ));
        } else {
            self.scanner = Some(scanner);
            self.phase = if self.page.len() == BLOCK_QUOTE_LINES_PER_PAGE_MAX {
                ProjectionJobPhase::OfferPage
            } else {
                ProjectionJobPhase::DiscoverLine
            };
        }
        Ok(())
    }

    fn record_line(
        &mut self,
        physical: M11PhysicalLineFacts,
        segmented: SegmentedLineFacts,
    ) -> Result<BlockQuoteLineV1, M11BlockQuoteProjectionJobError> {
        if let ProjectionJobShape::BulletList { marker, .. } = self.shape {
            return self.record_bullet_list_item(physical, segmented, marker);
        }
        let identity = physical.identity();
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let expected_eol = ending_bytes(physical.ending());
        let observed_eol = physical_bytes
            .checked_sub(
                usize::try_from(physical.content_bytes())
                    .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
            )
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        if observed_eol != expected_eol
            || segmented.had_ending != !matches!(physical.ending(), M11LineEnding::Eof)
            || segmented.blank
        {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "physical line is not an exact nonblank quote-paragraph line",
            ));
        }

        let (hidden_prefix_length, content_length, marked) =
            if let Some(quote) = segmented.block_quote_source {
                if !segmented.block_quote
                    || quote.hidden_prefix.start != 0
                    || quote.hidden_prefix.end != quote.content.start
                    || quote.opening_marker.start >= quote.opening_marker.end
                    || quote.opening_marker.end - quote.opening_marker.start != 1
                    || quote.opening_marker.start < quote.hidden_prefix.start
                    || quote.opening_marker.end > quote.hidden_prefix.end
                    || quote.content.end != quote.line_ending.start
                    || quote.line_ending.start
                        != usize::try_from(physical.content_bytes())
                            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?
                    || quote.line_ending.end != physical_bytes
                    || quote.residual_tab_columns != 0
                {
                    return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                        "marked quote spans do not partition physical source",
                    ));
                }
                (
                    u32::try_from(quote.hidden_prefix.end)
                        .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
                    u32::try_from(
                        quote
                            .content
                            .end
                            .checked_sub(quote.content.start)
                            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
                    )
                    .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
                    true,
                )
            } else {
                if segmented.block_quote || self.observed_line_count == 0 {
                    return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                        "first quote line is markerless or quote facts disagree",
                    ));
                }
                (0, physical.content_bytes(), false)
            };
        if content_length == 0 {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "quote paragraph line has no source-backed content",
            ));
        }

        let hidden_utf16 = hidden_prefix_length
            .checked_sub(if segmented.has_bof_bom { 2 } else { 0 })
            .ok_or(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "BOF BOM is outside the hidden quote prefix",
            ))?;
        if self.observed_line_count != 0 && segmented.has_bof_bom {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "BOF BOM appeared after the first fenced line",
            ));
        }
        let projected_utf8 = physical
            .physical_bytes()
            .checked_sub(hidden_prefix_length)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let projected_utf16 = physical
            .physical_utf16()
            .checked_sub(hidden_utf16)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;

        self.observed_line_count = self
            .observed_line_count
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_projected_utf8_length = self
            .observed_projected_utf8_length
            .checked_add(projected_utf8)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_projected_utf16_length = self
            .observed_projected_utf16_length
            .checked_add(projected_utf16)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_physical_utf16_length = self
            .observed_physical_utf16_length
            .checked_add(physical.physical_utf16())
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;

        let relative_start = identity
            .start_byte()
            .checked_sub(self.block_source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let physical_length = physical.physical_bytes();
        if marked {
            Ok(BlockQuoteLineV1::marked(
                relative_start,
                physical_length,
                hidden_prefix_length,
                content_length,
            )?)
        } else {
            Ok(BlockQuoteLineV1::lazy(
                relative_start,
                physical_length,
                content_length,
            )?)
        }
    }

    fn record_bullet_list_item(
        &mut self,
        physical: M11PhysicalLineFacts,
        segmented: SegmentedLineFacts,
        expected_marker: u8,
    ) -> Result<BlockQuoteLineV1, M11BlockQuoteProjectionJobError> {
        let identity = physical.identity();
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let content_bytes = usize::try_from(physical.content_bytes())
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let expected_eol = ending_bytes(physical.ending());
        let observed_eol = physical_bytes
            .checked_sub(content_bytes)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let item = segmented.list_item.ok_or(
            M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "variant-9 physical line has no parser-donor list item",
            ),
        )?;
        let SegmentedListMarker::Bullet(marker) = item.marker else {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "variant-9 physical line has an ordered marker",
            ));
        };
        let valid_span = |span: crate::segmented_lexical::SegmentedLineSpan| {
            span.start <= span.end && span.end <= physical_bytes
        };
        let bom_bytes = usize::from(segmented.has_bof_bom) * 3;
        if observed_eol != expected_eol
            || segmented.had_ending != !matches!(physical.ending(), M11LineEnding::Eof)
            || segmented.blank
            || !segmented.list
            || marker != expected_marker
            || item.opening_indent > 3
            || item.tab_padded
            || !item.empty && !(1..=4).contains(&item.padding_columns)
            || item.child.task
            || item.child.block_quote
            || item.child.atx_heading
            || item.child.fence
            || item.child.html_block_1_to_6
            || item.child.html_block_7
            || item.child.setext
            || item.child.thematic_break
            || item.child.list
            || item.child.table_delimiter_candidate
            || item.child.potential_reference_definition
            || segmented.has_bof_bom
                && (self.observed_line_count != 0 || identity.start_byte() != 0)
            || !valid_span(item.hidden_prefix)
            || !valid_span(item.continuation_prefix)
            || !valid_span(item.opening_marker)
            || !valid_span(item.content)
            || !valid_span(item.line_ending)
            || item.hidden_prefix.start != 0
            || item.continuation_prefix.start != 0
            || item.opening_marker.start != bom_bytes + item.opening_indent
            || item.opening_marker.end - item.opening_marker.start != 1
            || item.opening_marker.end > item.continuation_prefix.end
            || item.continuation_prefix.end > item.hidden_prefix.end
            || item.hidden_prefix.end != item.content.start
            || item.content.end != item.line_ending.start
            || item.line_ending.start != content_bytes
            || item.line_ending.end != physical_bytes
            || item.empty != (item.content.start == item.content.end)
        {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "variant-9 item spans do not partition exact physical source",
            ));
        }

        let hidden_prefix_length = u32::try_from(item.hidden_prefix.end)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        // The segmented donor's line-relative span begins at zero even when
        // that line owns the stripped BOF BOM. The exact item authority
        // deliberately excludes those three immutable BOM bytes from the
        // removable/continuable prefix.
        let continuation_prefix_start = u32::try_from(bom_bytes)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let continuation_prefix_end = u32::try_from(item.continuation_prefix.end)
            .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let content_length = u32::try_from(
            item.content
                .end
                .checked_sub(item.content.start)
                .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
        )
        .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let hidden_utf16 = hidden_prefix_length
            .checked_sub(if segmented.has_bof_bom { 2 } else { 0 })
            .ok_or(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "BOF BOM is outside the hidden list-item prefix",
            ))?;
        let content_utf16_length = physical.content_utf16().checked_sub(hidden_utf16).ok_or(
            M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "list-item hidden prefix exceeds physical UTF-16 content",
            ),
        )?;
        if (content_length == 0) != (content_utf16_length == 0) {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "list-item UTF-8 and UTF-16 content emptiness disagree",
            ));
        }

        let relative_start = identity
            .start_byte()
            .checked_sub(self.block_source.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        if item.empty {
            if self.observed_terminal_empty_relative_start.is_some() {
                return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                    "variant-9 list has more than one empty item",
                ));
            }
            self.observed_terminal_empty_relative_start = Some(relative_start);
        } else {
            self.observed_paragraph_count = self
                .observed_paragraph_count
                .checked_add(1)
                .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        }

        let projected_utf8 = content_length
            .checked_add(
                u32::try_from(expected_eol)
                    .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
            )
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let projected_utf16 = content_utf16_length
            .checked_add(
                u32::try_from(expected_eol)
                    .map_err(|_| M11BlockQuoteProjectionJobError::CoordinateOverflow)?,
            )
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_line_count = self
            .observed_line_count
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_projected_utf8_length = self
            .observed_projected_utf8_length
            .checked_add(projected_utf8)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_projected_utf16_length = self
            .observed_projected_utf16_length
            .checked_add(projected_utf16)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        self.observed_physical_utf16_length = self
            .observed_physical_utf16_length
            .checked_add(physical.physical_utf16())
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;

        Ok(BlockQuoteLineV1::bullet_item(
            relative_start,
            physical.physical_bytes(),
            hidden_prefix_length,
            continuation_prefix_start,
            continuation_prefix_end,
            content_length,
            content_utf16_length,
        )?)
    }

    fn offer_page(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
        if self.page.is_empty() {
            return Err(M11BlockQuoteProjectionJobError::InvalidState);
        }
        self.build
            .as_mut()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .offer_page(&self.page)?;
        self.page.clear();
        self.phase = ProjectionJobPhase::PollOfferedPage;
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
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
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
                self.phase = if self.scan_complete {
                    ProjectionJobPhase::FinishInput
                } else {
                    ProjectionJobPhase::DiscoverLine
                };
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
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
        self.validate_observed_summary()?;
        self.build
            .as_mut()
            .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?
            .finish_input()?;
        self.phase = ProjectionJobPhase::Seal;
        *transitions = transitions
            .checked_add(1)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        Ok(())
    }

    fn validate_observed_summary(&self) -> Result<(), M11BlockQuoteProjectionJobError> {
        let expected_physical_utf16 = self
            .block_source_utf16
            .end
            .checked_sub(self.block_source_utf16.start)
            .ok_or(M11BlockQuoteProjectionJobError::CoordinateOverflow)?;
        let mismatch = if !self.scan_complete || self.next_absolute_byte != self.block_source.end {
            Some("line coverage does not end at the fenced block boundary")
        } else if self.observed_line_count != self.expected_line_count {
            Some("line count differs from the published variant-8 summary")
        } else if self.observed_projected_utf8_length != self.expected_projected_utf8_length {
            Some("projected UTF-8 length differs from the published variant-8 summary")
        } else if self.observed_projected_utf16_length != self.expected_projected_utf16_length {
            Some("projected UTF-16 length differs from the published variant-8 summary")
        } else if self.observed_physical_utf16_length != expected_physical_utf16 {
            Some("physical UTF-16 length differs from the published block authority")
        } else if let ProjectionJobShape::BulletList {
            paragraph_count,
            terminal_empty_relative_start,
            ..
        } = self.shape
        {
            if self.observed_paragraph_count != paragraph_count {
                Some("paragraph count differs from the published variant-9 summary")
            } else if self.observed_terminal_empty_relative_start != terminal_empty_relative_start {
                Some("terminal-empty cut differs from the published variant-9 summary")
            } else {
                None
            }
        } else {
            None
        };
        mismatch.map_or(Ok(()), |message| {
            Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                message,
            ))
        })
    }

    fn poll_seal(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
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
                self.root = Some(root);
                drop(self.build.take());
                self.validate_root(
                    self.root
                        .as_ref()
                        .ok_or(M11BlockQuoteProjectionJobError::InvalidState)?,
                )?;
                drop(self.authority.take());
                self.phase = ProjectionJobPhase::Complete;
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
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
        let descriptor = root.descriptor();
        let expected_kind = match self.shape {
            ProjectionJobShape::BlockQuote => M11MarkedLineProjectionKind::BlockQuote,
            ProjectionJobShape::BulletList { .. } => M11MarkedLineProjectionKind::BulletList,
        };
        if descriptor.source() != self.source
            || descriptor.parser_profile() != self.binding.syntax_profile()
            || descriptor.projection_kind() != expected_kind
            || descriptor.physical_block_range() != &self.block_source
            || descriptor.requested_window() != &self.block_source
            || descriptor.projected_utf8_length() != self.expected_projected_utf8_length
            || descriptor.projected_utf16_length() != self.expected_projected_utf16_length
            || descriptor.line_count() != u64::from(self.expected_line_count)
        {
            return Err(M11BlockQuoteProjectionJobError::StructuralSummaryMismatch(
                "persistent root descriptor differs from exact block authority",
            ));
        }
        Ok(())
    }

    /// Transfers the ready persistent root to its caller.
    #[must_use]
    pub fn take_root(&mut self) -> Option<M11BlockQuoteProjectionRoot> {
        if self.phase != ProjectionJobPhase::Complete {
            return None;
        }
        let root = self.root.take()?;
        self.phase = ProjectionJobPhase::Transferred;
        Some(root)
    }

    /// Begins explicit cleanup of any source scan or persistent build/root.
    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockQuoteProjectionJobError> {
        if matches!(
            self.phase,
            ProjectionJobPhase::Transferred | ProjectionJobPhase::Cancelled
        ) {
            return Err(M11BlockQuoteProjectionJobError::InvalidState);
        }
        self.phase = ProjectionJobPhase::Cancelling;
        if let Some(source) = self.line_source.take() {
            let (_, scanner) = source.cancel();
            drop(scanner.into_source_lease());
        }
        if let Some(scanner) = self.scanner.take() {
            drop(scanner.into_source_lease());
        }
        let _ = self.line_scanner.take();
        self.page.clear();
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

    /// Advances explicit cancellation/release by at most `fuel` transitions.
    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockQuoteProjectionJobReleasePoll, M11BlockQuoteProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase != ProjectionJobPhase::Cancelling {
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
                return Ok(M11BlockQuoteProjectionJobReleasePoll {
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
                return Ok(M11BlockQuoteProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            drop(self.authority.take());
            self.phase = ProjectionJobPhase::Cancelled;
            return Ok(M11BlockQuoteProjectionJobReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11BlockQuoteProjectionJobReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11BlockQuoteProjectionJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    ProjectionJobPhase::Cancelled | ProjectionJobPhase::Transferred
                ),
                "block-quote projection jobs require root transfer or explicit fuelled cancellation"
            );
        }
    }
}

fn ending_bytes(ending: M11LineEnding) -> usize {
    match ending {
        M11LineEnding::Lf | M11LineEnding::Cr => 1,
        M11LineEnding::CrLf => 2,
        M11LineEnding::Eof => 0,
    }
}

fn validate_fuel(fuel: usize) -> Result<(), M11BlockQuoteProjectionJobError> {
    if fuel == 0 {
        return Err(M11BlockQuoteProjectionJobError::ZeroFuel);
    }
    if fuel > M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS {
        return Err(M11BlockQuoteProjectionJobError::PollLimitExceeded);
    }
    Ok(())
}
