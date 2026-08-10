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
    AuthoritativeReferenceFact, AuthoritativeReferenceFactStart, DetachedReferenceOccurrence,
    PersistentBytesCopyCursor, ReferenceBuildPoll, ReferenceOccurrenceCursor,
    ReferenceOccurrenceCursorPoll, ReferenceRootBuilder, ReferenceRootError, ReferenceRootLimits,
    ReferenceRootView, ReferenceSourceRange, ReferenceSubtreeRoot, ReferenceWinnerIndex,
    ReferenceWinnerIndexJournal, ReferenceWinnerIndexReclaimer, StreamedReferenceValueKind,
    BLOB_CHUNK_BYTES,
};
use crate::storage::{
    ArenaBuildOwner, ArenaBuildSession, ArenaError, CandidateBuild, CandidateSeal,
    CommittedArenaRoot,
};
use crate::{
    CandidateGeneration, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness, SourceVersion,
};

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
    first_source_byte_start: u64,
    first_source_utf16_start: u64,
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
            first_source_byte_start: 0,
            first_source_utf16_start: 0,
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
        let first_source_byte_start = occurrence.source.bytes.start;
        let first_source_utf16_start = occurrence.source.utf16.start;
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
        if self.last_source_byte_end == 0 && self.last_source_utf16_end == 0 {
            self.first_source_byte_start = first_source_byte_start;
            self.first_source_utf16_start = first_source_utf16_start;
        }
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
        let first_source_byte_start = occurrence.source.bytes.start;
        let first_source_utf16_start = occurrence.source.utf16.start;
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
        if self.last_source_byte_end == 0 && self.last_source_utf16_end == 0 {
            self.first_source_byte_start = first_source_byte_start;
            self.first_source_utf16_start = first_source_utf16_start;
        }
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
                first_source_byte_start: self.first_source_byte_start,
                first_source_utf16_start: self.first_source_utf16_start,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11ReferenceJournalRangeReplacementPhase {
    RetainingBase,
    OpeningBase,
    ReplayingPrefix,
    AcceptingReplacement,
    ReplayingSuffix,
    Finishing,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceJournalRangeReplacementStatus {
    Pending,
    NeedsReplacementInput,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceJournalRangeReplacementPoll {
    status: M11ReferenceJournalRangeReplacementStatus,
    transitions: usize,
}

impl M11ReferenceJournalRangeReplacementPoll {
    #[must_use]
    pub const fn status(self) -> M11ReferenceJournalRangeReplacementStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy)]
struct M11ReferenceJournalSuffixTranslation {
    base_byte_start: u64,
    base_utf16_start: u64,
    target_byte_start: u64,
    target_utf16_start: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11ReferenceReplayOccurrencePhase {
    Label,
    BeginJournal,
    Destination,
    Title,
    DrainJournal,
    Complete,
}

struct M11ReferenceReplayOccurrence {
    source: M11ReferenceJournalRange,
    label_source: M11ReferenceJournalRange,
    destination_source: M11ReferenceJournalRange,
    title_source: Option<M11ReferenceJournalRange>,
    normalized_label_cursor: PersistentBytesCopyCursor,
    normalized_label: Vec<u8>,
    cooked_destination: PersistentBytesCopyCursor,
    cooked_title: Option<PersistentBytesCopyCursor>,
    phase: M11ReferenceReplayOccurrencePhase,
}

impl M11ReferenceReplayOccurrence {
    fn new(
        occurrence: DetachedReferenceOccurrence,
        translation: Option<M11ReferenceJournalSuffixTranslation>,
    ) -> Result<Self, M11ReferenceJournalError> {
        let DetachedReferenceOccurrence {
            ordinal: _,
            source,
            label_source,
            destination_source,
            title_source,
            normalized_label,
            cooked_destination,
            cooked_title,
        } = occurrence;
        let translate = |range: ReferenceSourceRange| match translation {
            Some(translation) => translate_reference_range(range, translation),
            None => Ok(M11ReferenceJournalRange::new(range.bytes, range.utf16)),
        };
        let mut normalized_label_bytes = Vec::new();
        let label_len = usize::try_from(normalized_label.len())
            .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?;
        normalized_label_bytes
            .try_reserve_exact(label_len)
            .map_err(|_| {
                M11ReferenceJournalError(ErrorInner::Arena(ArenaError::AllocationFailed))
            })?;
        Ok(Self {
            source: translate(source)?,
            label_source: translate(label_source)?,
            destination_source: translate(destination_source)?,
            title_source: title_source.map(translate).transpose()?,
            normalized_label_cursor: normalized_label,
            normalized_label: normalized_label_bytes,
            cooked_destination,
            cooked_title,
            phase: M11ReferenceReplayOccurrencePhase::Label,
        })
    }

    fn poll_one(
        &mut self,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        match self.phase {
            M11ReferenceReplayOccurrencePhase::Label => {
                if self.normalized_label_cursor.complete() {
                    self.phase = M11ReferenceReplayOccurrencePhase::BeginJournal;
                    return Ok(());
                }
                let mut chunk = [0_u8; BLOB_CHUNK_BYTES];
                let polled = self.normalized_label_cursor.poll_copy(
                    runtime.producer_arena(),
                    &mut chunk,
                    1,
                )?;
                self.normalized_label
                    .extend_from_slice(&chunk[..polled.written]);
            }
            M11ReferenceReplayOccurrencePhase::BeginJournal => {
                if !journal.is_idle() {
                    let _ = journal.poll(runtime, 1)?;
                    return Ok(());
                }
                let destination_len = usize::try_from(self.cooked_destination.len())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?;
                let title_len = self
                    .cooked_title
                    .as_ref()
                    .map(PersistentBytesCopyCursor::len)
                    .map(usize::try_from)
                    .transpose()
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?;
                journal.begin_occurrence_stream(
                    runtime,
                    M11ReferenceJournalOccurrenceStart::new(
                        self.source.clone(),
                        self.label_source.clone(),
                        self.destination_source.clone(),
                        self.title_source.clone(),
                        std::mem::take(&mut self.normalized_label).into_boxed_slice(),
                        destination_len,
                        title_len,
                    ),
                )?;
                self.phase = M11ReferenceReplayOccurrencePhase::Destination;
            }
            M11ReferenceReplayOccurrencePhase::Destination => {
                if self.cooked_destination.complete() {
                    self.phase = if self.cooked_title.is_some() {
                        M11ReferenceReplayOccurrencePhase::Title
                    } else {
                        M11ReferenceReplayOccurrencePhase::DrainJournal
                    };
                    return Ok(());
                }
                replay_value_chunk(
                    &mut self.cooked_destination,
                    M11ReferenceJournalValueKind::Destination,
                    journal,
                    runtime,
                )?;
            }
            M11ReferenceReplayOccurrencePhase::Title => {
                let title = self
                    .cooked_title
                    .as_mut()
                    .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                if title.complete() {
                    self.phase = M11ReferenceReplayOccurrencePhase::DrainJournal;
                    return Ok(());
                }
                replay_value_chunk(title, M11ReferenceJournalValueKind::Title, journal, runtime)?;
            }
            M11ReferenceReplayOccurrencePhase::DrainJournal => {
                if journal.is_idle() {
                    self.phase = M11ReferenceReplayOccurrencePhase::Complete;
                } else {
                    let _ = journal.poll(runtime, 1)?;
                }
            }
            M11ReferenceReplayOccurrencePhase::Complete => {}
        }
        Ok(())
    }
}

fn replay_value_chunk(
    value: &mut PersistentBytesCopyCursor,
    kind: M11ReferenceJournalValueKind,
    journal: &mut M11ReferenceJournal,
    runtime: &mut DocumentRuntime,
) -> Result<(), M11ReferenceJournalError> {
    let capacity = journal.stream_capacity(kind)?;
    if capacity == 0 {
        let _ = journal.poll(runtime, 1)?;
        return Ok(());
    }
    let mut chunk = [0_u8; BLOB_CHUNK_BYTES];
    let permitted = capacity.min(chunk.len());
    let polled = value.poll_copy(runtime.producer_arena(), &mut chunk[..permitted], 1)?;
    if polled.written != 0 {
        let consumed = journal.offer_stream_bytes(kind, &chunk[..polled.written])?;
        if consumed != polled.written {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
    }
    Ok(())
}

fn translate_reference_coordinate(
    value: u64,
    base_start: u64,
    target_start: u64,
) -> Result<u64, M11ReferenceJournalError> {
    if value < base_start {
        return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
    }
    let relative = value - base_start;
    target_start
        .checked_add(relative)
        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))
}

fn translate_reference_range(
    range: ReferenceSourceRange,
    translation: M11ReferenceJournalSuffixTranslation,
) -> Result<M11ReferenceJournalRange, M11ReferenceJournalError> {
    Ok(M11ReferenceJournalRange::new(
        translate_reference_coordinate(
            range.bytes.start,
            translation.base_byte_start,
            translation.target_byte_start,
        )?
            ..translate_reference_coordinate(
                range.bytes.end,
                translation.base_byte_start,
                translation.target_byte_start,
            )?,
        translate_reference_coordinate(
            range.utf16.start,
            translation.base_utf16_start,
            translation.target_utf16_start,
        )?
            ..translate_reference_coordinate(
                range.utf16.end,
                translation.base_utf16_start,
                translation.target_utf16_start,
            )?,
    ))
}

/// Fuelled replacement of one parser-owned source crop in a committed
/// References journal.
///
/// The actor retains the immutable base root, copies the exact unchanged
/// prefix into a fresh target journal, temporarily lends that journal to the
/// parser for replacement occurrences, then translates and copies the exact
/// unchanged suffix. The canonical root and first-winner index are rebuilt by
/// the ordinary journal; no second recognition or normalization path exists.
#[must_use = "reference range replacement requires root transfer or explicit cancellation"]
pub struct M11ReferenceJournalRangeReplacement {
    runtime_identity: StrongIdentity,
    base_source: SourceVersion,
    target: SourceVersion,
    base_authority: CandidateAuthority,
    base_count: u64,
    prefix_byte_end: u64,
    prefix_utf16_end: u64,
    phase: M11ReferenceJournalRangeReplacementPhase,
    base_retain_seal: Option<CandidateSeal>,
    base_root: Option<CommittedArenaRoot>,
    cursor: Option<ReferenceOccurrenceCursor>,
    buffered_base: Option<DetachedReferenceOccurrence>,
    active_replay: Option<M11ReferenceReplayOccurrence>,
    suffix: Option<M11ReferenceJournalSuffixTranslation>,
    journal: Option<M11ReferenceJournal>,
    output: Option<M11ReferenceJournalRoot>,
    cancel_output: Option<M11ReferenceJournalRoot>,
    journal_cancel_complete: bool,
    output_release_complete: bool,
}

impl M11ReferenceJournalRangeReplacement {
    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceJournalRangeReplacementPoll, M11ReferenceJournalError> {
        self.ensure_target(runtime)?;
        if fuel == 0 {
            return Err(M11ReferenceJournalError(ErrorInner::ZeroFuel));
        }
        let mut transitions = 0;
        while transitions < fuel
            && !matches!(
                self.phase,
                M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement
                    | M11ReferenceJournalRangeReplacementPhase::Complete
                    | M11ReferenceJournalRangeReplacementPhase::Cancelled
            )
        {
            self.poll_one(runtime)?;
            transitions += 1;
        }
        Ok(M11ReferenceJournalRangeReplacementPoll {
            status: match self.phase {
                M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement => {
                    M11ReferenceJournalRangeReplacementStatus::NeedsReplacementInput
                }
                M11ReferenceJournalRangeReplacementPhase::Complete => {
                    M11ReferenceJournalRangeReplacementStatus::Complete
                }
                M11ReferenceJournalRangeReplacementPhase::Cancelled => {
                    M11ReferenceJournalRangeReplacementStatus::Cancelled
                }
                _ => M11ReferenceJournalRangeReplacementStatus::Pending,
            },
            transitions,
        })
    }

    /// Borrows the fresh target journal while the parser owns the replacement
    /// crop. The existing reference rendezvous may use its normal streaming
    /// API; this actor performs no recognition or cooking of replacement
    /// occurrences.
    pub fn replacement_journal_mut(
        &mut self,
    ) -> Result<&mut M11ReferenceJournal, M11ReferenceJournalError> {
        if self.phase != M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        self.journal
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))
    }

    /// Closes parser input and selects the exact unchanged base suffix.
    /// `None` is valid only for a replacement reaching EOF in both revisions.
    pub fn finish_replacement(
        &mut self,
        runtime: &DocumentRuntime,
        suffix: Option<ExactUnchangedSuffixWitness>,
    ) -> Result<(), M11ReferenceJournalError> {
        self.ensure_target(runtime)?;
        if self.phase != M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement
            || !self
                .journal
                .as_ref()
                .is_some_and(M11ReferenceJournal::is_idle)
        {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let translation = if let Some(suffix) = suffix {
            let suffix = runtime.take_exact_unchanged_suffix_witness(suffix)?;
            if suffix.base() != self.base_source
                || suffix.target() != self.target
                || suffix.base_byte_start() < self.prefix_byte_end as usize
                || suffix.base_utf16_start() < self.prefix_utf16_end as usize
                || suffix.target_byte_start() < self.prefix_byte_end as usize
                || suffix.target_utf16_start() < self.prefix_utf16_end as usize
            {
                return Err(M11ReferenceJournalError(
                    ErrorInner::SourceAuthorityMismatch,
                ));
            }
            M11ReferenceJournalSuffixTranslation {
                base_byte_start: u64::try_from(suffix.base_byte_start())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
                base_utf16_start: u64::try_from(suffix.base_utf16_start())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
                target_byte_start: u64::try_from(suffix.target_byte_start())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
                target_utf16_start: u64::try_from(suffix.target_utf16_start())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
            }
        } else {
            M11ReferenceJournalSuffixTranslation {
                base_byte_start: u64::try_from(self.base_source.byte_len())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
                base_utf16_start: u64::try_from(self.base_source.utf16_len())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
                target_byte_start: u64::try_from(self.target.byte_len())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
                target_utf16_start: u64::try_from(self.target.utf16_len())
                    .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?,
            }
        };
        if translation.base_byte_start < self.prefix_byte_end
            || translation.base_utf16_start < self.prefix_utf16_end
        {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        let journal = self
            .journal
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        if journal.last_source_byte_end > translation.target_byte_start
            || journal.last_source_utf16_end > translation.target_utf16_start
        {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        self.suffix = Some(translation);
        self.phase = M11ReferenceJournalRangeReplacementPhase::ReplayingSuffix;
        Ok(())
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11ReferenceJournalRoot> {
        (self.phase == M11ReferenceJournalRangeReplacementPhase::Complete)
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
        if self.phase == M11ReferenceJournalRangeReplacementPhase::Cancelled {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        if let Some(mut output) = self.output.take() {
            output.begin_release(runtime)?;
            self.cancel_output = Some(output);
            self.output_release_complete = false;
        }
        if let Some(seal) = self.base_retain_seal.take() {
            runtime.producer_arena_mut().abort_seal(seal)?;
        }
        if let Some(root) = self.base_root.take() {
            match runtime.producer_arena_mut().release_committed_root(root) {
                Ok(()) => {}
                Err(failure) => {
                    self.base_root = Some(failure.root);
                    return Err(failure.error.into());
                }
            }
        }
        self.journal
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .begin_cancel(runtime)?;
        self.journal_cancel_complete = false;
        self.cursor = None;
        self.buffered_base = None;
        self.active_replay = None;
        self.phase = M11ReferenceJournalRangeReplacementPhase::Cancelled;
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
        if self.phase != M11ReferenceJournalRangeReplacementPhase::Cancelled || fuel == 0 {
            return Err(M11ReferenceJournalError(if fuel == 0 {
                ErrorInner::ZeroFuel
            } else {
                ErrorInner::InvalidState
            }));
        }
        let mut transitions = 0;
        if !self.journal_cancel_complete {
            let polled = self
                .journal
                .as_mut()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                .poll_cancel(runtime, fuel)?;
            transitions += polled.transitions();
            self.journal_cancel_complete = polled.complete();
        }
        if transitions < fuel && !self.output_release_complete {
            let polled = self
                .cancel_output
                .as_mut()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
                .poll_release(runtime, fuel - transitions)?;
            transitions += polled.transitions();
            self.output_release_complete = polled.complete();
            if self.output_release_complete {
                self.cancel_output = None;
            }
        }
        if transitions < fuel {
            let polled = runtime
                .producer_arena_mut()
                .poll_reclaim(fuel - transitions);
            transitions += polled.transitions;
        }
        let metrics = runtime.arena_metrics();
        Ok(M11ReferenceJournalReclaimPoll {
            transitions,
            complete: self.journal_cancel_complete
                && self.output_release_complete
                && self.base_retain_seal.is_none()
                && self.base_root.is_none()
                && metrics.pending_build_aborts == 0
                && metrics.pending_reclaims == 0,
        })
    }

    fn poll_one(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11ReferenceJournalError> {
        let result = (|| match self.phase {
            M11ReferenceJournalRangeReplacementPhase::RetainingBase => {
                let polled = runtime.producer_arena_mut().poll_seal(
                    self.base_retain_seal
                        .as_mut()
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?,
                    1,
                )?;
                if let Some(root) = polled.root {
                    self.base_retain_seal = None;
                    self.base_root = Some(root);
                    self.phase = M11ReferenceJournalRangeReplacementPhase::OpeningBase;
                }
                Ok(())
            }
            M11ReferenceJournalRangeReplacementPhase::OpeningBase => {
                let root = self
                    .base_root
                    .as_ref()
                    .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                let view = ReferenceRootView::open(
                    runtime.producer_arena(),
                    self.base_authority,
                    root.id(),
                )?;
                if view.count() != self.base_count {
                    return self.fail(M11ReferenceJournalError(ErrorInner::InvalidState));
                }
                self.cursor = Some(view.occurrences());
                self.phase = M11ReferenceJournalRangeReplacementPhase::ReplayingPrefix;
                Ok(())
            }
            M11ReferenceJournalRangeReplacementPhase::ReplayingPrefix => {
                self.poll_prefix_one(runtime)
            }
            M11ReferenceJournalRangeReplacementPhase::ReplayingSuffix => {
                self.poll_suffix_one(runtime)
            }
            M11ReferenceJournalRangeReplacementPhase::Finishing => {
                let journal = self
                    .journal
                    .as_mut()
                    .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                let polled = journal.poll(runtime, 1)?;
                if polled.status() == M11ReferenceJournalStatus::Complete {
                    let output = journal
                        .take_root()
                        .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                    // Install the move-only target authority before releasing
                    // the replay retain so even an arena queue failure leaves
                    // cancellation with every owner reachable.
                    self.output = Some(output);
                    if let Some(root) = self.base_root.take() {
                        match runtime.producer_arena_mut().release_committed_root(root) {
                            Ok(()) => {}
                            Err(failure) => {
                                self.base_root = Some(failure.root);
                                return self.fail(failure.error.into());
                            }
                        }
                    }
                    self.cursor = None;
                    self.phase = M11ReferenceJournalRangeReplacementPhase::Complete;
                }
                Ok(())
            }
            M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement
            | M11ReferenceJournalRangeReplacementPhase::Complete
            | M11ReferenceJournalRangeReplacementPhase::Cancelled => Ok(()),
            M11ReferenceJournalRangeReplacementPhase::Failed => {
                Err(M11ReferenceJournalError(ErrorInner::InvalidState))
            }
        })();
        if let Err(error) = result {
            self.phase = M11ReferenceJournalRangeReplacementPhase::Failed;
            Err(error)
        } else {
            Ok(())
        }
    }

    fn poll_prefix_one(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        if self.poll_active_replay(runtime)? {
            return Ok(());
        }
        if let Some(occurrence) = self.buffered_base.take() {
            let in_prefix = occurrence.source.bytes.end <= self.prefix_byte_end
                && occurrence.source.utf16.end <= self.prefix_utf16_end;
            if in_prefix {
                self.active_replay = Some(M11ReferenceReplayOccurrence::new(occurrence, None)?);
            } else {
                self.buffered_base = Some(occurrence);
                self.phase = M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement;
            }
            return Ok(());
        }
        match self
            .cursor
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .poll_next(runtime.producer_arena(), 1)?
        {
            ReferenceOccurrenceCursorPoll::Pending { .. } => {}
            ReferenceOccurrenceCursorPoll::Occurrence { occurrence, .. } => {
                self.buffered_base = Some(occurrence)
            }
            ReferenceOccurrenceCursorPoll::Complete { .. } => {
                self.phase = M11ReferenceJournalRangeReplacementPhase::AcceptingReplacement
            }
        }
        Ok(())
    }

    fn poll_suffix_one(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceJournalError> {
        if self.poll_active_replay(runtime)? {
            return Ok(());
        }
        let suffix = self
            .suffix
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
        if let Some(occurrence) = self.buffered_base.take() {
            if occurrence.source.bytes.start >= suffix.base_byte_start
                && occurrence.source.utf16.start >= suffix.base_utf16_start
            {
                self.active_replay =
                    Some(M11ReferenceReplayOccurrence::new(occurrence, Some(suffix))?);
            }
            return Ok(());
        }
        match self
            .cursor
            .as_mut()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .poll_next(runtime.producer_arena(), 1)?
        {
            ReferenceOccurrenceCursorPoll::Pending { .. } => {}
            ReferenceOccurrenceCursorPoll::Occurrence { occurrence, .. } => {
                self.buffered_base = Some(occurrence)
            }
            ReferenceOccurrenceCursorPoll::Complete { .. } => {
                let journal = self
                    .journal
                    .as_mut()
                    .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?;
                if !journal.is_idle() {
                    let _ = journal.poll(runtime, 1)?;
                } else {
                    journal.finish_input(runtime)?;
                    self.phase = M11ReferenceJournalRangeReplacementPhase::Finishing;
                }
            }
        }
        Ok(())
    }

    /// Returns true when this transition belonged to an active replay.
    fn poll_active_replay(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<bool, M11ReferenceJournalError> {
        let Some(active) = self.active_replay.as_mut() else {
            return Ok(false);
        };
        active.poll_one(
            self.journal
                .as_mut()
                .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?,
            runtime,
        )?;
        if active.phase == M11ReferenceReplayOccurrencePhase::Complete {
            self.active_replay = None;
        }
        Ok(true)
    }

    fn ensure_target(&self, runtime: &DocumentRuntime) -> Result<(), M11ReferenceJournalError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11ReferenceJournalError(ErrorInner::WrongRuntime));
        }
        if runtime.current_source_version() != Some(self.target) {
            return Err(M11ReferenceJournalError(
                ErrorInner::SourceAuthorityMismatch,
            ));
        }
        Ok(())
    }

    fn fail<T>(&mut self, error: M11ReferenceJournalError) -> Result<T, M11ReferenceJournalError> {
        self.phase = M11ReferenceJournalRangeReplacementPhase::Failed;
        Err(error)
    }
}

impl Drop for M11ReferenceJournalRangeReplacement {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.base_retain_seal.is_none()
                    && self.base_root.is_none()
                    && self.output.is_none()
                    && self.cancel_output.is_none(),
                "reference range replacement requires root transfer or fuelled cancellation"
            );
        }
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
    first_source_byte_start: u64,
    first_source_utf16_start: u64,
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

    /// Start of the first parser-authenticated reference occurrence.
    ///
    /// Zero is also the valid start of a leading definition; callers must use
    /// [`Self::occurrence_count`] to distinguish that case from an empty root.
    #[must_use]
    pub const fn first_source_byte_start(&self) -> u64 {
        self.first_source_byte_start
    }

    #[must_use]
    pub const fn first_source_utf16_start(&self) -> u64 {
        self.first_source_utf16_start
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

    /// Clones the immutable first-winner acceleration needed by one
    /// parser-side reference resolver while this journal root remains live.
    pub(crate) fn resolver_parts(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<
        (
            StrongIdentity,
            CandidateAuthority,
            crate::ArenaId,
            ReferenceWinnerIndex,
        ),
        M11ReferenceJournalError,
    > {
        self.ensure_live(runtime)?;
        let root = self
            .root
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .id();
        let winner = self
            .winner
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .rebind_root(root);
        Ok((self.runtime_identity, self.authority, root, winner))
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

    /// Begins a parser-owned range replacement in the current target source.
    /// The supplied prefix boundary is the parser restart cut. A nonzero cut
    /// requires a move-only exact-prefix witness at identical byte and UTF-16
    /// coordinates; BOF requires no witness.
    #[doc(hidden)]
    pub fn begin_range_replacement(
        &self,
        runtime: &mut DocumentRuntime,
        base_prefix_byte_end: usize,
        base_prefix_utf16_end: usize,
        prefix: Option<ExactUnchangedPrefixWitness>,
    ) -> Result<M11ReferenceJournalRangeReplacement, M11ReferenceJournalError> {
        self.ensure_live(runtime)?;
        let target = runtime
            .current_source_version()
            .ok_or(M11ReferenceJournalError(
                ErrorInner::SourceAuthorityMismatch,
            ))?;
        if target == self.source
            || base_prefix_byte_end > self.source.byte_len()
            || base_prefix_utf16_end > self.source.utf16_len()
        {
            return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
        }
        if base_prefix_byte_end == 0 && base_prefix_utf16_end == 0 {
            if prefix.is_some() {
                return Err(M11ReferenceJournalError(ErrorInner::InvalidState));
            }
        } else {
            let prefix = runtime.take_exact_unchanged_prefix_witness(prefix.ok_or(
                M11ReferenceJournalError(ErrorInner::SourceAuthorityMismatch),
            )?)?;
            if prefix.base() != self.source
                || prefix.target() != target
                || prefix.byte_end() != base_prefix_byte_end
                || prefix.utf16_end() != base_prefix_utf16_end
            {
                return Err(M11ReferenceJournalError(
                    ErrorInner::SourceAuthorityMismatch,
                ));
            }
        }
        let prefix_byte_end = u64::try_from(base_prefix_byte_end)
            .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?;
        let prefix_utf16_end = u64::try_from(base_prefix_utf16_end)
            .map_err(|_| M11ReferenceJournalError(ErrorInner::InvalidState))?;

        let root_id = self
            .root
            .as_ref()
            .ok_or(M11ReferenceJournalError(ErrorInner::InvalidState))?
            .id();
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
        let journal = match M11ReferenceJournal::new(runtime, target, self.authority.syntax_profile)
        {
            Ok(journal) => journal,
            Err(error) => {
                runtime.producer_arena_mut().abort_seal(seal)?;
                return Err(error);
            }
        };
        Ok(M11ReferenceJournalRangeReplacement {
            runtime_identity: self.runtime_identity,
            base_source: self.source,
            target,
            base_authority: self.authority,
            base_count: self.metadata.record_count,
            prefix_byte_end,
            prefix_utf16_end,
            phase: M11ReferenceJournalRangeReplacementPhase::RetainingBase,
            base_retain_seal: Some(seal),
            base_root: None,
            cursor: None,
            buffered_base: None,
            active_replay: None,
            suffix: None,
            journal: Some(journal),
            output: None,
            cancel_output: None,
            journal_cancel_complete: false,
            output_release_complete: true,
        })
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
            first_source_byte_start: self.first_source_byte_start,
            first_source_utf16_start: self.first_source_utf16_start,
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
    first_source_byte_start: u64,
    first_source_utf16_start: u64,
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
                        first_source_byte_start: self.first_source_byte_start,
                        first_source_utf16_start: self.first_source_utf16_start,
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

    fn finish_journal(
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
    ) -> M11ReferenceJournalRoot {
        journal.finish_input(runtime).expect("finish input");
        loop {
            let polled = journal.poll(runtime, 64).expect("finish journal");
            if polled.status() == M11ReferenceJournalStatus::Complete {
                return journal.take_root().expect("journal root");
            }
            assert_eq!(polled.status(), M11ReferenceJournalStatus::Pending);
        }
    }

    fn release_root(runtime: &mut DocumentRuntime, root: &mut M11ReferenceJournalRoot) {
        root.begin_release(runtime).expect("begin root release");
        while !root
            .poll_release(runtime, 64)
            .expect("poll root release")
            .complete()
        {}
    }

    fn offer_ascii_occurrence(
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
        source: Range<u64>,
        label_source: Range<u64>,
        destination_source: Range<u64>,
        label: &[u8],
        destination: &[u8],
    ) {
        journal
            .offer_occurrence(
                runtime,
                M11ReferenceJournalOccurrence::new(
                    M11ReferenceJournalRange::new(source.clone(), source),
                    M11ReferenceJournalRange::new(label_source.clone(), label_source),
                    M11ReferenceJournalRange::new(destination_source.clone(), destination_source),
                    None,
                    label,
                    destination,
                    None,
                ),
            )
            .expect("offer occurrence");
        settle_input(journal, runtime);
    }

    #[test]
    fn range_replacement_streams_shifted_suffix_and_matches_clean_target() {
        let large_tail = "x".repeat(BLOB_CHUNK_BYTES * 2 + 17);
        let base_text = format!("[a]: /old\n[b]: /{large_tail}\n");
        let first_end = "[a]: /old".len();
        let second_start = first_end + 1;
        let second_end = base_text.len() - 1;
        let second_destination_start = second_start + "[b]: ".len();
        let second_destination = &base_text.as_bytes()[second_destination_start..second_end];

        let mut runtime =
            DocumentRuntime::new(&base_text, DocumentRuntimeConfig::default()).expect("runtime");
        let base_source = runtime.current_source_version().expect("base source");
        let mut base_journal =
            M11ReferenceJournal::new(&mut runtime, base_source, 1).expect("base journal");
        offer_ascii_occurrence(
            &mut base_journal,
            &mut runtime,
            0..first_end as u64,
            1..2,
            5..first_end as u64,
            b"a",
            b"/old",
        );
        offer_ascii_occurrence(
            &mut base_journal,
            &mut runtime,
            second_start as u64..second_end as u64,
            (second_start + 1) as u64..(second_start + 2) as u64,
            second_destination_start as u64..second_end as u64,
            b"b",
            second_destination,
        );
        let mut base_root = finish_journal(&mut base_journal, &mut runtime);

        runtime
            .apply_edit(base_source, 5..first_end, "/new-target")
            .expect("edit first definition");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(base_source, second_start, second_start)
            .expect("exact second-definition suffix");
        let target_source = runtime.current_source_version().expect("target source");
        let target_text = format!("[a]: /new-target\n[b]: /{large_tail}\n");
        assert_eq!(target_source.byte_len(), target_text.len());

        let mut replacement = base_root
            .begin_range_replacement(&mut runtime, 0, 0, None)
            .expect("begin range replacement");
        loop {
            let polled = replacement
                .poll(&mut runtime, 64)
                .expect("replay base prefix");
            if polled.status() == M11ReferenceJournalRangeReplacementStatus::NeedsReplacementInput {
                break;
            }
            assert_eq!(
                polled.status(),
                M11ReferenceJournalRangeReplacementStatus::Pending
            );
        }
        let target_first_end = "[a]: /new-target".len();
        offer_ascii_occurrence(
            replacement
                .replacement_journal_mut()
                .expect("replacement journal"),
            &mut runtime,
            0..target_first_end as u64,
            1..2,
            5..target_first_end as u64,
            b"a",
            b"/new-target",
        );
        replacement
            .finish_replacement(&runtime, Some(suffix))
            .expect("finish replacement input");
        loop {
            let polled = replacement
                .poll(&mut runtime, 64)
                .expect("finish range replacement");
            if polled.status() == M11ReferenceJournalRangeReplacementStatus::Complete {
                break;
            }
            assert_eq!(
                polled.status(),
                M11ReferenceJournalRangeReplacementStatus::Pending
            );
        }
        let mut target_root = replacement.take_root().expect("replacement root");

        let mut clean_runtime =
            DocumentRuntime::new(&target_text, DocumentRuntimeConfig::default())
                .expect("clean runtime");
        let clean_source = clean_runtime
            .current_source_version()
            .expect("clean source");
        let mut clean_journal =
            M11ReferenceJournal::new(&mut clean_runtime, clean_source, 1).expect("clean journal");
        let target_second_start = target_first_end + 1;
        let target_second_end = target_text.len() - 1;
        let target_second_destination_start = target_second_start + "[b]: ".len();
        offer_ascii_occurrence(
            &mut clean_journal,
            &mut clean_runtime,
            0..target_first_end as u64,
            1..2,
            5..target_first_end as u64,
            b"a",
            b"/new-target",
        );
        offer_ascii_occurrence(
            &mut clean_journal,
            &mut clean_runtime,
            target_second_start as u64..target_second_end as u64,
            (target_second_start + 1) as u64..(target_second_start + 2) as u64,
            target_second_destination_start as u64..target_second_end as u64,
            b"b",
            target_text.as_bytes()[target_second_destination_start..target_second_end].as_ref(),
        );
        let mut clean_root = finish_journal(&mut clean_journal, &mut clean_runtime);

        assert_eq!(target_root.metadata, clean_root.metadata);
        assert_eq!(target_root.occurrence_count(), 2);
        assert_eq!(target_root.winner_ordinal(&runtime, b"a").unwrap(), Some(0));
        assert_eq!(target_root.winner_ordinal(&runtime, b"b").unwrap(), Some(1));
        let target_view = ReferenceRootView::open(
            runtime.producer_arena(),
            target_root.authority,
            target_root.root.as_ref().expect("target storage").id(),
        )
        .expect("target view");
        let replayed = target_view
            .occurrence(1)
            .expect("replayed suffix lookup")
            .expect("replayed suffix occurrence");
        assert!(replayed
            .cooked_destination
            .equals(second_destination)
            .expect("replayed destination"));
        assert_eq!(
            replayed.destination_source.bytes,
            target_second_destination_start as u64..target_second_end as u64
        );

        release_root(&mut runtime, &mut base_root);
        release_root(&mut runtime, &mut target_root);
        release_root(&mut clean_runtime, &mut clean_root);
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        clean_runtime.begin_close().expect("begin clean close");
        while !clean_runtime
            .poll_close(64)
            .expect("poll clean close")
            .complete
        {}
    }

    #[test]
    fn range_replacement_cancels_during_streamed_prefix_replay() {
        let destination = format!("/{}", "z".repeat(BLOB_CHUNK_BYTES * 2 + 9));
        let source_text = format!("[a]: {destination}\nvisible");
        let definition_end = source_text.find('\n').expect("definition end");
        let mut runtime =
            DocumentRuntime::new(&source_text, DocumentRuntimeConfig::default()).expect("runtime");
        let base_source = runtime.current_source_version().expect("base source");
        let mut journal = M11ReferenceJournal::new(&mut runtime, base_source, 1).expect("journal");
        offer_ascii_occurrence(
            &mut journal,
            &mut runtime,
            0..definition_end as u64,
            1..2,
            5..definition_end as u64,
            b"a",
            destination.as_bytes(),
        );
        let mut base_root = finish_journal(&mut journal, &mut runtime);

        runtime
            .apply_edit(base_source, source_text.len()..source_text.len(), "!")
            .expect("append target edit");
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(base_source, definition_end, definition_end)
            .expect("definition prefix");
        let mut replacement = base_root
            .begin_range_replacement(&mut runtime, definition_end, definition_end, Some(prefix))
            .expect("begin replacement");
        for _ in 0..8 {
            let polled = replacement.poll(&mut runtime, 1).expect("prefix replay");
            if polled.status() == M11ReferenceJournalRangeReplacementStatus::NeedsReplacementInput {
                break;
            }
        }
        replacement
            .begin_cancel(&mut runtime)
            .expect("begin cancellation");
        while !replacement
            .poll_cancel(&mut runtime, 64)
            .expect("poll cancellation")
            .complete()
        {}

        release_root(&mut runtime, &mut base_root);
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
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
