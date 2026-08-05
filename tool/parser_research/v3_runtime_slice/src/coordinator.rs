//! Reduced three-clock, latest-one coordinator.
//!
//! This state machine knows nothing about Markdown. It admits exact source
//! transitions, makes every superseded parse generation unpublishable, and
//! transfers persistent arena roots through a bounded worker/UI handoff.

use std::fmt;

use crate::{
    ArenaError, ArenaIdentity, ArenaScopedId, GrammarRevision, OwnedArenaRef, OwnerTransferError,
    PageArena, ParseGeneration, RemoteRootId, SourceRevision, SourceRootId, SourceTransition,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParseToken {
    pub generation: ParseGeneration,
    pub source_revision: SourceRevision,
    pub source_root: SourceRootId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputRootLease {
    pub remote_root: RemoteRootId,
    pub arena_root: ArenaScopedId,
    pub source_revision: SourceRevision,
    pub parse_generation: ParseGeneration,
    pub grammar_revision: GrammarRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsePlan {
    pub token: ParseToken,
    pub base_output: OutputRootLease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionReceipt {
    pub active: ParsePlan,
    pub queued: Option<ParsePlan>,
    pub replaced_queued: Option<ParseToken>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromotionReceipt {
    pub cancelled: ParseToken,
    pub promoted: ParsePlan,
    pub retired_candidate: Option<ArenaScopedId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationDelta {
    pub base_output: OutputRootLease,
    pub target_source_revision: SourceRevision,
    pub parse_generation: ParseGeneration,
    pub offered_output: OutputRootLease,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoordinatorMetrics {
    pub published_roots: usize,
    pub maximum_published_roots: usize,
    pub has_active_parse: bool,
    pub has_queued_parse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorError {
    InvalidTransition,
    SourceRevisionExhausted,
    ParseGenerationExhausted,
    RemoteRootExhausted,
    NoActiveParse,
    NoQueuedParse,
    InitialParseAlreadyAdmitted,
    WrongParseToken,
    StaleGeneration {
        supplied: ParseGeneration,
        current: ParseGeneration,
    },
    CandidateAlreadyAttached,
    CandidateMissing,
    UnknownRoot(RemoteRootId),
    LeaseMismatch(RemoteRootId),
    RootNotOffered(RemoteRootId),
    RootNotWorkerCurrent(RemoteRootId),
    DuplicateArenaRoot(ArenaScopedId),
    Arena(ArenaError),
    Invariant(&'static str),
}

impl From<ArenaError> for CoordinatorError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition => formatter.write_str("source transition is not contiguous"),
            Self::SourceRevisionExhausted => formatter.write_str("source revision exhausted"),
            Self::ParseGenerationExhausted => formatter.write_str("parse generation exhausted"),
            Self::RemoteRootExhausted => formatter.write_str("remote root identity exhausted"),
            Self::NoActiveParse => formatter.write_str("there is no active parse"),
            Self::NoQueuedParse => formatter.write_str("there is no queued parse"),
            Self::InitialParseAlreadyAdmitted => {
                formatter.write_str("initial revision-zero parse was already admitted")
            }
            Self::WrongParseToken => {
                formatter.write_str("parse token does not identify active work")
            }
            Self::StaleGeneration { supplied, current } => write!(
                formatter,
                "parse generation {supplied:?} is stale; current is {current:?}"
            ),
            Self::CandidateAlreadyAttached => {
                formatter.write_str("active parse already has an attached candidate root")
            }
            Self::CandidateMissing => formatter.write_str("active parse has no candidate root"),
            Self::UnknownRoot(root) => write!(formatter, "unknown output root {root:?}"),
            Self::LeaseMismatch(root) => {
                write!(formatter, "output root lease mismatch for {root:?}")
            }
            Self::RootNotOffered(root) => write!(formatter, "output root {root:?} is not offered"),
            Self::RootNotWorkerCurrent(root) => {
                write!(formatter, "output root {root:?} is not worker-current")
            }
            Self::DuplicateArenaRoot(root) => {
                write!(formatter, "arena root {root:?} is already published")
            }
            Self::Arena(error) => error.fmt(formatter),
            Self::Invariant(message) => {
                write!(formatter, "coordinator invariant failed: {message}")
            }
        }
    }
}

impl std::error::Error for CoordinatorError {}

/// A rejected candidate transfer returns the caller's linear authority.
///
/// Candidate attachment is the coordinator's only public operation that takes
/// ownership. Consequently it cannot report a bare [`CoordinatorError`]: doing
/// so would make a wrong arena, stale token, or duplicate root silently strand
/// the caller-owned arena reference.
#[must_use = "the rejected candidate owner must be recovered or deliberately handled"]
#[derive(Debug, PartialEq, Eq)]
pub struct AttachCandidateError {
    pub error: CoordinatorError,
    pub candidate: OwnedArenaRef,
}

impl fmt::Display for AttachCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for AttachCandidateError {}

/// One fully constructed output root paired with the grammar which produced
/// it. The owner and grammar travel as one linear publication capability, so
/// coordinator commit cannot derive parser compatibility from source clocks
/// or cross a recovered owner with a later free scalar.
#[must_use = "the candidate output must be published or deliberately released"]
#[derive(Debug, PartialEq, Eq)]
pub struct CandidateOutput {
    owner: OwnedArenaRef,
    grammar_revision: GrammarRevision,
}

/// Failed retirement of an unpublished candidate returns the complete
/// owner-plus-grammar bundle. This is the cancellation counterpart to
/// `PublishCandidateError`: latest-wins invalidation after arena commit cannot
/// strand the candidate merely because the caller supplied the wrong arena or
/// the release queue rejected the transfer.
#[must_use = "the rejected candidate output must be recovered or deliberately handled"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CandidateOutputReleaseError {
    pub(crate) error: ArenaError,
    pub(crate) candidate: CandidateOutput,
}

impl CandidateOutput {
    /// Sole production constructor for an exact-parser output. The storage
    /// module can mint this carrier only after revalidating one committed
    /// restart-composite manifest; callers can therefore never cross an arena
    /// owner with an independently supplied grammar scalar.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn from_restart_composite_mint(
        mint: crate::storage_only_composite_document::RestartCompositeCandidateOutputMint,
    ) -> Self {
        let (owner, grammar_revision) = mint.into_candidate_output_parts();
        Self {
            owner,
            grammar_revision,
        }
    }

    /// Test-only mechanism seam. Production construction must be owned by the
    /// validated composite-manifest handoff, where the root owner and grammar
    /// revision are recovered from the same committed output rather than
    /// accepted as independently caller-supplied values.
    #[cfg(test)]
    #[must_use]
    const fn mechanism_only_for_test(
        owner: OwnedArenaRef,
        grammar_revision: GrammarRevision,
    ) -> Self {
        Self {
            owner,
            grammar_revision,
        }
    }

    #[must_use]
    pub const fn arena_root(&self) -> ArenaScopedId {
        self.owner.scoped_id()
    }

    #[must_use]
    pub const fn grammar_revision(&self) -> GrammarRevision {
        self.grammar_revision
    }

    pub(crate) fn release_later(
        self,
        arena: &mut PageArena,
    ) -> Result<(), CandidateOutputReleaseError> {
        let Self {
            owner,
            grammar_revision,
        } = self;
        match arena.release_later(owner) {
            Ok(()) => Ok(()),
            Err(failure) => Err(CandidateOutputReleaseError {
                error: failure.error,
                candidate: Self {
                    owner: failure.owner,
                    grammar_revision,
                },
            }),
        }
    }
}

/// Atomic publication failure. No pre-publication failure consumes or splits
/// the candidate bundle; callers can repair coordinator state and retry the
/// exact same owner-plus-grammar capability.
#[must_use = "the rejected candidate output must be recovered or deliberately handled"]
#[derive(Debug, PartialEq, Eq)]
pub struct PublishCandidateError {
    pub error: CoordinatorError,
    pub candidate: CandidateOutput,
}

impl fmt::Display for PublishCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for PublishCandidateError {}

#[derive(Debug, PartialEq, Eq)]
struct ActiveParse {
    plan: ParsePlan,
    candidate: Option<OwnedArenaRef>,
}

#[derive(Debug, PartialEq, Eq)]
struct PublishedRoot {
    lease: OutputRootLease,
    owner: OwnedArenaRef,
    worker_current: bool,
    offered: bool,
    acknowledged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CoordinatorAdmissionEpoch {
    source_revision: SourceRevision,
    source_root: SourceRootId,
    grammar_revision: GrammarRevision,
    parse_generation: ParseGeneration,
    current_output: OutputRootLease,
    active: Option<ParsePlan>,
    active_candidate: Option<ArenaScopedId>,
    queued: Option<ParsePlan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdmissionPlacement {
    Active,
    Queued,
}

/// Fully validated next coordinator clock state. Construction is crate-private
/// and the value is linear so a scalar [`SourceTransition`] is never commit
/// authority by itself.
#[derive(Debug)]
pub(crate) struct PreparedSourceTransition {
    expected: CoordinatorAdmissionEpoch,
    transition: SourceTransition,
    generation: ParseGeneration,
    plan: ParsePlan,
    placement: AdmissionPlacement,
    receipt: AdmissionReceipt,
}

/// Grammar-free coordinator for source, parser, and output lifetime clocks.
#[derive(Debug)]
pub struct Coordinator {
    arena_identity: ArenaIdentity,
    source_revision: SourceRevision,
    source_root: SourceRootId,
    grammar_revision: GrammarRevision,
    parse_generation: ParseGeneration,
    next_remote_root: u64,
    active: Option<ActiveParse>,
    queued: Option<ParsePlan>,
    published: Vec<PublishedRoot>,
    maximum_published_roots: usize,
}

impl Coordinator {
    /// Takes ownership of the caller-owned reference for `initial_output`.
    #[must_use]
    pub fn new(initial_source_root: SourceRootId, initial_output: OwnedArenaRef) -> Self {
        let initial_output_id = initial_output.scoped_id();
        let initial_lease = OutputRootLease {
            remote_root: RemoteRootId(1),
            arena_root: initial_output_id,
            source_revision: SourceRevision(0),
            parse_generation: ParseGeneration(0),
            grammar_revision: GrammarRevision(0),
        };
        Self {
            arena_identity: initial_output_id.arena(),
            source_revision: SourceRevision(0),
            source_root: initial_source_root,
            grammar_revision: GrammarRevision(0),
            parse_generation: ParseGeneration(0),
            next_remote_root: 2,
            active: None,
            queued: None,
            published: vec![PublishedRoot {
                lease: initial_lease,
                owner: initial_output,
                worker_current: true,
                offered: false,
                acknowledged: true,
            }],
            maximum_published_roots: 1,
        }
    }

    /// The single arena namespace in which all coordinator roots live.
    #[must_use]
    pub const fn arena_identity(&self) -> ArenaIdentity {
        self.arena_identity
    }

    #[must_use]
    pub const fn source_revision(&self) -> SourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn source_root(&self) -> SourceRootId {
        self.source_root
    }

    #[must_use]
    pub const fn grammar_revision(&self) -> GrammarRevision {
        self.grammar_revision
    }

    #[must_use]
    pub const fn parse_generation(&self) -> ParseGeneration {
        self.parse_generation
    }

    #[must_use]
    pub fn active_plan(&self) -> Option<ParsePlan> {
        self.active.as_ref().map(|active| active.plan)
    }

    #[must_use]
    pub const fn queued_plan(&self) -> Option<ParsePlan> {
        self.queued
    }

    #[must_use]
    pub fn current_output(&self) -> OutputRootLease {
        self.published
            .iter()
            .find(|root| root.worker_current)
            .expect("coordinator always owns one current output")
            .lease
    }

    #[must_use]
    pub fn acknowledged_output(&self) -> Option<OutputRootLease> {
        self.published
            .iter()
            .find(|root| root.acknowledged)
            .map(|root| root.lease)
    }

    #[must_use]
    pub fn offered_output(&self) -> Option<OutputRootLease> {
        self.published
            .iter()
            .find(|root| root.offered)
            .map(|root| root.lease)
    }

    #[must_use]
    pub fn metrics(&self) -> CoordinatorMetrics {
        CoordinatorMetrics {
            published_roots: self.published.len(),
            maximum_published_roots: self.maximum_published_roots,
            has_active_parse: self.active.is_some(),
            has_queued_parse: self.queued.is_some(),
        }
    }

    /// Admits the generation-one parse of the exact revision-zero source.
    /// Generation zero remains the bootstrap output and is never presented as
    /// a completed parse of a new document.
    pub fn begin_initial_parse(&mut self) -> Result<ParsePlan, CoordinatorError> {
        if self.source_revision != SourceRevision(0)
            || self.parse_generation != ParseGeneration(0)
            || self.active.is_some()
            || self.queued.is_some()
        {
            return Err(CoordinatorError::InitialParseAlreadyAdmitted);
        }
        let generation = ParseGeneration(1);
        let plan = ParsePlan {
            token: ParseToken {
                generation,
                source_revision: self.source_revision,
                source_root: self.source_root,
            },
            base_output: self.current_output(),
        };
        self.parse_generation = generation;
        self.active = Some(ActiveParse {
            plan,
            candidate: None,
        });
        Ok(plan)
    }

    /// Accepts one exact source transition and admits only the newest waiting
    /// plan. The coordinator retains identities, never a historical source.
    pub fn accept_source_transition(
        &mut self,
        transition: SourceTransition,
    ) -> Result<AdmissionReceipt, CoordinatorError> {
        let prepared = self.prepare_source_transition(transition)?;
        Ok(self.commit_prepared_source_transition(prepared))
    }

    /// Performs every fallible validation for a source-clock admission without
    /// changing coordinator state. The live-document actor pairs this linear
    /// value with a prepared source edit before publishing either clock.
    pub(crate) fn prepare_source_transition(
        &self,
        transition: SourceTransition,
    ) -> Result<PreparedSourceTransition, CoordinatorError> {
        self.check_root_bound()?;
        let expected_target = self
            .source_revision
            .0
            .checked_add(1)
            .ok_or(CoordinatorError::SourceRevisionExhausted)?;
        if transition.base_revision != self.source_revision
            || transition.target_revision != SourceRevision(expected_target)
            || transition.base_root != self.source_root
            || transition.result_root == SourceRootId(0)
            || transition.result_root == transition.base_root
        {
            return Err(CoordinatorError::InvalidTransition);
        }
        let next_generation = ParseGeneration(
            self.parse_generation
                .0
                .checked_add(1)
                .ok_or(CoordinatorError::ParseGenerationExhausted)?,
        );
        let plan = ParsePlan {
            token: ParseToken {
                generation: next_generation,
                source_revision: transition.target_revision,
                source_root: transition.result_root,
            },
            base_output: self.current_output(),
        };
        let (placement, active, queued, replaced_queued) = if let Some(active) = &self.active {
            (
                AdmissionPlacement::Queued,
                active.plan,
                Some(plan),
                self.queued.map(|queued| queued.token),
            )
        } else {
            if self.queued.is_some() {
                return Err(CoordinatorError::Invariant(
                    "queued parse exists without active parse",
                ));
            }
            (AdmissionPlacement::Active, plan, None, None)
        };
        let receipt = AdmissionReceipt {
            active,
            queued,
            replaced_queued,
        };
        Ok(PreparedSourceTransition {
            expected: self.admission_epoch(),
            transition,
            generation: next_generation,
            plan,
            placement,
            receipt,
        })
    }

    /// Publishes a prepared transition using assignments only. The invariant
    /// assertion runs before mutation and catches accidental cross-coordinator
    /// use without creating a half-published clock.
    #[allow(clippy::needless_pass_by_value)] // Moving this value is the linear commit authority.
    pub(crate) fn commit_prepared_source_transition(
        &mut self,
        prepared: PreparedSourceTransition,
    ) -> AdmissionReceipt {
        assert_eq!(
            self.admission_epoch(),
            prepared.expected,
            "prepared source transition must commit to the coordinator that issued it"
        );
        let PreparedSourceTransition {
            expected: _,
            transition,
            generation,
            plan,
            placement,
            receipt,
        } = prepared;
        self.source_revision = transition.target_revision;
        self.source_root = transition.result_root;
        self.parse_generation = generation;
        match placement {
            AdmissionPlacement::Active => {
                self.active = Some(ActiveParse {
                    plan,
                    candidate: None,
                });
            }
            AdmissionPlacement::Queued => self.queued = Some(plan),
        }
        receipt
    }

    /// Takes ownership of one caller-owned arena reference on success.
    pub fn attach_candidate(
        &mut self,
        token: ParseToken,
        candidate: OwnedArenaRef,
        arena: &mut PageArena,
    ) -> Result<(), AttachCandidateError> {
        if let Err(error) = self.require_arena(arena) {
            return Err(AttachCandidateError { error, candidate });
        }
        let candidate_id = candidate.scoped_id();
        if let Err(error) = arena
            .local_id(candidate_id)
            .map_err(CoordinatorError::Arena)
        {
            return Err(AttachCandidateError { error, candidate });
        }
        if let Err(error) = self.require_current_active(token) {
            return Err(AttachCandidateError { error, candidate });
        }
        if self
            .published
            .iter()
            .any(|root| root.lease.arena_root == candidate_id)
        {
            return Err(AttachCandidateError {
                error: CoordinatorError::DuplicateArenaRoot(candidate_id),
                candidate,
            });
        }
        let Some(active) = self.active.as_mut() else {
            return Err(AttachCandidateError {
                error: CoordinatorError::NoActiveParse,
                candidate,
            });
        };
        if active.candidate.is_some() {
            return Err(AttachCandidateError {
                error: CoordinatorError::CandidateAlreadyAttached,
                candidate,
            });
        }
        active.candidate = Some(candidate);
        Ok(())
    }

    /// Cancels the old active generation, schedules its candidate for bounded
    /// retirement, and promotes the single newest queued plan.
    pub fn promote_latest(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<PromotionReceipt, CoordinatorError> {
        self.promote_latest_with(arena, PageArena::release_later)
    }

    fn promote_latest_with<Release>(
        &mut self,
        arena: &mut PageArena,
        release: Release,
    ) -> Result<PromotionReceipt, CoordinatorError>
    where
        Release: FnOnce(&mut PageArena, OwnedArenaRef) -> Result<(), OwnerTransferError>,
    {
        self.require_arena(arena)?;
        let promoted = self.queued.ok_or(CoordinatorError::NoQueuedParse)?;
        let active = self
            .active
            .as_ref()
            .ok_or(CoordinatorError::NoActiveParse)?;
        if let Some(candidate) = active.candidate.as_ref() {
            arena.preflight_release(candidate)?;
        }
        let mut cancelled = self.active.take().ok_or(CoordinatorError::NoActiveParse)?;
        let retired_candidate = cancelled.candidate.as_ref().map(OwnedArenaRef::scoped_id);
        if let Some(candidate) = cancelled.candidate.take()
            && let Err(failure) = release(arena, candidate)
        {
            cancelled.candidate = Some(failure.owner);
            self.active = Some(cancelled);
            return Err(CoordinatorError::Arena(failure.error));
        }
        self.active = Some(ActiveParse {
            plan: promoted,
            candidate: None,
        });
        self.queued = None;
        Ok(PromotionReceipt {
            cancelled: cancelled.plan.token,
            promoted,
            retired_candidate,
        })
    }

    /// Atomically publishes a caller-owned output root as worker-current.
    ///
    /// Unlike the legacy attach-then-commit pair, this transition keeps the
    /// root owner and its authenticated grammar revision in one linear bundle.
    /// Every fallible coordinator, arena, and old-root retirement check occurs
    /// before the candidate is consumed; failures return the intact bundle.
    pub fn publish_candidate(
        &mut self,
        token: ParseToken,
        candidate: CandidateOutput,
        arena: &mut PageArena,
    ) -> Result<PublicationDelta, PublishCandidateError> {
        self.publish_candidate_with(token, candidate, arena, PageArena::release_later)
    }

    fn publish_candidate_with<Release>(
        &mut self,
        token: ParseToken,
        candidate: CandidateOutput,
        arena: &mut PageArena,
        release: Release,
    ) -> Result<PublicationDelta, PublishCandidateError>
    where
        Release: FnOnce(&mut PageArena, OwnedArenaRef) -> Result<(), OwnerTransferError>,
    {
        if let Err(error) = self.require_arena(arena) {
            return Err(PublishCandidateError { error, candidate });
        }
        if let Err(error) = self.require_current_active(token) {
            return Err(PublishCandidateError { error, candidate });
        }
        if let Err(error) = self.check_root_bound() {
            return Err(PublishCandidateError { error, candidate });
        }
        let candidate_id = candidate.arena_root();
        if let Err(error) = arena
            .local_id(candidate_id)
            .map_err(CoordinatorError::Arena)
        {
            return Err(PublishCandidateError { error, candidate });
        }
        if self
            .published
            .iter()
            .any(|root| root.lease.arena_root == candidate_id)
        {
            return Err(PublishCandidateError {
                error: CoordinatorError::DuplicateArenaRoot(candidate_id),
                candidate,
            });
        }
        let Some(active) = self.active.as_ref() else {
            return Err(PublishCandidateError {
                error: CoordinatorError::NoActiveParse,
                candidate,
            });
        };
        if active.candidate.is_some() {
            return Err(PublishCandidateError {
                error: CoordinatorError::CandidateAlreadyAttached,
                candidate,
            });
        }

        let remote_root = RemoteRootId(self.next_remote_root);
        let Some(next_remote_root) = self.next_remote_root.checked_add(1) else {
            return Err(PublishCandidateError {
                error: CoordinatorError::RemoteRootExhausted,
                candidate,
            });
        };
        let lease = OutputRootLease {
            remote_root,
            arena_root: candidate_id,
            source_revision: token.source_revision,
            parse_generation: token.generation,
            grammar_revision: candidate.grammar_revision(),
        };

        let mut retired_index = None;
        for (index, root) in self.published.iter().enumerate() {
            if !root.acknowledged && retired_index.replace(index).is_some() {
                return Err(PublishCandidateError {
                    error: CoordinatorError::Invariant(
                        "commit would retire more than one prior root",
                    ),
                    candidate,
                });
            }
        }
        let Some(prospective_root_count) = self.published.len().checked_add(1) else {
            return Err(PublishCandidateError {
                error: CoordinatorError::Invariant("published-root count overflow"),
                candidate,
            });
        };
        let retained_root_count = prospective_root_count - usize::from(retired_index.is_some());
        if retained_root_count > 3 {
            return Err(PublishCandidateError {
                error: CoordinatorError::Invariant(
                    "more than three output roots would be published",
                ),
                candidate,
            });
        }
        if self.published.try_reserve(1).is_err() {
            return Err(PublishCandidateError {
                error: CoordinatorError::Arena(ArenaError::AllocationFailed(
                    "coordinator published-root slot",
                )),
                candidate,
            });
        }
        if let Some(index) = retired_index
            && let Err(error) = self.release_published_at_with(index, arena, release)
        {
            return Err(PublishCandidateError { error, candidate });
        }

        // Old-root retirement was the final fallible step. The candidate
        // bundle is consumed only now, and every remaining mutation is an
        // infallible assignment under the preflighted invariants above.
        let CandidateOutput {
            owner,
            grammar_revision,
        } = candidate;
        let active = self.active.take().expect("active parse was preflighted");
        debug_assert!(active.candidate.is_none());
        for root in &mut self.published {
            root.worker_current = false;
            root.offered = false;
        }
        self.published.push(PublishedRoot {
            lease,
            owner,
            worker_current: true,
            offered: true,
            acknowledged: false,
        });
        self.maximum_published_roots = self.maximum_published_roots.max(self.published.len());
        self.grammar_revision = grammar_revision;
        self.next_remote_root = next_remote_root;
        self.active = None;
        debug_assert!(self.check_root_bound().is_ok());

        Ok(PublicationDelta {
            base_output: active.plan.base_output,
            target_source_revision: token.source_revision,
            parse_generation: token.generation,
            offered_output: lease,
        })
    }

    /// Publishes an exact latest-generation candidate as the worker-current
    /// and sole offered root. Superseded offers are retired immediately.
    pub fn commit(
        &mut self,
        token: ParseToken,
        arena: &mut PageArena,
    ) -> Result<PublicationDelta, CoordinatorError> {
        self.require_arena(arena)?;
        self.require_current_active(token)?;
        self.check_root_bound()?;
        let active = self
            .active
            .as_ref()
            .ok_or(CoordinatorError::NoActiveParse)?;
        let candidate_id = active
            .candidate
            .as_ref()
            .ok_or(CoordinatorError::CandidateMissing)?
            .scoped_id();
        arena.local_id(candidate_id)?;
        let remote_root = RemoteRootId(self.next_remote_root);
        let next_remote_root = self
            .next_remote_root
            .checked_add(1)
            .ok_or(CoordinatorError::RemoteRootExhausted)?;
        let grammar_revision = GrammarRevision(token.source_revision.0);
        let lease = OutputRootLease {
            remote_root,
            arena_root: candidate_id,
            source_revision: token.source_revision,
            parse_generation: token.generation,
            grammar_revision,
        };

        // Committing clears worker-current/offered from every old root. Under
        // the coordinator invariants this can make at most one root unowned.
        // Retire that owner before the publication mutation so a failed
        // ownership transfer can be restored exactly and no half-commit is
        // observable.
        let mut retired_index = None;
        for (index, root) in self.published.iter().enumerate() {
            if !root.acknowledged && retired_index.replace(index).is_some() {
                return Err(CoordinatorError::Invariant(
                    "commit would retire more than one prior root",
                ));
            }
        }
        let prospective_root_count = self
            .published
            .len()
            .checked_add(1)
            .ok_or(CoordinatorError::Invariant("published-root count overflow"))?;
        let retained_root_count = prospective_root_count - usize::from(retired_index.is_some());
        if retained_root_count > 3 {
            return Err(CoordinatorError::Invariant(
                "more than three output roots would be published",
            ));
        }
        if let Some(index) = retired_index {
            self.release_published_at(index, arena)?;
        }

        // Everything after the ownership transfer is infallible under the
        // preflight above.
        let active = self.active.take().expect("active parse was preflighted");
        let candidate = active
            .candidate
            .expect("candidate ownership was preflighted");

        for root in &mut self.published {
            root.worker_current = false;
            root.offered = false;
        }
        self.published.push(PublishedRoot {
            lease,
            owner: candidate,
            worker_current: true,
            offered: true,
            acknowledged: false,
        });
        self.maximum_published_roots = self.maximum_published_roots.max(self.published.len());
        self.grammar_revision = grammar_revision;
        self.next_remote_root = next_remote_root;
        self.active = None;
        debug_assert!(self.check_root_bound().is_ok());

        Ok(PublicationDelta {
            base_output: active.plan.base_output,
            target_source_revision: token.source_revision,
            parse_generation: token.generation,
            offered_output: lease,
        })
    }

    /// Atomically moves UI acknowledgement to the current offer and schedules
    /// the previous acknowledged root for bounded retirement.
    pub fn acknowledge(
        &mut self,
        lease: OutputRootLease,
        arena: &mut PageArena,
    ) -> Result<(), CoordinatorError> {
        self.require_arena(arena)?;
        self.check_root_bound()?;
        let index = self.find_exact_lease(lease)?;
        if self.published[index].acknowledged {
            return Ok(());
        }
        if !self.published[index].offered {
            return Err(CoordinatorError::RootNotOffered(lease.remote_root));
        }

        let mut retired_index = None;
        for (candidate_index, root) in self.published.iter().enumerate() {
            if candidate_index != index
                && !root.worker_current
                && retired_index.replace(candidate_index).is_some()
            {
                return Err(CoordinatorError::Invariant(
                    "acknowledgement would retire more than one prior root",
                ));
            }
        }
        if let Some(retired_index) = retired_index {
            self.release_published_at(retired_index, arena)?;
        }
        let index = self
            .find_exact_lease(lease)
            .expect("acknowledged offer survives retirement preflight");
        for root in &mut self.published {
            root.acknowledged = false;
            root.offered = false;
        }
        self.published[index].acknowledged = true;
        debug_assert!(self.check_root_bound().is_ok());
        Ok(())
    }

    /// Revokes remote access to an offered or acknowledged root. A released
    /// worker-current root remains internally retained until the next commit.
    pub fn release_root(
        &mut self,
        lease: OutputRootLease,
        arena: &mut PageArena,
    ) -> Result<(), CoordinatorError> {
        self.require_arena(arena)?;
        self.check_root_bound()?;
        let index = self.find_exact_lease(lease)?;
        if !self.published[index].offered && !self.published[index].acknowledged {
            return Err(CoordinatorError::UnknownRoot(lease.remote_root));
        }
        if !self.published[index].worker_current {
            self.release_published_at(index, arena)?;
            debug_assert!(self.check_root_bound().is_ok());
            return Ok(());
        }
        self.published[index].offered = false;
        self.published[index].acknowledged = false;
        debug_assert!(self.check_root_bound().is_ok());
        Ok(())
    }

    /// Resolves an exact live lease. Released, generation-mismatched, or
    /// revision-mismatched leases can never alias a newer arena occupant.
    pub fn resolve_root(
        &self,
        lease: OutputRootLease,
        arena: &PageArena,
    ) -> Result<ArenaScopedId, CoordinatorError> {
        self.require_arena(arena)?;
        let index = self.find_exact_lease(lease)?;
        if !self.published[index].offered && !self.published[index].acknowledged {
            return Err(CoordinatorError::UnknownRoot(lease.remote_root));
        }
        let root = self.published[index].lease.arena_root;
        arena.local_id(root).map_err(CoordinatorError::Arena)?;
        Ok(root)
    }

    /// Resolves only the exact root which the worker currently uses as its
    /// parse base. Remote release revokes UI access but deliberately does not
    /// invalidate this internal binding; a later publication is what retires
    /// the old worker-current root.
    pub fn resolve_worker_current(
        &self,
        lease: OutputRootLease,
        arena: &PageArena,
    ) -> Result<ArenaScopedId, CoordinatorError> {
        self.require_arena(arena)?;
        let index = self.find_exact_lease(lease)?;
        if !self.published[index].worker_current {
            return Err(CoordinatorError::RootNotWorkerCurrent(lease.remote_root));
        }
        let root = self.published[index].lease.arena_root;
        arena.local_id(root).map_err(CoordinatorError::Arena)?;
        Ok(root)
    }

    pub fn query_payload<'arena>(
        &self,
        lease: OutputRootLease,
        arena: &'arena PageArena,
    ) -> Result<&'arena [u8], CoordinatorError> {
        self.require_arena(arena)?;
        let root = self.resolve_root(lease, arena)?;
        let local = arena.local_id(root)?;
        arena.payload(local).map_err(CoordinatorError::Arena)
    }

    fn admission_epoch(&self) -> CoordinatorAdmissionEpoch {
        CoordinatorAdmissionEpoch {
            source_revision: self.source_revision,
            source_root: self.source_root,
            grammar_revision: self.grammar_revision,
            parse_generation: self.parse_generation,
            current_output: self.current_output(),
            active: self.active.as_ref().map(|active| active.plan),
            active_candidate: self
                .active
                .as_ref()
                .and_then(|active| active.candidate.as_ref())
                .map(OwnedArenaRef::scoped_id),
            queued: self.queued,
        }
    }

    fn require_current_active(&self, token: ParseToken) -> Result<(), CoordinatorError> {
        let Some(active) = self.active.as_ref() else {
            return Err(CoordinatorError::NoActiveParse);
        };
        if active.plan.token != token {
            if token.generation != self.parse_generation {
                return Err(CoordinatorError::StaleGeneration {
                    supplied: token.generation,
                    current: self.parse_generation,
                });
            }
            return Err(CoordinatorError::WrongParseToken);
        }
        if token.generation != self.parse_generation || self.queued.is_some() {
            return Err(CoordinatorError::StaleGeneration {
                supplied: token.generation,
                current: self.parse_generation,
            });
        }
        Ok(())
    }

    fn require_arena(&self, arena: &PageArena) -> Result<(), CoordinatorError> {
        if arena.identity() != self.arena_identity {
            return Err(CoordinatorError::Arena(ArenaError::WrongArena {
                expected: self.arena_identity,
                actual: arena.identity(),
            }));
        }
        Ok(())
    }

    fn find_exact_lease(&self, lease: OutputRootLease) -> Result<usize, CoordinatorError> {
        let Some((index, root)) = self
            .published
            .iter()
            .enumerate()
            .find(|(_, root)| root.lease.remote_root == lease.remote_root)
        else {
            return Err(CoordinatorError::UnknownRoot(lease.remote_root));
        };
        if root.lease != lease {
            return Err(CoordinatorError::LeaseMismatch(lease.remote_root));
        }
        Ok(index)
    }

    fn release_published_at(
        &mut self,
        index: usize,
        arena: &mut PageArena,
    ) -> Result<(), CoordinatorError> {
        self.release_published_at_with(index, arena, PageArena::release_later)
    }

    fn release_published_at_with<Release>(
        &mut self,
        index: usize,
        arena: &mut PageArena,
        release: Release,
    ) -> Result<(), CoordinatorError>
    where
        Release: FnOnce(&mut PageArena, OwnedArenaRef) -> Result<(), OwnerTransferError>,
    {
        arena.preflight_release(&self.published[index].owner)?;
        let PublishedRoot {
            lease,
            owner,
            worker_current,
            offered,
            acknowledged,
        } = self.published.remove(index);
        if let Err(failure) = release(arena, owner) {
            self.published.insert(
                index,
                PublishedRoot {
                    lease,
                    owner: failure.owner,
                    worker_current,
                    offered,
                    acknowledged,
                },
            );
            return Err(CoordinatorError::Arena(failure.error));
        }
        Ok(())
    }

    fn check_root_bound(&self) -> Result<(), CoordinatorError> {
        if self.published.len() > 3 {
            return Err(CoordinatorError::Invariant(
                "more than three output roots are published",
            ));
        }
        if self
            .published
            .iter()
            .filter(|root| root.worker_current)
            .count()
            != 1
        {
            return Err(CoordinatorError::Invariant(
                "coordinator must own one worker-current root",
            ));
        }
        let current = self
            .published
            .iter()
            .find(|root| root.worker_current)
            .expect("worker-current cardinality was checked")
            .lease;
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.plan.base_output != current)
            || self
                .queued
                .is_some_and(|queued| queued.base_output != current)
        {
            return Err(CoordinatorError::Invariant(
                "admitted parse base is not the worker-current output",
            ));
        }
        if self
            .published
            .iter()
            .filter(|root| root.acknowledged)
            .count()
            > 1
        {
            return Err(CoordinatorError::Invariant(
                "coordinator may own at most one acknowledged root",
            ));
        }
        if self.published.iter().filter(|root| root.offered).count() > 1 {
            return Err(CoordinatorError::Invariant(
                "coordinator may own at most one offered root",
            ));
        }
        for (index, root) in self.published.iter().enumerate() {
            if !root.worker_current && !root.offered && !root.acknowledged {
                return Err(CoordinatorError::Invariant(
                    "published root has no coordinator role",
                ));
            }
            if self
                .published
                .iter()
                .skip(index + 1)
                .any(|other| other.lease.arena_root == root.lease.arena_root)
            {
                return Err(CoordinatorError::Invariant(
                    "one arena root is published under multiple leases",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceStore;

    #[test]
    fn rejected_coordinator_preflight_does_not_publish_a_prepared_source() {
        let source = SourceStore::new("abc", 8);
        let source_before = source.descriptor();
        let text_before = source.query_snapshot().materialize_for_testing();
        let prepared = source
            .prepare_edit(source_before, 0..0, "x")
            .expect("source edit can be fully prepared");
        assert_eq!(source.descriptor(), source_before);

        let mut arena = PageArena::new();
        let initial = arena.allocate(b"initial", &[]).unwrap().owner;
        let coordinator = Coordinator::new(SourceRootId(u64::MAX), initial);
        let coordinator_before = coordinator.admission_epoch();
        assert!(matches!(
            coordinator.prepare_source_transition(prepared.transition()),
            Err(CoordinatorError::InvalidTransition)
        ));

        drop(prepared);
        assert_eq!(source.descriptor(), source_before);
        assert_eq!(
            source.query_snapshot().materialize_for_testing(),
            text_before
        );
        assert_eq!(coordinator.admission_epoch(), coordinator_before);
    }

    #[test]
    fn promotion_restores_linear_owner_and_every_field_on_release_failure() {
        let mut arena = PageArena::new();
        let initial = arena
            .allocate(b"initial", &[])
            .expect("initial allocation")
            .owner;
        let mut coordinator = Coordinator::new(SourceRootId(1), initial);
        let first = SourceTransition {
            base_revision: SourceRevision(0),
            target_revision: SourceRevision(1),
            base_root: SourceRootId(1),
            result_root: SourceRootId(2),
        };
        let first_token = coordinator
            .accept_source_transition(first)
            .expect("first parse")
            .active
            .token;
        let candidate = arena
            .allocate(b"candidate", &[])
            .expect("candidate allocation")
            .owner;
        let candidate_id = candidate.id();
        coordinator
            .attach_candidate(first_token, candidate, &mut arena)
            .expect("candidate attachment");
        let second = SourceTransition {
            base_revision: SourceRevision(1),
            target_revision: SourceRevision(2),
            base_root: SourceRootId(2),
            result_root: SourceRootId(3),
        };
        let promoted = coordinator
            .accept_source_transition(second)
            .expect("queued parse")
            .queued
            .expect("latest queued")
            .token;

        let coordinator_before = format!("{coordinator:?}");
        let arena_before = arena.metrics();
        let error = coordinator
            .promote_latest_with(&mut arena, |_arena, owner| {
                Err(OwnerTransferError {
                    error: ArenaError::Invariant("injected release failure"),
                    owner,
                })
            })
            .expect_err("injected ownership transfer failure");
        assert_eq!(
            error,
            CoordinatorError::Arena(ArenaError::Invariant("injected release failure"))
        );
        assert_eq!(format!("{coordinator:?}"), coordinator_before);
        assert_eq!(arena.metrics(), arena_before);
        assert!(arena.contains(candidate_id));

        let receipt = coordinator
            .promote_latest(&mut arena)
            .expect("restored authority remains promotable");
        assert_eq!(receipt.promoted.token, promoted);
        assert_eq!(
            receipt.retired_candidate.map(ArenaScopedId::local),
            Some(candidate_id)
        );
    }

    #[test]
    fn atomic_publish_returns_intact_bundle_and_uses_root_bound_grammar() {
        let mut arena = PageArena::new();
        let initial = arena
            .allocate(b"initial", &[])
            .expect("initial allocation")
            .owner;
        let mut coordinator = Coordinator::new(SourceRootId(1), initial);
        let token = coordinator
            .begin_initial_parse()
            .expect("revision-zero parse admission")
            .token;
        let candidate_owner = arena
            .allocate(b"candidate", &[])
            .expect("candidate allocation")
            .owner;
        let candidate =
            CandidateOutput::mechanism_only_for_test(candidate_owner, GrammarRevision(77));
        let candidate_root = candidate.arena_root();
        let wrong_token = ParseToken {
            source_root: SourceRootId(2),
            ..token
        };
        let coordinator_before = format!("{coordinator:?}");
        let arena_before = arena.metrics();

        let failure = coordinator
            .publish_candidate(wrong_token, candidate, &mut arena)
            .expect_err("wrong source identity must not consume publication authority");
        assert_eq!(failure.error, CoordinatorError::WrongParseToken);
        assert_eq!(failure.candidate.arena_root(), candidate_root);
        assert_eq!(failure.candidate.grammar_revision(), GrammarRevision(77));
        assert_eq!(format!("{coordinator:?}"), coordinator_before);
        assert_eq!(arena.metrics(), arena_before);

        let publication = coordinator
            .publish_candidate(token, failure.candidate, &mut arena)
            .expect("the exact recovered bundle remains publishable");
        assert_eq!(publication.offered_output.arena_root, candidate_root);
        assert_eq!(
            publication.offered_output.source_revision,
            SourceRevision(0)
        );
        assert_eq!(
            publication.offered_output.grammar_revision,
            GrammarRevision(77)
        );
        assert_eq!(coordinator.grammar_revision(), GrammarRevision(77));
        assert_eq!(coordinator.current_output(), publication.offered_output);
        assert_eq!(
            coordinator
                .resolve_worker_current(publication.offered_output, &arena)
                .unwrap(),
            candidate_root
        );

        // A source-derived grammar guess is not equivalent to the grammar
        // bound to this root, even though every other lease field is exact.
        let source_derived_grammar = OutputRootLease {
            grammar_revision: GrammarRevision(token.source_revision.0),
            ..publication.offered_output
        };
        assert_eq!(
            coordinator.resolve_worker_current(source_derived_grammar, &arena),
            Err(CoordinatorError::LeaseMismatch(
                publication.offered_output.remote_root
            ))
        );
    }

    #[test]
    fn atomic_publish_restores_old_root_and_bundle_on_release_failure() {
        let mut arena = PageArena::new();
        let initial = arena
            .allocate(b"initial", &[])
            .expect("initial allocation")
            .owner;
        let mut coordinator = Coordinator::new(SourceRootId(1), initial);
        let first_token = coordinator
            .begin_initial_parse()
            .expect("initial parse admission")
            .token;
        let first_owner = arena
            .allocate(b"first", &[])
            .expect("first candidate allocation")
            .owner;
        let first = coordinator
            .publish_candidate(
                first_token,
                CandidateOutput::mechanism_only_for_test(first_owner, GrammarRevision(11)),
                &mut arena,
            )
            .expect("first publication")
            .offered_output;
        let second_token = coordinator
            .accept_source_transition(SourceTransition {
                base_revision: SourceRevision(0),
                target_revision: SourceRevision(1),
                base_root: SourceRootId(1),
                result_root: SourceRootId(2),
            })
            .expect("next parse admission")
            .active
            .token;
        let second_owner = arena
            .allocate(b"second", &[])
            .expect("second candidate allocation")
            .owner;
        let second = CandidateOutput::mechanism_only_for_test(second_owner, GrammarRevision(29));
        let second_root = second.arena_root();
        let coordinator_before = format!("{coordinator:?}");
        let arena_before = arena.metrics();

        let failure = coordinator
            .publish_candidate_with(second_token, second, &mut arena, |_arena, owner| {
                Err(OwnerTransferError {
                    error: ArenaError::Invariant("injected published-root release failure"),
                    owner,
                })
            })
            .expect_err("failed retirement must roll publication back exactly");
        assert_eq!(
            failure.error,
            CoordinatorError::Arena(ArenaError::Invariant(
                "injected published-root release failure"
            ))
        );
        assert_eq!(failure.candidate.arena_root(), second_root);
        assert_eq!(failure.candidate.grammar_revision(), GrammarRevision(29));
        assert_eq!(format!("{coordinator:?}"), coordinator_before);
        assert_eq!(arena.metrics(), arena_before);
        assert_eq!(coordinator.current_output(), first);
        assert_eq!(
            coordinator.resolve_worker_current(first, &arena).unwrap(),
            first.arena_root
        );

        let publication = coordinator
            .publish_candidate(second_token, failure.candidate, &mut arena)
            .expect("the restored old root and candidate permit exact retry");
        assert_eq!(publication.base_output, first);
        assert_eq!(publication.offered_output.arena_root, second_root);
        assert_eq!(
            publication.offered_output.grammar_revision,
            GrammarRevision(29)
        );
    }

    #[test]
    fn worker_current_resolution_survives_remote_release_and_is_lease_exact() {
        let mut arena = PageArena::new();
        let initial = arena
            .allocate(b"initial", &[])
            .expect("initial allocation")
            .owner;
        let mut coordinator = Coordinator::new(SourceRootId(1), initial);
        let bootstrap = coordinator.current_output();
        let token = coordinator
            .begin_initial_parse()
            .expect("initial parse admission")
            .token;
        let candidate = arena
            .allocate(b"candidate", &[])
            .expect("candidate allocation")
            .owner;
        let lease = coordinator
            .publish_candidate(
                token,
                CandidateOutput::mechanism_only_for_test(candidate, GrammarRevision(7)),
                &mut arena,
            )
            .expect("candidate publication")
            .offered_output;

        assert_eq!(
            coordinator.resolve_worker_current(bootstrap, &arena),
            Err(CoordinatorError::RootNotWorkerCurrent(
                bootstrap.remote_root
            ))
        );
        coordinator
            .release_root(lease, &mut arena)
            .expect("remote release");
        assert_eq!(
            coordinator.resolve_root(lease, &arena),
            Err(CoordinatorError::UnknownRoot(lease.remote_root))
        );
        assert_eq!(
            coordinator.resolve_worker_current(lease, &arena).unwrap(),
            lease.arena_root
        );

        let wrong_grammar = OutputRootLease {
            grammar_revision: GrammarRevision(8),
            ..lease
        };
        assert_eq!(
            coordinator.resolve_worker_current(wrong_grammar, &arena),
            Err(CoordinatorError::LeaseMismatch(lease.remote_root))
        );
    }
}
