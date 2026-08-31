//! Candidate authority and the single five-role publication manifest.
//!
//! The parser/controller supplies canonical records; this module owns their
//! authority binding, strong canonical digests, fixed-role assembly, and the
//! sole arena-root transfer. It intentionally defines neither Markdown
//! recognition nor a public transport schema.

use std::cell::Cell;
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::block_sequence::{
    decode_persistent_m11_block_role_descriptor, encode_persistent_m11_block_role_descriptor,
    m11_block_sequence_canonical_record_count, validate_persistent_m11_block_root,
    M11BlockRoleLane, M11BlockSequenceError, M11BlockSequenceRoot, M11RetainedBlockSequenceRoot,
    PersistentM11BlockRoleDescriptor, PersistentM11BlockRootClaim,
    PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA, PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA,
    PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES,
};
use crate::identity::{CandidateGeneration, SourceRevision, SourceRootId};
use crate::inline_projection::{
    decode_persistent_inline_projection_descriptor, persistent_inline_link_value_record_at,
    persistent_inline_projection_record_at, validate_persistent_inline_projection_role,
    M11InlineProjectionError, M11InlineProjectionRoot, PersistentM11InlineProjectionDescriptor,
    RetainedM11InlineProjectionRole, PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES,
    PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA,
};
use crate::measured_sequence::SequenceInspectionReceipt;
use crate::parser_pages::M11ParserPageRecord;
use crate::recursive_green::{
    decode_persistent_m11_recursive_green_role_descriptor,
    encode_persistent_m11_recursive_green_role_descriptor,
    m11_recursive_green_canonical_record_count, validate_persistent_m11_recursive_green_root,
    M11RecursiveGreenError, M11RecursiveGreenRoot, M11RetainedRecursiveGreenRoot,
    PersistentM11RecursiveGreenRoleDescriptor, PersistentM11RecursiveGreenRootClaim,
    PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES,
};
#[cfg(feature = "parser-internal")]
use crate::reference_journal::{M11ReferenceJournalError, M11ReferenceJournalRoot};
use crate::reference_root::{
    AuthoritativeReferenceFact, AuthoritativeReferenceFactStart, ReferenceBuildPoll,
    ReferenceRootBuilder, ReferenceRootError, ReferenceRootLimits, ReferenceRootView,
    ReferenceSubtreeRoot, StreamedReferenceValueKind, REFERENCE_ROOT_PAYLOAD_BYTES,
};
use crate::source::SourceVersion;
use crate::source_facts::{
    is_persistent_source_facts_sequence_leaf_payload, persistent_source_facts_leaf_record_at,
    validate_persistent_source_facts_role, ParserProfileId, PersistentSourceFactsRoleValidation,
    PersistentSourceFactsRoot, RetainedPersistentSourceFactsRole, SourceFactsAssemblyError,
    SourceFactsScanProfile, PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES,
    PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
};
use crate::storage::{
    ArenaBuildOwner, ArenaError, ArenaLimits, CandidateBuild, CandidateSeal, CommittedArenaRoot,
    PageArena, ARENA_PAGE_BYTES,
};
use crate::ArenaId;

pub(crate) const CANDIDATE_FORMAT_VERSION: u8 = 1;
pub(crate) const CANDIDATE_HEADER_BYTES: usize = 84;
/// Authority-free header shared by reusable canonical role content.
///
/// Canonical records and their internal storage closure deliberately carry no
/// publication, source-root, revision, or generation identity. Fresh role
/// wrappers and the manifest remain authority-bound and are the only nodes
/// allowed to promote this content for a target revision.
pub(crate) const CANONICAL_NODE_HEADER_BYTES: usize = 4;
pub(crate) const STRONG_DIGEST_BYTES: usize = 32;
pub(crate) const CANDIDATE_ROLE_COUNT: usize = 5;
const CANDIDATE_ROLE_COUNT_U8: u8 = 5;
pub(crate) const SOURCE_FACTS_LEGACY_RECORD_SCHEMA: u32 = 1;
pub(crate) const GREEN_SCHEMA: u32 = 1;
pub(crate) const PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA: u32 = 3;
pub(crate) const PROJECTION_SCHEMA: u32 = 1;
pub(crate) const REFERENCES_SCHEMA: u32 = 1;
pub(crate) const CLEAN_EOF_SCHEMA: u32 = 1;

const AUTHORITY_RESERVED_OFFSET: usize = 60;
static NEXT_RUNTIME_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// Full-width identity. Byte interpretation remains an endpoint concern; the
/// engine compares and hashes all 128 bits without truncation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StrongIdentity(pub(crate) [u8; 16]);

impl StrongIdentity {
    pub(crate) fn new(bytes: [u8; 16]) -> Result<Self, ManifestError> {
        if bytes == [0; 16] {
            return Err(ManifestError::InvalidAuthority);
        }
        Ok(Self(bytes))
    }

    pub(crate) fn allocate(domain: &[u8]) -> Result<Self, ManifestError> {
        let ordinal = NEXT_RUNTIME_IDENTITY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ManifestError::InvalidAuthority)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.runtime.identity.v1\0");
        hasher.update(domain);
        hasher.update(&ordinal.to_le_bytes());
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        Self::new(bytes)
    }
}

/// Authority shared by every node of one candidate publication.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CandidateAuthority {
    pub(crate) document: StrongIdentity,
    pub(crate) publication: StrongIdentity,
    pub(crate) source_root: SourceRootId,
    pub(crate) source_revision: SourceRevision,
    pub(crate) parse_generation: CandidateGeneration,
    pub(crate) syntax_profile: u32,
    pub(crate) source_bytes: u64,
    pub(crate) source_utf16: u64,
}

impl CandidateAuthority {
    pub(crate) fn new(
        document: StrongIdentity,
        publication: StrongIdentity,
        source: SourceVersion,
        parse_generation: CandidateGeneration,
        syntax_profile: u32,
    ) -> Result<Self, ManifestError> {
        if document == publication || syntax_profile == 0 {
            return Err(ManifestError::InvalidAuthority);
        }
        Ok(Self {
            document,
            publication,
            source_root: source.root(),
            source_revision: source.revision(),
            parse_generation,
            syntax_profile,
            source_bytes: u64::try_from(source.byte_len())
                .map_err(|_| ManifestError::InvalidAuthority)?,
            source_utf16: u64::try_from(source.utf16_len())
                .map_err(|_| ManifestError::InvalidAuthority)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum CandidateRole {
    SourceFacts = 1,
    Green = 2,
    Projection = 3,
    References = 4,
    CleanEofOnly = 5,
}

impl CandidateRole {
    pub(crate) const ORDERED: [Self; CANDIDATE_ROLE_COUNT] = [
        Self::SourceFacts,
        Self::Green,
        Self::Projection,
        Self::References,
        Self::CleanEofOnly,
    ];

    pub(crate) fn decode(value: u8) -> Result<Self, ManifestError> {
        Self::ORDERED
            .into_iter()
            .find(|role| *role as u8 == value)
            .ok_or(ManifestError::Corrupt("unknown candidate role"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RoleMetadata {
    pub(crate) role: CandidateRole,
    pub(crate) schema: u32,
    pub(crate) record_count: u64,
    pub(crate) canonical_bytes: u64,
    pub(crate) digest: [u8; STRONG_DIGEST_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceReserve {
    pub(crate) nodes: usize,
    pub(crate) payload_bytes: usize,
}

#[derive(Debug)]
pub(crate) enum ManifestError {
    InvalidAuthority,
    InvalidLimits,
    InvalidRole,
    CrossAuthority,
    CapacityPreflight,
    ZeroFuel,
    Busy,
    Arena(ArenaError),
    Reference(ReferenceRootError),
    BlockSequence(M11BlockSequenceError),
    RecursiveGreen(M11RecursiveGreenError),
    InlineProjection(M11InlineProjectionError),
    Corrupt(&'static str),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAuthority => formatter.write_str("invalid candidate authority"),
            Self::InvalidLimits => formatter.write_str("invalid candidate manifest limits"),
            Self::InvalidRole => formatter.write_str("invalid candidate manifest role"),
            Self::CrossAuthority => formatter.write_str("candidate root crosses authority"),
            Self::CapacityPreflight => {
                formatter.write_str("candidate manifest exceeds remaining arena capacity")
            }
            Self::ZeroFuel => formatter.write_str("candidate manifest poll requires nonzero fuel"),
            Self::Busy => formatter.write_str("candidate manifest is busy"),
            Self::Arena(error) => write!(formatter, "candidate manifest storage failed: {error}"),
            Self::Reference(error) => {
                write!(formatter, "candidate reference build failed: {error}")
            }
            Self::BlockSequence(error) => {
                write!(formatter, "candidate block sequence failed: {error}")
            }
            Self::RecursiveGreen(error) => {
                write!(formatter, "candidate recursive Green failed: {error}")
            }
            Self::InlineProjection(error) => {
                write!(formatter, "candidate inline Projection failed: {error}")
            }
            Self::Corrupt(message) => write!(formatter, "corrupt candidate manifest: {message}"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<ArenaError> for ManifestError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<ReferenceRootError> for ManifestError {
    fn from(error: ReferenceRootError) -> Self {
        Self::Reference(error)
    }
}

impl From<M11BlockSequenceError> for ManifestError {
    fn from(error: M11BlockSequenceError) -> Self {
        Self::BlockSequence(error)
    }
}

impl From<M11RecursiveGreenError> for ManifestError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self::RecursiveGreen(error)
    }
}

impl From<M11InlineProjectionError> for ManifestError {
    fn from(error: M11InlineProjectionError) -> Self {
        Self::InlineProjection(error)
    }
}

pub(crate) fn encode_candidate_header(tag: u8, authority: CandidateAuthority) -> Vec<u8> {
    let mut output = Vec::with_capacity(CANDIDATE_HEADER_BYTES);
    output.extend_from_slice(&[tag, CANDIDATE_FORMAT_VERSION, 0, 0]);
    output.extend_from_slice(&authority.document.0);
    output.extend_from_slice(&authority.publication.0);
    output.extend_from_slice(&authority.source_root.get().to_le_bytes());
    output.extend_from_slice(&authority.source_revision.get().to_le_bytes());
    output.extend_from_slice(&authority.parse_generation.get().to_le_bytes());
    output.extend_from_slice(&authority.syntax_profile.to_le_bytes());
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(&authority.source_bytes.to_le_bytes());
    output.extend_from_slice(&authority.source_utf16.to_le_bytes());
    debug_assert_eq!(output.len(), CANDIDATE_HEADER_BYTES);
    output
}

pub(crate) fn decode_candidate_header(
    payload: &[u8],
    expected_tag: u8,
    expected: CandidateAuthority,
) -> Result<(), ManifestError> {
    if payload.len() < CANDIDATE_HEADER_BYTES
        || payload[..4] != [expected_tag, CANDIDATE_FORMAT_VERSION, 0, 0]
        || payload[4..20] != expected.document.0
        || payload[20..36] != expected.publication.0
        || read_u64(payload, 36)? != expected.source_root.get()
        || read_u64(payload, 44)? != expected.source_revision.get()
        || read_u64(payload, 52)? != expected.parse_generation.get()
        || read_u32(payload, 60)? != expected.syntax_profile
        || payload[64..68] != [0; 4]
        || read_u64(payload, 68)? != expected.source_bytes
        || read_u64(payload, 76)? != expected.source_utf16
    {
        return Err(ManifestError::Corrupt("candidate node authority changed"));
    }
    Ok(())
}

pub(crate) fn encode_canonical_node_header(tag: u8) -> Vec<u8> {
    vec![tag, CANDIDATE_FORMAT_VERSION, 0, 0]
}

pub(crate) fn decode_canonical_node_header(
    payload: &[u8],
    expected_tag: u8,
) -> Result<(), ManifestError> {
    if payload.len() < CANONICAL_NODE_HEADER_BYTES
        || payload[..CANONICAL_NODE_HEADER_BYTES] != [expected_tag, CANDIDATE_FORMAT_VERSION, 0, 0]
    {
        return Err(ManifestError::Corrupt(
            "canonical candidate node header changed",
        ));
    }
    Ok(())
}

/// Starts a domain-separated canonical role digest. Callers append only
/// engine-authenticated canonical records, then finalize with exact metadata.
pub(crate) fn role_hasher(
    _authority: CandidateAuthority,
    role: CandidateRole,
    schema: u32,
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.role.v2\0");
    hasher.update(&[role as u8]);
    hasher.update(&schema.to_le_bytes());
    hasher
}

pub(crate) fn finalize_role_digest(
    mut hasher: blake3::Hasher,
    record_count: u64,
    canonical_bytes: u64,
) -> [u8; STRONG_DIGEST_BYTES] {
    hasher.update(b"flark.candidate.role.trailer.v1\0");
    hasher.update(&record_count.to_le_bytes());
    hasher.update(&canonical_bytes.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn persistent_projection_role_digest(
    descriptor: &[u8; PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES],
    structural_record_count: u64,
    structural_canonical_bytes: u64,
    structural_digest: [u8; STRONG_DIGEST_BYTES],
    total_record_count: u64,
    total_canonical_bytes: u64,
) -> [u8; STRONG_DIGEST_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.projection.persistent.v1\0");
    hasher.update(&PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA.to_le_bytes());
    hasher.update(descriptor);
    hasher.update(&structural_record_count.to_le_bytes());
    hasher.update(&structural_canonical_bytes.to_le_bytes());
    hasher.update(&structural_digest);
    hasher.update(&total_record_count.to_le_bytes());
    hasher.update(&total_canonical_bytes.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn persistent_block_role_digest(
    authority: CandidateAuthority,
    role: CandidateRole,
    schema: u32,
    descriptor: &[u8; PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES],
    record_count: u64,
    canonical_bytes: u64,
) -> [u8; STRONG_DIGEST_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.block-role.persistent.v1\0");
    hash_authority(&mut hasher, authority);
    hasher.update(&[role as u8]);
    hasher.update(&schema.to_le_bytes());
    hasher.update(descriptor);
    hasher.update(&record_count.to_le_bytes());
    hasher.update(&canonical_bytes.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn persistent_recursive_green_role_digest(
    authority: CandidateAuthority,
    descriptor: &[u8; PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES],
    event_count: u64,
    canonical_event_bytes: u64,
) -> [u8; STRONG_DIGEST_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.recursive-green.persistent.v1\0");
    hash_authority(&mut hasher, authority);
    hasher.update(&[CandidateRole::Green as u8]);
    hasher.update(&PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA.to_le_bytes());
    hasher.update(descriptor);
    hasher.update(&event_count.to_le_bytes());
    hasher.update(&canonical_event_bytes.to_le_bytes());
    *hasher.finalize().as_bytes()
}

pub(crate) fn hash_authority(hasher: &mut blake3::Hasher, authority: CandidateAuthority) {
    hasher.update(&authority.document.0);
    hasher.update(&authority.publication.0);
    hasher.update(&authority.source_root.get().to_le_bytes());
    hasher.update(&authority.source_revision.get().to_le_bytes());
    hasher.update(&authority.parse_generation.get().to_le_bytes());
    hasher.update(&authority.syntax_profile.to_le_bytes());
    hasher.update(&authority.source_bytes.to_le_bytes());
    hasher.update(&authority.source_utf16.to_le_bytes());
}

pub(crate) fn hash_record_digest(
    role_hasher: &mut blake3::Hasher,
    canonical_bytes: u64,
    digest: [u8; STRONG_DIGEST_BYTES],
) {
    role_hasher.update(&canonical_bytes.to_le_bytes());
    role_hasher.update(&digest);
}

pub(crate) fn preflight_remaining(
    arena: &PageArena,
    limits: ArenaLimits,
    nodes: usize,
    payload_bytes: usize,
) -> Result<(), ManifestError> {
    let metrics = arena.metrics();
    if arena.limits() != limits
        || metrics
            .resident_nodes
            .checked_add(nodes)
            .is_none_or(|total| total > limits.max_slots)
        || metrics
            .live_payload_bytes
            .checked_add(metrics.reserved_external_payload_bytes)
            .and_then(|admitted| admitted.checked_add(payload_bytes))
            .is_none_or(|total| total > limits.max_live_payload_bytes)
    {
        return Err(ManifestError::CapacityPreflight);
    }
    Ok(())
}

pub(crate) fn read_u32(input: &[u8], offset: usize) -> Result<u32, ManifestError> {
    let bytes: [u8; 4] = input
        .get(offset..offset + 4)
        .ok_or(ManifestError::Corrupt("truncated u32"))?
        .try_into()
        .map_err(|_| ManifestError::Corrupt("invalid u32"))?;
    Ok(u32::from_le_bytes(bytes))
}

pub(crate) fn read_u64(input: &[u8], offset: usize) -> Result<u64, ManifestError> {
    let bytes: [u8; 8] = input
        .get(offset..offset + 8)
        .ok_or(ManifestError::Corrupt("truncated u64"))?
        .try_into()
        .map_err(|_| ManifestError::Corrupt("invalid u64"))?;
    Ok(u64::from_le_bytes(bytes))
}

const ANCHOR_TAG: u8 = 0xc0;
const RECORD_TAG: u8 = 0xc1;
const ROLE_ROOT_TAG: u8 = 0xc2;
const MANIFEST_TAG: u8 = 0xc3;
const RECORD_METADATA_BYTES: usize = 24;
pub(crate) const ROLE_METADATA_BYTES: usize = 56;
const RECORD_BASE_BYTES: usize = CANONICAL_NODE_HEADER_BYTES + RECORD_METADATA_BYTES;
const ROLE_ROOT_PAYLOAD_BYTES: usize = CANDIDATE_HEADER_BYTES + ROLE_METADATA_BYTES;
const MANIFEST_PAYLOAD_BYTES: usize =
    CANDIDATE_HEADER_BYTES + 8 + CANDIDATE_ROLE_COUNT * ROLE_METADATA_BYTES + 32;
const CHECKPOINT_RECORD_BYTES: usize = 24;
const MAX_REFERENCE_WORKING_OWNERS: usize = 72;

/// Counts one canonical role-record payload and no wrapper, page, or blob.
pub(crate) fn canonical_role_record_count(payload: &[u8]) -> u32 {
    u32::from(
        payload.get(..4) == Some([RECORD_TAG, CANDIDATE_FORMAT_VERSION, 0, 0].as_slice())
            || crate::reference_root::is_canonical_reference_record_payload(payload)
            || is_persistent_source_facts_sequence_leaf_payload(payload),
    )
    .saturating_add(crate::parser_pages::m11_parser_page_canonical_record_count(
        payload,
    ))
    .saturating_add(m11_block_sequence_canonical_record_count(payload))
    .saturating_add(m11_recursive_green_canonical_record_count(payload))
}

/// Canonical payloads for parser-owned roles.
///
/// The records are parser-produced data. The manifest layer deliberately
/// knows nothing about Markdown and only authenticates their bytes and role
/// placement. `SourceFacts` is a bounded sequence of complete canonical page
/// records; Green remains one record while Projection may add bounded,
/// parser-authored leaf pages after its root summary.
pub(crate) struct CanonicalRoleInputs {
    source_facts: Option<VecDeque<Box<[u8]>>>,
    green: Option<VecDeque<Box<[u8]>>>,
    projection: Option<VecDeque<Box<[u8]>>>,
}

impl CanonicalRoleInputs {
    pub(crate) fn single(
        source_facts: impl Into<Box<[u8]>>,
        green: impl Into<Box<[u8]>>,
        projection: impl Into<Box<[u8]>>,
    ) -> Self {
        Self::new([source_facts.into()], green, projection)
    }

    pub(crate) fn new(
        source_facts: impl IntoIterator<Item = Box<[u8]>>,
        green: impl Into<Box<[u8]>>,
        projection: impl Into<Box<[u8]>>,
    ) -> Self {
        Self {
            source_facts: Some(source_facts.into_iter().collect()),
            green: Some(VecDeque::from([green.into()])),
            projection: Some(VecDeque::from([projection.into()])),
        }
    }

    #[cfg(test)]
    pub(crate) fn persistent(
        green: impl Into<Box<[u8]>>,
        projection: impl Into<Box<[u8]>>,
    ) -> Self {
        Self::persistent_projection_records(green, [projection.into()])
    }

    pub(crate) fn persistent_projection_records(
        green: impl Into<Box<[u8]>>,
        projection: impl IntoIterator<Item = Box<[u8]>>,
    ) -> Self {
        Self {
            source_facts: None,
            green: Some(VecDeque::from([green.into()])),
            projection: Some(projection.into_iter().collect()),
        }
    }

    pub(crate) fn persistent_blocks() -> Self {
        Self {
            source_facts: None,
            green: None,
            projection: None,
        }
    }

    pub(crate) fn persistent_recursive_green_projection_records(
        projection: impl IntoIterator<Item = Box<[u8]>>,
    ) -> Self {
        Self {
            source_facts: None,
            green: None,
            projection: Some(projection.into_iter().collect()),
        }
    }

    fn validate(
        &self,
        maximum_records: usize,
        require_legacy_source_facts: bool,
    ) -> Result<(), ManifestError> {
        if require_legacy_source_facts && self.source_facts.is_none() {
            return Err(ManifestError::Corrupt(
                "legacy candidate lost its SourceFacts records",
            ));
        }
        for records in [&self.source_facts, &self.green, &self.projection]
            .into_iter()
            .flatten()
        {
            if records.is_empty() || records.len() > maximum_records {
                return Err(ManifestError::InvalidLimits);
            }
            for bytes in records {
                if bytes.len() > ARENA_PAGE_BYTES - RECORD_BASE_BYTES {
                    return Err(ManifestError::InvalidLimits);
                }
            }
        }
        Ok(())
    }

    fn take(&mut self, role: CandidateRole) -> Result<VecDeque<Box<[u8]>>, ManifestError> {
        match role {
            CandidateRole::SourceFacts => self.source_facts.take(),
            CandidateRole::Green => self.green.take(),
            CandidateRole::Projection => self.projection.take(),
            _ => return Err(ManifestError::InvalidRole),
        }
        .ok_or(ManifestError::Corrupt(
            "candidate role was already consumed",
        ))
    }

    fn payload_bytes(&self) -> Result<usize, ManifestError> {
        [&self.source_facts, &self.green, &self.projection]
            .into_iter()
            .try_fold(0_usize, |total, records| {
                records.as_ref().map_or(Some(total), |records| {
                    records.iter().try_fold(total, |subtotal, bytes| {
                        subtotal
                            .checked_add(RECORD_BASE_BYTES)
                            .and_then(|value| value.checked_add(bytes.len()))
                    })
                })
            })
            .ok_or(ManifestError::CapacityPreflight)
    }

    fn record_count(&self) -> Result<usize, ManifestError> {
        [&self.source_facts, &self.green, &self.projection]
            .into_iter()
            .try_fold(0_usize, |total, records| {
                total.checked_add(records.as_ref().map_or(0, VecDeque::len))
            })
            .ok_or(ManifestError::CapacityPreflight)
    }

    fn projection_record_count(&self) -> usize {
        self.projection.as_ref().map_or(0, VecDeque::len)
    }
}

struct RoleRoot {
    authority: CandidateAuthority,
    owner: ArenaBuildOwner,
    metadata: RoleMetadata,
}

struct RetainedReferenceContent {
    owner: ArenaBuildOwner,
    metadata: RoleMetadata,
}

struct PendingRoleRecords {
    role: CandidateRole,
    records: VecDeque<Box<[u8]>>,
    owners: VecDeque<ArenaBuildOwner>,
    hasher: blake3::Hasher,
    record_count: u64,
    canonical_bytes: u64,
}

struct ReleaseRoleRecords {
    role: CandidateRole,
    metadata: RoleMetadata,
    root: ArenaBuildOwner,
    owners: VecDeque<ArenaBuildOwner>,
}

struct ManifestRelease {
    manifest: ArenaBuildOwner,
    next_role: usize,
}

enum ManifestPhase {
    References,
    WrapReference(ReferenceSubtreeRoot),
    WrapRetainedReference(RetainedReferenceContent),
    WrapPersistentSourceFacts,
    WrapPersistentBlocks,
    WrapPersistentRecursiveGreen,
    Records(Box<PendingRoleRecords>),
    WrapPersistentProjection(Box<PendingRoleRecords>),
    ReleaseRecords(ReleaseRoleRecords),
    AllocateManifest,
    ReleaseRoles(ManifestRelease),
    Sealing(CandidateSeal),
    Aborting,
    Complete,
}

pub(crate) struct PublishedManifest {
    authority: CandidateAuthority,
    root: CommittedArenaRoot,
    _not_sync: PhantomData<Cell<()>>,
}

impl PublishedManifest {
    pub(crate) fn authority(&self) -> CandidateAuthority {
        self.authority
    }

    pub(crate) fn root_id(&self) -> ArenaId {
        self.root.id()
    }

    pub(crate) fn into_root(self) -> CommittedArenaRoot {
        self.root
    }
}

pub(crate) enum ManifestPoll {
    Pending {
        transitions: usize,
    },
    Published {
        transitions: usize,
        publication: PublishedManifest,
    },
    Aborting,
}

pub(crate) struct CandidateManifestAssembler {
    authority: CandidateAuthority,
    build: Option<CandidateBuild>,
    phase: ManifestPhase,
    references: Option<ReferenceRootBuilder>,
    persistent_source_facts: Option<RetainedPersistentSourceFactsRole>,
    persistent_source_facts_setup: Option<SequenceInspectionReceipt>,
    persistent_blocks: Option<M11RetainedBlockSequenceRoot>,
    persistent_recursive_green: Option<M11RetainedRecursiveGreenRoot>,
    persistent_inline_projection: Option<RetainedM11InlineProjectionRole>,
    reference_reserve: ReferenceReserve,
    records: CanonicalRoleInputs,
    source_facts_schema: u32,
    green_schema: u32,
    projection_schema: u32,
    roles: [Option<RoleRoot>; CANDIDATE_ROLE_COUNT],
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for CandidateManifestAssembler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateManifestAssembler")
            .field("authority", &self.authority)
            .field("phase", &std::mem::discriminant(&self.phase))
            .field(
                "completed_roles",
                &self.roles.iter().filter(|role| role.is_some()).count(),
            )
            .finish_non_exhaustive()
    }
}

impl Drop for CandidateManifestAssembler {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                matches!(
                    self.phase,
                    ManifestPhase::Aborting | ManifestPhase::Complete
                ),
                "candidate manifest must publish or transfer to fuelled abort on its worker"
            );
        }
    }
}

fn schema_for(role: CandidateRole) -> u32 {
    match role {
        CandidateRole::SourceFacts => SOURCE_FACTS_LEGACY_RECORD_SCHEMA,
        CandidateRole::Green => GREEN_SCHEMA,
        CandidateRole::Projection => PROJECTION_SCHEMA,
        CandidateRole::References => REFERENCES_SCHEMA,
        CandidateRole::CleanEofOnly => CLEAN_EOF_SCHEMA,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistentProjectionSchemaRoute {
    Inline,
    Block,
}

fn persistent_projection_schema_route(schema: u32) -> Option<PersistentProjectionSchemaRoute> {
    match schema {
        PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA => Some(PersistentProjectionSchemaRoute::Inline),
        PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA => Some(PersistentProjectionSchemaRoute::Block),
        _ => None,
    }
}

fn schema_is_supported(role: CandidateRole, schema: u32) -> bool {
    match role {
        CandidateRole::SourceFacts => matches!(
            schema,
            SOURCE_FACTS_LEGACY_RECORD_SCHEMA | PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA
        ),
        CandidateRole::Green => {
            matches!(
                schema,
                GREEN_SCHEMA
                    | PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA
                    | PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
            )
        }
        CandidateRole::Projection => {
            schema == PROJECTION_SCHEMA || persistent_projection_schema_route(schema).is_some()
        }
        _ => schema == schema_for(role),
    }
}

fn map_source_facts_manifest_error(error: SourceFactsAssemblyError) -> ManifestError {
    match error {
        SourceFactsAssemblyError::SourceMismatch
        | SourceFactsAssemblyError::ProfileMismatch
        | SourceFactsAssemblyError::SourceDimensionMismatch
        | SourceFactsAssemblyError::CanonicalSummaryMismatch => ManifestError::CrossAuthority,
        SourceFactsAssemblyError::Arena(error) => ManifestError::Arena(error),
        SourceFactsAssemblyError::InvalidLimits
        | SourceFactsAssemblyError::InvalidProfile
        | SourceFactsAssemblyError::AdmissionCheckpointLimitExceeded { .. }
        | SourceFactsAssemblyError::AdmissionPageLimitExceeded { .. }
        | SourceFactsAssemblyError::AdmissionResidentBytesLimitExceeded { .. }
        | SourceFactsAssemblyError::CounterExhausted
        | SourceFactsAssemblyError::AllocationFailed => ManifestError::InvalidLimits,
        _ => ManifestError::Corrupt("persistent SourceFacts validation failed"),
    }
}

fn map_recursive_green_manifest_error(error: M11RecursiveGreenError) -> ManifestError {
    match error {
        M11RecursiveGreenError::WrongRuntime | M11RecursiveGreenError::SourceAuthorityMismatch => {
            ManifestError::CrossAuthority
        }
        M11RecursiveGreenError::Arena(error) => ManifestError::Arena(error),
        error => ManifestError::RecursiveGreen(error),
    }
}

#[cfg(feature = "parser-internal")]
fn map_reference_journal_manifest_error(error: M11ReferenceJournalError) -> ManifestError {
    if error.is_wrong_runtime() || error.is_source_authority_mismatch() {
        ManifestError::CrossAuthority
    } else if error.is_resource_limit() {
        ManifestError::CapacityPreflight
    } else {
        ManifestError::Corrupt("persistent References validation failed")
    }
}

fn next_record_role(role: CandidateRole) -> Option<CandidateRole> {
    match role {
        CandidateRole::SourceFacts => Some(CandidateRole::Green),
        CandidateRole::Green => Some(CandidateRole::Projection),
        CandidateRole::Projection => Some(CandidateRole::CleanEofOnly),
        CandidateRole::CleanEofOnly | CandidateRole::References => None,
    }
}

pub(crate) fn role_index(role: CandidateRole) -> usize {
    usize::from(role as u8 - 1)
}

fn reference_reserve(inputs: &CanonicalRoleInputs) -> Result<ReferenceReserve, ManifestError> {
    // Reference wrapper, all parser records, four non-reference wrappers,
    // the clean-EOF record, and the final manifest.
    let nodes = inputs
        .record_count()?
        .checked_add(7)
        .ok_or(ManifestError::CapacityPreflight)?;
    let payload_bytes = ROLE_ROOT_PAYLOAD_BYTES
        .checked_add(inputs.payload_bytes()?)
        .and_then(|value| value.checked_add(3 * ROLE_ROOT_PAYLOAD_BYTES))
        .and_then(|value| value.checked_add(RECORD_BASE_BYTES + CHECKPOINT_RECORD_BYTES))
        .and_then(|value| value.checked_add(ROLE_ROOT_PAYLOAD_BYTES))
        .and_then(|value| value.checked_add(MANIFEST_PAYLOAD_BYTES))
        .ok_or(ManifestError::CapacityPreflight)?;
    Ok(ReferenceReserve {
        nodes,
        payload_bytes,
    })
}

fn checkpoint_record(authority: CandidateAuthority) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity(CHECKPOINT_RECORD_BYTES);
    bytes.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&authority.source_bytes.to_le_bytes());
    bytes.extend_from_slice(&authority.source_utf16.to_le_bytes());
    debug_assert_eq!(bytes.len(), CHECKPOINT_RECORD_BYTES);
    bytes.into_boxed_slice()
}

fn record_digest(
    _authority: CandidateAuthority,
    role: CandidateRole,
    schema: u32,
    ordinal: u64,
    bytes: &[u8],
) -> Result<([u8; STRONG_DIGEST_BYTES], u64), ManifestError> {
    let canonical_bytes = u64::try_from(bytes.len()).map_err(|_| ManifestError::InvalidLimits)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.record.v2\0");
    hasher.update(&[role as u8]);
    hasher.update(&schema.to_le_bytes());
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&canonical_bytes.to_le_bytes());
    hasher.update(bytes);
    Ok((*hasher.finalize().as_bytes(), canonical_bytes))
}

fn encode_record(
    _authority: CandidateAuthority,
    role: CandidateRole,
    schema: u32,
    ordinal: u64,
    bytes: &[u8],
) -> Result<Vec<u8>, ManifestError> {
    let mut output = encode_canonical_node_header(RECORD_TAG);
    output.push(role as u8);
    output.extend_from_slice(&[0; 3]);
    output.extend_from_slice(&schema.to_le_bytes());
    output.extend_from_slice(&ordinal.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(bytes.len())
            .map_err(|_| ManifestError::InvalidLimits)?
            .to_le_bytes(),
    );
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(bytes);
    Ok(output)
}

pub(crate) fn push_role_metadata(output: &mut Vec<u8>, metadata: RoleMetadata) {
    output.push(metadata.role as u8);
    output.extend_from_slice(&[0; 3]);
    output.extend_from_slice(&metadata.schema.to_le_bytes());
    output.extend_from_slice(&metadata.record_count.to_le_bytes());
    output.extend_from_slice(&metadata.canonical_bytes.to_le_bytes());
    output.extend_from_slice(&metadata.digest);
}

pub(crate) fn read_role_metadata(
    input: &[u8],
    offset: usize,
) -> Result<RoleMetadata, ManifestError> {
    if input
        .get(offset + 1..offset + 4)
        .is_none_or(|reserved| reserved != [0; 3])
    {
        return Err(ManifestError::Corrupt(
            "role metadata reserved bytes changed",
        ));
    }
    let role = CandidateRole::decode(
        *input
            .get(offset)
            .ok_or(ManifestError::Corrupt("truncated role metadata"))?,
    )?;
    let digest: [u8; STRONG_DIGEST_BYTES] = input
        .get(offset + 24..offset + ROLE_METADATA_BYTES)
        .ok_or(ManifestError::Corrupt("truncated role digest"))?
        .try_into()
        .map_err(|_| ManifestError::Corrupt("invalid role digest"))?;
    Ok(RoleMetadata {
        role,
        schema: read_u32(input, offset + 4)?,
        record_count: read_u64(input, offset + 8)?,
        canonical_bytes: read_u64(input, offset + 16)?,
        digest,
    })
}

fn encode_role_root(authority: CandidateAuthority, metadata: RoleMetadata) -> Vec<u8> {
    let mut output = encode_candidate_header(ROLE_ROOT_TAG, authority);
    push_role_metadata(&mut output, metadata);
    debug_assert_eq!(output.len(), ROLE_ROOT_PAYLOAD_BYTES);
    output
}

fn manifest_digest(
    authority: CandidateAuthority,
    metadata: &[RoleMetadata; CANDIDATE_ROLE_COUNT],
) -> [u8; STRONG_DIGEST_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.manifest.v1\0");
    hash_authority(&mut hasher, authority);
    hasher.update(&[CANDIDATE_ROLE_COUNT_U8]);
    for role in metadata {
        hasher.update(&[role.role as u8]);
        hasher.update(&role.schema.to_le_bytes());
        hasher.update(&role.record_count.to_le_bytes());
        hasher.update(&role.canonical_bytes.to_le_bytes());
        hasher.update(&role.digest);
    }
    *hasher.finalize().as_bytes()
}

fn encode_manifest(
    authority: CandidateAuthority,
    metadata: &[RoleMetadata; CANDIDATE_ROLE_COUNT],
) -> Vec<u8> {
    let mut output = encode_candidate_header(MANIFEST_TAG, authority);
    output.push(CANDIDATE_ROLE_COUNT_U8);
    output.extend_from_slice(&[0; 7]);
    for role in metadata {
        push_role_metadata(&mut output, *role);
    }
    output.extend_from_slice(&manifest_digest(authority, metadata));
    debug_assert_eq!(output.len(), MANIFEST_PAYLOAD_BYTES);
    output
}

enum DriveOutcome {
    Progress,
    Idle,
    Seal(ArenaBuildOwner),
}

impl CandidateManifestAssembler {
    pub(crate) const fn authority(&self) -> CandidateAuthority {
        self.authority
    }

    pub(crate) const fn persistent_source_facts_setup(&self) -> Option<SequenceInspectionReceipt> {
        self.persistent_source_facts_setup
    }

    pub(crate) fn new(
        arena: &mut PageArena,
        authority: CandidateAuthority,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, true)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
        {
            return Err(ManifestError::InvalidLimits);
        }
        let references = ReferenceRootBuilder::new(authority, reference_limits)?;
        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(2) // candidate anchor plus empty reference root
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(CANDIDATE_HEADER_BYTES)
            .and_then(|value| value.checked_add(REFERENCE_ROOT_PAYLOAD_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let build = {
            let mut session = arena.begin_build()?;
            let anchor = encode_candidate_header(ANCHOR_TAG, authority);
            let _ = session.allocate(&anchor, &[])?;
            session
                .suspend()
                .expect("journalled candidate anchor must suspend")
        };
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::References,
            references: Some(references),
            persistent_source_facts: None,
            persistent_source_facts_setup: None,
            persistent_blocks: None,
            persistent_recursive_green: None,
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: SOURCE_FACTS_LEGACY_RECORD_SCHEMA,
            green_schema: GREEN_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts a candidate whose SourceFacts role directly retains the current
    /// actor-owned persistent measured sequence.
    ///
    /// The runtime keeps its committed owner. This constructor validates exact
    /// source, parser profile, scan profile, and clean-EOF coverage before
    /// retaining the canonical root into the fresh candidate journal.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        let references = ReferenceRootBuilder::new(authority, reference_limits)?;
        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(2) // candidate anchor plus empty reference root
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .and_then(|value| value.checked_add(REFERENCE_ROOT_PAYLOAD_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained) = {
            let mut session = arena.begin_build()?;
            let anchor = encode_candidate_header(ANCHOR_TAG, authority);
            let _ = session.allocate(&anchor, &[])?;
            let retained = persistent
                .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                .map_err(map_source_facts_manifest_error)?;
            let build = session
                .suspend()
                .expect("journalled candidate anchor and SourceFacts retain must suspend");
            (build, retained)
        };
        let persistent_source_facts_setup = Some(retained.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::References,
            references: Some(references),
            persistent_source_facts: Some(retained),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: None,
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: GREEN_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts a candidate whose Green and Projection roles share one retained
    /// persistent block tree behind two fresh authority-bound wrappers.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_and_blocks(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        blocks: &M11BlockSequenceRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
    ) -> Result<Self, ManifestError> {
        let records = CanonicalRoleInputs::persistent_blocks();
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        let references = ReferenceRootBuilder::new(authority, reference_limits)?;
        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(2)
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| {
                value.checked_add(
                    2_usize
                        .checked_mul(PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES)
                        .expect("two block descriptors fit usize"),
                )
            })
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .and_then(|value| value.checked_add(REFERENCE_ROOT_PAYLOAD_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_blocks) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_blocks =
                    blocks.retain_for_publication(&mut session, runtime_identity, source)?;
                Ok((retained_source_facts, retained_blocks))
            })();
            match staged {
                Ok((source_facts, blocks)) => {
                    let build = session
                        .suspend()
                        .expect("journalled persistent candidate roots must suspend");
                    (build, source_facts, blocks)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed persistent block candidate journal must suspend");
                    arena
                        .abort_build(build)
                        .expect("same-arena persistent block candidate journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::References,
            references: Some(references),
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: Some(retained_blocks),
            persistent_recursive_green: None,
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
            projection_schema: PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts a candidate whose Green role directly retains one recursive
    /// Green tree while Projection remains a bounded ordinary record role.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_and_recursive_green(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        recursive_green: &M11RecursiveGreenRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        if records.source_facts.is_some() || records.green.is_some() || records.projection.is_none()
        {
            return Err(ManifestError::InvalidRole);
        }
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        let references = ReferenceRootBuilder::new(authority, reference_limits)?;
        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(2)
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES))
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .and_then(|value| value.checked_add(REFERENCE_ROOT_PAYLOAD_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_recursive_green) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_recursive_green = recursive_green
                    .retain_for_publication(&mut session, runtime_identity, source)
                    .map_err(map_recursive_green_manifest_error)?;
                Ok((retained_source_facts, retained_recursive_green))
            })();
            match staged {
                Ok((source_facts, recursive_green)) => {
                    let build = session
                        .suspend()
                        .expect("journalled recursive Green candidate roots must suspend");
                    (build, source_facts, recursive_green)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed recursive Green candidate journal must suspend");
                    arena
                        .abort_build(build)
                        .expect("same-arena recursive Green candidate journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::References,
            references: Some(references),
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: Some(retained_recursive_green),
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts one cold candidate that retains the parser session's three
    /// already-committed authorities into a single failure-atomic journal:
    ///
    /// - the runtime-owned persistent SourceFacts root for `source`;
    /// - one persistent recursive Green root for the same source; and
    /// - the session-owned canonical References journal root.
    ///
    /// Fresh candidate wrappers bind all retained canonical content to
    /// `authority`. The parser session continues to own `recursive_green` and
    /// `references`; this candidate only retains their arena roots.
    #[cfg(feature = "parser-internal")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_recursive_green_and_reference_journal(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        recursive_green: &M11RecursiveGreenRoot,
        references: &M11ReferenceJournalRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        if records.source_facts.is_some() || records.green.is_some() || records.projection.is_none()
        {
            return Err(ManifestError::InvalidRole);
        }
        let source_bytes =
            u64::try_from(source.byte_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        let source_utf16 =
            u64::try_from(source.utf16_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        if authority.source_root != source.root()
            || authority.source_revision != source.revision()
            || authority.source_bytes != source_bytes
            || authority.source_utf16 != source_utf16
        {
            return Err(ManifestError::CrossAuthority);
        }

        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(1) // candidate anchor; every canonical root is retained
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES))
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_recursive_green, retained_references, metadata) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_recursive_green = recursive_green
                    .retain_for_publication(&mut session, runtime_identity, source)
                    .map_err(map_recursive_green_manifest_error)?;
                let (retained_references, metadata) = references
                    .retain_for_publication(&mut session, runtime_identity, source)
                    .map_err(map_reference_journal_manifest_error)?;
                Ok((
                    retained_source_facts,
                    retained_recursive_green,
                    retained_references,
                    metadata,
                ))
            })();
            match staged {
                Ok((source_facts, recursive_green, references, metadata)) => {
                    let build = session
                        .suspend()
                        .expect("journalled cold recursive Green candidate must suspend");
                    (build, source_facts, recursive_green, references, metadata)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed cold recursive Green journal must suspend for abort");
                    arena
                        .abort_build(build)
                        .expect("same-arena cold recursive Green journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::WrapRetainedReference(RetainedReferenceContent {
                owner: retained_references,
                metadata,
            }),
            references: None,
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: Some(retained_recursive_green),
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts one exact target that retains all three canonical authorities
    /// into a single failure-atomic journal:
    ///
    /// - the runtime-owned persistent SourceFacts root for `source`;
    /// - one persistent recursive Green root for the same source; and
    /// - the canonical References content root from `base`.
    ///
    /// The base manifest and its authority-bound wrappers are never inherited.
    /// Fresh target wrappers bind the retained canonical roots to `authority`.
    /// The caller must already hold the parser proof that reference reuse is
    /// exact for the target source.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_and_recursive_green_reusing_references(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        recursive_green: &M11RecursiveGreenRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
        base: &PublishedManifest,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        if records.source_facts.is_some() || records.green.is_some() || records.projection.is_none()
        {
            return Err(ManifestError::InvalidRole);
        }
        let source_bytes =
            u64::try_from(source.byte_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        let source_utf16 =
            u64::try_from(source.utf16_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        if authority.source_root != source.root()
            || authority.source_revision != source.revision()
            || authority.source_bytes != source_bytes
            || authority.source_utf16 != source_utf16
        {
            return Err(ManifestError::CrossAuthority);
        }
        if base.authority.document != authority.document
            || base.authority.syntax_profile != authority.syntax_profile
            || base.authority.publication == authority.publication
            || base.authority.parse_generation >= authority.parse_generation
            || base.authority.source_revision > authority.source_revision
        {
            return Err(ManifestError::CrossAuthority);
        }

        let descriptor = decode_manifest(arena, base.root_id(), base.authority)?;
        let metadata = descriptor.metadata[role_index(CandidateRole::References)];
        let canonical_reference_root = decode_role_root(
            arena,
            descriptor.children[role_index(CandidateRole::References)],
            base.authority,
            metadata,
        )?
        .ok_or(ManifestError::Corrupt(
            "reference role lost its canonical root",
        ))?;

        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(1) // candidate anchor; every canonical root is retained
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES))
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_recursive_green, retained_references) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_recursive_green = recursive_green
                    .retain_for_publication(&mut session, runtime_identity, source)
                    .map_err(map_recursive_green_manifest_error)?;
                let retained_references = session.retain(canonical_reference_root)?;
                Ok((
                    retained_source_facts,
                    retained_recursive_green,
                    retained_references,
                ))
            })();
            match staged {
                Ok((source_facts, recursive_green, references)) => {
                    let build = session
                        .suspend()
                        .expect("journalled exact recursive Green candidate must suspend");
                    (build, source_facts, recursive_green, references)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed exact recursive Green journal must suspend for abort");
                    arena
                        .abort_build(build)
                        .expect("same-arena exact recursive Green journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::WrapRetainedReference(RetainedReferenceContent {
                owner: retained_references,
                metadata,
            }),
            references: None,
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: Some(retained_recursive_green),
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts a candidate whose SourceFacts and inline Projection roles both
    /// retain already-committed measured roots.
    ///
    /// Projection schema v2 keeps the old bounded structural Projection
    /// records beside one measured inline root. Logical inline pages therefore
    /// do not consume wrapper fanout and are never replayed into flat records.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_and_inline_projection(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        inline_projection: &M11InlineProjectionRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        let inline_root_count =
            usize::from(inline_projection.descriptor().storage_page_count() != 0)
                + usize::from(
                    inline_projection
                        .descriptor()
                        .link_value_storage_page_count()
                        != 0,
                );
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
            || records
                .projection_record_count()
                .checked_add(inline_root_count)
                .is_none_or(|children| children > reference_limits.arena.max_children_per_node)
        {
            return Err(ManifestError::InvalidLimits);
        }
        let references = ReferenceRootBuilder::new(authority, reference_limits)?;
        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(2)
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES))
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .and_then(|value| value.checked_add(REFERENCE_ROOT_PAYLOAD_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_inline_projection) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_inline_projection = inline_projection.retain_for_publication(
                    &mut session,
                    runtime_identity,
                    source,
                    parser_profile,
                )?;
                Ok((retained_source_facts, retained_inline_projection))
            })();
            match staged {
                Ok((source_facts, inline_projection)) => {
                    let build = session
                        .suspend()
                        .expect("journalled persistent candidate roots must suspend");
                    (build, source_facts, inline_projection)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed persistent candidate journal must suspend");
                    arena
                        .abort_build(build)
                        .expect("same-arena persistent candidate journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::References,
            references: Some(references),
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: None,
            persistent_inline_projection: Some(retained_inline_projection),
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: GREEN_SCHEMA,
            projection_schema: PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts a target candidate by directly retaining the canonical References
    /// content of one exact, already-published base in this same arena.
    ///
    /// This is an ownership primitive, not semantic reuse authority. Its caller
    /// must already have proved that the base References facts, including their
    /// absolute source ranges, remain exact for `authority`. The constructor
    /// authenticates the base manifest and role shape, then journals the
    /// canonical content root before any fresh target wrapper can refer to it.
    #[cfg(test)]
    pub(crate) fn new_reusing_references(
        arena: &mut PageArena,
        authority: CandidateAuthority,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
        base: &PublishedManifest,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, true)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
        {
            return Err(ManifestError::InvalidLimits);
        }
        if base.authority.document != authority.document
            || base.authority.syntax_profile != authority.syntax_profile
            || base.authority.publication == authority.publication
            || base.authority.parse_generation >= authority.parse_generation
            || base.authority.source_revision > authority.source_revision
        {
            return Err(ManifestError::CrossAuthority);
        }

        let descriptor = decode_manifest(arena, base.root_id(), base.authority)?;
        let metadata = descriptor.metadata[role_index(CandidateRole::References)];
        let canonical_root = decode_role_root(
            arena,
            descriptor.children[role_index(CandidateRole::References)],
            base.authority,
            metadata,
        )?
        .ok_or(ManifestError::Corrupt(
            "reference role lost its canonical root",
        ))?;

        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(1) // candidate anchor; retained content allocates no slot
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(CANDIDATE_HEADER_BYTES)
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained) = {
            let mut session = arena.begin_build()?;
            let anchor = encode_candidate_header(ANCHOR_TAG, authority);
            let _ = session.allocate(&anchor, &[])?;
            let retained = session.retain(canonical_root)?;
            let build = session
                .suspend()
                .expect("journalled candidate anchor and retained role must suspend");
            (build, retained)
        };
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::WrapRetainedReference(RetainedReferenceContent {
                owner: retained,
                metadata,
            }),
            references: None,
            persistent_source_facts: None,
            persistent_source_facts_setup: None,
            persistent_blocks: None,
            persistent_recursive_green: None,
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: SOURCE_FACTS_LEGACY_RECORD_SCHEMA,
            green_schema: GREEN_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts one fresh target candidate that combines two exact retained
    /// canonical authorities:
    ///
    /// - the runtime-owned persistent SourceFacts root for `source`; and
    /// - the canonical References content root from `base`.
    ///
    /// Both roots are retained into the same fresh candidate journal. Neither
    /// the base manifest nor either base role wrapper is inherited; the state
    /// machine always allocates target-authority wrappers and a target
    /// manifest around the retained canonical content.
    ///
    /// Reference reuse remains an ownership primitive. Its caller must first
    /// prove that all absolute reference ranges in `base` remain exact for the
    /// target source.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_reusing_references(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
        base: &PublishedManifest,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        let source_bytes =
            u64::try_from(source.byte_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        let source_utf16 =
            u64::try_from(source.utf16_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        if authority.source_root != source.root()
            || authority.source_revision != source.revision()
            || authority.source_bytes != source_bytes
            || authority.source_utf16 != source_utf16
        {
            return Err(ManifestError::CrossAuthority);
        }
        if base.authority.document != authority.document
            || base.authority.syntax_profile != authority.syntax_profile
            || base.authority.publication == authority.publication
            || base.authority.parse_generation >= authority.parse_generation
            || base.authority.source_revision > authority.source_revision
        {
            return Err(ManifestError::CrossAuthority);
        }

        let descriptor = decode_manifest(arena, base.root_id(), base.authority)?;
        let metadata = descriptor.metadata[role_index(CandidateRole::References)];
        let canonical_reference_root = decode_role_root(
            arena,
            descriptor.children[role_index(CandidateRole::References)],
            base.authority,
            metadata,
        )?
        .ok_or(ManifestError::Corrupt(
            "reference role lost its canonical root",
        ))?;

        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(1) // candidate anchor; retained content allocates no slot
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_references) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_references = session.retain(canonical_reference_root)?;
                Ok((retained_source_facts, retained_references))
            })();
            match staged {
                Ok((retained_source_facts, retained_references)) => {
                    let build = session
                        .suspend()
                        .expect("journalled combined candidate authority must suspend");
                    (build, retained_source_facts, retained_references)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed combined candidate journal must suspend for abort");
                    arena
                        .abort_build(build)
                        .expect("same-arena combined candidate journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::WrapRetainedReference(RetainedReferenceContent {
                owner: retained_references,
                metadata,
            }),
            references: None,
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: None,
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: GREEN_SCHEMA,
            projection_schema: PROJECTION_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts one fresh target candidate that combines three exact retained
    /// canonical authorities:
    ///
    /// - the runtime-owned persistent SourceFacts root for `source`;
    /// - one persistent block tree shared by Green and Projection; and
    /// - the canonical References content root from `base`.
    ///
    /// Every retained root is journalled before the state machine allocates
    /// fresh target-authority wrappers. The caller continues to own `blocks`
    /// and `base`; this constructor consumes neither capability.
    ///
    /// Reference reuse remains an ownership primitive. Its caller must first
    /// prove that all absolute reference ranges in `base` remain exact for the
    /// target source.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_and_blocks_reusing_references(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        blocks: &M11BlockSequenceRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        base: &PublishedManifest,
    ) -> Result<Self, ManifestError> {
        let records = CanonicalRoleInputs::persistent_blocks();
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        let source_bytes =
            u64::try_from(source.byte_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        let source_utf16 =
            u64::try_from(source.utf16_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
        {
            return Err(ManifestError::InvalidLimits);
        }
        if authority.source_root != source.root()
            || authority.source_revision != source.revision()
            || authority.source_bytes != source_bytes
            || authority.source_utf16 != source_utf16
        {
            return Err(ManifestError::CrossAuthority);
        }
        if base.authority.document != authority.document
            || base.authority.syntax_profile != authority.syntax_profile
            || base.authority.publication == authority.publication
            || base.authority.parse_generation >= authority.parse_generation
            || base.authority.source_revision > authority.source_revision
        {
            return Err(ManifestError::CrossAuthority);
        }

        let descriptor = decode_manifest(arena, base.root_id(), base.authority)?;
        let metadata = descriptor.metadata[role_index(CandidateRole::References)];
        let canonical_reference_root = decode_role_root(
            arena,
            descriptor.children[role_index(CandidateRole::References)],
            base.authority,
            metadata,
        )?
        .ok_or(ManifestError::Corrupt(
            "reference role lost its canonical root",
        ))?;

        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(1) // candidate anchor; retained content allocates no slot
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| {
                value.checked_add(
                    2_usize
                        .checked_mul(PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES)
                        .expect("two block descriptors fit usize"),
                )
            })
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_blocks, retained_references) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_blocks =
                    blocks.retain_for_publication(&mut session, runtime_identity, source)?;
                let retained_references = session.retain(canonical_reference_root)?;
                Ok((retained_source_facts, retained_blocks, retained_references))
            })();
            match staged {
                Ok((source_facts, blocks, references)) => {
                    let build = session
                        .suspend()
                        .expect("journalled exact block candidate authority must suspend");
                    (build, source_facts, blocks, references)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed exact block candidate journal must suspend for abort");
                    arena
                        .abort_build(build)
                        .expect("same-arena exact block candidate journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::WrapRetainedReference(RetainedReferenceContent {
                owner: retained_references,
                metadata,
            }),
            references: None,
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: Some(retained_blocks),
            persistent_recursive_green: None,
            persistent_inline_projection: None,
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
            projection_schema: PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    /// Starts one fresh target candidate that combines three exact retained
    /// canonical authorities:
    ///
    /// - the runtime-owned persistent SourceFacts root for `source`;
    /// - one typed persistent inline Projection root for the same source; and
    /// - the canonical References content root from `base`.
    ///
    /// All three roots are retained into the same fresh candidate journal.
    /// The base manifest and its authority-bound wrappers are not inherited;
    /// the state machine allocates fresh wrappers and a fresh manifest for the
    /// target authority.
    ///
    /// Reference reuse remains an ownership primitive. Its caller must first
    /// prove that all absolute reference ranges in `base` remain exact for the
    /// target source.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_persistent_source_facts_and_inline_projection_reusing_references(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        inline_projection: &M11InlineProjectionRoot,
        runtime_identity: StrongIdentity,
        authority: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
        scan_profile: SourceFactsScanProfile,
        reference_limits: ReferenceRootLimits,
        records: CanonicalRoleInputs,
        base: &PublishedManifest,
    ) -> Result<Self, ManifestError> {
        records.validate(reference_limits.arena.max_children_per_node, false)?;
        let source_bytes =
            u64::try_from(source.byte_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        let source_utf16 =
            u64::try_from(source.utf16_len()).map_err(|_| ManifestError::InvalidAuthority)?;
        let inline_root_count =
            usize::from(inline_projection.descriptor().storage_page_count() != 0)
                + usize::from(
                    inline_projection
                        .descriptor()
                        .link_value_storage_page_count()
                        != 0,
                );
        if arena.limits() != reference_limits.arena
            || reference_limits.arena.max_children_per_node < CANDIDATE_ROLE_COUNT
            || u32::try_from(parser_profile.get()).ok() != Some(authority.syntax_profile)
            || records
                .projection_record_count()
                .checked_add(inline_root_count)
                .is_none_or(|children| children > reference_limits.arena.max_children_per_node)
        {
            return Err(ManifestError::InvalidLimits);
        }
        if authority.source_root != source.root()
            || authority.source_revision != source.revision()
            || authority.source_bytes != source_bytes
            || authority.source_utf16 != source_utf16
        {
            return Err(ManifestError::CrossAuthority);
        }
        if base.authority.document != authority.document
            || base.authority.syntax_profile != authority.syntax_profile
            || base.authority.publication == authority.publication
            || base.authority.parse_generation >= authority.parse_generation
            || base.authority.source_revision > authority.source_revision
        {
            return Err(ManifestError::CrossAuthority);
        }

        let descriptor = decode_manifest(arena, base.root_id(), base.authority)?;
        let metadata = descriptor.metadata[role_index(CandidateRole::References)];
        let canonical_reference_root = decode_role_root(
            arena,
            descriptor.children[role_index(CandidateRole::References)],
            base.authority,
            metadata,
        )?
        .ok_or(ManifestError::Corrupt(
            "reference role lost its canonical root",
        ))?;

        let reserve = reference_reserve(&records)?;
        let initial_nodes = reserve
            .nodes
            .checked_add(1) // candidate anchor; retained content allocates no slot
            .ok_or(ManifestError::CapacityPreflight)?;
        let initial_payload = reserve
            .payload_bytes
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .and_then(|value| value.checked_add(PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES))
            .and_then(|value| value.checked_add(CANDIDATE_HEADER_BYTES))
            .ok_or(ManifestError::CapacityPreflight)?;
        preflight_remaining(
            arena,
            reference_limits.arena,
            initial_nodes,
            initial_payload,
        )?;

        let (build, retained_source_facts, retained_inline_projection, retained_references) = {
            let mut session = arena.begin_build()?;
            let staged = (|| -> Result<_, ManifestError> {
                let anchor = encode_candidate_header(ANCHOR_TAG, authority);
                let _ = session.allocate(&anchor, &[])?;
                let retained_source_facts = persistent
                    .retain_for_publication(&mut session, source, parser_profile, scan_profile)
                    .map_err(map_source_facts_manifest_error)?;
                let retained_inline_projection = inline_projection.retain_for_publication(
                    &mut session,
                    runtime_identity,
                    source,
                    parser_profile,
                )?;
                let retained_references = session.retain(canonical_reference_root)?;
                Ok((
                    retained_source_facts,
                    retained_inline_projection,
                    retained_references,
                ))
            })();
            match staged {
                Ok((source_facts, inline_projection, references)) => {
                    let build = session
                        .suspend()
                        .expect("journalled exact-crop candidate authority must suspend");
                    (build, source_facts, inline_projection, references)
                }
                Err(error) => {
                    let build = session
                        .suspend()
                        .expect("failed exact-crop candidate journal must suspend for abort");
                    arena
                        .abort_build(build)
                        .expect("same-arena exact-crop candidate journal must abort");
                    return Err(error);
                }
            }
        };
        let persistent_source_facts_setup = Some(retained_source_facts.inspection());
        Ok(Self {
            authority,
            build: Some(build),
            phase: ManifestPhase::WrapRetainedReference(RetainedReferenceContent {
                owner: retained_references,
                metadata,
            }),
            references: None,
            persistent_source_facts: Some(retained_source_facts),
            persistent_source_facts_setup,
            persistent_blocks: None,
            persistent_recursive_green: None,
            persistent_inline_projection: Some(retained_inline_projection),
            reference_reserve: reserve,
            records,
            source_facts_schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
            green_schema: GREEN_SCHEMA,
            projection_schema: PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA,
            roles: std::array::from_fn(|_| None),
            _not_sync: PhantomData,
        })
    }

    pub(crate) fn offer_reference(
        &mut self,
        arena: &PageArena,
        fact: AuthoritativeReferenceFact,
    ) -> Result<(), ManifestError> {
        if !matches!(self.phase, ManifestPhase::References) || self.build.is_none() {
            return Err(ManifestError::Busy);
        }
        self.references
            .as_mut()
            .ok_or(ManifestError::Corrupt(
                "reference-building candidate lost its builder",
            ))?
            .offer(fact, arena, self.reference_reserve)
            .map_err(Into::into)
    }

    pub(crate) fn begin_reference_stream(
        &mut self,
        arena: &PageArena,
        fact: AuthoritativeReferenceFactStart,
    ) -> Result<(), ManifestError> {
        if !matches!(self.phase, ManifestPhase::References) || self.build.is_none() {
            return Err(ManifestError::Busy);
        }
        self.references
            .as_mut()
            .ok_or(ManifestError::Corrupt(
                "reference-building candidate lost its builder",
            ))?
            .begin_stream(fact, arena, self.reference_reserve)
            .map_err(Into::into)
    }

    pub(crate) fn reference_stream_capacity(
        &self,
        kind: StreamedReferenceValueKind,
    ) -> Result<usize, ManifestError> {
        self.references
            .as_ref()
            .ok_or(ManifestError::Busy)?
            .stream_capacity(kind)
            .map_err(Into::into)
    }

    pub(crate) fn offer_reference_stream_bytes(
        &mut self,
        kind: StreamedReferenceValueKind,
        bytes: &[u8],
    ) -> Result<usize, ManifestError> {
        self.references
            .as_mut()
            .ok_or(ManifestError::Busy)?
            .offer_stream_bytes(kind, bytes)
            .map_err(Into::into)
    }

    pub(crate) fn reference_stream_retained_bytes(&self) -> usize {
        self.references
            .as_ref()
            .map_or(0, ReferenceRootBuilder::stream_retained_bytes)
    }

    pub(crate) fn references_idle(&self) -> bool {
        matches!(self.phase, ManifestPhase::References)
            && self
                .references
                .as_ref()
                .is_some_and(ReferenceRootBuilder::is_idle)
    }

    pub(crate) fn finish_references(&mut self, arena: &PageArena) -> Result<(), ManifestError> {
        if !matches!(self.phase, ManifestPhase::References) || self.build.is_none() {
            return Err(ManifestError::Busy);
        }
        self.references
            .as_mut()
            .ok_or(ManifestError::Corrupt(
                "reference-building candidate lost its builder",
            ))?
            .finish(arena, self.reference_reserve)
            .map_err(Into::into)
    }

    fn begin_role_records(
        &mut self,
        role: CandidateRole,
    ) -> Result<PendingRoleRecords, ManifestError> {
        let records = if role == CandidateRole::CleanEofOnly {
            VecDeque::from([checkpoint_record(self.authority)])
        } else {
            self.records.take(role)?
        };
        let mut owners = VecDeque::new();
        owners
            .try_reserve(records.len())
            .map_err(|_| ManifestError::Arena(ArenaError::AllocationFailed))?;
        Ok(PendingRoleRecords {
            role,
            records,
            owners,
            hasher: role_hasher(self.authority, role, self.role_schema(role)),
            record_count: 0,
            canonical_bytes: 0,
        })
    }

    fn role_schema(&self, role: CandidateRole) -> u32 {
        match role {
            CandidateRole::SourceFacts => self.source_facts_schema,
            CandidateRole::Green => self.green_schema,
            CandidateRole::Projection => self.projection_schema,
            _ => schema_for(role),
        }
    }

    pub(crate) fn poll(
        &mut self,
        arena: &mut PageArena,
        fuel: usize,
    ) -> Result<ManifestPoll, ManifestError> {
        if fuel == 0 {
            return Err(ManifestError::ZeroFuel);
        }
        if matches!(self.phase, ManifestPhase::Aborting) {
            return Ok(ManifestPoll::Aborting);
        }
        if matches!(self.phase, ManifestPhase::Complete) {
            return Err(ManifestError::Busy);
        }
        if matches!(self.phase, ManifestPhase::Sealing(_)) {
            return self.poll_seal(arena, fuel);
        }

        let Some(build) = self.build.take() else {
            return Err(ManifestError::Corrupt("manifest lost build capability"));
        };
        if let Err(error) = arena.validate_suspended_build(&build) {
            self.build = Some(build);
            return Err(error.into());
        }
        let mut session = arena
            .resume_build(build)
            .expect("prevalidated manifest build must resume");
        let mut transitions = 0;
        let mut seal_root = None;
        while transitions < fuel {
            match self.drive_one(&mut session) {
                Ok(DriveOutcome::Progress) => transitions += 1,
                Ok(DriveOutcome::Idle) => break,
                Ok(DriveOutcome::Seal(root)) => {
                    transitions += 1;
                    seal_root = Some(root);
                    break;
                }
                Err(error) => {
                    drop(session);
                    self.phase = ManifestPhase::Aborting;
                    self.roles = std::array::from_fn(|_| None);
                    return Err(error);
                }
            }
        }

        let build = match session.suspend() {
            Ok(build) => build,
            Err(error) => {
                self.phase = ManifestPhase::Aborting;
                self.roles = std::array::from_fn(|_| None);
                return Err(error.into());
            }
        };
        if let Some(root) = seal_root {
            match arena.begin_seal(build, root) {
                Ok(seal) => self.phase = ManifestPhase::Sealing(seal),
                Err(failure) => {
                    let original = failure.error;
                    self.build = Some(failure.build);
                    let _ = failure.root;
                    self.begin_abort(arena)?;
                    return Err(original.into());
                }
            }
        } else {
            if matches!(
                self.phase,
                ManifestPhase::References | ManifestPhase::WrapReference(_)
            ) {
                let owner_count = match arena.suspended_build_owner_count(&build) {
                    Ok(count) => count,
                    Err(error) => {
                        self.build = Some(build);
                        return Err(error.into());
                    }
                };
                if owner_count > MAX_REFERENCE_WORKING_OWNERS {
                    self.build = Some(build);
                    self.begin_abort(arena)?;
                    return Err(ManifestError::Corrupt(
                        "reference paging exceeded the fixed journal-owner envelope",
                    ));
                }
            }
            self.build = Some(build);
        }
        Ok(ManifestPoll::Pending { transitions })
    }

    #[allow(clippy::too_many_lines)]
    fn drive_one(
        &mut self,
        session: &mut crate::storage::ArenaBuildSession<'_>,
    ) -> Result<DriveOutcome, ManifestError> {
        let phase = std::mem::replace(&mut self.phase, ManifestPhase::Complete);
        match phase {
            ManifestPhase::References => match self
                .references
                .as_mut()
                .ok_or(ManifestError::Corrupt(
                    "reference-building candidate lost its builder",
                ))?
                .poll(session, 1)?
            {
                ReferenceBuildPoll::Pending { transitions, idle } => {
                    debug_assert!(transitions <= 1);
                    self.phase = ManifestPhase::References;
                    Ok(if idle {
                        DriveOutcome::Idle
                    } else {
                        DriveOutcome::Progress
                    })
                }
                ReferenceBuildPoll::Complete { transitions, root } => {
                    debug_assert_eq!(transitions, 1);
                    self.phase = ManifestPhase::WrapReference(root);
                    Ok(DriveOutcome::Progress)
                }
            },
            ManifestPhase::WrapReference(root) => {
                if root.authority != self.authority
                    || root.metadata.role != CandidateRole::References
                    || root.metadata.schema != REFERENCES_SCHEMA
                {
                    self.phase = ManifestPhase::WrapReference(root);
                    return Err(ManifestError::CrossAuthority);
                }
                let payload = encode_role_root(self.authority, root.metadata);
                let owner = session.allocate(&payload, &[root.owner.id()])?;
                session.release(root.owner)?;
                self.roles[role_index(CandidateRole::References)] = Some(RoleRoot {
                    authority: self.authority,
                    owner,
                    metadata: root.metadata,
                });
                self.phase = self.phase_after_references()?;
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::WrapRetainedReference(retained) => {
                if retained.metadata.role != CandidateRole::References
                    || retained.metadata.schema != REFERENCES_SCHEMA
                {
                    self.phase = ManifestPhase::WrapRetainedReference(retained);
                    return Err(ManifestError::InvalidRole);
                }
                let payload = encode_role_root(self.authority, retained.metadata);
                let owner = session.allocate(&payload, &[retained.owner.id()])?;
                session.release(retained.owner)?;
                self.roles[role_index(CandidateRole::References)] = Some(RoleRoot {
                    authority: self.authority,
                    owner,
                    metadata: retained.metadata,
                });
                self.phase = self.phase_after_references()?;
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::WrapPersistentSourceFacts => {
                let mut retained =
                    self.persistent_source_facts
                        .take()
                        .ok_or(ManifestError::Corrupt(
                            "persistent SourceFacts phase lost its retained root",
                        ))?;
                let metadata = RoleMetadata {
                    role: CandidateRole::SourceFacts,
                    schema: PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
                    record_count: retained.record_count(),
                    canonical_bytes: retained.canonical_bytes(),
                    digest: retained.digest(),
                };
                let mut payload = encode_role_root(self.authority, metadata);
                payload.extend_from_slice(retained.descriptor());
                let retained_root = retained.take_owner();
                let owner = match retained_root.as_ref() {
                    Some(root) => session.allocate(&payload, &[root.id()])?,
                    None => session.allocate(&payload, &[])?,
                };
                if let Some(root) = retained_root {
                    session.release(root)?;
                }
                self.roles[role_index(CandidateRole::SourceFacts)] = Some(RoleRoot {
                    authority: self.authority,
                    owner,
                    metadata,
                });
                self.phase = if self.persistent_blocks.is_some() {
                    ManifestPhase::WrapPersistentBlocks
                } else if self.persistent_recursive_green.is_some() {
                    ManifestPhase::WrapPersistentRecursiveGreen
                } else {
                    ManifestPhase::Records(Box::new(self.begin_role_records(CandidateRole::Green)?))
                };
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::WrapPersistentBlocks => {
                let mut retained = self.persistent_blocks.take().ok_or(ManifestError::Corrupt(
                    "persistent block phase lost its retained root",
                ))?;
                let green_descriptor = retained.descriptor(M11BlockRoleLane::Green);
                let projection_descriptor = retained.descriptor(M11BlockRoleLane::Projection);
                let green_descriptor_bytes =
                    encode_persistent_m11_block_role_descriptor(green_descriptor)?;
                let projection_descriptor_bytes =
                    encode_persistent_m11_block_role_descriptor(projection_descriptor)?;
                let green_metadata = RoleMetadata {
                    role: CandidateRole::Green,
                    schema: PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
                    record_count: green_descriptor.record_count(),
                    canonical_bytes: green_descriptor.canonical_bytes(),
                    digest: persistent_block_role_digest(
                        self.authority,
                        CandidateRole::Green,
                        PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
                        &green_descriptor_bytes,
                        green_descriptor.record_count(),
                        green_descriptor.canonical_bytes(),
                    ),
                };
                let projection_metadata = RoleMetadata {
                    role: CandidateRole::Projection,
                    schema: PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA,
                    record_count: projection_descriptor.record_count(),
                    canonical_bytes: projection_descriptor.canonical_bytes(),
                    digest: persistent_block_role_digest(
                        self.authority,
                        CandidateRole::Projection,
                        PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA,
                        &projection_descriptor_bytes,
                        projection_descriptor.record_count(),
                        projection_descriptor.canonical_bytes(),
                    ),
                };
                let retained_root = retained.take_owner();
                let green_payload = {
                    let mut payload = encode_role_root(self.authority, green_metadata);
                    payload.extend_from_slice(&green_descriptor_bytes);
                    payload
                };
                let projection_payload = {
                    let mut payload = encode_role_root(self.authority, projection_metadata);
                    payload.extend_from_slice(&projection_descriptor_bytes);
                    payload
                };
                let green_owner = match retained_root.as_ref() {
                    Some(root) => session.allocate(&green_payload, &[root.id()])?,
                    None => session.allocate(&green_payload, &[])?,
                };
                let projection_owner = match retained_root.as_ref() {
                    Some(root) => session.allocate(&projection_payload, &[root.id()])?,
                    None => session.allocate(&projection_payload, &[])?,
                };
                if let Some(root) = retained_root {
                    session.release(root)?;
                }
                self.roles[role_index(CandidateRole::Green)] = Some(RoleRoot {
                    authority: self.authority,
                    owner: green_owner,
                    metadata: green_metadata,
                });
                self.roles[role_index(CandidateRole::Projection)] = Some(RoleRoot {
                    authority: self.authority,
                    owner: projection_owner,
                    metadata: projection_metadata,
                });
                self.phase = ManifestPhase::Records(Box::new(
                    self.begin_role_records(CandidateRole::CleanEofOnly)?,
                ));
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::WrapPersistentRecursiveGreen => {
                let mut retained =
                    self.persistent_recursive_green
                        .take()
                        .ok_or(ManifestError::Corrupt(
                            "persistent recursive Green phase lost its retained root",
                        ))?;
                let descriptor = retained.descriptor();
                let descriptor_bytes =
                    encode_persistent_m11_recursive_green_role_descriptor(descriptor)?;
                let event_count = retained.event_count();
                let canonical_event_bytes = descriptor.canonical_event_bytes();
                let metadata = RoleMetadata {
                    role: CandidateRole::Green,
                    schema: PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA,
                    record_count: event_count,
                    canonical_bytes: canonical_event_bytes,
                    digest: persistent_recursive_green_role_digest(
                        self.authority,
                        &descriptor_bytes,
                        event_count,
                        canonical_event_bytes,
                    ),
                };
                let retained_root = retained.take_owner();
                let mut payload = encode_role_root(self.authority, metadata);
                payload.extend_from_slice(&descriptor_bytes);
                let owner = match retained_root.as_ref() {
                    Some(root) => session.allocate(&payload, &[root.id()])?,
                    None => session.allocate(&payload, &[])?,
                };
                if let Some(root) = retained_root {
                    session.release(root)?;
                }
                self.roles[role_index(CandidateRole::Green)] = Some(RoleRoot {
                    authority: self.authority,
                    owner,
                    metadata,
                });
                self.phase = ManifestPhase::Records(Box::new(
                    self.begin_role_records(CandidateRole::Projection)?,
                ));
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::Records(pending) => {
                let mut pending = *pending;
                let schema = self.role_schema(pending.role);
                if let Some(bytes) = pending.records.pop_front() {
                    let ordinal = pending.record_count;
                    let (digest, canonical_bytes) =
                        record_digest(self.authority, pending.role, schema, ordinal, &bytes)?;
                    let payload =
                        encode_record(self.authority, pending.role, schema, ordinal, &bytes)?;
                    let owner = session.allocate(&payload, &[])?;
                    hash_record_digest(&mut pending.hasher, canonical_bytes, digest);
                    pending.record_count = pending
                        .record_count
                        .checked_add(1)
                        .ok_or(ManifestError::CapacityPreflight)?;
                    pending.canonical_bytes = pending
                        .canonical_bytes
                        .checked_add(canonical_bytes)
                        .ok_or(ManifestError::CapacityPreflight)?;
                    pending.owners.push_back(owner);
                    self.phase = ManifestPhase::Records(Box::new(pending));
                    return Ok(DriveOutcome::Progress);
                }
                if pending.role == CandidateRole::Projection
                    && self.persistent_inline_projection.is_some()
                {
                    self.phase = ManifestPhase::WrapPersistentProjection(Box::new(pending));
                    return Ok(DriveOutcome::Progress);
                }
                let metadata = RoleMetadata {
                    role: pending.role,
                    schema,
                    record_count: pending.record_count,
                    canonical_bytes: pending.canonical_bytes,
                    digest: finalize_role_digest(
                        pending.hasher,
                        pending.record_count,
                        pending.canonical_bytes,
                    ),
                };
                let children: Vec<ArenaId> =
                    pending.owners.iter().map(ArenaBuildOwner::id).collect();
                let payload = encode_role_root(self.authority, metadata);
                let root = session.allocate(&payload, &children)?;
                self.phase = ManifestPhase::ReleaseRecords(ReleaseRoleRecords {
                    role: pending.role,
                    metadata,
                    root,
                    owners: pending.owners,
                });
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::WrapPersistentProjection(pending) => {
                let mut pending = *pending;
                let mut retained =
                    self.persistent_inline_projection
                        .take()
                        .ok_or(ManifestError::Corrupt(
                            "persistent Projection phase lost its retained root",
                        ))?;
                let structural_digest = finalize_role_digest(
                    pending.hasher,
                    pending.record_count,
                    pending.canonical_bytes,
                );
                let record_count = pending
                    .record_count
                    .checked_add(retained.canonical_record_count())
                    .ok_or(ManifestError::CapacityPreflight)?;
                let canonical_bytes = pending
                    .canonical_bytes
                    .checked_add(retained.canonical_bytes())
                    .ok_or(ManifestError::CapacityPreflight)?;
                let descriptor = *retained.descriptor();
                let metadata = RoleMetadata {
                    role: CandidateRole::Projection,
                    schema: PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA,
                    record_count,
                    canonical_bytes,
                    digest: persistent_projection_role_digest(
                        &descriptor,
                        pending.record_count,
                        pending.canonical_bytes,
                        structural_digest,
                        record_count,
                        canonical_bytes,
                    ),
                };
                if let Some(root) = retained.take_value_owner() {
                    pending.owners.push_front(root);
                }
                if let Some(root) = retained.take_fact_owner() {
                    pending.owners.push_front(root);
                }
                let children: Vec<ArenaId> =
                    pending.owners.iter().map(ArenaBuildOwner::id).collect();
                let mut payload = encode_role_root(self.authority, metadata);
                payload.extend_from_slice(&descriptor);
                let root = session.allocate(&payload, &children)?;
                self.phase = ManifestPhase::ReleaseRecords(ReleaseRoleRecords {
                    role: CandidateRole::Projection,
                    metadata,
                    root,
                    owners: pending.owners,
                });
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::ReleaseRecords(mut records) => {
                let record = records
                    .owners
                    .pop_front()
                    .ok_or(ManifestError::Corrupt("candidate role lost record owner"))?;
                session.release(record)?;
                if !records.owners.is_empty() {
                    self.phase = ManifestPhase::ReleaseRecords(records);
                    return Ok(DriveOutcome::Progress);
                }
                self.roles[role_index(records.role)] = Some(RoleRoot {
                    authority: self.authority,
                    owner: records.root,
                    metadata: records.metadata,
                });
                self.phase = if let Some(next) = next_record_role(records.role) {
                    ManifestPhase::Records(Box::new(self.begin_role_records(next)?))
                } else {
                    ManifestPhase::AllocateManifest
                };
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::AllocateManifest => {
                let metadata = self.role_metadata()?;
                let payload = encode_manifest(self.authority, &metadata);
                let children: [ArenaId; CANDIDATE_ROLE_COUNT] = std::array::from_fn(|index| {
                    self.roles[index]
                        .as_ref()
                        .expect("role completeness was validated")
                        .owner
                        .id()
                });
                let manifest = session.allocate(&payload, &children)?;
                self.phase = ManifestPhase::ReleaseRoles(ManifestRelease {
                    manifest,
                    next_role: 0,
                });
                Ok(DriveOutcome::Progress)
            }
            ManifestPhase::ReleaseRoles(mut release) => {
                let role = self.roles[release.next_role]
                    .take()
                    .ok_or(ManifestError::Corrupt("manifest lost role owner"))?;
                session.release(role.owner)?;
                release.next_role += 1;
                if release.next_role == CANDIDATE_ROLE_COUNT {
                    self.phase = ManifestPhase::Complete;
                    Ok(DriveOutcome::Seal(release.manifest))
                } else {
                    self.phase = ManifestPhase::ReleaseRoles(release);
                    Ok(DriveOutcome::Progress)
                }
            }
            other => {
                self.phase = other;
                Err(ManifestError::Corrupt("invalid manifest build phase"))
            }
        }
    }

    fn role_metadata(&self) -> Result<[RoleMetadata; CANDIDATE_ROLE_COUNT], ManifestError> {
        let mut output = [RoleMetadata {
            role: CandidateRole::SourceFacts,
            schema: 0,
            record_count: 0,
            canonical_bytes: 0,
            digest: [0; STRONG_DIGEST_BYTES],
        }; CANDIDATE_ROLE_COUNT];
        for (index, expected) in CandidateRole::ORDERED.into_iter().enumerate() {
            let root = self.roles[index]
                .as_ref()
                .ok_or(ManifestError::Corrupt("candidate role is missing"))?;
            if root.authority != self.authority
                || root.metadata.role != expected
                || root.metadata.schema != self.role_schema(expected)
            {
                return Err(ManifestError::CrossAuthority);
            }
            output[index] = root.metadata;
        }
        Ok(output)
    }

    fn phase_after_references(&mut self) -> Result<ManifestPhase, ManifestError> {
        if self.persistent_source_facts.is_some() {
            Ok(ManifestPhase::WrapPersistentSourceFacts)
        } else {
            Ok(ManifestPhase::Records(Box::new(
                self.begin_role_records(CandidateRole::SourceFacts)?,
            )))
        }
    }

    fn poll_seal(
        &mut self,
        arena: &mut PageArena,
        fuel: usize,
    ) -> Result<ManifestPoll, ManifestError> {
        let phase = std::mem::replace(&mut self.phase, ManifestPhase::Complete);
        let ManifestPhase::Sealing(mut seal) = phase else {
            return Err(ManifestError::Corrupt("manifest lost seal capability"));
        };
        let receipt = match arena.poll_seal(&mut seal, fuel) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.phase = ManifestPhase::Sealing(seal);
                self.begin_abort(arena)?;
                return Err(error.into());
            }
        };
        if let Some(root) = receipt.root {
            debug_assert_eq!(receipt.remaining_non_root_owners, 0);
            self.phase = ManifestPhase::Complete;
            return Ok(ManifestPoll::Published {
                transitions: receipt.transitions,
                publication: PublishedManifest {
                    authority: self.authority,
                    root,
                    _not_sync: PhantomData,
                },
            });
        }
        self.phase = ManifestPhase::Sealing(seal);
        Ok(ManifestPoll::Pending {
            transitions: receipt.transitions,
        })
    }

    pub(crate) fn begin_abort(&mut self, arena: &mut PageArena) -> Result<(), ManifestError> {
        let phase = std::mem::replace(&mut self.phase, ManifestPhase::Aborting);
        match phase {
            ManifestPhase::Sealing(seal) => {
                if let Err(error) = arena.validate_seal(&seal) {
                    self.phase = ManifestPhase::Sealing(seal);
                    return Err(error.into());
                }
                arena
                    .abort_seal(seal)
                    .expect("prevalidated manifest seal must abort");
            }
            ManifestPhase::Aborting => return Ok(()),
            ManifestPhase::Complete if self.build.is_none() => {
                self.phase = ManifestPhase::Complete;
                return Err(ManifestError::Busy);
            }
            other => {
                let Some(build) = self.build.take() else {
                    self.phase = other;
                    return Err(ManifestError::Corrupt(
                        "manifest lost build capability during abort",
                    ));
                };
                if let Err(error) = arena.validate_suspended_build(&build) {
                    self.build = Some(build);
                    self.phase = other;
                    return Err(error.into());
                }
                arena
                    .abort_build(build)
                    .expect("prevalidated manifest build must abort");
            }
        }
        self.roles = std::array::from_fn(|_| None);
        self.phase = ManifestPhase::Aborting;
        Ok(())
    }
}

pub(crate) struct ManifestDescriptor {
    pub(crate) metadata: [RoleMetadata; CANDIDATE_ROLE_COUNT],
    pub(crate) children: [ArenaId; CANDIDATE_ROLE_COUNT],
}

/// Typed view of the canonical persistent SourceFacts role owned by one
/// already-decoded manifest.
///
/// The wrapper remains authority-bound while the optional sequence root and
/// descriptor bytes are authority-free canonical content. Keeping this
/// resolution in the manifest module prevents exact-base host programs from
/// duplicating the private wrapper/metadata codec.
pub(crate) struct PersistentSourceFactsManifestRole<'arena> {
    pub(crate) root: Option<ArenaId>,
    pub(crate) metadata: RoleMetadata,
    pub(crate) descriptor_bytes: &'arena [u8],
}

/// Typed view of one mixed persistent Projection role.
///
/// Structural records remain ordinary schema-v2 record children. Inline
/// logical pages live behind one measured root and therefore do not consume
/// wrapper fanout.
pub(crate) struct PersistentInlineProjectionManifestRole {
    pub(crate) fact_root: Option<ArenaId>,
    pub(crate) link_value_root: Option<ArenaId>,
    pub(crate) metadata: RoleMetadata,
    pub(crate) descriptor: PersistentM11InlineProjectionDescriptor,
    pub(crate) structural_record_count: u64,
}

/// Paired authority-bound Green and Projection wrappers over one canonical
/// persistent block root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentBlockManifestRoles {
    pub(crate) root: Option<ArenaId>,
    pub(crate) green: PersistentM11BlockRoleDescriptor,
    pub(crate) projection: PersistentM11BlockRoleDescriptor,
    pub(crate) claim: PersistentM11BlockRootClaim,
}

/// One authority-bound Green wrapper over a canonical recursive Green root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentRecursiveGreenManifestRole {
    pub(crate) root: Option<ArenaId>,
    pub(crate) descriptor: PersistentM11RecursiveGreenRoleDescriptor,
    pub(crate) claim: PersistentM11RecursiveGreenRootClaim,
}

pub(crate) fn manifest_digest256(
    authority: CandidateAuthority,
    descriptor: &ManifestDescriptor,
) -> [u8; STRONG_DIGEST_BYTES] {
    manifest_digest(authority, &descriptor.metadata)
}

#[cfg(test)]
pub(crate) struct CandidateManifestSummary {
    pub(crate) manifest_digest256: [u8; STRONG_DIGEST_BYTES],
}

fn decode_record_bytes(
    arena: &PageArena,
    id: ArenaId,
    _authority: CandidateAuthority,
    expected: RoleMetadata,
    ordinal: u64,
) -> Result<&[u8], ManifestError> {
    let payload = arena.payload(id)?;
    decode_canonical_node_header(payload, RECORD_TAG)?;
    if payload.len() < RECORD_BASE_BYTES
        || payload[CANONICAL_NODE_HEADER_BYTES + 1..CANONICAL_NODE_HEADER_BYTES + 4] != [0; 3]
        || read_u64(payload, CANONICAL_NODE_HEADER_BYTES + 8)? != ordinal
        || payload[CANONICAL_NODE_HEADER_BYTES + 20..CANONICAL_NODE_HEADER_BYTES + 24] != [0; 4]
        || arena.child_count(id)? != 0
    {
        return Err(ManifestError::Corrupt("invalid candidate role record"));
    }
    let role = CandidateRole::decode(payload[CANONICAL_NODE_HEADER_BYTES])?;
    let schema = read_u32(payload, CANONICAL_NODE_HEADER_BYTES + 4)?;
    let byte_len = usize::try_from(read_u32(payload, CANONICAL_NODE_HEADER_BYTES + 16)?)
        .map_err(|_| ManifestError::Corrupt("record length overflow"))?;
    if payload.len() != RECORD_BASE_BYTES + byte_len
        || role != expected.role
        || schema != expected.schema
    {
        return Err(ManifestError::Corrupt("candidate record metadata changed"));
    }
    let bytes = &payload[RECORD_BASE_BYTES..];
    Ok(bytes)
}

fn decode_role_root(
    arena: &PageArena,
    id: ArenaId,
    authority: CandidateAuthority,
    expected: RoleMetadata,
) -> Result<Option<ArenaId>, ManifestError> {
    let payload = arena.payload(id)?;
    decode_candidate_header(payload, ROLE_ROOT_TAG, authority)?;
    let persistent_source_facts = expected.role == CandidateRole::SourceFacts
        && expected.schema == PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA;
    let persistent_projection_route = (expected.role == CandidateRole::Projection)
        .then(|| persistent_projection_schema_route(expected.schema))
        .flatten();
    let persistent_inline_projection =
        persistent_projection_route == Some(PersistentProjectionSchemaRoute::Inline);
    let persistent_blocks = matches!(
        (expected.role, expected.schema),
        (CandidateRole::Green, PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA)
    ) || persistent_projection_route
        == Some(PersistentProjectionSchemaRoute::Block);
    let persistent_recursive_green = expected.role == CandidateRole::Green
        && expected.schema == PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA;
    let expected_payload_bytes = if persistent_source_facts {
        ROLE_ROOT_PAYLOAD_BYTES
            .checked_add(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .ok_or(ManifestError::Corrupt(
                "SourceFacts wrapper length overflow",
            ))?
    } else if persistent_inline_projection {
        ROLE_ROOT_PAYLOAD_BYTES
            .checked_add(PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES)
            .ok_or(ManifestError::Corrupt("Projection wrapper length overflow"))?
    } else if persistent_blocks {
        ROLE_ROOT_PAYLOAD_BYTES
            .checked_add(PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES)
            .ok_or(ManifestError::Corrupt("block wrapper length overflow"))?
    } else if persistent_recursive_green {
        ROLE_ROOT_PAYLOAD_BYTES
            .checked_add(PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES)
            .ok_or(ManifestError::Corrupt(
                "recursive Green wrapper length overflow",
            ))?
    } else {
        ROLE_ROOT_PAYLOAD_BYTES
    };
    if payload.len() != expected_payload_bytes
        || read_role_metadata(payload, CANDIDATE_HEADER_BYTES)? != expected
        || !schema_is_supported(expected.role, expected.schema)
    {
        return Err(ManifestError::Corrupt("candidate role root changed"));
    }
    let child_count = arena.child_count(id)?;
    if persistent_source_facts {
        let root = match (expected.record_count, child_count) {
            (0, 0) => None,
            (0, _) => {
                return Err(ManifestError::Corrupt(
                    "empty SourceFacts sequence owns a root",
                ))
            }
            (_, 1) => Some(arena.child_at(id, 0)?),
            _ => {
                return Err(ManifestError::Corrupt(
                    "SourceFacts sequence wrapper changed shape",
                ))
            }
        };
        validate_persistent_source_facts_role(
            arena,
            PersistentSourceFactsRoleValidation {
                root,
                descriptor_bytes: &payload[ROLE_ROOT_PAYLOAD_BYTES..],
                record_count: expected.record_count,
                canonical_bytes: expected.canonical_bytes,
                digest: expected.digest,
                source_bytes: authority.source_bytes,
                source_utf16: authority.source_utf16,
            },
        )
        .map_err(map_source_facts_manifest_error)?;
        return Ok(root);
    }
    if persistent_inline_projection {
        let source = SourceVersion::from_authenticated_parts(
            authority.source_revision,
            authority.source_root,
            usize::try_from(authority.source_bytes)
                .map_err(|_| ManifestError::Corrupt("source byte length overflow"))?,
            usize::try_from(authority.source_utf16)
                .map_err(|_| ManifestError::Corrupt("source UTF-16 length overflow"))?,
        );
        let parser_profile = ParserProfileId::new(u64::from(authority.syntax_profile))
            .ok_or(ManifestError::Corrupt("Projection parser profile is zero"))?;
        let descriptor = decode_persistent_inline_projection_descriptor(
            &payload[ROLE_ROOT_PAYLOAD_BYTES..],
            source,
            parser_profile,
        )?;
        let has_inline_root = descriptor.storage_page_count() != 0;
        let has_link_value_root = descriptor.link_value_storage_page_count() != 0;
        let persistent_record_count = descriptor
            .logical_page_count()
            .checked_add(descriptor.link_value_record_count())
            .ok_or(ManifestError::Corrupt(
                "Projection persistent record count overflow",
            ))?;
        let structural_count = expected
            .record_count
            .checked_sub(persistent_record_count)
            .ok_or(ManifestError::Corrupt(
                "Projection record count is below its persistent page count",
            ))?;
        let structural_count_usize = usize::try_from(structural_count)
            .map_err(|_| ManifestError::Corrupt("Projection record count overflow"))?;
        let persistent_root_count = usize::from(has_inline_root) + usize::from(has_link_value_root);
        if child_count
            != structural_count_usize
                .checked_add(persistent_root_count)
                .ok_or(ManifestError::Corrupt("Projection child count overflow"))?
        {
            return Err(ManifestError::Corrupt(
                "persistent Projection wrapper changed shape",
            ));
        }
        let inline_root = has_inline_root.then(|| arena.child_at(id, 0)).transpose()?;
        let link_value_root = has_link_value_root
            .then(|| arena.child_at(id, usize::from(has_inline_root)))
            .transpose()?;
        let mut structural_hasher =
            role_hasher(authority, CandidateRole::Projection, expected.schema);
        let mut structural_bytes = 0_u64;
        for ordinal in 0..structural_count_usize {
            let child_index = ordinal + persistent_root_count;
            let record = arena.child_at(id, child_index)?;
            let ordinal_u64 = u64::try_from(ordinal)
                .map_err(|_| ManifestError::Corrupt("Projection ordinal overflow"))?;
            let bytes = decode_record_bytes(arena, record, authority, expected, ordinal_u64)?;
            let (digest, byte_len) = record_digest(
                authority,
                CandidateRole::Projection,
                expected.schema,
                ordinal_u64,
                bytes,
            )?;
            hash_record_digest(&mut structural_hasher, byte_len, digest);
            structural_bytes =
                structural_bytes
                    .checked_add(byte_len)
                    .ok_or(ManifestError::Corrupt(
                        "Projection canonical length overflow",
                    ))?;
        }
        let structural_digest =
            finalize_role_digest(structural_hasher, structural_count, structural_bytes);
        let canonical_bytes = structural_bytes
            .checked_add(descriptor.canonical_bytes())
            .ok_or(ManifestError::Corrupt(
                "Projection canonical length overflow",
            ))?;
        if canonical_bytes != expected.canonical_bytes
            || persistent_projection_role_digest(
                payload[ROLE_ROOT_PAYLOAD_BYTES..]
                    .try_into()
                    .expect("checked persistent Projection descriptor"),
                structural_count,
                structural_bytes,
                structural_digest,
                expected.record_count,
                canonical_bytes,
            ) != expected.digest
        {
            return Err(ManifestError::Corrupt(
                "persistent Projection role digest changed",
            ));
        }
        validate_persistent_inline_projection_role(
            arena,
            inline_root,
            link_value_root,
            &payload[ROLE_ROOT_PAYLOAD_BYTES..],
            source,
            parser_profile,
        )?;
        return Ok(inline_root);
    }
    if persistent_blocks {
        let lane = match expected.role {
            CandidateRole::Green => M11BlockRoleLane::Green,
            CandidateRole::Projection => M11BlockRoleLane::Projection,
            _ => unreachable!("persistent block schemas are role-specific"),
        };
        let descriptor_bytes: &[u8; PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES] = payload
            [ROLE_ROOT_PAYLOAD_BYTES..]
            .try_into()
            .expect("checked block role descriptor length");
        let descriptor = decode_persistent_m11_block_role_descriptor(
            descriptor_bytes,
            lane,
            authority.source_bytes,
            authority.source_utf16,
        )?;
        let root = match (descriptor.record_count(), child_count) {
            (0, 0) => None,
            (0, _) => {
                return Err(ManifestError::Corrupt(
                    "empty block sequence wrapper owns a root",
                ))
            }
            (_, 1) => Some(arena.child_at(id, 0)?),
            _ => {
                return Err(ManifestError::Corrupt(
                    "persistent block wrapper changed shape",
                ))
            }
        };
        if expected.record_count != descriptor.record_count()
            || expected.canonical_bytes != descriptor.canonical_bytes()
            || expected.digest
                != persistent_block_role_digest(
                    authority,
                    expected.role,
                    expected.schema,
                    descriptor_bytes,
                    expected.record_count,
                    expected.canonical_bytes,
                )
        {
            return Err(ManifestError::Corrupt(
                "persistent block role metadata changed",
            ));
        }
        return Ok(root);
    }
    if persistent_recursive_green {
        let descriptor_bytes: &[u8; PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES] = payload
            [ROLE_ROOT_PAYLOAD_BYTES..]
            .try_into()
            .expect("checked recursive Green role descriptor length");
        let descriptor = decode_persistent_m11_recursive_green_role_descriptor(
            descriptor_bytes,
            authority.source_bytes,
            authority.source_utf16,
        )?;
        let root = match (descriptor.record_count(), child_count) {
            (0, 0) => None,
            (0, _) => {
                return Err(ManifestError::Corrupt(
                    "empty recursive Green wrapper owns a root",
                ))
            }
            (_, 1) => Some(arena.child_at(id, 0)?),
            _ => {
                return Err(ManifestError::Corrupt(
                    "persistent recursive Green wrapper changed shape",
                ))
            }
        };
        if expected.record_count != descriptor.record_count()
            || expected.canonical_bytes != descriptor.canonical_bytes()
            || expected.digest
                != persistent_recursive_green_role_digest(
                    authority,
                    descriptor_bytes,
                    expected.record_count,
                    expected.canonical_bytes,
                )
        {
            return Err(ManifestError::Corrupt(
                "persistent recursive Green role metadata changed",
            ));
        }
        return Ok(root);
    }
    if child_count == 0 {
        return Err(ManifestError::Corrupt("candidate role has no records"));
    }
    let child = arena.child_at(id, 0)?;
    if expected.role == CandidateRole::References {
        if child_count != 1 {
            return Err(ManifestError::Corrupt("reference role root changed"));
        }
        let view = ReferenceRootView::open(arena, authority, child)?;
        if view.count() != expected.record_count {
            return Err(ManifestError::Corrupt("reference role count changed"));
        }
    } else {
        let expected_count = usize::try_from(expected.record_count)
            .map_err(|_| ManifestError::Corrupt("candidate role count overflow"))?;
        if child_count != expected_count {
            return Err(ManifestError::Corrupt(
                "candidate role record count changed",
            ));
        }
        let mut hasher = role_hasher(authority, expected.role, expected.schema);
        let mut canonical_bytes = 0_u64;
        for ordinal in 0..child_count {
            let record = arena.child_at(id, ordinal)?;
            let record_ordinal = u64::try_from(ordinal)
                .map_err(|_| ManifestError::Corrupt("candidate record ordinal overflow"))?;
            let bytes = decode_record_bytes(arena, record, authority, expected, record_ordinal)?;
            let (digest, bytes_len) = record_digest(
                authority,
                expected.role,
                expected.schema,
                record_ordinal,
                bytes,
            )?;
            hash_record_digest(&mut hasher, bytes_len, digest);
            canonical_bytes =
                canonical_bytes
                    .checked_add(bytes_len)
                    .ok_or(ManifestError::Corrupt(
                        "candidate canonical length overflow",
                    ))?;
            if expected.role == CandidateRole::CleanEofOnly
                && (record_ordinal != 0 || bytes != checkpoint_record(authority).as_ref())
            {
                return Err(ManifestError::Corrupt(
                    "publication checkpoint is not clean EOF",
                ));
            }
        }
        if canonical_bytes != expected.canonical_bytes
            || finalize_role_digest(hasher, expected.record_count, canonical_bytes)
                != expected.digest
        {
            return Err(ManifestError::Corrupt("candidate role digest changed"));
        }
    }
    Ok(Some(child))
}

pub(crate) fn decode_manifest(
    arena: &PageArena,
    id: ArenaId,
    authority: CandidateAuthority,
) -> Result<ManifestDescriptor, ManifestError> {
    let descriptor = decode_manifest_descriptor(arena, id, authority)?;
    for index in 0..CANDIDATE_ROLE_COUNT {
        decode_role_root(
            arena,
            descriptor.children[index],
            authority,
            descriptor.metadata[index],
        )?;
    }
    let green_is_blocks = descriptor.metadata[role_index(CandidateRole::Green)].schema
        == PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA;
    let projection_is_blocks = descriptor.metadata[role_index(CandidateRole::Projection)].schema
        == PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA;
    if green_is_blocks != projection_is_blocks {
        return Err(ManifestError::Corrupt(
            "persistent block roles must be installed as a pair",
        ));
    }
    if green_is_blocks {
        let _ = persistent_block_manifest_roles(arena, &descriptor, authority)?;
    }
    if descriptor.metadata[role_index(CandidateRole::Green)].schema
        == PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
    {
        let _ = persistent_recursive_green_manifest_role(arena, &descriptor, authority)?;
    }
    Ok(descriptor)
}

/// Opens only the fixed manifest descriptor. This is safe for an immutable
/// root that already passed [`decode_manifest`] at atomic installation; it
/// avoids revalidating every paged role for each bounded record query.
pub(crate) fn decode_manifest_descriptor(
    arena: &PageArena,
    id: ArenaId,
    authority: CandidateAuthority,
) -> Result<ManifestDescriptor, ManifestError> {
    let payload = arena.payload(id)?;
    decode_candidate_header(payload, MANIFEST_TAG, authority)?;
    if payload.len() != MANIFEST_PAYLOAD_BYTES
        || payload[CANDIDATE_HEADER_BYTES] != CANDIDATE_ROLE_COUNT_U8
        || payload[CANDIDATE_HEADER_BYTES + 1..CANDIDATE_HEADER_BYTES + 8] != [0; 7]
        || arena.child_count(id)? != CANDIDATE_ROLE_COUNT
    {
        return Err(ManifestError::Corrupt("invalid candidate manifest shape"));
    }
    let mut metadata = [RoleMetadata {
        role: CandidateRole::SourceFacts,
        schema: 0,
        record_count: 0,
        canonical_bytes: 0,
        digest: [0; STRONG_DIGEST_BYTES],
    }; CANDIDATE_ROLE_COUNT];
    for (index, expected_role) in CandidateRole::ORDERED.into_iter().enumerate() {
        let offset = CANDIDATE_HEADER_BYTES + 8 + index * ROLE_METADATA_BYTES;
        let role = read_role_metadata(payload, offset)?;
        if role.role != expected_role || !schema_is_supported(expected_role, role.schema) {
            return Err(ManifestError::Corrupt(
                "candidate manifest role order changed",
            ));
        }
        metadata[index] = role;
    }
    let digest_offset = CANDIDATE_HEADER_BYTES + 8 + CANDIDATE_ROLE_COUNT * ROLE_METADATA_BYTES;
    let stored_digest: [u8; STRONG_DIGEST_BYTES] = payload[digest_offset..]
        .try_into()
        .map_err(|_| ManifestError::Corrupt("invalid manifest digest"))?;
    if stored_digest != manifest_digest(authority, &metadata) {
        return Err(ManifestError::Corrupt("candidate manifest digest changed"));
    }
    let first = arena.child_at(id, 0)?;
    let mut children = [first; CANDIDATE_ROLE_COUNT];
    for (index, child) in children.iter_mut().enumerate().skip(1) {
        *child = arena.child_at(id, index)?;
    }
    Ok(ManifestDescriptor { metadata, children })
}

/// Opens the installed persistent SourceFacts role through its typed manifest
/// descriptor.
///
/// This checks the authority-bound wrapper, exact metadata, canonical
/// descriptor, and direct measured-root relationship. It does not traverse
/// the unchanged sequence closure: that closure was admitted node-by-node
/// before the manifest became an installed capability.
pub(crate) fn persistent_source_facts_manifest_role<'arena>(
    arena: &'arena PageArena,
    descriptor: &ManifestDescriptor,
    authority: CandidateAuthority,
) -> Result<PersistentSourceFactsManifestRole<'arena>, ManifestError> {
    let index = role_index(CandidateRole::SourceFacts);
    let metadata = descriptor.metadata[index];
    if metadata.role != CandidateRole::SourceFacts
        || metadata.schema != PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA
    {
        return Err(ManifestError::InvalidRole);
    }
    let wrapper = descriptor.children[index];
    let root = decode_role_root(arena, wrapper, authority, metadata)?;
    let payload = arena.payload(wrapper)?;
    Ok(PersistentSourceFactsManifestRole {
        root,
        metadata,
        descriptor_bytes: &payload[ROLE_ROOT_PAYLOAD_BYTES..],
    })
}

/// Returns the one canonical References root after validating its typed role
/// wrapper. The root exists even for an empty reference sequence, which lets
/// revision-owned acceleration bind to stable root identity without copying
/// any canonical facts.
pub(crate) fn persistent_reference_manifest_root(
    arena: &PageArena,
    descriptor: &ManifestDescriptor,
    authority: CandidateAuthority,
) -> Result<ArenaId, ManifestError> {
    let index = role_index(CandidateRole::References);
    let metadata = descriptor.metadata[index];
    if metadata.role != CandidateRole::References || metadata.schema != REFERENCES_SCHEMA {
        return Err(ManifestError::InvalidRole);
    }
    let wrapper = descriptor.children[index];
    decode_role_root(arena, wrapper, authority, metadata)?.ok_or(ManifestError::Corrupt(
        "reference role lost its canonical root",
    ))
}

pub(crate) fn persistent_inline_projection_manifest_role(
    arena: &PageArena,
    descriptor: &ManifestDescriptor,
    authority: CandidateAuthority,
) -> Result<PersistentInlineProjectionManifestRole, ManifestError> {
    let index = role_index(CandidateRole::Projection);
    let metadata = descriptor.metadata[index];
    if metadata.role != CandidateRole::Projection
        || metadata.schema != PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA
    {
        return Err(ManifestError::InvalidRole);
    }
    let wrapper = descriptor.children[index];
    let fact_root = decode_role_root(arena, wrapper, authority, metadata)?;
    let payload = arena.payload(wrapper)?;
    let source = SourceVersion::from_authenticated_parts(
        authority.source_revision,
        authority.source_root,
        usize::try_from(authority.source_bytes)
            .map_err(|_| ManifestError::Corrupt("source byte length overflow"))?,
        usize::try_from(authority.source_utf16)
            .map_err(|_| ManifestError::Corrupt("source UTF-16 length overflow"))?,
    );
    let parser_profile = ParserProfileId::new(u64::from(authority.syntax_profile))
        .ok_or(ManifestError::Corrupt("Projection parser profile is zero"))?;
    let descriptor_bytes = &payload[ROLE_ROOT_PAYLOAD_BYTES..];
    let inline_descriptor =
        decode_persistent_inline_projection_descriptor(descriptor_bytes, source, parser_profile)?;
    let persistent_record_count = inline_descriptor
        .logical_page_count()
        .checked_add(inline_descriptor.link_value_record_count())
        .ok_or(ManifestError::Corrupt(
            "Projection persistent record count overflow",
        ))?;
    let structural_record_count = metadata
        .record_count
        .checked_sub(persistent_record_count)
        .ok_or(ManifestError::Corrupt(
            "Projection record count is below its persistent page count",
        ))?;
    let has_fact_root = inline_descriptor.storage_page_count() != 0;
    let link_value_root = (inline_descriptor.link_value_storage_page_count() != 0)
        .then(|| arena.child_at(wrapper, usize::from(has_fact_root)))
        .transpose()?;
    Ok(PersistentInlineProjectionManifestRole {
        fact_root,
        link_value_root,
        metadata,
        descriptor: inline_descriptor,
        structural_record_count,
    })
}

pub(crate) fn persistent_block_manifest_roles(
    arena: &PageArena,
    descriptor: &ManifestDescriptor,
    authority: CandidateAuthority,
) -> Result<PersistentBlockManifestRoles, ManifestError> {
    let green_index = role_index(CandidateRole::Green);
    let projection_index = role_index(CandidateRole::Projection);
    let green_metadata = descriptor.metadata[green_index];
    let projection_metadata = descriptor.metadata[projection_index];
    if green_metadata.schema != PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA
        || projection_metadata.schema != PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
    {
        return Err(ManifestError::InvalidRole);
    }
    let green_wrapper = descriptor.children[green_index];
    let projection_wrapper = descriptor.children[projection_index];
    let green_root = decode_role_root(arena, green_wrapper, authority, green_metadata)?;
    let projection_root =
        decode_role_root(arena, projection_wrapper, authority, projection_metadata)?;
    if green_root != projection_root {
        return Err(ManifestError::Corrupt(
            "paired block wrappers do not share one root",
        ));
    }
    let green_payload = arena.payload(green_wrapper)?;
    let projection_payload = arena.payload(projection_wrapper)?;
    let green = decode_persistent_m11_block_role_descriptor(
        &green_payload[ROLE_ROOT_PAYLOAD_BYTES..],
        M11BlockRoleLane::Green,
        authority.source_bytes,
        authority.source_utf16,
    )?;
    let projection = decode_persistent_m11_block_role_descriptor(
        &projection_payload[ROLE_ROOT_PAYLOAD_BYTES..],
        M11BlockRoleLane::Projection,
        authority.source_bytes,
        authority.source_utf16,
    )?;
    let claim = validate_persistent_m11_block_root(arena, green_root, green, projection)?;
    Ok(PersistentBlockManifestRoles {
        root: green_root,
        green,
        projection,
        claim,
    })
}

pub(crate) fn persistent_recursive_green_manifest_role(
    arena: &PageArena,
    descriptor: &ManifestDescriptor,
    authority: CandidateAuthority,
) -> Result<PersistentRecursiveGreenManifestRole, ManifestError> {
    let index = role_index(CandidateRole::Green);
    let metadata = descriptor.metadata[index];
    if metadata.role != CandidateRole::Green
        || metadata.schema != PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
    {
        return Err(ManifestError::InvalidRole);
    }
    let wrapper = descriptor.children[index];
    let root = decode_role_root(arena, wrapper, authority, metadata)?;
    let payload = arena.payload(wrapper)?;
    let recursive_descriptor = decode_persistent_m11_recursive_green_role_descriptor(
        &payload[ROLE_ROOT_PAYLOAD_BYTES..],
        authority.source_bytes,
        authority.source_utf16,
    )?;
    let claim = validate_persistent_m11_recursive_green_root(arena, root, recursive_descriptor)?;
    Ok(PersistentRecursiveGreenManifestRole {
        root,
        descriptor: recursive_descriptor,
        claim,
    })
}

pub(crate) fn manifest_persistent_inline_projection_record_at(
    arena: &PageArena,
    descriptor: &ManifestDescriptor,
    authority: CandidateAuthority,
    inline_ordinal: u64,
) -> Result<M11ParserPageRecord, ManifestError> {
    let role = persistent_inline_projection_manifest_role(arena, descriptor, authority)?;
    if inline_ordinal < role.descriptor.logical_page_count() {
        persistent_inline_projection_record_at(
            arena,
            role.fact_root,
            role.descriptor,
            inline_ordinal,
        )
        .map_err(Into::into)
    } else {
        let value_ordinal = inline_ordinal - role.descriptor.logical_page_count();
        if value_ordinal >= role.descriptor.link_value_record_count() {
            return Err(ManifestError::Corrupt(
                "Projection persistent record ordinal is out of range",
            ));
        }
        persistent_inline_link_value_record_at(
            arena,
            role.link_value_root,
            role.descriptor,
            value_ordinal,
        )
        .map_err(Into::into)
    }
}

pub(crate) fn manifest_role_record_bytes_at<'a>(
    arena: &'a PageArena,
    authority: CandidateAuthority,
    descriptor: &ManifestDescriptor,
    role: CandidateRole,
    ordinal: u64,
) -> Result<&'a [u8], ManifestError> {
    if role == CandidateRole::References {
        return Err(ManifestError::InvalidRole);
    }
    let index = role_index(role);
    let wrapper = descriptor.children[index];
    if role == CandidateRole::SourceFacts
        && descriptor.metadata[index].schema == PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA
    {
        let root = if descriptor.metadata[index].record_count == 0 {
            None
        } else {
            Some(arena.child_at(wrapper, 0)?)
        };
        return persistent_source_facts_leaf_record_at(arena, root, ordinal)
            .map_err(map_source_facts_manifest_error)?
            .ok_or(ManifestError::Corrupt(
                "SourceFacts record ordinal is out of range",
            ));
    }
    if role == CandidateRole::Projection
        && descriptor.metadata[index].schema == PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA
    {
        let persistent = persistent_inline_projection_manifest_role(arena, descriptor, authority)?;
        if ordinal >= persistent.structural_record_count {
            return Err(ManifestError::InvalidRole);
        }
        let ordinal_index = usize::try_from(ordinal)
            .map_err(|_| ManifestError::Corrupt("candidate query ordinal overflow"))?;
        let child_index = ordinal_index
            + usize::from(persistent.fact_root.is_some())
            + usize::from(persistent.link_value_root.is_some());
        let record = arena.child_at(wrapper, child_index)?;
        return decode_record_bytes(
            arena,
            record,
            authority,
            descriptor.metadata[index],
            ordinal,
        );
    }
    if matches!(
        (role, descriptor.metadata[index].schema),
        (CandidateRole::Green, PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA)
            | (CandidateRole::Green, PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA)
            | (
                CandidateRole::Projection,
                PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
            )
    ) {
        return Err(ManifestError::InvalidRole);
    }
    let ordinal_index = usize::try_from(ordinal)
        .map_err(|_| ManifestError::Corrupt("candidate query ordinal overflow"))?;
    let record = arena.child_at(wrapper, ordinal_index)?;
    decode_record_bytes(
        arena,
        record,
        authority,
        descriptor.metadata[index],
        ordinal,
    )
}

#[cfg(test)]
struct ManifestView<'a> {
    arena: &'a PageArena,
    authority: CandidateAuthority,
    descriptor: ManifestDescriptor,
    _not_sync: PhantomData<Cell<()>>,
}

#[cfg(test)]
impl<'a> ManifestView<'a> {
    fn open(
        arena: &'a PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
    ) -> Result<Self, ManifestError> {
        Ok(Self {
            arena,
            authority,
            descriptor: decode_manifest(arena, root, authority)?,
            _not_sync: PhantomData,
        })
    }

    fn role_metadata(&self, role: CandidateRole) -> RoleMetadata {
        self.descriptor.metadata[role_index(role)]
    }

    fn references(&self) -> Result<ReferenceRootView<'_>, ManifestError> {
        let wrapper = self.descriptor.children[role_index(CandidateRole::References)];
        let root = self.arena.child_at(wrapper, 0)?;
        ReferenceRootView::open(self.arena, self.authority, root).map_err(Into::into)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManifestDocumentState {
    Open,
    Closing,
    Closed,
}

#[cfg(test)]
pub(crate) struct CandidateManifestDocument {
    arena: PageArena,
    publication: Option<PublishedManifest>,
    state: ManifestDocumentState,
    _not_sync: PhantomData<Cell<()>>,
}

#[cfg(test)]
impl Drop for CandidateManifestDocument {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert_eq!(
                self.state,
                ManifestDocumentState::Closed,
                "manifest document must be explicitly fuel-drained"
            );
        }
    }
}

#[cfg(test)]
impl CandidateManifestDocument {
    pub(crate) fn new(
        arena: PageArena,
        publication: PublishedManifest,
    ) -> Result<Self, ManifestError> {
        let _ = ManifestView::open(&arena, publication.authority, publication.root_id())?;
        Ok(Self {
            arena,
            publication: Some(publication),
            state: ManifestDocumentState::Open,
            _not_sync: PhantomData,
        })
    }

    fn view(&self) -> Result<ManifestView<'_>, ManifestError> {
        if self.state != ManifestDocumentState::Open {
            return Err(ManifestError::Busy);
        }
        let publication = self
            .publication
            .as_ref()
            .ok_or(ManifestError::Corrupt("open manifest lost publication"))?;
        ManifestView::open(&self.arena, publication.authority, publication.root_id())
    }

    pub(crate) fn publication(&self) -> Result<&PublishedManifest, ManifestError> {
        if self.state != ManifestDocumentState::Open {
            return Err(ManifestError::Busy);
        }
        self.publication
            .as_ref()
            .ok_or(ManifestError::Corrupt("open manifest lost publication"))
    }

    pub(crate) fn summary(&self) -> Result<CandidateManifestSummary, ManifestError> {
        let publication = self.publication()?;
        let descriptor = decode_manifest_descriptor(
            &self.arena,
            publication.root_id(),
            publication.authority(),
        )?;
        Ok(CandidateManifestSummary {
            manifest_digest256: manifest_digest256(publication.authority(), &descriptor),
        })
    }

    pub(crate) fn begin_close(&mut self) -> Result<(), ManifestError> {
        if self.state != ManifestDocumentState::Open {
            return Ok(());
        }
        let publication = self
            .publication
            .take()
            .ok_or(ManifestError::Corrupt("open manifest lost publication"))?;
        if let Err(failure) = self.arena.release_committed_root(publication.root) {
            self.publication = Some(PublishedManifest {
                authority: publication.authority,
                root: failure.root,
                _not_sync: PhantomData,
            });
            return Err(failure.error.into());
        }
        self.state = ManifestDocumentState::Closing;
        Ok(())
    }

    pub(crate) fn poll_close(&mut self, fuel: usize) -> Result<bool, ManifestError> {
        if fuel == 0 {
            return Err(ManifestError::ZeroFuel);
        }
        if self.state == ManifestDocumentState::Open {
            return Err(ManifestError::Busy);
        }
        if self.state == ManifestDocumentState::Closed {
            return Ok(true);
        }
        let receipt = self.arena.poll_reclaim(fuel);
        if receipt.complete && self.arena.metrics().resident_nodes == 0 {
            self.state = ManifestDocumentState::Closed;
        }
        Ok(self.state == ManifestDocumentState::Closed)
    }
}

const _: () = {
    assert!(CANDIDATE_HEADER_BYTES + STRONG_DIGEST_BYTES < ARENA_PAGE_BYTES);
    assert!(AUTHORITY_RESERVED_OFFSET == 60);
    assert!(MANIFEST_PAYLOAD_BYTES <= ARENA_PAGE_BYTES);
    assert!(CANDIDATE_ROLE_COUNT == 5);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_root::{
        PersistentBytesView, ReferenceOccurrenceView, ReferenceRoleDigestValidator,
        ReferenceSourceRange, BLOB_CHUNK_BYTES, INLINE_FACT_VALUE_BYTES,
    };
    use crate::source_facts::{
        PersistentSourceFactsBuild, PersistentSourceFactsBuildOutput,
        PersistentSourceFactsBuildPoll,
    };
    use crate::{
        CertifiedSource, ParserProfileId, SourceFactsPoll, SourceFactsRootBuilder,
        SourceFactsRootLimits, SourceFactsScanProfile, SourceFactsScanner, SourceStore,
    };
    use std::collections::HashSet;

    fn authority(seed: u8, source_bytes: usize) -> CandidateAuthority {
        let source_text = "x".repeat(source_bytes);
        let source = SourceStore::new(&source_text).expect("source");
        CandidateAuthority::new(
            StrongIdentity::new([seed; 16]).expect("document identity"),
            StrongIdentity::new([seed.wrapping_add(64); 16]).expect("publication identity"),
            source.version(),
            CandidateGeneration::FIRST,
            1,
        )
        .expect("candidate authority")
    }

    #[test]
    fn inline_schema_five_and_block_schema_three_route_to_distinct_projection_owners() {
        assert_eq!(PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA, 5);
        assert_eq!(PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA, 3);
        assert_ne!(
            PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA,
            PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
        );
        assert_eq!(
            persistent_projection_schema_route(PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA),
            Some(PersistentProjectionSchemaRoute::Inline)
        );
        assert_eq!(
            persistent_projection_schema_route(PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA),
            Some(PersistentProjectionSchemaRoute::Block)
        );
        assert!(schema_is_supported(
            CandidateRole::Projection,
            PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA
        ));
        assert!(schema_is_supported(
            CandidateRole::Projection,
            PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
        ));
    }

    #[test]
    fn persistent_block_role_digest_binds_fresh_publication_authority() {
        let first = authority(7, 4);
        let second = CandidateAuthority {
            publication: StrongIdentity::new([0xe7; 16]).expect("fresh publication"),
            ..first
        };
        let descriptor = [0x5a; PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES];
        let first_digest = persistent_block_role_digest(
            first,
            CandidateRole::Green,
            PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
            &descriptor,
            3,
            99,
        );
        let second_digest = persistent_block_role_digest(
            second,
            CandidateRole::Green,
            PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
            &descriptor,
            3,
            99,
        );
        assert_ne!(first_digest, second_digest);
    }

    fn certified_source(store: &SourceStore, spacing: usize) -> CertifiedSource {
        let profile = SourceFactsScanProfile::new(spacing).expect("scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let lease = store.snapshot();
        let mut scanner =
            SourceFactsScanner::with_profile(lease.duplicate(), profile).expect("scanner");
        let mut builder = SourceFactsRootBuilder::new(
            lease,
            profile,
            parser_profile,
            SourceFactsRootLimits::default(),
        )
        .expect("builder");
        loop {
            match scanner.poll(64, 64).expect("source facts poll") {
                SourceFactsPoll::Pending(_) => {}
                SourceFactsPoll::Page { page, .. } => {
                    builder.push_page(page).expect("source facts page");
                }
                SourceFactsPoll::Complete { completion, .. } => {
                    return builder.certify(completion).expect("certification");
                }
                SourceFactsPoll::Cancelled => panic!("source facts scan cancelled"),
            }
        }
    }

    fn persistent_source_facts(
        arena: &mut PageArena,
        store: &SourceStore,
        spacing: usize,
    ) -> PersistentSourceFactsRoot {
        let mut build = PersistentSourceFactsBuild::new(certified_source(store, spacing));
        loop {
            match build.poll(arena).expect("persistent SourceFacts poll") {
                PersistentSourceFactsBuildPoll::Pending => {}
                PersistentSourceFactsBuildPoll::Complete(output) => {
                    let PersistentSourceFactsBuildOutput { certified, root } = *output;
                    drop(certified);
                    return root;
                }
            }
        }
    }

    fn persistent_authority(
        document_seed: u8,
        publication_seed: u8,
        store: &SourceStore,
        generation: u64,
    ) -> CandidateAuthority {
        CandidateAuthority::new(
            StrongIdentity::new([document_seed; 16]).expect("document identity"),
            StrongIdentity::new([publication_seed; 16]).expect("publication identity"),
            store.version(),
            CandidateGeneration::from_wire(generation).expect("generation"),
            1,
        )
        .expect("candidate authority")
    }

    fn persistent_assembler(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        authority: CandidateAuthority,
        source: SourceVersion,
        spacing: usize,
    ) -> CandidateManifestAssembler {
        CandidateManifestAssembler::new_with_persistent_source_facts(
            arena,
            persistent,
            authority,
            source,
            ParserProfileId::new(1).expect("parser profile"),
            SourceFactsScanProfile::new(spacing).expect("scan profile"),
            limits(arena.limits(), 1),
            CanonicalRoleInputs::persistent(&b"green"[..], &b"projection"[..]),
        )
        .expect("persistent candidate assembler")
    }

    fn range(start: u64, end: u64) -> ReferenceSourceRange {
        ReferenceSourceRange {
            bytes: start..end,
            utf16: start..end,
        }
    }

    fn fact(
        authority: CandidateAuthority,
        ordinal: u64,
        label: impl Into<Box<[u8]>>,
        destination: impl Into<Box<[u8]>>,
    ) -> AuthoritativeReferenceFact {
        let start = ordinal * 16;
        AuthoritativeReferenceFact {
            authority,
            source: range(start, start + 16),
            label_source: range(start + 1, start + 4),
            destination_source: range(start + 6, start + 12),
            title_source: None,
            normalized_label: label.into(),
            cooked_destination: destination.into(),
            cooked_title: None,
            _not_sync: PhantomData,
        }
    }

    fn limits(arena: ArenaLimits, max_occurrences: u64) -> ReferenceRootLimits {
        ReferenceRootLimits {
            arena,
            max_occurrences,
            max_cooked_bytes_per_fact: 16 * 1024 * 1024,
            facts_per_page: 64,
        }
    }

    fn assembler(
        arena: &mut PageArena,
        authority: CandidateAuthority,
        max_occurrences: u64,
    ) -> CandidateManifestAssembler {
        CandidateManifestAssembler::new(
            arena,
            authority,
            limits(arena.limits(), max_occurrences),
            CanonicalRoleInputs::single(
                b"source".as_slice(),
                b"green".as_slice(),
                b"projection".as_slice(),
            ),
        )
        .expect("manifest assembler")
    }

    fn drain_reference(arena: &mut PageArena, assembler: &mut CandidateManifestAssembler) {
        while !assembler.references_idle() {
            assert!(matches!(
                assembler.poll(arena, 17).expect("reference poll"),
                ManifestPoll::Pending { .. }
            ));
        }
        let owner_count = arena
            .suspended_build_owner_count(assembler.build.as_ref().expect("build"))
            .expect("owner count");
        assert!(owner_count <= MAX_REFERENCE_WORKING_OWNERS);
    }

    fn publish(
        mut arena: PageArena,
        mut assembler: CandidateManifestAssembler,
    ) -> CandidateManifestDocument {
        assembler
            .finish_references(&arena)
            .expect("finish references");
        loop {
            match assembler.poll(&mut arena, 17).expect("manifest poll") {
                ManifestPoll::Pending { .. } => {}
                ManifestPoll::Published { publication, .. } => {
                    drop(assembler);
                    return CandidateManifestDocument::new(arena, publication)
                        .expect("manifest document");
                }
                ManifestPoll::Aborting => panic!("manifest unexpectedly aborted"),
            }
        }
    }

    fn publish_ready_in_arena(
        arena: &mut PageArena,
        mut assembler: CandidateManifestAssembler,
    ) -> PublishedManifest {
        loop {
            match assembler.poll(arena, 17).expect("manifest poll") {
                ManifestPoll::Pending { .. } => {}
                ManifestPoll::Published { publication, .. } => {
                    drop(assembler);
                    return publication;
                }
                ManifestPoll::Aborting => panic!("manifest unexpectedly aborted"),
            }
        }
    }

    fn publish_reference_manifest_in_arena(
        arena: &mut PageArena,
        authority: CandidateAuthority,
        occurrences: u64,
    ) -> PublishedManifest {
        let mut assembler = assembler(arena, authority, occurrences);
        for ordinal in 0..occurrences {
            assembler
                .offer_reference(
                    arena,
                    fact(
                        authority,
                        ordinal,
                        format!("label-{ordinal}").into_bytes().into_boxed_slice(),
                        format!("destination-{ordinal}")
                            .into_bytes()
                            .into_boxed_slice(),
                    ),
                )
                .expect("reference fact");
            drain_reference(arena, &mut assembler);
        }
        assembler
            .finish_references(arena)
            .expect("finish references");
        publish_ready_in_arena(arena, assembler)
    }

    fn successor_authority(base: CandidateAuthority, publication_seed: u8) -> CandidateAuthority {
        let source_bytes = usize::try_from(base.source_bytes).expect("source bytes");
        let source_utf16 = usize::try_from(base.source_utf16).expect("source utf16");
        let source = SourceVersion::from_authenticated_parts(
            base.source_revision
                .checked_next()
                .expect("source revision"),
            SourceRootId::allocate().expect("source root"),
            source_bytes,
            source_utf16,
        );
        CandidateAuthority::new(
            base.document,
            StrongIdentity::new([publication_seed; 16]).expect("publication identity"),
            source,
            base.parse_generation
                .checked_next()
                .expect("candidate generation"),
            base.syntax_profile,
        )
        .expect("successor authority")
    }

    fn reachable_ids(arena: &PageArena, root: ArenaId) -> HashSet<ArenaId> {
        let mut reachable = HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            for index in 0..arena.child_count(id).expect("child count") {
                pending.push(arena.child_at(id, index).expect("child"));
            }
        }
        reachable
    }

    fn release_publication(arena: &mut PageArena, publication: PublishedManifest) {
        assert!(
            arena
                .release_committed_root(publication.into_root())
                .is_ok(),
            "release publication"
        );
        while !arena.poll_reclaim(31).complete {}
    }

    fn close(document: &mut CandidateManifestDocument) {
        document.begin_close().expect("begin close");
        assert!(matches!(
            document.poll_close(0),
            Err(ManifestError::ZeroFuel)
        ));
        while !document.poll_close(31).expect("close poll") {}
        assert_eq!(document.arena.metrics().resident_nodes, 0);
    }

    fn validate_reference_digest(document: &CandidateManifestDocument) {
        let (root, metadata) = {
            let view = document.view().expect("manifest view");
            let wrapper = view.descriptor.children[role_index(CandidateRole::References)];
            (
                document.arena.child_at(wrapper, 0).expect("reference root"),
                view.role_metadata(CandidateRole::References),
            )
        };
        let mut validator = ReferenceRoleDigestValidator::new(
            &document.arena,
            document.publication().expect("publication").authority(),
            root,
            metadata,
        )
        .expect("reference validator");
        loop {
            let receipt = validator
                .poll(&document.arena, 1)
                .expect("reference digest poll");
            assert_eq!(receipt.transitions, 1);
            if receipt.complete {
                break;
            }
        }
    }

    #[test]
    fn persistent_source_facts_keep_exact_root_identity_under_fresh_wrappers() {
        let store = SourceStore::new(&"x".repeat(4_000)).expect("source");
        let spacing = 2;
        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 16_384,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 128,
        })
        .expect("arena");
        let persistent = persistent_source_facts(&mut arena, &store, spacing);
        let canonical_root = persistent.tree_root_id_for_test().expect("persistent root");

        let first_authority = persistent_authority(41, 81, &store, 1);
        let mut first_assembler = persistent_assembler(
            &mut arena,
            &persistent,
            first_authority,
            store.version(),
            spacing,
        );
        first_assembler
            .finish_references(&arena)
            .expect("finish first references");
        let first = publish_ready_in_arena(&mut arena, first_assembler);
        let first_descriptor =
            decode_manifest(&arena, first.root_id(), first.authority()).expect("first manifest");
        let first_wrapper = first_descriptor.children[role_index(CandidateRole::SourceFacts)];
        let first_role =
            persistent_source_facts_manifest_role(&arena, &first_descriptor, first.authority())
                .expect("typed persistent SourceFacts role");
        assert_eq!(first_role.root, Some(canonical_root));
        assert_eq!(
            first_role.metadata,
            first_descriptor.metadata[role_index(CandidateRole::SourceFacts)]
        );
        assert_eq!(
            first_role.descriptor_bytes.len(),
            PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES
        );
        assert_eq!(
            arena.child_at(first_wrapper, 0).expect("first direct root"),
            canonical_root
        );

        let second_authority = persistent_authority(41, 82, &store, 2);
        let mut second_assembler = persistent_assembler(
            &mut arena,
            &persistent,
            second_authority,
            store.version(),
            spacing,
        );
        second_assembler
            .finish_references(&arena)
            .expect("finish second references");
        let second = publish_ready_in_arena(&mut arena, second_assembler);
        let second_descriptor =
            decode_manifest(&arena, second.root_id(), second.authority()).expect("second manifest");
        let second_wrapper = second_descriptor.children[role_index(CandidateRole::SourceFacts)];
        assert_ne!(first_wrapper, second_wrapper);
        assert_eq!(
            arena
                .child_at(second_wrapper, 0)
                .expect("second direct root"),
            canonical_root
        );

        let second_closure = reachable_ids(&arena, second.root_id());
        assert!(!second_closure.contains(&first.root_id()));
        assert!(!second_closure.contains(&first_wrapper));
        assert!(second_closure.contains(&canonical_root));

        release_publication(&mut arena, first);
        assert_eq!(
            arena
                .child_at(second_wrapper, 0)
                .expect("second wrapper survives first release"),
            canonical_root
        );
        release_publication(&mut arena, second);
        assert!(
            arena.payload(canonical_root).is_ok(),
            "runtime-owned persistent root survives every publication"
        );
        assert!(persistent.release(&mut arena).is_ok());
        while !arena.poll_reclaim(31).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn persistent_publication_setup_is_page_count_independent() {
        let small_store = SourceStore::new(&"x".repeat(2_000)).expect("small source");
        let large_store = SourceStore::new(&"x".repeat(20_000)).expect("large source");
        let spacing = 2;
        let mut arena = PageArena::new(ArenaLimits {
            max_slots: 65_536,
            max_live_payload_bytes: 128 * 1024 * 1024,
            max_children_per_node: 128,
        })
        .expect("arena");
        let small = persistent_source_facts(&mut arena, &small_store, spacing);
        let large = persistent_source_facts(&mut arena, &large_store, spacing);
        assert!(large.page_count() > small.page_count() * 8);

        let setup = |arena: &mut PageArena,
                     persistent: &PersistentSourceFactsRoot,
                     store: &SourceStore,
                     publication_seed: u8| {
            let authority = persistent_authority(42, publication_seed, store, 1);
            let mut assembler =
                persistent_assembler(arena, persistent, authority, store.version(), spacing);
            let receipt = assembler
                .persistent_source_facts_setup()
                .expect("persistent setup receipt");
            assembler.begin_abort(arena).expect("abort setup probe");
            drop(assembler);
            while !arena.poll_reclaim(31).complete {}
            receipt
        };
        let small_setup = setup(&mut arena, &small, &small_store, 83);
        let large_setup = setup(&mut arena, &large, &large_store, 84);
        assert_eq!(small_setup, large_setup);
        assert_eq!(large_setup.node_headers_decoded, 3);
        assert_eq!(large_setup.summary_combinations, 1);
        assert_eq!(large_setup.spec.spec_items_hashed, 0);

        assert!(small.release(&mut arena).is_ok());
        assert!(large.release(&mut arena).is_ok());
        while !arena.poll_reclaim(31).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn inline_reference_values_round_trip_query_and_canonical_digest() {
        let authority = authority(20, 64);
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let mut assembler = assembler(&mut arena, authority, 2);
        let mut inline = fact(authority, 0, &b"label"[..], &b"destination"[..]);
        inline.title_source = Some(range(13, 15));
        inline.cooked_title = Some(Box::from(&b"title"[..]));
        assembler
            .offer_reference(&arena, inline)
            .expect("inline fact");
        drain_reference(&mut arena, &mut assembler);
        let mut document = publish(arena, assembler);

        assert_eq!(document.arena.metrics().resident_nodes, 14);
        let view = document.view().unwrap();
        let references = view.references().unwrap();
        let occurrence = references.occurrence(0).unwrap().unwrap();
        assert!(occurrence.normalized_label.equals(b"label").unwrap());
        assert!(occurrence
            .cooked_destination
            .equals(b"destination")
            .unwrap());
        assert!(occurrence.cooked_title.unwrap().equals(b"title").unwrap());
        let mut middle = [0_u8; 5];
        assert_eq!(
            occurrence.cooked_destination.read(3, &mut middle).unwrap(),
            middle.len()
        );
        assert_eq!(&middle, b"tinat");
        validate_reference_digest(&document);
        close(&mut document);
    }

    #[test]
    fn streamed_inline_fact_matches_atomic_digest_and_storage_shape() {
        let authority = authority(21, 64);
        let mut atomic_arena = PageArena::new(ArenaLimits::default()).expect("atomic arena");
        let mut atomic = assembler(&mut atomic_arena, authority, 1);
        let mut atomic_fact = fact(authority, 0, &b"label"[..], &b"dest"[..]);
        atomic_fact.title_source = Some(range(13, 15));
        atomic_fact.cooked_title = Some(Box::from(&b"title"[..]));
        atomic
            .offer_reference(&atomic_arena, atomic_fact)
            .expect("atomic fact");
        drain_reference(&mut atomic_arena, &mut atomic);
        let mut atomic_document = publish(atomic_arena, atomic);

        let mut stream_arena = PageArena::new(ArenaLimits::default()).expect("stream arena");
        let mut stream = assembler(&mut stream_arena, authority, 1);
        stream
            .begin_reference_stream(
                &stream_arena,
                AuthoritativeReferenceFactStart {
                    authority,
                    source: range(0, 16),
                    label_source: range(1, 4),
                    destination_source: range(6, 12),
                    title_source: Some(range(13, 15)),
                    normalized_label: Box::from(&b"label"[..]),
                    destination_len: 4,
                    title_len: Some(5),
                    _not_sync: PhantomData,
                },
            )
            .expect("stream start");
        assert_eq!(
            stream
                .reference_stream_capacity(StreamedReferenceValueKind::Destination)
                .unwrap(),
            0
        );
        assert!(matches!(
            stream.poll(&mut stream_arena, 1).unwrap(),
            ManifestPoll::Pending { .. }
        ));
        assert_eq!(
            stream
                .offer_reference_stream_bytes(StreamedReferenceValueKind::Destination, b"dest")
                .unwrap(),
            4
        );
        assert!(matches!(
            stream.poll(&mut stream_arena, 1).unwrap(),
            ManifestPoll::Pending { .. }
        ));
        assert_eq!(
            stream
                .offer_reference_stream_bytes(StreamedReferenceValueKind::Title, b"title")
                .unwrap(),
            5
        );
        assert!(stream.reference_stream_retained_bytes() <= INLINE_FACT_VALUE_BYTES);
        drain_reference(&mut stream_arena, &mut stream);
        let mut stream_document = publish(stream_arena, stream);

        let atomic_metadata = atomic_document
            .view()
            .unwrap()
            .role_metadata(CandidateRole::References);
        let stream_metadata = stream_document
            .view()
            .unwrap()
            .role_metadata(CandidateRole::References);
        assert_eq!(atomic_metadata, stream_metadata);
        assert_eq!(atomic_document.arena.metrics().resident_nodes, 14);
        assert_eq!(stream_document.arena.metrics().resident_nodes, 14);
        validate_reference_digest(&stream_document);
        close(&mut stream_document);
        close(&mut atomic_document);
    }

    #[test]
    fn five_role_manifest_preserves_reference_truth_and_first_winner() {
        let authority = authority(1, 64);
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let mut assembler = assembler(&mut arena, authority, 10);
        assembler
            .offer_reference(&arena, fact(authority, 0, &b"x"[..], &b"one"[..]))
            .expect("first fact");
        drain_reference(&mut arena, &mut assembler);
        let mut duplicate = fact(authority, 1, &b"x"[..], &b"two"[..]);
        duplicate.title_source = Some(range(28, 31));
        duplicate.cooked_title = Some(Box::from(&b"title"[..]));
        assembler
            .offer_reference(&arena, duplicate)
            .expect("duplicate fact");
        drain_reference(&mut arena, &mut assembler);

        let mut document = publish(arena, assembler);
        {
            let view = document.view().expect("manifest view");
            assert_eq!(view.descriptor.children.len(), CANDIDATE_ROLE_COUNT);
            for role in CandidateRole::ORDERED {
                let metadata = view.role_metadata(role);
                assert_eq!(metadata.role, role);
                assert_eq!(metadata.schema, schema_for(role));
                assert_ne!(metadata.digest, [0; STRONG_DIGEST_BYTES]);
            }
            let references = view.references().expect("references");
            assert_eq!(references.count(), 2);
            let second = references.occurrence(1).unwrap().unwrap();
            assert_eq!(second.ordinal, 1);
            assert_eq!(second.source, range(16, 32));
            assert!(second.normalized_label.equals(b"x").unwrap());
            assert!(second.cooked_destination.equals(b"two").unwrap());
            assert!(second.cooked_title.unwrap().equals(b"title").unwrap());
            let winner = references.winner("x").unwrap().unwrap();
            assert_eq!(winner.ordinal, 0);
            assert!(winner.cooked_destination.equals(b"one").unwrap());
        }
        close(&mut document);
    }

    #[test]
    fn canonical_role_content_is_authority_free_beneath_fresh_wrappers() {
        fn one_reference_document(authority: CandidateAuthority) -> CandidateManifestDocument {
            let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
            let mut assembler = assembler(&mut arena, authority, 1);
            assembler
                .offer_reference(
                    &arena,
                    fact(authority, 0, &b"label"[..], &b"destination"[..]),
                )
                .expect("reference fact");
            drain_reference(&mut arena, &mut assembler);
            publish(arena, assembler)
        }

        let mut first = one_reference_document(authority(21, 64));
        let mut second = one_reference_document(authority(22, 64));
        let first_manifest = first.publication().unwrap().root_id();
        let second_manifest = second.publication().unwrap().root_id();
        let first_view = first.view().expect("first view");
        let second_view = second.view().expect("second view");

        assert_eq!(
            first_view.descriptor.metadata,
            second_view.descriptor.metadata
        );
        assert_ne!(
            first.summary().unwrap().manifest_digest256,
            second.summary().unwrap().manifest_digest256,
            "fresh manifests must remain target-authority bound"
        );
        assert_ne!(
            first.arena.payload(first_manifest).unwrap(),
            second.arena.payload(second_manifest).unwrap(),
            "manifest wrappers must carry their target authority"
        );

        for role in CandidateRole::ORDERED {
            let index = role_index(role);
            let first_wrapper = first_view.descriptor.children[index];
            let second_wrapper = second_view.descriptor.children[index];
            assert_ne!(
                first.arena.payload(first_wrapper).unwrap(),
                second.arena.payload(second_wrapper).unwrap(),
                "{role:?} wrapper must carry fresh target authority"
            );

            let first_canonical = first.arena.child_at(first_wrapper, 0).unwrap();
            let second_canonical = second.arena.child_at(second_wrapper, 0).unwrap();
            assert_eq!(
                first.arena.payload(first_canonical).unwrap(),
                second.arena.payload(second_canonical).unwrap(),
                "{role:?} canonical root must exclude target authority"
            );

            if role == CandidateRole::References {
                let first_page = first.arena.child_at(first_canonical, 0).unwrap();
                let second_page = second.arena.child_at(second_canonical, 0).unwrap();
                assert_eq!(
                    first.arena.payload(first_page).unwrap(),
                    second.arena.payload(second_page).unwrap()
                );
                let first_fact = first.arena.child_at(first_page, 0).unwrap();
                let second_fact = second.arena.child_at(second_page, 0).unwrap();
                assert_eq!(
                    first.arena.payload(first_fact).unwrap(),
                    second.arena.payload(second_fact).unwrap()
                );
            }
        }

        close(&mut second);
        close(&mut first);
    }

    #[test]
    fn reused_references_keep_canonical_identity_under_fresh_target_wrappers() {
        const OCCURRENCES: u64 = 8;
        let arena_limits = ArenaLimits::default();
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let base_authority = authority(30, OCCURRENCES as usize * 16);
        let base = publish_reference_manifest_in_arena(&mut arena, base_authority, OCCURRENCES);
        while !arena.poll_reclaim(31).complete {}
        let base_descriptor =
            decode_manifest(&arena, base.root_id(), base_authority).expect("base manifest");
        let references_index = role_index(CandidateRole::References);
        let base_wrapper = base_descriptor.children[references_index];
        let base_canonical = arena
            .child_at(base_wrapper, 0)
            .expect("base canonical references");

        let target_authority = successor_authority(base_authority, 201);
        let reused = CandidateManifestAssembler::new_reusing_references(
            &mut arena,
            target_authority,
            limits(arena_limits, OCCURRENCES),
            CanonicalRoleInputs::single(
                b"source".as_slice(),
                b"green".as_slice(),
                b"projection".as_slice(),
            ),
            &base,
        )
        .expect("reused candidate");
        let target = publish_ready_in_arena(&mut arena, reused);
        let target_descriptor =
            decode_manifest(&arena, target.root_id(), target_authority).expect("target manifest");
        let target_wrapper = target_descriptor.children[references_index];
        let target_canonical = arena
            .child_at(target_wrapper, 0)
            .expect("target canonical references");

        assert_eq!(target_canonical, base_canonical);
        assert_ne!(target_wrapper, base_wrapper);
        assert_ne!(target.root_id(), base.root_id());
        let target_closure = reachable_ids(&arena, target.root_id());
        assert!(!target_closure.contains(&base.root_id()));
        assert!(!target_closure.contains(&base_wrapper));

        let mut clean_arena = PageArena::new(arena_limits).expect("clean arena");
        let clean =
            publish_reference_manifest_in_arena(&mut clean_arena, target_authority, OCCURRENCES);
        let clean_descriptor = decode_manifest(&clean_arena, clean.root_id(), target_authority)
            .expect("clean target manifest");
        assert_eq!(target_descriptor.metadata, clean_descriptor.metadata);
        assert_eq!(
            manifest_digest256(target_authority, &target_descriptor),
            manifest_digest256(target_authority, &clean_descriptor)
        );

        release_publication(&mut clean_arena, clean);
        assert_eq!(clean_arena.metrics().resident_nodes, 0);
        release_publication(&mut arena, target);
        assert!(arena.metrics().resident_nodes > 0);
        release_publication(&mut arena, base);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn combined_candidate_keeps_both_exact_canonical_roots_under_fresh_wrappers() {
        const OCCURRENCES: u64 = 8;
        let spacing = 2;
        let arena_limits = ArenaLimits {
            max_slots: 16_384,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 128,
        };
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let mut store = SourceStore::new(&"x".repeat(4_000)).expect("source");
        let base_source = store.version();
        let document = StrongIdentity::new([45; 16]).expect("document");
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([85; 16]).expect("base publication"),
            base_source,
            CandidateGeneration::FIRST,
            1,
        )
        .expect("base authority");
        let base = publish_reference_manifest_in_arena(&mut arena, base_authority, OCCURRENCES);
        while !arena.poll_reclaim(31).complete {}

        let prepared = store
            .prepare_edit(
                base_source,
                base_source.byte_len()..base_source.byte_len(),
                "\n",
            )
            .expect("append edit");
        drop(
            store
                .commit_prepared_edit(prepared)
                .expect("commit append edit"),
        );
        let target_source = store.version();
        let persistent = persistent_source_facts(&mut arena, &store, spacing);
        let persistent_root = persistent
            .tree_root_id_for_test()
            .expect("persistent target root");
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([86; 16]).expect("target publication"),
            target_source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");

        let combined =
            CandidateManifestAssembler::new_with_persistent_source_facts_reusing_references(
                &mut arena,
                &persistent,
                target_authority,
                target_source,
                ParserProfileId::new(1).expect("parser profile"),
                SourceFactsScanProfile::new(spacing).expect("scan profile"),
                limits(arena_limits, OCCURRENCES),
                CanonicalRoleInputs::persistent(&b"green"[..], &b"projection"[..]),
                &base,
            )
            .expect("combined candidate");
        let target = publish_ready_in_arena(&mut arena, combined);
        let base_descriptor =
            decode_manifest(&arena, base.root_id(), base_authority).expect("base manifest");
        let target_descriptor =
            decode_manifest(&arena, target.root_id(), target_authority).expect("target manifest");
        let source_facts_index = role_index(CandidateRole::SourceFacts);
        let references_index = role_index(CandidateRole::References);
        let base_reference_root = arena
            .child_at(base_descriptor.children[references_index], 0)
            .expect("base canonical references");

        assert_eq!(
            arena
                .child_at(target_descriptor.children[source_facts_index], 0)
                .expect("target canonical SourceFacts"),
            persistent_root
        );
        assert_eq!(
            arena
                .child_at(target_descriptor.children[references_index], 0)
                .expect("target canonical references"),
            base_reference_root
        );
        assert_ne!(target.root_id(), base.root_id());
        let target_closure = reachable_ids(&arena, target.root_id());
        assert!(!target_closure.contains(&base.root_id()));
        for base_wrapper in base_descriptor.children {
            assert!(
                !target_closure.contains(&base_wrapper),
                "target inherited a base role wrapper"
            );
        }
        for role in CandidateRole::ORDERED {
            let index = role_index(role);
            assert_ne!(
                target_descriptor.children[index], base_descriptor.children[index],
                "{role:?} wrapper was not fresh"
            );
        }

        release_publication(&mut arena, target);
        assert!(arena.payload(persistent_root).is_ok());
        assert!(decode_manifest(&arena, base.root_id(), base_authority).is_ok());
        release_publication(&mut arena, base);
        assert!(arena.payload(persistent_root).is_ok());
        assert!(persistent.release(&mut arena).is_ok());
        while !arena.poll_reclaim(31).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn aborting_combined_candidate_preserves_both_retained_authorities() {
        const OCCURRENCES: u64 = 4;
        let spacing = 2;
        let arena_limits = ArenaLimits {
            max_slots: 16_384,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 128,
        };
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let store = SourceStore::new(&"x".repeat(4_000)).expect("source");
        let source = store.version();
        let document = StrongIdentity::new([46; 16]).expect("document");
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([87; 16]).expect("base publication"),
            source,
            CandidateGeneration::FIRST,
            1,
        )
        .expect("base authority");
        let base = publish_reference_manifest_in_arena(&mut arena, base_authority, OCCURRENCES);
        let persistent = persistent_source_facts(&mut arena, &store, spacing);
        while !arena.poll_reclaim(31).complete {}
        let persistent_authority = persistent.authority_snapshot();
        let persistent_root = persistent.tree_root_id_for_test().expect("persistent root");
        let base_descriptor =
            decode_manifest(&arena, base.root_id(), base_authority).expect("base manifest");
        let references_index = role_index(CandidateRole::References);
        let base_reference_root = arena
            .child_at(base_descriptor.children[references_index], 0)
            .expect("base canonical references");
        let baseline = arena.metrics();
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([88; 16]).expect("target publication"),
            source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");

        let mut combined =
            CandidateManifestAssembler::new_with_persistent_source_facts_reusing_references(
                &mut arena,
                &persistent,
                target_authority,
                source,
                ParserProfileId::new(1).expect("parser profile"),
                SourceFactsScanProfile::new(spacing).expect("scan profile"),
                limits(arena_limits, OCCURRENCES),
                CanonicalRoleInputs::persistent(&b"green"[..], &b"projection"[..]),
                &base,
            )
            .expect("combined candidate");
        assert!(matches!(
            combined.poll(&mut arena, 1).expect("wrap references"),
            ManifestPoll::Pending { transitions: 1 }
        ));
        combined.begin_abort(&mut arena).expect("abort target");
        drop(combined);
        while !arena.poll_reclaim(1).complete {}

        let after_abort = arena.metrics();
        assert_eq!(after_abort.resident_nodes, baseline.resident_nodes);
        assert_eq!(after_abort.live_payload_bytes, baseline.live_payload_bytes);
        assert_eq!(after_abort.pending_reclaims, 0);
        assert_eq!(after_abort.live_builds, 0);
        assert_eq!(after_abort.pending_build_aborts, 0);
        assert!(persistent.authority_snapshot() == persistent_authority);
        assert!(arena.payload(persistent_root).is_ok());
        let surviving =
            decode_manifest(&arena, base.root_id(), base_authority).expect("surviving base");
        assert_eq!(
            arena
                .child_at(surviving.children[references_index], 0)
                .expect("surviving references"),
            base_reference_root
        );

        release_publication(&mut arena, base);
        assert!(persistent.release(&mut arena).is_ok());
        while !arena.poll_reclaim(31).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    fn reused_reference_target_receipt(
        occurrences: u64,
        authority_seed: u8,
        publication_seed: u8,
    ) -> (usize, usize, usize) {
        let arena_limits = ArenaLimits::default();
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let base_authority = authority(authority_seed, occurrences as usize * 16);
        let base = publish_reference_manifest_in_arena(&mut arena, base_authority, occurrences);
        while !arena.poll_reclaim(31).complete {}
        let baseline = arena.metrics();

        let target_authority = successor_authority(base_authority, publication_seed);
        let mut reused = CandidateManifestAssembler::new_reusing_references(
            &mut arena,
            target_authority,
            limits(arena_limits, occurrences),
            CanonicalRoleInputs::single(
                b"source".as_slice(),
                b"green".as_slice(),
                b"projection".as_slice(),
            ),
            &base,
        )
        .expect("reused candidate");
        let mut transitions = 0;
        let target = loop {
            match reused.poll(&mut arena, 1).expect("manifest poll") {
                ManifestPoll::Pending {
                    transitions: consumed,
                } => transitions += consumed,
                ManifestPoll::Published {
                    transitions: consumed,
                    publication,
                } => {
                    transitions += consumed;
                    drop(reused);
                    break publication;
                }
                ManifestPoll::Aborting => panic!("manifest unexpectedly aborted"),
            }
        };
        while !arena.poll_reclaim(31).complete {}
        let target_resident = arena.metrics();
        let receipt = (
            transitions,
            target_resident.resident_nodes - baseline.resident_nodes,
            target_resident.live_payload_bytes - baseline.live_payload_bytes,
        );

        release_publication(&mut arena, target);
        release_publication(&mut arena, base);
        assert_eq!(arena.metrics().resident_nodes, 0);
        receipt
    }

    fn combined_target_receipt(
        source_bytes: usize,
        occurrences: u64,
        document_seed: u8,
    ) -> (u64, SequenceInspectionReceipt, usize, usize, usize) {
        let spacing = 2;
        let arena_limits = ArenaLimits {
            max_slots: 65_536,
            max_live_payload_bytes: 128 * 1024 * 1024,
            max_children_per_node: 128,
        };
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let mut store = SourceStore::new(&"x".repeat(source_bytes)).expect("source");
        let base_source = store.version();
        let document = StrongIdentity::new([document_seed; 16]).expect("document");
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([document_seed.wrapping_add(40); 16]).expect("base publication"),
            base_source,
            CandidateGeneration::FIRST,
            1,
        )
        .expect("base authority");
        let base = publish_reference_manifest_in_arena(&mut arena, base_authority, occurrences);
        let prepared = store
            .prepare_edit(base_source, source_bytes..source_bytes, "\n")
            .expect("append edit");
        drop(
            store
                .commit_prepared_edit(prepared)
                .expect("commit append edit"),
        );
        let target_source = store.version();
        let persistent = persistent_source_facts(&mut arena, &store, spacing);
        let page_count = persistent.page_count();
        while !arena.poll_reclaim(31).complete {}
        let baseline = arena.metrics();
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([document_seed.wrapping_add(41); 16]).expect("target publication"),
            target_source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");
        let mut combined =
            CandidateManifestAssembler::new_with_persistent_source_facts_reusing_references(
                &mut arena,
                &persistent,
                target_authority,
                target_source,
                ParserProfileId::new(1).expect("parser profile"),
                SourceFactsScanProfile::new(spacing).expect("scan profile"),
                limits(arena_limits, occurrences),
                CanonicalRoleInputs::persistent(&b"green"[..], &b"projection"[..]),
                &base,
            )
            .expect("combined candidate");
        let setup = combined
            .persistent_source_facts_setup()
            .expect("bounded SourceFacts setup");
        let mut transitions = 0;
        let target = loop {
            match combined.poll(&mut arena, 1).expect("combined poll") {
                ManifestPoll::Pending {
                    transitions: consumed,
                } => transitions += consumed,
                ManifestPoll::Published {
                    transitions: consumed,
                    publication,
                } => {
                    transitions += consumed;
                    drop(combined);
                    break publication;
                }
                ManifestPoll::Aborting => panic!("combined candidate unexpectedly aborted"),
            }
        };
        while !arena.poll_reclaim(31).complete {}
        let resident = arena.metrics();
        let receipt = (
            page_count,
            setup,
            transitions,
            resident.resident_nodes - baseline.resident_nodes,
            resident.live_payload_bytes - baseline.live_payload_bytes,
        );

        release_publication(&mut arena, target);
        release_publication(&mut arena, base);
        assert!(persistent.release(&mut arena).is_ok());
        while !arena.poll_reclaim(31).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
        receipt
    }

    #[test]
    fn combined_candidate_setup_is_independent_of_both_reused_root_sizes() {
        let small = combined_target_receipt(2_000, 1, 47);
        let large = combined_target_receipt(20_000, 1_024, 48);
        assert!(large.0 > small.0 * 8);
        assert_eq!(small.1, large.1);
        assert_eq!((small.2, small.3, small.4), (large.2, large.3, large.4));
        assert_eq!(large.1.node_headers_decoded, 3);
        assert_eq!(large.1.summary_combinations, 1);
        assert_eq!(large.1.spec.spec_items_hashed, 0);
    }

    #[test]
    fn reused_reference_target_work_is_independent_of_reference_count() {
        assert_eq!(
            reused_reference_target_receipt(1, 32, 203),
            reused_reference_target_receipt(1_024, 33, 204),
        );
    }

    #[test]
    fn cancelling_reused_reference_wrapper_preserves_base_publication() {
        const OCCURRENCES: u64 = 4;
        let arena_limits = ArenaLimits::default();
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let base_authority = authority(31, OCCURRENCES as usize * 16);
        let base = publish_reference_manifest_in_arena(&mut arena, base_authority, OCCURRENCES);
        while !arena.poll_reclaim(31).complete {}
        let base_descriptor =
            decode_manifest(&arena, base.root_id(), base_authority).expect("base manifest");
        let base_digest = manifest_digest256(base_authority, &base_descriptor);
        let references_index = role_index(CandidateRole::References);
        let base_canonical = arena
            .child_at(base_descriptor.children[references_index], 0)
            .expect("base canonical references");
        let baseline = arena.metrics();

        let target_authority = successor_authority(base_authority, 202);
        let mut reused = CandidateManifestAssembler::new_reusing_references(
            &mut arena,
            target_authority,
            limits(arena_limits, OCCURRENCES),
            CanonicalRoleInputs::single(
                b"source".as_slice(),
                b"green".as_slice(),
                b"projection".as_slice(),
            ),
            &base,
        )
        .expect("reused candidate");
        assert!(matches!(
            reused.poll(&mut arena, 1).expect("wrap retained root"),
            ManifestPoll::Pending { transitions: 1 }
        ));
        reused.begin_abort(&mut arena).expect("abort target");
        drop(reused);
        while !arena.poll_reclaim(1).complete {}

        let after_abort = arena.metrics();
        assert_eq!(after_abort.resident_nodes, baseline.resident_nodes);
        assert_eq!(after_abort.live_payload_bytes, baseline.live_payload_bytes);
        assert_eq!(after_abort.live_builds, 0);
        assert_eq!(after_abort.pending_build_aborts, 0);
        let surviving =
            decode_manifest(&arena, base.root_id(), base_authority).expect("surviving base");
        assert_eq!(
            arena
                .child_at(surviving.children[references_index], 0)
                .expect("surviving canonical references"),
            base_canonical
        );
        assert_eq!(manifest_digest256(base_authority, &surviving), base_digest);

        release_publication(&mut arena, base);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn multi_page_cooked_value_is_persistent_and_queryable() {
        let authority = authority(2, 32);
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let mut assembler = assembler(&mut arena, authority, 2);
        let destination = vec![b'd'; BLOB_CHUNK_BYTES * 3 + 17].into_boxed_slice();
        assembler
            .offer_reference(&arena, fact(authority, 0, &b"large"[..], destination))
            .expect("large fact");
        drain_reference(&mut arena, &mut assembler);
        let mut document = publish(arena, assembler);
        let view = document.view().unwrap();
        let occurrence = view.references().unwrap().occurrence(0).unwrap().unwrap();
        assert_eq!(
            occurrence.cooked_destination.len(),
            (BLOB_CHUNK_BYTES * 3 + 17) as u64
        );
        let mut tail = [0_u8; 32];
        let offset = occurrence.cooked_destination.len() - 32;
        assert_eq!(
            occurrence
                .cooked_destination
                .read(offset, &mut tail)
                .unwrap(),
            32
        );
        assert_eq!(tail, [b'd'; 32]);
        close(&mut document);
    }

    #[test]
    fn streamed_reference_matches_atomic_canonical_fact_with_one_page_retention() {
        let authority = authority(12, 32);
        let destination = vec![b'd'; BLOB_CHUNK_BYTES * 3 + 17];
        let title = vec![b't'; BLOB_CHUNK_BYTES + 9];

        let mut atomic_arena = PageArena::new(ArenaLimits::default()).expect("atomic arena");
        let mut atomic = assembler(&mut atomic_arena, authority, 2);
        let mut atomic_fact = fact(
            authority,
            0,
            &b"streamed"[..],
            destination.clone().into_boxed_slice(),
        );
        atomic_fact.title_source = Some(range(13, 15));
        atomic_fact.cooked_title = Some(title.clone().into_boxed_slice());
        atomic
            .offer_reference(&atomic_arena, atomic_fact)
            .expect("atomic fact");
        drain_reference(&mut atomic_arena, &mut atomic);
        let mut atomic_document = publish(atomic_arena, atomic);

        let mut streamed_arena = PageArena::new(ArenaLimits::default()).expect("stream arena");
        let mut streamed = assembler(&mut streamed_arena, authority, 2);
        streamed
            .begin_reference_stream(
                &streamed_arena,
                AuthoritativeReferenceFactStart {
                    authority,
                    source: range(0, 16),
                    label_source: range(1, 4),
                    destination_source: range(6, 12),
                    title_source: Some(range(13, 15)),
                    normalized_label: Box::from(&b"streamed"[..]),
                    destination_len: destination.len(),
                    title_len: Some(title.len()),
                    _not_sync: PhantomData,
                },
            )
            .expect("stream start");
        assert!(matches!(
            streamed
                .offer_reference_stream_bytes(StreamedReferenceValueKind::Title, b"wrong order",),
            Err(ManifestError::Reference(
                ReferenceRootError::StreamValueMismatch
            ))
        ));

        let mut maximum_retained = 0;
        let mut offer = |kind: StreamedReferenceValueKind, bytes: &[u8]| {
            let mut offset = 0;
            while offset < bytes.len() {
                let capacity = streamed
                    .reference_stream_capacity(kind)
                    .expect("stream capacity");
                if capacity == 0 {
                    assert!(matches!(
                        streamed.poll(&mut streamed_arena, 1).expect("stream poll"),
                        ManifestPoll::Pending { .. }
                    ));
                } else {
                    let end = offset.saturating_add(capacity.min(37)).min(bytes.len());
                    let consumed = streamed
                        .offer_reference_stream_bytes(kind, &bytes[offset..end])
                        .expect("stream bytes");
                    assert!(consumed > 0);
                    offset += consumed;
                }
                maximum_retained = maximum_retained.max(streamed.reference_stream_retained_bytes());
            }
        };
        offer(StreamedReferenceValueKind::Destination, &destination);
        offer(StreamedReferenceValueKind::Title, &title);
        drain_reference(&mut streamed_arena, &mut streamed);
        assert!(maximum_retained <= BLOB_CHUNK_BYTES + b"streamed".len());
        let mut streamed_document = publish(streamed_arena, streamed);

        let atomic_view = atomic_document.view().expect("atomic view");
        let streamed_view = streamed_document.view().expect("stream view");
        assert_eq!(
            atomic_view.role_metadata(CandidateRole::References),
            streamed_view.role_metadata(CandidateRole::References)
        );
        let occurrence = streamed_view
            .references()
            .expect("references")
            .occurrence(0)
            .expect("occurrence query")
            .expect("occurrence");
        assert!(occurrence.normalized_label.equals(b"streamed").unwrap());
        assert!(occurrence.cooked_destination.equals(&destination).unwrap());
        assert!(occurrence
            .cooked_title
            .expect("title")
            .equals(&title)
            .unwrap());
        assert_eq!(
            atomic_document.arena.metrics().resident_nodes,
            streamed_document.arena.metrics().resident_nodes
        );
        assert_eq!(
            atomic_document.arena.metrics().live_payload_bytes,
            streamed_document.arena.metrics().live_payload_bytes
        );
        assert!(streamed_document.arena.metrics().resident_nodes > 14);
        validate_reference_digest(&atomic_document);
        validate_reference_digest(&streamed_document);

        close(&mut streamed_document);
        close(&mut atomic_document);
    }

    #[test]
    fn admission_failure_and_cross_authority_do_not_mutate_arena() {
        let authority = authority(3, 64);
        let other = self::authority(4, 64);
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let mut assembler = assembler(&mut arena, authority, 4);
        let before = arena.metrics();
        assert!(matches!(
            assembler.offer_reference(&arena, fact(other, 0, &b"x"[..], &b"u"[..])),
            Err(ManifestError::Reference(ReferenceRootError::CrossAuthority))
        ));
        assert_eq!(arena.metrics(), before);
        let mut outside = fact(authority, 0, &b"x"[..], &b"u"[..]);
        outside.source = range(60, 76);
        outside.label_source = range(61, 64);
        outside.destination_source = range(66, 72);
        assert!(matches!(
            assembler.offer_reference(&arena, outside),
            Err(ManifestError::Reference(ReferenceRootError::InvalidRange))
        ));
        assert_eq!(arena.metrics(), before);
        assembler.begin_abort(&mut arena).expect("abort");
        drop(assembler);
        while !arena.poll_reclaim(1).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);

        let tiny_limits = ArenaLimits {
            max_slots: 4,
            max_live_payload_bytes: ARENA_PAGE_BYTES * 4,
            max_children_per_node: 65,
        };
        let mut tiny = PageArena::new(tiny_limits).expect("tiny arena");
        let before = tiny.metrics();
        assert!(matches!(
            CandidateManifestAssembler::new(
                &mut tiny,
                authority,
                limits(tiny_limits, 1),
                CanonicalRoleInputs::single(&b"s"[..], &b"g"[..], &b"p"[..]),
            ),
            Err(ManifestError::CapacityPreflight)
        ));
        assert_eq!(tiny.metrics(), before);
    }

    #[test]
    fn manifest_preflight_counts_parser_local_scratch_against_payload_budget() {
        let arena_limits = ArenaLimits {
            max_slots: 8,
            max_live_payload_bytes: ARENA_PAGE_BYTES,
            max_children_per_node: 8,
        };
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let reservation = arena
            .reserve_external_payload(ARENA_PAGE_BYTES - 6)
            .expect("scratch reservation");
        assert!(matches!(
            preflight_remaining(&arena, arena_limits, 1, 7),
            Err(ManifestError::CapacityPreflight)
        ));
        preflight_remaining(&arena, arena_limits, 1, 6).expect("exact remaining payload");
        arena
            .release_external_payload(reservation)
            .map_err(|failure| failure.error)
            .expect("release scratch reservation");
    }

    #[test]
    fn cancellation_of_partial_reference_build_is_fuelled() {
        let authority = authority(5, 32);
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let mut assembler = assembler(&mut arena, authority, 2);
        let destination = vec![b'x'; BLOB_CHUNK_BYTES * 8].into_boxed_slice();
        assembler
            .offer_reference(&arena, fact(authority, 0, &b"x"[..], destination))
            .expect("fact");
        assembler.poll(&mut arena, 3).expect("partial poll");
        assert!(arena.metrics().resident_nodes > 1);
        assembler.begin_abort(&mut arena).expect("abort");
        drop(assembler);
        let mut polls = 0;
        while !arena.poll_reclaim(1).complete {
            polls += 1;
        }
        assert!(polls > 3);
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn decoder_rejects_manifest_digest_corruption() {
        let authority = authority(6, 16);
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let (build, manifest_id) = {
            let mut session = arena.begin_build().expect("build");
            let mut child_ids = Vec::new();
            let mut child_owners = Vec::new();
            for _ in 0..CANDIDATE_ROLE_COUNT {
                let owner = session
                    .allocate(&encode_candidate_header(ANCHOR_TAG, authority), &[])
                    .expect("dummy child");
                child_ids.push(owner.id());
                child_owners.push(owner);
            }
            let metadata: [RoleMetadata; CANDIDATE_ROLE_COUNT] =
                std::array::from_fn(|index| RoleMetadata {
                    role: CandidateRole::ORDERED[index],
                    schema: schema_for(CandidateRole::ORDERED[index]),
                    record_count: 0,
                    canonical_bytes: 0,
                    digest: [index as u8; STRONG_DIGEST_BYTES],
                });
            let mut payload = encode_manifest(authority, &metadata);
            *payload.last_mut().expect("digest byte") ^= 0xff;
            let manifest = session.allocate(&payload, &child_ids).expect("manifest");
            let manifest_id = manifest.id();
            let build = session.suspend().expect("suspend");
            (build, manifest_id)
        };
        assert!(matches!(
            decode_manifest(&arena, manifest_id, authority),
            Err(ManifestError::Corrupt("candidate manifest digest changed"))
        ));
        arena.abort_build(build).expect("abort malformed build");
        while !arena.poll_reclaim(8).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn one_hundred_thousand_definitions_stay_inside_shared_journal_envelope() {
        const DEFINITIONS: u64 = 100_000;
        let authority = authority(7, DEFINITIONS as usize * 16);
        let arena_limits = ArenaLimits {
            max_slots: crate::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS,
            max_live_payload_bytes: 64 * 1024 * 1024,
            max_children_per_node: 65,
        };
        let mut arena = PageArena::new(arena_limits).expect("arena");
        let mut assembler = CandidateManifestAssembler::new(
            &mut arena,
            authority,
            limits(arena_limits, DEFINITIONS),
            CanonicalRoleInputs::single(&b"s"[..], &b"g"[..], &b"p"[..]),
        )
        .expect("assembler");
        for ordinal in 0..DEFINITIONS {
            let label: Box<[u8]> = if ordinal % 10_000 == 0 {
                Box::from(&b"shared"[..])
            } else {
                format!("label-{ordinal}").into_bytes().into_boxed_slice()
            };
            assembler
                .offer_reference(
                    &arena,
                    fact(authority, ordinal, label, Box::<[u8]>::from(&b"u"[..])),
                )
                .expect("offer");
            drain_reference(&mut arena, &mut assembler);
        }
        let mut document = publish(arena, assembler);
        {
            let view = document.view().expect("view");
            let references = view.references().expect("references");
            assert_eq!(references.count(), DEFINITIONS);
            assert_eq!(
                references
                    .occurrence(DEFINITIONS - 1)
                    .unwrap()
                    .unwrap()
                    .ordinal,
                DEFINITIONS - 1
            );
            assert_eq!(references.winner("shared").unwrap().unwrap().ordinal, 0);
            let metrics = document.arena.metrics();
            assert_eq!(metrics.resident_nodes, 101_575);
            assert_eq!(metrics.live_payload_bytes, 18_433_955);
            assert_eq!(
                26_559_395 - metrics.live_payload_bytes,
                (metrics.resident_nodes - 7)
                    * (CANDIDATE_HEADER_BYTES - CANONICAL_NODE_HEADER_BYTES),
                "only the seven fresh authority wrappers retain the full header"
            );
            eprintln!(
                "m11_100k_reference_storage definitions={DEFINITIONS} resident_nodes={} live_payload_bytes={} pending_reclaims={} live_builds={}",
                metrics.resident_nodes,
                metrics.live_payload_bytes,
                metrics.pending_reclaims,
                metrics.live_builds,
            );
        }
        close(&mut document);
        let closed = document.arena.metrics();
        assert_eq!(closed.resident_nodes, 0);
        assert_eq!(closed.live_payload_bytes, 0);
        assert_eq!(closed.pending_reclaims, 0);
        assert_eq!(closed.live_builds, 0);
        assert_eq!(closed.pending_build_aborts, 0);
        eprintln!(
            "m11_100k_reference_storage_closed resident_nodes={} live_payload_bytes={} pending_reclaims={} allocated_slots={} live_builds={} pending_build_aborts={}",
            closed.resident_nodes,
            closed.live_payload_bytes,
            closed.pending_reclaims,
            closed.allocated_slots,
            closed.live_builds,
            closed.pending_build_aborts,
        );
    }

    #[test]
    fn candidate_manifest_and_reference_capabilities_are_send() {
        fn assert_send<T: Send>() {}

        assert_send::<CandidateManifestAssembler>();
        assert_send::<PublishedManifest>();
        assert_send::<ManifestPoll>();
        assert_send::<ManifestView<'static>>();
        assert_send::<AuthoritativeReferenceFact>();
        assert_send::<ReferenceRootBuilder>();
        assert_send::<ReferenceSubtreeRoot>();
        assert_send::<ReferenceBuildPoll>();
        assert_send::<ReferenceRootView>();
        assert_send::<ReferenceOccurrenceView<'static>>();
        assert_send::<PersistentBytesView<'static>>();
    }
}
