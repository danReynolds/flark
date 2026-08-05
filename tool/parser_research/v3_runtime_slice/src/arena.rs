//! Minimal generation-safe persistent page arena with iterative retirement.
//!
//! Child edges are integer IDs, never recursive Rust owners. Releasing a root
//! queues reference transitions in an intrusive FIFO stored in the slots. A
//! reclaim poll performs at most caller-supplied fuel and drops at most one
//! bounded payload per transition.

use std::fmt;
use std::ops::{Index, IndexMut};
use std::sync::atomic::{AtomicU64, Ordering};

pub const ARENA_PAGE_BYTES: usize = 4 * 1024;
pub const MAX_ARENA_CHILDREN: usize = 2;
pub const MAX_PACKED_ARENA_CHILDREN: usize = 128;
const ENCODED_ARENA_ID_BYTES: usize = 8;
const SLOT_SEGMENT_SLOTS: usize = 64;
const OWNER_JOURNAL_SEGMENT_SLOTS: usize = 16;
const OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK: usize = 64;
const OWNER_JOURNAL_SLOTS_PER_DIRECTORY_BLOCK: usize =
    OWNER_JOURNAL_SEGMENT_SLOTS * OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
const MAX_OWNER_JOURNAL_SLOTS: usize = 131_072;
const MAX_OWNER_JOURNAL_DIRECTORY_BLOCKS: usize =
    MAX_OWNER_JOURNAL_SLOTS / OWNER_JOURNAL_SLOTS_PER_DIRECTORY_BLOCK;

pub const DEFAULT_MAX_ARENA_SLOTS: u32 = 1 << 20;
pub const DEFAULT_MAX_ARENA_STORAGE_BYTES: usize = 512 * 1024 * 1024;
pub const DEFAULT_MAX_ACTIVE_ARENA_BUILDS: u32 = 16;
pub const DEFAULT_MAX_BUILD_OWNER_SLOTS: usize = 2_048;
pub const DEFAULT_MAX_TRANSACTION_OWNER_SLOTS: usize = MAX_OWNER_JOURNAL_SLOTS;

/// Explicit process-local bounds for one persistent page arena.
///
/// Slot IDs and encoded node storage are separate limits because packed pages
/// are intentionally variable-sized. The default slot envelope admits more
/// than an order of magnitude above the measured 100 MiB ordinary-document
/// shapes while still making allocator exhaustion a recoverable admission
/// decision instead of a surprise metadata-vector copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // Public names keep every bound self-describing.
pub struct ArenaLimits {
    max_slots: u32,
    max_live_storage_bytes: usize,
    max_active_builds: u32,
    max_build_owner_slots: usize,
    max_transaction_owner_slots: usize,
}

impl ArenaLimits {
    #[must_use]
    pub const fn new(
        max_slots: u32,
        max_live_storage_bytes: usize,
        max_active_builds: u32,
        max_build_owner_slots: usize,
    ) -> Self {
        Self {
            max_slots,
            max_live_storage_bytes,
            max_active_builds,
            max_build_owner_slots,
            max_transaction_owner_slots: DEFAULT_MAX_TRANSACTION_OWNER_SLOTS,
        }
    }

    /// Overrides the non-yielding compatibility transaction envelope without
    /// widening the production resumable-build journal bound.
    #[must_use]
    pub const fn with_transaction_owner_slots(mut self, max_slots: usize) -> Self {
        self.max_transaction_owner_slots = max_slots;
        self
    }

    #[must_use]
    pub const fn max_slots(self) -> u32 {
        self.max_slots
    }

    #[must_use]
    pub const fn max_live_storage_bytes(self) -> usize {
        self.max_live_storage_bytes
    }

    #[must_use]
    pub const fn max_active_builds(self) -> u32 {
        self.max_active_builds
    }

    #[must_use]
    pub const fn max_build_owner_slots(self) -> usize {
        self.max_build_owner_slots
    }

    #[must_use]
    pub const fn max_transaction_owner_slots(self) -> usize {
        self.max_transaction_owner_slots
    }
}

impl Default for ArenaLimits {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_ARENA_SLOTS,
            DEFAULT_MAX_ARENA_STORAGE_BYTES,
            DEFAULT_MAX_ACTIVE_ARENA_BUILDS,
            DEFAULT_MAX_BUILD_OWNER_SLOTS,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaId {
    pub index: u32,
    pub generation: u32,
}

/// Process-local identity of one [`PageArena`] instance.
///
/// Packed child edges deliberately omit this value: every such edge is
/// interpreted inside its containing arena. Capabilities that can cross an
/// API boundary carry it so the same `(index, generation)` pair in two arenas
/// can never be mistaken for the same object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaIdentity(u64);

/// Arena-bound query identity for an externally held root or manifest.
///
/// Internal packed edges remain [`ArenaId`] and therefore stay eight bytes.
/// Higher-level typed root IDs wrap this value rather than exporting a bare
/// local slot ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaScopedId {
    arena: ArenaIdentity,
    local: ArenaId,
}

impl ArenaScopedId {
    #[must_use]
    pub const fn arena(self) -> ArenaIdentity {
        self.arena
    }

    #[must_use]
    pub const fn local(self) -> ArenaId {
        self.local
    }

    const fn new(arena: ArenaIdentity, local: ArenaId) -> Self {
        Self { arena, local }
    }
}

/// One transferable caller-owned arena reference.
///
/// Query handles remain [`ArenaId`] and are freely copyable. Ownership is
/// intentionally separate and non-`Clone`: moving this token into a parent or
/// coordinator makes it impossible for the former owner to release the same
/// reference again.
///
/// ```compile_fail
/// use flark_v3_runtime_slice::PageArena;
/// let mut arena = PageArena::new();
/// let allocation = arena.allocate(b"root", &[]).unwrap();
/// let id = allocation.owner.id();
/// arena.release_later(id); // Query IDs are not ownership tokens.
/// ```
#[must_use = "dropping an arena owner without transferring or releasing it leaks its reference"]
#[derive(Debug, PartialEq, Eq)]
pub struct OwnedArenaRef {
    arena: ArenaIdentity,
    id: ArenaId,
}

/// A failed ownership transfer returns the linear capability unchanged.
/// Callers may correct the target arena or state and retry; failure never
/// silently strands a live arena reference.
#[must_use = "the returned owner must be recovered or deliberately handled"]
#[derive(Debug, PartialEq, Eq)]
pub struct OwnerTransferError {
    pub error: ArenaError,
    pub owner: OwnedArenaRef,
}

impl fmt::Display for OwnerTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for OwnerTransferError {}

impl OwnedArenaRef {
    #[must_use]
    pub const fn id(&self) -> ArenaId {
        self.id
    }

    /// Returns an arena-bound query capability for this owned object.
    #[must_use]
    pub const fn scoped_id(&self) -> ArenaScopedId {
        ArenaScopedId::new(self.arena, self.id)
    }

    const fn new(arena: ArenaIdentity, id: ArenaId) -> Self {
        Self { arena, id }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaError {
    PayloadTooLarge(usize),
    TooManyChildren(usize),
    PackedStorageTooLarge(usize),
    WrongArena {
        expected: ArenaIdentity,
        actual: ArenaIdentity,
    },
    StaleId(ArenaId),
    NoOwnedReference(ArenaId),
    ReferenceCountOverflow(ArenaId),
    SlotLimitReached {
        limit: u32,
    },
    StorageBudgetExceeded {
        requested: usize,
        limit: usize,
    },
    OwnerJournalLimitReached {
        limit: usize,
    },
    AllocationFailed(&'static str),
    InvalidLimits(&'static str),
    NodeIndexOverflow,
    GenerationExhausted(ArenaId),
    Invariant(&'static str),
}

impl fmt::Display for ArenaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge(bytes) => write!(
                formatter,
                "arena payload has {bytes} bytes; maximum is {ARENA_PAGE_BYTES}"
            ),
            Self::TooManyChildren(children) => write!(
                formatter,
                "arena node has {children} children; maximum is {MAX_ARENA_CHILDREN}"
            ),
            Self::PackedStorageTooLarge(bytes) => write!(
                formatter,
                "packed arena node uses {bytes} bytes; maximum is {ARENA_PAGE_BYTES}"
            ),
            Self::WrongArena { expected, actual } => write!(
                formatter,
                "arena capability belongs to {actual:?}, not target {expected:?}"
            ),
            Self::StaleId(id) => write!(formatter, "stale arena ID {id:?}"),
            Self::NoOwnedReference(id) => {
                write!(formatter, "arena ID {id:?} has no caller-owned reference")
            }
            Self::ReferenceCountOverflow(id) => {
                write!(formatter, "arena reference count overflow for {id:?}")
            }
            Self::SlotLimitReached { limit } => {
                write!(formatter, "arena reached its configured {limit}-slot limit")
            }
            Self::StorageBudgetExceeded { requested, limit } => write!(
                formatter,
                "arena storage would reach {requested} bytes; configured limit is {limit}"
            ),
            Self::OwnerJournalLimitReached { limit } => write!(
                formatter,
                "arena build journal reached its configured {limit}-owner limit"
            ),
            Self::AllocationFailed(component) => {
                write!(
                    formatter,
                    "arena could not allocate bounded {component} storage"
                )
            }
            Self::InvalidLimits(message) => write!(formatter, "invalid arena limits: {message}"),
            Self::NodeIndexOverflow => formatter.write_str("arena node index exceeds u32"),
            Self::GenerationExhausted(id) => {
                write!(formatter, "arena generation exhausted for {id:?}")
            }
            Self::Invariant(message) => write!(formatter, "arena invariant failed: {message}"),
        }
    }
}

impl std::error::Error for ArenaError {}

#[derive(Debug)]
struct ArenaNode {
    storage: Vec<u8>,
    payload_len: u16,
    child_count: u16,
}

fn decode_child(node: &ArenaNode, index: usize) -> ArenaId {
    let start = usize::from(node.payload_len) + index * ENCODED_ARENA_ID_BYTES;
    ArenaId {
        index: u32::from_le_bytes(
            node.storage[start..start + 4]
                .try_into()
                .expect("encoded arena child index"),
        ),
        generation: u32::from_le_bytes(
            node.storage[start + 4..start + 8]
                .try_into()
                .expect("encoded arena child generation"),
        ),
    }
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    references: u32,
    owned_references: u32,
    scheduled_releases: u32,
    node: Option<ArenaNode>,
    next_free: Option<u32>,
    next_pending: Option<u32>,
    queued: bool,
    retired: bool,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            generation: 1,
            references: 0,
            owned_references: 0,
            scheduled_releases: 0,
            node: None,
            next_free: None,
            next_pending: None,
            queued: false,
            retired: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MetadataGrowthReceipt {
    segment_added: bool,
    entries_initialized: usize,
    prior_entries_moved: usize,
    segment_actual_capacity: usize,
    directory_block_added: bool,
    directory_descriptors_preflighted: usize,
    directory_actual_capacity: usize,
    prior_directory_descriptors_moved: usize,
}

/// Arena-specific segmented storage for generation-safe page slots.
///
/// The descriptor directory reserves its complete configured envelope before
/// the arena is admitted. Appending a segment therefore moves zero existing
/// `Slot` values and zero existing segment descriptors; it allocates and
/// initializes exactly [`SLOT_SEGMENT_SLOTS`] new entries.
#[derive(Debug)]
struct SlotStorage {
    segments: Vec<Vec<Slot>>,
    len: usize,
    max_slots: usize,
    max_segments: usize,
}

impl SlotStorage {
    fn try_new(max_slots: usize) -> Result<Self, ArenaError> {
        let max_segments = max_slots.div_ceil(SLOT_SEGMENT_SLOTS);
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(max_segments)
            .map_err(|_| ArenaError::AllocationFailed("slot-directory"))?;
        Ok(Self {
            segments,
            len: 0,
            max_slots,
            max_segments,
        })
    }

    fn preflight_append(&mut self) -> Result<MetadataGrowthReceipt, ArenaError> {
        if self.len == self.max_slots {
            return Err(ArenaError::SlotLimitReached {
                limit: u32::try_from(self.max_slots)
                    .expect("validated slot limit fits in the arena ID"),
            });
        }
        if !self.len.is_multiple_of(SLOT_SEGMENT_SLOTS) {
            return Ok(MetadataGrowthReceipt::default());
        }
        debug_assert_eq!(self.segments.len(), self.len / SLOT_SEGMENT_SLOTS);
        if self.segments.len() == self.max_segments {
            return Err(ArenaError::SlotLimitReached {
                limit: u32::try_from(self.max_slots)
                    .expect("validated slot limit fits in the arena ID"),
            });
        }

        let directory_pointer = self.segments.as_ptr();
        let directory_capacity = self.segments.capacity();
        let mut segment = Vec::new();
        segment
            .try_reserve_exact(SLOT_SEGMENT_SLOTS)
            .map_err(|_| ArenaError::AllocationFailed("slot-segment"))?;
        for _ in 0..SLOT_SEGMENT_SLOTS {
            segment.push(Slot::default());
        }
        let segment_actual_capacity = segment.capacity();
        self.segments.push(segment);
        debug_assert_eq!(self.segments.as_ptr(), directory_pointer);
        debug_assert_eq!(self.segments.capacity(), directory_capacity);
        Ok(MetadataGrowthReceipt {
            segment_added: true,
            entries_initialized: SLOT_SEGMENT_SLOTS,
            prior_entries_moved: 0,
            segment_actual_capacity,
            ..MetadataGrowthReceipt::default()
        })
    }

    fn append_preflighted(&mut self) -> usize {
        debug_assert!(self.len < self.max_slots);
        debug_assert!(self.len / SLOT_SEGMENT_SLOTS < self.segments.len());
        let index = self.len;
        self.len += 1;
        index
    }

    const fn len(&self) -> usize {
        self.len
    }

    fn initialized_capacity(&self) -> usize {
        self.segments.len() * SLOT_SEGMENT_SLOTS
    }

    fn actual_capacity(&self) -> usize {
        self.segments.iter().map(Vec::capacity).sum()
    }

    fn storage_bytes(&self) -> usize {
        self.segments.capacity() * std::mem::size_of::<Vec<Slot>>()
            + self
                .segments
                .iter()
                .map(|segment| segment.capacity() * std::mem::size_of::<Slot>())
                .sum::<usize>()
    }

    fn get(&self, index: usize) -> Option<&Slot> {
        if index >= self.len {
            return None;
        }
        let segment = index / SLOT_SEGMENT_SLOTS;
        let offset = index % SLOT_SEGMENT_SLOTS;
        self.segments.get(segment)?.get(offset)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut Slot> {
        if index >= self.len {
            return None;
        }
        let segment = index / SLOT_SEGMENT_SLOTS;
        let offset = index % SLOT_SEGMENT_SLOTS;
        self.segments.get_mut(segment)?.get_mut(offset)
    }
}

impl Index<usize> for SlotStorage {
    type Output = Slot;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("arena slot index is in bounds")
    }
}

impl IndexMut<usize> for SlotStorage {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("arena slot index is in bounds")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // Independent slot/journal receipt axes.
pub struct AllocationReceipt {
    pub payload_bytes_copied: usize,
    pub edge_bytes_copied: usize,
    pub child_references_added: usize,
    pub slot_reused: bool,
    pub slot_added: bool,
    pub slot_metadata_segment_added: bool,
    pub slot_metadata_entries_initialized: usize,
    pub slot_metadata_prior_entries_moved: usize,
    pub slot_metadata_segment_actual_capacity: usize,
    pub owner_journal_segment_added: bool,
    pub owner_journal_entries_initialized: usize,
    pub owner_journal_prior_entries_moved: usize,
    pub owner_journal_segment_actual_capacity: usize,
    pub owner_journal_directory_block_added: bool,
    pub owner_journal_directory_descriptors_preflighted: usize,
    pub owner_journal_directory_actual_capacity: usize,
    pub owner_journal_prior_directory_descriptors_moved: usize,
}

impl AllocationReceipt {
    fn note_owner_journal_growth(&mut self, growth: MetadataGrowthReceipt) {
        self.owner_journal_segment_added = growth.segment_added;
        self.owner_journal_entries_initialized = growth.entries_initialized;
        self.owner_journal_prior_entries_moved = growth.prior_entries_moved;
        self.owner_journal_segment_actual_capacity = growth.segment_actual_capacity;
        self.owner_journal_directory_block_added = growth.directory_block_added;
        self.owner_journal_directory_descriptors_preflighted =
            growth.directory_descriptors_preflighted;
        self.owner_journal_directory_actual_capacity = growth.directory_actual_capacity;
        self.owner_journal_prior_directory_descriptors_moved =
            growth.prior_directory_descriptors_moved;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArenaAllocation {
    pub owner: OwnedArenaRef,
    pub receipt: AllocationReceipt,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReclaimReceipt {
    pub reference_transitions: usize,
    pub nodes_reclaimed: usize,
    pub payload_bytes_reclaimed: usize,
    pub child_releases_enqueued: usize,
    pub slots_retired: usize,
    pub pending_after: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimPollError {
    pub error: ArenaError,
    pub receipt: ReclaimReceipt,
}

impl fmt::Display for ReclaimPollError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} after {} reference transitions",
            self.error, self.receipt.reference_transitions
        )
    }
}

impl std::error::Error for ReclaimPollError {}

/// Generation-safe identity for one arena-owned resumable build journal.
///
/// This is a query identity, not the authority to resume or cancel a build.
/// [`ArenaBuildTicket`] is the linear capability for those transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArenaBuildId {
    arena_nonce: u64,
    index: u32,
    generation: u32,
}

/// The externally observable lifecycle of one resumable build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaBuildLifecycle {
    Suspended,
    Resumed,
    Aborting,
}

/// Failures at the resumable-build ownership boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaBuildError {
    WrongArena(ArenaBuildId),
    StaleBuild(ArenaBuildId),
    StaleTicket(ArenaBuildId),
    ReplayedTicket(ArenaBuildId),
    BuildAborting(ArenaBuildId),
    BuildNotResumed(ArenaBuildId),
    BuildNotAborting(ArenaBuildId),
    CrossBuildOwner {
        expected: ArenaBuildId,
        actual: ArenaBuildId,
    },
    StaleOwnerHandle(ArenaBuildId),
    ExpectedExactlyOneOwner {
        build: ArenaBuildId,
        actual: usize,
    },
    BuildLimitReached {
        limit: u32,
    },
    BuildIndexOverflow,
    LeaseGenerationExhausted(ArenaBuildId),
    Arena(ArenaError),
    Invariant(&'static str),
}

impl fmt::Display for ArenaBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongArena(id) => write!(formatter, "build {id:?} belongs to another arena"),
            Self::StaleBuild(id) => write!(formatter, "stale arena build {id:?}"),
            Self::StaleTicket(id) => write!(formatter, "stale ticket for arena build {id:?}"),
            Self::ReplayedTicket(id) => write!(formatter, "replayed ticket for arena build {id:?}"),
            Self::BuildAborting(id) => write!(formatter, "arena build {id:?} is aborting"),
            Self::BuildNotResumed(id) => write!(formatter, "arena build {id:?} is not resumed"),
            Self::BuildNotAborting(id) => {
                write!(formatter, "arena build {id:?} is not aborting")
            }
            Self::CrossBuildOwner { expected, actual } => write!(
                formatter,
                "owner from build {actual:?} used in build {expected:?}"
            ),
            Self::StaleOwnerHandle(id) => {
                write!(formatter, "stale owner handle for arena build {id:?}")
            }
            Self::ExpectedExactlyOneOwner { build, actual } => write!(
                formatter,
                "arena build {build:?} has {actual} owners; commit requires exactly one"
            ),
            Self::BuildLimitReached { limit } => {
                write!(
                    formatter,
                    "arena reached its configured {limit}-build limit"
                )
            }
            Self::BuildIndexOverflow => formatter.write_str("arena build index exceeds u32"),
            Self::LeaseGenerationExhausted(id) => {
                write!(
                    formatter,
                    "ticket lease generation exhausted for build {id:?}"
                )
            }
            Self::Arena(error) => error.fmt(formatter),
            Self::Invariant(message) => {
                write!(formatter, "arena build invariant failed: {message}")
            }
        }
    }
}

impl std::error::Error for ArenaBuildError {}

impl From<ArenaError> for ArenaBuildError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<OwnerTransferError> for ArenaBuildError {
    fn from(error: OwnerTransferError) -> Self {
        Self::Arena(error.error)
    }
}

/// Linear authority to resume or begin aborting one suspended build.
///
/// The ticket is deliberately neither `Copy` nor `Clone`. A successful resume
/// consumes it, and suspension returns a ticket with a fresh lease generation.
/// Losing a suspended ticket does not scan or release the arena-owned journal;
/// coordinators must explicitly call [`PageArena::begin_build_abort`].
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a suspended build ticket must be resumed or explicitly aborted"]
pub struct ArenaBuildTicket {
    id: ArenaBuildId,
    lease_generation: u64,
}

impl ArenaBuildTicket {
    #[must_use]
    pub const fn id(&self) -> ArenaBuildId {
        self.id
    }
}

/// Linear handle for one owner retained by an arena-owned build journal.
///
/// Handles remain valid across suspension slices, but are capability-bound to
/// one [`ArenaBuildId`] and cannot be replayed after transfer.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "the build journal still owns the referenced arena page"]
pub struct ArenaBuildOwner {
    build: ArenaBuildId,
    handle: ArenaOwnerHandle,
}

/// One short-lived mutable borrow of an arena-owned resumable build.
///
/// Suspending consumes the session and returns a fresh linear ticket. Dropping
/// an unfinished session performs only a constant-time transition to
/// `Aborting`; owner scheduling remains the responsibility of fuelled
/// [`PageArena::poll_build_abort`] calls.
#[derive(Debug)]
#[must_use = "suspend, commit, or explicitly abort the resumed build"]
pub struct ArenaBuildSession<'a> {
    arena: &'a mut PageArena,
    id: ArenaBuildId,
    lease_generation: u64,
    finished: bool,
}

/// A failed ticket transition returns the still-linear ticket to its caller.
#[derive(Debug, PartialEq, Eq)]
pub struct ArenaBuildTicketError {
    pub error: ArenaBuildError,
    pub ticket: ArenaBuildTicket,
}

impl fmt::Display for ArenaBuildTicketError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for ArenaBuildTicketError {}

/// Receipt for one strictly fuelled cancellation poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaBuildAbortReceipt {
    pub owners_scheduled: usize,
    pub owners_remaining: usize,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaBuildJournalMetrics {
    pub live_owners: usize,
    pub maximum_live_owners: usize,
    pub slots: usize,
    pub slot_capacity: usize,
    pub initialized_slot_capacity: usize,
    pub storage_bytes: usize,
    pub segments: usize,
    pub directory_blocks: usize,
    pub top_directory_block_slots: usize,
    pub hard_slot_limit: usize,
    pub prior_entries_moved: usize,
    pub prior_directory_descriptors_moved: usize,
    pub maximum_segment_entries_initialized: usize,
    pub maximum_directory_descriptors_preflighted: usize,
}

/// Receipt for admitting one arena-owned resumable build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaBuildAdmissionReceipt {
    pub build_slot_reused: bool,
    pub build_slot_added: bool,
    pub build_slots_initialized: usize,
    pub prior_build_slots_moved: usize,
    pub build_directory_logical_limit: usize,
    pub build_directory_actual_capacity: usize,
    pub owner_journal_top_directory_block_slots: usize,
    pub owner_journal_segments_per_directory_block: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaMetrics {
    pub slots: usize,
    pub slot_capacity: usize,
    pub slot_initialized_capacity: usize,
    pub slot_storage_bytes: usize,
    pub slot_segments: usize,
    pub slot_directory_logical_segments: usize,
    pub slot_directory_actual_capacity: usize,
    pub slot_hard_limit: usize,
    pub slot_metadata_prior_entries_moved: usize,
    pub maximum_slot_segment_entries_initialized: usize,
    pub live_storage_hard_limit: usize,
    pub build_slots: usize,
    pub build_slot_capacity: usize,
    pub build_slot_hard_limit: usize,
    pub build_metadata_prior_entries_moved: usize,
    pub reusable_slots: usize,
    pub retired_slots: usize,
    pub live_nodes: usize,
    pub live_payload_bytes: usize,
    pub live_edge_bytes: usize,
    pub live_storage_bytes: usize,
    pub heap_page_allocations: usize,
    pub pending_releases: usize,
    pub queued_release_nodes: usize,
    pub high_water_live_nodes: usize,
    pub high_water_payload_bytes: usize,
    pub high_water_storage_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildSlotLifecycle {
    Vacant,
    Suspended,
    Resumed,
    Aborting,
    Retired,
}

#[derive(Debug)]
struct BuildSlot {
    generation: u32,
    lease_generation: u64,
    lifecycle: BuildSlotLifecycle,
    journal: OwnerJournal,
    next_free: Option<u32>,
}

impl BuildSlot {
    fn new(max_journal_slots: usize) -> Self {
        Self {
            generation: 1,
            lease_generation: 1,
            lifecycle: BuildSlotLifecycle::Vacant,
            journal: OwnerJournal::new(max_journal_slots),
            next_free: None,
        }
    }
}

static NEXT_ARENA_NONCE: AtomicU64 = AtomicU64::new(1);

fn next_arena_nonce() -> u64 {
    NEXT_ARENA_NONCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |nonce| {
            nonce.checked_add(1)
        })
        .expect("page-arena identity space exhausted")
}

/// Fixed-payload arena with intrusive free and pending-release lists.
#[derive(Debug)]
pub struct PageArena {
    arena_nonce: u64,
    limits: ArenaLimits,
    slots: SlotStorage,
    free_head: Option<u32>,
    pending_head: Option<u32>,
    pending_tail: Option<u32>,
    reusable_slots: usize,
    retired_slots: usize,
    live_nodes: usize,
    live_payload_bytes: usize,
    live_edge_bytes: usize,
    live_storage_bytes: usize,
    pending_releases: usize,
    queued_release_nodes: usize,
    high_water_live_nodes: usize,
    high_water_payload_bytes: usize,
    high_water_storage_bytes: usize,
    slot_metadata_prior_entries_moved: usize,
    maximum_slot_segment_entries_initialized: usize,
    builds: Vec<BuildSlot>,
    build_free_head: Option<u32>,
    build_metadata_prior_entries_moved: usize,
    #[cfg(test)]
    release_transfer_test_error: Option<ArenaError>,
}

impl Default for PageArena {
    fn default() -> Self {
        Self::try_with_limits(ArenaLimits::default())
            .expect("default bounded page-arena metadata admission")
    }
}

impl PageArena {
    /// Fallibly admits one arena and pre-reserves its fixed segment/build
    /// descriptor directories. Neither directory grows after this call.
    pub fn try_with_limits(limits: ArenaLimits) -> Result<Self, ArenaError> {
        if limits.max_slots == 0 {
            return Err(ArenaError::InvalidLimits("max_slots must be nonzero"));
        }
        if limits.max_live_storage_bytes == 0 {
            return Err(ArenaError::InvalidLimits(
                "max_live_storage_bytes must be nonzero",
            ));
        }
        if limits.max_active_builds == 0 {
            return Err(ArenaError::InvalidLimits(
                "max_active_builds must be nonzero",
            ));
        }
        if !(1..=MAX_OWNER_JOURNAL_SLOTS).contains(&limits.max_build_owner_slots) {
            return Err(ArenaError::InvalidLimits(
                "max_build_owner_slots must be in 1..=131072",
            ));
        }
        if !(1..=MAX_OWNER_JOURNAL_SLOTS).contains(&limits.max_transaction_owner_slots) {
            return Err(ArenaError::InvalidLimits(
                "max_transaction_owner_slots must be in 1..=131072",
            ));
        }

        let slots = SlotStorage::try_new(limits.max_slots as usize)?;
        let build_limit = usize::try_from(limits.max_active_builds)
            .map_err(|_| ArenaError::InvalidLimits("active-build limit exceeds usize"))?;
        let mut builds = Vec::new();
        builds
            .try_reserve_exact(build_limit)
            .map_err(|_| ArenaError::AllocationFailed("build-directory"))?;
        Ok(Self {
            arena_nonce: next_arena_nonce(),
            limits,
            slots,
            free_head: None,
            pending_head: None,
            pending_tail: None,
            reusable_slots: 0,
            retired_slots: 0,
            live_nodes: 0,
            live_payload_bytes: 0,
            live_edge_bytes: 0,
            live_storage_bytes: 0,
            pending_releases: 0,
            queued_release_nodes: 0,
            high_water_live_nodes: 0,
            high_water_payload_bytes: 0,
            high_water_storage_bytes: 0,
            slot_metadata_prior_entries_moved: 0,
            maximum_slot_segment_entries_initialized: 0,
            builds,
            build_free_head: None,
            build_metadata_prior_entries_moved: 0,
            #[cfg(test)]
            release_transfer_test_error: None,
        })
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn limits(&self) -> ArenaLimits {
        self.limits
    }

    /// Returns the stable process-local identity used by external root
    /// capabilities. It is not encoded on local packed child edges.
    #[must_use]
    pub const fn identity(&self) -> ArenaIdentity {
        ArenaIdentity(self.arena_nonce)
    }

    /// Validates an arena-bound query capability and returns its local ID for
    /// internal traversal.
    pub fn local_id(&self, id: ArenaScopedId) -> Result<ArenaId, ArenaError> {
        if id.arena != self.identity() {
            return Err(ArenaError::WrongArena {
                expected: self.identity(),
                actual: id.arena,
            });
        }
        self.validate_live(id.local)?;
        Ok(id.local)
    }

    /// Binds one already-selected live local node to this arena for an
    /// internal typed query capability. This does not retain the node and is
    /// deliberately crate-private: public callers obtain scoped identities
    /// only from an owned typed root.
    pub(crate) fn scoped_query_id(&self, id: ArenaId) -> Result<ArenaScopedId, ArenaError> {
        self.validate_live(id)?;
        Ok(ArenaScopedId::new(self.identity(), id))
    }

    /// Creates an empty arena-owned build journal and returns its linear lease.
    pub fn begin_build(&mut self) -> Result<ArenaBuildTicket, ArenaBuildError> {
        self.begin_build_with_receipt().map(|(ticket, _)| ticket)
    }

    /// Admits one build and reports the exact fixed metadata work performed.
    pub fn begin_build_with_receipt(
        &mut self,
    ) -> Result<(ArenaBuildTicket, ArenaBuildAdmissionReceipt), ArenaBuildError> {
        let mut receipt = ArenaBuildAdmissionReceipt {
            build_directory_logical_limit: self.limits.max_active_builds as usize,
            build_directory_actual_capacity: self.builds.capacity(),
            owner_journal_top_directory_block_slots: MAX_OWNER_JOURNAL_DIRECTORY_BLOCKS,
            owner_journal_segments_per_directory_block: OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK,
            ..ArenaBuildAdmissionReceipt::default()
        };
        let index = if let Some(index) = self.build_free_head {
            let slot = &mut self.builds[index as usize];
            self.build_free_head = slot.next_free.take();
            debug_assert_eq!(slot.lifecycle, BuildSlotLifecycle::Vacant);
            debug_assert_eq!(slot.journal.live_owners(), 0);
            receipt.build_slot_reused = true;
            index
        } else {
            if self.builds.len() == self.limits.max_active_builds as usize {
                return Err(ArenaBuildError::BuildLimitReached {
                    limit: self.limits.max_active_builds,
                });
            }
            let index = u32::try_from(self.builds.len())
                .map_err(|_| ArenaBuildError::BuildIndexOverflow)?;
            let pointer = self.builds.as_ptr();
            let capacity = self.builds.capacity();
            self.builds
                .push(BuildSlot::new(self.limits.max_build_owner_slots));
            debug_assert_eq!(self.builds.as_ptr(), pointer);
            debug_assert_eq!(self.builds.capacity(), capacity);
            receipt.build_slot_added = true;
            receipt.build_slots_initialized = 1;
            receipt.prior_build_slots_moved = 0;
            index
        };
        let slot = &mut self.builds[index as usize];
        slot.journal.reset_high_water();
        slot.lifecycle = BuildSlotLifecycle::Suspended;
        slot.lease_generation = 1;
        let id = ArenaBuildId {
            arena_nonce: self.arena_nonce,
            index,
            generation: slot.generation,
        };
        Ok((
            ArenaBuildTicket {
                id,
                lease_generation: slot.lease_generation,
            },
            receipt,
        ))
    }

    /// Exchanges one suspended ticket for a short-lived mutable build session.
    ///
    /// On validation failure, ownership of the ticket is returned in the error
    /// so an accidental wrong-arena call cannot orphan a suspended journal.
    pub fn resume_build(
        &mut self,
        ticket: ArenaBuildTicket,
    ) -> Result<ArenaBuildSession<'_>, ArenaBuildTicketError> {
        let id = ticket.id;
        let validation = self.validate_suspended_ticket(&ticket);
        if let Err(error) = validation {
            return Err(ArenaBuildTicketError { error, ticket });
        }
        self.builds[id.index as usize].lifecycle = BuildSlotLifecycle::Resumed;
        Ok(ArenaBuildSession {
            arena: self,
            id,
            lease_generation: ticket.lease_generation,
            finished: false,
        })
    }

    /// Starts cancellation without walking or scheduling any journal owner.
    ///
    /// Use [`Self::poll_build_abort`] to make bounded cleanup progress.
    pub fn begin_build_abort(
        &mut self,
        ticket: ArenaBuildTicket,
    ) -> Result<ArenaBuildId, ArenaBuildTicketError> {
        let id = ticket.id;
        let validation = self.validate_suspended_ticket(&ticket);
        if let Err(error) = validation {
            return Err(ArenaBuildTicketError { error, ticket });
        }
        self.builds[id.index as usize].lifecycle = BuildSlotLifecycle::Aborting;
        Ok(id)
    }

    /// Schedules at most `fuel` build-owned references for iterative reclaim.
    ///
    /// One unit of fuel transfers exactly one owner from the build journal into
    /// the arena's existing release queue. Reclaiming pages and their child
    /// edges remains separately bounded by [`Self::poll_reclaim`]. Sparse
    /// journal holes do not consume hidden work because the journal maintains
    /// an intrusive live-owner list.
    pub fn poll_build_abort(
        &mut self,
        id: ArenaBuildId,
        fuel: usize,
    ) -> Result<ArenaBuildAbortReceipt, ArenaBuildError> {
        self.validate_build_id(id)?;
        if self.builds[id.index as usize].lifecycle != BuildSlotLifecycle::Aborting {
            return Err(ArenaBuildError::BuildNotAborting(id));
        }

        let mut owners_scheduled = 0;
        while owners_scheduled < fuel {
            let removed = self.builds[id.index as usize]
                .journal
                .pop_live_owner_for_transfer()
                .map_err(|OwnerJournalError::StaleHandle| {
                    ArenaBuildError::Invariant("abort live-owner list contains a stale handle")
                })?;
            let Some(removed) = removed else {
                break;
            };
            let RemovedOwnerJournalEntry {
                owner,
                index,
                generation,
            } = removed;
            if let Err(failure) = self.release_later(owner) {
                // Restore the returned linear owner into the exact journal slot
                // from which this poll removed it before exposing the transfer
                // failure. The build remains aborting and a later bounded poll
                // can retry scheduling it.
                self.builds[id.index as usize]
                    .journal
                    .restore_removed(RemovedOwnerJournalEntry {
                        owner: failure.owner,
                        index,
                        generation,
                    });
                return Err(failure.error.into());
            }
            owners_scheduled += 1;
        }
        let owners_remaining = self.builds[id.index as usize].journal.live_owners();
        let complete = owners_remaining == 0;
        if complete {
            self.finish_build_slot(id)?;
        }
        Ok(ArenaBuildAbortReceipt {
            owners_scheduled,
            owners_remaining,
            complete,
        })
    }

    pub fn build_lifecycle(
        &self,
        id: ArenaBuildId,
    ) -> Result<ArenaBuildLifecycle, ArenaBuildError> {
        let slot = self.validate_build_id(id)?;
        match slot.lifecycle {
            BuildSlotLifecycle::Suspended => Ok(ArenaBuildLifecycle::Suspended),
            BuildSlotLifecycle::Resumed => Ok(ArenaBuildLifecycle::Resumed),
            BuildSlotLifecycle::Aborting => Ok(ArenaBuildLifecycle::Aborting),
            BuildSlotLifecycle::Vacant | BuildSlotLifecycle::Retired => {
                Err(ArenaBuildError::StaleBuild(id))
            }
        }
    }

    /// Resolves one journal owner while the caller holds the exact current
    /// suspended ticket for that build generation.
    ///
    /// Resumable jobs are constructed between sessions, so they cannot use
    /// [`ArenaBuildSession::owner_id`]. This narrow sibling keeps the same
    /// build/handle checks without exposing a way to mutate the journal or to
    /// query it from a copied [`ArenaBuildId`].
    pub(crate) fn suspended_owner_id(
        &self,
        ticket: &ArenaBuildTicket,
        owner: &ArenaBuildOwner,
    ) -> Result<ArenaId, ArenaBuildError> {
        self.validate_suspended_ticket(ticket)?;
        if owner.build != ticket.id {
            return Err(ArenaBuildError::CrossBuildOwner {
                expected: ticket.id,
                actual: owner.build,
            });
        }
        self.builds[ticket.id.index as usize]
            .journal
            .id(&owner.handle)
            .map_err(|OwnerJournalError::StaleHandle| ArenaBuildError::StaleOwnerHandle(ticket.id))
    }

    pub fn build_journal_metrics(
        &self,
        id: ArenaBuildId,
    ) -> Result<ArenaBuildJournalMetrics, ArenaBuildError> {
        let journal = &self.validate_build_id(id)?.journal;
        Ok(ArenaBuildJournalMetrics {
            live_owners: journal.live_owners(),
            maximum_live_owners: journal.maximum_live_owners(),
            slots: journal.slots(),
            slot_capacity: journal.capacity(),
            initialized_slot_capacity: journal.initialized_capacity(),
            storage_bytes: journal.storage_bytes(),
            segments: journal.segments(),
            directory_blocks: journal.directory_blocks(),
            top_directory_block_slots: MAX_OWNER_JOURNAL_DIRECTORY_BLOCKS,
            hard_slot_limit: journal.hard_slot_limit(),
            prior_entries_moved: journal.prior_entries_moved(),
            prior_directory_descriptors_moved: journal.prior_directory_descriptors_moved(),
            maximum_segment_entries_initialized: journal.maximum_segment_entries_initialized(),
            maximum_directory_descriptors_preflighted: journal
                .maximum_directory_descriptors_preflighted(),
        })
    }

    pub fn allocate(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocation, ArenaError> {
        if payload.len() > ARENA_PAGE_BYTES {
            return Err(ArenaError::PayloadTooLarge(payload.len()));
        }
        if children.len() > MAX_ARENA_CHILDREN {
            return Err(ArenaError::TooManyChildren(children.len()));
        }

        self.allocate_inner(payload, children)
    }

    /// Allocates one packed page whose payload and variable ownership-edge
    /// table share the same bounded storage allocation.
    ///
    /// Ordinary sequence branches continue to use [`Self::allocate`] and its
    /// two-child contract. This entry point exists for packed local-forest
    /// pages that can name many out-of-line semantic subtrees without proxy
    /// arena nodes.
    pub fn allocate_packed(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocation, ArenaError> {
        if children.len() > MAX_PACKED_ARENA_CHILDREN {
            return Err(ArenaError::TooManyChildren(children.len()));
        }
        let storage_bytes = payload
            .len()
            .checked_add(
                children
                    .len()
                    .checked_mul(ENCODED_ARENA_ID_BYTES)
                    .ok_or(ArenaError::PackedStorageTooLarge(usize::MAX))?,
            )
            .ok_or(ArenaError::PackedStorageTooLarge(usize::MAX))?;
        if storage_bytes > ARENA_PAGE_BYTES {
            return Err(ArenaError::PackedStorageTooLarge(storage_bytes));
        }
        self.allocate_inner(payload, children)
    }

    fn allocate_inner(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocation, ArenaError> {
        if payload.len() > ARENA_PAGE_BYTES {
            return Err(ArenaError::PayloadTooLarge(payload.len()));
        }

        let mut occurrences = Vec::<(ArenaId, u32)>::new();
        occurrences
            .try_reserve_exact(children.len())
            .map_err(|_| ArenaError::AllocationFailed("child-occurrence scratch"))?;
        for child in children {
            self.validate_live(*child)?;
            if let Some((_, count)) = occurrences
                .iter_mut()
                .find(|(candidate, _)| candidate == child)
            {
                *count = count
                    .checked_add(1)
                    .ok_or(ArenaError::ReferenceCountOverflow(*child))?;
            } else {
                occurrences.push((*child, 1));
            }
        }
        for (child, count) in &occurrences {
            let slot = self.slot(*child)?;
            slot.references
                .checked_add(*count)
                .ok_or(ArenaError::ReferenceCountOverflow(*child))?;
        }

        let payload_len =
            u16::try_from(payload.len()).map_err(|_| ArenaError::PayloadTooLarge(payload.len()))?;
        let child_count = u16::try_from(children.len())
            .map_err(|_| ArenaError::TooManyChildren(children.len()))?;
        let edge_bytes = children.len() * ENCODED_ARENA_ID_BYTES;
        let requested_storage_bytes = payload.len() + edge_bytes;
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(requested_storage_bytes)
            .map_err(|_| ArenaError::AllocationFailed("encoded arena page"))?;
        let reserved_storage_bytes = storage.capacity();
        let requested_live_storage = self
            .live_storage_bytes
            .checked_add(reserved_storage_bytes)
            .ok_or(ArenaError::StorageBudgetExceeded {
                requested: usize::MAX,
                limit: self.limits.max_live_storage_bytes,
            })?;
        if requested_live_storage > self.limits.max_live_storage_bytes {
            return Err(ArenaError::StorageBudgetExceeded {
                requested: requested_live_storage,
                limit: self.limits.max_live_storage_bytes,
            });
        }
        storage.extend_from_slice(payload);
        for child in children {
            storage.extend_from_slice(&child.index.to_le_bytes());
            storage.extend_from_slice(&child.generation.to_le_bytes());
        }

        let (index, slot_reused, slot_growth) = self.take_free_or_append()?;
        let generation = self.slots[index as usize].generation;
        let id = ArenaId { index, generation };
        for child in children {
            self.slot_mut(*child)
                .expect("validated children remain live before arena mutation")
                .references += 1;
        }
        let slot = &mut self.slots[index as usize];
        debug_assert!(slot.node.is_none() && slot.references == 0 && !slot.retired);
        slot.references = 1;
        slot.owned_references = 1;
        slot.scheduled_releases = 0;
        slot.next_free = None;
        slot.node = Some(ArenaNode {
            storage,
            payload_len,
            child_count,
        });

        self.live_nodes += 1;
        self.live_payload_bytes += payload.len();
        self.live_edge_bytes += edge_bytes;
        self.live_storage_bytes = requested_live_storage;
        self.slot_metadata_prior_entries_moved += slot_growth.prior_entries_moved;
        self.maximum_slot_segment_entries_initialized = self
            .maximum_slot_segment_entries_initialized
            .max(slot_growth.entries_initialized);
        self.high_water_live_nodes = self.high_water_live_nodes.max(self.live_nodes);
        self.high_water_payload_bytes = self.high_water_payload_bytes.max(self.live_payload_bytes);
        self.high_water_storage_bytes = self.high_water_storage_bytes.max(self.live_storage_bytes);
        Ok(ArenaAllocation {
            owner: OwnedArenaRef::new(self.identity(), id),
            receipt: AllocationReceipt {
                payload_bytes_copied: payload.len(),
                edge_bytes_copied: edge_bytes,
                child_references_added: children.len(),
                slot_reused,
                slot_added: !slot_reused,
                slot_metadata_segment_added: slot_growth.segment_added,
                slot_metadata_entries_initialized: slot_growth.entries_initialized,
                slot_metadata_prior_entries_moved: slot_growth.prior_entries_moved,
                slot_metadata_segment_actual_capacity: slot_growth.segment_actual_capacity,
                ..AllocationReceipt::default()
            },
        })
    }

    /// Mints one new transferable owner for a live query ID.
    pub fn retain(&mut self, id: ArenaId) -> Result<OwnedArenaRef, ArenaError> {
        let slot = self.slot(id)?;
        let references = slot
            .references
            .checked_add(1)
            .ok_or(ArenaError::ReferenceCountOverflow(id))?;
        let owned_references = slot
            .owned_references
            .checked_add(1)
            .ok_or(ArenaError::ReferenceCountOverflow(id))?;
        let slot = self.slot_mut(id)?;
        slot.references = references;
        slot.owned_references = owned_references;
        Ok(OwnedArenaRef::new(self.identity(), id))
    }

    /// Transfers one caller-owned reference into the iterative reclaim queue.
    #[allow(clippy::needless_pass_by_value)] // Moving the token is the ownership proof.
    pub fn preflight_release(&self, owner: &OwnedArenaRef) -> Result<(), ArenaError> {
        if owner.arena != self.identity() {
            return Err(ArenaError::WrongArena {
                expected: self.identity(),
                actual: owner.arena,
            });
        }
        let slot = self.slot(owner.id)?;
        if slot.owned_references == 0 {
            return Err(ArenaError::NoOwnedReference(owner.id));
        }
        if slot.scheduled_releases == slot.references {
            return Err(ArenaError::Invariant(
                "scheduled releases exceed live references",
            ));
        }
        Ok(())
    }

    /// Transfers one caller-owned reference into the iterative reclaim queue.
    /// Validation failure returns the owner so authority is never lost.
    #[allow(clippy::needless_pass_by_value)] // Moving the token is the ownership proof.
    pub fn release_later(&mut self, owner: OwnedArenaRef) -> Result<(), OwnerTransferError> {
        if let Err(error) = self.preflight_release(&owner) {
            return Err(OwnerTransferError { error, owner });
        }
        #[cfg(test)]
        if let Some(error) = self.release_transfer_test_error.take() {
            return Err(OwnerTransferError { error, owner });
        }
        let id = owner.id;
        let slot = self
            .slot_mut(id)
            .expect("release preflight keeps the owner live");
        slot.owned_references -= 1;
        if let Err(error) = self.schedule_release(id) {
            self.slot_mut(id)
                .expect("failed scheduling keeps the owner live")
                .owned_references += 1;
            return Err(OwnerTransferError { error, owner });
        }
        Ok(())
    }

    pub fn poll_reclaim(&mut self, fuel: usize) -> Result<ReclaimReceipt, ReclaimPollError> {
        let mut receipt = ReclaimReceipt::default();
        while receipt.reference_transitions < fuel && self.pending_head.is_some() {
            if let Err(error) = self.reclaim_one(&mut receipt) {
                receipt.pending_after = self.pending_releases;
                return Err(ReclaimPollError { error, receipt });
            }
        }
        receipt.pending_after = self.pending_releases;
        Ok(receipt)
    }

    #[must_use]
    pub fn contains(&self, id: ArenaId) -> bool {
        self.slot(id).is_ok()
    }

    pub fn payload(&self, id: ArenaId) -> Result<&[u8], ArenaError> {
        let node = self.slot(id)?.node.as_ref().expect("live slot has a node");
        Ok(&node.storage[..usize::from(node.payload_len)])
    }

    /// Returns the ownership edges encoded by an arena node.
    ///
    /// Payload-level IDs are deliberately insufficient for reachability. Higher
    /// level persistent structures use this accessor to traverse the same edges
    /// that reference counting will retire iteratively.
    pub fn children(
        &self,
        id: ArenaId,
    ) -> Result<[Option<ArenaId>; MAX_ARENA_CHILDREN], ArenaError> {
        let node = self.slot(id)?.node.as_ref().expect("live slot has a node");
        if usize::from(node.child_count) > MAX_ARENA_CHILDREN {
            return Err(ArenaError::Invariant(
                "fixed child accessor used on packed fanout node",
            ));
        }
        let mut output = [None; MAX_ARENA_CHILDREN];
        for (index, target) in output
            .iter_mut()
            .enumerate()
            .take(usize::from(node.child_count))
        {
            *target = Some(decode_child(node, index));
        }
        Ok(output)
    }

    pub fn packed_child_count(&self, id: ArenaId) -> Result<usize, ArenaError> {
        Ok(usize::from(
            self.slot(id)?
                .node
                .as_ref()
                .expect("live slot has a node")
                .child_count,
        ))
    }

    pub fn packed_child_at(&self, id: ArenaId, index: usize) -> Result<ArenaId, ArenaError> {
        let node = self.slot(id)?.node.as_ref().expect("live slot has a node");
        if index >= usize::from(node.child_count) {
            return Err(ArenaError::Invariant("packed child index out of range"));
        }
        Ok(decode_child(node, index))
    }

    #[must_use]
    pub fn metrics(&self) -> ArenaMetrics {
        ArenaMetrics {
            slots: self.slots.len(),
            slot_capacity: self.slots.actual_capacity(),
            slot_initialized_capacity: self.slots.initialized_capacity(),
            slot_storage_bytes: self.slots.storage_bytes(),
            slot_segments: self.slots.segments.len(),
            slot_directory_logical_segments: self.slots.max_segments,
            slot_directory_actual_capacity: self.slots.segments.capacity(),
            slot_hard_limit: self.slots.max_slots,
            slot_metadata_prior_entries_moved: self.slot_metadata_prior_entries_moved,
            maximum_slot_segment_entries_initialized: self.maximum_slot_segment_entries_initialized,
            live_storage_hard_limit: self.limits.max_live_storage_bytes,
            build_slots: self.builds.len(),
            build_slot_capacity: self.builds.capacity(),
            build_slot_hard_limit: self.limits.max_active_builds as usize,
            build_metadata_prior_entries_moved: self.build_metadata_prior_entries_moved,
            reusable_slots: self.reusable_slots,
            retired_slots: self.retired_slots,
            live_nodes: self.live_nodes,
            live_payload_bytes: self.live_payload_bytes,
            live_edge_bytes: self.live_edge_bytes,
            live_storage_bytes: self.live_storage_bytes,
            heap_page_allocations: self.live_nodes,
            pending_releases: self.pending_releases,
            queued_release_nodes: self.queued_release_nodes,
            high_water_live_nodes: self.high_water_live_nodes,
            high_water_payload_bytes: self.high_water_payload_bytes,
            high_water_storage_bytes: self.high_water_storage_bytes,
        }
    }

    fn validate_build_id(&self, id: ArenaBuildId) -> Result<&BuildSlot, ArenaBuildError> {
        if id.arena_nonce != self.arena_nonce {
            return Err(ArenaBuildError::WrongArena(id));
        }
        let Some(slot) = self.builds.get(id.index as usize) else {
            return Err(ArenaBuildError::StaleBuild(id));
        };
        if slot.generation != id.generation
            || matches!(
                slot.lifecycle,
                BuildSlotLifecycle::Vacant | BuildSlotLifecycle::Retired
            )
        {
            return Err(ArenaBuildError::StaleBuild(id));
        }
        Ok(slot)
    }

    fn validate_suspended_ticket(&self, ticket: &ArenaBuildTicket) -> Result<(), ArenaBuildError> {
        let slot = self.validate_build_id(ticket.id)?;
        match slot.lifecycle {
            BuildSlotLifecycle::Suspended => {
                if slot.lease_generation == ticket.lease_generation {
                    Ok(())
                } else {
                    Err(ArenaBuildError::StaleTicket(ticket.id))
                }
            }
            BuildSlotLifecycle::Resumed => Err(ArenaBuildError::ReplayedTicket(ticket.id)),
            BuildSlotLifecycle::Aborting => Err(ArenaBuildError::BuildAborting(ticket.id)),
            BuildSlotLifecycle::Vacant | BuildSlotLifecycle::Retired => {
                Err(ArenaBuildError::StaleBuild(ticket.id))
            }
        }
    }

    fn finish_build_slot(&mut self, id: ArenaBuildId) -> Result<(), ArenaBuildError> {
        self.validate_build_id(id)?;
        let slot = &mut self.builds[id.index as usize];
        if slot.journal.live_owners() != 0 {
            return Err(ArenaBuildError::Invariant(
                "completed build retains journal owners",
            ));
        }
        if slot.generation == u32::MAX {
            slot.lifecycle = BuildSlotLifecycle::Retired;
            slot.next_free = None;
        } else {
            slot.generation += 1;
            slot.lifecycle = BuildSlotLifecycle::Vacant;
            slot.next_free = self.build_free_head;
            self.build_free_head = Some(id.index);
        }
        Ok(())
    }

    fn take_free_or_append(&mut self) -> Result<(u32, bool, MetadataGrowthReceipt), ArenaError> {
        if let Some(index) = self.free_head {
            let slot = &mut self.slots[index as usize];
            self.free_head = slot.next_free.take();
            self.reusable_slots -= 1;
            return Ok((index, true, MetadataGrowthReceipt::default()));
        }
        let index = u32::try_from(self.slots.len()).map_err(|_| ArenaError::NodeIndexOverflow)?;
        let growth = self.slots.preflight_append()?;
        let appended = self.slots.append_preflighted();
        debug_assert_eq!(appended, index as usize);
        Ok((index, false, growth))
    }

    fn validate_live(&self, id: ArenaId) -> Result<(), ArenaError> {
        self.slot(id).map(|_| ())
    }

    fn slot(&self, id: ArenaId) -> Result<&Slot, ArenaError> {
        let Some(slot) = self.slots.get(id.index as usize) else {
            return Err(ArenaError::StaleId(id));
        };
        if slot.generation != id.generation || slot.node.is_none() || slot.references == 0 {
            return Err(ArenaError::StaleId(id));
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, id: ArenaId) -> Result<&mut Slot, ArenaError> {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return Err(ArenaError::StaleId(id));
        };
        if slot.generation != id.generation || slot.node.is_none() || slot.references == 0 {
            return Err(ArenaError::StaleId(id));
        }
        Ok(slot)
    }

    fn schedule_release(&mut self, id: ArenaId) -> Result<(), ArenaError> {
        let should_enqueue = {
            let slot = self.slot_mut(id)?;
            if slot.scheduled_releases == slot.references {
                return Err(ArenaError::Invariant(
                    "scheduled releases exceed live references",
                ));
            }
            slot.scheduled_releases += 1;
            !slot.queued
        };
        self.pending_releases += 1;
        if should_enqueue {
            self.enqueue(id.index);
        }
        Ok(())
    }

    fn enqueue(&mut self, index: u32) {
        {
            let slot = &mut self.slots[index as usize];
            debug_assert!(!slot.queued);
            slot.queued = true;
            slot.next_pending = None;
        }
        if let Some(tail) = self.pending_tail {
            self.slots[tail as usize].next_pending = Some(index);
        } else {
            self.pending_head = Some(index);
        }
        self.pending_tail = Some(index);
        self.queued_release_nodes += 1;
    }

    fn dequeue(&mut self) -> Option<u32> {
        let index = self.pending_head?;
        let next = self.slots[index as usize].next_pending.take();
        self.pending_head = next;
        if next.is_none() {
            self.pending_tail = None;
        }
        self.slots[index as usize].queued = false;
        self.queued_release_nodes -= 1;
        Some(index)
    }

    fn reclaim_one(&mut self, receipt: &mut ReclaimReceipt) -> Result<(), ArenaError> {
        let index = self
            .dequeue()
            .ok_or(ArenaError::Invariant("pending queue became empty"))?;
        let (id, remaining_releases, references, node) = {
            let slot = &mut self.slots[index as usize];
            let id = ArenaId {
                index,
                generation: slot.generation,
            };
            if slot.scheduled_releases == 0 || slot.references == 0 {
                return Err(ArenaError::Invariant("queued slot has no release"));
            }
            slot.scheduled_releases -= 1;
            slot.references -= 1;
            self.pending_releases -= 1;
            let node =
                (slot.references == 0).then(|| slot.node.take().expect("live slot has a node"));
            (id, slot.scheduled_releases, slot.references, node)
        };
        receipt.reference_transitions += 1;

        if remaining_releases > 0 {
            self.enqueue(index);
        }
        if references > 0 {
            return Ok(());
        }
        if remaining_releases != 0 {
            return Err(ArenaError::Invariant(
                "zero-reference slot retains scheduled releases",
            ));
        }
        let node = node.expect("zero-reference transition owns node");
        let payload_bytes = usize::from(node.payload_len);
        let edge_bytes = usize::from(node.child_count) * ENCODED_ARENA_ID_BYTES;
        let storage_bytes = node.storage.capacity();
        self.live_nodes -= 1;
        self.live_payload_bytes -= payload_bytes;
        self.live_edge_bytes -= edge_bytes;
        self.live_storage_bytes -= storage_bytes;
        receipt.nodes_reclaimed += 1;
        receipt.payload_bytes_reclaimed += payload_bytes;

        let generation_exhausted = id.generation == u32::MAX;
        {
            let slot = &mut self.slots[index as usize];
            debug_assert_eq!(slot.owned_references, 0);
            if generation_exhausted {
                slot.retired = true;
            } else {
                slot.generation += 1;
                slot.next_free = self.free_head;
                self.free_head = Some(index);
            }
        }
        if generation_exhausted {
            self.retired_slots += 1;
            receipt.slots_retired += 1;
        } else {
            self.reusable_slots += 1;
        }

        for child_index in 0..usize::from(node.child_count) {
            let child = decode_child(&node, child_index);
            self.schedule_release(child)?;
            receipt.child_releases_enqueued += 1;
        }
        Ok(())
    }
}

impl ArenaBuildSession<'_> {
    #[must_use]
    pub const fn id(&self) -> ArenaBuildId {
        self.id
    }

    #[must_use]
    pub fn arena(&self) -> &PageArena {
        self.arena
    }

    pub fn owner_id(&self, owner: &ArenaBuildOwner) -> Result<ArenaId, ArenaBuildError> {
        self.ensure_resumed()?;
        self.validate_owner_build(owner)?;
        self.journal()?
            .id(&owner.handle)
            .map_err(|OwnerJournalError::StaleHandle| ArenaBuildError::StaleOwnerHandle(self.id))
    }

    pub fn retain(&mut self, id: ArenaId) -> Result<ArenaBuildOwner, ArenaBuildError> {
        self.ensure_resumed()?;
        self.journal_mut()?.preflight_track()?;
        let owner = self.arena.retain(id)?;
        Ok(self.track_owner_preflighted(owner))
    }

    pub fn allocate(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(ArenaBuildOwner, AllocationReceipt), ArenaBuildError> {
        self.ensure_resumed()?;
        // Journal capacity is admitted before a new page owner exists. A
        // later page/budget failure may leave this one fixed segment allocated
        // for reuse, but changes no journal length, live-owner list, reference
        // count, arena slot, or candidate-visible state.
        let journal_growth = self.journal_mut()?.preflight_track()?;
        let allocation = self.arena.allocate(payload, children)?;
        let mut receipt = allocation.receipt;
        receipt.note_owner_journal_growth(journal_growth);
        Ok((self.track_owner_preflighted(allocation.owner), receipt))
    }

    pub fn allocate_packed(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(ArenaBuildOwner, AllocationReceipt), ArenaBuildError> {
        self.ensure_resumed()?;
        // As above, only metadata capacity can survive a later allocation
        // failure; logical journal and arena state remain unchanged.
        let journal_growth = self.journal_mut()?.preflight_track()?;
        let allocation = self.arena.allocate_packed(payload, children)?;
        let mut receipt = allocation.receipt;
        receipt.note_owner_journal_growth(journal_growth);
        Ok((self.track_owner_preflighted(allocation.owner), receipt))
    }

    /// Transfers one build owner into the arena's iterative release queue.
    #[allow(clippy::needless_pass_by_value)] // Moving the handle is the linear transfer.
    pub fn release(&mut self, owner: ArenaBuildOwner) -> Result<(), ArenaBuildError> {
        self.ensure_resumed()?;
        self.validate_owner_build(&owner)?;
        let removed = self
            .journal_mut()?
            .remove_for_transfer(owner.handle)
            .map_err(|OwnerJournalError::StaleHandle| ArenaBuildError::StaleOwnerHandle(self.id))?;
        let RemovedOwnerJournalEntry {
            owner: owned_reference,
            index,
            generation,
        } = removed;
        match self.arena.release_later(owned_reference) {
            Ok(()) => Ok(()),
            Err(failure) => {
                // Re-journal the returned owner in its exact former slot so a
                // failed transfer can never erase authority. The caller no
                // longer has a usable linear handle and must abort this build;
                // session Drop or `begin_abort` keeps cleanup strictly fuelled.
                self.arena.builds[self.id.index as usize]
                    .journal
                    .restore_removed(RemovedOwnerJournalEntry {
                        owner: failure.owner,
                        index,
                        generation,
                    });
                Err(failure.error.into())
            }
        }
    }

    pub fn live_owners(&self) -> Result<usize, ArenaBuildError> {
        Ok(self.journal()?.live_owners())
    }

    /// Yields the arena borrow and returns a fresh linear lease.
    pub fn suspend(mut self) -> Result<ArenaBuildTicket, ArenaBuildError> {
        self.ensure_resumed()?;
        let Some(next_lease) = self.lease_generation.checked_add(1) else {
            return Err(ArenaBuildError::LeaseGenerationExhausted(self.id));
        };
        let slot = &mut self.arena.builds[self.id.index as usize];
        slot.lease_generation = next_lease;
        slot.lifecycle = BuildSlotLifecycle::Suspended;
        self.finished = true;
        Ok(ArenaBuildTicket {
            id: self.id,
            lease_generation: next_lease,
        })
    }

    /// Begins cancellation without scheduling any owner in this call.
    pub fn begin_abort(mut self) -> Result<ArenaBuildId, ArenaBuildError> {
        self.ensure_resumed()?;
        self.arena.builds[self.id.index as usize].lifecycle = BuildSlotLifecycle::Aborting;
        self.finished = true;
        Ok(self.id)
    }

    /// Atomically publishes the build's sole remaining owner to its caller.
    ///
    /// Any failure leaves every arena reference in the journal and lets this
    /// session's constant-time `Drop` begin a fuelled abort.
    #[allow(clippy::needless_pass_by_value)] // Moving the handle is the commit gate.
    pub fn commit(mut self, root: ArenaBuildOwner) -> Result<OwnedArenaRef, ArenaBuildError> {
        self.ensure_resumed()?;
        self.validate_owner_build(&root)?;
        let actual = self.journal()?.live_owners();
        if actual != 1 {
            return Err(ArenaBuildError::ExpectedExactlyOneOwner {
                build: self.id,
                actual,
            });
        }
        let owner = self
            .journal_mut()?
            .remove(root.handle)
            .map_err(|OwnerJournalError::StaleHandle| ArenaBuildError::StaleOwnerHandle(self.id))?;
        self.arena.finish_build_slot(self.id)?;
        self.finished = true;
        Ok(owner)
    }

    fn ensure_resumed(&self) -> Result<(), ArenaBuildError> {
        let slot = self.arena.validate_build_id(self.id)?;
        if slot.lifecycle != BuildSlotLifecycle::Resumed
            || slot.lease_generation != self.lease_generation
        {
            return Err(ArenaBuildError::BuildNotResumed(self.id));
        }
        Ok(())
    }

    fn validate_owner_build(&self, owner: &ArenaBuildOwner) -> Result<(), ArenaBuildError> {
        if owner.build != self.id {
            return Err(ArenaBuildError::CrossBuildOwner {
                expected: self.id,
                actual: owner.build,
            });
        }
        Ok(())
    }

    fn journal(&self) -> Result<&OwnerJournal, ArenaBuildError> {
        self.ensure_resumed()?;
        Ok(&self.arena.builds[self.id.index as usize].journal)
    }

    fn journal_mut(&mut self) -> Result<&mut OwnerJournal, ArenaBuildError> {
        self.ensure_resumed()?;
        Ok(&mut self.arena.builds[self.id.index as usize].journal)
    }

    fn track_owner_preflighted(&mut self, owner: OwnedArenaRef) -> ArenaBuildOwner {
        let handle = self.arena.builds[self.id.index as usize]
            .journal
            .track_preflighted(owner);
        ArenaBuildOwner {
            build: self.id,
            handle,
        }
    }
}

impl Drop for ArenaBuildSession<'_> {
    fn drop(&mut self) {
        if self.finished || self.id.arena_nonce != self.arena.arena_nonce {
            return;
        }
        let Some(slot) = self.arena.builds.get_mut(self.id.index as usize) else {
            return;
        };
        if slot.generation == self.id.generation
            && slot.lease_generation == self.lease_generation
            && slot.lifecycle == BuildSlotLifecycle::Resumed
        {
            // Constant-time only: no owner is inspected or scheduled here.
            slot.lifecycle = BuildSlotLifecycle::Aborting;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ArenaOwnerHandle {
    index: usize,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerJournalError {
    StaleHandle,
}

#[derive(Debug)]
struct OwnerJournalSlot {
    owner: Option<OwnedArenaRef>,
    generation: u64,
    next_free: Option<usize>,
    previous_live: Option<usize>,
    next_live: Option<usize>,
}

/// One journal entry removed immediately before an external ownership
/// transfer. Keeping its exact slot identity permits failure restoration even
/// when the handle generation is exhausted and that slot cannot enter the
/// reusable free list.
#[derive(Debug)]
struct RemovedOwnerJournalEntry {
    owner: OwnedArenaRef,
    index: usize,
    generation: u64,
}

impl Default for OwnerJournalSlot {
    fn default() -> Self {
        Self {
            owner: None,
            generation: 1,
            next_free: None,
            previous_live: None,
            next_live: None,
        }
    }
}

/// Shared ownership journal used by both the legacy lexical transaction and
/// the production-shaped arena-owned resumable build.
///
/// Its intrusive live list makes removing one cancellation owner O(1), even
/// after many handle transfers leave holes in the segmented slot table. A
/// fixed top directory points to fallibly preflighted 64-descriptor blocks;
/// each descriptor owns one fixed 16-entry segment, so neither growth boundary
/// copies prior owner entries or prior segment descriptors.
#[derive(Debug)]
struct OwnerJournal {
    directory_blocks: [Option<Vec<Vec<OwnerJournalSlot>>>; MAX_OWNER_JOURNAL_DIRECTORY_BLOCKS],
    len: usize,
    max_slots: usize,
    allocated_segments: usize,
    allocated_directory_blocks: usize,
    free_head: Option<usize>,
    live_head: Option<usize>,
    live_owners: usize,
    maximum_live_owners: usize,
    prior_entries_moved: usize,
    prior_directory_descriptors_moved: usize,
    maximum_segment_entries_initialized: usize,
    maximum_directory_descriptors_preflighted: usize,
}

impl OwnerJournal {
    fn new(max_slots: usize) -> Self {
        debug_assert!((1..=MAX_OWNER_JOURNAL_SLOTS).contains(&max_slots));
        Self {
            directory_blocks: std::array::from_fn(|_| None),
            len: 0,
            max_slots,
            allocated_segments: 0,
            allocated_directory_blocks: 0,
            free_head: None,
            live_head: None,
            live_owners: 0,
            maximum_live_owners: 0,
            prior_entries_moved: 0,
            prior_directory_descriptors_moved: 0,
            maximum_segment_entries_initialized: 0,
            maximum_directory_descriptors_preflighted: 0,
        }
    }

    fn reset_high_water(&mut self) {
        debug_assert_eq!(self.live_owners, 0);
        self.maximum_live_owners = 0;
    }

    fn preflight_track(&mut self) -> Result<MetadataGrowthReceipt, ArenaError> {
        if self.free_head.is_some() {
            return Ok(MetadataGrowthReceipt::default());
        }
        if self.len == self.max_slots {
            return Err(ArenaError::OwnerJournalLimitReached {
                limit: self.max_slots,
            });
        }
        if !self.len.is_multiple_of(OWNER_JOURNAL_SEGMENT_SLOTS) {
            return Ok(MetadataGrowthReceipt::default());
        }

        let segment_index = self.len / OWNER_JOURNAL_SEGMENT_SLOTS;
        debug_assert_eq!(segment_index, self.allocated_segments);
        let directory_block_index = segment_index / OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
        let segment_in_block = segment_index % OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
        let mut segment = Vec::new();
        segment
            .try_reserve_exact(OWNER_JOURNAL_SEGMENT_SLOTS)
            .map_err(|_| ArenaError::AllocationFailed("owner-journal-segment"))?;
        for _ in 0..OWNER_JOURNAL_SEGMENT_SLOTS {
            segment.push(OwnerJournalSlot::default());
        }
        let segment_actual_capacity = segment.capacity();
        let mut directory_block_added = false;
        let mut directory_actual_capacity = 0;
        if self.directory_blocks[directory_block_index].is_none() {
            debug_assert_eq!(directory_block_index, self.allocated_directory_blocks);
            let mut directory_block = Vec::new();
            directory_block
                .try_reserve_exact(OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK)
                .map_err(|_| ArenaError::AllocationFailed("owner-journal-directory-block"))?;
            directory_actual_capacity = directory_block.capacity();
            self.directory_blocks[directory_block_index] = Some(directory_block);
            self.allocated_directory_blocks += 1;
            self.maximum_directory_descriptors_preflighted = self
                .maximum_directory_descriptors_preflighted
                .max(OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK);
            directory_block_added = true;
        }
        let directory_block = self.directory_blocks[directory_block_index]
            .as_mut()
            .expect("journal directory block was admitted");
        debug_assert_eq!(directory_block.len(), segment_in_block);
        let directory_pointer = directory_block.as_ptr();
        let directory_capacity = directory_block.capacity();
        directory_block.push(segment);
        debug_assert_eq!(directory_block.as_ptr(), directory_pointer);
        debug_assert_eq!(directory_block.capacity(), directory_capacity);
        self.allocated_segments += 1;
        self.maximum_segment_entries_initialized = self
            .maximum_segment_entries_initialized
            .max(OWNER_JOURNAL_SEGMENT_SLOTS);
        Ok(MetadataGrowthReceipt {
            segment_added: true,
            entries_initialized: OWNER_JOURNAL_SEGMENT_SLOTS,
            prior_entries_moved: 0,
            segment_actual_capacity,
            directory_block_added,
            directory_descriptors_preflighted: if directory_block_added {
                OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK
            } else {
                0
            },
            directory_actual_capacity,
            prior_directory_descriptors_moved: 0,
        })
    }

    fn track_preflighted(&mut self, owner: OwnedArenaRef) -> ArenaOwnerHandle {
        if let Some(index) = self.free_head {
            let (next_free, generation) = {
                let slot = self.slot_mut(index).expect("free journal slot exists");
                let next_free = slot.next_free.take();
                let generation = slot
                    .generation
                    .checked_add(1)
                    .expect("generation-exhausted journal slots are retired on removal");
                slot.generation = generation;
                slot.owner = Some(owner);
                (next_free, generation)
            };
            self.free_head = next_free;
            self.link_live(index);
            self.note_live_owner();
            return ArenaOwnerHandle { index, generation };
        }

        debug_assert!(self.len < self.max_slots);
        debug_assert!(self.len / OWNER_JOURNAL_SEGMENT_SLOTS < self.allocated_segments);
        let index = self.len;
        self.len += 1;
        self.slot_mut(index)
            .expect("preflighted journal slot")
            .owner = Some(owner);
        self.link_live(index);
        self.note_live_owner();
        ArenaOwnerHandle {
            index,
            generation: 1,
        }
    }

    fn id(&self, handle: &ArenaOwnerHandle) -> Result<ArenaId, OwnerJournalError> {
        let Some(slot) = self.slot(handle.index) else {
            return Err(OwnerJournalError::StaleHandle);
        };
        if slot.generation != handle.generation {
            return Err(OwnerJournalError::StaleHandle);
        }
        slot.owner
            .as_ref()
            .map(OwnedArenaRef::id)
            .ok_or(OwnerJournalError::StaleHandle)
    }

    #[allow(clippy::needless_pass_by_value)] // Moving the handle enforces one transfer.
    fn remove(&mut self, handle: ArenaOwnerHandle) -> Result<OwnedArenaRef, OwnerJournalError> {
        self.remove_for_transfer(handle)
            .map(|removed| removed.owner)
    }

    #[allow(clippy::needless_pass_by_value)] // Moving the handle enforces one transfer.
    fn remove_for_transfer(
        &mut self,
        handle: ArenaOwnerHandle,
    ) -> Result<RemovedOwnerJournalEntry, OwnerJournalError> {
        let ArenaOwnerHandle { index, generation } = handle;
        let Some(slot) = self.slot(index) else {
            return Err(OwnerJournalError::StaleHandle);
        };
        if slot.generation != generation || slot.owner.is_none() {
            return Err(OwnerJournalError::StaleHandle);
        }
        self.unlink_live(index);
        let free_head = self.free_head;
        let slot = self.slot_mut(index).expect("validated journal slot");
        let owner = slot.owner.take().ok_or(OwnerJournalError::StaleHandle)?;
        if slot.generation != u64::MAX {
            slot.next_free = free_head;
            self.free_head = Some(index);
        }
        self.live_owners -= 1;
        Ok(RemovedOwnerJournalEntry {
            owner,
            index,
            generation,
        })
    }

    fn pop_live_owner(&mut self) -> Result<Option<OwnedArenaRef>, OwnerJournalError> {
        self.pop_live_owner_for_transfer()
            .map(|removed| removed.map(|removed| removed.owner))
    }

    fn pop_live_owner_for_transfer(
        &mut self,
    ) -> Result<Option<RemovedOwnerJournalEntry>, OwnerJournalError> {
        let Some(index) = self.live_head else {
            return Ok(None);
        };
        let generation = self
            .slot(index)
            .expect("live journal head exists")
            .generation;
        self.remove_for_transfer(ArenaOwnerHandle { index, generation })
            .map(Some)
    }

    /// Restores one failed transfer without allocating or changing its handle
    /// generation. For reusable generations, removal put this exact slot at the
    /// free-list head; a terminal-generation slot was never linked there.
    fn restore_removed(&mut self, removed: RemovedOwnerJournalEntry) {
        let RemovedOwnerJournalEntry {
            owner,
            index,
            generation,
        } = removed;
        let slot = self.slot(index).expect("removed journal slot exists");
        debug_assert_eq!(slot.generation, generation);
        debug_assert!(slot.owner.is_none());

        if generation != u64::MAX {
            debug_assert_eq!(self.free_head, Some(index));
            let next_free = self
                .slot_mut(index)
                .expect("removed reusable journal slot exists")
                .next_free
                .take();
            self.free_head = next_free;
        }
        self.slot_mut(index)
            .expect("removed journal slot exists")
            .owner = Some(owner);
        self.link_live(index);
        self.note_live_owner();
    }

    fn link_live(&mut self, index: usize) {
        let former_head = self.live_head;
        {
            let slot = self.slot_mut(index).expect("journal slot exists");
            slot.previous_live = None;
            slot.next_live = former_head;
        }
        if let Some(former_head) = former_head {
            self.slot_mut(former_head)
                .expect("former live journal head exists")
                .previous_live = Some(index);
        }
        self.live_head = Some(index);
    }

    fn unlink_live(&mut self, index: usize) {
        let slot = self.slot_mut(index).expect("live journal slot exists");
        let previous = slot.previous_live.take();
        let next = slot.next_live.take();
        if let Some(previous) = previous {
            self.slot_mut(previous)
                .expect("previous live journal slot exists")
                .next_live = next;
        } else {
            self.live_head = next;
        }
        if let Some(next) = next {
            self.slot_mut(next)
                .expect("next live journal slot exists")
                .previous_live = previous;
        }
    }

    fn note_live_owner(&mut self) {
        self.live_owners += 1;
        self.maximum_live_owners = self.maximum_live_owners.max(self.live_owners);
    }

    const fn live_owners(&self) -> usize {
        self.live_owners
    }

    const fn maximum_live_owners(&self) -> usize {
        self.maximum_live_owners
    }

    fn slots(&self) -> usize {
        self.len
    }

    fn capacity(&self) -> usize {
        self.directory_blocks
            .iter()
            .flatten()
            .flat_map(|block| block.iter())
            .map(Vec::capacity)
            .sum()
    }

    const fn initialized_capacity(&self) -> usize {
        self.allocated_segments * OWNER_JOURNAL_SEGMENT_SLOTS
    }

    fn storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .directory_blocks
                .iter()
                .flatten()
                .map(|block| block.capacity() * std::mem::size_of::<Vec<OwnerJournalSlot>>())
                .sum::<usize>()
            + self
                .directory_blocks
                .iter()
                .flatten()
                .flat_map(|block| block.iter())
                .map(|segment| segment.capacity() * std::mem::size_of::<OwnerJournalSlot>())
                .sum::<usize>()
    }

    const fn segments(&self) -> usize {
        self.allocated_segments
    }

    const fn directory_blocks(&self) -> usize {
        self.allocated_directory_blocks
    }

    const fn hard_slot_limit(&self) -> usize {
        self.max_slots
    }

    const fn prior_entries_moved(&self) -> usize {
        self.prior_entries_moved
    }

    const fn prior_directory_descriptors_moved(&self) -> usize {
        self.prior_directory_descriptors_moved
    }

    const fn maximum_segment_entries_initialized(&self) -> usize {
        self.maximum_segment_entries_initialized
    }

    const fn maximum_directory_descriptors_preflighted(&self) -> usize {
        self.maximum_directory_descriptors_preflighted
    }

    fn slot(&self, index: usize) -> Option<&OwnerJournalSlot> {
        if index >= self.len {
            return None;
        }
        let segment = index / OWNER_JOURNAL_SEGMENT_SLOTS;
        let directory_block = segment / OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
        let segment_in_block = segment % OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
        let offset = index % OWNER_JOURNAL_SEGMENT_SLOTS;
        self.directory_blocks
            .get(directory_block)?
            .as_ref()?
            .get(segment_in_block)?
            .get(offset)
    }

    fn slot_mut(&mut self, index: usize) -> Option<&mut OwnerJournalSlot> {
        if index >= self.len {
            return None;
        }
        let segment = index / OWNER_JOURNAL_SEGMENT_SLOTS;
        let directory_block = segment / OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
        let segment_in_block = segment % OWNER_JOURNAL_SEGMENTS_PER_DIRECTORY_BLOCK;
        let offset = index % OWNER_JOURNAL_SEGMENT_SLOTS;
        self.directory_blocks
            .get_mut(directory_block)?
            .as_mut()?
            .get_mut(segment_in_block)?
            .get_mut(offset)
    }
}

/// Legacy lexical rollback boundary for non-yielding build paths.
///
/// This now uses the same ownership journal as [`ArenaBuildSession`], but its
/// `Drop` still drains every owner synchronously because it retains `&mut
/// PageArena` for its entire lifetime. Yielding production paths must use
/// [`PageArena::begin_build`] and fuelled abort polling instead.
#[derive(Debug)]
pub(crate) struct ArenaBuildTransaction<'a> {
    arena: &'a mut PageArena,
    journal: OwnerJournal,
}

impl<'a> ArenaBuildTransaction<'a> {
    pub(crate) fn new(arena: &'a mut PageArena) -> Self {
        let max_journal_slots = arena.limits.max_transaction_owner_slots;
        Self {
            arena,
            journal: OwnerJournal::new(max_journal_slots),
        }
    }

    pub(crate) fn arena(&self) -> &PageArena {
        self.arena
    }

    pub(crate) fn track(&mut self, owner: OwnedArenaRef) -> Result<ArenaOwnerHandle, ArenaError> {
        if let Err(error) = self.journal.preflight_track() {
            // This legacy adapter has already received an independently live
            // owner. Returning a bare error would leak authority, so saturation
            // performs exactly one bounded transfer into iterative reclaim. It
            // never scans the journal; Drop separately schedules tracked owners.
            self.arena
                .release_later(owner)
                .map_err(|transfer| transfer.error)?;
            return Err(error);
        }
        Ok(self.journal.track_preflighted(owner))
    }

    pub(crate) fn id(&self, handle: &ArenaOwnerHandle) -> ArenaId {
        self.journal
            .id(handle)
            .expect("live arena transaction handle")
    }

    pub(crate) fn retain(&mut self, id: ArenaId) -> Result<ArenaOwnerHandle, ArenaError> {
        self.journal.preflight_track()?;
        let owner = self.arena.retain(id)?;
        Ok(self.journal.track_preflighted(owner))
    }

    pub(crate) fn allocate(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(ArenaOwnerHandle, AllocationReceipt), ArenaError> {
        let journal_growth = self.journal.preflight_track()?;
        let allocation = self.arena.allocate(payload, children)?;
        let mut receipt = allocation.receipt;
        receipt.note_owner_journal_growth(journal_growth);
        Ok((self.journal.track_preflighted(allocation.owner), receipt))
    }

    pub(crate) fn allocate_packed(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(ArenaOwnerHandle, AllocationReceipt), ArenaError> {
        let journal_growth = self.journal.preflight_track()?;
        let allocation = self.arena.allocate_packed(payload, children)?;
        let mut receipt = allocation.receipt;
        receipt.note_owner_journal_growth(journal_growth);
        Ok((self.journal.track_preflighted(allocation.owner), receipt))
    }

    pub(crate) fn release(&mut self, handle: ArenaOwnerHandle) -> Result<(), ArenaError> {
        let owner = self.remove(handle);
        self.arena
            .release_later(owner)
            .map_err(|transfer| transfer.error)
    }

    pub(crate) fn take(&mut self, handle: ArenaOwnerHandle) -> OwnedArenaRef {
        self.remove(handle)
    }

    // Passing the handle by value is the compile-time single-transfer gate.
    #[allow(clippy::needless_pass_by_value)]
    fn remove(&mut self, handle: ArenaOwnerHandle) -> OwnedArenaRef {
        self.journal
            .remove(handle)
            .expect("arena owner transferred exactly once")
    }

    pub(crate) const fn live_owners(&self) -> usize {
        self.journal.live_owners()
    }

    pub(crate) const fn maximum_live_owners(&self) -> usize {
        self.journal.maximum_live_owners()
    }

    pub(crate) fn owner_journal_slots(&self) -> usize {
        self.journal.slots()
    }

    pub(crate) fn owner_journal_capacity(&self) -> usize {
        self.journal.capacity()
    }

    pub(crate) fn owner_journal_bytes(&self) -> usize {
        self.journal.storage_bytes()
    }
}

impl Drop for ArenaBuildTransaction<'_> {
    fn drop(&mut self) {
        while let Ok(Some(owner)) = self.journal.pop_live_owner() {
            // Failure here means PageArena already violated its ownership
            // invariant; there is no sound secondary recovery action.
            let _ = self.arena.release_later(owner);
        }
    }
}

#[cfg(test)]
mod resumable_build_ticket_tests {
    use super::{
        ArenaBuildError, ArenaBuildTicket, ArenaBuildTransaction, ArenaError, ArenaLimits,
        PageArena,
    };

    #[test]
    fn a_ticket_from_a_prior_suspension_lease_is_stale() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let stale_ticket = ArenaBuildTicket {
            id: ticket.id,
            lease_generation: ticket.lease_generation,
        };
        let session = arena.resume_build(ticket).unwrap();
        let fresh_ticket = session.suspend().unwrap();

        let error = arena.resume_build(stale_ticket).unwrap_err();
        assert_eq!(error.error, ArenaBuildError::StaleTicket(fresh_ticket.id()));
        let build = arena.begin_build_abort(fresh_ticket).unwrap();
        assert!(arena.poll_build_abort(build, 0).unwrap().complete);
    }

    #[test]
    fn legacy_track_saturation_releases_every_received_owner_once() {
        let limits = ArenaLimits::new(8, 1024, 1, 2).with_transaction_owner_slots(2);
        let mut arena = PageArena::try_with_limits(limits).unwrap();
        let first = arena.allocate(b"first", &[]).unwrap().owner;
        let second = arena.allocate(b"second", &[]).unwrap().owner;
        let third = arena.allocate(b"third", &[]).unwrap().owner;
        {
            let mut transaction = ArenaBuildTransaction::new(&mut arena);
            let _first = transaction.track(first).unwrap();
            let _second = transaction.track(second).unwrap();
            assert_eq!(
                transaction.track(third),
                Err(ArenaError::OwnerJournalLimitReached { limit: 2 })
            );
        }
        assert_eq!(arena.metrics().pending_releases, 3);
        assert_eq!(arena.poll_reclaim(3).unwrap().nodes_reclaimed, 3);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().pending_releases, 0);
    }

    #[test]
    fn failed_session_release_restores_the_owner_to_its_just_freed_journal_slot() {
        const INJECTED: &str = "injected build-owner transfer failure";

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut session = arena.resume_build(ticket).unwrap();
        let (mut owner, _) = session.allocate(b"retained", &[]).unwrap();
        let owner_id = session.owner_id(&owner).unwrap();
        let original_journal_slot = owner.handle.index;
        // Terminal-generation journal slots deliberately never enter the free
        // list after removal. Restoration must still return authority to this
        // exact slot without attempting to increment its generation.
        owner.handle.generation = u64::MAX;
        session.arena.builds[build.index as usize]
            .journal
            .slot_mut(original_journal_slot)
            .expect("live journal slot")
            .generation = u64::MAX;
        assert_eq!(session.live_owners().unwrap(), 1);

        session.arena.release_transfer_test_error = Some(ArenaError::Invariant(INJECTED));
        assert_eq!(
            session.release(owner),
            Err(ArenaBuildError::Arena(ArenaError::Invariant(INJECTED)))
        );

        let journal = &session.arena.builds[build.index as usize].journal;
        assert_eq!(journal.live_owners(), 1);
        assert_eq!(journal.live_head, Some(original_journal_slot));
        assert_eq!(
            journal
                .slot(original_journal_slot)
                .expect("restored journal slot")
                .generation,
            u64::MAX
        );
        assert_eq!(session.arena.metrics().pending_releases, 0);
        assert_eq!(session.arena.payload(owner_id).unwrap(), b"retained");

        let build = session.begin_abort().unwrap();
        let zero = arena.poll_build_abort(build, 0).unwrap();
        assert_eq!(zero.owners_scheduled, 0);
        assert_eq!(zero.owners_remaining, 1);
        assert!(!zero.complete);
        let complete = arena.poll_build_abort(build, 1).unwrap();
        assert_eq!(complete.owners_scheduled, 1);
        assert_eq!(complete.owners_remaining, 0);
        assert!(complete.complete);
        assert_eq!(arena.poll_reclaim(1).unwrap().nodes_reclaimed, 1);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn failed_abort_transfer_rejournals_the_owner_and_later_polls_remain_fuel_bounded() {
        const INJECTED: &str = "injected abort-owner transfer failure";
        const OWNERS: usize = 3;

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut session = arena.resume_build(ticket).unwrap();
        for index in 0_u8..u8::try_from(OWNERS).unwrap() {
            let _owner = session.allocate(&[index], &[]).unwrap().0;
        }
        let failed_slot = session.arena.builds[build.index as usize]
            .journal
            .live_head
            .expect("nonempty abort journal");
        let failed_generation = session.arena.builds[build.index as usize]
            .journal
            .slot(failed_slot)
            .expect("live abort slot")
            .generation;
        let build = session.begin_abort().unwrap();

        arena.release_transfer_test_error = Some(ArenaError::Invariant(INJECTED));
        assert_eq!(
            arena.poll_build_abort(build, 2),
            Err(ArenaBuildError::Arena(ArenaError::Invariant(INJECTED)))
        );
        assert_eq!(
            arena.build_lifecycle(build).unwrap(),
            super::ArenaBuildLifecycle::Aborting
        );
        let journal = &arena.builds[build.index as usize].journal;
        assert_eq!(journal.live_owners(), OWNERS);
        assert_eq!(journal.live_head, Some(failed_slot));
        assert_eq!(
            journal
                .slot(failed_slot)
                .expect("re-journalled abort slot")
                .generation,
            failed_generation
        );
        assert_eq!(arena.metrics().pending_releases, 0);

        let zero = arena.poll_build_abort(build, 0).unwrap();
        assert_eq!(zero.owners_scheduled, 0);
        assert_eq!(zero.owners_remaining, OWNERS);
        assert!(!zero.complete);
        let first = arena.poll_build_abort(build, 1).unwrap();
        assert_eq!(first.owners_scheduled, 1);
        assert_eq!(first.owners_remaining, OWNERS - 1);
        assert!(!first.complete);
        let final_poll = arena.poll_build_abort(build, OWNERS - 1).unwrap();
        assert_eq!(final_poll.owners_scheduled, OWNERS - 1);
        assert_eq!(final_poll.owners_remaining, 0);
        assert!(final_poll.complete);
        assert_eq!(arena.metrics().pending_releases, OWNERS);
        assert_eq!(arena.poll_reclaim(OWNERS).unwrap().nodes_reclaimed, OWNERS);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn generation_exhausted_page_slot_retires_without_affecting_other_segments() {
        let limits = ArenaLimits::new(2, 1024, 1, 2);
        let mut arena = PageArena::try_with_limits(limits).unwrap();
        let mut owner = arena.allocate(b"retire", &[]).unwrap().owner;
        arena.slots[0].generation = u32::MAX;
        owner.id.generation = u32::MAX;
        arena.release_later(owner).unwrap();
        let receipt = arena.poll_reclaim(1).unwrap();
        assert_eq!(receipt.slots_retired, 1);
        assert_eq!(arena.metrics().retired_slots, 1);

        let replacement = arena.allocate(b"next", &[]).unwrap();
        assert_eq!(replacement.owner.id().index, 1);
        assert!(!replacement.receipt.slot_metadata_segment_added);
    }
}
