//! Exact, resumable derivation of one published indented-code projection.
//!
//! The job deliberately reuses the parser's shared segmented physical-line
//! scanner. It does not recognize indentation independently: the retained
//! variant-7 block fence supplies structural authority, while
//! [`SegmentedLineScanner`] supplies the exact per-line deindent spans used by
//! the clean parser.

use std::fmt;
use std::ops::Range;

use comrak::block_spine_facade::FacadeError;
use flark_engine::parser_internal::{
    IndentedCodeLineV1, M11IndentedCodeProjectionBuild, M11IndentedCodeProjectionBuildStatus,
    M11IndentedCodeProjectionError, M11IndentedCodeProjectionRoot, M11ParserSourceRangeAuthority,
    INDENTED_CODE_LINES_PER_PAGE_MAX, INDENTED_CODE_WINDOW_MAX_BYTES,
    M11_PARSER_PAGE_MAX_POLL_TRANSITIONS,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::contract::{M11LineEnding, M11PhysicalLineFacts, M11SourceLineSource};
use crate::exact_clean::{M11ParserBinding, M11_GRAMMAR_REVISION};
use crate::publication::{
    M11PublishedIndentedCodeLeafFence, PublishedIndentedCodeProjectionAuthority,
};
use crate::segmented_lexical::{SegmentedLineFacts, SegmentedLineScanner};
use crate::source_adapter::{
    SnapshotLineRetainedPoll, SnapshotLineScanner, SnapshotLineSource, SourceAdapterError,
};

/// Maximum work accepted by one projection-job poll.
pub const M11_INDENTED_CODE_PROJECTION_JOB_MAX_POLL_TRANSITIONS: usize =
    M11_PARSER_PAGE_MAX_POLL_TRANSITIONS;

/// Failure while deriving or explicitly reclaiming one exact projection root.
#[derive(Debug)]
pub enum M11IndentedCodeProjectionJobError {
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
    Projection(M11IndentedCodeProjectionError),
}

impl fmt::Display for M11IndentedCodeProjectionJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceAuthorityMismatch => {
                formatter.write_str("indented-code projection crossed source authority")
            }
            Self::BlockFenceRangeMismatch => {
                formatter.write_str("indented-code projection range differs from its exact fence")
            }
            Self::UnsupportedGrammarRevision { expected, actual } => write!(
                formatter,
                "indented-code projection requires grammar revision {expected}, received {actual}"
            ),
            Self::WindowTooLarge { bytes, cap } => write!(
                formatter,
                "indented-code projection window has {bytes} bytes above the {cap}-byte cap"
            ),
            Self::StructuralSummaryMismatch(message) => {
                write!(
                    formatter,
                    "indented-code structural summary mismatch: {message}"
                )
            }
            Self::CoordinateOverflow => {
                formatter.write_str("indented-code projection coordinate or counter overflow")
            }
            Self::AllocationFailed => {
                formatter.write_str("indented-code projection page allocation failed")
            }
            Self::ZeroFuel => {
                formatter.write_str("indented-code projection poll requires nonzero fuel")
            }
            Self::PollLimitExceeded => {
                formatter.write_str("indented-code projection poll exceeds its transition limit")
            }
            Self::InvalidState => {
                formatter.write_str("indented-code projection job is in an invalid state")
            }
            Self::LexicalDonorOverCap { bytes, cap } => write!(
                formatter,
                "indented-code segmented lexical donor received {bytes} bytes above its {cap}-byte cap"
            ),
            Self::UnsupportedDonorHtmlBlockType(kind) => write!(
                formatter,
                "indented-code segmented lexical donor returned unsupported HTML block type {kind}"
            ),
            Self::Source(error) => write!(formatter, "indented-code source scan failed: {error}"),
            Self::Projection(error) => {
                write!(
                    formatter,
                    "indented-code projection persistence failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for M11IndentedCodeProjectionJobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Projection(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SourceAdapterError> for M11IndentedCodeProjectionJobError {
    fn from(value: SourceAdapterError) -> Self {
        Self::Source(value)
    }
}

impl From<M11IndentedCodeProjectionError> for M11IndentedCodeProjectionJobError {
    fn from(value: M11IndentedCodeProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<FacadeError> for M11IndentedCodeProjectionJobError {
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
pub enum M11IndentedCodeProjectionJobPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11IndentedCodeProjectionJobPoll {
    status: M11IndentedCodeProjectionJobPollStatus,
    transitions: usize,
}

impl M11IndentedCodeProjectionJobPoll {
    #[must_use]
    pub const fn status(self) -> M11IndentedCodeProjectionJobPollStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11IndentedCodeProjectionJobReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11IndentedCodeProjectionJobReleasePoll {
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

/// Move-only parser work that turns one retained variant-7 leaf into a typed
/// persistent projection root.
#[must_use = "projection jobs require root transfer or explicit fuelled cancellation"]
pub struct M11IndentedCodeProjectionJob {
    source: SourceVersion,
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    binding: M11ParserBinding,
    expected_line_count: u32,
    expected_projected_utf8_length: u32,
    expected_projected_utf16_length: u32,
    expected_terminal_eol_bytes: u32,
    expected_bof_bom: bool,
    authority: Option<M11ParserSourceRangeAuthority>,
    scanner: Option<SnapshotLineScanner>,
    line_source: Option<SnapshotLineSource>,
    line_scanner: Option<SegmentedLineScanner>,
    page: Vec<IndentedCodeLineV1>,
    build: Option<M11IndentedCodeProjectionBuild>,
    root: Option<M11IndentedCodeProjectionRoot>,
    phase: ProjectionJobPhase,
    scan_complete: bool,
    next_absolute_byte: u32,
    observed_line_count: u32,
    observed_projected_utf8_length: u32,
    observed_projected_utf16_length: u32,
    observed_physical_utf16_length: u32,
    observed_terminal_eol_bytes: u32,
    observed_bof_bom: Option<bool>,
    first_line_blank: Option<bool>,
    last_line_blank: Option<bool>,
    build_cancel_started: bool,
    root_release_started: bool,
}

impl fmt::Debug for M11IndentedCodeProjectionJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11IndentedCodeProjectionJob")
            .field("source", &self.source)
            .field("block_source", &self.block_source)
            .field("binding", &self.binding)
            .field("phase", &self.phase)
            .field("observed_line_count", &self.observed_line_count)
            .finish_non_exhaustive()
    }
}

impl M11IndentedCodeProjectionJob {
    /// Starts exact projection work from a retained published block fence.
    ///
    /// The fence is consumed so neither its source range nor parser profile can
    /// be substituted after validation.
    pub fn new(
        runtime: &DocumentRuntime,
        fence: M11PublishedIndentedCodeLeafFence,
    ) -> Result<Self, M11IndentedCodeProjectionJobError> {
        Self::from_authority(runtime, fence.into_projection_authority())
    }

    fn from_authority(
        runtime: &DocumentRuntime,
        fenced: PublishedIndentedCodeProjectionAuthority,
    ) -> Result<Self, M11IndentedCodeProjectionJobError> {
        if fenced.binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(
                M11IndentedCodeProjectionJobError::UnsupportedGrammarRevision {
                    expected: M11_GRAMMAR_REVISION,
                    actual: fenced.binding.grammar_revision(),
                },
            );
        }
        fenced
            .authority
            .validate(runtime)
            .map_err(|_| M11IndentedCodeProjectionJobError::SourceAuthorityMismatch)?;
        let block_start = usize::try_from(fenced.block_source.start)
            .map_err(|_| M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let block_end = usize::try_from(fenced.block_source.end)
            .map_err(|_| M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let block = block_start..block_end;
        if fenced.authority.source() != fenced.source || fenced.authority.source_range() != block {
            return Err(M11IndentedCodeProjectionJobError::BlockFenceRangeMismatch);
        }
        let bytes = block
            .end
            .checked_sub(block.start)
            .ok_or(M11IndentedCodeProjectionJobError::BlockFenceRangeMismatch)?;
        if bytes == 0 {
            return Err(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "variant-7 block is empty",
                ),
            );
        }
        if bytes > INDENTED_CODE_WINDOW_MAX_BYTES {
            return Err(M11IndentedCodeProjectionJobError::WindowTooLarge {
                bytes,
                cap: INDENTED_CODE_WINDOW_MAX_BYTES,
            });
        }
        let _ = fenced
            .block_source_utf16
            .end
            .checked_sub(fenced.block_source_utf16.start)
            .ok_or(M11IndentedCodeProjectionJobError::BlockFenceRangeMismatch)?;

        let mut page = Vec::new();
        page.try_reserve_exact(INDENTED_CODE_LINES_PER_PAGE_MAX)
            .map_err(|_| M11IndentedCodeProjectionJobError::AllocationFailed)?;

        let scan_lease = runtime
            .snapshot_current_source()
            .map_err(|_| M11IndentedCodeProjectionJobError::SourceAuthorityMismatch)?;
        if scan_lease.version() != fenced.source {
            return Err(M11IndentedCodeProjectionJobError::SourceAuthorityMismatch);
        }
        let scanner = SnapshotLineScanner::new_in(scan_lease, block.clone(), 0)?;

        let build_lease = runtime
            .snapshot_current_source()
            .map_err(|_| M11IndentedCodeProjectionJobError::SourceAuthorityMismatch)?;
        if build_lease.version() != fenced.source {
            return Err(M11IndentedCodeProjectionJobError::SourceAuthorityMismatch);
        }
        let build = M11IndentedCodeProjectionBuild::new(
            runtime,
            build_lease,
            block.clone(),
            block,
            fenced.binding.syntax_profile(),
        )?;

        Ok(Self {
            source: fenced.source,
            block_source: fenced.block_source.clone(),
            block_source_utf16: fenced.block_source_utf16,
            binding: fenced.binding,
            expected_line_count: fenced.line_count,
            expected_projected_utf8_length: fenced.projected_utf8_length,
            expected_projected_utf16_length: fenced.projected_utf16_length,
            expected_terminal_eol_bytes: fenced.terminal_eol_bytes,
            expected_bof_bom: fenced.has_bof_bom,
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
            observed_terminal_eol_bytes: 0,
            observed_bof_bom: None,
            first_line_blank: None,
            last_line_blank: None,
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
    ) -> Result<M11IndentedCodeProjectionJobPoll, M11IndentedCodeProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase == ProjectionJobPhase::Complete {
            return Ok(M11IndentedCodeProjectionJobPoll {
                status: M11IndentedCodeProjectionJobPollStatus::Complete,
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
            return Err(M11IndentedCodeProjectionJobError::InvalidState);
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
                    Err(M11IndentedCodeProjectionJobError::InvalidState)
                }
            };
            if let Err(error) = step {
                self.phase = ProjectionJobPhase::Faulted;
                return Err(error);
            }
            if transitions == before && self.phase != phase_before {
                transitions = transitions
                    .checked_add(1)
                    .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
            }
            if self.phase == ProjectionJobPhase::Complete {
                return Ok(M11IndentedCodeProjectionJobPoll {
                    status: M11IndentedCodeProjectionJobPollStatus::Complete,
                    transitions,
                });
            }
            if transitions == before {
                break;
            }
        }
        Ok(M11IndentedCodeProjectionJobPoll {
            status: M11IndentedCodeProjectionJobPollStatus::Pending,
            transitions,
        })
    }

    fn poll_line_discovery(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        let scanner = self
            .scanner
            .take()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?;
        let (poll, inspected) = scanner.poll_counted_retaining_complete(fuel - *transitions)?;
        *transitions = transitions
            .checked_add(inspected)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
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
                    return Err(
                        M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                            "physical line discovery crossed fenced coverage",
                        ),
                    );
                }
                self.line_scanner = Some(SegmentedLineScanner::new(
                    facts.identity().start_byte() == 0,
                ));
                self.line_source = Some(line.into_source()?);
                self.phase = ProjectionJobPhase::ReadLine;
            }
            SnapshotLineRetainedPoll::Complete(scanner) => {
                drop(scanner.into_source_lease());
                return Err(
                    M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                        "physical line discovery ended before fenced coverage",
                    ),
                );
            }
        }
        Ok(())
    }

    fn poll_line_read(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
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
                .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?;
            if source.access_budget() == 0 {
                let _ = source.replenish_access_budget(remaining_fuel)?;
            }
            let offset = source.position();
            let byte = source.read_byte(offset)?;
            self.line_scanner
                .as_mut()
                .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
                .push(byte);
            *transitions = transitions
                .checked_add(1)
                .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        }
        Ok(())
    }

    fn finish_line(&mut self) -> Result<(), M11IndentedCodeProjectionJobError> {
        let source = self
            .line_source
            .take()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?;
        let physical = source.facts();
        let scanner = source.finish()?;
        let segmented = self
            .line_scanner
            .take()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
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
            return Err(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "physical line exceeded fenced coverage",
                ),
            );
        } else {
            self.scanner = Some(scanner);
            self.phase = if self.page.len() == INDENTED_CODE_LINES_PER_PAGE_MAX {
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
    ) -> Result<IndentedCodeLineV1, M11IndentedCodeProjectionJobError> {
        let identity = physical.identity();
        let physical_bytes = usize::try_from(physical.physical_bytes())
            .map_err(|_| M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let code = segmented.indented_code.ok_or(
            M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                "published variant-7 line lacks exact indented-code facts",
            ),
        )?;
        if code.hidden_prefix.start != 0
            || code.hidden_prefix.end != code.content.start
            || code.content.end != code.line_ending.start
            || code.line_ending.end != physical_bytes
        {
            return Err(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "segmented line spans do not partition physical source",
                ),
            );
        }
        let expected_eol = ending_bytes(physical.ending());
        let observed_eol = code
            .line_ending
            .end
            .checked_sub(code.line_ending.start)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        if observed_eol != expected_eol
            || segmented.had_ending != !matches!(physical.ending(), M11LineEnding::Eof)
        {
            return Err(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "segmented line ending disagrees with source facts",
                ),
            );
        }
        if self.observed_line_count == 0 {
            self.first_line_blank = Some(segmented.blank);
            self.observed_bof_bom = Some(segmented.has_bof_bom);
        } else if segmented.has_bof_bom {
            return Err(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "BOF BOM appeared after the first fenced line",
                ),
            );
        }
        self.last_line_blank = Some(segmented.blank);

        let relative_start = identity
            .start_byte()
            .checked_sub(self.block_source.start)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let physical_length = physical.physical_bytes();
        let hidden_prefix_length = u32::try_from(code.hidden_prefix.end)
            .map_err(|_| M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let content_length = u32::try_from(
            code.content
                .end
                .checked_sub(code.content.start)
                .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?,
        )
        .map_err(|_| M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let hidden_utf16 = hidden_prefix_length
            .checked_sub(if segmented.has_bof_bom { 2 } else { 0 })
            .ok_or(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "BOF BOM is outside the hidden prefix",
                ),
            )?;
        let projected_utf8 = physical_length
            .checked_sub(hidden_prefix_length)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let projected_utf16 = physical
            .physical_utf16()
            .checked_sub(hidden_utf16)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;

        self.observed_line_count = self
            .observed_line_count
            .checked_add(1)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        self.observed_projected_utf8_length = self
            .observed_projected_utf8_length
            .checked_add(projected_utf8)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        self.observed_projected_utf16_length = self
            .observed_projected_utf16_length
            .checked_add(projected_utf16)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        self.observed_physical_utf16_length = self
            .observed_physical_utf16_length
            .checked_add(physical.physical_utf16())
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        self.observed_terminal_eol_bytes = u32::try_from(expected_eol)
            .map_err(|_| M11IndentedCodeProjectionJobError::CoordinateOverflow)?;

        if segmented.blank && content_length == 0 {
            Ok(IndentedCodeLineV1::internal_blank(
                relative_start,
                physical_length,
                hidden_prefix_length,
            )?)
        } else {
            if !segmented.blank && content_length == 0 {
                return Err(
                    M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                        "nonblank indented-code line has no source-backed content",
                    ),
                );
            }
            Ok(IndentedCodeLineV1::code(
                relative_start,
                physical_length,
                hidden_prefix_length,
                content_length,
            )?)
        }
    }

    fn offer_page(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        if self.page.is_empty() {
            return Err(M11IndentedCodeProjectionJobError::InvalidState);
        }
        self.build
            .as_mut()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
            .offer_page(&self.page)?;
        self.page.clear();
        self.phase = ProjectionJobPhase::PollOfferedPage;
        *transitions = transitions
            .checked_add(1)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        Ok(())
    }

    fn poll_offered_page(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        let poll = self
            .build
            .as_mut()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        match poll.status() {
            M11IndentedCodeProjectionBuildStatus::NeedsPage => {
                self.phase = if self.scan_complete {
                    ProjectionJobPhase::FinishInput
                } else {
                    ProjectionJobPhase::DiscoverLine
                };
            }
            M11IndentedCodeProjectionBuildStatus::Pending => {}
            M11IndentedCodeProjectionBuildStatus::Complete
            | M11IndentedCodeProjectionBuildStatus::Cancelled => {
                return Err(M11IndentedCodeProjectionJobError::InvalidState);
            }
        }
        Ok(())
    }

    fn finish_input(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        self.validate_observed_summary()?;
        self.build
            .as_mut()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
            .finish_input()?;
        self.phase = ProjectionJobPhase::Seal;
        *transitions = transitions
            .checked_add(1)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        Ok(())
    }

    fn validate_observed_summary(&self) -> Result<(), M11IndentedCodeProjectionJobError> {
        let expected_physical_utf16 = self
            .block_source_utf16
            .end
            .checked_sub(self.block_source_utf16.start)
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        let mismatch = if !self.scan_complete || self.next_absolute_byte != self.block_source.end {
            Some("line coverage does not end at the fenced block boundary")
        } else if self.first_line_blank != Some(false) || self.last_line_blank != Some(false) {
            Some("published code leaf has a leading or trailing lexical blank")
        } else if self.observed_line_count != self.expected_line_count {
            Some("line count differs from the published variant-7 summary")
        } else if self.observed_projected_utf8_length != self.expected_projected_utf8_length {
            Some("projected UTF-8 length differs from the published variant-7 summary")
        } else if self.observed_projected_utf16_length != self.expected_projected_utf16_length {
            Some("projected UTF-16 length differs from the published variant-7 summary")
        } else if self.observed_physical_utf16_length != expected_physical_utf16 {
            Some("physical UTF-16 length differs from the published block authority")
        } else if self.observed_terminal_eol_bytes != self.expected_terminal_eol_bytes {
            Some("terminal EOL width differs from the published variant-7 summary")
        } else if self.observed_bof_bom != Some(self.expected_bof_bom) {
            Some("BOF BOM fact differs from the published variant-7 summary")
        } else {
            None
        };
        mismatch.map_or(Ok(()), |message| {
            Err(M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(message))
        })
    }

    fn poll_seal(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        let poll = self
            .build
            .as_mut()
            .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
        match poll.status() {
            M11IndentedCodeProjectionBuildStatus::Pending => {}
            M11IndentedCodeProjectionBuildStatus::Complete => {
                let root = self
                    .build
                    .as_mut()
                    .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?
                    .take_root()
                    .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?;
                self.root = Some(root);
                drop(self.build.take());
                self.validate_root(
                    self.root
                        .as_ref()
                        .ok_or(M11IndentedCodeProjectionJobError::InvalidState)?,
                )?;
                drop(self.authority.take());
                self.phase = ProjectionJobPhase::Complete;
            }
            M11IndentedCodeProjectionBuildStatus::NeedsPage
            | M11IndentedCodeProjectionBuildStatus::Cancelled => {
                return Err(M11IndentedCodeProjectionJobError::InvalidState);
            }
        }
        Ok(())
    }

    fn validate_root(
        &self,
        root: &M11IndentedCodeProjectionRoot,
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        let descriptor = root.descriptor();
        if descriptor.source() != self.source
            || descriptor.parser_profile() != self.binding.syntax_profile()
            || descriptor.physical_block_range() != &self.block_source
            || descriptor.requested_window() != &self.block_source
            || descriptor.line_count() != u64::from(self.expected_line_count)
            || descriptor.has_synthetic_final_lf() != (self.expected_terminal_eol_bytes == 0)
        {
            return Err(
                M11IndentedCodeProjectionJobError::StructuralSummaryMismatch(
                    "persistent root descriptor differs from exact block authority",
                ),
            );
        }
        Ok(())
    }

    /// Transfers the ready persistent root to its caller.
    #[must_use]
    pub fn take_root(&mut self) -> Option<M11IndentedCodeProjectionRoot> {
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
    ) -> Result<(), M11IndentedCodeProjectionJobError> {
        if matches!(
            self.phase,
            ProjectionJobPhase::Transferred | ProjectionJobPhase::Cancelled
        ) {
            return Err(M11IndentedCodeProjectionJobError::InvalidState);
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
    ) -> Result<M11IndentedCodeProjectionJobReleasePoll, M11IndentedCodeProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase != ProjectionJobPhase::Cancelling {
            return Err(M11IndentedCodeProjectionJobError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if let Some(build) = self.build.as_mut() {
                if !self.build_cancel_started {
                    return Err(M11IndentedCodeProjectionJobError::InvalidState);
                }
                let poll = build.poll_cancel(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.receipt().transitions)
                    .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.build.take());
                    continue;
                }
                return Ok(M11IndentedCodeProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(root) = self.root.as_ref() {
                if !self.root_release_started {
                    return Err(M11IndentedCodeProjectionJobError::InvalidState);
                }
                let poll = root.poll_release(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.receipt().transitions)
                    .ok_or(M11IndentedCodeProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.root.take());
                    continue;
                }
                return Ok(M11IndentedCodeProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            drop(self.authority.take());
            self.phase = ProjectionJobPhase::Cancelled;
            return Ok(M11IndentedCodeProjectionJobReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11IndentedCodeProjectionJobReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11IndentedCodeProjectionJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    ProjectionJobPhase::Cancelled | ProjectionJobPhase::Transferred
                ),
                "indented-code projection jobs require root transfer or explicit fuelled cancellation"
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

fn validate_fuel(fuel: usize) -> Result<(), M11IndentedCodeProjectionJobError> {
    if fuel == 0 {
        return Err(M11IndentedCodeProjectionJobError::ZeroFuel);
    }
    if fuel > M11_INDENTED_CODE_PROJECTION_JOB_MAX_POLL_TRANSITIONS {
        return Err(M11IndentedCodeProjectionJobError::PollLimitExceeded);
    }
    Ok(())
}
