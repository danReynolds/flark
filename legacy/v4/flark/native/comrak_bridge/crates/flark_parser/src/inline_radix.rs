//! Engine-admitted fixed-radix scratch for the resumable inline machine.
//!
//! Mutable algorithm state stays parser-local, while every retained heap
//! allocation is charged against the document arena's live-payload ceiling.
//! The directory is fixed-depth, allocation capacity is acquired fallibly
//! before admission or mutation, and cancellation reclaims at most one fixed
//! allocation per charged step.

use std::array;
use std::collections::TryReserveError;
use std::fmt;
use std::mem::size_of;
use std::ops::{Deref, DerefMut};

use flark_engine::parser_internal::{M11ParserScratchAdmission, M11ParserScratchError};
use flark_engine::{DocumentRuntime, SourceVersion};

const RADIX_BITS: usize = 6;
const RADIX: usize = 1 << RADIX_BITS;
const RADIX_MASK: usize = RADIX - 1;
const RADIX_LEVELS: usize = 4;
pub(crate) const M11_INLINE_RADIX_DATA_PAGE_MAX_BYTES: usize = 4 * 1024;
pub(crate) const M11_INLINE_RADIX_MAX_POLL_TRANSITIONS: usize = 4_096;

/// One fallibly allocated value retained in its one-element Vec.
///
/// Stable Rust does not yet expose fallible `Box::new`. Reserving the Vec
/// before any engine admission lets allocation failure remain recoverable;
/// pushing into that reserved capacity cannot grow. Keeping the Vec also
/// avoids relying on `into_boxed_slice` shrink behavior after admission.
struct FallibleBox<T>(Vec<T>);

impl<T> FallibleBox<T> {
    fn from_prepared(mut prepared: Vec<T>, value: T) -> Self {
        debug_assert!(prepared.is_empty());
        debug_assert!(prepared.capacity() >= 1);
        prepared.push(value);
        debug_assert_eq!(prepared.len(), 1);
        Self(prepared)
    }
}

impl<T> Deref for FallibleBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0[0]
    }
}

impl<T> DerefMut for FallibleBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0[0]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11InlineRadixAllocationReceipt {
    allocations: usize,
    admitted_bytes: usize,
}

impl M11InlineRadixAllocationReceipt {
    #[cfg(test)]
    pub(crate) const fn allocations(self) -> usize {
        self.allocations
    }

    #[cfg(test)]
    pub(crate) const fn admitted_bytes(self) -> usize {
        self.admitted_bytes
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11InlineRadixReclaimPoll {
    transitions: usize,
    allocations_reclaimed: usize,
    admitted_bytes_reclaimed: usize,
    complete: bool,
}

impl M11InlineRadixReclaimPoll {
    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }

    #[cfg(test)]
    pub(crate) const fn allocations_reclaimed(self) -> usize {
        self.allocations_reclaimed
    }

    #[cfg(test)]
    pub(crate) const fn admitted_bytes_reclaimed(self) -> usize {
        self.admitted_bytes_reclaimed
    }

    pub(crate) const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineRadixError {
    Scratch(M11ParserScratchError),
    AllocationFailed,
    AddressExhausted,
    ZeroPageRecords,
    ZeroSizedRecord,
    RecordPageTooLarge { bytes: usize, cap: usize },
    AllocationCapacityMismatch { admitted: usize, retained: usize },
    ZeroFuel,
    PollLimitExceeded,
    ReclaimAlreadyStarted,
    ReclaimNotStarted,
    InvalidState,
}

impl fmt::Display for M11InlineRadixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scratch(error) => write!(formatter, "inline radix scratch failed: {error}"),
            Self::AllocationFailed => formatter.write_str("inline radix typed allocation failed"),
            Self::AddressExhausted => {
                formatter.write_str("inline radix fixed address space exhausted")
            }
            Self::ZeroPageRecords => {
                formatter.write_str("inline radix pages require at least one record")
            }
            Self::ZeroSizedRecord => {
                formatter.write_str("inline radix pages do not admit zero-sized records")
            }
            Self::RecordPageTooLarge { bytes, cap } => {
                write!(
                    formatter,
                    "inline radix record page has {bytes} bytes above its {cap}-byte quantum"
                )
            }
            Self::AllocationCapacityMismatch { admitted, retained } => write!(
                formatter,
                "inline radix allocation retained {retained} bytes after exact admission of \
                 {admitted} bytes"
            ),
            Self::ZeroFuel => formatter.write_str("inline radix reclaim requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("inline radix reclaim exceeds its transition limit")
            }
            Self::ReclaimAlreadyStarted => {
                formatter.write_str("inline radix reclamation is already active")
            }
            Self::ReclaimNotStarted => {
                formatter.write_str("inline radix reclamation has not started")
            }
            Self::InvalidState => formatter.write_str("inline radix state is invalid"),
        }
    }
}

impl std::error::Error for M11InlineRadixError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Scratch(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11ParserScratchError> for M11InlineRadixError {
    fn from(value: M11ParserScratchError) -> Self {
        Self::Scratch(value)
    }
}

impl From<TryReserveError> for M11InlineRadixError {
    fn from(_: TryReserveError) -> Self {
        Self::AllocationFailed
    }
}

/// One record-buffer allocation. Its handle lives inside the already-admitted
/// leaf directory, so this admission covers only the boxed record capacity.
struct RadixDataPage<T: Copy + Default> {
    admission: Option<M11ParserScratchAdmission>,
    previous_allocated_page: Option<usize>,
    records: Vec<T>,
}

struct RadixLeaf<T: Copy + Default> {
    admission: Option<M11ParserScratchAdmission>,
    pages: [Option<RadixDataPage<T>>; RADIX],
    live_pages: u8,
}

impl<T: Copy + Default> RadixLeaf<T> {
    fn new(admission: M11ParserScratchAdmission) -> Self {
        Self {
            admission: Some(admission),
            pages: array::from_fn(|_| None),
            live_pages: 0,
        }
    }
}

struct RadixMiddle<T: Copy + Default> {
    admission: Option<M11ParserScratchAdmission>,
    children: [Option<FallibleBox<RadixLeaf<T>>>; RADIX],
    live_children: u8,
}

impl<T: Copy + Default> RadixMiddle<T> {
    fn new(admission: M11ParserScratchAdmission) -> Self {
        Self {
            admission: Some(admission),
            children: array::from_fn(|_| None),
            live_children: 0,
        }
    }
}

struct RadixTop<T: Copy + Default> {
    admission: Option<M11ParserScratchAdmission>,
    children: [Option<FallibleBox<RadixMiddle<T>>>; RADIX],
    live_children: u8,
}

impl<T: Copy + Default> RadixTop<T> {
    fn new(admission: M11ParserScratchAdmission) -> Self {
        Self {
            admission: Some(admission),
            children: array::from_fn(|_| None),
            live_children: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AllocationPlan {
    top: bool,
    middle: bool,
    leaf: bool,
    page: bool,
}

impl AllocationPlan {
    const fn allocations(self) -> usize {
        self.top as usize + self.middle as usize + self.leaf as usize + self.page as usize
    }
}

/// Fallible capacity acquired after one unsplit admission bundle is held, but
/// before that bundle is split or any directory is mutated.
struct PreparedAllocations<T: Copy + Default> {
    top: Option<Vec<RadixTop<T>>>,
    middle: Option<Vec<RadixMiddle<T>>>,
    leaf: Option<Vec<RadixLeaf<T>>>,
    records: Vec<T>,
}

impl<T: Copy + Default> PreparedAllocations<T> {
    fn new<const PAGE_RECORDS: usize>(plan: AllocationPlan) -> Result<Self, M11InlineRadixError> {
        let top = plan.top.then(prepare_one).transpose()?;
        let middle = plan.middle.then(prepare_one).transpose()?;
        let leaf = plan.leaf.then(prepare_one).transpose()?;
        let mut records = Vec::new();
        records.try_reserve_exact(PAGE_RECORDS)?;
        Ok(Self {
            top,
            middle,
            leaf,
            records,
        })
    }
}

fn prepare_one<T>() -> Result<Vec<T>, TryReserveError> {
    let mut prepared = Vec::new();
    prepared.try_reserve_exact(1)?;
    Ok(prepared)
}

struct PendingReclaim<T: Copy + Default> {
    page: Option<RadixDataPage<T>>,
    leaf: Option<FallibleBox<RadixLeaf<T>>>,
    middle: Option<FallibleBox<RadixMiddle<T>>>,
    top: Option<FallibleBox<RadixTop<T>>>,
}

impl<T: Copy + Default> PendingReclaim<T> {
    fn take_one_admission(&mut self) -> Option<M11ParserScratchAdmission> {
        if let Some(mut page) = self.page.take() {
            let admission = page
                .admission
                .take()
                .expect("live data page retains exact admission");
            drop(page);
            return Some(admission);
        }
        if let Some(mut leaf) = self.leaf.take() {
            let admission = leaf
                .admission
                .take()
                .expect("empty leaf retains exact admission");
            drop(leaf);
            return Some(admission);
        }
        if let Some(mut middle) = self.middle.take() {
            let admission = middle
                .admission
                .take()
                .expect("empty middle retains exact admission");
            drop(middle);
            return Some(admission);
        }
        if let Some(mut top) = self.top.take() {
            let admission = top
                .admission
                .take()
                .expect("empty top retains exact admission");
            drop(top);
            return Some(admission);
        }
        None
    }

    const fn is_empty(&self) -> bool {
        self.page.is_none() && self.leaf.is_none() && self.middle.is_none() && self.top.is_none()
    }
}

/// Sparse four-level pages for one exact source-bound inline job.
pub(crate) struct M11InlineRadixPages<T: Copy + Default, const PAGE_RECORDS: usize> {
    source: SourceVersion,
    roots: [Option<FallibleBox<RadixTop<T>>>; RADIX],
    last_allocated_page: Option<usize>,
    retained_allocations: usize,
    retained_admitted_bytes: usize,
    reclaiming: bool,
    pending_reclaim: Option<PendingReclaim<T>>,
    pending_release: Option<M11ParserScratchAdmission>,
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> fmt::Debug
    for M11InlineRadixPages<T, PAGE_RECORDS>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineRadixPages")
            .field("source", &self.source)
            .field("page_records", &PAGE_RECORDS)
            .field("last_allocated_page", &self.last_allocated_page)
            .field("retained_allocations", &self.retained_allocations)
            .field("retained_admitted_bytes", &self.retained_admitted_bytes)
            .field("reclaiming", &self.reclaiming)
            .finish_non_exhaustive()
    }
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> M11InlineRadixPages<T, PAGE_RECORDS> {
    pub(crate) fn new(source: SourceVersion) -> Result<Self, M11InlineRadixError> {
        if PAGE_RECORDS == 0 {
            return Err(M11InlineRadixError::ZeroPageRecords);
        }
        if size_of::<T>() == 0 {
            return Err(M11InlineRadixError::ZeroSizedRecord);
        }
        let record_bytes = size_of::<T>()
            .checked_mul(PAGE_RECORDS)
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        if record_bytes > M11_INLINE_RADIX_DATA_PAGE_MAX_BYTES {
            return Err(M11InlineRadixError::RecordPageTooLarge {
                bytes: record_bytes,
                cap: M11_INLINE_RADIX_DATA_PAGE_MAX_BYTES,
            });
        }
        Ok(Self {
            source,
            roots: array::from_fn(|_| None),
            last_allocated_page: None,
            retained_allocations: 0,
            retained_admitted_bytes: 0,
            reclaiming: false,
            pending_reclaim: None,
            pending_release: None,
        })
    }

    pub(crate) fn set(
        &mut self,
        runtime: &mut DocumentRuntime,
        record: usize,
        value: T,
    ) -> Result<M11InlineRadixAllocationReceipt, M11InlineRadixError> {
        if self.reclaiming {
            return Err(M11InlineRadixError::ReclaimAlreadyStarted);
        }
        if self.pending_release.is_some() {
            return Err(M11InlineRadixError::InvalidState);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ParserScratchError::SourceAuthorityMismatch.into());
        }
        let (page, slot, coordinates) = Self::coordinates(record)?;
        let plan = self.allocation_plan(coordinates);
        let receipt = if plan.page {
            self.install_page(runtime, page, coordinates, plan)?
        } else {
            M11InlineRadixAllocationReceipt::default()
        };
        self.page_mut(coordinates)
            .expect("successful page installation leaves a live page")
            .records[slot] = value;
        Ok(receipt)
    }

    /// Reads one slot if its containing page exists.
    ///
    /// `None` means the page has never been allocated. Once any record creates
    /// a page, every other unset slot in that page returns
    /// `Some(T::default())`; this radix deliberately carries no occupancy
    /// bitmap. Inline scratch schemas must reserve their default value as the
    /// absent/sentinel representation or track logical length separately.
    pub(crate) fn get(&self, record: usize) -> Result<Option<T>, M11InlineRadixError> {
        if self.reclaiming {
            return Err(M11InlineRadixError::ReclaimAlreadyStarted);
        }
        let (_, slot, coordinates) = Self::coordinates(record)?;
        Ok(self.page(coordinates).map(|page| page.records[slot]))
    }

    #[cfg(test)]
    pub(crate) const fn retained_allocations(&self) -> usize {
        self.retained_allocations
    }

    #[cfg(test)]
    pub(crate) const fn retained_admitted_bytes(&self) -> usize {
        self.retained_admitted_bytes
    }

    pub(crate) fn begin_reclaim(&mut self) -> Result<(), M11InlineRadixError> {
        if self.reclaiming {
            return Err(M11InlineRadixError::ReclaimAlreadyStarted);
        }
        self.reclaiming = true;
        Ok(())
    }

    pub(crate) fn poll_reclaim(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineRadixReclaimPoll, M11InlineRadixError> {
        if !self.reclaiming {
            return Err(M11InlineRadixError::ReclaimNotStarted);
        }
        if fuel == 0 {
            return Err(M11InlineRadixError::ZeroFuel);
        }
        if fuel > M11_INLINE_RADIX_MAX_POLL_TRANSITIONS {
            return Err(M11InlineRadixError::PollLimitExceeded);
        }

        let mut poll = M11InlineRadixReclaimPoll::default();
        while poll.transitions < fuel {
            if let Some(admission) = self.pending_release.take() {
                self.release_one(runtime, admission, &mut poll)?;
                continue;
            }

            if let Some(pending) = self.pending_reclaim.as_mut() {
                if let Some(admission) = pending.take_one_admission() {
                    self.release_one(runtime, admission, &mut poll)?;
                    continue;
                }
                debug_assert!(pending.is_empty());
                self.pending_reclaim = None;
                continue;
            }

            if self.last_allocated_page.is_some() {
                self.pending_reclaim = Some(self.detach_last_page()?);
                poll.transitions += 1;
                continue;
            }

            poll.complete = true;
            return Ok(poll);
        }
        poll.complete = self.last_allocated_page.is_none()
            && self.pending_reclaim.is_none()
            && self.pending_release.is_none();
        Ok(poll)
    }

    fn release_one(
        &mut self,
        runtime: &mut DocumentRuntime,
        admission: M11ParserScratchAdmission,
        poll: &mut M11InlineRadixReclaimPoll,
    ) -> Result<(), M11InlineRadixError> {
        let bytes = admission.bytes();
        match runtime.release_parser_scratch(admission) {
            Ok(()) => {
                self.retained_allocations = self
                    .retained_allocations
                    .checked_sub(1)
                    .ok_or(M11InlineRadixError::InvalidState)?;
                self.retained_admitted_bytes = self
                    .retained_admitted_bytes
                    .checked_sub(bytes)
                    .ok_or(M11InlineRadixError::InvalidState)?;
                poll.transitions += 1;
                poll.allocations_reclaimed += 1;
                poll.admitted_bytes_reclaimed = poll
                    .admitted_bytes_reclaimed
                    .checked_add(bytes)
                    .ok_or(M11InlineRadixError::AddressExhausted)?;
                Ok(())
            }
            Err(failure) => {
                let error = failure.error();
                self.pending_release = Some(failure.into_admission());
                Err(error.into())
            }
        }
    }

    fn coordinates(
        record: usize,
    ) -> Result<(usize, usize, [usize; RADIX_LEVELS]), M11InlineRadixError> {
        if PAGE_RECORDS == 0 {
            return Err(M11InlineRadixError::ZeroPageRecords);
        }
        let page = record / PAGE_RECORDS;
        let slot = record % PAGE_RECORDS;
        Ok((page, slot, Self::page_coordinates(page)?))
    }

    fn page_coordinates(page: usize) -> Result<[usize; RADIX_LEVELS], M11InlineRadixError> {
        let capacity_pages = 1usize
            .checked_shl((RADIX_BITS * RADIX_LEVELS) as u32)
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        if page >= capacity_pages {
            return Err(M11InlineRadixError::AddressExhausted);
        }
        Ok([
            (page >> (RADIX_BITS * 3)) & RADIX_MASK,
            (page >> (RADIX_BITS * 2)) & RADIX_MASK,
            (page >> RADIX_BITS) & RADIX_MASK,
            page & RADIX_MASK,
        ])
    }

    fn allocation_plan(&self, [a, b, c, d]: [usize; RADIX_LEVELS]) -> AllocationPlan {
        let Some(top) = self.roots[a].as_ref() else {
            return AllocationPlan {
                top: true,
                middle: true,
                leaf: true,
                page: true,
            };
        };
        let Some(middle) = top.children[b].as_ref() else {
            return AllocationPlan {
                middle: true,
                leaf: true,
                page: true,
                ..AllocationPlan::default()
            };
        };
        let Some(leaf) = middle.children[c].as_ref() else {
            return AllocationPlan {
                leaf: true,
                page: true,
                ..AllocationPlan::default()
            };
        };
        AllocationPlan {
            page: leaf.pages[d].is_none(),
            ..AllocationPlan::default()
        }
    }

    fn install_page(
        &mut self,
        runtime: &mut DocumentRuntime,
        page_index: usize,
        [a, b, c, d]: [usize; RADIX_LEVELS],
        plan: AllocationPlan,
    ) -> Result<M11InlineRadixAllocationReceipt, M11InlineRadixError> {
        if !plan.page {
            return Err(M11InlineRadixError::InvalidState);
        }

        let admitted_sizes = [
            usize::from(plan.top) * size_of::<RadixTop<T>>(),
            usize::from(plan.middle) * size_of::<RadixMiddle<T>>(),
            usize::from(plan.leaf) * size_of::<RadixLeaf<T>>(),
            size_of::<T>()
                .checked_mul(PAGE_RECORDS)
                .ok_or(M11InlineRadixError::AddressExhausted)?,
        ];
        let admitted_bytes = admitted_sizes
            .iter()
            .try_fold(0usize, |sum, size| sum.checked_add(*size))
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        let allocations = plan.allocations();
        let next_allocations = self
            .retained_allocations
            .checked_add(allocations)
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        let next_admitted_bytes = self
            .retained_admitted_bytes
            .checked_add(admitted_bytes)
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        let retained_with_unsplit_bundle = self
            .retained_allocations
            .checked_add(1)
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        let admitted_with_unsplit_bundle = self
            .retained_admitted_bytes
            .checked_add(admitted_bytes)
            .ok_or(M11InlineRadixError::AddressExhausted)?;
        // Admission precedes actual residency. It remains one recoverable
        // token until every fallible reserve succeeds, so failure can drop
        // empty Vec capacity and release the unsplit charge atomically.
        let mut bundle = Some(runtime.try_admit_parser_scratch(self.source, admitted_bytes)?);
        let mut prepared = match PreparedAllocations::<T>::new::<PAGE_RECORDS>(plan) {
            Ok(prepared) => prepared,
            Err(error) => {
                let admission = bundle.take().expect("unsplit admission remains live");
                match runtime.release_parser_scratch(admission) {
                    Ok(()) => return Err(error),
                    Err(failure) => {
                        let release_error = failure.error();
                        let admission = failure.into_admission();
                        debug_assert_eq!(admission.bytes(), admitted_bytes);
                        self.retained_allocations = retained_with_unsplit_bundle;
                        self.retained_admitted_bytes = admitted_with_unsplit_bundle;
                        self.pending_release = Some(admission);
                        return Err(release_error.into());
                    }
                }
            }
        };
        let retained_sizes = [
            prepared.top.as_ref().map_or(0, |allocation| {
                allocation
                    .capacity()
                    .saturating_mul(size_of::<RadixTop<T>>())
            }),
            prepared.middle.as_ref().map_or(0, |allocation| {
                allocation
                    .capacity()
                    .saturating_mul(size_of::<RadixMiddle<T>>())
            }),
            prepared.leaf.as_ref().map_or(0, |allocation| {
                allocation
                    .capacity()
                    .saturating_mul(size_of::<RadixLeaf<T>>())
            }),
            prepared.records.capacity().saturating_mul(size_of::<T>()),
        ];
        let retained_bytes = retained_sizes
            .iter()
            .fold(0usize, |sum, size| sum.saturating_add(*size));
        if retained_sizes != admitted_sizes {
            drop(prepared);
            let mismatch = M11InlineRadixError::AllocationCapacityMismatch {
                admitted: admitted_bytes,
                retained: retained_bytes,
            };
            let admission = bundle.take().expect("unsplit admission remains live");
            match runtime.release_parser_scratch(admission) {
                Ok(()) => return Err(mismatch),
                Err(failure) => {
                    let release_error = failure.error();
                    let admission = failure.into_admission();
                    debug_assert_eq!(admission.bytes(), admitted_bytes);
                    self.retained_allocations = retained_with_unsplit_bundle;
                    self.retained_admitted_bytes = admitted_with_unsplit_bundle;
                    self.pending_release = Some(admission);
                    return Err(release_error.into());
                }
            }
        }
        let mut tokens: [Option<M11ParserScratchAdmission>; RADIX_LEVELS] =
            array::from_fn(|_| None);
        let mut remaining = allocations;
        for (index, size) in admitted_sizes.into_iter().enumerate() {
            if size == 0 {
                continue;
            }
            remaining -= 1;
            tokens[index] = Some(if remaining == 0 {
                bundle.take().expect("last radix allocation owns remainder")
            } else {
                bundle
                    .as_mut()
                    .expect("radix admission bundle remains live")
                    .split_prefix(size)
                    .expect("radix allocation size is a strict admitted prefix")
            });
        }
        debug_assert!(bundle.is_none());

        // No operation below this line allocates or returns a recoverable
        // error. Values are pushed into already-reserved capacity.
        let new_top = plan.top.then(|| {
            FallibleBox::from_prepared(
                prepared.top.take().expect("prepared top"),
                RadixTop::new(tokens[0].take().expect("top admission")),
            )
        });
        let new_middle = plan.middle.then(|| {
            FallibleBox::from_prepared(
                prepared.middle.take().expect("prepared middle"),
                RadixMiddle::new(tokens[1].take().expect("middle admission")),
            )
        });
        let new_leaf = plan.leaf.then(|| {
            FallibleBox::from_prepared(
                prepared.leaf.take().expect("prepared leaf"),
                RadixLeaf::new(tokens[2].take().expect("leaf admission")),
            )
        });
        prepared.records.resize(PAGE_RECORDS, T::default());
        let new_page = RadixDataPage {
            admission: Some(tokens[3].take().expect("record admission")),
            previous_allocated_page: self.last_allocated_page,
            records: prepared.records,
        };

        if let Some(top) = new_top {
            self.roots[a] = Some(top);
        }
        let top = self.roots[a]
            .as_mut()
            .expect("top exists after prepared installation");
        if let Some(middle) = new_middle {
            top.children[b] = Some(middle);
            top.live_children += 1;
        }
        let middle = top.children[b]
            .as_mut()
            .expect("middle exists after prepared installation");
        if let Some(leaf) = new_leaf {
            middle.children[c] = Some(leaf);
            middle.live_children += 1;
        }
        let leaf = middle.children[c]
            .as_mut()
            .expect("leaf exists after prepared installation");
        leaf.pages[d] = Some(new_page);
        leaf.live_pages += 1;
        self.last_allocated_page = Some(page_index);
        self.retained_allocations = next_allocations;
        self.retained_admitted_bytes = next_admitted_bytes;
        Ok(M11InlineRadixAllocationReceipt {
            allocations,
            admitted_bytes,
        })
    }

    fn page(&self, [a, b, c, d]: [usize; RADIX_LEVELS]) -> Option<&RadixDataPage<T>> {
        self.roots[a].as_ref()?.children[b].as_ref()?.children[c]
            .as_ref()?
            .pages[d]
            .as_ref()
    }

    fn page_mut(&mut self, [a, b, c, d]: [usize; RADIX_LEVELS]) -> Option<&mut RadixDataPage<T>> {
        self.roots[a].as_mut()?.children[b].as_mut()?.children[c]
            .as_mut()?
            .pages[d]
            .as_mut()
    }

    fn detach_last_page(&mut self) -> Result<PendingReclaim<T>, M11InlineRadixError> {
        let page_index = self
            .last_allocated_page
            .ok_or(M11InlineRadixError::InvalidState)?;
        let [a, b, c, d] = Self::page_coordinates(page_index)?;
        let (page, remove_leaf) = {
            let top = self.roots[a]
                .as_mut()
                .ok_or(M11InlineRadixError::InvalidState)?;
            let middle = top.children[b]
                .as_mut()
                .ok_or(M11InlineRadixError::InvalidState)?;
            let leaf = middle.children[c]
                .as_mut()
                .ok_or(M11InlineRadixError::InvalidState)?;
            let page = leaf.pages[d]
                .take()
                .ok_or(M11InlineRadixError::InvalidState)?;
            leaf.live_pages = leaf
                .live_pages
                .checked_sub(1)
                .ok_or(M11InlineRadixError::InvalidState)?;
            let remove_leaf = leaf.live_pages == 0;
            (page, remove_leaf)
        };
        self.last_allocated_page = page.previous_allocated_page;

        let mut leaf = None;
        let mut middle = None;
        let mut top = None;
        let mut remove_middle = false;
        if remove_leaf {
            let top_ref = self.roots[a]
                .as_mut()
                .ok_or(M11InlineRadixError::InvalidState)?;
            let middle_ref = top_ref.children[b]
                .as_mut()
                .ok_or(M11InlineRadixError::InvalidState)?;
            leaf = middle_ref.children[c].take();
            middle_ref.live_children = middle_ref
                .live_children
                .checked_sub(1)
                .ok_or(M11InlineRadixError::InvalidState)?;
            remove_middle = middle_ref.live_children == 0;
        }
        let mut remove_top = false;
        if remove_middle {
            let top_ref = self.roots[a]
                .as_mut()
                .ok_or(M11InlineRadixError::InvalidState)?;
            middle = top_ref.children[b].take();
            top_ref.live_children = top_ref
                .live_children
                .checked_sub(1)
                .ok_or(M11InlineRadixError::InvalidState)?;
            remove_top = top_ref.live_children == 0;
        }
        if remove_top {
            top = self.roots[a].take();
        }
        Ok(PendingReclaim {
            page: Some(page),
            leaf,
            middle,
            top,
        })
    }
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> Drop for M11InlineRadixPages<T, PAGE_RECORDS> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.retained_allocations == 0
                    && self.retained_admitted_bytes == 0
                    && self.last_allocated_page.is_none()
                    && self.pending_reclaim.is_none()
                    && self.pending_release.is_none()
                    && self.roots.iter().all(Option::is_none),
                "inline radix pages require explicit fuelled reclamation"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flark_engine::{ArenaLimits, DocumentRuntimeConfig};

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close").complete {}
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
    }

    fn reclaim<T: Copy + Default, const PAGE_RECORDS: usize>(
        pages: &mut M11InlineRadixPages<T, PAGE_RECORDS>,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> (usize, usize) {
        pages.begin_reclaim().expect("begin reclaim");
        let mut allocations = 0;
        let mut admitted_bytes = 0;
        loop {
            let poll = pages.poll_reclaim(runtime, fuel).expect("reclaim");
            assert!(poll.transitions() <= fuel);
            allocations += poll.allocations_reclaimed();
            admitted_bytes += poll.admitted_bytes_reclaimed();
            if poll.complete() {
                return (allocations, admitted_bytes);
            }
        }
    }

    #[test]
    fn sparse_pages_share_directories_and_reclaim_under_fuel_one() {
        let mut runtime =
            DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut pages = M11InlineRadixPages::<u32, 1_000>::new(source).expect("pages");

        let first = pages.set(&mut runtime, 0, 11).expect("first page");
        assert_eq!(first.allocations(), 4);
        assert!(first.admitted_bytes() > 4_000);
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            first.admitted_bytes()
        );
        assert_eq!(
            pages.set(&mut runtime, 17, 12).expect("same page"),
            M11InlineRadixAllocationReceipt::default()
        );
        assert_eq!(
            pages.get(18).expect("allocated default slot"),
            Some(u32::default())
        );
        let second = pages.set(&mut runtime, 1_000, 21).expect("second page");
        assert_eq!(second.allocations(), 1);
        let distant = pages
            .set(&mut runtime, 1_000 * RADIX * RADIX, 31)
            .expect("distant page");
        assert!(distant.allocations() >= 2);
        assert_eq!(pages.get(0).expect("get"), Some(11));
        assert_eq!(pages.get(17).expect("get"), Some(12));
        assert_eq!(pages.get(1_000).expect("get"), Some(21));
        assert_eq!(
            pages.get(1_000 * RADIX * RADIX).expect("get distant"),
            Some(31)
        );
        assert_eq!(
            runtime.arena_metrics().reserved_external_payload_bytes,
            pages.retained_admitted_bytes()
        );

        let expected_allocations = pages.retained_allocations();
        let expected_admitted_bytes = pages.retained_admitted_bytes();
        let (allocations, admitted_bytes) = reclaim(&mut pages, &mut runtime, 1);
        assert_eq!(allocations, expected_allocations);
        assert_eq!(admitted_bytes, expected_admitted_bytes);
        assert_eq!(pages.retained_allocations(), 0);
        assert_eq!(pages.retained_admitted_bytes(), 0);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        drop(pages);
        close_runtime(runtime);
    }

    #[test]
    fn failed_bundle_admission_is_atomic_and_leaves_no_directory() {
        let config = DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_live_payload_bytes: 1_024,
                ..ArenaLimits::default()
            },
            ..DocumentRuntimeConfig::default()
        };
        let mut runtime = DocumentRuntime::new("source", config).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut pages = M11InlineRadixPages::<u64, 512>::new(source).expect("pages");
        let error = pages
            .set(&mut runtime, 0, 1)
            .expect_err("bundle exceeds budget");
        assert!(matches!(
            error,
            M11InlineRadixError::Scratch(error) if error.is_resource_limit()
        ));
        assert_eq!(pages.retained_allocations(), 0);
        assert_eq!(pages.retained_admitted_bytes(), 0);
        assert_eq!(pages.get(0).expect("empty get"), None);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);

        let (allocations, admitted_bytes) = reclaim(&mut pages, &mut runtime, 1);
        assert_eq!((allocations, admitted_bytes), (0, 0));
        drop(pages);
        close_runtime(runtime);
    }

    #[test]
    fn failed_cross_runtime_release_preserves_admission_for_retry() {
        let mut owner =
            DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("owner");
        let source = owner.current_source_version().expect("source");
        let mut pages = M11InlineRadixPages::<u32, 1_000>::new(source).expect("pages");
        pages.set(&mut owner, 0, 7).expect("page");
        let admitted = pages.retained_admitted_bytes();
        pages.begin_reclaim().expect("begin reclaim");
        let first = pages.poll_reclaim(&mut owner, 1).expect("detach");
        assert_eq!(first.transitions(), 1);

        let mut foreign =
            DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("foreign");
        assert!(matches!(
            pages.poll_reclaim(&mut foreign, 1),
            Err(M11InlineRadixError::Scratch(
                M11ParserScratchError::WrongRuntime
            ))
        ));
        assert_eq!(
            owner.arena_metrics().reserved_external_payload_bytes,
            admitted
        );
        assert_eq!(foreign.arena_metrics().reserved_external_payload_bytes, 0);

        loop {
            if pages.poll_reclaim(&mut owner, 1).expect("retry").complete() {
                break;
            }
        }
        drop(pages);
        close_runtime(foreign);
        close_runtime(owner);
    }

    #[test]
    fn reads_are_rejected_once_progressive_reclamation_begins() {
        let mut runtime =
            DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut pages = M11InlineRadixPages::<u32, 1_000>::new(source).expect("pages");
        pages.set(&mut runtime, 0, 7).expect("page");
        pages.begin_reclaim().expect("begin reclaim");
        assert!(matches!(
            pages.get(0),
            Err(M11InlineRadixError::ReclaimAlreadyStarted)
        ));
        loop {
            if pages
                .poll_reclaim(&mut runtime, 1)
                .expect("reclaim")
                .complete()
            {
                break;
            }
        }
        drop(pages);
        close_runtime(runtime);
    }

    #[test]
    fn page_coordinate_recovery_does_not_multiply_large_record_indices() {
        let mut runtime =
            DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let mut pages = M11InlineRadixPages::<u8, 1>::new(source).expect("pages");
        let final_page = (1usize << (RADIX_BITS * RADIX_LEVELS)) - 1;
        pages
            .set(&mut runtime, final_page, 9)
            .expect("last addressable page");
        assert_eq!(pages.get(final_page).expect("get"), Some(9));
        let (allocations, _) = reclaim(&mut pages, &mut runtime, 1);
        assert_eq!(allocations, 4);
        drop(pages);
        close_runtime(runtime);
    }

    #[test]
    fn oversized_record_page_is_rejected_before_admission_or_mutation() {
        let runtime =
            DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let error = M11InlineRadixPages::<u64, 513>::new(source)
            .expect_err("record page exceeds fixed quantum");
        assert!(matches!(
            error,
            M11InlineRadixError::RecordPageTooLarge {
                bytes: 4_104,
                cap: M11_INLINE_RADIX_DATA_PAGE_MAX_BYTES,
            }
        ));
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }
}
