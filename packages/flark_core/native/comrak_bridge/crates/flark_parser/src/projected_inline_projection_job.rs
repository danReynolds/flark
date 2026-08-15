//! Marker-free inline projection for one certified recursive-Green quote.
//!
//! The job is a bounded transducer: it first authenticates the quote's
//! physical-line map, materializes the exact logical quote stream in a private
//! runtime, parses that stream with the ordinary inline pipeline, then rebuilds
//! accepted facts under the original physical source authority. Projected
//! coordinates never masquerade as absolute source coordinates.

use std::fmt;

use flark_engine::parser_internal::{
    BlockQuoteLineV1, M11BlockQuoteProjectionError, M11BlockQuoteProjectionRoot,
    M11InlineProjectionBuild, M11InlineProjectionBuildStatus, M11InlineProjectionError,
    M11InlineProjectionFact, M11InlineProjectionKind, M11InlineProjectionRoot, M11ParserPageError,
    M11ParserRangeCursor, M11ParserRangeStatus, M11ParserSourceRangeAuthority,
    M11ProjectedInlineProjectionRoot, M11_PARSER_PAGE_MAX_POLL_TRANSITIONS,
    M11_PARSER_RANGE_MAX_POLL_BYTES,
};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError};

use crate::{
    M11BlockQuoteProjectionJob, M11BlockQuoteProjectionJobError,
    M11BlockQuoteProjectionJobPollStatus, M11InlineProjectionJob, M11InlineProjectionJobError,
    M11InlineProjectionJobPollStatus, M11InlineProjectionPublication, M11ParserBinding,
    M11RecursiveGreenBlockQuoteProjectionFence,
};

pub const M11_PROJECTED_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS: usize =
    M11_PARSER_PAGE_MAX_POLL_TRANSITIONS;

const UNSUPPORTED_SCRATCH_PARSE: u32 = 1;
const UNSUPPORTED_PROJECTED_FACT_KIND: u32 = 2;

#[derive(Debug)]
pub enum M11ProjectedInlineProjectionJobError {
    Quote(M11BlockQuoteProjectionJobError),
    QuoteProjection(M11BlockQuoteProjectionError),
    Inline(M11InlineProjectionJobError),
    Projection(M11InlineProjectionError),
    Pages(M11ParserPageError),
    Document(DocumentRuntimeError),
    InvalidUtf8,
    CoordinateOverflow,
    ZeroFuel,
    PollLimitExceeded,
    InvalidState,
}

impl fmt::Display for M11ProjectedInlineProjectionJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quote(error) => error.fmt(formatter),
            Self::QuoteProjection(error) => error.fmt(formatter),
            Self::Inline(error) => error.fmt(formatter),
            Self::Projection(error) => error.fmt(formatter),
            Self::Pages(error) => error.fmt(formatter),
            Self::Document(error) => error.fmt(formatter),
            Self::InvalidUtf8 => formatter.write_str("projected quote source is not UTF-8"),
            Self::CoordinateOverflow => {
                formatter.write_str("projected inline coordinate or counter overflow")
            }
            Self::ZeroFuel => formatter.write_str("projected inline poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("projected inline poll exceeds its transition limit")
            }
            Self::InvalidState => {
                formatter.write_str("projected inline job is in an invalid state")
            }
        }
    }
}

impl std::error::Error for M11ProjectedInlineProjectionJobError {}

impl From<M11BlockQuoteProjectionJobError> for M11ProjectedInlineProjectionJobError {
    fn from(value: M11BlockQuoteProjectionJobError) -> Self {
        Self::Quote(value)
    }
}

impl From<M11BlockQuoteProjectionError> for M11ProjectedInlineProjectionJobError {
    fn from(value: M11BlockQuoteProjectionError) -> Self {
        Self::QuoteProjection(value)
    }
}

impl From<M11InlineProjectionJobError> for M11ProjectedInlineProjectionJobError {
    fn from(value: M11InlineProjectionJobError) -> Self {
        Self::Inline(value)
    }
}

impl From<M11InlineProjectionError> for M11ProjectedInlineProjectionJobError {
    fn from(value: M11InlineProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<M11ParserPageError> for M11ProjectedInlineProjectionJobError {
    fn from(value: M11ParserPageError) -> Self {
        Self::Pages(value)
    }
}

impl From<DocumentRuntimeError> for M11ProjectedInlineProjectionJobError {
    fn from(value: DocumentRuntimeError) -> Self {
        Self::Document(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ProjectedInlineProjectionJobPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ProjectedInlineProjectionJobPoll {
    status: M11ProjectedInlineProjectionJobPollStatus,
    transitions: usize,
}

impl M11ProjectedInlineProjectionJobPoll {
    #[must_use]
    pub const fn status(self) -> M11ProjectedInlineProjectionJobPollStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ProjectedInlineProjectionJobReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11ProjectedInlineProjectionJobReleasePoll {
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Debug)]
pub enum M11ProjectedInlineProjectionOutput {
    Authoritative(M11ProjectedInlineProjectionRoot),
    Unsupported { reason: u32, metadata: Box<[u8]> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Quote,
    TakeQuote,
    CopyPhysical,
    Materialize,
    ReleaseQuote,
    StartScratch,
    Scratch,
    TakeScratch,
    ReleaseScratch,
    StartBuild,
    OfferFact,
    PollOfferedFact,
    FinishBuild,
    SealBuild,
    Complete,
    Faulted,
    Cancelling,
    Cancelled,
    Transferred,
}

#[must_use = "projected inline jobs require output transfer or explicit fuelled cancellation"]
pub struct M11ProjectedInlineProjectionJob {
    binding: M11ParserBinding,
    quote_job: Option<Box<M11BlockQuoteProjectionJob>>,
    quote_root: Option<M11BlockQuoteProjectionRoot>,
    authority: Option<M11ParserSourceRangeAuthority>,
    source_cursor: Option<M11ParserRangeCursor>,
    lines: Vec<BlockQuoteLineV1>,
    physical: Vec<u8>,
    logical: Vec<u8>,
    line_index: usize,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    source_projection_commitment256: [u8; 32],
    scratch_runtime: Option<Box<DocumentRuntime>>,
    scratch_job: Option<Box<M11InlineProjectionJob>>,
    scratch_root: Option<M11InlineProjectionRoot>,
    facts: Vec<M11InlineProjectionFact>,
    fact_index: usize,
    maximum_fact_end: u32,
    build: Option<Box<M11InlineProjectionBuild>>,
    root: Option<M11ProjectedInlineProjectionRoot>,
    unsupported: Option<(u32, Box<[u8]>)>,
    phase: Phase,
    quote_release_started: bool,
    scratch_release_started: bool,
    scratch_runtime_close_started: bool,
    build_cancel_started: bool,
    root_release_started: bool,
}

impl fmt::Debug for M11ProjectedInlineProjectionJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ProjectedInlineProjectionJob")
            .field("phase", &self.phase)
            .field("line_index", &self.line_index)
            .field("fact_index", &self.fact_index)
            .field("projected_utf8_length", &self.projected_utf8_length)
            .finish_non_exhaustive()
    }
}

impl M11ProjectedInlineProjectionJob {
    pub fn new(
        runtime: &DocumentRuntime,
        fence: M11RecursiveGreenBlockQuoteProjectionFence,
        binding: M11ParserBinding,
    ) -> Result<Self, M11ProjectedInlineProjectionJobError> {
        Ok(Self {
            binding,
            quote_job: Some(Box::new(
                M11BlockQuoteProjectionJob::new_for_recursive_green_projected_inline(
                    runtime, fence, binding,
                )?,
            )),
            quote_root: None,
            authority: None,
            source_cursor: None,
            lines: Vec::new(),
            physical: Vec::new(),
            logical: Vec::new(),
            line_index: 0,
            projected_utf8_length: 0,
            projected_utf16_length: 0,
            source_projection_commitment256: [0; 32],
            scratch_runtime: None,
            scratch_job: None,
            scratch_root: None,
            facts: Vec::new(),
            fact_index: 0,
            maximum_fact_end: 0,
            build: None,
            root: None,
            unsupported: None,
            phase: Phase::Quote,
            quote_release_started: false,
            scratch_release_started: false,
            scratch_runtime_close_started: false,
            build_cancel_started: false,
            root_release_started: false,
        })
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ProjectedInlineProjectionJobPoll, M11ProjectedInlineProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase == Phase::Complete {
            return Ok(M11ProjectedInlineProjectionJobPoll {
                status: M11ProjectedInlineProjectionJobPollStatus::Complete,
                transitions: 0,
            });
        }
        if matches!(
            self.phase,
            Phase::Faulted | Phase::Cancelling | Phase::Cancelled | Phase::Transferred
        ) {
            return Err(M11ProjectedInlineProjectionJobError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            let before = transitions;
            let phase_before = self.phase;
            let result = match self.phase {
                Phase::Quote => self.poll_quote(runtime, fuel, &mut transitions),
                Phase::TakeQuote => self.take_quote(runtime, &mut transitions),
                Phase::CopyPhysical => self.copy_physical(fuel, &mut transitions),
                Phase::Materialize => self.materialize_line(&mut transitions),
                Phase::ReleaseQuote => self.release_quote(runtime, fuel, &mut transitions),
                Phase::StartScratch => self.start_scratch(&mut transitions),
                Phase::Scratch => self.poll_scratch(fuel, &mut transitions),
                Phase::TakeScratch => self.take_scratch(&mut transitions),
                Phase::ReleaseScratch => self.release_scratch(fuel, &mut transitions),
                Phase::StartBuild => self.start_build(runtime, &mut transitions),
                Phase::OfferFact => self.offer_fact(&mut transitions),
                Phase::PollOfferedFact => self.poll_offered_fact(runtime, fuel, &mut transitions),
                Phase::FinishBuild => self.finish_build(&mut transitions),
                Phase::SealBuild => self.seal_build(runtime, fuel, &mut transitions),
                Phase::Complete => break,
                Phase::Faulted | Phase::Cancelling | Phase::Cancelled | Phase::Transferred => {
                    Err(M11ProjectedInlineProjectionJobError::InvalidState)
                }
            };
            if let Err(error) = result {
                self.phase = Phase::Faulted;
                return Err(error);
            }
            if transitions == before && self.phase != phase_before {
                transitions = transitions
                    .checked_add(1)
                    .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
            }
            if self.phase == Phase::Complete {
                return Ok(M11ProjectedInlineProjectionJobPoll {
                    status: M11ProjectedInlineProjectionJobPollStatus::Complete,
                    transitions,
                });
            }
            if transitions == before {
                break;
            }
        }
        Ok(M11ProjectedInlineProjectionJobPoll {
            status: M11ProjectedInlineProjectionJobPollStatus::Pending,
            transitions,
        })
    }

    fn poll_quote(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let poll = self
            .quote_job
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11BlockQuoteProjectionJobPollStatus::Complete {
            self.phase = Phase::TakeQuote;
        }
        Ok(())
    }

    fn take_quote(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let (root, authority, lines) = self
            .quote_job
            .as_mut()
            .and_then(|job| job.take_projected_inline_parts())
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
        drop(self.quote_job.take());
        let descriptor = root.descriptor();
        self.projected_utf8_length = descriptor.projected_utf8_length();
        self.projected_utf16_length = descriptor.projected_utf16_length();
        self.source_projection_commitment256 = descriptor.ordered_commitment256();
        let physical_length = authority
            .source_range()
            .end
            .checked_sub(authority.source_range().start)
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        self.physical
            .try_reserve_exact(physical_length)
            .map_err(|_| {
                M11ProjectedInlineProjectionJobError::Quote(
                    M11BlockQuoteProjectionJobError::AllocationFailed,
                )
            })?;
        self.logical
            .try_reserve_exact(self.projected_utf8_length as usize)
            .map_err(|_| {
                M11ProjectedInlineProjectionJobError::Quote(
                    M11BlockQuoteProjectionJobError::AllocationFailed,
                )
            })?;
        self.source_cursor = Some(authority.cursor(runtime)?);
        self.quote_root = Some(root);
        self.authority = Some(authority);
        self.lines = lines;
        self.phase = Phase::CopyPhysical;
        *transitions += 1;
        Ok(())
    }

    fn copy_physical(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let quantum = (fuel - *transitions)
            .min(M11_PARSER_RANGE_MAX_POLL_BYTES)
            .min(256);
        if quantum == 0 {
            return Ok(());
        }
        let mut buffer = [0_u8; 256];
        let poll = self
            .source_cursor
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .poll(quantum, &mut buffer[..quantum])?;
        self.physical
            .extend_from_slice(&buffer[..poll.bytes_read()]);
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11ParserRangeStatus::Complete {
            drop(self.source_cursor.take());
            let expected = self
                .authority
                .as_ref()
                .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
                .source_range();
            if self.physical.len() != expected.end - expected.start {
                return Err(M11ProjectedInlineProjectionJobError::InvalidState);
            }
            self.phase = Phase::Materialize;
        }
        Ok(())
    }

    fn materialize_line(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let Some(line) = self.lines.get(self.line_index).copied() else {
            if self.logical.len() != self.projected_utf8_length as usize {
                return Err(M11ProjectedInlineProjectionJobError::InvalidState);
            }
            let text = std::str::from_utf8(&self.logical)
                .map_err(|_| M11ProjectedInlineProjectionJobError::InvalidUtf8)?;
            let utf16 = u32::try_from(text.encode_utf16().count())
                .map_err(|_| M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
            if utf16 != self.projected_utf16_length {
                return Err(M11ProjectedInlineProjectionJobError::InvalidState);
            }
            self.phase = Phase::ReleaseQuote;
            *transitions += 1;
            return Ok(());
        };
        let start = usize::try_from(
            line.relative_line_start()
                .checked_add(line.hidden_prefix_length())
                .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?,
        )
        .map_err(|_| M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        let length = usize::try_from(
            line.content_length()
                .checked_add(line.physical_eol_length())
                .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?,
        )
        .map_err(|_| M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        let end = start
            .checked_add(length)
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        self.logical.extend_from_slice(
            self.physical
                .get(start..end)
                .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
        );
        self.line_index += 1;
        *transitions += 1;
        Ok(())
    }

    fn release_quote(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let root = self
            .quote_root
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
        if !self.quote_release_started {
            root.begin_release(runtime)?;
            self.quote_release_started = true;
            *transitions += 1;
            return Ok(());
        }
        let poll = root.poll_release(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.receipt().transitions)
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        if poll.complete() {
            drop(self.quote_root.take());
            self.phase = Phase::StartScratch;
        }
        Ok(())
    }

    fn start_scratch(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let text = std::str::from_utf8(&self.logical)
            .map_err(|_| M11ProjectedInlineProjectionJobError::InvalidUtf8)?;
        let runtime = Box::new(DocumentRuntime::new(
            text,
            DocumentRuntimeConfig::default(),
        )?);
        let source_length = self.logical.len();
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source()?,
            0..source_length,
        )?;
        let job = M11InlineProjectionJob::new_for_exact_projected_source(
            &runtime,
            authority,
            self.binding,
        )?;
        self.scratch_runtime = Some(runtime);
        self.scratch_job = Some(Box::new(job));
        self.phase = Phase::Scratch;
        *transitions += 1;
        Ok(())
    }

    fn poll_scratch(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let poll = self
            .scratch_job
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .poll(
                self.scratch_runtime
                    .as_mut()
                    .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                fuel - *transitions,
            )?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            self.phase = Phase::TakeScratch;
        }
        Ok(())
    }

    fn take_scratch(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let job = self
            .scratch_job
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
        self.facts = job
            .take_projected_facts()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
        let output = job
            .take_output()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
        drop(self.scratch_job.take());
        let (_, _, _, authority, publication) = output.into_publication_parts().into_parts();
        drop(authority);
        let mut unsupported_reason = None;
        match publication {
            M11InlineProjectionPublication::Authoritative(mut root) => {
                for fact in &self.facts {
                    let range = fact.relative_range();
                    self.maximum_fact_end = self.maximum_fact_end.max(range.end);
                    if !matches!(
                        fact.kind(),
                        M11InlineProjectionKind::Emphasis
                            | M11InlineProjectionKind::Strong
                            | M11InlineProjectionKind::Code
                    ) {
                        unsupported_reason = Some(UNSUPPORTED_PROJECTED_FACT_KIND);
                    }
                }
                root.begin_release(
                    self.scratch_runtime
                        .as_mut()
                        .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                )?;
                self.scratch_release_started = true;
                self.scratch_root = Some(root);
            }
            M11InlineProjectionPublication::Unsupported(record) => {
                drop(record);
                unsupported_reason = Some(UNSUPPORTED_SCRATCH_PARSE);
            }
        }
        if let Some(reason) = unsupported_reason {
            self.unsupported = Some((reason, unsupported_metadata(reason)));
            self.facts.clear();
        }
        self.phase = Phase::ReleaseScratch;
        *transitions += 1;
        Ok(())
    }

    fn release_scratch(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        if let Some(root) = self.scratch_root.as_ref() {
            if !self.scratch_release_started {
                return Err(M11ProjectedInlineProjectionJobError::InvalidState);
            }
            let poll = root.poll_release(
                self.scratch_runtime
                    .as_mut()
                    .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                fuel - *transitions,
            )?;
            *transitions = transitions
                .checked_add(poll.receipt().transitions)
                .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
            if !poll.complete() {
                return Ok(());
            }
            drop(self.scratch_root.take());
            if *transitions == fuel {
                return Ok(());
            }
        }
        let scratch_runtime = self
            .scratch_runtime
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
        if !self.scratch_runtime_close_started {
            scratch_runtime.begin_close()?;
            self.scratch_runtime_close_started = true;
            *transitions += 1;
            return Ok(());
        }
        let poll = scratch_runtime.poll_close(fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.released_source_leases)
            .and_then(|value| value.checked_add(poll.arena_transitions))
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        if !poll.complete {
            return Ok(());
        }
        drop(self.scratch_runtime.take());
        self.phase = if self.unsupported.is_some() {
            drop(self.authority.take());
            Phase::Complete
        } else {
            Phase::StartBuild
        };
        Ok(())
    }

    fn start_build(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        self.build = Some(Box::new(
            M11InlineProjectionBuild::new_from_source_authority(
                runtime,
                self.authority
                    .as_ref()
                    .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                self.binding.syntax_profile(),
            )?,
        ));
        self.phase = Phase::OfferFact;
        *transitions += 1;
        Ok(())
    }

    fn offer_fact(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let Some(fact) = self.facts.get(self.fact_index).copied() else {
            self.phase = Phase::FinishBuild;
            return Ok(());
        };
        if fact.relative_range().end > self.projected_utf8_length {
            return Err(M11ProjectedInlineProjectionJobError::InvalidState);
        }
        self.build
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .offer_page(&[fact])?;
        self.fact_index += 1;
        self.phase = Phase::PollOfferedFact;
        *transitions += 1;
        Ok(())
    }

    fn poll_offered_fact(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let poll = self
            .build
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        match poll.status() {
            M11InlineProjectionBuildStatus::NeedsPage => self.phase = Phase::OfferFact,
            M11InlineProjectionBuildStatus::Pending => {}
            M11InlineProjectionBuildStatus::Complete
            | M11InlineProjectionBuildStatus::Cancelled => {
                return Err(M11ProjectedInlineProjectionJobError::InvalidState)
            }
        }
        Ok(())
    }

    fn finish_build(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        self.build
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .finish_input()?;
        self.phase = Phase::SealBuild;
        *transitions += 1;
        Ok(())
    }

    fn seal_build(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        let poll = self
            .build
            .as_mut()
            .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
        match poll.status() {
            M11InlineProjectionBuildStatus::Pending => {}
            M11InlineProjectionBuildStatus::Complete => {
                let root = self
                    .build
                    .as_mut()
                    .and_then(|build| build.take_root())
                    .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?;
                drop(self.build.take());
                if root.descriptor().fact_count() != self.facts.len() as u64 {
                    return Err(M11ProjectedInlineProjectionJobError::InvalidState);
                }
                self.root = Some(M11ProjectedInlineProjectionRoot::new(
                    root,
                    self.projected_utf8_length,
                    self.projected_utf16_length,
                    self.maximum_fact_end,
                    self.source_projection_commitment256,
                )?);
                drop(self.authority.take());
                self.phase = Phase::Complete;
            }
            M11InlineProjectionBuildStatus::NeedsPage
            | M11InlineProjectionBuildStatus::Cancelled => {
                return Err(M11ProjectedInlineProjectionJobError::InvalidState)
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn take_output(&mut self) -> Option<M11ProjectedInlineProjectionOutput> {
        if self.phase != Phase::Complete {
            return None;
        }
        let output = if let Some(root) = self.root.take() {
            M11ProjectedInlineProjectionOutput::Authoritative(root)
        } else {
            let (reason, metadata) = self.unsupported.take()?;
            M11ProjectedInlineProjectionOutput::Unsupported { reason, metadata }
        };
        self.phase = Phase::Transferred;
        Some(output)
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ProjectedInlineProjectionJobError> {
        if matches!(self.phase, Phase::Transferred | Phase::Cancelled) {
            return Err(M11ProjectedInlineProjectionJobError::InvalidState);
        }
        if let Some(cursor) = self.source_cursor.as_mut() {
            cursor.cancel();
        }
        drop(self.source_cursor.take());
        if let Some(job) = self.quote_job.as_mut() {
            job.begin_cancel(runtime)?;
        }
        if let Some(root) = self.quote_root.as_mut() {
            if !self.quote_release_started {
                root.begin_release(runtime)?;
                self.quote_release_started = true;
            }
        }
        if let Some(job) = self.scratch_job.as_mut() {
            job.begin_abort(
                self.scratch_runtime
                    .as_mut()
                    .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
            )?;
        }
        if let Some(root) = self.scratch_root.as_mut() {
            if !self.scratch_release_started {
                root.begin_release(
                    self.scratch_runtime
                        .as_mut()
                        .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                )?;
                self.scratch_release_started = true;
            }
        }
        if let Some(build) = self.build.as_mut() {
            build.begin_cancel(runtime)?;
            self.build_cancel_started = true;
        }
        if let Some(root) = self.root.as_mut() {
            root.begin_release(runtime)?;
            self.root_release_started = true;
        }
        self.phase = Phase::Cancelling;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ProjectedInlineProjectionJobReleasePoll, M11ProjectedInlineProjectionJobError>
    {
        validate_fuel(fuel)?;
        if self.phase != Phase::Cancelling {
            return Err(M11ProjectedInlineProjectionJobError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if let Some(job) = self.quote_job.as_mut() {
                let poll = job.poll_cancel(runtime, fuel - transitions)?;
                transitions += poll.transitions();
                if !poll.complete() {
                    break;
                }
                drop(self.quote_job.take());
                continue;
            }
            if let Some(root) = self.quote_root.as_ref() {
                let poll = root.poll_release(runtime, fuel - transitions)?;
                transitions += poll.receipt().transitions;
                if !poll.complete() {
                    break;
                }
                drop(self.quote_root.take());
                continue;
            }
            if let Some(job) = self.scratch_job.as_mut() {
                let poll = job.poll_abort(
                    self.scratch_runtime
                        .as_mut()
                        .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                    fuel - transitions,
                )?;
                transitions += poll.transitions();
                if !poll.complete() {
                    break;
                }
                drop(self.scratch_job.take());
                continue;
            }
            if let Some(root) = self.scratch_root.as_ref() {
                let poll = root.poll_release(
                    self.scratch_runtime
                        .as_mut()
                        .ok_or(M11ProjectedInlineProjectionJobError::InvalidState)?,
                    fuel - transitions,
                )?;
                transitions += poll.receipt().transitions;
                if !poll.complete() {
                    break;
                }
                drop(self.scratch_root.take());
                continue;
            }
            if let Some(scratch_runtime) = self.scratch_runtime.as_mut() {
                if !self.scratch_runtime_close_started {
                    scratch_runtime.begin_close()?;
                    self.scratch_runtime_close_started = true;
                    transitions += 1;
                    continue;
                }
                let poll = scratch_runtime.poll_close(fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.released_source_leases)
                    .and_then(|value| value.checked_add(poll.arena_transitions))
                    .ok_or(M11ProjectedInlineProjectionJobError::CoordinateOverflow)?;
                if !poll.complete {
                    break;
                }
                drop(self.scratch_runtime.take());
                continue;
            }
            if let Some(build) = self.build.as_mut() {
                if !self.build_cancel_started {
                    return Err(M11ProjectedInlineProjectionJobError::InvalidState);
                }
                let poll = build.poll_cancel(runtime, fuel - transitions)?;
                transitions += poll.receipt().transitions;
                if !poll.complete() {
                    break;
                }
                drop(self.build.take());
                continue;
            }
            if let Some(root) = self.root.as_ref() {
                if !self.root_release_started {
                    return Err(M11ProjectedInlineProjectionJobError::InvalidState);
                }
                let poll = root.poll_release(runtime, fuel - transitions)?;
                transitions += poll.receipt().transitions;
                if !poll.complete() {
                    break;
                }
                drop(self.root.take());
                continue;
            }
            drop(self.authority.take());
            drop(self.unsupported.take());
            self.phase = Phase::Cancelled;
            return Ok(M11ProjectedInlineProjectionJobReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11ProjectedInlineProjectionJobReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11ProjectedInlineProjectionJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(self.phase, Phase::Cancelled | Phase::Transferred),
                "projected inline jobs require output transfer or explicit fuelled cancellation"
            );
        }
    }
}

fn unsupported_metadata(reason: u32) -> Box<[u8]> {
    let mut bytes = [0_u8; 12];
    bytes[..4].copy_from_slice(b"PUI1");
    bytes[4..8].copy_from_slice(&1_u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&reason.to_le_bytes());
    Box::new(bytes)
}

fn validate_fuel(fuel: usize) -> Result<(), M11ProjectedInlineProjectionJobError> {
    if fuel == 0 {
        return Err(M11ProjectedInlineProjectionJobError::ZeroFuel);
    }
    if fuel > M11_PROJECTED_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS {
        return Err(M11ProjectedInlineProjectionJobError::PollLimitExceeded);
    }
    Ok(())
}
