//! Checkpoint-free local deltas for one admitted tight list.
//!
//! The admitted subset has one physical line per item, homogeneous markers,
//! no continuation blocks, and at most one terminal empty item. Source-native
//! line rank/select therefore supplies the absolute item ordinal without a
//! parser checkpoint vector. One unchanged predecessor reconstructs the only
//! cross-item state (content indent), while one unchanged successor proves
//! convergence after a bounded base/target reparse. Bullet and ordered lists
//! share this machinery; only their parser-certified marker facts differ.

use std::marker::PhantomData;
use std::ops::Range;

use comrak::block_spine_facade::FacadeError;
use flark_engine::{
    ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness, SourceBoundaryAffinity,
    SourceSnapshotLease, SourceVersion,
};

use crate::contract::{M11PhysicalLineFacts, M11SourceLineSource};
use crate::exact_clean::{
    M11BulletListItemMapping, M11CleanBlockController, M11CleanControllerFault,
    M11ListUnsupportedReason, M11OrderedListItemMapping, M11ParserBinding, SourceCut,
    M11_GRAMMAR_REVISION,
};
use crate::publication::{
    M11PublishedBulletListLeafFence, M11PublishedOrderedListLeafFence,
    PublishedBulletListProjectionAuthority, PublishedOrderedListProjectionAuthority,
};
use crate::segmented_lexical::{SegmentedLineFacts, SegmentedLineScanner, SegmentedListMarker};
use crate::source_adapter::{
    SnapshotLineRetainedPoll, SnapshotLineScanner, SnapshotLineSource, SnapshotPhysicalLine,
    SourceAdapterError,
};

pub const M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES: usize = 64 * 1024;
pub const M11_ORDERED_LIST_LOCAL_DELTA_MAX_BYTES: usize = M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11TightListLocalDeltaBoundaryFallback {
    FirstItem,
    LastItem,
}

#[derive(Debug)]
pub enum M11TightListLocalDeltaError {
    InvalidChangedRange,
    BoundaryFallback(M11TightListLocalDeltaBoundaryFallback),
    AuthorityMismatch,
    BindingMismatch,
    UnsupportedGrammarRevision { actual: u32 },
    WindowTooLarge { bytes: usize, cap: usize },
    UnsupportedList(M11ListUnsupportedReason),
    ConvergenceMismatch,
    MetricOverflow,
    AllocationFailed,
    InvalidState,
    Source(SourceAdapterError),
    Donor(FacadeError),
}

impl From<SourceAdapterError> for M11TightListLocalDeltaError {
    fn from(value: SourceAdapterError) -> Self {
        Self::Source(value)
    }
}

impl From<FacadeError> for M11TightListLocalDeltaError {
    fn from(value: FacadeError) -> Self {
        Self::Donor(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListSummary {
    source: Range<u32>,
    source_utf16: Range<u32>,
    entry_ordinal: u64,
    marker: TightListMarker,
    item_count: u32,
    paragraph_count: u32,
    terminal_empty_relative_start: Option<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TightListMarker {
    Bullet(u8),
    Ordered { start: u32, delimiter: u8 },
}

/// Type-level selector behind the source-compatible bullet and ordered APIs.
#[doc(hidden)]
pub trait M11TightListLocalDeltaFlavor {
    type Terminal;

    fn terminal(
        terminal: M11TightListLocalDeltaTerminalData,
    ) -> Result<Self::Terminal, M11TightListLocalDeltaError>;
}

#[doc(hidden)]
pub enum M11BulletListLocalDeltaFlavor {}

#[doc(hidden)]
pub enum M11OrderedListLocalDeltaFlavor {}

pub type M11BulletListLocalDeltaBoundaryFallback = M11TightListLocalDeltaBoundaryFallback;
pub type M11OrderedListLocalDeltaBoundaryFallback = M11TightListLocalDeltaBoundaryFallback;
pub type M11BulletListLocalDeltaError = M11TightListLocalDeltaError;
pub type M11OrderedListLocalDeltaError = M11TightListLocalDeltaError;
pub type M11BulletListLocalDeltaPlan = M11TightListLocalDeltaPlan<M11BulletListLocalDeltaFlavor>;
pub type M11OrderedListLocalDeltaPlan = M11TightListLocalDeltaPlan<M11OrderedListLocalDeltaFlavor>;
pub type M11BulletListLocalDeltaWork = M11TightListLocalDeltaWork;
pub type M11OrderedListLocalDeltaWork = M11TightListLocalDeltaWork;
pub type M11BulletListLocalDeltaResult =
    M11TightListLocalDeltaResult<M11BulletListLocalDeltaFlavor>;
pub type M11OrderedListLocalDeltaResult =
    M11TightListLocalDeltaResult<M11OrderedListLocalDeltaFlavor>;
pub type M11BulletListLocalDeltaPoll = M11TightListLocalDeltaPoll<M11BulletListLocalDeltaFlavor>;
pub type M11OrderedListLocalDeltaPoll = M11TightListLocalDeltaPoll<M11OrderedListLocalDeltaFlavor>;
pub type M11BulletListLocalDeltaCancellation =
    M11TightListLocalDeltaCancellation<M11BulletListLocalDeltaFlavor>;
pub type M11OrderedListLocalDeltaCancellation =
    M11TightListLocalDeltaCancellation<M11OrderedListLocalDeltaFlavor>;
pub type M11BulletListLocalDeltaJob = M11TightListLocalDeltaJob<M11BulletListLocalDeltaFlavor>;
pub type M11OrderedListLocalDeltaJob = M11TightListLocalDeltaJob<M11OrderedListLocalDeltaFlavor>;

/// Move-only exact base list authority plus one source-native local window.
#[must_use = "a local-delta plan must be consumed or deliberately dropped"]
pub struct M11TightListLocalDeltaPlan<F> {
    source: SourceVersion,
    binding: M11ParserBinding,
    summary: ListSummary,
    base_source: Option<SourceSnapshotLease>,
    base_window: Range<usize>,
    base_window_utf16: Range<usize>,
    predecessor_source: Range<usize>,
    predecessor_source_utf16: Range<usize>,
    successor_source: Range<usize>,
    successor_source_utf16: Range<usize>,
    predecessor_item_ordinal: u32,
    flavor: PhantomData<F>,
}

impl M11TightListLocalDeltaPlan<M11BulletListLocalDeltaFlavor> {
    pub fn new(
        runtime: &flark_engine::DocumentRuntime,
        fence: M11PublishedBulletListLeafFence,
        changed_base_bytes: Range<usize>,
    ) -> Result<Self, M11TightListLocalDeltaError> {
        let fenced: PublishedBulletListProjectionAuthority = fence.into_projection_authority();
        if fenced.binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(M11TightListLocalDeltaError::UnsupportedGrammarRevision {
                actual: fenced.binding.grammar_revision(),
            });
        }
        fenced
            .authority
            .validate(runtime)
            .map_err(|_| M11TightListLocalDeltaError::AuthorityMismatch)?;
        let block = fenced.block_source.start as usize..fenced.block_source.end as usize;
        if fenced.authority.source() != fenced.source
            || fenced.authority.source_range() != block
            || changed_base_bytes.start > changed_base_bytes.end
            || changed_base_bytes.start < block.start
            || changed_base_bytes.end > block.end
            || (changed_base_bytes.is_empty() && changed_base_bytes.start == block.end)
        {
            return Err(M11TightListLocalDeltaError::InvalidChangedRange);
        }
        let lease = fenced.authority.into_source_lease();
        if lease.version() != fenced.source {
            return Err(M11TightListLocalDeltaError::AuthorityMismatch);
        }
        let first_list_line = locate_line(&lease, block.start, SourceBoundaryAffinity::After)?;
        let first_changed = locate_line(
            &lease,
            changed_base_bytes.start,
            SourceBoundaryAffinity::After,
        )?;
        let last_changed = if changed_base_bytes.is_empty() {
            locate_line(
                &lease,
                changed_base_bytes.start,
                SourceBoundaryAffinity::After,
            )?
        } else {
            locate_line(
                &lease,
                changed_base_bytes.end,
                SourceBoundaryAffinity::Before,
            )?
        };
        let first_changed_range = first_changed.byte_range();
        let last_changed_range = last_changed.byte_range();
        if first_list_line.byte_range().start != block.start
            || first_changed_range.start < block.start
            || last_changed_range.end > block.end
            || first_changed.ordinal() > last_changed.ordinal()
        {
            return Err(M11TightListLocalDeltaError::InvalidChangedRange);
        }
        if first_changed.ordinal() == first_list_line.ordinal() {
            return Err(M11TightListLocalDeltaError::BoundaryFallback(
                M11TightListLocalDeltaBoundaryFallback::FirstItem,
            ));
        }
        let last_changed_item_ordinal = last_changed
            .ordinal()
            .checked_sub(first_list_line.ordinal())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        if last_changed_item_ordinal.checked_add(1) == Some(fenced.item_count) {
            return Err(M11TightListLocalDeltaError::BoundaryFallback(
                M11TightListLocalDeltaBoundaryFallback::LastItem,
            ));
        }
        let predecessor = locate_line(
            &lease,
            first_changed_range.start,
            SourceBoundaryAffinity::Before,
        )?;
        let successor = locate_line(
            &lease,
            last_changed_range.end,
            SourceBoundaryAffinity::After,
        )?;
        let predecessor_source = predecessor.byte_range();
        let successor_source = successor.byte_range();
        if predecessor.ordinal() < first_list_line.ordinal()
            || predecessor.ordinal().checked_add(1) != Some(first_changed.ordinal())
            || last_changed.ordinal().checked_add(1) != Some(successor.ordinal())
            || predecessor_source.start < block.start
            || successor_source.end > block.end
            || changed_base_bytes.start < predecessor_source.end
            || changed_base_bytes.end > successor_source.start
        {
            return Err(M11TightListLocalDeltaError::InvalidChangedRange);
        }
        let predecessor_item_ordinal = predecessor
            .ordinal()
            .checked_sub(first_list_line.ordinal())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let successor_item_ordinal = successor
            .ordinal()
            .checked_sub(first_list_line.ordinal())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        if successor_item_ordinal >= fenced.item_count {
            return Err(M11TightListLocalDeltaError::BoundaryFallback(
                M11TightListLocalDeltaBoundaryFallback::LastItem,
            ));
        }
        let predecessor_source_utf16 = utf16_range(&lease, &predecessor_source)?;
        let successor_source_utf16 = utf16_range(&lease, &successor_source)?;
        let base_window = predecessor_source.start..successor_source.end;
        if base_window.len() > M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES {
            return Err(M11TightListLocalDeltaError::WindowTooLarge {
                bytes: base_window.len(),
                cap: M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES,
            });
        }
        let base_window_utf16 = predecessor_source_utf16.start..successor_source_utf16.end;
        Ok(Self {
            source: fenced.source,
            binding: fenced.binding,
            summary: ListSummary {
                source: fenced.block_source,
                source_utf16: fenced.block_source_utf16,
                entry_ordinal: fenced.entry_ordinal,
                marker: TightListMarker::Bullet(fenced.marker),
                item_count: fenced.item_count,
                paragraph_count: fenced.paragraph_count,
                terminal_empty_relative_start: fenced.terminal_empty_relative_start,
                projected_utf8_length: fenced.projected_utf8_length,
                projected_utf16_length: fenced.projected_utf16_length,
            },
            base_source: Some(lease),
            base_window,
            base_window_utf16,
            predecessor_source,
            predecessor_source_utf16,
            successor_source,
            successor_source_utf16,
            predecessor_item_ordinal,
            flavor: PhantomData,
        })
    }
}

impl M11TightListLocalDeltaPlan<M11OrderedListLocalDeltaFlavor> {
    pub fn new(
        runtime: &flark_engine::DocumentRuntime,
        fence: M11PublishedOrderedListLeafFence,
        changed_base_bytes: Range<usize>,
    ) -> Result<Self, M11TightListLocalDeltaError> {
        let fenced: PublishedOrderedListProjectionAuthority = fence.into_projection_authority();
        if fenced.binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(M11TightListLocalDeltaError::UnsupportedGrammarRevision {
                actual: fenced.binding.grammar_revision(),
            });
        }
        fenced
            .authority
            .validate(runtime)
            .map_err(|_| M11TightListLocalDeltaError::AuthorityMismatch)?;
        let block = fenced.block_source.start as usize..fenced.block_source.end as usize;
        if fenced.authority.source() != fenced.source
            || fenced.authority.source_range() != block
            || changed_base_bytes.start > changed_base_bytes.end
            || changed_base_bytes.start < block.start
            || changed_base_bytes.end > block.end
            || (changed_base_bytes.is_empty() && changed_base_bytes.start == block.end)
        {
            return Err(M11TightListLocalDeltaError::InvalidChangedRange);
        }
        let lease = fenced.authority.into_source_lease();
        if lease.version() != fenced.source {
            return Err(M11TightListLocalDeltaError::AuthorityMismatch);
        }
        let first_list_line = locate_line(&lease, block.start, SourceBoundaryAffinity::After)?;
        let first_changed = locate_line(
            &lease,
            changed_base_bytes.start,
            SourceBoundaryAffinity::After,
        )?;
        let last_changed = if changed_base_bytes.is_empty() {
            locate_line(
                &lease,
                changed_base_bytes.start,
                SourceBoundaryAffinity::After,
            )?
        } else {
            locate_line(
                &lease,
                changed_base_bytes.end,
                SourceBoundaryAffinity::Before,
            )?
        };
        let first_changed_range = first_changed.byte_range();
        let last_changed_range = last_changed.byte_range();
        if first_list_line.byte_range().start != block.start
            || first_changed_range.start < block.start
            || last_changed_range.end > block.end
            || first_changed.ordinal() > last_changed.ordinal()
        {
            return Err(M11TightListLocalDeltaError::InvalidChangedRange);
        }
        if first_changed.ordinal() == first_list_line.ordinal() {
            return Err(M11TightListLocalDeltaError::BoundaryFallback(
                M11TightListLocalDeltaBoundaryFallback::FirstItem,
            ));
        }
        let last_changed_item_ordinal = last_changed
            .ordinal()
            .checked_sub(first_list_line.ordinal())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        if last_changed_item_ordinal.checked_add(1) == Some(fenced.item_count) {
            return Err(M11TightListLocalDeltaError::BoundaryFallback(
                M11TightListLocalDeltaBoundaryFallback::LastItem,
            ));
        }
        let predecessor = locate_line(
            &lease,
            first_changed_range.start,
            SourceBoundaryAffinity::Before,
        )?;
        let successor = locate_line(
            &lease,
            last_changed_range.end,
            SourceBoundaryAffinity::After,
        )?;
        let predecessor_source = predecessor.byte_range();
        let successor_source = successor.byte_range();
        if predecessor.ordinal() < first_list_line.ordinal()
            || predecessor.ordinal().checked_add(1) != Some(first_changed.ordinal())
            || last_changed.ordinal().checked_add(1) != Some(successor.ordinal())
            || predecessor_source.start < block.start
            || successor_source.end > block.end
            || changed_base_bytes.start < predecessor_source.end
            || changed_base_bytes.end > successor_source.start
        {
            return Err(M11TightListLocalDeltaError::InvalidChangedRange);
        }
        let predecessor_item_ordinal = predecessor
            .ordinal()
            .checked_sub(first_list_line.ordinal())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let successor_item_ordinal = successor
            .ordinal()
            .checked_sub(first_list_line.ordinal())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        if successor_item_ordinal >= fenced.item_count {
            return Err(M11TightListLocalDeltaError::BoundaryFallback(
                M11TightListLocalDeltaBoundaryFallback::LastItem,
            ));
        }
        let predecessor_source_utf16 = utf16_range(&lease, &predecessor_source)?;
        let successor_source_utf16 = utf16_range(&lease, &successor_source)?;
        let base_window = predecessor_source.start..successor_source.end;
        if base_window.len() > M11_ORDERED_LIST_LOCAL_DELTA_MAX_BYTES {
            return Err(M11TightListLocalDeltaError::WindowTooLarge {
                bytes: base_window.len(),
                cap: M11_ORDERED_LIST_LOCAL_DELTA_MAX_BYTES,
            });
        }
        let base_window_utf16 = predecessor_source_utf16.start..successor_source_utf16.end;
        Ok(Self {
            source: fenced.source,
            binding: fenced.binding,
            summary: ListSummary {
                source: fenced.block_source,
                source_utf16: fenced.block_source_utf16,
                entry_ordinal: fenced.entry_ordinal,
                marker: TightListMarker::Ordered {
                    start: fenced.start,
                    delimiter: fenced.delimiter,
                },
                item_count: fenced.item_count,
                paragraph_count: fenced.paragraph_count,
                terminal_empty_relative_start: fenced.terminal_empty_relative_start,
                projected_utf8_length: fenced.projected_utf8_length,
                projected_utf16_length: fenced.projected_utf16_length,
            },
            base_source: Some(lease),
            base_window,
            base_window_utf16,
            predecessor_source,
            predecessor_source_utf16,
            successor_source,
            successor_source_utf16,
            predecessor_item_ordinal,
            flavor: PhantomData,
        })
    }
}

impl<F> M11TightListLocalDeltaPlan<F> {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn binding(&self) -> M11ParserBinding {
        self.binding
    }

    #[must_use]
    pub const fn prefix_witness_byte_end(&self) -> usize {
        self.predecessor_source.end
    }

    #[must_use]
    pub const fn prefix_witness_utf16_end(&self) -> usize {
        self.predecessor_source_utf16.end
    }

    #[must_use]
    pub const fn suffix_witness_byte_start(&self) -> usize {
        self.successor_source.start
    }

    #[must_use]
    pub const fn suffix_witness_utf16_start(&self) -> usize {
        self.successor_source_utf16.start
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct M11TightListLocalDeltaTerminalData {
    pub source: SourceVersion,
    pub list_source: Range<u32>,
    pub list_source_utf16: Range<u32>,
    pub block_entry_ordinal: u64,
    marker: TightListMarker,
    pub item_count: u32,
    pub paragraph_count: u32,
    pub terminal_empty_relative_start: Option<u32>,
    pub projected_utf8_length: u32,
    pub projected_utf16_length: u32,
}

#[derive(Debug)]
#[non_exhaustive]
pub struct M11BulletListLocalDeltaTerminal {
    pub source: SourceVersion,
    pub list_source: Range<u32>,
    pub list_source_utf16: Range<u32>,
    pub block_entry_ordinal: u64,
    pub marker: u8,
    pub item_count: u32,
    pub paragraph_count: u32,
    pub terminal_empty_relative_start: Option<u32>,
    pub projected_utf8_length: u32,
    pub projected_utf16_length: u32,
}

#[derive(Debug)]
#[non_exhaustive]
pub struct M11OrderedListLocalDeltaTerminal {
    pub source: SourceVersion,
    pub list_source: Range<u32>,
    pub list_source_utf16: Range<u32>,
    pub block_entry_ordinal: u64,
    pub start: u32,
    pub delimiter: u8,
    pub item_count: u32,
    pub paragraph_count: u32,
    pub terminal_empty_relative_start: Option<u32>,
    pub projected_utf8_length: u32,
    pub projected_utf16_length: u32,
}

impl M11TightListLocalDeltaFlavor for M11BulletListLocalDeltaFlavor {
    type Terminal = M11BulletListLocalDeltaTerminal;

    fn terminal(
        terminal: M11TightListLocalDeltaTerminalData,
    ) -> Result<Self::Terminal, M11TightListLocalDeltaError> {
        let TightListMarker::Bullet(marker) = terminal.marker else {
            return Err(M11TightListLocalDeltaError::InvalidState);
        };
        Ok(M11BulletListLocalDeltaTerminal {
            source: terminal.source,
            list_source: terminal.list_source,
            list_source_utf16: terminal.list_source_utf16,
            block_entry_ordinal: terminal.block_entry_ordinal,
            marker,
            item_count: terminal.item_count,
            paragraph_count: terminal.paragraph_count,
            terminal_empty_relative_start: terminal.terminal_empty_relative_start,
            projected_utf8_length: terminal.projected_utf8_length,
            projected_utf16_length: terminal.projected_utf16_length,
        })
    }
}

impl M11TightListLocalDeltaFlavor for M11OrderedListLocalDeltaFlavor {
    type Terminal = M11OrderedListLocalDeltaTerminal;

    fn terminal(
        terminal: M11TightListLocalDeltaTerminalData,
    ) -> Result<Self::Terminal, M11TightListLocalDeltaError> {
        let TightListMarker::Ordered { start, delimiter } = terminal.marker else {
            return Err(M11TightListLocalDeltaError::InvalidState);
        };
        Ok(M11OrderedListLocalDeltaTerminal {
            source: terminal.source,
            list_source: terminal.list_source,
            list_source_utf16: terminal.list_source_utf16,
            block_entry_ordinal: terminal.block_entry_ordinal,
            start,
            delimiter,
            item_count: terminal.item_count,
            paragraph_count: terminal.paragraph_count,
            terminal_empty_relative_start: terminal.terminal_empty_relative_start,
            projected_utf8_length: terminal.projected_utf8_length,
            projected_utf16_length: terminal.projected_utf16_length,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct M11TightListLocalDeltaWork {
    pub base_window_bytes: usize,
    pub target_window_bytes: usize,
    pub base_source_bytes_discovered: usize,
    pub target_source_bytes_discovered: usize,
    pub base_source_bytes_read: usize,
    pub target_source_bytes_read: usize,
    pub base_physical_lines: usize,
    pub target_physical_lines: usize,
    pub poll_transitions: usize,
}

#[must_use = "the result retains exact base and target source authority"]
pub struct M11TightListLocalDeltaResult<F: M11TightListLocalDeltaFlavor> {
    terminal: F::Terminal,
    work: M11TightListLocalDeltaWork,
    base_plan: Option<M11TightListLocalDeltaPlan<F>>,
    target_source: Option<SourceSnapshotLease>,
}

impl<F: M11TightListLocalDeltaFlavor> M11TightListLocalDeltaResult<F> {
    #[must_use]
    pub const fn terminal(&self) -> &F::Terminal {
        &self.terminal
    }

    #[must_use]
    pub const fn work(&self) -> &M11TightListLocalDeltaWork {
        &self.work
    }

    /// Returns the original local-delta plan with its exact base lease restored.
    #[must_use]
    pub fn take_base_plan(&mut self) -> Option<M11TightListLocalDeltaPlan<F>> {
        self.base_plan.take()
    }

    #[must_use]
    pub fn take_target_source_lease(&mut self) -> Option<SourceSnapshotLease> {
        self.target_source.take()
    }
}

// The resumable local-delta job lives below. It parses each bounded window
// through the same list-item kernel while retaining only the first and latest
// item mappings needed to prove predecessor/successor convergence.

pub enum M11TightListLocalDeltaPoll<F: M11TightListLocalDeltaFlavor> {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: M11TightListLocalDeltaResult<F>,
    },
}

/// Exact authority recovered from a cancelled local-delta job.
#[must_use = "cancellation retains exact base and target source authority"]
pub struct M11TightListLocalDeltaCancellation<F> {
    base_plan: Option<M11TightListLocalDeltaPlan<F>>,
    target_source: Option<SourceSnapshotLease>,
}

impl<F> M11TightListLocalDeltaCancellation<F> {
    #[must_use]
    pub fn take_base_plan(&mut self) -> Option<M11TightListLocalDeltaPlan<F>> {
        self.base_plan.take()
    }

    #[must_use]
    pub fn take_target_source_lease(&mut self) -> Option<SourceSnapshotLease> {
        self.target_source.take()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalDeltaPhase {
    Base,
    Target,
    Finalize,
    Faulted,
    Complete,
    Cancelled,
}

/// Fuel-bounded checkpoint-free delta over one admitted bullet-list window.
#[must_use = "poll the local-delta job to completion or cancel it"]
pub struct M11TightListLocalDeltaJob<F: M11TightListLocalDeltaFlavor> {
    plan: Option<M11TightListLocalDeltaPlan<F>>,
    target_window: Range<usize>,
    target_successor_start: usize,
    target_successor_utf16_start: usize,
    base_parse: Option<WindowParseJob>,
    target_parse: Option<WindowParseJob>,
    base: Option<ParsedWindow>,
    target: Option<ParsedWindow>,
    phase: LocalDeltaPhase,
    total_transitions: usize,
}

impl<F: M11TightListLocalDeltaFlavor> M11TightListLocalDeltaJob<F> {
    /// Binds exact unchanged witnesses and two capped source windows.
    pub fn new(
        mut plan: M11TightListLocalDeltaPlan<F>,
        prefix: ExactUnchangedPrefixWitness,
        suffix: ExactUnchangedSuffixWitness,
        target: SourceSnapshotLease,
    ) -> Result<Self, M11TightListLocalDeltaError> {
        if prefix.base() != plan.source
            || suffix.base() != plan.source
            || prefix.target() != target.version()
            || suffix.target() != target.version()
            || prefix.byte_end() != plan.predecessor_source.end
            || prefix.utf16_end() != plan.predecessor_source_utf16.end
            || suffix.base_byte_start() != plan.successor_source.start
            || suffix.base_utf16_start() != plan.successor_source_utf16.start
        {
            return Err(M11TightListLocalDeltaError::AuthorityMismatch);
        }

        let target_successor_start = suffix.target_byte_start();
        let target_successor_utf16_start = suffix.target_utf16_start();
        let target_successor_end = target_successor_start
            .checked_add(plan.successor_source.len())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let target_successor_utf16_end = target_successor_utf16_start
            .checked_add(plan.successor_source_utf16.len())
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let target_window = plan.predecessor_source.start..target_successor_end;
        let target_window_utf16 = plan.predecessor_source_utf16.start..target_successor_utf16_end;
        if target_window.len() > M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES {
            return Err(M11TightListLocalDeltaError::WindowTooLarge {
                bytes: target_window.len(),
                cap: M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES,
            });
        }
        if target_window.end > target.version().byte_len()
            || target_window_utf16.end > target.version().utf16_len()
            || target.utf16_offset_for_byte(target_window.start).ok()
                != Some(target_window_utf16.start)
            || target.utf16_offset_for_byte(target_window.end).ok() != Some(target_window_utf16.end)
        {
            return Err(M11TightListLocalDeltaError::AuthorityMismatch);
        }

        let base_source = plan
            .base_source
            .take()
            .ok_or(M11TightListLocalDeltaError::AuthorityMismatch)?;
        let base_parse = WindowParseJob::new(
            base_source,
            plan.base_window.clone(),
            plan.base_window_utf16.start,
            plan.predecessor_item_ordinal,
            plan.summary.marker,
        )?;
        let target_parse = WindowParseJob::new(
            target,
            target_window.clone(),
            target_window_utf16.start,
            plan.predecessor_item_ordinal,
            plan.summary.marker,
        )?;
        Ok(Self {
            plan: Some(plan),
            target_window,
            target_successor_start,
            target_successor_utf16_start,
            base_parse: Some(base_parse),
            target_parse: Some(target_parse),
            base: None,
            target: None,
            phase: LocalDeltaPhase::Base,
            total_transitions: 0,
        })
    }

    /// Advances by at most `fuel` explicitly accounted transitions.
    ///
    /// Zero fuel is a non-mutating readiness probe and returns `Pending(0)`.
    pub fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<M11TightListLocalDeltaPoll<F>, M11TightListLocalDeltaError> {
        if matches!(
            self.phase,
            LocalDeltaPhase::Complete | LocalDeltaPhase::Cancelled
        ) {
            return Err(M11TightListLocalDeltaError::InvalidState);
        }
        if self.phase == LocalDeltaPhase::Faulted {
            return Err(M11TightListLocalDeltaError::InvalidState);
        }
        if fuel == 0 {
            return Ok(M11TightListLocalDeltaPoll::Pending { transitions: 0 });
        }
        match self.poll_active(fuel) {
            Ok(poll) => Ok(poll),
            Err(error) => {
                self.phase = LocalDeltaPhase::Faulted;
                Err(error)
            }
        }
    }

    fn poll_active(
        &mut self,
        fuel: usize,
    ) -> Result<M11TightListLocalDeltaPoll<F>, M11TightListLocalDeltaError> {
        let mut transitions = 0_usize;
        while transitions < fuel {
            match self.phase {
                LocalDeltaPhase::Base => {
                    let poll = self
                        .base_parse
                        .as_mut()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?
                        .poll(fuel - transitions)?;
                    let consumed = poll.transitions();
                    transitions = checked_add(transitions, consumed)?;
                    self.total_transitions = checked_add(self.total_transitions, consumed)?;
                    if !poll.complete() {
                        return Ok(M11TightListLocalDeltaPoll::Pending { transitions });
                    }
                    let mut parse = self
                        .base_parse
                        .take()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?;
                    self.base = Some(
                        parse
                            .take_completion()
                            .ok_or(M11TightListLocalDeltaError::InvalidState)?,
                    );
                    self.phase = LocalDeltaPhase::Target;
                }
                LocalDeltaPhase::Target => {
                    let poll = self
                        .target_parse
                        .as_mut()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?
                        .poll(fuel - transitions)?;
                    let consumed = poll.transitions();
                    transitions = checked_add(transitions, consumed)?;
                    self.total_transitions = checked_add(self.total_transitions, consumed)?;
                    if !poll.complete() {
                        return Ok(M11TightListLocalDeltaPoll::Pending { transitions });
                    }
                    let mut parse = self
                        .target_parse
                        .take()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?;
                    self.target = Some(
                        parse
                            .take_completion()
                            .ok_or(M11TightListLocalDeltaError::InvalidState)?,
                    );
                    self.phase = LocalDeltaPhase::Finalize;
                }
                LocalDeltaPhase::Finalize => {
                    transitions = checked_add(transitions, 1)?;
                    self.total_transitions = checked_add(self.total_transitions, 1)?;
                    let result = self.finish()?;
                    self.phase = LocalDeltaPhase::Complete;
                    return Ok(M11TightListLocalDeltaPoll::Complete {
                        transitions,
                        result,
                    });
                }
                LocalDeltaPhase::Faulted
                | LocalDeltaPhase::Complete
                | LocalDeltaPhase::Cancelled => {
                    return Err(M11TightListLocalDeltaError::InvalidState);
                }
            }
        }
        Ok(M11TightListLocalDeltaPoll::Pending { transitions })
    }

    fn finish(&mut self) -> Result<M11TightListLocalDeltaResult<F>, M11TightListLocalDeltaError> {
        let plan = self
            .plan
            .as_ref()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?;
        let base = self
            .base
            .as_ref()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?;
        let target = self
            .target
            .as_ref()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?;
        let base_predecessor = u32_range(&plan.predecessor_source)?;
        let base_successor = u32_range(&plan.successor_source)?;
        if base.item_count < 3
            || target.item_count < 2
            || !same_item_shape(&base.first, &target.first)
            || !same_item_shape(&base.last, &target.last)
            || base.first.source() != &base_predecessor
            || base.last.source() != &base_successor
            || target.first.source() != &base_predecessor
            || target.last.source().start as usize != self.target_successor_start
        {
            return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
        }

        let byte_delta = i64::try_from(self.target_successor_start)
            .map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?
            - i64::try_from(plan.successor_source.start)
                .map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?;
        let utf16_delta = i64::try_from(self.target_successor_utf16_start)
            .map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?
            - i64::try_from(plan.successor_source_utf16.start)
                .map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?;
        let item_delta = signed_delta(target.item_count, base.item_count)?;
        let paragraph_delta = i64::from(target.paragraph_count) - i64::from(base.paragraph_count);
        let projected_utf8_delta =
            i64::from(target.projected_utf8_length) - i64::from(base.projected_utf8_length);
        let projected_utf16_delta =
            i64::from(target.projected_utf16_length) - i64::from(base.projected_utf16_length);
        let list_source = plan.summary.source.start
            ..shift_u32(plan.summary.source.end, byte_delta)
                .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let list_source_utf16 = plan.summary.source_utf16.start
            ..shift_u32(plan.summary.source_utf16.end, utf16_delta)
                .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let terminal_empty_relative_start = match plan.summary.terminal_empty_relative_start {
            Some(relative) => {
                let absolute = plan
                    .summary
                    .source
                    .start
                    .checked_add(relative)
                    .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
                let shifted = if absolute >= plan.successor_source.start as u32 {
                    shift_u32(absolute, byte_delta)
                        .ok_or(M11TightListLocalDeltaError::MetricOverflow)?
                } else {
                    absolute
                };
                Some(
                    shifted
                        .checked_sub(plan.summary.source.start)
                        .ok_or(M11TightListLocalDeltaError::MetricOverflow)?,
                )
            }
            None => None,
        };
        let item_count = shift_u32(plan.summary.item_count, item_delta)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let paragraph_count = shift_u32(plan.summary.paragraph_count, paragraph_delta)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let projected_utf8_length =
            shift_u32(plan.summary.projected_utf8_length, projected_utf8_delta)
                .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let projected_utf16_length =
            shift_u32(plan.summary.projected_utf16_length, projected_utf16_delta)
                .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        if item_count == 0 || paragraph_count > item_count {
            return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
        }

        let terminal = F::terminal(M11TightListLocalDeltaTerminalData {
            source: target.source.version(),
            list_source,
            list_source_utf16,
            block_entry_ordinal: plan.summary.entry_ordinal,
            marker: plan.summary.marker,
            item_count,
            paragraph_count,
            terminal_empty_relative_start,
            projected_utf8_length,
            projected_utf16_length,
        })?;
        let work = M11TightListLocalDeltaWork {
            base_window_bytes: plan.base_window.len(),
            target_window_bytes: self.target_window.len(),
            base_source_bytes_discovered: base.source_bytes_discovered,
            target_source_bytes_discovered: target.source_bytes_discovered,
            base_source_bytes_read: base.source_bytes_read,
            target_source_bytes_read: target.source_bytes_read,
            base_physical_lines: base.item_count,
            target_physical_lines: target.item_count,
            poll_transitions: self.total_transitions,
        };

        let base_source = self
            .base
            .take()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?
            .source;
        let target_source = self
            .target
            .take()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?
            .source;
        let mut plan = self
            .plan
            .take()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?;
        plan.base_source = Some(base_source);
        Ok(M11TightListLocalDeltaResult {
            terminal,
            work,
            base_plan: Some(plan),
            target_source: Some(target_source),
        })
    }

    /// Stops immediately and restores the exact reusable base plan.
    pub fn cancel_into_source_authority(
        mut self,
    ) -> Result<M11TightListLocalDeltaCancellation<F>, M11TightListLocalDeltaError> {
        if matches!(
            self.phase,
            LocalDeltaPhase::Complete | LocalDeltaPhase::Cancelled
        ) {
            return Err(M11TightListLocalDeltaError::InvalidState);
        }
        let base_source = if let Some(base) = self.base.take() {
            Some(base.source)
        } else {
            self.base_parse
                .take()
                .and_then(WindowParseJob::cancel_into_source_lease)
        };
        let target_source = if let Some(target) = self.target.take() {
            Some(target.source)
        } else {
            self.target_parse
                .take()
                .and_then(WindowParseJob::cancel_into_source_lease)
        };
        let mut plan = self
            .plan
            .take()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?;
        plan.base_source = Some(base_source.ok_or(M11TightListLocalDeltaError::AuthorityMismatch)?);
        self.phase = LocalDeltaPhase::Cancelled;
        Ok(M11TightListLocalDeltaCancellation {
            base_plan: Some(plan),
            target_source,
        })
    }
}

#[derive(Clone, Copy)]
struct WindowParsePoll {
    transitions: usize,
    complete: bool,
}

impl WindowParsePoll {
    const fn transitions(self) -> usize {
        self.transitions
    }

    const fn complete(self) -> bool {
        self.complete
    }
}

struct ParsedWindow {
    source: SourceSnapshotLease,
    first: TightListItemMapping,
    last: TightListItemMapping,
    item_count: usize,
    paragraph_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    source_bytes_discovered: usize,
    source_bytes_read: usize,
}

enum TightListItemMapping {
    Bullet(M11BulletListItemMapping),
    Ordered(M11OrderedListItemMapping),
}

impl TightListItemMapping {
    const fn source(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.source,
            Self::Ordered(item) => &item.source,
        }
    }

    const fn source_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.source_utf16,
            Self::Ordered(item) => &item.source_utf16,
        }
    }

    const fn opening_marker(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.opening_marker,
            Self::Ordered(item) => &item.opening_marker,
        }
    }

    const fn hidden_prefix(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.hidden_prefix,
            Self::Ordered(item) => &item.hidden_prefix,
        }
    }

    const fn hidden_prefix_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.hidden_prefix_utf16,
            Self::Ordered(item) => &item.hidden_prefix_utf16,
        }
    }

    const fn continuation_prefix_source(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.continuation_prefix_source,
            Self::Ordered(item) => &item.continuation_prefix_source,
        }
    }

    const fn continuation_prefix_source_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.continuation_prefix_source_utf16,
            Self::Ordered(item) => &item.continuation_prefix_source_utf16,
        }
    }

    const fn content_source(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.content_source,
            Self::Ordered(item) => &item.content_source,
        }
    }

    const fn content_source_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.content_source_utf16,
            Self::Ordered(item) => &item.content_source_utf16,
        }
    }

    const fn line_ending(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.line_ending,
            Self::Ordered(item) => &item.line_ending,
        }
    }

    const fn line_ending_utf16(&self) -> &Range<u32> {
        match self {
            Self::Bullet(item) => &item.line_ending_utf16,
            Self::Ordered(item) => &item.line_ending_utf16,
        }
    }

    const fn has_paragraph(&self) -> bool {
        match self {
            Self::Bullet(item) => item.paragraph.is_some(),
            Self::Ordered(item) => item.paragraph.is_some(),
        }
    }
}

struct ActiveWindowLine {
    facts: M11PhysicalLineFacts,
    source: SnapshotLineSource,
    segmented: SegmentedLineScanner,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowParsePhase {
    Discover,
    Admit,
    Read,
    Commit,
    Finish,
    Complete,
    Cancelled,
}

struct WindowParseJob {
    source_version: SourceVersion,
    range: Range<usize>,
    marker: TightListMarker,
    scanner: Option<SnapshotLineScanner>,
    pending_line: Option<SnapshotPhysicalLine>,
    active: Option<ActiveWindowLine>,
    pending_commit: Option<(M11PhysicalLineFacts, SegmentedLineFacts)>,
    completion: Option<ParsedWindow>,
    phase: WindowParsePhase,
    first: Option<TightListItemMapping>,
    last: Option<TightListItemMapping>,
    item_count: usize,
    next_byte: usize,
    next_utf16: usize,
    next_ordinal: u32,
    current_content_indent: Option<usize>,
    previous_empty: bool,
    paragraph_count: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    source_bytes_discovered: usize,
    source_bytes_read: usize,
}

impl WindowParseJob {
    fn new(
        lease: SourceSnapshotLease,
        range: Range<usize>,
        start_utf16: usize,
        start_item_ordinal: u32,
        marker: TightListMarker,
    ) -> Result<Self, M11TightListLocalDeltaError> {
        if range.start >= range.end || range.len() > M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES {
            return Err(M11TightListLocalDeltaError::WindowTooLarge {
                bytes: range.len(),
                cap: M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES,
            });
        }
        let source_version = lease.version();
        let scanner = SnapshotLineScanner::new_in(lease, range.clone(), start_item_ordinal)?;
        Ok(Self {
            source_version,
            next_byte: range.start,
            next_utf16: start_utf16,
            next_ordinal: start_item_ordinal,
            range,
            marker,
            scanner: Some(scanner),
            pending_line: None,
            active: None,
            pending_commit: None,
            completion: None,
            phase: WindowParsePhase::Discover,
            first: None,
            last: None,
            item_count: 0,
            current_content_indent: None,
            previous_empty: false,
            paragraph_count: 0,
            projected_utf8_length: 0,
            projected_utf16_length: 0,
            source_bytes_discovered: 0,
            source_bytes_read: 0,
        })
    }

    fn poll(&mut self, fuel: usize) -> Result<WindowParsePoll, M11TightListLocalDeltaError> {
        if fuel == 0 {
            return Ok(WindowParsePoll {
                transitions: 0,
                complete: false,
            });
        }
        if matches!(
            self.phase,
            WindowParsePhase::Complete | WindowParsePhase::Cancelled
        ) {
            return Err(M11TightListLocalDeltaError::InvalidState);
        }

        let mut transitions = 0_usize;
        while transitions < fuel {
            match self.phase {
                WindowParsePhase::Discover => {
                    if self.next_byte == self.range.end {
                        self.phase = WindowParsePhase::Finish;
                        continue;
                    }
                    if self.next_byte > self.range.end {
                        return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
                    }
                    let scanner = self
                        .scanner
                        .take()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?;
                    let grant = fuel - transitions;
                    let (poll, inspected) = scanner.poll_counted_retaining_complete(grant)?;
                    if inspected > grant {
                        return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
                    }
                    transitions = checked_add(transitions, inspected)?;
                    self.source_bytes_discovered =
                        checked_add(self.source_bytes_discovered, inspected)?;
                    match poll {
                        SnapshotLineRetainedPoll::Pending(scanner) => {
                            if inspected == 0 {
                                return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
                            }
                            self.scanner = Some(scanner);
                            return Ok(WindowParsePoll {
                                transitions,
                                complete: false,
                            });
                        }
                        SnapshotLineRetainedPoll::Line(line) => {
                            self.pending_line = Some(line);
                            self.phase = WindowParsePhase::Admit;
                        }
                        SnapshotLineRetainedPoll::Complete(scanner) => {
                            self.scanner = Some(scanner);
                            return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
                        }
                    }
                }
                WindowParsePhase::Admit => {
                    transitions = checked_add(transitions, 1)?;
                    let line = self
                        .pending_line
                        .take()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?;
                    let facts = line.facts();
                    let identity = facts.identity();
                    if identity.source() != self.source_version
                        || identity.start_byte() as usize != self.next_byte
                        || identity.end_byte() as usize > self.range.end
                    {
                        self.scanner = Some(line.skip());
                        return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
                    }
                    let source = line.into_source()?;
                    self.active = Some(ActiveWindowLine {
                        facts,
                        source,
                        segmented: SegmentedLineScanner::new(self.next_byte == 0),
                    });
                    self.phase = WindowParsePhase::Read;
                }
                WindowParsePhase::Read => {
                    let active = self
                        .active
                        .as_mut()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?;
                    if active.source.position() < active.source.len() {
                        if active.source.access_budget() == 0 {
                            let remaining = active.source.len() - active.source.position();
                            let _ = active.source.replenish_access_budget(remaining)?;
                        }
                        let offset = active.source.position();
                        active.segmented.push(active.source.read_byte(offset)?);
                        self.source_bytes_read = checked_add(self.source_bytes_read, 1)?;
                        transitions = checked_add(transitions, 1)?;
                        continue;
                    }

                    transitions = checked_add(transitions, 1)?;
                    let active = self
                        .active
                        .take()
                        .ok_or(M11TightListLocalDeltaError::InvalidState)?;
                    let facts = active.facts;
                    let scanner = active.source.finish()?;
                    self.scanner = Some(scanner);
                    let segmented = active.segmented.finish()?;
                    self.pending_commit = Some((facts, segmented));
                    self.phase = WindowParsePhase::Commit;
                }
                WindowParsePhase::Commit => {
                    transitions = checked_add(transitions, 1)?;
                    self.commit_pending_line()?;
                    self.phase = WindowParsePhase::Discover;
                }
                WindowParsePhase::Finish => {
                    transitions = checked_add(transitions, 1)?;
                    self.finish_window()?;
                    self.phase = WindowParsePhase::Complete;
                    return Ok(WindowParsePoll {
                        transitions,
                        complete: true,
                    });
                }
                WindowParsePhase::Complete | WindowParsePhase::Cancelled => {
                    return Err(M11TightListLocalDeltaError::InvalidState);
                }
            }
        }
        Ok(WindowParsePoll {
            transitions,
            complete: false,
        })
    }

    fn commit_pending_line(&mut self) -> Result<(), M11TightListLocalDeltaError> {
        let (physical, segmented) = self
            .pending_commit
            .ok_or(M11TightListLocalDeltaError::InvalidState)?;
        let mapping = classify_item(
            physical,
            segmented,
            SourceCut {
                byte: u32::try_from(self.next_byte)
                    .map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?,
                utf16: u32::try_from(self.next_utf16)
                    .map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?,
            },
            self.next_ordinal,
            self.marker,
            self.current_content_indent,
            self.previous_empty,
        )?;
        let item_projected_utf8 = (mapping.content_source().end - mapping.content_source().start)
            .checked_add(mapping.line_ending().end - mapping.line_ending().start)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        let item_projected_utf16 = (mapping.content_source_utf16().end
            - mapping.content_source_utf16().start)
            .checked_add(mapping.line_ending_utf16().end - mapping.line_ending_utf16().start)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        self.paragraph_count = self
            .paragraph_count
            .checked_add(u32::from(mapping.has_paragraph()))
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        self.projected_utf8_length = self
            .projected_utf8_length
            .checked_add(item_projected_utf8)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        self.projected_utf16_length = self
            .projected_utf16_length
            .checked_add(item_projected_utf16)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        self.previous_empty = !mapping.has_paragraph();
        self.current_content_indent = Some(
            mapping
                .continuation_prefix_source()
                .end
                .checked_sub(mapping.continuation_prefix_source().start)
                .ok_or(M11TightListLocalDeltaError::MetricOverflow)? as usize,
        );
        self.next_byte = mapping.source().end as usize;
        self.next_utf16 = mapping.source_utf16().end as usize;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(M11TightListLocalDeltaError::MetricOverflow)?;
        if self.first.is_none() {
            self.first = Some(mapping);
        } else {
            self.last = Some(mapping);
        }
        self.item_count = checked_add(self.item_count, 1)?;
        self.pending_commit = None;
        Ok(())
    }

    fn finish_window(&mut self) -> Result<(), M11TightListLocalDeltaError> {
        if self.item_count < 2
            || self.next_byte != self.range.end
            || self.active.is_some()
            || self.pending_line.is_some()
            || self.pending_commit.is_some()
        {
            return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
        }
        let source = self
            .scanner
            .take()
            .ok_or(M11TightListLocalDeltaError::InvalidState)?
            .into_source_lease();
        self.completion = Some(ParsedWindow {
            source,
            first: self
                .first
                .take()
                .ok_or(M11TightListLocalDeltaError::ConvergenceMismatch)?,
            last: self
                .last
                .take()
                .ok_or(M11TightListLocalDeltaError::ConvergenceMismatch)?,
            item_count: self.item_count,
            paragraph_count: self.paragraph_count,
            projected_utf8_length: self.projected_utf8_length,
            projected_utf16_length: self.projected_utf16_length,
            source_bytes_discovered: self.source_bytes_discovered,
            source_bytes_read: self.source_bytes_read,
        });
        Ok(())
    }

    fn take_completion(&mut self) -> Option<ParsedWindow> {
        self.completion.take()
    }

    fn cancel_into_source_lease(mut self) -> Option<SourceSnapshotLease> {
        self.phase = WindowParsePhase::Cancelled;
        if let Some(completion) = self.completion.take() {
            return Some(completion.source);
        }
        if let Some(active) = self.active.take() {
            let (_, scanner) = active.source.cancel();
            return Some(scanner.into_source_lease());
        }
        if let Some(line) = self.pending_line.take() {
            return Some(line.skip().into_source_lease());
        }
        self.scanner
            .take()
            .map(SnapshotLineScanner::into_source_lease)
    }
}

#[allow(clippy::too_many_arguments)]
fn classify_item(
    physical: M11PhysicalLineFacts,
    segmented: SegmentedLineFacts,
    line_start: SourceCut,
    ordinal: u32,
    expected_marker: TightListMarker,
    prior_content_indent: Option<usize>,
    previous_empty: bool,
) -> Result<TightListItemMapping, M11TightListLocalDeltaError> {
    if previous_empty {
        return Err(M11TightListLocalDeltaError::UnsupportedList(
            M11ListUnsupportedReason::NonTerminalEmptyItem,
        ));
    }
    if segmented.blank {
        return Err(M11TightListLocalDeltaError::UnsupportedList(
            M11ListUnsupportedReason::Loose,
        ));
    }
    let item = segmented
        .list_item
        .ok_or(M11TightListLocalDeltaError::ConvergenceMismatch)?;
    if prior_content_indent.is_some_and(|indent| item.opening_indent >= indent) {
        return Err(M11TightListLocalDeltaError::UnsupportedList(
            M11ListUnsupportedReason::Nested,
        ));
    }
    if let Some(reason) = M11CleanBlockController::list_item_unsupported_reason(item) {
        return Err(M11TightListLocalDeltaError::UnsupportedList(reason));
    }
    if !segmented.list {
        return Err(M11TightListLocalDeltaError::ConvergenceMismatch);
    }
    match (expected_marker, item.marker) {
        (TightListMarker::Bullet(expected), SegmentedListMarker::Bullet(actual))
            if expected == actual =>
        {
            M11CleanBlockController::bullet_list_item_mapping(
                line_start,
                physical,
                item,
                segmented.has_bof_bom,
                ordinal,
            )
            .map(TightListItemMapping::Bullet)
            .map_err(map_controller_fault)
        }
        (
            TightListMarker::Ordered {
                delimiter: expected,
                start: _,
            },
            SegmentedListMarker::Ordered {
                delimiter: actual,
                start: _,
            },
        ) if expected == actual => M11CleanBlockController::ordered_list_item_mapping(
            line_start,
            physical,
            item,
            segmented.has_bof_bom,
            ordinal,
        )
        .map(TightListItemMapping::Ordered)
        .map_err(map_controller_fault),
        _ => Err(M11TightListLocalDeltaError::ConvergenceMismatch),
    }
}

fn map_controller_fault(error: M11CleanControllerFault) -> M11TightListLocalDeltaError {
    match error {
        M11CleanControllerFault::MetricOverflow | M11CleanControllerFault::OrdinalExhausted => {
            M11TightListLocalDeltaError::MetricOverflow
        }
        M11CleanControllerFault::LeafAllocationFailed
        | M11CleanControllerFault::CheckpointAllocationFailed => {
            M11TightListLocalDeltaError::AllocationFailed
        }
        _ => M11TightListLocalDeltaError::ConvergenceMismatch,
    }
}

fn locate_line(
    lease: &SourceSnapshotLease,
    byte: usize,
    affinity: SourceBoundaryAffinity,
) -> Result<flark_engine::SourcePhysicalLineLocation, M11TightListLocalDeltaError> {
    lease
        .locate_physical_line(byte, affinity)
        .map_err(|_| M11TightListLocalDeltaError::AuthorityMismatch)?
        .ok_or(M11TightListLocalDeltaError::InvalidChangedRange)
}

fn utf16_range(
    lease: &SourceSnapshotLease,
    range: &Range<usize>,
) -> Result<Range<usize>, M11TightListLocalDeltaError> {
    Ok(lease
        .utf16_offset_for_byte(range.start)
        .map_err(|_| M11TightListLocalDeltaError::AuthorityMismatch)?
        ..lease
            .utf16_offset_for_byte(range.end)
            .map_err(|_| M11TightListLocalDeltaError::AuthorityMismatch)?)
}

fn same_item_shape(left: &TightListItemMapping, right: &TightListItemMapping) -> bool {
    relative(left.source(), left.source().start) == relative(right.source(), right.source().start)
        && relative(left.source_utf16(), left.source_utf16().start)
            == relative(right.source_utf16(), right.source_utf16().start)
        && relative(left.opening_marker(), left.source().start)
            == relative(right.opening_marker(), right.source().start)
        && relative(left.hidden_prefix(), left.source().start)
            == relative(right.hidden_prefix(), right.source().start)
        && relative(left.hidden_prefix_utf16(), left.source_utf16().start)
            == relative(right.hidden_prefix_utf16(), right.source_utf16().start)
        && relative(left.continuation_prefix_source(), left.source().start)
            == relative(right.continuation_prefix_source(), right.source().start)
        && relative(
            left.continuation_prefix_source_utf16(),
            left.source_utf16().start,
        ) == relative(
            right.continuation_prefix_source_utf16(),
            right.source_utf16().start,
        )
        && relative(left.content_source(), left.source().start)
            == relative(right.content_source(), right.source().start)
        && relative(left.content_source_utf16(), left.source_utf16().start)
            == relative(right.content_source_utf16(), right.source_utf16().start)
        && relative(left.line_ending(), left.source().start)
            == relative(right.line_ending(), right.source().start)
        && relative(left.line_ending_utf16(), left.source_utf16().start)
            == relative(right.line_ending_utf16(), right.source_utf16().start)
        && left.has_paragraph() == right.has_paragraph()
        && match (left, right) {
            (TightListItemMapping::Bullet(left), TightListItemMapping::Bullet(right)) => {
                left.marker == right.marker
            }
            (TightListItemMapping::Ordered(left), TightListItemMapping::Ordered(right)) => {
                left.delimiter == right.delimiter && left.marker_value == right.marker_value
            }
            _ => false,
        }
}

fn relative(range: &Range<u32>, start: u32) -> Option<Range<u32>> {
    Some(range.start.checked_sub(start)?..range.end.checked_sub(start)?)
}

fn signed_delta(target: usize, base: usize) -> Result<i64, M11TightListLocalDeltaError> {
    Ok(
        i64::try_from(target).map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?
            - i64::try_from(base).map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?,
    )
}

fn checked_add(left: usize, right: usize) -> Result<usize, M11TightListLocalDeltaError> {
    left.checked_add(right)
        .ok_or(M11TightListLocalDeltaError::MetricOverflow)
}

fn shift_u32(value: u32, delta: i64) -> Option<u32> {
    i64::from(value)
        .checked_add(delta)
        .and_then(|shifted| u32::try_from(shifted).ok())
}

fn u32_range(range: &Range<usize>) -> Result<Range<u32>, M11TightListLocalDeltaError> {
    Ok(
        u32::try_from(range.start).map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?
            ..u32::try_from(range.end).map_err(|_| M11TightListLocalDeltaError::MetricOverflow)?,
    )
}
