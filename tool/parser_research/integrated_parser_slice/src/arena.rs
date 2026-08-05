//! Safe, fixed-slab ownership for persistent parser pages.
//!
//! Rust reference-counted trees make construction persistent, but dropping the
//! last root can still recurse through an entire tree in one worker turn. This
//! arena stores child links as integer IDs and applies reference decrements only
//! through [`PageArena::poll_reclaim`]. Pending work is an intrusive FIFO: each
//! live slot contributes its own queue link, so child expansion cannot exhaust a
//! separate fixed stack. Payload destruction is iterative and charged one
//! bounded page at a time.

use std::fmt;
use std::mem::size_of;

/// Maximum payload released by one reclaim transition.
pub const ARENA_PAGE_BYTES: usize = 4 * 1024;
/// Slots initialized by one bounded arena growth operation.
pub const ARENA_SLAB_SLOTS: usize = 256;
/// Fan-out of each fixed arena slab-directory block.
///
/// Three byte-indexed levels address every possible `u32` slot while making
/// directory growth a constant-size, exactly chargeable operation. The root
/// level lives inline in [`PageArena`]; lower levels are allocated lazily.
const SLAB_DIRECTORY_WIDTH: usize = 256;
/// Persistent sequence nodes are binary in the commitment slice.
pub const MAX_ARENA_CHILDREN: usize = 2;
/// Historical stress width retained for regression tests.
///
/// The reclaim queue is no longer capped at this width. It is intrusive in
/// already allocated slots, so it needs no document-sized side allocation.
pub const MAX_RECLAIM_FRONTIER: usize = 256;

/// Generation-checked identity of one arena node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaId {
    pub index: u32,
    pub generation: u32,
}

/// Arena operation failure. Stale IDs fail closed instead of aliasing reused slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaError {
    PayloadTooLarge(usize),
    TooManyChildren(usize),
    StaleId(ArenaId),
    NoOwnedReference(ArenaId),
    InvalidReferenceState(ArenaId),
    ReferenceCountOverflow(ArenaId),
    GenerationExhausted(ArenaId),
    NodeIndexOverflow,
    MutationEpochExhausted,
    StaleAllocationPreview,
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
            Self::StaleId(id) => write!(formatter, "stale arena ID {id:?}"),
            Self::NoOwnedReference(id) => {
                write!(
                    formatter,
                    "arena ID {id:?} has no unscheduled owned reference"
                )
            }
            Self::InvalidReferenceState(id) => {
                write!(
                    formatter,
                    "arena ID {id:?} has inconsistent reference ownership"
                )
            }
            Self::ReferenceCountOverflow(id) => {
                write!(formatter, "reference count overflow for {id:?}")
            }
            Self::GenerationExhausted(id) => {
                write!(formatter, "arena generation exhausted for {id:?}")
            }
            Self::NodeIndexOverflow => formatter.write_str("arena node index exceeds u32"),
            Self::MutationEpochExhausted => {
                formatter.write_str("arena mutation epoch is exhausted")
            }
            Self::StaleAllocationPreview => {
                formatter.write_str("arena allocation preview is stale or mismatched")
            }
        }
    }
}

impl std::error::Error for ArenaError {}

#[derive(Debug)]
enum ArenaPayload {
    Copied(Box<[u8]>),
    AdoptedPage {
        allocation: Box<[u8; ARENA_PAGE_BYTES]>,
        used_len: usize,
    },
}

impl ArenaPayload {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Copied(payload) => payload,
            Self::AdoptedPage {
                allocation,
                used_len,
            } => &allocation[..*used_len],
        }
    }

    const fn allocated_bytes(&self) -> usize {
        match self {
            Self::Copied(payload) => payload.len(),
            Self::AdoptedPage { .. } => ARENA_PAGE_BYTES,
        }
    }
}

#[derive(Debug)]
struct ArenaNode {
    payload: ArenaPayload,
    children: [Option<ArenaId>; MAX_ARENA_CHILDREN],
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

type Slab = [Slot; ARENA_SLAB_SLOTS];

#[derive(Debug)]
struct SlabLeafDirectory {
    slabs: [Option<Box<Slab>>; SLAB_DIRECTORY_WIDTH],
}

impl Default for SlabLeafDirectory {
    fn default() -> Self {
        Self {
            slabs: std::array::from_fn(|_| None),
        }
    }
}

#[derive(Debug)]
struct SlabBranchDirectory {
    leaves: [Option<Box<SlabLeafDirectory>>; SLAB_DIRECTORY_WIDTH],
}

impl Default for SlabBranchDirectory {
    fn default() -> Self {
        Self {
            leaves: std::array::from_fn(|_| None),
        }
    }
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

/// Work and memory charged to one node allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaAllocationReceipt {
    pub id: ArenaId,
    pub payload_bytes_copied: usize,
    /// Existing fixed-page allocation bytes moved into the arena without a
    /// second allocation or payload copy.
    pub payload_allocation_bytes_adopted: usize,
    pub child_references_added: usize,
    pub child_owned_references_transferred: usize,
    pub slabs_added: usize,
    /// Slots initialized by newly allocated slabs.
    pub slots_initialized: usize,
    pub slot_bytes_initialized: usize,
    /// Lazily allocated fixed directory blocks. The inline root directory is
    /// construction-time baseline state and is never charged to a live slice.
    pub directory_blocks_added: usize,
    pub directory_entries_initialized: usize,
    pub directory_bytes_initialized: usize,
    allocation_epoch: u64,
    transfer_owned_children: bool,
    adopt_owned_payload: bool,
    bound_payload_bytes: usize,
    bound_children: [Option<ArenaId>; MAX_ARENA_CHILDREN],
}

/// Exact logical work required by the next arena allocation before mutation.
///
/// This is a preflight receipt rather than an allocator-size claim: payload,
/// slot, and directory bytes are the precise initialized Rust object bytes.
/// General allocator metadata and size-class slack remain outside this model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaAllocationPreview {
    pub payload_bytes_copied: usize,
    pub payload_allocation_bytes_adopted: usize,
    pub child_references_added: usize,
    pub child_owned_references_transferred: usize,
    pub slabs_added: usize,
    pub slots_initialized: usize,
    pub slot_bytes_initialized: usize,
    pub directory_blocks_added: usize,
    pub directory_entries_initialized: usize,
    pub directory_bytes_initialized: usize,
    allocation_epoch: u64,
    transfer_owned_children: bool,
    adopt_owned_payload: bool,
    bound_payload_bytes: usize,
    bound_children: [Option<ArenaId>; MAX_ARENA_CHILDREN],
}

impl ArenaAllocationReceipt {
    /// The allocation's ID-free work dimensions, suitable for comparing a
    /// preflight capability with the actual transition.
    #[must_use]
    pub const fn preview(self) -> ArenaAllocationPreview {
        ArenaAllocationPreview {
            payload_bytes_copied: self.payload_bytes_copied,
            payload_allocation_bytes_adopted: self.payload_allocation_bytes_adopted,
            child_references_added: self.child_references_added,
            child_owned_references_transferred: self.child_owned_references_transferred,
            slabs_added: self.slabs_added,
            slots_initialized: self.slots_initialized,
            slot_bytes_initialized: self.slot_bytes_initialized,
            directory_blocks_added: self.directory_blocks_added,
            directory_entries_initialized: self.directory_entries_initialized,
            directory_bytes_initialized: self.directory_bytes_initialized,
            allocation_epoch: self.allocation_epoch,
            transfer_owned_children: self.transfer_owned_children,
            adopt_owned_payload: self.adopt_owned_payload,
            bound_payload_bytes: self.bound_payload_bytes,
            bound_children: self.bound_children,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SlabGrowth {
    slabs_added: usize,
    slots_initialized: usize,
    slot_bytes_initialized: usize,
    directory_blocks_added: usize,
    directory_entries_initialized: usize,
    directory_bytes_initialized: usize,
}

const fn preview_growth(preview: ArenaAllocationPreview) -> SlabGrowth {
    SlabGrowth {
        slabs_added: preview.slabs_added,
        slots_initialized: preview.slots_initialized,
        slot_bytes_initialized: preview.slot_bytes_initialized,
        directory_blocks_added: preview.directory_blocks_added,
        directory_entries_initialized: preview.directory_entries_initialized,
        directory_bytes_initialized: preview.directory_bytes_initialized,
    }
}

/// Work charged to one bounded reclamation poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReclaimReceipt {
    pub reference_transitions: usize,
    pub nodes_reclaimed: usize,
    pub payload_bytes_reclaimed: usize,
    pub child_releases_enqueued: usize,
    pub slots_retired: usize,
    pub pending_after: usize,
}

/// A reclaim failure that preserves every unit of work completed by the poll.
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

impl std::error::Error for ReclaimPollError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Current and high-water ownership accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaMetrics {
    pub slabs: usize,
    pub total_slots: usize,
    pub live_nodes: usize,
    pub live_payload_bytes: usize,
    pub high_water_live_nodes: usize,
    pub high_water_payload_bytes: usize,
    pub pending_releases: usize,
    pub queued_release_nodes: usize,
    pub retired_slots: usize,
}

/// A safe manual-reference arena with nonrecursive, fuelled destruction.
#[derive(Debug)]
pub struct PageArena {
    slab_roots: [Option<Box<SlabBranchDirectory>>; SLAB_DIRECTORY_WIDTH],
    slab_count: usize,
    free_head: Option<u32>,
    pending_head: Option<u32>,
    pending_tail: Option<u32>,
    pending_transitions: usize,
    queued_release_nodes: usize,
    live_nodes: usize,
    live_payload_bytes: usize,
    high_water_live_nodes: usize,
    high_water_payload_bytes: usize,
    retired_slots: usize,
    generation_ceiling: u32,
    mutation_epoch: u64,
}

impl Default for PageArena {
    fn default() -> Self {
        Self::new()
    }
}

impl PageArena {
    /// Creates an empty arena with the full 32-bit generation space.
    #[must_use]
    pub fn new() -> Self {
        Self::with_generation_ceiling(u32::MAX)
    }

    /// Creates an arena with a smaller generation ceiling for exhaustion tests.
    ///
    /// Production callers should use [`Self::new`]. A slot reaching the ceiling
    /// is permanently retired rather than wrapped or reused.
    ///
    /// # Panics
    ///
    /// Panics when `generation_ceiling` is zero because zero is reserved for
    /// invalid IDs.
    #[doc(hidden)]
    #[must_use]
    pub fn with_generation_ceiling_for_tests(generation_ceiling: u32) -> Self {
        assert!(generation_ceiling > 0);
        Self::with_generation_ceiling(generation_ceiling)
    }

    fn with_generation_ceiling(generation_ceiling: u32) -> Self {
        Self {
            slab_roots: std::array::from_fn(|_| None),
            slab_count: 0,
            free_head: None,
            pending_head: None,
            pending_tail: None,
            pending_transitions: 0,
            queued_release_nodes: 0,
            live_nodes: 0,
            live_payload_bytes: 0,
            high_water_live_nodes: 0,
            high_water_payload_bytes: 0,
            retired_slots: 0,
            generation_ceiling,
            mutation_epoch: 1,
        }
    }

    /// Allocates one bounded node and returns one owned reference.
    ///
    /// Each child ID adds one persistent edge/reference. The caller continues
    /// to own any child references it passed and must release those separately.
    ///
    /// # Panics
    ///
    /// Panics only if the arena's internal free-list invariant is corrupt.
    ///
    /// # Errors
    ///
    /// Rejects oversized payloads, more than two children, stale child IDs,
    /// reference-count overflow, and arenas whose 32-bit slot space is exhausted.
    pub fn allocate(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        self.allocate_inner(payload, children, false)
    }

    /// Allocates one node while atomically moving one caller-owned reference
    /// per child into the corresponding parent edge.
    ///
    /// Unlike [`Self::allocate`], this does not increment child reference
    /// counts and requires no follow-up reclaim transition. It is the intended
    /// operation for replacing an evolving job root with a new immutable
    /// parent. Duplicate child IDs require the same number of owned references.
    ///
    /// # Errors
    ///
    /// In addition to normal allocation failures, rejects a child without
    /// enough unscheduled caller-owned references. Validation is atomic: no
    /// child ownership or arena slot changes on rejection.
    pub fn allocate_transferring_owned_children(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        self.allocate_inner(payload, children, true)
    }

    /// Consumes a previously issued bound preview for a normal allocation.
    ///
    /// Any intervening arena mutation, changed payload size, changed child
    /// list, or changed ownership mode is rejected in release builds before
    /// physical state changes.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError::StaleAllocationPreview`] when the bound preview
    /// no longer describes the arena and request exactly. It also propagates
    /// the allocation validation errors documented by [`Self::allocate`].
    pub fn allocate_preflighted(
        &mut self,
        preview: ArenaAllocationPreview,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        self.allocate_preflighted_inner(preview, payload, children, false)
    }

    /// Consumes a bound preview for an ownership-transferring allocation.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError::StaleAllocationPreview`] when the bound preview
    /// no longer describes the arena and request exactly. It also propagates
    /// the validation errors documented by
    /// [`Self::allocate_transferring_owned_children`].
    pub fn allocate_transferring_owned_children_preflighted(
        &mut self,
        preview: ArenaAllocationPreview,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        self.allocate_preflighted_inner(preview, payload, children, true)
    }

    /// Consumes an existing fixed 4 KiB page allocation while atomically
    /// moving caller-owned child references into its arena node.
    ///
    /// The page allocation itself is neither copied nor allocated here. Its
    /// original allocation work must already have been charged by the producer;
    /// this receipt charges only arena node/slab/directory work and the child
    /// ownership transfer.
    ///
    /// # Errors
    ///
    /// Rejects a stale preview, a `used_len` above 4 KiB, stale children, or
    /// invalid child ownership. The supplied page is dropped on error.
    pub fn adopt_owned_page_transferring_owned_children_preflighted(
        &mut self,
        preview: ArenaAllocationPreview,
        allocation: Box<[u8; ARENA_PAGE_BYTES]>,
        used_len: usize,
        children: &[ArenaId],
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        let current = self.preview_allocation_inner(used_len, children, true, true)?;
        if preview != current {
            return Err(ArenaError::StaleAllocationPreview);
        }
        self.commit_allocation(
            preview,
            ArenaPayload::AdoptedPage {
                allocation,
                used_len,
            },
            children,
        )
    }

    fn allocate_preflighted_inner(
        &mut self,
        preview: ArenaAllocationPreview,
        payload: &[u8],
        children: &[ArenaId],
        transfer_owned_children: bool,
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        let current =
            self.preview_allocation_inner(payload.len(), children, transfer_owned_children, false)?;
        if preview != current {
            return Err(ArenaError::StaleAllocationPreview);
        }
        self.commit_allocation(
            preview,
            ArenaPayload::Copied(payload.to_vec().into_boxed_slice()),
            children,
        )
    }

    /// Preflights one normal allocation without changing arena state.
    ///
    /// The returned receipt includes any constant-size slab and lazy directory
    /// initialization that the next allocation would perform.
    ///
    /// # Errors
    ///
    /// Applies the same payload, child, reference-count, and index validation
    /// as [`Self::allocate`].
    pub fn preview_allocate(
        &self,
        payload_bytes: usize,
        children: &[ArenaId],
    ) -> Result<ArenaAllocationPreview, ArenaError> {
        self.preview_allocation_inner(payload_bytes, children, false, false)
    }

    /// Preflights one ownership-transferring allocation without mutation.
    ///
    /// # Errors
    ///
    /// Applies the same validation as
    /// [`Self::allocate_transferring_owned_children`].
    pub fn preview_allocate_transferring_owned_children(
        &self,
        payload_bytes: usize,
        children: &[ArenaId],
    ) -> Result<ArenaAllocationPreview, ArenaError> {
        self.preview_allocation_inner(payload_bytes, children, true, false)
    }

    /// Preflights adoption of an existing fixed 4 KiB page allocation while
    /// transferring caller-owned child references.
    ///
    /// The returned preview binds the exact used byte length and adoption mode
    /// so it cannot be replayed as a copying allocation.
    ///
    /// # Errors
    ///
    /// Applies the same payload and child validation as the consuming method.
    pub fn preview_adopt_owned_page_transferring_owned_children(
        &self,
        used_len: usize,
        children: &[ArenaId],
    ) -> Result<ArenaAllocationPreview, ArenaError> {
        self.preview_allocation_inner(used_len, children, true, true)
    }

    fn allocate_inner(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
        transfer_owned_children: bool,
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        let preview =
            self.preview_allocation_inner(payload.len(), children, transfer_owned_children, false)?;
        self.commit_allocation(
            preview,
            ArenaPayload::Copied(payload.to_vec().into_boxed_slice()),
            children,
        )
    }

    fn commit_allocation(
        &mut self,
        preview: ArenaAllocationPreview,
        payload: ArenaPayload,
        children: &[ArenaId],
    ) -> Result<ArenaAllocationReceipt, ArenaError> {
        debug_assert_eq!(payload.as_bytes().len(), preview.bound_payload_bytes);
        debug_assert_eq!(
            matches!(&payload, ArenaPayload::AdoptedPage { .. }),
            preview.adopt_owned_payload
        );
        self.advance_mutation_epoch()?;
        if preview.slabs_added != 0 {
            let actual_growth = self.add_slab()?;
            debug_assert_eq!(actual_growth, preview_growth(preview));
        }

        for &child in children {
            let slot = self.slot_mut(child)?;
            if preview.transfer_owned_children {
                slot.owned_references -= 1;
            } else {
                slot.references += 1;
            }
        }

        let index = self.free_head.expect("a slab provides a free slot");
        let next_free = self.slot_by_index(index).next_free;
        self.free_head = next_free;
        let generation = self.slot_by_index(index).generation;
        let id = ArenaId { index, generation };
        let mut child_array = [None; MAX_ARENA_CHILDREN];
        for (target, child) in child_array.iter_mut().zip(children.iter().copied()) {
            *target = Some(child);
        }
        let slot = self.slot_by_index_mut(index);
        debug_assert!(
            slot.node.is_none()
                && slot.references == 0
                && slot.owned_references == 0
                && slot.scheduled_releases == 0
                && !slot.queued
                && !slot.retired
        );
        slot.node = Some(ArenaNode {
            payload,
            children: child_array,
        });
        slot.references = 1;
        slot.owned_references = 1;
        slot.scheduled_releases = 0;
        slot.next_free = None;
        slot.next_pending = None;
        slot.queued = false;

        self.live_nodes += 1;
        let payload_allocated_bytes = self
            .slot(id)?
            .node
            .as_ref()
            .expect("new allocation has a node")
            .payload
            .allocated_bytes();
        self.live_payload_bytes += payload_allocated_bytes;
        self.high_water_live_nodes = self.high_water_live_nodes.max(self.live_nodes);
        self.high_water_payload_bytes = self.high_water_payload_bytes.max(self.live_payload_bytes);

        Ok(ArenaAllocationReceipt {
            id,
            payload_bytes_copied: preview.payload_bytes_copied,
            payload_allocation_bytes_adopted: preview.payload_allocation_bytes_adopted,
            child_references_added: preview.child_references_added,
            child_owned_references_transferred: preview.child_owned_references_transferred,
            slabs_added: preview.slabs_added,
            slots_initialized: preview.slots_initialized,
            slot_bytes_initialized: preview.slot_bytes_initialized,
            directory_blocks_added: preview.directory_blocks_added,
            directory_entries_initialized: preview.directory_entries_initialized,
            directory_bytes_initialized: preview.directory_bytes_initialized,
            allocation_epoch: preview.allocation_epoch,
            transfer_owned_children: preview.transfer_owned_children,
            adopt_owned_payload: preview.adopt_owned_payload,
            bound_payload_bytes: preview.bound_payload_bytes,
            bound_children: preview.bound_children,
        })
    }

    fn preview_allocation_inner(
        &self,
        payload_bytes: usize,
        children: &[ArenaId],
        transfer_owned_children: bool,
        adopt_owned_payload: bool,
    ) -> Result<ArenaAllocationPreview, ArenaError> {
        if payload_bytes > ARENA_PAGE_BYTES {
            return Err(ArenaError::PayloadTooLarge(payload_bytes));
        }
        if children.len() > MAX_ARENA_CHILDREN {
            return Err(ArenaError::TooManyChildren(children.len()));
        }
        if transfer_owned_children {
            self.validate_child_transfers(children)?;
        } else {
            self.validate_child_increments(children)?;
        }
        let growth = if self.free_head.is_none() {
            self.preview_slab_growth()?
        } else {
            SlabGrowth::default()
        };
        Ok(ArenaAllocationPreview {
            payload_bytes_copied: if adopt_owned_payload {
                0
            } else {
                payload_bytes
            },
            payload_allocation_bytes_adopted: if adopt_owned_payload {
                ARENA_PAGE_BYTES
            } else {
                0
            },
            child_references_added: if transfer_owned_children {
                0
            } else {
                children.len()
            },
            child_owned_references_transferred: if transfer_owned_children {
                children.len()
            } else {
                0
            },
            slabs_added: growth.slabs_added,
            slots_initialized: growth.slots_initialized,
            slot_bytes_initialized: growth.slot_bytes_initialized,
            directory_blocks_added: growth.directory_blocks_added,
            directory_entries_initialized: growth.directory_entries_initialized,
            directory_bytes_initialized: growth.directory_bytes_initialized,
            allocation_epoch: self.mutation_epoch,
            transfer_owned_children,
            adopt_owned_payload,
            bound_payload_bytes: payload_bytes,
            bound_children: bound_children(children),
        })
    }

    /// Adds another owned reference to a live node.
    ///
    /// # Errors
    ///
    /// Rejects stale IDs and reference-count overflow.
    pub fn retain(&mut self, id: ArenaId) -> Result<(), ArenaError> {
        let (references, owned_references) = {
            let slot = self.slot(id)?;
            let references = slot
                .references
                .checked_add(1)
                .ok_or(ArenaError::ReferenceCountOverflow(id))?;
            let owned_references = slot
                .owned_references
                .checked_add(1)
                .ok_or(ArenaError::ReferenceCountOverflow(id))?;
            (references, owned_references)
        };
        self.advance_mutation_epoch()?;
        let slot = self.slot_mut(id)?;
        slot.references = references;
        slot.owned_references = owned_references;
        Ok(())
    }

    /// Immediately discards one unlinked, solely owned leaf node.
    ///
    /// This narrow rollback primitive is nonrecursive and independent of the
    /// reclaim FIFO: the node must have exactly one caller-owned reference, no
    /// parent/child edges, and no scheduled release. It is intended for an
    /// anchor allocation whose subsequent scheduler adoption failed.
    ///
    /// A slot at the generation ceiling is retired and reported in the receipt
    /// rather than reused. No generation can wrap or alias.
    ///
    /// # Errors
    ///
    /// Rejects stale IDs and any node that is shared, linked, or queued. On
    /// error the arena is unchanged.
    ///
    /// # Panics
    ///
    /// Panics only if a slot loses its validated node before removal, which
    /// would indicate internal arena corruption.
    pub fn discard_unlinked_owned(&mut self, id: ArenaId) -> Result<ReclaimReceipt, ArenaError> {
        {
            let slot = self.slot(id)?;
            let node = slot.node.as_ref().ok_or(ArenaError::StaleId(id))?;
            if slot.references != 1
                || slot.owned_references != 1
                || slot.scheduled_releases != 0
                || slot.queued
                || node.children.iter().any(Option::is_some)
            {
                return Err(ArenaError::InvalidReferenceState(id));
            }
        }

        self.advance_mutation_epoch()?;
        let generation_exhausted = id.generation == self.generation_ceiling;
        let free_head = self.free_head;
        let node = {
            let slot = self.slot_by_index_mut(id.index);
            slot.references = 0;
            slot.owned_references = 0;
            slot.scheduled_releases = 0;
            slot.next_pending = None;
            if generation_exhausted {
                slot.retired = true;
                slot.next_free = None;
            } else {
                slot.generation += 1;
                slot.next_free = free_head;
            }
            slot.node.take().expect("validated live node exists")
        };
        if generation_exhausted {
            self.retired_slots += 1;
        } else {
            self.free_head = Some(id.index);
        }
        self.live_nodes -= 1;
        self.live_payload_bytes -= node.payload.allocated_bytes();
        Ok(ReclaimReceipt {
            reference_transitions: 1,
            nodes_reclaimed: 1,
            payload_bytes_reclaimed: node.payload.allocated_bytes(),
            slots_retired: usize::from(generation_exhausted),
            pending_after: self.pending_transitions,
            ..ReclaimReceipt::default()
        })
    }

    /// Schedules consumption of one owned reference.
    ///
    /// The method never walks child nodes or frees payload memory.
    ///
    /// The caller-owned reference is reserved immediately. Calling this method
    /// again without another [`Self::retain`] is rejected before queue state is
    /// changed. Parent edges are not caller-owned references and cannot be
    /// consumed through this method.
    ///
    /// # Errors
    ///
    /// Rejects stale IDs and IDs with no unscheduled caller-owned reference.
    pub fn release_later(&mut self, id: ArenaId) -> Result<(), ArenaError> {
        let should_enqueue = {
            let slot = self.slot(id)?;
            if slot.owned_references == 0 {
                return Err(ArenaError::NoOwnedReference(id));
            }
            !slot.queued
        };
        self.advance_mutation_epoch()?;
        {
            let slot = self.slot_mut(id)?;
            slot.owned_references -= 1;
            slot.scheduled_releases += 1;
        }
        self.pending_transitions += 1;
        if should_enqueue {
            self.enqueue_index(id.index);
        }
        Ok(())
    }

    /// Applies at most `fuel` reference transitions without recursive drops.
    ///
    /// One transition can destroy at most one 4 KiB payload and schedule at most
    /// two child releases. The intrusive FIFO coalesces transitions by live slot,
    /// so child expansion needs no separately capped frontier.
    ///
    /// # Panics
    ///
    /// Panics only if a generation-checked live slot loses its node between
    /// validation and removal, which would indicate internal arena corruption.
    ///
    /// # Errors
    ///
    /// Returns ownership errors if bookkeeping is invalid. Generation
    /// exhaustion retires the affected slot and returns an error containing the
    /// completed transition's receipt; no completed work is hidden.
    pub fn poll_reclaim(&mut self, fuel: usize) -> Result<ReclaimReceipt, ReclaimPollError> {
        let mut receipt = ReclaimReceipt::default();
        while receipt.reference_transitions < fuel && self.pending_head.is_some() {
            if let Err(error) = self.reclaim_one(&mut receipt) {
                receipt.pending_after = self.pending_transitions;
                return Err(ReclaimPollError { error, receipt });
            }
        }
        receipt.pending_after = self.pending_transitions;
        Ok(receipt)
    }

    fn reclaim_one(&mut self, receipt: &mut ReclaimReceipt) -> Result<(), ArenaError> {
        let index = self
            .pending_head
            .ok_or(ArenaError::InvalidReferenceState(ArenaId::default()))?;
        let id = {
            let slot = self.slot_by_index(index);
            ArenaId {
                index,
                generation: slot.generation,
            }
        };
        let (references, scheduled_releases, children) = {
            let slot = self.slot(id)?;
            let node = slot.node.as_ref().ok_or(ArenaError::StaleId(id))?;
            (slot.references, slot.scheduled_releases, node.children)
        };
        if scheduled_releases == 0 || scheduled_releases > references {
            return Err(ArenaError::InvalidReferenceState(id));
        }
        if references == 1 {
            self.validate_child_releases(&children)?;
        }

        self.advance_mutation_epoch()?;
        let (remaining_references, remaining_scheduled) = {
            let slot = self.slot_mut(id)?;
            slot.references -= 1;
            slot.scheduled_releases -= 1;
            (slot.references, slot.scheduled_releases)
        };
        self.pending_transitions -= 1;
        receipt.reference_transitions += 1;

        if remaining_scheduled == 0 {
            self.dequeue_head(index);
        }
        if remaining_references > 0 {
            return Ok(());
        }
        if remaining_scheduled != 0 {
            return Err(ArenaError::InvalidReferenceState(id));
        }

        let generation_exhausted = id.generation == self.generation_ceiling;
        let free_head = self.free_head;
        let node = {
            let slot = self.slot_by_index_mut(id.index);
            debug_assert_eq!(slot.generation, id.generation);
            debug_assert!(slot.node.is_some());
            if slot.owned_references != 0 || slot.queued {
                return Err(ArenaError::InvalidReferenceState(id));
            }
            slot.references = 0;
            slot.scheduled_releases = 0;
            slot.next_pending = None;
            if generation_exhausted {
                slot.retired = true;
                slot.next_free = None;
            } else {
                slot.generation += 1;
                slot.next_free = free_head;
            }
            slot.node.take().expect("validated live node exists")
        };
        if generation_exhausted {
            self.retired_slots += 1;
            receipt.slots_retired += 1;
        } else {
            self.free_head = Some(id.index);
        }
        self.live_nodes -= 1;
        self.live_payload_bytes -= node.payload.allocated_bytes();
        receipt.nodes_reclaimed += 1;
        receipt.payload_bytes_reclaimed += node.payload.allocated_bytes();
        for child in node.children.into_iter().flatten() {
            self.schedule_child_release(child);
            receipt.child_releases_enqueued += 1;
        }
        // Dropping `node` here frees one bounded payload. Its children are
        // integer IDs, so Rust cannot recursively destroy the graph.

        if generation_exhausted {
            return Err(ArenaError::GenerationExhausted(id));
        }
        Ok(())
    }

    /// Reads the immutable payload of a live node.
    ///
    /// # Errors
    ///
    /// Rejects stale or already-reclaimed IDs.
    pub fn payload(&self, id: ArenaId) -> Result<&[u8], ArenaError> {
        Ok(self
            .slot(id)?
            .node
            .as_ref()
            .ok_or(ArenaError::StaleId(id))?
            .payload
            .as_bytes())
    }

    /// Reads the first immutable child edge without traversing farther.
    ///
    /// The append-only document graph uses this edge as its previous-page
    /// link. Keeping traversal scalar lets query code retain a fixed-size
    /// cursor instead of materializing a document-sized handle list.
    ///
    /// # Errors
    ///
    /// Rejects stale or already-reclaimed IDs.
    pub fn first_child(&self, id: ArenaId) -> Result<Option<ArenaId>, ArenaError> {
        Ok(self
            .slot(id)?
            .node
            .as_ref()
            .ok_or(ArenaError::StaleId(id))?
            .children[0])
    }

    /// Returns current and high-water accounting without traversing nodes.
    #[must_use]
    pub fn metrics(&self) -> ArenaMetrics {
        ArenaMetrics {
            slabs: self.slab_count,
            total_slots: self.slab_count * ARENA_SLAB_SLOTS,
            live_nodes: self.live_nodes,
            live_payload_bytes: self.live_payload_bytes,
            high_water_live_nodes: self.high_water_live_nodes,
            high_water_payload_bytes: self.high_water_payload_bytes,
            pending_releases: self.pending_transitions,
            queued_release_nodes: self.queued_release_nodes,
            retired_slots: self.retired_slots,
        }
    }

    fn enqueue_index(&mut self, index: u32) {
        debug_assert!(!self.slot_by_index(index).queued);
        debug_assert!(self.slot_by_index(index).scheduled_releases > 0);
        if let Some(tail) = self.pending_tail {
            self.slot_by_index_mut(tail).next_pending = Some(index);
        } else {
            debug_assert!(self.pending_head.is_none());
            self.pending_head = Some(index);
        }
        let slot = self.slot_by_index_mut(index);
        slot.queued = true;
        slot.next_pending = None;
        self.pending_tail = Some(index);
        self.queued_release_nodes += 1;
    }

    fn dequeue_head(&mut self, expected: u32) {
        debug_assert_eq!(self.pending_head, Some(expected));
        let next = self.slot_by_index(expected).next_pending;
        let slot = self.slot_by_index_mut(expected);
        slot.queued = false;
        slot.next_pending = None;
        self.pending_head = next;
        if next.is_none() {
            self.pending_tail = None;
        }
        self.queued_release_nodes -= 1;
    }

    fn validate_child_releases(
        &self,
        children: &[Option<ArenaId>; MAX_ARENA_CHILDREN],
    ) -> Result<(), ArenaError> {
        for (index, child) in children.iter().flatten().copied().enumerate() {
            let prior_duplicates = children
                .iter()
                .flatten()
                .take(index + 1)
                .filter(|&&candidate| candidate == child)
                .count();
            if prior_duplicates != 1 {
                continue;
            }
            let releases = children
                .iter()
                .flatten()
                .filter(|&&candidate| candidate == child)
                .count();
            let slot = self.slot(child)?;
            if slot.references.saturating_sub(slot.scheduled_releases)
                < u32::try_from(releases).expect("at most two child edges")
            {
                return Err(ArenaError::InvalidReferenceState(child));
            }
        }
        Ok(())
    }

    fn schedule_child_release(&mut self, child: ArenaId) {
        let should_enqueue = {
            let slot = self
                .slot_mut(child)
                .expect("child release was validated before parent reclamation");
            debug_assert!(slot.scheduled_releases < slot.references);
            slot.scheduled_releases += 1;
            !slot.queued
        };
        self.pending_transitions += 1;
        if should_enqueue {
            self.enqueue_index(child.index);
        }
    }

    fn validate_child_increments(&self, children: &[ArenaId]) -> Result<(), ArenaError> {
        for (index, &child) in children.iter().enumerate() {
            let duplicates = children[..=index]
                .iter()
                .filter(|&&candidate| candidate == child)
                .count();
            if duplicates == 1 {
                let total = children
                    .iter()
                    .filter(|&&candidate| candidate == child)
                    .count();
                let total = u32::try_from(total).expect("at most two children");
                if self.slot(child)?.references > u32::MAX - total {
                    return Err(ArenaError::ReferenceCountOverflow(child));
                }
            }
        }
        Ok(())
    }

    fn validate_child_transfers(&self, children: &[ArenaId]) -> Result<(), ArenaError> {
        for (index, &child) in children.iter().enumerate() {
            if children[..index].contains(&child) {
                continue;
            }
            let required = children
                .iter()
                .filter(|&&candidate| candidate == child)
                .count();
            let required = u32::try_from(required).expect("at most two children");
            if self.slot(child)?.owned_references < required {
                return Err(ArenaError::NoOwnedReference(child));
            }
        }
        Ok(())
    }

    fn advance_mutation_epoch(&mut self) -> Result<(), ArenaError> {
        self.mutation_epoch = self
            .mutation_epoch
            .checked_add(1)
            .ok_or(ArenaError::MutationEpochExhausted)?;
        Ok(())
    }

    fn preview_slab_growth(&self) -> Result<SlabGrowth, ArenaError> {
        let base = self
            .slab_count
            .checked_mul(ARENA_SLAB_SLOTS)
            .ok_or(ArenaError::NodeIndexOverflow)?;
        let last = base
            .checked_add(ARENA_SLAB_SLOTS - 1)
            .ok_or(ArenaError::NodeIndexOverflow)?;
        let _ = u32::try_from(last).map_err(|_| ArenaError::NodeIndexOverflow)?;

        let (root_index, branch_index, _) = slab_directory_indexes(self.slab_count)?;
        let branch_missing = self.slab_roots[root_index].is_none();
        let leaf_missing = branch_missing
            || self.slab_roots[root_index]
                .as_ref()
                .is_none_or(|branch| branch.leaves[branch_index].is_none());
        let directory_blocks_added = usize::from(branch_missing) + usize::from(leaf_missing);
        let directory_entries_initialized =
            directory_blocks_added.saturating_mul(SLAB_DIRECTORY_WIDTH);
        let directory_bytes_initialized = usize::from(branch_missing)
            .saturating_mul(size_of::<SlabBranchDirectory>())
            .saturating_add(
                usize::from(leaf_missing).saturating_mul(size_of::<SlabLeafDirectory>()),
            );
        Ok(SlabGrowth {
            slabs_added: 1,
            slots_initialized: ARENA_SLAB_SLOTS,
            slot_bytes_initialized: ARENA_SLAB_SLOTS.saturating_mul(size_of::<Slot>()),
            directory_blocks_added,
            directory_entries_initialized,
            directory_bytes_initialized,
        })
    }

    fn add_slab(&mut self) -> Result<SlabGrowth, ArenaError> {
        let growth = self.preview_slab_growth()?;
        let base = self
            .slab_count
            .checked_mul(ARENA_SLAB_SLOTS)
            .ok_or(ArenaError::NodeIndexOverflow)?;
        let previous_head = self.free_head;
        let mut slab = Box::new(std::array::from_fn(|_| Slot::default()));
        for (offset, slot) in slab.iter_mut().enumerate() {
            slot.next_free = if offset + 1 < ARENA_SLAB_SLOTS {
                Some(u32::try_from(base + offset + 1).expect("slab indexes were bounded"))
            } else {
                previous_head
            };
        }
        let (root_index, branch_index, leaf_index) = slab_directory_indexes(self.slab_count)?;
        let branch = self.slab_roots[root_index]
            .get_or_insert_with(|| Box::new(SlabBranchDirectory::default()));
        let leaf = branch.leaves[branch_index]
            .get_or_insert_with(|| Box::new(SlabLeafDirectory::default()));
        debug_assert!(leaf.slabs[leaf_index].is_none());
        leaf.slabs[leaf_index] = Some(slab);
        self.slab_count += 1;
        self.free_head = Some(u32::try_from(base).expect("slab indexes were bounded"));
        Ok(growth)
    }

    fn slot(&self, id: ArenaId) -> Result<&Slot, ArenaError> {
        let slot = self.slot_by_index_checked(id.index)?;
        if slot.generation != id.generation || slot.node.is_none() || slot.references == 0 {
            return Err(ArenaError::StaleId(id));
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, id: ArenaId) -> Result<&mut Slot, ArenaError> {
        let slot = self.slot_by_index_mut_checked(id.index)?;
        if slot.generation != id.generation || slot.node.is_none() || slot.references == 0 {
            return Err(ArenaError::StaleId(id));
        }
        Ok(slot)
    }

    fn slot_by_index(&self, index: u32) -> &Slot {
        self.slot_by_index_checked(index)
            .expect("free-list index belongs to an allocated slab")
    }

    fn slot_by_index_mut(&mut self, index: u32) -> &mut Slot {
        self.slot_by_index_mut_checked(index)
            .expect("free-list index belongs to an allocated slab")
    }

    fn slot_by_index_checked(&self, index: u32) -> Result<&Slot, ArenaError> {
        let raw_index = index;
        let index = index as usize;
        let slab_index = index / ARENA_SLAB_SLOTS;
        let (root_index, branch_index, leaf_index) =
            slab_directory_indexes(slab_index).map_err(|_| {
                ArenaError::StaleId(ArenaId {
                    index: raw_index,
                    generation: 0,
                })
            })?;
        self.slab_roots
            .get(root_index)
            .and_then(Option::as_deref)
            .and_then(|branch| branch.leaves.get(branch_index))
            .and_then(Option::as_deref)
            .and_then(|leaf| leaf.slabs.get(leaf_index))
            .and_then(Option::as_deref)
            .and_then(|slab| slab.get(index % ARENA_SLAB_SLOTS))
            .ok_or(ArenaError::StaleId(ArenaId {
                index: raw_index,
                generation: 0,
            }))
    }

    fn slot_by_index_mut_checked(&mut self, index: u32) -> Result<&mut Slot, ArenaError> {
        let raw_index = index;
        let index = index as usize;
        let slab_index = index / ARENA_SLAB_SLOTS;
        let (root_index, branch_index, leaf_index) =
            slab_directory_indexes(slab_index).map_err(|_| {
                ArenaError::StaleId(ArenaId {
                    index: raw_index,
                    generation: 0,
                })
            })?;
        self.slab_roots
            .get_mut(root_index)
            .and_then(Option::as_deref_mut)
            .and_then(|branch| branch.leaves.get_mut(branch_index))
            .and_then(Option::as_deref_mut)
            .and_then(|leaf| leaf.slabs.get_mut(leaf_index))
            .and_then(Option::as_deref_mut)
            .and_then(|slab| slab.get_mut(index % ARENA_SLAB_SLOTS))
            .ok_or(ArenaError::StaleId(ArenaId {
                index: raw_index,
                generation: 0,
            }))
    }
}

fn slab_directory_indexes(slab_index: usize) -> Result<(usize, usize, usize), ArenaError> {
    let capacity = SLAB_DIRECTORY_WIDTH
        .checked_mul(SLAB_DIRECTORY_WIDTH)
        .and_then(|value| value.checked_mul(SLAB_DIRECTORY_WIDTH))
        .ok_or(ArenaError::NodeIndexOverflow)?;
    if slab_index >= capacity {
        return Err(ArenaError::NodeIndexOverflow);
    }
    let leaf_index = slab_index % SLAB_DIRECTORY_WIDTH;
    let branch_index = (slab_index / SLAB_DIRECTORY_WIDTH) % SLAB_DIRECTORY_WIDTH;
    let root_index = slab_index / (SLAB_DIRECTORY_WIDTH * SLAB_DIRECTORY_WIDTH);
    Ok((root_index, branch_index, leaf_index))
}

fn bound_children(children: &[ArenaId]) -> [Option<ArenaId>; MAX_ARENA_CHILDREN] {
    let mut bound = [None; MAX_ARENA_CHILDREN];
    for (target, child) in bound.iter_mut().zip(children.iter().copied()) {
        *target = Some(child);
    }
    bound
}
