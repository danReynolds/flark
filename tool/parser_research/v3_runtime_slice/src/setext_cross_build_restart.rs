//! Narrow in-memory cross-build Setext mechanism proof.
//!
//! This module is intentionally **not** a durable document format and does
//! not publish a coordinator root. It joins typed drafts captured from one
//! already-validated same-build parser/writer checkpoint, lets the donor half
//! enter the sparse checkpoint index, and carries the source/green halves only
//! long enough to construct one fresh candidate after an edit.

use std::num::NonZeroU64;

use crate::committed_checkpoint_index::{
    DonorCheckpointSampleDraft, LocatedDonorCheckpointRecipe, RelativeCheckpointMeasure,
};
use crate::indexed_donor_checkpoint::OpaqueDonorIdentityWitness;
use crate::retained_restart_coordinate::{
    PreferredDeferredLfRestart, RetainedRestartCoordinateAuthority, RetainedRestartCoordinateError,
    RetainedRestartCoordinateJob, RetainedRestartCoordinateProgress, StoredDeferredLfRestart,
};
use crate::same_build_checkpoint::ParserLineBoundaryCheckpointAuthority;
use crate::serialized_green::setext_retained_restart::{
    SealedSetextNormalizationManifest, SetextRetainedGreenRestart,
    SetextRetainedGreenRestartOutput, SetextRetainedGreenRestartProgress,
};
use crate::serialized_green::{
    RetainedSetextGreenCheckpointDraft, SerializedGreenError, SerializedGreenManifestDescriptor,
};
use crate::source_bound_ledger::{CandidateSourceLedger, RetainedSetextSourceLedgerDraft};
use crate::{
    ArenaBuildSession, ArenaBuildTicket, CandidateWriter, CandidateWriterBuiltDocument,
    CandidateWriterError, DonorResumedRetainedSetextSourceActivation, GreenHeadingOpenFacts,
    LiveCandidateEpoch, PageArena, SerializedGreenRootSpec, SourceStore,
};
use flark_comrak_value_block_core::{
    DirectLineBoundaryResumeCursor, DirectValueBlockParser, ParseError,
};

/// Transient output of borrowing the actor's paused joined checkpoint.
/// Neither half is independently resumable.
#[derive(Debug)]
pub(crate) struct InMemorySetextCheckpointDraft {
    source: RetainedSetextSourceLedgerDraft,
    green: RetainedSetextGreenCheckpointDraft,
    parser: ParserLineBoundaryCheckpointAuthority,
    donor: DonorCheckpointSampleDraft,
    donor_identity: OpaqueDonorIdentityWitness,
    checkpoint_cut: RelativeCheckpointMeasure,
}

impl InMemorySetextCheckpointDraft {
    pub(crate) fn from_joined_checkpoint(
        source: RetainedSetextSourceLedgerDraft,
        green: RetainedSetextGreenCheckpointDraft,
        parser: ParserLineBoundaryCheckpointAuthority,
        donor: DonorCheckpointSampleDraft,
        donor_identity: OpaqueDonorIdentityWitness,
        checkpoint_cut: RelativeCheckpointMeasure,
    ) -> Self {
        Self {
            source,
            green,
            parser,
            donor,
            donor_identity,
            checkpoint_cut,
        }
    }

    /// Splits only after the joined capture has become a typed index sample.
    /// The integrated path never exposes coordinate-free donor bytes.
    pub(crate) fn split(self) -> (DonorCheckpointSampleDraft, InMemorySetextRestartDraft) {
        (
            self.donor,
            InMemorySetextRestartDraft {
                source: self.source,
                green: self.green,
                parser: self.parser,
                donor_identity: self.donor_identity,
                checkpoint_cut: self.checkpoint_cut,
            },
        )
    }
}

/// In-memory writer/source/green portion retained while the old candidate
/// finishes and its finalized Heading validates the inverse recipe.
#[derive(Debug)]
pub(crate) struct InMemorySetextRestartDraft {
    source: RetainedSetextSourceLedgerDraft,
    green: RetainedSetextGreenCheckpointDraft,
    parser: ParserLineBoundaryCheckpointAuthority,
    donor_identity: OpaqueDonorIdentityWitness,
    checkpoint_cut: RelativeCheckpointMeasure,
}

impl InMemorySetextRestartDraft {
    pub(crate) const fn source(&self) -> &RetainedSetextSourceLedgerDraft {
        &self.source
    }

    pub(crate) const fn green(&self) -> &RetainedSetextGreenCheckpointDraft {
        &self.green
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        RetainedSetextSourceLedgerDraft,
        RetainedSetextGreenCheckpointDraft,
        RelativeCheckpointMeasure,
    ) {
        (self.source, self.green, self.checkpoint_cut)
    }

    /// Joins the transient checkpoint back to the exact actor-produced old
    /// mechanism document. This is where low semantic IDs become provenance,
    /// not merely numbers below an allocator high-water mark.
    pub(crate) fn seal_against_old_document(
        self,
        arena: &PageArena,
        old_document: &CandidateWriterBuiltDocument,
        final_heading: GreenHeadingOpenFacts,
    ) -> Result<SealedInMemorySetextRestart, InMemorySetextRestartError> {
        let old_epoch = self.source.old_epoch();
        let old_binding = old_document.green_document().manifest_descriptor(arena)?;
        let accepted = self.green.accepted_source_cut();
        if old_epoch.arena_identity() != arena.identity()
            || old_epoch.source() != self.source.descriptor()
            || old_epoch.source() != old_document.source()
            || old_epoch.build_id() != self.green.old_build()
            || old_epoch.parse_token().generation != old_binding.parse_generation
            || old_epoch.source().revision != old_binding.source_revision
            || old_epoch.source().root != old_binding.source_root
            || u64::try_from(old_epoch.source().bytes).ok() != Some(old_binding.source_bytes)
            || self.green.block() != self.source.terminal_block()?
            || accepted.bytes != self.source.accepted_bytes()
            || accepted.utf16 != self.source.accepted_utf16()
            || self.checkpoint_cut.source_bytes() != self.source.physical_bytes()
            || self.checkpoint_cut.source_utf16() != self.source.physical_utf16()
        {
            return Err(InMemorySetextRestartError::ProvenanceMismatch);
        }
        let stored_coordinate = StoredDeferredLfRestart::capture_from_joined_setext(&self.source)?;
        let green_manifest = old_document.seal_in_memory_setext_normalization(
            arena,
            &self.source,
            &self.green,
            final_heading,
        )?;
        Ok(SealedInMemorySetextRestart {
            stored_coordinate,
            green_manifest,
            donor_identity: self.donor_identity,
            parser: self.parser,
            checkpoint_cut: self.checkpoint_cut,
            activation: InMemorySetextActivationAuthority {
                old_epoch,
                old_binding,
                source: self.source,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InMemorySetextRestartError {
    Coordinate(RetainedRestartCoordinateError),
    Green(SerializedGreenError),
    Source(crate::SourceBoundLedgerError),
    Writer(CandidateWriterError),
    ProvenanceMismatch,
}

impl From<RetainedRestartCoordinateError> for InMemorySetextRestartError {
    fn from(value: RetainedRestartCoordinateError) -> Self {
        Self::Coordinate(value)
    }
}

impl From<SerializedGreenError> for InMemorySetextRestartError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl From<crate::SourceBoundLedgerError> for InMemorySetextRestartError {
    fn from(value: crate::SourceBoundLedgerError) -> Self {
        Self::Source(value)
    }
}

impl From<CandidateWriterError> for InMemorySetextRestartError {
    fn from(value: CandidateWriterError) -> Self {
        Self::Writer(value)
    }
}

#[derive(Debug)]
pub(crate) struct SealedInMemorySetextRestart {
    stored_coordinate: StoredDeferredLfRestart,
    green_manifest: SealedSetextNormalizationManifest,
    donor_identity: OpaqueDonorIdentityWitness,
    parser: ParserLineBoundaryCheckpointAuthority,
    checkpoint_cut: RelativeCheckpointMeasure,
    activation: InMemorySetextActivationAuthority,
}

impl SealedInMemorySetextRestart {
    /// Joins the selected persisted donor to the exact five-axis cut minted
    /// by the original parser/writer checkpoint. `RelativeCheckpointMeasure`
    /// equality covers physical bytes, UTF-16 units, completed lines, green
    /// events, and projection runs. Parser resume is intentionally unavailable
    /// until this join succeeds.
    pub(crate) fn join_located_donor(
        self,
        donor: LocatedDonorCheckpointRecipe,
    ) -> Result<JoinedInMemorySetextRestart, Box<InMemorySetextDonorJoinFailure>> {
        let error = if donor.checkpoint_cut() != self.checkpoint_cut {
            Some(InMemorySetextDonorJoinError::WrongCheckpointCut)
        } else if !donor.matches_identity_witness(&self.donor_identity) {
            Some(InMemorySetextDonorJoinError::WrongDonorIdentity)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(Box::new(InMemorySetextDonorJoinFailure {
                error,
                restart: self,
                donor,
            }));
        }
        Ok(JoinedInMemorySetextRestart {
            stored_coordinate: self.stored_coordinate,
            green_manifest: self.green_manifest,
            activation: self.activation,
            donor: WitnessValidatedSetextDonorRecipe {
                parser: self.parser,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InMemorySetextDonorJoinError {
    WrongCheckpointCut,
    WrongDonorIdentity,
}

/// Retryable rejection: neither the sealed restart nor selected donor is
/// consumed when the index returned a recipe at a different composite cut.
#[derive(Debug)]
pub(crate) struct InMemorySetextDonorJoinFailure {
    pub(crate) error: InMemorySetextDonorJoinError,
    pub(crate) restart: SealedInMemorySetextRestart,
    pub(crate) donor: LocatedDonorCheckpointRecipe,
}

/// The only integrated-path carrier from index selection into fresh-candidate
/// activation. Its donor has already matched the original joined cut on every
/// index axis.
#[derive(Debug)]
pub(crate) struct JoinedInMemorySetextRestart {
    stored_coordinate: StoredDeferredLfRestart,
    green_manifest: SealedSetextNormalizationManifest,
    activation: InMemorySetextActivationAuthority,
    donor: WitnessValidatedSetextDonorRecipe,
}

impl JoinedInMemorySetextRestart {
    pub(crate) const fn old_epoch(&self) -> LiveCandidateEpoch {
        self.activation.old_epoch()
    }

    pub(crate) const fn old_binding(&self) -> SerializedGreenManifestDescriptor {
        self.activation.old_binding()
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        StoredDeferredLfRestart,
        SealedSetextNormalizationManifest,
        InMemorySetextActivationAuthority,
        WitnessValidatedSetextDonorRecipe,
    ) {
        (
            self.stored_coordinate,
            self.green_manifest,
            self.activation,
            self.donor,
        )
    }
}

/// Located donor recipe after both exact composite-cut and byte-for-byte
/// opaque identity checks. There is no constructor outside the sealed restart
/// join, so downstream activation cannot substitute another same-cut recipe.
#[derive(Debug)]
pub(crate) struct WitnessValidatedSetextDonorRecipe {
    parser: ParserLineBoundaryCheckpointAuthority,
}

impl WitnessValidatedSetextDonorRecipe {
    pub(crate) fn resume_donor(
        self,
        cursor: DirectLineBoundaryResumeCursor,
    ) -> Result<DirectValueBlockParser, ParseError> {
        let (grammar, output) = self.parser.into_pause().into_restart_parts()?;
        DirectValueBlockParser::resume_restart_parts(&grammar, output, cursor)
    }
}

/// External-borrow, in-memory-only activation job for the first 4/5 Setext
/// proof. It is resumable, but it is not yet the production actor-owned job:
/// the old mechanism document is borrowed from the caller until a committed
/// composite root can provide an actor-owned retained lease.
///
/// This first gate also assumes the retained checkpoint prefix is unchanged,
/// so old and current absolute 4/5 cuts are equal. It does not claim the later
/// length-changing convergence/rebasing design.
pub(crate) struct InMemorySetextActivationJob<'old> {
    activation_id: NonZeroU64,
    epoch: LiveCandidateEpoch,
    coordinate: Option<RetainedRestartCoordinateJob>,
    preferred: Option<PreferredDeferredLfRestart>,
    green: Option<SetextRetainedGreenRestart<'old>>,
    green_output: Option<SetextRetainedGreenRestartOutput>,
    green_started: bool,
    activation: Option<InMemorySetextActivationAuthority>,
    donor: Option<WitnessValidatedSetextDonorRecipe>,
    donor_source: Option<DonorResumedRetainedSetextSourceActivation>,
    old_epoch: LiveCandidateEpoch,
    old_binding: SerializedGreenManifestDescriptor,
    new_spec: SerializedGreenRootSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InMemorySetextActivationProgress {
    Pending,
    Ready,
}

#[derive(Debug)]
pub(crate) enum InMemorySetextActivationError {
    Actor(crate::LiveDocumentError),
    Coordinate(RetainedRestartCoordinateError),
    Green(SerializedGreenError),
    Source(crate::SourceBoundLedgerError),
    Writer(CandidateWriterError),
    Parser(ParseError),
    ZeroFallback,
    InvalidState(&'static str),
}

impl From<crate::LiveDocumentError> for InMemorySetextActivationError {
    fn from(value: crate::LiveDocumentError) -> Self {
        Self::Actor(value)
    }
}

impl From<RetainedRestartCoordinateError> for InMemorySetextActivationError {
    fn from(value: RetainedRestartCoordinateError) -> Self {
        Self::Coordinate(value)
    }
}

impl From<SerializedGreenError> for InMemorySetextActivationError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl From<crate::SourceBoundLedgerError> for InMemorySetextActivationError {
    fn from(value: crate::SourceBoundLedgerError) -> Self {
        Self::Source(value)
    }
}

impl From<CandidateWriterError> for InMemorySetextActivationError {
    fn from(value: CandidateWriterError) -> Self {
        Self::Writer(value)
    }
}

impl From<ParseError> for InMemorySetextActivationError {
    fn from(value: ParseError) -> Self {
        Self::Parser(value)
    }
}

impl<'old> InMemorySetextActivationJob<'old> {
    pub(crate) const fn old_epoch(&self) -> LiveCandidateEpoch {
        self.old_epoch
    }

    pub(crate) const fn old_binding(&self) -> SerializedGreenManifestDescriptor {
        self.old_binding
    }

    pub(crate) const fn activation_id(&self) -> NonZeroU64 {
        self.activation_id
    }

    #[allow(clippy::too_many_arguments)] // Constructor joins eight independent actor-owned proof domains.
    pub(crate) fn try_new(
        source: &SourceStore,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        old_document: &'old CandidateWriterBuiltDocument,
        epoch: LiveCandidateEpoch,
        activation_id: NonZeroU64,
        joined: JoinedInMemorySetextRestart,
        new_spec: SerializedGreenRootSpec,
    ) -> Result<Self, InMemorySetextActivationError> {
        let (stored, manifest, activation, donor) = joined.into_parts();
        let old_epoch = activation.old_epoch();
        let old_binding = activation.old_binding();
        let coordinate = RetainedRestartCoordinateJob::begin(source, stored)?;
        let green = SetextRetainedGreenRestart::try_new(
            ticket,
            arena,
            old_document.green_document(),
            manifest,
            new_spec.clone(),
        )?;
        Ok(Self {
            activation_id,
            epoch,
            coordinate: Some(coordinate),
            preferred: None,
            green: Some(green),
            green_output: None,
            green_started: false,
            activation: Some(activation),
            donor: Some(donor),
            donor_source: None,
            old_epoch,
            old_binding,
            new_spec,
        })
    }

    pub(crate) fn poll_coordinate(
        &mut self,
        source: &SourceStore,
        fuel: usize,
    ) -> Result<InMemorySetextActivationProgress, InMemorySetextActivationError> {
        if self.preferred.is_some() || self.donor_source.is_some() {
            return Ok(InMemorySetextActivationProgress::Ready);
        }
        let progress = self
            .coordinate
            .as_mut()
            .ok_or(InMemorySetextActivationError::InvalidState(
                "retained coordinate job is missing",
            ))?
            .poll(source, fuel)?;
        match progress {
            RetainedRestartCoordinateProgress::Pending { .. } => {
                Ok(InMemorySetextActivationProgress::Pending)
            }
            RetainedRestartCoordinateProgress::Ready(authority) => {
                self.coordinate.take();
                match *authority {
                    RetainedRestartCoordinateAuthority::Preferred(preferred) => {
                        self.preferred = Some(preferred);
                        Ok(InMemorySetextActivationProgress::Ready)
                    }
                    RetainedRestartCoordinateAuthority::ZeroFallback(_) => {
                        Err(InMemorySetextActivationError::ZeroFallback)
                    }
                }
            }
        }
    }

    /// Completes all source/donor fallible work before the first fresh-build
    /// arena allocation. A failure here leaves the ordinary candidate ticket
    /// and allocator untouched and can safely fall back to a full parse.
    pub(crate) fn prepare_source_and_donor(
        &mut self,
        ticket: &ArenaBuildTicket,
    ) -> Result<(), InMemorySetextActivationError> {
        if self.donor_source.is_some() {
            return Ok(());
        }
        let preferred =
            self.preferred
                .take()
                .ok_or(InMemorySetextActivationError::InvalidState(
                    "preferred retained coordinate is not ready",
                ))?;
        let activation =
            self.activation
                .take()
                .ok_or(InMemorySetextActivationError::InvalidState(
                    "retained activation authority is missing",
                ))?;
        let source = CandidateSourceLedger::restore_retained_setext(
            self.epoch,
            activation.into_source(),
            preferred,
        )?;
        let source = CandidateWriter::validate_retained_setext_source_activation(
            self.epoch, source, ticket,
        )?;
        let donor = self
            .donor
            .take()
            .ok_or(InMemorySetextActivationError::InvalidState(
                "witness-validated donor is missing",
            ))?;
        self.donor_source = Some(source.resume_donor(donor)?);
        Ok(())
    }

    pub(crate) fn poll_green(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<InMemorySetextActivationProgress, InMemorySetextActivationError> {
        if self.donor_source.is_none() {
            return Err(InMemorySetextActivationError::InvalidState(
                "green restart cannot allocate before donor resume",
            ));
        }
        if self.green_output.is_some() {
            return Ok(InMemorySetextActivationProgress::Ready);
        }
        self.green_started = true;
        let progress = self
            .green
            .as_mut()
            .ok_or(InMemorySetextActivationError::InvalidState(
                "retained green restart is missing",
            ))?
            .poll(session)?;
        match progress {
            SetextRetainedGreenRestartProgress::Pending => {
                Ok(InMemorySetextActivationProgress::Pending)
            }
            SetextRetainedGreenRestartProgress::Ready => {
                let green =
                    self.green
                        .take()
                        .ok_or(InMemorySetextActivationError::InvalidState(
                            "ready retained green restart is missing",
                        ))?;
                self.green_output = Some(green.take_output(session)?);
                Ok(InMemorySetextActivationProgress::Ready)
            }
        }
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.donor_source.is_some() && self.green_output.is_some()
    }

    pub(crate) const fn green_started(&self) -> bool {
        self.green_started
    }

    pub(crate) fn take_ready(
        mut self,
    ) -> Result<ReadyInMemorySetextActivation, InMemorySetextActivationError> {
        if !self.is_ready() {
            return Err(InMemorySetextActivationError::InvalidState(
                "retained activation is not ready",
            ));
        }
        Ok(ReadyInMemorySetextActivation {
            activation_id: self.activation_id,
            old_epoch: self.old_epoch,
            old_binding: self.old_binding,
            new_spec: self.new_spec,
            donor_source: self.donor_source.take().expect("ready was checked"),
            green: self.green_output.take().expect("ready was checked"),
        })
    }
}

pub(crate) struct ReadyInMemorySetextActivation {
    activation_id: NonZeroU64,
    old_epoch: LiveCandidateEpoch,
    old_binding: SerializedGreenManifestDescriptor,
    new_spec: SerializedGreenRootSpec,
    donor_source: DonorResumedRetainedSetextSourceActivation,
    green: SetextRetainedGreenRestartOutput,
}

impl ReadyInMemorySetextActivation {
    pub(crate) const fn activation_id(&self) -> NonZeroU64 {
        self.activation_id
    }

    pub(crate) const fn old_epoch(&self) -> LiveCandidateEpoch {
        self.old_epoch
    }

    pub(crate) const fn old_binding(&self) -> SerializedGreenManifestDescriptor {
        self.old_binding
    }

    #[cfg(test)]
    pub(crate) fn corrupt_old_binding_for_test(&mut self) {
        self.old_binding.semantic_epoch = self
            .old_binding
            .semantic_epoch
            .checked_add(1)
            .expect("test fixture semantic epoch does not overflow");
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        DonorResumedRetainedSetextSourceActivation,
        SetextRetainedGreenRestartOutput,
        SerializedGreenManifestDescriptor,
        SerializedGreenRootSpec,
    ) {
        (
            self.donor_source,
            self.green,
            self.old_binding,
            self.new_spec,
        )
    }
}

/// Linear provenance/source authority consumed by fresh-candidate activation.
/// `old_binding` identifies the exact old manifest, including semantic epoch;
/// `old_epoch` binds it to the actor arena/build that minted the joined pause.
#[derive(Debug)]
pub(crate) struct InMemorySetextActivationAuthority {
    old_epoch: LiveCandidateEpoch,
    old_binding: SerializedGreenManifestDescriptor,
    source: RetainedSetextSourceLedgerDraft,
}

impl InMemorySetextActivationAuthority {
    pub(crate) const fn old_epoch(&self) -> LiveCandidateEpoch {
        self.old_epoch
    }

    pub(crate) const fn old_binding(&self) -> SerializedGreenManifestDescriptor {
        self.old_binding
    }

    pub(crate) const fn source(&self) -> &RetainedSetextSourceLedgerDraft {
        &self.source
    }

    pub(crate) fn into_source(self) -> RetainedSetextSourceLedgerDraft {
        self.source
    }
}
