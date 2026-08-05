//! Physical parser-root ownership spanning scheduler and arena handles.
//!
//! [`Scheduler`](crate::scheduler::Scheduler) deliberately treats parser jobs
//! and committed roots as opaque values. This module is the other side of that
//! seam: every published [`ArenaRootId`] encodes a generation-checked
//! [`ArenaId`], the one active [`ArenaJobId`] resolves to an actual arena graph,
//! and scheduler reclaim tickets drive [`PageArena::poll_reclaim`] directly.
//!
//! There is no document-sized handle map. A document has one active-job slot
//! and, because scheduler reclamation is FIFO, one in-flight physical ticket.
//! Up to four additional tickets may wait in the scheduler without acquiring
//! any lifetime-side storage.

use std::fmt;

use crate::arena::{
    ArenaAllocationPreview, ArenaAllocationReceipt, ArenaError, ArenaId, ArenaMetrics, PageArena,
    ReclaimPollError, ReclaimReceipt, ARENA_PAGE_BYTES,
};
use crate::scheduler::{
    self, ArenaJobId, ArenaRootId, Audit, InitialState, JobActivationPermit, ParseToken,
    ReclaimHandle, ReclaimOutcome, ReclaimRequest, ReclaimTarget, Reclaimer, Scheduler,
    SliceLimits, SourceRootId,
};

/// Work performed while adding one physical page to a candidate graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageAllocationReceipt {
    pub allocation: ArenaAllocationReceipt,
}

impl PageAllocationReceipt {
    /// Converts the physical receipt to the scheduler's seven-dimensional
    /// ledger. Slab initialization is charged as allocation instead of being
    /// hidden behind the arena abstraction.
    #[must_use]
    pub fn audit(self) -> Audit {
        allocation_preview_audit(self.allocation.preview())
    }
}

/// Result of attempting to transfer one already-allocated canonical page into
/// the active arena graph under the current hard slice limits.
#[derive(Debug)]
pub enum OwnedPageAppend {
    /// The original allocation is now owned by the arena node.
    Appended(PageAllocationReceipt),
    /// The atomic node/slab unit fits a fresh slice but not the current
    /// remaining ledger. The producer retains the exact original allocation.
    Deferred(Box<[u8; ARENA_PAGE_BYTES]>),
}

/// Fixed-size reverse-chain cursor for one sealed arena root.
#[derive(Clone, Copy, Debug)]
pub struct ArenaChainCursor {
    next: Option<ArenaId>,
}

fn allocation_preview_audit(preview: ArenaAllocationPreview) -> Audit {
    let reference_transitions = preview
        .child_references_added
        .saturating_add(preview.child_owned_references_transferred);
    let initialization_transitions = preview
        .slots_initialized
        .saturating_add(preview.directory_entries_initialized)
        .saturating_add(preview.slabs_added)
        .saturating_add(preview.directory_blocks_added);
    let initialized_bytes = preview
        .slot_bytes_initialized
        .saturating_add(preview.directory_bytes_initialized);
    Audit {
        source_bytes: 0,
        // The immutable node allocation, each persistent reference operation,
        // and every lazily initialized slab/directory element are separately
        // charged. This makes a slab boundary larger but still constant.
        transitions: 1_u64
            .saturating_add(u64::try_from(reference_transitions).unwrap_or(u64::MAX))
            .saturating_add(u64::try_from(initialization_transitions).unwrap_or(u64::MAX)),
        allocated_bytes: u64::try_from(
            preview
                .payload_bytes_copied
                .saturating_add(initialized_bytes),
        )
        .unwrap_or(u64::MAX),
        copied_bytes: u64::try_from(preview.payload_bytes_copied).unwrap_or(u64::MAX),
        hashed_bytes: 0,
        index_nodes: 1,
        reclaimed_nodes: 0,
    }
}

/// Receipt for physically activating a parser job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobActivationReceipt {
    pub job: ArenaJobId,
    pub anchor: PageAllocationReceipt,
}

impl JobActivationReceipt {
    /// Complete activation ledger: physical anchor allocation plus the atomic
    /// scheduler job-binding transition that makes parser work legal.
    #[must_use]
    pub fn audit(self) -> Audit {
        job_activation_preview_audit(self.anchor.allocation.preview())
    }
}

fn job_activation_preview_audit(preview: ArenaAllocationPreview) -> Audit {
    let mut audit = allocation_preview_audit(preview);
    audit.transitions = audit.transitions.saturating_add(1);
    audit
}

#[derive(Clone, Copy, Debug)]
struct ActiveJob {
    handle: ArenaJobId,
    root: ArenaId,
    pages: usize,
}

#[derive(Clone, Copy, Debug)]
struct InFlightReclaim {
    target: ReclaimTarget,
}

/// Failure at the physical scheduler/arena ownership seam.
#[derive(Debug)]
pub enum LifetimeError {
    Arena(ArenaError),
    ArenaReclaim(ReclaimPollError),
    Scheduler(scheduler::Error),
    ActiveJobExists(ArenaJobId),
    NoActiveJob,
    StaleJob {
        expected: ArenaJobId,
        provided: ArenaJobId,
    },
    JobIdentityExhausted,
    /// A scheduler reclaim ticket may start only after the preceding physical
    /// FIFO is empty. Anchor rollback uses a separate immediate leaf discard.
    ReclaimStartNeedsIdleArena {
        pending_releases: usize,
    },
    InvalidRootHandle(ArenaRootId),
    /// One indivisible physical allocation cannot fit the issued slice even
    /// before any other work. Repeating the same permit would otherwise wedge.
    AllocationUnitExceedsSliceLimits {
        audit: Audit,
        limits: SliceLimits,
    },
}

impl fmt::Display for LifetimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => write!(formatter, "arena operation failed: {error}"),
            Self::ArenaReclaim(error) => write!(formatter, "arena reclaim failed: {error}"),
            Self::Scheduler(error) => write!(formatter, "scheduler operation failed: {error}"),
            Self::ActiveJobExists(job) => {
                write!(formatter, "arena job {job:?} is still active")
            }
            Self::NoActiveJob => formatter.write_str("there is no active physical arena job"),
            Self::StaleJob { expected, provided } => write!(
                formatter,
                "arena job {provided:?} is stale; active job is {expected:?}"
            ),
            Self::JobIdentityExhausted => formatter.write_str("arena job identity exhausted"),
            Self::ReclaimStartNeedsIdleArena { pending_releases } => write!(
                formatter,
                "cannot start another reclaim target while {pending_releases} arena releases are pending"
            ),
            Self::InvalidRootHandle(root) => write!(formatter, "invalid arena root {root:?}"),
            Self::AllocationUnitExceedsSliceLimits { .. } => formatter.write_str(
                "one physical allocation, including arena growth, exceeds slice limits",
            ),
        }
    }
}

impl std::error::Error for LifetimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Arena(error) => Some(error),
            Self::ArenaReclaim(error) => Some(error),
            Self::Scheduler(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ArenaError> for LifetimeError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<ReclaimPollError> for LifetimeError {
    fn from(error: ReclaimPollError) -> Self {
        Self::ArenaReclaim(error)
    }
}

impl From<scheduler::Error> for LifetimeError {
    fn from(error: scheduler::Error) -> Self {
        Self::Scheduler(error)
    }
}

/// One document's physically owned parser pages.
#[derive(Debug)]
pub struct PhysicalLifetime {
    arena: PageArena,
    next_job_identity: u64,
    active_job: Option<ActiveJob>,
    in_flight_reclaim: Option<InFlightReclaim>,
    last_reclaim_error: Option<ArenaError>,
}

impl Default for PhysicalLifetime {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicalLifetime {
    /// Creates an empty physical lifetime with no handle-side heap map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            arena: PageArena::new(),
            next_job_identity: 1,
            active_job: None,
            in_flight_reclaim: None,
            last_reclaim_error: None,
        }
    }

    /// Allocates the revision-zero arena root and constructs its scheduler.
    ///
    /// # Errors
    ///
    /// Returns an arena allocation error or scheduler validation error. A
    /// scheduler error immediately discards the unlinked anchor before return.
    pub fn initialize_scheduler(
        &mut self,
        limits: SliceLimits,
        source_root: SourceRootId,
        visible_pages: u32,
        payload: &[u8],
    ) -> Result<(Scheduler, ArenaRootId, PageAllocationReceipt), LifetimeError> {
        let allocation = self.arena.allocate(payload, &[])?;
        let arena_root = encode_root(allocation.id);
        let receipt = PageAllocationReceipt { allocation };
        match Scheduler::new(
            limits,
            InitialState {
                source_root,
                arena_root,
                visible_pages,
            },
        ) {
            Ok(scheduler) => Ok((scheduler, arena_root, receipt)),
            Err(error) => {
                self.arena.discard_unlinked_owned(allocation.id)?;
                Err(LifetimeError::Scheduler(error))
            }
        }
    }

    /// Allocates a real anchor page and atomically binds it to the scheduler's
    /// active parse. A failed scheduler activation rolls the page back.
    ///
    /// # Errors
    ///
    /// Rejects a second active job, exhausted job identities, arena failures,
    /// and scheduler token/state failures. An older graph may be mid-reclaim:
    /// failed adoption discards the new unlinked anchor without touching that
    /// graph's FIFO or audit.
    pub fn activate_scheduler_job(
        &mut self,
        scheduler: &mut Scheduler,
        token: ParseToken,
        anchor_payload: &[u8],
    ) -> Result<JobActivationReceipt, LifetimeError> {
        if let Some(active) = self.active_job {
            return Err(LifetimeError::ActiveJobExists(active.handle));
        }
        let job = ArenaJobId(self.next_job_identity);
        let next_identity = self
            .next_job_identity
            .checked_add(1)
            .ok_or(LifetimeError::JobIdentityExhausted)?;
        scheduler.validate_forecast_job_activation(token, job)?;
        let allocation = self.arena.allocate(anchor_payload, &[])?;
        if let Err(error) = scheduler.activate_active(token, job) {
            self.arena.discard_unlinked_owned(allocation.id)?;
            return Err(LifetimeError::Scheduler(error));
        }
        self.next_job_identity = next_identity;
        self.active_job = Some(ActiveJob {
            handle: job,
            root: allocation.id,
            pages: 1,
        });
        Ok(JobActivationReceipt {
            job,
            anchor: PageAllocationReceipt { allocation },
        })
    }

    /// Creates and binds a measured candidate under a one-shot scheduler
    /// activation capability.
    ///
    /// The anchor allocation is preflighted exactly, including lazy slab and
    /// directory growth, before mutation. An impossible unit consumes the
    /// capability without allocating; an adoption failure immediately discards
    /// the still-unlinked leaf. Thus parser work can begin only after the
    /// physical activation receipt has been accepted under a hard slice limit.
    ///
    /// # Errors
    ///
    /// Rejects stale/mismatched permits, a still-active older physical job,
    /// exhausted identities, arena failures, or an activation receipt larger
    /// than the issued capability.
    pub fn activate_measured_scheduler_job(
        &mut self,
        scheduler: &mut Scheduler,
        permit: JobActivationPermit,
        anchor_payload: &[u8],
    ) -> Result<JobActivationReceipt, LifetimeError> {
        if let Some(active) = self.active_job {
            let _ = scheduler.cancel_measured_job_activation(permit);
            return Err(LifetimeError::ActiveJobExists(active.handle));
        }
        let job = ArenaJobId(self.next_job_identity);
        let Some(next_identity) = self.next_job_identity.checked_add(1) else {
            let _ = scheduler.cancel_measured_job_activation(permit);
            return Err(LifetimeError::JobIdentityExhausted);
        };
        let preview = match self.arena.preview_allocate(anchor_payload.len(), &[]) {
            Ok(preview) => preview,
            Err(error) => {
                let _ = scheduler.cancel_measured_job_activation(permit);
                return Err(LifetimeError::Arena(error));
            }
        };
        let audit = job_activation_preview_audit(preview);
        if let Err(error) = scheduler.validate_measured_job_activation(permit, audit) {
            let _ = scheduler.cancel_measured_job_activation(permit);
            return Err(LifetimeError::Scheduler(error));
        }
        let allocation = match self
            .arena
            .allocate_preflighted(preview, anchor_payload, &[])
        {
            Ok(allocation) => allocation,
            Err(error) => {
                let _ = scheduler.cancel_measured_job_activation(permit);
                return Err(LifetimeError::Arena(error));
            }
        };
        debug_assert_eq!(allocation.preview(), preview);
        if let Err(error) = scheduler.adopt_measured_job_activation(permit, job, audit) {
            self.arena.discard_unlinked_owned(allocation.id)?;
            return Err(LifetimeError::Scheduler(error));
        }
        self.next_job_identity = next_identity;
        self.active_job = Some(ActiveJob {
            handle: job,
            root: allocation.id,
            pages: 1,
        });
        let receipt = JobActivationReceipt {
            job,
            anchor: PageAllocationReceipt { allocation },
        };
        debug_assert_eq!(receipt.audit(), audit);
        Ok(receipt)
    }

    /// Adds one immutable page above the current job root using a real arena
    /// child edge.
    ///
    /// [`PageArena::allocate_transferring_owned_children`] moves the sole owned
    /// reference into the new parent edge atomically. No housekeeping release
    /// is added to the global reclaim FIFO, so construction can safely overlap
    /// bounded retirement of an older graph.
    ///
    /// # Errors
    ///
    /// Rejects stale jobs, oversized payloads, and invalid child ownership.
    pub fn append_job_page(
        &mut self,
        job: ArenaJobId,
        payload: &[u8],
    ) -> Result<PageAllocationReceipt, LifetimeError> {
        let active = self.active_job.ok_or(LifetimeError::NoActiveJob)?;
        if active.handle != job {
            return Err(LifetimeError::StaleJob {
                expected: active.handle,
                provided: job,
            });
        }
        let allocation = self
            .arena
            .allocate_transferring_owned_children(payload, &[active.root])?;
        let active = self.active_job.as_mut().ok_or(LifetimeError::NoActiveJob)?;
        active.root = allocation.id;
        active.pages = active.pages.saturating_add(1);
        Ok(PageAllocationReceipt { allocation })
    }

    /// Returns the exact receipt expected for the next one-child job append
    /// without mutating arena or scheduler state. Slab and fixed-directory
    /// growth are included rather than treated as an exceptional boundary.
    ///
    /// # Errors
    ///
    /// Rejects a stale job, oversized payload, invalid root ownership, or arena
    /// index exhaustion.
    pub fn preview_append_job_page_audit(
        &self,
        job: ArenaJobId,
        payload_bytes: usize,
    ) -> Result<Audit, LifetimeError> {
        let active = self.active_job.ok_or(LifetimeError::NoActiveJob)?;
        if active.handle != job {
            return Err(LifetimeError::StaleJob {
                expected: active.handle,
                provided: job,
            });
        }
        if payload_bytes > ARENA_PAGE_BYTES {
            return Err(LifetimeError::Arena(ArenaError::PayloadTooLarge(
                payload_bytes,
            )));
        }
        let preview = self
            .arena
            .preview_allocate_transferring_owned_children(payload_bytes, &[active.root])?;
        Ok(allocation_preview_audit(preview))
    }

    /// Performs one append only when its exact preflight fits `remaining`.
    ///
    /// `Ok(None)` means the unit fits an otherwise empty slice but not the
    /// caller's remaining ledger, so yielding is sufficient. If the unit cannot
    /// fit the full `slice_limits`, a typed error prevents an infinite yield
    /// loop. Preflight and mutation occur inside one exclusive borrow, and the
    /// actual receipt is checked against the preview.
    ///
    /// # Errors
    ///
    /// Rejects stale jobs, arena failures, inconsistent remaining limits, or
    /// an indivisible allocation larger than `slice_limits`.
    pub fn try_append_job_page_under_limits(
        &mut self,
        job: ArenaJobId,
        payload: &[u8],
        remaining: SliceLimits,
        slice_limits: SliceLimits,
    ) -> Result<Option<PageAllocationReceipt>, LifetimeError> {
        let active = self.active_job.ok_or(LifetimeError::NoActiveJob)?;
        if active.handle != job {
            return Err(LifetimeError::StaleJob {
                expected: active.handle,
                provided: job,
            });
        }
        let preview = self
            .arena
            .preview_allocate_transferring_owned_children(payload.len(), &[active.root])?;
        let audit = allocation_preview_audit(preview);
        if !audit.fits(slice_limits) {
            return Err(LifetimeError::AllocationUnitExceedsSliceLimits {
                audit,
                limits: slice_limits,
            });
        }
        if !audit.fits(remaining) {
            return Ok(None);
        }
        let allocation = self
            .arena
            .allocate_transferring_owned_children_preflighted(preview, payload, &[active.root])?;
        let active = self.active_job.as_mut().ok_or(LifetimeError::NoActiveJob)?;
        active.root = allocation.id;
        active.pages = active.pages.saturating_add(1);
        let receipt = PageAllocationReceipt { allocation };
        debug_assert_eq!(receipt.audit(), audit);
        Ok(Some(receipt))
    }

    /// Transfers one existing fixed 4 KiB allocation above the active job root
    /// only after exact arena-node/slab/directory preflight.
    ///
    /// The producer already charged creation of the payload allocation. This
    /// operation therefore charges no payload allocation or copy a second
    /// time; it charges the arena node, child ownership transfer, and any lazy
    /// slab/directory initialization. When only the current remaining ledger
    /// is too small, [`OwnedPageAppend::Deferred`] returns the allocation
    /// unchanged for the next slice.
    ///
    /// # Errors
    ///
    /// Rejects stale jobs, oversized used lengths, arena failures, inconsistent
    /// remaining limits, or an indivisible arena unit larger than the full
    /// slice limits. Errors consume and drop the supplied bounded page.
    pub fn try_adopt_job_page_under_limits(
        &mut self,
        job: ArenaJobId,
        allocation: Box<[u8; ARENA_PAGE_BYTES]>,
        used_len: usize,
        remaining: SliceLimits,
        slice_limits: SliceLimits,
    ) -> Result<OwnedPageAppend, LifetimeError> {
        let active = self.active_job.ok_or(LifetimeError::NoActiveJob)?;
        if active.handle != job {
            return Err(LifetimeError::StaleJob {
                expected: active.handle,
                provided: job,
            });
        }
        let preview = self
            .arena
            .preview_adopt_owned_page_transferring_owned_children(used_len, &[active.root])?;
        let audit = allocation_preview_audit(preview);
        if !audit.fits(slice_limits) {
            return Err(LifetimeError::AllocationUnitExceedsSliceLimits {
                audit,
                limits: slice_limits,
            });
        }
        if !audit.fits(remaining) {
            return Ok(OwnedPageAppend::Deferred(allocation));
        }
        let allocation = self
            .arena
            .adopt_owned_page_transferring_owned_children_preflighted(
                preview,
                allocation,
                used_len,
                &[active.root],
            )?;
        let active = self.active_job.as_mut().ok_or(LifetimeError::NoActiveJob)?;
        active.root = allocation.id;
        active.pages = active.pages.saturating_add(1);
        let receipt = PageAllocationReceipt { allocation };
        debug_assert_eq!(receipt.audit(), audit);
        Ok(OwnedPageAppend::Appended(receipt))
    }

    /// Atomically substitutes the scheduler's active job with the job's current
    /// generation-safe root. No arena allocation, copy, traversal, or reference
    /// transition occurs.
    ///
    /// # Errors
    ///
    /// Leaves the physical job active when scheduler sealing rejects the token,
    /// parse state, or source snapshot.
    pub fn seal_scheduler_job(
        &mut self,
        scheduler: &mut Scheduler,
        token: ParseToken,
        visible_pages: u32,
    ) -> Result<ArenaRootId, LifetimeError> {
        let active = self.active_job.ok_or(LifetimeError::NoActiveJob)?;
        let root = encode_root(active.root);
        scheduler.seal_active(token, active.handle, root, visible_pages)?;
        self.active_job = None;
        Ok(root)
    }

    /// Reads a published root payload through its generation-checked arena ID.
    ///
    /// # Errors
    ///
    /// Rejects malformed, stale, or already reclaimed roots.
    pub fn root_payload(&self, root: ArenaRootId) -> Result<&[u8], LifetimeError> {
        let id = decode_root(root)?;
        self.arena.payload(id).map_err(LifetimeError::Arena)
    }

    /// Starts a fixed-size cursor at a sealed root. Pages are returned from the
    /// newest root toward the activation anchor, matching the physical child
    /// chain and performing no eager traversal.
    ///
    /// # Errors
    ///
    /// Rejects malformed or stale roots.
    pub fn root_chain_cursor(&self, root: ArenaRootId) -> Result<ArenaChainCursor, LifetimeError> {
        let id = decode_root(root)?;
        self.arena.payload(id)?;
        Ok(ArenaChainCursor { next: Some(id) })
    }

    /// Reads one root-chain page and advances the cursor by one integer edge.
    ///
    /// # Errors
    ///
    /// Rejects a page reclaimed or otherwise made stale after cursor creation.
    pub fn root_chain_step<'a>(
        &'a self,
        cursor: &mut ArenaChainCursor,
    ) -> Result<Option<&'a [u8]>, LifetimeError> {
        let Some(id) = cursor.next.take() else {
            return Ok(None);
        };
        cursor.next = self.arena.first_child(id)?;
        self.arena
            .payload(id)
            .map(Some)
            .map_err(LifetimeError::Arena)
    }

    /// Current physical arena accounting.
    #[must_use]
    pub fn arena_metrics(&self) -> ArenaMetrics {
        self.arena.metrics()
    }

    /// Identity of the sole active job, if any.
    #[must_use]
    pub fn active_job(&self) -> Option<ArenaJobId> {
        self.active_job.map(|active| active.handle)
    }

    /// Number of pages reachable from the active job root in the append-only
    /// chain used by this commitment slice.
    #[must_use]
    pub fn active_job_pages(&self) -> usize {
        self.active_job.map_or(0, |active| active.pages)
    }

    /// Physical ticket currently being polled. Waiting scheduler tickets do not
    /// allocate lifetime-side records.
    #[must_use]
    pub fn in_flight_target(&self) -> Option<ReclaimTarget> {
        self.in_flight_reclaim.map(|in_flight| in_flight.target)
    }

    /// Last arena error observed after a reclaim poll made reportable progress.
    /// Generation exhaustion is fail-closed but its completed work still has to
    /// be returned to the scheduler as an audit receipt.
    #[must_use]
    pub const fn last_reclaim_error(&self) -> Option<ArenaError> {
        self.last_reclaim_error
    }

    fn require_idle_reclaim_queue(&self) -> Result<(), LifetimeError> {
        let pending_releases = self.arena.metrics().pending_releases;
        if pending_releases == 0 {
            Ok(())
        } else {
            Err(LifetimeError::ReclaimStartNeedsIdleArena { pending_releases })
        }
    }

    fn begin_reclaim(&mut self, target: ReclaimTarget) -> Result<(), LifetimeError> {
        debug_assert!(self.in_flight_reclaim.is_none());
        self.require_idle_reclaim_queue()?;
        let (root, cancelled_job) = match target.handle {
            ReclaimHandle::Job(job) => {
                let active = self.active_job.ok_or(LifetimeError::NoActiveJob)?;
                if active.handle != job {
                    return Err(LifetimeError::StaleJob {
                        expected: active.handle,
                        provided: job,
                    });
                }
                (active.root, true)
            }
            ReclaimHandle::Root(root) => {
                let id = decode_root(root)?;
                if self.active_job.is_some_and(|active| active.root == id) {
                    return Err(LifetimeError::InvalidRootHandle(root));
                }
                self.arena.payload(id)?;
                (id, false)
            }
        };
        self.arena.release_later(root)?;
        if cancelled_job {
            self.active_job = None;
        }
        self.in_flight_reclaim = Some(InFlightReclaim { target });
        Ok(())
    }

    fn poll_physical_reclaim(&mut self, request: ReclaimRequest) -> ReclaimOutcome {
        if let Some(in_flight) = self.in_flight_reclaim {
            if in_flight.target != request.ticket.target {
                return ReclaimOutcome::Rejected;
            }
        } else if self.begin_reclaim(request.ticket.target).is_err() {
            return ReclaimOutcome::Rejected;
        }

        let fuel_u64 = request
            .limits
            .transitions
            .min(request.limits.reclaimed_nodes);
        let fuel = usize::try_from(fuel_u64).unwrap_or(usize::MAX);
        if fuel == 0 {
            return ReclaimOutcome::Progress {
                audit: Audit::ZERO,
                complete: false,
            };
        }

        let (receipt, error) = match self.arena.poll_reclaim(fuel) {
            Ok(receipt) => (receipt, None),
            Err(failure) => (failure.receipt, Some(failure.error)),
        };
        self.last_reclaim_error = error;
        let complete = receipt.pending_after == 0;
        let audit = reclaim_audit(receipt);
        if complete {
            self.in_flight_reclaim = None;
        }
        ReclaimOutcome::Progress { audit, complete }
    }
}

impl Reclaimer for PhysicalLifetime {
    fn reclaim(&mut self, request: ReclaimRequest) -> ReclaimOutcome {
        self.poll_physical_reclaim(request)
    }
}

fn encode_root(id: ArenaId) -> ArenaRootId {
    ArenaRootId((u64::from(id.generation) << 32) | u64::from(id.index))
}

fn decode_root(root: ArenaRootId) -> Result<ArenaId, LifetimeError> {
    let generation =
        u32::try_from(root.0 >> 32).map_err(|_| LifetimeError::InvalidRootHandle(root))?;
    if generation == 0 {
        return Err(LifetimeError::InvalidRootHandle(root));
    }
    let index = u32::try_from(root.0 & u64::from(u32::MAX))
        .map_err(|_| LifetimeError::InvalidRootHandle(root))?;
    Ok(ArenaId { index, generation })
}

fn reclaim_audit(receipt: ReclaimReceipt) -> Audit {
    Audit {
        source_bytes: 0,
        transitions: u64::try_from(receipt.reference_transitions).unwrap_or(u64::MAX),
        allocated_bytes: 0,
        copied_bytes: 0,
        hashed_bytes: 0,
        index_nodes: 0,
        reclaimed_nodes: u64::try_from(receipt.nodes_reclaimed).unwrap_or(u64::MAX),
    }
}
