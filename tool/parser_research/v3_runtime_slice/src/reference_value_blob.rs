//! Transaction-local materialization of parser-authenticated reference values.
//!
//! The parser and active-Paragraph projection own recognition and range
//! provenance. This module owns only Comrak-compatible value cleaning and the
//! persistent cooked-value sink. Destination trimming and title-delimiter
//! removal intentionally use a complete probe pass followed by a replay of the
//! same projected range; no source bytes or projection coordinates survive in
//! the resulting blob.

use std::fmt;
use std::ops::Range;

use flark_reference_value_service::{
    DestinationTrimProbe, MAX_ENTITY_EXPANSION_DENOMINATOR,
    MAX_ENTITY_EXPANSION_NUMERATOR, ReferenceValueBodyCleaner, ReferenceValueCleanerError,
    ReferenceValueCleanerStatus, clean_title_body_range,
};

use crate::arena::{ArenaBuildId, ArenaBuildSession};
use crate::persistent_blob::{
    PersistentBlobBuildProgress, PersistentBlobError, PersistentByteBlob,
    PersistentByteBlobBuilder,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceValueKind {
    Destination,
    Title,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceValueBlobError {
    Cleaner(ReferenceValueCleanerError),
    Blob(PersistentBlobError),
    Invalid(&'static str),
    Overflow(&'static str),
    InjectedFault(u64),
    Cancelled,
}

impl From<ReferenceValueCleanerError> for ReferenceValueBlobError {
    fn from(value: ReferenceValueCleanerError) -> Self {
        Self::Cleaner(value)
    }
}

impl From<PersistentBlobError> for ReferenceValueBlobError {
    fn from(value: PersistentBlobError) -> Self {
        Self::Blob(value)
    }
}

impl fmt::Display for ReferenceValueBlobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "reference value blob error: {self:?}")
    }
}

impl std::error::Error for ReferenceValueBlobError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReferenceValueBlobReceipt {
    pub(crate) polls: u64,
    pub(crate) probe_bytes: u64,
    pub(crate) replay_bytes: u64,
    pub(crate) selected_input_bytes: u64,
    pub(crate) cooked_output_bytes: u64,
    pub(crate) maximum_output_bound: u64,
    pub(crate) maximum_pending_output_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceValueBlobProgress {
    ReadyForReplayByte,
    ReadyToFinishReplay,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceValueBlobPhase {
    Probe,
    Replay,
    FinishCleaner,
    FinishBlob,
    Complete,
    Taken,
    Failed,
    Cancelled,
}

/// Two-pass, bounded-memory materializer for one destination or title.
///
/// Probe and replay bytes must each be offered in source order. One `poll`
/// advances at most one cleaner transition, copies at most one tiny cleaner
/// chunk into the blob page buffer, or advances the blob builder once.
pub(crate) struct ReferenceValueBlobMaterializer {
    build: ArenaBuildId,
    kind: ReferenceValueKind,
    phase: ReferenceValueBlobPhase,
    destination_probe: DestinationTrimProbe,
    probe_len: usize,
    probe_first: Option<u8>,
    probe_last: Option<u8>,
    selected: Option<Range<usize>>,
    replay_offset: usize,
    cleaner: ReferenceValueBodyCleaner,
    cleaner_needs_input: bool,
    pending_output: Option<flark_reference_value_service::CleanReferenceValueChunk>,
    pending_output_offset: usize,
    blob: PersistentByteBlobBuilder,
    receipt: ReferenceValueBlobReceipt,
    fault_after_poll: Option<u64>,
}

impl fmt::Debug for ReferenceValueBlobMaterializer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceValueBlobMaterializer")
            .field("kind", &self.kind)
            .field("phase", &self.phase)
            .field("probe_len", &self.probe_len)
            .field("selected", &self.selected)
            .field("replay_offset", &self.replay_offset)
            .field("cleaner_needs_input", &self.cleaner_needs_input)
            .field("pending_output_offset", &self.pending_output_offset)
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

impl ReferenceValueBlobMaterializer {
    pub(crate) fn try_new(
        build: ArenaBuildId,
        kind: ReferenceValueKind,
    ) -> Result<Self, ReferenceValueBlobError> {
        Ok(Self {
            build,
            kind,
            phase: ReferenceValueBlobPhase::Probe,
            destination_probe: DestinationTrimProbe::default(),
            probe_len: 0,
            probe_first: None,
            probe_last: None,
            selected: None,
            replay_offset: 0,
            cleaner: ReferenceValueBodyCleaner::new(),
            cleaner_needs_input: true,
            pending_output: None,
            pending_output_offset: 0,
            blob: PersistentByteBlobBuilder::try_new(build)?,
            receipt: ReferenceValueBlobReceipt::default(),
            fault_after_poll: None,
        })
    }

    #[cfg(test)]
    fn with_fault_after_poll(mut self, poll: u64) -> Self {
        self.fault_after_poll = Some(poll);
        self
    }

    pub(crate) const fn receipt(&self) -> ReferenceValueBlobReceipt {
        self.receipt
    }

    pub(crate) fn offer_probe_byte(&mut self, byte: u8) -> Result<(), ReferenceValueBlobError> {
        if self.phase != ReferenceValueBlobPhase::Probe {
            return Err(ReferenceValueBlobError::Invalid(
                "reference value probe is not accepting bytes",
            ));
        }
        self.probe_first.get_or_insert(byte);
        self.probe_last = Some(byte);
        self.probe_len = self
            .probe_len
            .checked_add(1)
            .ok_or(ReferenceValueBlobError::Overflow("reference value probe length"))?;
        if self.kind == ReferenceValueKind::Destination {
            self.destination_probe.push(byte)?;
        }
        self.receipt.probe_bytes = self
            .receipt
            .probe_bytes
            .checked_add(1)
            .ok_or(ReferenceValueBlobError::Overflow("reference value probe receipt"))?;
        Ok(())
    }

    pub(crate) fn finish_probe(&mut self) -> Result<Range<usize>, ReferenceValueBlobError> {
        if self.phase != ReferenceValueBlobPhase::Probe {
            return Err(ReferenceValueBlobError::Invalid(
                "reference value probe was already finished",
            ));
        }
        let selected = match self.kind {
            ReferenceValueKind::Destination => self.destination_probe.finish(),
            ReferenceValueKind::Title => {
                clean_title_body_range(self.probe_len, self.probe_first, self.probe_last)
            }
        };
        if selected.start > selected.end || selected.end > self.probe_len {
            self.phase = ReferenceValueBlobPhase::Failed;
            return Err(ReferenceValueBlobError::Invalid(
                "reference value selection escaped its projected range",
            ));
        }
        let selected_len = selected.end - selected.start;
        let output_bound = expansion_bound(selected_len)?;
        self.receipt.selected_input_bytes = u64::try_from(selected_len)
            .map_err(|_| ReferenceValueBlobError::Overflow("selected reference value bytes"))?;
        self.receipt.maximum_output_bound = u64::try_from(output_bound)
            .map_err(|_| ReferenceValueBlobError::Overflow("reference value output bound"))?;
        self.selected = Some(selected.clone());
        self.phase = ReferenceValueBlobPhase::Replay;
        Ok(selected)
    }

    pub(crate) fn ready_for_replay_byte(&self) -> bool {
        self.phase == ReferenceValueBlobPhase::Replay
            && self.cleaner_needs_input
            && self.pending_output.is_none()
            && self.replay_offset < self.probe_len
    }

    pub(crate) fn ready_to_finish_replay(&self) -> bool {
        self.phase == ReferenceValueBlobPhase::Replay
            && self.cleaner_needs_input
            && self.pending_output.is_none()
            && self.replay_offset == self.probe_len
    }

    pub(crate) fn offer_replay_byte(&mut self, byte: u8) -> Result<(), ReferenceValueBlobError> {
        if self.phase != ReferenceValueBlobPhase::Replay
            || !self.cleaner_needs_input
            || self.pending_output.is_some()
            || self.replay_offset >= self.probe_len
        {
            return Err(ReferenceValueBlobError::Invalid(
                "reference value replay is not ready for one byte",
            ));
        }
        let selected = self.selected.as_ref().ok_or(ReferenceValueBlobError::Invalid(
            "reference value replay lost its selected body",
        ))?;
        let offset = self.replay_offset;
        self.replay_offset = self
            .replay_offset
            .checked_add(1)
            .ok_or(ReferenceValueBlobError::Overflow("reference value replay offset"))?;
        self.receipt.replay_bytes = self
            .receipt
            .replay_bytes
            .checked_add(1)
            .ok_or(ReferenceValueBlobError::Overflow("reference value replay receipt"))?;
        if selected.contains(&offset) {
            self.cleaner.offer_byte(byte)?;
            self.cleaner_needs_input = false;
        }
        Ok(())
    }

    pub(crate) fn finish_replay(&mut self) -> Result<(), ReferenceValueBlobError> {
        if self.phase != ReferenceValueBlobPhase::Replay
            || self.replay_offset != self.probe_len
            || !self.cleaner_needs_input
            || self.pending_output.is_some()
        {
            return Err(ReferenceValueBlobError::Invalid(
                "reference value replay is not completely drained",
            ));
        }
        self.cleaner.finish_input()?;
        self.cleaner_needs_input = false;
        self.phase = ReferenceValueBlobPhase::FinishCleaner;
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ReferenceValueBlobProgress, ReferenceValueBlobError> {
        if session.id() != self.build {
            self.phase = ReferenceValueBlobPhase::Failed;
            return Err(ReferenceValueBlobError::Invalid(
                "reference value materializer crossed arena build authority",
            ));
        }
        match self.phase {
            ReferenceValueBlobPhase::Probe => {
                return Err(ReferenceValueBlobError::Invalid(
                    "reference value probe must be driven before materializer polling",
                ));
            }
            ReferenceValueBlobPhase::Complete => {
                return Ok(ReferenceValueBlobProgress::Complete);
            }
            ReferenceValueBlobPhase::Cancelled => {
                return Ok(ReferenceValueBlobProgress::Cancelled);
            }
            ReferenceValueBlobPhase::Taken | ReferenceValueBlobPhase::Failed => {
                return Err(ReferenceValueBlobError::Invalid(
                    "reference value materializer is consumed or failed",
                ));
            }
            ReferenceValueBlobPhase::Replay
            | ReferenceValueBlobPhase::FinishCleaner
            | ReferenceValueBlobPhase::FinishBlob => {}
        }

        self.receipt.polls = self
            .receipt
            .polls
            .checked_add(1)
            .ok_or(ReferenceValueBlobError::Overflow("reference value materializer polls"))?;
        if self.fault_after_poll == Some(self.receipt.polls) {
            self.phase = ReferenceValueBlobPhase::Failed;
            return Err(ReferenceValueBlobError::InjectedFault(self.receipt.polls));
        }

        if let Some(output) = self.pending_output.as_ref() {
            if !self.blob.is_ready_for_bytes() {
                let _ = self.blob.poll(session)?;
                return Ok(ReferenceValueBlobProgress::Pending);
            }
            let bytes = output.bytes();
            let copied = self.blob.push_bytes(&bytes[self.pending_output_offset..])?;
            self.pending_output_offset = self
                .pending_output_offset
                .checked_add(copied)
                .ok_or(ReferenceValueBlobError::Overflow("pending cooked output offset"))?;
            self.receipt.cooked_output_bytes = self
                .receipt
                .cooked_output_bytes
                .checked_add(u64::try_from(copied).map_err(|_| {
                    ReferenceValueBlobError::Overflow("cooked reference value bytes")
                })?)
                .ok_or(ReferenceValueBlobError::Overflow(
                    "cooked reference value receipt",
                ))?;
            if self.receipt.cooked_output_bytes > self.receipt.maximum_output_bound {
                self.phase = ReferenceValueBlobPhase::Failed;
                return Err(ReferenceValueBlobError::Invalid(
                    "cleaned reference value exceeded the proved expansion bound",
                ));
            }
            if self.pending_output_offset == bytes.len() {
                self.pending_output = None;
                self.pending_output_offset = 0;
            }
            return Ok(ReferenceValueBlobProgress::Pending);
        }

        if self.phase == ReferenceValueBlobPhase::FinishBlob {
            if self.blob.poll(session)? == PersistentBlobBuildProgress::Complete {
                self.phase = ReferenceValueBlobPhase::Complete;
                return Ok(ReferenceValueBlobProgress::Complete);
            }
            return Ok(ReferenceValueBlobProgress::Pending);
        }

        match self.cleaner.poll()? {
            ReferenceValueCleanerStatus::Progress => Ok(ReferenceValueBlobProgress::Pending),
            ReferenceValueCleanerStatus::NeedInput => {
                if self.phase != ReferenceValueBlobPhase::Replay {
                    self.phase = ReferenceValueBlobPhase::Failed;
                    return Err(ReferenceValueBlobError::Invalid(
                        "finished reference cleaner requested more replay input",
                    ));
                }
                self.cleaner_needs_input = true;
                if self.replay_offset == self.probe_len {
                    Ok(ReferenceValueBlobProgress::ReadyToFinishReplay)
                } else {
                    Ok(ReferenceValueBlobProgress::ReadyForReplayByte)
                }
            }
            ReferenceValueCleanerStatus::OutputReady => {
                let output = self.cleaner.take_output()?;
                self.receipt.maximum_pending_output_bytes = self
                    .receipt
                    .maximum_pending_output_bytes
                    .max(output.bytes().len());
                self.pending_output = Some(output);
                Ok(ReferenceValueBlobProgress::Pending)
            }
            ReferenceValueCleanerStatus::Complete => {
                if self.phase != ReferenceValueBlobPhase::FinishCleaner {
                    self.phase = ReferenceValueBlobPhase::Failed;
                    return Err(ReferenceValueBlobError::Invalid(
                        "reference cleaner completed before replay EOF",
                    ));
                }
                if self.cleaner.receipt().input_bytes != self.receipt.selected_input_bytes
                    || self.cleaner.receipt().output_bytes != self.receipt.cooked_output_bytes
                {
                    self.phase = ReferenceValueBlobPhase::Failed;
                    return Err(ReferenceValueBlobError::Invalid(
                        "cleaner input/output receipts diverged from the selected persistent value",
                    ));
                }
                self.blob.begin_finish()?;
                self.phase = ReferenceValueBlobPhase::FinishBlob;
                Ok(ReferenceValueBlobProgress::Pending)
            }
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.pending_output = None;
        self.phase = ReferenceValueBlobPhase::Cancelled;
    }

    pub(crate) fn take_blob(&mut self) -> Result<PersistentByteBlob, ReferenceValueBlobError> {
        if self.phase != ReferenceValueBlobPhase::Complete {
            return Err(ReferenceValueBlobError::Invalid(
                "cooked reference value blob is not complete",
            ));
        }
        let blob = self.blob.take_blob()?;
        self.phase = ReferenceValueBlobPhase::Taken;
        Ok(blob)
    }
}

fn expansion_bound(input_bytes: usize) -> Result<usize, ReferenceValueBlobError> {
    let scaled = input_bytes
        .checked_mul(MAX_ENTITY_EXPANSION_NUMERATOR)
        .ok_or(ReferenceValueBlobError::Overflow(
            "reference value expansion numerator",
        ))?;
    scaled
        .checked_add(MAX_ENTITY_EXPANSION_DENOMINATOR - 1)
        .ok_or(ReferenceValueBlobError::Overflow(
            "reference value expansion rounding",
        ))
        .map(|value| value / MAX_ENTITY_EXPANSION_DENOMINATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_blob::{PersistentBlobReadProgress, PersistentByteBlobReadCursor};
    use crate::PageArena;

    fn materialize(kind: ReferenceValueKind, input: &[u8]) -> Vec<u8> {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut materializer = ReferenceValueBlobMaterializer::try_new(build, kind).unwrap();
        for &byte in input {
            materializer.offer_probe_byte(byte).unwrap();
        }
        materializer.finish_probe().unwrap();
        let mut ticket = Some(ticket);
        let mut offset = 0;
        while offset < input.len() {
            if materializer.ready_for_replay_byte() {
                materializer.offer_replay_byte(input[offset]).unwrap();
                offset += 1;
            } else {
                let current = ticket.take().unwrap();
                let mut session = arena.resume_build(current).unwrap();
                materializer.poll(&mut session).unwrap();
                ticket = Some(session.suspend().unwrap());
            }
        }
        while !materializer.ready_to_finish_replay() {
            let current = ticket.take().unwrap();
            let mut session = arena.resume_build(current).unwrap();
            materializer.poll(&mut session).unwrap();
            ticket = Some(session.suspend().unwrap());
        }
        materializer.finish_replay().unwrap();
        loop {
            let current = ticket.take().unwrap();
            let mut session = arena.resume_build(current).unwrap();
            let progress = materializer.poll(&mut session).unwrap();
            ticket = Some(session.suspend().unwrap());
            if progress == ReferenceValueBlobProgress::Complete {
                break;
            }
        }
        let blob = materializer.take_blob().unwrap();
        let current = ticket.take().unwrap();
        let mut session = arena.resume_build(current).unwrap();
        let mut cursor =
            PersistentByteBlobReadCursor::try_new(blob.metadata(&session).unwrap()).unwrap();
        let mut output = Vec::new();
        loop {
            match cursor.poll(session.arena()).unwrap() {
                PersistentBlobReadProgress::Pending => {}
                PersistentBlobReadProgress::Chunk(chunk) => {
                    output.extend_from_slice(chunk.bytes(session.arena()).unwrap());
                }
                PersistentBlobReadProgress::Complete => break,
            }
        }
        if let Some(owner) = blob.into_owner() {
            session.release(owner).unwrap();
        }
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        output
    }

    #[test]
    fn two_pass_destination_and_title_materialization_is_exact() {
        assert_eq!(
            materialize(ReferenceValueKind::Destination, b" \t/a&amp;b\\* \r"),
            b"/a&b*"
        );
        assert_eq!(
            materialize(ReferenceValueKind::Title, b"\"a&amp;b\\*\""),
            b"a&b*"
        );
        assert_eq!(materialize(ReferenceValueKind::Destination, b" \t\r"), b"");
        assert_eq!(materialize(ReferenceValueKind::Title, b"\"\""), b"");
    }

    #[test]
    fn poll_fault_and_cancel_leave_cleanup_to_the_whole_build_journal() {
        for cancel in [false, true] {
            let mut arena = PageArena::new();
            let ticket = arena.begin_build().unwrap();
            let build = ticket.id();
            let mut materializer = ReferenceValueBlobMaterializer::try_new(
                build,
                ReferenceValueKind::Destination,
            )
            .unwrap()
            .with_fault_after_poll(1);
            materializer.offer_probe_byte(b'a').unwrap();
            materializer.finish_probe().unwrap();
            materializer.offer_replay_byte(b'a').unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            if cancel {
                materializer.cancel();
                assert_eq!(
                    materializer.poll(&mut session).unwrap(),
                    ReferenceValueBlobProgress::Cancelled
                );
            } else {
                assert_eq!(
                    materializer.poll(&mut session),
                    Err(ReferenceValueBlobError::InjectedFault(1))
                );
            }
            let abort = session.begin_abort().unwrap();
            while !arena.poll_build_abort(abort, 1).unwrap().complete {}
            while arena.poll_reclaim(1).unwrap().pending_after != 0 {}
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }
}
