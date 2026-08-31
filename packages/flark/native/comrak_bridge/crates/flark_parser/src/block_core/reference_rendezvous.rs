//! Atomic join between reference recognition, persistent semantics, and Green.
//!
//! The donor owns CommonMark recognition, the frozen terminal Green fragment
//! owns projected source, and `M11ReferenceJournal` owns ordered first-winner
//! semantics.  This actor is the only place those three linear capabilities
//! meet.  It never materializes a Paragraph or an unbounded destination/title.

use std::fmt;
use std::ops::Range;
#[cfg(any(test, feature = "m11-compact-probe"))]
use std::sync::Arc;

use flark_block_core_donor as donor;
use flark_block_core_donor::DirectReferencePrefixSource;
use flark_engine::parser_internal::{
    M11RecursiveGreenBuildStatus, M11RecursiveGreenFrameId, M11RecursiveGreenLogicalPosition,
    M11RecursiveGreenLogicalRange, M11RecursiveGreenStructuralBoundary,
    M11RecursiveGreenTerminalFragmentBarrierStatus, M11RecursiveGreenTerminalFragmentCursorStatus,
    M11RecursiveGreenTerminalFragmentDisposition, M11ReferenceJournal, M11ReferenceJournalError,
    M11ReferenceJournalOccurrenceStart, M11ReferenceJournalRange, M11ReferenceJournalValueKind,
};
#[cfg(any(test, feature = "m11-compact-probe"))]
use flark_engine::parser_internal::{
    M11ReferenceResolution, M11ResolvedReference, M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES,
};
use flark_engine::DocumentRuntime;

use super::controller::M11DirectLeadingReferenceRemainderContinuation;
use super::writer::{
    M11ReferenceOutputBinding, M11ReferenceOutputCursor, M11ReferenceOutputIdentity,
    M11ReferenceOutputRewrite, M11ReferenceOutputRewritePoll, M11ReferenceOutputRewriteWork,
    M11ReferenceStagedTerminator,
};
use super::{M11BlockWriter, M11BlockWriterError, M11DirectBlockController, M11DirectBlockError};
use crate::reference_value::{
    ReferenceValueBodyCleaner, ReferenceValueCleanerError, ReferenceValueCleanerStatus,
};

type Identity = M11ReferenceOutputIdentity;
type Work = donor::DirectReferencePrefixWork<Identity>;
type OutputAck = donor::DirectReferencePrefixOutputAck<Identity>;
type TerminalOutput = donor::DirectReferencePrefixTerminalOutput<Identity>;

#[cfg(any(test, feature = "m11-compact-probe"))]
const COMPACT_REFERENCE_LOOKUP_MAX_SOURCE_BYTES: usize = 64 * 1024;

trait M11ReferenceJournalSink {
    fn source_backed_values(&self) -> bool {
        false
    }

    fn record_rendezvous_phase(&mut self, _phase: usize) {}

    fn is_idle(&self) -> bool;

    fn poll_one(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError>;

    fn stream_capacity(
        &self,
        kind: M11ReferenceJournalValueKind,
    ) -> Result<usize, M11ReferenceRendezvousError>;

    fn offer_stream_bytes(
        &mut self,
        kind: M11ReferenceJournalValueKind,
        bytes: &[u8],
    ) -> Result<usize, M11ReferenceRendezvousError>;

    fn begin_occurrence_stream(
        &mut self,
        runtime: &DocumentRuntime,
        occurrence: M11ReferenceJournalOccurrenceStart,
    ) -> Result<(), M11ReferenceRendezvousError>;
}

impl M11ReferenceJournalSink for M11ReferenceJournal {
    fn is_idle(&self) -> bool {
        M11ReferenceJournal::is_idle(self)
    }

    fn poll_one(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let _ = M11ReferenceJournal::poll(self, runtime, 1)?;
        Ok(())
    }

    fn stream_capacity(
        &self,
        kind: M11ReferenceJournalValueKind,
    ) -> Result<usize, M11ReferenceRendezvousError> {
        Ok(M11ReferenceJournal::stream_capacity(self, kind)?)
    }

    fn offer_stream_bytes(
        &mut self,
        kind: M11ReferenceJournalValueKind,
        bytes: &[u8],
    ) -> Result<usize, M11ReferenceRendezvousError> {
        Ok(M11ReferenceJournal::offer_stream_bytes(self, kind, bytes)?)
    }

    fn begin_occurrence_stream(
        &mut self,
        runtime: &DocumentRuntime,
        occurrence: M11ReferenceJournalOccurrenceStart,
    ) -> Result<(), M11ReferenceRendezvousError> {
        Ok(M11ReferenceJournal::begin_occurrence_stream(
            self, runtime, occurrence,
        )?)
    }
}

#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M11CompactReferenceRecord {
    digest: [u8; 16],
    source_start: u32,
    source_end: u32,
    label_source_start: u32,
    label_source_end: u32,
    destination_source_start: u32,
    destination_source_end: u32,
    title_source_start: u32,
    title_source_end: u32,
    normalized_start: u32,
    normalized_len: u32,
    winner: u32,
}

#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Debug)]
struct M11CompactReferencePending {
    record: M11CompactReferenceRecord,
    destination_len: usize,
    destination_received: usize,
    title_len: Option<usize>,
    title_received: usize,
}

#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11CompactReferenceReceipt {
    pub(crate) occurrences: usize,
    pub(crate) winners: usize,
    pub(crate) allocated_bytes: usize,
    pub(crate) normalized_label_bytes: usize,
    pub(crate) rendezvous_phase_transitions: [u64; 8],
}

#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Debug, Default)]
pub(crate) struct M11CompactReferenceJournal {
    records: Vec<M11CompactReferenceRecord>,
    normalized_labels: Vec<u8>,
    order: Vec<u32>,
    pending: Option<M11CompactReferencePending>,
    complete: bool,
    winners: usize,
    rendezvous_phase_transitions: [u64; 8],
}

/// The reach of one compact reference resolver's winner directory.
///
/// A `Final` resolver was built after EOF: a missing label is authoritative
/// literalness. A `CommittedPrefix` resolver covers only an admitted prefix:
/// its present winners are final under the GFM first-winner rule (every
/// earlier position is already admitted, so a later duplicate always loses),
/// but a missing label stays `Unknown` because a later definition could
/// still bind it.
#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11CompactReferenceAuthority {
    Final,
    CommittedPrefix,
}

/// Immutable, source-version-bound first-winner authority retained by the
/// compact parse index. Unlike the full reference journal, this owns no arena
/// tree: one sorted ordinal directory points into packed labels and exact
/// source ranges. Cooked values are derived on demand through the same bounded
/// parser-owned cleaner, keeping dense documents inside the retained budget.
#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Clone, Debug)]
pub(crate) struct M11CompactReferenceResolver {
    source: flark_engine::SourceVersion,
    authority: M11CompactReferenceAuthority,
    /// Count of `Unknown` outcomes served, shared across clones so a bounded
    /// certification audit can prove that no capture in a pass depended on an
    /// absent winner. Final-authority resolvers never increment it.
    unknown_lookups: Arc<std::sync::atomic::AtomicU64>,
    index: Arc<M11CompactReferenceIndex>,
}

#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Debug)]
struct M11CompactReferenceIndex {
    records: Box<[M11CompactReferenceRecord]>,
    normalized_labels: Box<[u8]>,
    order: Box<[u32]>,
}

#[cfg(any(test, feature = "m11-compact-probe"))]
impl M11CompactReferenceJournal {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn finish_input(&mut self) -> Result<(), M11ReferenceRendezvousError> {
        if self.pending.is_some() || self.complete {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "compact reference index finished in a non-idle state",
            ));
        }
        self.winners = compute_compact_reference_winners(
            &mut self.records,
            &self.normalized_labels,
            &mut self.order,
        )?;
        self.complete = true;
        Ok(())
    }

    /// Snapshots the committed prefix into an immutable resolver without
    /// consuming or finishing the journal. Present winners are final under
    /// the GFM first-winner rule; missing labels stay `Unknown`. A definition
    /// still pending at the frontier is deliberately absent: it follows every
    /// committed record, so ignoring it can only defer, never misresolve.
    #[cfg(feature = "m11-compact-probe")]
    pub(crate) fn committed_prefix_resolver(
        &self,
        source: flark_engine::SourceVersion,
    ) -> Result<M11CompactReferenceResolver, M11ReferenceRendezvousError> {
        if self.complete {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "a finished compact reference index resolves with final authority",
            ));
        }
        let mut records = Vec::new();
        records.try_reserve_exact(self.records.len()).map_err(|_| {
            M11ReferenceRendezvousError::InvalidState("compact prefix record allocation failed")
        })?;
        records.extend_from_slice(&self.records);
        let mut normalized_labels = Vec::new();
        normalized_labels
            .try_reserve_exact(self.normalized_labels.len())
            .map_err(|_| {
                M11ReferenceRendezvousError::InvalidState("compact prefix label allocation failed")
            })?;
        normalized_labels.extend_from_slice(&self.normalized_labels);
        let mut order = Vec::new();
        compute_compact_reference_winners(&mut records, &normalized_labels, &mut order)?;
        Ok(M11CompactReferenceResolver {
            source,
            authority: M11CompactReferenceAuthority::CommittedPrefix,
            unknown_lookups: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            index: Arc::new(M11CompactReferenceIndex {
                records: records.into_boxed_slice(),
                normalized_labels: normalized_labels.into_boxed_slice(),
                order: order.into_boxed_slice(),
            }),
        })
    }

    pub(crate) fn receipt(&self) -> M11CompactReferenceReceipt {
        M11CompactReferenceReceipt {
            occurrences: self.records.len(),
            winners: self.winners,
            allocated_bytes: self
                .records
                .capacity()
                .saturating_mul(std::mem::size_of::<M11CompactReferenceRecord>())
                .saturating_add(self.normalized_labels.capacity())
                .saturating_add(
                    self.order
                        .capacity()
                        .saturating_mul(std::mem::size_of::<u32>()),
                ),
            normalized_label_bytes: self.normalized_labels.len(),
            rendezvous_phase_transitions: self.rendezvous_phase_transitions,
        }
    }

    pub(crate) fn into_resolver(
        self,
        source: flark_engine::SourceVersion,
    ) -> Result<M11CompactReferenceResolver, M11ReferenceRendezvousError> {
        if !self.complete || self.pending.is_some() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "compact reference resolver requires a completed index",
            ));
        }
        Ok(M11CompactReferenceResolver {
            source,
            authority: M11CompactReferenceAuthority::Final,
            unknown_lookups: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            index: Arc::new(M11CompactReferenceIndex {
                records: self.records.into_boxed_slice(),
                normalized_labels: self.normalized_labels.into_boxed_slice(),
                order: self.order.into_boxed_slice(),
            }),
        })
    }

    fn finish_pending_if_ready(&mut self) -> Result<(), M11ReferenceRendezvousError> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        if pending.destination_received != pending.destination_len
            || pending.title_received != pending.title_len.unwrap_or(0)
        {
            return Ok(());
        }
        let pending = self
            .pending
            .take()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "compact reference occurrence disappeared",
            ))?;
        self.records.try_reserve(1).map_err(|_| {
            M11ReferenceRendezvousError::InvalidState("compact reference record allocation failed")
        })?;
        self.records.push(pending.record);
        Ok(())
    }
}

/// One retained reference record flattened for relocatability probes.
#[cfg(any(test, feature = "m11-compact-probe"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11CompactReferenceProbeRecord {
    pub(crate) digest: [u8; 16],
    pub(crate) label: Vec<u8>,
    pub(crate) source: std::ops::Range<u32>,
    pub(crate) destination: std::ops::Range<u32>,
    pub(crate) title: Option<std::ops::Range<u32>>,
    pub(crate) winner: u32,
}

/// Sorts one record set into the winner directory and assigns each record its
/// first-winner ordinal. Shared by EOF completion and committed-prefix
/// snapshots so both authorities apply the identical GFM first-winner rule.
#[cfg(any(test, feature = "m11-compact-probe"))]
fn compute_compact_reference_winners(
    records: &mut [M11CompactReferenceRecord],
    normalized_labels: &[u8],
    order: &mut Vec<u32>,
) -> Result<usize, M11ReferenceRendezvousError> {
    order.try_reserve_exact(records.len()).map_err(|_| {
        M11ReferenceRendezvousError::InvalidState("compact reference order allocation failed")
    })?;
    for ordinal in 0..records.len() {
        order.push(
            u32::try_from(ordinal).map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?,
        );
    }
    order.sort_unstable_by(|left, right| {
        let left_record = &records[*left as usize];
        let right_record = &records[*right as usize];
        let left_label = compact_reference_label(normalized_labels, left_record);
        let right_label = compact_reference_label(normalized_labels, right_record);
        left_record
            .digest
            .cmp(&right_record.digest)
            .then_with(|| left_label.cmp(right_label))
            .then_with(|| left_record.source_start.cmp(&right_record.source_start))
    });
    let mut winners = 0;
    let mut previous: Option<u32> = None;
    for ordinal in order.iter().copied() {
        let winner = previous.filter(|previous_ordinal| {
            let previous_record = &records[*previous_ordinal as usize];
            let current = &records[ordinal as usize];
            previous_record.digest == current.digest
                && compact_reference_label(normalized_labels, previous_record)
                    == compact_reference_label(normalized_labels, current)
        });
        let winner = winner.unwrap_or_else(|| {
            winners += 1;
            ordinal
        });
        records[ordinal as usize].winner = winner;
        previous = Some(winner);
    }
    Ok(winners)
}

#[cfg(any(test, feature = "m11-compact-probe"))]
impl M11CompactReferenceResolver {
    /// Returns how many lookups this resolver (including every clone sharing
    /// its counter) answered with `Unknown`. Zero after a bounded capture
    /// pass proves no captured fact depended on an absent winner.
    #[cfg(feature = "m11-compact-probe")]
    pub(crate) fn unknown_lookups(&self) -> u64 {
        self.unknown_lookups
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns whether one source byte lies inside a committed reference
    /// definition's exact range. Definition text is suffix-independent — its
    /// presentation is fixed and its first-winner status is final — so
    /// bracket bytes inside it are not reference-use hazards.
    #[cfg(feature = "m11-compact-probe")]
    pub(crate) fn byte_is_inside_committed_definition(&self, byte: usize) -> bool {
        self.index.records.iter().any(|record| {
            (record.source_start as usize) <= byte && byte < record.source_end as usize
        })
    }

    /// Flattens the retained record layout for relocatability probes: the
    /// digest and label identify a record across revisions, everything else
    /// is the exact stored coordinate payload under measurement.
    pub(crate) fn probe_records(&self) -> Vec<M11CompactReferenceProbeRecord> {
        self.index
            .records
            .iter()
            .map(|record| M11CompactReferenceProbeRecord {
                digest: record.digest,
                label: compact_reference_label(&self.index.normalized_labels, record).to_vec(),
                source: record.source_start..record.source_end,
                destination: record.destination_source_start..record.destination_source_end,
                title: (record.title_source_start != u32::MAX)
                    .then(|| record.title_source_start..record.title_source_end),
                winner: record.winner,
            })
            .collect()
    }

    pub(crate) fn resolve(
        &self,
        runtime: &DocumentRuntime,
        normalized_label: &str,
        maximum_cooked_bytes: usize,
    ) -> Result<M11ReferenceResolution, M11ReferenceRendezvousError> {
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "compact reference resolver crossed source authority",
            ));
        }
        if normalized_label.is_empty() {
            return Ok(M11ReferenceResolution::Missing);
        }
        let digest = blake3::hash(normalized_label.as_bytes());
        let digest = &digest.as_bytes()[..16];
        let found = self.index.order.binary_search_by(|ordinal| {
            let record = &self.index.records[*ordinal as usize];
            record.digest.as_slice().cmp(digest).then_with(|| {
                compact_reference_label(&self.index.normalized_labels, record)
                    .cmp(normalized_label.as_bytes())
            })
        });
        let Ok(found) = found else {
            // Prefix authority cannot prove absence: a later definition may
            // still bind this label, so the caller must fail closed instead
            // of treating the use as literal text.
            if self.authority == M11CompactReferenceAuthority::CommittedPrefix {
                self.unknown_lookups
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(M11ReferenceResolution::Unknown);
            }
            return Ok(M11ReferenceResolution::Missing);
        };
        let record = self
            .index
            .records
            .get(self.index.order[found] as usize)
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "compact reference ordinal is in bounds",
            ))?;
        let winner = self.index.records.get(record.winner as usize).ok_or(
            M11ReferenceRendezvousError::InvalidState("compact reference winner is in bounds"),
        )?;
        let destination_source_len = winner
            .destination_source_end
            .checked_sub(winner.destination_source_start)
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "compact reference destination range is monotonic",
            ))? as usize;
        let title_source_len = if winner.title_source_start == u32::MAX {
            0
        } else {
            winner
                .title_source_end
                .checked_sub(winner.title_source_start)
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "compact reference title range is monotonic",
                ))? as usize
        };
        if destination_source_len
            .checked_add(title_source_len)
            .is_none_or(|total| total > COMPACT_REFERENCE_LOOKUP_MAX_SOURCE_BYTES)
        {
            return Ok(M11ReferenceResolution::ValueTooLarge);
        }
        let maximum_cooked_bytes =
            maximum_cooked_bytes.min(M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES.saturating_sub(32));
        let Some(destination) = cook_compact_reference_value(
            runtime,
            winner.destination_source_start..winner.destination_source_end,
            ValueKind::Destination,
            maximum_cooked_bytes,
        )?
        else {
            return Ok(M11ReferenceResolution::ValueTooLarge);
        };
        let title = if winner.title_source_start == u32::MAX {
            None
        } else {
            let remaining = maximum_cooked_bytes.saturating_sub(destination.len());
            let Some(title) = cook_compact_reference_value(
                runtime,
                winner.title_source_start..winner.title_source_end,
                ValueKind::Title,
                remaining,
            )?
            else {
                return Ok(M11ReferenceResolution::ValueTooLarge);
            };
            Some(title)
        };
        Ok(M11ReferenceResolution::Resolved(M11ResolvedReference::new(
            u64::from(record.winner),
            u64::from(winner.destination_source_start)..u64::from(winner.destination_source_end),
            (winner.title_source_start != u32::MAX)
                .then(|| u64::from(winner.title_source_start)..u64::from(winner.title_source_end)),
            destination,
            title,
        )))
    }
}

#[cfg(any(test, feature = "m11-compact-probe"))]
fn cook_compact_reference_value(
    runtime: &DocumentRuntime,
    range: Range<u32>,
    kind: ValueKind,
    maximum: usize,
) -> Result<Option<Box<str>>, M11ReferenceRendezvousError> {
    let mut cursor = runtime
        .snapshot_current_source()
        .map_err(|_| {
            M11ReferenceRendezvousError::InvalidState(
                "compact reference resolver lost current source",
            )
        })?
        .cursor_in(range.start as usize..range.end as usize)
        .map_err(|_| {
            M11ReferenceRendezvousError::InvalidState(
                "compact reference value range is not source-aligned",
            )
        })?;
    let mut cook = StreamingValueCook::new(kind, maximum);
    loop {
        match cook.poll_one() {
            Ok(StreamingValuePoll::NeedsSource) => {
                let mut byte = [0_u8; 1];
                if cursor.read(&mut byte) == 1 {
                    cook.offer_source_byte(byte[0])?;
                } else {
                    cook.finish_source()?;
                }
            }
            Ok(StreamingValuePoll::Progress) => {}
            Ok(StreamingValuePoll::Complete) => return cook.output.into_boxed_str().map(Some),
            Err(M11ReferenceRendezvousError::InvalidState(
                "reference cooked value exceeds its hard per-fact bound",
            )) => return Ok(None),
            Err(error) => return Err(error),
        }
    }
}

#[cfg(any(test, feature = "m11-compact-probe"))]
fn compact_reference_label<'a>(labels: &'a [u8], record: &M11CompactReferenceRecord) -> &'a [u8] {
    let start = record.normalized_start as usize;
    let end = start.saturating_add(record.normalized_len as usize);
    labels.get(start..end).unwrap_or_default()
}

#[cfg(any(test, feature = "m11-compact-probe"))]
impl M11ReferenceJournalSink for M11CompactReferenceJournal {
    fn source_backed_values(&self) -> bool {
        true
    }

    fn record_rendezvous_phase(&mut self, phase: usize) {
        if let Some(count) = self.rendezvous_phase_transitions.get_mut(phase) {
            *count = count.saturating_add(1);
        }
    }

    fn is_idle(&self) -> bool {
        self.pending.is_none() && !self.complete
    }

    fn poll_one(
        &mut self,
        _runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        self.finish_pending_if_ready()
    }

    fn stream_capacity(
        &self,
        kind: M11ReferenceJournalValueKind,
    ) -> Result<usize, M11ReferenceRendezvousError> {
        let pending = self
            .pending
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "compact reference stream has no occurrence",
            ))?;
        let remaining = match kind {
            M11ReferenceJournalValueKind::Destination => pending
                .destination_len
                .saturating_sub(pending.destination_received),
            M11ReferenceJournalValueKind::Title => pending
                .title_len
                .unwrap_or(0)
                .saturating_sub(pending.title_received),
        };
        Ok(remaining.min(flark_engine::SOURCE_CURSOR_WINDOW_BYTES))
    }

    fn offer_stream_bytes(
        &mut self,
        kind: M11ReferenceJournalValueKind,
        bytes: &[u8],
    ) -> Result<usize, M11ReferenceRendezvousError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "compact reference stream has no occurrence",
            ))?;
        let received = match kind {
            M11ReferenceJournalValueKind::Destination => &mut pending.destination_received,
            M11ReferenceJournalValueKind::Title => &mut pending.title_received,
        };
        let expected = match kind {
            M11ReferenceJournalValueKind::Destination => pending.destination_len,
            M11ReferenceJournalValueKind::Title => pending.title_len.unwrap_or(0),
        };
        let accepted = bytes.len().min(expected.saturating_sub(*received));
        *received = received
            .checked_add(accepted)
            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
        self.finish_pending_if_ready()?;
        Ok(accepted)
    }

    fn begin_occurrence_stream(
        &mut self,
        _runtime: &DocumentRuntime,
        occurrence: M11ReferenceJournalOccurrenceStart,
    ) -> Result<(), M11ReferenceRendezvousError> {
        if !self.is_idle() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "compact reference index was not idle",
            ));
        }
        let (
            source,
            label_source,
            destination_source,
            title_source,
            normalized,
            destination_len,
            title_len,
        ) = occurrence.into_parts();
        let normalized_start = u32::try_from(self.normalized_labels.len())
            .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
        let normalized_len = u32::try_from(normalized.len())
            .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
        self.normalized_labels
            .try_reserve(normalized.len())
            .map_err(|_| {
                M11ReferenceRendezvousError::InvalidState(
                    "compact normalized-label allocation failed",
                )
            })?;
        self.normalized_labels.extend_from_slice(&normalized);
        let digest = blake3::hash(&normalized);
        let mut digest_prefix = [0_u8; 16];
        digest_prefix.copy_from_slice(&digest.as_bytes()[..16]);
        let range = |range: &M11ReferenceJournalRange| {
            Ok::<(u32, u32), M11ReferenceRendezvousError>((
                u32::try_from(range.byte_range().start)
                    .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?,
                u32::try_from(range.byte_range().end)
                    .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?,
            ))
        };
        let (source_start, source_end) = range(&source)?;
        let (label_source_start, label_source_end) = range(&label_source)?;
        let (destination_source_start, destination_source_end) = range(&destination_source)?;
        let (title_source_start, title_source_end) = title_source
            .as_ref()
            .map(range)
            .transpose()?
            .unwrap_or((u32::MAX, u32::MAX));
        self.pending = Some(M11CompactReferencePending {
            record: M11CompactReferenceRecord {
                digest: digest_prefix,
                source_start,
                source_end,
                label_source_start,
                label_source_end,
                destination_source_start,
                destination_source_end,
                title_source_start,
                title_source_end,
                normalized_start,
                normalized_len,
                winner: u32::MAX,
            },
            destination_len,
            destination_received: 0,
            title_len,
            title_received: 0,
        });
        self.finish_pending_if_ready()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceRendezvousStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceRendezvousPoll {
    pub transitions: usize,
    pub status: M11ReferenceRendezvousStatus,
}

/// Joined parser and Green authority at the internal cut after a leading
/// reference-definition prefix and before its visible Paragraph remainder.
pub(crate) struct M11LeadingReferenceRemainder {
    parser: M11DirectLeadingReferenceRemainderContinuation,
    green: M11RecursiveGreenStructuralBoundary,
}

impl M11LeadingReferenceRemainder {
    pub(crate) fn into_parts(
        self,
    ) -> (
        M11DirectLeadingReferenceRemainderContinuation,
        M11RecursiveGreenStructuralBoundary,
    ) {
        (self.parser, self.green)
    }
}

#[derive(Debug)]
pub enum M11ReferenceRendezvousError {
    Controller(M11DirectBlockError),
    Writer(M11BlockWriterError),
    Journal(M11ReferenceJournalError),
    Cleaner(&'static str),
    InvalidState(&'static str),
    CounterOverflow,
    ZeroFuel,
}

impl fmt::Display for M11ReferenceRendezvousError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => write!(formatter, "{error:?}"),
            Self::Writer(error) => error.fmt(formatter),
            Self::Journal(error) => error.fmt(formatter),
            Self::Cleaner(message) => formatter.write_str(message),
            Self::InvalidState(message) => formatter.write_str(message),
            Self::CounterOverflow => formatter.write_str("reference rendezvous counter overflow"),
            Self::ZeroFuel => formatter.write_str("reference rendezvous requires nonzero fuel"),
        }
    }
}

impl std::error::Error for M11ReferenceRendezvousError {}

impl From<M11DirectBlockError> for M11ReferenceRendezvousError {
    fn from(error: M11DirectBlockError) -> Self {
        Self::Controller(error)
    }
}

impl From<M11BlockWriterError> for M11ReferenceRendezvousError {
    fn from(error: M11BlockWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<M11ReferenceJournalError> for M11ReferenceRendezvousError {
    fn from(error: M11ReferenceJournalError) -> Self {
        Self::Journal(error)
    }
}

impl From<ReferenceValueCleanerError> for M11ReferenceRendezvousError {
    fn from(error: ReferenceValueCleanerError) -> Self {
        Self::Cleaner(error.message())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Barrier,
    Scan,
    Occurrence,
    TerminalRange,
    Rewrite,
    Gap,
    Commit,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OccurrencePhase {
    SourcePrefix,
    Label,
    LabelDestinationGap,
    Destination,
    DestinationTitleGap,
    Title,
    SourceSuffix,
    BeginJournal,
    EmitDestination,
    EmitTitle,
    AwaitJournal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegmentKind {
    SourcePrefix,
    Label,
    Gap,
    Destination,
    Title,
    SourceSuffix,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueKind {
    Destination,
    Title,
}

impl ValueKind {
    const fn journal(self) -> M11ReferenceJournalValueKind {
        match self {
            Self::Destination => M11ReferenceJournalValueKind::Destination,
            Self::Title => M11ReferenceJournalValueKind::Title,
        }
    }
}

#[derive(Clone, Debug)]
struct LogicalSpan {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl LogicalSpan {
    fn from_direct(
        range: &donor::DirectReferenceLogicalRange,
        base: donor::DirectReferenceLogicalPosition,
    ) -> Result<Self, M11ReferenceRendezvousError> {
        let start_bytes = range.bytes.start.checked_sub(base.bytes).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its logical base"),
        )?;
        let end_bytes = range.bytes.end.checked_sub(base.bytes).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its logical base"),
        )?;
        let start_utf16 = range.utf16.start.checked_sub(base.utf16).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its UTF-16 base"),
        )?;
        let end_utf16 = range.utf16.end.checked_sub(base.utf16).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its UTF-16 base"),
        )?;
        if start_bytes > end_bytes || start_utf16 > end_utf16 {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference range is reversed",
            ));
        }
        Ok(Self {
            bytes: start_bytes..end_bytes,
            utf16: start_utf16..end_utf16,
        })
    }

    fn green(&self) -> Result<M11RecursiveGreenLogicalRange, M11ReferenceRendezvousError> {
        let start = M11RecursiveGreenLogicalPosition::new(self.bytes.start, self.utf16.start)
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference start is not a valid logical point",
            ))?;
        let end = M11RecursiveGreenLogicalPosition::new(self.bytes.end, self.utf16.end).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference end is not a valid logical point"),
        )?;
        M11RecursiveGreenLogicalRange::new(start, end).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range is not monotonic"),
        )
    }
}

const COOKED_SCRATCH_PAGE_BYTES: usize = 4 * 1024;
// This is the exact production `ReferenceRootLimits` per-fact bound. The
// rendezvous enforces it before retaining cooked scratch, then the journal
// independently preflights the same bound before accepting the occurrence.
const MAX_COOKED_REFERENCE_FACT_BYTES: usize = 16 * 1024 * 1024;

struct CookedScratch {
    pages: Vec<Box<[u8]>>,
    len: usize,
    maximum: usize,
}

impl CookedScratch {
    fn new(maximum: usize) -> Self {
        Self {
            pages: Vec::new(),
            len: 0,
            maximum,
        }
    }

    fn append(&mut self, mut bytes: &[u8]) -> Result<(), M11ReferenceRendezvousError> {
        let target = self
            .len
            .checked_add(bytes.len())
            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
        if target > self.maximum {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference cooked value exceeds its hard per-fact bound",
            ));
        }
        while !bytes.is_empty() {
            let page_offset = self.len % COOKED_SCRATCH_PAGE_BYTES;
            if page_offset == 0 {
                self.pages.try_reserve(1).map_err(|_| {
                    M11ReferenceRendezvousError::InvalidState(
                        "reference cooked scratch allocation failed",
                    )
                })?;
                let mut page = Vec::new();
                page.try_reserve_exact(COOKED_SCRATCH_PAGE_BYTES)
                    .map_err(|_| {
                        M11ReferenceRendezvousError::InvalidState(
                            "reference cooked scratch allocation failed",
                        )
                    })?;
                page.resize(COOKED_SCRATCH_PAGE_BYTES, 0);
                self.pages.push(page.into_boxed_slice());
            }
            let take = bytes.len().min(COOKED_SCRATCH_PAGE_BYTES - page_offset);
            let page = self
                .pages
                .last_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference cooked scratch lost its page",
                ))?;
            page[page_offset..page_offset + take].copy_from_slice(&bytes[..take]);
            self.len += take;
            bytes = &bytes[take..];
        }
        Ok(())
    }

    fn remaining_from(&self, offset: usize, maximum: usize) -> &[u8] {
        debug_assert!(offset < self.len);
        let page_index = offset / COOKED_SCRATCH_PAGE_BYTES;
        let page_offset = offset % COOKED_SCRATCH_PAGE_BYTES;
        let available = (self.len - offset)
            .min(COOKED_SCRATCH_PAGE_BYTES - page_offset)
            .min(maximum);
        &self.pages[page_index][page_offset..page_offset + available]
    }

    const fn len(&self) -> usize {
        self.len
    }

    #[cfg(any(test, feature = "m11-compact-probe"))]
    fn into_boxed_str(self) -> Result<Box<str>, M11ReferenceRendezvousError> {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(self.len).map_err(|_| {
            M11ReferenceRendezvousError::InvalidState("compact reference result allocation failed")
        })?;
        let mut remaining = self.len;
        for page in self.pages {
            let accepted = remaining.min(page.len());
            bytes.extend_from_slice(&page[..accepted]);
            remaining -= accepted;
        }
        if remaining != 0 {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "compact reference result pages were truncated",
            ));
        }
        String::from_utf8(bytes)
            .map(String::into_boxed_str)
            .map_err(|_| {
                M11ReferenceRendezvousError::InvalidState(
                    "compact reference cooked value remains UTF-8",
                )
            })
    }
}

enum StreamingValueMode {
    Destination {
        saw_non_space: bool,
        pending_spaces: usize,
        pending_non_space: Option<u8>,
    },
    Title {
        saw_first: bool,
        expected_close: Option<u8>,
        held_last: Option<u8>,
        pending_feed: Option<u8>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamingValuePoll {
    NeedsSource,
    Progress,
    Complete,
}

struct StreamingValueCook {
    mode: StreamingValueMode,
    cleaner: ReferenceValueBodyCleaner,
    cleaner_needs_input: bool,
    source_finished: bool,
    finish_sent: bool,
    complete: bool,
    output: CookedScratch,
}

impl StreamingValueCook {
    fn new(kind: ValueKind, maximum: usize) -> Self {
        Self {
            mode: match kind {
                ValueKind::Destination => StreamingValueMode::Destination {
                    saw_non_space: false,
                    pending_spaces: 0,
                    pending_non_space: None,
                },
                ValueKind::Title => StreamingValueMode::Title {
                    saw_first: false,
                    expected_close: None,
                    held_last: None,
                    pending_feed: None,
                },
            },
            cleaner: ReferenceValueBodyCleaner::new(),
            cleaner_needs_input: true,
            source_finished: false,
            finish_sent: false,
            complete: false,
            output: CookedScratch::new(maximum),
        }
    }

    fn can_accept_source(&self) -> bool {
        if self.source_finished {
            return false;
        }
        match &self.mode {
            StreamingValueMode::Destination {
                pending_non_space, ..
            } => pending_non_space.is_none(),
            StreamingValueMode::Title { pending_feed, .. } => pending_feed.is_none(),
        }
    }

    fn offer_source_byte(&mut self, byte: u8) -> Result<(), M11ReferenceRendezvousError> {
        if !self.can_accept_source() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference value source advanced before its cleaner",
            ));
        }
        match &mut self.mode {
            StreamingValueMode::Destination {
                saw_non_space,
                pending_spaces,
                pending_non_space,
            } => {
                if is_comrak_space(byte) {
                    if *saw_non_space {
                        *pending_spaces = pending_spaces
                            .checked_add(1)
                            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
                    }
                } else {
                    *saw_non_space = true;
                    *pending_non_space = Some(byte);
                }
            }
            StreamingValueMode::Title {
                saw_first,
                expected_close,
                held_last,
                pending_feed,
            } => {
                if !*saw_first {
                    *saw_first = true;
                    *expected_close = match byte {
                        b'\'' | b'"' => Some(byte),
                        b'(' => Some(b')'),
                        _ => {
                            *pending_feed = Some(byte);
                            None
                        }
                    };
                } else if expected_close.is_some() {
                    if let Some(previous) = held_last.replace(byte) {
                        *pending_feed = Some(previous);
                    }
                } else {
                    *pending_feed = Some(byte);
                }
            }
        }
        Ok(())
    }

    fn finish_source(&mut self) -> Result<(), M11ReferenceRendezvousError> {
        if self.source_finished {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference value source finished twice",
            ));
        }
        match &mut self.mode {
            StreamingValueMode::Destination { pending_spaces, .. } => {
                // These are trailing spaces. Internal runs were retained until
                // the next non-space proved that they belong to the body.
                *pending_spaces = 0;
            }
            StreamingValueMode::Title {
                saw_first,
                expected_close,
                held_last,
                ..
            } => {
                if !*saw_first {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference title source is empty",
                    ));
                }
                if let Some(expected) = expected_close {
                    if held_last.take() != Some(*expected) {
                        return Err(M11ReferenceRendezvousError::InvalidState(
                            "reference title delimiters changed after recognition",
                        ));
                    }
                }
            }
        }
        self.source_finished = true;
        Ok(())
    }

    fn poll_one(&mut self) -> Result<StreamingValuePoll, M11ReferenceRendezvousError> {
        if self.complete {
            return Ok(StreamingValuePoll::Complete);
        }
        if !self.cleaner_needs_input {
            return match self.cleaner.poll()? {
                ReferenceValueCleanerStatus::Progress => Ok(StreamingValuePoll::Progress),
                ReferenceValueCleanerStatus::NeedInput => {
                    self.cleaner_needs_input = true;
                    Ok(StreamingValuePoll::Progress)
                }
                ReferenceValueCleanerStatus::OutputReady => {
                    let chunk = self.cleaner.take_output()?;
                    self.output.append(chunk.bytes())?;
                    Ok(StreamingValuePoll::Progress)
                }
                ReferenceValueCleanerStatus::Complete => {
                    self.complete = true;
                    Ok(StreamingValuePoll::Complete)
                }
            };
        }

        let next = match &mut self.mode {
            StreamingValueMode::Destination {
                pending_spaces,
                pending_non_space,
                ..
            } => {
                if pending_non_space.is_some() && *pending_spaces > 0 {
                    *pending_spaces -= 1;
                    Some(b' ')
                } else {
                    pending_non_space.take()
                }
            }
            StreamingValueMode::Title { pending_feed, .. } => pending_feed.take(),
        };
        if let Some(byte) = next {
            self.cleaner.offer_byte(byte)?;
            self.cleaner_needs_input = false;
            return Ok(StreamingValuePoll::Progress);
        }
        if !self.source_finished {
            return Ok(StreamingValuePoll::NeedsSource);
        }
        if !self.finish_sent {
            self.cleaner.finish_input()?;
            self.cleaner_needs_input = false;
            self.finish_sent = true;
            return Ok(StreamingValuePoll::Progress);
        }
        Err(M11ReferenceRendezvousError::InvalidState(
            "reference cleaner requested input after source completion",
        ))
    }

    fn take_output(&mut self) -> Result<CookedScratch, M11ReferenceRendezvousError> {
        if !self.complete {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference cooked scratch was taken before completion",
            ));
        }
        Ok(std::mem::replace(&mut self.output, CookedScratch::new(0)))
    }
}

fn is_comrak_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

#[derive(Clone, Debug)]
struct PhysicalEnvelope {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl PhysicalEnvelope {
    fn include(&mut self, bytes: Range<u64>, utf16: Range<u64>) {
        self.bytes.end = bytes.end;
        self.utf16.end = utf16.end;
    }

    fn into_journal(self) -> M11ReferenceJournalRange {
        M11ReferenceJournalRange::new(self.bytes, self.utf16)
    }
}

struct ActiveOccurrence {
    definition: donor::DirectReferenceDefinition,
    ack: Option<OutputAck>,
    phase: OccurrencePhase,
    segment_started: bool,
    value_cook: Option<StreamingValueCook>,
    source_envelope: Option<PhysicalEnvelope>,
    source: Option<M11ReferenceJournalRange>,
    label_source: Option<M11ReferenceJournalRange>,
    destination_source: Option<M11ReferenceJournalRange>,
    title_source: Option<M11ReferenceJournalRange>,
    cooked_destination: Option<CookedScratch>,
    cooked_title: Option<CookedScratch>,
    emit_offset: usize,
}

impl ActiveOccurrence {
    fn new(definition: donor::DirectReferenceDefinition, ack: OutputAck) -> Self {
        Self {
            definition,
            ack: Some(ack),
            phase: OccurrencePhase::SourcePrefix,
            segment_started: false,
            value_cook: None,
            source_envelope: None,
            source: None,
            label_source: None,
            destination_source: None,
            title_source: None,
            cooked_destination: None,
            cooked_title: None,
            emit_offset: 0,
        }
    }
}

fn logical_span(byte_start: u64, byte_end: u64, utf16_start: u64, utf16_end: u64) -> LogicalSpan {
    LogicalSpan {
        bytes: byte_start..byte_end,
        utf16: utf16_start..utf16_end,
    }
}

fn advance_occurrence_segment(
    active: &mut ActiveOccurrence,
    has_title: bool,
) -> Result<(), M11ReferenceRendezvousError> {
    active.segment_started = false;
    active.phase = match active.phase {
        OccurrencePhase::SourcePrefix => OccurrencePhase::Label,
        OccurrencePhase::Label => OccurrencePhase::LabelDestinationGap,
        OccurrencePhase::LabelDestinationGap => OccurrencePhase::Destination,
        OccurrencePhase::Destination => {
            if has_title {
                OccurrencePhase::DestinationTitleGap
            } else {
                OccurrencePhase::SourceSuffix
            }
        }
        OccurrencePhase::DestinationTitleGap => OccurrencePhase::Title,
        OccurrencePhase::Title => OccurrencePhase::SourceSuffix,
        OccurrencePhase::SourceSuffix => OccurrencePhase::BeginJournal,
        _ => {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference segment completed outside the segment transaction",
            ));
        }
    };
    Ok(())
}

fn finalize_occurrence_source(
    active: &mut ActiveOccurrence,
    base: donor::DirectReferenceLogicalPosition,
    fragment_end: M11RecursiveGreenLogicalPosition,
    staged: Option<M11ReferenceStagedTerminator>,
) -> Result<(), M11ReferenceRendezvousError> {
    let source = LogicalSpan::from_direct(&active.definition.logical_source, base)?;
    let mut envelope =
        active
            .source_envelope
            .take()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference source traversal produced no physical envelope",
            ))?;
    if source.bytes.end > fragment_end.bytes() || source.utf16.end > fragment_end.utf16() {
        let staged = staged.ok_or(M11ReferenceRendezvousError::InvalidState(
            "reference source escaped Green without a staged terminator",
        ))?;
        if envelope.bytes.end != staged.start.bytes() || envelope.utf16.end != staged.start.utf16()
        {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference source did not join its staged terminator",
            ));
        }
        envelope.bytes.end = staged.end.bytes();
        envelope.utf16.end = staged.end.utf16();
    }
    active.source = Some(envelope.into_journal());
    Ok(())
}

/// One fuelled reference-prefix transaction for the active Paragraph.
#[must_use = "reference rendezvous must be polled to completion"]
pub struct M11ReferenceRendezvous {
    request: donor::DirectReferencePrefixRequest,
    frame: M11RecursiveGreenFrameId,
    staged: Option<M11ReferenceStagedTerminator>,
    phase: Phase,
    binding: Option<M11ReferenceOutputBinding>,
    identity: Option<Identity>,
    scan: Option<M11ReferenceOutputCursor>,
    range_replay: Option<M11ReferenceOutputCursor>,
    work: Option<Work>,
    active: Option<ActiveOccurrence>,
    terminal: Option<TerminalOutput>,
    terminal_replay: Option<M11ReferenceOutputCursor>,
    rewrite: Option<M11ReferenceOutputRewriteWork>,
    checkpoint_invalidation_start: Option<super::SourceMetric>,
    remainder_boundary: Option<M11RecursiveGreenStructuralBoundary>,
    remainder: Option<M11LeadingReferenceRemainder>,
}

impl M11ReferenceRendezvous {
    pub fn begin(
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
    ) -> Result<Self, M11ReferenceRendezvousError> {
        let request = controller.pending_reference_prefix_request()?;
        let frame = writer.reference_paragraph_frame()?;
        let staged = writer.reference_staged_terminator(frame)?;
        if request.include_pending_terminator() != staged.is_some() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "parser and writer disagree about the staged Paragraph terminator",
            ));
        }
        writer.begin_reference_output_fragment(frame)?;
        Ok(Self {
            request,
            frame,
            staged,
            phase: Phase::Barrier,
            binding: None,
            identity: None,
            scan: None,
            range_replay: None,
            work: None,
            active: None,
            terminal: None,
            terminal_replay: None,
            rewrite: None,
            checkpoint_invalidation_start: None,
            remainder_boundary: None,
            remainder: None,
        })
    }

    pub(crate) fn take_checkpoint_invalidation_start(&mut self) -> Option<super::SourceMetric> {
        self.checkpoint_invalidation_start.take()
    }

    pub(crate) fn take_leading_reference_remainder(
        &mut self,
    ) -> Option<M11LeadingReferenceRemainder> {
        self.remainder.take()
    }

    pub fn poll(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceRendezvousPoll, M11ReferenceRendezvousError> {
        self.poll_with_sink(controller, writer, journal, runtime, fuel)
    }

    #[cfg(any(test, feature = "m11-compact-probe"))]
    pub(crate) fn poll_compact(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        journal: &mut M11CompactReferenceJournal,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceRendezvousPoll, M11ReferenceRendezvousError> {
        self.poll_with_sink(controller, writer, journal, runtime, fuel)
    }

    fn poll_with_sink<J: M11ReferenceJournalSink>(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        journal: &mut J,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceRendezvousPoll, M11ReferenceRendezvousError> {
        if fuel == 0 {
            return Err(M11ReferenceRendezvousError::ZeroFuel);
        }
        if self.phase == Phase::Complete {
            return Ok(M11ReferenceRendezvousPoll {
                transitions: 0,
                status: M11ReferenceRendezvousStatus::Complete,
            });
        }
        if self.phase == Phase::Failed {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference rendezvous is failed",
            ));
        }
        let mut transitions = 0;
        while transitions < fuel && self.phase != Phase::Complete {
            let result = self.drive_one(controller, writer, journal, runtime);
            if let Err(error) = result {
                self.phase = Phase::Failed;
                return Err(error);
            }
            transitions += 1;
        }
        Ok(M11ReferenceRendezvousPoll {
            transitions,
            status: if self.phase == Phase::Complete {
                M11ReferenceRendezvousStatus::Complete
            } else {
                M11ReferenceRendezvousStatus::Pending
            },
        })
    }

    fn drive_one<J: M11ReferenceJournalSink>(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        journal: &mut J,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        journal.record_rendezvous_phase(match self.phase {
            Phase::Barrier => 0,
            Phase::Scan => 1,
            Phase::Occurrence => 2,
            Phase::TerminalRange => 3,
            Phase::Rewrite => 4,
            Phase::Gap => 5,
            Phase::Commit => 6,
            Phase::Complete | Phase::Failed => 7,
        });
        match self.phase {
            Phase::Barrier => self.poll_barrier(controller, writer, runtime),
            Phase::Scan => self.poll_scan(writer, runtime),
            Phase::Occurrence => self.poll_occurrence(writer, journal, runtime),
            Phase::TerminalRange => self.poll_terminal_range(writer, runtime),
            Phase::Rewrite => self.poll_rewrite(writer, runtime),
            Phase::Gap => self.poll_gap(writer, runtime),
            Phase::Commit => self.commit_terminal(controller, runtime),
            Phase::Complete => Ok(()),
            Phase::Failed => Err(M11ReferenceRendezvousError::InvalidState(
                "reference rendezvous is failed",
            )),
        }
    }

    fn poll_barrier(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let status = writer.poll_reference_output_barrier(runtime, 1)?;
        if status != M11RecursiveGreenTerminalFragmentBarrierStatus::Ready {
            return Ok(());
        }
        let binding = writer.take_reference_output_binding()?;
        let identity = binding.identity();
        let scan = writer.open_reference_output_cursor(&binding)?;
        let work = controller.begin_reference_prefix_work(self.request, identity)?;
        self.binding = Some(binding);
        self.identity = Some(identity);
        self.scan = Some(scan);
        self.work = Some(work);
        self.phase = Phase::Scan;
        Ok(())
    }

    fn poll_scan(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let scan = self
            .scan
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference scan cursor disappeared",
            ))?;
        if scan.ready_chunk().is_empty() && !scan.is_final() {
            let _ = writer.poll_reference_output_cursor(runtime, scan, 1, true)?;
        }
        let identity = self
            .identity
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference projection identity disappeared",
            ))?;
        let staged = self.staged;
        let mut source = ProjectedReferenceSource {
            identity,
            cursor: scan,
            virtual_lf: staged.is_some(),
            virtual_raw: staged.map_or(0, |value| value.raw_codepoint_contribution),
        };
        let scan_fuel = source
            .access_budget()
            .min(flark_engine::SOURCE_CURSOR_WINDOW_BYTES)
            .max(1);
        let receipt = self
            .work
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference scanner work disappeared",
            ))?
            .poll_source(&mut source, scan_fuel, false)
            .map_err(map_donor_poll_error)?;
        match receipt.status {
            donor::DirectReferencePrefixPollStatus::NeedMore => {}
            donor::DirectReferencePrefixPollStatus::OutputReady => {
                let output = self
                    .work
                    .as_mut()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference output lost its scanner work",
                    ))?
                    .take_output()
                    .map_err(map_infallible_donor_error)?;
                if output.source_identity() != identity {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference output crossed its projected source",
                    ));
                }
                let (definition, ack) = output.acknowledge();
                if definition.destination_transform
                    != donor::DirectReferenceValueTransform::CleanDestination
                    || definition.title_transform
                        != definition
                            .logical_title
                            .as_ref()
                            .map(|_| donor::DirectReferenceValueTransform::CleanTitle)
                {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference output selected an unsupported value transform",
                    ));
                }
                self.active = Some(ActiveOccurrence::new(definition, ack));
                self.phase = Phase::Occurrence;
            }
            donor::DirectReferencePrefixPollStatus::Complete => {
                let work = self
                    .work
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "completed reference scan lost its work",
                    ))?;
                let terminal = work.take_terminal().map_err(|_| {
                    M11ReferenceRendezvousError::InvalidState(
                        "completed reference scan lost its terminal",
                    )
                })?;
                if terminal.source_identity() != identity {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference terminal crossed its projected source",
                    ));
                }
                self.terminal = Some(terminal);
                self.begin_terminal_rewrite(writer, runtime)?;
            }
            donor::DirectReferencePrefixPollStatus::Cancelled => {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference scanner was unexpectedly cancelled",
                ));
            }
        }
        Ok(())
    }

    fn poll_occurrence<J: M11ReferenceJournalSink>(
        &mut self,
        writer: &mut M11BlockWriter,
        journal: &mut J,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let phase = self
            .active
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?
            .phase;
        match phase {
            OccurrencePhase::SourcePrefix
            | OccurrencePhase::Label
            | OccurrencePhase::LabelDestinationGap
            | OccurrencePhase::Destination
            | OccurrencePhase::DestinationTitleGap
            | OccurrencePhase::Title
            | OccurrencePhase::SourceSuffix => {
                self.poll_occurrence_segment(writer, runtime, journal.source_backed_values())
            }
            OccurrencePhase::BeginJournal => self.begin_journal(journal, runtime),
            OccurrencePhase::EmitDestination | OccurrencePhase::EmitTitle => {
                self.poll_occurrence_scratch(journal, runtime)
            }
            OccurrencePhase::AwaitJournal => {
                if !journal.is_idle() {
                    journal.poll_one(runtime)?;
                    return Ok(());
                }
                let mut active =
                    self.active
                        .take()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "durable reference occurrence disappeared",
                        ))?;
                let ack = active
                    .ack
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "durable reference occurrence lost its parser acknowledgement",
                    ))?;
                let ack_status = self
                    .work
                    .as_mut()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference scanner work disappeared after publication",
                    ))?
                    .acknowledge_output(ack)
                    .map_err(map_infallible_donor_error)?;
                if ack_status == donor::DirectReferencePrefixOutputAckStatus::Complete {
                    let work =
                        self.work
                            .take()
                            .ok_or(M11ReferenceRendezvousError::InvalidState(
                                "completed reference occurrence lost its scanner work",
                            ))?;
                    let terminal = work.take_terminal().map_err(|_| {
                        M11ReferenceRendezvousError::InvalidState(
                            "completed reference occurrence lost its terminal",
                        )
                    })?;
                    self.terminal = Some(terminal);
                    self.begin_terminal_rewrite(writer, runtime)?;
                } else {
                    self.phase = Phase::Scan;
                }
                Ok(())
            }
        }
    }

    fn poll_occurrence_segment(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
        source_backed_values: bool,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let base = self.request.logical_base();
        let fragment_end = self.fragment_logical_end()?;
        let (kind, span, has_title) = {
            let active = self
                .active
                .as_ref()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence disappeared",
                ))?;
            let source = LogicalSpan::from_direct(&active.definition.logical_source, base)?;
            let label = LogicalSpan::from_direct(&active.definition.logical_label, base)?;
            let destination =
                LogicalSpan::from_direct(&active.definition.logical_destination, base)?;
            let title = active
                .definition
                .logical_title
                .as_ref()
                .map(|range| LogicalSpan::from_direct(range, base))
                .transpose()?;
            let selected = match active.phase {
                OccurrencePhase::SourcePrefix => (
                    SegmentKind::SourcePrefix,
                    logical_span(
                        source.bytes.start,
                        label.bytes.start,
                        source.utf16.start,
                        label.utf16.start,
                    ),
                ),
                OccurrencePhase::Label => (SegmentKind::Label, label.clone()),
                OccurrencePhase::LabelDestinationGap => (
                    SegmentKind::Gap,
                    logical_span(
                        label.bytes.end,
                        destination.bytes.start,
                        label.utf16.end,
                        destination.utf16.start,
                    ),
                ),
                OccurrencePhase::Destination => (SegmentKind::Destination, destination.clone()),
                OccurrencePhase::DestinationTitleGap => {
                    let title = title
                        .as_ref()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference title gap has no title",
                        ))?;
                    (
                        SegmentKind::Gap,
                        logical_span(
                            destination.bytes.end,
                            title.bytes.start,
                            destination.utf16.end,
                            title.utf16.start,
                        ),
                    )
                }
                OccurrencePhase::Title => (
                    SegmentKind::Title,
                    title
                        .clone()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference title phase has no title",
                        ))?,
                ),
                OccurrencePhase::SourceSuffix => {
                    let start = title.as_ref().unwrap_or(&destination);
                    (
                        SegmentKind::SourceSuffix,
                        logical_span(
                            start.bytes.end,
                            source.bytes.end,
                            start.utf16.end,
                            source.utf16.end,
                        ),
                    )
                }
                _ => {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference segment entered a non-segment phase",
                    ));
                }
            };
            (selected.0, selected.1, title.is_some())
        };
        let clipped = clip_to_fragment(&span, fragment_end, self.staged.is_some())?;
        let empty =
            clipped.bytes.start == clipped.bytes.end && clipped.utf16.start == clipped.utf16.end;

        if !source_backed_values && matches!(kind, SegmentKind::Destination | SegmentKind::Title) {
            let active = self
                .active
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence disappeared",
                ))?;
            if active.value_cook.is_none() {
                let normalized_len = active.definition.normalized_label.as_bytes().len();
                let already_cooked = active
                    .cooked_destination
                    .as_ref()
                    .map_or(0, CookedScratch::len);
                let maximum = MAX_COOKED_REFERENCE_FACT_BYTES
                    .checked_sub(normalized_len)
                    .and_then(|remaining| remaining.checked_sub(already_cooked))
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference cooked values exceed their hard per-fact bound",
                    ))?;
                active.value_cook = Some(StreamingValueCook::new(
                    if kind == SegmentKind::Destination {
                        ValueKind::Destination
                    } else {
                        ValueKind::Title
                    },
                    maximum,
                ));
            }
            match active
                .value_cook
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference value cleaner disappeared",
                ))?
                .poll_one()?
            {
                StreamingValuePoll::Progress => return Ok(()),
                StreamingValuePoll::Complete => {
                    let output = active
                        .value_cook
                        .as_mut()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference value cleaner disappeared",
                        ))?
                        .take_output()?;
                    active.value_cook = None;
                    if kind == SegmentKind::Destination {
                        active.cooked_destination = Some(output);
                    } else {
                        active.cooked_title = Some(output);
                    }
                    advance_occurrence_segment(active, has_title)?;
                    return Ok(());
                }
                StreamingValuePoll::NeedsSource => {}
            }
        }

        let segment_started = self
            .active
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?
            .segment_started;
        if !segment_started {
            // Label/value ranges are durable source facts even when their
            // logical extent is empty (for example the destination in
            // `[foo]: <>`). Replay the zero-width range so Green authenticates
            // its exact physical point; only syntax/gap segments may advance
            // without retaining range authority.
            let retains_range = matches!(
                kind,
                SegmentKind::Label | SegmentKind::Destination | SegmentKind::Title
            );
            if empty && !retains_range {
                let active =
                    self.active
                        .as_mut()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference occurrence disappeared",
                        ))?;
                active.segment_started = true;
                if let Some(cook) = active.value_cook.as_mut() {
                    cook.finish_source()?;
                } else {
                    if active.phase == OccurrencePhase::SourceSuffix {
                        finalize_occurrence_source(active, base, fragment_end, self.staged)?;
                    }
                    advance_occurrence_segment(active, has_title)?;
                }
                return Ok(());
            }
            let binding =
                self.binding
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference segment lost its fragment binding",
                    ))?;
            let range = writer.bind_reference_output_logical_range(binding, clipped.green()?)?;
            if let Some(replay) = self.range_replay.as_mut() {
                writer.retarget_reference_output_range_replay_forward(binding, replay, range)?;
            } else {
                self.range_replay =
                    Some(writer.open_reference_output_range_replay(binding, range)?);
            }
            self.active
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence disappeared",
                ))?
                .segment_started = true;
            return Ok(());
        }

        let replay =
            self.range_replay
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference forward replay disappeared",
                ))?;
        let polled = if !source_backed_values
            && matches!(kind, SegmentKind::Destination | SegmentKind::Title)
        {
            writer.poll_reference_output_cursor(runtime, replay, 1, false)?
        } else {
            writer.poll_reference_output_cursor(runtime, replay, 1, true)?
        };
        match polled {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => Ok(()),
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                if let Some(cook) = self
                    .active
                    .as_mut()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence disappeared",
                    ))?
                    .value_cook
                    .as_mut()
                {
                    let (relative_offset, _) =
                        replay
                            .ready_byte()
                            .ok_or(M11ReferenceRendezvousError::InvalidState(
                                "reference value replay reported no ready byte",
                            ))?;
                    let byte = replay.read_byte(relative_offset)?;
                    cook.offer_source_byte(byte)?;
                } else {
                    let ready = replay.ready_chunk().len();
                    if ready == 0 {
                        return Err(M11ReferenceRendezvousError::InvalidState(
                            "reference range replay reported an empty ready chunk",
                        ));
                    }
                    replay.consume_ready_prefix(ready)?;
                }
                Ok(())
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => {
                let completed = replay.take_completed_range()?;
                let (bytes, utf16) =
                    completed
                        .physical_range()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "nonempty reference segment has no physical envelope",
                        ))?;
                let active =
                    self.active
                        .as_mut()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference occurrence disappeared",
                        ))?;
                match active.source_envelope.as_mut() {
                    Some(envelope) => envelope.include(bytes.clone(), utf16.clone()),
                    None => {
                        active.source_envelope = Some(PhysicalEnvelope {
                            bytes: bytes.clone(),
                            utf16: utf16.clone(),
                        });
                    }
                }
                match kind {
                    SegmentKind::Label => {
                        active.label_source = Some(M11ReferenceJournalRange::new(bytes, utf16));
                    }
                    SegmentKind::Destination => {
                        active.destination_source =
                            Some(M11ReferenceJournalRange::new(bytes, utf16));
                    }
                    SegmentKind::Title => {
                        active.title_source = Some(M11ReferenceJournalRange::new(bytes, utf16));
                    }
                    SegmentKind::SourcePrefix | SegmentKind::Gap | SegmentKind::SourceSuffix => {}
                }
                if let Some(cook) = active.value_cook.as_mut() {
                    cook.finish_source()?;
                } else {
                    if active.phase == OccurrencePhase::SourceSuffix {
                        finalize_occurrence_source(active, base, fragment_end, self.staged)?;
                    }
                    advance_occurrence_segment(active, has_title)?;
                }
                Ok(())
            }
        }
    }

    fn poll_occurrence_scratch<J: M11ReferenceJournalSink>(
        &mut self,
        journal: &mut J,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let active = self
            .active
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?;
        let (kind, scratch) = match active.phase {
            OccurrencePhase::EmitDestination => (
                ValueKind::Destination,
                active.cooked_destination.as_ref().ok_or(
                    M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its cooked destination",
                    ),
                )?,
            ),
            OccurrencePhase::EmitTitle => (
                ValueKind::Title,
                active
                    .cooked_title
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its cooked title",
                    ))?,
            ),
            _ => {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference scratch entered a non-emission phase",
                ));
            }
        };
        if active.emit_offset == scratch.len() {
            active.emit_offset = 0;
            active.phase = if kind == ValueKind::Destination && active.cooked_title.is_some() {
                OccurrencePhase::EmitTitle
            } else {
                OccurrencePhase::AwaitJournal
            };
            return Ok(());
        }
        let capacity = journal.stream_capacity(kind.journal())?;
        if capacity == 0 {
            journal.poll_one(runtime)?;
            return Ok(());
        }
        let bytes = scratch.remaining_from(active.emit_offset, capacity);
        let consumed = journal.offer_stream_bytes(kind.journal(), bytes)?;
        if consumed == 0 {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference journal accepted zero retained bytes with positive capacity",
            ));
        }
        active.emit_offset = active
            .emit_offset
            .checked_add(consumed)
            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
        Ok(())
    }
    fn begin_journal<J: M11ReferenceJournalSink>(
        &mut self,
        journal: &mut J,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        if !journal.is_idle() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference journal was not idle at the occurrence boundary",
            ));
        }
        let active = self
            .active
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?;
        let normalized = std::mem::take(&mut active.definition.normalized_label)
            .into_bytes()
            .into_boxed_slice();
        let source_backed_values = journal.source_backed_values();
        let destination_len = if source_backed_values {
            0
        } else {
            active
                .cooked_destination
                .as_ref()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence lost its cooked destination",
                ))?
                .len()
        };
        let title_len = if source_backed_values {
            active.definition.logical_title.as_ref().map(|_| 0)
        } else {
            active.cooked_title.as_ref().map(CookedScratch::len)
        };
        journal.begin_occurrence_stream(
            runtime,
            M11ReferenceJournalOccurrenceStart::new(
                active
                    .source
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its source range",
                    ))?,
                active
                    .label_source
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its label range",
                    ))?,
                active.destination_source.take().ok_or(
                    M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its destination range",
                    ),
                )?,
                active.title_source.take(),
                normalized,
                destination_len,
                title_len,
            ),
        )?;
        active.phase = if source_backed_values {
            OccurrencePhase::AwaitJournal
        } else {
            OccurrencePhase::EmitDestination
        };
        Ok(())
    }

    fn begin_terminal_rewrite(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let terminal = self
            .terminal
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference terminal disappeared",
            ))?
            .terminal()
            .clone();
        if terminal.disposition == donor::DirectReferencePrefixDisposition::NoDefinitions {
            let binding = self
                .binding
                .take()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "unchanged reference terminal lost its binding",
                ))?;
            self.rewrite = Some(writer.begin_reference_output_rewrite(
                runtime,
                binding,
                M11ReferenceOutputRewrite::Unchanged,
            )?);
            self.phase = Phase::Rewrite;
            return Ok(());
        }
        let span =
            if terminal.disposition == donor::DirectReferencePrefixDisposition::VisibleRemainder {
                LogicalSpan::from_direct(
                    &terminal.logical_reference_prefix,
                    self.request.logical_base(),
                )?
            } else {
                let end = self.fragment_logical_end()?;
                LogicalSpan {
                    bytes: 0..end.bytes(),
                    utf16: 0..end.utf16(),
                }
            };
        let binding = self
            .binding
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference terminal lost its fragment binding",
            ))?;
        // Every occurrence range has already been consumed monotonically.
        // The final structural rewrite performs one independent linear prefix
        // validation, never one replay per occurrence.
        self.range_replay = None;
        let range = writer.bind_reference_output_logical_range(binding, span.green()?)?;
        self.terminal_replay = Some(writer.open_reference_output_range_replay(binding, range)?);
        self.phase = Phase::TerminalRange;
        Ok(())
    }

    fn poll_terminal_range(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let replay =
            self.terminal_replay
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "terminal range replay disappeared",
                ))?;
        match writer.poll_reference_output_cursor(runtime, replay, 1, true)? {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => Ok(()),
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready = replay.ready_chunk().len();
                if ready == 0 {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "terminal range replay reported an empty ready chunk",
                    ));
                }
                replay.consume_ready_prefix(ready)?;
                Ok(())
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => {
                let range = self
                    .terminal_replay
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "completed terminal range disappeared",
                    ))?
                    .take_completed_range()?;
                let (physical_bytes, physical_utf16) =
                    range
                        .physical_range()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "completed reference rewrite range has no physical envelope",
                        ))?;
                let checkpoint_invalidation_start =
                    super::SourceMetric::new(physical_bytes.start, physical_utf16.start).ok_or(
                        M11ReferenceRendezvousError::InvalidState(
                            "reference rewrite begins at a valid source metric",
                        ),
                    )?;
                if self
                    .checkpoint_invalidation_start
                    .replace(checkpoint_invalidation_start)
                    .is_some()
                {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference rewrite replaced two checkpoint ranges",
                    ));
                }
                let terminal = self
                    .terminal
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "terminal rewrite lost its parser disposition",
                    ))?
                    .terminal();
                let rewrite = if terminal.disposition
                    == donor::DirectReferencePrefixDisposition::VisibleRemainder
                    || terminal.disposition
                        == donor::DirectReferencePrefixDisposition::ReferenceOnly
                        && self.request.context()
                            == donor::DirectReferencePrefixContext::SetextCandidate
                {
                    M11ReferenceOutputRewrite::RetainVisibleSuffix {
                        removed_prefix: range,
                    }
                } else {
                    M11ReferenceOutputRewrite::RemoveWrapper {
                        whole_fragment: range,
                    }
                };
                let binding =
                    self.binding
                        .take()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "terminal rewrite lost its fragment binding",
                        ))?;
                self.rewrite =
                    Some(writer.begin_reference_output_rewrite(runtime, binding, rewrite)?);
                self.phase = Phase::Rewrite;
                Ok(())
            }
        }
    }

    fn poll_rewrite(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let rewrite = self
            .rewrite
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite work disappeared",
            ))?;
        let poll = writer.poll_reference_output_rewrite(runtime, rewrite, 1)?;
        let M11ReferenceOutputRewritePoll::Complete(mut authority) = poll else {
            return Ok(());
        };
        if authority.frame() != self.frame {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite returned the wrong Paragraph frame",
            ));
        }
        self.rewrite = None;
        let terminal = self
            .terminal
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite lost its parser terminal",
            ))?
            .terminal();
        let reference_only =
            terminal.disposition == donor::DirectReferencePrefixDisposition::ReferenceOnly;
        let remove = reference_only
            && self.request.context() == donor::DirectReferencePrefixContext::ParagraphFinalization;
        let expected = if remove {
            M11RecursiveGreenTerminalFragmentDisposition::Removed
        } else {
            M11RecursiveGreenTerminalFragmentDisposition::Surviving
        };
        if authority.disposition() != expected {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite disposition disagrees with parser chronology",
            ));
        }
        let visible_remainder = authority.visible_remainder_physical();
        self.remainder_boundary = authority.take_visible_remainder_boundary();
        let gap = writer.complete_reference_fragment(
            self.frame,
            remove,
            reference_only && self.staged.is_some(),
            visible_remainder,
        )?;
        if let Some(gap) = gap {
            writer.offer_reference_output_event(gap)?;
            self.phase = Phase::Gap;
        } else {
            self.phase = Phase::Commit;
        }
        Ok(())
    }

    fn poll_gap(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let status = writer.poll_reference_output(runtime, 1)?;
        if status == M11RecursiveGreenBuildStatus::NeedsInput {
            self.phase = Phase::Commit;
        }
        Ok(())
    }

    fn commit_terminal(
        &mut self,
        controller: &mut M11DirectBlockController,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let identity = self
            .identity
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference commit lost its projection identity",
            ))?;
        let terminal = self
            .terminal
            .take()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference commit lost its terminal",
            ))?;
        let disposition = terminal.terminal().disposition;
        let status =
            controller.commit_reference_prefix_terminal(terminal.acknowledge(), identity)?;
        let valid = matches!(
            (disposition, status),
            (
                donor::DirectReferencePrefixDisposition::NoDefinitions,
                donor::DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
            ) | (
                donor::DirectReferencePrefixDisposition::VisibleRemainder,
                donor::DirectReferencePrefixCommitStatus::VisibleRemainderArmed
            ) | (
                donor::DirectReferencePrefixDisposition::ReferenceOnly,
                donor::DirectReferencePrefixCommitStatus::ReferenceOnlyArmed
            )
        );
        if !valid {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference parser commit disagrees with its terminal",
            ));
        }
        if disposition == donor::DirectReferencePrefixDisposition::VisibleRemainder {
            let Some(green) = self.remainder_boundary.take() else {
                // A local high-level fragment has an exact physical suffix
                // cut, but cannot mint a Green structural boundary before the
                // enclosing convergence splice.  The rewrite is complete;
                // only the optional retrospective checkpoint is omitted.
                self.phase = Phase::Complete;
                return Ok(());
            };
            let Some(mut parser) = controller.capture_leading_reference_remainder_continuation()?
            else {
                // A visible remainder may be recognized while a later line is
                // still active (notably Setext resolution). The parse remains
                // authoritative, but this retrospective cut is then not a
                // safe line-boundary restart checkpoint.
                self.phase = Phase::Complete;
                return Ok(());
            };
            let physical = green.physical_metric();
            let cut = super::SourceMetric::new(physical.bytes(), physical.utf16()).ok_or(
                M11ReferenceRendezvousError::InvalidState(
                    "visible reference remainder cut has valid source metrics",
                ),
            )?;
            let lease = runtime.snapshot_current_source().map_err(|_| {
                M11ReferenceRendezvousError::InvalidState(
                    "visible reference remainder retains current source authority",
                )
            })?;
            parser.bind_authenticated_source_cut(&lease, cut)?;
            self.remainder = Some(M11LeadingReferenceRemainder { parser, green });
        } else {
            self.remainder_boundary = None;
        }
        self.phase = Phase::Complete;
        Ok(())
    }

    fn fragment_logical_end(
        &self,
    ) -> Result<M11RecursiveGreenLogicalPosition, M11ReferenceRendezvousError> {
        self.scan
            .as_ref()
            .map(M11ReferenceOutputCursor::logical_position)
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference fragment lost its scan cursor",
            ))
    }
}

struct ProjectedReferenceSource<'a> {
    identity: Identity,
    cursor: &'a mut M11ReferenceOutputCursor,
    virtual_lf: bool,
    virtual_raw: u8,
}

fn clip_to_fragment(
    span: &LogicalSpan,
    physical_end: M11RecursiveGreenLogicalPosition,
    has_staged_terminator: bool,
) -> Result<LogicalSpan, M11ReferenceRendezvousError> {
    if span.bytes.end <= physical_end.bytes() && span.utf16.end <= physical_end.utf16() {
        return Ok(span.clone());
    }
    if !has_staged_terminator
        || span.bytes.end != physical_end.bytes().saturating_add(1)
        || span.utf16.end != physical_end.utf16().saturating_add(1)
        || span.bytes.start > physical_end.bytes()
        || span.utf16.start > physical_end.utf16()
    {
        return Err(M11ReferenceRendezvousError::InvalidState(
            "reference range escaped the frozen fragment",
        ));
    }
    Ok(LogicalSpan {
        bytes: span.bytes.start..physical_end.bytes(),
        utf16: span.utf16.start..physical_end.utf16(),
    })
}

impl donor::DirectReferencePrefixSource for ProjectedReferenceSource<'_> {
    type Identity = Identity;
    type Error = M11BlockWriterError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn available_len(&self) -> usize {
        let physical = usize::try_from(self.cursor.available_len()).unwrap_or(usize::MAX);
        physical.saturating_add(usize::from(self.cursor.is_final() && self.virtual_lf))
    }

    fn is_final(&self) -> bool {
        self.cursor.is_final()
    }

    fn access_budget(&self) -> usize {
        self.cursor
            .ready_chunk()
            .len()
            .saturating_add(usize::from(self.cursor.is_final() && self.virtual_lf))
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        let physical = usize::try_from(self.cursor.available_len())
            .map_err(|_| M11BlockWriterError::CounterOverflow)?;
        if self.cursor.is_final() && self.virtual_lf && relative_offset == physical {
            return Ok(b'\n');
        }
        self.cursor.read_byte(
            u64::try_from(relative_offset).map_err(|_| M11BlockWriterError::CounterOverflow)?,
        )
    }

    fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8 {
        let physical = usize::try_from(self.cursor.available_len()).unwrap_or(usize::MAX);
        if self.cursor.is_final() && self.virtual_lf && logical_scalar_end_offset == physical {
            self.virtual_raw
        } else {
            u64::try_from(logical_scalar_end_offset)
                .ok()
                .map_or(0, |offset| self.cursor.raw_codepoint_contribution(offset))
        }
    }
}

fn map_donor_poll_error(
    error: donor::DirectReferencePrefixPollError<M11BlockWriterError>,
) -> M11ReferenceRendezvousError {
    match error {
        donor::DirectReferencePrefixPollError::Source(error) => error.into(),
        donor::DirectReferencePrefixPollError::ZeroFuel => {
            M11ReferenceRendezvousError::InvalidState("reference scanner received zero fuel")
        }
        donor::DirectReferencePrefixPollError::WrongSource => {
            M11ReferenceRendezvousError::InvalidState("reference scanner crossed source identity")
        }
        donor::DirectReferencePrefixPollError::SourceBudgetContractViolated => {
            M11ReferenceRendezvousError::InvalidState("reference source exceeded its access grant")
        }
        donor::DirectReferencePrefixPollError::NonSequentialSource => {
            M11ReferenceRendezvousError::InvalidState("reference source was not sequential")
        }
        donor::DirectReferencePrefixPollError::InvalidUtf8 { .. } => {
            M11ReferenceRendezvousError::InvalidState("reference projection is invalid UTF-8")
        }
        donor::DirectReferencePrefixPollError::InvalidRawCodepointContribution { .. } => {
            M11ReferenceRendezvousError::InvalidState(
                "reference projection has an invalid raw-codepoint contribution",
            )
        }
        donor::DirectReferencePrefixPollError::PollAfterComplete => {
            M11ReferenceRendezvousError::InvalidState(
                "reference scanner was polled after completion",
            )
        }
        donor::DirectReferencePrefixPollError::PollAfterCancelled => {
            M11ReferenceRendezvousError::InvalidState(
                "reference scanner was polled after cancellation",
            )
        }
        donor::DirectReferencePrefixPollError::OutputNotAcknowledged => {
            M11ReferenceRendezvousError::InvalidState("reference output was not acknowledged")
        }
        donor::DirectReferencePrefixPollError::OutputNotReady => {
            M11ReferenceRendezvousError::InvalidState("reference output was not ready")
        }
        donor::DirectReferencePrefixPollError::WrongOutput => {
            M11ReferenceRendezvousError::InvalidState("reference output acknowledgement was wrong")
        }
        donor::DirectReferencePrefixPollError::CounterOverflow => {
            M11ReferenceRendezvousError::CounterOverflow
        }
    }
}

fn map_infallible_donor_error(
    _error: donor::DirectReferencePrefixPollError<std::convert::Infallible>,
) -> M11ReferenceRendezvousError {
    M11ReferenceRendezvousError::InvalidState(
        "reference scanner rejected its linear acknowledgement",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_recursive_green_session::{
        M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanPlan,
    };
    use flark_engine::DocumentRuntimeConfig;

    fn cook_once(kind: ValueKind, source: &[u8]) -> Vec<u8> {
        let mut cook = StreamingValueCook::new(kind, 1024);
        let mut source_offset = 0;
        loop {
            match cook.poll_one().expect("single-pass value cook") {
                StreamingValuePoll::NeedsSource if source_offset < source.len() => {
                    cook.offer_source_byte(source[source_offset])
                        .expect("offer value source");
                    source_offset += 1;
                }
                StreamingValuePoll::NeedsSource => {
                    cook.finish_source().expect("finish value source");
                }
                StreamingValuePoll::Progress => {}
                StreamingValuePoll::Complete => break,
            }
        }
        let scratch = cook.take_output().expect("take cooked scratch");
        let mut output = Vec::with_capacity(scratch.len());
        let mut offset = 0;
        while offset < scratch.len() {
            let bytes = scratch.remaining_from(offset, usize::MAX);
            output.extend_from_slice(bytes);
            offset += bytes.len();
        }
        output
    }

    fn same_paragraph_reference_transitions(definitions: usize) -> usize {
        let mut source = String::new();
        for ordinal in 0..definitions {
            use std::fmt::Write as _;
            writeln!(&mut source, "[ref-{ordinal}]: /target-{ordinal}")
                .expect("reference fixture write");
        }
        let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("reference slope runtime");
        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime.snapshot_current_source().expect("scanner lease"),
            runtime.snapshot_current_source().expect("writer lease"),
            1,
        )
        .expect("reference slope plan");
        let mut build = plan.begin(&mut runtime).expect("reference slope build");
        let mut transitions = 0_usize;
        loop {
            let poll = build.poll(&mut runtime, 1).expect("reference slope poll");
            transitions = transitions
                .checked_add(poll.transitions())
                .expect("reference slope transition count");
            if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                break;
            }
        }
        let mut session = build.take_session().expect("reference slope session");
        assert_eq!(session.reference_occurrence_count(), definitions as u64);
        session
            .begin_release(&mut runtime)
            .expect("begin session release");
        while !session
            .poll_release(&mut runtime, 64)
            .expect("poll session release")
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        transitions
    }

    #[test]
    fn same_paragraph_reference_work_has_linear_doubling_slope() {
        let small = same_paragraph_reference_transitions(32);
        let doubled = same_paragraph_reference_transitions(64);
        eprintln!(
            "same_paragraph_reference_slope definitions=32 transitions={small} \
             definitions=64 transitions={doubled} ratio={:.3}",
            doubled as f64 / small as f64,
        );
        assert!(
            doubled < small * 3,
            "doubling same-Paragraph definitions grew from {small} to {doubled} transitions"
        );
    }

    #[test]
    fn single_pass_value_cooking_preserves_trim_title_entity_and_escape_semantics() {
        assert_eq!(
            cook_once(ValueKind::Destination, b" \t/a&amp;b\\* \r"),
            b"/a&b*"
        );
        assert_eq!(cook_once(ValueKind::Title, b"\"a&amp;b\\*\""), b"a&b*");
    }
}
