//! Resumable, exact-source-bound raw backtick indexing.
//!
//! This is the first grammar stage above the repeatable inline scanner. It
//! keeps no document-sized lexical tree: raw backtick runs and the sparse
//! same-length reverse index live in engine-admitted radix pages. Every source
//! pass and scratch mutation is cooperatively polled. It deliberately does
//! not pair code spans or derive cooking flags; the later opaque resolver is
//! the one ownership authority that pairs the retained runs together with
//! angle-autolink openers.

use std::fmt;
use std::ops::Range;

use flark_engine::parser_internal::{
    M11ParserPageError, M11ParserRangeCursor, M11ParserSourceRangeAuthority,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::inline_lex::{
    M11InlineLexError, M11InlineLexEventKind, M11InlineLexPollStatus, M11InlineLexScanner,
};
use crate::inline_radix::{
    M11InlineRadixError, M11InlineRadixPages, M11InlineRadixReclaimPoll,
    M11_INLINE_RADIX_MAX_POLL_TRANSITIONS,
};

pub(crate) const M11_INLINE_CODE_MAX_POLL_TRANSITIONS: usize =
    M11_INLINE_RADIX_MAX_POLL_TRANSITIONS;

const RUN_PAGE_RECORDS: usize = 256;
const HEAD_PAGE_RECORDS: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CodeRun {
    start: u32,
    len: u32,
    next_closer_plus_one: u32,
    escaped_prefix: u8,
    reserved: [u8; 3],
}

impl CodeRun {
    pub(crate) fn raw_start(self) -> u32 {
        self.start
    }

    pub(crate) fn raw_end(self) -> Result<u32, M11InlineCodeError> {
        self.start
            .checked_add(self.len)
            .ok_or(M11InlineCodeError::CoordinateOverflow)
    }

    pub(crate) fn top_level_opener(self) -> Result<Option<(u32, u32)>, M11InlineCodeError> {
        if self.escaped_prefix == 0 {
            return Ok(Some((self.start, self.len)));
        }
        let Some(len) = self.len.checked_sub(1).filter(|len| *len != 0) else {
            return Ok(None);
        };
        Ok(Some((
            self.start
                .checked_add(1)
                .ok_or(M11InlineCodeError::CoordinateOverflow)?,
            len,
        )))
    }

    pub(crate) const fn next_closer(self) -> Option<u32> {
        self.next_closer_plus_one.checked_sub(1)
    }

    pub(crate) const fn len(self) -> u32 {
        self.len
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineCodePollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineCodePoll {
    status: M11InlineCodePollStatus,
    transitions: usize,
}

impl M11InlineCodePoll {
    pub(crate) const fn status(self) -> M11InlineCodePollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineCodeReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11InlineCodeReleasePoll {
    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }

    pub(crate) const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineCodeError {
    Source(M11ParserPageError),
    Lex(M11InlineLexError),
    Scratch(M11InlineRadixError),
    ZeroFuel,
    PollLimitExceeded,
    CoordinateOverflow,
    InvalidState,
}

impl fmt::Display for M11InlineCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "inline code source failed: {error}"),
            Self::Lex(error) => write!(formatter, "inline code scan failed: {error}"),
            Self::Scratch(error) => write!(formatter, "inline code scratch failed: {error}"),
            Self::ZeroFuel => formatter.write_str("inline code poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("inline code poll exceeds its transition limit")
            }
            Self::CoordinateOverflow => {
                formatter.write_str("inline code coordinate or counter overflow")
            }
            Self::InvalidState => formatter.write_str("inline code job is in an invalid state"),
        }
    }
}

impl std::error::Error for M11InlineCodeError {}

impl From<M11ParserPageError> for M11InlineCodeError {
    fn from(value: M11ParserPageError) -> Self {
        Self::Source(value)
    }
}

impl From<M11InlineLexError> for M11InlineCodeError {
    fn from(value: M11InlineLexError) -> Self {
        Self::Lex(value)
    }
}

impl From<M11InlineRadixError> for M11InlineCodeError {
    fn from(value: M11InlineRadixError) -> Self {
        Self::Scratch(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CodePhase {
    Scanning,
    ReverseIndex,
    Cleanup,
    Complete,
    Faulted,
    Aborting,
    Aborted,
    Transferred,
}

/// One exact-source raw-run derivation.
///
/// Completion is not enough to drop this owner: callers must either transfer
/// the result with [`Self::take_output`] or explicitly abort and poll all
/// admitted scratch back to the document runtime.
pub(crate) struct M11InlineCodeJob {
    authority: Option<M11ParserSourceRangeAuthority>,
    source: SourceVersion,
    source_range: Range<u32>,
    scanner: M11InlineLexScanner,
    runs: Option<M11InlineRadixPages<CodeRun, RUN_PAGE_RECORDS>>,
    heads: Option<M11InlineRadixPages<u32, HEAD_PAGE_RECORDS>>,
    runs_reclaim_started: bool,
    heads_reclaim_started: bool,
    pending_run: Option<CodeRun>,
    run_count: u32,
    reverse_index: u32,
    phase: CodePhase,
}

impl fmt::Debug for M11InlineCodeJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineCodeJob")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("run_count", &self.run_count)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl M11InlineCodeJob {
    pub(crate) fn new(
        runtime: &DocumentRuntime,
        authority: M11ParserSourceRangeAuthority,
    ) -> Result<Self, M11InlineCodeError> {
        authority.validate(runtime)?;
        let source = authority.source();
        let source_range = authority.source_range();
        let source_range_u32 = u32::try_from(source_range.start)
            .map_err(|_| M11InlineCodeError::CoordinateOverflow)?
            ..u32::try_from(source_range.end)
                .map_err(|_| M11InlineCodeError::CoordinateOverflow)?;

        let runs = M11InlineRadixPages::new(source)?;
        let heads = M11InlineRadixPages::new(source)?;
        let scanner_cursor = authority.cursor(runtime)?;

        Ok(Self {
            authority: Some(authority),
            source,
            source_range: source_range_u32,
            scanner: M11InlineLexScanner::new(scanner_cursor),
            runs: Some(runs),
            heads: Some(heads),
            runs_reclaim_started: false,
            heads_reclaim_started: false,
            pending_run: None,
            run_count: 0,
            reverse_index: 0,
            phase: CodePhase::Scanning,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineCodePoll, M11InlineCodeError> {
        validate_fuel(fuel)?;
        if matches!(self.phase, CodePhase::Aborting | CodePhase::Aborted) {
            return Err(M11InlineCodeError::InvalidState);
        }
        if matches!(self.phase, CodePhase::Faulted | CodePhase::Transferred) {
            return Err(M11InlineCodeError::InvalidState);
        }
        if self.phase == CodePhase::Complete {
            return Ok(M11InlineCodePoll {
                status: M11InlineCodePollStatus::Complete,
                transitions: 0,
            });
        }
        if self.phase != CodePhase::Cleanup {
            self.authority_ref()?.validate(runtime)?;
        }

        let mut transitions = 0;
        while transitions < fuel {
            let step = match self.phase {
                CodePhase::Scanning => self.poll_scanning(runtime, fuel, &mut transitions),
                CodePhase::ReverseIndex => self.poll_reverse_index(runtime, &mut transitions),
                CodePhase::Cleanup => self.poll_cleanup(runtime, fuel, &mut transitions),
                CodePhase::Complete => {
                    return Ok(M11InlineCodePoll {
                        status: M11InlineCodePollStatus::Complete,
                        transitions,
                    });
                }
                CodePhase::Faulted
                | CodePhase::Aborting
                | CodePhase::Aborted
                | CodePhase::Transferred => {
                    return Err(M11InlineCodeError::InvalidState);
                }
            };
            if let Err(error) = step {
                self.scanner.cancel();
                self.phase = CodePhase::Faulted;
                return Err(error);
            }
        }
        Ok(M11InlineCodePoll {
            status: if self.phase == CodePhase::Complete {
                M11InlineCodePollStatus::Complete
            } else {
                M11InlineCodePollStatus::Pending
            },
            transitions,
        })
    }

    pub(crate) const fn lexical_receipt(&self) -> crate::M11InlineLexReceipt {
        self.scanner.receipt()
    }

    fn poll_scanning(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineCodeError> {
        if let Some(run) = self.pending_run.take() {
            let run_index = usize::try_from(self.run_count)
                .map_err(|_| M11InlineCodeError::CoordinateOverflow)?;
            self.runs_mut()?.set(runtime, run_index, run)?;
            self.run_count = self
                .run_count
                .checked_add(1)
                .ok_or(M11InlineCodeError::CoordinateOverflow)?;
            *transitions += 1;
            return Ok(());
        }

        let poll = self.scanner.poll(fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineCodeError::CoordinateOverflow)?;
        match poll.status() {
            M11InlineLexPollStatus::Pending => {}
            M11InlineLexPollStatus::Event(event) => {
                if let M11InlineLexEventKind::BacktickRun {
                    len,
                    escaped_prefix,
                } = event.kind()
                {
                    self.pending_run = Some(CodeRun {
                        start: event.start(),
                        len,
                        next_closer_plus_one: 0,
                        escaped_prefix: u8::from(escaped_prefix),
                        reserved: [0; 3],
                    });
                }
            }
            M11InlineLexPollStatus::Complete => {
                self.reverse_index = self.run_count;
                self.phase = CodePhase::ReverseIndex;
            }
        }
        Ok(())
    }

    fn poll_reverse_index(
        &mut self,
        runtime: &mut DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineCodeError> {
        if self.reverse_index == 0 {
            self.begin_cleanup()?;
            return Ok(());
        }
        let index = self.reverse_index - 1;
        let mut run = self.run(index)?;
        run.next_closer_plus_one = if let Some((_, opener_len)) = run.top_level_opener()? {
            self.heads_ref()?
                .get(
                    usize::try_from(opener_len)
                        .map_err(|_| M11InlineCodeError::CoordinateOverflow)?,
                )?
                .unwrap_or(0)
        } else {
            0
        };
        self.runs_mut()?.set(
            runtime,
            usize::try_from(index).map_err(|_| M11InlineCodeError::CoordinateOverflow)?,
            run,
        )?;
        self.heads_mut()?.set(
            runtime,
            usize::try_from(run.len).map_err(|_| M11InlineCodeError::CoordinateOverflow)?,
            index
                .checked_add(1)
                .ok_or(M11InlineCodeError::CoordinateOverflow)?,
        )?;
        self.reverse_index = index;
        *transitions += 1;
        Ok(())
    }

    fn begin_cleanup(&mut self) -> Result<(), M11InlineCodeError> {
        if !self.heads_reclaim_started {
            self.heads_mut()?.begin_reclaim()?;
            self.heads_reclaim_started = true;
        }
        self.phase = CodePhase::Cleanup;
        Ok(())
    }

    fn poll_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineCodeError> {
        if self.heads.is_some() {
            let poll = self
                .heads_mut()?
                .poll_reclaim(runtime, fuel - *transitions)?;
            *transitions = transitions
                .checked_add(poll.transitions())
                .ok_or(M11InlineCodeError::CoordinateOverflow)?;
            if poll.complete() {
                drop(self.heads.take());
            }
            return Ok(());
        }
        self.phase = CodePhase::Complete;
        Ok(())
    }

    pub(crate) fn take_output(&mut self) -> Option<M11InlineCodeRuns> {
        if self.phase != CodePhase::Complete {
            return None;
        }
        let raw_runs = self.runs.take()?;
        let authority = self.authority.take()?;
        self.phase = CodePhase::Transferred;
        Some(M11InlineCodeRuns {
            authority: Some(authority),
            source: self.source,
            source_range: self.source_range.clone(),
            raw_run_count: self.run_count,
            raw_runs: Some(raw_runs),
            raw_runs_reclaim_started: false,
        })
    }

    pub(crate) fn begin_abort(&mut self) -> Result<(), M11InlineCodeError> {
        if matches!(self.phase, CodePhase::Transferred | CodePhase::Aborted) {
            return Err(M11InlineCodeError::InvalidState);
        }
        if self.phase == CodePhase::Aborting {
            return Ok(());
        }
        self.scanner.cancel();
        if self.runs.is_some() && !self.runs_reclaim_started {
            self.runs_mut()?.begin_reclaim()?;
            self.runs_reclaim_started = true;
        }
        if self.heads.is_some() && !self.heads_reclaim_started {
            self.heads_mut()?.begin_reclaim()?;
            self.heads_reclaim_started = true;
        }
        self.phase = CodePhase::Aborting;
        Ok(())
    }

    pub(crate) fn poll_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineCodeReleasePoll, M11InlineCodeError> {
        validate_fuel(fuel)?;
        if self.phase != CodePhase::Aborting {
            return Err(M11InlineCodeError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if poll_and_drop(
                &mut self.runs,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            if poll_and_drop(
                &mut self.heads,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            self.phase = CodePhase::Aborted;
            return Ok(M11InlineCodeReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11InlineCodeReleasePoll {
            transitions,
            complete: false,
        })
    }

    fn run(&self, index: u32) -> Result<CodeRun, M11InlineCodeError> {
        self.runs_ref()?
            .get(usize::try_from(index).map_err(|_| M11InlineCodeError::CoordinateOverflow)?)?
            .ok_or(M11InlineCodeError::InvalidState)
    }

    fn authority_ref(&self) -> Result<&M11ParserSourceRangeAuthority, M11InlineCodeError> {
        self.authority
            .as_ref()
            .ok_or(M11InlineCodeError::InvalidState)
    }

    fn runs_ref(
        &self,
    ) -> Result<&M11InlineRadixPages<CodeRun, RUN_PAGE_RECORDS>, M11InlineCodeError> {
        self.runs.as_ref().ok_or(M11InlineCodeError::InvalidState)
    }

    fn runs_mut(
        &mut self,
    ) -> Result<&mut M11InlineRadixPages<CodeRun, RUN_PAGE_RECORDS>, M11InlineCodeError> {
        self.runs.as_mut().ok_or(M11InlineCodeError::InvalidState)
    }

    fn heads_ref(
        &self,
    ) -> Result<&M11InlineRadixPages<u32, HEAD_PAGE_RECORDS>, M11InlineCodeError> {
        self.heads.as_ref().ok_or(M11InlineCodeError::InvalidState)
    }

    fn heads_mut(
        &mut self,
    ) -> Result<&mut M11InlineRadixPages<u32, HEAD_PAGE_RECORDS>, M11InlineCodeError> {
        self.heads.as_mut().ok_or(M11InlineCodeError::InvalidState)
    }
}

impl Drop for M11InlineCodeJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(self.phase, CodePhase::Aborted | CodePhase::Transferred),
                "inline code jobs require output transfer or explicit fuelled abort"
            );
        }
    }
}

/// Move-only raw backtick runs with reverse-indexed same-length closers.
pub(crate) struct M11InlineCodeRuns {
    authority: Option<M11ParserSourceRangeAuthority>,
    source: SourceVersion,
    source_range: Range<u32>,
    raw_run_count: u32,
    raw_runs: Option<M11InlineRadixPages<CodeRun, RUN_PAGE_RECORDS>>,
    raw_runs_reclaim_started: bool,
}

impl M11InlineCodeRuns {
    #[cfg(test)]
    pub(crate) const fn raw_run_len(&self) -> u32 {
        self.raw_run_count
    }

    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    pub(crate) fn raw_run(&self, index: u32) -> Result<Option<CodeRun>, M11InlineCodeError> {
        if index >= self.raw_run_count {
            return Ok(None);
        }
        Ok(Some(
            self.raw_runs
                .as_ref()
                .ok_or(M11InlineCodeError::InvalidState)?
                .get(usize::try_from(index).map_err(|_| M11InlineCodeError::CoordinateOverflow)?)?
                .ok_or(M11InlineCodeError::InvalidState)?,
        ))
    }

    pub(crate) fn source_cursor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserRangeCursor, M11InlineCodeError> {
        if self.raw_runs_reclaim_started {
            return Err(M11InlineCodeError::InvalidState);
        }
        self.authority
            .as_ref()
            .ok_or(M11InlineCodeError::InvalidState)?
            .cursor(runtime)
            .map_err(Into::into)
    }

    pub(crate) fn validate_source(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11InlineCodeError> {
        self.authority
            .as_ref()
            .ok_or(M11InlineCodeError::InvalidState)?
            .validate(runtime)
            .map_err(Into::into)
    }

    pub(crate) fn source_authority(
        &self,
    ) -> Result<&M11ParserSourceRangeAuthority, M11InlineCodeError> {
        if self.raw_runs_reclaim_started {
            return Err(M11InlineCodeError::InvalidState);
        }
        self.authority
            .as_ref()
            .ok_or(M11InlineCodeError::InvalidState)
    }

    pub(crate) fn begin_release(&mut self) -> Result<(), M11InlineCodeError> {
        if self.raw_runs_reclaim_started {
            return Err(M11InlineCodeError::InvalidState);
        }
        self.raw_runs
            .as_mut()
            .ok_or(M11InlineCodeError::InvalidState)?
            .begin_reclaim()?;
        self.raw_runs_reclaim_started = true;
        Ok(())
    }

    pub(crate) fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineCodeReleasePoll, M11InlineCodeError> {
        validate_fuel(fuel)?;
        if !self.raw_runs_reclaim_started {
            return Err(M11InlineCodeError::InvalidState);
        }
        let poll = self
            .raw_runs
            .as_mut()
            .ok_or(M11InlineCodeError::InvalidState)?
            .poll_reclaim(runtime, fuel)?;
        if poll.complete() {
            drop(self.raw_runs.take());
        }
        Ok(M11InlineCodeReleasePoll {
            transitions: poll.transitions(),
            complete: poll.complete(),
        })
    }

    pub(crate) fn take_source_authority(&mut self) -> Option<M11ParserSourceRangeAuthority> {
        if self.raw_runs.is_some() {
            return None;
        }
        self.authority.take()
    }
}

impl Drop for M11InlineCodeRuns {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.raw_runs.is_none(),
                "inline code runs require explicit fuelled release"
            );
        }
    }
}

fn poll_and_drop<T: Copy + Default, const PAGE_RECORDS: usize>(
    pages: &mut Option<M11InlineRadixPages<T, PAGE_RECORDS>>,
    runtime: &mut DocumentRuntime,
    fuel: usize,
    transitions: &mut usize,
) -> Result<bool, M11InlineCodeError> {
    let Some(owner) = pages.as_mut() else {
        return Ok(false);
    };
    let poll: M11InlineRadixReclaimPoll = owner.poll_reclaim(runtime, fuel)?;
    *transitions = transitions
        .checked_add(poll.transitions())
        .ok_or(M11InlineCodeError::CoordinateOverflow)?;
    if poll.complete() {
        drop(pages.take());
    }
    Ok(true)
}

fn validate_fuel(fuel: usize) -> Result<(), M11InlineCodeError> {
    if fuel == 0 {
        return Err(M11InlineCodeError::ZeroFuel);
    }
    if fuel > M11_INLINE_CODE_MAX_POLL_TRANSITIONS {
        return Err(M11InlineCodeError::PollLimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flark_engine::parser_internal::{
        M11ParserPageError, M11ParserScratchError, M11ParserSourceRangeAuthority,
    };
    use flark_engine::{ArenaLimits, DocumentRuntimeConfig};

    #[derive(Debug)]
    struct Resolution {
        source_range: Range<u32>,
        runs: Vec<CodeRun>,
        maximum_retained_scratch_bytes: usize,
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close").complete {}
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
    }

    fn resolve(source_text: &str, fuel: usize) -> Resolution {
        resolve_in(source_text, 0..source_text.len(), fuel)
    }

    fn resolve_in(source_text: &str, source_range: Range<usize>, fuel: usize) -> Resolution {
        let mut runtime =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            source_range.clone(),
        )
        .expect("authority");
        let mut job = M11InlineCodeJob::new(&runtime, authority).expect("job");
        let mut maximum_retained_scratch_bytes = 0;
        loop {
            let poll = job.poll(&mut runtime, fuel).expect("code poll");
            assert!(poll.transitions() <= fuel);
            maximum_retained_scratch_bytes = maximum_retained_scratch_bytes
                .max(runtime.arena_metrics().reserved_external_payload_bytes);
            if poll.status() == M11InlineCodePollStatus::Complete {
                break;
            }
        }
        let mut output = job.take_output().expect("resolved output");
        assert_eq!(output.source(), source);
        let expected_source_range = u32::try_from(source_range.start).expect("range start")
            ..u32::try_from(source_range.end).expect("range end");
        assert_eq!(output.source_range(), expected_source_range);
        let mut runs = Vec::new();
        for index in 0..output.raw_run_len() {
            runs.push(
                output
                    .raw_run(index)
                    .expect("raw run read")
                    .expect("present raw run"),
            );
        }
        assert_eq!(
            output
                .raw_run(output.raw_run_len())
                .expect("past raw run end"),
            None
        );
        output.begin_release().expect("begin output release");
        loop {
            let poll = output
                .poll_release(&mut runtime, 1)
                .expect("output release");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        drop(output);
        drop(job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
        Resolution {
            source_range: expected_source_range,
            runs,
            maximum_retained_scratch_bytes,
        }
    }

    #[test]
    fn raw_runs_retain_nearest_same_length_closer_links() {
        let result = resolve("`` `_`_`b`", 1);
        assert_eq!(result.runs.len(), 5);
        assert_eq!(
            result
                .runs
                .iter()
                .map(|run| (run.raw_start(), run.len(), run.next_closer()))
                .collect::<Vec<_>>(),
            vec![
                (0, 2, None),
                (3, 1, Some(2)),
                (5, 1, Some(3)),
                (7, 1, Some(4)),
                (9, 1, None),
            ]
        );
    }

    #[test]
    fn backslash_escape_only_adjusts_top_level_openers() {
        let inside = resolve("Some `code\\` yep.", 1);
        assert_eq!(inside.runs.len(), 2);
        assert_eq!(inside.runs[0].top_level_opener().unwrap(), Some((5, 1)));
        assert_eq!(inside.runs[0].next_closer(), Some(1));
        assert_eq!(inside.runs[1].top_level_opener().unwrap(), None);

        let escaped_single = resolve(r"\`x`", 2);
        assert_eq!(escaped_single.runs.len(), 2);
        assert_eq!(escaped_single.runs[0].top_level_opener().unwrap(), None);
        assert_eq!(
            escaped_single.runs[1].top_level_opener().unwrap(),
            Some((3, 1))
        );

        let escaped_prefix_of_longer_run = resolve(r"\``x`", 2);
        assert_eq!(escaped_prefix_of_longer_run.runs.len(), 2);
        assert_eq!(
            escaped_prefix_of_longer_run.runs[0]
                .top_level_opener()
                .unwrap(),
            Some((2, 1))
        );
        assert_eq!(escaped_prefix_of_longer_run.runs[0].next_closer(), Some(1));

        let even_backslashes = resolve(r"\\`x`", 2);
        assert_eq!(even_backslashes.runs.len(), 2);
        assert_eq!(
            even_backslashes.runs[0].top_level_opener().unwrap(),
            Some((2, 1))
        );
    }

    #[test]
    fn middle_source_range_keeps_absolute_authority_and_relative_runs_separate() {
        let prefix = "OUTé:";
        let visible = "α `code` β";
        let source = format!("{prefix}{visible}:終OUT");
        let range = prefix.len()..prefix.len() + visible.len();
        let result = resolve_in(&source, range.clone(), 3);
        assert_eq!(
            result.source_range,
            u32::try_from(range.start).expect("start")..u32::try_from(range.end).expect("end")
        );
        assert_eq!(result.runs.len(), 2);
        assert_eq!(result.runs[0].raw_start(), 3);
        assert_eq!(result.runs[1].raw_start(), 8);
        assert_eq!(result.runs[0].next_closer(), Some(1));
    }

    #[test]
    fn raw_run_index_is_independent_of_poll_partition() {
        let source = "`a` ``b`` `c`";
        let expected = resolve(source, M11_INLINE_CODE_MAX_POLL_TRANSITIONS);
        for fuel in [1, 2, 7, 31, 257] {
            let actual = resolve(source, fuel);
            assert_eq!(actual.runs, expected.runs, "fuel={fuel}");
        }
        assert_eq!(
            expected
                .runs
                .iter()
                .map(|run| (run.raw_start(), run.len(), run.next_closer()))
                .collect::<Vec<_>>(),
            vec![
                (0, 1, Some(1)),
                (2, 1, Some(4)),
                (4, 2, Some(3)),
                (7, 2, None),
                (10, 1, Some(5)),
                (12, 1, None),
            ]
        );
    }

    #[test]
    fn backtick_runs_above_the_pinned_donor_limit_are_indexed_exactly() {
        let delimiter = "`".repeat(81);
        let source = format!("{delimiter}x{delimiter}");
        let result = resolve(&source, 7);
        assert_eq!(result.runs.len(), 2);
        assert_eq!(result.runs[0].raw_start(), 0);
        assert_eq!(result.runs[0].len(), 81);
        assert_eq!(result.runs[0].next_closer(), Some(1));
        assert_eq!(result.runs[1].raw_start(), 82);
        assert_eq!(result.runs[1].len(), 81);
    }

    #[test]
    fn one_mib_source_keeps_every_poll_and_retained_scratch_bounded() {
        let prefix = "a".repeat(512 * 1024);
        let suffix = "β".repeat((512 * 1024 - 8) / 2);
        let source = format!("{prefix}`code`{suffix}");
        assert!(source.len() >= 1_048_570);
        let result = resolve(&source, M11_INLINE_CODE_MAX_POLL_TRANSITIONS);
        assert_eq!(result.runs.len(), 2);
        assert_eq!(result.runs[0].raw_start(), prefix.len() as u32);
        assert_eq!(result.runs[0].next_closer(), Some(1));
        assert_eq!(result.runs[1].raw_start(), prefix.len() as u32 + 5);
        assert!(
            result.maximum_retained_scratch_bytes < 64 * 1024,
            "retained {} bytes",
            result.maximum_retained_scratch_bytes
        );
    }

    #[test]
    fn one_mib_dense_delimiters_stay_inside_the_shared_scratch_ceiling() {
        let source = "`x` ".repeat(256 * 1024);
        assert_eq!(source.len(), 1024 * 1024);
        let result = resolve(&source, M11_INLINE_CODE_MAX_POLL_TRANSITIONS);
        assert_eq!(result.runs.len(), 512 * 1024);
        assert_eq!(result.runs[0].raw_start(), 0);
        assert_eq!(result.runs[0].next_closer(), Some(1));
        assert_eq!(
            result.runs.last().expect("last").raw_start(),
            source.len() as u32 - 2
        );
        assert!(
            result.maximum_retained_scratch_bytes < 32 * 1024 * 1024,
            "retained {} bytes",
            result.maximum_retained_scratch_bytes
        );
    }

    #[test]
    fn partial_work_aborts_and_reclaims_with_fuel_one() {
        let source_text = "`x` ".repeat(5_000);
        let mut runtime =
            DocumentRuntime::new(&source_text, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source_text.len(),
        )
        .expect("authority");
        let mut job = M11InlineCodeJob::new(&runtime, authority).expect("job");
        while runtime.arena_metrics().reserved_external_payload_bytes == 0 {
            let poll = job.poll(&mut runtime, 257).expect("partial poll");
            assert_eq!(poll.status(), M11InlineCodePollStatus::Pending);
        }
        job.begin_abort().expect("begin abort");
        loop {
            let poll = job.poll_abort(&mut runtime, 1).expect("abort poll");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        drop(job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }

    #[test]
    fn runtime_and_source_authority_fail_before_progress_and_remain_abortable() {
        let source_text = "`code`";
        let mut owner =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("owner");
        let authority = M11ParserSourceRangeAuthority::new(
            &owner,
            owner.snapshot_current_source().expect("lease"),
            0..source_text.len(),
        )
        .expect("authority");
        let mut job = M11InlineCodeJob::new(&owner, authority).expect("job");

        let mut foreign =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("foreign");
        assert!(matches!(
            job.poll(&mut foreign, 1),
            Err(M11InlineCodeError::Source(M11ParserPageError::WrongRuntime))
        ));
        close_runtime(foreign);

        let current = owner.current_source_version().expect("source");
        owner
            .apply_edit(current, source_text.len()..source_text.len(), "!")
            .expect("advance source");
        assert!(matches!(
            job.poll(&mut owner, 1),
            Err(M11InlineCodeError::Source(
                M11ParserPageError::SourceAuthorityMismatch
            ))
        ));
        job.begin_abort().expect("begin abort");
        loop {
            if job.poll_abort(&mut owner, 1).expect("abort").complete() {
                break;
            }
        }
        drop(job);
        close_runtime(owner);
    }

    #[test]
    fn output_release_rejects_foreign_runtime_without_losing_ownership() {
        let source_text = "`code`";
        let mut owner =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("owner");
        let authority = M11ParserSourceRangeAuthority::new(
            &owner,
            owner.snapshot_current_source().expect("lease"),
            0..source_text.len(),
        )
        .expect("authority");
        let mut job = M11InlineCodeJob::new(&owner, authority).expect("job");
        loop {
            if job.poll(&mut owner, 31).expect("poll").status() == M11InlineCodePollStatus::Complete
            {
                break;
            }
        }
        let mut output = job.take_output().expect("output");
        output.begin_release().expect("begin release");

        let mut foreign =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("foreign");
        assert!(matches!(
            output.poll_release(&mut foreign, 31),
            Err(M11InlineCodeError::Scratch(M11InlineRadixError::Scratch(
                M11ParserScratchError::WrongRuntime
            )))
        ));
        assert_eq!(foreign.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(foreign);

        loop {
            if output
                .poll_release(&mut owner, 1)
                .expect("owner retry")
                .complete()
            {
                break;
            }
        }
        let authority = output
            .take_source_authority()
            .expect("released candidate pages hand the source baton onward");
        let mut replay = authority.cursor(&owner).expect("baton cursor");
        replay.cancel();
        drop(replay);
        drop(authority);
        drop(output);
        drop(job);
        assert_eq!(owner.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(owner);
    }

    #[test]
    fn mutation_failure_is_terminal_but_explicit_abort_still_reclaims() {
        let source_text = "`code`";
        let config = DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_live_payload_bytes: 1_024,
                ..ArenaLimits::default()
            },
            ..DocumentRuntimeConfig::default()
        };
        let mut runtime = DocumentRuntime::new(source_text, config).expect("runtime");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("lease"),
            0..source_text.len(),
        )
        .expect("authority");
        let mut job = M11InlineCodeJob::new(&runtime, authority).expect("job");
        let error = job
            .poll(&mut runtime, M11_INLINE_CODE_MAX_POLL_TRANSITIONS)
            .expect_err("first admitted run page exceeds the budget");
        assert!(matches!(
            error,
            M11InlineCodeError::Scratch(M11InlineRadixError::Scratch(error))
                if error.is_resource_limit()
        ));
        assert!(matches!(
            job.poll(&mut runtime, 1),
            Err(M11InlineCodeError::InvalidState)
        ));

        job.begin_abort().expect("begin abort");
        loop {
            if job.poll_abort(&mut runtime, 1).expect("abort").complete() {
                break;
            }
        }
        drop(job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }
}
