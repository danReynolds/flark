//! Revision-local compact-index updates (RFC 029 Experiment B).
//!
//! After one committed ordinary edit, the persistent compact checkpoint
//! index converges through bounded replay from a predecessor checkpoint
//! instead of a clean rebuild from BOF. The unchanged suffix is structurally
//! shared as stored: checkpoint payload pages are translation-invariant
//! (proven by the 2026-08-17 relocatability receipt), so reuse never
//! re-encodes a shared payload, and the absolute-coordinate manifest is
//! translated through an explicit remapping layer rather than rewritten
//! record by record (RFC 029 section 5).
//!
//! Convergence equality is parser/writer continuation-state equality through
//! the durable codec, never local source-byte equality: the replayed cut and
//! the old checkpoint converge only when the encoded parser payload and the
//! encoded writer payload are byte-identical and every manifest dimension is
//! consistent with the committed edit's declared per-dimension deltas. A
//! candidate that fails any dimension is replaced, never patched; if no
//! candidate converges before EOF the replay itself becomes the complete new
//! index — bounded by the document and never wrong.

use super::{
    CleanPhase, DocumentRuntime, M11CompactCheckpointEntry,
    M11CompactCheckpointJournal, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanBuild, M11PersistentRecursiveGreenCleanPlan,
    M11PersistentRecursiveGreenSession, M11PersistentRecursiveGreenSessionError, SourceMetric,
    CHECKPOINT_STRIDE_BYTES,
};
use crate::block_core::{
    M11CompactProbeCheckpointFacts, M11CompactReferenceProbeRecord, M11CompactReferenceResolver,
    M11DirectDurableBlockRestart,
};
use std::ops::Range;

/// Names the session-error domain locally: every fail-closed rejection in the
/// revision updater is a typed invariant statement, not a panic.
type RevisionError = M11PersistentRecursiveGreenSessionError;

const fn invalid(reason: &'static str) -> RevisionError {
    RevisionError::InvalidState(reason)
}

/// Carried non-positional deltas declared by the committed edit for one
/// breakpoint: line ordinals and the monotone writer counters. Positional
/// byte/UTF-16 deltas derive from the replaced range and replacement widths;
/// these carried dimensions cannot be derived from widths alone, so the
/// committing caller declares them and the convergence equality check
/// verifies them against the actually replayed parse. A wrong declaration
/// can only suppress convergence (forcing a longer, still-correct replay);
/// it can never splice a wrong suffix.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct M11CompactRevisionCarriedDeltas {
    pub(super) line_ordinal: i64,
    pub(super) logical_bytes: i64,
    pub(super) logical_utf16: i64,
    pub(super) event_cut: i64,
    pub(super) high_level_events: i64,
    pub(super) renderable_rows: i64,
    pub(super) next_frame: i64,
}

/// One committed ordinary source edit in base-revision coordinates with its
/// declared per-dimension deltas. This is the input record for one remap
/// breakpoint: position plus per-dimension deltas, exactly the RFC 029
/// section 5 indirection contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct M11CompactRevisionEdit {
    base_bytes: Range<u64>,
    base_utf16: Range<u64>,
    replacement_bytes: u64,
    replacement_utf16: u64,
    carried: M11CompactRevisionCarriedDeltas,
}

impl M11CompactRevisionEdit {
    pub(super) fn new(
        base_bytes: Range<u64>,
        base_utf16: Range<u64>,
        replacement_bytes: u64,
        replacement_utf16: u64,
        carried: M11CompactRevisionCarriedDeltas,
    ) -> Result<Self, RevisionError> {
        if base_bytes.start > base_bytes.end
            || base_utf16.start > base_utf16.end
            || base_utf16.end - base_utf16.start > base_bytes.end - base_bytes.start
            || replacement_utf16 > replacement_bytes
        {
            return Err(invalid(
                "committed edit ranges order byte and UTF-16 dimensions consistently",
            ));
        }
        let width_fits = |value: u64| i64::try_from(value).is_ok();
        if !width_fits(base_bytes.end - base_bytes.start)
            || !width_fits(base_utf16.end - base_utf16.start)
            || !width_fits(replacement_bytes)
            || !width_fits(replacement_utf16)
        {
            return Err(invalid("committed edit widths fit the signed delta domain"));
        }
        Ok(Self {
            base_bytes,
            base_utf16,
            replacement_bytes,
            replacement_utf16,
            carried,
        })
    }

    fn byte_delta(&self) -> i64 {
        // Both operands were bounds-checked into i64 by the constructor.
        self.replacement_bytes as i64 - (self.base_bytes.end - self.base_bytes.start) as i64
    }

    fn utf16_delta(&self) -> i64 {
        self.replacement_utf16 as i64 - (self.base_utf16.end - self.base_utf16.start) as i64
    }
}

/// Cumulative per-dimension deltas through one remap breakpoint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct M11CompactRevisionRemapDeltas {
    bytes: i64,
    utf16: i64,
    line_ordinal: i64,
    logical_bytes: i64,
    logical_utf16: i64,
    event_cut: i64,
    high_level_events: i64,
    renderable_rows: i64,
    next_frame: i64,
}

/// One breakpoint of the revision remap: the replaced base range plus the
/// cumulative deltas of every breakpoint up to and including this one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M11CompactRevisionRemapBreakpoint {
    base_byte_start: u64,
    base_byte_end: u64,
    base_utf16_start: u64,
    base_utf16_end: u64,
    cumulative: M11CompactRevisionRemapDeltas,
}

/// The RFC 029 section 5 explicit remapping layer for one revision step:
/// a piecewise delta table mapping base-revision coordinates (bytes, UTF-16,
/// line ordinal, event cut, high-level events, renderable rows, logical
/// projection, frame counter) to current-revision coordinates. One committed
/// edit contributes one breakpoint; resolution is `O(log breakpoints)`
/// through binary search over the sorted breakpoint ends. Base coordinates
/// strictly inside a replaced range have no current-revision image and fail
/// closed.
#[derive(Clone, Debug, Default)]
pub(super) struct M11CompactRevisionRemap {
    breakpoints: Vec<M11CompactRevisionRemapBreakpoint>,
}

impl M11CompactRevisionRemap {
    /// Builds the table from committed edits ordered by base position. Edits
    /// must be disjoint and sorted in both coordinate dimensions; anything
    /// else is a caller error and fails closed.
    pub(super) fn from_edits(edits: &[M11CompactRevisionEdit]) -> Result<Self, RevisionError> {
        let mut breakpoints = Vec::new();
        breakpoints
            .try_reserve_exact(edits.len())
            .map_err(|_| invalid("revision remap breakpoint allocation failed"))?;
        let mut cumulative = M11CompactRevisionRemapDeltas::default();
        let mut previous_byte_end = 0_u64;
        let mut previous_utf16_end = 0_u64;
        for edit in edits {
            if edit.base_bytes.start < previous_byte_end
                || edit.base_utf16.start < previous_utf16_end
            {
                return Err(invalid(
                    "revision remap breakpoints are sorted and disjoint in both dimensions",
                ));
            }
            previous_byte_end = edit.base_bytes.end;
            previous_utf16_end = edit.base_utf16.end;
            let add = |current: i64, delta: i64, reason: &'static str| {
                current.checked_add(delta).ok_or(invalid(reason))
            };
            cumulative = M11CompactRevisionRemapDeltas {
                bytes: add(
                    cumulative.bytes,
                    edit.byte_delta(),
                    "revision remap byte delta overflowed",
                )?,
                utf16: add(
                    cumulative.utf16,
                    edit.utf16_delta(),
                    "revision remap UTF-16 delta overflowed",
                )?,
                line_ordinal: add(
                    cumulative.line_ordinal,
                    edit.carried.line_ordinal,
                    "revision remap line delta overflowed",
                )?,
                logical_bytes: add(
                    cumulative.logical_bytes,
                    edit.carried.logical_bytes,
                    "revision remap logical byte delta overflowed",
                )?,
                logical_utf16: add(
                    cumulative.logical_utf16,
                    edit.carried.logical_utf16,
                    "revision remap logical UTF-16 delta overflowed",
                )?,
                event_cut: add(
                    cumulative.event_cut,
                    edit.carried.event_cut,
                    "revision remap event delta overflowed",
                )?,
                high_level_events: add(
                    cumulative.high_level_events,
                    edit.carried.high_level_events,
                    "revision remap high-level event delta overflowed",
                )?,
                renderable_rows: add(
                    cumulative.renderable_rows,
                    edit.carried.renderable_rows,
                    "revision remap renderable-row delta overflowed",
                )?,
                next_frame: add(
                    cumulative.next_frame,
                    edit.carried.next_frame,
                    "revision remap frame delta overflowed",
                )?,
            };
            breakpoints.push(M11CompactRevisionRemapBreakpoint {
                base_byte_start: edit.base_bytes.start,
                base_byte_end: edit.base_bytes.end,
                base_utf16_start: edit.base_utf16.start,
                base_utf16_end: edit.base_utf16.end,
                cumulative,
            });
        }
        Ok(Self { breakpoints })
    }

    pub(super) fn breakpoint_count(&self) -> usize {
        self.breakpoints.len()
    }

    /// Resolves the cumulative deltas applying at one base byte coordinate,
    /// along with the count of breakpoints strictly before it (its region
    /// index). A coordinate at a replaced range's start maps by the deltas
    /// before the edit; a coordinate at its end maps by the deltas including
    /// it; a coordinate strictly inside has no image and fails closed.
    fn region_at_base_byte(
        &self,
        byte: u64,
    ) -> Result<(usize, M11CompactRevisionRemapDeltas), RevisionError> {
        let applied = self
            .breakpoints
            .partition_point(|breakpoint| breakpoint.base_byte_end <= byte);
        if let Some(next) = self.breakpoints.get(applied) {
            if next.base_byte_start < byte {
                return Err(invalid(
                    "base coordinate inside a replaced range has no revision image",
                ));
            }
        }
        let cumulative = if applied == 0 {
            M11CompactRevisionRemapDeltas::default()
        } else {
            self.breakpoints[applied - 1].cumulative
        };
        Ok((applied, cumulative))
    }

    /// Maps one base-revision physical metric into current-revision
    /// coordinates, keyed by the byte dimension.
    pub(super) fn map_metric(&self, metric: SourceMetric) -> Result<SourceMetric, RevisionError> {
        let (_, deltas) = self.region_at_base_byte(metric.bytes())?;
        let bytes = apply_delta(
            metric.bytes(),
            deltas.bytes,
            "remapped byte coordinate left the document",
        )?;
        let utf16 = apply_delta(
            metric.utf16(),
            deltas.utf16,
            "remapped UTF-16 coordinate left the document",
        )?;
        SourceMetric::new(bytes, utf16)
            .ok_or(invalid("remapped physical metric is structurally valid"))
    }

    /// Translates one base-revision manifest entry into current-revision
    /// coordinates without touching its payload records. The entry is
    /// classified by its accepted cut; an entry whose accepted and parser
    /// cuts straddle a breakpoint has no uniform image and fails closed.
    pub(super) fn resolve_entry(
        &self,
        entry: &M11CompactCheckpointEntry,
    ) -> Result<M11CompactRevisionResolvedEntry, RevisionError> {
        let (accepted_region, deltas) = self.region_at_base_byte(entry.accepted_physical.bytes())?;
        let (parser_region, _) = self.region_at_base_byte(entry.parser_physical.bytes())?;
        if accepted_region != parser_region {
            return Err(invalid(
                "manifest entry straddles a replaced range and has no uniform image",
            ));
        }
        if deltas.next_frame != 0 && entry.cold_document_frame.is_some() {
            // Translating individual frame identities under a nonzero frame
            // delta requires the allocation-watermark rule; no measured
            // receipt covers it yet, so it stays fail-closed future work.
            return Err(invalid(
                "frame-identity translation under a nonzero frame delta is not certified",
            ));
        }
        Ok(M11CompactRevisionResolvedEntry {
            line_ordinal: apply_delta(
                entry.line_ordinal,
                deltas.line_ordinal,
                "remapped line ordinal left the document",
            )?,
            last_line_length: entry.last_line_length,
            accepted_physical: self.map_metric(entry.accepted_physical)?,
            parser_physical: self.map_metric(entry.parser_physical)?,
            logical: {
                let logical_bytes = apply_delta(
                    entry.logical.bytes(),
                    deltas.logical_bytes,
                    "remapped logical byte coordinate left the document",
                )?;
                let logical_utf16 = apply_delta(
                    entry.logical.utf16(),
                    deltas.logical_utf16,
                    "remapped logical UTF-16 coordinate left the document",
                )?;
                SourceMetric::new(logical_bytes, logical_utf16)
                    .ok_or(invalid("remapped logical metric is structurally valid"))?
            },
            event_cut: apply_delta(
                entry.event_cut,
                deltas.event_cut,
                "remapped event cut left the journal",
            )?,
            high_level_events: apply_delta(
                entry.high_level_events,
                deltas.high_level_events,
                "remapped high-level event count left the journal",
            )?,
            renderable_rows: apply_delta(
                entry.renderable_rows,
                deltas.renderable_rows,
                "remapped renderable-row count left the journal",
            )?,
            open_depth: entry.open_depth,
            cold_document_frame: entry.cold_document_frame,
            cold_staged_blank_gap: entry.cold_staged_blank_gap,
            next_frame: apply_delta(
                entry.next_frame,
                deltas.next_frame,
                "remapped frame counter left the journal",
            )?,
            encoded_len: entry.encoded_len,
            writer_encoded_len: entry.writer_encoded_len,
        })
    }
}

fn apply_delta(value: u64, delta: i64, reason: &'static str) -> Result<u64, RevisionError> {
    let shifted = i128::from(value) + i128::from(delta);
    u64::try_from(shifted).map_err(|_| invalid(reason))
}

/// One manifest entry resolved into current-revision coordinates. Stream
/// offsets are deliberately absent: payload identity is exposed through the
/// owning index's payload accessors, never through rewritten offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct M11CompactRevisionResolvedEntry {
    pub(super) line_ordinal: u64,
    pub(super) last_line_length: u64,
    pub(super) accepted_physical: SourceMetric,
    pub(super) parser_physical: SourceMetric,
    pub(super) logical: SourceMetric,
    pub(super) event_cut: u64,
    pub(super) high_level_events: u64,
    pub(super) renderable_rows: u64,
    pub(super) open_depth: u32,
    pub(super) cold_document_frame:
        Option<flark_engine::parser_internal::M11RecursiveGreenFrameId>,
    pub(super) cold_staged_blank_gap: Option<SourceMetric>,
    pub(super) next_frame: u64,
    pub(super) encoded_len: u32,
    pub(super) writer_encoded_len: u32,
}

impl M11CompactRevisionResolvedEntry {
    pub(super) fn from_current_entry(entry: &M11CompactCheckpointEntry) -> Self {
        Self {
            line_ordinal: entry.line_ordinal,
            last_line_length: entry.last_line_length,
            accepted_physical: entry.accepted_physical,
            parser_physical: entry.parser_physical,
            logical: entry.logical,
            event_cut: entry.event_cut,
            high_level_events: entry.high_level_events,
            renderable_rows: entry.renderable_rows,
            open_depth: entry.open_depth,
            cold_document_frame: entry.cold_document_frame,
            cold_staged_blank_gap: entry.cold_staged_blank_gap,
            next_frame: entry.next_frame,
            encoded_len: entry.encoded_len,
            writer_encoded_len: entry.writer_encoded_len,
        }
    }
}

/// Where one revision-index entry's records live: `Base` entries keep their
/// original manifest record and payload stream position (payload shared as
/// stored, coordinates translated through the remap on resolution), `Window`
/// entries were re-emitted by the bounded replay into the appended overlay
/// stream and already carry current-revision coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11CompactRevisionEntrySource {
    Base(u32),
    Window(u32),
}

/// Reference-index disposition for one revision step. A replayed window
/// containing no reference definitions carries the base first-winner index
/// forward under the remap. A definition-bearing window pays an honest
/// whole-document reference rebuild in this v1 — recorded in the receipt's
/// definition counts, never silent; bounding that rebuild is declared future
/// work (RFC 029 section 5.2).
#[derive(Debug)]
pub(super) enum M11CompactRevisionReferences {
    CarriedForward {
        resolver: M11CompactReferenceResolver,
    },
    RebuildRequired,
}

/// Storage and locality receipt for one revision update, in the relocatability
/// probe's vocabulary: reused checkpoints are prefix plus suffix entries whose
/// payload bytes were shared as stored, replaced checkpoints are base entries
/// superseded by the replayed window, and appended pages are the only new
/// payload storage this revision allocated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct M11CompactIndexRevisionReceipt {
    pub(super) predecessor_index: usize,
    pub(super) predecessor_cut_bytes: u64,
    pub(super) replay_start_bytes: u64,
    pub(super) source_bytes_replayed: u64,
    pub(super) boundaries_observed: usize,
    pub(super) replay_transitions: u64,
    pub(super) converged: bool,
    pub(super) convergence_entry_index: Option<usize>,
    pub(super) convergence_cut_bytes: Option<u64>,
    pub(super) candidates_rejected: usize,
    pub(super) checkpoints_reused_prefix: usize,
    pub(super) checkpoints_reused_suffix: usize,
    pub(super) checkpoints_replaced: usize,
    pub(super) checkpoints_window: usize,
    pub(super) base_entries_total: usize,
    pub(super) pages_shared: usize,
    pub(super) pages_appended: usize,
    pub(super) base_stream_bytes: usize,
    pub(super) overlay_stream_bytes: usize,
    pub(super) window_definition_records: usize,
    pub(super) base_definitions_intersecting: usize,
    pub(super) references_rebuild_required: bool,
    pub(super) last_convergence_reject: Option<(usize, &'static str)>,
}

/// The updated persistent compact index for the current revision: the base
/// journal retained whole (pages and manifest as stored), the appended
/// overlay holding only the replayed window's records, the spliced entry
/// order, and the remap that translates retained base coordinates. Nothing
/// in the base journal was rewritten to build this value.
#[derive(Debug)]
pub(super) struct M11CompactIndexRevision {
    base: M11CompactCheckpointJournal,
    overlay: M11CompactCheckpointJournal,
    entries: Vec<M11CompactRevisionEntrySource>,
    remap: M11CompactRevisionRemap,
    references: M11CompactRevisionReferences,
    receipt: M11CompactIndexRevisionReceipt,
}

impl M11CompactIndexRevision {
    pub(super) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn receipt(&self) -> &M11CompactIndexRevisionReceipt {
        &self.receipt
    }

    pub(super) fn references(&self) -> &M11CompactRevisionReferences {
        &self.references
    }

    /// Resolves one entry into current-revision coordinates. Base entries
    /// translate through the remap at query time; their stored records are
    /// never rewritten.
    pub(super) fn resolved_entry(
        &self,
        index: usize,
    ) -> Result<M11CompactRevisionResolvedEntry, RevisionError> {
        match self.entry_source(index)? {
            M11CompactRevisionEntrySource::Base(base_index) => {
                let entry = self
                    .base
                    .entries
                    .get(base_index as usize)
                    .ok_or(invalid("revision base entry index is in bounds"))?;
                self.remap.resolve_entry(entry)
            }
            M11CompactRevisionEntrySource::Window(window_index) => {
                let entry = self
                    .overlay
                    .entries
                    .get(window_index as usize)
                    .ok_or(invalid("revision window entry index is in bounds"))?;
                Ok(M11CompactRevisionResolvedEntry::from_current_entry(entry))
            }
        }
    }

    /// Returns the stored parser payload record for one entry, bit-exact from
    /// whichever stream retains it.
    pub(super) fn encoded_parser_record(&self, index: usize) -> Result<Vec<u8>, RevisionError> {
        match self.entry_source(index)? {
            M11CompactRevisionEntrySource::Base(base_index) => self
                .base
                .encoded_entry(base_index as usize)
                .map_err(invalid),
            M11CompactRevisionEntrySource::Window(window_index) => self
                .overlay
                .encoded_entry(window_index as usize)
                .map_err(invalid),
        }
    }

    /// Returns the stored bounded writer restart record for one entry, or
    /// `None` for checkpoints that declare the cold-jump fallback.
    pub(super) fn encoded_writer_record(
        &self,
        index: usize,
    ) -> Result<Option<Vec<u8>>, RevisionError> {
        let (journal, journal_index) = match self.entry_source(index)? {
            M11CompactRevisionEntrySource::Base(base_index) => (&self.base, base_index as usize),
            M11CompactRevisionEntrySource::Window(window_index) => {
                (&self.overlay, window_index as usize)
            }
        };
        let entry = journal
            .entries
            .get(journal_index)
            .ok_or(invalid("revision entry index is in bounds"))?;
        if entry.writer_encoded_len == 0 {
            return Ok(None);
        }
        journal
            .encoded_writer_entry(journal_index)
            .map(Some)
            .map_err(invalid)
    }

    /// The carried-forward reference records translated into current-revision
    /// coordinates, or `None` when this revision requires a reference
    /// rebuild.
    pub(super) fn resolved_reference_records(
        &self,
    ) -> Result<Option<Vec<M11CompactReferenceProbeRecord>>, RevisionError> {
        let M11CompactRevisionReferences::CarriedForward { resolver } = &self.references else {
            return Ok(None);
        };
        let records = resolver.probe_records();
        let mut resolved = Vec::new();
        resolved
            .try_reserve_exact(records.len())
            .map_err(|_| invalid("carried reference record allocation failed"))?;
        for record in records {
            resolved.push(self.resolve_reference_record(record)?);
        }
        Ok(Some(resolved))
    }

    fn resolve_reference_record(
        &self,
        record: M11CompactReferenceProbeRecord,
    ) -> Result<M11CompactReferenceProbeRecord, RevisionError> {
        let (start_region, deltas) = self
            .remap
            .region_at_base_byte(u64::from(record.source.start))?;
        let (end_region, _) = self.remap.region_at_base_byte(u64::from(record.source.end))?;
        if start_region != end_region {
            return Err(invalid(
                "carried reference record straddles a replaced range",
            ));
        }
        let shift = |range: &Range<u32>| -> Result<Range<u32>, RevisionError> {
            let start = apply_delta(
                u64::from(range.start),
                deltas.bytes,
                "remapped reference coordinate left the document",
            )?;
            let end = apply_delta(
                u64::from(range.end),
                deltas.bytes,
                "remapped reference coordinate left the document",
            )?;
            Ok(u32::try_from(start).map_err(|_| {
                invalid("remapped reference coordinate exceeds the candidate ABI")
            })?
                ..u32::try_from(end).map_err(|_| {
                    invalid("remapped reference coordinate exceeds the candidate ABI")
                })?)
        };
        Ok(M11CompactReferenceProbeRecord {
            digest: record.digest,
            label: record.label,
            source: shift(&record.source)?,
            destination: shift(&record.destination)?,
            title: record.title.as_ref().map(&shift).transpose()?,
            winner: record.winner,
        })
    }

    fn entry_source(&self, index: usize) -> Result<M11CompactRevisionEntrySource, RevisionError> {
        self.entries
            .get(index)
            .copied()
            .ok_or(invalid("revision entry index is in bounds"))
    }

    /// Revalidates the spliced manifest in current-revision coordinates:
    /// strictly advancing physical cuts and monotone counters across every
    /// seam (prefix to window, window to suffix), exactly the invariants the
    /// contiguous journal validates for itself.
    fn validate_resolved_monotonicity(&self) -> Result<(), RevisionError> {
        let mut previous: Option<M11CompactRevisionResolvedEntry> = None;
        for index in 0..self.entries.len() {
            let resolved = self.resolved_entry(index)?;
            if let Some(previous) = previous {
                if previous.accepted_physical >= resolved.accepted_physical
                    || previous.parser_physical >= resolved.parser_physical
                    || previous.logical > resolved.logical
                    || previous.event_cut > resolved.event_cut
                    || previous.high_level_events > resolved.high_level_events
                    || previous.renderable_rows > resolved.renderable_rows
                    || previous.next_frame > resolved.next_frame
                    || previous.line_ordinal >= resolved.line_ordinal
                {
                    return Err(invalid(
                        "spliced revision manifest is monotone in current coordinates",
                    ));
                }
            }
            previous = Some(resolved);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum M11CompactIndexRevisionStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct M11CompactIndexRevisionPoll {
    status: M11CompactIndexRevisionStatus,
    transitions: usize,
}

impl M11CompactIndexRevisionPoll {
    pub(super) const fn status(self) -> M11CompactIndexRevisionStatus {
        self.status
    }

    pub(super) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RevisionUpdatePhase {
    Replay,
    ReleaseSession,
    Assemble,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RevisionConvergence {
    candidate_index: usize,
    cut: SourceMetric,
}

/// Caller-fuelled convergence updater for one committed ordinary edit: it
/// resumes the real primary parser from the nearest resumable predecessor
/// checkpoint (BOF only when nothing nearer can resume), replays forward
/// re-emitting window checkpoints at the production stride cadence, tests
/// continuation-state convergence against each surviving base checkpoint at
/// its remapped cut, and splices prefix, window, and structurally shared
/// suffix into an [`M11CompactIndexRevision`]. A replay that never converges
/// runs to EOF and becomes the complete new index — bounded by the document,
/// never wrong.
#[must_use = "revision updates require completion or explicit abandonment"]
pub(super) struct M11CompactIndexRevisionUpdate {
    phase: RevisionUpdatePhase,
    base: Option<M11CompactCheckpointJournal>,
    base_references: Option<M11CompactReferenceResolver>,
    remap: M11CompactRevisionRemap,
    predecessor_index: usize,
    replay_seed_cut: SourceMetric,
    replay_start: SourceMetric,
    next_candidate: usize,
    candidates_rejected: usize,
    overlay: M11CompactCheckpointJournal,
    build: Option<M11PersistentRecursiveGreenCleanBuild>,
    completed_session: Option<M11PersistentRecursiveGreenSession>,
    boundaries_seen: usize,
    boundaries_observed: usize,
    replay_transitions: u64,
    window_frontier: SourceMetric,
    window_definition_records: usize,
    convergence: Option<RevisionConvergence>,
    last_convergence_reject: Option<(usize, &'static str)>,
    output: Option<M11CompactIndexRevision>,
}

impl M11CompactIndexRevisionUpdate {
    /// Starts one revision update. `base` is the compact index built at the
    /// predecessor revision, `base_references` its first-winner authority,
    /// `edit` the one committed ordinary edit separating that revision from
    /// the runtime's current source, which must already be committed.
    pub(super) fn begin(
        base: M11CompactCheckpointJournal,
        base_references: M11CompactReferenceResolver,
        edit: M11CompactRevisionEdit,
        runtime: &mut DocumentRuntime,
        syntax_profile: u32,
    ) -> Result<Self, RevisionError> {
        if syntax_profile == 0 {
            return Err(invalid("revision update requires a declared syntax profile"));
        }
        base.validate_metadata_and_durable_samples()
            .map_err(invalid)?;
        // The last checkpoint's parser cut is the true consumed frontier: a
        // trailing staged blank gap keeps the accepted cut short of EOF.
        let base_end = base
            .entries
            .last()
            .map(|entry| entry.parser_physical)
            .ok_or(invalid("revision update requires a nonempty base journal"))?;
        if edit.base_bytes.end > base_end.bytes() || edit.base_utf16.end > base_end.utf16() {
            return Err(invalid("committed edit lies inside the base source extent"));
        }
        let remap = M11CompactRevisionRemap::from_edits(std::slice::from_ref(&edit))?;
        let current = runtime
            .current_source_version()
            .ok_or(invalid("revision update requires an open current source"))?;
        let expected_bytes = apply_delta(
            base_end.bytes(),
            remap.breakpoints.last().map_or(0, |b| b.cumulative.bytes),
            "edited source extent reconciles with the base extent",
        )?;
        let expected_utf16 = apply_delta(
            base_end.utf16(),
            remap.breakpoints.last().map_or(0, |b| b.cumulative.utf16),
            "edited source extent reconciles with the base extent",
        )?;
        if current.byte_len() as u64 != expected_bytes || current.utf16_len() as u64 != expected_utf16
        {
            return Err(invalid(
                "committed edit description does not reconcile base and current source extents",
            ));
        }

        // Nearest predecessor whose durable state precedes every replaced
        // byte. Selection is on the parser cut, not the accepted cut: a
        // checkpoint whose staged blank gap reaches past the edit start
        // consumed replaced bytes and cannot anchor the replay. Falling
        // back past checkpoints without a bounded writer restart record is
        // the declared cold-jump fallback; BOF resumes through the
        // canonical fresh-parser constructor.
        let boundary = base
            .entries
            .partition_point(|entry| entry.parser_physical.bytes() <= edit.base_bytes.start);
        let mut predecessor_index = boundary.saturating_sub(1);
        while predecessor_index > 0 && base.entries[predecessor_index].writer_encoded_len == 0 {
            predecessor_index -= 1;
        }
        let predecessor = base.entries[predecessor_index];
        // Convergence candidates: base checkpoints whose accepted cut sits at
        // or beyond the replaced range's base end. Anything nearer either
        // precedes the edit (retained prefix) or has no current-revision
        // image (replaced by the window). Testing may only begin past the
        // replaced range: converging on an untouched pre-edit boundary would
        // splice an unverified suffix across the edit.
        let first_candidate = base
            .entries
            .partition_point(|entry| entry.accepted_physical.bytes() < edit.base_bytes.end)
            .max(predecessor_index + 1);

        let build = if predecessor_index == 0 {
            M11PersistentRecursiveGreenCleanPlan::new(
                runtime.snapshot_current_source()?,
                runtime.snapshot_current_source()?,
                syntax_profile,
            )?
            .begin_compact_probe(runtime)?
        } else {
            let (build, entry, _open_frame_bases) =
                base.begin_cold_slice_probe(predecessor_index, runtime, syntax_profile)?;
            if entry != predecessor {
                return Err(invalid("cold-resumed predecessor is the selected entry"));
            }
            build
        };
        let replay_seed_cut = if predecessor_index == 0 {
            SourceMetric::default()
        } else {
            predecessor.accepted_physical
        };
        let replay_start = if predecessor_index == 0 {
            SourceMetric::default()
        } else {
            predecessor.parser_physical
        };
        Ok(Self {
            phase: RevisionUpdatePhase::Replay,
            base: Some(base),
            base_references: Some(base_references),
            remap,
            predecessor_index,
            replay_seed_cut,
            replay_start,
            next_candidate: first_candidate,
            candidates_rejected: 0,
            overlay: M11CompactCheckpointJournal::new(),
            build: Some(build),
            completed_session: None,
            boundaries_seen: 0,
            boundaries_observed: 0,
            replay_transitions: 0,
            window_frontier: replay_start,
            window_definition_records: 0,
            convergence: None,
            last_convergence_reject: None,
            output: None,
        })
    }

    pub(super) fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11CompactIndexRevisionPoll, RevisionError> {
        if fuel == 0 {
            return Err(RevisionError::ZeroFuel);
        }
        let mut transitions = 0;
        while transitions < fuel && self.phase != RevisionUpdatePhase::Complete {
            transitions += 1;
            match self.phase {
                RevisionUpdatePhase::Replay => self.poll_replay(runtime)?,
                RevisionUpdatePhase::ReleaseSession => {
                    let session = self
                        .completed_session
                        .as_mut()
                        .ok_or(invalid("EOF revision replay retains its session"))?;
                    if session.poll_release(runtime, 1)? {
                        self.completed_session = None;
                        self.phase = RevisionUpdatePhase::Assemble;
                    }
                }
                RevisionUpdatePhase::Assemble => self.assemble()?,
                RevisionUpdatePhase::Complete => {}
            }
        }
        Ok(M11CompactIndexRevisionPoll {
            status: if self.phase == RevisionUpdatePhase::Complete {
                M11CompactIndexRevisionStatus::Complete
            } else {
                M11CompactIndexRevisionStatus::Pending
            },
            transitions,
        })
    }

    #[must_use]
    pub(super) fn take_revision(&mut self) -> Option<M11CompactIndexRevision> {
        (self.phase == RevisionUpdatePhase::Complete)
            .then(|| self.output.take())
            .flatten()
    }

    fn poll_replay(&mut self, runtime: &mut DocumentRuntime) -> Result<(), RevisionError> {
        let mut build = self
            .build
            .take()
            .ok_or(invalid("revision replay retains its build"))?;
        let outcome = self.poll_replay_step(runtime, &mut build);
        match outcome {
            Ok(status) => {
                if self.convergence.is_some() {
                    // Compact-probe teardown is synchronous and harness-owned:
                    // the build holds no Green root, only plain parser/writer
                    // state and source leases that release on drop — the same
                    // discipline the clean build applies when it discards its
                    // compact writer at completion.
                    drop(build);
                    self.build = None;
                    self.phase = RevisionUpdatePhase::Assemble;
                    return Ok(());
                }
                match status {
                    M11PersistentRecursiveGreenBuildStatus::Pending => {
                        self.build = Some(build);
                    }
                    M11PersistentRecursiveGreenBuildStatus::Complete => {
                        let mut session = build
                            .take_session()
                            .ok_or(invalid("completed revision replay yields its session"))?;
                        session.begin_release(runtime)?;
                        self.completed_session = Some(session);
                        self.build = None;
                        self.phase = RevisionUpdatePhase::ReleaseSession;
                    }
                    _ => {
                        self.build = Some(build);
                        return Err(invalid(
                            "revision replay reached an impossible build status",
                        ));
                    }
                }
                Ok(())
            }
            Err(error) => {
                self.build = Some(build);
                Err(error)
            }
        }
    }

    fn poll_replay_step(
        &mut self,
        runtime: &mut DocumentRuntime,
        build: &mut M11PersistentRecursiveGreenCleanBuild,
    ) -> Result<M11PersistentRecursiveGreenBuildStatus, RevisionError> {
        let poll = build.poll(runtime, 1)?;
        self.replay_transitions = self
            .replay_transitions
            .checked_add(1)
            .ok_or(invalid("revision replay transition count fits u64"))?;
        if build.compact_checkpoint_boundaries_seen != self.boundaries_seen {
            self.boundaries_seen = build.compact_checkpoint_boundaries_seen;
            self.boundaries_observed = self
                .boundaries_observed
                .checked_add(1)
                .ok_or(invalid("revision boundary count fits usize"))?;
            // The forced EOF capture transitions the build out of its
            // scanning phases in the same step; every interior boundary
            // returns to scanning.
            let at_eof = matches!(build.phase, CleanPhase::BeginFinish);
            self.observe_boundary(build, at_eof)?;
        }
        Ok(poll.status())
    }

    /// Runs at every quiescent physical-line boundary the replay reaches,
    /// with the writer and controller still parked exactly at that boundary.
    /// Convergence is tested before the cadence capture so a converged
    /// boundary is never duplicated into the window.
    fn observe_boundary(
        &mut self,
        build: &M11PersistentRecursiveGreenCleanBuild,
        at_eof: bool,
    ) -> Result<(), RevisionError> {
        let writer = build
            .writer
            .as_ref()
            .ok_or(invalid("replay boundary retains its writer"))?;
        let (metric, open_depth) = writer.compact_probe_checkpoint_candidate()?;
        self.window_frontier = metric;
        if let Some(journal) = build.compact_reference_journal.as_ref() {
            self.window_definition_records = journal.receipt().occurrences;
        }

        let mut captured: Option<(M11DirectDurableBlockRestart, M11CompactProbeCheckpointFacts)> =
            None;
        loop {
            let Some(candidate) = self
                .base
                .as_ref()
                .ok_or(invalid("revision replay retains its base journal"))?
                .entries
                .get(self.next_candidate)
                .copied()
            else {
                break;
            };
            let Ok(target) = self.remap.map_metric(candidate.accepted_physical) else {
                // No current-revision image: the base cut died inside the
                // replaced range and is definitionally superseded.
                self.next_candidate += 1;
                self.candidates_rejected += 1;
                continue;
            };
            if target.bytes() < metric.bytes() {
                // The replay moved past this cut without converging on it.
                self.next_candidate += 1;
                self.candidates_rejected += 1;
                continue;
            }
            if target.bytes() > metric.bytes() {
                break;
            }
            if target.utf16() != metric.utf16() {
                self.record_reject("aligned byte cut disagrees on the UTF-16 dimension");
                self.next_candidate += 1;
                self.candidates_rejected += 1;
                continue;
            }
            if captured.is_none() {
                captured = Self::capture_boundary_state(build)?;
                if captured.is_none() {
                    // The base build captured durable state here; the
                    // replayed parse cannot. That is a structural
                    // divergence, so the candidate is replaced.
                    self.record_reject("durable state is unavailable at the aligned cut");
                    self.next_candidate += 1;
                    self.candidates_rejected += 1;
                    break;
                }
            }
            let (parser, facts) = captured
                .as_ref()
                .ok_or(invalid("aligned boundary retains its captured state"))?;
            match self.test_convergence(self.next_candidate, &candidate, parser, facts)? {
                Ok(()) => {
                    self.convergence = Some(RevisionConvergence {
                        candidate_index: self.next_candidate,
                        cut: metric,
                    });
                    return Ok(());
                }
                Err(reason) => {
                    self.record_reject(reason);
                    self.next_candidate += 1;
                    self.candidates_rejected += 1;
                }
            }
        }

        // Window re-emission at the production stride cadence, seeded from
        // the predecessor cut so the replayed window reproduces exactly the
        // selection a clean build over the current source would make from
        // the same predecessor.
        let previous_cut = self.overlay.last_cut().unwrap_or(self.replay_seed_cut);
        if previous_cut == metric {
            return Ok(());
        }
        let minimum_stride = CHECKPOINT_STRIDE_BYTES
            .checked_mul(u64::try_from(open_depth).unwrap_or(u64::MAX).max(1))
            .ok_or(invalid("revision checkpoint spacing fits u64"))?;
        if !at_eof && metric.bytes().saturating_sub(previous_cut.bytes()) < minimum_stride {
            return Ok(());
        }
        if captured.is_none() {
            captured = Self::capture_boundary_state(build)?;
        }
        let Some((parser, facts)) = captured else {
            // Mirrors the clean builder: a boundary without durable state is
            // simply not a checkpoint.
            return Ok(());
        };
        if facts.accepted_physical != metric || facts.open_depth != open_depth {
            return Err(invalid(
                "revision checkpoint selection and joined durable facts differ",
            ));
        }
        self.overlay.push(&parser, facts).map_err(invalid)?;
        Ok(())
    }

    fn capture_boundary_state(
        build: &M11PersistentRecursiveGreenCleanBuild,
    ) -> Result<Option<(M11DirectDurableBlockRestart, M11CompactProbeCheckpointFacts)>, RevisionError>
    {
        let controller = build
            .controller
            .as_ref()
            .ok_or(invalid("replay boundary retains its controller"))?;
        let Some(parser) = controller.capture_durable_restart_if_available()? else {
            return Ok(None);
        };
        let writer = build
            .writer
            .as_ref()
            .ok_or(invalid("replay boundary retains its writer"))?;
        let facts = writer.capture_compact_probe_checkpoint_facts(&parser)?;
        Ok(Some((parser, facts)))
    }

    /// The Experiment B convergence equality: encoded parser payload bytes
    /// equal, encoded writer payload bytes equal, and every manifest
    /// dimension consistent with the committed edit's declared deltas at the
    /// same logical boundary. Equal local source bytes are never consulted;
    /// state equality goes through the durable codec alone. The inner
    /// `Err(&str)` names the first dimension that broke, for the receipt.
    #[allow(clippy::type_complexity)]
    fn test_convergence(
        &self,
        candidate_index: usize,
        candidate: &M11CompactCheckpointEntry,
        parser: &M11DirectDurableBlockRestart,
        facts: &M11CompactProbeCheckpointFacts,
    ) -> Result<Result<(), &'static str>, RevisionError> {
        let base = self
            .base
            .as_ref()
            .ok_or(invalid("revision replay retains its base journal"))?;
        let resolved = match self.remap.resolve_entry(candidate) {
            Ok(resolved) => resolved,
            Err(_) => return Ok(Err("candidate manifest entry has no uniform remap image")),
        };
        if facts.accepted_physical != resolved.accepted_physical {
            return Ok(Err("accepted physical cut"));
        }
        if facts.parser_physical != resolved.parser_physical {
            return Ok(Err("parser physical cut"));
        }
        if facts.logical != resolved.logical {
            return Ok(Err("logical projection metric"));
        }
        if parser.line_ordinal() != resolved.line_ordinal {
            return Ok(Err("line ordinal"));
        }
        if parser.last_line_length() != resolved.last_line_length {
            return Ok(Err("last line length"));
        }
        if facts.event_cut != resolved.event_cut {
            return Ok(Err("event cut"));
        }
        if facts.high_level_events != resolved.high_level_events {
            return Ok(Err("high-level event count"));
        }
        if facts.renderable_rows != resolved.renderable_rows {
            return Ok(Err("renderable row count"));
        }
        if facts.open_depth != resolved.open_depth as usize {
            return Ok(Err("open container depth"));
        }
        if facts.cold_document_frame != resolved.cold_document_frame {
            return Ok(Err("cold document frame identity"));
        }
        if facts.cold_staged_blank_gap != resolved.cold_staged_blank_gap {
            return Ok(Err("staged blank gap"));
        }
        if facts.next_frame != resolved.next_frame {
            return Ok(Err("frame counter"));
        }
        let mut live_parser_payload = Vec::new();
        live_parser_payload
            .try_reserve_exact(parser.encoded_len())
            .map_err(|_| invalid("convergence parser payload allocation failed"))?;
        parser.visit_encoded_bytes(|bytes| live_parser_payload.extend_from_slice(bytes));
        let stored_parser_payload = base
            .encoded_entry(candidate_index)
            .map_err(invalid)?;
        if live_parser_payload != stored_parser_payload {
            return Ok(Err("encoded parser payload bytes"));
        }
        let writer_equal = match (candidate.writer_encoded_len, &facts.cold_container_restart) {
            (0, None) => true,
            (0, Some(_)) | (_, None) => false,
            (_, Some(live_writer_payload)) => {
                let stored_writer_payload = base
                    .encoded_writer_entry(candidate_index)
                    .map_err(invalid)?;
                *live_writer_payload == stored_writer_payload
            }
        };
        if !writer_equal {
            return Ok(Err("encoded writer payload bytes"));
        }
        Ok(Ok(()))
    }

    fn record_reject(&mut self, reason: &'static str) {
        self.last_convergence_reject = Some((self.next_candidate, reason));
    }

    fn assemble(&mut self) -> Result<(), RevisionError> {
        let base = self
            .base
            .take()
            .ok_or(invalid("revision assembly retains its base journal"))?;
        let base_references = self
            .base_references
            .take()
            .ok_or(invalid("revision assembly retains its reference authority"))?;
        let overlay = std::mem::take(&mut self.overlay);
        let base_total = base.entries.len();
        let prefix_len = self.predecessor_index + 1;
        let suffix_start = match self.convergence {
            Some(convergence) => convergence.candidate_index,
            None => base_total,
        };
        if suffix_start < prefix_len {
            return Err(invalid("revision suffix begins after its retained prefix"));
        }
        let suffix_len = base_total - suffix_start;
        let replaced = suffix_start - prefix_len;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(prefix_len + overlay.entries.len() + suffix_len)
            .map_err(|_| invalid("revision entry allocation failed"))?;
        for index in 0..prefix_len {
            entries.push(M11CompactRevisionEntrySource::Base(
                u32::try_from(index).map_err(|_| invalid("revision entry index fits u32"))?,
            ));
        }
        for index in 0..overlay.entries.len() {
            entries.push(M11CompactRevisionEntrySource::Window(
                u32::try_from(index).map_err(|_| invalid("revision entry index fits u32"))?,
            ));
        }
        for index in suffix_start..base_total {
            entries.push(M11CompactRevisionEntrySource::Base(
                u32::try_from(index).map_err(|_| invalid("revision entry index fits u32"))?,
            ));
        }

        // Reference disposition: the replayed window's own definition records
        // and the base definitions intersecting the replayed base range both
        // force the honest whole-document rebuild.
        let window_end_base = match self.convergence {
            Some(convergence) => base.entries[convergence.candidate_index]
                .accepted_physical
                .bytes(),
            None => base
                .entries
                .last()
                .map(|entry| entry.parser_physical.bytes())
                .ok_or(invalid("revision assembly retains base entries"))?,
        };
        let window_start_base = self.replay_start.bytes();
        let base_definitions_intersecting = base_references
            .probe_records()
            .iter()
            .filter(|record| {
                u64::from(record.source.start) < window_end_base
                    && u64::from(record.source.end) > window_start_base
            })
            .count();
        let references = if self.window_definition_records > 0 || base_definitions_intersecting > 0
        {
            M11CompactRevisionReferences::RebuildRequired
        } else {
            M11CompactRevisionReferences::CarriedForward {
                resolver: base_references,
            }
        };

        let receipt = M11CompactIndexRevisionReceipt {
            predecessor_index: self.predecessor_index,
            predecessor_cut_bytes: self.replay_seed_cut.bytes(),
            replay_start_bytes: self.replay_start.bytes(),
            source_bytes_replayed: self
                .window_frontier
                .bytes()
                .saturating_sub(self.replay_start.bytes()),
            boundaries_observed: self.boundaries_observed,
            replay_transitions: self.replay_transitions,
            converged: self.convergence.is_some(),
            convergence_entry_index: self.convergence.map(|c| c.candidate_index),
            convergence_cut_bytes: self.convergence.map(|c| c.cut.bytes()),
            candidates_rejected: self.candidates_rejected,
            checkpoints_reused_prefix: prefix_len,
            checkpoints_reused_suffix: suffix_len,
            checkpoints_replaced: replaced,
            checkpoints_window: overlay.entries.len(),
            base_entries_total: base_total,
            pages_shared: base.pages.len(),
            pages_appended: overlay.pages.len(),
            base_stream_bytes: base.stream_len,
            overlay_stream_bytes: overlay.stream_len,
            window_definition_records: self.window_definition_records,
            base_definitions_intersecting,
            references_rebuild_required: matches!(
                references,
                M11CompactRevisionReferences::RebuildRequired
            ),
            last_convergence_reject: self.last_convergence_reject,
        };
        let revision = M11CompactIndexRevision {
            base,
            overlay,
            entries,
            remap: self.remap.clone(),
            references,
            receipt,
        };
        revision.validate_resolved_monotonicity()?;
        self.output = Some(revision);
        self.phase = RevisionUpdatePhase::Complete;
        Ok(())
    }
}
