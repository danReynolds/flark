//! Same-build composite line-boundary checkpoint authority.
//!
//! This module deliberately stops before committed/cross-build restart. The
//! types here move the one live parser pause through the one live candidate
//! writer and can resume only under the exact same [`LiveCandidateEpoch`].

use flark_comrak_value_block_core::{
    DirectBlockKind, DirectDurableGrammarCapture, DirectLineBoundaryDeferredRole,
    DirectLineBoundaryPairingView, DirectLineBoundaryPause, DirectValueBlockParser, ParseError,
    SyntaxProfile,
};

use crate::{
    CandidateWriterBinding, CandidateWriterError, CandidateWriterLineBoundaryContinuation,
    GreenKind, LiveCandidateEpoch,
};

/// Parser half of one same-build composite capture. It owns the real opaque
/// direct-parser pause; the copied views below are validation observations,
/// never an independently resumable parser state.
#[must_use = "parser checkpoint authority must enter the writer join or be discarded"]
pub(crate) struct ParserLineBoundaryCheckpointAuthority {
    epoch: LiveCandidateEpoch,
    pause: DirectLineBoundaryPause,
}

impl std::fmt::Debug for ParserLineBoundaryCheckpointAuthority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ParserLineBoundaryCheckpointAuthority")
            .field("epoch", &self.epoch)
            .field("pause", &self.pause.receipt())
            .finish_non_exhaustive()
    }
}

impl ParserLineBoundaryCheckpointAuthority {
    pub(crate) fn capture(
        epoch: LiveCandidateEpoch,
        parser: DirectValueBlockParser,
    ) -> Result<Self, Box<ParserLineBoundaryCaptureFailure>> {
        match parser.capture_line_boundary_pause() {
            Ok(pause) => Ok(Self::new(epoch, pause)),
            Err(error) => Err(Box::new(ParserLineBoundaryCaptureFailure { error, parser })),
        }
    }

    pub(crate) fn new(epoch: LiveCandidateEpoch, pause: DirectLineBoundaryPause) -> Self {
        debug_assert_eq!(pause.pairing_view().profile(), SyntaxProfile::CommonMark);
        Self { epoch, pause }
    }

    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    pub(crate) const fn pairing_view(&self) -> DirectLineBoundaryPairingView<'_> {
        self.pause.pairing_view()
    }

    pub(crate) fn open_green_kinds(
        &self,
    ) -> impl ExactSizeIterator<Item = GreenKind> + DoubleEndedIterator + '_ {
        self.pause
            .pairing_view()
            .open_kinds()
            .map(direct_green_kind)
    }

    pub(crate) const fn deferred_role(&self) -> DirectLineBoundaryDeferredRole {
        self.pause.pairing_view().deferred_role()
    }

    pub(crate) fn into_pause(self) -> DirectLineBoundaryPause {
        self.pause
    }

    /// Derives the donor's own split durable representation from the real
    /// joined pause. The pause remains intact for ordinary same-build resume;
    /// no consumer reconstructs donor frames or copies pairing scalars.
    pub(crate) fn capture_durable_sample(&self) -> Result<DirectDurableGrammarCapture, ParseError> {
        DirectValueBlockParser::resume_line_boundary_pause(self.pause.clone())?
            .capture_durable_grammar_line_boundary_checkpoint()
    }

    fn clone_for_joined_donor(&self) -> Self {
        Self {
            epoch: self.epoch,
            pause: self.pause.clone(),
        }
    }
}

pub(crate) struct ParserLineBoundaryCaptureFailure {
    pub(crate) error: ParseError,
    pub(crate) parser: DirectValueBlockParser,
}

/// Driver-owned half of one fully joined same-build checkpoint. All arena,
/// source, projection, and green authority remains inside the document's
/// paused writer slot. This token owns only the exact parser pause and the
/// coordinate-free proof that the document-side cut passed the composite
/// join.
#[must_use = "a joined checkpoint must be resumed or cancelled with its candidate"]
pub(crate) struct SameBuildLineBoundaryCheckpoint {
    epoch: LiveCandidateEpoch,
    parser: ParserLineBoundaryCheckpointAuthority,
    _reset: CheckpointProjectionResetAtCut,
}

/// Parser pairing authority and donor bytes minted from the same opaque pause.
/// A caller cannot pass a bare donor capture beside a different pause.
#[must_use = "the joined donor sample must enter the writer draft or be discarded"]
pub(crate) struct JoinedParserDonorSample {
    parser: ParserLineBoundaryCheckpointAuthority,
    donor: DirectDurableGrammarCapture,
}

impl std::fmt::Debug for JoinedParserDonorSample {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JoinedParserDonorSample")
            .field("epoch", &self.parser.epoch())
            .field("donor", &self.donor.receipt())
            .finish_non_exhaustive()
    }
}

impl JoinedParserDonorSample {
    pub(crate) const fn parser(&self) -> &ParserLineBoundaryCheckpointAuthority {
        &self.parser
    }

    pub(crate) fn into_donor(self) -> DirectDurableGrammarCapture {
        self.donor
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        ParserLineBoundaryCheckpointAuthority,
        DirectDurableGrammarCapture,
    ) {
        (self.parser, self.donor)
    }
}

impl std::fmt::Debug for SameBuildLineBoundaryCheckpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SameBuildLineBoundaryCheckpoint")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

impl SameBuildLineBoundaryCheckpoint {
    pub(crate) fn join(
        parser: ParserLineBoundaryCheckpointAuthority,
        writer: &CandidateWriterLineBoundaryContinuation,
        bindings: &[CandidateWriterBinding],
    ) -> Result<Self, Box<SameBuildLineBoundaryJoinFailure>> {
        if let Err(error) = writer.validate_parser_pairing(&parser, bindings) {
            return Err(Box::new(SameBuildLineBoundaryJoinFailure { error, parser }));
        }
        Ok(Self {
            epoch: parser.epoch(),
            parser,
            _reset: CheckpointProjectionResetAtCut { _private: () },
        })
    }

    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    /// Purely validates the opaque parser half before the actor-owned writer
    /// is moved out of its paused slot. Resume of the same unchanged value is
    /// deterministic, so a later consuming resume cannot introduce a new
    /// validation outcome after the writer transition succeeds.
    pub(crate) fn validate_parser_resume(&self) -> Result<(), ParseError> {
        DirectValueBlockParser::resume_line_boundary_pause(self.parser.pause.clone()).map(drop)
    }

    pub(crate) fn capture_joined_donor_sample(
        &self,
    ) -> Result<JoinedParserDonorSample, ParseError> {
        Ok(JoinedParserDonorSample {
            parser: self.parser.clone_for_joined_donor(),
            donor: self.parser.capture_durable_sample()?,
        })
    }

    #[cfg(test)]
    pub(crate) fn parser_retention_for_test(
        &self,
    ) -> flark_comrak_value_block_core::DirectLineBoundaryPauseReceipt {
        self.parser.pause.receipt()
    }

    pub(crate) fn resume_parser(self) -> Result<DirectValueBlockParser, ParseError> {
        DirectValueBlockParser::resume_line_boundary_pause(self.parser.into_pause())
    }
}

pub(crate) struct SameBuildLineBoundaryJoinFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) parser: ParserLineBoundaryCheckpointAuthority,
}

/// Role proof that the exact cut already owned by the enclosing composite
/// checkpoint is a projection restart point. It deliberately stores no
/// coordinate: the parser/source/composer/green join has one cut, not a second
/// caller-supplied reset location.
///
/// Construction stays private to this module. In particular, neither a plain
/// parser pause nor a plain green cut can mint it.
#[derive(Debug)]
struct CheckpointProjectionResetAtCut {
    _private: (),
}

const fn direct_green_kind(kind: DirectBlockKind) -> GreenKind {
    match kind {
        DirectBlockKind::Document => GreenKind::DOCUMENT,
        DirectBlockKind::BlockQuote => GreenKind::BLOCK_QUOTE,
        DirectBlockKind::List(_) => GreenKind::LIST,
        DirectBlockKind::Item(_) => GreenKind::ITEM,
        DirectBlockKind::Paragraph => GreenKind::PARAGRAPH,
        DirectBlockKind::Heading(_) => GreenKind::HEADING,
        DirectBlockKind::FencedCode(_) => GreenKind::FENCED_CODE,
    }
}
