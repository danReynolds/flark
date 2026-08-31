//! Paged, serialized-actor reference occurrence substrate.
//!
//! This module accepts only already-authoritative controller facts. It does
//! not recognize Markdown, normalize labels, clean destinations, or define a
//! transport schema. One pending fact is streamed into bounded blob pages,
//! one fact node, and fixed-fanout occurrence pages. The build journal retains
//! only a bounded working set; each parent edge is followed by transfer of the
//! superseded owner. Final root transfer uses the arena's fuelled seal.
//!
//! This builder emits an unsealed reference subtree into the caller-owned
//! candidate arena/journal. Only the five-role manifest may seal that journal.
//! Ordinal lookup, winner lookup, and multi-page blob reads are synchronous
//! immutable queries. Winner lookup is exact but linear in occurrence pages;
//! the host-facing path still needs a resumable query cursor or indexed winner
//! root with an explicit work budget before it may run on a UI isolate.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use crate::candidate_manifest::{
    decode_canonical_node_header, encode_canonical_node_header, finalize_role_digest,
    hash_record_digest, preflight_remaining, role_hasher, CandidateAuthority, CandidateRole,
    ManifestError, ReferenceReserve, RoleMetadata, CANDIDATE_FORMAT_VERSION,
    CANONICAL_NODE_HEADER_BYTES, REFERENCES_SCHEMA, STRONG_DIGEST_BYTES,
};
use crate::storage::{ArenaBuildOwner, ArenaError, ArenaLimits, PageArena, ARENA_PAGE_BYTES};
use crate::ArenaId;

const BLOB_TAG: u8 = 0xd1;
const FACT_TAG: u8 = 0xd2;
const PAGE_TAG: u8 = 0xd3;
const ROOT_TAG: u8 = 0xd4;
pub(crate) const INLINE_FACT_TAG: u8 = 0xd5;
const NODE_HEADER_BYTES: usize = CANONICAL_NODE_HEADER_BYTES;
const BLOB_METADATA_BYTES: usize = 32;
pub(crate) const BLOB_CHUNK_BYTES: usize =
    ARENA_PAGE_BYTES - NODE_HEADER_BYTES - BLOB_METADATA_BYTES;
const FACT_PAYLOAD_BYTES: usize = NODE_HEADER_BYTES + 8 + (4 * 32) + 32;
pub(crate) const INLINE_FACT_VALUE_BYTES: usize = ARENA_PAGE_BYTES - FACT_PAYLOAD_BYTES;
const PAGE_PAYLOAD_BYTES: usize = NODE_HEADER_BYTES + 24;
const ROOT_PAYLOAD_BYTES: usize = NODE_HEADER_BYTES + 16;
pub(crate) const REFERENCE_ROOT_PAYLOAD_BYTES: usize = ROOT_PAYLOAD_BYTES;
const DEFAULT_FACTS_PER_PAGE: usize = 64;
const REFERENCE_WINNER_LABEL_DIGEST_DOMAIN: &[u8] = b"flark.reference-winner-label.v1\0";
const REFERENCE_WINNER_MAX_DIGEST_BUCKET_LABELS: usize = 4;
const REFERENCE_WINNER_ACCOUNTED_BYTES_PER_OCCURRENCE: usize = 128;
const REFERENCE_WINNER_ACCOUNTED_FIXED_BYTES: usize = 8 * 1024;

/// CommonMark admits at most 999 Unicode scalars in a reference label. The
/// production label service pins Unicode default case folding to at most six
/// output bytes per input scalar, so 999 * 6 is the exact derived envelope.
/// Keeping the index on that finite bound makes every transition bounded while
/// still admitting expanding labels such as repeated U+0130.
pub(crate) const REFERENCE_WINNER_INDEX_MAX_NORMALIZED_LABEL_BYTES: u64 = 5_994;

type ReferenceAuthority = CandidateAuthority;

/// Returns whether a payload claims one authority-free canonical References
/// storage node. Exact shape and closure validation still happen in the typed
/// decoders; this predicate is only the snapshot host's header-routing seam.
pub(crate) fn is_canonical_reference_node_payload(payload: &[u8]) -> bool {
    matches!(
        payload.get(..CANONICAL_NODE_HEADER_BYTES),
        Some(&[BLOB_TAG, CANDIDATE_FORMAT_VERSION, 0, 0])
            | Some(&[FACT_TAG, CANDIDATE_FORMAT_VERSION, 0, 0])
            | Some(&[PAGE_TAG, CANDIDATE_FORMAT_VERSION, 0, 0])
            | Some(&[ROOT_TAG, CANDIDATE_FORMAT_VERSION, 0, 0])
            | Some(&[INLINE_FACT_TAG, CANDIDATE_FORMAT_VERSION, 0, 0])
    )
}

/// Returns whether `payload` is one canonical v1 reference-fact node rather
/// than a blob, occurrence page, or reference root.
pub(crate) fn is_canonical_reference_record_payload(payload: &[u8]) -> bool {
    let title_flag = NODE_HEADER_BYTES + 8 + (4 * 32);
    let common = payload.len() >= FACT_PAYLOAD_BYTES
        && matches!(payload.get(title_flag), Some(0 | 1))
        && payload.get(title_flag + 1..title_flag + 8) == Some(&[0; 7]);
    if !common {
        return false;
    }
    match payload.get(..4) {
        Some(&[FACT_TAG, 1, 0, 0]) => payload.len() == FACT_PAYLOAD_BYTES,
        Some(&[INLINE_FACT_TAG, 1, 0, 0]) => inline_value_ranges(payload).is_some(),
        _ => false,
    }
}

fn inline_value_ranges(payload: &[u8]) -> Option<[Range<usize>; 3]> {
    if payload.len() < FACT_PAYLOAD_BYTES || payload.len() > ARENA_PAGE_BYTES {
        return None;
    }
    let read_len = |offset: usize| {
        payload
            .get(offset..offset + 8)?
            .try_into()
            .ok()
            .map(u64::from_le_bytes)
            .and_then(|value| usize::try_from(value).ok())
    };
    let label_len = read_len(NODE_HEADER_BYTES + 144)?;
    let destination_len = read_len(NODE_HEADER_BYTES + 152)?;
    let title_len = read_len(NODE_HEADER_BYTES + 160)?;
    if payload[NODE_HEADER_BYTES + 136] == 0 && title_len != 0 {
        return None;
    }
    let label_start = FACT_PAYLOAD_BYTES;
    let destination_start = label_start.checked_add(label_len)?;
    let title_start = destination_start.checked_add(destination_len)?;
    let end = title_start.checked_add(title_len)?;
    (end == payload.len()).then_some([
        label_start..destination_start,
        destination_start..title_start,
        title_start..end,
    ])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceSourceRange {
    pub(crate) bytes: Range<u64>,
    pub(crate) utf16: Range<u64>,
}

impl ReferenceSourceRange {
    fn is_valid(&self) -> bool {
        self.bytes.start <= self.bytes.end && self.utf16.start <= self.utf16.end
    }

    fn contains(&self, inner: &Self) -> bool {
        self.bytes.start <= inner.bytes.start
            && inner.bytes.end <= self.bytes.end
            && self.utf16.start <= inner.utf16.start
            && inner.utf16.end <= self.utf16.end
    }
}

/// One fact already authenticated and normalized by the exact controller.
///
/// Only this one fact's cooked bytes may be owned while it is being paged.
pub(crate) struct AuthoritativeReferenceFact {
    pub(crate) authority: ReferenceAuthority,
    pub(crate) source: ReferenceSourceRange,
    pub(crate) label_source: ReferenceSourceRange,
    pub(crate) destination_source: ReferenceSourceRange,
    pub(crate) title_source: Option<ReferenceSourceRange>,
    pub(crate) normalized_label: Box<[u8]>,
    pub(crate) cooked_destination: Box<[u8]>,
    pub(crate) cooked_title: Option<Box<[u8]>>,
    pub(crate) _not_sync: PhantomData<Cell<()>>,
}

/// Exact metadata for a reference whose cooked values will arrive as bounded
/// source-derived chunks.
pub(crate) struct AuthoritativeReferenceFactStart {
    pub(crate) authority: ReferenceAuthority,
    pub(crate) source: ReferenceSourceRange,
    pub(crate) label_source: ReferenceSourceRange,
    pub(crate) destination_source: ReferenceSourceRange,
    pub(crate) title_source: Option<ReferenceSourceRange>,
    pub(crate) normalized_label: Box<[u8]>,
    pub(crate) destination_len: usize,
    pub(crate) title_len: Option<usize>,
    pub(crate) _not_sync: PhantomData<Cell<()>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamedReferenceValueKind {
    Destination,
    Title,
}

impl fmt::Debug for AuthoritativeReferenceFact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoritativeReferenceFact")
            .field("authority", &self.authority)
            .field("source", &self.source)
            .field("label_source", &self.label_source)
            .field("destination_source", &self.destination_source)
            .field("title_source", &self.title_source)
            .field("normalized_label_bytes", &self.normalized_label.len())
            .field("cooked_destination_bytes", &self.cooked_destination.len())
            .field(
                "cooked_title_bytes",
                &self.cooked_title.as_ref().map(|title| title.len()),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceRootLimits {
    pub(crate) arena: ArenaLimits,
    pub(crate) max_occurrences: u64,
    pub(crate) max_cooked_bytes_per_fact: usize,
    pub(crate) facts_per_page: usize,
}

impl Default for ReferenceRootLimits {
    fn default() -> Self {
        Self {
            arena: ArenaLimits {
                max_children_per_node: DEFAULT_FACTS_PER_PAGE + 1,
                ..ArenaLimits::default()
            },
            max_occurrences: 1_000_000,
            max_cooked_bytes_per_fact: 16 * 1024 * 1024,
            facts_per_page: DEFAULT_FACTS_PER_PAGE,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ReferenceRootError {
    InvalidLimits,
    InvalidAuthority,
    CrossAuthority,
    Busy,
    Finishing,
    InvalidRange,
    OutOfSourceOrder,
    EmptyNormalizedLabel,
    InvalidNormalizedLabelUtf8,
    FactTooLarge,
    StreamValueMismatch,
    StreamLengthExceeded,
    OccurrenceLimit,
    CapacityPreflight,
    ZeroFuel,
    Arena(ArenaError),
    Corrupt(&'static str),
}

impl fmt::Display for ReferenceRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid reference-root limits"),
            Self::InvalidAuthority => formatter.write_str("invalid reference authority"),
            Self::CrossAuthority => formatter.write_str("reference fact crosses authority"),
            Self::Busy => formatter.write_str("a reference fact is already active"),
            Self::Finishing => formatter.write_str("reference root is finishing"),
            Self::InvalidRange => formatter.write_str("invalid reference source ranges"),
            Self::OutOfSourceOrder => {
                formatter.write_str("reference facts are out of source order")
            }
            Self::EmptyNormalizedLabel => formatter.write_str("normalized label is empty"),
            Self::InvalidNormalizedLabelUtf8 => {
                formatter.write_str("normalized label is not UTF-8")
            }
            Self::FactTooLarge => formatter.write_str("reference fact exceeds its hard byte bound"),
            Self::StreamValueMismatch => {
                formatter.write_str("reference stream offered the wrong cooked value")
            }
            Self::StreamLengthExceeded => {
                formatter.write_str("reference stream exceeded its declared cooked length")
            }
            Self::OccurrenceLimit => formatter.write_str("reference occurrence limit reached"),
            Self::CapacityPreflight => {
                formatter.write_str("reference fact exceeds remaining arena capacity")
            }
            Self::ZeroFuel => formatter.write_str("reference poll requires nonzero fuel"),
            Self::Arena(error) => write!(formatter, "reference storage failed: {error}"),
            Self::Corrupt(message) => write!(formatter, "corrupt reference root: {message}"),
        }
    }
}

impl std::error::Error for ReferenceRootError {}

impl From<ArenaError> for ReferenceRootError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<ManifestError> for ReferenceRootError {
    fn from(error: ManifestError) -> Self {
        match error {
            ManifestError::InvalidAuthority => Self::InvalidAuthority,
            ManifestError::CapacityPreflight => Self::CapacityPreflight,
            ManifestError::ZeroFuel => Self::ZeroFuel,
            ManifestError::Arena(error) => Self::Arena(error),
            _ => Self::Corrupt("candidate manifest rejected reference storage"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlobKind {
    Label = 1,
    Destination = 2,
    Title = 3,
}

impl BlobKind {
    fn from_u8(value: u8) -> Result<Self, ReferenceRootError> {
        match value {
            1 => Ok(Self::Label),
            2 => Ok(Self::Destination),
            3 => Ok(Self::Title),
            _ => Err(ReferenceRootError::Corrupt("unknown blob kind")),
        }
    }
}

struct BlobBuild {
    kind: BlobKind,
    bytes: Box<[u8]>,
    offset: usize,
    root: Option<ArenaBuildOwner>,
}

impl BlobBuild {
    fn new(kind: BlobKind, bytes: Box<[u8]>) -> Self {
        Self {
            kind,
            bytes,
            offset: 0,
            root: None,
        }
    }

    fn complete(&self) -> bool {
        self.root.is_some() && self.offset == self.bytes.len()
    }

    fn drive_one(
        &mut self,
        authority: ReferenceAuthority,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<(), ReferenceRootError> {
        if self.complete() {
            return Ok(());
        }
        let start = self.offset;
        let end = if self.bytes.is_empty() {
            0
        } else {
            start.saturating_add(BLOB_CHUNK_BYTES).min(self.bytes.len())
        };
        let chunk = &self.bytes[start..end];
        let payload = encode_blob(authority, self.kind, self.bytes.len(), start, chunk)?;
        let children = self
            .root
            .as_ref()
            .map_or_else(Vec::new, |root| vec![root.id()]);
        let parent = session.allocate(&payload, &children)?;
        if let Some(previous) = self.root.take() {
            session.release(previous)?;
        }
        self.root = Some(parent);
        self.offset = end;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FactPhase {
    Label,
    Destination,
    Title,
    Node,
}

enum PendingFactStorage {
    Inline {
        normalized_label: Box<[u8]>,
        cooked_destination: Box<[u8]>,
        cooked_title: Option<Box<[u8]>>,
    },
    Spilled {
        phase: FactPhase,
        blob: BlobBuild,
        label_root: Option<ArenaBuildOwner>,
        destination_root: Option<ArenaBuildOwner>,
        title_root: Option<ArenaBuildOwner>,
        destination_bytes: Option<Box<[u8]>>,
        title_bytes: Option<Box<[u8]>>,
    },
}

struct PendingFact {
    ordinal: u64,
    source: ReferenceSourceRange,
    label_source: ReferenceSourceRange,
    destination_source: ReferenceSourceRange,
    title_source: Option<ReferenceSourceRange>,
    label_len: u64,
    destination_len: u64,
    title_len: Option<u64>,
    storage: PendingFactStorage,
    canonical_bytes: u64,
    canonical_digest: [u8; STRONG_DIGEST_BYTES],
}

impl PendingFact {
    fn new(ordinal: u64, fact: AuthoritativeReferenceFact) -> Result<Self, ReferenceRootError> {
        let (canonical_bytes, canonical_digest) = canonical_fact_digest(ordinal, &fact)?;
        let label_len = u64::try_from(fact.normalized_label.len())
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        let destination_len = u64::try_from(fact.cooked_destination.len())
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        let title_len = fact
            .cooked_title
            .as_ref()
            .map(|title| u64::try_from(title.len()))
            .transpose()
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        let inline = inline_fact_fits(
            fact.normalized_label.len(),
            fact.cooked_destination.len(),
            fact.cooked_title.as_ref().map_or(0, |title| title.len()),
        )?;
        let storage = if inline {
            PendingFactStorage::Inline {
                normalized_label: fact.normalized_label,
                cooked_destination: fact.cooked_destination,
                cooked_title: fact.cooked_title,
            }
        } else {
            PendingFactStorage::Spilled {
                phase: FactPhase::Label,
                blob: BlobBuild::new(BlobKind::Label, fact.normalized_label),
                label_root: None,
                destination_root: None,
                title_root: None,
                destination_bytes: Some(fact.cooked_destination),
                title_bytes: fact.cooked_title,
            }
        };
        Ok(Self {
            ordinal,
            source: fact.source,
            label_source: fact.label_source,
            destination_source: fact.destination_source,
            title_source: fact.title_source,
            label_len,
            destination_len,
            title_len,
            storage,
            canonical_bytes,
            canonical_digest,
        })
    }

    fn drive_one(
        &mut self,
        authority: ReferenceAuthority,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<Option<ArenaBuildOwner>, ReferenceRootError> {
        if let PendingFactStorage::Inline {
            normalized_label,
            cooked_destination,
            cooked_title,
        } = &self.storage
        {
            let payload = encode_inline_fact(
                self,
                authority,
                normalized_label,
                cooked_destination,
                cooked_title.as_deref(),
            );
            return session
                .allocate(&payload, &[])
                .map(Some)
                .map_err(Into::into);
        }

        if matches!(
            self.storage,
            PendingFactStorage::Spilled {
                phase: FactPhase::Node,
                ..
            }
        ) {
            let payload = encode_fact(self, authority);
            let PendingFactStorage::Spilled {
                label_root,
                destination_root,
                title_root,
                ..
            } = &mut self.storage
            else {
                unreachable!();
            };
            let mut children = Vec::with_capacity(3);
            children.push(
                label_root
                    .as_ref()
                    .ok_or(ReferenceRootError::Corrupt("fact lost label root"))?
                    .id(),
            );
            children.push(
                destination_root
                    .as_ref()
                    .ok_or(ReferenceRootError::Corrupt("fact lost destination root"))?
                    .id(),
            );
            if let Some(title) = &title_root {
                children.push(title.id());
            }
            let fact = session.allocate(&payload, &children)?;
            session.release(
                label_root
                    .take()
                    .ok_or(ReferenceRootError::Corrupt("fact lost label owner"))?,
            )?;
            session.release(
                destination_root
                    .take()
                    .ok_or(ReferenceRootError::Corrupt("fact lost destination owner"))?,
            )?;
            if let Some(title) = title_root.take() {
                session.release(title)?;
            }
            return Ok(Some(fact));
        }

        let PendingFactStorage::Spilled {
            phase,
            blob,
            label_root,
            destination_root,
            title_root,
            destination_bytes,
            title_bytes,
        } = &mut self.storage
        else {
            unreachable!();
        };
        blob.drive_one(authority, session)?;
        if !blob.complete() {
            return Ok(None);
        }
        let root = blob
            .root
            .take()
            .ok_or(ReferenceRootError::Corrupt("completed blob lost root"))?;
        match *phase {
            FactPhase::Label => {
                *label_root = Some(root);
                *phase = FactPhase::Destination;
                *blob = BlobBuild::new(
                    BlobKind::Destination,
                    destination_bytes.take().ok_or(ReferenceRootError::Corrupt(
                        "pending fact lost destination bytes",
                    ))?,
                );
            }
            FactPhase::Destination => {
                *destination_root = Some(root);
                if let Some(title) = title_bytes.take() {
                    *phase = FactPhase::Title;
                    *blob = BlobBuild::new(BlobKind::Title, title);
                } else {
                    *phase = FactPhase::Node;
                }
            }
            FactPhase::Title => {
                *title_root = Some(root);
                *phase = FactPhase::Node;
            }
            FactPhase::Node => unreachable!(),
        }
        Ok(None)
    }
}

struct StreamBlobBuild {
    kind: BlobKind,
    total_len: usize,
    received: usize,
    buffer: Vec<u8>,
    root: Option<ArenaBuildOwner>,
}

enum StreamBlobPoll {
    Idle,
    Progress,
    Complete(ArenaBuildOwner),
}

impl StreamBlobBuild {
    fn new(kind: BlobKind, total_len: usize) -> Result<Self, ReferenceRootError> {
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(BLOB_CHUNK_BYTES)
            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
        Ok(Self {
            kind,
            total_len,
            received: 0,
            buffer,
            root: None,
        })
    }

    fn capacity(&self) -> usize {
        if self.received == self.total_len {
            0
        } else {
            (BLOB_CHUNK_BYTES - self.buffer.len()).min(self.total_len - self.received)
        }
    }

    fn offer(&mut self, bytes: &[u8]) -> Result<usize, ReferenceRootError> {
        let take = self.capacity().min(bytes.len());
        self.buffer
            .try_reserve(take)
            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
        self.buffer.extend_from_slice(&bytes[..take]);
        self.received = self
            .received
            .checked_add(take)
            .ok_or(ReferenceRootError::FactTooLarge)?;
        if take == 0 && !bytes.is_empty() && self.received < self.total_len {
            return Err(ReferenceRootError::Busy);
        }
        if self.received > self.total_len {
            return Err(ReferenceRootError::StreamLengthExceeded);
        }
        Ok(take)
    }

    fn retained_bytes(&self) -> usize {
        self.buffer.len()
    }

    fn drive_one(
        &mut self,
        authority: ReferenceAuthority,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<StreamBlobPoll, ReferenceRootError> {
        if self.received == self.total_len && self.buffer.is_empty() {
            if let Some(root) = self.root.take() {
                return Ok(StreamBlobPoll::Complete(root));
            }
        }
        let must_flush = self.buffer.len() == BLOB_CHUNK_BYTES
            || self.received == self.total_len && (self.root.is_none() || !self.buffer.is_empty());
        if !must_flush {
            return Ok(StreamBlobPoll::Idle);
        }
        let chunk_start = self
            .received
            .checked_sub(self.buffer.len())
            .ok_or(ReferenceRootError::Corrupt("streamed blob start underflow"))?;
        let payload = encode_blob(
            authority,
            self.kind,
            self.total_len,
            chunk_start,
            &self.buffer,
        )?;
        let children = self
            .root
            .as_ref()
            .map_or_else(Vec::new, |root| vec![root.id()]);
        let parent = session.allocate(&payload, &children)?;
        if let Some(previous) = self.root.take() {
            session.release(previous)?;
        }
        self.root = Some(parent);
        self.buffer.clear();
        Ok(StreamBlobPoll::Progress)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamFactPhase {
    Label,
    Destination,
    Title,
    Node,
}

struct PendingStreamFact {
    ordinal: u64,
    source: ReferenceSourceRange,
    label_source: ReferenceSourceRange,
    destination_source: ReferenceSourceRange,
    title_source: Option<ReferenceSourceRange>,
    label_len: u64,
    destination_len: u64,
    title_len: Option<u64>,
    phase: StreamFactPhase,
    label: Option<BlobBuild>,
    value: Option<StreamBlobBuild>,
    inline_label: Option<Box<[u8]>>,
    inline_destination: Option<Vec<u8>>,
    inline_title: Option<Vec<u8>>,
    label_root: Option<ArenaBuildOwner>,
    destination_root: Option<ArenaBuildOwner>,
    title_root: Option<ArenaBuildOwner>,
    canonical_bytes: u64,
    canonical_hasher: blake3::Hasher,
}

enum StreamFactPoll {
    Idle,
    Progress,
    Complete(ArenaBuildOwner),
}

impl PendingStreamFact {
    fn new(
        ordinal: u64,
        fact: AuthoritativeReferenceFactStart,
    ) -> Result<Self, ReferenceRootError> {
        let label_len = u64::try_from(fact.normalized_label.len())
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        let destination_len =
            u64::try_from(fact.destination_len).map_err(|_| ReferenceRootError::FactTooLarge)?;
        let title_len = fact
            .title_len
            .map(u64::try_from)
            .transpose()
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        let canonical_bytes = canonical_fact_bytes(
            fact.title_source.is_some(),
            label_len,
            destination_len,
            title_len.unwrap_or(0),
        )?;
        let mut canonical_hasher = canonical_fact_hasher(
            fact.authority,
            ordinal,
            &fact.source,
            &fact.label_source,
            &fact.destination_source,
            fact.title_source.as_ref(),
            label_len,
            destination_len,
            title_len.unwrap_or(0),
        );
        canonical_hasher.update(&fact.normalized_label);
        let inline = inline_fact_fits(
            fact.normalized_label.len(),
            fact.destination_len,
            fact.title_len.unwrap_or(0),
        )?;
        let (label, inline_label, inline_destination, inline_title) = if inline {
            let mut destination = Vec::new();
            destination
                .try_reserve_exact(fact.destination_len)
                .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
            let title = fact
                .title_len
                .map(|len| {
                    let mut bytes = Vec::new();
                    bytes
                        .try_reserve_exact(len)
                        .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
                    Ok::<Vec<u8>, ReferenceRootError>(bytes)
                })
                .transpose()?;
            (None, Some(fact.normalized_label), Some(destination), title)
        } else {
            (
                Some(BlobBuild::new(BlobKind::Label, fact.normalized_label)),
                None,
                None,
                None,
            )
        };
        Ok(Self {
            ordinal,
            source: fact.source,
            label_source: fact.label_source,
            destination_source: fact.destination_source,
            title_source: fact.title_source,
            label_len,
            destination_len,
            title_len,
            phase: StreamFactPhase::Label,
            label,
            value: None,
            inline_label,
            inline_destination,
            inline_title,
            label_root: None,
            destination_root: None,
            title_root: None,
            canonical_bytes,
            canonical_hasher,
        })
    }

    fn capacity(&self, kind: StreamedReferenceValueKind) -> Result<usize, ReferenceRootError> {
        let matches = matches!(
            (self.phase, kind),
            (
                StreamFactPhase::Destination,
                StreamedReferenceValueKind::Destination
            ) | (StreamFactPhase::Title, StreamedReferenceValueKind::Title)
        );
        if !matches {
            return Ok(0);
        }
        if let Some(destination) = &self.inline_destination {
            let (received, total) = match kind {
                StreamedReferenceValueKind::Destination => (
                    destination.len(),
                    usize::try_from(self.destination_len)
                        .map_err(|_| ReferenceRootError::FactTooLarge)?,
                ),
                StreamedReferenceValueKind::Title => (
                    self.inline_title.as_ref().map_or(0, Vec::len),
                    usize::try_from(self.title_len.unwrap_or(0))
                        .map_err(|_| ReferenceRootError::FactTooLarge)?,
                ),
            };
            return total
                .checked_sub(received)
                .ok_or(ReferenceRootError::StreamLengthExceeded);
        }
        Ok(self.value.as_ref().map_or(0, StreamBlobBuild::capacity))
    }

    fn offer(
        &mut self,
        kind: StreamedReferenceValueKind,
        bytes: &[u8],
    ) -> Result<usize, ReferenceRootError> {
        let matches = matches!(
            (self.phase, kind),
            (
                StreamFactPhase::Destination,
                StreamedReferenceValueKind::Destination
            ) | (StreamFactPhase::Title, StreamedReferenceValueKind::Title)
        );
        if !matches {
            return Err(ReferenceRootError::StreamValueMismatch);
        }
        let consumed = if self.inline_destination.is_some() {
            let capacity = self.capacity(kind)?;
            let take = capacity.min(bytes.len());
            let target = match kind {
                StreamedReferenceValueKind::Destination => {
                    self.inline_destination
                        .as_mut()
                        .ok_or(ReferenceRootError::Corrupt(
                            "inline fact lost destination buffer",
                        ))?
                }
                StreamedReferenceValueKind::Title => self
                    .inline_title
                    .as_mut()
                    .ok_or(ReferenceRootError::Corrupt("inline fact lost title buffer"))?,
            };
            target
                .try_reserve(take)
                .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
            target.extend_from_slice(&bytes[..take]);
            take
        } else {
            self.value
                .as_mut()
                .ok_or(ReferenceRootError::Corrupt(
                    "streamed fact lost value builder",
                ))?
                .offer(bytes)?
        };
        self.canonical_hasher.update(&bytes[..consumed]);
        Ok(consumed)
    }

    fn retained_bytes(&self) -> usize {
        if let Some(label) = &self.inline_label {
            return label.len()
                + self.inline_destination.as_ref().map_or(0, Vec::len)
                + self.inline_title.as_ref().map_or(0, Vec::len);
        }
        self.label.as_ref().map_or(0, |label| label.bytes.len())
            + self
                .value
                .as_ref()
                .map_or(0, StreamBlobBuild::retained_bytes)
    }

    fn canonical_digest(&self) -> [u8; STRONG_DIGEST_BYTES] {
        *self.canonical_hasher.clone().finalize().as_bytes()
    }

    fn drive_one(
        &mut self,
        authority: ReferenceAuthority,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<StreamFactPoll, ReferenceRootError> {
        match self.phase {
            StreamFactPhase::Label => {
                if self.inline_label.is_some() {
                    self.phase = StreamFactPhase::Destination;
                    return Ok(StreamFactPoll::Progress);
                }
                let label = self.label.as_mut().ok_or(ReferenceRootError::Corrupt(
                    "spilled fact lost label builder",
                ))?;
                label.drive_one(authority, session)?;
                if !label.complete() {
                    return Ok(StreamFactPoll::Progress);
                }
                self.label_root = label.root.take();
                self.value = Some(StreamBlobBuild::new(
                    BlobKind::Destination,
                    usize::try_from(self.destination_len)
                        .map_err(|_| ReferenceRootError::FactTooLarge)?,
                )?);
                self.phase = StreamFactPhase::Destination;
                Ok(StreamFactPoll::Progress)
            }
            StreamFactPhase::Destination | StreamFactPhase::Title => {
                if self.inline_destination.is_some() {
                    if self.capacity(match self.phase {
                        StreamFactPhase::Destination => StreamedReferenceValueKind::Destination,
                        StreamFactPhase::Title => StreamedReferenceValueKind::Title,
                        _ => unreachable!(),
                    })? != 0
                    {
                        return Ok(StreamFactPoll::Idle);
                    }
                    self.phase =
                        if self.phase == StreamFactPhase::Destination && self.title_len.is_some() {
                            StreamFactPhase::Title
                        } else {
                            StreamFactPhase::Node
                        };
                    return Ok(StreamFactPoll::Progress);
                }
                let poll = self
                    .value
                    .as_mut()
                    .ok_or(ReferenceRootError::Corrupt(
                        "streamed fact lost active value",
                    ))?
                    .drive_one(authority, session)?;
                match poll {
                    StreamBlobPoll::Idle => Ok(StreamFactPoll::Idle),
                    StreamBlobPoll::Progress => Ok(StreamFactPoll::Progress),
                    StreamBlobPoll::Complete(root) => {
                        if self.phase == StreamFactPhase::Destination {
                            self.destination_root = Some(root);
                            if let Some(title_len) = self.title_len {
                                self.value = Some(StreamBlobBuild::new(
                                    BlobKind::Title,
                                    usize::try_from(title_len)
                                        .map_err(|_| ReferenceRootError::FactTooLarge)?,
                                )?);
                                self.phase = StreamFactPhase::Title;
                            } else {
                                self.value = None;
                                self.phase = StreamFactPhase::Node;
                            }
                        } else {
                            self.title_root = Some(root);
                            self.value = None;
                            self.phase = StreamFactPhase::Node;
                        }
                        Ok(StreamFactPoll::Progress)
                    }
                }
            }
            StreamFactPhase::Node => {
                if let Some(label) = &self.inline_label {
                    let destination =
                        self.inline_destination
                            .as_deref()
                            .ok_or(ReferenceRootError::Corrupt(
                                "inline fact lost destination bytes",
                            ))?;
                    let payload = encode_inline_stream_fact(
                        self,
                        authority,
                        label,
                        destination,
                        self.inline_title.as_deref(),
                    );
                    let fact = session.allocate(&payload, &[])?;
                    return Ok(StreamFactPoll::Complete(fact));
                }
                let payload = encode_stream_fact(self, authority);
                let mut children = Vec::with_capacity(3);
                children.push(
                    self.label_root
                        .as_ref()
                        .ok_or(ReferenceRootError::Corrupt("streamed fact lost label root"))?
                        .id(),
                );
                children.push(
                    self.destination_root
                        .as_ref()
                        .ok_or(ReferenceRootError::Corrupt(
                            "streamed fact lost destination root",
                        ))?
                        .id(),
                );
                if let Some(title) = &self.title_root {
                    children.push(title.id());
                }
                let fact = session.allocate(&payload, &children)?;
                session.release(self.label_root.take().ok_or(ReferenceRootError::Corrupt(
                    "streamed fact lost label owner",
                ))?)?;
                session.release(self.destination_root.take().ok_or(
                    ReferenceRootError::Corrupt("streamed fact lost destination owner"),
                )?)?;
                if let Some(title) = self.title_root.take() {
                    session.release(title)?;
                }
                Ok(StreamFactPoll::Complete(fact))
            }
        }
    }
}

fn canonical_fact_digest(
    ordinal: u64,
    fact: &AuthoritativeReferenceFact,
) -> Result<(u64, [u8; STRONG_DIGEST_BYTES]), ReferenceRootError> {
    let label_len =
        u64::try_from(fact.normalized_label.len()).map_err(|_| ReferenceRootError::FactTooLarge)?;
    let destination_len = u64::try_from(fact.cooked_destination.len())
        .map_err(|_| ReferenceRootError::FactTooLarge)?;
    let title_len = u64::try_from(fact.cooked_title.as_ref().map_or(0, |title| title.len()))
        .map_err(|_| ReferenceRootError::FactTooLarge)?;
    let canonical_bytes = canonical_fact_bytes(
        fact.title_source.is_some(),
        label_len,
        destination_len,
        title_len,
    )?;

    let mut hasher = canonical_fact_hasher(
        fact.authority,
        ordinal,
        &fact.source,
        &fact.label_source,
        &fact.destination_source,
        fact.title_source.as_ref(),
        label_len,
        destination_len,
        title_len,
    );
    hasher.update(&fact.normalized_label);
    hasher.update(&fact.cooked_destination);
    if let Some(title) = &fact.cooked_title {
        hasher.update(title);
    }
    Ok((canonical_bytes, *hasher.finalize().as_bytes()))
}

fn canonical_fact_bytes(
    has_title: bool,
    label_len: u64,
    destination_len: u64,
    title_len: u64,
) -> Result<u64, ReferenceRootError> {
    8_u64
        .checked_add(3 * 32)
        .and_then(|bytes| bytes.checked_add(1))
        .and_then(|bytes| bytes.checked_add(if has_title { 32 } else { 0 }))
        .and_then(|bytes| bytes.checked_add(3 * 8))
        .and_then(|bytes| bytes.checked_add(label_len))
        .and_then(|bytes| bytes.checked_add(destination_len))
        .and_then(|bytes| bytes.checked_add(title_len))
        .ok_or(ReferenceRootError::FactTooLarge)
}

#[allow(clippy::too_many_arguments)]
fn canonical_fact_hasher(
    _authority: CandidateAuthority,
    ordinal: u64,
    source: &ReferenceSourceRange,
    label_source: &ReferenceSourceRange,
    destination_source: &ReferenceSourceRange,
    title_source: Option<&ReferenceSourceRange>,
    label_len: u64,
    destination_len: u64,
    title_len: u64,
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.reference.fact.v2\0");
    hasher.update(&ordinal.to_le_bytes());
    for range in [source, label_source, destination_source] {
        hash_source_range(&mut hasher, range);
    }
    hasher.update(&[u8::from(title_source.is_some())]);
    if let Some(range) = title_source {
        hash_source_range(&mut hasher, range);
    }
    hasher.update(&label_len.to_le_bytes());
    hasher.update(&destination_len.to_le_bytes());
    hasher.update(&title_len.to_le_bytes());
    hasher
}

fn hash_source_range(hasher: &mut blake3::Hasher, range: &ReferenceSourceRange) {
    hasher.update(&range.bytes.start.to_le_bytes());
    hasher.update(&range.bytes.end.to_le_bytes());
    hasher.update(&range.utf16.start.to_le_bytes());
    hasher.update(&range.utf16.end.to_le_bytes());
}

struct PageRelease {
    page: ArenaBuildOwner,
    owners: Vec<ArenaBuildOwner>,
}

enum BuildPhase {
    Idle,
    Fact(Box<PendingFact>),
    StreamFact(Box<PendingStreamFact>),
    PageRelease(PageRelease),
    Complete,
}

pub(crate) struct ReferenceSubtreeRoot {
    pub(crate) authority: CandidateAuthority,
    pub(crate) owner: ArenaBuildOwner,
    pub(crate) metadata: RoleMetadata,
    pub(crate) _not_sync: PhantomData<Cell<()>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceCommittedOccurrence {
    pub(crate) ordinal: u64,
    pub(crate) fact: ArenaId,
}

pub(crate) enum ReferenceBuildPoll {
    Pending {
        transitions: usize,
        idle: bool,
    },
    Complete {
        transitions: usize,
        root: ReferenceSubtreeRoot,
    },
}

pub(crate) struct ReferenceRootBuilder {
    authority: ReferenceAuthority,
    limits: ReferenceRootLimits,
    phase: BuildPhase,
    page_root: Option<ArenaBuildOwner>,
    active_facts: Vec<ArenaBuildOwner>,
    next_ordinal: u64,
    last_source_byte_end: u64,
    last_source_utf16_end: u64,
    finish_requested: bool,
    canonical_bytes: u64,
    digest: blake3::Hasher,
    committed_occurrence: Option<ReferenceCommittedOccurrence>,
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for ReferenceRootBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceRootBuilder")
            .field("authority", &self.authority)
            .field("next_ordinal", &self.next_ordinal)
            .field("active_facts", &self.active_facts.len())
            .field("finish_requested", &self.finish_requested)
            .field("canonical_bytes", &self.canonical_bytes)
            .finish_non_exhaustive()
    }
}

impl ReferenceRootBuilder {
    pub(crate) fn new(
        authority: ReferenceAuthority,
        limits: ReferenceRootLimits,
    ) -> Result<Self, ReferenceRootError> {
        if limits.max_occurrences == 0
            || limits.max_cooked_bytes_per_fact == 0
            || limits.facts_per_page == 0
            || limits.facts_per_page > DEFAULT_FACTS_PER_PAGE
            || limits.arena.max_children_per_node < (limits.facts_per_page + 1).max(3)
            || limits.arena.max_slots == 0
            || limits.arena.max_live_payload_bytes < ROOT_PAYLOAD_BYTES
        {
            return Err(ReferenceRootError::InvalidLimits);
        }
        let mut active_facts = Vec::new();
        active_facts
            .try_reserve_exact(limits.facts_per_page)
            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
        Ok(Self {
            authority,
            limits,
            phase: BuildPhase::Idle,
            page_root: None,
            active_facts,
            next_ordinal: 0,
            last_source_byte_end: 0,
            last_source_utf16_end: 0,
            finish_requested: false,
            canonical_bytes: 0,
            digest: role_hasher(authority, CandidateRole::References, REFERENCES_SCHEMA),
            committed_occurrence: None,
            _not_sync: PhantomData,
        })
    }

    /// Transfers the fact identity emitted by the most recent completed
    /// append. The fact remains owned by this build journal; the identity is
    /// only an authenticated input to the sibling first-winner journal.
    pub(crate) fn take_committed_occurrence(&mut self) -> Option<ReferenceCommittedOccurrence> {
        self.committed_occurrence.take()
    }

    pub(crate) fn offer(
        &mut self,
        fact: AuthoritativeReferenceFact,
        arena: &PageArena,
        reserve: ReferenceReserve,
    ) -> Result<(), ReferenceRootError> {
        if self.finish_requested {
            return Err(ReferenceRootError::Finishing);
        }
        if !matches!(self.phase, BuildPhase::Idle)
            || self.active_facts.len() >= self.limits.facts_per_page
        {
            return Err(ReferenceRootError::Busy);
        }
        self.validate_fact(&fact)?;
        self.preflight_fact(&fact, arena, reserve)?;
        self.phase = BuildPhase::Fact(Box::new(PendingFact::new(self.next_ordinal, fact)?));
        Ok(())
    }

    pub(crate) fn begin_stream(
        &mut self,
        fact: AuthoritativeReferenceFactStart,
        arena: &PageArena,
        reserve: ReferenceReserve,
    ) -> Result<(), ReferenceRootError> {
        if self.finish_requested {
            return Err(ReferenceRootError::Finishing);
        }
        if !matches!(self.phase, BuildPhase::Idle)
            || self.active_facts.len() >= self.limits.facts_per_page
        {
            return Err(ReferenceRootError::Busy);
        }
        self.validate_stream_start(&fact)?;
        self.preflight_stream_start(&fact, arena, reserve)?;
        self.phase =
            BuildPhase::StreamFact(Box::new(PendingStreamFact::new(self.next_ordinal, fact)?));
        Ok(())
    }

    pub(crate) fn stream_capacity(
        &self,
        kind: StreamedReferenceValueKind,
    ) -> Result<usize, ReferenceRootError> {
        let BuildPhase::StreamFact(fact) = &self.phase else {
            return Ok(0);
        };
        fact.capacity(kind)
    }

    pub(crate) fn offer_stream_bytes(
        &mut self,
        kind: StreamedReferenceValueKind,
        bytes: &[u8],
    ) -> Result<usize, ReferenceRootError> {
        let BuildPhase::StreamFact(fact) = &mut self.phase else {
            return Err(ReferenceRootError::StreamValueMismatch);
        };
        fact.offer(kind, bytes)
    }

    pub(crate) fn stream_retained_bytes(&self) -> usize {
        match &self.phase {
            BuildPhase::StreamFact(fact) => fact.retained_bytes(),
            _ => 0,
        }
    }

    pub(crate) fn finish(
        &mut self,
        arena: &PageArena,
        reserve: ReferenceReserve,
    ) -> Result<(), ReferenceRootError> {
        if !matches!(self.phase, BuildPhase::Idle) {
            return Err(ReferenceRootError::Busy);
        }
        let page_nodes = usize::from(!self.active_facts.is_empty());
        let nodes = reserve
            .nodes
            .checked_add(page_nodes)
            .and_then(|value| value.checked_add(1))
            .ok_or(ReferenceRootError::CapacityPreflight)?;
        let payload_bytes = reserve
            .payload_bytes
            .checked_add(page_nodes * PAGE_PAYLOAD_BYTES)
            .and_then(|value| value.checked_add(ROOT_PAYLOAD_BYTES))
            .ok_or(ReferenceRootError::CapacityPreflight)?;
        preflight_remaining(arena, self.limits.arena, nodes, payload_bytes)?;
        self.finish_requested = true;
        Ok(())
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self.phase, BuildPhase::Idle)
            && !self.finish_requested
            && self.active_facts.len() < self.limits.facts_per_page
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut crate::storage::ArenaBuildSession<'_>,
        fuel: usize,
    ) -> Result<ReferenceBuildPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        if matches!(self.phase, BuildPhase::Complete) {
            return Err(ReferenceRootError::Finishing);
        }
        let mut transitions = 0;
        while transitions < fuel {
            match self.drive_one(session) {
                Ok(DriveOutcome::Progress) => transitions += 1,
                Ok(DriveOutcome::Idle) => {
                    return Ok(ReferenceBuildPoll::Pending {
                        transitions,
                        idle: true,
                    });
                }
                Ok(DriveOutcome::Root(owner)) => {
                    transitions += 1;
                    let metadata = RoleMetadata {
                        role: CandidateRole::References,
                        schema: REFERENCES_SCHEMA,
                        record_count: self.next_ordinal,
                        canonical_bytes: self.canonical_bytes,
                        digest: finalize_role_digest(
                            self.digest.clone(),
                            self.next_ordinal,
                            self.canonical_bytes,
                        ),
                    };
                    return Ok(ReferenceBuildPoll::Complete {
                        transitions,
                        root: ReferenceSubtreeRoot {
                            authority: self.authority,
                            owner,
                            metadata,
                            _not_sync: PhantomData,
                        },
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(ReferenceBuildPoll::Pending {
            transitions,
            idle: false,
        })
    }

    fn drive_one(
        &mut self,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<DriveOutcome, ReferenceRootError> {
        let phase = std::mem::replace(&mut self.phase, BuildPhase::Complete);
        match phase {
            BuildPhase::Fact(mut pending) => {
                match pending.drive_one(self.authority, session)? {
                    Some(owner) => {
                        let committed = ReferenceCommittedOccurrence {
                            ordinal: self.next_ordinal,
                            fact: owner.id(),
                        };
                        self.canonical_bytes = self
                            .canonical_bytes
                            .checked_add(pending.canonical_bytes)
                            .ok_or(ReferenceRootError::FactTooLarge)?;
                        hash_record_digest(
                            &mut self.digest,
                            pending.canonical_bytes,
                            pending.canonical_digest,
                        );
                        self.last_source_byte_end = pending.source.bytes.end;
                        self.last_source_utf16_end = pending.source.utf16.end;
                        self.next_ordinal = self
                            .next_ordinal
                            .checked_add(1)
                            .ok_or(ReferenceRootError::OccurrenceLimit)?;
                        self.active_facts.push(owner);
                        self.committed_occurrence = Some(committed);
                        self.phase = BuildPhase::Idle;
                    }
                    None => self.phase = BuildPhase::Fact(pending),
                }
                Ok(DriveOutcome::Progress)
            }
            BuildPhase::StreamFact(mut pending) => {
                match pending.drive_one(self.authority, session)? {
                    StreamFactPoll::Idle => {
                        self.phase = BuildPhase::StreamFact(pending);
                        return Ok(DriveOutcome::Idle);
                    }
                    StreamFactPoll::Progress => {
                        self.phase = BuildPhase::StreamFact(pending);
                    }
                    StreamFactPoll::Complete(owner) => {
                        let committed = ReferenceCommittedOccurrence {
                            ordinal: self.next_ordinal,
                            fact: owner.id(),
                        };
                        self.canonical_bytes = self
                            .canonical_bytes
                            .checked_add(pending.canonical_bytes)
                            .ok_or(ReferenceRootError::FactTooLarge)?;
                        hash_record_digest(
                            &mut self.digest,
                            pending.canonical_bytes,
                            pending.canonical_digest(),
                        );
                        self.last_source_byte_end = pending.source.bytes.end;
                        self.last_source_utf16_end = pending.source.utf16.end;
                        self.next_ordinal = self
                            .next_ordinal
                            .checked_add(1)
                            .ok_or(ReferenceRootError::OccurrenceLimit)?;
                        self.active_facts.push(owner);
                        self.committed_occurrence = Some(committed);
                        self.phase = BuildPhase::Idle;
                    }
                }
                Ok(DriveOutcome::Progress)
            }
            BuildPhase::PageRelease(mut release) => {
                if let Some(owner) = release.owners.pop() {
                    session.release(owner)?;
                }
                if release.owners.is_empty() {
                    self.page_root = Some(release.page);
                    self.phase = BuildPhase::Idle;
                } else {
                    self.phase = BuildPhase::PageRelease(release);
                }
                Ok(DriveOutcome::Progress)
            }
            BuildPhase::Idle => {
                if self.active_facts.len() == self.limits.facts_per_page
                    || (self.finish_requested && !self.active_facts.is_empty())
                {
                    self.start_page(session)?;
                    return Ok(DriveOutcome::Progress);
                }
                if self.finish_requested {
                    let payload = encode_root(self.authority, self.next_ordinal);
                    let children = self
                        .page_root
                        .as_ref()
                        .map_or_else(Vec::new, |root| vec![root.id()]);
                    let root = session.allocate(&payload, &children)?;
                    if let Some(page) = self.page_root.take() {
                        session.release(page)?;
                    }
                    self.phase = BuildPhase::Complete;
                    return Ok(DriveOutcome::Root(root));
                }
                self.phase = BuildPhase::Idle;
                Ok(DriveOutcome::Idle)
            }
            other => {
                self.phase = other;
                Err(ReferenceRootError::Corrupt("invalid active build phase"))
            }
        }
    }

    fn start_page(
        &mut self,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<(), ReferenceRootError> {
        let count = self.active_facts.len();
        if count == 0 {
            return Err(ReferenceRootError::Corrupt(
                "attempted empty occurrence page",
            ));
        }
        let count_u64 = u64::try_from(count).map_err(|_| ReferenceRootError::CapacityPreflight)?;
        let start = self
            .next_ordinal
            .checked_sub(count_u64)
            .ok_or(ReferenceRootError::Corrupt("page ordinal underflow"))?;
        let payload = encode_page(
            self.authority,
            start,
            count,
            self.next_ordinal,
            self.page_root.is_some(),
        )?;
        let mut child_ids = Vec::with_capacity(count + 1);
        if let Some(previous) = &self.page_root {
            child_ids.push(previous.id());
        }
        child_ids.extend(self.active_facts.iter().map(ArenaBuildOwner::id));
        let page = session.allocate(&payload, &child_ids)?;
        let mut owners = Vec::with_capacity(count + 1);
        if let Some(previous) = self.page_root.take() {
            owners.push(previous);
        }
        owners.append(&mut self.active_facts);
        self.phase = BuildPhase::PageRelease(PageRelease { page, owners });
        Ok(())
    }

    fn validate_fact(&self, fact: &AuthoritativeReferenceFact) -> Result<(), ReferenceRootError> {
        if fact.authority != self.authority {
            return Err(ReferenceRootError::CrossAuthority);
        }
        if !fact.source.is_valid()
            || !fact.label_source.is_valid()
            || !fact.destination_source.is_valid()
            || fact
                .title_source
                .as_ref()
                .is_some_and(|range| !range.is_valid())
            || !fact.source.contains(&fact.label_source)
            || !fact.source.contains(&fact.destination_source)
            || fact
                .title_source
                .as_ref()
                .is_some_and(|range| !fact.source.contains(range))
            || fact.title_source.is_some() != fact.cooked_title.is_some()
            || fact.source.bytes.end > self.authority.source_bytes
            || fact.source.utf16.end > self.authority.source_utf16
        {
            return Err(ReferenceRootError::InvalidRange);
        }
        if fact.source.bytes.start < self.last_source_byte_end
            || fact.source.utf16.start < self.last_source_utf16_end
        {
            return Err(ReferenceRootError::OutOfSourceOrder);
        }
        if fact.normalized_label.is_empty() {
            return Err(ReferenceRootError::EmptyNormalizedLabel);
        }
        std::str::from_utf8(&fact.normalized_label)
            .map_err(|_| ReferenceRootError::InvalidNormalizedLabelUtf8)?;
        if self.next_ordinal >= self.limits.max_occurrences {
            return Err(ReferenceRootError::OccurrenceLimit);
        }
        Ok(())
    }

    fn validate_stream_start(
        &self,
        fact: &AuthoritativeReferenceFactStart,
    ) -> Result<(), ReferenceRootError> {
        if fact.authority != self.authority {
            return Err(ReferenceRootError::CrossAuthority);
        }
        if !fact.source.is_valid()
            || !fact.label_source.is_valid()
            || !fact.destination_source.is_valid()
            || fact
                .title_source
                .as_ref()
                .is_some_and(|range| !range.is_valid())
            || !fact.source.contains(&fact.label_source)
            || !fact.source.contains(&fact.destination_source)
            || fact
                .title_source
                .as_ref()
                .is_some_and(|range| !fact.source.contains(range))
            || fact.title_source.is_some() != fact.title_len.is_some()
            || fact.source.bytes.end > self.authority.source_bytes
            || fact.source.utf16.end > self.authority.source_utf16
        {
            return Err(ReferenceRootError::InvalidRange);
        }
        if fact.source.bytes.start < self.last_source_byte_end
            || fact.source.utf16.start < self.last_source_utf16_end
        {
            return Err(ReferenceRootError::OutOfSourceOrder);
        }
        if fact.normalized_label.is_empty() {
            return Err(ReferenceRootError::EmptyNormalizedLabel);
        }
        std::str::from_utf8(&fact.normalized_label)
            .map_err(|_| ReferenceRootError::InvalidNormalizedLabelUtf8)?;
        if self.next_ordinal >= self.limits.max_occurrences {
            return Err(ReferenceRootError::OccurrenceLimit);
        }
        Ok(())
    }

    fn preflight_fact(
        &self,
        fact: &AuthoritativeReferenceFact,
        arena: &PageArena,
        reserve: ReferenceReserve,
    ) -> Result<(), ReferenceRootError> {
        let cooked_bytes = fact
            .normalized_label
            .len()
            .checked_add(fact.cooked_destination.len())
            .and_then(|bytes| {
                bytes.checked_add(fact.cooked_title.as_ref().map_or(0, |title| title.len()))
            })
            .ok_or(ReferenceRootError::FactTooLarge)?;
        if cooked_bytes > self.limits.max_cooked_bytes_per_fact {
            return Err(ReferenceRootError::FactTooLarge);
        }
        let (fact_nodes, fact_payload) = fact_storage_requirements(
            fact.normalized_label.len(),
            fact.cooked_destination.len(),
            fact.cooked_title.as_ref().map(|title| title.len()),
        )?;
        let required_nodes = fact_nodes
            .checked_add(2)
            .and_then(|nodes| nodes.checked_add(reserve.nodes))
            .ok_or(ReferenceRootError::CapacityPreflight)?;
        let required_payload = fact_payload
            .checked_add(PAGE_PAYLOAD_BYTES + ROOT_PAYLOAD_BYTES)
            .and_then(|bytes| bytes.checked_add(reserve.payload_bytes))
            .ok_or(ReferenceRootError::CapacityPreflight)?;
        preflight_remaining(arena, self.limits.arena, required_nodes, required_payload)
            .map_err(Into::into)
    }

    fn preflight_stream_start(
        &self,
        fact: &AuthoritativeReferenceFactStart,
        arena: &PageArena,
        reserve: ReferenceReserve,
    ) -> Result<(), ReferenceRootError> {
        let cooked_bytes = fact
            .normalized_label
            .len()
            .checked_add(fact.destination_len)
            .and_then(|bytes| bytes.checked_add(fact.title_len.unwrap_or(0)))
            .ok_or(ReferenceRootError::FactTooLarge)?;
        if cooked_bytes > self.limits.max_cooked_bytes_per_fact {
            return Err(ReferenceRootError::FactTooLarge);
        }
        let (fact_nodes, fact_payload) = fact_storage_requirements(
            fact.normalized_label.len(),
            fact.destination_len,
            fact.title_len,
        )?;
        let required_nodes = fact_nodes
            .checked_add(2)
            .and_then(|nodes| nodes.checked_add(reserve.nodes))
            .ok_or(ReferenceRootError::CapacityPreflight)?;
        let required_payload = fact_payload
            .checked_add(PAGE_PAYLOAD_BYTES + ROOT_PAYLOAD_BYTES)
            .and_then(|bytes| bytes.checked_add(reserve.payload_bytes))
            .ok_or(ReferenceRootError::CapacityPreflight)?;
        preflight_remaining(arena, self.limits.arena, required_nodes, required_payload)
            .map_err(Into::into)
    }
}

enum DriveOutcome {
    Progress,
    Idle,
    Root(ArenaBuildOwner),
}

fn blob_pages(bytes: usize) -> usize {
    bytes.max(1).div_ceil(BLOB_CHUNK_BYTES)
}

fn inline_fact_fits(
    label_len: usize,
    destination_len: usize,
    title_len: usize,
) -> Result<bool, ReferenceRootError> {
    label_len
        .checked_add(destination_len)
        .and_then(|bytes| bytes.checked_add(title_len))
        .map(|bytes| bytes <= INLINE_FACT_VALUE_BYTES)
        .ok_or(ReferenceRootError::FactTooLarge)
}

fn fact_storage_requirements(
    label_len: usize,
    destination_len: usize,
    title_len: Option<usize>,
) -> Result<(usize, usize), ReferenceRootError> {
    let cooked_bytes = label_len
        .checked_add(destination_len)
        .and_then(|bytes| bytes.checked_add(title_len.unwrap_or(0)))
        .ok_or(ReferenceRootError::CapacityPreflight)?;
    if cooked_bytes <= INLINE_FACT_VALUE_BYTES {
        return FACT_PAYLOAD_BYTES
            .checked_add(cooked_bytes)
            .map(|payload| (1, payload))
            .ok_or(ReferenceRootError::CapacityPreflight);
    }
    let blob_pages = blob_pages(label_len)
        .checked_add(blob_pages(destination_len))
        .and_then(|pages| pages.checked_add(title_len.map_or(0, blob_pages)))
        .ok_or(ReferenceRootError::CapacityPreflight)?;
    let nodes = blob_pages
        .checked_add(1)
        .ok_or(ReferenceRootError::CapacityPreflight)?;
    let payload = cooked_bytes
        .checked_add(blob_pages * (NODE_HEADER_BYTES + BLOB_METADATA_BYTES))
        .and_then(|bytes| bytes.checked_add(FACT_PAYLOAD_BYTES))
        .ok_or(ReferenceRootError::CapacityPreflight)?;
    Ok((nodes, payload))
}

pub(crate) struct ReferenceRootView<'a> {
    #[cfg(test)]
    arena: &'a PageArena,
    #[cfg(not(test))]
    _arena: PhantomData<&'a PageArena>,
    authority: ReferenceAuthority,
    count: u64,
    page_root: Option<ArenaId>,
    _not_sync: PhantomData<Cell<()>>,
}

/// Detached source-order traversal of one committed References root.
///
/// Occurrence pages point toward their older siblings, so reaching the first
/// occurrence requires one bounded descent over the page spine. The cursor
/// retains only page identities and subsequently visits every fact exactly
/// once; unlike repeated ordinal lookup, replay is linear in pages + facts.
pub(crate) struct ReferenceOccurrenceCursor {
    authority: ReferenceAuthority,
    count: u64,
    next_page: Option<ArenaId>,
    expected_page_end: u64,
    pages: Vec<ArenaId>,
    page: Option<ReferenceOccurrenceCursorPage>,
    next_ordinal: u64,
    descending: bool,
}

struct ReferenceOccurrenceCursorPage {
    id: ArenaId,
    next_fact: usize,
    count: usize,
    first_fact: usize,
}

pub(crate) enum ReferenceOccurrenceCursorPoll {
    Pending {},
    Occurrence {
        occurrence: DetachedReferenceOccurrence,
    },
    Complete {},
}

/// One occurrence detached from the arena borrow that produced it. Cooked
/// values remain represented by bounded copy cursors rather than contiguous
/// allocations.
pub(crate) struct DetachedReferenceOccurrence {
    pub(crate) source: ReferenceSourceRange,
    pub(crate) label_source: ReferenceSourceRange,
    pub(crate) destination_source: ReferenceSourceRange,
    pub(crate) title_source: Option<ReferenceSourceRange>,
    pub(crate) normalized_label: PersistentBytesCopyCursor,
    pub(crate) cooked_destination: PersistentBytesCopyCursor,
    pub(crate) cooked_title: Option<PersistentBytesCopyCursor>,
}

/// Arena-borrow-free, forward-only value copier. Blob pages are first
/// descended in bounded steps and then copied oldest-to-newest. This avoids
/// both value-sized materialization and the quadratic re-seeking of repeated
/// [`PersistentBytesView::read`] calls.
pub(crate) struct PersistentBytesCopyCursor {
    authority: ReferenceAuthority,
    len: u64,
    copied: u64,
    storage: PersistentBytesCopyStorage,
}

enum PersistentBytesCopyStorage {
    Inline {
        fact: ArenaId,
        start: usize,
    },
    Blob {
        kind: BlobKind,
        next: Option<ArenaId>,
        expected_end: u64,
        pages: Vec<ArenaId>,
        current: Option<(ArenaId, usize)>,
        descending: bool,
    },
}

pub(crate) struct PersistentBytesCopyPoll {
    pub(crate) written: usize,
}

impl<'a> ReferenceRootView<'a> {
    pub(crate) fn open(
        arena: &'a PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
    ) -> Result<Self, ReferenceRootError> {
        let descriptor = decode_root(arena, root, authority)?;
        if let Some(page) = descriptor.page_root {
            let latest = decode_page(arena, page, authority)?;
            if latest
                .start
                .checked_add(u64::from(latest.count))
                .is_none_or(|end| end != descriptor.count)
            {
                return Err(ReferenceRootError::Corrupt(
                    "latest occurrence page disagrees with root count",
                ));
            }
        }
        Ok(Self {
            #[cfg(test)]
            arena,
            #[cfg(not(test))]
            _arena: PhantomData,
            authority,
            count: descriptor.count,
            page_root: descriptor.page_root,
            _not_sync: PhantomData,
        })
    }

    pub(crate) fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn occurrences(&self) -> ReferenceOccurrenceCursor {
        ReferenceOccurrenceCursor {
            authority: self.authority,
            count: self.count,
            next_page: self.page_root,
            expected_page_end: self.count,
            pages: Vec::new(),
            page: None,
            next_ordinal: 0,
            descending: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn occurrence(
        &self,
        ordinal: u64,
    ) -> Result<Option<ReferenceOccurrenceView<'a>>, ReferenceRootError> {
        if ordinal >= self.count {
            return Ok(None);
        }
        let mut page = self.page_root;
        let mut expected_end = self.count;
        while let Some(page_id) = page {
            let descriptor = decode_page(self.arena, page_id, self.authority)?;
            let page_end = descriptor
                .start
                .checked_add(u64::from(descriptor.count))
                .ok_or(ReferenceRootError::Corrupt("page ordinal overflow"))?;
            if page_end != expected_end {
                return Err(ReferenceRootError::Corrupt(
                    "occurrence pages are not contiguous",
                ));
            }
            if (descriptor.start..page_end).contains(&ordinal) {
                let local = usize::try_from(ordinal - descriptor.start)
                    .map_err(|_| ReferenceRootError::Corrupt("local ordinal overflow"))?;
                let child_index = usize::from(descriptor.has_previous) + local;
                let fact_id = self.arena.child_at(page_id, child_index)?;
                return Ok(Some(decode_fact(self.arena, fact_id, self.authority)?));
            }
            expected_end = descriptor.start;
            page = descriptor.previous;
        }
        Err(ReferenceRootError::Corrupt(
            "occurrence missing from page chain",
        ))
    }

    #[cfg(test)]
    pub(crate) fn winner(
        &self,
        normalized_label: &str,
    ) -> Result<Option<ReferenceOccurrenceView<'a>>, ReferenceRootError> {
        // Intentionally synchronous in this storage proof. The production
        // host query must wrap this traversal in a resumable cursor or replace
        // it with the planned paged winner directory.
        let mut page = self.page_root;
        let mut winner = None;
        let mut expected_end = self.count;
        while let Some(page_id) = page {
            let descriptor = decode_page(self.arena, page_id, self.authority)?;
            let page_end = descriptor
                .start
                .checked_add(u64::from(descriptor.count))
                .ok_or(ReferenceRootError::Corrupt("page ordinal overflow"))?;
            if page_end != expected_end {
                return Err(ReferenceRootError::Corrupt(
                    "occurrence pages are not contiguous",
                ));
            }
            let first_fact = usize::from(descriptor.has_previous);
            for index in 0..usize::from(descriptor.count) {
                let fact_id = self.arena.child_at(page_id, first_fact + index)?;
                let fact = decode_fact(self.arena, fact_id, self.authority)?;
                if fact.normalized_label.equals(normalized_label.as_bytes())?
                    && winner.is_none_or(|(_, ordinal)| fact.ordinal < ordinal)
                {
                    winner = Some((fact_id, fact.ordinal));
                }
            }
            expected_end = descriptor.start;
            page = descriptor.previous;
        }
        if expected_end != 0 {
            return Err(ReferenceRootError::Corrupt(
                "occurrence page chain is incomplete",
            ));
        }
        winner
            .map(|(id, _)| decode_fact(self.arena, id, self.authority))
            .transpose()
    }
}

impl ReferenceOccurrenceCursor {
    pub(crate) fn poll_next(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<ReferenceOccurrenceCursorPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if self.descending {
                if let Some(page_id) = self.next_page {
                    let descriptor = decode_page(arena, page_id, self.authority)?;
                    let page_end = descriptor
                        .start
                        .checked_add(u64::from(descriptor.count))
                        .ok_or(ReferenceRootError::Corrupt("page ordinal overflow"))?;
                    if page_end != self.expected_page_end {
                        return Err(ReferenceRootError::Corrupt(
                            "occurrence pages are not contiguous",
                        ));
                    }
                    self.pages
                        .try_reserve(1)
                        .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
                    self.pages.push(page_id);
                    self.expected_page_end = descriptor.start;
                    self.next_page = descriptor.previous;
                    transitions += 1;
                    continue;
                }
                if self.expected_page_end != 0 {
                    return Err(ReferenceRootError::Corrupt(
                        "occurrence page chain is incomplete",
                    ));
                }
                self.descending = false;
                transitions += 1;
                continue;
            }

            if let Some(page) = self.page.as_mut() {
                if page.next_fact < page.count {
                    let fact_id = arena.child_at(page.id, page.first_fact + page.next_fact)?;
                    let occurrence = decode_fact(arena, fact_id, self.authority)?;
                    if occurrence.ordinal != self.next_ordinal {
                        return Err(ReferenceRootError::Corrupt(
                            "occurrence ordinal changed during replay",
                        ));
                    }
                    page.next_fact += 1;
                    self.next_ordinal = self
                        .next_ordinal
                        .checked_add(1)
                        .ok_or(ReferenceRootError::OccurrenceLimit)?;
                    return Ok(ReferenceOccurrenceCursorPoll::Occurrence {
                        occurrence: occurrence.detach(),
                    });
                }
                self.page = None;
                transitions += 1;
                continue;
            }

            if let Some(page_id) = self.pages.pop() {
                let descriptor = decode_page(arena, page_id, self.authority)?;
                if descriptor.start != self.next_ordinal {
                    return Err(ReferenceRootError::Corrupt(
                        "occurrence page order changed during replay",
                    ));
                }
                self.page = Some(ReferenceOccurrenceCursorPage {
                    id: page_id,
                    next_fact: 0,
                    count: usize::from(descriptor.count),
                    first_fact: usize::from(descriptor.has_previous),
                });
                transitions += 1;
                continue;
            }

            if self.next_ordinal != self.count {
                return Err(ReferenceRootError::Corrupt(
                    "occurrence replay ended before the root count",
                ));
            }
            return Ok(ReferenceOccurrenceCursorPoll::Complete {});
        }
        Ok(ReferenceOccurrenceCursorPoll::Pending {})
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReferenceWinnerEntry {
    fact: ArenaId,
}

#[derive(Debug)]
struct ReferenceWinnerBucket {
    first: ReferenceWinnerEntry,
    collisions: Vec<ReferenceWinnerEntry>,
    overflowed: bool,
}

impl ReferenceWinnerBucket {
    fn entries(&self) -> impl Iterator<Item = ReferenceWinnerEntry> + '_ {
        std::iter::once(self.first).chain(self.collisions.iter().copied())
    }
}

/// Revision-independent acceleration over one immutable canonical References
/// root. Entries retain only generation-checked arena ids; the publication
/// remains the authority and independently owns the referenced pages.
///
/// A B-tree is intentional here. Its insertion cost grows logarithmically and
/// never hides a whole-table resize inside one editor quantum. Digest buckets
/// still exact-compare canonical normalized-label bytes, so a digest collision
/// cannot change Markdown semantics.
pub(crate) struct ReferenceWinnerIndex {
    root: ArenaId,
    payload: Arc<ReferenceWinnerIndexPayload>,
}

struct ReferenceWinnerIndexPayload {
    occurrence_count: u64,
    indexed_occurrences: u64,
    skipped_oversized_occurrences: u64,
    buckets: BTreeMap<[u8; STRONG_DIGEST_BYTES], ReferenceWinnerBucket>,
}

impl fmt::Debug for ReferenceWinnerIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceWinnerIndex")
            .field("root", &self.root)
            .field("occurrence_count", &self.payload.occurrence_count)
            .field("indexed_occurrences", &self.payload.indexed_occurrences)
            .field(
                "skipped_oversized_occurrences",
                &self.payload.skipped_oversized_occurrences,
            )
            .field("digest_bucket_count", &self.payload.buckets.len())
            .finish()
    }
}

impl ReferenceWinnerIndex {
    pub(crate) const fn root(&self) -> ArenaId {
        self.root
    }

    pub(crate) fn occurrence_count(&self) -> u64 {
        self.payload.occurrence_count
    }

    pub(crate) fn indexed_occurrences(&self) -> u64 {
        self.payload.indexed_occurrences
    }

    #[cfg(test)]
    pub(crate) fn skipped_oversized_occurrences(&self) -> u64 {
        self.payload.skipped_oversized_occurrences
    }

    pub(crate) fn unique_label_count(&self) -> u64 {
        self.payload
            .buckets
            .values()
            .map(|bucket| 1_u64 + bucket.collisions.len() as u64)
            .sum()
    }

    pub(crate) fn winner<'a>(
        &self,
        arena: &'a PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
        normalized_label: &[u8],
    ) -> Result<Option<ReferenceOccurrenceView<'a>>, ReferenceRootError> {
        if root != self.root {
            return Err(ReferenceRootError::Corrupt(
                "reference winner index is bound to another root",
            ));
        }
        if normalized_label.is_empty()
            || normalized_label.len() as u64 > REFERENCE_WINNER_INDEX_MAX_NORMALIZED_LABEL_BYTES
        {
            return Ok(None);
        }
        let digest = reference_winner_label_digest(normalized_label);
        let Some(bucket) = self.payload.buckets.get(&digest) else {
            return Ok(None);
        };
        if bucket.overflowed {
            return Ok(None);
        }
        for entry in bucket.entries() {
            let occurrence = decode_fact(arena, entry.fact, authority)?;
            if occurrence.normalized_label.equals(normalized_label)? {
                return Ok(Some(occurrence));
            }
        }
        Ok(None)
    }

    pub(crate) fn into_reclaimer(self) -> ReferenceWinnerIndexReclaimer {
        ReferenceWinnerIndexReclaimer::from_shared(self.payload)
    }

    /// Rebinds revision-local lookup authority to another retained wrapper
    /// over the exact same canonical fact ids. The immutable buckets are
    /// shared in O(1); the last owner alone performs fuelled reclamation.
    pub(crate) fn rebind_root(&self, root: ArenaId) -> Self {
        Self {
            root,
            payload: Arc::clone(&self.payload),
        }
    }
}

/// Incremental exact-label authority for a reference root that is still being
/// built. Each occurrence is offered only after its canonical fact node has
/// entered the sibling arena journal. The label digest is then computed one
/// storage page per poll and the first occurrence for an exactly equal label
/// is retained. Later duplicates never replace that first winner.
///
/// This is the build-local counterpart of [`ReferenceWinnerIndexBuilder`]. It
/// avoids rescanning every prior occurrence after each paragraph while still
/// deriving all equality decisions from persistent canonical bytes.
pub(crate) struct ReferenceWinnerIndexJournal {
    pending: Option<ReferenceWinnerFactVisit>,
    occurrence_count: u64,
    indexed_occurrences: u64,
    skipped_oversized_occurrences: u64,
    buckets: BTreeMap<[u8; STRONG_DIGEST_BYTES], ReferenceWinnerBucket>,
    label_scratch: Vec<u8>,
}

impl fmt::Debug for ReferenceWinnerIndexJournal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceWinnerIndexJournal")
            .field("occurrence_count", &self.occurrence_count)
            .field("indexed_occurrences", &self.indexed_occurrences)
            .field(
                "skipped_oversized_occurrences",
                &self.skipped_oversized_occurrences,
            )
            .field("digest_bucket_count", &self.buckets.len())
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}

impl ReferenceWinnerIndexJournal {
    pub(crate) fn new() -> Self {
        Self {
            pending: None,
            occurrence_count: 0,
            indexed_occurrences: 0,
            skipped_oversized_occurrences: 0,
            buckets: BTreeMap::new(),
            label_scratch: Vec::new(),
        }
    }

    pub(crate) fn begin_occurrence(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        committed: ReferenceCommittedOccurrence,
    ) -> Result<(), ReferenceRootError> {
        if self.pending.is_some() {
            return Err(ReferenceRootError::Busy);
        }
        let occurrence = decode_fact(arena, committed.fact, authority)?;
        if committed.ordinal != self.occurrence_count || occurrence.ordinal != committed.ordinal {
            return Err(ReferenceRootError::OutOfSourceOrder);
        }
        if occurrence.normalized_label.len() > REFERENCE_WINNER_INDEX_MAX_NORMALIZED_LABEL_BYTES {
            self.skipped_oversized_occurrences = self
                .skipped_oversized_occurrences
                .checked_add(1)
                .ok_or(ReferenceRootError::FactTooLarge)?;
            self.occurrence_count = self
                .occurrence_count
                .checked_add(1)
                .ok_or(ReferenceRootError::OccurrenceLimit)?;
            return Ok(());
        }
        let digest = PersistentBytesDigestCursor::new(occurrence.normalized_label)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(REFERENCE_WINNER_LABEL_DIGEST_DOMAIN);
        self.pending = Some(ReferenceWinnerFactVisit {
            id: committed.fact,
            digest,
            hasher,
        });
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        fuel: usize,
    ) -> Result<ReferenceWinnerIndexBuildPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        let mut transitions = 0;
        while transitions < fuel {
            let Some(pending) = self.pending.as_mut() else {
                break;
            };
            let complete = pending
                .digest
                .drive_one(arena, authority, &mut pending.hasher)?;
            transitions += 1;
            if complete {
                let pending = self.pending.take().ok_or(ReferenceRootError::Corrupt(
                    "reference winner journal lost its pending fact",
                ))?;
                let digest = *pending.hasher.finalize().as_bytes();
                self.install_first_fact(arena, authority, pending.id, digest)?;
                self.indexed_occurrences = self
                    .indexed_occurrences
                    .checked_add(1)
                    .ok_or(ReferenceRootError::FactTooLarge)?;
                self.occurrence_count = self
                    .occurrence_count
                    .checked_add(1)
                    .ok_or(ReferenceRootError::OccurrenceLimit)?;
            }
        }
        Ok(ReferenceWinnerIndexBuildPoll {
            transitions,
            complete: self.pending.is_none(),
        })
    }

    pub(crate) fn finish(
        self,
        arena: &PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
    ) -> Result<ReferenceWinnerIndex, ReferenceRootError> {
        if self.pending.is_some() {
            return Err(ReferenceRootError::Busy);
        }
        let view = ReferenceRootView::open(arena, authority, root)?;
        if view.count != self.occurrence_count
            || self.indexed_occurrences + self.skipped_oversized_occurrences
                != self.occurrence_count
        {
            return Err(ReferenceRootError::Corrupt(
                "reference winner journal disagrees with canonical root",
            ));
        }
        Ok(ReferenceWinnerIndex {
            root,
            payload: Arc::new(ReferenceWinnerIndexPayload {
                occurrence_count: self.occurrence_count,
                indexed_occurrences: self.indexed_occurrences,
                skipped_oversized_occurrences: self.skipped_oversized_occurrences,
                buckets: self.buckets,
            }),
        })
    }

    pub(crate) fn into_reclaimer(mut self) -> ReferenceWinnerIndexReclaimer {
        self.pending = None;
        self.label_scratch.clear();
        ReferenceWinnerIndexReclaimer {
            buckets: Some(std::mem::take(&mut self.buckets)),
        }
    }

    fn install_first_fact(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        fact_id: ArenaId,
        digest: [u8; STRONG_DIGEST_BYTES],
    ) -> Result<(), ReferenceRootError> {
        let occurrence = decode_fact(arena, fact_id, authority)?;
        let label_len = usize::try_from(occurrence.normalized_label.len())
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        self.label_scratch.clear();
        self.label_scratch
            .try_reserve_exact(label_len)
            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
        self.label_scratch.resize(label_len, 0);
        if occurrence
            .normalized_label
            .read(0, &mut self.label_scratch)?
            != label_len
        {
            return Err(ReferenceRootError::Corrupt(
                "reference winner label read was truncated",
            ));
        }

        let candidate = ReferenceWinnerEntry { fact: fact_id };
        match self.buckets.entry(digest) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(ReferenceWinnerBucket {
                    first: candidate,
                    collisions: Vec::new(),
                    overflowed: false,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let bucket = entry.get_mut();
                if bucket.overflowed {
                    return Ok(());
                }
                for existing in bucket.entries() {
                    let existing = decode_fact(arena, existing.fact, authority)?;
                    if existing.normalized_label.equals(&self.label_scratch)? {
                        return Ok(());
                    }
                }
                if 1 + bucket.collisions.len() >= REFERENCE_WINNER_MAX_DIGEST_BUCKET_LABELS {
                    bucket.overflowed = true;
                } else {
                    bucket
                        .collisions
                        .try_reserve(1)
                        .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
                    bucket.collisions.push(candidate);
                }
            }
        }
        Ok(())
    }
}

/// Explicit one-bucket-at-a-time destruction for the heap-owned acceleration
/// map. Arena pages are not owned here; this cursor exists so closing a huge
/// document never recursively frees an entire B-tree in one UI quantum.
pub(crate) struct ReferenceWinnerIndexReclaimer {
    buckets: Option<BTreeMap<[u8; STRONG_DIGEST_BYTES], ReferenceWinnerBucket>>,
}

impl fmt::Debug for ReferenceWinnerIndexReclaimer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceWinnerIndexReclaimer")
            .field(
                "remaining_digest_buckets",
                &self.buckets.as_ref().map_or(0, BTreeMap::len),
            )
            .finish()
    }
}

impl ReferenceWinnerIndexReclaimer {
    fn from_shared(payload: Arc<ReferenceWinnerIndexPayload>) -> Self {
        Self {
            buckets: Arc::try_unwrap(payload).ok().map(|payload| payload.buckets),
        }
    }

    pub(crate) fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<ReferenceWinnerIndexBuildPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        let mut transitions = 0_usize;
        while transitions < fuel
            && self
                .buckets
                .as_mut()
                .is_some_and(|buckets| buckets.pop_first().is_some())
        {
            transitions += 1;
        }
        if self.buckets.as_ref().is_some_and(BTreeMap::is_empty) {
            self.buckets = None;
        }
        Ok(ReferenceWinnerIndexBuildPoll {
            transitions,
            complete: self.buckets.is_none(),
        })
    }
}

impl Drop for ReferenceWinnerIndexReclaimer {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.buckets.as_ref().is_none_or(BTreeMap::is_empty),
                "reference winner index requires fuelled reclamation"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReferenceWinnerPageVisit {
    id: ArenaId,
    start: u64,
    first_fact: usize,
    next_fact: usize,
}

struct ReferenceWinnerFactVisit {
    id: ArenaId,
    digest: PersistentBytesDigestCursor,
    hasher: blake3::Hasher,
}

impl fmt::Debug for ReferenceWinnerFactVisit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceWinnerFactVisit")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Fuel-driven builder for [`ReferenceWinnerIndex`]. One transition decodes
/// one occurrence page, starts one fact, hashes at most one bounded label
/// storage page, or commits one digest bucket update.
pub(crate) struct ReferenceWinnerIndexBuilder {
    root: ArenaId,
    occurrence_count: u64,
    next_page: Option<ArenaId>,
    expected_page_end: u64,
    page: Option<ReferenceWinnerPageVisit>,
    fact: Option<ReferenceWinnerFactVisit>,
    indexed_occurrences: u64,
    skipped_oversized_occurrences: u64,
    buckets: BTreeMap<[u8; STRONG_DIGEST_BYTES], ReferenceWinnerBucket>,
    label_scratch: Vec<u8>,
    complete: bool,
}

impl fmt::Debug for ReferenceWinnerIndexBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceWinnerIndexBuilder")
            .field("root", &self.root)
            .field("occurrence_count", &self.occurrence_count)
            .field("indexed_occurrences", &self.indexed_occurrences)
            .field(
                "skipped_oversized_occurrences",
                &self.skipped_oversized_occurrences,
            )
            .field("digest_bucket_count", &self.buckets.len())
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceWinnerIndexBuildPoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

impl ReferenceWinnerIndexBuilder {
    pub(crate) fn new(
        arena: &PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
    ) -> Result<Self, ReferenceRootError> {
        let view = ReferenceRootView::open(arena, authority, root)?;
        Ok(Self {
            root,
            occurrence_count: view.count,
            next_page: view.page_root,
            expected_page_end: view.count,
            page: None,
            fact: None,
            indexed_occurrences: 0,
            skipped_oversized_occurrences: 0,
            buckets: BTreeMap::new(),
            label_scratch: Vec::new(),
            complete: false,
        })
    }

    pub(crate) fn maximum_external_payload_bytes(&self) -> Result<usize, ReferenceRootError> {
        usize::try_from(self.occurrence_count)
            .ok()
            .and_then(|count| count.checked_mul(REFERENCE_WINNER_ACCOUNTED_BYTES_PER_OCCURRENCE))
            .and_then(|bytes| bytes.checked_add(REFERENCE_WINNER_ACCOUNTED_FIXED_BYTES))
            .ok_or(ReferenceRootError::FactTooLarge)
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
        fuel: usize,
    ) -> Result<ReferenceWinnerIndexBuildPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        if root != self.root {
            return Err(ReferenceRootError::Corrupt(
                "reference winner builder is bound to another root",
            ));
        }
        if self.complete {
            return Ok(ReferenceWinnerIndexBuildPoll {
                transitions: 0,
                complete: true,
            });
        }

        let mut transitions = 0_usize;
        while transitions < fuel {
            if let Some(fact) = self.fact.as_mut() {
                let complete = fact.digest.drive_one(arena, authority, &mut fact.hasher)?;
                transitions += 1;
                if complete {
                    let visit = self.fact.take().ok_or(ReferenceRootError::Corrupt(
                        "reference winner fact disappeared",
                    ))?;
                    let digest = *visit.hasher.finalize().as_bytes();
                    self.install_fact(arena, authority, visit.id, digest)?;
                }
                continue;
            }

            if let Some(page) = self.page.as_mut() {
                if page.next_fact == 0 {
                    self.page = None;
                    transitions += 1;
                    continue;
                }
                page.next_fact -= 1;
                let local = page.next_fact;
                let fact_id = arena.child_at(page.id, page.first_fact + local)?;
                let occurrence = decode_fact(arena, fact_id, authority)?;
                let expected_ordinal =
                    page.start
                        .checked_add(local as u64)
                        .ok_or(ReferenceRootError::Corrupt(
                            "reference winner ordinal overflow",
                        ))?;
                if occurrence.ordinal != expected_ordinal {
                    return Err(ReferenceRootError::Corrupt(
                        "reference winner fact ordinal changed",
                    ));
                }
                if occurrence.normalized_label.len()
                    > REFERENCE_WINNER_INDEX_MAX_NORMALIZED_LABEL_BYTES
                {
                    self.skipped_oversized_occurrences = self
                        .skipped_oversized_occurrences
                        .checked_add(1)
                        .ok_or(ReferenceRootError::FactTooLarge)?;
                } else {
                    let digest = PersistentBytesDigestCursor::new(occurrence.normalized_label)?;
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(REFERENCE_WINNER_LABEL_DIGEST_DOMAIN);
                    self.fact = Some(ReferenceWinnerFactVisit {
                        id: fact_id,
                        digest,
                        hasher,
                    });
                }
                transitions += 1;
                continue;
            }

            if let Some(page_id) = self.next_page {
                let descriptor = decode_page(arena, page_id, authority)?;
                let page_end = descriptor
                    .start
                    .checked_add(u64::from(descriptor.count))
                    .ok_or(ReferenceRootError::Corrupt(
                        "reference winner page ordinal overflow",
                    ))?;
                if page_end != self.expected_page_end {
                    return Err(ReferenceRootError::Corrupt(
                        "reference winner pages are not contiguous",
                    ));
                }
                self.next_page = descriptor.previous;
                self.expected_page_end = descriptor.start;
                self.page = Some(ReferenceWinnerPageVisit {
                    id: page_id,
                    start: descriptor.start,
                    first_fact: usize::from(descriptor.has_previous),
                    next_fact: usize::from(descriptor.count),
                });
                transitions += 1;
                continue;
            }

            if self.expected_page_end != 0
                || self.indexed_occurrences + self.skipped_oversized_occurrences
                    != self.occurrence_count
            {
                return Err(ReferenceRootError::Corrupt(
                    "reference winner page chain is incomplete",
                ));
            }
            self.complete = true;
            transitions += 1;
            return Ok(ReferenceWinnerIndexBuildPoll {
                transitions,
                complete: true,
            });
        }
        Ok(ReferenceWinnerIndexBuildPoll {
            transitions,
            complete: false,
        })
    }

    pub(crate) fn into_index(self) -> Result<ReferenceWinnerIndex, ReferenceRootError> {
        if !self.complete || self.page.is_some() || self.fact.is_some() {
            return Err(ReferenceRootError::Busy);
        }
        Ok(ReferenceWinnerIndex {
            root: self.root,
            payload: Arc::new(ReferenceWinnerIndexPayload {
                occurrence_count: self.occurrence_count,
                indexed_occurrences: self.indexed_occurrences,
                skipped_oversized_occurrences: self.skipped_oversized_occurrences,
                buckets: self.buckets,
            }),
        })
    }

    pub(crate) fn into_reclaimer(mut self) -> ReferenceWinnerIndexReclaimer {
        self.fact = None;
        self.page = None;
        self.label_scratch.clear();
        ReferenceWinnerIndexReclaimer {
            buckets: Some(std::mem::take(&mut self.buckets)),
        }
    }

    fn install_fact(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        fact_id: ArenaId,
        digest: [u8; STRONG_DIGEST_BYTES],
    ) -> Result<(), ReferenceRootError> {
        let occurrence = decode_fact(arena, fact_id, authority)?;
        let label_len = usize::try_from(occurrence.normalized_label.len())
            .map_err(|_| ReferenceRootError::FactTooLarge)?;
        if label_len as u64 > REFERENCE_WINNER_INDEX_MAX_NORMALIZED_LABEL_BYTES {
            return Err(ReferenceRootError::Corrupt(
                "oversized reference entered winner bucket",
            ));
        }
        self.label_scratch.clear();
        self.label_scratch
            .try_reserve_exact(label_len)
            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
        self.label_scratch.resize(label_len, 0);
        if occurrence
            .normalized_label
            .read(0, &mut self.label_scratch)?
            != label_len
        {
            return Err(ReferenceRootError::Corrupt(
                "reference winner label read was truncated",
            ));
        }

        let candidate = ReferenceWinnerEntry { fact: fact_id };
        match self.buckets.entry(digest) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(ReferenceWinnerBucket {
                    first: candidate,
                    collisions: Vec::new(),
                    overflowed: false,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let bucket = entry.get_mut();
                if bucket.overflowed {
                    self.indexed_occurrences = self
                        .indexed_occurrences
                        .checked_add(1)
                        .ok_or(ReferenceRootError::FactTooLarge)?;
                    return Ok(());
                }
                let mut matching = None;
                for (index, existing) in bucket.entries().enumerate() {
                    let existing = decode_fact(arena, existing.fact, authority)?;
                    if existing.normalized_label.equals(&self.label_scratch)? {
                        matching = Some(index);
                        break;
                    }
                }
                match matching {
                    Some(0) => bucket.first = candidate,
                    Some(index) => bucket.collisions[index - 1] = candidate,
                    None => {
                        if 1 + bucket.collisions.len() >= REFERENCE_WINNER_MAX_DIGEST_BUCKET_LABELS
                        {
                            bucket.overflowed = true;
                            self.indexed_occurrences = self
                                .indexed_occurrences
                                .checked_add(1)
                                .ok_or(ReferenceRootError::FactTooLarge)?;
                            return Ok(());
                        }
                        bucket
                            .collisions
                            .try_reserve(1)
                            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
                        bucket.collisions.push(candidate);
                    }
                }
            }
        }
        self.indexed_occurrences = self
            .indexed_occurrences
            .checked_add(1)
            .ok_or(ReferenceRootError::FactTooLarge)?;
        Ok(())
    }
}

fn reference_winner_label_digest(label: &[u8]) -> [u8; STRONG_DIGEST_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(REFERENCE_WINNER_LABEL_DIGEST_DOMAIN);
    hasher.update(label);
    *hasher.finalize().as_bytes()
}

/// Fuel-bounded independent verification of the canonical References role.
///
/// The manifest stores the claimed role digest, while the arena stores the
/// paged fact/blob closure. A host must recompute the former from the latter;
/// merely copying the manifest payload would trust an unverified claim. This
/// cursor walks pages and cooked-value chains without flattening either.
pub(crate) struct ReferenceRoleDigestValidator {
    authority: CandidateAuthority,
    expected: RoleMetadata,
    next_page: Option<ArenaId>,
    expected_page_end: u64,
    pages: Vec<ArenaId>,
    page: Option<ReferencePageVisit>,
    fact: Option<ReferenceFactDigestCursor>,
    next_ordinal: u64,
    last_source_byte_end: u64,
    last_source_utf16_end: u64,
    canonical_bytes: u64,
    digest: blake3::Hasher,
    phase: ReferenceValidationPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceValidationPhase {
    DescendPages,
    VisitFacts,
    Complete,
}

struct ReferencePageVisit {
    id: ArenaId,
    next_fact: usize,
    count: usize,
    first_fact: usize,
}

struct ReferenceFactDigestCursor {
    canonical_bytes: u64,
    hasher: blake3::Hasher,
    destination: Option<PersistentBytesDigestCursor>,
    title: Option<PersistentBytesDigestCursor>,
    value: PersistentBytesDigestCursor,
    stage: FactDigestStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FactDigestStage {
    Label,
    Destination,
    Title,
}

struct BlobDigestCursor {
    kind: BlobKind,
    total_len: u64,
    next: Option<ArenaId>,
    expected_end: u64,
    pages: Vec<ArenaId>,
    hashing: bool,
}

enum PersistentBytesDigestCursor {
    Inline {
        fact: ArenaId,
        start: usize,
        len: usize,
        complete: bool,
    },
    Blob(BlobDigestCursor),
}

pub(crate) struct ReferenceRoleValidationPoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

impl ReferenceRoleDigestValidator {
    pub(crate) fn new(
        arena: &PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
        expected: RoleMetadata,
    ) -> Result<Self, ReferenceRootError> {
        if expected.role != CandidateRole::References || expected.schema != REFERENCES_SCHEMA {
            return Err(ReferenceRootError::Corrupt(
                "invalid reference role metadata",
            ));
        }
        let descriptor = decode_root(arena, root, authority)?;
        if descriptor.count != expected.record_count {
            return Err(ReferenceRootError::Corrupt("reference role count changed"));
        }
        Ok(Self {
            authority,
            expected,
            next_page: descriptor.page_root,
            expected_page_end: descriptor.count,
            pages: Vec::new(),
            page: None,
            fact: None,
            next_ordinal: 0,
            last_source_byte_end: 0,
            last_source_utf16_end: 0,
            canonical_bytes: 0,
            digest: role_hasher(authority, CandidateRole::References, REFERENCES_SCHEMA),
            phase: ReferenceValidationPhase::DescendPages,
        })
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<ReferenceRoleValidationPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        let mut transitions = 0;
        while transitions < fuel && self.phase != ReferenceValidationPhase::Complete {
            self.drive_one(arena)?;
            transitions += 1;
        }
        Ok(ReferenceRoleValidationPoll {
            transitions,
            complete: self.phase == ReferenceValidationPhase::Complete,
        })
    }

    fn drive_one(&mut self, arena: &PageArena) -> Result<(), ReferenceRootError> {
        if let Some(fact) = self.fact.as_mut() {
            if let Some((canonical_bytes, digest)) = fact.drive_one(arena, self.authority)? {
                self.canonical_bytes = self
                    .canonical_bytes
                    .checked_add(canonical_bytes)
                    .ok_or(ReferenceRootError::FactTooLarge)?;
                hash_record_digest(&mut self.digest, canonical_bytes, digest);
                self.next_ordinal = self
                    .next_ordinal
                    .checked_add(1)
                    .ok_or(ReferenceRootError::OccurrenceLimit)?;
                self.fact = None;
            }
            return Ok(());
        }

        if let Some(page) = self.page.as_mut() {
            if page.next_fact == page.count {
                self.page = None;
                return Ok(());
            }
            let fact_id = arena.child_at(page.id, page.first_fact + page.next_fact)?;
            let occurrence = decode_fact(arena, fact_id, self.authority)?;
            if occurrence.ordinal != self.next_ordinal
                || !occurrence.source.is_valid()
                || !occurrence.label_source.is_valid()
                || !occurrence.destination_source.is_valid()
                || occurrence
                    .title_source
                    .as_ref()
                    .is_some_and(|range| !range.is_valid())
                || !occurrence.source.contains(&occurrence.label_source)
                || !occurrence.source.contains(&occurrence.destination_source)
                || occurrence
                    .title_source
                    .as_ref()
                    .is_some_and(|range| !occurrence.source.contains(range))
                || occurrence.source.bytes.start < self.last_source_byte_end
                || occurrence.source.utf16.start < self.last_source_utf16_end
                || occurrence.source.bytes.end > self.authority.source_bytes
                || occurrence.source.utf16.end > self.authority.source_utf16
            {
                return Err(ReferenceRootError::Corrupt(
                    "reference fact authority or source order changed",
                ));
            }
            self.last_source_byte_end = occurrence.source.bytes.end;
            self.last_source_utf16_end = occurrence.source.utf16.end;
            self.fact = Some(ReferenceFactDigestCursor::new(self.authority, occurrence)?);
            page.next_fact += 1;
            return Ok(());
        }

        match self.phase {
            ReferenceValidationPhase::DescendPages => {
                if let Some(page_id) = self.next_page {
                    let descriptor = decode_page(arena, page_id, self.authority)?;
                    let page_end = descriptor
                        .start
                        .checked_add(u64::from(descriptor.count))
                        .ok_or(ReferenceRootError::Corrupt("page ordinal overflow"))?;
                    if page_end != self.expected_page_end {
                        return Err(ReferenceRootError::Corrupt(
                            "occurrence pages are not contiguous",
                        ));
                    }
                    self.pages
                        .try_reserve(1)
                        .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
                    self.pages.push(page_id);
                    self.expected_page_end = descriptor.start;
                    self.next_page = descriptor.previous;
                } else {
                    if self.expected_page_end != 0 {
                        return Err(ReferenceRootError::Corrupt(
                            "occurrence page chain is incomplete",
                        ));
                    }
                    self.phase = ReferenceValidationPhase::VisitFacts;
                }
            }
            ReferenceValidationPhase::VisitFacts => {
                if let Some(page_id) = self.pages.pop() {
                    let descriptor = decode_page(arena, page_id, self.authority)?;
                    self.page = Some(ReferencePageVisit {
                        id: page_id,
                        next_fact: 0,
                        count: usize::from(descriptor.count),
                        first_fact: usize::from(descriptor.has_previous),
                    });
                } else {
                    let digest = finalize_role_digest(
                        self.digest.clone(),
                        self.next_ordinal,
                        self.canonical_bytes,
                    );
                    if self.next_ordinal != self.expected.record_count
                        || self.canonical_bytes != self.expected.canonical_bytes
                        || digest != self.expected.digest
                    {
                        return Err(ReferenceRootError::Corrupt("reference role digest changed"));
                    }
                    self.phase = ReferenceValidationPhase::Complete;
                }
            }
            ReferenceValidationPhase::Complete => {}
        }
        Ok(())
    }
}

impl ReferenceFactDigestCursor {
    fn new(
        authority: CandidateAuthority,
        occurrence: ReferenceOccurrenceView<'_>,
    ) -> Result<Self, ReferenceRootError> {
        let label_len = occurrence.normalized_label.len;
        let destination_len = occurrence.cooked_destination.len;
        let title_len = occurrence
            .cooked_title
            .as_ref()
            .map_or(0, |title| title.len);
        let canonical_bytes = 8_u64
            .checked_add(3 * 32)
            .and_then(|bytes| bytes.checked_add(1))
            .and_then(|bytes| {
                bytes.checked_add(if occurrence.title_source.is_some() {
                    32
                } else {
                    0
                })
            })
            .and_then(|bytes| bytes.checked_add(3 * 8))
            .and_then(|bytes| bytes.checked_add(label_len))
            .and_then(|bytes| bytes.checked_add(destination_len))
            .and_then(|bytes| bytes.checked_add(title_len))
            .ok_or(ReferenceRootError::FactTooLarge)?;
        let hasher = canonical_fact_hasher(
            authority,
            occurrence.ordinal,
            &occurrence.source,
            &occurrence.label_source,
            &occurrence.destination_source,
            occurrence.title_source.as_ref(),
            label_len,
            destination_len,
            title_len,
        );
        let label = PersistentBytesDigestCursor::new(occurrence.normalized_label)?;
        let destination = PersistentBytesDigestCursor::new(occurrence.cooked_destination)?;
        let title = occurrence
            .cooked_title
            .map(PersistentBytesDigestCursor::new)
            .transpose()?;
        Ok(Self {
            canonical_bytes,
            hasher,
            destination: Some(destination),
            title,
            value: label,
            stage: FactDigestStage::Label,
        })
    }

    fn drive_one(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
    ) -> Result<Option<(u64, [u8; STRONG_DIGEST_BYTES])>, ReferenceRootError> {
        if !self.value.drive_one(arena, authority, &mut self.hasher)? {
            return Ok(None);
        }
        match self.stage {
            FactDigestStage::Label => {
                self.value = self.destination.take().ok_or(ReferenceRootError::Corrupt(
                    "reference digest lost destination",
                ))?;
                self.stage = FactDigestStage::Destination;
                Ok(None)
            }
            FactDigestStage::Destination => {
                if let Some(title) = self.title.take() {
                    self.value = title;
                    self.stage = FactDigestStage::Title;
                    Ok(None)
                } else {
                    Ok(Some((
                        self.canonical_bytes,
                        *self.hasher.finalize().as_bytes(),
                    )))
                }
            }
            FactDigestStage::Title => Ok(Some((
                self.canonical_bytes,
                *self.hasher.finalize().as_bytes(),
            ))),
        }
    }
}

impl PersistentBytesDigestCursor {
    fn new(view: PersistentBytesView<'_>) -> Result<Self, ReferenceRootError> {
        let len = usize::try_from(view.len)
            .map_err(|_| ReferenceRootError::Corrupt("value digest length overflow"))?;
        Ok(match view.storage {
            PersistentBytesStorage::Inline { fact, start } => Self::Inline {
                fact,
                start,
                len,
                complete: false,
            },
            PersistentBytesStorage::Blob { kind, root } => {
                Self::Blob(BlobDigestCursor::new(kind, root, view.len))
            }
        })
    }

    fn drive_one(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        hasher: &mut blake3::Hasher,
    ) -> Result<bool, ReferenceRootError> {
        match self {
            Self::Inline {
                fact,
                start,
                len,
                complete,
            } => {
                if *complete {
                    return Ok(true);
                }
                let payload = arena.payload(*fact)?;
                decode_header(payload, INLINE_FACT_TAG, authority)?;
                let end = start
                    .checked_add(*len)
                    .ok_or(ReferenceRootError::Corrupt("inline digest range overflow"))?;
                let bytes = payload
                    .get(*start..end)
                    .ok_or(ReferenceRootError::Corrupt("inline digest range changed"))?;
                hasher.update(bytes);
                *complete = true;
                Ok(true)
            }
            Self::Blob(blob) => blob.drive_one(arena, authority, hasher),
        }
    }
}

impl BlobDigestCursor {
    fn new(kind: BlobKind, root: ArenaId, total_len: u64) -> Self {
        Self {
            kind,
            total_len,
            next: Some(root),
            expected_end: total_len,
            pages: Vec::new(),
            hashing: false,
        }
    }

    fn drive_one(
        &mut self,
        arena: &PageArena,
        authority: CandidateAuthority,
        hasher: &mut blake3::Hasher,
    ) -> Result<bool, ReferenceRootError> {
        if !self.hashing {
            let id = self
                .next
                .ok_or(ReferenceRootError::Corrupt("blob chain ended early"))?;
            let descriptor = decode_blob(arena, id, authority, self.kind)?;
            let end = descriptor
                .chunk_start
                .checked_add(u64::from(descriptor.chunk_len))
                .ok_or(ReferenceRootError::Corrupt("blob chunk overflow"))?;
            if descriptor.total_len != self.total_len || end != self.expected_end {
                return Err(ReferenceRootError::Corrupt(
                    "blob chunks are not contiguous",
                ));
            }
            self.pages
                .try_reserve(1)
                .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
            self.pages.push(id);
            self.expected_end = descriptor.chunk_start;
            self.next = descriptor.previous;
            if descriptor.chunk_start == 0 {
                self.hashing = true;
            }
            return Ok(false);
        }

        let Some(id) = self.pages.pop() else {
            return Ok(true);
        };
        let descriptor = decode_blob(arena, id, authority, self.kind)?;
        let payload = arena.payload(id)?;
        let start = NODE_HEADER_BYTES + BLOB_METADATA_BYTES;
        hasher.update(&payload[start..start + usize::from(descriptor.chunk_len)]);
        Ok(self.pages.is_empty())
    }
}

pub(crate) struct ReferenceOccurrenceView<'a> {
    pub(crate) ordinal: u64,
    pub(crate) source: ReferenceSourceRange,
    pub(crate) label_source: ReferenceSourceRange,
    pub(crate) destination_source: ReferenceSourceRange,
    pub(crate) title_source: Option<ReferenceSourceRange>,
    pub(crate) normalized_label: PersistentBytesView<'a>,
    pub(crate) cooked_destination: PersistentBytesView<'a>,
    pub(crate) cooked_title: Option<PersistentBytesView<'a>>,
}

pub(crate) struct PersistentBytesView<'a> {
    arena: &'a PageArena,
    authority: ReferenceAuthority,
    storage: PersistentBytesStorage,
    len: u64,
    _not_sync: PhantomData<Cell<()>>,
}

#[derive(Clone, Copy)]
enum PersistentBytesStorage {
    Inline { fact: ArenaId, start: usize },
    Blob { kind: BlobKind, root: ArenaId },
}

impl ReferenceOccurrenceView<'_> {
    fn detach(self) -> DetachedReferenceOccurrence {
        DetachedReferenceOccurrence {
            source: self.source,
            label_source: self.label_source,
            destination_source: self.destination_source,
            title_source: self.title_source,
            normalized_label: self.normalized_label.into_copy_cursor(),
            cooked_destination: self.cooked_destination.into_copy_cursor(),
            cooked_title: self.cooked_title.map(PersistentBytesView::into_copy_cursor),
        }
    }
}

impl PersistentBytesView<'_> {
    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    fn into_copy_cursor(self) -> PersistentBytesCopyCursor {
        let storage = match self.storage {
            PersistentBytesStorage::Inline { fact, start } => {
                PersistentBytesCopyStorage::Inline { fact, start }
            }
            PersistentBytesStorage::Blob { kind, root } => PersistentBytesCopyStorage::Blob {
                kind,
                next: Some(root),
                expected_end: self.len,
                pages: Vec::new(),
                current: None,
                descending: true,
            },
        };
        PersistentBytesCopyCursor {
            authority: self.authority,
            len: self.len,
            copied: 0,
            storage,
        }
    }

    pub(crate) fn read(&self, offset: u64, output: &mut [u8]) -> Result<usize, ReferenceRootError> {
        // Random-access proof only. Repeated reads across a giant blob should
        // use a future resumable reverse-page cursor rather than re-seeking.
        if offset >= self.len || output.is_empty() {
            return Ok(0);
        }
        let permitted = usize::try_from((self.len - offset).min(output.len() as u64))
            .map_err(|_| ReferenceRootError::Corrupt("value read length overflow"))?;
        if matches!(self.storage, PersistentBytesStorage::Inline { .. }) {
            let bytes = self.inline_bytes()?;
            let start = usize::try_from(offset)
                .map_err(|_| ReferenceRootError::Corrupt("inline value offset overflow"))?;
            output[..permitted].copy_from_slice(&bytes[start..start + permitted]);
            return Ok(permitted);
        }
        let mut written = 0;
        while written < permitted {
            let target = offset + written as u64;
            let (node, chunk) = self.find_chunk(target)?;
            let relative = usize::try_from(target - node.chunk_start)
                .map_err(|_| ReferenceRootError::Corrupt("blob offset overflow"))?;
            let available = chunk.len() - relative;
            let take = available.min(permitted - written);
            output[written..written + take].copy_from_slice(&chunk[relative..relative + take]);
            written += take;
        }
        Ok(written)
    }

    pub(crate) fn equals(&self, expected: &[u8]) -> Result<bool, ReferenceRootError> {
        if self.len != expected.len() as u64 {
            return Ok(false);
        }
        if matches!(self.storage, PersistentBytesStorage::Inline { .. }) {
            return Ok(self.inline_bytes()? == expected);
        }
        let PersistentBytesStorage::Blob { kind, root } = self.storage else {
            unreachable!();
        };
        let mut node_id = root;
        let mut expected_end = self.len;
        loop {
            let descriptor = decode_blob(self.arena, node_id, self.authority, kind)?;
            let chunk_end = descriptor
                .chunk_start
                .checked_add(u64::from(descriptor.chunk_len))
                .ok_or(ReferenceRootError::Corrupt("blob chunk overflow"))?;
            if chunk_end != expected_end {
                return Err(ReferenceRootError::Corrupt(
                    "blob chunks are not contiguous",
                ));
            }
            let start = usize::try_from(descriptor.chunk_start)
                .map_err(|_| ReferenceRootError::Corrupt("blob chunk start overflow"))?;
            let end = usize::try_from(chunk_end)
                .map_err(|_| ReferenceRootError::Corrupt("blob chunk end overflow"))?;
            let payload = self.arena.payload(node_id)?;
            let chunk_start = NODE_HEADER_BYTES + BLOB_METADATA_BYTES;
            if payload[chunk_start..] != expected[start..end] {
                return Ok(false);
            }
            if start == 0 {
                return Ok(true);
            }
            node_id = descriptor
                .previous
                .ok_or(ReferenceRootError::Corrupt("blob predecessor missing"))?;
            expected_end = descriptor.chunk_start;
        }
    }

    fn find_chunk(&self, target: u64) -> Result<(BlobDescriptor, &[u8]), ReferenceRootError> {
        let PersistentBytesStorage::Blob { kind, root } = self.storage else {
            return Err(ReferenceRootError::Corrupt(
                "inline value entered blob lookup",
            ));
        };
        let mut node_id = root;
        loop {
            let descriptor = decode_blob(self.arena, node_id, self.authority, kind)?;
            let end = descriptor
                .chunk_start
                .checked_add(u64::from(descriptor.chunk_len))
                .ok_or(ReferenceRootError::Corrupt("blob chunk overflow"))?;
            if (descriptor.chunk_start..end).contains(&target) {
                let payload = self.arena.payload(node_id)?;
                let start = NODE_HEADER_BYTES + BLOB_METADATA_BYTES;
                return Ok((
                    descriptor,
                    &payload[start..start + usize::from(descriptor.chunk_len)],
                ));
            }
            node_id = descriptor
                .previous
                .ok_or(ReferenceRootError::Corrupt("blob offset has no page"))?;
        }
    }

    fn inline_bytes(&self) -> Result<&[u8], ReferenceRootError> {
        let PersistentBytesStorage::Inline { fact, start } = self.storage else {
            return Err(ReferenceRootError::Corrupt(
                "blob value entered inline lookup",
            ));
        };
        let payload = self.arena.payload(fact)?;
        decode_header(payload, INLINE_FACT_TAG, self.authority)?;
        let len = usize::try_from(self.len)
            .map_err(|_| ReferenceRootError::Corrupt("inline value length overflow"))?;
        let end = start
            .checked_add(len)
            .ok_or(ReferenceRootError::Corrupt("inline value range overflow"))?;
        payload
            .get(start..end)
            .ok_or(ReferenceRootError::Corrupt("inline value range changed"))
    }
}

impl PersistentBytesCopyCursor {
    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    pub(crate) fn complete(&self) -> bool {
        self.copied == self.len
    }

    pub(crate) fn poll_copy(
        &mut self,
        arena: &PageArena,
        output: &mut [u8],
        fuel: usize,
    ) -> Result<PersistentBytesCopyPoll, ReferenceRootError> {
        if fuel == 0 {
            return Err(ReferenceRootError::ZeroFuel);
        }
        if self.complete() {
            return Ok(PersistentBytesCopyPoll { written: 0 });
        }

        let mut transitions = 0;
        let mut written = 0;
        while transitions < fuel && self.copied < self.len {
            match &mut self.storage {
                PersistentBytesCopyStorage::Inline { fact, start } => {
                    if output.len() == written {
                        break;
                    }
                    let payload = arena.payload(*fact)?;
                    decode_header(payload, INLINE_FACT_TAG, self.authority)?;
                    let copied = usize::try_from(self.copied)
                        .map_err(|_| ReferenceRootError::Corrupt("inline copy offset overflow"))?;
                    let remaining = usize::try_from(self.len - self.copied)
                        .map_err(|_| ReferenceRootError::Corrupt("inline copy length overflow"))?;
                    let take = remaining.min(output.len() - written);
                    let value_start = start
                        .checked_add(copied)
                        .ok_or(ReferenceRootError::Corrupt("inline copy range overflow"))?;
                    let value_end = value_start
                        .checked_add(take)
                        .ok_or(ReferenceRootError::Corrupt("inline copy range overflow"))?;
                    let source = payload
                        .get(value_start..value_end)
                        .ok_or(ReferenceRootError::Corrupt("inline copy range changed"))?;
                    output[written..written + take].copy_from_slice(source);
                    self.copied += take as u64;
                    written += take;
                    transitions += 1;
                }
                PersistentBytesCopyStorage::Blob {
                    kind,
                    next,
                    expected_end,
                    pages,
                    current,
                    descending,
                } => {
                    if *descending {
                        let page_id =
                            next.ok_or(ReferenceRootError::Corrupt("blob copy chain ended early"))?;
                        let descriptor = decode_blob(arena, page_id, self.authority, *kind)?;
                        let end = descriptor
                            .chunk_start
                            .checked_add(u64::from(descriptor.chunk_len))
                            .ok_or(ReferenceRootError::Corrupt("blob chunk overflow"))?;
                        if descriptor.total_len != self.len || end != *expected_end {
                            return Err(ReferenceRootError::Corrupt(
                                "blob copy chunks are not contiguous",
                            ));
                        }
                        pages
                            .try_reserve(1)
                            .map_err(|_| ReferenceRootError::Arena(ArenaError::AllocationFailed))?;
                        pages.push(page_id);
                        *expected_end = descriptor.chunk_start;
                        *next = descriptor.previous;
                        if descriptor.chunk_start == 0 {
                            *descending = false;
                        }
                        transitions += 1;
                        continue;
                    }

                    if output.len() == written {
                        break;
                    }
                    if current.is_none() {
                        let page_id = pages.pop().ok_or(ReferenceRootError::Corrupt(
                            "blob copy page stack ended early",
                        ))?;
                        *current = Some((page_id, 0));
                    }
                    let (page_id, page_offset) = current.as_mut().ok_or(
                        ReferenceRootError::Corrupt("blob copy lost its current page"),
                    )?;
                    let descriptor = decode_blob(arena, *page_id, self.authority, *kind)?;
                    let chunk_len = usize::from(descriptor.chunk_len);
                    if *page_offset > chunk_len {
                        return Err(ReferenceRootError::Corrupt(
                            "blob copy page offset escaped its chunk",
                        ));
                    }
                    let take = (chunk_len - *page_offset).min(output.len() - written);
                    let payload = arena.payload(*page_id)?;
                    let chunk_start = NODE_HEADER_BYTES + BLOB_METADATA_BYTES + *page_offset;
                    let chunk_end = chunk_start
                        .checked_add(take)
                        .ok_or(ReferenceRootError::Corrupt("blob copy range overflow"))?;
                    let source = payload
                        .get(chunk_start..chunk_end)
                        .ok_or(ReferenceRootError::Corrupt("blob copy range changed"))?;
                    output[written..written + take].copy_from_slice(source);
                    self.copied += take as u64;
                    written += take;
                    *page_offset += take;
                    if *page_offset == chunk_len {
                        *current = None;
                    }
                    transitions += 1;
                }
            }
        }
        Ok(PersistentBytesCopyPoll { written })
    }
}

struct RootDescriptor {
    count: u64,
    page_root: Option<ArenaId>,
}

struct PageDescriptor {
    start: u64,
    count: u16,
    has_previous: bool,
    previous: Option<ArenaId>,
}

#[derive(Clone, Copy)]
struct BlobDescriptor {
    total_len: u64,
    chunk_start: u64,
    chunk_len: u16,
    previous: Option<ArenaId>,
}

fn encode_header(tag: u8, authority: ReferenceAuthority) -> Vec<u8> {
    let _ = authority;
    encode_canonical_node_header(tag)
}

fn encode_blob(
    authority: ReferenceAuthority,
    kind: BlobKind,
    total_len: usize,
    chunk_start: usize,
    chunk: &[u8],
) -> Result<Vec<u8>, ReferenceRootError> {
    let mut output = encode_header(BLOB_TAG, authority);
    output.push(kind as u8);
    output.extend_from_slice(&[0; 7]);
    push_u64(
        &mut output,
        u64::try_from(total_len).map_err(|_| ReferenceRootError::FactTooLarge)?,
    );
    push_u64(
        &mut output,
        u64::try_from(chunk_start).map_err(|_| ReferenceRootError::FactTooLarge)?,
    );
    output.extend_from_slice(
        &u16::try_from(chunk.len())
            .map_err(|_| ReferenceRootError::FactTooLarge)?
            .to_le_bytes(),
    );
    output.extend_from_slice(&[0; 6]);
    output.extend_from_slice(chunk);
    Ok(output)
}

fn encode_fact(fact: &PendingFact, authority: ReferenceAuthority) -> Vec<u8> {
    encode_fact_fields(
        FACT_TAG,
        authority,
        fact.ordinal,
        &fact.source,
        &fact.label_source,
        &fact.destination_source,
        fact.title_source.as_ref(),
        fact.label_len,
        fact.destination_len,
        fact.title_len,
    )
}

fn encode_inline_fact(
    fact: &PendingFact,
    authority: ReferenceAuthority,
    normalized_label: &[u8],
    cooked_destination: &[u8],
    cooked_title: Option<&[u8]>,
) -> Vec<u8> {
    let mut output = encode_fact_fields(
        INLINE_FACT_TAG,
        authority,
        fact.ordinal,
        &fact.source,
        &fact.label_source,
        &fact.destination_source,
        fact.title_source.as_ref(),
        fact.label_len,
        fact.destination_len,
        fact.title_len,
    );
    output.extend_from_slice(normalized_label);
    output.extend_from_slice(cooked_destination);
    if let Some(title) = cooked_title {
        output.extend_from_slice(title);
    }
    debug_assert!(output.len() <= ARENA_PAGE_BYTES);
    output
}

fn encode_stream_fact(fact: &PendingStreamFact, authority: ReferenceAuthority) -> Vec<u8> {
    encode_fact_fields(
        FACT_TAG,
        authority,
        fact.ordinal,
        &fact.source,
        &fact.label_source,
        &fact.destination_source,
        fact.title_source.as_ref(),
        fact.label_len,
        fact.destination_len,
        fact.title_len,
    )
}

fn encode_inline_stream_fact(
    fact: &PendingStreamFact,
    authority: ReferenceAuthority,
    normalized_label: &[u8],
    cooked_destination: &[u8],
    cooked_title: Option<&[u8]>,
) -> Vec<u8> {
    let mut output = encode_fact_fields(
        INLINE_FACT_TAG,
        authority,
        fact.ordinal,
        &fact.source,
        &fact.label_source,
        &fact.destination_source,
        fact.title_source.as_ref(),
        fact.label_len,
        fact.destination_len,
        fact.title_len,
    );
    output.extend_from_slice(normalized_label);
    output.extend_from_slice(cooked_destination);
    if let Some(title) = cooked_title {
        output.extend_from_slice(title);
    }
    debug_assert!(output.len() <= ARENA_PAGE_BYTES);
    output
}

#[allow(clippy::too_many_arguments)]
fn encode_fact_fields(
    tag: u8,
    authority: ReferenceAuthority,
    ordinal: u64,
    source: &ReferenceSourceRange,
    label_source: &ReferenceSourceRange,
    destination_source: &ReferenceSourceRange,
    title_source: Option<&ReferenceSourceRange>,
    label_len: u64,
    destination_len: u64,
    title_len: Option<u64>,
) -> Vec<u8> {
    let mut output = encode_header(tag, authority);
    push_u64(&mut output, ordinal);
    for range in [source, label_source, destination_source] {
        push_source_range(&mut output, range);
    }
    if let Some(title) = title_source {
        push_source_range(&mut output, title);
    } else {
        output.extend_from_slice(&[0xff; 32]);
    }
    output.push(u8::from(title_source.is_some()));
    output.extend_from_slice(&[0; 7]);
    push_u64(&mut output, label_len);
    push_u64(&mut output, destination_len);
    push_u64(&mut output, title_len.unwrap_or(0));
    debug_assert_eq!(output.len(), FACT_PAYLOAD_BYTES);
    output
}

fn encode_page(
    authority: ReferenceAuthority,
    start: u64,
    count: usize,
    total: u64,
    has_previous: bool,
) -> Result<Vec<u8>, ReferenceRootError> {
    let mut output = encode_header(PAGE_TAG, authority);
    push_u64(&mut output, start);
    output.extend_from_slice(
        &u16::try_from(count)
            .map_err(|_| ReferenceRootError::CapacityPreflight)?
            .to_le_bytes(),
    );
    output.push(u8::from(has_previous));
    output.extend_from_slice(&[0; 5]);
    push_u64(&mut output, total);
    debug_assert_eq!(output.len(), PAGE_PAYLOAD_BYTES);
    Ok(output)
}

fn encode_root(authority: ReferenceAuthority, count: u64) -> Vec<u8> {
    let mut output = encode_header(ROOT_TAG, authority);
    push_u64(&mut output, count);
    output.extend_from_slice(&[0; 8]);
    debug_assert_eq!(output.len(), ROOT_PAYLOAD_BYTES);
    output
}

fn decode_header(
    payload: &[u8],
    expected_tag: u8,
    expected: ReferenceAuthority,
) -> Result<(), ReferenceRootError> {
    let _ = expected;
    decode_canonical_node_header(payload, expected_tag).map_err(Into::into)
}

fn decode_root(
    arena: &PageArena,
    root: ArenaId,
    authority: ReferenceAuthority,
) -> Result<RootDescriptor, ReferenceRootError> {
    let payload = arena.payload(root)?;
    decode_header(payload, ROOT_TAG, authority)?;
    if payload.len() != ROOT_PAYLOAD_BYTES || payload[NODE_HEADER_BYTES + 8..] != [0; 8] {
        return Err(ReferenceRootError::Corrupt("invalid reference root"));
    }
    let count = read_u64(payload, NODE_HEADER_BYTES)?;
    let children = arena.child_count(root)?;
    if children != usize::from(count > 0) {
        return Err(ReferenceRootError::Corrupt(
            "root/page presence disagrees with count",
        ));
    }
    Ok(RootDescriptor {
        count,
        page_root: (children == 1)
            .then(|| arena.child_at(root, 0))
            .transpose()?,
    })
}

fn decode_page(
    arena: &PageArena,
    page: ArenaId,
    authority: ReferenceAuthority,
) -> Result<PageDescriptor, ReferenceRootError> {
    let payload = arena.payload(page)?;
    decode_header(payload, PAGE_TAG, authority)?;
    if payload.len() != PAGE_PAYLOAD_BYTES
        || payload[NODE_HEADER_BYTES + 11..NODE_HEADER_BYTES + 16] != [0; 5]
    {
        return Err(ReferenceRootError::Corrupt("invalid occurrence page"));
    }
    let start = read_u64(payload, NODE_HEADER_BYTES)?;
    let count = read_u16(payload, NODE_HEADER_BYTES + 8)?;
    let has_previous = match payload[NODE_HEADER_BYTES + 10] {
        0 => false,
        1 => true,
        _ => return Err(ReferenceRootError::Corrupt("invalid previous-page flag")),
    };
    let total = read_u64(payload, NODE_HEADER_BYTES + 16)?;
    if count == 0
        || start
            .checked_add(u64::from(count))
            .is_none_or(|end| total != end)
        || arena.child_count(page)? != usize::from(count) + usize::from(has_previous)
    {
        return Err(ReferenceRootError::Corrupt(
            "invalid occurrence page metrics",
        ));
    }
    Ok(PageDescriptor {
        start,
        count,
        has_previous,
        previous: has_previous.then(|| arena.child_at(page, 0)).transpose()?,
    })
}

fn decode_fact<'a>(
    arena: &'a PageArena,
    fact: ArenaId,
    authority: ReferenceAuthority,
) -> Result<ReferenceOccurrenceView<'a>, ReferenceRootError> {
    let payload = arena.payload(fact)?;
    let inline = match payload.first() {
        Some(&FACT_TAG) => {
            decode_header(payload, FACT_TAG, authority)?;
            false
        }
        Some(&INLINE_FACT_TAG) => {
            decode_header(payload, INLINE_FACT_TAG, authority)?;
            true
        }
        _ => return Err(ReferenceRootError::Corrupt("invalid occurrence fact tag")),
    };
    if payload.len() < FACT_PAYLOAD_BYTES
        || (!inline && payload.len() != FACT_PAYLOAD_BYTES)
        || payload[NODE_HEADER_BYTES + 137..NODE_HEADER_BYTES + 144] != [0; 7]
    {
        return Err(ReferenceRootError::Corrupt("invalid occurrence fact"));
    }
    let ordinal = read_u64(payload, NODE_HEADER_BYTES)?;
    let source = read_source_range(payload, NODE_HEADER_BYTES + 8)?;
    let label_source = read_source_range(payload, NODE_HEADER_BYTES + 40)?;
    let destination_source = read_source_range(payload, NODE_HEADER_BYTES + 72)?;
    let title_present = match payload[NODE_HEADER_BYTES + 136] {
        0 => false,
        1 => true,
        _ => return Err(ReferenceRootError::Corrupt("invalid title flag")),
    };
    let title_source = title_present
        .then(|| read_source_range(payload, NODE_HEADER_BYTES + 104))
        .transpose()?;
    let label_len = read_u64(payload, NODE_HEADER_BYTES + 144)?;
    let destination_len = read_u64(payload, NODE_HEADER_BYTES + 152)?;
    let title_len = read_u64(payload, NODE_HEADER_BYTES + 160)?;
    if !title_present && title_len != 0 {
        return Err(ReferenceRootError::Corrupt("absent title has bytes"));
    }

    if inline {
        if arena.child_count(fact)? != 0 {
            return Err(ReferenceRootError::Corrupt("inline fact has children"));
        }
        let [label_range, destination_range, title_range] = inline_value_ranges(payload)
            .ok_or(ReferenceRootError::Corrupt("invalid inline fact values"))?;
        let value = |range: Range<usize>, len: u64| PersistentBytesView {
            arena,
            authority,
            storage: PersistentBytesStorage::Inline {
                fact,
                start: range.start,
            },
            len,
            _not_sync: PhantomData,
        };
        let cooked_title = title_present.then(|| value(title_range, title_len));
        return Ok(ReferenceOccurrenceView {
            ordinal,
            source,
            label_source,
            destination_source,
            title_source,
            normalized_label: value(label_range, label_len),
            cooked_destination: value(destination_range, destination_len),
            cooked_title,
        });
    }

    let expected_children = 2 + usize::from(title_present);
    if arena.child_count(fact)? != expected_children {
        return Err(ReferenceRootError::Corrupt("fact child roles changed"));
    }
    let label_root = arena.child_at(fact, 0)?;
    let destination_root = arena.child_at(fact, 1)?;
    let title_root = title_present.then(|| arena.child_at(fact, 2)).transpose()?;
    let label = decode_blob(arena, label_root, authority, BlobKind::Label)?;
    let destination = decode_blob(arena, destination_root, authority, BlobKind::Destination)?;
    if label.total_len != label_len || destination.total_len != destination_len {
        return Err(ReferenceRootError::Corrupt("fact/blob length mismatch"));
    }
    let cooked_title = if let Some(root) = title_root {
        let title = decode_blob(arena, root, authority, BlobKind::Title)?;
        if title.total_len != title_len {
            return Err(ReferenceRootError::Corrupt("title length mismatch"));
        }
        Some(PersistentBytesView {
            arena,
            authority,
            storage: PersistentBytesStorage::Blob {
                kind: BlobKind::Title,
                root,
            },
            len: title_len,
            _not_sync: PhantomData,
        })
    } else {
        None
    };
    Ok(ReferenceOccurrenceView {
        ordinal,
        source,
        label_source,
        destination_source,
        title_source,
        normalized_label: PersistentBytesView {
            arena,
            authority,
            storage: PersistentBytesStorage::Blob {
                kind: BlobKind::Label,
                root: label_root,
            },
            len: label_len,
            _not_sync: PhantomData,
        },
        cooked_destination: PersistentBytesView {
            arena,
            authority,
            storage: PersistentBytesStorage::Blob {
                kind: BlobKind::Destination,
                root: destination_root,
            },
            len: destination_len,
            _not_sync: PhantomData,
        },
        cooked_title,
    })
}

fn decode_blob(
    arena: &PageArena,
    root: ArenaId,
    authority: ReferenceAuthority,
    expected_kind: BlobKind,
) -> Result<BlobDescriptor, ReferenceRootError> {
    let payload = arena.payload(root)?;
    decode_header(payload, BLOB_TAG, authority)?;
    if payload.len() < NODE_HEADER_BYTES + BLOB_METADATA_BYTES
        || payload[NODE_HEADER_BYTES + 1..NODE_HEADER_BYTES + 8] != [0; 7]
        || payload[NODE_HEADER_BYTES + 26..NODE_HEADER_BYTES + 32] != [0; 6]
    {
        return Err(ReferenceRootError::Corrupt("invalid blob page"));
    }
    let kind = BlobKind::from_u8(payload[NODE_HEADER_BYTES])?;
    if kind != expected_kind {
        return Err(ReferenceRootError::Corrupt("blob role changed"));
    }
    let total_len = read_u64(payload, NODE_HEADER_BYTES + 8)?;
    let chunk_start = read_u64(payload, NODE_HEADER_BYTES + 16)?;
    let chunk_len = read_u16(payload, NODE_HEADER_BYTES + 24)?;
    if payload.len() != NODE_HEADER_BYTES + BLOB_METADATA_BYTES + usize::from(chunk_len)
        || chunk_start
            .checked_add(u64::from(chunk_len))
            .is_none_or(|end| end > total_len)
    {
        return Err(ReferenceRootError::Corrupt("invalid blob metrics"));
    }
    let child_count = arena.child_count(root)?;
    let previous = match chunk_start {
        0 if child_count == 0 => None,
        0 => {
            return Err(ReferenceRootError::Corrupt(
                "first blob page has predecessor",
            ))
        }
        _ if child_count == 1 => Some(arena.child_at(root, 0)?),
        _ => return Err(ReferenceRootError::Corrupt("blob predecessor missing")),
    };
    Ok(BlobDescriptor {
        total_len,
        chunk_start,
        chunk_len,
        previous,
    })
}

fn push_source_range(output: &mut Vec<u8>, range: &ReferenceSourceRange) {
    push_u64(output, range.bytes.start);
    push_u64(output, range.bytes.end);
    push_u64(output, range.utf16.start);
    push_u64(output, range.utf16.end);
}

fn read_source_range(
    input: &[u8],
    offset: usize,
) -> Result<ReferenceSourceRange, ReferenceRootError> {
    Ok(ReferenceSourceRange {
        bytes: read_u64(input, offset)?..read_u64(input, offset + 8)?,
        utf16: read_u64(input, offset + 16)?..read_u64(input, offset + 24)?,
    })
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(input: &[u8], offset: usize) -> Result<u16, ReferenceRootError> {
    let bytes: [u8; 2] = input
        .get(offset..offset + 2)
        .ok_or(ReferenceRootError::Corrupt("truncated u16"))?
        .try_into()
        .map_err(|_| ReferenceRootError::Corrupt("invalid u16"))?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u64(input: &[u8], offset: usize) -> Result<u64, ReferenceRootError> {
    let bytes: [u8; 8] = input
        .get(offset..offset + 8)
        .ok_or(ReferenceRootError::Corrupt("truncated u64"))?
        .try_into()
        .map_err(|_| ReferenceRootError::Corrupt("invalid u64"))?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::RuntimeIdentity;
    use crate::{CandidateGeneration, SourceStore};

    struct ReferenceIndexFixture {
        arena: PageArena,
        build: crate::storage::CandidateBuild,
        authority: CandidateAuthority,
        root: ArenaId,
    }

    fn authority() -> CandidateAuthority {
        let source = SourceStore::new("0123456789abcdef").expect("source");
        CandidateAuthority::new(
            RuntimeIdentity::new([31; 16]).expect("document"),
            RuntimeIdentity::new([63; 16]).expect("publication"),
            source.version(),
            CandidateGeneration::FIRST,
            1,
        )
        .expect("authority")
    }

    fn build_reference_index_fixture(
        facts: Vec<(Vec<u8>, Vec<u8>)>,
        facts_per_page: usize,
    ) -> ReferenceIndexFixture {
        assert!(!facts.is_empty());
        let source = SourceStore::new(&"x".repeat(facts.len() * 16)).expect("source");
        let authority = CandidateAuthority::new(
            RuntimeIdentity::new([41; 16]).expect("document"),
            RuntimeIdentity::new([73; 16]).expect("publication"),
            source.version(),
            CandidateGeneration::FIRST,
            1,
        )
        .expect("authority");
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let mut builder = ReferenceRootBuilder::new(
            authority,
            ReferenceRootLimits {
                arena: arena.limits(),
                max_occurrences: facts.len() as u64,
                max_cooked_bytes_per_fact: 16 * 1024 * 1024,
                facts_per_page,
            },
        )
        .expect("reference builder");
        let reserve = ReferenceReserve {
            nodes: 0,
            payload_bytes: 0,
        };
        let mut build = None;

        for (ordinal, (label, destination)) in facts.into_iter().enumerate() {
            let start = ordinal as u64 * 16;
            builder
                .offer(
                    AuthoritativeReferenceFact {
                        authority,
                        source: ReferenceSourceRange {
                            bytes: start..start + 16,
                            utf16: start..start + 16,
                        },
                        label_source: ReferenceSourceRange {
                            bytes: start + 1..start + 4,
                            utf16: start + 1..start + 4,
                        },
                        destination_source: ReferenceSourceRange {
                            bytes: start + 6..start + 12,
                            utf16: start + 6..start + 12,
                        },
                        title_source: None,
                        normalized_label: label.into_boxed_slice(),
                        cooked_destination: destination.into_boxed_slice(),
                        cooked_title: None,
                        _not_sync: PhantomData,
                    },
                    &arena,
                    reserve,
                )
                .expect("reference fact");
            let mut session = match build.take() {
                Some(build) => arena.resume_build(build).expect("resume build"),
                None => arena.begin_build().expect("begin build"),
            };
            loop {
                match builder.poll(&mut session, 17).expect("reference poll") {
                    ReferenceBuildPoll::Pending { idle: true, .. } => break,
                    ReferenceBuildPoll::Pending { idle: false, .. } => {}
                    ReferenceBuildPoll::Complete { .. } => {
                        panic!("reference root completed before finish")
                    }
                }
            }
            build = Some(session.suspend().expect("suspend build"));
        }

        builder.finish(&arena, reserve).expect("finish references");
        let mut session = arena
            .resume_build(build.take().expect("suspended reference build"))
            .expect("resume finishing build");
        let subtree = loop {
            match builder.poll(&mut session, 17).expect("finish poll") {
                ReferenceBuildPoll::Pending { .. } => {}
                ReferenceBuildPoll::Complete { root, .. } => break root,
            }
        };
        let root = subtree.owner.id();
        drop(subtree);
        let build = session.suspend().expect("suspend completed build");
        ReferenceIndexFixture {
            arena,
            build,
            authority,
            root,
        }
    }

    fn reference_page_count(
        arena: &PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
    ) -> usize {
        let mut page = decode_root(arena, root, authority)
            .expect("reference root")
            .page_root;
        let mut count = 0;
        while let Some(id) = page {
            let descriptor = decode_page(arena, id, authority).expect("reference page");
            page = descriptor.previous;
            count += 1;
        }
        count
    }

    #[test]
    fn winner_index_is_fuel_bounded_exact_and_reclaims_incrementally() {
        // U+0130 case-folds to `i` + U+0307. Five hundred repetitions are
        // only 1,000 raw UTF-8 bytes but produce 1,500 normalized bytes, so a
        // byte-capped approximation of CommonMark's scalar limit rejects a
        // valid key that the normative service and this index must retain.
        let expanded_label = "i\u{0307}".repeat(500).into_bytes();
        assert_eq!(expanded_label.len(), 1_500);
        assert!(expanded_label.len() as u64 <= REFERENCE_WINNER_INDEX_MAX_NORMALIZED_LABEL_BYTES);

        let facts = vec![
            (b"duplicate".to_vec(), b"/first".to_vec()),
            (b"alpha".to_vec(), b"/alpha".to_vec()),
            (b"beta".to_vec(), b"/beta".to_vec()),
            (b"duplicate".to_vec(), b"/later-page".to_vec()),
            (expanded_label.clone(), b"/expanded".to_vec()),
            (b"omega".to_vec(), b"/omega".to_vec()),
        ];
        let ReferenceIndexFixture {
            mut arena,
            build,
            authority,
            root,
        } = build_reference_index_fixture(facts, 2);
        assert_eq!(reference_page_count(&arena, authority, root), 3);

        let mut builder = ReferenceWinnerIndexBuilder::new(&arena, authority, root)
            .expect("winner index builder");
        assert!(matches!(
            builder.poll(&arena, authority, root, 0),
            Err(ReferenceRootError::ZeroFuel)
        ));
        let mut build_polls = 0;
        loop {
            let receipt = builder
                .poll(&arena, authority, root, 1)
                .expect("winner index poll");
            assert_eq!(receipt.transitions, 1);
            build_polls += 1;
            assert!(build_polls < 128, "winner index did not converge");
            if receipt.complete {
                break;
            }
        }

        let index = builder.into_index().expect("completed winner index");
        assert_eq!(index.root(), root);
        assert_eq!(index.occurrence_count(), 6);
        assert_eq!(index.indexed_occurrences(), 6);
        assert_eq!(index.skipped_oversized_occurrences(), 0);
        assert_eq!(index.unique_label_count(), 5);

        let indexed_duplicate = index
            .winner(&arena, authority, root, b"duplicate")
            .expect("indexed duplicate")
            .expect("duplicate winner");
        assert_eq!(indexed_duplicate.ordinal, 0);
        assert!(indexed_duplicate
            .cooked_destination
            .equals(b"/first")
            .expect("winner destination"));
        let linear_duplicate = ReferenceRootView::open(&arena, authority, root)
            .expect("linear root view")
            .winner("duplicate")
            .expect("linear winner")
            .expect("linear duplicate");
        assert_eq!(indexed_duplicate.ordinal, linear_duplicate.ordinal);

        let indexed_expanded = index
            .winner(&arena, authority, root, &expanded_label)
            .expect("expanded lookup")
            .expect("expanded winner");
        assert_eq!(indexed_expanded.ordinal, 4);
        assert!(indexed_expanded
            .cooked_destination
            .equals(b"/expanded")
            .expect("expanded destination"));
        assert!(index
            .winner(&arena, authority, root, b"DUPLICATE")
            .expect("exact-case miss")
            .is_none());
        assert!(index
            .winner(&arena, authority, root, b"missing")
            .expect("missing label")
            .is_none());
        let truncated_expanded = &expanded_label[..expanded_label.len() - 3];
        assert!(index
            .winner(&arena, authority, root, truncated_expanded)
            .expect("near expanded miss")
            .is_none());

        let expected_reclaim_transitions = index.unique_label_count() as usize;
        let mut reclaimer = index.into_reclaimer();
        assert!(matches!(
            reclaimer.poll(0),
            Err(ReferenceRootError::ZeroFuel)
        ));
        let mut reclaimed = 0;
        loop {
            let receipt = reclaimer.poll(1).expect("winner index reclaim");
            assert!(receipt.transitions <= 1);
            reclaimed += receipt.transitions;
            if receipt.complete {
                break;
            }
        }
        assert_eq!(reclaimed, expected_reclaim_transitions);
        drop(reclaimer);

        arena.abort_build(build).expect("abort fixture build");
        loop {
            let receipt = arena.poll_reclaim(1);
            assert!(receipt.transitions <= 1);
            if receipt.complete {
                break;
            }
        }
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn inline_decoder_rejects_length_and_child_corruption() {
        let authority = authority();
        let source = ReferenceSourceRange {
            bytes: 0..16,
            utf16: 0..16,
        };
        let label = ReferenceSourceRange {
            bytes: 1..2,
            utf16: 1..2,
        };
        let destination = ReferenceSourceRange {
            bytes: 4..5,
            utf16: 4..5,
        };
        let mut valid = encode_fact_fields(
            INLINE_FACT_TAG,
            authority,
            0,
            &source,
            &label,
            &destination,
            None,
            1,
            1,
            None,
        );
        valid.extend_from_slice(b"xu");
        assert!(is_canonical_reference_record_payload(&valid));

        let mut invalid_length = valid.clone();
        invalid_length[NODE_HEADER_BYTES + 144..NODE_HEADER_BYTES + 152]
            .copy_from_slice(&2_u64.to_le_bytes());
        assert!(!is_canonical_reference_record_payload(&invalid_length));

        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let (build, valid_id, invalid_length_id, invalid_child_id) = {
            let mut session = arena.begin_build().expect("build");
            let valid_owner = session.allocate(&valid, &[]).expect("valid inline fact");
            let invalid_length_owner = session
                .allocate(&invalid_length, &[])
                .expect("invalid length fact");
            let child = session
                .allocate(&encode_root(authority, 0), &[])
                .expect("unexpected child");
            let invalid_child_owner = session
                .allocate(&valid, &[child.id()])
                .expect("invalid child fact");
            let ids = (
                valid_owner.id(),
                invalid_length_owner.id(),
                invalid_child_owner.id(),
            );
            let build = session.suspend().expect("suspend");
            (build, ids.0, ids.1, ids.2)
        };

        let occurrence = decode_fact(&arena, valid_id, authority).expect("valid inline fact");
        assert!(occurrence.normalized_label.equals(b"x").unwrap());
        assert!(occurrence.cooked_destination.equals(b"u").unwrap());
        assert!(matches!(
            decode_fact(&arena, invalid_length_id, authority),
            Err(ReferenceRootError::Corrupt("invalid inline fact values"))
        ));
        assert!(matches!(
            decode_fact(&arena, invalid_child_id, authority),
            Err(ReferenceRootError::Corrupt("inline fact has children"))
        ));

        arena.abort_build(build).expect("abort malformed build");
        while !arena.poll_reclaim(8).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }
}
