//! Version-3 parser-publication payloads for native and Wasm worker endpoints.
//!
//! Publication events travel from the parser worker to Dart as request frames.
//! Terminal host-poll results travel back as response frames. Event receipts
//! deliberately remain owned by [`crate::v3_session_wire`], which is the only
//! Rust implementation of that global session protocol.

use std::fmt;

use crate::{
    v3_session_wire::SessionBinding,
    v3_wire::{self, DecodeLimits, FrameKind, Header, Opcode, Status},
};

pub const PAYLOAD_SCHEMA: u16 = 4;
pub const PAYLOAD_PREFIX_BYTES: usize = 28;
pub const POLL_TICKET_BYTES: usize = 24;
pub const STRUCTURAL_ACK_BYTES: usize = 124;
pub const BEGIN_BYTES_WITHOUT_BASE: usize = 144;
pub const HOT_INLINE_SIDECAR_BEGIN_BYTES: usize = 364;
pub const INLINE_SIDECAR_ACK_BYTES: usize = 212;
pub const VIEWPORT_PRESENTATION_BEGIN_BYTES: usize = 348;
pub const VIEWPORT_PRESENTATION_ACK_BYTES: usize = 296;
pub const VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES: usize = 144;
pub const VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES: usize = 12;
pub const VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES: usize = 192;
pub const VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES: usize = 28;
pub const VIEWPORT_PRESENTATION_END_FRAME_BYTES: usize = 52;
pub const VIEWPORT_PRESENTATION_FRAME_SCHEMA: u16 = 1;
pub const VIEWPORT_PRESENTATION_FRAME_FLAGS: u16 = 0;
pub const VIEWPORT_PRESENTATION_PARENT_MAGIC: u32 = u32::from_le_bytes(*b"VPB1");
pub const VIEWPORT_PRESENTATION_DIRECTORY_MAGIC: u32 = u32::from_le_bytes(*b"VPD1");
pub const VIEWPORT_PRESENTATION_CHILD_MAGIC: u32 = u32::from_le_bytes(*b"VPC1");
pub const VIEWPORT_PRESENTATION_END_MAGIC: u32 = u32::from_le_bytes(*b"VPE1");
pub const HIO1_ENVELOPE_BYTES: u32 = 256;
/// Current dual-root persistent inline descriptor width.
///
/// The constant name is retained as a legacy wire/API label.
pub const IPR3_DESCRIPTOR_BYTES: u32 = 280;
pub const PROJECTED_INLINE_PROJECTION_DESCRIPTOR_BYTES: u32 = 328;
pub const INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES: u32 = 160;
pub const BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES: u32 = 168;
/// Little-endian bytes are the ASCII tag `FPK3` at the raw host ABI seam.
pub const PACKET_MAGIC: u32 = u32::from_le_bytes(*b"FPK3");
pub const PACKET_SCHEMA: u16 = 1;
pub const PACKET_FLAGS: u16 = 0;
pub const PACKET_HEADER_BYTES: usize = 44;
pub const PACKET_FRAME_DESCRIPTOR_BYTES: usize = 24;
pub const MAXIMUM_PACKET_FRAME_COUNT: u32 = 256;
pub const MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES: u32 = 64 * 1024;
pub const MAXIMUM_PACKET_ENCODED_BYTES: usize = PACKET_HEADER_BYTES
    + PACKET_FRAME_DESCRIPTOR_BYTES * MAXIMUM_PACKET_FRAME_COUNT as usize
    + MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize;

const SUPPORTED_MANIFEST_SCHEMA: u32 = 1;
pub const HOT_INLINE_SIDECAR_SCHEMA: u32 = 3;
const KNOWN_AUTHORITY_BITS: u32 = 0x1f;
const HOT_INLINE_SIDECAR_VARIANT: u16 = 0x0100;
const HOT_INLINE_SIDECAR_FAILED_VARIANT: u16 = 0x0101;
const HOT_INLINE_SIDECAR_PACKET_CREDIT_VARIANT: u16 = 0x0110;
const HOT_INLINE_SIDECAR_COMMITTED_VARIANT: u16 = 0x0111;
const HOT_INLINE_SIDECAR_ABORT_COMPLETE_VARIANT: u16 = 0x0112;
const SUPPORTED_VIEWPORT_PRESENTATION_SCHEMA: u32 = 1;
const VIEWPORT_PRESENTATION_VARIANT: u16 = 0x0200;
const VIEWPORT_PRESENTATION_FAILED_VARIANT: u16 = 0x0201;
const VIEWPORT_PRESENTATION_PACKET_CREDIT_VARIANT: u16 = 0x0210;
const VIEWPORT_PRESENTATION_COMMITTED_VARIANT: u16 = 0x0211;
const VIEWPORT_PRESENTATION_ABORT_COMPLETE_VARIANT: u16 = 0x0212;
const PRODUCT_MAX_PACKET_BYTES: u32 = MAXIMUM_PACKET_ENCODED_BYTES as u32;
const PRODUCT_MAX_FRAME_BYTES: u32 = flark_engine::m11_host::M11_HOST_MAXIMUM_FRAME_BYTES as u32;
const PRODUCT_MAX_PROGRAM_CHILDREN: u32 =
    flark_engine::m11_host::M11_HOST_MAXIMUM_PROGRAM_CHILDREN as u32;

pub type Id128 = [u32; 4];
pub type Digest128 = [u32; 4];

/// Domain of one deliberate 256-bit to protocol-128 digest conversion.
///
/// The candidate snapshot keeps its full engine-owned BLAKE3 proof. These
/// domains produce only the fixed-width transport witnesses carried by the v3
/// wire; callers must never truncate an engine digest ad hoc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolDigestDomain {
    CandidateFrame,
    CandidateStream,
    CandidateAckSequence,
    CandidateManifest,
    CandidateTransport,
    HotInlineSidecarFrame,
    HotInlineSidecarRootStream,
    HotInlineSidecarAckSequence,
    HotInlineSidecarTransport,
    ViewportPresentationFrame,
    ViewportPresentationEnvelope,
    ViewportPresentationRootStream,
    ViewportPresentationAckSequence,
    ViewportPresentationTransport,
}

impl ProtocolDigestDomain {
    const fn label(self) -> &'static [u8] {
        match self {
            Self::CandidateFrame => b"candidate-frame",
            Self::CandidateStream => b"candidate-stream",
            Self::CandidateAckSequence => b"candidate-ack-sequence",
            Self::CandidateManifest => b"candidate-manifest",
            Self::CandidateTransport => b"candidate-transport",
            Self::HotInlineSidecarFrame => b"hot-inline-sidecar-frame",
            Self::HotInlineSidecarRootStream => b"hot-inline-sidecar-root-stream",
            Self::HotInlineSidecarAckSequence => b"hot-inline-sidecar-ack-sequence",
            Self::HotInlineSidecarTransport => b"hot-inline-sidecar-transport",
            Self::ViewportPresentationFrame => b"viewport-presentation-frame",
            Self::ViewportPresentationEnvelope => b"viewport-presentation-envelope",
            Self::ViewportPresentationRootStream => b"viewport-presentation-root-stream",
            Self::ViewportPresentationAckSequence => b"viewport-presentation-ack-sequence",
            Self::ViewportPresentationTransport => b"viewport-presentation-transport",
        }
    }
}

/// Converts one complete 256-bit BLAKE3 proof into a domain-bound wire digest.
///
/// This is the only supported narrowing seam for M1.1 candidate publication.
/// The conversion rehashes all 256 input bits with a named protocol domain
/// before selecting the four little-endian wire words.
#[must_use]
pub fn protocol_digest128_from_blake3(
    domain: ProtocolDigestDomain,
    digest256: [u8; 32],
) -> Digest128 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.protocol.digest128.v1\0");
    hasher.update(domain.label());
    hasher.update(&[0]);
    hasher.update(&digest256);
    let narrowed = hasher.finalize();
    let bytes = narrowed.as_bytes();
    [
        u32::from_le_bytes(bytes[0..4].try_into().expect("four-byte digest lane")),
        u32::from_le_bytes(bytes[4..8].try_into().expect("four-byte digest lane")),
        u32::from_le_bytes(bytes[8..12].try_into().expect("four-byte digest lane")),
        u32::from_le_bytes(bytes[12..16].try_into().expect("four-byte digest lane")),
    ]
}

/// Semantic kind of one complete candidate-snapshot frame carried by a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CandidateSnapshotFrameKind {
    Begin = 1,
    Node = 2,
    End = 3,
    /// One canonical SourceFacts v2 replacement page in an exact-base
    /// transaction. Its embedded ordinal is a target page ordinal, not a
    /// snapshot-node ordinal.
    SourceFactsReplacementPage = 4,
    /// One canonical packed BlockSequence replacement page in an exact-base
    /// transaction. Its embedded ordinal is a target storage-page ordinal.
    BlockSequenceReplacementPage = 5,
    /// One canonical recursive-Green RGL1 replacement leaf in an exact-base
    /// transaction. RGB1 branches are rebuilt by the independent host.
    RecursiveGreenReplacementPage = 6,
}

/// Semantic kind of one frame in a hot-inline sidecar root closure.
///
/// The sidecar uses the same FPK3 packet envelope as structural publication,
/// but a separate digest domain prevents a valid sidecar frame stream from
/// being replayed as a canonical candidate stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HotInlineSidecarFrameKind {
    Begin = 1,
    Node = 2,
    End = 3,
}

/// Semantic kind of one frame in an aggregate viewport-presentation closure.
///
/// The parent envelope and ordered directory are distinct from each opaque
/// child frame. This prevents a structurally valid HIO1 or candidate frame
/// stream from being replayed as one VPB1 page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ViewportPresentationFrameKind {
    Begin = 1,
    Directory = 2,
    Child = 3,
    End = 4,
}

/// Ordered transport digest failure. It is separate from wire encoding so the
/// independent host can validate copied frame bodies before mutating staging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateTransportDigestError {
    OutOfOrder,
    MetricOverflow,
}

/// Final exact totals of one candidate snapshot's credited frame stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateTransportDigestReceipt {
    pub frame_count: u32,
    pub encoded_frame_bytes: u32,
    pub digest256: [u8; 32],
}

/// Domain-separated digest of one complete snapshot frame.
#[must_use]
pub fn candidate_frame_digest256(
    ordinal: u32,
    kind: CandidateSnapshotFrameKind,
    bytes: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.candidate.frame.v1\0");
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&[kind as u8]);
    hasher.update(
        &u64::try_from(bytes.len())
            .expect("slice length always fits u64 on supported targets")
            .to_le_bytes(),
    );
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

/// One-pass ordered digest shared by the producer endpoint and independent
/// host adapter. It retains only counters and a BLAKE3 state, never frames.
pub struct CandidateTransportDigest {
    inner: OrderedTransportDigest,
}

struct OrderedTransportDigest {
    hasher: blake3::Hasher,
    next_frame_ordinal: u32,
    encoded_frame_bytes: u32,
}

impl OrderedTransportDigest {
    fn new(domain: &'static [u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(domain);
        Self {
            hasher,
            next_frame_ordinal: 0,
            encoded_frame_bytes: 0,
        }
    }

    fn push(
        &mut self,
        ordinal: u32,
        first_record_ordinal: u32,
        record_count: u32,
        kind: u8,
        bytes: &[u8],
        digest: [u8; 32],
    ) -> Result<(), CandidateTransportDigestError> {
        if ordinal != self.next_frame_ordinal {
            return Err(CandidateTransportDigestError::OutOfOrder);
        }
        let byte_len = u32::try_from(bytes.len())
            .map_err(|_| CandidateTransportDigestError::MetricOverflow)?;
        let next_bytes = self
            .encoded_frame_bytes
            .checked_add(byte_len)
            .ok_or(CandidateTransportDigestError::MetricOverflow)?;
        let next_frame_ordinal = self
            .next_frame_ordinal
            .checked_add(1)
            .ok_or(CandidateTransportDigestError::MetricOverflow)?;
        self.hasher.update(&ordinal.to_le_bytes());
        self.hasher.update(&first_record_ordinal.to_le_bytes());
        self.hasher.update(&record_count.to_le_bytes());
        self.hasher.update(&[kind]);
        self.hasher.update(&byte_len.to_le_bytes());
        self.hasher.update(&digest);
        self.encoded_frame_bytes = next_bytes;
        self.next_frame_ordinal = next_frame_ordinal;
        Ok(())
    }

    fn receipt(&self) -> CandidateTransportDigestReceipt {
        CandidateTransportDigestReceipt {
            frame_count: self.next_frame_ordinal,
            encoded_frame_bytes: self.encoded_frame_bytes,
            digest256: *self.hasher.finalize().as_bytes(),
        }
    }
}

impl CandidateTransportDigest {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: OrderedTransportDigest::new(b"flark.v3.candidate.transport.v1\0"),
        }
    }

    /// Admits one exact next frame and returns its full 256-bit frame digest.
    pub fn push(
        &mut self,
        ordinal: u32,
        first_record_ordinal: u32,
        record_count: u32,
        kind: CandidateSnapshotFrameKind,
        bytes: &[u8],
    ) -> Result<[u8; 32], CandidateTransportDigestError> {
        let digest = candidate_frame_digest256(ordinal, kind, bytes);
        self.inner.push(
            ordinal,
            first_record_ordinal,
            record_count,
            kind as u8,
            bytes,
            digest,
        )?;
        Ok(digest)
    }

    #[must_use]
    pub fn finish(self) -> CandidateTransportDigestReceipt {
        self.receipt()
    }

    /// Returns exact current totals without consuming the accumulator. The
    /// independent host uses this to validate Commit before relinquishing the
    /// active offer's abort capability.
    #[must_use]
    pub fn receipt(&self) -> CandidateTransportDigestReceipt {
        self.inner.receipt()
    }
}

impl Default for CandidateTransportDigest {
    fn default() -> Self {
        Self::new()
    }
}

/// Domain-separated digest of one complete hot-inline sidecar frame.
#[must_use]
pub fn hot_inline_sidecar_frame_digest256(
    ordinal: u32,
    kind: HotInlineSidecarFrameKind,
    bytes: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.hot-inline-sidecar.frame.v1\0");
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&[kind as u8]);
    hasher.update(
        &u64::try_from(bytes.len())
            .expect("slice length always fits u64 on supported targets")
            .to_le_bytes(),
    );
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

/// One-pass ordered transport digest for a hot-inline sidecar root.
///
/// Packet boundaries remain absent from the digest, exactly as for structural
/// publication. Only the semantic frame domain differs.
pub struct HotInlineSidecarTransportDigest {
    inner: OrderedTransportDigest,
}

impl HotInlineSidecarTransportDigest {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: OrderedTransportDigest::new(b"flark.v3.hot-inline-sidecar.transport.v1\0"),
        }
    }

    pub fn push(
        &mut self,
        ordinal: u32,
        first_record_ordinal: u32,
        record_count: u32,
        kind: HotInlineSidecarFrameKind,
        bytes: &[u8],
    ) -> Result<[u8; 32], CandidateTransportDigestError> {
        let digest = hot_inline_sidecar_frame_digest256(ordinal, kind, bytes);
        self.inner.push(
            ordinal,
            first_record_ordinal,
            record_count,
            kind as u8,
            bytes,
            digest,
        )?;
        Ok(digest)
    }

    #[must_use]
    pub fn finish(self) -> CandidateTransportDigestReceipt {
        self.receipt()
    }

    #[must_use]
    pub fn receipt(&self) -> CandidateTransportDigestReceipt {
        self.inner.receipt()
    }
}

impl Default for HotInlineSidecarTransportDigest {
    fn default() -> Self {
        Self::new()
    }
}

/// Domain-separated digest of one complete viewport-presentation frame.
#[must_use]
pub fn viewport_presentation_frame_digest256(
    ordinal: u32,
    kind: ViewportPresentationFrameKind,
    bytes: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.viewport-presentation.frame.v1\0");
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&[kind as u8]);
    hasher.update(
        &u64::try_from(bytes.len())
            .expect("slice length always fits u64 on supported targets")
            .to_le_bytes(),
    );
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPresentationTransportDigestError {
    OutOfOrder,
    MetricOverflow,
    InvalidFrameSequence,
    Incomplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewportPresentationTransportPhase {
    Begin,
    Directory,
    Children,
    Complete,
}

/// One-pass ordered digest for an aggregate VPB1 root closure.
///
/// Unlike the generic FPK3 codec, this accumulator authenticates the semantic
/// `Begin -> Directory -> Child* -> End` order. Packet boundaries remain
/// absent from the digest.
pub struct ViewportPresentationTransportDigest {
    inner: OrderedTransportDigest,
    phase: ViewportPresentationTransportPhase,
}

impl ViewportPresentationTransportDigest {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: OrderedTransportDigest::new(b"flark.v3.viewport-presentation.transport.v1\0"),
            phase: ViewportPresentationTransportPhase::Begin,
        }
    }

    pub fn push(
        &mut self,
        ordinal: u32,
        first_record_ordinal: u32,
        record_count: u32,
        kind: ViewportPresentationFrameKind,
        bytes: &[u8],
    ) -> Result<[u8; 32], ViewportPresentationTransportDigestError> {
        let next_phase = match (self.phase, kind) {
            (ViewportPresentationTransportPhase::Begin, ViewportPresentationFrameKind::Begin) => {
                ViewportPresentationTransportPhase::Directory
            }
            (
                ViewportPresentationTransportPhase::Directory,
                ViewportPresentationFrameKind::Directory,
            ) => ViewportPresentationTransportPhase::Children,
            (
                ViewportPresentationTransportPhase::Children,
                ViewportPresentationFrameKind::Child,
            ) => ViewportPresentationTransportPhase::Children,
            (ViewportPresentationTransportPhase::Children, ViewportPresentationFrameKind::End) => {
                ViewportPresentationTransportPhase::Complete
            }
            _ => {
                return Err(ViewportPresentationTransportDigestError::InvalidFrameSequence);
            }
        };
        let digest = viewport_presentation_frame_digest256(ordinal, kind, bytes);
        self.inner
            .push(
                ordinal,
                first_record_ordinal,
                record_count,
                kind as u8,
                bytes,
                digest,
            )
            .map_err(|error| match error {
                CandidateTransportDigestError::OutOfOrder => {
                    ViewportPresentationTransportDigestError::OutOfOrder
                }
                CandidateTransportDigestError::MetricOverflow => {
                    ViewportPresentationTransportDigestError::MetricOverflow
                }
            })?;
        self.phase = next_phase;
        Ok(digest)
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.phase, ViewportPresentationTransportPhase::Complete)
    }

    #[must_use]
    pub fn receipt(&self) -> CandidateTransportDigestReceipt {
        self.inner.receipt()
    }

    pub fn finish(
        self,
    ) -> Result<CandidateTransportDigestReceipt, ViewportPresentationTransportDigestError> {
        if self.is_complete() {
            Ok(self.inner.receipt())
        } else {
            Err(ViewportPresentationTransportDigestError::Incomplete)
        }
    }
}

impl Default for ViewportPresentationTransportDigest {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical aggregate commitment over the VPB1 parent metadata and complete
/// fixed-width directory.
///
/// The digest lane inside `envelope` is treated as zero, avoiding a circular
/// commitment. Both producer and host must validate the directory against the
/// exact Begin independently before trusting this shared recipe.
pub fn viewport_presentation_aggregate_envelope_digest256(
    binding: ViewportPresentationBinding,
    mut envelope: ViewportPresentationEnvelopeMetrics,
    directory_bytes: &[u8],
) -> Result<[u8; 32], DecodeError> {
    if directory_bytes.len() < VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES {
        return Err(exact_frame_length_error(
            directory_bytes.len(),
            VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES,
        ));
    }
    let mut directory_reader = PayloadReader::new(directory_bytes);
    read_viewport_presentation_frame_header(
        &mut directory_reader,
        VIEWPORT_PRESENTATION_DIRECTORY_MAGIC,
    )?;
    let entry_count = directory_reader.u32()?;
    let expected_directory_bytes = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
        .checked_add(
            usize::try_from(entry_count)
                .map_err(|_| invalid(8, None, Some(entry_count as usize)))?
                .checked_mul(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
                .ok_or_else(|| invalid(8, None, Some(entry_count as usize)))?,
        )
        .ok_or_else(|| invalid(8, None, Some(entry_count as usize)))?;
    if entry_count != envelope.ordered_leaf_count
        || expected_directory_bytes != directory_bytes.len()
    {
        return Err(invalid(
            8,
            Some(envelope.ordered_leaf_count as usize),
            Some(entry_count as usize),
        ));
    }

    envelope.aggregate_envelope_digest256 = [0; 32];
    let mut parent = [0_u8; VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES];
    let mut parent_writer = PayloadWriter::new(&mut parent);
    parent_writer.u32(VIEWPORT_PRESENTATION_PARENT_MAGIC);
    parent_writer.u16(VIEWPORT_PRESENTATION_FRAME_SCHEMA);
    parent_writer.u16(VIEWPORT_PRESENTATION_FRAME_FLAGS);
    parent_writer.u32(envelope.ordered_leaf_count);
    write_viewport_presentation_binding(&mut parent_writer, binding);
    write_viewport_presentation_envelope(&mut parent_writer, envelope);
    debug_assert_eq!(
        parent_writer.len(),
        VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES
    );

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.viewport-presentation.envelope.v1\0");
    hasher.update(&(parent.len() as u64).to_le_bytes());
    hasher.update(&parent);
    hasher.update(&(directory_bytes.len() as u64).to_le_bytes());
    hasher.update(directory_bytes);
    Ok(*hasher.finalize().as_bytes())
}

/// Canonical full root-stream commitment after every wrapper has been
/// consumed in semantic order.
#[must_use]
pub fn viewport_presentation_root_stream_digest256(
    aggregate_envelope_digest256: [u8; 32],
    transport: CandidateTransportDigestReceipt,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.viewport-presentation.root-stream.v1\0");
    hasher.update(&aggregate_envelope_digest256);
    hasher.update(&transport.frame_count.to_le_bytes());
    hasher.update(&transport.encoded_frame_bytes.to_le_bytes());
    hasher.update(&transport.digest256);
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceVersion {
    pub document_session: Id128,
    pub revision: u32,
    pub utf8_length: u32,
    pub utf16_length: u32,
    pub content_hash128: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralAck {
    pub publication_session: Id128,
    pub host_revision: u32,
    pub source_version: SourceVersion,
    pub source_root: [u32; 2],
    pub parse_generation: u32,
    pub grammar_revision: u32,
    pub syntax_profile: u32,
    pub authority_mask: u32,
    pub record_count: u32,
    pub sequence_digest: Digest128,
    pub manifest_digest: Digest128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationMode {
    FullSnapshot,
    /// Reuses only the canonical References root authenticated by `base_ack`.
    ///
    /// Every target wrapper and the target manifest remain fresh transferred
    /// nodes. This is intentionally not a generic record splice.
    ExactBaseReferencesDelta,
    /// Reuses exact installed References and applies an authenticated
    /// persistent SourceFacts page splice before admitting ordinary target
    /// nodes.
    ExactBaseDelta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfferLimits {
    /// Maximum number of snapshot frames in the complete publication.
    pub maximum_frame_count: u32,
    /// Maximum sum of snapshot frame-body bytes in the publication.
    pub maximum_encoded_frame_bytes: u32,
    /// Maximum encoded bytes in one FPK3 packet carried by schema-4 events.
    pub maximum_packet_bytes: u32,
    /// Maximum bytes in one snapshot frame.
    pub maximum_frame_bytes: u32,
    pub maximum_program_children: u32,
}

/// VPB1 transport bounds with a wider public-wrapper frame ceiling.
///
/// This is intentionally distinct from [`OfferLimits`], whose
/// engine-internal frame bound remains appropriate for structural and HIO1
/// publication only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationOfferLimits {
    pub maximum_frame_count: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_packet_bytes: u32,
    pub maximum_frame_bytes: u32,
    pub maximum_program_children: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfferBegin {
    pub schema: u32,
    pub offer_id: Id128,
    pub publication_session: Id128,
    pub target_host_revision: u32,
    pub source_version: SourceVersion,
    pub source_root: [u32; 2],
    pub parse_generation: u32,
    pub grammar_revision: u32,
    pub syntax_profile: u32,
    pub authority_mask: u32,
    pub mode: PublicationMode,
    pub base_ack: Option<StructuralAck>,
    /// Canonical target-role records represented by transferred frames.
    ///
    /// For a full snapshot this equals `target_record_count`. For an exact-base
    /// References delta it excludes only the reused References-role records.
    pub transferred_record_count: u32,
    pub target_record_count: u32,
    pub limits: OfferLimits,
}

/// Publication target of the sibling inline-refinement protocol.
///
/// This is deliberately not a [`PublicationMode`]: a sidecar cannot be
/// admitted as a structural target and can never replace a `StructuralAck`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotInlineSidecarMode {
    HotInlineSidecar,
}

/// Typed structural owner carried in the existing 64-bit HIO1 binding slot.
///
/// Structural block ordinals occupy the low half of the domain. Recursive
/// Green frame IDs set the high tag bit; both IDs are bounded far below that
/// bit by the protocol's 32-bit source metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotInlineSidecarOwner {
    BlockOrdinal(u64),
    RecursiveGreenFrame(u64),
}

const HOT_INLINE_RECURSIVE_GREEN_OWNER_TAG: u64 = 1_u64 << 63;

impl HotInlineSidecarOwner {
    #[must_use]
    pub const fn into_wire(self) -> Option<u64> {
        match self {
            Self::BlockOrdinal(ordinal) if ordinal < HOT_INLINE_RECURSIVE_GREEN_OWNER_TAG => {
                Some(ordinal)
            }
            Self::RecursiveGreenFrame(frame)
                if frame > 0 && frame < HOT_INLINE_RECURSIVE_GREEN_OWNER_TAG =>
            {
                Some(HOT_INLINE_RECURSIVE_GREEN_OWNER_TAG | frame)
            }
            _ => None,
        }
    }

    #[must_use]
    pub const fn from_wire(encoded: u64) -> Option<Self> {
        if encoded & HOT_INLINE_RECURSIVE_GREEN_OWNER_TAG == 0 {
            Some(Self::BlockOrdinal(encoded))
        } else {
            let frame = encoded & !HOT_INLINE_RECURSIVE_GREEN_OWNER_TAG;
            if frame == 0 {
                None
            } else {
                Some(Self::RecursiveGreenFrame(frame))
            }
        }
    }
}

/// Exact HIO1 block fence advertised before the root closure is transferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotInlineSidecarBinding {
    pub parser_profile: u64,
    pub refinement_generation: u64,
    /// Packed [`HotInlineSidecarOwner`] retained in the legacy ABI slot.
    pub block_ordinal: u64,
    pub physical_start_utf8: u32,
    pub physical_end_utf8: u32,
    pub visible_start_utf8: u32,
    pub visible_end_utf8: u32,
    pub physical_start_utf16: u32,
    pub physical_end_utf16: u32,
    pub visible_start_utf16: u32,
    pub visible_end_utf16: u32,
}

impl HotInlineSidecarBinding {
    #[must_use]
    pub const fn owner(self) -> Option<HotInlineSidecarOwner> {
        HotInlineSidecarOwner::from_wire(self.block_ordinal)
    }
}

/// Semantic summary encoded by the engine-owned HIO1 envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotInlineSidecarDisposition {
    Authoritative {
        logical_page_count: u64,
        fact_count: u64,
        storage_page_count: u64,
        /// Number of direct-link/image companion records in `FLKIV001`.
        link_value_entry_count: u32,
        /// Exact public `FLKIV001` width, or zero for the absent value lane.
        link_value_encoded_bytes: u32,
        /// Persistent pages owned by the companion-value tree.
        link_value_storage_page_count: u64,
        ordered_commitment256: [u8; 32],
    },
    Unsupported {
        reason: u32,
        metadata_commitment256: [u8; 32],
    },
}

/// Exact metrics needed to authenticate the HIO1 Begin frame without
/// inventing a candidate-role manifest for the bounded sidecar closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotInlineSidecarEnvelopeMetrics {
    /// Always the engine-owned fixed HIO1 envelope size.
    pub hio1_encoded_bytes: u32,
    /// Persistent projection descriptor bytes following HIO1 in the typed
    /// root Begin frame.
    ///
    /// The field name is retained as a legacy ABI slot label.
    ///
    /// Authoritative roots require the fixed descriptor. Unsupported
    /// certificates require zero descriptor bytes and one literal metadata
    /// Node before End.
    pub ipr2_descriptor_bytes: u32,
    /// Number of transferred persistent closure nodes. The typed Begin and End
    /// frames are not included.
    pub transferred_node_count: u32,
    /// Full commitment stored at the end of the exact HIO1 envelope.
    pub hio1_envelope_digest256: [u8; 32],
    pub disposition: HotInlineSidecarDisposition,
}

/// Typed sibling offer for one exact hot-inline root.
///
/// `base_ack` is complete, not a digest shortcut. Host admission must compare
/// it byte-for-byte with the currently installed structural ACK before
/// accepting the first FPK3 packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotInlineSidecarBegin {
    pub schema: u32,
    pub mode: HotInlineSidecarMode,
    pub offer_id: Id128,
    pub publication_session: Id128,
    pub base_ack: StructuralAck,
    pub binding: HotInlineSidecarBinding,
    pub envelope: HotInlineSidecarEnvelopeMetrics,
    pub limits: OfferLimits,
}

impl HotInlineSidecarBegin {
    /// Checks the exact installed structural base without weakening it to a
    /// selected subset of fields.
    pub fn require_exact_base(&self, installed: StructuralAck) -> Result<(), HostRejectReason> {
        if self.base_ack == installed {
            Ok(())
        } else {
            Err(HostRejectReason::BaseMismatch)
        }
    }
}

/// Sidecar-specific commit. Its root digest is not a candidate canonical
/// stream digest even though the fixed wire width matches [`CommitRequest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotInlineSidecarCommitRequest {
    pub offer_id: Id128,
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub rolling_transport_digest: Digest128,
    pub root_stream_digest: Digest128,
}

/// Receipt for one installed hot-inline generation.
///
/// The receipt retains the complete structural base and has no host revision
/// field of its own. Installing it therefore cannot be confused with
/// advancing or replacing the structural ACK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineSidecarAck {
    pub publication_session: Id128,
    pub base_ack: StructuralAck,
    pub refinement_generation: u64,
    pub block_ordinal: u64,
    pub transferred_node_count: u32,
    pub disposition: InlineSidecarAckDisposition,
    pub hio1_envelope_digest256: [u8; 32],
    pub root_stream_digest: Digest128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineSidecarAckDisposition {
    Authoritative,
    Unsupported,
}

/// Publication target of one aggregate, passive viewport page.
///
/// This is a sibling protocol rather than a structural publication mode or a
/// singleton inline sidecar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPresentationMode {
    AggregatePage,
}

/// Exact UTF-8/UTF-16 source range authenticated by one VPB1 page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationMetricRange {
    pub start_utf8: u32,
    pub start_utf16: u32,
    pub end_utf8: u32,
    pub end_utf16: u32,
}

/// Stable measured-sequence cut used to begin or resume one viewport page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationVisitStart {
    pub block_ordinal: u64,
    pub utf8_offset: u32,
    pub utf16_offset: u32,
}

/// Exact caller demand and producer-authenticated coverage for one VPB1 page.
///
/// `next` is always present, including on a complete page, so the receipt
/// authenticates the ordered structural-entry count without an optional cursor
/// encoding. `complete` is true exactly when that cut reaches the requested
/// range end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationBinding {
    pub viewport_generation: u32,
    pub requested_range: ViewportPresentationMetricRange,
    pub covered_range: ViewportPresentationMetricRange,
    pub start: ViewportPresentationVisitStart,
    pub next: ViewportPresentationVisitStart,
    pub complete: bool,
}

/// Caller-owned semantic work bounds repeated in VPB1 Begin.
///
/// These mirror the session command independently of [`OfferLimits`], which
/// bounds only publication transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationQueryLimits {
    pub maximum_structural_entries: u32,
    pub maximum_storage_pages: u32,
    pub maximum_inline_leaves: u32,
    pub maximum_inline_leaf_source_bytes: u32,
    pub maximum_inline_source_bytes: u32,
    pub maximum_fact_records: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_parser_transitions: u32,
}

/// Exact aggregate totals committed by the VPB1 parent envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationEnvelopeMetrics {
    pub visited_structural_entries: u32,
    pub visited_storage_pages: u32,
    pub ordered_leaf_count: u32,
    pub inline_source_bytes: u32,
    pub fact_count: u32,
    pub transferred_node_count: u32,
    pub parser_transitions: u32,
    pub aggregate_envelope_digest256: [u8; 32],
}

/// Typed offer for one atomic aggregate viewport page.
///
/// `base_ack` is complete and must compare byte-for-byte with the currently
/// installed structural ACK before the first FPK3 packet is admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationBegin {
    pub schema: u32,
    pub mode: ViewportPresentationMode,
    pub offer_id: Id128,
    pub publication_session: Id128,
    pub base_ack: StructuralAck,
    pub binding: ViewportPresentationBinding,
    pub envelope: ViewportPresentationEnvelopeMetrics,
    pub query_limits: ViewportPresentationQueryLimits,
    pub limits: ViewportPresentationOfferLimits,
}

impl ViewportPresentationBegin {
    pub fn require_exact_base(&self, installed: StructuralAck) -> Result<(), HostRejectReason> {
        if self.base_ack == installed {
            Ok(())
        } else {
            Err(HostRejectReason::BaseMismatch)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationCommitRequest {
    pub offer_id: Id128,
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub rolling_transport_digest: Digest128,
    pub aggregate_root_stream_digest: Digest128,
}

/// Receipt for one installed, non-structural viewport page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationAck {
    pub publication_session: Id128,
    pub base_ack: StructuralAck,
    pub binding: ViewportPresentationBinding,
    pub envelope: ViewportPresentationEnvelopeMetrics,
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub aggregate_root_stream_digest: Digest128,
}

/// Canonical VPB1 parent frame repeated inside the authenticated root stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationParentFrame {
    pub binding: ViewportPresentationBinding,
    pub envelope: ViewportPresentationEnvelopeMetrics,
}

/// One fixed-width leaf entry in the canonical VPB1 directory.
///
/// The child frame span addresses an opaque complete HIO1 closure in the same
/// ordered FPK3 root stream. Its envelope metadata is repeated here so a host
/// can locate and admit one leaf without decoding unrelated children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationDirectoryEntry {
    pub ordered_child_index: u32,
    /// Exact source-ordered row ordinal from the recursive-Green row window.
    ///
    /// `binding.block_ordinal` carries the independently authenticated
    /// recursive-Green owner frame in its legacy ABI slot.  Keeping the row
    /// ordinal separate prevents frame allocation order from being mistaken
    /// for document order after a splice.
    pub global_row_ordinal: u64,
    pub binding: HotInlineSidecarBinding,
    pub hio1_envelope: HotInlineSidecarEnvelopeMetrics,
}

/// Borrowed, allocation-free view over one validated fixed-width directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationDirectory<'payload> {
    pub entry_count: u32,
    encoded: &'payload [u8],
    entries: &'payload [u8],
}

impl<'payload> ViewportPresentationDirectory<'payload> {
    #[must_use]
    pub const fn encoded(&self) -> &'payload [u8] {
        self.encoded
    }

    #[must_use]
    pub fn entry(&self, index: u32) -> Option<ViewportPresentationDirectoryEntry> {
        if index >= self.entry_count {
            return None;
        }
        let start = usize::try_from(index)
            .ok()?
            .checked_mul(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)?;
        let end = start.checked_add(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)?;
        let mut reader = PayloadReader::new(self.entries.get(start..end)?);
        let entry = read_viewport_presentation_directory_entry(&mut reader).ok()?;
        reader.finish().ok()?;
        Some(entry)
    }

    #[must_use]
    pub fn entries(&self) -> ViewportPresentationDirectoryEntries<'payload> {
        ViewportPresentationDirectoryEntries {
            directory: *self,
            next_index: 0,
        }
    }
}

pub struct ViewportPresentationDirectoryEntries<'payload> {
    directory: ViewportPresentationDirectory<'payload>,
    next_index: u32,
}

impl Iterator for ViewportPresentationDirectoryEntries<'_> {
    type Item = ViewportPresentationDirectoryEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.directory.entry(self.next_index)?;
        self.next_index += 1;
        Some(entry)
    }
}

/// Canonical public wrapper around one opaque HIO1 frame body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationChildFrameInput<'payload> {
    pub directory_index: u32,
    pub child_frame_ordinal: u32,
    pub kind: HotInlineSidecarFrameKind,
    pub record_count: u32,
    pub payload: &'payload [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationChildFrame<'payload> {
    pub directory_index: u32,
    pub child_frame_ordinal: u32,
    pub kind: HotInlineSidecarFrameKind,
    pub record_count: u32,
    encoded: &'payload [u8],
    payload: &'payload [u8],
}

impl<'payload> ViewportPresentationChildFrame<'payload> {
    #[must_use]
    pub const fn encoded(&self) -> &'payload [u8] {
        self.encoded
    }

    #[must_use]
    pub const fn payload(&self) -> &'payload [u8] {
        self.payload
    }
}

/// Canonical terminal totals for one VPB1 root stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationEndFrame {
    pub ordered_leaf_count: u32,
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub aggregate_envelope_digest256: [u8; 32],
}

/// One frame supplied to the bounded publication-packet encoder.
///
/// Frame and record ordinals are deliberately omitted: they are derived from
/// the packet header and the preceding descriptors, so the wire cannot carry
/// contradictory ordinal copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationPacketFrameInput<'payload> {
    pub record_count: u32,
    pub digest: Digest128,
    pub bytes: &'payload [u8],
}

/// Borrowed input for one canonical FPK3 publication packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationPacketInput<'frames, 'payload> {
    pub offer_id: Id128,
    pub first_frame_ordinal: u32,
    pub first_record_ordinal: u32,
    pub frames: &'frames [PublicationPacketFrameInput<'payload>],
}

/// One decoded frame from an FPK3 publication packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationPacketFrame<'payload> {
    pub ordinal: u32,
    pub first_record_ordinal: u32,
    pub record_count: u32,
    pub digest: Digest128,
    pub bytes: &'payload [u8],
}

/// A bounded, borrowed FPK3 packet carried by the schema-4 event protocol.
///
/// The fixed header is validated in constant time before this value is
/// constructed. [`decode_publication_packet`] additionally validates every
/// descriptor and both aggregate sums. Packet boundaries are intentionally
/// absent from the candidate transport digest; callers digest each yielded
/// frame with its derived ordinals exactly as they did before batching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationPacket<'payload> {
    pub offer_id: Id128,
    pub first_frame_ordinal: u32,
    pub first_record_ordinal: u32,
    pub frame_count: u32,
    pub aggregate_record_count: u32,
    pub aggregate_frame_bytes: u32,
    encoded: &'payload [u8],
    directory: &'payload [u8],
    frame_bytes: &'payload [u8],
}

impl<'payload> PublicationPacket<'payload> {
    /// Exact packet bytes beginning with `offer_id` and ending with the final
    /// frame body. These bytes are suitable for the independent host ABI.
    #[must_use]
    pub const fn encoded(&self) -> &'payload [u8] {
        self.encoded
    }

    /// Fixed-width descriptor table. Hosts may inspect it incrementally after
    /// constant-time envelope admission.
    #[must_use]
    pub const fn directory(&self) -> &'payload [u8] {
        self.directory
    }

    /// Concatenated frame bodies in descriptor order.
    #[must_use]
    pub const fn frame_bytes(&self) -> &'payload [u8] {
        self.frame_bytes
    }

    /// Iterates frames with ordinals derived from the packet header. Packets
    /// returned by [`decode_publication_packet`] are guaranteed not to yield an
    /// error. Envelope-only callers must handle descriptor validation errors.
    #[must_use]
    pub fn frames(&self) -> PublicationPacketFrames<'payload> {
        PublicationPacketFrames {
            packet: *self,
            next_index: 0,
            directory_offset: 0,
            body_offset: 0,
            next_record_ordinal: self.first_record_ordinal,
            finished: false,
        }
    }
}

/// Incremental, allocation-free iterator over a publication packet.
pub struct PublicationPacketFrames<'payload> {
    packet: PublicationPacket<'payload>,
    next_index: u32,
    directory_offset: usize,
    body_offset: usize,
    next_record_ordinal: u32,
    finished: bool,
}

impl<'payload> Iterator for PublicationPacketFrames<'payload> {
    type Item = Result<PublicationPacketFrame<'payload>, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        if self.next_index == self.packet.frame_count {
            self.finished = true;
            if self.body_offset != self.packet.frame_bytes.len()
                || self.next_record_ordinal
                    != self
                        .packet
                        .first_record_ordinal
                        .checked_add(self.packet.aggregate_record_count)
                        .expect("packet envelope checked record aggregate")
            {
                return Some(Err(invalid(
                    PACKET_HEADER_BYTES + self.directory_offset + self.body_offset,
                    None,
                    None,
                )));
            }
            return None;
        }

        let descriptor_end = match self
            .directory_offset
            .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES)
        {
            Some(end) if end <= self.packet.directory.len() => end,
            _ => {
                self.finished = true;
                return Some(Err(publication_error(
                    DecodeFailure::TruncatedPayload,
                    PACKET_HEADER_BYTES + self.directory_offset,
                    None,
                    Some(self.packet.encoded.len()),
                )));
            }
        };
        let descriptor = &self.packet.directory[self.directory_offset..descriptor_end];
        let frame_length =
            u32::from_le_bytes(descriptor[0..4].try_into().expect("four-byte length"));
        if frame_length == 0 {
            self.finished = true;
            return Some(Err(invalid(
                PACKET_HEADER_BYTES + self.directory_offset,
                Some(1),
                Some(0),
            )));
        }
        let record_count =
            u32::from_le_bytes(descriptor[4..8].try_into().expect("four-byte count"));
        let digest = [
            u32::from_le_bytes(descriptor[8..12].try_into().expect("digest lane")),
            u32::from_le_bytes(descriptor[12..16].try_into().expect("digest lane")),
            u32::from_le_bytes(descriptor[16..20].try_into().expect("digest lane")),
            u32::from_le_bytes(descriptor[20..24].try_into().expect("digest lane")),
        ];
        let body_end = match self.body_offset.checked_add(frame_length as usize) {
            Some(end) if end <= self.packet.frame_bytes.len() => end,
            _ => {
                self.finished = true;
                return Some(Err(publication_error(
                    DecodeFailure::TruncatedPayload,
                    PACKET_HEADER_BYTES + self.packet.directory.len() + self.body_offset,
                    None,
                    Some(self.packet.encoded.len()),
                )));
            }
        };
        let ordinal = match self.packet.first_frame_ordinal.checked_add(self.next_index) {
            Some(value) => value,
            None => {
                self.finished = true;
                return Some(Err(invalid(PACKET_HEADER_BYTES, None, None)));
            }
        };
        let next_record_ordinal = match self.next_record_ordinal.checked_add(record_count) {
            Some(value) => value,
            None => {
                self.finished = true;
                return Some(Err(invalid(
                    PACKET_HEADER_BYTES + self.directory_offset + 4,
                    None,
                    None,
                )));
            }
        };
        let frame = PublicationPacketFrame {
            ordinal,
            first_record_ordinal: self.next_record_ordinal,
            record_count,
            digest,
            bytes: &self.packet.frame_bytes[self.body_offset..body_end],
        };
        self.next_index += 1;
        self.directory_offset = descriptor_end;
        self.body_offset = body_end;
        self.next_record_ordinal = next_record_ordinal;
        Some(Ok(frame))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitRequest {
    pub offer_id: Id128,
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub rolling_transport_digest: Digest128,
    pub canonical_stream_digest: Digest128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationEventBody<'payload> {
    Begin(OfferBegin),
    Packet(PublicationPacket<'payload>),
    Commit(CommitRequest),
    AbortRequested { offer_id: Id128 },
    Failed { offer_id: Id128, failure_code: u32 },
    DeliveryAcknowledged(StructuralAck),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationEvent<'payload> {
    pub event_id: u32,
    pub binding: SessionBinding,
    pub body: PublicationEventBody<'payload>,
}

/// Sidecar sibling of [`PublicationEventBody`].
///
/// The variants use the same FLK3 opcodes and FPK3 packet bytes but carry a
/// reserved sidecar payload tag. Structural decoders reject those tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotInlineSidecarEventBody<'payload> {
    Begin(HotInlineSidecarBegin),
    Packet(PublicationPacket<'payload>),
    Commit(HotInlineSidecarCommitRequest),
    AbortRequested { offer_id: Id128 },
    Failed { offer_id: Id128, failure_code: u32 },
    DeliveryAcknowledged(InlineSidecarAck),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotInlineSidecarEvent<'payload> {
    pub event_id: u32,
    pub binding: SessionBinding,
    pub body: HotInlineSidecarEventBody<'payload>,
}

/// Aggregate viewport-presentation sibling of structural and HIO1 events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPresentationEventBody<'payload> {
    Begin(ViewportPresentationBegin),
    Packet(PublicationPacket<'payload>),
    Commit(ViewportPresentationCommitRequest),
    AbortRequested { offer_id: Id128 },
    Failed { offer_id: Id128, failure_code: u32 },
    DeliveryAcknowledged(ViewportPresentationAck),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationEvent<'payload> {
    pub event_id: u32,
    pub binding: SessionBinding,
    pub body: ViewportPresentationEventBody<'payload>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPollPhase {
    PacketCredit,
    Commit,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostPollTicket {
    pub binding: SessionBinding,
    pub poll_ticket: u32,
    pub offer_id: Id128,
    pub phase: HostPollPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPollOutcome {
    PacketCredit {
        offer_id: Id128,
        next_frame_ordinal: u32,
    },
    Committed(StructuralAck),
    AbortComplete {
        offer_id: Id128,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRejectReason {
    Invalid,
    Backpressure,
    StaleSource,
    ExactSourceMismatch,
    SessionSnapshotRequired,
    BaseMismatch,
    WrongOffer,
    CorruptPublication,
    QueryBoundExceeded,
    ForegroundBoundExceeded,
    Superseded,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPollResult {
    Completed(HostPollOutcome),
    Rejected(HostRejectReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedHostPollCommand {
    pub correlation_id: u32,
    pub binding: SessionBinding,
    pub ticket: HostPollTicket,
    pub result: HostPollResult,
}

/// Sidecar phases use distinct wire codes, including for rejected polls, so a
/// sidecar ticket cannot be decoded as a structural ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineSidecarHostPollPhase {
    PacketCredit,
    Commit,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineSidecarHostPollTicket {
    pub binding: SessionBinding,
    pub poll_ticket: u32,
    pub offer_id: Id128,
    pub phase: InlineSidecarHostPollPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineSidecarHostPollOutcome {
    PacketCredit {
        offer_id: Id128,
        next_frame_ordinal: u32,
    },
    Committed(InlineSidecarAck),
    AbortComplete {
        offer_id: Id128,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineSidecarHostPollResult {
    Completed(InlineSidecarHostPollOutcome),
    Rejected(HostRejectReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedInlineSidecarHostPollCommand {
    pub correlation_id: u32,
    pub binding: SessionBinding,
    pub ticket: InlineSidecarHostPollTicket,
    pub result: InlineSidecarHostPollResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPresentationHostPollPhase {
    PacketCredit,
    Commit,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationHostPollTicket {
    pub binding: SessionBinding,
    pub poll_ticket: u32,
    pub offer_id: Id128,
    pub phase: ViewportPresentationHostPollPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPresentationHostPollOutcome {
    PacketCredit {
        offer_id: Id128,
        next_frame_ordinal: u32,
    },
    Committed(ViewportPresentationAck),
    AbortComplete {
        offer_id: Id128,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPresentationHostPollResult {
    Completed(ViewportPresentationHostPollOutcome),
    Rejected(HostRejectReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedViewportPresentationHostPollCommand {
    pub correlation_id: u32,
    pub binding: SessionBinding,
    pub ticket: ViewportPresentationHostPollTicket,
    pub result: ViewportPresentationHostPollResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeFailure {
    Envelope(v3_wire::DecodeFailure),
    UnsupportedSchema,
    UnexpectedOpcode,
    UnknownVariant,
    TruncatedPayload,
    TrailingPayload,
    InvalidValue,
    OversizedValue,
    IdentityMismatch,
    CorrelationMismatch,
    UnmappedStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub failure: DecodeFailure,
    pub byte_offset: usize,
    pub expected: Option<usize>,
    pub actual: Option<usize>,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid Flark v3 publication frame: {:?}",
            self.failure
        )
    }
}

impl std::error::Error for DecodeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    InvalidValue,
    IdentityMismatch,
    PayloadTooLarge,
    Envelope(v3_wire::EncodeError),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot encode Flark v3 publication event: {self:?}"
        )
    }
}

impl std::error::Error for EncodeError {}

/// Encodes one bounded publication packet into caller-owned storage.
///
/// The descriptor table is emitted before all frame bodies. The function does
/// not inspect frame syntax or recompute frame digests; those remain producer
/// and independent-host responsibilities because the digest includes the
/// independently classified frame kind.
pub fn encode_publication_packet_into(
    packet: PublicationPacketInput<'_, '_>,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    let frame_count =
        u32::try_from(packet.frames.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
    if frame_count == 0 {
        return Err(EncodeError::InvalidValue);
    }
    if frame_count > MAXIMUM_PACKET_FRAME_COUNT {
        return Err(EncodeError::PayloadTooLarge);
    }
    packet
        .first_frame_ordinal
        .checked_add(frame_count)
        .ok_or(EncodeError::InvalidValue)?;

    let mut aggregate_record_count = 0_u32;
    let mut aggregate_frame_bytes = 0_u32;
    for frame in packet.frames {
        if frame.bytes.is_empty() {
            return Err(EncodeError::InvalidValue);
        }
        aggregate_record_count = aggregate_record_count
            .checked_add(frame.record_count)
            .ok_or(EncodeError::InvalidValue)?;
        let frame_bytes =
            u32::try_from(frame.bytes.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
        aggregate_frame_bytes = aggregate_frame_bytes
            .checked_add(frame_bytes)
            .ok_or(EncodeError::PayloadTooLarge)?;
    }
    packet
        .first_record_ordinal
        .checked_add(aggregate_record_count)
        .ok_or(EncodeError::InvalidValue)?;
    if aggregate_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES {
        return Err(EncodeError::PayloadTooLarge);
    }

    let directory_bytes = packet
        .frames
        .len()
        .checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES)
        .ok_or(EncodeError::PayloadTooLarge)?;
    let required = PACKET_HEADER_BYTES
        .checked_add(directory_bytes)
        .and_then(|length| length.checked_add(aggregate_frame_bytes as usize))
        .ok_or(EncodeError::PayloadTooLarge)?;
    if required > MAXIMUM_PACKET_ENCODED_BYTES
        || required > v3_wire::MAXIMUM_PAYLOAD_BYTES - PAYLOAD_PREFIX_BYTES
    {
        return Err(EncodeError::PayloadTooLarge);
    }
    if output.len() < required {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            },
        ));
    }

    let mut writer = PayloadWriter::new(&mut output[..required]);
    writer.u32(PACKET_MAGIC);
    writer.u16(PACKET_SCHEMA);
    writer.u16(PACKET_FLAGS);
    writer.id128(packet.offer_id);
    writer.u32(packet.first_frame_ordinal);
    writer.u32(packet.first_record_ordinal);
    writer.u32(frame_count);
    writer.u32(aggregate_record_count);
    writer.u32(aggregate_frame_bytes);
    for frame in packet.frames {
        writer.u32(
            u32::try_from(frame.bytes.len()).expect("validated packet frame length must fit u32"),
        );
        writer.u32(frame.record_count);
        writer.id128(frame.digest);
    }
    for frame in packet.frames {
        writer.raw(frame.bytes);
    }
    debug_assert_eq!(writer.len(), required);
    Ok(required)
}

/// Validates only the fixed packet header and exact outer envelope.
///
/// This constant-time seam is intended for the independent host's admission
/// path. Descriptor sums and bodies must still be inspected incrementally
/// before packet credit is returned.
pub fn decode_publication_packet_envelope(
    bytes: &[u8],
) -> Result<PublicationPacket<'_>, DecodeError> {
    let mut reader = PayloadReader::new(bytes);
    let magic = reader.u32()?;
    if magic != PACKET_MAGIC {
        return Err(invalid(
            0,
            Some(PACKET_MAGIC as usize),
            Some(magic as usize),
        ));
    }
    let schema = reader.u16()?;
    if schema != PACKET_SCHEMA {
        return Err(publication_error(
            DecodeFailure::UnsupportedSchema,
            4,
            Some(PACKET_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let flags = reader.u16()?;
    if flags != PACKET_FLAGS {
        return Err(invalid(
            6,
            Some(PACKET_FLAGS as usize),
            Some(flags as usize),
        ));
    }
    let offer_id = reader.id128()?;
    let first_frame_ordinal = reader.u32()?;
    let first_record_ordinal = reader.u32()?;
    let frame_count = reader.u32()?;
    let aggregate_record_count = reader.u32()?;
    let aggregate_frame_bytes = reader.u32()?;

    if frame_count == 0 {
        return Err(invalid(32, Some(1), Some(0)));
    }
    if frame_count > MAXIMUM_PACKET_FRAME_COUNT {
        return Err(publication_error(
            DecodeFailure::OversizedValue,
            32,
            Some(MAXIMUM_PACKET_FRAME_COUNT as usize),
            Some(frame_count as usize),
        ));
    }
    first_frame_ordinal
        .checked_add(frame_count)
        .ok_or_else(|| invalid(24, None, None))?;
    first_record_ordinal
        .checked_add(aggregate_record_count)
        .ok_or_else(|| invalid(28, None, None))?;
    if aggregate_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES {
        return Err(publication_error(
            DecodeFailure::OversizedValue,
            40,
            Some(MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize),
            Some(aggregate_frame_bytes as usize),
        ));
    }

    let directory_bytes = (frame_count as usize)
        .checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES)
        .ok_or_else(|| invalid(32, None, None))?;
    let directory_end = PACKET_HEADER_BYTES
        .checked_add(directory_bytes)
        .ok_or_else(|| invalid(32, None, None))?;
    let expected = directory_end
        .checked_add(aggregate_frame_bytes as usize)
        .ok_or_else(|| invalid(40, None, None))?;
    if expected > MAXIMUM_PACKET_ENCODED_BYTES {
        return Err(publication_error(
            DecodeFailure::OversizedValue,
            0,
            Some(MAXIMUM_PACKET_ENCODED_BYTES),
            Some(expected),
        ));
    }
    if bytes.len() < expected {
        return Err(publication_error(
            DecodeFailure::TruncatedPayload,
            bytes.len(),
            Some(expected),
            Some(bytes.len()),
        ));
    }
    if bytes.len() > expected {
        return Err(publication_error(
            DecodeFailure::TrailingPayload,
            expected,
            Some(expected),
            Some(bytes.len()),
        ));
    }

    Ok(PublicationPacket {
        offer_id,
        first_frame_ordinal,
        first_record_ordinal,
        frame_count,
        aggregate_record_count,
        aggregate_frame_bytes,
        encoded: bytes,
        directory: &bytes[PACKET_HEADER_BYTES..directory_end],
        frame_bytes: &bytes[directory_end..expected],
    })
}

/// Fully validates one packet and borrows its descriptor and frame storage.
pub fn decode_publication_packet(bytes: &[u8]) -> Result<PublicationPacket<'_>, DecodeError> {
    let packet = decode_publication_packet_envelope(bytes)?;
    for frame in packet.frames() {
        frame?;
    }
    Ok(packet)
}

/// Encodes the canonical VPB1 parent frame from an already validated offer.
pub fn encode_viewport_presentation_parent_frame_into(
    begin: ViewportPresentationBegin,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_viewport_presentation_begin(begin).map_err(|failure| match failure {
        ValidationFailure::Invalid => EncodeError::InvalidValue,
        ValidationFailure::Oversized => EncodeError::PayloadTooLarge,
    })?;
    if VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES > begin.limits.maximum_frame_bytes as usize {
        return Err(EncodeError::PayloadTooLarge);
    }
    if output.len() < VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required: VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES,
                available: output.len(),
            },
        ));
    }
    let mut writer = PayloadWriter::new(&mut output[..VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES]);
    writer.u32(VIEWPORT_PRESENTATION_PARENT_MAGIC);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_SCHEMA);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_FLAGS);
    writer.u32(begin.envelope.ordered_leaf_count);
    write_viewport_presentation_binding(&mut writer, begin.binding);
    write_viewport_presentation_envelope(&mut writer, begin.envelope);
    debug_assert_eq!(writer.len(), VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES);
    Ok(VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES)
}

/// Decodes a canonical VPB1 parent and requires exact equality with Begin.
pub fn decode_viewport_presentation_parent_frame(
    bytes: &[u8],
    expected_begin: ViewportPresentationBegin,
) -> Result<ViewportPresentationParentFrame, DecodeError> {
    validate_viewport_presentation_begin(expected_begin)
        .map_err(|failure| validation_error(failure, 0))?;
    if bytes.len() != VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES {
        return Err(exact_frame_length_error(
            bytes.len(),
            VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES,
        ));
    }
    let mut reader = PayloadReader::new(bytes);
    read_viewport_presentation_frame_header(&mut reader, VIEWPORT_PRESENTATION_PARENT_MAGIC)?;
    let entry_count = reader.u32()?;
    let frame = ViewportPresentationParentFrame {
        binding: read_viewport_presentation_binding(&mut reader)?,
        envelope: read_viewport_presentation_envelope(&mut reader)?,
    };
    reader.finish()?;
    if entry_count != frame.envelope.ordered_leaf_count
        || frame.binding != expected_begin.binding
        || frame.envelope != expected_begin.envelope
    {
        return Err(invalid(8, None, None));
    }
    Ok(frame)
}

/// Encodes one fixed-width, random-access VPB1 leaf directory.
pub fn encode_viewport_presentation_directory_into(
    begin: ViewportPresentationBegin,
    entries: &[ViewportPresentationDirectoryEntry],
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_viewport_presentation_begin(begin).map_err(|failure| match failure {
        ValidationFailure::Invalid => EncodeError::InvalidValue,
        ValidationFailure::Oversized => EncodeError::PayloadTooLarge,
    })?;
    let entry_count = u32::try_from(entries.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
    let required = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
        .checked_add(
            entries
                .len()
                .checked_mul(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
                .ok_or(EncodeError::PayloadTooLarge)?,
        )
        .ok_or(EncodeError::PayloadTooLarge)?;
    if required > begin.limits.maximum_frame_bytes as usize
        || required > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
    {
        return Err(EncodeError::PayloadTooLarge);
    }
    let mut validator = ViewportPresentationDirectoryValidator::new(begin, entry_count)
        .map_err(|_| EncodeError::InvalidValue)?;
    for entry in entries.iter().copied() {
        validator
            .push(entry)
            .map_err(|_| EncodeError::InvalidValue)?;
    }
    validator.finish().map_err(|_| EncodeError::InvalidValue)?;
    if output.len() < required {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            },
        ));
    }

    let mut writer = PayloadWriter::new(&mut output[..required]);
    writer.u32(VIEWPORT_PRESENTATION_DIRECTORY_MAGIC);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_SCHEMA);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_FLAGS);
    writer.u32(entry_count);
    for entry in entries {
        write_viewport_presentation_directory_entry(&mut writer, *entry);
    }
    debug_assert_eq!(writer.len(), required);
    Ok(required)
}

/// Decodes and fully validates one bounded VPB1 directory without allocation.
pub fn decode_viewport_presentation_directory<'payload>(
    bytes: &'payload [u8],
    expected_begin: ViewportPresentationBegin,
) -> Result<ViewportPresentationDirectory<'payload>, DecodeError> {
    validate_viewport_presentation_begin(expected_begin)
        .map_err(|failure| validation_error(failure, 0))?;
    if bytes.len() < VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES {
        return Err(exact_frame_length_error(
            bytes.len(),
            VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES,
        ));
    }
    let mut reader = PayloadReader::new(bytes);
    read_viewport_presentation_frame_header(&mut reader, VIEWPORT_PRESENTATION_DIRECTORY_MAGIC)?;
    let entry_count = reader.u32()?;
    if entry_count != expected_begin.envelope.ordered_leaf_count {
        return Err(invalid(
            8,
            Some(expected_begin.envelope.ordered_leaf_count as usize),
            Some(entry_count as usize),
        ));
    }
    let required = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
        .checked_add(
            usize::try_from(entry_count)
                .map_err(|_| invalid(8, None, Some(entry_count as usize)))?
                .checked_mul(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
                .ok_or_else(|| invalid(8, None, Some(entry_count as usize)))?,
        )
        .ok_or_else(|| invalid(8, None, Some(entry_count as usize)))?;
    if required > expected_begin.limits.maximum_frame_bytes as usize
        || required > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
    {
        return Err(publication_error(
            DecodeFailure::OversizedValue,
            8,
            Some(expected_begin.limits.maximum_frame_bytes as usize),
            Some(required),
        ));
    }
    if bytes.len() != required {
        return Err(exact_frame_length_error(bytes.len(), required));
    }

    let entries_offset = reader.offset;
    let mut validator = ViewportPresentationDirectoryValidator::new(expected_begin, entry_count)
        .map_err(|failure| validation_error(failure, entries_offset))?;
    for _ in 0..entry_count {
        let offset = reader.offset;
        let entry = read_viewport_presentation_directory_entry(&mut reader)?;
        validator
            .push(entry)
            .map_err(|failure| validation_error(failure, offset))?;
    }
    validator
        .finish()
        .map_err(|failure| validation_error(failure, reader.offset))?;
    reader.finish()?;
    Ok(ViewportPresentationDirectory {
        entry_count,
        encoded: bytes,
        entries: &bytes[entries_offset..],
    })
}

/// Encodes one canonical public wrapper around opaque HIO1 frame bytes.
pub fn encode_viewport_presentation_child_frame_into(
    begin: ViewportPresentationBegin,
    frame: ViewportPresentationChildFrameInput<'_>,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_viewport_presentation_begin(begin).map_err(|failure| match failure {
        ValidationFailure::Invalid => EncodeError::InvalidValue,
        ValidationFailure::Oversized => EncodeError::PayloadTooLarge,
    })?;
    validate_viewport_presentation_child(
        begin.envelope.ordered_leaf_count,
        frame.directory_index,
        frame.child_frame_ordinal,
        frame.kind,
        frame.record_count,
        frame.payload,
    )
    .map_err(|_| EncodeError::InvalidValue)?;
    let payload_length =
        u32::try_from(frame.payload.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
    let required = VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES
        .checked_add(frame.payload.len())
        .ok_or(EncodeError::PayloadTooLarge)?;
    if required > begin.limits.maximum_frame_bytes as usize
        || required > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
    {
        return Err(EncodeError::PayloadTooLarge);
    }
    if output.len() < required {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            },
        ));
    }

    let mut writer = PayloadWriter::new(&mut output[..required]);
    writer.u32(VIEWPORT_PRESENTATION_CHILD_MAGIC);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_SCHEMA);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_FLAGS);
    writer.u32(frame.directory_index);
    writer.u32(frame.child_frame_ordinal);
    writer.u16(frame.kind as u16);
    writer.u16(0);
    writer.u32(frame.record_count);
    writer.u32(payload_length);
    writer.raw(frame.payload);
    debug_assert_eq!(writer.len(), required);
    Ok(required)
}

/// Decodes one public child wrapper while leaving its HIO1 body opaque.
pub fn decode_viewport_presentation_child_frame<'payload>(
    bytes: &'payload [u8],
    expected_begin: ViewportPresentationBegin,
) -> Result<ViewportPresentationChildFrame<'payload>, DecodeError> {
    validate_viewport_presentation_begin(expected_begin)
        .map_err(|failure| validation_error(failure, 0))?;
    if bytes.len() < VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES {
        return Err(exact_frame_length_error(
            bytes.len(),
            VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES,
        ));
    }
    let mut reader = PayloadReader::new(bytes);
    read_viewport_presentation_frame_header(&mut reader, VIEWPORT_PRESENTATION_CHILD_MAGIC)?;
    let directory_index = reader.u32()?;
    let child_frame_ordinal = reader.u32()?;
    let kind = match reader.u16()? {
        1 => HotInlineSidecarFrameKind::Begin,
        2 => HotInlineSidecarFrameKind::Node,
        3 => HotInlineSidecarFrameKind::End,
        value => return Err(invalid(reader.offset - 2, Some(3), Some(value as usize))),
    };
    let reserved = reader.u16()?;
    if reserved != 0 {
        return Err(invalid(reader.offset - 2, Some(0), Some(reserved as usize)));
    }
    let record_count = reader.u32()?;
    let payload_length = reader.u32()?;
    let expected_length = VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES
        .checked_add(payload_length as usize)
        .ok_or_else(|| invalid(reader.offset - 4, None, None))?;
    if expected_length != bytes.len() {
        return Err(exact_frame_length_error(bytes.len(), expected_length));
    }
    if expected_length > expected_begin.limits.maximum_frame_bytes as usize
        || expected_length > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
    {
        return Err(publication_error(
            DecodeFailure::OversizedValue,
            reader.offset - 4,
            Some(expected_begin.limits.maximum_frame_bytes as usize),
            Some(expected_length),
        ));
    }
    let payload_offset = reader.offset;
    let payload = reader.remainder();
    validate_viewport_presentation_child(
        expected_begin.envelope.ordered_leaf_count,
        directory_index,
        child_frame_ordinal,
        kind,
        record_count,
        payload,
    )
    .map_err(|failure| validation_error(failure, payload_offset))?;
    Ok(ViewportPresentationChildFrame {
        directory_index,
        child_frame_ordinal,
        kind,
        record_count,
        encoded: bytes,
        payload,
    })
}

/// Encodes one canonical VPB1 End frame.
pub fn encode_viewport_presentation_end_frame_into(
    begin: ViewportPresentationBegin,
    end: ViewportPresentationEndFrame,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_viewport_presentation_end(begin, end).map_err(|failure| match failure {
        ValidationFailure::Invalid => EncodeError::InvalidValue,
        ValidationFailure::Oversized => EncodeError::PayloadTooLarge,
    })?;
    if output.len() < VIEWPORT_PRESENTATION_END_FRAME_BYTES {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required: VIEWPORT_PRESENTATION_END_FRAME_BYTES,
                available: output.len(),
            },
        ));
    }
    let mut writer = PayloadWriter::new(&mut output[..VIEWPORT_PRESENTATION_END_FRAME_BYTES]);
    writer.u32(VIEWPORT_PRESENTATION_END_MAGIC);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_SCHEMA);
    writer.u16(VIEWPORT_PRESENTATION_FRAME_FLAGS);
    writer.u32(end.ordered_leaf_count);
    writer.u32(end.actual_frame_count);
    writer.u32(end.actual_encoded_frame_bytes);
    writer.digest256(end.aggregate_envelope_digest256);
    debug_assert_eq!(writer.len(), VIEWPORT_PRESENTATION_END_FRAME_BYTES);
    Ok(VIEWPORT_PRESENTATION_END_FRAME_BYTES)
}

pub fn decode_viewport_presentation_end_frame(
    bytes: &[u8],
    expected_begin: ViewportPresentationBegin,
) -> Result<ViewportPresentationEndFrame, DecodeError> {
    if bytes.len() != VIEWPORT_PRESENTATION_END_FRAME_BYTES {
        return Err(exact_frame_length_error(
            bytes.len(),
            VIEWPORT_PRESENTATION_END_FRAME_BYTES,
        ));
    }
    let mut reader = PayloadReader::new(bytes);
    read_viewport_presentation_frame_header(&mut reader, VIEWPORT_PRESENTATION_END_MAGIC)?;
    let end = ViewportPresentationEndFrame {
        ordered_leaf_count: reader.u32()?,
        actual_frame_count: reader.u32()?,
        actual_encoded_frame_bytes: reader.u32()?,
        aggregate_envelope_digest256: reader.digest256()?,
    };
    reader.finish()?;
    validate_viewport_presentation_end(expected_begin, end)
        .map_err(|failure| validation_error(failure, 8))?;
    Ok(end)
}

/// Encodes one worker-to-Dart publication event into caller-owned storage.
///
/// Packet bytes are copied exactly once into the final transport frame. The
/// implementation does not allocate a document- or packet-sized staging buffer.
pub fn encode_event_into(
    event: PublicationEvent<'_>,
    expected_binding: SessionBinding,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_binding_for_encode(event.binding)?;
    validate_binding_for_encode(expected_binding)?;
    if event.event_id == 0 {
        return Err(EncodeError::InvalidValue);
    }
    if event.binding != expected_binding {
        return Err(EncodeError::IdentityMismatch);
    }
    validate_event_body(event.binding, event.body)?;

    let (opcode, variant, body_length) = match event.body {
        PublicationEventBody::Begin(begin) => (
            Opcode::PublishBegin,
            0,
            BEGIN_BYTES_WITHOUT_BASE + usize::from(begin.base_ack.is_some()) * STRUCTURAL_ACK_BYTES,
        ),
        PublicationEventBody::Packet(packet) => (Opcode::PublishPacket, 0, packet.encoded().len()),
        PublicationEventBody::Commit(_) => (Opcode::PublishCommit, 0, 56),
        PublicationEventBody::AbortRequested { .. } => (Opcode::PublishAbort, 0, 16),
        PublicationEventBody::Failed { .. } => (Opcode::PublishAbort, 1, 20),
        PublicationEventBody::DeliveryAcknowledged(_) => {
            (Opcode::AcknowledgeDelivery, 0, STRUCTURAL_ACK_BYTES)
        }
    };
    let payload_length = PAYLOAD_PREFIX_BYTES
        .checked_add(body_length)
        .ok_or(EncodeError::PayloadTooLarge)?;
    if payload_length > v3_wire::MAXIMUM_PAYLOAD_BYTES {
        return Err(EncodeError::PayloadTooLarge);
    }
    let required = v3_wire::HEADER_BYTES + payload_length;
    if output.len() < required {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            },
        ));
    }

    v3_wire::encode_into(
        FrameKind::Request,
        Header {
            opcode,
            status: Status::Ok,
            flags: 0,
            correlation_id: event.event_id,
        },
        &[],
        output,
    )
    .map_err(EncodeError::Envelope)?;
    output[20..24].copy_from_slice(&(payload_length as u32).to_le_bytes());
    let mut writer = PayloadWriter::new(&mut output[v3_wire::HEADER_BYTES..required]);
    write_payload_header(&mut writer, variant, event.binding);
    match event.body {
        PublicationEventBody::Begin(begin) => write_begin(&mut writer, begin),
        PublicationEventBody::Packet(packet) => writer.raw(packet.encoded()),
        PublicationEventBody::Commit(commit) => write_commit(&mut writer, commit),
        PublicationEventBody::AbortRequested { offer_id } => writer.id128(offer_id),
        PublicationEventBody::Failed {
            offer_id,
            failure_code,
        } => {
            writer.id128(offer_id);
            writer.u32(failure_code);
        }
        PublicationEventBody::DeliveryAcknowledged(ack) => write_ack(&mut writer, ack),
    }
    debug_assert_eq!(writer.len(), payload_length);
    Ok(required)
}

/// Decodes one Dart-facing publication event without copying its packet body.
pub fn decode_event<'buffer>(
    bytes: &'buffer [u8],
    expected_binding: SessionBinding,
) -> Result<PublicationEvent<'buffer>, DecodeError> {
    validate_expected_binding(expected_binding)?;
    let frame = decode_envelope(bytes, FrameKind::Request)?;
    let mut reader = PayloadReader::new(frame.payload);
    let header = read_payload_header(&mut reader)?;
    if frame.header.correlation_id == 0 {
        return Err(invalid(16, Some(1), Some(0)));
    }
    require_binding(header.binding, expected_binding, reader.offset)?;

    let body = match (frame.header.opcode, header.variant) {
        (Opcode::PublishBegin, 0) => PublicationEventBody::Begin(read_begin(&mut reader)?),
        (Opcode::PublishPacket, 0) => PublicationEventBody::Packet(read_packet(&mut reader)?),
        (Opcode::PublishCommit, 0) => PublicationEventBody::Commit(read_commit(&mut reader)?),
        (Opcode::PublishAbort, 0) => PublicationEventBody::AbortRequested {
            offer_id: reader.id128()?,
        },
        (Opcode::PublishAbort, 1) => PublicationEventBody::Failed {
            offer_id: reader.id128()?,
            failure_code: reader.u32()?,
        },
        (Opcode::AcknowledgeDelivery, 0) => {
            PublicationEventBody::DeliveryAcknowledged(read_ack(&mut reader)?)
        }
        (
            Opcode::PublishBegin
            | Opcode::PublishPacket
            | Opcode::PublishCommit
            | Opcode::PublishAbort
            | Opcode::AcknowledgeDelivery,
            variant,
        ) => return Err(unknown_variant(variant)),
        (opcode, _) => {
            return Err(publication_error(
                DecodeFailure::UnexpectedOpcode,
                8,
                None,
                Some(opcode.code() as usize),
            ));
        }
    };
    validate_event_body_decoded(header.binding, body, reader.offset)?;
    reader.finish()?;
    Ok(PublicationEvent {
        event_id: frame.header.correlation_id,
        binding: header.binding,
        body,
    })
}

/// Encodes one hot-inline sidecar event using the existing publication
/// opcodes and FPK3 packet codec with a disjoint payload tag.
pub fn encode_hot_inline_sidecar_event_into(
    event: HotInlineSidecarEvent<'_>,
    expected_binding: SessionBinding,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_binding_for_encode(event.binding)?;
    validate_binding_for_encode(expected_binding)?;
    if event.event_id == 0 {
        return Err(EncodeError::InvalidValue);
    }
    if event.binding != expected_binding {
        return Err(EncodeError::IdentityMismatch);
    }
    validate_hot_inline_sidecar_event_body(event.binding, event.body)?;

    let (opcode, variant, body_length) = match event.body {
        HotInlineSidecarEventBody::Begin(_) => (
            Opcode::PublishBegin,
            HOT_INLINE_SIDECAR_VARIANT,
            HOT_INLINE_SIDECAR_BEGIN_BYTES,
        ),
        HotInlineSidecarEventBody::Packet(packet) => (
            Opcode::PublishPacket,
            HOT_INLINE_SIDECAR_VARIANT,
            packet.encoded().len(),
        ),
        HotInlineSidecarEventBody::Commit(_) => {
            (Opcode::PublishCommit, HOT_INLINE_SIDECAR_VARIANT, 56)
        }
        HotInlineSidecarEventBody::AbortRequested { .. } => {
            (Opcode::PublishAbort, HOT_INLINE_SIDECAR_VARIANT, 16)
        }
        HotInlineSidecarEventBody::Failed { .. } => {
            (Opcode::PublishAbort, HOT_INLINE_SIDECAR_FAILED_VARIANT, 20)
        }
        HotInlineSidecarEventBody::DeliveryAcknowledged(_) => (
            Opcode::AcknowledgeDelivery,
            HOT_INLINE_SIDECAR_VARIANT,
            INLINE_SIDECAR_ACK_BYTES,
        ),
    };
    let payload_length = PAYLOAD_PREFIX_BYTES
        .checked_add(body_length)
        .ok_or(EncodeError::PayloadTooLarge)?;
    if payload_length > v3_wire::MAXIMUM_PAYLOAD_BYTES {
        return Err(EncodeError::PayloadTooLarge);
    }
    let required = v3_wire::HEADER_BYTES + payload_length;
    if output.len() < required {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            },
        ));
    }

    v3_wire::encode_into(
        FrameKind::Request,
        Header {
            opcode,
            status: Status::Ok,
            flags: 0,
            correlation_id: event.event_id,
        },
        &[],
        output,
    )
    .map_err(EncodeError::Envelope)?;
    output[20..24].copy_from_slice(&(payload_length as u32).to_le_bytes());
    let mut writer = PayloadWriter::new(&mut output[v3_wire::HEADER_BYTES..required]);
    write_payload_header(&mut writer, variant, event.binding);
    match event.body {
        HotInlineSidecarEventBody::Begin(begin) => {
            write_hot_inline_sidecar_begin(&mut writer, begin)
        }
        HotInlineSidecarEventBody::Packet(packet) => writer.raw(packet.encoded()),
        HotInlineSidecarEventBody::Commit(commit) => {
            write_hot_inline_sidecar_commit(&mut writer, commit)
        }
        HotInlineSidecarEventBody::AbortRequested { offer_id } => writer.id128(offer_id),
        HotInlineSidecarEventBody::Failed {
            offer_id,
            failure_code,
        } => {
            writer.id128(offer_id);
            writer.u32(failure_code);
        }
        HotInlineSidecarEventBody::DeliveryAcknowledged(ack) => {
            write_inline_sidecar_ack(&mut writer, ack)
        }
    }
    debug_assert_eq!(writer.len(), payload_length);
    Ok(required)
}

/// Decodes one hot-inline sidecar event. Structural publication events are
/// rejected even when they use the same FLK3 opcode.
pub fn decode_hot_inline_sidecar_event<'buffer>(
    bytes: &'buffer [u8],
    expected_binding: SessionBinding,
) -> Result<HotInlineSidecarEvent<'buffer>, DecodeError> {
    validate_expected_binding(expected_binding)?;
    let frame = decode_envelope(bytes, FrameKind::Request)?;
    let mut reader = PayloadReader::new(frame.payload);
    let header = read_payload_header(&mut reader)?;
    if frame.header.correlation_id == 0 {
        return Err(invalid(16, Some(1), Some(0)));
    }
    require_binding(header.binding, expected_binding, reader.offset)?;

    let body = match (frame.header.opcode, header.variant) {
        (Opcode::PublishBegin, HOT_INLINE_SIDECAR_VARIANT) => {
            HotInlineSidecarEventBody::Begin(read_hot_inline_sidecar_begin(&mut reader)?)
        }
        (Opcode::PublishPacket, HOT_INLINE_SIDECAR_VARIANT) => {
            HotInlineSidecarEventBody::Packet(read_packet(&mut reader)?)
        }
        (Opcode::PublishCommit, HOT_INLINE_SIDECAR_VARIANT) => {
            HotInlineSidecarEventBody::Commit(read_hot_inline_sidecar_commit(&mut reader)?)
        }
        (Opcode::PublishAbort, HOT_INLINE_SIDECAR_VARIANT) => {
            HotInlineSidecarEventBody::AbortRequested {
                offer_id: reader.id128()?,
            }
        }
        (Opcode::PublishAbort, HOT_INLINE_SIDECAR_FAILED_VARIANT) => {
            HotInlineSidecarEventBody::Failed {
                offer_id: reader.id128()?,
                failure_code: reader.u32()?,
            }
        }
        (Opcode::AcknowledgeDelivery, HOT_INLINE_SIDECAR_VARIANT) => {
            HotInlineSidecarEventBody::DeliveryAcknowledged(read_inline_sidecar_ack(&mut reader)?)
        }
        (
            Opcode::PublishBegin
            | Opcode::PublishPacket
            | Opcode::PublishCommit
            | Opcode::PublishAbort
            | Opcode::AcknowledgeDelivery,
            variant,
        ) => return Err(unknown_variant(variant)),
        (opcode, _) => {
            return Err(publication_error(
                DecodeFailure::UnexpectedOpcode,
                8,
                None,
                Some(opcode.code() as usize),
            ));
        }
    };
    validate_hot_inline_sidecar_event_body_decoded(header.binding, body, reader.offset)?;
    reader.finish()?;
    Ok(HotInlineSidecarEvent {
        event_id: frame.header.correlation_id,
        binding: header.binding,
        body,
    })
}

/// Encodes one aggregate viewport-presentation event with a disjoint payload
/// family while retaining the bounded FPK3 packet envelope.
pub fn encode_viewport_presentation_event_into(
    event: ViewportPresentationEvent<'_>,
    expected_binding: SessionBinding,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_binding_for_encode(event.binding)?;
    validate_binding_for_encode(expected_binding)?;
    if event.event_id == 0 {
        return Err(EncodeError::InvalidValue);
    }
    if event.binding != expected_binding {
        return Err(EncodeError::IdentityMismatch);
    }
    validate_viewport_presentation_event_body(event.binding, event.body)?;

    let (opcode, variant, body_length) = match event.body {
        ViewportPresentationEventBody::Begin(_) => (
            Opcode::PublishBegin,
            VIEWPORT_PRESENTATION_VARIANT,
            VIEWPORT_PRESENTATION_BEGIN_BYTES,
        ),
        ViewportPresentationEventBody::Packet(packet) => (
            Opcode::PublishPacket,
            VIEWPORT_PRESENTATION_VARIANT,
            packet.encoded().len(),
        ),
        ViewportPresentationEventBody::Commit(_) => {
            (Opcode::PublishCommit, VIEWPORT_PRESENTATION_VARIANT, 56)
        }
        ViewportPresentationEventBody::AbortRequested { .. } => {
            (Opcode::PublishAbort, VIEWPORT_PRESENTATION_VARIANT, 16)
        }
        ViewportPresentationEventBody::Failed { .. } => (
            Opcode::PublishAbort,
            VIEWPORT_PRESENTATION_FAILED_VARIANT,
            20,
        ),
        ViewportPresentationEventBody::DeliveryAcknowledged(_) => (
            Opcode::AcknowledgeDelivery,
            VIEWPORT_PRESENTATION_VARIANT,
            VIEWPORT_PRESENTATION_ACK_BYTES,
        ),
    };
    let payload_length = PAYLOAD_PREFIX_BYTES
        .checked_add(body_length)
        .ok_or(EncodeError::PayloadTooLarge)?;
    if payload_length > v3_wire::MAXIMUM_PAYLOAD_BYTES {
        return Err(EncodeError::PayloadTooLarge);
    }
    let required = v3_wire::HEADER_BYTES + payload_length;
    if output.len() < required {
        return Err(EncodeError::Envelope(
            v3_wire::EncodeError::BufferTooSmall {
                required,
                available: output.len(),
            },
        ));
    }

    v3_wire::encode_into(
        FrameKind::Request,
        Header {
            opcode,
            status: Status::Ok,
            flags: 0,
            correlation_id: event.event_id,
        },
        &[],
        output,
    )
    .map_err(EncodeError::Envelope)?;
    output[20..24].copy_from_slice(&(payload_length as u32).to_le_bytes());
    let mut writer = PayloadWriter::new(&mut output[v3_wire::HEADER_BYTES..required]);
    write_payload_header(&mut writer, variant, event.binding);
    match event.body {
        ViewportPresentationEventBody::Begin(begin) => {
            write_viewport_presentation_begin(&mut writer, begin)
        }
        ViewportPresentationEventBody::Packet(packet) => writer.raw(packet.encoded()),
        ViewportPresentationEventBody::Commit(commit) => {
            write_viewport_presentation_commit(&mut writer, commit)
        }
        ViewportPresentationEventBody::AbortRequested { offer_id } => writer.id128(offer_id),
        ViewportPresentationEventBody::Failed {
            offer_id,
            failure_code,
        } => {
            writer.id128(offer_id);
            writer.u32(failure_code);
        }
        ViewportPresentationEventBody::DeliveryAcknowledged(ack) => {
            write_viewport_presentation_ack(&mut writer, ack)
        }
    }
    debug_assert_eq!(writer.len(), payload_length);
    Ok(required)
}

/// Decodes one VPB1 event. Structural and HIO1 variants fail closed.
pub fn decode_viewport_presentation_event<'buffer>(
    bytes: &'buffer [u8],
    expected_binding: SessionBinding,
) -> Result<ViewportPresentationEvent<'buffer>, DecodeError> {
    validate_expected_binding(expected_binding)?;
    let frame = decode_envelope(bytes, FrameKind::Request)?;
    let mut reader = PayloadReader::new(frame.payload);
    let header = read_payload_header(&mut reader)?;
    if frame.header.correlation_id == 0 {
        return Err(invalid(16, Some(1), Some(0)));
    }
    require_binding(header.binding, expected_binding, reader.offset)?;

    let body = match (frame.header.opcode, header.variant) {
        (Opcode::PublishBegin, VIEWPORT_PRESENTATION_VARIANT) => {
            ViewportPresentationEventBody::Begin(read_viewport_presentation_begin(&mut reader)?)
        }
        (Opcode::PublishPacket, VIEWPORT_PRESENTATION_VARIANT) => {
            ViewportPresentationEventBody::Packet(read_packet(&mut reader)?)
        }
        (Opcode::PublishCommit, VIEWPORT_PRESENTATION_VARIANT) => {
            ViewportPresentationEventBody::Commit(read_viewport_presentation_commit(&mut reader)?)
        }
        (Opcode::PublishAbort, VIEWPORT_PRESENTATION_VARIANT) => {
            ViewportPresentationEventBody::AbortRequested {
                offer_id: reader.id128()?,
            }
        }
        (Opcode::PublishAbort, VIEWPORT_PRESENTATION_FAILED_VARIANT) => {
            ViewportPresentationEventBody::Failed {
                offer_id: reader.id128()?,
                failure_code: reader.u32()?,
            }
        }
        (Opcode::AcknowledgeDelivery, VIEWPORT_PRESENTATION_VARIANT) => {
            ViewportPresentationEventBody::DeliveryAcknowledged(read_viewport_presentation_ack(
                &mut reader,
            )?)
        }
        (
            Opcode::PublishBegin
            | Opcode::PublishPacket
            | Opcode::PublishCommit
            | Opcode::PublishAbort
            | Opcode::AcknowledgeDelivery,
            variant,
        ) => return Err(unknown_variant(variant)),
        (opcode, _) => {
            return Err(publication_error(
                DecodeFailure::UnexpectedOpcode,
                8,
                None,
                Some(opcode.code() as usize),
            ));
        }
    };
    validate_viewport_presentation_event_body_decoded(header.binding, body, reader.offset)?;
    reader.finish()?;
    Ok(ViewportPresentationEvent {
        event_id: frame.header.correlation_id,
        binding: header.binding,
        body,
    })
}

/// Decodes a terminal Dart host-poll response and its exact causal ticket.
///
/// Parser event receipts are intentionally rejected here; their sole decoder
/// is [`crate::v3_session_wire::decode_command`].
pub fn decode_host_poll_command(
    bytes: &[u8],
    expected_binding: SessionBinding,
) -> Result<DecodedHostPollCommand, DecodeError> {
    validate_expected_binding(expected_binding)?;
    let frame = decode_envelope(bytes, FrameKind::Response)?;
    if frame.header.correlation_id == 0 {
        return Err(invalid(16, Some(1), Some(0)));
    }
    let mut reader = PayloadReader::new(frame.payload);
    let header = read_payload_header(&mut reader)?;
    require_binding(header.binding, expected_binding, reader.offset)?;
    if frame.header.opcode != Opcode::HostPoll {
        return Err(publication_error(
            DecodeFailure::UnexpectedOpcode,
            8,
            Some(Opcode::HostPoll.code() as usize),
            Some(frame.header.opcode.code() as usize),
        ));
    }
    let ticket = read_poll_ticket(&mut reader, header.binding)?;
    if ticket.poll_ticket != frame.header.correlation_id {
        return Err(publication_error(
            DecodeFailure::CorrelationMismatch,
            PAYLOAD_PREFIX_BYTES,
            Some(ticket.poll_ticket as usize),
            Some(frame.header.correlation_id as usize),
        ));
    }

    let result = if frame.header.status == Status::Ok {
        let outcome = match header.variant {
            1 => HostPollOutcome::PacketCredit {
                offer_id: reader.id128()?,
                next_frame_ordinal: reader.u32()?,
            },
            2 => HostPollOutcome::Committed(read_ack(&mut reader)?),
            3 => HostPollOutcome::AbortComplete {
                offer_id: reader.id128()?,
            },
            variant => return Err(unknown_variant(variant)),
        };
        validate_outcome(ticket, outcome, reader.offset)?;
        HostPollResult::Completed(outcome)
    } else {
        if header.variant != 0 {
            return Err(unknown_variant(header.variant));
        }
        HostPollResult::Rejected(reject_reason(frame.header.status)?)
    };
    reader.finish()?;
    Ok(DecodedHostPollCommand {
        correlation_id: frame.header.correlation_id,
        binding: header.binding,
        ticket,
        result,
    })
}

/// Decodes a terminal host-poll response for the sibling inline sidecar flow.
///
/// Its ticket phases and successful outcome tags are disjoint from the
/// structural host-poll protocol, including on rejection paths.
pub fn decode_inline_sidecar_host_poll_command(
    bytes: &[u8],
    expected_binding: SessionBinding,
) -> Result<DecodedInlineSidecarHostPollCommand, DecodeError> {
    validate_expected_binding(expected_binding)?;
    let frame = decode_envelope(bytes, FrameKind::Response)?;
    if frame.header.correlation_id == 0 {
        return Err(invalid(16, Some(1), Some(0)));
    }
    let mut reader = PayloadReader::new(frame.payload);
    let header = read_payload_header(&mut reader)?;
    require_binding(header.binding, expected_binding, reader.offset)?;
    if frame.header.opcode != Opcode::HostPoll {
        return Err(publication_error(
            DecodeFailure::UnexpectedOpcode,
            8,
            Some(Opcode::HostPoll.code() as usize),
            Some(frame.header.opcode.code() as usize),
        ));
    }
    let ticket = read_inline_sidecar_poll_ticket(&mut reader, header.binding)?;
    if ticket.poll_ticket != frame.header.correlation_id {
        return Err(publication_error(
            DecodeFailure::CorrelationMismatch,
            PAYLOAD_PREFIX_BYTES,
            Some(ticket.poll_ticket as usize),
            Some(frame.header.correlation_id as usize),
        ));
    }

    let result = if frame.header.status == Status::Ok {
        let outcome = match header.variant {
            HOT_INLINE_SIDECAR_PACKET_CREDIT_VARIANT => {
                InlineSidecarHostPollOutcome::PacketCredit {
                    offer_id: reader.id128()?,
                    next_frame_ordinal: reader.u32()?,
                }
            }
            HOT_INLINE_SIDECAR_COMMITTED_VARIANT => {
                InlineSidecarHostPollOutcome::Committed(read_inline_sidecar_ack(&mut reader)?)
            }
            HOT_INLINE_SIDECAR_ABORT_COMPLETE_VARIANT => {
                InlineSidecarHostPollOutcome::AbortComplete {
                    offer_id: reader.id128()?,
                }
            }
            variant => return Err(unknown_variant(variant)),
        };
        validate_inline_sidecar_outcome(ticket, outcome, reader.offset)?;
        InlineSidecarHostPollResult::Completed(outcome)
    } else {
        if header.variant != HOT_INLINE_SIDECAR_VARIANT {
            return Err(unknown_variant(header.variant));
        }
        InlineSidecarHostPollResult::Rejected(reject_reason(frame.header.status)?)
    };
    reader.finish()?;
    Ok(DecodedInlineSidecarHostPollCommand {
        correlation_id: frame.header.correlation_id,
        binding: header.binding,
        ticket,
        result,
    })
}

/// Decodes a terminal host-poll response for aggregate viewport publication.
///
/// Ticket phase codes and successful outcome variants are disjoint from both
/// structural and HIO1 poll families, including rejection paths.
pub fn decode_viewport_presentation_host_poll_command(
    bytes: &[u8],
    expected_binding: SessionBinding,
) -> Result<DecodedViewportPresentationHostPollCommand, DecodeError> {
    validate_expected_binding(expected_binding)?;
    let frame = decode_envelope(bytes, FrameKind::Response)?;
    if frame.header.correlation_id == 0 {
        return Err(invalid(16, Some(1), Some(0)));
    }
    let mut reader = PayloadReader::new(frame.payload);
    let header = read_payload_header(&mut reader)?;
    require_binding(header.binding, expected_binding, reader.offset)?;
    if frame.header.opcode != Opcode::HostPoll {
        return Err(publication_error(
            DecodeFailure::UnexpectedOpcode,
            8,
            Some(Opcode::HostPoll.code() as usize),
            Some(frame.header.opcode.code() as usize),
        ));
    }
    let ticket = read_viewport_presentation_poll_ticket(&mut reader, header.binding)?;
    if ticket.poll_ticket != frame.header.correlation_id {
        return Err(publication_error(
            DecodeFailure::CorrelationMismatch,
            PAYLOAD_PREFIX_BYTES,
            Some(ticket.poll_ticket as usize),
            Some(frame.header.correlation_id as usize),
        ));
    }

    let result = if frame.header.status == Status::Ok {
        let outcome = match header.variant {
            VIEWPORT_PRESENTATION_PACKET_CREDIT_VARIANT => {
                ViewportPresentationHostPollOutcome::PacketCredit {
                    offer_id: reader.id128()?,
                    next_frame_ordinal: reader.u32()?,
                }
            }
            VIEWPORT_PRESENTATION_COMMITTED_VARIANT => {
                ViewportPresentationHostPollOutcome::Committed(read_viewport_presentation_ack(
                    &mut reader,
                )?)
            }
            VIEWPORT_PRESENTATION_ABORT_COMPLETE_VARIANT => {
                ViewportPresentationHostPollOutcome::AbortComplete {
                    offer_id: reader.id128()?,
                }
            }
            variant => return Err(unknown_variant(variant)),
        };
        validate_viewport_presentation_outcome(ticket, outcome, reader.offset)?;
        ViewportPresentationHostPollResult::Completed(outcome)
    } else {
        if header.variant != VIEWPORT_PRESENTATION_VARIANT {
            return Err(unknown_variant(header.variant));
        }
        ViewportPresentationHostPollResult::Rejected(reject_reason(frame.header.status)?)
    };
    reader.finish()?;
    Ok(DecodedViewportPresentationHostPollCommand {
        correlation_id: frame.header.correlation_id,
        binding: header.binding,
        ticket,
        result,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PayloadHeader {
    variant: u16,
    binding: SessionBinding,
}

fn decode_envelope(
    bytes: &[u8],
    kind: FrameKind,
) -> Result<v3_wire::DecodedFrame<'_>, DecodeError> {
    v3_wire::decode(bytes, kind, DecodeLimits::default()).map_err(|error| DecodeError {
        failure: DecodeFailure::Envelope(error.failure),
        byte_offset: error.byte_offset,
        expected: error.expected,
        actual: error.actual,
    })
}

fn validate_expected_binding(binding: SessionBinding) -> Result<(), DecodeError> {
    if binding_is_canonical(binding) {
        Ok(())
    } else {
        Err(invalid(0, None, None))
    }
}

fn validate_binding_for_encode(binding: SessionBinding) -> Result<(), EncodeError> {
    if binding_is_canonical(binding) {
        Ok(())
    } else {
        Err(EncodeError::InvalidValue)
    }
}

fn binding_is_canonical(binding: SessionBinding) -> bool {
    binding.source_session_identity != 0
        && binding.worker_generation != 0
        && binding.document_session != [0; 4]
}

fn require_binding(
    actual: SessionBinding,
    expected: SessionBinding,
    offset: usize,
) -> Result<(), DecodeError> {
    if actual == expected {
        Ok(())
    } else {
        Err(publication_error(
            DecodeFailure::IdentityMismatch,
            offset,
            None,
            None,
        ))
    }
}

fn validate_event_body(
    binding: SessionBinding,
    body: PublicationEventBody<'_>,
) -> Result<(), EncodeError> {
    match body {
        PublicationEventBody::Begin(begin) => {
            if begin.source_version.document_session != binding.document_session {
                return Err(EncodeError::IdentityMismatch);
            }
            validate_begin(begin).map_err(|_| EncodeError::InvalidValue)
        }
        PublicationEventBody::Packet(packet) => decode_publication_packet(packet.encoded())
            .map(|_| ())
            .map_err(|error| match error.failure {
                DecodeFailure::OversizedValue => EncodeError::PayloadTooLarge,
                _ => EncodeError::InvalidValue,
            }),
        PublicationEventBody::Commit(_) => Ok(()),
        PublicationEventBody::AbortRequested { .. } | PublicationEventBody::Failed { .. } => Ok(()),
        PublicationEventBody::DeliveryAcknowledged(ack) => {
            if ack.source_version.document_session != binding.document_session {
                return Err(EncodeError::IdentityMismatch);
            }
            validate_ack(ack).map_err(|_| EncodeError::InvalidValue)
        }
    }
}

fn validate_event_body_decoded(
    binding: SessionBinding,
    body: PublicationEventBody<'_>,
    offset: usize,
) -> Result<(), DecodeError> {
    match body {
        PublicationEventBody::Begin(begin) => {
            if begin.source_version.document_session != binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_begin(begin).map_err(|failure| validation_error(failure, offset))
        }
        // `read_packet` performs full descriptor validation before this seam.
        PublicationEventBody::Packet(_) => Ok(()),
        PublicationEventBody::Commit(_) => Ok(()),
        PublicationEventBody::AbortRequested { .. } | PublicationEventBody::Failed { .. } => Ok(()),
        PublicationEventBody::DeliveryAcknowledged(ack) => {
            if ack.source_version.document_session != binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_ack(ack).map_err(|failure| validation_error(failure, offset))
        }
    }
}

fn validate_hot_inline_sidecar_event_body(
    binding: SessionBinding,
    body: HotInlineSidecarEventBody<'_>,
) -> Result<(), EncodeError> {
    match body {
        HotInlineSidecarEventBody::Begin(begin) => {
            if begin.base_ack.source_version.document_session != binding.document_session {
                return Err(EncodeError::IdentityMismatch);
            }
            validate_hot_inline_sidecar_begin(begin).map_err(|failure| match failure {
                ValidationFailure::Invalid => EncodeError::InvalidValue,
                ValidationFailure::Oversized => EncodeError::PayloadTooLarge,
            })
        }
        HotInlineSidecarEventBody::Packet(packet) => decode_publication_packet(packet.encoded())
            .map(|_| ())
            .map_err(|error| match error.failure {
                DecodeFailure::OversizedValue => EncodeError::PayloadTooLarge,
                _ => EncodeError::InvalidValue,
            }),
        HotInlineSidecarEventBody::Commit(commit) => {
            validate_hot_inline_sidecar_commit(commit).map_err(|_| EncodeError::InvalidValue)
        }
        HotInlineSidecarEventBody::AbortRequested { offer_id }
        | HotInlineSidecarEventBody::Failed { offer_id, .. } => {
            if offer_id == [0; 4] {
                Err(EncodeError::InvalidValue)
            } else {
                Ok(())
            }
        }
        HotInlineSidecarEventBody::DeliveryAcknowledged(ack) => {
            if ack.base_ack.source_version.document_session != binding.document_session {
                return Err(EncodeError::IdentityMismatch);
            }
            validate_inline_sidecar_ack(ack).map_err(|_| EncodeError::InvalidValue)
        }
    }
}

fn validate_hot_inline_sidecar_event_body_decoded(
    binding: SessionBinding,
    body: HotInlineSidecarEventBody<'_>,
    offset: usize,
) -> Result<(), DecodeError> {
    match body {
        HotInlineSidecarEventBody::Begin(begin) => {
            if begin.base_ack.source_version.document_session != binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_hot_inline_sidecar_begin(begin)
                .map_err(|failure| validation_error(failure, offset))
        }
        HotInlineSidecarEventBody::Packet(_) => Ok(()),
        HotInlineSidecarEventBody::Commit(commit) => validate_hot_inline_sidecar_commit(commit)
            .map_err(|failure| validation_error(failure, offset)),
        HotInlineSidecarEventBody::AbortRequested { offer_id }
        | HotInlineSidecarEventBody::Failed { offer_id, .. } => {
            if offer_id == [0; 4] {
                Err(invalid(offset, None, None))
            } else {
                Ok(())
            }
        }
        HotInlineSidecarEventBody::DeliveryAcknowledged(ack) => {
            if ack.base_ack.source_version.document_session != binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_inline_sidecar_ack(ack).map_err(|failure| validation_error(failure, offset))
        }
    }
}

fn validate_viewport_presentation_event_body(
    binding: SessionBinding,
    body: ViewportPresentationEventBody<'_>,
) -> Result<(), EncodeError> {
    match body {
        ViewportPresentationEventBody::Begin(begin) => {
            if begin.base_ack.source_version.document_session != binding.document_session {
                return Err(EncodeError::IdentityMismatch);
            }
            validate_viewport_presentation_begin(begin).map_err(|failure| match failure {
                ValidationFailure::Invalid => EncodeError::InvalidValue,
                ValidationFailure::Oversized => EncodeError::PayloadTooLarge,
            })
        }
        ViewportPresentationEventBody::Packet(packet) => {
            decode_publication_packet(packet.encoded())
                .map(|_| ())
                .map_err(|error| match error.failure {
                    DecodeFailure::OversizedValue => EncodeError::PayloadTooLarge,
                    _ => EncodeError::InvalidValue,
                })
        }
        ViewportPresentationEventBody::Commit(commit) => {
            validate_viewport_presentation_commit(commit).map_err(|_| EncodeError::InvalidValue)
        }
        ViewportPresentationEventBody::AbortRequested { offer_id }
        | ViewportPresentationEventBody::Failed { offer_id, .. } => {
            if offer_id == [0; 4] {
                Err(EncodeError::InvalidValue)
            } else {
                Ok(())
            }
        }
        ViewportPresentationEventBody::DeliveryAcknowledged(ack) => {
            if ack.base_ack.source_version.document_session != binding.document_session {
                return Err(EncodeError::IdentityMismatch);
            }
            validate_viewport_presentation_ack(ack).map_err(|_| EncodeError::InvalidValue)
        }
    }
}

fn validate_viewport_presentation_event_body_decoded(
    binding: SessionBinding,
    body: ViewportPresentationEventBody<'_>,
    offset: usize,
) -> Result<(), DecodeError> {
    match body {
        ViewportPresentationEventBody::Begin(begin) => {
            if begin.base_ack.source_version.document_session != binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_viewport_presentation_begin(begin)
                .map_err(|failure| validation_error(failure, offset))
        }
        ViewportPresentationEventBody::Packet(_) => Ok(()),
        ViewportPresentationEventBody::Commit(commit) => {
            validate_viewport_presentation_commit(commit)
                .map_err(|failure| validation_error(failure, offset))
        }
        ViewportPresentationEventBody::AbortRequested { offer_id }
        | ViewportPresentationEventBody::Failed { offer_id, .. } => {
            if offer_id == [0; 4] {
                Err(invalid(offset, None, None))
            } else {
                Ok(())
            }
        }
        ViewportPresentationEventBody::DeliveryAcknowledged(ack) => {
            if ack.base_ack.source_version.document_session != binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_viewport_presentation_ack(ack)
                .map_err(|failure| validation_error(failure, offset))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidationFailure {
    Invalid,
    Oversized,
}

struct ViewportPresentationDirectoryValidator {
    begin: ViewportPresentationBegin,
    expected_entry_count: u32,
    next_index: u32,
    previous_global_row_ordinal: Option<u64>,
    previous_block_end_utf8: Option<u32>,
    previous_block_end_utf16: Option<u32>,
    inline_source_bytes: u32,
    fact_count: u32,
    transferred_node_count: u32,
}

impl ViewportPresentationDirectoryValidator {
    fn new(
        begin: ViewportPresentationBegin,
        expected_entry_count: u32,
    ) -> Result<Self, ValidationFailure> {
        if expected_entry_count != begin.envelope.ordered_leaf_count {
            return Err(ValidationFailure::Invalid);
        }
        Ok(Self {
            begin,
            expected_entry_count,
            next_index: 0,
            previous_global_row_ordinal: None,
            previous_block_end_utf8: None,
            previous_block_end_utf16: None,
            inline_source_bytes: 0,
            fact_count: 0,
            transferred_node_count: 0,
        })
    }

    fn push(&mut self, entry: ViewportPresentationDirectoryEntry) -> Result<(), ValidationFailure> {
        if self.next_index >= self.expected_entry_count
            || entry.ordered_child_index != self.next_index
        {
            return Err(ValidationFailure::Invalid);
        }
        validate_hot_inline_sidecar_binding(entry.binding, self.begin.base_ack)?;
        validate_hot_inline_sidecar_envelope(entry.hio1_envelope)?;
        if entry.binding.refinement_generation != u64::from(self.begin.binding.viewport_generation)
        {
            return Err(ValidationFailure::Invalid);
        }

        let covered = self.begin.binding.covered_range;
        let block_valid = entry.binding.physical_start_utf8 >= covered.start_utf8
            && entry.binding.physical_start_utf16 >= covered.start_utf16
            && entry.binding.physical_end_utf8 <= covered.end_utf8
            && entry.binding.physical_end_utf16 <= covered.end_utf16;
        let ordinal_valid = entry.global_row_ordinal >= self.begin.binding.start.block_ordinal
            && entry.global_row_ordinal < self.begin.binding.next.block_ordinal
            && self
                .previous_global_row_ordinal
                .is_none_or(|previous| entry.global_row_ordinal > previous);
        let recursive_green_owner_valid = matches!(
            entry.binding.owner(),
            Some(HotInlineSidecarOwner::RecursiveGreenFrame(_))
        );
        let range_order_valid = self
            .previous_block_end_utf8
            .is_none_or(|previous| entry.binding.physical_start_utf8 >= previous)
            && self
                .previous_block_end_utf16
                .is_none_or(|previous| entry.binding.physical_start_utf16 >= previous);
        let inline_bytes = entry
            .binding
            .visible_end_utf8
            .checked_sub(entry.binding.visible_start_utf8)
            .ok_or(ValidationFailure::Invalid)?;
        if !block_valid
            || !ordinal_valid
            || !recursive_green_owner_valid
            || !range_order_valid
            || inline_bytes > self.begin.query_limits.maximum_inline_leaf_source_bytes
        {
            return Err(ValidationFailure::Invalid);
        }

        let child_fact_count = match entry.hio1_envelope.disposition {
            HotInlineSidecarDisposition::Authoritative { fact_count, .. } => {
                u32::try_from(fact_count).map_err(|_| ValidationFailure::Invalid)?
            }
            HotInlineSidecarDisposition::Unsupported { .. } => 0,
        };
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or(ValidationFailure::Invalid)?;
        self.inline_source_bytes = self
            .inline_source_bytes
            .checked_add(inline_bytes)
            .ok_or(ValidationFailure::Invalid)?;
        self.fact_count = self
            .fact_count
            .checked_add(child_fact_count)
            .ok_or(ValidationFailure::Invalid)?;
        self.transferred_node_count = self
            .transferred_node_count
            .checked_add(entry.hio1_envelope.transferred_node_count)
            .ok_or(ValidationFailure::Invalid)?;
        self.previous_global_row_ordinal = Some(entry.global_row_ordinal);
        self.previous_block_end_utf8 = Some(entry.binding.physical_end_utf8);
        self.previous_block_end_utf16 = Some(entry.binding.physical_end_utf16);
        Ok(())
    }

    fn finish(self) -> Result<(), ValidationFailure> {
        if self.next_index != self.expected_entry_count
            || self.inline_source_bytes != self.begin.envelope.inline_source_bytes
            || self.fact_count != self.begin.envelope.fact_count
            || self.transferred_node_count != self.begin.envelope.transferred_node_count
        {
            return Err(ValidationFailure::Invalid);
        }
        Ok(())
    }
}

fn validation_error(failure: ValidationFailure, offset: usize) -> DecodeError {
    publication_error(
        match failure {
            ValidationFailure::Invalid => DecodeFailure::InvalidValue,
            ValidationFailure::Oversized => DecodeFailure::OversizedValue,
        },
        offset,
        None,
        None,
    )
}

fn validate_begin(begin: OfferBegin) -> Result<(), ValidationFailure> {
    if begin.schema != SUPPORTED_MANIFEST_SCHEMA
        || begin.target_host_revision == 0
        || begin.source_root == [0; 2]
        || begin.parse_generation == 0
        || begin.grammar_revision == 0
        || begin.syntax_profile == 0
        || begin.authority_mask == 0
        || begin.authority_mask & !KNOWN_AUTHORITY_BITS != 0
    {
        return Err(ValidationFailure::Invalid);
    }
    validate_limits(begin.limits)?;

    match (begin.mode, begin.base_ack) {
        (PublicationMode::FullSnapshot, None) => {
            if begin.transferred_record_count == 0
                || begin.transferred_record_count != begin.target_record_count
            {
                return Err(ValidationFailure::Invalid);
            }
        }
        (PublicationMode::ExactBaseReferencesDelta, Some(base)) => {
            validate_ack(base)?;
            if base.publication_session == begin.publication_session
                || base.grammar_revision != begin.grammar_revision
                || base.syntax_profile != begin.syntax_profile
                || base.authority_mask != begin.authority_mask
                || base.parse_generation >= begin.parse_generation
                || base.source_version.document_session != begin.source_version.document_session
                || base.source_version.revision >= begin.source_version.revision
                || begin.transferred_record_count == 0
                || begin.transferred_record_count >= begin.target_record_count
            {
                return Err(ValidationFailure::Invalid);
            }
        }
        (PublicationMode::ExactBaseDelta, Some(base)) => {
            validate_ack(base)?;
            if base.publication_session == begin.publication_session
                || base.grammar_revision != begin.grammar_revision
                || base.syntax_profile != begin.syntax_profile
                || base.authority_mask != begin.authority_mask
                || base.parse_generation >= begin.parse_generation
                || base.source_version.document_session != begin.source_version.document_session
                || base.source_version.revision >= begin.source_version.revision
                || begin.transferred_record_count > begin.target_record_count
            {
                return Err(ValidationFailure::Invalid);
            }
        }
        _ => return Err(ValidationFailure::Invalid),
    }
    Ok(())
}

fn validate_hot_inline_sidecar_begin(
    begin: HotInlineSidecarBegin,
) -> Result<(), ValidationFailure> {
    if begin.schema != HOT_INLINE_SIDECAR_SCHEMA
        || begin.offer_id == [0; 4]
        || begin.publication_session == [0; 4]
        || begin.publication_session == begin.base_ack.publication_session
    {
        return Err(ValidationFailure::Invalid);
    }
    validate_ack(begin.base_ack)?;
    validate_hot_inline_sidecar_binding(begin.binding, begin.base_ack)?;
    validate_hot_inline_sidecar_envelope(begin.envelope)?;
    validate_limits(begin.limits)?;

    let required_frame_count = begin
        .envelope
        .transferred_node_count
        .checked_add(2)
        .ok_or(ValidationFailure::Invalid)?;
    if required_frame_count > begin.limits.maximum_frame_count {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_hot_inline_sidecar_binding(
    binding: HotInlineSidecarBinding,
    base_ack: StructuralAck,
) -> Result<(), ValidationFailure> {
    let parser_profile =
        u32::try_from(binding.parser_profile).map_err(|_| ValidationFailure::Invalid)?;
    if binding.parser_profile == 0
        || parser_profile != base_ack.syntax_profile
        || binding.refinement_generation == 0
        || binding.owner().is_none()
        || binding.physical_start_utf8 >= binding.physical_end_utf8
        || binding.physical_end_utf8 > base_ack.source_version.utf8_length
        || binding.visible_start_utf8 >= binding.visible_end_utf8
        || binding.visible_start_utf8 < binding.physical_start_utf8
        || binding.visible_end_utf8 > binding.physical_end_utf8
        || binding.physical_start_utf16 >= binding.physical_end_utf16
        || binding.physical_end_utf16 > base_ack.source_version.utf16_length
        || binding.visible_start_utf16 >= binding.visible_end_utf16
        || binding.visible_start_utf16 < binding.physical_start_utf16
        || binding.visible_end_utf16 > binding.physical_end_utf16
    {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_hot_inline_sidecar_envelope(
    envelope: HotInlineSidecarEnvelopeMetrics,
) -> Result<(), ValidationFailure> {
    if envelope.hio1_encoded_bytes != HIO1_ENVELOPE_BYTES {
        return Err(ValidationFailure::Invalid);
    }
    match envelope.disposition {
        HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ..
        } => {
            if !matches!(
                envelope.ipr2_descriptor_bytes,
                IPR3_DESCRIPTOR_BYTES
                    | PROJECTED_INLINE_PROJECTION_DESCRIPTOR_BYTES
                    | INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES
                    | BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
            ) || (logical_page_count == 0) != (fact_count == 0 && storage_page_count == 0)
                || logical_page_count > 0
                    && (fact_count < logical_page_count || storage_page_count == 0)
                || !valid_hot_inline_link_value_summary(
                    envelope.ipr2_descriptor_bytes,
                    fact_count,
                    link_value_entry_count,
                    link_value_encoded_bytes,
                    link_value_storage_page_count,
                )
            {
                return Err(ValidationFailure::Invalid);
            }
        }
        HotInlineSidecarDisposition::Unsupported { reason, .. } => {
            if reason == 0
                || envelope.ipr2_descriptor_bytes != 0
                || envelope.transferred_node_count != 1
            {
                return Err(ValidationFailure::Invalid);
            }
        }
    }
    Ok(())
}

fn valid_hot_inline_link_value_summary(
    descriptor_bytes: u32,
    fact_count: u64,
    entry_count: u32,
    encoded_bytes: u32,
    storage_page_count: u64,
) -> bool {
    if entry_count == 0 {
        return encoded_bytes == 0 && storage_page_count == 0;
    }
    let minimum_encoded_bytes = entry_count
        .checked_mul(32)
        .and_then(|entries| entries.checked_add(16));
    descriptor_bytes == IPR3_DESCRIPTOR_BYTES
        && u64::from(entry_count) <= fact_count
        && storage_page_count != 0
        && minimum_encoded_bytes.is_some_and(|minimum| encoded_bytes >= minimum)
        && encoded_bytes <= 64 * 1024
}

fn validate_hot_inline_sidecar_commit(
    commit: HotInlineSidecarCommitRequest,
) -> Result<(), ValidationFailure> {
    if commit.offer_id == [0; 4] || commit.actual_frame_count < 2 {
        Err(ValidationFailure::Invalid)
    } else {
        Ok(())
    }
}

fn validate_inline_sidecar_ack(ack: InlineSidecarAck) -> Result<(), ValidationFailure> {
    validate_ack(ack.base_ack)?;
    if ack.publication_session == [0; 4]
        || ack.publication_session == ack.base_ack.publication_session
        || ack.refinement_generation == 0
        || HotInlineSidecarOwner::from_wire(ack.block_ordinal).is_none()
        || matches!(ack.disposition, InlineSidecarAckDisposition::Unsupported)
            && ack.transferred_node_count != 1
    {
        Err(ValidationFailure::Invalid)
    } else {
        Ok(())
    }
}

fn validate_viewport_presentation_begin(
    begin: ViewportPresentationBegin,
) -> Result<(), ValidationFailure> {
    if begin.schema != SUPPORTED_VIEWPORT_PRESENTATION_SCHEMA
        || begin.offer_id == [0; 4]
        || begin.publication_session == [0; 4]
        || begin.publication_session == begin.base_ack.publication_session
        || !matches!(begin.mode, ViewportPresentationMode::AggregatePage)
    {
        return Err(ValidationFailure::Invalid);
    }
    validate_ack(begin.base_ack)?;
    validate_viewport_presentation_binding(begin.binding, begin.base_ack)?;
    validate_viewport_presentation_query_limits(begin.query_limits)?;
    validate_viewport_presentation_envelope(begin.envelope, begin.binding, begin.query_limits)?;
    validate_viewport_presentation_offer_limits(begin.limits)?;
    if begin.limits.maximum_frame_count < 3 {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_viewport_presentation_binding(
    binding: ViewportPresentationBinding,
    base_ack: StructuralAck,
) -> Result<(), ValidationFailure> {
    let requested = binding.requested_range;
    let covered = binding.covered_range;
    let requested_valid = requested.start_utf8 < requested.end_utf8
        && requested.start_utf16 < requested.end_utf16
        && requested.end_utf8 <= base_ack.source_version.utf8_length
        && requested.end_utf16 <= base_ack.source_version.utf16_length;
    let covered_valid = covered.start_utf8 < covered.end_utf8
        && covered.start_utf16 < covered.end_utf16
        && covered.start_utf8 >= requested.start_utf8
        && covered.start_utf16 >= requested.start_utf16
        && covered.end_utf8 <= requested.end_utf8
        && covered.end_utf16 <= requested.end_utf16;
    let cuts_match = binding.start.utf8_offset == covered.start_utf8
        && binding.start.utf16_offset == covered.start_utf16
        && binding.next.utf8_offset == covered.end_utf8
        && binding.next.utf16_offset == covered.end_utf16
        && binding.next.block_ordinal > binding.start.block_ordinal;
    let reaches_end =
        covered.end_utf8 == requested.end_utf8 && covered.end_utf16 == requested.end_utf16;
    if binding.viewport_generation == 0
        || !requested_valid
        || !covered_valid
        || !cuts_match
        || binding.complete != reaches_end
    {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_viewport_presentation_query_limits(
    limits: ViewportPresentationQueryLimits,
) -> Result<(), ValidationFailure> {
    if limits.maximum_structural_entries == 0
        || limits.maximum_storage_pages == 0
        || limits.maximum_inline_leaves == 0
        || limits.maximum_inline_leaf_source_bytes == 0
        || limits.maximum_inline_source_bytes == 0
        || limits.maximum_fact_records == 0
        || limits.maximum_encoded_frame_bytes == 0
        || limits.maximum_parser_transitions == 0
        || limits.maximum_inline_leaves > limits.maximum_structural_entries
        || limits.maximum_inline_leaf_source_bytes > limits.maximum_inline_source_bytes
    {
        return Err(ValidationFailure::Invalid);
    }
    if limits.maximum_structural_entries
        > crate::v3_session_wire::MAXIMUM_VIEWPORT_STRUCTURAL_ENTRIES
        || limits.maximum_storage_pages > crate::v3_session_wire::MAXIMUM_VIEWPORT_STORAGE_PAGES
        || limits.maximum_inline_leaves > crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_LEAVES
        || limits.maximum_inline_leaf_source_bytes
            > crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_LEAF_SOURCE_BYTES
        || limits.maximum_inline_source_bytes
            > crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_SOURCE_BYTES
        || limits.maximum_fact_records > crate::v3_session_wire::MAXIMUM_VIEWPORT_FACT_RECORDS
        || limits.maximum_encoded_frame_bytes
            > crate::v3_session_wire::MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES
        || limits.maximum_parser_transitions
            > crate::v3_session_wire::MAXIMUM_VIEWPORT_PARSER_TRANSITIONS
    {
        return Err(ValidationFailure::Oversized);
    }
    Ok(())
}

fn validate_viewport_presentation_envelope(
    envelope: ViewportPresentationEnvelopeMetrics,
    binding: ViewportPresentationBinding,
    limits: ViewportPresentationQueryLimits,
) -> Result<(), ValidationFailure> {
    let ordinal_span = binding
        .next
        .block_ordinal
        .checked_sub(binding.start.block_ordinal)
        .ok_or(ValidationFailure::Invalid)?;
    if ordinal_span != u64::from(envelope.visited_structural_entries)
        || envelope.visited_structural_entries == 0
        || envelope.visited_storage_pages == 0
        || envelope.ordered_leaf_count > envelope.visited_structural_entries
        || envelope.visited_structural_entries > limits.maximum_structural_entries
        || envelope.visited_storage_pages > limits.maximum_storage_pages
        || envelope.ordered_leaf_count > limits.maximum_inline_leaves
        || envelope.inline_source_bytes > limits.maximum_inline_source_bytes
        || envelope.fact_count > limits.maximum_fact_records
        || envelope.parser_transitions > limits.maximum_parser_transitions
    {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn product_viewport_presentation_query_limits() -> ViewportPresentationQueryLimits {
    ViewportPresentationQueryLimits {
        maximum_structural_entries: crate::v3_session_wire::MAXIMUM_VIEWPORT_STRUCTURAL_ENTRIES,
        maximum_storage_pages: crate::v3_session_wire::MAXIMUM_VIEWPORT_STORAGE_PAGES,
        maximum_inline_leaves: crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_LEAVES,
        maximum_inline_leaf_source_bytes:
            crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_LEAF_SOURCE_BYTES,
        maximum_inline_source_bytes: crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_SOURCE_BYTES,
        maximum_fact_records: crate::v3_session_wire::MAXIMUM_VIEWPORT_FACT_RECORDS,
        maximum_encoded_frame_bytes: crate::v3_session_wire::MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES,
        maximum_parser_transitions: crate::v3_session_wire::MAXIMUM_VIEWPORT_PARSER_TRANSITIONS,
    }
}

fn validate_viewport_presentation_commit(
    commit: ViewportPresentationCommitRequest,
) -> Result<(), ValidationFailure> {
    if commit.offer_id == [0; 4]
        || commit.actual_frame_count < 3
        || commit.actual_encoded_frame_bytes == 0
    {
        Err(ValidationFailure::Invalid)
    } else {
        Ok(())
    }
}

fn validate_viewport_presentation_ack(
    ack: ViewportPresentationAck,
) -> Result<(), ValidationFailure> {
    validate_ack(ack.base_ack)?;
    validate_viewport_presentation_binding(ack.binding, ack.base_ack)?;
    validate_viewport_presentation_envelope(
        ack.envelope,
        ack.binding,
        product_viewport_presentation_query_limits(),
    )?;
    let expected_frame_count = ack
        .envelope
        .ordered_leaf_count
        .checked_mul(2)
        .and_then(|count| count.checked_add(ack.envelope.transferred_node_count))
        .and_then(|count| count.checked_add(3))
        .ok_or(ValidationFailure::Invalid)?;
    if ack.publication_session == [0; 4]
        || ack.publication_session == ack.base_ack.publication_session
        || ack.actual_frame_count != expected_frame_count
        || ack.actual_encoded_frame_bytes == 0
        || ack.actual_encoded_frame_bytes
            > crate::v3_session_wire::MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES
    {
        Err(ValidationFailure::Invalid)
    } else {
        Ok(())
    }
}

fn validate_viewport_presentation_end(
    begin: ViewportPresentationBegin,
    end: ViewportPresentationEndFrame,
) -> Result<(), ValidationFailure> {
    validate_viewport_presentation_begin(begin)?;
    let expected_child_frame_count = begin
        .envelope
        .ordered_leaf_count
        .checked_mul(2)
        .and_then(|count| count.checked_add(begin.envelope.transferred_node_count))
        .ok_or(ValidationFailure::Invalid)?;
    let expected_root_frame_count = expected_child_frame_count
        .checked_add(3)
        .ok_or(ValidationFailure::Invalid)?;
    if end.ordered_leaf_count != begin.envelope.ordered_leaf_count
        || end.actual_frame_count != expected_root_frame_count
        || end.actual_encoded_frame_bytes == 0
        || end.actual_encoded_frame_bytes > begin.query_limits.maximum_encoded_frame_bytes
        || end.actual_encoded_frame_bytes > begin.limits.maximum_encoded_frame_bytes
        || end.aggregate_envelope_digest256 != begin.envelope.aggregate_envelope_digest256
        || end.actual_frame_count > begin.limits.maximum_frame_count
        || VIEWPORT_PRESENTATION_END_FRAME_BYTES > begin.limits.maximum_frame_bytes as usize
    {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_viewport_presentation_child(
    directory_entry_count: u32,
    directory_index: u32,
    child_frame_ordinal: u32,
    kind: HotInlineSidecarFrameKind,
    record_count: u32,
    payload: &[u8],
) -> Result<(), ValidationFailure> {
    let shape_valid = match kind {
        HotInlineSidecarFrameKind::Begin => child_frame_ordinal == 0 && record_count == 0,
        HotInlineSidecarFrameKind::Node => child_frame_ordinal > 0 && record_count == 1,
        HotInlineSidecarFrameKind::End => child_frame_ordinal > 0 && record_count == 0,
    };
    if directory_index >= directory_entry_count || payload.is_empty() || !shape_valid {
        Err(ValidationFailure::Invalid)
    } else {
        Ok(())
    }
}

fn validate_limits(limits: OfferLimits) -> Result<(), ValidationFailure> {
    let minimum_packet_bytes = PACKET_HEADER_BYTES
        .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES)
        .and_then(|bytes| bytes.checked_add(limits.maximum_frame_bytes as usize))
        .ok_or(ValidationFailure::Invalid)?;
    if limits.maximum_frame_count == 0
        || limits.maximum_encoded_frame_bytes == 0
        || limits.maximum_packet_bytes == 0
        || limits.maximum_frame_bytes == 0
        || limits.maximum_program_children == 0
    {
        return Err(ValidationFailure::Invalid);
    }
    if limits.maximum_packet_bytes > PRODUCT_MAX_PACKET_BYTES
        || limits.maximum_frame_bytes > PRODUCT_MAX_FRAME_BYTES
        || limits.maximum_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
        || limits.maximum_program_children > PRODUCT_MAX_PROGRAM_CHILDREN
    {
        return Err(ValidationFailure::Oversized);
    }
    if limits.maximum_frame_bytes > limits.maximum_encoded_frame_bytes
        || minimum_packet_bytes > limits.maximum_packet_bytes as usize
    {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_viewport_presentation_offer_limits(
    limits: ViewportPresentationOfferLimits,
) -> Result<(), ValidationFailure> {
    let minimum_packet_bytes = PACKET_HEADER_BYTES
        .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES)
        .and_then(|bytes| bytes.checked_add(limits.maximum_frame_bytes as usize))
        .ok_or(ValidationFailure::Invalid)?;
    if limits.maximum_frame_count == 0
        || limits.maximum_encoded_frame_bytes == 0
        || limits.maximum_packet_bytes == 0
        || limits.maximum_frame_bytes == 0
        || limits.maximum_program_children == 0
    {
        return Err(ValidationFailure::Invalid);
    }
    if limits.maximum_packet_bytes > PRODUCT_MAX_PACKET_BYTES
        || limits.maximum_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
        || limits.maximum_program_children > PRODUCT_MAX_PROGRAM_CHILDREN
    {
        return Err(ValidationFailure::Oversized);
    }
    if limits.maximum_frame_bytes > limits.maximum_encoded_frame_bytes
        || minimum_packet_bytes > limits.maximum_packet_bytes as usize
    {
        return Err(ValidationFailure::Invalid);
    }
    Ok(())
}

fn validate_ack(ack: StructuralAck) -> Result<(), ValidationFailure> {
    if ack.host_revision == 0
        || ack.source_root == [0; 2]
        || ack.parse_generation == 0
        || ack.grammar_revision == 0
        || ack.syntax_profile == 0
        || ack.authority_mask == 0
        || ack.authority_mask & !KNOWN_AUTHORITY_BITS != 0
    {
        Err(ValidationFailure::Invalid)
    } else {
        Ok(())
    }
}

/// Shared canonicality check for wire surfaces that carry a complete
/// structural ACK. Exact-base equality remains the caller's responsibility.
pub(crate) fn structural_ack_is_valid(ack: StructuralAck) -> bool {
    validate_ack(ack).is_ok()
}

fn validate_outcome(
    ticket: HostPollTicket,
    outcome: HostPollOutcome,
    offset: usize,
) -> Result<(), DecodeError> {
    let valid = match (ticket.phase, outcome) {
        (
            HostPollPhase::PacketCredit,
            HostPollOutcome::PacketCredit {
                offer_id,
                next_frame_ordinal,
            },
        ) => offer_id == ticket.offer_id && next_frame_ordinal != 0,
        (HostPollPhase::Commit, HostPollOutcome::Committed(ack)) => {
            if ack.source_version.document_session != ticket.binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_ack(ack).map_err(|failure| validation_error(failure, offset))?;
            true
        }
        (HostPollPhase::Abort, HostPollOutcome::AbortComplete { offer_id }) => {
            offer_id == ticket.offer_id
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(offset, None, None))
    }
}

fn validate_inline_sidecar_outcome(
    ticket: InlineSidecarHostPollTicket,
    outcome: InlineSidecarHostPollOutcome,
    offset: usize,
) -> Result<(), DecodeError> {
    let valid = match (ticket.phase, outcome) {
        (
            InlineSidecarHostPollPhase::PacketCredit,
            InlineSidecarHostPollOutcome::PacketCredit {
                offer_id,
                next_frame_ordinal,
            },
        ) => offer_id == ticket.offer_id && next_frame_ordinal != 0,
        (InlineSidecarHostPollPhase::Commit, InlineSidecarHostPollOutcome::Committed(ack)) => {
            if ack.base_ack.source_version.document_session != ticket.binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_inline_sidecar_ack(ack)
                .map_err(|failure| validation_error(failure, offset))?;
            ack.publication_session != ack.base_ack.publication_session
        }
        (
            InlineSidecarHostPollPhase::Abort,
            InlineSidecarHostPollOutcome::AbortComplete { offer_id },
        ) => offer_id == ticket.offer_id,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(offset, None, None))
    }
}

fn validate_viewport_presentation_outcome(
    ticket: ViewportPresentationHostPollTicket,
    outcome: ViewportPresentationHostPollOutcome,
    offset: usize,
) -> Result<(), DecodeError> {
    let valid = match (ticket.phase, outcome) {
        (
            ViewportPresentationHostPollPhase::PacketCredit,
            ViewportPresentationHostPollOutcome::PacketCredit {
                offer_id,
                next_frame_ordinal,
            },
        ) => offer_id == ticket.offer_id && next_frame_ordinal != 0,
        (
            ViewportPresentationHostPollPhase::Commit,
            ViewportPresentationHostPollOutcome::Committed(ack),
        ) => {
            if ack.base_ack.source_version.document_session != ticket.binding.document_session {
                return Err(identity_mismatch(offset));
            }
            validate_viewport_presentation_ack(ack)
                .map_err(|failure| validation_error(failure, offset))?;
            ack.publication_session != ack.base_ack.publication_session
        }
        (
            ViewportPresentationHostPollPhase::Abort,
            ViewportPresentationHostPollOutcome::AbortComplete { offer_id },
        ) => offer_id == ticket.offer_id,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(offset, None, None))
    }
}

fn reject_reason(status: Status) -> Result<HostRejectReason, DecodeError> {
    let reason = match status {
        Status::Invalid => HostRejectReason::Invalid,
        Status::Backpressure => HostRejectReason::Backpressure,
        Status::StaleSource => HostRejectReason::StaleSource,
        Status::ExactSourceMismatch => HostRejectReason::ExactSourceMismatch,
        Status::SessionSnapshotRequired => HostRejectReason::SessionSnapshotRequired,
        Status::BaseMismatch => HostRejectReason::BaseMismatch,
        Status::WrongOffer => HostRejectReason::WrongOffer,
        Status::CorruptPayload => HostRejectReason::CorruptPublication,
        Status::QueryBoundExceeded => HostRejectReason::QueryBoundExceeded,
        Status::ForegroundBoundExceeded => HostRejectReason::ForegroundBoundExceeded,
        Status::Superseded => HostRejectReason::Superseded,
        Status::Closed => HostRejectReason::Closed,
        _ => {
            return Err(publication_error(
                DecodeFailure::UnmappedStatus,
                10,
                None,
                Some(status.code() as usize),
            ));
        }
    };
    Ok(reason)
}

fn write_payload_header(writer: &mut PayloadWriter<'_>, variant: u16, binding: SessionBinding) {
    writer.u16(PAYLOAD_SCHEMA);
    writer.u16(variant);
    writer.u32(binding.worker_generation);
    writer.id128(binding.document_session);
    writer.u32(binding.source_session_identity);
}

fn read_payload_header(reader: &mut PayloadReader<'_>) -> Result<PayloadHeader, DecodeError> {
    let schema = reader.u16()?;
    if schema != PAYLOAD_SCHEMA {
        return Err(publication_error(
            DecodeFailure::UnsupportedSchema,
            0,
            Some(PAYLOAD_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let variant = reader.u16()?;
    let worker_generation = reader.u32()?;
    let document_session = reader.id128()?;
    let source_session_identity = reader.u32()?;
    let binding = SessionBinding {
        document_session,
        source_session_identity,
        worker_generation,
    };
    if !binding_is_canonical(binding) {
        return Err(invalid(
            if worker_generation == 0 {
                4
            } else if source_session_identity == 0 {
                24
            } else {
                8
            },
            None,
            None,
        ));
    }
    Ok(PayloadHeader { variant, binding })
}

fn read_poll_ticket(
    reader: &mut PayloadReader<'_>,
    binding: SessionBinding,
) -> Result<HostPollTicket, DecodeError> {
    let poll_ticket = reader.u32()?;
    let offer_id = reader.id128()?;
    let phase_code = reader.u32()?;
    let phase = match phase_code {
        0 => HostPollPhase::PacketCredit,
        1 => HostPollPhase::Commit,
        2 => HostPollPhase::Abort,
        code => {
            return Err(invalid(reader.offset - 4, Some(2), Some(code as usize)));
        }
    };
    if poll_ticket == 0 {
        return Err(invalid(reader.offset - POLL_TICKET_BYTES, Some(1), Some(0)));
    }
    Ok(HostPollTicket {
        binding,
        poll_ticket,
        offer_id,
        phase,
    })
}

fn read_inline_sidecar_poll_ticket(
    reader: &mut PayloadReader<'_>,
    binding: SessionBinding,
) -> Result<InlineSidecarHostPollTicket, DecodeError> {
    let poll_ticket = reader.u32()?;
    let offer_id = reader.id128()?;
    let phase_code = reader.u32()?;
    let phase = match phase_code {
        0x0100 => InlineSidecarHostPollPhase::PacketCredit,
        0x0101 => InlineSidecarHostPollPhase::Commit,
        0x0102 => InlineSidecarHostPollPhase::Abort,
        code => {
            return Err(invalid(
                reader.offset - 4,
                Some(0x0102),
                Some(code as usize),
            ));
        }
    };
    if poll_ticket == 0 || offer_id == [0; 4] {
        return Err(invalid(reader.offset - POLL_TICKET_BYTES, Some(1), Some(0)));
    }
    Ok(InlineSidecarHostPollTicket {
        binding,
        poll_ticket,
        offer_id,
        phase,
    })
}

fn read_viewport_presentation_poll_ticket(
    reader: &mut PayloadReader<'_>,
    binding: SessionBinding,
) -> Result<ViewportPresentationHostPollTicket, DecodeError> {
    let poll_ticket = reader.u32()?;
    let offer_id = reader.id128()?;
    let phase_code = reader.u32()?;
    let phase = match phase_code {
        0x0200 => ViewportPresentationHostPollPhase::PacketCredit,
        0x0201 => ViewportPresentationHostPollPhase::Commit,
        0x0202 => ViewportPresentationHostPollPhase::Abort,
        code => {
            return Err(invalid(
                reader.offset - 4,
                Some(0x0202),
                Some(code as usize),
            ));
        }
    };
    if poll_ticket == 0 || offer_id == [0; 4] {
        return Err(invalid(reader.offset - POLL_TICKET_BYTES, Some(1), Some(0)));
    }
    Ok(ViewportPresentationHostPollTicket {
        binding,
        poll_ticket,
        offer_id,
        phase,
    })
}

fn write_begin(writer: &mut PayloadWriter<'_>, begin: OfferBegin) {
    writer.u32(begin.schema);
    writer.id128(begin.offer_id);
    writer.id128(begin.publication_session);
    writer.u32(begin.target_host_revision);
    write_source_version(writer, begin.source_version);
    writer.u32(begin.source_root[0]);
    writer.u32(begin.source_root[1]);
    writer.u32(begin.parse_generation);
    writer.u32(begin.grammar_revision);
    writer.u32(begin.syntax_profile);
    writer.u32(begin.authority_mask);
    writer.u32(match begin.mode {
        PublicationMode::FullSnapshot => 0,
        PublicationMode::ExactBaseReferencesDelta => 1,
        PublicationMode::ExactBaseDelta => 2,
    });
    writer.u32(u32::from(begin.base_ack.is_some()));
    if let Some(base_ack) = begin.base_ack {
        write_ack(writer, base_ack);
    }
    writer.u32(begin.transferred_record_count);
    writer.u32(begin.target_record_count);
    write_limits(writer, begin.limits);
}

fn read_begin(reader: &mut PayloadReader<'_>) -> Result<OfferBegin, DecodeError> {
    let schema = reader.u32()?;
    if schema != SUPPORTED_MANIFEST_SCHEMA {
        return Err(publication_error(
            DecodeFailure::UnsupportedSchema,
            reader.offset - 4,
            Some(SUPPORTED_MANIFEST_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let offer_id = reader.id128()?;
    let publication_session = reader.id128()?;
    let target_host_revision = reader.u32()?;
    let source_version = read_source_version(reader)?;
    let source_root = [reader.u32()?, reader.u32()?];
    let parse_generation = reader.u32()?;
    let grammar_revision = reader.u32()?;
    let syntax_profile = reader.u32()?;
    let authority_mask = reader.u32()?;
    let mode_code = reader.u32()?;
    let mode = match mode_code {
        0 => PublicationMode::FullSnapshot,
        1 => PublicationMode::ExactBaseReferencesDelta,
        2 => PublicationMode::ExactBaseDelta,
        value => {
            return Err(invalid(reader.offset - 4, Some(2), Some(value as usize)));
        }
    };
    let has_base = reader.u32()?;
    let base_ack = match has_base {
        0 => None,
        1 => Some(read_ack(reader)?),
        value => {
            return Err(invalid(reader.offset - 4, Some(1), Some(value as usize)));
        }
    };
    let transferred_record_count = reader.u32()?;
    let target_record_count = reader.u32()?;
    let limits = read_limits(reader)?;
    let begin = OfferBegin {
        schema,
        offer_id,
        publication_session,
        target_host_revision,
        source_version,
        source_root,
        parse_generation,
        grammar_revision,
        syntax_profile,
        authority_mask,
        mode,
        base_ack,
        transferred_record_count,
        target_record_count,
        limits,
    };
    validate_begin(begin).map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(begin)
}

fn write_hot_inline_sidecar_begin(writer: &mut PayloadWriter<'_>, begin: HotInlineSidecarBegin) {
    writer.u32(begin.schema);
    writer.u32(match begin.mode {
        HotInlineSidecarMode::HotInlineSidecar => 1,
    });
    writer.id128(begin.offer_id);
    writer.id128(begin.publication_session);
    write_ack(writer, begin.base_ack);
    writer.u64(begin.binding.parser_profile);
    writer.u64(begin.binding.refinement_generation);
    writer.u64(begin.binding.block_ordinal);
    writer.u32(begin.binding.physical_start_utf8);
    writer.u32(begin.binding.physical_end_utf8);
    writer.u32(begin.binding.visible_start_utf8);
    writer.u32(begin.binding.visible_end_utf8);
    writer.u32(begin.binding.physical_start_utf16);
    writer.u32(begin.binding.physical_end_utf16);
    writer.u32(begin.binding.visible_start_utf16);
    writer.u32(begin.binding.visible_end_utf16);
    writer.u32(begin.envelope.hio1_encoded_bytes);
    writer.u32(begin.envelope.ipr2_descriptor_bytes);
    writer.u32(begin.envelope.transferred_node_count);
    match begin.envelope.disposition {
        HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ordered_commitment256,
        } => {
            writer.u32(1);
            writer.u32(0);
            writer.u64(logical_page_count);
            writer.u64(fact_count);
            writer.u64(storage_page_count);
            writer.u32(link_value_entry_count);
            writer.u32(link_value_encoded_bytes);
            writer.u64(link_value_storage_page_count);
            writer.digest256(ordered_commitment256);
        }
        HotInlineSidecarDisposition::Unsupported {
            reason,
            metadata_commitment256,
        } => {
            writer.u32(2);
            writer.u32(reason);
            writer.u64(0);
            writer.u64(0);
            writer.u64(0);
            writer.u32(0);
            writer.u32(0);
            writer.u64(0);
            writer.digest256(metadata_commitment256);
        }
    }
    writer.digest256(begin.envelope.hio1_envelope_digest256);
    write_limits(writer, begin.limits);
}

fn read_hot_inline_sidecar_begin(
    reader: &mut PayloadReader<'_>,
) -> Result<HotInlineSidecarBegin, DecodeError> {
    let schema = reader.u32()?;
    if schema != HOT_INLINE_SIDECAR_SCHEMA {
        return Err(publication_error(
            DecodeFailure::UnsupportedSchema,
            reader.offset - 4,
            Some(HOT_INLINE_SIDECAR_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let mode = match reader.u32()? {
        1 => HotInlineSidecarMode::HotInlineSidecar,
        value => {
            return Err(invalid(reader.offset - 4, Some(1), Some(value as usize)));
        }
    };
    let offer_id = reader.id128()?;
    let publication_session = reader.id128()?;
    let base_ack = read_ack(reader)?;
    let binding = HotInlineSidecarBinding {
        parser_profile: reader.u64()?,
        refinement_generation: reader.u64()?,
        block_ordinal: reader.u64()?,
        physical_start_utf8: reader.u32()?,
        physical_end_utf8: reader.u32()?,
        visible_start_utf8: reader.u32()?,
        visible_end_utf8: reader.u32()?,
        physical_start_utf16: reader.u32()?,
        physical_end_utf16: reader.u32()?,
        visible_start_utf16: reader.u32()?,
        visible_end_utf16: reader.u32()?,
    };
    let hio1_encoded_bytes = reader.u32()?;
    let ipr2_descriptor_bytes = reader.u32()?;
    let transferred_node_count = reader.u32()?;
    let disposition_tag = reader.u32()?;
    let reason = reader.u32()?;
    let logical_page_count = reader.u64()?;
    let fact_count = reader.u64()?;
    let storage_page_count = reader.u64()?;
    let link_value_entry_count = reader.u32()?;
    let link_value_encoded_bytes = reader.u32()?;
    let link_value_storage_page_count = reader.u64()?;
    let disposition_commitment = reader.digest256()?;
    let disposition = match disposition_tag {
        1 if reason == 0 => HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ordered_commitment256: disposition_commitment,
        },
        2 if logical_page_count == 0
            && fact_count == 0
            && storage_page_count == 0
            && link_value_entry_count == 0
            && link_value_encoded_bytes == 0
            && link_value_storage_page_count == 0 =>
        {
            HotInlineSidecarDisposition::Unsupported {
                reason,
                metadata_commitment256: disposition_commitment,
            }
        }
        _ => return Err(invalid(reader.offset, None, None)),
    };
    let envelope = HotInlineSidecarEnvelopeMetrics {
        hio1_encoded_bytes,
        ipr2_descriptor_bytes,
        transferred_node_count,
        hio1_envelope_digest256: reader.digest256()?,
        disposition,
    };
    let limits = read_limits(reader)?;
    let begin = HotInlineSidecarBegin {
        schema,
        mode,
        offer_id,
        publication_session,
        base_ack,
        binding,
        envelope,
        limits,
    };
    validate_hot_inline_sidecar_begin(begin)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(begin)
}

fn write_viewport_presentation_begin(
    writer: &mut PayloadWriter<'_>,
    begin: ViewportPresentationBegin,
) {
    writer.u32(begin.schema);
    writer.u32(match begin.mode {
        ViewportPresentationMode::AggregatePage => 1,
    });
    writer.id128(begin.offer_id);
    writer.id128(begin.publication_session);
    write_ack(writer, begin.base_ack);
    write_viewport_presentation_binding(writer, begin.binding);
    write_viewport_presentation_envelope(writer, begin.envelope);
    write_viewport_presentation_query_limits(writer, begin.query_limits);
    write_viewport_presentation_offer_limits(writer, begin.limits);
}

fn read_viewport_presentation_begin(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationBegin, DecodeError> {
    let schema = reader.u32()?;
    if schema != SUPPORTED_VIEWPORT_PRESENTATION_SCHEMA {
        return Err(publication_error(
            DecodeFailure::UnsupportedSchema,
            reader.offset - 4,
            Some(SUPPORTED_VIEWPORT_PRESENTATION_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let mode = match reader.u32()? {
        1 => ViewportPresentationMode::AggregatePage,
        value => {
            return Err(invalid(reader.offset - 4, Some(1), Some(value as usize)));
        }
    };
    let begin = ViewportPresentationBegin {
        schema,
        mode,
        offer_id: reader.id128()?,
        publication_session: reader.id128()?,
        base_ack: read_ack(reader)?,
        binding: read_viewport_presentation_binding(reader)?,
        envelope: read_viewport_presentation_envelope(reader)?,
        query_limits: read_viewport_presentation_query_limits(reader)?,
        limits: read_viewport_presentation_offer_limits(reader)?,
    };
    validate_viewport_presentation_begin(begin)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(begin)
}

fn write_viewport_presentation_metric_range(
    writer: &mut PayloadWriter<'_>,
    range: ViewportPresentationMetricRange,
) {
    writer.u32(range.start_utf8);
    writer.u32(range.start_utf16);
    writer.u32(range.end_utf8);
    writer.u32(range.end_utf16);
}

fn read_viewport_presentation_metric_range(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationMetricRange, DecodeError> {
    Ok(ViewportPresentationMetricRange {
        start_utf8: reader.u32()?,
        start_utf16: reader.u32()?,
        end_utf8: reader.u32()?,
        end_utf16: reader.u32()?,
    })
}

fn write_viewport_presentation_visit_start(
    writer: &mut PayloadWriter<'_>,
    start: ViewportPresentationVisitStart,
) {
    writer.u64(start.block_ordinal);
    writer.u32(start.utf8_offset);
    writer.u32(start.utf16_offset);
}

fn read_viewport_presentation_visit_start(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationVisitStart, DecodeError> {
    Ok(ViewportPresentationVisitStart {
        block_ordinal: reader.u64()?,
        utf8_offset: reader.u32()?,
        utf16_offset: reader.u32()?,
    })
}

fn write_viewport_presentation_binding(
    writer: &mut PayloadWriter<'_>,
    binding: ViewportPresentationBinding,
) {
    writer.u32(binding.viewport_generation);
    write_viewport_presentation_metric_range(writer, binding.requested_range);
    write_viewport_presentation_metric_range(writer, binding.covered_range);
    write_viewport_presentation_visit_start(writer, binding.start);
    write_viewport_presentation_visit_start(writer, binding.next);
    writer.u32(u32::from(binding.complete));
}

fn read_viewport_presentation_binding(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationBinding, DecodeError> {
    let viewport_generation = reader.u32()?;
    let requested_range = read_viewport_presentation_metric_range(reader)?;
    let covered_range = read_viewport_presentation_metric_range(reader)?;
    let start = read_viewport_presentation_visit_start(reader)?;
    let next = read_viewport_presentation_visit_start(reader)?;
    let complete = match reader.u32()? {
        0 => false,
        1 => true,
        value => {
            return Err(invalid(reader.offset - 4, Some(1), Some(value as usize)));
        }
    };
    Ok(ViewportPresentationBinding {
        viewport_generation,
        requested_range,
        covered_range,
        start,
        next,
        complete,
    })
}

fn write_viewport_presentation_envelope(
    writer: &mut PayloadWriter<'_>,
    envelope: ViewportPresentationEnvelopeMetrics,
) {
    writer.u32(envelope.visited_structural_entries);
    writer.u32(envelope.visited_storage_pages);
    writer.u32(envelope.ordered_leaf_count);
    writer.u32(envelope.inline_source_bytes);
    writer.u32(envelope.fact_count);
    writer.u32(envelope.transferred_node_count);
    writer.u32(envelope.parser_transitions);
    writer.digest256(envelope.aggregate_envelope_digest256);
}

fn read_viewport_presentation_envelope(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationEnvelopeMetrics, DecodeError> {
    Ok(ViewportPresentationEnvelopeMetrics {
        visited_structural_entries: reader.u32()?,
        visited_storage_pages: reader.u32()?,
        ordered_leaf_count: reader.u32()?,
        inline_source_bytes: reader.u32()?,
        fact_count: reader.u32()?,
        transferred_node_count: reader.u32()?,
        parser_transitions: reader.u32()?,
        aggregate_envelope_digest256: reader.digest256()?,
    })
}

fn write_viewport_presentation_query_limits(
    writer: &mut PayloadWriter<'_>,
    limits: ViewportPresentationQueryLimits,
) {
    writer.u32(limits.maximum_structural_entries);
    writer.u32(limits.maximum_storage_pages);
    writer.u32(limits.maximum_inline_leaves);
    writer.u32(limits.maximum_inline_leaf_source_bytes);
    writer.u32(limits.maximum_inline_source_bytes);
    writer.u32(limits.maximum_fact_records);
    writer.u32(limits.maximum_encoded_frame_bytes);
    writer.u32(limits.maximum_parser_transitions);
}

fn read_viewport_presentation_query_limits(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationQueryLimits, DecodeError> {
    let limits = ViewportPresentationQueryLimits {
        maximum_structural_entries: reader.u32()?,
        maximum_storage_pages: reader.u32()?,
        maximum_inline_leaves: reader.u32()?,
        maximum_inline_leaf_source_bytes: reader.u32()?,
        maximum_inline_source_bytes: reader.u32()?,
        maximum_fact_records: reader.u32()?,
        maximum_encoded_frame_bytes: reader.u32()?,
        maximum_parser_transitions: reader.u32()?,
    };
    validate_viewport_presentation_query_limits(limits)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(limits)
}

fn write_viewport_presentation_directory_entry(
    writer: &mut PayloadWriter<'_>,
    entry: ViewportPresentationDirectoryEntry,
) {
    writer.u32(entry.ordered_child_index);
    writer.u64(entry.global_row_ordinal);
    write_viewport_presentation_hio1_binding(writer, entry.binding);
    write_viewport_presentation_hio1_envelope(writer, entry.hio1_envelope);
}

fn read_viewport_presentation_directory_entry(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationDirectoryEntry, DecodeError> {
    Ok(ViewportPresentationDirectoryEntry {
        ordered_child_index: reader.u32()?,
        global_row_ordinal: reader.u64()?,
        binding: read_viewport_presentation_hio1_binding(reader)?,
        hio1_envelope: read_viewport_presentation_hio1_envelope(reader)?,
    })
}

fn write_viewport_presentation_hio1_binding(
    writer: &mut PayloadWriter<'_>,
    binding: HotInlineSidecarBinding,
) {
    writer.u64(binding.parser_profile);
    writer.u64(binding.refinement_generation);
    writer.u64(binding.block_ordinal);
    writer.u32(binding.physical_start_utf8);
    writer.u32(binding.physical_end_utf8);
    writer.u32(binding.visible_start_utf8);
    writer.u32(binding.visible_end_utf8);
    writer.u32(binding.physical_start_utf16);
    writer.u32(binding.physical_end_utf16);
    writer.u32(binding.visible_start_utf16);
    writer.u32(binding.visible_end_utf16);
}

fn read_viewport_presentation_hio1_binding(
    reader: &mut PayloadReader<'_>,
) -> Result<HotInlineSidecarBinding, DecodeError> {
    Ok(HotInlineSidecarBinding {
        parser_profile: reader.u64()?,
        refinement_generation: reader.u64()?,
        block_ordinal: reader.u64()?,
        physical_start_utf8: reader.u32()?,
        physical_end_utf8: reader.u32()?,
        visible_start_utf8: reader.u32()?,
        visible_end_utf8: reader.u32()?,
        physical_start_utf16: reader.u32()?,
        physical_end_utf16: reader.u32()?,
        visible_start_utf16: reader.u32()?,
        visible_end_utf16: reader.u32()?,
    })
}

fn write_viewport_presentation_hio1_envelope(
    writer: &mut PayloadWriter<'_>,
    envelope: HotInlineSidecarEnvelopeMetrics,
) {
    writer.u32(envelope.hio1_encoded_bytes);
    writer.u32(envelope.ipr2_descriptor_bytes);
    writer.u32(envelope.transferred_node_count);
    match envelope.disposition {
        HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ordered_commitment256,
        } => {
            writer.u32(1);
            writer.u32(0);
            writer.u64(logical_page_count);
            writer.u64(fact_count);
            writer.u64(storage_page_count);
            writer.u32(link_value_entry_count);
            writer.u32(link_value_encoded_bytes);
            writer.u64(link_value_storage_page_count);
            writer.digest256(ordered_commitment256);
        }
        HotInlineSidecarDisposition::Unsupported {
            reason,
            metadata_commitment256,
        } => {
            writer.u32(2);
            writer.u32(reason);
            writer.u64(0);
            writer.u64(0);
            writer.u64(0);
            writer.u32(0);
            writer.u32(0);
            writer.u64(0);
            writer.digest256(metadata_commitment256);
        }
    }
    writer.digest256(envelope.hio1_envelope_digest256);
}

fn read_viewport_presentation_hio1_envelope(
    reader: &mut PayloadReader<'_>,
) -> Result<HotInlineSidecarEnvelopeMetrics, DecodeError> {
    let hio1_encoded_bytes = reader.u32()?;
    let ipr2_descriptor_bytes = reader.u32()?;
    let transferred_node_count = reader.u32()?;
    let disposition_tag = reader.u32()?;
    let reason = reader.u32()?;
    let logical_page_count = reader.u64()?;
    let fact_count = reader.u64()?;
    let storage_page_count = reader.u64()?;
    let link_value_entry_count = reader.u32()?;
    let link_value_encoded_bytes = reader.u32()?;
    let link_value_storage_page_count = reader.u64()?;
    let commitment = reader.digest256()?;
    let disposition = match disposition_tag {
        1 if reason == 0 => HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ordered_commitment256: commitment,
        },
        2 if logical_page_count == 0
            && fact_count == 0
            && storage_page_count == 0
            && link_value_entry_count == 0
            && link_value_encoded_bytes == 0
            && link_value_storage_page_count == 0 =>
        {
            HotInlineSidecarDisposition::Unsupported {
                reason,
                metadata_commitment256: commitment,
            }
        }
        _ => return Err(invalid(reader.offset, None, None)),
    };
    let envelope = HotInlineSidecarEnvelopeMetrics {
        hio1_encoded_bytes,
        ipr2_descriptor_bytes,
        transferred_node_count,
        hio1_envelope_digest256: reader.digest256()?,
        disposition,
    };
    validate_hot_inline_sidecar_envelope(envelope)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(envelope)
}

fn read_packet<'payload>(
    reader: &mut PayloadReader<'payload>,
) -> Result<PublicationPacket<'payload>, DecodeError> {
    let packet_offset = reader.offset;
    let bytes = reader.remainder();
    decode_publication_packet(bytes).map_err(|mut error| {
        error.byte_offset = error.byte_offset.saturating_add(packet_offset);
        error
    })
}

fn write_commit(writer: &mut PayloadWriter<'_>, commit: CommitRequest) {
    writer.id128(commit.offer_id);
    writer.u32(commit.actual_frame_count);
    writer.u32(commit.actual_encoded_frame_bytes);
    writer.id128(commit.rolling_transport_digest);
    writer.id128(commit.canonical_stream_digest);
}

fn read_commit(reader: &mut PayloadReader<'_>) -> Result<CommitRequest, DecodeError> {
    Ok(CommitRequest {
        offer_id: reader.id128()?,
        actual_frame_count: reader.u32()?,
        actual_encoded_frame_bytes: reader.u32()?,
        rolling_transport_digest: reader.id128()?,
        canonical_stream_digest: reader.id128()?,
    })
}

fn write_hot_inline_sidecar_commit(
    writer: &mut PayloadWriter<'_>,
    commit: HotInlineSidecarCommitRequest,
) {
    writer.id128(commit.offer_id);
    writer.u32(commit.actual_frame_count);
    writer.u32(commit.actual_encoded_frame_bytes);
    writer.id128(commit.rolling_transport_digest);
    writer.id128(commit.root_stream_digest);
}

fn read_hot_inline_sidecar_commit(
    reader: &mut PayloadReader<'_>,
) -> Result<HotInlineSidecarCommitRequest, DecodeError> {
    let commit = HotInlineSidecarCommitRequest {
        offer_id: reader.id128()?,
        actual_frame_count: reader.u32()?,
        actual_encoded_frame_bytes: reader.u32()?,
        rolling_transport_digest: reader.id128()?,
        root_stream_digest: reader.id128()?,
    };
    validate_hot_inline_sidecar_commit(commit)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(commit)
}

fn write_viewport_presentation_commit(
    writer: &mut PayloadWriter<'_>,
    commit: ViewportPresentationCommitRequest,
) {
    writer.id128(commit.offer_id);
    writer.u32(commit.actual_frame_count);
    writer.u32(commit.actual_encoded_frame_bytes);
    writer.id128(commit.rolling_transport_digest);
    writer.id128(commit.aggregate_root_stream_digest);
}

fn read_viewport_presentation_commit(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationCommitRequest, DecodeError> {
    let commit = ViewportPresentationCommitRequest {
        offer_id: reader.id128()?,
        actual_frame_count: reader.u32()?,
        actual_encoded_frame_bytes: reader.u32()?,
        rolling_transport_digest: reader.id128()?,
        aggregate_root_stream_digest: reader.id128()?,
    };
    validate_viewport_presentation_commit(commit)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(commit)
}

fn write_ack(writer: &mut PayloadWriter<'_>, ack: StructuralAck) {
    writer.id128(ack.publication_session);
    writer.u32(ack.host_revision);
    write_source_version(writer, ack.source_version);
    writer.u32(ack.source_root[0]);
    writer.u32(ack.source_root[1]);
    writer.u32(ack.parse_generation);
    writer.u32(ack.grammar_revision);
    writer.u32(ack.syntax_profile);
    writer.u32(ack.authority_mask);
    writer.u32(ack.record_count);
    writer.id128(ack.sequence_digest);
    writer.id128(ack.manifest_digest);
}

fn read_ack(reader: &mut PayloadReader<'_>) -> Result<StructuralAck, DecodeError> {
    let ack = StructuralAck {
        publication_session: reader.id128()?,
        host_revision: reader.u32()?,
        source_version: read_source_version(reader)?,
        source_root: [reader.u32()?, reader.u32()?],
        parse_generation: reader.u32()?,
        grammar_revision: reader.u32()?,
        syntax_profile: reader.u32()?,
        authority_mask: reader.u32()?,
        record_count: reader.u32()?,
        sequence_digest: reader.id128()?,
        manifest_digest: reader.id128()?,
    };
    validate_ack(ack).map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(ack)
}

fn write_inline_sidecar_ack(writer: &mut PayloadWriter<'_>, ack: InlineSidecarAck) {
    writer.id128(ack.publication_session);
    write_ack(writer, ack.base_ack);
    writer.u64(ack.refinement_generation);
    writer.u64(ack.block_ordinal);
    writer.u32(ack.transferred_node_count);
    writer.u32(match ack.disposition {
        InlineSidecarAckDisposition::Authoritative => 1,
        InlineSidecarAckDisposition::Unsupported => 2,
    });
    writer.digest256(ack.hio1_envelope_digest256);
    writer.id128(ack.root_stream_digest);
}

fn read_inline_sidecar_ack(
    reader: &mut PayloadReader<'_>,
) -> Result<InlineSidecarAck, DecodeError> {
    let publication_session = reader.id128()?;
    let base_ack = read_ack(reader)?;
    let refinement_generation = reader.u64()?;
    let block_ordinal = reader.u64()?;
    let transferred_node_count = reader.u32()?;
    let disposition = match reader.u32()? {
        1 => InlineSidecarAckDisposition::Authoritative,
        2 => InlineSidecarAckDisposition::Unsupported,
        value => {
            return Err(invalid(reader.offset - 4, Some(2), Some(value as usize)));
        }
    };
    let ack = InlineSidecarAck {
        publication_session,
        base_ack,
        refinement_generation,
        block_ordinal,
        transferred_node_count,
        disposition,
        hio1_envelope_digest256: reader.digest256()?,
        root_stream_digest: reader.id128()?,
    };
    validate_inline_sidecar_ack(ack).map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(ack)
}

fn write_viewport_presentation_ack(writer: &mut PayloadWriter<'_>, ack: ViewportPresentationAck) {
    writer.id128(ack.publication_session);
    write_ack(writer, ack.base_ack);
    write_viewport_presentation_binding(writer, ack.binding);
    write_viewport_presentation_envelope(writer, ack.envelope);
    writer.u32(ack.actual_frame_count);
    writer.u32(ack.actual_encoded_frame_bytes);
    writer.id128(ack.aggregate_root_stream_digest);
}

fn read_viewport_presentation_ack(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationAck, DecodeError> {
    let ack = ViewportPresentationAck {
        publication_session: reader.id128()?,
        base_ack: read_ack(reader)?,
        binding: read_viewport_presentation_binding(reader)?,
        envelope: read_viewport_presentation_envelope(reader)?,
        actual_frame_count: reader.u32()?,
        actual_encoded_frame_bytes: reader.u32()?,
        aggregate_root_stream_digest: reader.id128()?,
    };
    validate_viewport_presentation_ack(ack)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(ack)
}

fn write_source_version(writer: &mut PayloadWriter<'_>, source: SourceVersion) {
    writer.id128(source.document_session);
    writer.u32(source.revision);
    writer.u32(source.utf8_length);
    writer.u32(source.utf16_length);
    writer.id128(source.content_hash128);
}

fn read_source_version(reader: &mut PayloadReader<'_>) -> Result<SourceVersion, DecodeError> {
    Ok(SourceVersion {
        document_session: reader.id128()?,
        revision: reader.u32()?,
        utf8_length: reader.u32()?,
        utf16_length: reader.u32()?,
        content_hash128: reader.id128()?,
    })
}

fn write_limits(writer: &mut PayloadWriter<'_>, limits: OfferLimits) {
    writer.u32(limits.maximum_frame_count);
    writer.u32(limits.maximum_encoded_frame_bytes);
    writer.u32(limits.maximum_packet_bytes);
    writer.u32(limits.maximum_frame_bytes);
    writer.u32(limits.maximum_program_children);
}

fn write_viewport_presentation_offer_limits(
    writer: &mut PayloadWriter<'_>,
    limits: ViewportPresentationOfferLimits,
) {
    writer.u32(limits.maximum_frame_count);
    writer.u32(limits.maximum_encoded_frame_bytes);
    writer.u32(limits.maximum_packet_bytes);
    writer.u32(limits.maximum_frame_bytes);
    writer.u32(limits.maximum_program_children);
}

fn read_limits(reader: &mut PayloadReader<'_>) -> Result<OfferLimits, DecodeError> {
    let limits = OfferLimits {
        maximum_frame_count: reader.u32()?,
        maximum_encoded_frame_bytes: reader.u32()?,
        maximum_packet_bytes: reader.u32()?,
        maximum_frame_bytes: reader.u32()?,
        maximum_program_children: reader.u32()?,
    };
    validate_limits(limits).map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(limits)
}

fn read_viewport_presentation_offer_limits(
    reader: &mut PayloadReader<'_>,
) -> Result<ViewportPresentationOfferLimits, DecodeError> {
    let limits = ViewportPresentationOfferLimits {
        maximum_frame_count: reader.u32()?,
        maximum_encoded_frame_bytes: reader.u32()?,
        maximum_packet_bytes: reader.u32()?,
        maximum_frame_bytes: reader.u32()?,
        maximum_program_children: reader.u32()?,
    };
    validate_viewport_presentation_offer_limits(limits)
        .map_err(|failure| validation_error(failure, reader.offset))?;
    Ok(limits)
}

fn read_viewport_presentation_frame_header(
    reader: &mut PayloadReader<'_>,
    expected_magic: u32,
) -> Result<(), DecodeError> {
    let magic = reader.u32()?;
    if magic != expected_magic {
        return Err(invalid(
            0,
            Some(expected_magic as usize),
            Some(magic as usize),
        ));
    }
    let schema = reader.u16()?;
    if schema != VIEWPORT_PRESENTATION_FRAME_SCHEMA {
        return Err(publication_error(
            DecodeFailure::UnsupportedSchema,
            4,
            Some(VIEWPORT_PRESENTATION_FRAME_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let flags = reader.u16()?;
    if flags != VIEWPORT_PRESENTATION_FRAME_FLAGS {
        return Err(invalid(
            6,
            Some(VIEWPORT_PRESENTATION_FRAME_FLAGS as usize),
            Some(flags as usize),
        ));
    }
    Ok(())
}

fn exact_frame_length_error(actual: usize, expected: usize) -> DecodeError {
    publication_error(
        if actual < expected {
            DecodeFailure::TruncatedPayload
        } else {
            DecodeFailure::TrailingPayload
        },
        actual.min(expected),
        Some(expected),
        Some(actual),
    )
}

fn unknown_variant(actual: u16) -> DecodeError {
    publication_error(
        DecodeFailure::UnknownVariant,
        2,
        None,
        Some(actual as usize),
    )
}

fn invalid(offset: usize, expected: Option<usize>, actual: Option<usize>) -> DecodeError {
    publication_error(DecodeFailure::InvalidValue, offset, expected, actual)
}

fn identity_mismatch(offset: usize) -> DecodeError {
    publication_error(DecodeFailure::IdentityMismatch, offset, None, None)
}

const fn publication_error(
    failure: DecodeFailure,
    byte_offset: usize,
    expected: Option<usize>,
    actual: Option<usize>,
) -> DecodeError {
    DecodeError {
        failure,
        byte_offset,
        expected,
        actual,
    }
}

struct PayloadWriter<'buffer> {
    bytes: &'buffer mut [u8],
    offset: usize,
}

impl<'buffer> PayloadWriter<'buffer> {
    fn new(bytes: &'buffer mut [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn len(&self) -> usize {
        self.offset
    }

    fn u16(&mut self, value: u16) {
        self.bytes[self.offset..self.offset + 2].copy_from_slice(&value.to_le_bytes());
        self.offset += 2;
    }

    fn u32(&mut self, value: u32) {
        self.bytes[self.offset..self.offset + 4].copy_from_slice(&value.to_le_bytes());
        self.offset += 4;
    }

    fn u64(&mut self, value: u64) {
        self.bytes[self.offset..self.offset + 8].copy_from_slice(&value.to_le_bytes());
        self.offset += 8;
    }

    fn id128(&mut self, value: [u32; 4]) {
        for word in value {
            self.u32(word);
        }
    }

    fn digest256(&mut self, value: [u8; 32]) {
        self.raw(&value);
    }

    fn raw(&mut self, value: &[u8]) {
        self.bytes[self.offset..self.offset + value.len()].copy_from_slice(value);
        self.offset += value.len();
    }
}

struct PayloadReader<'payload> {
    bytes: &'payload [u8],
    offset: usize,
}

impl<'payload> PayloadReader<'payload> {
    const fn new(bytes: &'payload [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        self.require(2)?;
        let value = u16::from_le_bytes(
            self.bytes[self.offset..self.offset + 2]
                .try_into()
                .expect("checked publication u16 slice must remain exact"),
        );
        self.offset += 2;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        self.require(4)?;
        let value = u32::from_le_bytes(
            self.bytes[self.offset..self.offset + 4]
                .try_into()
                .expect("checked publication u32 slice must remain exact"),
        );
        self.offset += 4;
        Ok(value)
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        self.require(8)?;
        let value = u64::from_le_bytes(
            self.bytes[self.offset..self.offset + 8]
                .try_into()
                .expect("checked publication u64 slice must remain exact"),
        );
        self.offset += 8;
        Ok(value)
    }

    fn id128(&mut self) -> Result<[u32; 4], DecodeError> {
        Ok([self.u32()?, self.u32()?, self.u32()?, self.u32()?])
    }

    fn digest256(&mut self) -> Result<[u8; 32], DecodeError> {
        self.require(32)?;
        let value = self.bytes[self.offset..self.offset + 32]
            .try_into()
            .expect("checked publication digest slice must remain exact");
        self.offset += 32;
        Ok(value)
    }

    fn remainder(&mut self) -> &'payload [u8] {
        let bytes = &self.bytes[self.offset..];
        self.offset = self.bytes.len();
        bytes
    }

    fn finish(self) -> Result<(), DecodeError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(publication_error(
                DecodeFailure::TrailingPayload,
                self.offset,
                Some(self.offset),
                Some(self.bytes.len()),
            ))
        }
    }

    fn require(&self, count: usize) -> Result<(), DecodeError> {
        let required = self.offset.checked_add(count).ok_or_else(|| {
            publication_error(
                DecodeFailure::TruncatedPayload,
                self.offset,
                None,
                Some(self.bytes.len()),
            )
        })?;
        if required > self.bytes.len() {
            Err(publication_error(
                DecodeFailure::TruncatedPayload,
                self.offset,
                Some(required),
                Some(self.bytes.len()),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed output shared with the live Dart schema-v4 codec. These are
    // intentionally not reconstructed by a Rust helper, so lane/order drift is
    // visible in either direction.
    const BEGIN_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 10 01 00 00 00 00 00 00 07 00 00 00 ac 00 00 00 ",
        "04 00 00 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "01 00 00 00 09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 ",
        "05 00 00 00 06 00 00 00 07 00 00 00 08 00 00 00 02 00 00 00 ",
        "01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 14 00 00 00 10 00 00 00 ",
        "02 00 00 00 03 00 00 00 04 00 00 00 05 00 00 00 00 00 00 00 02 00 00 00 ",
        "02 00 00 00 01 00 00 00 01 00 00 00 1f 00 00 00 00 00 00 00 00 00 00 00 ",
        "02 00 00 00 02 00 00 00 08 00 00 00 00 10 00 00 00 08 00 00 00 04 00 00 20 00 00 00"
    );
    const PACKET_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 11 01 00 00 00 00 00 00 08 00 00 00 96 00 00 00 ",
        "04 00 00 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "46 50 4b 33 01 00 00 00 ",
        "09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 04 00 00 00 14 00 00 00 ",
        "03 00 00 00 06 00 00 00 06 00 00 00 ",
        "01 00 00 00 02 00 00 00 1e 00 00 00 1f 00 00 00 20 00 00 00 21 00 00 00 ",
        "02 00 00 00 03 00 00 00 28 00 00 00 29 00 00 00 2a 00 00 00 2b 00 00 00 ",
        "03 00 00 00 01 00 00 00 32 00 00 00 33 00 00 00 34 00 00 00 35 00 00 00 ",
        "aa bb cc dd ee ff"
    );
    const COMMIT_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 12 01 00 00 00 00 00 00 09 00 00 00 54 00 00 00 ",
        "04 00 00 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 02 00 00 00 20 03 00 00 ",
        "28 00 00 00 29 00 00 00 2a 00 00 00 2b 00 00 00 32 00 00 00 33 00 00 00 34 00 00 00 35 00 00 00"
    );
    const DELIVERY_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 40 01 00 00 00 00 00 00 0c 00 00 00 98 00 00 00 ",
        "04 00 00 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "05 00 00 00 06 00 00 00 07 00 00 00 08 00 00 00 01 00 00 00 ",
        "01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 01 00 00 00 0a 00 00 00 08 00 00 00 ",
        "01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 00 00 00 00 01 00 00 00 ",
        "01 00 00 00 01 00 00 00 01 00 00 00 1f 00 00 00 03 00 00 00 ",
        "3c 00 00 00 3d 00 00 00 3e 00 00 00 3f 00 00 00 46 00 00 00 47 00 00 00 48 00 00 00 49 00 00 00"
    );
    const PACKET_CREDIT_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 20 01 00 00 00 00 00 00 64 00 00 00 48 00 00 00 ",
        "04 00 01 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "64 00 00 00 09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 00 00 00 00 ",
        "09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 06 00 00 00"
    );
    const COMMITTED_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 20 01 00 00 00 00 00 00 65 00 00 00 b0 00 00 00 ",
        "04 00 02 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "65 00 00 00 09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 01 00 00 00 ",
        "05 00 00 00 06 00 00 00 07 00 00 00 08 00 00 00 01 00 00 00 ",
        "01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 01 00 00 00 0a 00 00 00 08 00 00 00 ",
        "01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 00 00 00 00 01 00 00 00 ",
        "01 00 00 00 01 00 00 00 01 00 00 00 1f 00 00 00 03 00 00 00 ",
        "3c 00 00 00 3d 00 00 00 3e 00 00 00 3f 00 00 00 46 00 00 00 47 00 00 00 48 00 00 00 49 00 00 00"
    );
    const ABORT_COMPLETE_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 20 01 00 00 00 00 00 00 66 00 00 00 44 00 00 00 ",
        "04 00 03 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "66 00 00 00 09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 02 00 00 00 ",
        "09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00"
    );
    const REJECTION_GOLDEN: &str = concat!(
        "46 4c 4b 33 01 00 01 00 20 01 08 01 00 00 00 00 67 00 00 00 34 00 00 00 ",
        "04 00 00 00 03 00 00 00 01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 02 00 00 00 ",
        "67 00 00 00 09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 01 00 00 00"
    );

    fn binding() -> SessionBinding {
        SessionBinding {
            document_session: [1, 2, 3, 4],
            source_session_identity: 2,
            worker_generation: 3,
        }
    }

    fn digest(first: u32) -> Digest128 {
        [first, first + 1, first + 2, first + 3]
    }

    fn source(revision: u32) -> SourceVersion {
        SourceVersion {
            document_session: [1, 2, 3, 4],
            revision,
            utf8_length: revision * 10,
            utf16_length: revision * 8,
            content_hash128: digest(revision),
        }
    }

    fn ack() -> StructuralAck {
        StructuralAck {
            publication_session: [5, 6, 7, 8],
            host_revision: 1,
            source_version: source(1),
            source_root: [0, 1],
            parse_generation: 1,
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: KNOWN_AUTHORITY_BITS,
            record_count: 3,
            sequence_digest: digest(60),
            manifest_digest: digest(70),
        }
    }

    fn begin() -> OfferBegin {
        OfferBegin {
            schema: 1,
            offer_id: [9, 10, 11, 12],
            publication_session: [5, 6, 7, 8],
            target_host_revision: 2,
            source_version: source(2),
            source_root: [0, 2],
            parse_generation: 2,
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: KNOWN_AUTHORITY_BITS,
            mode: PublicationMode::FullSnapshot,
            base_ack: None,
            transferred_record_count: 2,
            target_record_count: 2,
            limits: OfferLimits {
                maximum_frame_count: 8,
                maximum_encoded_frame_bytes: 4096,
                maximum_packet_bytes: 2048,
                maximum_frame_bytes: 1024,
                maximum_program_children: 32,
            },
        }
    }

    fn digest256(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn hot_inline_begin(disposition: HotInlineSidecarDisposition) -> HotInlineSidecarBegin {
        let (ipr2_descriptor_bytes, transferred_node_count) = match disposition {
            HotInlineSidecarDisposition::Authoritative { .. } => (IPR3_DESCRIPTOR_BYTES, 2),
            HotInlineSidecarDisposition::Unsupported { .. } => (0, 1),
        };
        HotInlineSidecarBegin {
            schema: HOT_INLINE_SIDECAR_SCHEMA,
            mode: HotInlineSidecarMode::HotInlineSidecar,
            offer_id: [9, 10, 11, 12],
            publication_session: [15, 16, 17, 18],
            base_ack: ack(),
            binding: HotInlineSidecarBinding {
                parser_profile: 1,
                refinement_generation: 7,
                block_ordinal: 3,
                physical_start_utf8: 0,
                physical_end_utf8: 10,
                visible_start_utf8: 1,
                visible_end_utf8: 9,
                physical_start_utf16: 0,
                physical_end_utf16: 8,
                visible_start_utf16: 1,
                visible_end_utf16: 7,
            },
            envelope: HotInlineSidecarEnvelopeMetrics {
                hio1_encoded_bytes: HIO1_ENVELOPE_BYTES,
                ipr2_descriptor_bytes,
                transferred_node_count,
                hio1_envelope_digest256: digest256(0x91),
                disposition,
            },
            limits: OfferLimits {
                maximum_frame_count: 8,
                maximum_encoded_frame_bytes: 4096,
                maximum_packet_bytes: 2048,
                maximum_frame_bytes: 1024,
                maximum_program_children: 32,
            },
        }
    }

    fn authoritative_hot_inline_begin() -> HotInlineSidecarBegin {
        hot_inline_begin(HotInlineSidecarDisposition::Authoritative {
            logical_page_count: 1,
            fact_count: 2,
            storage_page_count: 1,
            link_value_entry_count: 0,
            link_value_encoded_bytes: 0,
            link_value_storage_page_count: 0,
            ordered_commitment256: digest256(0x81),
        })
    }

    fn unsupported_hot_inline_begin() -> HotInlineSidecarBegin {
        hot_inline_begin(HotInlineSidecarDisposition::Unsupported {
            reason: 0x2000_0001,
            metadata_commitment256: digest256(0x82),
        })
    }

    fn inline_sidecar_ack(
        disposition: InlineSidecarAckDisposition,
        transferred_node_count: u32,
    ) -> InlineSidecarAck {
        InlineSidecarAck {
            publication_session: [15, 16, 17, 18],
            base_ack: ack(),
            refinement_generation: 7,
            block_ordinal: 3,
            transferred_node_count,
            disposition,
            hio1_envelope_digest256: digest256(0x91),
            root_stream_digest: digest(90),
        }
    }

    fn viewport_presentation_begin() -> ViewportPresentationBegin {
        ViewportPresentationBegin {
            schema: SUPPORTED_VIEWPORT_PRESENTATION_SCHEMA,
            mode: ViewportPresentationMode::AggregatePage,
            offer_id: [19, 20, 21, 22],
            publication_session: [23, 24, 25, 26],
            base_ack: ack(),
            binding: ViewportPresentationBinding {
                viewport_generation: 9,
                requested_range: ViewportPresentationMetricRange {
                    start_utf8: 0,
                    start_utf16: 0,
                    end_utf8: 10,
                    end_utf16: 8,
                },
                covered_range: ViewportPresentationMetricRange {
                    start_utf8: 0,
                    start_utf16: 0,
                    end_utf8: 10,
                    end_utf16: 8,
                },
                start: ViewportPresentationVisitStart {
                    block_ordinal: 7,
                    utf8_offset: 0,
                    utf16_offset: 0,
                },
                next: ViewportPresentationVisitStart {
                    block_ordinal: 10,
                    utf8_offset: 10,
                    utf16_offset: 8,
                },
                complete: true,
            },
            envelope: ViewportPresentationEnvelopeMetrics {
                visited_structural_entries: 3,
                visited_storage_pages: 2,
                ordered_leaf_count: 2,
                inline_source_bytes: 8,
                fact_count: 4,
                transferred_node_count: 4,
                parser_transitions: 12,
                aggregate_envelope_digest256: digest256(0xa1),
            },
            query_limits: ViewportPresentationQueryLimits {
                maximum_structural_entries: 8,
                maximum_storage_pages: 8,
                maximum_inline_leaves: 8,
                maximum_inline_leaf_source_bytes: 1024,
                maximum_inline_source_bytes: 4096,
                maximum_fact_records: 32,
                maximum_encoded_frame_bytes: 4096,
                maximum_parser_transitions: 1000,
            },
            limits: ViewportPresentationOfferLimits {
                maximum_frame_count: 16,
                maximum_encoded_frame_bytes: 4096,
                maximum_packet_bytes: 2048,
                maximum_frame_bytes: 1024,
                maximum_program_children: 32,
            },
        }
    }

    fn viewport_presentation_ack() -> ViewportPresentationAck {
        let begin = viewport_presentation_begin();
        ViewportPresentationAck {
            publication_session: begin.publication_session,
            base_ack: begin.base_ack,
            binding: begin.binding,
            envelope: begin.envelope,
            actual_frame_count: 11,
            actual_encoded_frame_bytes: 960,
            aggregate_root_stream_digest: digest(150),
        }
    }

    fn viewport_presentation_directory_entries() -> [ViewportPresentationDirectoryEntry; 2] {
        [
            ViewportPresentationDirectoryEntry {
                ordered_child_index: 0,
                global_row_ordinal: 7,
                binding: HotInlineSidecarBinding {
                    parser_profile: 1,
                    refinement_generation: 9,
                    block_ordinal: HotInlineSidecarOwner::RecursiveGreenFrame(101)
                        .into_wire()
                        .unwrap(),
                    physical_start_utf8: 0,
                    physical_end_utf8: 5,
                    visible_start_utf8: 0,
                    visible_end_utf8: 4,
                    physical_start_utf16: 0,
                    physical_end_utf16: 4,
                    visible_start_utf16: 0,
                    visible_end_utf16: 3,
                },
                hio1_envelope: HotInlineSidecarEnvelopeMetrics {
                    hio1_encoded_bytes: HIO1_ENVELOPE_BYTES,
                    ipr2_descriptor_bytes: IPR3_DESCRIPTOR_BYTES,
                    transferred_node_count: 2,
                    hio1_envelope_digest256: digest256(0xb1),
                    disposition: HotInlineSidecarDisposition::Authoritative {
                        logical_page_count: 1,
                        fact_count: 2,
                        storage_page_count: 1,
                        link_value_entry_count: 1,
                        link_value_encoded_bytes: 49,
                        link_value_storage_page_count: 1,
                        ordered_commitment256: digest256(0xc1),
                    },
                },
            },
            ViewportPresentationDirectoryEntry {
                ordered_child_index: 1,
                global_row_ordinal: 9,
                binding: HotInlineSidecarBinding {
                    parser_profile: 1,
                    refinement_generation: 9,
                    block_ordinal: HotInlineSidecarOwner::RecursiveGreenFrame(102)
                        .into_wire()
                        .unwrap(),
                    physical_start_utf8: 5,
                    physical_end_utf8: 10,
                    visible_start_utf8: 6,
                    visible_end_utf8: 10,
                    physical_start_utf16: 4,
                    physical_end_utf16: 8,
                    visible_start_utf16: 5,
                    visible_end_utf16: 8,
                },
                hio1_envelope: HotInlineSidecarEnvelopeMetrics {
                    hio1_encoded_bytes: HIO1_ENVELOPE_BYTES,
                    ipr2_descriptor_bytes: IPR3_DESCRIPTOR_BYTES,
                    transferred_node_count: 2,
                    hio1_envelope_digest256: digest256(0xb2),
                    disposition: HotInlineSidecarDisposition::Authoritative {
                        logical_page_count: 1,
                        fact_count: 2,
                        storage_page_count: 1,
                        link_value_entry_count: 0,
                        link_value_encoded_bytes: 0,
                        link_value_storage_page_count: 0,
                        ordered_commitment256: digest256(0xc2),
                    },
                },
            },
        ]
    }

    fn encode(event_id: u32, body: PublicationEventBody<'_>) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + v3_wire::MAXIMUM_PAYLOAD_BYTES];
        let written = encode_event_into(
            PublicationEvent {
                event_id,
                binding: binding(),
                body,
            },
            binding(),
            &mut output,
        )
        .unwrap();
        output.truncate(written);
        output
    }

    fn encode_viewport(event_id: u32, body: ViewportPresentationEventBody<'_>) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + v3_wire::MAXIMUM_PAYLOAD_BYTES];
        let written = encode_viewport_presentation_event_into(
            ViewportPresentationEvent {
                event_id,
                binding: binding(),
                body,
            },
            binding(),
            &mut output,
        )
        .unwrap();
        output.truncate(written);
        output
    }

    fn encode_hot_inline(event_id: u32, body: HotInlineSidecarEventBody<'_>) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + v3_wire::MAXIMUM_PAYLOAD_BYTES];
        let written = encode_hot_inline_sidecar_event_into(
            HotInlineSidecarEvent {
                event_id,
                binding: binding(),
                body,
            },
            binding(),
            &mut output,
        )
        .unwrap();
        output.truncate(written);
        output
    }

    fn encode_packet(input: PublicationPacketInput<'_, '_>) -> Vec<u8> {
        let mut output = vec![0; MAXIMUM_PACKET_ENCODED_BYTES];
        let written = encode_publication_packet_into(input, &mut output).unwrap();
        output.truncate(written);
        output
    }

    fn hex(value: &str) -> Vec<u8> {
        value
            .split_ascii_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
    }

    fn resize_payload(bytes: &[u8], delta: isize) -> Vec<u8> {
        let new_len = bytes.len().checked_add_signed(delta).unwrap();
        let copy_len = new_len.min(bytes.len());
        let mut resized = vec![0; new_len];
        resized[..copy_len].copy_from_slice(&bytes[..copy_len]);
        let payload_len = new_len - v3_wire::HEADER_BYTES;
        resized[20..24].copy_from_slice(&(payload_len as u32).to_le_bytes());
        resized
    }

    fn assert_failure(result: Result<impl fmt::Debug, DecodeError>, failure: DecodeFailure) {
        assert_eq!(result.unwrap_err().failure, failure);
    }

    fn assert_golden(actual: &[u8], expected: &[u8]) {
        assert_eq!(actual.len(), expected.len(), "golden length");
        if let Some(index) = actual
            .iter()
            .zip(expected)
            .position(|(actual, expected)| actual != expected)
        {
            panic!(
                "golden differs at byte {index}: actual={:#04x} expected={:#04x}",
                actual[index], expected[index]
            );
        }
    }

    fn encode_inline_sidecar_poll_response(
        ticket: InlineSidecarHostPollTicket,
        result: InlineSidecarHostPollResult,
    ) -> Vec<u8> {
        let (status, variant, outcome_bytes) = match result {
            InlineSidecarHostPollResult::Completed(
                InlineSidecarHostPollOutcome::PacketCredit { .. },
            ) => (Status::Ok, HOT_INLINE_SIDECAR_PACKET_CREDIT_VARIANT, 20),
            InlineSidecarHostPollResult::Completed(InlineSidecarHostPollOutcome::Committed(_)) => (
                Status::Ok,
                HOT_INLINE_SIDECAR_COMMITTED_VARIANT,
                INLINE_SIDECAR_ACK_BYTES,
            ),
            InlineSidecarHostPollResult::Completed(
                InlineSidecarHostPollOutcome::AbortComplete { .. },
            ) => (Status::Ok, HOT_INLINE_SIDECAR_ABORT_COMPLETE_VARIANT, 16),
            InlineSidecarHostPollResult::Rejected(reason) => (
                status_for_reject_reason(reason),
                HOT_INLINE_SIDECAR_VARIANT,
                0,
            ),
        };
        let payload_length = PAYLOAD_PREFIX_BYTES + POLL_TICKET_BYTES + outcome_bytes;
        let mut output = vec![0; v3_wire::HEADER_BYTES + payload_length];
        v3_wire::encode_into(
            FrameKind::Response,
            Header {
                opcode: Opcode::HostPoll,
                status,
                flags: 0,
                correlation_id: ticket.poll_ticket,
            },
            &[],
            &mut output,
        )
        .unwrap();
        output[20..24].copy_from_slice(&(payload_length as u32).to_le_bytes());
        let mut writer = PayloadWriter::new(&mut output[v3_wire::HEADER_BYTES..]);
        write_payload_header(&mut writer, variant, ticket.binding);
        writer.u32(ticket.poll_ticket);
        writer.id128(ticket.offer_id);
        writer.u32(match ticket.phase {
            InlineSidecarHostPollPhase::PacketCredit => 0x0100,
            InlineSidecarHostPollPhase::Commit => 0x0101,
            InlineSidecarHostPollPhase::Abort => 0x0102,
        });
        if let InlineSidecarHostPollResult::Completed(outcome) = result {
            match outcome {
                InlineSidecarHostPollOutcome::PacketCredit {
                    offer_id,
                    next_frame_ordinal,
                } => {
                    writer.id128(offer_id);
                    writer.u32(next_frame_ordinal);
                }
                InlineSidecarHostPollOutcome::Committed(ack) => {
                    write_inline_sidecar_ack(&mut writer, ack)
                }
                InlineSidecarHostPollOutcome::AbortComplete { offer_id } => writer.id128(offer_id),
            }
        }
        assert_eq!(writer.len(), payload_length);
        output
    }

    fn encode_viewport_presentation_poll_response(
        ticket: ViewportPresentationHostPollTicket,
        result: ViewportPresentationHostPollResult,
    ) -> Vec<u8> {
        let (status, variant, outcome_bytes) = match result {
            ViewportPresentationHostPollResult::Completed(
                ViewportPresentationHostPollOutcome::PacketCredit { .. },
            ) => (Status::Ok, VIEWPORT_PRESENTATION_PACKET_CREDIT_VARIANT, 20),
            ViewportPresentationHostPollResult::Completed(
                ViewportPresentationHostPollOutcome::Committed(_),
            ) => (
                Status::Ok,
                VIEWPORT_PRESENTATION_COMMITTED_VARIANT,
                VIEWPORT_PRESENTATION_ACK_BYTES,
            ),
            ViewportPresentationHostPollResult::Completed(
                ViewportPresentationHostPollOutcome::AbortComplete { .. },
            ) => (Status::Ok, VIEWPORT_PRESENTATION_ABORT_COMPLETE_VARIANT, 16),
            ViewportPresentationHostPollResult::Rejected(reason) => (
                status_for_reject_reason(reason),
                VIEWPORT_PRESENTATION_VARIANT,
                0,
            ),
        };
        let payload_length = PAYLOAD_PREFIX_BYTES + POLL_TICKET_BYTES + outcome_bytes;
        let mut output = vec![0; v3_wire::HEADER_BYTES + payload_length];
        v3_wire::encode_into(
            FrameKind::Response,
            Header {
                opcode: Opcode::HostPoll,
                status,
                flags: 0,
                correlation_id: ticket.poll_ticket,
            },
            &[],
            &mut output,
        )
        .unwrap();
        output[20..24].copy_from_slice(&(payload_length as u32).to_le_bytes());
        let mut writer = PayloadWriter::new(&mut output[v3_wire::HEADER_BYTES..]);
        write_payload_header(&mut writer, variant, ticket.binding);
        writer.u32(ticket.poll_ticket);
        writer.id128(ticket.offer_id);
        writer.u32(match ticket.phase {
            ViewportPresentationHostPollPhase::PacketCredit => 0x0200,
            ViewportPresentationHostPollPhase::Commit => 0x0201,
            ViewportPresentationHostPollPhase::Abort => 0x0202,
        });
        if let ViewportPresentationHostPollResult::Completed(outcome) = result {
            match outcome {
                ViewportPresentationHostPollOutcome::PacketCredit {
                    offer_id,
                    next_frame_ordinal,
                } => {
                    writer.id128(offer_id);
                    writer.u32(next_frame_ordinal);
                }
                ViewportPresentationHostPollOutcome::Committed(ack) => {
                    write_viewport_presentation_ack(&mut writer, ack)
                }
                ViewportPresentationHostPollOutcome::AbortComplete { offer_id } => {
                    writer.id128(offer_id)
                }
            }
        }
        assert_eq!(writer.len(), payload_length);
        output
    }

    fn status_for_reject_reason(reason: HostRejectReason) -> Status {
        match reason {
            HostRejectReason::Invalid => Status::Invalid,
            HostRejectReason::Backpressure => Status::Backpressure,
            HostRejectReason::StaleSource => Status::StaleSource,
            HostRejectReason::ExactSourceMismatch => Status::ExactSourceMismatch,
            HostRejectReason::SessionSnapshotRequired => Status::SessionSnapshotRequired,
            HostRejectReason::BaseMismatch => Status::BaseMismatch,
            HostRejectReason::WrongOffer => Status::WrongOffer,
            HostRejectReason::CorruptPublication => Status::CorruptPayload,
            HostRejectReason::QueryBoundExceeded => Status::QueryBoundExceeded,
            HostRejectReason::ForegroundBoundExceeded => Status::ForegroundBoundExceeded,
            HostRejectReason::Superseded => Status::Superseded,
            HostRejectReason::Closed => Status::Closed,
        }
    }

    #[test]
    fn worker_event_encoder_matches_schema_v4_fixed_goldens() {
        let begin_bytes = encode(7, PublicationEventBody::Begin(begin()));
        assert_golden(&begin_bytes, &hex(BEGIN_GOLDEN));

        let commit_bytes = encode(
            9,
            PublicationEventBody::Commit(CommitRequest {
                offer_id: [9, 10, 11, 12],
                actual_frame_count: 2,
                actual_encoded_frame_bytes: 800,
                rolling_transport_digest: digest(40),
                canonical_stream_digest: digest(50),
            }),
        );
        assert_eq!(commit_bytes, hex(COMMIT_GOLDEN));

        let delivery_bytes = encode(12, PublicationEventBody::DeliveryAcknowledged(ack()));
        assert_eq!(delivery_bytes, hex(DELIVERY_GOLDEN));
    }

    #[test]
    fn three_frame_packet_matches_schema_v4_golden_and_borrows_frames() {
        let frames = [
            PublicationPacketFrameInput {
                record_count: 2,
                digest: digest(30),
                bytes: &[0xaa],
            },
            PublicationPacketFrameInput {
                record_count: 3,
                digest: digest(40),
                bytes: &[0xbb, 0xcc],
            },
            PublicationPacketFrameInput {
                record_count: 1,
                digest: digest(50),
                bytes: &[0xdd, 0xee, 0xff],
            },
        ];
        let packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 4,
            first_record_ordinal: 20,
            frames: &frames,
        });
        let packet = decode_publication_packet(&packet_bytes).unwrap();
        let event_bytes = encode(8, PublicationEventBody::Packet(packet));
        assert_golden(&event_bytes, &hex(PACKET_GOLDEN));

        let decoded = decode_event(&event_bytes, binding()).unwrap();
        let PublicationEventBody::Packet(decoded) = decoded.body else {
            panic!("expected packet event");
        };
        assert_eq!(decoded.offer_id, [9, 10, 11, 12]);
        assert_eq!(decoded.first_frame_ordinal, 4);
        assert_eq!(decoded.first_record_ordinal, 20);
        assert_eq!(decoded.frame_count, 3);
        assert_eq!(decoded.aggregate_record_count, 6);
        assert_eq!(decoded.aggregate_frame_bytes, 6);
        assert_eq!(
            decoded.encoded().as_ptr(),
            event_bytes[v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES..].as_ptr()
        );

        let decoded_frames = decoded.frames().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            decoded_frames,
            [
                PublicationPacketFrame {
                    ordinal: 4,
                    first_record_ordinal: 20,
                    record_count: 2,
                    digest: digest(30),
                    bytes: &[0xaa],
                },
                PublicationPacketFrame {
                    ordinal: 5,
                    first_record_ordinal: 22,
                    record_count: 3,
                    digest: digest(40),
                    bytes: &[0xbb, 0xcc],
                },
                PublicationPacketFrame {
                    ordinal: 6,
                    first_record_ordinal: 25,
                    record_count: 1,
                    digest: digest(50),
                    bytes: &[0xdd, 0xee, 0xff],
                },
            ]
        );
    }

    #[test]
    fn packet_regrouping_does_not_change_frame_transport_digest() {
        let bodies: [&[u8]; 3] = [&[0x01, 0x02], &[0x03], &[0x04, 0x05]];
        let kinds = [
            CandidateSnapshotFrameKind::Begin,
            CandidateSnapshotFrameKind::Node,
            CandidateSnapshotFrameKind::End,
        ];
        let records = [0_u32, 1, 0];
        let descriptors = (0..3)
            .map(|index| PublicationPacketFrameInput {
                record_count: records[index],
                digest: protocol_digest128_from_blake3(
                    ProtocolDigestDomain::CandidateFrame,
                    candidate_frame_digest256(index as u32, kinds[index], bodies[index]),
                ),
                bytes: bodies[index],
            })
            .collect::<Vec<_>>();

        let one_packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &descriptors,
        });
        let first_packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &descriptors[..1],
        });
        let second_packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 1,
            first_record_ordinal: 0,
            frames: &descriptors[1..],
        });

        let mut one_packet_digest = CandidateTransportDigest::new();
        for frame in decode_publication_packet(&one_packet_bytes)
            .unwrap()
            .frames()
        {
            let frame = frame.unwrap();
            one_packet_digest
                .push(
                    frame.ordinal,
                    frame.first_record_ordinal,
                    frame.record_count,
                    kinds[frame.ordinal as usize],
                    frame.bytes,
                )
                .unwrap();
        }

        let mut split_packet_digest = CandidateTransportDigest::new();
        for packet_bytes in [&first_packet_bytes, &second_packet_bytes] {
            for frame in decode_publication_packet(packet_bytes).unwrap().frames() {
                let frame = frame.unwrap();
                split_packet_digest
                    .push(
                        frame.ordinal,
                        frame.first_record_ordinal,
                        frame.record_count,
                        kinds[frame.ordinal as usize],
                        frame.bytes,
                    )
                    .unwrap();
            }
        }

        assert_eq!(one_packet_digest.finish(), split_packet_digest.finish());
    }

    #[test]
    fn block_replacement_pages_have_a_distinct_stable_transport_domain_value() {
        assert_eq!(
            CandidateSnapshotFrameKind::SourceFactsReplacementPage as u8,
            4
        );
        assert_eq!(
            CandidateSnapshotFrameKind::BlockSequenceReplacementPage as u8,
            5
        );
        assert_ne!(
            candidate_frame_digest256(
                1,
                CandidateSnapshotFrameKind::SourceFactsReplacementPage,
                b"replacement",
            ),
            candidate_frame_digest256(
                1,
                CandidateSnapshotFrameKind::BlockSequenceReplacementPage,
                b"replacement",
            ),
        );
    }

    #[test]
    fn fixed_events_round_trip_every_non_packet_variant() {
        assert_eq!(
            decode_event(&hex(BEGIN_GOLDEN), binding()).unwrap().body,
            PublicationEventBody::Begin(begin())
        );
        assert_eq!(
            decode_event(&hex(COMMIT_GOLDEN), binding()).unwrap().body,
            PublicationEventBody::Commit(CommitRequest {
                offer_id: [9, 10, 11, 12],
                actual_frame_count: 2,
                actual_encoded_frame_bytes: 800,
                rolling_transport_digest: digest(40),
                canonical_stream_digest: digest(50),
            })
        );
        assert_eq!(
            decode_event(&hex(DELIVERY_GOLDEN), binding()).unwrap().body,
            PublicationEventBody::DeliveryAcknowledged(ack())
        );

        for body in [
            PublicationEventBody::AbortRequested {
                offer_id: [9, 10, 11, 12],
            },
            PublicationEventBody::Failed {
                offer_id: [9, 10, 11, 12],
                failure_code: 0x1020,
            },
        ] {
            let bytes = encode(15, body);
            assert_eq!(decode_event(&bytes, binding()).unwrap().body, body);
        }
    }

    #[test]
    fn host_poll_decoder_matches_live_dart_terminal_goldens() {
        let packet_credit =
            decode_host_poll_command(&hex(PACKET_CREDIT_GOLDEN), binding()).unwrap();
        assert_eq!(packet_credit.correlation_id, 100);
        assert_eq!(packet_credit.ticket.poll_ticket, 100);
        assert_eq!(packet_credit.ticket.offer_id, [9, 10, 11, 12]);
        assert_eq!(packet_credit.ticket.phase, HostPollPhase::PacketCredit);
        assert_eq!(
            packet_credit.result,
            HostPollResult::Completed(HostPollOutcome::PacketCredit {
                offer_id: [9, 10, 11, 12],
                next_frame_ordinal: 6,
            })
        );

        let committed = decode_host_poll_command(&hex(COMMITTED_GOLDEN), binding()).unwrap();
        assert_eq!(committed.ticket.phase, HostPollPhase::Commit);
        assert_eq!(
            committed.result,
            HostPollResult::Completed(HostPollOutcome::Committed(ack()))
        );

        let abort = decode_host_poll_command(&hex(ABORT_COMPLETE_GOLDEN), binding()).unwrap();
        assert_eq!(abort.ticket.phase, HostPollPhase::Abort);
        assert_eq!(
            abort.result,
            HostPollResult::Completed(HostPollOutcome::AbortComplete {
                offer_id: [9, 10, 11, 12],
            })
        );

        let rejection = decode_host_poll_command(&hex(REJECTION_GOLDEN), binding()).unwrap();
        assert_eq!(rejection.ticket.phase, HostPollPhase::Commit);
        assert_eq!(
            rejection.result,
            HostPollResult::Rejected(HostRejectReason::CorruptPublication)
        );
    }

    #[test]
    fn every_dart_rejection_status_maps_and_non_outcomes_stay_unrepresentable() {
        let mappings = [
            (Status::Invalid, HostRejectReason::Invalid),
            (Status::Backpressure, HostRejectReason::Backpressure),
            (Status::StaleSource, HostRejectReason::StaleSource),
            (
                Status::ExactSourceMismatch,
                HostRejectReason::ExactSourceMismatch,
            ),
            (
                Status::SessionSnapshotRequired,
                HostRejectReason::SessionSnapshotRequired,
            ),
            (Status::BaseMismatch, HostRejectReason::BaseMismatch),
            (Status::WrongOffer, HostRejectReason::WrongOffer),
            (Status::CorruptPayload, HostRejectReason::CorruptPublication),
            (
                Status::QueryBoundExceeded,
                HostRejectReason::QueryBoundExceeded,
            ),
            (
                Status::ForegroundBoundExceeded,
                HostRejectReason::ForegroundBoundExceeded,
            ),
            (Status::Superseded, HostRejectReason::Superseded),
            (Status::Closed, HostRejectReason::Closed),
        ];
        for (status, expected) in mappings {
            let mut bytes = hex(REJECTION_GOLDEN);
            bytes[10..12].copy_from_slice(&status.code().to_le_bytes());
            let decoded = decode_host_poll_command(&bytes, binding()).unwrap();
            assert_eq!(decoded.result, HostPollResult::Rejected(expected));
        }

        let mut pending_variant = hex(REJECTION_GOLDEN);
        pending_variant[10..12].copy_from_slice(&Status::Ok.code().to_le_bytes());
        assert_failure(
            decode_host_poll_command(&pending_variant, binding()),
            DecodeFailure::UnknownVariant,
        );
        let mut unmapped = hex(REJECTION_GOLDEN);
        unmapped[10..12].copy_from_slice(&Status::NotReady.code().to_le_bytes());
        assert_failure(
            decode_host_poll_command(&unmapped, binding()),
            DecodeFailure::UnmappedStatus,
        );
    }

    #[test]
    fn crossed_endpoint_and_document_identities_fail_closed() {
        let event = hex(BEGIN_GOLDEN);
        for crossed in [
            SessionBinding {
                document_session: [9, 2, 3, 4],
                ..binding()
            },
            SessionBinding {
                source_session_identity: 9,
                ..binding()
            },
            SessionBinding {
                worker_generation: 9,
                ..binding()
            },
        ] {
            assert_failure(
                decode_event(&event, crossed),
                DecodeFailure::IdentityMismatch,
            );
            assert_failure(
                decode_host_poll_command(&hex(COMMITTED_GOLDEN), crossed),
                DecodeFailure::IdentityMismatch,
            );
        }

        let mut crossed_source_document = event;
        crossed_source_document[92..96].copy_from_slice(&9_u32.to_le_bytes());
        assert_failure(
            decode_event(&crossed_source_document, binding()),
            DecodeFailure::IdentityMismatch,
        );

        let mut crossed_ack_document = hex(COMMITTED_GOLDEN);
        crossed_ack_document[100..104].copy_from_slice(&9_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&crossed_ack_document, binding()),
            DecodeFailure::IdentityMismatch,
        );
    }

    #[test]
    fn causal_ticket_requires_exact_correlation_phase_offer_and_outcome() {
        let mut crossed_correlation = hex(PACKET_CREDIT_GOLDEN);
        crossed_correlation[16..20].copy_from_slice(&101_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&crossed_correlation, binding()),
            DecodeFailure::CorrelationMismatch,
        );

        let mut zero_ticket = hex(PACKET_CREDIT_GOLDEN);
        zero_ticket[52..56].copy_from_slice(&0_u32.to_le_bytes());
        zero_ticket[16..20].copy_from_slice(&0_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&zero_ticket, binding()),
            DecodeFailure::InvalidValue,
        );

        let mut wrong_phase = hex(PACKET_CREDIT_GOLDEN);
        wrong_phase[72..76].copy_from_slice(&2_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&wrong_phase, binding()),
            DecodeFailure::InvalidValue,
        );

        let mut wrong_offer = hex(PACKET_CREDIT_GOLDEN);
        wrong_offer[76..80].copy_from_slice(&13_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&wrong_offer, binding()),
            DecodeFailure::InvalidValue,
        );

        let mut zero_credit = hex(PACKET_CREDIT_GOLDEN);
        zero_credit[92..96].copy_from_slice(&0_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&zero_credit, binding()),
            DecodeFailure::InvalidValue,
        );

        let mut wrong_abort_offer = hex(ABORT_COMPLETE_GOLDEN);
        wrong_abort_offer[76..80].copy_from_slice(&13_u32.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&wrong_abort_offer, binding()),
            DecodeFailure::InvalidValue,
        );
    }

    #[test]
    fn variants_statuses_and_receipt_opcode_are_strict() {
        let mut event_variant = hex(COMMIT_GOLDEN);
        event_variant[26..28].copy_from_slice(&2_u16.to_le_bytes());
        assert_failure(
            decode_event(&event_variant, binding()),
            DecodeFailure::UnknownVariant,
        );

        let mut rejected_variant = hex(REJECTION_GOLDEN);
        rejected_variant[26..28].copy_from_slice(&1_u16.to_le_bytes());
        assert_failure(
            decode_host_poll_command(&rejected_variant, binding()),
            DecodeFailure::UnknownVariant,
        );

        let mut receipt_opcode = hex(REJECTION_GOLDEN);
        receipt_opcode[8..10].copy_from_slice(&Opcode::ParserAcknowledge.code().to_le_bytes());
        assert_failure(
            decode_host_poll_command(&receipt_opcode, binding()),
            DecodeFailure::UnexpectedOpcode,
        );
    }

    #[test]
    fn schema_correlation_and_binding_prefix_are_canonical() {
        let event = hex(COMMIT_GOLDEN);

        let mut schema = event.clone();
        schema[24..26].copy_from_slice(&2_u16.to_le_bytes());
        assert_failure(
            decode_event(&schema, binding()),
            DecodeFailure::UnsupportedSchema,
        );

        let mut zero_correlation = event.clone();
        zero_correlation[16..20].copy_from_slice(&0_u32.to_le_bytes());
        assert_failure(
            decode_event(&zero_correlation, binding()),
            DecodeFailure::InvalidValue,
        );

        for range in [28..32, 48..52] {
            let mut zero_identity = event.clone();
            zero_identity[range].copy_from_slice(&0_u32.to_le_bytes());
            assert_failure(
                decode_event(&zero_identity, binding()),
                DecodeFailure::InvalidValue,
            );
        }

        let mut unknown_document = event;
        unknown_document[32..48].fill(0);
        assert_failure(
            decode_event(&unknown_document, binding()),
            DecodeFailure::InvalidValue,
        );
    }

    #[test]
    fn caller_owned_event_encoding_requires_exact_capacity() {
        let body = PublicationEventBody::AbortRequested {
            offer_id: [9, 10, 11, 12],
        };
        let required = v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + 16;
        let mut exact = vec![0; required];
        assert_eq!(
            encode_event_into(
                PublicationEvent {
                    event_id: 1,
                    binding: binding(),
                    body,
                },
                binding(),
                &mut exact,
            ),
            Ok(required)
        );
        let mut short = vec![0; required - 1];
        assert_eq!(
            encode_event_into(
                PublicationEvent {
                    event_id: 1,
                    binding: binding(),
                    body,
                },
                binding(),
                &mut short,
            ),
            Err(EncodeError::Envelope(
                v3_wire::EncodeError::BufferTooSmall {
                    required,
                    available: required - 1,
                }
            ))
        );
    }

    #[test]
    fn fixed_payloads_require_exact_exhaustion() {
        let commit = hex(COMMIT_GOLDEN);
        assert_failure(
            decode_event(&resize_payload(&commit, -1), binding()),
            DecodeFailure::TruncatedPayload,
        );
        assert_failure(
            decode_event(&resize_payload(&commit, 1), binding()),
            DecodeFailure::TrailingPayload,
        );

        let rejection = hex(REJECTION_GOLDEN);
        assert_failure(
            decode_host_poll_command(&resize_payload(&rejection, -1), binding()),
            DecodeFailure::TruncatedPayload,
        );
        assert_failure(
            decode_host_poll_command(&resize_payload(&rejection, 1), binding()),
            DecodeFailure::TrailingPayload,
        );
    }

    #[test]
    fn packet_envelope_accepts_exact_product_maximum() {
        let body = vec![0xa5; MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize];
        let frame_length = body.len() / MAXIMUM_PACKET_FRAME_COUNT as usize;
        let frames = (0..MAXIMUM_PACKET_FRAME_COUNT as usize)
            .map(|index| PublicationPacketFrameInput {
                record_count: 0,
                digest: [0; 4],
                bytes: &body[index * frame_length..(index + 1) * frame_length],
            })
            .collect::<Vec<_>>();
        let encoded = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &frames,
        });
        assert_eq!(encoded.len(), MAXIMUM_PACKET_ENCODED_BYTES);
        assert!(
            encoded.len() + PAYLOAD_PREFIX_BYTES <= v3_wire::MAXIMUM_PAYLOAD_BYTES,
            "product packet envelope must remain inside FLK3"
        );
        let packet = decode_publication_packet(&encoded).unwrap();
        assert_eq!(packet.frame_count, MAXIMUM_PACKET_FRAME_COUNT);
        assert_eq!(
            packet.aggregate_frame_bytes,
            MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
        );
        assert_eq!(packet.frames().count(), frames.len());
    }

    #[test]
    fn packet_encoder_rejects_zero_excess_and_overflow_inputs() {
        let mut output = vec![0; MAXIMUM_PACKET_ENCODED_BYTES + 1];
        assert_eq!(
            encode_publication_packet_into(
                PublicationPacketInput {
                    offer_id: [9, 10, 11, 12],
                    first_frame_ordinal: 0,
                    first_record_ordinal: 0,
                    frames: &[],
                },
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        );
        assert_eq!(
            encode_publication_packet_into(
                PublicationPacketInput {
                    offer_id: [9, 10, 11, 12],
                    first_frame_ordinal: 0,
                    first_record_ordinal: 0,
                    frames: &[PublicationPacketFrameInput {
                        record_count: 0,
                        digest: [0; 4],
                        bytes: &[],
                    }],
                },
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        );

        let frame = PublicationPacketFrameInput {
            record_count: 0,
            digest: [0; 4],
            bytes: &[0xaa],
        };
        let excess = vec![frame; MAXIMUM_PACKET_FRAME_COUNT as usize + 1];
        assert_eq!(
            encode_publication_packet_into(
                PublicationPacketInput {
                    offer_id: [9, 10, 11, 12],
                    first_frame_ordinal: 0,
                    first_record_ordinal: 0,
                    frames: &excess,
                },
                &mut output,
            ),
            Err(EncodeError::PayloadTooLarge)
        );

        let frames = [frame];
        for input in [
            PublicationPacketInput {
                offer_id: [9, 10, 11, 12],
                first_frame_ordinal: u32::MAX,
                first_record_ordinal: 0,
                frames: &frames,
            },
            PublicationPacketInput {
                offer_id: [9, 10, 11, 12],
                first_frame_ordinal: 0,
                first_record_ordinal: u32::MAX,
                frames: &[PublicationPacketFrameInput {
                    record_count: 1,
                    ..frame
                }],
            },
        ] {
            assert_eq!(
                encode_publication_packet_into(input, &mut output),
                Err(EncodeError::InvalidValue)
            );
        }

        let oversized_body = vec![0; MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize + 1];
        assert_eq!(
            encode_publication_packet_into(
                PublicationPacketInput {
                    offer_id: [9, 10, 11, 12],
                    first_frame_ordinal: 0,
                    first_record_ordinal: 0,
                    frames: &[PublicationPacketFrameInput {
                        bytes: &oversized_body,
                        ..frame
                    }],
                },
                &mut output,
            ),
            Err(EncodeError::PayloadTooLarge)
        );
    }

    #[test]
    fn packet_decoder_rejects_bad_envelope_and_descriptor_aggregates() {
        let frames = [
            PublicationPacketFrameInput {
                record_count: 2,
                digest: digest(30),
                bytes: &[0xaa],
            },
            PublicationPacketFrameInput {
                record_count: 3,
                digest: digest(40),
                bytes: &[0xbb, 0xcc],
            },
            PublicationPacketFrameInput {
                record_count: 1,
                digest: digest(50),
                bytes: &[0xdd, 0xee, 0xff],
            },
        ];
        let valid = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 4,
            first_record_ordinal: 20,
            frames: &frames,
        });

        let mut bad_magic = valid.clone();
        bad_magic[0] ^= 0xff;
        assert_failure(
            decode_publication_packet(&bad_magic),
            DecodeFailure::InvalidValue,
        );
        let mut bad_schema = valid.clone();
        bad_schema[4..6].copy_from_slice(&(PACKET_SCHEMA + 1).to_le_bytes());
        assert_failure(
            decode_publication_packet(&bad_schema),
            DecodeFailure::UnsupportedSchema,
        );
        let mut bad_flags = valid.clone();
        bad_flags[6..8].copy_from_slice(&1_u16.to_le_bytes());
        assert_failure(
            decode_publication_packet(&bad_flags),
            DecodeFailure::InvalidValue,
        );

        let mut zero_frames = valid.clone();
        zero_frames[32..36].copy_from_slice(&0_u32.to_le_bytes());
        assert_failure(
            decode_publication_packet(&zero_frames),
            DecodeFailure::InvalidValue,
        );

        let mut excess_frames = valid.clone();
        excess_frames[32..36].copy_from_slice(&(MAXIMUM_PACKET_FRAME_COUNT + 1).to_le_bytes());
        assert_failure(
            decode_publication_packet(&excess_frames),
            DecodeFailure::OversizedValue,
        );

        let mut oversized = valid.clone();
        oversized[40..44]
            .copy_from_slice(&(MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES + 1).to_le_bytes());
        assert_failure(
            decode_publication_packet(&oversized),
            DecodeFailure::OversizedValue,
        );

        assert_failure(
            decode_publication_packet(&valid[..valid.len() - 1]),
            DecodeFailure::TruncatedPayload,
        );
        let mut trailing = valid.clone();
        trailing.push(0);
        assert_failure(
            decode_publication_packet(&trailing),
            DecodeFailure::TrailingPayload,
        );

        let mut frame_overflow = valid.clone();
        frame_overflow[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_failure(
            decode_publication_packet(&frame_overflow),
            DecodeFailure::InvalidValue,
        );
        let mut record_overflow = valid.clone();
        record_overflow[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_failure(
            decode_publication_packet(&record_overflow),
            DecodeFailure::InvalidValue,
        );

        let mut short_body_sum = valid.clone();
        short_body_sum[44..48].copy_from_slice(&0_u32.to_le_bytes());
        assert_failure(
            decode_publication_packet(&short_body_sum),
            DecodeFailure::InvalidValue,
        );
        let mut long_body_sum = valid.clone();
        long_body_sum[44..48].copy_from_slice(&2_u32.to_le_bytes());
        assert_failure(
            decode_publication_packet(&long_body_sum),
            DecodeFailure::TruncatedPayload,
        );
        let mut bad_record_sum = valid;
        bad_record_sum[48..52].copy_from_slice(&3_u32.to_le_bytes());
        assert_failure(
            decode_publication_packet(&bad_record_sum),
            DecodeFailure::InvalidValue,
        );
    }

    #[test]
    fn schema_v4_offer_limits_are_hard_bounded() {
        let mut oversized_advertisement = hex(BEGIN_GOLDEN);
        oversized_advertisement[184..188]
            .copy_from_slice(&(PRODUCT_MAX_PACKET_BYTES + 1).to_le_bytes());
        assert_failure(
            decode_event(&oversized_advertisement, binding()),
            DecodeFailure::OversizedValue,
        );

        let mut oversized_frame = hex(BEGIN_GOLDEN);
        oversized_frame[188..192].copy_from_slice(&(PRODUCT_MAX_FRAME_BYTES + 1).to_le_bytes());
        assert_failure(
            decode_event(&oversized_frame, binding()),
            DecodeFailure::OversizedValue,
        );
    }

    #[test]
    fn full_and_delta_begin_invariants_match_the_dart_value_model() {
        let mut delta = begin();
        delta.mode = PublicationMode::ExactBaseReferencesDelta;
        delta.base_ack = Some(ack());
        delta.publication_session = [15, 16, 17, 18];
        delta.target_record_count = 4;
        let encoded = encode(20, PublicationEventBody::Begin(delta));
        assert_eq!(
            decode_event(&encoded, binding()).unwrap().body,
            PublicationEventBody::Begin(delta)
        );

        let mut invalid = delta;
        invalid.base_ack.as_mut().unwrap().publication_session = invalid.publication_session;
        let mut output = vec![0; 512];
        assert_eq!(
            encode_event_into(
                PublicationEvent {
                    event_id: 21,
                    binding: binding(),
                    body: PublicationEventBody::Begin(invalid),
                },
                binding(),
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        );
    }

    #[test]
    fn hot_inline_authoritative_flow_round_trips_without_structural_aliasing() {
        let begin = authoritative_hot_inline_begin();
        let mut block_quote_begin = begin;
        block_quote_begin.envelope.ipr2_descriptor_bytes = BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES;
        let block_quote_bytes =
            encode_hot_inline(29, HotInlineSidecarEventBody::Begin(block_quote_begin));
        assert_eq!(
            decode_hot_inline_sidecar_event(&block_quote_bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::Begin(block_quote_begin)
        );
        let mut unknown_descriptor = begin;
        unknown_descriptor.envelope.ipr2_descriptor_bytes =
            IPR3_DESCRIPTOR_BYTES.checked_add(1).expect("test width");
        let mut scratch = vec![0; 1024];
        assert_eq!(
            encode_hot_inline_sidecar_event_into(
                HotInlineSidecarEvent {
                    event_id: 29,
                    binding: binding(),
                    body: HotInlineSidecarEventBody::Begin(unknown_descriptor),
                },
                binding(),
                &mut scratch,
            ),
            Err(EncodeError::InvalidValue)
        );
        let begin_bytes = encode_hot_inline(30, HotInlineSidecarEventBody::Begin(begin));
        assert_eq!(
            begin_bytes.len(),
            v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + HOT_INLINE_SIDECAR_BEGIN_BYTES
        );
        assert_eq!(
            decode_hot_inline_sidecar_event(&begin_bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::Begin(begin)
        );
        assert_failure(
            decode_event(&begin_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );

        let commit = HotInlineSidecarCommitRequest {
            offer_id: begin.offer_id,
            actual_frame_count: 4,
            actual_encoded_frame_bytes: 900,
            rolling_transport_digest: digest(100),
            root_stream_digest: digest(110),
        };
        let commit_bytes = encode_hot_inline(31, HotInlineSidecarEventBody::Commit(commit));
        assert_eq!(
            decode_hot_inline_sidecar_event(&commit_bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::Commit(commit)
        );
        assert_failure(
            decode_event(&commit_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );

        let sidecar_ack = inline_sidecar_ack(InlineSidecarAckDisposition::Authoritative, 2);
        let ack_bytes = encode_hot_inline(
            32,
            HotInlineSidecarEventBody::DeliveryAcknowledged(sidecar_ack),
        );
        assert_eq!(
            decode_hot_inline_sidecar_event(&ack_bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::DeliveryAcknowledged(sidecar_ack)
        );
        assert_failure(
            decode_event(&ack_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );

        assert_eq!(begin.require_exact_base(ack()), Ok(()));
        let mut wrong_base = ack();
        wrong_base.manifest_digest[0] ^= 1;
        assert_eq!(
            begin.require_exact_base(wrong_base),
            Err(HostRejectReason::BaseMismatch)
        );
    }

    #[test]
    fn hot_inline_link_value_metrics_round_trip_and_fail_closed() {
        let mut begin = authoritative_hot_inline_begin();
        begin.envelope.transferred_node_count = 3;
        let HotInlineSidecarDisposition::Authoritative {
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ..
        } = &mut begin.envelope.disposition
        else {
            unreachable!("authoritative fixture")
        };
        *link_value_entry_count = 1;
        *link_value_encoded_bytes = 49;
        *link_value_storage_page_count = 1;

        let bytes = encode_hot_inline(33, HotInlineSidecarEventBody::Begin(begin));
        assert_eq!(
            decode_hot_inline_sidecar_event(&bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::Begin(begin)
        );

        let mut invalid = begin;
        let HotInlineSidecarDisposition::Authoritative {
            link_value_encoded_bytes,
            ..
        } = &mut invalid.envelope.disposition
        else {
            unreachable!("authoritative fixture")
        };
        *link_value_encoded_bytes = 0;
        let mut output = vec![0; 1024];
        assert_eq!(
            encode_hot_inline_sidecar_event_into(
                HotInlineSidecarEvent {
                    event_id: 34,
                    binding: binding(),
                    body: HotInlineSidecarEventBody::Begin(invalid),
                },
                binding(),
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        );
    }

    #[test]
    fn hot_inline_unsupported_is_terminal_and_cannot_claim_a_root() {
        let begin = unsupported_hot_inline_begin();
        let bytes = encode_hot_inline(33, HotInlineSidecarEventBody::Begin(begin));
        assert_eq!(
            decode_hot_inline_sidecar_event(&bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::Begin(begin)
        );

        let mut invalid = begin;
        invalid.envelope.transferred_node_count = 0;
        let mut output = vec![0; 1024];
        assert_eq!(
            encode_hot_inline_sidecar_event_into(
                HotInlineSidecarEvent {
                    event_id: 34,
                    binding: binding(),
                    body: HotInlineSidecarEventBody::Begin(invalid),
                },
                binding(),
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        );

        let mut invalid = begin;
        invalid.envelope.ipr2_descriptor_bytes = IPR3_DESCRIPTOR_BYTES;
        assert_eq!(
            encode_hot_inline_sidecar_event_into(
                HotInlineSidecarEvent {
                    event_id: 35,
                    binding: binding(),
                    body: HotInlineSidecarEventBody::Begin(invalid),
                },
                binding(),
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        );

        let terminal_frames = [
            PublicationPacketFrameInput {
                record_count: 0,
                digest: digest(120),
                bytes: &[0xe6, 1, 0, 0],
            },
            PublicationPacketFrameInput {
                record_count: 1,
                digest: digest(130),
                bytes: &[0xe1, b'u', b'n', b's', b'u', b'p'],
            },
            PublicationPacketFrameInput {
                record_count: 0,
                digest: digest(140),
                bytes: &[0xe2, 0, 0, 0],
            },
        ];
        let packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: begin.offer_id,
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &terminal_frames,
        });
        let packet = decode_publication_packet(&packet_bytes).unwrap();
        let packet_event = encode_hot_inline(36, HotInlineSidecarEventBody::Packet(packet));
        let HotInlineSidecarEventBody::Packet(decoded) =
            decode_hot_inline_sidecar_event(&packet_event, binding())
                .unwrap()
                .body
        else {
            panic!("expected unsupported terminal packet");
        };
        assert_eq!(decoded.frame_count, 3);
        assert_eq!(decoded.aggregate_record_count, 1);

        let unsupported_ack = inline_sidecar_ack(InlineSidecarAckDisposition::Unsupported, 1);
        let ack_bytes = encode_hot_inline(
            37,
            HotInlineSidecarEventBody::DeliveryAcknowledged(unsupported_ack),
        );
        assert_eq!(
            decode_hot_inline_sidecar_event(&ack_bytes, binding())
                .unwrap()
                .body,
            HotInlineSidecarEventBody::DeliveryAcknowledged(unsupported_ack)
        );
    }

    #[test]
    fn hot_inline_begin_tamper_wrong_base_and_limits_fail_closed() {
        let begin = authoritative_hot_inline_begin();
        let bytes = encode_hot_inline(38, HotInlineSidecarEventBody::Begin(begin));

        let mut structural_variant = bytes.clone();
        structural_variant[26..28].copy_from_slice(&0_u16.to_le_bytes());
        assert_failure(
            decode_hot_inline_sidecar_event(&structural_variant, binding()),
            DecodeFailure::UnknownVariant,
        );

        let mut unknown_mode = bytes.clone();
        let mode_offset = v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + 4;
        unknown_mode[mode_offset..mode_offset + 4].fill(0);
        assert_failure(
            decode_hot_inline_sidecar_event(&unknown_mode, binding()),
            DecodeFailure::InvalidValue,
        );

        // Base manifest digest is part of the complete embedded ACK. It can be
        // decoded as a different, internally valid base, but exact admission
        // rejects it against the installed ACK.
        let base_manifest_offset =
            v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + 40 + STRUCTURAL_ACK_BYTES - 16;
        let mut wrong_base_bytes = bytes.clone();
        wrong_base_bytes[base_manifest_offset] ^= 1;
        let HotInlineSidecarEventBody::Begin(wrong_base) =
            decode_hot_inline_sidecar_event(&wrong_base_bytes, binding())
                .unwrap()
                .body
        else {
            panic!("expected sidecar Begin");
        };
        assert_eq!(
            wrong_base.require_exact_base(ack()),
            Err(HostRejectReason::BaseMismatch)
        );

        let mut zero_generation = bytes.clone();
        let generation_offset =
            v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + 40 + STRUCTURAL_ACK_BYTES + 8;
        zero_generation[generation_offset..generation_offset + 8].fill(0);
        assert_failure(
            decode_hot_inline_sidecar_event(&zero_generation, binding()),
            DecodeFailure::InvalidValue,
        );

        let mut oversized_frame_limit = bytes;
        let limits_offset =
            v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + HOT_INLINE_SIDECAR_BEGIN_BYTES - 20;
        oversized_frame_limit[limits_offset + 12..limits_offset + 16]
            .copy_from_slice(&(PRODUCT_MAX_FRAME_BYTES + 1).to_le_bytes());
        assert_failure(
            decode_hot_inline_sidecar_event(&oversized_frame_limit, binding()),
            DecodeFailure::OversizedValue,
        );
    }

    #[test]
    fn hot_inline_root_frames_reuse_fpk3_but_have_a_separate_transport_domain() {
        let bodies: [&[u8]; 4] = [
            &[0xe6, 1, 0, 0],
            &[0xe1, 0xaa],
            &[0xe1, 0xbb],
            &[0xe2, 0, 0, 0],
        ];
        let kinds = [
            HotInlineSidecarFrameKind::Begin,
            HotInlineSidecarFrameKind::Node,
            HotInlineSidecarFrameKind::Node,
            HotInlineSidecarFrameKind::End,
        ];
        let record_counts = [0, 1, 1, 0];
        let frames = (0..bodies.len())
            .map(|index| PublicationPacketFrameInput {
                record_count: record_counts[index],
                digest: protocol_digest128_from_blake3(
                    ProtocolDigestDomain::HotInlineSidecarFrame,
                    hot_inline_sidecar_frame_digest256(index as u32, kinds[index], bodies[index]),
                ),
                bytes: bodies[index],
            })
            .collect::<Vec<_>>();
        let packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: [9, 10, 11, 12],
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &frames,
        });
        let packet = decode_publication_packet(&packet_bytes).unwrap();
        let event_bytes = encode_hot_inline(39, HotInlineSidecarEventBody::Packet(packet));
        let HotInlineSidecarEventBody::Packet(decoded) =
            decode_hot_inline_sidecar_event(&event_bytes, binding())
                .unwrap()
                .body
        else {
            panic!("expected sidecar packet");
        };
        assert_eq!(decoded.aggregate_record_count, 2);

        let mut digest_accumulator = HotInlineSidecarTransportDigest::new();
        for frame in decoded.frames() {
            let frame = frame.unwrap();
            let computed = digest_accumulator
                .push(
                    frame.ordinal,
                    frame.first_record_ordinal,
                    frame.record_count,
                    kinds[frame.ordinal as usize],
                    frame.bytes,
                )
                .unwrap();
            assert_eq!(
                frame.digest,
                protocol_digest128_from_blake3(
                    ProtocolDigestDomain::HotInlineSidecarFrame,
                    computed,
                )
            );
        }
        let receipt = digest_accumulator.finish();
        assert_eq!(receipt.frame_count, 4);
        let mut structural_transport = CandidateTransportDigest::new();
        for (ordinal, bytes) in bodies.iter().enumerate() {
            let kind = match kinds[ordinal] {
                HotInlineSidecarFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
                HotInlineSidecarFrameKind::Node => CandidateSnapshotFrameKind::Node,
                HotInlineSidecarFrameKind::End => CandidateSnapshotFrameKind::End,
            };
            structural_transport
                .push(
                    ordinal as u32,
                    record_counts[..ordinal].iter().sum(),
                    record_counts[ordinal],
                    kind,
                    bytes,
                )
                .unwrap();
        }
        assert_ne!(receipt.digest256, structural_transport.finish().digest256);

        let structural_frame =
            candidate_frame_digest256(0, CandidateSnapshotFrameKind::Begin, bodies[0]);
        assert_ne!(
            hot_inline_sidecar_frame_digest256(0, HotInlineSidecarFrameKind::Begin, bodies[0]),
            structural_frame
        );

        let mut tampered = packet_bytes;
        *tampered.last_mut().unwrap() ^= 1;
        let tampered_packet = decode_publication_packet(&tampered).unwrap();
        let last = tampered_packet.frames().last().unwrap().unwrap();
        let computed = hot_inline_sidecar_frame_digest256(
            last.ordinal,
            HotInlineSidecarFrameKind::End,
            last.bytes,
        );
        assert_ne!(
            last.digest,
            protocol_digest128_from_blake3(ProtocolDigestDomain::HotInlineSidecarFrame, computed,)
        );
    }

    #[test]
    fn hot_inline_poll_ack_is_distinct_and_never_decodes_as_structural() {
        let ticket = InlineSidecarHostPollTicket {
            binding: binding(),
            poll_ticket: 200,
            offer_id: [9, 10, 11, 12],
            phase: InlineSidecarHostPollPhase::Commit,
        };
        let sidecar_ack = inline_sidecar_ack(InlineSidecarAckDisposition::Authoritative, 2);
        let response = encode_inline_sidecar_poll_response(
            ticket,
            InlineSidecarHostPollResult::Completed(InlineSidecarHostPollOutcome::Committed(
                sidecar_ack,
            )),
        );
        assert_eq!(
            decode_inline_sidecar_host_poll_command(&response, binding())
                .unwrap()
                .result,
            InlineSidecarHostPollResult::Completed(InlineSidecarHostPollOutcome::Committed(
                sidecar_ack
            ))
        );
        assert_failure(
            decode_host_poll_command(&response, binding()),
            DecodeFailure::InvalidValue,
        );

        let rejected = encode_inline_sidecar_poll_response(
            ticket,
            InlineSidecarHostPollResult::Rejected(HostRejectReason::BaseMismatch),
        );
        assert_eq!(
            decode_inline_sidecar_host_poll_command(&rejected, binding())
                .unwrap()
                .result,
            InlineSidecarHostPollResult::Rejected(HostRejectReason::BaseMismatch)
        );
        assert_failure(
            decode_host_poll_command(&rejected, binding()),
            DecodeFailure::InvalidValue,
        );
    }

    #[test]
    fn viewport_presentation_flow_round_trips_without_structural_or_hio1_aliasing() {
        let viewport_begin = viewport_presentation_begin();
        let begin_bytes =
            encode_viewport(210, ViewportPresentationEventBody::Begin(viewport_begin));
        assert_eq!(
            begin_bytes.len(),
            v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + VIEWPORT_PRESENTATION_BEGIN_BYTES
        );
        assert_eq!(
            decode_viewport_presentation_event(&begin_bytes, binding())
                .unwrap()
                .body,
            ViewportPresentationEventBody::Begin(viewport_begin)
        );
        assert_failure(
            decode_event(&begin_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );
        assert_failure(
            decode_hot_inline_sidecar_event(&begin_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );

        let structural = encode(211, PublicationEventBody::Begin(begin()));
        assert_failure(
            decode_viewport_presentation_event(&structural, binding()),
            DecodeFailure::UnknownVariant,
        );
        let hio1 = encode_hot_inline(
            212,
            HotInlineSidecarEventBody::Begin(authoritative_hot_inline_begin()),
        );
        assert_failure(
            decode_viewport_presentation_event(&hio1, binding()),
            DecodeFailure::UnknownVariant,
        );

        let commit = ViewportPresentationCommitRequest {
            offer_id: viewport_begin.offer_id,
            actual_frame_count: 11,
            actual_encoded_frame_bytes: 960,
            rolling_transport_digest: digest(140),
            aggregate_root_stream_digest: digest(150),
        };
        let commit_bytes = encode_viewport(213, ViewportPresentationEventBody::Commit(commit));
        assert_eq!(
            decode_viewport_presentation_event(&commit_bytes, binding())
                .unwrap()
                .body,
            ViewportPresentationEventBody::Commit(commit)
        );

        for body in [
            ViewportPresentationEventBody::AbortRequested {
                offer_id: viewport_begin.offer_id,
            },
            ViewportPresentationEventBody::Failed {
                offer_id: viewport_begin.offer_id,
                failure_code: 0x3000_0001,
            },
        ] {
            let bytes = encode_viewport(214, body);
            assert_eq!(
                decode_viewport_presentation_event(&bytes, binding())
                    .unwrap()
                    .body,
                body
            );
        }

        let viewport_ack = viewport_presentation_ack();
        let ack_bytes = encode_viewport(
            215,
            ViewportPresentationEventBody::DeliveryAcknowledged(viewport_ack),
        );
        assert_eq!(
            ack_bytes.len(),
            v3_wire::HEADER_BYTES + PAYLOAD_PREFIX_BYTES + VIEWPORT_PRESENTATION_ACK_BYTES
        );
        assert_eq!(
            decode_viewport_presentation_event(&ack_bytes, binding())
                .unwrap()
                .body,
            ViewportPresentationEventBody::DeliveryAcknowledged(viewport_ack)
        );
        assert_eq!(viewport_begin.require_exact_base(ack()), Ok(()));
        let mut wrong_base = ack();
        wrong_base.sequence_digest[0] ^= 1;
        assert_eq!(
            viewport_begin.require_exact_base(wrong_base),
            Err(HostRejectReason::BaseMismatch)
        );
    }

    #[test]
    fn viewport_presentation_geometry_totals_and_limits_fail_closed() {
        let mut partial = viewport_presentation_begin();
        partial.binding.covered_range.end_utf8 = 5;
        partial.binding.covered_range.end_utf16 = 4;
        partial.binding.next.block_ordinal = 9;
        partial.binding.next.utf8_offset = 5;
        partial.binding.next.utf16_offset = 4;
        partial.binding.complete = false;
        partial.envelope.visited_structural_entries = 2;
        let partial_bytes = encode_viewport(216, ViewportPresentationEventBody::Begin(partial));
        assert_eq!(
            decode_viewport_presentation_event(&partial_bytes, binding())
                .unwrap()
                .body,
            ViewportPresentationEventBody::Begin(partial)
        );

        let mut output = vec![0; 1024];
        let encode_begin = |candidate, output: &mut [u8]| {
            encode_viewport_presentation_event_into(
                ViewportPresentationEvent {
                    event_id: 217,
                    binding: binding(),
                    body: ViewportPresentationEventBody::Begin(candidate),
                },
                binding(),
                output,
            )
        };

        let mut invalid = partial;
        invalid.binding.complete = true;
        assert_eq!(
            encode_begin(invalid, &mut output),
            Err(EncodeError::InvalidValue)
        );

        let mut invalid = partial;
        invalid.binding.next.block_ordinal += 1;
        assert_eq!(
            encode_begin(invalid, &mut output),
            Err(EncodeError::InvalidValue)
        );

        let mut invalid = partial;
        invalid.envelope.fact_count = invalid.query_limits.maximum_fact_records + 1;
        assert_eq!(
            encode_begin(invalid, &mut output),
            Err(EncodeError::InvalidValue)
        );

        let mut oversized = partial;
        oversized.query_limits.maximum_structural_entries =
            crate::v3_session_wire::MAXIMUM_VIEWPORT_STRUCTURAL_ENTRIES + 1;
        assert_eq!(
            encode_begin(oversized, &mut output),
            Err(EncodeError::PayloadTooLarge)
        );
    }

    #[test]
    fn viewport_presentation_fpk3_order_and_digest_domains_are_authenticated() {
        let bodies: [&[u8]; 4] = [
            b"VPB1",
            b"ordered-directory",
            b"opaque-HIO1-child",
            b"VPB1-end",
        ];
        let kinds = [
            ViewportPresentationFrameKind::Begin,
            ViewportPresentationFrameKind::Directory,
            ViewportPresentationFrameKind::Child,
            ViewportPresentationFrameKind::End,
        ];
        let record_counts = [0_u32, 2, 1, 0];
        let frames = (0..bodies.len())
            .map(|index| PublicationPacketFrameInput {
                record_count: record_counts[index],
                digest: protocol_digest128_from_blake3(
                    ProtocolDigestDomain::ViewportPresentationFrame,
                    viewport_presentation_frame_digest256(
                        index as u32,
                        kinds[index],
                        bodies[index],
                    ),
                ),
                bytes: bodies[index],
            })
            .collect::<Vec<_>>();
        let packet_bytes = encode_packet(PublicationPacketInput {
            offer_id: viewport_presentation_begin().offer_id,
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &frames,
        });
        let packet = decode_publication_packet(&packet_bytes).unwrap();
        let event_bytes = encode_viewport(218, ViewportPresentationEventBody::Packet(packet));
        let ViewportPresentationEventBody::Packet(decoded) =
            decode_viewport_presentation_event(&event_bytes, binding())
                .unwrap()
                .body
        else {
            panic!("expected viewport packet");
        };
        assert_failure(
            decode_event(&event_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );
        assert_failure(
            decode_hot_inline_sidecar_event(&event_bytes, binding()),
            DecodeFailure::UnknownVariant,
        );

        let mut transport = ViewportPresentationTransportDigest::new();
        for frame in decoded.frames() {
            let frame = frame.unwrap();
            let computed = transport
                .push(
                    frame.ordinal,
                    frame.first_record_ordinal,
                    frame.record_count,
                    kinds[frame.ordinal as usize],
                    frame.bytes,
                )
                .unwrap();
            assert_eq!(
                frame.digest,
                protocol_digest128_from_blake3(
                    ProtocolDigestDomain::ViewportPresentationFrame,
                    computed,
                )
            );
        }
        assert!(transport.is_complete());
        let receipt = transport.finish().unwrap();
        assert_eq!(receipt.frame_count, 4);
        assert_eq!(receipt.encoded_frame_bytes, 46);

        let same_bytes = bodies[0];
        assert_ne!(
            viewport_presentation_frame_digest256(
                0,
                ViewportPresentationFrameKind::Begin,
                same_bytes,
            ),
            candidate_frame_digest256(0, CandidateSnapshotFrameKind::Begin, same_bytes),
        );
        assert_ne!(
            viewport_presentation_frame_digest256(
                0,
                ViewportPresentationFrameKind::Begin,
                same_bytes,
            ),
            hot_inline_sidecar_frame_digest256(0, HotInlineSidecarFrameKind::Begin, same_bytes,),
        );
        let digest256 = digest256(0xcc);
        assert_ne!(
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationEnvelope,
                digest256,
            ),
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::HotInlineSidecarRootStream,
                digest256,
            ),
        );

        let mut invalid = ViewportPresentationTransportDigest::new();
        assert_eq!(
            invalid.push(
                0,
                0,
                0,
                ViewportPresentationFrameKind::Directory,
                b"directory",
            ),
            Err(ViewportPresentationTransportDigestError::InvalidFrameSequence)
        );
        assert_eq!(
            ViewportPresentationTransportDigest::new().finish(),
            Err(ViewportPresentationTransportDigestError::Incomplete)
        );

        let mut out_of_order = ViewportPresentationTransportDigest::new();
        out_of_order
            .push(0, 0, 0, ViewportPresentationFrameKind::Begin, b"begin")
            .unwrap();
        assert_eq!(
            out_of_order.push(
                2,
                0,
                0,
                ViewportPresentationFrameKind::Directory,
                b"directory",
            ),
            Err(ViewportPresentationTransportDigestError::OutOfOrder)
        );
    }

    #[test]
    fn viewport_presentation_parent_directory_children_and_end_are_canonical() {
        let mut begin = viewport_presentation_begin();
        begin.envelope.aggregate_envelope_digest256 = [0; 32];
        let entries = viewport_presentation_directory_entries();

        let mut directory_bytes = vec![0; 4096];
        let directory_length =
            encode_viewport_presentation_directory_into(begin, &entries, &mut directory_bytes)
                .unwrap();
        directory_bytes.truncate(directory_length);
        let envelope_digest = viewport_presentation_aggregate_envelope_digest256(
            begin.binding,
            begin.envelope,
            &directory_bytes,
        )
        .unwrap();
        begin.envelope.aggregate_envelope_digest256 = envelope_digest;

        let mut parent_bytes = vec![0; VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES];
        assert_eq!(
            encode_viewport_presentation_parent_frame_into(begin, &mut parent_bytes).unwrap(),
            VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES
        );
        assert_eq!(
            decode_viewport_presentation_parent_frame(&parent_bytes, begin).unwrap(),
            ViewportPresentationParentFrame {
                binding: begin.binding,
                envelope: begin.envelope,
            }
        );
        let directory = decode_viewport_presentation_directory(&directory_bytes, begin).unwrap();
        assert_eq!(directory.entry_count, 2);
        assert_eq!(directory.entry(0), Some(entries[0]));
        assert_eq!(directory.entry(1), Some(entries[1]));
        assert_eq!(directory.entry(2), None);
        assert_eq!(directory.entries().collect::<Vec<_>>(), entries);
        let mut ordered_entries = directory.entries();
        assert_eq!(ordered_entries.next(), Some(entries[0]));
        assert_eq!(ordered_entries.next(), Some(entries[1]));
        assert_eq!(ordered_entries.next(), None);
        assert_eq!(ordered_entries.next(), None);
        assert_eq!(
            viewport_presentation_aggregate_envelope_digest256(
                begin.binding,
                begin.envelope,
                &directory_bytes,
            )
            .unwrap(),
            envelope_digest
        );

        let child_specs = [
            (0, 0, HotInlineSidecarFrameKind::Begin, 0, b"h0" as &[u8]),
            (0, 1, HotInlineSidecarFrameKind::Node, 1, b"n00"),
            (0, 2, HotInlineSidecarFrameKind::Node, 1, b"n01"),
            (0, 3, HotInlineSidecarFrameKind::End, 0, b"e0"),
            (1, 0, HotInlineSidecarFrameKind::Begin, 0, b"h1"),
            (1, 1, HotInlineSidecarFrameKind::Node, 1, b"n10"),
            (1, 2, HotInlineSidecarFrameKind::Node, 1, b"n11"),
            (1, 3, HotInlineSidecarFrameKind::End, 0, b"e1"),
        ];
        let mut child_wrappers = Vec::new();
        for (directory_index, child_frame_ordinal, kind, record_count, payload) in child_specs {
            let mut encoded = vec![0; 128];
            let written = encode_viewport_presentation_child_frame_into(
                begin,
                ViewportPresentationChildFrameInput {
                    directory_index,
                    child_frame_ordinal,
                    kind,
                    record_count,
                    payload,
                },
                &mut encoded,
            )
            .unwrap();
            encoded.truncate(written);
            let decoded = decode_viewport_presentation_child_frame(&encoded, begin).unwrap();
            assert_eq!(decoded.directory_index, directory_index);
            assert_eq!(decoded.child_frame_ordinal, child_frame_ordinal);
            assert_eq!(decoded.kind, kind);
            assert_eq!(decoded.record_count, record_count);
            assert_eq!(decoded.payload(), payload);
            assert_eq!(decoded.encoded(), encoded);
            child_wrappers.push(encoded);
        }

        let actual_frame_count = 3 + child_wrappers.len() as u32;
        let actual_encoded_frame_bytes = u32::try_from(
            parent_bytes.len()
                + directory_bytes.len()
                + child_wrappers.iter().map(Vec::len).sum::<usize>()
                + VIEWPORT_PRESENTATION_END_FRAME_BYTES,
        )
        .unwrap();
        let end = ViewportPresentationEndFrame {
            ordered_leaf_count: 2,
            actual_frame_count,
            actual_encoded_frame_bytes,
            aggregate_envelope_digest256: envelope_digest,
        };
        let mut end_bytes = vec![0; VIEWPORT_PRESENTATION_END_FRAME_BYTES];
        encode_viewport_presentation_end_frame_into(begin, end, &mut end_bytes).unwrap();
        assert_eq!(
            decode_viewport_presentation_end_frame(&end_bytes, begin).unwrap(),
            end
        );

        let mut transport = ViewportPresentationTransportDigest::new();
        transport
            .push(0, 0, 0, ViewportPresentationFrameKind::Begin, &parent_bytes)
            .unwrap();
        transport
            .push(
                1,
                0,
                2,
                ViewportPresentationFrameKind::Directory,
                &directory_bytes,
            )
            .unwrap();
        let mut next_record_ordinal = 2;
        for (index, wrapper) in child_wrappers.iter().enumerate() {
            let decoded = decode_viewport_presentation_child_frame(wrapper, begin).unwrap();
            transport
                .push(
                    2 + index as u32,
                    next_record_ordinal,
                    decoded.record_count,
                    ViewportPresentationFrameKind::Child,
                    wrapper,
                )
                .unwrap();
            next_record_ordinal += decoded.record_count;
        }
        transport
            .push(
                actual_frame_count - 1,
                next_record_ordinal,
                0,
                ViewportPresentationFrameKind::End,
                &end_bytes,
            )
            .unwrap();
        let receipt = transport.finish().unwrap();
        assert_eq!(receipt.frame_count, actual_frame_count);
        assert_eq!(receipt.encoded_frame_bytes, actual_encoded_frame_bytes);
        let root_digest = viewport_presentation_root_stream_digest256(envelope_digest, receipt);
        assert_ne!(root_digest, envelope_digest);
        assert_ne!(
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationRootStream,
                root_digest,
            ),
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::HotInlineSidecarRootStream,
                root_digest,
            ),
        );

        assert_failure(
            decode_viewport_presentation_parent_frame(&directory_bytes, begin),
            DecodeFailure::TrailingPayload,
        );
        assert_failure(
            decode_viewport_presentation_directory(&parent_bytes, begin),
            DecodeFailure::InvalidValue,
        );
        assert_failure(
            decode_viewport_presentation_child_frame(b"HIO1", begin),
            DecodeFailure::TruncatedPayload,
        );

        let mut crossed_index = entries;
        crossed_index[1].ordered_child_index = 0;
        let mut scratch = vec![0; 4096];
        assert_eq!(
            encode_viewport_presentation_directory_into(begin, &crossed_index, &mut scratch,),
            Err(EncodeError::InvalidValue)
        );
    }

    #[test]
    fn viewport_presentation_directory_uses_the_fpk3_64k_frame_ceiling() {
        let mut begin = viewport_presentation_begin();
        let maximum_directory_bytes = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
            + VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
                * crate::v3_session_wire::MAXIMUM_VIEWPORT_INLINE_LEAVES as usize;
        begin.limits.maximum_frame_bytes = u32::try_from(maximum_directory_bytes).unwrap();
        begin.limits.maximum_packet_bytes =
            u32::try_from(PACKET_HEADER_BYTES + PACKET_FRAME_DESCRIPTOR_BYTES).unwrap()
                + begin.limits.maximum_frame_bytes;
        begin.limits.maximum_encoded_frame_bytes = begin.limits.maximum_frame_bytes;
        let encoded = encode_viewport(219, ViewportPresentationEventBody::Begin(begin));
        assert_eq!(
            decode_viewport_presentation_event(&encoded, binding())
                .unwrap()
                .body,
            ViewportPresentationEventBody::Begin(begin)
        );

        let frame_body = vec![0x5a; maximum_directory_bytes];
        let packet = encode_packet(PublicationPacketInput {
            offer_id: begin.offer_id,
            first_frame_ordinal: 0,
            first_record_ordinal: 0,
            frames: &[PublicationPacketFrameInput {
                record_count: 128,
                digest: digest(190),
                bytes: &frame_body,
            }],
        });
        assert_eq!(
            decode_publication_packet(&packet)
                .unwrap()
                .aggregate_frame_bytes,
            maximum_directory_bytes as u32
        );
    }

    #[test]
    fn viewport_presentation_poll_family_covers_every_phase_without_aliasing() {
        let offer_id = viewport_presentation_begin().offer_id;
        let cases = [
            (
                ViewportPresentationHostPollTicket {
                    binding: binding(),
                    poll_ticket: 220,
                    offer_id,
                    phase: ViewportPresentationHostPollPhase::PacketCredit,
                },
                ViewportPresentationHostPollResult::Completed(
                    ViewportPresentationHostPollOutcome::PacketCredit {
                        offer_id,
                        next_frame_ordinal: 4,
                    },
                ),
            ),
            (
                ViewportPresentationHostPollTicket {
                    binding: binding(),
                    poll_ticket: 221,
                    offer_id,
                    phase: ViewportPresentationHostPollPhase::Commit,
                },
                ViewportPresentationHostPollResult::Completed(
                    ViewportPresentationHostPollOutcome::Committed(viewport_presentation_ack()),
                ),
            ),
            (
                ViewportPresentationHostPollTicket {
                    binding: binding(),
                    poll_ticket: 222,
                    offer_id,
                    phase: ViewportPresentationHostPollPhase::Abort,
                },
                ViewportPresentationHostPollResult::Completed(
                    ViewportPresentationHostPollOutcome::AbortComplete { offer_id },
                ),
            ),
            (
                ViewportPresentationHostPollTicket {
                    binding: binding(),
                    poll_ticket: 223,
                    offer_id,
                    phase: ViewportPresentationHostPollPhase::Commit,
                },
                ViewportPresentationHostPollResult::Rejected(HostRejectReason::BaseMismatch),
            ),
        ];
        for (ticket, result) in cases {
            let response = encode_viewport_presentation_poll_response(ticket, result);
            assert_eq!(
                decode_viewport_presentation_host_poll_command(&response, binding())
                    .unwrap()
                    .result,
                result
            );
            assert!(decode_host_poll_command(&response, binding()).is_err());
            assert!(decode_inline_sidecar_host_poll_command(&response, binding()).is_err());
        }

        let wrong_phase_ticket = ViewportPresentationHostPollTicket {
            binding: binding(),
            poll_ticket: 224,
            offer_id,
            phase: ViewportPresentationHostPollPhase::Commit,
        };
        let wrong_phase = encode_viewport_presentation_poll_response(
            wrong_phase_ticket,
            ViewportPresentationHostPollResult::Completed(
                ViewportPresentationHostPollOutcome::PacketCredit {
                    offer_id,
                    next_frame_ordinal: 4,
                },
            ),
        );
        assert_failure(
            decode_viewport_presentation_host_poll_command(&wrong_phase, binding()),
            DecodeFailure::InvalidValue,
        );
    }
}
