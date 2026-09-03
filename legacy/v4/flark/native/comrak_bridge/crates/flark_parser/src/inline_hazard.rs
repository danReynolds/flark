//! Bounded whole-leaf veto for unsupported lexical hazards outside resolved
//! opaque spans.
//!
//! This stage does not decide HTML, links, GFM tilde interaction, or general
//! text transformations. Parser-cooked character references, supported
//! backslash escapes, and exact unindented hard line breaks are non-hazard
//! lexical events and pass through here. An indented hard-break continuation
//! remains fail-closed until its discarded indentation can be projected
//! exactly. Until the remaining grammar stages exist, the
//! correctness-preserving answer is to fail the leaf closed when an unresolved
//! lexical candidate is not fully shielded by a code span or accepted angle
//! autolink.

use std::fmt;
use std::ops::Range;

use flark_engine::{DocumentRuntime, SourceVersion};

use crate::inline_autolink::{M11InlineAutolinkError, M11InlineOpaqueCandidates};
use crate::inline_direct::{M11InlineDirectCandidates, M11InlineDirectError};
use crate::inline_lex::{
    M11InlineLexError, M11InlineLexEvent, M11InlineLexEventKind, M11InlineLexHazardKind,
    M11InlineLexPollStatus, M11InlineLexScanner, M11_INLINE_LEX_MAX_POLL_TRANSITIONS,
};

pub(crate) const M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS: usize =
    M11_INLINE_LEX_MAX_POLL_TRANSITIONS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineHazardDisposition {
    Clean,
    Unsupported {
        kind: M11InlineLexHazardKind,
        start: u32,
        end: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineHazardResult {
    source: SourceVersion,
    source_range: Range<u32>,
    disposition: M11InlineHazardDisposition,
}

impl M11InlineHazardResult {
    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    pub(crate) const fn disposition(&self) -> M11InlineHazardDisposition {
        self.disposition
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineHazardPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineHazardPoll {
    status: M11InlineHazardPollStatus,
    transitions: usize,
}

impl M11InlineHazardPoll {
    pub(crate) const fn status(self) -> M11InlineHazardPollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineHazardError {
    Opaque(M11InlineAutolinkError),
    Direct(M11InlineDirectError),
    Lex(M11InlineLexError),
    ZeroFuel,
    PollLimitExceeded,
    InvalidState,
}

impl fmt::Display for M11InlineHazardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opaque(error) => {
                write!(formatter, "inline hazard opaque candidates failed: {error}")
            }
            Self::Direct(error) => {
                write!(
                    formatter,
                    "inline hazard direct-link candidates failed: {error}"
                )
            }
            Self::Lex(error) => write!(formatter, "inline hazard scan failed: {error}"),
            Self::ZeroFuel => formatter.write_str("inline hazard poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("inline hazard poll exceeds its transition limit")
            }
            Self::InvalidState => formatter.write_str("inline hazard job is in an invalid state"),
        }
    }
}

impl std::error::Error for M11InlineHazardError {}

impl From<M11InlineAutolinkError> for M11InlineHazardError {
    fn from(value: M11InlineAutolinkError) -> Self {
        Self::Opaque(value)
    }
}

impl From<M11InlineDirectError> for M11InlineHazardError {
    fn from(value: M11InlineDirectError) -> Self {
        Self::Direct(value)
    }
}

impl From<M11InlineLexError> for M11InlineHazardError {
    fn from(value: M11InlineLexError) -> Self {
        Self::Lex(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HazardPhase {
    Active,
    Complete,
    Faulted,
    Cancelled,
    Transferred,
}

pub(crate) struct M11InlineHazardJob {
    source: SourceVersion,
    source_range: Range<u32>,
    scanner: M11InlineLexScanner,
    opaque_index: u32,
    direct_syntax: Vec<Range<u32>>,
    direct_index: usize,
    direct_bare_ownership: Vec<Range<u32>>,
    direct_bare_index: usize,
    exhaustive_bracket_classification: bool,
    pending_event: Option<M11InlineLexEvent>,
    disposition: Option<M11InlineHazardDisposition>,
    phase: HazardPhase,
}

impl M11InlineHazardJob {
    #[cfg(test)]
    pub(crate) fn new(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
    ) -> Result<Self, M11InlineHazardError> {
        Self::new_with_syntax(runtime, opaque, Vec::new(), Vec::new(), false)
    }

    pub(crate) fn new_with_direct(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        direct: &M11InlineDirectCandidates,
    ) -> Result<Self, M11InlineHazardError> {
        direct.validate_source(runtime, opaque)?;
        Self::new_with_syntax(
            runtime,
            opaque,
            direct.syntax_ranges().collect(),
            direct.fact_ranges().collect(),
            direct.exhaustive_bracket_classification(),
        )
    }

    fn new_with_syntax(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        direct_syntax: Vec<Range<u32>>,
        direct_bare_ownership: Vec<Range<u32>>,
        exhaustive_bracket_classification: bool,
    ) -> Result<Self, M11InlineHazardError> {
        opaque.validate_source(runtime)?;
        Ok(Self {
            source: opaque.source(),
            source_range: opaque.source_range(),
            scanner: M11InlineLexScanner::new(opaque.source_cursor(runtime)?),
            opaque_index: 0,
            direct_syntax,
            direct_index: 0,
            direct_bare_ownership,
            direct_bare_index: 0,
            exhaustive_bracket_classification,
            pending_event: None,
            disposition: None,
            phase: HazardPhase::Active,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        fuel: usize,
    ) -> Result<M11InlineHazardPoll, M11InlineHazardError> {
        validate_fuel(fuel)?;
        if self.phase == HazardPhase::Complete {
            return Ok(M11InlineHazardPoll {
                status: M11InlineHazardPollStatus::Complete,
                transitions: 0,
            });
        }
        if self.phase != HazardPhase::Active {
            return Err(M11InlineHazardError::InvalidState);
        }
        opaque.validate_source(runtime)?;
        if opaque.source() != self.source || opaque.source_range() != self.source_range {
            return Err(M11InlineHazardError::InvalidState);
        }

        let mut transitions = 0;
        while transitions < fuel {
            let step = if self.pending_event.is_some() {
                self.process_pending(opaque, &mut transitions)
            } else {
                self.poll_scanner(fuel, &mut transitions)
            };
            if let Err(error) = step {
                self.scanner.cancel();
                self.phase = HazardPhase::Faulted;
                return Err(error);
            }
            if self.phase == HazardPhase::Complete {
                return Ok(M11InlineHazardPoll {
                    status: M11InlineHazardPollStatus::Complete,
                    transitions,
                });
            }
        }
        Ok(M11InlineHazardPoll {
            status: M11InlineHazardPollStatus::Pending,
            transitions,
        })
    }

    fn poll_scanner(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineHazardError> {
        let poll = self.scanner.poll(fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineHazardError::InvalidState)?;
        match poll.status() {
            M11InlineLexPollStatus::Pending => {}
            M11InlineLexPollStatus::Event(event) => self.pending_event = Some(event),
            M11InlineLexPollStatus::Complete => {
                self.disposition = Some(M11InlineHazardDisposition::Clean);
                self.phase = HazardPhase::Complete;
            }
        }
        Ok(())
    }

    fn process_pending(
        &mut self,
        opaque: &M11InlineOpaqueCandidates,
        transitions: &mut usize,
    ) -> Result<(), M11InlineHazardError> {
        let event = self
            .pending_event
            .ok_or(M11InlineHazardError::InvalidState)?;
        let kind = match event.kind() {
            M11InlineLexEventKind::Hazard(kind) => kind,
            M11InlineLexEventKind::HardLineBreak {
                continuation_indented: true,
                ..
            } => M11InlineLexHazardKind::HardBreakCandidate,
            M11InlineLexEventKind::BackslashEscape
            | M11InlineLexEventKind::CharacterReference { .. }
            | M11InlineLexEventKind::HardLineBreak {
                continuation_indented: false,
                ..
            }
            | M11InlineLexEventKind::BacktickRun { .. }
            | M11InlineLexEventKind::EmphasisRun { .. }
            | M11InlineLexEventKind::StrikethroughRun { .. } => {
                self.pending_event = None;
                *transitions += 1;
                return Ok(());
            }
        };
        if let Some(candidate) = opaque.candidate(self.opaque_index)? {
            let range = candidate.relative_range();
            if range.end <= event.start() {
                self.opaque_index = self
                    .opaque_index
                    .checked_add(1)
                    .ok_or(M11InlineHazardError::InvalidState)?;
                *transitions += 1;
                return Ok(());
            }
        }
        if self
            .direct_syntax
            .get(self.direct_index)
            .is_some_and(|range| range.end <= event.start())
        {
            self.direct_index = self
                .direct_index
                .checked_add(1)
                .ok_or(M11InlineHazardError::InvalidState)?;
            *transitions += 1;
            return Ok(());
        }
        if kind == M11InlineLexHazardKind::BareAutolinkCandidate
            && self
                .direct_bare_ownership
                .get(self.direct_bare_index)
                .is_some_and(|range| range.end <= event.start())
        {
            self.direct_bare_index = self
                .direct_bare_index
                .checked_add(1)
                .ok_or(M11InlineHazardError::InvalidState)?;
            *transitions += 1;
            return Ok(());
        }
        let candidate_range = opaque
            .candidate(self.opaque_index)?
            .map(|candidate| candidate.relative_range());
        let direct_range = self.direct_syntax.get(self.direct_index);
        let direct_bare_range = (kind == M11InlineLexHazardKind::BareAutolinkCandidate)
            .then(|| self.direct_bare_ownership.get(self.direct_bare_index))
            .flatten();
        let opaque_shielded = candidate_range
            .as_ref()
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        let direct_shielded = direct_range
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        let direct_bare_shielded = direct_bare_range
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        let opaque_partially_overlaps = candidate_range.as_ref().is_some_and(|range| {
            range.start < event.end() && event.start() < range.end && !opaque_shielded
        });
        let direct_partially_overlaps = direct_range.is_some_and(|range| {
            range.start < event.end() && event.start() < range.end && !direct_shielded
        });
        let direct_bare_partially_overlaps = direct_bare_range.is_some_and(|range| {
            range.start < event.end() && event.start() < range.end && !direct_bare_shielded
        });
        if opaque_partially_overlaps || direct_partially_overlaps || direct_bare_partially_overlaps
        {
            return Err(M11InlineHazardError::InvalidState);
        }
        self.pending_event = None;
        *transitions += 1;
        let definitively_literal_bracket = self.exhaustive_bracket_classification
            && kind == M11InlineLexHazardKind::LinkOrImageCandidate;
        if !opaque_shielded
            && !direct_shielded
            && !direct_bare_shielded
            && !definitively_literal_bracket
        {
            self.scanner.cancel();
            self.disposition = Some(M11InlineHazardDisposition::Unsupported {
                kind,
                start: event.start(),
                end: event.end(),
            });
            self.phase = HazardPhase::Complete;
        }
        Ok(())
    }

    pub(crate) fn take_result(&mut self) -> Option<M11InlineHazardResult> {
        if self.phase != HazardPhase::Complete {
            return None;
        }
        let disposition = self.disposition.take()?;
        self.phase = HazardPhase::Transferred;
        Some(M11InlineHazardResult {
            source: self.source,
            source_range: self.source_range.clone(),
            disposition,
        })
    }

    pub(crate) fn cancel(&mut self) {
        if matches!(self.phase, HazardPhase::Active | HazardPhase::Faulted) {
            self.scanner.cancel();
            self.phase = HazardPhase::Cancelled;
        }
    }
}

impl Drop for M11InlineHazardJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    HazardPhase::Cancelled | HazardPhase::Transferred
                ),
                "inline hazard jobs require result transfer or explicit cancellation"
            );
        }
    }
}

fn validate_fuel(fuel: usize) -> Result<(), M11InlineHazardError> {
    if fuel == 0 {
        return Err(M11InlineHazardError::ZeroFuel);
    }
    if fuel > M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS {
        return Err(M11InlineHazardError::PollLimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_autolink::{
        M11InlineAutolinkError, M11InlineAutolinkJob, M11InlineAutolinkPollStatus,
        M11InlineOpaqueCandidates, M11InlineOpaquePollStatus, M11InlineOpaqueResolveJob,
    };
    use crate::inline_code::{
        M11InlineCodeError, M11InlineCodeJob, M11InlineCodePollStatus, M11InlineCodeRuns,
    };
    use flark_engine::parser_internal::{M11ParserRangeError, M11ParserSourceRangeAuthority};
    use flark_engine::{DocumentRuntimeConfig, SourceVersion};

    struct Fixture {
        runtime: DocumentRuntime,
        code_job: M11InlineCodeJob,
        autolink_job: M11InlineAutolinkJob,
        opaque: M11InlineOpaqueCandidates,
    }

    impl Fixture {
        fn new(source_text: &str, source_range: Range<usize>) -> Self {
            let mut runtime = DocumentRuntime::new(source_text, DocumentRuntimeConfig::default())
                .expect("runtime");
            let authority = M11ParserSourceRangeAuthority::new(
                &runtime,
                runtime.snapshot_current_source().expect("source lease"),
                source_range,
            )
            .expect("source authority");
            let mut code_job = M11InlineCodeJob::new(&runtime, authority).expect("code job");
            loop {
                let poll = code_job
                    .poll(&mut runtime, M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS)
                    .expect("code poll");
                if poll.status() == M11InlineCodePollStatus::Complete {
                    break;
                }
            }
            let code = code_job.take_output().expect("code output");
            let mut autolink_job =
                M11InlineAutolinkJob::new(&runtime, &code).expect("autolink job");
            loop {
                let poll = autolink_job
                    .poll(&mut runtime, M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS)
                    .expect("autolink poll");
                if poll.status() == M11InlineAutolinkPollStatus::Complete {
                    break;
                }
            }
            let mut code = Some(code);
            let mut opaque_job =
                M11InlineOpaqueResolveJob::take_new(&runtime, &mut code, &mut autolink_job)
                    .expect("opaque job");
            assert!(code.is_none());
            loop {
                let poll = opaque_job
                    .poll(&mut runtime, M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS)
                    .expect("opaque poll");
                if poll.status() == M11InlineOpaquePollStatus::Complete {
                    break;
                }
            }
            let opaque = opaque_job.take_output().expect("opaque output");
            drop(opaque_job);
            Self {
                runtime,
                code_job,
                autolink_job,
                opaque,
            }
        }

        fn finish_hazard(&mut self, fuel: usize) -> (M11InlineHazardResult, Vec<usize>) {
            let mut job = M11InlineHazardJob::new(&self.runtime, &self.opaque).expect("hazard job");
            let mut receipts = Vec::new();
            loop {
                let poll = job
                    .poll(&self.runtime, &self.opaque, fuel)
                    .expect("hazard poll");
                assert!(poll.transitions() <= fuel);
                receipts.push(poll.transitions());
                if poll.status() == M11InlineHazardPollStatus::Complete {
                    break;
                }
            }
            let result = job.take_result().expect("hazard result");
            drop(job);
            (result, receipts)
        }

        fn close(mut self) {
            self.opaque.begin_release().expect("begin opaque release");
            loop {
                if self
                    .opaque
                    .poll_release(&mut self.runtime, 1)
                    .expect("opaque release")
                    .complete()
                {
                    break;
                }
            }
            drop(self.opaque);
            drop(self.autolink_job);
            drop(self.code_job);
            self.runtime.begin_close().expect("begin runtime close");
            while !self.runtime.poll_close(64).expect("runtime close").complete {}
            assert_eq!(
                self.runtime.arena_metrics().reserved_external_payload_bytes,
                0
            );
        }
    }

    fn complete_code(
        runtime: &mut DocumentRuntime,
        source_range: Range<usize>,
    ) -> (M11InlineCodeJob, M11InlineCodeRuns) {
        let authority = M11ParserSourceRangeAuthority::new(
            runtime,
            runtime.snapshot_current_source().expect("source lease"),
            source_range,
        )
        .expect("source authority");
        let mut job = M11InlineCodeJob::new(runtime, authority).expect("code job");
        loop {
            if job
                .poll(runtime, M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS)
                .expect("code poll")
                .status()
                == M11InlineCodePollStatus::Complete
            {
                break;
            }
        }
        let output = job.take_output().expect("code output");
        (job, output)
    }

    fn release_code(runtime: &mut DocumentRuntime, code: &mut M11InlineCodeRuns) {
        code.begin_release().expect("begin code release");
        loop {
            if code
                .poll_release(runtime, 1)
                .expect("code release")
                .complete()
            {
                break;
            }
        }
    }

    #[test]
    fn opaque_preflight_errors_leave_both_move_only_owners_reclaimable() {
        let source = "<ab:x> <cd:y>";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let (code_job_a, code_a) = complete_code(&mut runtime, 0..6);
        let (code_job_b, mut code_b) = complete_code(&mut runtime, 7..13);
        let mut autolink_job = M11InlineAutolinkJob::new(&runtime, &code_b).expect("autolink job");
        loop {
            if autolink_job
                .poll(&mut runtime, M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS)
                .expect("autolink poll")
                .status()
                == M11InlineAutolinkPollStatus::Complete
            {
                break;
            }
        }
        let mut code_a = Some(code_a);

        let mut foreign =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("foreign");
        assert!(matches!(
            M11InlineOpaqueResolveJob::take_new(&foreign, &mut code_a, &mut autolink_job),
            Err(M11InlineAutolinkError::Code(M11InlineCodeError::Source(
                M11ParserRangeError::WrongRuntime
            )))
        ));
        assert!(code_a.is_some());
        foreign.begin_close().expect("begin foreign close");
        while !foreign.poll_close(64).expect("foreign close").complete {}
        drop(foreign);

        assert!(matches!(
            M11InlineOpaqueResolveJob::take_new(&runtime, &mut code_a, &mut autolink_job),
            Err(M11InlineAutolinkError::InvalidState)
        ));
        assert!(code_a.is_some());

        let current = runtime.current_source_version().expect("source");
        runtime
            .apply_edit(current, source.len()..source.len(), "!")
            .expect("advance source");
        assert!(matches!(
            M11InlineOpaqueResolveJob::take_new(&runtime, &mut code_a, &mut autolink_job),
            Err(M11InlineAutolinkError::Code(M11InlineCodeError::Source(
                M11ParserRangeError::SourceAuthorityMismatch
            )))
        ));
        assert!(code_a.is_some());

        autolink_job.begin_abort().expect("begin autolink abort");
        loop {
            if autolink_job
                .poll_abort(&mut runtime, 1)
                .expect("autolink abort")
                .complete()
            {
                break;
            }
        }
        let mut code_a = code_a.take().expect("code a owner");
        release_code(&mut runtime, &mut code_a);
        release_code(&mut runtime, &mut code_b);
        drop(code_a);
        drop(code_b);
        drop(autolink_job);
        drop(code_job_a);
        drop(code_job_b);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("runtime close").complete {}
    }

    fn disposition(source: &str, fuel: usize) -> M11InlineHazardDisposition {
        let mut fixture = Fixture::new(source, 0..source.len());
        let result = fixture.finish_hazard(fuel).0;
        assert_eq!(
            result.source(),
            fixture.runtime.current_source_version().expect("source")
        );
        assert_eq!(result.source_range(), 0..source.len() as u32);
        let disposition = result.disposition();
        fixture.close();
        disposition
    }

    fn exhaustive_bracket_disposition(source: &str, fuel: usize) -> M11InlineHazardDisposition {
        let fixture = Fixture::new(source, 0..source.len());
        let mut job = M11InlineHazardJob::new_with_syntax(
            &fixture.runtime,
            &fixture.opaque,
            Vec::new(),
            Vec::new(),
            true,
        )
        .expect("certified bracket hazard job");
        loop {
            let polled = job
                .poll(&fixture.runtime, &fixture.opaque, fuel)
                .expect("certified bracket hazard poll");
            assert!(polled.transitions() <= fuel);
            if polled.status() == M11InlineHazardPollStatus::Complete {
                break;
            }
        }
        let disposition = job
            .take_result()
            .expect("certified bracket hazard result")
            .disposition();
        drop(job);
        fixture.close();
        disposition
    }

    #[test]
    fn code_and_accepted_angle_autolinks_are_the_only_shielding_map() {
        let source = "`[x] <tag> &amp; ~~a~~ name@example.test https://x \\\\*  \n b`";
        assert_eq!(disposition(source, 1), M11InlineHazardDisposition::Clean);
    }

    #[test]
    fn exact_unindented_hard_line_breaks_are_resolved_before_the_hazard_veto() {
        for source in ["a\\\nb", "a  \nb", "a\\\rb", "a  \r\nb"] {
            assert_eq!(
                disposition(source, 1),
                M11InlineHazardDisposition::Clean,
                "{source:?}"
            );
        }
    }

    #[test]
    fn tilde_runs_are_owned_by_the_shared_delimiter_stage() {
        for source in ["~x~", "~~x~~", "~~~literal~~~", "~~unclosed"] {
            assert_eq!(
                disposition(source, 1),
                M11InlineHazardDisposition::Clean,
                "{source:?}"
            );
        }
    }

    #[test]
    fn first_unshielded_candidate_fails_the_leaf_closed() {
        let cases = [
            ("[x]", M11InlineLexHazardKind::LinkOrImageCandidate, 0..1),
            ("<x", M11InlineLexHazardKind::HtmlCandidate, 0..1),
            ("a  \n b", M11InlineLexHazardKind::HardBreakCandidate, 1..4),
            (
                "name@example.test",
                M11InlineLexHazardKind::BareAutolinkCandidate,
                4..5,
            ),
            (
                "https://example.test",
                M11InlineLexHazardKind::BareAutolinkCandidate,
                0..8,
            ),
        ];
        for (source, kind, range) in cases {
            assert_eq!(
                disposition(source, 7),
                M11InlineHazardDisposition::Unsupported {
                    kind,
                    start: range.start,
                    end: range.end,
                },
                "{source:?}"
            );
        }
    }

    #[test]
    fn exhaustive_bracket_certificate_keeps_literal_brackets_without_weakening_other_hazards() {
        for source in ["[missing] *styled*", "] unmatched [", "![missing]"] {
            assert_eq!(
                exhaustive_bracket_disposition(source, 1),
                M11InlineHazardDisposition::Clean,
                "{source:?}"
            );
        }
        assert_eq!(
            exhaustive_bracket_disposition("[missing] <tag>", 1),
            M11InlineHazardDisposition::Unsupported {
                kind: M11InlineLexHazardKind::HtmlCandidate,
                start: 10,
                end: 11,
            }
        );
    }

    #[test]
    fn html_vetoes_code_while_accepted_autolink_shields_internal_ticks() {
        assert_eq!(
            disposition("<a href=\"`\">`", 31),
            M11InlineHazardDisposition::Unsupported {
                kind: M11InlineLexHazardKind::HtmlCandidate,
                start: 0,
                end: 1,
            }
        );
        assert_eq!(
            disposition("<https://foo.bar.`baz>`", 31),
            M11InlineHazardDisposition::Clean
        );
    }

    #[test]
    fn supported_escape_is_not_a_hazard_beside_unmatched_backticks() {
        assert_eq!(disposition(r"\``x`", 2), M11InlineHazardDisposition::Clean);
    }

    #[test]
    fn result_is_fuel_invariant() {
        let source = "`inside & [x]` plain *em* then name@example.test";
        let expected = disposition(source, 1);
        for fuel in [2, 7, 31, M11_INLINE_HAZARD_MAX_POLL_TRANSITIONS] {
            assert_eq!(disposition(source, fuel), expected, "fuel {fuel}");
        }
    }

    #[test]
    fn middle_range_preserves_absolute_source_authority_and_relative_events() {
        let prefix = "outside:";
        let visible = "`safe &` then <x";
        let source = format!("{prefix}{visible}:outside");
        let start = prefix.len();
        let end = start + visible.len();
        let mut fixture = Fixture::new(&source, start..end);
        let result = fixture.finish_hazard(1).0;
        assert_eq!(result.source_range(), start as u32..end as u32);
        assert_eq!(
            result.disposition(),
            M11InlineHazardDisposition::Unsupported {
                kind: M11InlineLexHazardKind::HtmlCandidate,
                start: 14,
                end: 15,
            }
        );
        fixture.close();
    }

    #[test]
    fn wrong_runtime_and_stale_source_fail_without_losing_cancellation() {
        let source = "`safe` then <x";
        let mut fixture = Fixture::new(source, 0..source.len());
        let mut job =
            M11InlineHazardJob::new(&fixture.runtime, &fixture.opaque).expect("hazard job");
        let foreign =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("foreign");
        assert!(matches!(
            job.poll(&foreign, &fixture.opaque, 1),
            Err(M11InlineHazardError::Opaque(_))
        ));
        let mut foreign = foreign;
        foreign.begin_close().expect("foreign close");
        while !foreign.poll_close(64).expect("foreign poll").complete {}

        let source_version: SourceVersion =
            fixture.runtime.current_source_version().expect("source");
        fixture
            .runtime
            .apply_edit(source_version, source.len()..source.len(), "!")
            .expect("advance source");
        assert!(matches!(
            job.poll(&fixture.runtime, &fixture.opaque, 1),
            Err(M11InlineHazardError::Opaque(_))
        ));
        job.cancel();
        drop(job);
        fixture.close();
    }

    #[test]
    fn cancellation_is_explicit_and_does_not_own_opaque_pages() {
        let source = "`x` ".repeat(20_000);
        let fixture = Fixture::new(&source, 0..source.len());
        let retained_before = fixture
            .runtime
            .arena_metrics()
            .reserved_external_payload_bytes;
        let mut job =
            M11InlineHazardJob::new(&fixture.runtime, &fixture.opaque).expect("hazard job");
        let poll = job
            .poll(&fixture.runtime, &fixture.opaque, 1)
            .expect("partial poll");
        assert_eq!(poll.status(), M11InlineHazardPollStatus::Pending);
        assert_eq!(poll.transitions(), 1);
        job.cancel();
        drop(job);
        assert_eq!(
            fixture
                .runtime
                .arena_metrics()
                .reserved_external_payload_bytes,
            retained_before
        );
        fixture.close();
    }
}
