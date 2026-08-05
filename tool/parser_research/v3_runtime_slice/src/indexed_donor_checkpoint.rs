//! Exact-parser-only type firewall around donor checkpoint bytes.
//!
//! The checkpoint index stores and structurally shares opaque bytes. This
//! module is the only route from those bytes back to donor parser state: every
//! header and frame is first passed through the donor's current-schema
//! `from_bytes` validators, and contextual path validation remains in the
//! donor resume call. The resulting parser is still donor-only scratch, not a
//! product restart capability; source, green, and writer authority are absent.

use flark_comrak_value_block_core::{
    DIRECT_DURABLE_GRAMMAR_FRAME_BYTES, DIRECT_DURABLE_GRAMMAR_HEADER_BYTES,
    DirectDurableGrammarCapture, DirectDurableGrammarFrameRecord, DirectDurableGrammarHeader,
    DirectGrammarContinuation, DirectRestartLineLocalContinuation, DirectValueBlockParser,
    ParseError,
};

pub(crate) const DONOR_HEADER_BYTES: usize = DIRECT_DURABLE_GRAMMAR_HEADER_BYTES;
pub(crate) const DONOR_FRAME_BYTES: usize = DIRECT_DURABLE_GRAMMAR_FRAME_BYTES;
pub(crate) type OpaqueDonorHeader = [u8; DONOR_HEADER_BYTES];
pub(crate) type OpaqueDonorFrame = [u8; DONOR_FRAME_BYTES];

/// In-memory-only exact identity witness for one source-free donor sample.
/// This duplicates O(open-depth) opaque bytes so the first cross-build proof
/// can reject a same-cut recipe selected from another index. The durable
/// architecture should instead authenticate the index descriptor under the
/// same composite document root as source and green storage.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OpaqueDonorIdentityWitness {
    header: OpaqueDonorHeader,
    frames: Box<[OpaqueDonorFrame]>,
}

/// Transient, typed input accepted only from the donor capture API.
#[derive(Debug)]
pub(crate) struct OpaqueDonorCaptureDraft {
    header: OpaqueDonorHeader,
    frames: Vec<OpaqueDonorFrame>,
    donor_materialized_path_bytes: usize,
    conversion_scratch_bytes: usize,
    retained_source_bytes: usize,
}

impl OpaqueDonorCaptureDraft {
    #[allow(clippy::needless_pass_by_value)] // Consume the typed donor handoff; raw bytes never enter this API.
    pub(crate) fn try_from_capture(
        capture: DirectDurableGrammarCapture,
    ) -> Result<Self, &'static str> {
        let capture_receipt = capture.receipt();
        if capture_receipt.sample_header_bytes != DONOR_HEADER_BYTES
            || capture_receipt.materialized_path_bytes
                != capture_receipt.materialized_path_records * DONOR_FRAME_BYTES
            || capture_receipt.retained_source_bytes != 0
        {
            return Err("donor durable capture receipt disagrees with the split codec");
        }
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(capture_receipt.materialized_path_records)
            .map_err(|_| "donor frame draft allocation failed")?;
        for frame in capture.frame_records() {
            frames.push(*frame.as_bytes());
        }
        let draft_bytes = frames.capacity() * DONOR_FRAME_BYTES;
        Ok(Self {
            header: *capture.header().as_bytes(),
            frames,
            donor_materialized_path_bytes: capture_receipt.materialized_path_bytes,
            conversion_scratch_bytes: capture_receipt.materialized_path_bytes + draft_bytes,
            retained_source_bytes: capture_receipt.retained_source_bytes,
        })
    }

    pub(crate) const fn header(&self) -> &OpaqueDonorHeader {
        &self.header
    }

    pub(crate) fn frames(&self) -> &[OpaqueDonorFrame] {
        &self.frames
    }

    pub(crate) fn into_frames(self) -> Vec<OpaqueDonorFrame> {
        self.frames
    }

    pub(crate) const fn donor_materialized_path_bytes(&self) -> usize {
        self.donor_materialized_path_bytes
    }

    pub(crate) const fn conversion_scratch_bytes(&self) -> usize {
        self.conversion_scratch_bytes
    }

    pub(crate) const fn retained_source_bytes(&self) -> usize {
        self.retained_source_bytes
    }

    pub(crate) fn draft_storage_bytes(&self) -> usize {
        self.frames.capacity() * DONOR_FRAME_BYTES
    }

    pub(crate) fn identity_witness(&self) -> Result<OpaqueDonorIdentityWitness, &'static str> {
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(self.frames.len())
            .map_err(|_| "donor identity witness allocation failed")?;
        frames.extend_from_slice(&self.frames);
        Ok(OpaqueDonorIdentityWitness {
            header: self.header,
            frames: frames.into_boxed_slice(),
        })
    }
}

/// Donor-validated recipe reconstructed from one selected index sample.
/// Consuming it can rebuild donor scratch only; no source cursor is stored.
#[derive(Debug)]
pub(crate) struct IndexedDonorCheckpointRecipe {
    header: DirectDurableGrammarHeader,
    frames: Vec<DirectDurableGrammarFrameRecord>,
}

#[derive(Debug)]
pub(crate) struct ValidatedIndexedDonorHeader(DirectDurableGrammarHeader);

impl IndexedDonorCheckpointRecipe {
    pub(crate) fn validate_header(
        header: &OpaqueDonorHeader,
    ) -> Result<ValidatedIndexedDonorHeader, ParseError> {
        DirectDurableGrammarHeader::from_bytes(header).map(ValidatedIndexedDonorHeader)
    }

    #[allow(clippy::needless_pass_by_value)] // The validation token is deliberately one-shot.
    pub(crate) fn from_validated_storage(
        header: ValidatedIndexedDonorHeader,
        frames: impl IntoIterator<Item = OpaqueDonorFrame>,
    ) -> Result<Self, ParseError> {
        let frames = frames.into_iter();
        let (minimum, maximum) = frames.size_hint();
        if maximum != Some(minimum) {
            return Err(ParseError::Invariant(
                "indexed donor path reconstruction has an exact length",
            ));
        }
        let mut typed_frames = Vec::new();
        typed_frames
            .try_reserve_exact(minimum)
            .map_err(|_| ParseError::Invariant("indexed donor frame allocation failed"))?;
        for frame in frames {
            typed_frames.push(DirectDurableGrammarFrameRecord::from_bytes(&frame)?);
        }
        Ok(Self {
            header: header.0,
            frames: typed_frames,
        })
    }

    /// Decode suffix-persisted grammar plus its opaque line-local half. This
    /// remains mechanism only: the caller must separately establish prefix or
    /// suffix induction and current committed-output authority before binding.
    pub(crate) fn decode_grammar_parts(
        &self,
    ) -> Result<
        (
            DirectGrammarContinuation,
            DirectRestartLineLocalContinuation,
        ),
        ParseError,
    > {
        DirectValueBlockParser::decode_durable_grammar_restart_parts(
            self.header,
            self.frames.iter().copied(),
        )
    }

    pub(crate) const fn retained_source_bytes() -> usize {
        0
    }

    pub(crate) fn scratch_storage_bytes(&self) -> usize {
        DONOR_HEADER_BYTES
            + self.frames.capacity() * std::mem::size_of::<DirectDurableGrammarFrameRecord>()
    }

    pub(crate) fn matches_identity_witness(&self, witness: &OpaqueDonorIdentityWitness) -> bool {
        let matches = self.header.as_bytes() == &witness.header
            && self.frames.len() == witness.frames.len()
            && self
                .frames
                .iter()
                .zip(witness.frames.iter())
                .all(|(frame, expected)| frame.as_bytes() == expected);
        matches
    }
}
