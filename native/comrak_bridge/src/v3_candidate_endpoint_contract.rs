//! Public endpoint errors, credits, and publication events.

use super::*;

#[derive(Debug)]
pub(crate) enum CandidateEndpointError {
    Busy,
    InvalidState,
    InvalidAuthority,
    MetricOverflow,
    AllocationFailed,
    Parse(M11CleanParseJobError),
    Crop(M11LeadingReferencesCropError),
    OrdinaryCrop(M11OrdinaryParagraphCropError),
    OrdinaryBoundaryCrop(M11OrdinaryParagraphBoundaryCropError),
    Document(DocumentRuntimeError),
    Derive(M11CandidateDerivationError),
    InlinePublication(M11InlinePublicationError),
    InlineRange(M11PublishedInlineRangeError),
    InlineProjectionJob(M11InlineProjectionJobError),
    InlineProjection(M11InlineProjectionError),
    RecursiveGreenParagraph(M11RecursiveGreenParagraphPreparationError),
    PersistentRecursiveGreen(M11PersistentRecursiveGreenSessionError),
    IndentedCodeProjectionJob(M11IndentedCodeProjectionJobError),
    IndentedCodeProjection(M11IndentedCodeProjectionError),
    BlockQuoteProjectionJob(M11BlockQuoteProjectionJobError),
    BlockQuoteProjection(M11BlockQuoteProjectionError),
    BulletListLocal(M11BulletListLocalDeltaError),
    ParserPage(M11ParserPageError),
    Publication(M11PublicationError),
    Transport(CandidateTransportDigestError),
    ViewportTransport(ViewportPresentationTransportDigestError),
    Wire(EncodeError),
    ViewportInlineLimitExceeded(&'static str),
}

impl fmt::Display for CandidateEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Flark v3 candidate endpoint failure: ")?;
        match self {
            Self::Busy => formatter.write_str("busy"),
            Self::InvalidState => formatter.write_str("invalid state"),
            Self::InvalidAuthority => formatter.write_str("invalid authority"),
            Self::MetricOverflow => formatter.write_str("metric overflow"),
            Self::AllocationFailed => formatter.write_str("bounded allocation failed"),
            Self::Parse(error) => error.fmt(formatter),
            Self::Crop(error) => error.fmt(formatter),
            Self::OrdinaryCrop(error) => error.fmt(formatter),
            Self::OrdinaryBoundaryCrop(error) => error.fmt(formatter),
            Self::Document(error) => error.fmt(formatter),
            Self::Derive(error) => error.fmt(formatter),
            Self::InlinePublication(error) => error.fmt(formatter),
            Self::InlineRange(error) => error.fmt(formatter),
            Self::InlineProjectionJob(error) => error.fmt(formatter),
            Self::InlineProjection(error) => error.fmt(formatter),
            Self::RecursiveGreenParagraph(error) => error.fmt(formatter),
            Self::PersistentRecursiveGreen(error) => error.fmt(formatter),
            Self::IndentedCodeProjectionJob(error) => error.fmt(formatter),
            Self::IndentedCodeProjection(error) => error.fmt(formatter),
            Self::BlockQuoteProjectionJob(error) => error.fmt(formatter),
            Self::BlockQuoteProjection(error) => error.fmt(formatter),
            Self::BulletListLocal(error) => write!(formatter, "{error:?}"),
            Self::ParserPage(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
            Self::Transport(error) => write!(formatter, "{error:?}"),
            Self::ViewportTransport(error) => write!(formatter, "{error:?}"),
            Self::Wire(error) => error.fmt(formatter),
            Self::ViewportInlineLimitExceeded(limit) => {
                write!(formatter, "viewport inline batch exceeded {limit}")
            }
        }
    }
}

impl std::error::Error for CandidateEndpointError {}

impl From<M11CleanParseJobError> for CandidateEndpointError {
    fn from(error: M11CleanParseJobError) -> Self {
        Self::Parse(error)
    }
}

impl From<M11LeadingReferencesCropError> for CandidateEndpointError {
    fn from(error: M11LeadingReferencesCropError) -> Self {
        Self::Crop(error)
    }
}

impl From<M11OrdinaryParagraphCropError> for CandidateEndpointError {
    fn from(error: M11OrdinaryParagraphCropError) -> Self {
        Self::OrdinaryCrop(error)
    }
}

impl From<M11OrdinaryParagraphBoundaryCropError> for CandidateEndpointError {
    fn from(error: M11OrdinaryParagraphBoundaryCropError) -> Self {
        Self::OrdinaryBoundaryCrop(error)
    }
}

impl From<DocumentRuntimeError> for CandidateEndpointError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self::Document(error)
    }
}

impl From<M11CandidateDerivationError> for CandidateEndpointError {
    fn from(error: M11CandidateDerivationError) -> Self {
        Self::Derive(error)
    }
}

impl From<M11InlinePublicationError> for CandidateEndpointError {
    fn from(error: M11InlinePublicationError) -> Self {
        Self::InlinePublication(error)
    }
}

impl From<M11PublishedInlineRangeError> for CandidateEndpointError {
    fn from(error: M11PublishedInlineRangeError) -> Self {
        Self::InlineRange(error)
    }
}

impl From<M11InlineProjectionJobError> for CandidateEndpointError {
    fn from(error: M11InlineProjectionJobError) -> Self {
        Self::InlineProjectionJob(error)
    }
}

impl From<M11InlineProjectionError> for CandidateEndpointError {
    fn from(error: M11InlineProjectionError) -> Self {
        Self::InlineProjection(error)
    }
}

impl From<M11RecursiveGreenParagraphPreparationError> for CandidateEndpointError {
    fn from(error: M11RecursiveGreenParagraphPreparationError) -> Self {
        Self::RecursiveGreenParagraph(error)
    }
}

impl From<M11PersistentRecursiveGreenSessionError> for CandidateEndpointError {
    fn from(error: M11PersistentRecursiveGreenSessionError) -> Self {
        Self::PersistentRecursiveGreen(error)
    }
}

impl From<M11IndentedCodeProjectionJobError> for CandidateEndpointError {
    fn from(error: M11IndentedCodeProjectionJobError) -> Self {
        Self::IndentedCodeProjectionJob(error)
    }
}

impl From<M11IndentedCodeProjectionError> for CandidateEndpointError {
    fn from(error: M11IndentedCodeProjectionError) -> Self {
        Self::IndentedCodeProjection(error)
    }
}

impl From<M11BlockQuoteProjectionJobError> for CandidateEndpointError {
    fn from(error: M11BlockQuoteProjectionJobError) -> Self {
        Self::BlockQuoteProjectionJob(error)
    }
}

impl From<M11BlockQuoteProjectionError> for CandidateEndpointError {
    fn from(error: M11BlockQuoteProjectionError) -> Self {
        Self::BlockQuoteProjection(error)
    }
}

impl From<M11BulletListLocalDeltaError> for CandidateEndpointError {
    fn from(error: M11BulletListLocalDeltaError) -> Self {
        Self::BulletListLocal(error)
    }
}

impl From<M11ParserPageError> for CandidateEndpointError {
    fn from(error: M11ParserPageError) -> Self {
        Self::ParserPage(error)
    }
}

impl From<M11PublicationError> for CandidateEndpointError {
    fn from(error: M11PublicationError) -> Self {
        Self::Publication(error)
    }
}

impl From<CandidateTransportDigestError> for CandidateEndpointError {
    fn from(error: CandidateTransportDigestError) -> Self {
        Self::Transport(error)
    }
}

impl From<ViewportPresentationTransportDigestError> for CandidateEndpointError {
    fn from(error: ViewportPresentationTransportDigestError) -> Self {
        Self::ViewportTransport(error)
    }
}

impl From<EncodeError> for CandidateEndpointError {
    fn from(error: EncodeError) -> Self {
        Self::Wire(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CandidateCredit {
    Begin,
    Packet {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    Commit,
    Delivery,
}

pub(crate) enum CandidateEventBody {
    Begin(OfferBegin),
    Packet { encoded: Vec<u8> },
    Commit(CommitRequest),
    DeliveryAcknowledged(StructuralAck),
}

impl CandidateEventBody {
    pub(crate) fn borrowed(&self) -> Result<PublicationEventBody<'_>, DecodeError> {
        Ok(match self {
            Self::Begin(begin) => PublicationEventBody::Begin(*begin),
            Self::Packet { encoded } => {
                PublicationEventBody::Packet(decode_publication_packet(encoded)?)
            }
            Self::Commit(commit) => PublicationEventBody::Commit(*commit),
            Self::DeliveryAcknowledged(ack) => PublicationEventBody::DeliveryAcknowledged(*ack),
        })
    }
}

pub(crate) struct CandidateEvent {
    pub(crate) credit: CandidateCredit,
    pub(crate) body: CandidateEventBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotInlineCredit {
    Begin,
    Packet {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    Commit,
    Delivery,
}

pub(crate) enum HotInlineEventBody {
    Begin(HotInlineSidecarBegin),
    Packet { encoded: Vec<u8> },
    Commit(HotInlineSidecarCommitRequest),
    DeliveryAcknowledged(InlineSidecarAck),
}

impl HotInlineEventBody {
    pub(crate) fn borrowed(&self) -> Result<HotInlineSidecarEventBody<'_>, DecodeError> {
        Ok(match self {
            Self::Begin(begin) => HotInlineSidecarEventBody::Begin(*begin),
            Self::Packet { encoded } => {
                HotInlineSidecarEventBody::Packet(decode_publication_packet(encoded)?)
            }
            Self::Commit(commit) => HotInlineSidecarEventBody::Commit(*commit),
            Self::DeliveryAcknowledged(ack) => {
                HotInlineSidecarEventBody::DeliveryAcknowledged(*ack)
            }
        })
    }
}

pub(crate) struct HotInlineEvent {
    pub(crate) credit: HotInlineCredit,
    pub(crate) body: HotInlineEventBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewportPresentationCredit {
    Begin,
    Packet {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    Commit,
    Delivery,
}

pub(crate) enum CandidateViewportPresentationEventBody {
    Begin(ViewportPresentationBegin),
    Packet { encoded: Vec<u8> },
    Commit(ViewportPresentationCommitRequest),
    DeliveryAcknowledged(ViewportPresentationAck),
}

impl CandidateViewportPresentationEventBody {
    pub(crate) fn borrowed(&self) -> Result<ViewportPresentationEventBody<'_>, DecodeError> {
        Ok(match self {
            Self::Begin(begin) => ViewportPresentationEventBody::Begin(*begin),
            Self::Packet { encoded } => {
                ViewportPresentationEventBody::Packet(decode_publication_packet(encoded)?)
            }
            Self::Commit(commit) => ViewportPresentationEventBody::Commit(*commit),
            Self::DeliveryAcknowledged(ack) => {
                ViewportPresentationEventBody::DeliveryAcknowledged(*ack)
            }
        })
    }
}

pub(crate) struct CandidateViewportPresentationEvent {
    pub(crate) credit: ViewportPresentationCredit,
    pub(crate) body: CandidateViewportPresentationEventBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewportPresentationUnavailableReason {
    BudgetExceeded,
    DerivationFailed,
}

pub(crate) enum CandidatePoll {
    Pending {
        transitions: usize,
    },
    Event {
        transitions: usize,
        event: Box<CandidateEvent>,
    },
    HotInlineEvent {
        transitions: usize,
        event: Box<HotInlineEvent>,
    },
    ViewportPresentationEvent {
        transitions: usize,
        event: Box<CandidateViewportPresentationEvent>,
    },
    ViewportPresentationUnavailable {
        transitions: usize,
        viewport_generation: u32,
        reason: ViewportPresentationUnavailableReason,
    },
}
