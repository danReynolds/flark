//! Build-local persistent References authority for the exact parser.
//!
//! This actor deliberately owns no Markdown recognition. It accepts only
//! parser-authenticated, normalized, cooked occurrences. Each occurrence is
//! appended to the existing canonical reference root and then installed into
//! the exact first-winner index before another occurrence may enter. Both
//! structures therefore advance behind one recoverable actor boundary.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;

use crate::candidate_manifest::{
    CandidateAuthority, ManifestError, ReferenceReserve, RoleMetadata, StrongIdentity,
};
use crate::document::{DocumentRuntime, DocumentRuntimeError};
use crate::reference_root::{
    AuthoritativeReferenceFact, AuthoritativeReferenceFactStart, ReferenceBuildPoll,
    ReferenceRootBuilder, ReferenceRootError, ReferenceRootLimits, ReferenceSourceRange,
    ReferenceSubtreeRoot, ReferenceWinnerIndex, ReferenceWinnerIndexJournal,
    ReferenceWinnerIndexReclaimer, StreamedReferenceValueKind,
};
use crate::storage::{
    ArenaBuildOwner, ArenaBuildSession, ArenaError, CandidateBuild, CandidateSeal,
    CommittedArenaRoot,
};
use crate::{CandidateGeneration, ExactUnchangedPrefixWitness, SourceVersion};

const JOURNAL_ANCHOR: [u8; 4] = [0xe0, 1, 0, 0];

#[derive(Debug)]
enum ErrorInner {
    InvalidState,
    WrongRuntime,
    SourceAuthorityMismatch,
    ZeroFuel,
    Document(DocumentRuntimeError),
    Arena(ArenaError),
    Manifest(ManifestError),
    Reference(ReferenceRootError),
}

/// Opaque failure from the parser-local persistent References journal.
#[derive(Debug)]
pub struct M11ReferenceJournalError(ErrorInner);

impl M11ReferenceJournalError {
    #[must_use]
    pub fn is_invalid_state(&self) -> bool {
        matches!(self.0, ErrorInner::InvalidState)
    }

    #[must_use]
    pub fn is_wrong_runtime(&self) -> bool {
        matches!(self.0, ErrorInner::WrongRuntime)
    }

    #[must_use]
    pub fn is_source_authority_mismatch(&self) -> bool {
        matches!(self.0, ErrorInner::SourceAuthorityMismatch)
    }

    #[must_use]
    pub fn is_resource_limit(&self) -> bool {
        matches!(
            self.0,
            ErrorInner::Arena(
                ArenaError::CapacityExceeded
                    | ArenaError::PayloadTooLarge
                    | ArenaError::TooManyChildren
                    | ArenaError::PayloadBudgetExceeded
                    | ArenaError::BuildCapacityExceeded
                    | ArenaError::AllocationFailed
            ) | ErrorInner::Reference(
                ReferenceRootError::InvalidLimits
                    | ReferenceRootError::FactTooLarge
                    | ReferenceRootError::OccurrenceLimit
                    | ReferenceRootError::CapacityPreflight
                    | ReferenceRootError::Arena(
                        ArenaError::CapacityExceeded
                            | ArenaError::PayloadTooLarge
                            | ArenaError::TooManyChildren
                            | ArenaError::PayloadBudgetExceeded
                            | ArenaError::BuildCapacityExceeded
                            | ArenaError::AllocationFailed
                    )
            )
        )
    }
}

impl fmt::Display for M11ReferenceJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ErrorInner::InvalidState => formatter.write_str("reference journal is not ready"),
            ErrorInner::WrongRuntime => formatter.write_str("reference journal crossed runtimes"),
            ErrorInner::SourceAuthorityMismatch => {
                formatter.write_str("reference journal source authority changed")
            }
            ErrorInner::ZeroFuel => formatter.write_str("reference journal requires nonzero fuel"),
            ErrorInner::Document(error) => error.fmt(formatter),
            ErrorInner::Arena(error) => error.fmt(formatter),
            ErrorInner::Manifest(error) => error.fmt(formatter),
            ErrorInner::Reference(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11ReferenceJournalError {}

impl From<ArenaError> for M11ReferenceJournalError {
    fn from(error: ArenaError) -> Self {
        Self(ErrorInner::Arena(error))
    }
}

impl From<DocumentRuntimeError> for M11ReferenceJournalError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self(ErrorInner::Document(error))
    }
}

impl From<ManifestError> for M11ReferenceJournalError {
    fn from(error: ManifestError) -> Self {
        Self(ErrorInner::Manifest(error))
    }
}

impl From<ReferenceRootError> for M11ReferenceJournalError {
    fn from(error: ReferenceRootError) -> Self {
        Self(ErrorInner::Reference(error))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11ReferenceJournalRange {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl M11ReferenceJournalRange {
    #[must_use]
    pub const fn new(bytes: Range<u64>, utf16: Range<u64>) -> Self {
        Self { bytes, utf16 }
    }
}

/// One parser-authenticated occurrence. Recognition, normalization and value
/// cooking remain outside the storage actor.
pub struct M11ReferenceJournalOccurrence {
    source: M11ReferenceJournalRange,
    label_source: M11ReferenceJournalRange,
    destination_source: M11ReferenceJournalRange,
    title_source: Option<M11ReferenceJournalRange>,
    normalized_label: Box<[u8]>,
    cooked_destination: Box<[u8]>,
    cooked_title: Option<Box<[u8]>>,
}

impl M11ReferenceJournalOccurrence {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        source: M11ReferenceJournalRange,
        label_source: M11ReferenceJournalRange,
        destination_source: M11ReferenceJournalRange,
        title_source: Option<M11ReferenceJournalRange>,
        normalized_label: impl Into<Box<[u8]>>,
        cooked_destination: impl Into<Box<[u8]>>,
        cooked_title: Option<Box<[u8]>>,
    ) -> Self {
        Self {
            source,
            label_source,
            destination_source,
            title_source,
            normalized_label: normalized_label.into(),
            cooked_destination: cooked_destination.into(),
            cooked_title,
        }
    }
}

pub struct M11ReferenceJournalOccurrenceStart {
    source: M11ReferenceJournalRange,
    label_source: M11ReferenceJournalRange,
    destination_source: M11ReferenceJournalRange,
    title_source: Option<M11ReferenceJournalRange>,
    normalized_label: Box<[u8]>,
    destination_len: usize,
    title_len: Option<usize>,
}

impl M11ReferenceJournalOccurrenceStart {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        source: M11ReferenceJournalRange,
        label_source: M11ReferenceJournalRange,
        destination_source: M11ReferenceJournalRange,
        title_source: Option<M11ReferenceJournalRange>,
        normalized_label: impl Into<Box<[u8]>>,
        destination_len: usize,
        title_len: Option<usize>,
    ) -> Self {
        Self {
            source,
            label_source,
            destination_source,
            title_source,
            normalized_label: normalized_label.into(),
            destination_len,
            title_len,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceJournalValueKind {
    Destination,
    Title,
}

impl M11ReferenceJournalValueKind {
    const fn engine(self) -> StreamedReferenceValueKind {
        match self {
            Self::Destination => StreamedReferenceValueKind::Destination,
            Self::Title => StreamedReferenceValueKind::Title,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceJournalStatus {
    NeedsInput,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceJournalPoll {
    status: M11ReferenceJournalStatus,
    transitions: usize,
}

impl M11ReferenceJournalPoll {
    #[must_use]
    pub const fn status(self) -> M11ReferenceJournalStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JournalPhase {
    Accepting,
    Building,
    Streaming,
    Indexing,
    DrainingPage,
    Finishing,
    ReadyForSeal,
    Sealing,
    Complete,
    Cancelled,
    Failed,
}

/// Persistent References journal built alongside the writer's Green journal.
#[must_use = "reference journals require root transfer or explicit cancellation"]
pub struct M11ReferenceJournal {
    runtime_identity: StrongIdentity,
    source: SourceVersion,
    authority: CandidateAuthority,
    phase: JournalPhase,
    builder: Option<ReferenceRootBuilder>,
    winner: Option<ReferenceWinnerIndexJournal>,
    winner_reclaimer: Option<ReferenceWinnerIndexReclaimer>,
    build: Option<CandidateBuild>,
    subtree: Option<ReferenceSubtreeRoot>,
    metadata: Option<RoleMetadata>,
    seal: Option<CandidateSeal>,
    sealed_root: Option<CommittedArenaRoot>,
    output: Option<M11ReferenceJournalRoot>,
    last_source_byte_end: u64,
    last_source_utf16_end: u64,
}

impl fmt::Debug for M11ReferenceJournal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ReferenceJournal")
            .field("source", &self.source)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl M11ReferenceJournal {
    pub fn new(
        runtime: &mut DocumentRuntime,
        source: SourceVersion,
        syntax_profile: u32,
    ) -> Result<Self, M11ReferenceJournalError> {
        if runtime.current_source_version() != Some(source) {
            return Err(M11ReferenceJournalError(
                ErrorInner::SourceAuthorityMismatch,
            ));
        }
        if syntax_profile == 0 {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let runtime_identity = runtime.producer_identity();
        let authority = CandidateAuthority::new(
            runtime_identity,
            StrongIdentity::allocate(b"reference-journal")?,
            source,
            CandidateGeneration::from_wire(1)
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?,
            syntax_profile,
        )?;
        let limits = ReferenceRootLimits {
            arena: runtime.producer_arena().limits(),
            ..ReferenceRootLimits::default()
        };
        let builder = ReferenceRootBuilder::new(authority, limits)?;
        let build = {
            let mut session = runtime.producer_arena_mut().begin_build()?;
            let _ = session.allocate(&JOURNAL_ANCHOR, &[])?;
            session.suspend()?
        };
        Ok(Self {
            runtime_identity,
            source,
            authority,
            phase: JournalPhase::Accepting,
            builder: Some(builder),
            winner: Some(ReferenceWinnerIndexJournal::new()),
            winner_reclaimer: None,
            build: Some(build),
            subtree: None,
            metadata: None,
            seal: None,
            sealed_root: None,
            output: None,
            last_source_byte_end: 0,
            last_source_utf16_end: 0,
        })
    }

    pub fn offer_occurrence(
        &mut self,
        runtime: &DocumentRuntime,
        occurrence: M11ReferenceJournalOccurrence,
    ) -> Result<(), M11ReferenceJournalError> {
        self.ensure_runtime(runtime)?;
        if self.phase != JournalPhase::Accepting {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let last_source_byte_end = occurrence.source.bytes.end;
        let last_source_utf16_end = occurrence.source.utf16.end;
        let range = |range: M11ReferenceJournalRange| ReferenceSourceRange {
            bytes: range.bytes,
            utf16: range.utf16,
        };
        self.builder
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .offer(
                AuthoritativeReferenceFact {
                    authority: self.authority,
                    source: range(occurrence.source),
                    label_source: range(occurrence.label_source),
                    destination_source: range(occurrence.destination_source),
                    title_source: occurrence.title_source.map(range),
                    normalized_label: occurrence.normalized_label,
                    cooked_destination: occurrence.cooked_destination,
                    cooked_title: occurrence.cooked_title,
                    _not_sync: PhantomData,
                },
                runtime.producer_arena(),
                empty_reserve(),
            )?;
        self.phase = JournalPhase::Building;
        self.last_source_byte_end = last_source_byte_end;
        self.last_source_utf16_end = last_source_utf16_end;
        Ok(())
    }

    pub fn begin_occurrence_stream(
        &mut self,
        runtime: &DocumentRuntime,
        occurrence: M11ReferenceJournalOccurrenceStart,
    ) -> Result<(), M11ReferenceJournalError> {
        self.ensure_runtime(runtime)?;
        if self.phase != JournalPhase::Accepting {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let last_source_byte_end = occurrence.source.bytes.end;
        let last_source_utf16_end = occurrence.source.utf16.end;
        let range = |range: M11ReferenceJournalRange| ReferenceSourceRange {
            bytes: range.bytes,
            utf16: range.utf16,
        };
        self.builder
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .begin_stream(
                AuthoritativeReferenceFactStart {
                    authority: self.authority,
                    source: range(occurrence.source),
                    label_source: range(occurrence.label_source),
                    destination_source: range(occurrence.destination_source),
                    title_source: occurrence.title_source.map(range),
                    normalized_label: occurrence.normalized_label,
                    destination_len: occurrence.destination_len,
                    title_len: occurrence.title_len,
                    _not_sync: PhantomData,
                },
                runtime.producer_arena(),
                empty_reserve(),
            )?;
        self.phase = JournalPhase::Streaming;
        self.last_source_byte_end = last_source_byte_end;
        self.last_source_utf16_end = last_source_utf16_end;
        Ok(())
    }

    pub fn stream_capacity(
        &self,
        kind: M11ReferenceJournalValueKind,
    ) -> Result<usize, M11ReferenceJournalError> {
        if self.phase != JournalPhase::Streaming {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        self.builder
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .stream_capacity(kind.engine())
            .map_err(Into::into)
    }

    pub fn offer_stream_bytes(
        &mut self,
        kind: M11ReferenceJournalValueKind,
        bytes: &[u8],
    ) -> Result<usize, M11ReferenceJournalError> {
        if self.phase != JournalPhase::Streaming {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        self.builder
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .offer_stream_bytes(kind.engine(), bytes)
            .map_err(Into::into)
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        matches!(self.phase, JournalPhase::Accepting)
            && self
                .builder
                .as_ref()
                .is_some_and(ReferenceRootBuilder::is_idle)
    }

    pub fn finish_input(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        self.ensure_runtime(runtime)?;
        if self.phase != JournalPhase::Accepting {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        self.builder
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .finish(runtime.producer_arena(), empty_reserve())?;
        self.phase = JournalPhase::Finishing;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceJournalPoll, M11ReferenceJournalError> {
        self.ensure_runtime(runtime)?;
        if fuel == 0 {
            return Err(M11ReferenceJournalError(ErrorInner::ZeroFuel));
        }
        let mut transitions = 0;
        while transitions < fuel {
            match self.phase {
                JournalPhase::Accepting => break,
                JournalPhase::Building
                | JournalPhase::Streaming
                | JournalPhase::DrainingPage
                | JournalPhase::Finishing => {
                    let used = self.poll_builder_once(runtime)?;
                    transitions += used;
                    if used == 0 {
                        break;
                    }
                }
                JournalPhase::Indexing => {
                    let polled = self
                        .winner
                        .as_mut()
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                        .poll(runtime.producer_arena(), self.authority, 1)?;
                    transitions = transitions
                        .checked_add(polled.transitions)
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                    if polled.complete {
                        self.phase = if self
                            .builder
                            .as_ref()
                            .is_some_and(ReferenceRootBuilder::is_idle)
                        {
                            JournalPhase::Accepting
                        } else {
                            JournalPhase::DrainingPage
                        };
                    }
                }
                JournalPhase::ReadyForSeal => {
                    self.begin_seal(runtime)?;
                    transitions += 1;
                }
                JournalPhase::Sealing => {
                    transitions += self.poll_seal_once(runtime)?;
                }
                JournalPhase::Complete | JournalPhase::Cancelled | JournalPhase::Failed => break,
            }
        }
        Ok(M11ReferenceJournalPoll {
            status: match self.phase {
                JournalPhase::Accepting => M11ReferenceJournalStatus::NeedsInput,
                JournalPhase::Complete => M11ReferenceJournalStatus::Complete,
                JournalPhase::Cancelled => M11ReferenceJournalStatus::Cancelled,
                _ => M11ReferenceJournalStatus::Pending,
            },
            transitions,
        })
    }

    fn poll_builder_once(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<usize, M11ReferenceJournalError> {
        let was_building = self.phase == JournalPhase::Building;
        let was_draining_page = self.phase == JournalPhase::DrainingPage;
        let build = self
            .build
            .take()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let polled = self
            .builder
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .poll(&mut session, 1);
        let build = session.suspend()?;
        self.build = Some(build);
        match polled? {
            ReferenceBuildPoll::Pending { transitions, idle } => {
                if let Some(committed) = self
                    .builder
                    .as_mut()
                    .and_then(ReferenceRootBuilder::take_committed_occurrence)
                {
                    self.winner
                        .as_mut()
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                        .begin_occurrence(runtime.producer_arena(), self.authority, committed)?;
                    self.phase = JournalPhase::Indexing;
                } else if idle && was_building {
                    return self.fail(M11ReferenceJournalError(ErrorInner::InvalidState));
                } else if was_draining_page
                    && (idle
                        || self
                            .builder
                            .as_ref()
                            .is_some_and(ReferenceRootBuilder::is_idle))
                {
                    self.phase = JournalPhase::Accepting;
                }
                Ok(transitions)
            }
            ReferenceBuildPoll::Complete { transitions, root } => {
                if root.authority != self.authority {
                    return self.fail(M11ReferenceJournalError(ErrorInner::InvalidState));
                }
                self.subtree = Some(root);
                self.phase = JournalPhase::ReadyForSeal;
                Ok(transitions)
            }
        }
    }

    fn begin_seal(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        let build = self
            .build
            .take()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        let subtree = self
            .subtree
            .take()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        let ReferenceSubtreeRoot {
            authority,
            owner,
            metadata,
            _not_sync: _,
        } = subtree;
        match runtime.producer_arena_mut().begin_seal(build, owner) {
            Ok(seal) => {
                self.metadata = Some(metadata);
                self.seal = Some(seal);
                self.phase = JournalPhase::Sealing;
                Ok(())
            }
            Err(failure) => {
                self.build = Some(failure.build);
                self.subtree = Some(ReferenceSubtreeRoot {
                    authority,
                    owner: failure.root,
                    metadata,
                    _not_sync: PhantomData,
                });
                self.fail(failure.error.into())
            }
        }
    }

    fn poll_seal_once(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<usize, M11ReferenceJournalError> {
        let polled = runtime.producer_arena_mut().poll_seal(
            self.seal
                .as_mut()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?,
            1,
        )?;
        if let Some(root) = polled.root {
            self.seal = None;
            self.sealed_root = Some(root);
            let root_id = self
                .sealed_root
                .as_ref()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                .id();
            let winner = self
                .winner
                .take()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                .finish(runtime.producer_arena(), self.authority, root_id)?;
            let root = self
                .sealed_root
                .take()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
            self.output = Some(M11ReferenceJournalRoot {
                runtime_identity: self.runtime_identity,
                source: self.source,
                authority: self.authority,
                root: Some(root),
                metadata: self
                    .metadata
                    .take()
                    .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?,
                winner: Some(winner),
                winner_reclaimer: None,
                released: false,
                last_source_byte_end: self.last_source_byte_end,
                last_source_utf16_end: self.last_source_utf16_end,
            });
            self.builder = None;
            self.phase = JournalPhase::Complete;
        }
        Ok(polled.transitions)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11ReferenceJournalRoot> {
        (self.phase == JournalPhase::Complete)
            .then(|| self.output.take())
            .flatten()
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if let Some(mut output) = self.output.take() {
            output.begin_release(runtime)?;
            self.winner_reclaimer = output.winner_reclaimer.take();
            output.released = true;
        }
        if let Some(root) = self.sealed_root.take() {
            match runtime.producer_arena_mut().release_committed_root(root) {
                Ok(()) => {}
                Err(failure) => {
                    self.sealed_root = Some(failure.root);
                    return Err(failure.error.into());
                }
            }
        }
        if let Some(seal) = self.seal.take() {
            runtime.producer_arena_mut().abort_seal(seal)?;
        }
        if let Some(build) = self.build.take() {
            runtime.producer_arena_mut().abort_build(build)?;
        }
        self.subtree = None;
        self.builder = None;
        if let Some(winner) = self.winner.take() {
            self.winner_reclaimer = Some(winner.into_reclaimer());
        }
        self.phase = JournalPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceJournalReclaimPoll, M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if self.phase != JournalPhase::Cancelled || fuel == 0 {
            return Err(M11ReferenceJournalError(if fuel == 0 {
                ErrorInner::ZeroFuel
            } else {
                ErrorInner::InvalidState
            }));
        }
        let mut transitions = 0;
        if let Some(reclaimer) = self.winner_reclaimer.as_mut() {
            let polled = reclaimer.poll(fuel)?;
            transitions = polled.transitions;
            if polled.complete {
                self.winner_reclaimer = None;
            }
        }
        if transitions < fuel {
            let reclaimed = runtime
                .producer_arena_mut()
                .poll_reclaim(fuel - transitions);
            transitions += reclaimed.transitions;
        }
        let metrics = runtime.arena_metrics();
        Ok(M11ReferenceJournalReclaimPoll {
            transitions,
            complete: self.winner_reclaimer.is_none()
                && metrics.pending_build_aborts == 0
                && metrics.pending_reclaims == 0,
        })
    }

    fn ensure_runtime(&self, runtime: &DocumentRuntime) -> Result<(), M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ReferenceJournalError(
                ErrorInner::SourceAuthorityMismatch,
            ));
        }
        Ok(())
    }

    fn fail<T>(&mut self, error: M11ReferenceJournalError) -> Result<T, M11ReferenceJournalError> {
        self.phase = JournalPhase::Failed;
        Err(error)
    }
}

impl Drop for M11ReferenceJournal {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.build.is_none()
                    && self.seal.is_none()
                    && self.sealed_root.is_none()
                    && self.output.is_none()
                    && self.winner_reclaimer.is_none(),
                "reference journals require root transfer or fuelled cancellation"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceJournalReclaimPoll {
    transitions: usize,
    complete: bool,
}

impl M11ReferenceJournalReclaimPoll {
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn complete(self) -> bool {
        self.complete
    }
}

/// Committed canonical occurrence root plus its exact first-winner authority.
#[must_use = "reference roots require explicit release or publication transfer"]
pub struct M11ReferenceJournalRoot {
    runtime_identity: StrongIdentity,
    source: SourceVersion,
    authority: CandidateAuthority,
    root: Option<CommittedArenaRoot>,
    metadata: RoleMetadata,
    winner: Option<ReferenceWinnerIndex>,
    winner_reclaimer: Option<ReferenceWinnerIndexReclaimer>,
    released: bool,
    last_source_byte_end: u64,
    last_source_utf16_end: u64,
}

impl fmt::Debug for M11ReferenceJournalRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11ReferenceJournalRoot")
            .field("source", &self.source)
            .field("occurrences", &self.metadata.record_count)
            .finish_non_exhaustive()
    }
}

impl M11ReferenceJournalRoot {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn occurrence_count(&self) -> u64 {
        self.metadata.record_count
    }

    /// Returns the complete canonical role identity to engine-owned
    /// publication code without exposing its representation across the
    /// parser boundary.
    pub(crate) fn canonical_metadata(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<RoleMetadata, M11ReferenceJournalError> {
        self.ensure_live(runtime)?;
        Ok(self.metadata)
    }

    /// End of the last parser-authenticated reference occurrence.
    ///
    /// Zero means the committed reference set is empty. A target revision may
    /// retain this root only when the parser proves that its replacement crop
    /// emitted no definitions and, for a non-empty root, source lineage proves
    /// this complete prefix unchanged.
    #[must_use]
    pub const fn last_source_byte_end(&self) -> u64 {
        self.last_source_byte_end
    }

    #[must_use]
    pub const fn last_source_utf16_end(&self) -> u64 {
        self.last_source_utf16_end
    }

    pub fn winner_ordinal(
        &self,
        runtime: &DocumentRuntime,
        normalized_label: &[u8],
    ) -> Result<Option<u64>, M11ReferenceJournalError> {
        self.ensure_live(runtime)?;
        let root = self
            .root
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        self.winner
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .winner(
                runtime.producer_arena(),
                self.authority,
                root.id(),
                normalized_label,
            )
            .map(|winner| winner.map(|occurrence| occurrence.ordinal))
            .map_err(Into::into)
    }

    pub(crate) fn retain_for_publication(
        &self,
        session: &mut ArenaBuildSession<'_>,
        runtime_identity: StrongIdentity,
        source: SourceVersion,
    ) -> Result<(ArenaBuildOwner, RoleMetadata), M11ReferenceJournalError> {
        if self.released
            || self.runtime_identity != runtime_identity
            || self.source != source
            || self.root.is_none()
        {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let owner = session.retain(
            self.root
                .as_ref()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                .id(),
        )?;
        Ok((owner, self.metadata))
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        self.ensure_live(runtime)?;
        if let Some(winner) = self.winner.take() {
            self.winner_reclaimer = Some(winner.into_reclaimer());
        }
        if let Some(root) = self.root.take() {
            match runtime.producer_arena_mut().release_committed_root(root) {
                Ok(()) => {}
                Err(failure) => {
                    self.root = Some(failure.root);
                    return Err(failure.error.into());
                }
            }
        }
        self.released = true;
        Ok(())
    }

    pub fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceJournalReclaimPoll, M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if !self.released || fuel == 0 {
            return Err(M11ReferenceJournalError(if fuel == 0 {
                ErrorInner::ZeroFuel
            } else {
                ErrorInner::InvalidState
            }));
        }
        let mut transitions = 0;
        if let Some(reclaimer) = self.winner_reclaimer.as_mut() {
            let polled = reclaimer.poll(fuel)?;
            transitions = polled.transitions;
            if polled.complete {
                self.winner_reclaimer = None;
            }
        }
        if transitions < fuel {
            let reclaimed = runtime
                .producer_arena_mut()
                .poll_reclaim(fuel - transitions);
            transitions += reclaimed.transitions;
        }
        let metrics = runtime.arena_metrics();
        Ok(M11ReferenceJournalReclaimPoll {
            transitions,
            complete: self.winner_reclaimer.is_none()
                && metrics.pending_reclaims == 0
                && metrics.pending_build_aborts == 0,
        })
    }

    fn ensure_live(&self, runtime: &DocumentRuntime) -> Result<(), M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if self.released || self.root.is_none() || self.winner.is_none() {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        Ok(())
    }

    /// Starts a target-revision wrapper over an unchanged canonical reference
    /// prefix. This feature-gated parser seam deliberately accepts the
    /// parser's zero-occurrence result as a boolean: `flark-parser` keeps that
    /// result inside its move-only composite adoption transaction and does not
    /// expose this lower-level constructor to product callers.
    ///
    /// Non-empty roots additionally require an exact runtime-minted prefix
    /// witness ending at the final committed reference occurrence. Empty
    /// roots accept no prefix witness, but still require the parser's explicit
    /// zero-new-definition result so an edit cannot silently introduce the
    /// first definition.
    #[doc(hidden)]
    pub fn begin_unchanged_prefix_adoption(
        &self,
        runtime: &mut DocumentRuntime,
        prefix: Option<ExactUnchangedPrefixWitness>,
        parser_proved_zero_occurrences: bool,
    ) -> Result<M11ReferenceJournalUnchangedPrefixAdoption, M11ReferenceJournalError> {
        self.ensure_live(runtime)?;
        if !parser_proved_zero_occurrences {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let target = runtime
            .current_source_version()
            .ok_or(M11ReferenceJournalError(
                ErrorInner::SourceAuthorityMismatch,
            ))?;
        if target == self.source {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        if self.metadata.record_count == 0 {
            if prefix.is_some() || self.last_source_byte_end != 0 || self.last_source_utf16_end != 0
            {
                return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
            }
        } else {
            let prefix = runtime.take_exact_unchanged_prefix_witness(prefix.ok_or(
                M11ReferenceJournalError(ErrorInner::SourceAuthorityMismatch),
            )?)?;
            if prefix.base() != self.source
                || prefix.target() != target
                || u64::try_from(prefix.byte_end()).ok() != Some(self.last_source_byte_end)
                || u64::try_from(prefix.utf16_end()).ok() != Some(self.last_source_utf16_end)
            {
                return Err(M11ReferenceJournalError(
                    ErrorInner::SourceAuthorityMismatch,
                ));
            }
        }

        let generation = self
            .authority
            .parse_generation
            .get()
            .checked_add(1)
            .and_then(CandidateGeneration::from_wire)
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        let authority = CandidateAuthority::new(
            self.runtime_identity,
            StrongIdentity::allocate(b"reference-prefix-adoption")?,
            target,
            generation,
            self.authority.syntax_profile,
        )?;
        let root_id = self
            .root
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .id();
        let winner = self
            .winner
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .rebind_root(root_id);
        let (build, retained) = {
            let mut session = runtime.producer_arena_mut().begin_build()?;
            let retained = session.retain(root_id)?;
            (session.suspend()?, retained)
        };
        let seal = match runtime.producer_arena_mut().begin_seal(build, retained) {
            Ok(seal) => seal,
            Err(failure) => {
                let _ = failure.root;
                runtime.producer_arena_mut().abort_build(failure.build)?;
                return Err(failure.error.into());
            }
        };
        Ok(M11ReferenceJournalUnchangedPrefixAdoption {
            runtime_identity: self.runtime_identity,
            target,
            authority,
            metadata: self.metadata,
            last_source_byte_end: self.last_source_byte_end,
            last_source_utf16_end: self.last_source_utf16_end,
            phase: M11ReferenceJournalAdoptionPhase::Sealing,
            seal: Some(seal),
            root: None,
            winner: Some(winner),
            winner_reclaimer: None,
            output: None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11ReferenceJournalAdoptionPhase {
    Sealing,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceJournalAdoptionStatus {
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceJournalAdoptionPoll {
    status: M11ReferenceJournalAdoptionStatus,
    transitions: usize,
}

impl M11ReferenceJournalAdoptionPoll {
    #[must_use]
    pub const fn status(self) -> M11ReferenceJournalAdoptionStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

/// Fuelled target wrapper and first-winner-index rebuild for one unchanged
/// canonical reference prefix.
#[must_use = "reference adoption requires root transfer or explicit cancellation"]
pub struct M11ReferenceJournalUnchangedPrefixAdoption {
    runtime_identity: StrongIdentity,
    target: SourceVersion,
    authority: CandidateAuthority,
    metadata: RoleMetadata,
    last_source_byte_end: u64,
    last_source_utf16_end: u64,
    phase: M11ReferenceJournalAdoptionPhase,
    seal: Option<CandidateSeal>,
    root: Option<CommittedArenaRoot>,
    winner: Option<ReferenceWinnerIndex>,
    winner_reclaimer: Option<ReferenceWinnerIndexReclaimer>,
    output: Option<M11ReferenceJournalRoot>,
}

impl M11ReferenceJournalUnchangedPrefixAdoption {
    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceJournalAdoptionPoll, M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if runtime.current_source_version() != Some(self.target) {
            return Err(M11ReferenceJournalError(
                ErrorInner::SourceAuthorityMismatch,
            ));
        }
        if fuel == 0 {
            return Err(M11ReferenceJournalError(ErrorInner::ZeroFuel));
        }
        let mut transitions = 0;
        while transitions < fuel {
            match self.phase {
                M11ReferenceJournalAdoptionPhase::Sealing => {
                    let polled = runtime.producer_arena_mut().poll_seal(
                        self.seal
                            .as_mut()
                            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?,
                        fuel - transitions,
                    )?;
                    transitions = transitions
                        .checked_add(polled.transitions)
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                    let Some(root) = polled.root else { break };
                    self.seal = None;
                    if self
                        .winner
                        .as_ref()
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                        .root()
                        != root.id()
                    {
                        return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
                    }
                    let winner = self
                        .winner
                        .take()
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                    self.output = Some(M11ReferenceJournalRoot {
                        runtime_identity: self.runtime_identity,
                        source: self.target,
                        authority: self.authority,
                        root: Some(root),
                        metadata: self.metadata,
                        winner: Some(winner),
                        winner_reclaimer: None,
                        released: false,
                        last_source_byte_end: self.last_source_byte_end,
                        last_source_utf16_end: self.last_source_utf16_end,
                    });
                    self.phase = M11ReferenceJournalAdoptionPhase::Complete;
                }
                M11ReferenceJournalAdoptionPhase::Complete
                | M11ReferenceJournalAdoptionPhase::Cancelled => break,
            }
        }
        Ok(M11ReferenceJournalAdoptionPoll {
            status: match self.phase {
                M11ReferenceJournalAdoptionPhase::Complete => {
                    M11ReferenceJournalAdoptionStatus::Complete
                }
                M11ReferenceJournalAdoptionPhase::Cancelled => {
                    M11ReferenceJournalAdoptionStatus::Cancelled
                }
                _ => M11ReferenceJournalAdoptionStatus::Pending,
            },
            transitions,
        })
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11ReferenceJournalRoot> {
        (self.phase == M11ReferenceJournalAdoptionPhase::Complete)
            .then(|| self.output.take())
            .flatten()
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if let Some(mut output) = self.output.take() {
            output.begin_release(runtime)?;
            self.winner_reclaimer = output.winner_reclaimer.take();
            output.released = true;
        }
        if let Some(winner) = self.winner.take() {
            self.winner_reclaimer = Some(winner.into_reclaimer());
        }
        if let Some(root) = self.root.take() {
            match runtime.producer_arena_mut().release_committed_root(root) {
                Ok(()) => {}
                Err(failure) => {
                    self.root = Some(failure.root);
                    return Err(failure.error.into());
                }
            }
        }
        if let Some(seal) = self.seal.take() {
            runtime.producer_arena_mut().abort_seal(seal)?;
        }
        self.phase = M11ReferenceJournalAdoptionPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceJournalReclaimPoll, M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if self.phase != M11ReferenceJournalAdoptionPhase::Cancelled || fuel == 0 {
            return Err(M11ReferenceJournalError(if fuel == 0 {
                ErrorInner::ZeroFuel
            } else {
                ErrorInner::InvalidState
            }));
        }
        let mut transitions = 0;
        if let Some(reclaimer) = self.winner_reclaimer.as_mut() {
            let polled = reclaimer.poll(fuel)?;
            transitions = polled.transitions;
            if polled.complete {
                self.winner_reclaimer = None;
            }
        }
        if transitions < fuel {
            let reclaimed = runtime
                .producer_arena_mut()
                .poll_reclaim(fuel - transitions);
            transitions += reclaimed.transitions;
        }
        let metrics = runtime.arena_metrics();
        Ok(M11ReferenceJournalReclaimPoll {
            transitions,
            complete: self.winner_reclaimer.is_none()
                && self.root.is_none()
                && metrics.pending_reclaims == 0
                && metrics.pending_build_aborts == 0,
        })
    }
}

impl Drop for M11ReferenceJournalUnchangedPrefixAdoption {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.seal.is_none()
                    && self.root.is_none()
                    && self.winner.is_none()
                    && self.output.is_none()
                    && self.winner_reclaimer.is_none(),
                "reference prefix adoption requires root transfer or fuelled cancellation"
            );
        }
    }
}

impl Drop for M11ReferenceJournalRoot {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.released,
                "reference roots require explicit release or publication transfer"
            );
        }
    }
}

const fn empty_reserve() -> ReferenceReserve {
    ReferenceReserve {
        nodes: 0,
        payload_bytes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentRuntimeConfig;

    fn settle_input(journal: &mut M11ReferenceJournal, runtime: &mut DocumentRuntime) {
        loop {
            let polled = journal.poll(runtime, 64).expect("poll reference journal");
            if polled.status() == M11ReferenceJournalStatus::NeedsInput {
                break;
            }
            assert_eq!(polled.status(), M11ReferenceJournalStatus::Pending);
        }
    }

    #[test]
    fn duplicate_occurrences_keep_the_exact_first_winner_in_the_persistent_journal() {
        let mut runtime =
            DocumentRuntime::new("[a]: /1\n[a]: /2", DocumentRuntimeConfig::default())
                .expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut journal = M11ReferenceJournal::new(&mut runtime, source, 1).expect("journal");

        journal
            .offer_occurrence(
                &runtime,
                M11ReferenceJournalOccurrence::new(
                    M11ReferenceJournalRange::new(0..7, 0..7),
                    M11ReferenceJournalRange::new(1..2, 1..2),
                    M11ReferenceJournalRange::new(5..7, 5..7),
                    None,
                    &b"a"[..],
                    &b"/1"[..],
                    None,
                ),
            )
            .expect("first occurrence");
        settle_input(&mut journal, &mut runtime);
        journal
            .offer_occurrence(
                &runtime,
                M11ReferenceJournalOccurrence::new(
                    M11ReferenceJournalRange::new(8..15, 8..15),
                    M11ReferenceJournalRange::new(9..10, 9..10),
                    M11ReferenceJournalRange::new(13..15, 13..15),
                    None,
                    &b"a"[..],
                    &b"/2"[..],
                    None,
                ),
            )
            .expect("duplicate occurrence");
        settle_input(&mut journal, &mut runtime);

        journal.finish_input(&runtime).expect("finish input");
        loop {
            let polled = journal.poll(&mut runtime, 64).expect("finish journal");
            if polled.status() == M11ReferenceJournalStatus::Complete {
                break;
            }
            assert_eq!(polled.status(), M11ReferenceJournalStatus::Pending);
        }
        let mut root = journal.take_root().expect("journal root");
        assert_eq!(root.occurrence_count(), 2);
        assert_eq!(
            root.winner_ordinal(&runtime, b"a").expect("winner"),
            Some(0)
        );
        root.begin_release(&mut runtime)
            .expect("begin root release");
        while !root
            .poll_release(&mut runtime, 64)
            .expect("poll root release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
    }

    #[test]
    fn full_reference_pages_drain_before_accepting_the_next_occurrence() {
        const OCCURRENCES: usize = 129;
        let mut source_text = String::new();
        let mut occurrences = Vec::with_capacity(OCCURRENCES);
        for ordinal in 0..OCCURRENCES {
            let label = format!("r{ordinal:03}");
            let start = source_text.len();
            let destination_start = start + label.len() + 4;
            source_text.push('[');
            source_text.push_str(&label);
            source_text.push_str("]: /d\n");
            let end = source_text.len() - 1;
            occurrences.push((
                start as u64..end as u64,
                (start + 1) as u64..(start + 1 + label.len()) as u64,
                destination_start as u64..(destination_start + 2) as u64,
                label.into_bytes(),
            ));
        }

        let mut runtime =
            DocumentRuntime::new(&source_text, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut journal = M11ReferenceJournal::new(&mut runtime, source, 1).expect("journal");
        for (source, label_source, destination_source, label) in occurrences {
            assert!(journal.is_idle(), "journal must authenticate readiness");
            journal
                .begin_occurrence_stream(
                    &runtime,
                    M11ReferenceJournalOccurrenceStart::new(
                        M11ReferenceJournalRange::new(source.clone(), source),
                        M11ReferenceJournalRange::new(label_source.clone(), label_source),
                        M11ReferenceJournalRange::new(
                            destination_source.clone(),
                            destination_source,
                        ),
                        None,
                        label,
                        2,
                        None,
                    ),
                )
                .expect("begin occurrence after any full-page drain");
            while journal
                .stream_capacity(M11ReferenceJournalValueKind::Destination)
                .expect("destination capacity")
                == 0
            {
                let polled = journal.poll(&mut runtime, 64).expect("prepare stream");
                assert_eq!(polled.status(), M11ReferenceJournalStatus::Pending);
            }
            assert_eq!(
                journal
                    .offer_stream_bytes(M11ReferenceJournalValueKind::Destination, b"/d")
                    .expect("stream destination"),
                2
            );
            settle_input(&mut journal, &mut runtime);
        }

        journal.finish_input(&runtime).expect("finish input");
        loop {
            let polled = journal.poll(&mut runtime, 64).expect("finish journal");
            if polled.status() == M11ReferenceJournalStatus::Complete {
                break;
            }
            assert_eq!(polled.status(), M11ReferenceJournalStatus::Pending);
        }
        let mut root = journal.take_root().expect("journal root");
        assert_eq!(root.occurrence_count(), OCCURRENCES as u64);
        root.begin_release(&mut runtime)
            .expect("begin root release");
        while !root
            .poll_release(&mut runtime, 64)
            .expect("poll root release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
    }
}
