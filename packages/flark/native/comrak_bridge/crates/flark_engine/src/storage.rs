use std::cell::Cell;
use std::collections::TryReserveError;
use std::fmt;
use std::marker::PhantomData;

use crate::identity::{ArenaId, ArenaIdentity};

/// Logical page size used by the arena's segmented directory.
pub const ARENA_PAGE_BYTES: usize = 4 * 1024;
const SLOTS_PER_SEGMENT: usize = 64;

/// Hard limits for one arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArenaLimits {
    /// Maximum resident node slots and maximum owners journalled by one build.
    ///
    /// Sharing does not consume another node slot, so the second bound is
    /// enforced explicitly when a build retains existing persistent nodes.
    pub max_slots: usize,
    pub max_live_payload_bytes: usize,
    pub max_children_per_node: usize,
}

impl Default for ArenaLimits {
    fn default() -> Self {
        Self {
            max_slots: 65_536,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 128,
        }
    }
}

/// A generation-safe arena operation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArenaError {
    InvalidLimits,
    IdentityExhausted,
    CapacityExceeded,
    PayloadTooLarge,
    TooManyChildren,
    PayloadBudgetExceeded,
    ForeignArena,
    StaleHandle,
    RefCountExhausted,
    BuildCapacityExceeded,
    StaleBuild,
    BuildNotActive,
    EmptyBuild,
    OwnerNotJournalled,
    RootNotLatest,
    AllocationFailed,
}

impl fmt::Display for ArenaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "arena limits are invalid",
            Self::IdentityExhausted => "arena identity space is exhausted",
            Self::CapacityExceeded => "arena slot capacity is exhausted",
            Self::PayloadTooLarge => "node payload exceeds the arena payload limit",
            Self::TooManyChildren => "node exceeds the per-node child limit",
            Self::PayloadBudgetExceeded => "arena live payload budget is exhausted",
            Self::ForeignArena => "handle belongs to another arena",
            Self::StaleHandle => "handle is stale or is being retired",
            Self::RefCountExhausted => "node reference count is exhausted",
            Self::BuildCapacityExceeded => "candidate build capacity is exhausted",
            Self::StaleBuild => "candidate build capability is stale",
            Self::BuildNotActive => "candidate build is not in the required lifecycle state",
            Self::EmptyBuild => "candidate build has no journalled owners",
            Self::OwnerNotJournalled => "candidate owner is not journalled by this build",
            Self::RootNotLatest => "candidate root is not the latest journal owner",
            Self::AllocationFailed => "arena allocation failed",
        })
    }
}

impl std::error::Error for ArenaError {}

impl From<TryReserveError> for ArenaError {
    fn from(_: TryReserveError) -> Self {
        Self::AllocationFailed
    }
}

/// An arena-internal owning reference to one node.
///
/// Raw owners never cross the storage module boundary. Candidate allocations
/// enter an arena-owned build journal before their non-owning handle is
/// returned, so abandoning a handle cannot strand a reference.
struct OwnedArenaRef {
    id: ArenaId,
}

impl OwnedArenaRef {
    /// Returns the generation-checked node identity.
    #[must_use]
    pub const fn id(&self) -> ArenaId {
        self.id
    }
}

impl fmt::Debug for OwnedArenaRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("OwnedArenaRef")
            .field(&self.id)
            .finish()
    }
}

/// A failed internal owner transfer; the original owner remains recoverable.
#[derive(Debug)]
struct ReleaseFailure {
    error: ArenaError,
    owner: OwnedArenaRef,
}

/// Current arena residency and retirement metrics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArenaMetrics {
    pub resident_nodes: usize,
    pub live_payload_bytes: usize,
    /// Parser-local fixed allocations admitted against this arena's payload
    /// budget but owned outside its immutable node graph.
    pub reserved_external_payload_bytes: usize,
    pub pending_reclaims: usize,
    pub allocated_slots: usize,
    pub live_builds: usize,
    pub pending_build_aborts: usize,
}

/// Work completed by one fuel-bounded reclamation poll.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReclaimReceipt {
    pub transitions: usize,
    pub nodes_reclaimed: usize,
    pub payload_bytes_reclaimed: usize,
    pub complete: bool,
}

/// Move-only accounting capability for fixed parser scratch owned outside the
/// immutable arena graph.
///
/// The parser owns and explicitly destroys the corresponding allocation. The
/// arena owns only its admission charge so persistent nodes and transient
/// parser scratch cannot independently spend the same payload budget.
pub(crate) struct ExternalPayloadReservation {
    arena: ArenaIdentity,
    bytes: usize,
    armed: bool,
}

impl fmt::Debug for ExternalPayloadReservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalPayloadReservation")
            .field("arena", &self.arena)
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl ExternalPayloadReservation {
    #[must_use]
    pub(crate) const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Splits a strict nonempty prefix from this reservation without changing
    /// the arena's aggregate admitted-byte count.
    pub(crate) fn split_prefix(&mut self, prefix_bytes: usize) -> Option<Self> {
        if prefix_bytes == 0 || prefix_bytes >= self.bytes {
            return None;
        }
        self.bytes -= prefix_bytes;
        Some(Self {
            arena: self.arena,
            bytes: prefix_bytes,
            armed: true,
        })
    }
}

impl Drop for ExternalPayloadReservation {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                !self.armed,
                "external arena payload reservation requires explicit release"
            );
        }
    }
}

pub(crate) struct ExternalPayloadReservationReleaseFailure {
    pub(crate) error: ArenaError,
    pub(crate) reservation: ExternalPayloadReservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotState {
    Vacant,
    Live,
    Retiring,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildState {
    Vacant,
    Active,
    Suspended,
    Sealing,
    Aborting,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArenaBuildId {
    arena: ArenaIdentity,
    slot: u32,
    generation: u32,
}

/// Opaque identity of one exact arena-owned build journal.
///
/// Persistent resumable jobs retain this copyable key across suspensions and
/// require it to match before every poll. It carries no ownership and cannot
/// resume, seal, or abort a build by itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArenaBuildKey(ArenaBuildId);

/// A non-owning allocation handle. The build journal, not this value, owns the
/// reference, so dropping the handle is always safe.
#[derive(Debug)]
pub(crate) struct ArenaBuildOwner {
    build: ArenaBuildId,
    id: ArenaId,
    _not_sync: PhantomData<Cell<()>>,
}

impl ArenaBuildOwner {
    #[must_use]
    pub(crate) const fn id(&self) -> ArenaId {
        self.id
    }
}

/// The suspended candidate build stored by the document actor.
///
/// This capability is intentionally crate-private and move-only. The document
/// runtime must transfer it back to the arena for fuelled cancellation.
pub(crate) struct CandidateBuild {
    id: ArenaBuildId,
    _not_sync: PhantomData<Cell<()>>,
}

/// Linear capability for a build whose latest owner was selected as root.
pub(crate) struct CandidateSeal {
    id: ArenaBuildId,
    complete: bool,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for CandidateSeal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateSeal")
            .field("id", &self.id)
            .field("complete", &self.complete)
            .finish()
    }
}

/// Private sole owner of one committed arena root.
pub(crate) struct CommittedArenaRoot {
    owner: OwnedArenaRef,
    _not_sync: PhantomData<Cell<()>>,
}

impl CommittedArenaRoot {
    #[must_use]
    pub(crate) const fn id(&self) -> ArenaId {
        self.owner.id()
    }
}

pub(crate) struct CommittedArenaRootReleaseFailure {
    pub(crate) error: ArenaError,
    pub(crate) root: CommittedArenaRoot,
}

/// One bounded root-seal poll.
pub(crate) struct SealPoll {
    pub(crate) transitions: usize,
    pub(crate) remaining_non_root_owners: usize,
    pub(crate) root: Option<CommittedArenaRoot>,
}

#[derive(Debug)]
pub(crate) struct BeginSealFailure {
    pub(crate) error: ArenaError,
    pub(crate) build: CandidateBuild,
    pub(crate) root: ArenaBuildOwner,
}

impl fmt::Debug for CandidateBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CandidateBuild")
            .field(&self.id)
            .finish()
    }
}

struct BuildSlot {
    generation: u32,
    state: BuildState,
    owners: Vec<OwnedArenaRef>,
    next_free: Option<u32>,
    pending_next: Option<u32>,
    seal_root: Option<ArenaId>,
}

impl BuildSlot {
    const fn vacant() -> Self {
        Self {
            generation: 1,
            state: BuildState::Vacant,
            owners: Vec::new(),
            next_free: None,
            pending_next: None,
            seal_root: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReclaimLane {
    BuildAbort,
    Node,
}

struct Slot {
    generation: u32,
    state: SlotState,
    ref_count: u32,
    payload: Option<Box<[u8]>>,
    children: Option<Box<[ArenaId]>>,
    next_free: Option<u32>,
    pending_next: Option<u32>,
    retire_child_index: usize,
}

impl Slot {
    const fn vacant() -> Self {
        Self {
            generation: 1,
            state: SlotState::Vacant,
            ref_count: 0,
            payload: None,
            children: None,
            next_free: None,
            pending_next: None,
            retire_child_index: 0,
        }
    }
}

/// Segmented, generation-checked storage with iterative reclamation.
pub(crate) struct PageArena {
    identity: ArenaIdentity,
    limits: ArenaLimits,
    segments: Vec<Box<[Slot]>>,
    next_unused: usize,
    free_head: Option<u32>,
    pending_head: Option<u32>,
    pending_tail: Option<u32>,
    resident_nodes: usize,
    live_payload_bytes: usize,
    reserved_external_payload_bytes: usize,
    pending_reclaims: usize,
    builds: Vec<BuildSlot>,
    build_free_head: Option<u32>,
    pending_build_head: Option<u32>,
    pending_build_tail: Option<u32>,
    live_builds: usize,
    pending_build_aborts: usize,
    next_reclaim_lane: ReclaimLane,
}

impl fmt::Debug for PageArena {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageArena")
            .field("identity", &self.identity)
            .field("limits", &self.limits)
            .field("metrics", &self.metrics())
            .finish_non_exhaustive()
    }
}

impl PageArena {
    /// Creates an empty arena and reserves its fixed segment directory.
    pub(crate) fn new(limits: ArenaLimits) -> Result<Self, ArenaError> {
        if limits.max_slots == 0 || limits.max_slots > u32::MAX as usize {
            return Err(ArenaError::InvalidLimits);
        }
        let identity = ArenaIdentity::allocate().ok_or(ArenaError::IdentityExhausted)?;
        let segment_count = limits.max_slots.div_ceil(SLOTS_PER_SEGMENT);
        let mut segments = Vec::new();
        segments.try_reserve_exact(segment_count)?;
        Ok(Self {
            identity,
            limits,
            segments,
            next_unused: 0,
            free_head: None,
            pending_head: None,
            pending_tail: None,
            resident_nodes: 0,
            live_payload_bytes: 0,
            reserved_external_payload_bytes: 0,
            pending_reclaims: 0,
            builds: Vec::new(),
            build_free_head: None,
            pending_build_head: None,
            pending_build_tail: None,
            live_builds: 0,
            pending_build_aborts: 0,
            next_reclaim_lane: ReclaimLane::BuildAbort,
        })
    }

    /// Returns current residency metrics.
    #[must_use]
    pub const fn metrics(&self) -> ArenaMetrics {
        ArenaMetrics {
            resident_nodes: self.resident_nodes,
            live_payload_bytes: self.live_payload_bytes,
            reserved_external_payload_bytes: self.reserved_external_payload_bytes,
            pending_reclaims: self.pending_reclaims,
            allocated_slots: self.next_unused,
            live_builds: self.live_builds,
            pending_build_aborts: self.pending_build_aborts,
        }
    }

    /// Returns the immutable hard limits used for admission preflight.
    #[must_use]
    pub(crate) const fn limits(&self) -> ArenaLimits {
        self.limits
    }

    /// Admits parser-local fixed storage against the same hard payload budget
    /// used by immutable arena nodes.
    ///
    /// No allocation is performed here. The returned move-only capability must
    /// remain paired with the caller-owned bytes until explicit release.
    pub(crate) fn reserve_external_payload(
        &mut self,
        bytes: usize,
    ) -> Result<ExternalPayloadReservation, ArenaError> {
        if bytes == 0 {
            return Err(ArenaError::PayloadTooLarge);
        }
        let next_reserved = self
            .reserved_external_payload_bytes
            .checked_add(bytes)
            .ok_or(ArenaError::PayloadBudgetExceeded)?;
        let next_admitted = self
            .live_payload_bytes
            .checked_add(next_reserved)
            .ok_or(ArenaError::PayloadBudgetExceeded)?;
        if next_admitted > self.limits.max_live_payload_bytes {
            return Err(ArenaError::PayloadBudgetExceeded);
        }
        self.reserved_external_payload_bytes = next_reserved;
        Ok(ExternalPayloadReservation {
            arena: self.identity,
            bytes,
            armed: true,
        })
    }

    /// Releases one exact external admission charge.
    pub(crate) fn release_external_payload(
        &mut self,
        mut reservation: ExternalPayloadReservation,
    ) -> Result<(), ExternalPayloadReservationReleaseFailure> {
        if reservation.arena != self.identity {
            return Err(ExternalPayloadReservationReleaseFailure {
                error: ArenaError::ForeignArena,
                reservation,
            });
        }
        let Some(next_reserved) = self
            .reserved_external_payload_bytes
            .checked_sub(reservation.bytes)
        else {
            return Err(ExternalPayloadReservationReleaseFailure {
                error: ArenaError::StaleHandle,
                reservation,
            });
        };
        self.reserved_external_payload_bytes = next_reserved;
        reservation.armed = false;
        Ok(())
    }

    /// Begins an arena-owned candidate build.
    ///
    /// Every allocation made through the returned session is journalled before
    /// its non-owning handle is returned. Dropping the session starts a
    /// fuelled abort in constant time.
    pub(crate) fn begin_build(&mut self) -> Result<ArenaBuildSession<'_>, ArenaError> {
        let id = self.acquire_build_slot()?;
        Ok(ArenaBuildSession {
            arena: self,
            id,
            finished: false,
            _not_sync: PhantomData,
        })
    }

    /// Resumes the sole suspended build capability.
    pub(crate) fn resume_build(
        &mut self,
        build: CandidateBuild,
    ) -> Result<ArenaBuildSession<'_>, ArenaError> {
        self.validate_build(build.id, BuildState::Suspended)?;
        self.build_slot_mut(build.id.slot).state = BuildState::Active;
        Ok(ArenaBuildSession {
            arena: self,
            id: build.id,
            finished: false,
            _not_sync: PhantomData,
        })
    }

    /// Checks a suspended capability without consuming it.
    ///
    /// Move-only controller code uses this before a transfer whose legacy
    /// `Result` shape cannot return the capability on failure. Once this
    /// succeeds, the matching transfer is infallible on the same worker: no
    /// intervening operation can mutate the arena lifecycle state.
    pub(crate) fn validate_suspended_build(
        &self,
        build: &CandidateBuild,
    ) -> Result<(), ArenaError> {
        self.validate_build(build.id, BuildState::Suspended)
    }

    /// Checks a sealing capability without consuming it.
    pub(crate) fn validate_seal(&self, seal: &CandidateSeal) -> Result<(), ArenaError> {
        if seal.complete {
            return Err(ArenaError::StaleBuild);
        }
        self.validate_build(seal.id, BuildState::Sealing)
    }

    pub(crate) fn suspended_build_owner_count(
        &self,
        build: &CandidateBuild,
    ) -> Result<usize, ArenaError> {
        self.validate_build(build.id, BuildState::Suspended)?;
        Ok(self.build_slot(build.id.slot).owners.len())
    }

    /// Transfers a suspended candidate to the fuelled abort queue.
    pub(crate) fn abort_build(&mut self, build: CandidateBuild) -> Result<(), ArenaError> {
        self.validate_build(build.id, BuildState::Suspended)?;
        self.begin_abort(build.id);
        Ok(())
    }

    /// Selects the most recently journalled owner as the sole committed root.
    ///
    /// Selection is O(1). Non-root owner release happens only in
    /// [`Self::poll_seal`], one journal transfer per transition, so a caller
    /// cannot accidentally turn a large build into one synchronous seal loop.
    pub(crate) fn begin_seal(
        &mut self,
        build: CandidateBuild,
        root: ArenaBuildOwner,
    ) -> Result<CandidateSeal, BeginSealFailure> {
        if let Err(error) = self.validate_build(build.id, BuildState::Suspended) {
            return Err(BeginSealFailure { error, build, root });
        }
        if root.build != build.id {
            return Err(BeginSealFailure {
                error: ArenaError::StaleBuild,
                build,
                root,
            });
        }
        let Some(latest) = self
            .build_slot(build.id.slot)
            .owners
            .last()
            .map(OwnedArenaRef::id)
        else {
            return Err(BeginSealFailure {
                error: ArenaError::EmptyBuild,
                build,
                root,
            });
        };
        if latest != root.id {
            return Err(BeginSealFailure {
                error: ArenaError::RootNotLatest,
                build,
                root,
            });
        }
        let slot = self.build_slot_mut(build.id.slot);
        slot.state = BuildState::Sealing;
        slot.seal_root = Some(root.id);
        Ok(CandidateSeal {
            id: build.id,
            complete: false,
            _not_sync: PhantomData,
        })
    }

    /// Releases at most `fuel` non-root journal owners and finally transfers
    /// the selected root. Each transition is constant-time with respect to the
    /// journal length.
    pub(crate) fn poll_seal(
        &mut self,
        seal: &mut CandidateSeal,
        fuel: usize,
    ) -> Result<SealPoll, ArenaError> {
        if seal.complete {
            return Err(ArenaError::StaleBuild);
        }
        self.validate_build(seal.id, BuildState::Sealing)?;
        let mut transitions = 0;
        while transitions < fuel {
            let owner_count = self.build_slot(seal.id.slot).owners.len();
            if owner_count > 1 {
                let owner = self
                    .build_slot_mut(seal.id.slot)
                    .owners
                    .swap_remove(owner_count - 2);
                if let Err(failure) = self.release_owned_later(owner) {
                    let owners = &mut self.build_slot_mut(seal.id.slot).owners;
                    owners.push(failure.owner);
                    let last = owners.len() - 1;
                    owners.swap(last - 1, last);
                    return Err(failure.error);
                }
                transitions += 1;
                continue;
            }

            let root_id = self
                .build_slot(seal.id.slot)
                .seal_root
                .ok_or(ArenaError::BuildNotActive)?;
            let root = self
                .build_slot_mut(seal.id.slot)
                .owners
                .pop()
                .ok_or(ArenaError::EmptyBuild)?;
            if root.id() != root_id {
                self.build_slot_mut(seal.id.slot).owners.push(root);
                return Err(ArenaError::RootNotLatest);
            }
            self.recycle_build(seal.id.slot);
            seal.complete = true;
            transitions += 1;
            return Ok(SealPoll {
                transitions,
                remaining_non_root_owners: 0,
                root: Some(CommittedArenaRoot {
                    owner: root,
                    _not_sync: PhantomData,
                }),
            });
        }
        let remaining = self.build_slot(seal.id.slot).owners.len().saturating_sub(1);
        Ok(SealPoll {
            transitions,
            remaining_non_root_owners: remaining,
            root: None,
        })
    }

    /// Cancels an in-progress seal through the ordinary fuelled abort queue.
    pub(crate) fn abort_seal(&mut self, seal: CandidateSeal) -> Result<(), ArenaError> {
        if seal.complete {
            return Err(ArenaError::StaleBuild);
        }
        self.validate_build(seal.id, BuildState::Sealing)?;
        self.begin_abort(seal.id);
        Ok(())
    }

    pub(crate) fn release_committed_root(
        &mut self,
        root: CommittedArenaRoot,
    ) -> Result<(), CommittedArenaRootReleaseFailure> {
        match self.release_owned_later(root.owner) {
            Ok(()) => Ok(()),
            Err(failure) => Err(CommittedArenaRootReleaseFailure {
                error: failure.error,
                root: CommittedArenaRoot {
                    owner: failure.owner,
                    _not_sync: PhantomData,
                },
            }),
        }
    }

    /// Allocates a node and retains each child edge.
    fn allocate_owned(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<OwnedArenaRef, ArenaError> {
        if payload.len() > ARENA_PAGE_BYTES {
            return Err(ArenaError::PayloadTooLarge);
        }
        if children.len() > self.limits.max_children_per_node {
            return Err(ArenaError::TooManyChildren);
        }
        let next_payload_bytes = self
            .live_payload_bytes
            .checked_add(payload.len())
            .ok_or(ArenaError::PayloadBudgetExceeded)?;
        let next_admitted = next_payload_bytes
            .checked_add(self.reserved_external_payload_bytes)
            .ok_or(ArenaError::PayloadBudgetExceeded)?;
        if next_admitted > self.limits.max_live_payload_bytes {
            return Err(ArenaError::PayloadBudgetExceeded);
        }

        self.validate_child_ref_increments(children)?;

        let mut owned_payload = Vec::new();
        owned_payload.try_reserve_exact(payload.len())?;
        owned_payload.extend_from_slice(payload);
        let mut owned_children = Vec::new();
        owned_children.try_reserve_exact(children.len())?;
        owned_children.extend_from_slice(children);

        let slot_index = self.acquire_slot()?;
        for child in children {
            let slot = self.slot_mut(child.slot);
            slot.ref_count += 1;
        }

        let identity = self.identity;
        let slot = self.slot_mut(slot_index);
        debug_assert_eq!(slot.state, SlotState::Vacant);
        slot.state = SlotState::Live;
        slot.ref_count = 1;
        slot.payload = Some(owned_payload.into_boxed_slice());
        slot.children = Some(owned_children.into_boxed_slice());
        slot.next_free = None;
        slot.pending_next = None;
        slot.retire_child_index = 0;
        let id = ArenaId {
            arena: identity,
            slot: slot_index,
            generation: slot.generation,
        };

        self.resident_nodes += 1;
        self.live_payload_bytes = next_payload_bytes;
        Ok(OwnedArenaRef { id })
    }

    /// Acquires another explicit owner for a live node.
    fn retain_owned(&mut self, id: ArenaId) -> Result<OwnedArenaRef, ArenaError> {
        self.validate_owned_retain(id)?;
        let slot = self.slot_mut(id.slot);
        slot.ref_count += 1;
        Ok(OwnedArenaRef { id })
    }

    /// Checks a retain without changing either the node or a build journal.
    ///
    /// A build uses this before reserving journal storage, then performs the
    /// retain only after that allocation succeeds. Because the arena is
    /// actor-local and exclusively borrowed by the build session, the checked
    /// refcount cannot change between the two operations.
    fn validate_owned_retain(&self, id: ArenaId) -> Result<(), ArenaError> {
        self.validate_live(id)?;
        self.slot(id.slot)
            .ref_count
            .checked_add(1)
            .ok_or(ArenaError::RefCountExhausted)?;
        Ok(())
    }

    /// Reserves journal entries while keeping each build's logical owner set
    /// bounded by the arena slot envelope.
    fn reserve_build_owners(
        &mut self,
        id: ArenaBuildId,
        additional: usize,
    ) -> Result<(), ArenaError> {
        if self
            .build_slot(id.slot)
            .owners
            .len()
            .checked_add(additional)
            .is_none_or(|required| required > self.limits.max_slots)
        {
            return Err(ArenaError::BuildCapacityExceeded);
        }
        self.build_slot_mut(id.slot)
            .owners
            .try_reserve(additional)?;
        Ok(())
    }

    /// Returns a live node's payload.
    pub(crate) fn payload(&self, id: ArenaId) -> Result<&[u8], ArenaError> {
        self.validate_live(id)?;
        Ok(self
            .slot(id.slot)
            .payload
            .as_deref()
            .expect("live nodes always have a payload"))
    }

    pub(crate) fn child_count(&self, id: ArenaId) -> Result<usize, ArenaError> {
        self.validate_live(id)?;
        Ok(self
            .slot(id.slot)
            .children
            .as_deref()
            .expect("live nodes always have child storage")
            .len())
    }

    pub(crate) fn child_at(&self, id: ArenaId, index: usize) -> Result<ArenaId, ArenaError> {
        self.validate_live(id)?;
        self.slot(id.slot)
            .children
            .as_deref()
            .expect("live nodes always have child storage")
            .get(index)
            .copied()
            .ok_or(ArenaError::StaleHandle)
    }

    /// Transfers an owner into the bounded retirement queue.
    fn release_owned_later(&mut self, owner: OwnedArenaRef) -> Result<(), ReleaseFailure> {
        let id = owner.id;
        if let Err(error) = self.validate_live(id) {
            return Err(ReleaseFailure { error, owner });
        }

        let became_unreferenced = {
            let slot = self.slot_mut(id.slot);
            debug_assert!(slot.ref_count > 0);
            slot.ref_count -= 1;
            if slot.ref_count == 0 {
                slot.state = SlotState::Retiring;
                true
            } else {
                false
            }
        };
        if became_unreferenced {
            self.append_pending(id.slot);
        }
        Ok(())
    }

    /// Performs at most `fuel` child-edge or node-finalization transitions.
    pub(crate) fn poll_reclaim(&mut self, fuel: usize) -> ReclaimReceipt {
        let mut receipt = ReclaimReceipt::default();
        while receipt.transitions < fuel {
            let build_pending = self.pending_build_head.is_some();
            let node_pending = self.pending_head.is_some();
            if !build_pending && !node_pending {
                break;
            }

            let lane = match (build_pending, node_pending) {
                (true, true) => self.next_reclaim_lane,
                (true, false) => ReclaimLane::BuildAbort,
                (false, true) => ReclaimLane::Node,
                (false, false) => unreachable!(),
            };
            match lane {
                ReclaimLane::BuildAbort => {
                    self.abort_one_owner();
                    self.next_reclaim_lane = ReclaimLane::Node;
                    receipt.transitions += 1;
                    continue;
                }
                ReclaimLane::Node => {
                    self.next_reclaim_lane = ReclaimLane::BuildAbort;
                }
            }

            let head = self
                .pending_head
                .expect("node reclaim lane requires a pending node");

            if let Some(child) = self.next_retiring_child(head) {
                self.slot_mut(head).retire_child_index += 1;
                let became_unreferenced = {
                    let child_slot = self.slot_mut(child.slot);
                    debug_assert_eq!(child_slot.state, SlotState::Live);
                    debug_assert!(child_slot.ref_count > 0);
                    child_slot.ref_count -= 1;
                    if child_slot.ref_count == 0 {
                        child_slot.state = SlotState::Retiring;
                        true
                    } else {
                        false
                    }
                };
                if became_unreferenced {
                    self.append_pending(child.slot);
                }
                receipt.transitions += 1;
                continue;
            }

            let reclaimed = self.finalize_pending_head(head);
            receipt.transitions += 1;
            receipt.nodes_reclaimed += 1;
            receipt.payload_bytes_reclaimed += reclaimed;
        }
        receipt.complete = self.pending_head.is_none() && self.pending_build_head.is_none();
        receipt
    }

    fn validate_child_ref_increments(&self, children: &[ArenaId]) -> Result<(), ArenaError> {
        for (index, child) in children.iter().copied().enumerate() {
            self.validate_live(child)?;
            if children[..index].contains(&child) {
                continue;
            }
            let increment = children
                .iter()
                .filter(|candidate| **candidate == child)
                .count();
            let increment = u32::try_from(increment).map_err(|_| ArenaError::RefCountExhausted)?;
            self.slot(child.slot)
                .ref_count
                .checked_add(increment)
                .ok_or(ArenaError::RefCountExhausted)?;
        }
        Ok(())
    }

    fn validate_live(&self, id: ArenaId) -> Result<(), ArenaError> {
        if id.arena != self.identity {
            return Err(ArenaError::ForeignArena);
        }
        let index = id.slot as usize;
        if index >= self.next_unused {
            return Err(ArenaError::StaleHandle);
        }
        let slot = self.slot(id.slot);
        if slot.generation != id.generation || slot.state != SlotState::Live {
            return Err(ArenaError::StaleHandle);
        }
        Ok(())
    }

    fn acquire_slot(&mut self) -> Result<u32, ArenaError> {
        if let Some(index) = self.free_head {
            self.free_head = self.slot(index).next_free;
            self.slot_mut(index).next_free = None;
            return Ok(index);
        }
        if self.next_unused >= self.limits.max_slots {
            return Err(ArenaError::CapacityExceeded);
        }

        let index = self.next_unused;
        let segment_index = index / SLOTS_PER_SEGMENT;
        if segment_index == self.segments.len() {
            let mut segment = Vec::new();
            segment.try_reserve_exact(SLOTS_PER_SEGMENT)?;
            for _ in 0..SLOTS_PER_SEGMENT {
                segment.push(Slot::vacant());
            }
            self.segments.push(segment.into_boxed_slice());
        }
        self.next_unused += 1;
        u32::try_from(index).map_err(|_| ArenaError::CapacityExceeded)
    }

    fn acquire_build_slot(&mut self) -> Result<ArenaBuildId, ArenaError> {
        let index = if let Some(index) = self.build_free_head {
            self.build_free_head = self.build_slot(index).next_free;
            index
        } else {
            if self.builds.len() >= self.limits.max_slots {
                return Err(ArenaError::BuildCapacityExceeded);
            }
            self.builds.try_reserve(1)?;
            let index =
                u32::try_from(self.builds.len()).map_err(|_| ArenaError::BuildCapacityExceeded)?;
            self.builds.push(BuildSlot::vacant());
            index
        };

        let arena = self.identity;
        let slot = self.build_slot_mut(index);
        debug_assert_eq!(slot.state, BuildState::Vacant);
        debug_assert!(slot.owners.is_empty());
        slot.state = BuildState::Active;
        slot.next_free = None;
        slot.pending_next = None;
        slot.seal_root = None;
        let id = ArenaBuildId {
            arena,
            slot: index,
            generation: slot.generation,
        };
        self.live_builds += 1;
        Ok(id)
    }

    fn validate_build(
        &self,
        id: ArenaBuildId,
        expected_state: BuildState,
    ) -> Result<(), ArenaError> {
        if id.arena != self.identity {
            return Err(ArenaError::StaleBuild);
        }
        let Some(slot) = self.builds.get(id.slot as usize) else {
            return Err(ArenaError::StaleBuild);
        };
        if slot.generation != id.generation {
            return Err(ArenaError::StaleBuild);
        }
        if slot.state != expected_state {
            return Err(ArenaError::BuildNotActive);
        }
        Ok(())
    }

    fn begin_abort(&mut self, id: ArenaBuildId) {
        debug_assert!(
            self.validate_build(id, BuildState::Active).is_ok()
                || self.validate_build(id, BuildState::Suspended).is_ok()
                || self.validate_build(id, BuildState::Sealing).is_ok()
        );
        let owners_empty = {
            let slot = self.build_slot_mut(id.slot);
            slot.state = BuildState::Aborting;
            slot.seal_root = None;
            slot.owners.is_empty()
        };
        if owners_empty {
            self.recycle_build(id.slot);
            return;
        }

        self.build_slot_mut(id.slot).pending_next = None;
        if let Some(tail) = self.pending_build_tail {
            self.build_slot_mut(tail).pending_next = Some(id.slot);
        } else {
            self.pending_build_head = Some(id.slot);
        }
        self.pending_build_tail = Some(id.slot);
        self.pending_build_aborts += 1;
    }

    fn abort_one_owner(&mut self) {
        let build_index = self
            .pending_build_head
            .expect("abort work requires a pending build");
        let owner = self
            .build_slot_mut(build_index)
            .owners
            .pop()
            .expect("pending build aborts always own a journal entry");
        if let Err(ReleaseFailure { error, owner }) = self.release_owned_later(owner) {
            panic!(
                "journalled owner {:?} became invalid during abort: {error}",
                owner.id()
            );
        }
        if self.build_slot(build_index).owners.is_empty() {
            let next = self.build_slot(build_index).pending_next;
            self.pending_build_head = next;
            if next.is_none() {
                self.pending_build_tail = None;
            }
            self.pending_build_aborts -= 1;
            self.recycle_build(build_index);
        }
    }

    fn recycle_build(&mut self, index: u32) {
        let next_generation = self.build_slot(index).generation.checked_add(1);
        if let Some(generation) = next_generation {
            let free_head = self.build_free_head;
            let slot = self.build_slot_mut(index);
            slot.generation = generation;
            slot.state = BuildState::Vacant;
            slot.next_free = free_head;
            slot.pending_next = None;
            slot.seal_root = None;
            self.build_free_head = Some(index);
        } else {
            let slot = self.build_slot_mut(index);
            slot.state = BuildState::Retired;
            slot.next_free = None;
            slot.pending_next = None;
            slot.seal_root = None;
        }
        self.live_builds -= 1;
    }

    fn append_pending(&mut self, index: u32) {
        self.slot_mut(index).pending_next = None;
        if let Some(tail) = self.pending_tail {
            self.slot_mut(tail).pending_next = Some(index);
        } else {
            self.pending_head = Some(index);
        }
        self.pending_tail = Some(index);
        self.pending_reclaims += 1;
    }

    fn next_retiring_child(&self, index: u32) -> Option<ArenaId> {
        let slot = self.slot(index);
        debug_assert_eq!(slot.state, SlotState::Retiring);
        slot.children
            .as_deref()
            .and_then(|children| children.get(slot.retire_child_index))
            .copied()
    }

    fn finalize_pending_head(&mut self, index: u32) -> usize {
        debug_assert_eq!(self.pending_head, Some(index));
        let next = self.slot(index).pending_next;
        self.pending_head = next;
        if next.is_none() {
            self.pending_tail = None;
        }

        let reclaimed_bytes;
        let next_generation;
        {
            let slot = self.slot_mut(index);
            reclaimed_bytes = slot.payload.take().map_or(0, |payload| payload.len());
            let _ = slot.children.take();
            slot.ref_count = 0;
            slot.pending_next = None;
            slot.retire_child_index = 0;
            next_generation = slot.generation.checked_add(1);
        }

        if let Some(generation) = next_generation {
            let old_free_head = self.free_head;
            let slot = self.slot_mut(index);
            slot.generation = generation;
            slot.state = SlotState::Vacant;
            slot.next_free = old_free_head;
            self.free_head = Some(index);
        } else {
            let slot = self.slot_mut(index);
            slot.state = SlotState::Retired;
            slot.next_free = None;
        }

        self.resident_nodes -= 1;
        self.live_payload_bytes -= reclaimed_bytes;
        self.pending_reclaims -= 1;
        reclaimed_bytes
    }

    fn slot(&self, index: u32) -> &Slot {
        let index = index as usize;
        &self.segments[index / SLOTS_PER_SEGMENT][index % SLOTS_PER_SEGMENT]
    }

    fn slot_mut(&mut self, index: u32) -> &mut Slot {
        let index = index as usize;
        &mut self.segments[index / SLOTS_PER_SEGMENT][index % SLOTS_PER_SEGMENT]
    }

    fn build_slot(&self, index: u32) -> &BuildSlot {
        &self.builds[index as usize]
    }

    fn build_slot_mut(&mut self, index: u32) -> &mut BuildSlot {
        &mut self.builds[index as usize]
    }
}

/// An actor-local borrow of one arena-owned build journal.
pub(crate) struct ArenaBuildSession<'arena> {
    arena: &'arena mut PageArena,
    id: ArenaBuildId,
    finished: bool,
    _not_sync: PhantomData<Cell<()>>,
}

impl ArenaBuildSession<'_> {
    /// Returns the non-owning identity used to bind resumable jobs to this
    /// exact journal across suspend/resume boundaries.
    #[must_use]
    pub(crate) const fn key(&self) -> ArenaBuildKey {
        ArenaBuildKey(self.id)
    }

    /// Fails before mutation when a resumable job is polled with another
    /// arena, build slot, or generation.
    pub(crate) fn validate_key(&self, key: ArenaBuildKey) -> Result<(), ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        if key.0 != self.id {
            return Err(ArenaError::StaleBuild);
        }
        Ok(())
    }

    /// Preallocates room for at least `additional` journal owners beyond the
    /// owners currently held by this active build.
    ///
    /// Resumable jobs call this before a poll performs its first mutation.
    /// Every later [`Self::retain`] or [`Self::allocate`] within that reserved
    /// owner count still performs the ordinary logical-cap check, but its
    /// journal push cannot grow the backing allocation.
    pub(crate) fn reserve_owner_capacity(&mut self, additional: usize) -> Result<(), ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        self.arena.reserve_build_owners(self.id, additional)
    }

    /// Returns how many more owners this journal can ever admit under the
    /// arena's hard logical cap. Reserving up to this value is sufficient to
    /// eliminate later vector growth even when a deliberately small test or
    /// embedded arena cannot need the generic structure's global worst case.
    pub(crate) fn remaining_owner_capacity(&self) -> Result<usize, ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        self.arena
            .limits
            .max_slots
            .checked_sub(self.arena.build_slot(self.id.slot).owners.len())
            .ok_or(ArenaError::BuildCapacityExceeded)
    }

    #[cfg(test)]
    fn owner_journal_state(&self) -> (usize, usize) {
        let owners = &self.arena.build_slot(self.id.slot).owners;
        (owners.len(), owners.capacity())
    }

    /// Returns a read-only arena view for persistent-structure traversal.
    ///
    /// The session retains its exclusive actor-local arena borrow, so callers
    /// can inspect an existing root while constructing a replacement but
    /// cannot mutate storage outside the journalled build operations.
    pub(crate) const fn arena(&self) -> &PageArena {
        self.arena
    }

    /// Allocates and journals before returning a non-owning handle.
    pub(crate) fn allocate(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaBuildOwner, ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        self.arena.reserve_build_owners(self.id, 1)?;
        let owner = self.arena.allocate_owned(payload, children)?;
        let id = owner.id();
        self.arena.build_slot_mut(self.id.slot).owners.push(owner);
        Ok(ArenaBuildOwner {
            build: self.id,
            id,
            _not_sync: PhantomData,
        })
    }

    /// Acquires and journals an existing live node before returning its
    /// non-owning build handle.
    ///
    /// Validation and journal reservation both happen before the refcount is
    /// incremented. Therefore every error leaves the node refcount and the
    /// journal's logical owner set unchanged; after the increment, the
    /// pre-reserved `push` cannot fail.
    pub(crate) fn retain(&mut self, id: ArenaId) -> Result<ArenaBuildOwner, ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        self.arena.validate_owned_retain(id)?;
        self.arena.reserve_build_owners(self.id, 1)?;
        let owner = self.arena.retain_owned(id)?;
        self.arena.build_slot_mut(self.id.slot).owners.push(owner);
        Ok(ArenaBuildOwner {
            build: self.id,
            id,
            _not_sync: PhantomData,
        })
    }

    /// Verifies that a non-owning handle still names an owner journalled by
    /// this active build.
    ///
    /// Resumable persistent-structure jobs call this before any mutation, so a
    /// handle restored with the wrong build capability cannot first retain
    /// nodes into an unrelated journal and fail only during release.
    pub(crate) fn validate_owner(&self, owner: &ArenaBuildOwner) -> Result<(), ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        if owner.build != self.id {
            return Err(ArenaError::StaleBuild);
        }
        if !self
            .arena
            .build_slot(self.id.slot)
            .owners
            .iter()
            .any(|candidate| candidate.id() == owner.id)
        {
            return Err(ArenaError::OwnerNotJournalled);
        }
        Ok(())
    }

    /// Releases one journal owner after a newly allocated parent retained it.
    /// Callers must keep their live owner set independently bounded; the
    /// reference-root builder caps it at a small constant.
    pub(crate) fn release(&mut self, owner: ArenaBuildOwner) -> Result<(), ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        if owner.build != self.id {
            return Err(ArenaError::StaleBuild);
        }
        let position = self
            .arena
            .build_slot(self.id.slot)
            .owners
            .iter()
            .position(|candidate| candidate.id() == owner.id)
            .ok_or(ArenaError::OwnerNotJournalled)?;
        let owned = self
            .arena
            .build_slot_mut(self.id.slot)
            .owners
            .swap_remove(position);
        if let Err(failure) = self.arena.release_owned_later(owned) {
            self.arena
                .build_slot_mut(self.id.slot)
                .owners
                .push(failure.owner);
            return Err(failure.error);
        }
        Ok(())
    }

    /// Yields the arena borrow while retaining every owner in the journal.
    pub(crate) fn suspend(mut self) -> Result<CandidateBuild, ArenaError> {
        self.arena.validate_build(self.id, BuildState::Active)?;
        if self.arena.build_slot(self.id.slot).owners.is_empty() {
            return Err(ArenaError::EmptyBuild);
        }
        self.arena.build_slot_mut(self.id.slot).state = BuildState::Suspended;
        self.finished = true;
        Ok(CandidateBuild {
            id: self.id,
            _not_sync: PhantomData,
        })
    }
}

impl Drop for ArenaBuildSession<'_> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if self
            .arena
            .validate_build(self.id, BuildState::Active)
            .is_ok()
        {
            // Constant time: owner scheduling and node reclamation stay fuelled.
            self.arena.begin_abort(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_slots: usize) -> ArenaLimits {
        ArenaLimits {
            max_slots,
            max_live_payload_bytes: ARENA_PAGE_BYTES * max_slots,
            max_children_per_node: 8,
        }
    }

    fn drain(arena: &mut PageArena) {
        while arena.metrics().pending_reclaims > 0 || arena.metrics().pending_build_aborts > 0 {
            let receipt = arena.poll_reclaim(1);
            assert!(receipt.transitions <= 1);
        }
    }

    fn commit_single_node(arena: &mut PageArena, payload: &[u8]) -> CommittedArenaRoot {
        let (build, root) = {
            let mut session = arena.begin_build().expect("build");
            let root = session.allocate(payload, &[]).expect("root allocation");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let mut seal = arena.begin_seal(build, root).expect("begin seal");
        arena
            .poll_seal(&mut seal, 1)
            .expect("seal")
            .root
            .expect("committed root")
    }

    #[test]
    fn external_scratch_and_persistent_nodes_share_one_payload_budget() {
        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 4,
            max_live_payload_bytes: ARENA_PAGE_BYTES,
            max_children_per_node: 2,
        })
        .expect("arena");
        let reservation = arena
            .reserve_external_payload(1024)
            .expect("scratch reservation");
        assert_eq!(arena.metrics().reserved_external_payload_bytes, 1024);
        assert_eq!(
            arena
                .allocate_owned(&vec![0; ARENA_PAGE_BYTES - 1024 + 1], &[])
                .expect_err("combined payload must reject over-admission"),
            ArenaError::PayloadBudgetExceeded
        );
        let persistent = arena
            .allocate_owned(&vec![0; ARENA_PAGE_BYTES - 1024], &[])
            .expect("remaining persistent payload");
        assert_eq!(
            arena
                .reserve_external_payload(1)
                .expect_err("persistent payload must constrain scratch"),
            ArenaError::PayloadBudgetExceeded
        );

        arena
            .release_external_payload(reservation)
            .map_err(|failure| failure.error)
            .expect("release scratch");
        assert_eq!(arena.metrics().reserved_external_payload_bytes, 0);
        let replacement = arena
            .reserve_external_payload(1024)
            .expect("released budget is reusable");
        arena
            .release_external_payload(replacement)
            .map_err(|failure| failure.error)
            .expect("release replacement");
        arena
            .release_owned_later(persistent)
            .expect("release persistent");
        drain(&mut arena);
        assert_eq!(arena.metrics().live_payload_bytes, 0);
    }

    #[test]
    fn split_external_reservations_preserve_aggregate_accounting_and_authority() {
        let mut owner = PageArena::new(limits(2)).expect("owner arena");
        let mut foreign = PageArena::new(limits(2)).expect("foreign arena");
        let mut tail = owner.reserve_external_payload(4096).expect("reservation");
        let prefix = tail.split_prefix(1024).expect("strict prefix");
        assert_eq!(prefix.bytes(), 1024);
        assert_eq!(tail.bytes(), 3072);
        assert_eq!(owner.metrics().reserved_external_payload_bytes, 4096);

        let failure = foreign
            .release_external_payload(prefix)
            .expect_err("foreign release");
        assert_eq!(failure.error, ArenaError::ForeignArena);
        assert_eq!(foreign.metrics().reserved_external_payload_bytes, 0);
        owner
            .release_external_payload(failure.reservation)
            .map_err(|failure| failure.error)
            .expect("owner releases recovered prefix");
        assert_eq!(owner.metrics().reserved_external_payload_bytes, 3072);
        owner
            .release_external_payload(tail)
            .map_err(|failure| failure.error)
            .expect("owner releases tail");
        assert_eq!(owner.metrics().reserved_external_payload_bytes, 0);
    }

    #[test]
    fn abandoned_build_handle_keeps_owner_journalled_and_reclaims_it() {
        let mut arena = PageArena::new(limits(1)).expect("arena");
        let abandoned_id = {
            let mut build = arena.begin_build().expect("build");
            let handle = build.allocate(b"abandoned", &[]).expect("allocation");
            // The non-owning handle falls out of scope before the build; the
            // arena journal remains the sole owner.
            handle.id()
        };

        assert_eq!(arena.metrics().resident_nodes, 1);
        assert_eq!(arena.metrics().pending_build_aborts, 1);
        let scheduled = arena.poll_reclaim(1);
        assert_eq!(scheduled.transitions, 1);
        assert_eq!(scheduled.nodes_reclaimed, 0);
        assert_eq!(arena.metrics().pending_build_aborts, 0);
        assert_eq!(arena.metrics().pending_reclaims, 1);
        let reclaimed = arena.poll_reclaim(1);
        assert_eq!(reclaimed.nodes_reclaimed, 1);
        assert_eq!(arena.metrics().resident_nodes, 0);
        assert_eq!(
            arena.payload(abandoned_id).expect_err("stale abandoned id"),
            ArenaError::StaleHandle
        );
    }

    #[test]
    fn stale_handle_cannot_alias_a_reused_slot() {
        let mut arena = PageArena::new(limits(1)).expect("arena");
        let first = arena.allocate_owned(b"first", &[]).expect("first node");
        let stale_id = first.id();
        arena.release_owned_later(first).expect("release first");
        drain(&mut arena);

        let second = arena.allocate_owned(b"second", &[]).expect("second node");
        assert_eq!(second.id().slot(), stale_id.slot());
        assert_ne!(second.id().generation(), stale_id.generation());
        assert_eq!(
            arena.retain_owned(stale_id).expect_err("stale retain"),
            ArenaError::StaleHandle
        );
        assert_eq!(
            arena.payload(stale_id).expect_err("stale payload"),
            ArenaError::StaleHandle
        );

        arena.release_owned_later(second).expect("release second");
        drain(&mut arena);
    }

    #[test]
    fn cross_arena_release_returns_the_original_owner() {
        let mut left = PageArena::new(limits(1)).expect("left arena");
        let mut right = PageArena::new(limits(1)).expect("right arena");
        let owner = left.allocate_owned(b"node", &[]).expect("node");
        let id = owner.id();

        assert_eq!(
            right.retain_owned(id).expect_err("foreign retain"),
            ArenaError::ForeignArena
        );
        let failure = right
            .release_owned_later(owner)
            .expect_err("foreign release must return owner");
        assert_eq!(failure.error, ArenaError::ForeignArena);
        left.release_owned_later(failure.owner)
            .expect("release through owner arena");
        drain(&mut left);
    }

    #[test]
    fn build_retain_rejects_foreign_and_stale_nodes_without_journalling() {
        let mut arena = PageArena::new(limits(2)).expect("arena");
        let mut foreign_arena = PageArena::new(limits(1)).expect("foreign arena");
        let foreign = foreign_arena
            .allocate_owned(b"foreign", &[])
            .expect("foreign node");

        {
            let mut build = arena.begin_build().expect("foreign build");
            assert_eq!(
                build.retain(foreign.id()).expect_err("foreign retain"),
                ArenaError::ForeignArena
            );
        }
        assert_eq!(arena.metrics().live_builds, 0);
        assert_eq!(arena.metrics().pending_build_aborts, 0);

        let stale = arena.allocate_owned(b"stale", &[]).expect("stale node");
        let stale_id = stale.id();
        arena.release_owned_later(stale).expect("release stale");
        drain(&mut arena);
        {
            let mut build = arena.begin_build().expect("stale build");
            assert_eq!(
                build.retain(stale_id).expect_err("stale retain"),
                ArenaError::StaleHandle
            );
        }
        assert_eq!(arena.metrics().live_builds, 0);
        assert_eq!(arena.metrics().pending_build_aborts, 0);

        foreign_arena
            .release_owned_later(foreign)
            .expect("release foreign");
        drain(&mut foreign_arena);
    }

    #[test]
    fn failed_build_retain_does_not_change_an_exhausted_refcount() {
        let mut arena = PageArena::new(limits(1)).expect("arena");
        let owner = arena.allocate_owned(b"node", &[]).expect("node");
        let id = owner.id();
        arena.slot_mut(id.slot).ref_count = u32::MAX;

        {
            let mut build = arena.begin_build().expect("build");
            assert_eq!(
                build.retain(id).expect_err("exhausted retain"),
                ArenaError::RefCountExhausted
            );
        }
        assert_eq!(arena.slot(id.slot).ref_count, u32::MAX);
        assert_eq!(arena.metrics().live_builds, 0);
        assert_eq!(arena.metrics().pending_build_aborts, 0);

        arena.slot_mut(id.slot).ref_count = 1;
        arena.release_owned_later(owner).expect("release node");
        drain(&mut arena);
    }

    #[test]
    fn build_owner_journal_is_bounded_even_when_retain_shares_one_slot() {
        let mut arena = PageArena::new(limits(1)).expect("arena");
        let owner = arena.allocate_owned(b"shared", &[]).expect("shared node");
        let id = owner.id();
        let build = {
            let mut session = arena.begin_build().expect("build");
            let _retained = session.retain(id).expect("first retain");
            assert_eq!(
                session.retain(id).expect_err("owner journal cap"),
                ArenaError::BuildCapacityExceeded
            );
            session.suspend().expect("suspend")
        };
        assert_eq!(arena.suspended_build_owner_count(&build), Ok(1));
        assert_eq!(arena.slot(id.slot).ref_count, 2);

        arena.abort_build(build).expect("abort build");
        let released = arena.poll_reclaim(1);
        assert_eq!(released.transitions, 1);
        assert_eq!(released.nodes_reclaimed, 0);
        assert_eq!(arena.slot(id.slot).ref_count, 1);
        assert_eq!(
            arena.payload(id).expect("original owner remains"),
            b"shared"
        );

        arena.release_owned_later(owner).expect("release original");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn reserved_owner_capacity_prevents_poll_time_growth_across_resume() {
        let mut arena = PageArena::new(limits(8)).expect("arena");
        let shared = arena.allocate_owned(b"shared", &[]).expect("shared node");
        let shared_id = shared.id();
        let (build, reserved_capacity) = {
            let mut session = arena.begin_build().expect("build");
            session
                .reserve_owner_capacity(4)
                .expect("reserve poll owner budget");
            let (owners, capacity) = session.owner_journal_state();
            assert_eq!(owners, 0);
            assert!(capacity >= 4);

            let _retained = session.retain(shared_id).expect("retained owner");
            assert_eq!(session.owner_journal_state(), (1, capacity));
            let _allocated = session.allocate(b"first", &[]).expect("first allocation");
            assert_eq!(session.owner_journal_state(), (2, capacity));
            (session.suspend().expect("suspend"), capacity)
        };

        let build = {
            let mut session = arena.resume_build(build).expect("resume");
            assert_eq!(session.owner_journal_state(), (2, reserved_capacity));
            let _second = session.allocate(b"second", &[]).expect("second allocation");
            assert_eq!(session.owner_journal_state(), (3, reserved_capacity));
            let _third = session.allocate(b"third", &[]).expect("third allocation");
            assert_eq!(session.owner_journal_state(), (4, reserved_capacity));
            session.suspend().expect("suspend complete poll")
        };

        arena.abort_build(build).expect("abort build");
        drain(&mut arena);
        assert_eq!(
            arena.payload(shared_id).expect("shared owner remains"),
            b"shared"
        );
        arena.release_owned_later(shared).expect("release shared");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn owner_capacity_reservation_rejects_logical_overflow_before_growth() {
        let mut arena = PageArena::new(limits(2)).expect("arena");
        let shared = arena.allocate_owned(b"shared", &[]).expect("shared node");
        let shared_id = shared.id();
        let build = {
            let mut session = arena.begin_build().expect("build");
            let initial = session.owner_journal_state();
            assert_eq!(
                session
                    .reserve_owner_capacity(usize::MAX)
                    .expect_err("overflowing reservation"),
                ArenaError::BuildCapacityExceeded
            );
            assert_eq!(session.owner_journal_state(), initial);

            session
                .reserve_owner_capacity(2)
                .expect("reserve complete journal envelope");
            let reserved_capacity = session.owner_journal_state().1;
            assert!(reserved_capacity >= 2);
            let _retained = session.retain(shared_id).expect("retain within reserve");
            assert_eq!(session.owner_journal_state(), (1, reserved_capacity));
            assert_eq!(
                session
                    .reserve_owner_capacity(2)
                    .expect_err("reservation exceeds logical envelope"),
                ArenaError::BuildCapacityExceeded
            );
            assert_eq!(session.owner_journal_state(), (1, reserved_capacity));

            let _allocated = session
                .allocate(b"target", &[])
                .expect("allocate within reserve");
            assert_eq!(session.owner_journal_state(), (2, reserved_capacity));
            assert_eq!(
                session
                    .reserve_owner_capacity(1)
                    .expect_err("full logical owner journal"),
                ArenaError::BuildCapacityExceeded
            );
            assert_eq!(session.owner_journal_state(), (2, reserved_capacity));
            session.suspend().expect("suspend")
        };

        arena.abort_build(build).expect("abort build");
        drain(&mut arena);
        assert_eq!(
            arena.payload(shared_id).expect("shared owner remains"),
            b"shared"
        );
        arena.release_owned_later(shared).expect("release shared");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn cancelling_retained_owners_is_fuelled_and_preserves_the_committed_root() {
        let mut arena = PageArena::new(limits(4)).expect("arena");
        let committed = commit_single_node(&mut arena, b"persistent");
        let id = committed.id();
        {
            let mut session = arena.begin_build().expect("replacement build");
            let _first = session.retain(id).expect("first retain");
            let _second = session.retain(id).expect("second retain");
        }
        assert_eq!(arena.metrics().pending_build_aborts, 1);
        assert_eq!(arena.slot(id.slot).ref_count, 3);

        let first = arena.poll_reclaim(1);
        assert_eq!(first.transitions, 1);
        assert_eq!(first.nodes_reclaimed, 0);
        assert_eq!(arena.slot(id.slot).ref_count, 2);
        assert_eq!(arena.metrics().pending_build_aborts, 1);

        let second = arena.poll_reclaim(1);
        assert_eq!(second.transitions, 1);
        assert_eq!(second.nodes_reclaimed, 0);
        assert_eq!(arena.slot(id.slot).ref_count, 1);
        assert_eq!(arena.metrics().pending_build_aborts, 0);
        assert_eq!(
            arena.payload(id).expect("committed root remains"),
            b"persistent"
        );

        arena
            .release_committed_root(committed)
            .map_err(|failure| failure.error)
            .expect("release committed root");
        let reclaimed = arena.poll_reclaim(1);
        assert_eq!(reclaimed.nodes_reclaimed, 1);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn retained_root_can_be_sealed_as_a_new_persistent_owner() {
        let mut arena = PageArena::new(limits(2)).expect("arena");
        let first_root = commit_single_node(&mut arena, b"reused");
        let id = first_root.id();
        let (build, retained) = {
            let mut session = arena.begin_build().expect("replacement build");
            assert_eq!(
                session.arena().payload(id).expect("read old root"),
                b"reused"
            );
            let retained = session.retain(id).expect("retain old root");
            let build = session.suspend().expect("suspend replacement");
            (build, retained)
        };
        let mut seal = arena.begin_seal(build, retained).expect("begin seal");
        let second_root = arena
            .poll_seal(&mut seal, 1)
            .expect("seal replacement")
            .root
            .expect("second committed root");
        assert_eq!(second_root.id(), id);

        arena
            .release_committed_root(first_root)
            .map_err(|failure| failure.error)
            .expect("release first root");
        drain(&mut arena);
        assert_eq!(arena.payload(id).expect("second root remains"), b"reused");
        arena
            .release_committed_root(second_root)
            .map_err(|failure| failure.error)
            .expect("release second root");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn owner_validation_rejects_a_handle_from_another_build_before_mutation() {
        let mut arena = PageArena::new(limits(3)).expect("arena");
        let (first_build, first_owner) = {
            let mut session = arena.begin_build().expect("first build");
            let owner = session.allocate(b"first", &[]).expect("first owner");
            session.validate_owner(&owner).expect("current owner");
            let build = session.suspend().expect("suspend first");
            (build, owner)
        };

        {
            let second_session = arena.begin_build().expect("second build");
            assert_eq!(
                second_session
                    .validate_owner(&first_owner)
                    .expect_err("wrong build owner"),
                ArenaError::StaleBuild
            );
        }

        {
            let session = arena.resume_build(first_build).expect("resume first");
            session
                .validate_owner(&first_owner)
                .expect("restored current owner");
        }
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn deep_chain_reclaims_iteratively_one_transition_at_a_time() {
        const DEPTH: usize = 2_000;
        let mut arena = PageArena::new(limits(DEPTH + 1)).expect("arena");
        let mut root = arena.allocate_owned(&[], &[]).expect("leaf");

        for _ in 0..DEPTH {
            let parent = arena.allocate_owned(&[], &[root.id()]).expect("parent");
            arena
                .release_owned_later(root)
                .expect("release child owner");
            root = parent;
        }
        arena.release_owned_later(root).expect("release root");

        let mut transitions = 0;
        while arena.metrics().pending_reclaims > 0 {
            let receipt = arena.poll_reclaim(1);
            assert_eq!(receipt.transitions, 1);
            transitions += receipt.transitions;
        }

        assert_eq!(transitions, (DEPTH + 1) + DEPTH);
        assert_eq!(arena.metrics().resident_nodes, 0);
        assert_eq!(arena.metrics().live_payload_bytes, 0);
    }

    #[test]
    fn shared_child_lives_until_its_last_edge_is_retired() {
        let mut arena = PageArena::new(limits(3)).expect("arena");
        let child = arena.allocate_owned(b"child", &[]).expect("child");
        let child_id = child.id();
        let left = arena.allocate_owned(b"left", &[child_id]).expect("left");
        let right = arena.allocate_owned(b"right", &[child_id]).expect("right");
        arena
            .release_owned_later(child)
            .expect("release child owner");

        arena.release_owned_later(left).expect("release left");
        drain(&mut arena);
        assert_eq!(arena.payload(child_id).expect("child remains"), b"child");

        arena.release_owned_later(right).expect("release right");
        drain(&mut arena);
        assert_eq!(
            arena.payload(child_id).expect_err("child retired"),
            ArenaError::StaleHandle
        );
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn page_and_live_payload_limits_fail_before_mutation() {
        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 2,
            max_live_payload_bytes: ARENA_PAGE_BYTES + 1,
            max_children_per_node: 1,
        })
        .expect("arena");

        let too_large = vec![0_u8; ARENA_PAGE_BYTES + 1];
        assert_eq!(
            arena.allocate_owned(&too_large, &[]).expect_err("page cap"),
            ArenaError::PayloadTooLarge
        );
        assert_eq!(arena.metrics().resident_nodes, 0);

        let first = arena
            .allocate_owned(&vec![1_u8; ARENA_PAGE_BYTES], &[])
            .expect("page-sized node");
        assert_eq!(
            arena.allocate_owned(&[2, 3], &[]).expect_err("live budget"),
            ArenaError::PayloadBudgetExceeded
        );
        assert_eq!(arena.metrics().resident_nodes, 1);
        assert_eq!(arena.metrics().live_payload_bytes, ARENA_PAGE_BYTES);

        arena.release_owned_later(first).expect("release first");
        drain(&mut arena);
    }

    #[test]
    fn root_seal_releases_exactly_one_non_root_owner_per_fuel() {
        let mut arena = PageArena::new(limits(32)).expect("arena");
        let (build, root) = {
            let mut session = arena.begin_build().expect("build");
            for index in 0_u8..12 {
                let _ = session.allocate(&[index], &[]).expect("non-root");
            }
            let root = session.allocate(b"root", &[]).expect("root");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let mut seal = arena.begin_seal(build, root).expect("begin seal");
        for remaining in (1..=12).rev() {
            let receipt = arena.poll_seal(&mut seal, 1).expect("seal poll");
            assert_eq!(receipt.transitions, 1);
            assert_eq!(receipt.remaining_non_root_owners, remaining - 1);
            assert!(receipt.root.is_none());
        }
        let receipt = arena.poll_seal(&mut seal, 1).expect("root transfer");
        assert_eq!(receipt.transitions, 1);
        let root = receipt.root.expect("committed root");
        assert_eq!(arena.payload(root.id()).expect("root payload"), b"root");
        arena
            .release_committed_root(root)
            .map_err(|failure| failure.error)
            .expect("release root");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn rejected_non_latest_root_returns_both_linear_capabilities() {
        let mut arena = PageArena::new(limits(4)).expect("arena");
        let (build, first) = {
            let mut session = arena.begin_build().expect("build");
            let first = session.allocate(b"first", &[]).expect("first");
            let _latest = session.allocate(b"latest", &[]).expect("latest");
            let build = session.suspend().expect("suspend");
            (build, first)
        };
        let failure = arena.begin_seal(build, first).expect_err("non-latest root");
        assert_eq!(failure.error, ArenaError::RootNotLatest);
        assert_eq!(arena.payload(failure.root.id()).unwrap(), b"first");
        arena
            .abort_build(failure.build)
            .expect("abort recovered build");
        drain(&mut arena);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn seal_can_cancel_into_the_same_fuelled_abort_queue() {
        let mut arena = PageArena::new(limits(16)).expect("arena");
        let (build, root) = {
            let mut session = arena.begin_build().expect("build");
            for _ in 0..8 {
                let _ = session.allocate(b"node", &[]).expect("node");
            }
            let root = session.allocate(b"root", &[]).expect("root");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let seal = arena.begin_seal(build, root).expect("seal");
        arena.abort_seal(seal).expect("cancel seal");
        let mut transitions = 0;
        while arena.metrics().resident_nodes > 0 {
            let receipt = arena.poll_reclaim(1);
            assert!(receipt.transitions <= 1);
            transitions += receipt.transitions;
        }
        assert!(transitions >= 18);
    }
}
