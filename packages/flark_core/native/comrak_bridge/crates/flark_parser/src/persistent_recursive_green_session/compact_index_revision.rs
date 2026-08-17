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
    M11CompactCheckpointEntry, M11PersistentRecursiveGreenSessionError, SourceMetric,
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
