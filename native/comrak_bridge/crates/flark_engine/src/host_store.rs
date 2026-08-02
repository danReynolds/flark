//! Canonical candidate transfer into an independent host.
//!
//! A candidate manifest payload is not self-contained: all five role roots and
//! their record/page/blob closures are arena child edges. This module supplies
//! the missing final-shaped seam without serializing source-arena identities.
//! The producer emits each reachable node once in postorder; frames carry only
//! payload bytes and stream-local child ordinals, never source `ArenaId`s. The
//! independent host preserves DAG sharing in its own arena, recomputes
//! transport and canonical role digests, and swaps the root only after complete
//! validation. An exact-base References delta uses the same program: virtual
//! ordinal zero denotes only the already-validated canonical References root,
//! while the target role wrapper and manifest are still transferred fresh.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

use crate::block_sequence::{
    decode_persistent_m11_block_role_descriptor, encode_persistent_m11_block_role_descriptor,
    is_m11_block_sequence_node_payload, persistent_m11_block_locate_ordinal_window,
    persistent_m11_block_locate_point, persistent_m11_block_storage_page_at,
    persistent_m11_block_visit_entries, plan_persistent_m11_block_semantic_splice,
    validate_imported_m11_block_sequence_node, M11BlockRoleLane, M11BlockSequenceError,
    M11BlockSequenceHostReplay, M11BlockSequenceHostReplayPoll, M11BlockSequenceHostSpliceClaim,
    M11BlockSequenceHostSpliceWork, M11BlockSequenceLocation, M11BlockSequenceOrdinalWindow,
    M11BlockSequencePoint, M11BlockSequenceVisitControl, M11BlockSequenceVisitEntry,
    M11BlockSequenceVisitReceipt, M11BlockSequenceVisitStart, PersistentM11BlockRoleDescriptor,
    PersistentM11BlockRootClaim, PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA,
    PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA, PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES,
};
use crate::candidate_manifest::{
    canonical_role_record_count, decode_candidate_header, decode_canonical_node_header,
    decode_manifest, decode_manifest_descriptor, encode_candidate_header, manifest_digest256,
    manifest_persistent_inline_projection_record_at, manifest_role_record_bytes_at,
    persistent_block_manifest_roles, persistent_inline_projection_manifest_role,
    persistent_recursive_green_manifest_role, persistent_source_facts_manifest_role,
    push_role_metadata, read_role_metadata, read_u32, read_u64, role_index, CandidateAuthority,
    CandidateRole, ManifestError, PublishedManifest, RoleMetadata, StrongIdentity,
    CANDIDATE_FORMAT_VERSION, CANDIDATE_HEADER_BYTES, PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA,
};
use crate::identity::{CandidateGeneration, SourceRevision, SourceRootId};
use crate::inline_overlay::{M11InlineOverlayBase, M11InlineOverlayError};
use crate::inline_projection::{
    encode_persistent_inline_link_values, PersistentM11InlineLinkValueEncodeReceipt,
    PersistentM11InlineProjectionHostCursor, PersistentM11InlineProjectionHostValidationPoll,
    PersistentM11InlineProjectionHostValidator,
};
use crate::parser_pages::{
    is_m11_parser_page_node_payload, validate_imported_m11_parser_page_node,
};
use crate::recursive_green::{
    decode_persistent_m11_recursive_green_role_descriptor,
    encode_persistent_m11_recursive_green_role_descriptor, is_m11_recursive_green_node_payload,
    persistent_m11_recursive_green_locate_point,
    persistent_m11_recursive_green_locate_row_ordinal_window,
    persistent_m11_recursive_green_locate_rows, persistent_m11_recursive_green_storage_page_at,
    plan_persistent_m11_recursive_green_semantic_splice,
    validate_imported_m11_recursive_green_node, M11RecursiveGreenError,
    M11RecursiveGreenHostReplay, M11RecursiveGreenHostReplayPoll, M11RecursiveGreenHostSpliceClaim,
    M11RecursiveGreenHostSpliceWork, M11RecursiveGreenLocation, M11RecursiveGreenPoint,
    M11RecursiveGreenRowOrdinalWindow, M11RecursiveGreenRowQueryLimits,
    M11RecursiveGreenRowQueryOutcome, PersistentM11RecursiveGreenRoleDescriptor,
    PersistentM11RecursiveGreenRootClaim, PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES,
};
use crate::reference_root::{
    ReferenceRoleDigestValidator, ReferenceRoleValidationPoll, ReferenceRootError,
};
use crate::source::SourceVersion;
use crate::source_facts::{
    is_persistent_source_facts_sequence_leaf_payload,
    is_persistent_source_facts_sequence_node_payload, persistent_source_facts_leaf_record_at,
    validate_imported_persistent_source_facts_node,
    validate_persistent_source_facts_host_replay_descriptor, PersistentSourceFactsHostReplay,
    PersistentSourceFactsHostReplayDescriptorClaim, PersistentSourceFactsHostReplayPoll,
    PersistentSourceFactsHostReplayWork, SourceFactsAssemblyError,
    PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES, PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA,
};
use crate::storage::{
    ArenaBuildOwner, ArenaError, ArenaLimits, CandidateBuild, CandidateSeal, CommittedArenaRoot,
    PageArena, ARENA_PAGE_BYTES,
};
use crate::ArenaId;

const SNAPSHOT_BEGIN_TAG: u8 = 0xe0;
const SNAPSHOT_NODE_TAG: u8 = 0xe1;
const SNAPSHOT_END_TAG: u8 = 0xe2;
/// Begins a program whose virtual ordinal zero is the exact installed
/// References canonical root. The root's storage closure is not transferred.
const SNAPSHOT_REFERENCES_DELTA_BEGIN_TAG: u8 = 0xe3;
/// Begins the typed exact-base program. Unlike the legacy References-only
/// optimization, this program authenticates the complete installed authority,
/// reuses References, and splices persistent SourceFacts v2 pages before any
/// ordinary target node may be admitted.
const SNAPSHOT_EXACT_BASE_DELTA_BEGIN_TAG: u8 = 0xe4;
/// Carries one canonical SFL2 replacement page for the SourceFacts splice op.
const SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG: u8 = 0xe5;
/// Carries one canonical BSL1 replacement page for the block splice op.
const SNAPSHOT_BLOCK_REPLACEMENT_TAG: u8 = 0xe6;
/// Carries one canonical RGL1 replacement leaf for the recursive Green splice.
const SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_TAG: u8 = 0xe7;
const SNAPSHOT_EXACT_BASE_PROGRAM_SCHEMA: u32 = 1;
const SNAPSHOT_EXACT_BASE_OPERATION_COUNT: u16 = 2;
const SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_COUNT: u16 = 3;
const SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_OP: u8 = 1;
const SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_OP: u8 = 2;
const SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_OP: u8 = 3;
const SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_OP: u8 = 4;
const SNAPSHOT_EXACT_BASE_OPERATION_VERSION: u8 = 1;
const SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_BYTES: usize = 72;
const SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_BYTES: usize = 320;
const SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_BYTES: usize =
    8 + 8 + (8 * 8) + (4 * 56) + (4 * PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES);
const SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_BYTES: usize =
    8 + 8 + (8 * 8) + (2 * 56) + (2 * PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES);
const SNAPSHOT_EXACT_BASE_OPERATION_TABLE_BYTES: usize =
    SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_BYTES + SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_BYTES;
const SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_TABLE_BYTES: usize =
    SNAPSHOT_EXACT_BASE_OPERATION_TABLE_BYTES + SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_BYTES;
const SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_OPERATION_TABLE_BYTES: usize =
    SNAPSHOT_EXACT_BASE_OPERATION_TABLE_BYTES + SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_BYTES;
const SNAPSHOT_EXACT_BASE_BASE_AUTHORITY_BYTES: usize = 64;
const SNAPSHOT_EXACT_BASE_BEGIN_BYTES: usize = CANDIDATE_HEADER_BYTES
    + 12
    + SNAPSHOT_EXACT_BASE_BASE_AUTHORITY_BYTES
    + SNAPSHOT_EXACT_BASE_OPERATION_TABLE_BYTES;
const SNAPSHOT_EXACT_BASE_BLOCK_BEGIN_BYTES: usize = CANDIDATE_HEADER_BYTES
    + 12
    + SNAPSHOT_EXACT_BASE_BASE_AUTHORITY_BYTES
    + SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_TABLE_BYTES;
const SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_BEGIN_BYTES: usize = CANDIDATE_HEADER_BYTES
    + 12
    + SNAPSHOT_EXACT_BASE_BASE_AUTHORITY_BYTES
    + SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_OPERATION_TABLE_BYTES;
const SNAPSHOT_SOURCE_FACTS_REPLACEMENT_HEADER_BYTES: usize = 16;
const SNAPSHOT_BLOCK_REPLACEMENT_HEADER_BYTES: usize = 16;
const SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES: usize = 16;
const SNAPSHOT_SOURCE_FACTS_OPERATION_INDEX: u8 = 1;
const SNAPSHOT_BLOCK_OPERATION_INDEX: u8 = 2;
const SNAPSHOT_RECURSIVE_GREEN_OPERATION_INDEX: u8 = 2;
const SNAPSHOT_ABSENT_VIRTUAL_ORDINAL: u64 = u64::MAX;
pub(crate) const SNAPSHOT_NODE_HEADER_BYTES: usize = 20;
pub(crate) const SNAPSHOT_CHILD_ORDINAL_BYTES: usize = std::mem::size_of::<u64>();
/// Production M1.1 arenas and offer validation admit at most this many child
/// edges on one node.
pub(crate) const M11_MAXIMUM_SNAPSHOT_CHILDREN: usize = 128;
/// One complete Node frame: fixed header, every serialized child ordinal, and
/// the largest arena payload. Begin and End frames are smaller.
pub(crate) const M11_MAXIMUM_SNAPSHOT_FRAME_BYTES: usize = SNAPSHOT_NODE_HEADER_BYTES
    + M11_MAXIMUM_SNAPSHOT_CHILDREN * SNAPSHOT_CHILD_ORDINAL_BYTES
    + ARENA_PAGE_BYTES;
const SNAPSHOT_END_BYTES: usize = 60;
const SNAPSHOT_DIGEST_BYTES: usize = 32;

/// Semantic kind of one complete self-contained snapshot frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotFrameKind {
    Begin,
    Node,
    End,
    SourceFactsReplacementPage,
    BlockSequenceReplacementPage,
    RecursiveGreenReplacementPage,
}

/// Independently decoded metadata used by both producer and host adapters.
///
/// The bridge never identifies an engine frame by peeking at tag bytes. This
/// classifier is the single engine-owned interpretation seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotFrameMetadata {
    pub(crate) kind: SnapshotFrameKind,
    pub(crate) node_ordinal: Option<u64>,
    pub(crate) canonical_record_count: u32,
    pub(crate) canonical_stream_digest256: Option<[u8; SNAPSHOT_DIGEST_BYTES]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CandidateHostLimits {
    pub(crate) arena: ArenaLimits,
    pub(crate) maximum_snapshot_nodes: u64,
    pub(crate) maximum_snapshot_wire_bytes: u64,
    pub(crate) maximum_query_bytes: usize,
}

impl Default for CandidateHostLimits {
    fn default() -> Self {
        Self {
            arena: ArenaLimits::default(),
            maximum_snapshot_nodes: 1_000_000,
            maximum_snapshot_wire_bytes: 512 * 1024 * 1024,
            maximum_query_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug)]
pub(crate) enum CandidateHostError {
    InvalidLimits,
    InvalidFrame(&'static str),
    CrossAuthority,
    BaseMismatch,
    StaleCandidate,
    Busy,
    NoOffer,
    ZeroFuel,
    AllocationFailed,
    Arena(ArenaError),
    Manifest(ManifestError),
    Reference(ReferenceRootError),
    BlockSequence(M11BlockSequenceError),
    RecursiveGreen(M11RecursiveGreenError),
    SourceFacts(SourceFactsAssemblyError),
}

impl fmt::Display for CandidateHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid candidate host limits"),
            Self::InvalidFrame(message) => write!(formatter, "invalid snapshot frame: {message}"),
            Self::CrossAuthority => formatter.write_str("snapshot crosses host authority"),
            Self::BaseMismatch => formatter.write_str("snapshot does not name the installed base"),
            Self::StaleCandidate => {
                formatter.write_str("snapshot generation is not newer than the installed candidate")
            }
            Self::Busy => formatter.write_str("candidate host is busy"),
            Self::NoOffer => formatter.write_str("candidate host has no active offer"),
            Self::ZeroFuel => formatter.write_str("candidate host poll requires nonzero fuel"),
            Self::AllocationFailed => formatter.write_str("candidate host allocation failed"),
            Self::Arena(error) => write!(formatter, "candidate host arena failed: {error}"),
            Self::Manifest(error) => write!(formatter, "candidate manifest failed: {error}"),
            Self::Reference(error) => write!(formatter, "reference validation failed: {error}"),
            Self::BlockSequence(error) => {
                write!(formatter, "persistent block query failed: {error}")
            }
            Self::RecursiveGreen(error) => {
                write!(
                    formatter,
                    "persistent recursive Green query failed: {error}"
                )
            }
            Self::SourceFacts(error) => {
                write!(formatter, "persistent SourceFacts replay failed: {error}")
            }
        }
    }
}

impl std::error::Error for CandidateHostError {}

impl From<ArenaError> for CandidateHostError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ManifestError> for CandidateHostError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl From<ReferenceRootError> for CandidateHostError {
    fn from(value: ReferenceRootError) -> Self {
        Self::Reference(value)
    }
}

impl From<M11BlockSequenceError> for CandidateHostError {
    fn from(value: M11BlockSequenceError) -> Self {
        Self::BlockSequence(value)
    }
}

impl From<M11RecursiveGreenError> for CandidateHostError {
    fn from(value: M11RecursiveGreenError) -> Self {
        Self::RecursiveGreen(value)
    }
}

impl From<SourceFactsAssemblyError> for CandidateHostError {
    fn from(value: SourceFactsAssemblyError) -> Self {
        Self::SourceFacts(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InstalledCandidateSnapshot {
    authority: CandidateAuthority,
}

impl InstalledCandidateSnapshot {
    pub(crate) const fn source_revision(self) -> SourceRevision {
        self.authority.source_revision
    }

    pub(crate) const fn parse_generation(self) -> CandidateGeneration {
        self.authority.parse_generation
    }

    pub(crate) const fn publication_identity(self) -> StrongIdentity {
        self.authority.publication
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InstalledPersistentInlineProjectionDescriptor {
    pub(crate) source_start: u32,
    pub(crate) source_end: u32,
    pub(crate) structural_record_count: u64,
    pub(crate) logical_page_count: u64,
    pub(crate) fact_count: u64,
    pub(crate) storage_page_count: u64,
    pub(crate) link_value_entry_count: u32,
    pub(crate) link_value_storage_page_count: u64,
    pub(crate) link_value_encoded_bytes: u32,
    pub(crate) maximum_open_depth: u32,
    pub(crate) maximum_tree_nodes_visited: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InstalledPersistentBlockDescriptor {
    pub(crate) source_bytes: u64,
    pub(crate) source_utf16: u64,
    pub(crate) entry_count: u64,
    pub(crate) reference_definition_count: u64,
    pub(crate) storage_page_count: u64,
    pub(crate) tree_height: u16,
    pub(crate) maximum_tree_nodes_visited: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InstalledPersistentRecursiveGreenDescriptor {
    pub(crate) source_bytes: u64,
    pub(crate) source_utf16: u64,
    pub(crate) event_count: u64,
    pub(crate) renderable_row_count: u64,
    pub(crate) storage_page_count: u64,
    pub(crate) tree_height: u16,
}

pub(crate) struct CandidateSnapshotEncoder<'a> {
    arena: &'a PageArena,
    state: CandidateSnapshotEncoderState,
    _publication_borrow: PhantomData<&'a PublishedManifest>,
}

/// Arena-independent traversal state for an owned producer stream.
///
/// Keeping the arena borrow at the `poll` call boundary lets a higher-level
/// owner retain the complete manifest document and this bounded state without
/// a self-referential Rust value. No arena identity or payload is copied here.
pub(crate) struct CandidateSnapshotEncoderState {
    authority: CandidateAuthority,
    program: SnapshotProgram,
    closure: ArenaClosureSnapshotEncoder,
    exact_base_delta: Option<ExactBaseDeltaEncodeState>,
}

/// Generic postorder transport for one arena closure.
///
/// Program-specific Begin bytes are supplied by the typed caller. Node and End
/// frames are the single shared arena transport used by canonical candidates
/// and sibling sidecars; source `ArenaId`s never cross the wire.
pub(crate) struct ArenaClosureSnapshotEncoder {
    stack: Vec<EncodeVisit>,
    visits: HashMap<ArenaId, EncodeVisitState>,
    pending_roots: Vec<ArenaId>,
    synthetic_root: Option<(Vec<ArenaId>, Box<[u8]>)>,
    literal_root: Option<Box<[u8]>>,
    next_ordinal: u64,
    payload_bytes: u64,
    wire_bytes: u64,
    digest: blake3::Hasher,
    begun: bool,
    ended: bool,
}

struct ExactBaseDeltaEncodeState {
    base_authority: CandidateAuthority,
    reused_references: RoleMetadata,
    base_source_facts: RoleMetadata,
    target_source_facts: RoleMetadata,
    target_source_facts_descriptor: [u8; PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES],
    base_page_range: std::ops::Range<u64>,
    target_page_range: std::ops::Range<u64>,
    target_source_facts_root: Option<ArenaId>,
    next_replacement_page: u64,
    structural_splice: Option<ExactBaseStructuralSpliceEncodeState>,
    replay_barrier_issued: bool,
    replay_resumed: bool,
}

enum ExactBaseStructuralSpliceEncodeState {
    Blocks(ExactBaseBlockSpliceEncodeState),
    RecursiveGreen(ExactBaseRecursiveGreenSpliceEncodeState),
}

enum ExactBaseStructuralRanges {
    Blocks {
        base: std::ops::Range<u64>,
        target: std::ops::Range<u64>,
    },
    RecursiveGreen {
        base: std::ops::Range<u64>,
        target: std::ops::Range<u64>,
    },
}

struct ExactBaseBlockSpliceEncodeState {
    claim: M11BlockSequenceHostSpliceClaim,
    base_green_metadata: RoleMetadata,
    base_projection_metadata: RoleMetadata,
    target_green_metadata: RoleMetadata,
    target_projection_metadata: RoleMetadata,
    target_root: Option<ArenaId>,
    virtual_ordinal: u64,
    next_replacement_page: u64,
}

struct ExactBaseRecursiveGreenSpliceEncodeState {
    claim: M11RecursiveGreenHostSpliceClaim,
    base_green_metadata: RoleMetadata,
    target_green_metadata: RoleMetadata,
    target_root: Option<ArenaId>,
    virtual_ordinal: u64,
    next_replacement_page: u64,
}

struct EncodeVisit {
    id: ArenaId,
    next_child: usize,
    child_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EncodeVisitState {
    Visiting,
    Emitted(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SnapshotProgram {
    Full,
    ExactBaseReferences,
    ExactBaseDelta,
}

pub(crate) enum CandidateSnapshotEncodePoll {
    Pending {
        transitions: usize,
    },
    Frame {
        transitions: usize,
        bytes: Box<[u8]>,
    },
    /// Every replacement page is on the wire, but ordinary manifest nodes are
    /// still forbidden. The producer must wait for the host to complete the
    /// exact-base replay and then explicitly resume this encoder.
    ReplayRequired {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        bytes: Box<[u8]>,
    },
}

pub(crate) enum ArenaClosureSnapshotEncodePoll {
    Pending {
        transitions: usize,
    },
    Frame {
        transitions: usize,
        bytes: Box<[u8]>,
    },
    Complete {
        transitions: usize,
        bytes: Box<[u8]>,
    },
}

impl<'a> CandidateSnapshotEncoder<'a> {
    pub(crate) fn new(
        arena: &'a PageArena,
        publication: &'a PublishedManifest,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            arena,
            state: CandidateSnapshotEncoderState::new(arena, publication)?,
            _publication_borrow: PhantomData,
        })
    }

    /// Encodes a target manifest while representing its canonical References
    /// root as virtual ordinal zero rather than traversing that closure.
    ///
    /// The target publication must already own the reused root beneath a fresh
    /// target References wrapper. Exact-base authority is independently
    /// authenticated by [`CandidateHostStore::begin_references_delta`].
    pub(crate) fn new_references_delta(
        arena: &'a PageArena,
        publication: &'a PublishedManifest,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            arena,
            state: CandidateSnapshotEncoderState::new_references_delta(arena, publication)?,
            _publication_borrow: PhantomData,
        })
    }

    /// Encodes one exact-base transaction:
    ///
    /// 1. a typed Begin/op table bound to `base`;
    /// 2. only the target SourceFacts replacement pages;
    /// 3. a hard replay barrier;
    /// 4. the remaining target closure with canonical References and
    ///    SourceFacts represented by virtual ordinals.
    pub(crate) fn new_exact_base_delta(
        arena: &'a PageArena,
        base: &'a PublishedManifest,
        target: &'a PublishedManifest,
        base_page_range: std::ops::Range<u64>,
        target_page_range: std::ops::Range<u64>,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            arena,
            state: CandidateSnapshotEncoderState::new_exact_base_delta(
                arena,
                base,
                target,
                base_page_range,
                target_page_range,
            )?,
            _publication_borrow: PhantomData,
        })
    }

    /// Engine/host vertical slice for an exact transaction that additionally
    /// virtualizes one persistent BlockSequence splice.
    ///
    /// The semantic ranges are independently mapped to packed storage pages
    /// in both producer and host arenas. Only target pages intersecting the
    /// replacement plus fresh path/wrapper nodes cross the stream.
    pub(crate) fn new_exact_base_delta_with_block_splice(
        arena: &'a PageArena,
        base: &'a PublishedManifest,
        target: &'a PublishedManifest,
        base_source_facts_page_range: std::ops::Range<u64>,
        target_source_facts_page_range: std::ops::Range<u64>,
        base_block_entry_range: std::ops::Range<u64>,
        target_block_entry_range: std::ops::Range<u64>,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            arena,
            state: CandidateSnapshotEncoderState::new_exact_base_delta_with_block_splice(
                arena,
                base,
                target,
                base_source_facts_page_range,
                target_source_facts_page_range,
                base_block_entry_range,
                target_block_entry_range,
            )?,
            _publication_borrow: PhantomData,
        })
    }

    /// Exact-base transaction whose structural authority is recursive Green.
    /// Only complete target RGL1 leaves intersecting the semantic event cut
    /// cross the wire; target RGB1 branches are reconstructed by the host.
    pub(crate) fn new_exact_base_delta_with_recursive_green_splice(
        arena: &'a PageArena,
        base: &'a PublishedManifest,
        target: &'a PublishedManifest,
        base_source_facts_page_range: std::ops::Range<u64>,
        target_source_facts_page_range: std::ops::Range<u64>,
        base_green_event_range: std::ops::Range<u64>,
        target_green_event_range: std::ops::Range<u64>,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            arena,
            state: CandidateSnapshotEncoderState::new_exact_base_delta_with_recursive_green_splice(
                arena,
                base,
                target,
                base_source_facts_page_range,
                target_source_facts_page_range,
                base_green_event_range,
                target_green_event_range,
            )?,
            _publication_borrow: PhantomData,
        })
    }

    fn from_root(
        arena: &'a PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            arena,
            state: CandidateSnapshotEncoderState::from_root(
                arena,
                authority,
                root,
                SnapshotProgram::Full,
                &[],
                None,
            )?,
            _publication_borrow: PhantomData,
        })
    }

    pub(crate) fn begin_frame(&mut self) -> Result<Box<[u8]>, CandidateHostError> {
        self.state.begin_frame()
    }

    pub(crate) fn poll(
        &mut self,
        fuel: usize,
    ) -> Result<CandidateSnapshotEncodePoll, CandidateHostError> {
        self.state.poll(self.arena, fuel)
    }

    pub(crate) fn resume_exact_base_delta(&mut self) -> Result<(), CandidateHostError> {
        self.state.resume_exact_base_delta()
    }
}

impl CandidateSnapshotEncoderState {
    pub(crate) fn new(
        arena: &PageArena,
        publication: &PublishedManifest,
    ) -> Result<Self, CandidateHostError> {
        let authority = publication.authority();
        let root = publication.root_id();
        // This also proves that the stream starts from exactly one complete
        // five-role manifest, not an arbitrary arena subgraph.
        let _ = decode_manifest(arena, root, authority)?;
        Self::from_root(arena, authority, root, SnapshotProgram::Full, &[], None)
    }

    pub(crate) fn new_references_delta(
        arena: &PageArena,
        publication: &PublishedManifest,
    ) -> Result<Self, CandidateHostError> {
        let authority = publication.authority();
        let root = publication.root_id();
        let descriptor = decode_manifest(arena, root, authority)?;
        let references_wrapper = descriptor.children[role_index(CandidateRole::References)];
        let references_root = arena.child_at(references_wrapper, 0)?;
        Self::from_root(
            arena,
            authority,
            root,
            SnapshotProgram::ExactBaseReferences,
            &[references_root],
            None,
        )
    }

    pub(crate) fn new_exact_base_delta(
        arena: &PageArena,
        base: &PublishedManifest,
        target: &PublishedManifest,
        base_page_range: std::ops::Range<u64>,
        target_page_range: std::ops::Range<u64>,
    ) -> Result<Self, CandidateHostError> {
        Self::new_exact_base_delta_inner(
            arena,
            base,
            target,
            base_page_range,
            target_page_range,
            None,
        )
    }

    pub(crate) fn new_exact_base_delta_with_block_splice(
        arena: &PageArena,
        base: &PublishedManifest,
        target: &PublishedManifest,
        base_page_range: std::ops::Range<u64>,
        target_page_range: std::ops::Range<u64>,
        base_block_entry_range: std::ops::Range<u64>,
        target_block_entry_range: std::ops::Range<u64>,
    ) -> Result<Self, CandidateHostError> {
        Self::new_exact_base_delta_inner(
            arena,
            base,
            target,
            base_page_range,
            target_page_range,
            Some(ExactBaseStructuralRanges::Blocks {
                base: base_block_entry_range,
                target: target_block_entry_range,
            }),
        )
    }

    pub(crate) fn new_exact_base_delta_with_recursive_green_splice(
        arena: &PageArena,
        base: &PublishedManifest,
        target: &PublishedManifest,
        base_source_facts_page_range: std::ops::Range<u64>,
        target_source_facts_page_range: std::ops::Range<u64>,
        base_green_event_range: std::ops::Range<u64>,
        target_green_event_range: std::ops::Range<u64>,
    ) -> Result<Self, CandidateHostError> {
        Self::new_exact_base_delta_inner(
            arena,
            base,
            target,
            base_source_facts_page_range,
            target_source_facts_page_range,
            Some(ExactBaseStructuralRanges::RecursiveGreen {
                base: base_green_event_range,
                target: target_green_event_range,
            }),
        )
    }

    fn new_exact_base_delta_inner(
        arena: &PageArena,
        base: &PublishedManifest,
        target: &PublishedManifest,
        base_page_range: std::ops::Range<u64>,
        target_page_range: std::ops::Range<u64>,
        structural_ranges: Option<ExactBaseStructuralRanges>,
    ) -> Result<Self, CandidateHostError> {
        let base_authority = base.authority();
        let target_authority = target.authority();
        if base_authority.document != target_authority.document
            || base_authority.syntax_profile != target_authority.syntax_profile
            || base_authority.publication == target_authority.publication
            || base_authority.parse_generation >= target_authority.parse_generation
            || base_authority.source_revision >= target_authority.source_revision
            || base_page_range.start > base_page_range.end
            || target_page_range.start > target_page_range.end
            || base_page_range.start != target_page_range.start
        {
            return Err(CandidateHostError::BaseMismatch);
        }

        let base_descriptor = decode_manifest_descriptor(arena, base.root_id(), base_authority)?;
        let target_descriptor =
            decode_manifest_descriptor(arena, target.root_id(), target_authority)?;
        let references_index = role_index(CandidateRole::References);
        let base_references_wrapper = base_descriptor.children[references_index];
        let target_references_wrapper = target_descriptor.children[references_index];
        let base_references_root = arena.child_at(base_references_wrapper, 0)?;
        let target_references_root = arena.child_at(target_references_wrapper, 0)?;
        let reused_references = base_descriptor.metadata[references_index];
        if target_references_root != base_references_root
            || target_descriptor.metadata[references_index] != reused_references
        {
            return Err(CandidateHostError::BaseMismatch);
        }

        let base_source_facts =
            persistent_source_facts_manifest_role(arena, &base_descriptor, base_authority)?;
        let target_source_facts =
            persistent_source_facts_manifest_role(arena, &target_descriptor, target_authority)?;
        if base_page_range.end > base_source_facts.metadata.record_count
            || target_page_range.end > target_source_facts.metadata.record_count
        {
            return Err(CandidateHostError::InvalidFrame(
                "SourceFacts delta range exceeds its role",
            ));
        }
        let retained_pages = base_source_facts
            .metadata
            .record_count
            .checked_sub(base_page_range.end - base_page_range.start)
            .ok_or(CandidateHostError::InvalidFrame(
                "SourceFacts delta base range underflow",
            ))?;
        if retained_pages.checked_add(target_page_range.end - target_page_range.start)
            != Some(target_source_facts.metadata.record_count)
        {
            return Err(CandidateHostError::InvalidFrame(
                "SourceFacts delta page arithmetic changed",
            ));
        }
        let target_source_facts_descriptor: [u8; PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES] =
            target_source_facts
                .descriptor_bytes
                .try_into()
                .map_err(|_| {
                    CandidateHostError::InvalidFrame("invalid target SourceFacts descriptor")
                })?;

        let structural_splice = match structural_ranges {
            Some(ExactBaseStructuralRanges::Blocks {
                base: base_entry_range,
                target: target_entry_range,
            }) => {
                let base_blocks =
                    persistent_block_manifest_roles(arena, &base_descriptor, base_authority)?;
                let target_blocks =
                    persistent_block_manifest_roles(arena, &target_descriptor, target_authority)?;
                let base_plan = plan_persistent_m11_block_semantic_splice(
                    arena,
                    base_blocks.root,
                    base_blocks.claim,
                    base_entry_range.clone(),
                )?;
                let target_plan = plan_persistent_m11_block_semantic_splice(
                    arena,
                    target_blocks.root,
                    target_blocks.claim,
                    target_entry_range.clone(),
                )?;
                if base_entry_range.start != target_entry_range.start
                    || base_plan.storage_page_range.start != target_plan.storage_page_range.start
                {
                    return Err(CandidateHostError::InvalidFrame(
                        "block splice ranges do not share one semantic boundary",
                    ));
                }
                let green_index = role_index(CandidateRole::Green);
                let projection_index = role_index(CandidateRole::Projection);
                Some(ExactBaseStructuralSpliceEncodeState::Blocks(
                    ExactBaseBlockSpliceEncodeState {
                        claim: M11BlockSequenceHostSpliceClaim {
                            base_entry_range,
                            target_entry_range,
                            base_storage_range: base_plan.storage_page_range,
                            target_storage_range: target_plan.storage_page_range.clone(),
                            base_green: base_blocks.green,
                            base_projection: base_blocks.projection,
                            target_green: target_blocks.green,
                            target_projection: target_blocks.projection,
                        },
                        base_green_metadata: base_descriptor.metadata[green_index],
                        base_projection_metadata: base_descriptor.metadata[projection_index],
                        target_green_metadata: target_descriptor.metadata[green_index],
                        target_projection_metadata: target_descriptor.metadata[projection_index],
                        target_root: target_blocks.root,
                        virtual_ordinal: 0,
                        next_replacement_page: target_plan.storage_page_range.start,
                    },
                ))
            }
            Some(ExactBaseStructuralRanges::RecursiveGreen {
                base: base_event_range,
                target: target_event_range,
            }) => {
                let base_green = persistent_recursive_green_manifest_role(
                    arena,
                    &base_descriptor,
                    base_authority,
                )?;
                let target_green = persistent_recursive_green_manifest_role(
                    arena,
                    &target_descriptor,
                    target_authority,
                )?;
                let base_plan = plan_persistent_m11_recursive_green_semantic_splice(
                    arena,
                    base_green.root,
                    base_green.descriptor,
                    base_event_range.clone(),
                )?;
                let target_plan = plan_persistent_m11_recursive_green_semantic_splice(
                    arena,
                    target_green.root,
                    target_green.descriptor,
                    target_event_range.clone(),
                )?;
                if base_event_range.start != target_event_range.start
                    || base_plan.storage_page_range.start != target_plan.storage_page_range.start
                {
                    return Err(CandidateHostError::InvalidFrame(
                        "recursive Green splice ranges do not share one semantic boundary",
                    ));
                }
                let green_index = role_index(CandidateRole::Green);
                Some(ExactBaseStructuralSpliceEncodeState::RecursiveGreen(
                    ExactBaseRecursiveGreenSpliceEncodeState {
                        claim: M11RecursiveGreenHostSpliceClaim {
                            base_event_range,
                            target_event_range,
                            base_storage_range: base_plan.storage_page_range,
                            target_storage_range: target_plan.storage_page_range.clone(),
                            base_descriptor: base_green.descriptor,
                            target_descriptor: target_green.descriptor,
                        },
                        base_green_metadata: base_descriptor.metadata[green_index],
                        target_green_metadata: target_descriptor.metadata[green_index],
                        target_root: target_green.root,
                        virtual_ordinal: SNAPSHOT_ABSENT_VIRTUAL_ORDINAL,
                        next_replacement_page: target_plan.storage_page_range.start,
                    },
                ))
            }
            None => None,
        };

        let mut virtual_roots = Vec::new();
        virtual_roots
            .try_reserve_exact(2 + usize::from(structural_splice.is_some()))
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        virtual_roots.push(base_references_root);
        if let Some(root) = target_source_facts.root {
            virtual_roots.push(root);
        }
        let mut structural_splice = structural_splice;
        if let Some(structural) = structural_splice.as_mut() {
            match structural {
                ExactBaseStructuralSpliceEncodeState::Blocks(block) => {
                    block.virtual_ordinal = u64::try_from(virtual_roots.len()).map_err(|_| {
                        CandidateHostError::InvalidFrame("virtual ordinal overflow")
                    })?;
                    let root = block.target_root.ok_or(CandidateHostError::InvalidFrame(
                        "nonempty block splice target lost its root",
                    ))?;
                    virtual_roots.push(root);
                }
                ExactBaseStructuralSpliceEncodeState::RecursiveGreen(green) => {
                    if let Some(root) = green.target_root {
                        green.virtual_ordinal =
                            u64::try_from(virtual_roots.len()).map_err(|_| {
                                CandidateHostError::InvalidFrame("virtual ordinal overflow")
                            })?;
                        virtual_roots.push(root);
                    }
                }
            }
        }
        let next_replacement_page = target_page_range.start;
        Self::from_root(
            arena,
            target_authority,
            target.root_id(),
            SnapshotProgram::ExactBaseDelta,
            &virtual_roots,
            Some(ExactBaseDeltaEncodeState {
                base_authority,
                reused_references,
                base_source_facts: base_source_facts.metadata,
                target_source_facts: target_source_facts.metadata,
                target_source_facts_descriptor,
                base_page_range,
                target_page_range,
                target_source_facts_root: target_source_facts.root,
                next_replacement_page,
                structural_splice,
                replay_barrier_issued: false,
                replay_resumed: false,
            }),
        )
    }

    fn from_root(
        arena: &PageArena,
        authority: CandidateAuthority,
        root: ArenaId,
        program: SnapshotProgram,
        virtual_roots: &[ArenaId],
        exact_base_delta: Option<ExactBaseDeltaEncodeState>,
    ) -> Result<Self, CandidateHostError> {
        Ok(Self {
            authority,
            program,
            closure: ArenaClosureSnapshotEncoder::new(arena, Some(root), virtual_roots)?,
            exact_base_delta,
        })
    }

    pub(crate) fn begin_frame(&mut self) -> Result<Box<[u8]>, CandidateHostError> {
        let bytes = match self.program {
            SnapshotProgram::Full => encode_candidate_header(SNAPSHOT_BEGIN_TAG, self.authority),
            SnapshotProgram::ExactBaseReferences => {
                encode_candidate_header(SNAPSHOT_REFERENCES_DELTA_BEGIN_TAG, self.authority)
            }
            SnapshotProgram::ExactBaseDelta => encode_exact_base_delta_begin(
                self.authority,
                self.exact_base_delta
                    .as_ref()
                    .ok_or(CandidateHostError::InvalidFrame(
                        "exact-base encoder lost its program",
                    ))?,
            )?,
        };
        self.closure.begin(&bytes)?;
        Ok(bytes.into_boxed_slice())
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<CandidateSnapshotEncodePoll, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        if !self.closure.begun || self.closure.ended {
            return Err(CandidateHostError::Busy);
        }
        if self.program == SnapshotProgram::ExactBaseDelta {
            let delta = self
                .exact_base_delta
                .as_mut()
                .ok_or(CandidateHostError::InvalidFrame(
                    "exact-base encoder lost its program",
                ))?;
            if delta.next_replacement_page < delta.target_page_range.end {
                let ordinal = delta.next_replacement_page;
                let payload = persistent_source_facts_leaf_record_at(
                    arena,
                    delta.target_source_facts_root,
                    ordinal,
                )?
                .ok_or(CandidateHostError::InvalidFrame(
                    "target SourceFacts replacement page disappeared",
                ))?;
                let frame = encode_source_facts_replacement_frame(ordinal, payload)?;
                delta.next_replacement_page =
                    ordinal
                        .checked_add(1)
                        .ok_or(CandidateHostError::InvalidFrame(
                            "replacement page ordinal overflow",
                        ))?;
                self.closure.payload_bytes =
                    self.closure
                        .payload_bytes
                        .checked_add(u64::try_from(payload.len()).map_err(|_| {
                            CandidateHostError::InvalidFrame("payload length overflow")
                        })?)
                        .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
                self.closure.wire_bytes =
                    self.closure
                        .wire_bytes
                        .checked_add(u64::try_from(frame.len()).map_err(|_| {
                            CandidateHostError::InvalidFrame("wire length overflow")
                        })?)
                        .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
                self.closure.digest.update(&frame);
                return Ok(CandidateSnapshotEncodePoll::Frame {
                    transitions: 1,
                    bytes: frame.into_boxed_slice(),
                });
            }
            let structural_frame = match delta.structural_splice.as_mut() {
                Some(ExactBaseStructuralSpliceEncodeState::Blocks(block))
                    if block.next_replacement_page < block.claim.target_storage_range.end =>
                {
                    let ordinal = block.next_replacement_page;
                    let payload =
                        persistent_m11_block_storage_page_at(arena, block.target_root, ordinal)?
                            .ok_or(CandidateHostError::InvalidFrame(
                                "target block replacement page disappeared",
                            ))?;
                    block.next_replacement_page =
                        ordinal
                            .checked_add(1)
                            .ok_or(CandidateHostError::InvalidFrame(
                                "block replacement page ordinal overflow",
                            ))?;
                    Some((payload, encode_block_replacement_frame(ordinal, payload)?))
                }
                Some(ExactBaseStructuralSpliceEncodeState::RecursiveGreen(green))
                    if green.next_replacement_page < green.claim.target_storage_range.end =>
                {
                    let ordinal = green.next_replacement_page;
                    let payload = persistent_m11_recursive_green_storage_page_at(
                        arena,
                        green.target_root,
                        ordinal,
                    )?
                    .ok_or(CandidateHostError::InvalidFrame(
                        "target recursive Green replacement page disappeared",
                    ))?;
                    green.next_replacement_page =
                        ordinal
                            .checked_add(1)
                            .ok_or(CandidateHostError::InvalidFrame(
                                "recursive Green replacement page ordinal overflow",
                            ))?;
                    Some((
                        payload,
                        encode_recursive_green_replacement_frame(ordinal, payload)?,
                    ))
                }
                _ => None,
            };
            if let Some((payload, frame)) = structural_frame {
                self.closure.payload_bytes =
                    self.closure
                        .payload_bytes
                        .checked_add(u64::try_from(payload.len()).map_err(|_| {
                            CandidateHostError::InvalidFrame("payload length overflow")
                        })?)
                        .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
                self.closure.wire_bytes =
                    self.closure
                        .wire_bytes
                        .checked_add(u64::try_from(frame.len()).map_err(|_| {
                            CandidateHostError::InvalidFrame("wire length overflow")
                        })?)
                        .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
                self.closure.digest.update(&frame);
                return Ok(CandidateSnapshotEncodePoll::Frame {
                    transitions: 1,
                    bytes: frame.into_boxed_slice(),
                });
            }
            if !delta.replay_barrier_issued {
                delta.replay_barrier_issued = true;
                return Ok(CandidateSnapshotEncodePoll::ReplayRequired { transitions: 0 });
            }
            if !delta.replay_resumed {
                return Err(CandidateHostError::Busy);
            }
        }
        match self.closure.poll(arena, fuel)? {
            ArenaClosureSnapshotEncodePoll::Pending { transitions } => {
                Ok(CandidateSnapshotEncodePoll::Pending { transitions })
            }
            ArenaClosureSnapshotEncodePoll::Frame { transitions, bytes } => {
                Ok(CandidateSnapshotEncodePoll::Frame { transitions, bytes })
            }
            ArenaClosureSnapshotEncodePoll::Complete { transitions, bytes } => {
                Ok(CandidateSnapshotEncodePoll::Complete { transitions, bytes })
            }
        }
    }

    pub(crate) fn resume_exact_base_delta(&mut self) -> Result<(), CandidateHostError> {
        if self.program != SnapshotProgram::ExactBaseDelta
            || !self.closure.begun
            || self.closure.ended
        {
            return Err(CandidateHostError::Busy);
        }
        let delta = self
            .exact_base_delta
            .as_mut()
            .ok_or(CandidateHostError::InvalidFrame(
                "exact-base encoder lost its program",
            ))?;
        if !delta.replay_barrier_issued
            || delta.replay_resumed
            || delta.next_replacement_page != delta.target_page_range.end
            || delta
                .structural_splice
                .as_ref()
                .is_some_and(|structural| match structural {
                    ExactBaseStructuralSpliceEncodeState::Blocks(block) => {
                        block.next_replacement_page != block.claim.target_storage_range.end
                    }
                    ExactBaseStructuralSpliceEncodeState::RecursiveGreen(green) => {
                        green.next_replacement_page != green.claim.target_storage_range.end
                    }
                })
        {
            return Err(CandidateHostError::Busy);
        }
        delta.replay_resumed = true;
        Ok(())
    }
}

impl ArenaClosureSnapshotEncoder {
    pub(crate) fn new(
        arena: &PageArena,
        root: Option<ArenaId>,
        virtual_roots: &[ArenaId],
    ) -> Result<Self, CandidateHostError> {
        let mut stack = Vec::new();
        if let Some(root) = root {
            stack
                .try_reserve_exact(1)
                .map_err(|_| CandidateHostError::AllocationFailed)?;
            stack.push(EncodeVisit {
                id: root,
                next_child: 0,
                child_count: arena.child_count(root)?,
            });
        }
        let mut visits = HashMap::new();
        visits
            .try_reserve(usize::from(root.is_some()) + virtual_roots.len())
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        for (ordinal, virtual_root) in virtual_roots.iter().copied().enumerate() {
            if Some(virtual_root) == root {
                return Err(CandidateHostError::InvalidFrame(
                    "virtual canonical root aliases the transported root",
                ));
            }
            if visits
                .insert(
                    virtual_root,
                    EncodeVisitState::Emitted(u64::try_from(ordinal).map_err(|_| {
                        CandidateHostError::InvalidFrame("virtual ordinal overflow")
                    })?),
                )
                .is_some()
            {
                return Err(CandidateHostError::InvalidFrame(
                    "virtual canonical roots alias each other",
                ));
            }
        }
        if let Some(root) = root {
            visits.insert(root, EncodeVisitState::Visiting);
        }
        Ok(Self {
            stack,
            visits,
            pending_roots: Vec::new(),
            synthetic_root: None,
            literal_root: None,
            next_ordinal: u64::try_from(virtual_roots.len())
                .map_err(|_| CandidateHostError::InvalidFrame("virtual ordinal overflow"))?,
            payload_bytes: 0,
            wire_bytes: 0,
            digest: snapshot_hasher(),
            begun: false,
            ended: false,
        })
    }

    /// Transports a small ordered forest beneath one synthetic terminal root.
    ///
    /// The synthetic node exists only in the imported arena; its payload and
    /// ordered child edges are covered by the ordinary closure digest. This
    /// lets typed sidecars transport paired roots without exposing producer
    /// arena identities or inventing a second Node/End protocol.
    pub(crate) fn new_bundle(
        arena: &PageArena,
        roots: &[ArenaId],
        synthetic_payload: Box<[u8]>,
    ) -> Result<Self, CandidateHostError> {
        if synthetic_payload.is_empty()
            || synthetic_payload.len() > ARENA_PAGE_BYTES
            || roots.len() > M11_MAXIMUM_SNAPSHOT_CHILDREN
        {
            return Err(CandidateHostError::InvalidFrame(
                "synthetic closure bundle exceeds its envelope",
            ));
        }
        let mut visits = HashMap::new();
        visits
            .try_reserve(roots.len())
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        for root in roots {
            if visits.insert(*root, EncodeVisitState::Visiting).is_some() {
                return Err(CandidateHostError::InvalidFrame(
                    "synthetic closure roots alias each other",
                ));
            }
        }
        // Only the currently traversed root is visiting. Pending roots are
        // removed so DAG sharing discovered from the first root can become an
        // already-emitted root instead of a false cycle.
        visits.clear();
        let mut stack = Vec::new();
        let mut pending_roots = roots.to_vec();
        pending_roots.reverse();
        if let Some(root) = pending_roots.pop() {
            visits.insert(root, EncodeVisitState::Visiting);
            stack.push(EncodeVisit {
                id: root,
                next_child: 0,
                child_count: arena.child_count(root)?,
            });
        }
        Ok(Self {
            stack,
            visits,
            pending_roots,
            synthetic_root: Some((roots.to_vec(), synthetic_payload)),
            literal_root: None,
            next_ordinal: 0,
            payload_bytes: 0,
            wire_bytes: 0,
            digest: snapshot_hasher(),
            begun: false,
            ended: false,
        })
    }

    /// Creates the same one-root closure program for a bounded typed terminal
    /// record that is already owned as canonical bytes rather than as an arena
    /// node. The host still imports and seals it in its independent arena.
    pub(crate) fn new_literal(payload: Box<[u8]>) -> Result<Self, CandidateHostError> {
        if payload.is_empty() || payload.len() > ARENA_PAGE_BYTES {
            return Err(CandidateHostError::InvalidFrame(
                "literal closure root exceeds its envelope",
            ));
        }
        Ok(Self {
            stack: Vec::new(),
            visits: HashMap::new(),
            pending_roots: Vec::new(),
            synthetic_root: None,
            literal_root: Some(payload),
            next_ordinal: 0,
            payload_bytes: 0,
            wire_bytes: 0,
            digest: snapshot_hasher(),
            begun: false,
            ended: false,
        })
    }

    pub(crate) fn begin(&mut self, frame: &[u8]) -> Result<(), CandidateHostError> {
        if self.begun || self.ended {
            return Err(CandidateHostError::Busy);
        }
        self.digest.update(frame);
        self.wire_bytes = u64::try_from(frame.len())
            .map_err(|_| CandidateHostError::InvalidFrame("begin frame length overflow"))?;
        self.begun = true;
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<ArenaClosureSnapshotEncodePoll, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        if !self.begun || self.ended {
            return Err(CandidateHostError::Busy);
        }
        if let Some(payload) = self.literal_root.take() {
            let frame = encode_node_frame(0, &[], &payload)?;
            self.digest.update(&frame);
            self.next_ordinal = 1;
            self.payload_bytes = u64::try_from(payload.len())
                .map_err(|_| CandidateHostError::InvalidFrame("payload length overflow"))?;
            self.wire_bytes = self
                .wire_bytes
                .checked_add(
                    u64::try_from(frame.len())
                        .map_err(|_| CandidateHostError::InvalidFrame("wire length overflow"))?,
                )
                .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
            return Ok(ArenaClosureSnapshotEncodePoll::Frame {
                transitions: 1,
                bytes: frame.into_boxed_slice(),
            });
        }
        let mut transitions = 0;
        while transitions < fuel {
            let Some(visit) = self.stack.last_mut() else {
                if let Some(root) = self.pending_roots.pop() {
                    match self.visits.get(&root).copied() {
                        Some(EncodeVisitState::Emitted(_)) => {}
                        Some(EncodeVisitState::Visiting) => {
                            return Err(CandidateHostError::InvalidFrame(
                                "synthetic closure roots contain a cycle",
                            ));
                        }
                        None => {
                            self.visits.insert(root, EncodeVisitState::Visiting);
                            self.stack.push(EncodeVisit {
                                id: root,
                                next_child: 0,
                                child_count: arena.child_count(root)?,
                            });
                        }
                    }
                    transitions += 1;
                    continue;
                }
                if let Some((children, payload)) = self.synthetic_root.take() {
                    let mut child_ordinals = Vec::new();
                    child_ordinals
                        .try_reserve_exact(children.len())
                        .map_err(|_| CandidateHostError::AllocationFailed)?;
                    for child in children {
                        let Some(EncodeVisitState::Emitted(ordinal)) =
                            self.visits.get(&child).copied()
                        else {
                            return Err(CandidateHostError::InvalidFrame(
                                "synthetic closure child was not emitted",
                            ));
                        };
                        child_ordinals.push(ordinal);
                    }
                    let frame = encode_node_frame(self.next_ordinal, &child_ordinals, &payload)?;
                    self.digest.update(&frame);
                    self.next_ordinal = self
                        .next_ordinal
                        .checked_add(1)
                        .ok_or(CandidateHostError::InvalidFrame("node count overflow"))?;
                    self.payload_bytes = self
                        .payload_bytes
                        .checked_add(u64::try_from(payload.len()).map_err(|_| {
                            CandidateHostError::InvalidFrame("payload length overflow")
                        })?)
                        .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
                    self.wire_bytes = self
                        .wire_bytes
                        .checked_add(u64::try_from(frame.len()).map_err(|_| {
                            CandidateHostError::InvalidFrame("wire length overflow")
                        })?)
                        .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
                    transitions += 1;
                    return Ok(ArenaClosureSnapshotEncodePoll::Frame {
                        transitions,
                        bytes: frame.into_boxed_slice(),
                    });
                }
                let bytes = encode_end_frame(
                    self.next_ordinal,
                    self.payload_bytes,
                    self.wire_bytes,
                    *self.digest.finalize().as_bytes(),
                );
                self.ended = true;
                return Ok(ArenaClosureSnapshotEncodePoll::Complete {
                    transitions,
                    bytes: bytes.into_boxed_slice(),
                });
            };
            if visit.next_child < visit.child_count {
                let child = arena.child_at(visit.id, visit.next_child)?;
                visit.next_child += 1;
                match self.visits.get(&child).copied() {
                    Some(EncodeVisitState::Emitted(_)) => {}
                    Some(EncodeVisitState::Visiting) => {
                        return Err(CandidateHostError::InvalidFrame(
                            "arena closure contains a cycle",
                        ));
                    }
                    None => {
                        self.stack
                            .try_reserve(1)
                            .map_err(|_| CandidateHostError::AllocationFailed)?;
                        self.visits
                            .try_reserve(1)
                            .map_err(|_| CandidateHostError::AllocationFailed)?;
                        self.visits.insert(child, EncodeVisitState::Visiting);
                        self.stack.push(EncodeVisit {
                            id: child,
                            next_child: 0,
                            child_count: arena.child_count(child)?,
                        });
                    }
                }
                transitions += 1;
                continue;
            }

            let visit = self.stack.pop().expect("nonempty traversal stack");
            let payload = arena.payload(visit.id)?;
            let mut child_ordinals = Vec::new();
            child_ordinals
                .try_reserve_exact(visit.child_count)
                .map_err(|_| CandidateHostError::AllocationFailed)?;
            for index in 0..visit.child_count {
                let child = arena.child_at(visit.id, index)?;
                let Some(EncodeVisitState::Emitted(ordinal)) = self.visits.get(&child).copied()
                else {
                    return Err(CandidateHostError::InvalidFrame(
                        "postorder child was not emitted",
                    ));
                };
                child_ordinals.push(ordinal);
            }
            let frame = encode_node_frame(self.next_ordinal, &child_ordinals, payload)?;
            self.digest.update(&frame);
            self.visits
                .insert(visit.id, EncodeVisitState::Emitted(self.next_ordinal));
            self.next_ordinal = self
                .next_ordinal
                .checked_add(1)
                .ok_or(CandidateHostError::InvalidFrame("node count overflow"))?;
            self.payload_bytes = self
                .payload_bytes
                .checked_add(
                    u64::try_from(payload.len())
                        .map_err(|_| CandidateHostError::InvalidFrame("payload length overflow"))?,
                )
                .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
            self.wire_bytes = self
                .wire_bytes
                .checked_add(
                    u64::try_from(frame.len())
                        .map_err(|_| CandidateHostError::InvalidFrame("wire length overflow"))?,
                )
                .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
            transitions += 1;
            return Ok(ArenaClosureSnapshotEncodePoll::Frame {
                transitions,
                bytes: frame.into_boxed_slice(),
            });
        }
        Ok(ArenaClosureSnapshotEncodePoll::Pending { transitions })
    }
}

/// Generic host-side receiver for the shared Node/End arena program.
///
/// Typed callers own Begin decoding and payload-schema validation. This state
/// owns only the independent arena journal, strict postorder ordinal graph,
/// stream metrics, connected-closure proof, and fuelled seal.
pub(crate) struct ArenaClosureSnapshotReceiver {
    build: Option<CandidateBuild>,
    nodes: Vec<ArenaBuildOwner>,
    incoming_edges: Vec<u64>,
    node_count: u64,
    payload_bytes: u64,
    wire_bytes: u64,
    digest: blake3::Hasher,
    phase: ArenaClosureReceivePhase,
}

enum ArenaClosureReceivePhase {
    Receiving,
    Checking {
        claim: SnapshotEndClaim,
        next_index: usize,
    },
    Ready,
    Sealing(CandidateSeal),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArenaClosureCheckPoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

impl ArenaClosureSnapshotReceiver {
    pub(crate) fn new(begin_frame: &[u8]) -> Result<Self, CandidateHostError> {
        let wire_bytes = u64::try_from(begin_frame.len())
            .map_err(|_| CandidateHostError::InvalidFrame("begin frame length overflow"))?;
        let mut digest = snapshot_hasher();
        digest.update(begin_frame);
        Ok(Self {
            build: None,
            nodes: Vec::new(),
            incoming_edges: Vec::new(),
            node_count: 0,
            payload_bytes: 0,
            wire_bytes,
            digest,
            phase: ArenaClosureReceivePhase::Receiving,
        })
    }

    pub(crate) fn offer_node<F>(
        &mut self,
        arena: &mut PageArena,
        limits: CandidateHostLimits,
        frame: &[u8],
        validate: F,
    ) -> Result<(), CandidateHostError>
    where
        F: FnOnce(&PageArena, ArenaId, &[u8]) -> Result<(), CandidateHostError>,
    {
        if !matches!(self.phase, ArenaClosureReceivePhase::Receiving) {
            return Err(CandidateHostError::Busy);
        }
        let node = decode_node_frame(frame, self.node_count)?;
        if node.payload.len() > ARENA_PAGE_BYTES
            || node.child_count > limits.arena.max_children_per_node
        {
            return Err(CandidateHostError::InvalidFrame(
                "node payload or child arity exceeds its envelope",
            ));
        }
        let next_nodes = self
            .node_count
            .checked_add(1)
            .ok_or(CandidateHostError::InvalidFrame("node count overflow"))?;
        let next_payload = self
            .payload_bytes
            .checked_add(node.payload.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
        let next_wire = self
            .wire_bytes
            .checked_add(frame.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
        if next_nodes > limits.maximum_snapshot_nodes
            || next_wire > limits.maximum_snapshot_wire_bytes
        {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot exceeds its declared host envelope",
            ));
        }

        let mut child_ids = Vec::new();
        child_ids
            .try_reserve_exact(node.child_count)
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        for index in 0..node.child_count {
            let ordinal = node.child_ordinal(index)?;
            if ordinal >= self.node_count {
                return Err(CandidateHostError::InvalidFrame(
                    "node child is not a strict backward stream reference",
                ));
            }
            let child_index = usize::try_from(ordinal)
                .map_err(|_| CandidateHostError::InvalidFrame("child ordinal overflow"))?;
            child_ids.push(self.nodes[child_index].id());
            self.incoming_edges[child_index].checked_add(1).ok_or(
                CandidateHostError::InvalidFrame("incoming edge count overflow"),
            )?;
        }
        self.nodes
            .try_reserve(1)
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        self.incoming_edges
            .try_reserve(1)
            .map_err(|_| CandidateHostError::AllocationFailed)?;

        let mut session = if let Some(build) = self.build.take() {
            arena.validate_suspended_build(&build)?;
            arena
                .resume_build(build)
                .expect("prevalidated closure build must resume")
        } else {
            arena.begin_build()?
        };
        let parent = session.allocate(node.payload, &child_ids)?;
        let validation = validate(session.arena(), parent.id(), node.payload);
        self.build = Some(session.suspend()?);
        validation?;

        for index in 0..node.child_count {
            let child_index = usize::try_from(node.child_ordinal(index)?)
                .map_err(|_| CandidateHostError::InvalidFrame("child ordinal overflow"))?;
            self.incoming_edges[child_index] += 1;
        }
        self.nodes.push(parent);
        self.incoming_edges.push(0);
        self.node_count = next_nodes;
        self.payload_bytes = next_payload;
        self.wire_bytes = next_wire;
        self.digest.update(frame);
        Ok(())
    }

    pub(crate) fn finish(
        &mut self,
        frame: &[u8],
        allow_empty: bool,
    ) -> Result<(), CandidateHostError> {
        if !matches!(self.phase, ArenaClosureReceivePhase::Receiving) {
            return Err(CandidateHostError::Busy);
        }
        let claim = decode_end_frame(frame)?;
        let node_count = usize::try_from(self.node_count)
            .map_err(|_| CandidateHostError::InvalidFrame("node count overflow"))?;
        if claim.nodes != self.node_count
            || claim.payload_bytes != self.payload_bytes
            || claim.wire_bytes != self.wire_bytes
            || claim.digest != *self.digest.finalize().as_bytes()
            || (!allow_empty && node_count == 0)
            || self.nodes.len() != node_count
            || self.incoming_edges.len() != node_count
            || (node_count == 0) != self.build.is_none()
        {
            return Err(CandidateHostError::InvalidFrame(
                "truncated closure or snapshot digest mismatch",
            ));
        }
        self.phase = ArenaClosureReceivePhase::Checking {
            claim,
            next_index: 0,
        };
        Ok(())
    }

    pub(crate) fn poll_check(
        &mut self,
        fuel: usize,
        virtual_root_count: usize,
    ) -> Result<ArenaClosureCheckPoll, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        let ArenaClosureReceivePhase::Checking { claim, next_index } = &mut self.phase else {
            return Err(CandidateHostError::Busy);
        };
        let node_count = usize::try_from(claim.nodes)
            .map_err(|_| CandidateHostError::InvalidFrame("node count overflow"))?;
        if virtual_root_count > node_count {
            return Err(CandidateHostError::InvalidFrame(
                "virtual-root count exceeds closure",
            ));
        }
        let mut transitions = 0;
        while *next_index < node_count && transitions < fuel {
            let incoming = self.incoming_edges[*next_index];
            let is_root = *next_index == node_count - 1;
            let is_virtual = *next_index < virtual_root_count;
            if (is_root && incoming != 0)
                || (is_virtual && incoming != 1)
                || (!is_root && !is_virtual && incoming == 0)
            {
                return Err(CandidateHostError::InvalidFrame(
                    "snapshot stream is not one connected root closure",
                ));
            }
            *next_index += 1;
            transitions += 1;
        }
        let complete = *next_index == node_count;
        if complete {
            self.phase = ArenaClosureReceivePhase::Ready;
        }
        Ok(ArenaClosureCheckPoll {
            transitions,
            complete,
        })
    }

    pub(crate) fn root_id(&self) -> Result<Option<ArenaId>, CandidateHostError> {
        if !matches!(self.phase, ArenaClosureReceivePhase::Ready) {
            return Err(CandidateHostError::Busy);
        }
        Ok(self.nodes.last().map(ArenaBuildOwner::id))
    }

    pub(crate) fn begin_seal(&mut self, arena: &mut PageArena) -> Result<bool, CandidateHostError> {
        if !matches!(self.phase, ArenaClosureReceivePhase::Ready) {
            return Err(CandidateHostError::Busy);
        }
        if self.nodes.is_empty() {
            self.phase = ArenaClosureReceivePhase::Complete;
            return Ok(true);
        }
        let build = self.build.take().ok_or(CandidateHostError::InvalidFrame(
            "validated closure lost its build capability",
        ))?;
        let root = self.nodes.pop().ok_or(CandidateHostError::InvalidFrame(
            "validated closure lost its root",
        ))?;
        self.nodes.clear();
        self.incoming_edges.clear();
        let seal = match arena.begin_seal(build, root) {
            Ok(seal) => seal,
            Err(failure) => {
                arena.abort_build(failure.build)?;
                return Err(failure.error.into());
            }
        };
        self.phase = ArenaClosureReceivePhase::Sealing(seal);
        Ok(false)
    }

    pub(crate) fn poll_seal(
        &mut self,
        arena: &mut PageArena,
        fuel: usize,
    ) -> Result<Option<CommittedArenaRoot>, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        let ArenaClosureReceivePhase::Sealing(seal) = &mut self.phase else {
            return Err(CandidateHostError::Busy);
        };
        let receipt = arena.poll_seal(seal, fuel)?;
        if receipt.root.is_some() {
            self.phase = ArenaClosureReceivePhase::Complete;
        }
        Ok(receipt.root)
    }

    pub(crate) fn abort(mut self, arena: &mut PageArena) -> Result<(), CandidateHostError> {
        match self.phase {
            ArenaClosureReceivePhase::Sealing(seal) => arena.abort_seal(seal)?,
            ArenaClosureReceivePhase::Receiving
            | ArenaClosureReceivePhase::Checking { .. }
            | ArenaClosureReceivePhase::Ready => {
                if let Some(build) = self.build.take() {
                    arena.abort_build(build)?;
                }
            }
            ArenaClosureReceivePhase::Complete => {}
        }
        Ok(())
    }
}

pub(crate) struct CandidateHostStore {
    arena: PageArena,
    limits: CandidateHostLimits,
    document: StrongIdentity,
    current_source: SourceVersion,
    syntax_profile: u32,
    installed: Option<InstalledRoot>,
    active: Option<ActiveOffer>,
    closing: bool,
    closed: bool,
    _not_sync: PhantomData<Cell<()>>,
}

struct InstalledRoot {
    authority: CandidateAuthority,
    source: SourceVersion,
    root: CommittedArenaRoot,
    persistent_blocks: Option<InstalledPersistentBlockRoot>,
    persistent_recursive_green: Option<InstalledPersistentRecursiveGreenRoot>,
}

#[derive(Clone, Copy)]
struct InstalledPersistentBlockRoot {
    root: Option<ArenaId>,
    claim: PersistentM11BlockRootClaim,
}

#[derive(Clone, Copy)]
struct InstalledPersistentRecursiveGreenRoot {
    root: Option<ArenaId>,
    claim: PersistentM11RecursiveGreenRootClaim,
}

struct ActiveOffer {
    authority: CandidateAuthority,
    program: SnapshotProgram,
    reused_references: Option<ReusedReferences>,
    exact_base_delta: Option<ExactBaseDeltaHostState>,
    build: Option<CandidateBuild>,
    nodes: Vec<ArenaBuildOwner>,
    incoming_edges: Vec<u64>,
    node_count: u64,
    payload_bytes: u64,
    wire_bytes: u64,
    digest: blake3::Hasher,
    phase: HostOfferPhase,
}

#[derive(Clone, Copy)]
struct ReusedReferences {
    canonical_root: ArenaId,
    metadata: RoleMetadata,
}

struct ExactBaseDeltaHostState {
    replay: PersistentSourceFactsHostReplay,
    target_source_facts_root: Option<ArenaId>,
    target_source_facts_present: bool,
    target_source_facts_metadata: RoleMetadata,
    target_source_facts_descriptor: [u8; PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES],
    next_replacement_page: u64,
    replacement_page_end: u64,
    virtual_root_count: usize,
    replay_work: Option<PersistentSourceFactsHostReplayWork>,
    structural_splice: Option<ExactBaseStructuralSpliceHostState>,
}

enum ExactBaseStructuralSpliceHostState {
    Blocks(ExactBaseBlockSpliceHostState),
    RecursiveGreen(ExactBaseRecursiveGreenSpliceHostState),
}

enum ExactBaseStructuralReplay {
    Blocks(M11BlockSequenceHostReplay),
    RecursiveGreen(M11RecursiveGreenHostReplay),
}

enum ExactBaseStructuralHostSetup {
    Blocks {
        base_root: Option<ArenaId>,
        claim: M11BlockSequenceHostSpliceClaim,
    },
    RecursiveGreen {
        base_root: Option<ArenaId>,
        claim: M11RecursiveGreenHostSpliceClaim,
    },
}

struct ExactBaseBlockSpliceHostState {
    replay: M11BlockSequenceHostReplay,
    target_root: Option<ArenaId>,
    target_green_metadata: RoleMetadata,
    target_projection_metadata: RoleMetadata,
    target_green: PersistentM11BlockRoleDescriptor,
    target_projection: PersistentM11BlockRoleDescriptor,
    next_replacement_page: u64,
    replacement_page_end: u64,
    work: Option<M11BlockSequenceHostSpliceWork>,
}

struct ExactBaseRecursiveGreenSpliceHostState {
    replay: M11RecursiveGreenHostReplay,
    target_root: Option<ArenaId>,
    target_green_metadata: RoleMetadata,
    target_descriptor: PersistentM11RecursiveGreenRoleDescriptor,
    next_replacement_page: u64,
    replacement_page_end: u64,
    work: Option<M11RecursiveGreenHostSpliceWork>,
}

enum HostOfferPhase {
    ReceivingReplacementPages,
    AdvancingReplacementPage,
    FinishingReplacement,
    CompletingReplay,
    ReceivingBlockReplacementPages,
    AdvancingBlockReplacementPage,
    FinishingBlockReplacement,
    CompletingBlockReplay,
    ReceivingRecursiveGreenReplacementPages,
    AdvancingRecursiveGreenReplacementPage,
    FinishingRecursiveGreenReplacement,
    CompletingRecursiveGreenReplay,
    Receiving,
    CheckingClosure {
        claim: SnapshotEndClaim,
        next_index: usize,
    },
    ValidatingProjection {
        validator: Box<PersistentM11InlineProjectionHostValidator>,
        references: Option<Box<ReferenceRoleDigestValidator>>,
    },
    Validating(Box<ReferenceRoleDigestValidator>),
    Sealing(CandidateSeal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CandidateHostReplayPoll {
    pub(crate) transitions: usize,
    pub(crate) ready_for_replacement_page: bool,
    pub(crate) ready_for_nodes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CandidateHostInstallPoll {
    pub(crate) transitions: usize,
    pub(crate) installed: Option<InstalledCandidateSnapshot>,
}

impl CandidateHostStore {
    pub(crate) fn new(
        document: StrongIdentity,
        current_source: SourceVersion,
        syntax_profile: u32,
        limits: CandidateHostLimits,
    ) -> Result<Self, CandidateHostError> {
        if syntax_profile == 0
            || limits.maximum_snapshot_nodes == 0
            || limits.maximum_snapshot_wire_bytes < CANDIDATE_HEADER_BYTES as u64
            || limits.maximum_query_bytes == 0
        {
            return Err(CandidateHostError::InvalidLimits);
        }
        Ok(Self {
            arena: PageArena::new(limits.arena)?,
            limits,
            document,
            current_source,
            syntax_profile,
            installed: None,
            active: None,
            closing: false,
            closed: false,
            _not_sync: PhantomData,
        })
    }

    /// Advances the exact externally authenticated source authority without
    /// withdrawing the last installed structural root.
    ///
    /// Any staging offer for the prior source is synchronously detached from
    /// admission and its arena journal is moved to the ordinary fuelled
    /// reclaim path. Skipped source revisions are valid because the UI owner
    /// may coalesce edits before the next parser candidate.
    pub(crate) fn observe_source_version(
        &mut self,
        source: SourceVersion,
    ) -> Result<bool, CandidateHostError> {
        if self.closing || self.closed {
            return Err(CandidateHostError::Busy);
        }
        if source.revision() < self.current_source.revision() {
            return Err(CandidateHostError::StaleCandidate);
        }
        if source.revision() == self.current_source.revision() {
            return if source == self.current_source {
                Ok(false)
            } else {
                Err(CandidateHostError::CrossAuthority)
            };
        }
        if let Some(active) = self.active.take() {
            self.abort_active(active)?;
        }
        self.current_source = source;
        Ok(true)
    }

    /// Rebinds candidate admission to a replacement parser replica root for
    /// the same exact source revision and dimensions.
    ///
    /// The caller must separately authenticate content identity. This narrow
    /// seam deliberately leaves the installed root and its original replica
    /// authority intact while allowing a recovery worker's self-contained
    /// snapshot to name its newly allocated source root.
    pub(crate) fn rebind_source_replica(
        &mut self,
        source: SourceVersion,
    ) -> Result<(), CandidateHostError> {
        if self.closing || self.closed || self.active.is_some() {
            return Err(CandidateHostError::Busy);
        }
        if source.revision() != self.current_source.revision()
            || source.byte_len() != self.current_source.byte_len()
            || source.utf16_len() != self.current_source.utf16_len()
        {
            return Err(CandidateHostError::CrossAuthority);
        }
        self.current_source = source;
        Ok(())
    }

    pub(crate) fn begin_snapshot(&mut self, frame: &[u8]) -> Result<(), CandidateHostError> {
        self.begin_snapshot_program(frame, SnapshotProgram::Full, None)
    }

    /// Begins a target program that reuses only the canonical References root
    /// of the exact currently installed candidate.
    pub(crate) fn begin_references_delta(
        &mut self,
        base: InstalledCandidateSnapshot,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if self.closing || self.closed || self.active.is_some() {
            return Err(CandidateHostError::Busy);
        }
        let installed = self
            .installed
            .as_ref()
            .ok_or(CandidateHostError::BaseMismatch)?;
        if installed.authority != base.authority {
            return Err(CandidateHostError::BaseMismatch);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        let index = role_index(CandidateRole::References);
        let wrapper = descriptor.children[index];
        let canonical_root = self.arena.child_at(wrapper, 0)?;
        let reused = ReusedReferences {
            canonical_root,
            metadata: descriptor.metadata[index],
        };
        self.begin_snapshot_program(
            frame,
            SnapshotProgram::ExactBaseReferences,
            Some((base, reused)),
        )
    }

    /// Begins a typed exact-base transaction bound to the currently installed
    /// capability. Both unchanged canonical roots are retained into one fresh
    /// journal before the host accepts any replacement or target node bytes.
    pub(crate) fn begin_exact_base_delta(
        &mut self,
        base: InstalledCandidateSnapshot,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if self.closing || self.closed || self.active.is_some() {
            return Err(CandidateHostError::Busy);
        }
        let installed = self
            .installed
            .as_ref()
            .ok_or(CandidateHostError::BaseMismatch)?;
        if installed.authority != base.authority {
            return Err(CandidateHostError::BaseMismatch);
        }
        let begin = decode_exact_base_delta_begin(frame)?;
        if begin.base_authority != installed.authority {
            return Err(CandidateHostError::BaseMismatch);
        }
        let authority = begin.authority;
        if authority.document != self.document
            || authority.source_root != self.current_source.root()
            || authority.source_revision != self.current_source.revision()
            || authority.source_bytes != self.current_source.byte_len() as u64
            || authority.source_utf16 != self.current_source.utf16_len() as u64
            || authority.syntax_profile != self.syntax_profile
        {
            return Err(CandidateHostError::CrossAuthority);
        }
        if authority.parse_generation <= installed.authority.parse_generation {
            return Err(CandidateHostError::StaleCandidate);
        }
        if authority.publication == installed.authority.publication
            || authority.source_revision <= installed.authority.source_revision
        {
            return Err(CandidateHostError::BaseMismatch);
        }

        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        let references_index = role_index(CandidateRole::References);
        let references_wrapper = descriptor.children[references_index];
        let references_root = self.arena.child_at(references_wrapper, 0)?;
        let reused_references = ReusedReferences {
            canonical_root: references_root,
            metadata: descriptor.metadata[references_index],
        };
        if begin.reused_references != reused_references.metadata {
            return Err(CandidateHostError::BaseMismatch);
        }

        let base_source_facts =
            persistent_source_facts_manifest_role(&self.arena, &descriptor, installed.authority)?;
        if begin.base_source_facts != base_source_facts.metadata {
            return Err(CandidateHostError::BaseMismatch);
        }
        if begin.base_page_range.end > begin.base_source_facts.record_count
            || begin.target_page_range.end > begin.target_source_facts.record_count
        {
            return Err(CandidateHostError::InvalidFrame(
                "exact-base SourceFacts range exceeds its role",
            ));
        }
        let retained_pages = begin
            .base_source_facts
            .record_count
            .checked_sub(begin.base_page_range.end - begin.base_page_range.start)
            .ok_or(CandidateHostError::InvalidFrame(
                "exact-base SourceFacts range underflow",
            ))?;
        if retained_pages.checked_add(begin.target_page_range.end - begin.target_page_range.start)
            != Some(begin.target_source_facts.record_count)
        {
            return Err(CandidateHostError::InvalidFrame(
                "exact-base SourceFacts page arithmetic changed",
            ));
        }
        let base_replay_descriptor = validate_persistent_source_facts_host_replay_descriptor(
            PersistentSourceFactsHostReplayDescriptorClaim {
                descriptor_bytes: base_source_facts.descriptor_bytes,
                record_count: base_source_facts.metadata.record_count,
                canonical_bytes: base_source_facts.metadata.canonical_bytes,
                digest: base_source_facts.metadata.digest,
                source_bytes: installed.authority.source_bytes,
                source_utf16: installed.authority.source_utf16,
            },
        )?;
        let base_source_facts_root = base_source_facts.root;
        let target_replay_descriptor = validate_persistent_source_facts_host_replay_descriptor(
            PersistentSourceFactsHostReplayDescriptorClaim {
                descriptor_bytes: &begin.target_source_facts_descriptor,
                record_count: begin.target_source_facts.record_count,
                canonical_bytes: begin.target_source_facts.canonical_bytes,
                digest: begin.target_source_facts.digest,
                source_bytes: authority.source_bytes,
                source_utf16: authority.source_utf16,
            },
        )?;

        let target_source_facts_present = begin.target_source_facts.record_count != 0;
        let structural_setup = match begin.structural_splice.as_ref() {
            Some(DecodedExactBaseStructuralSplice::Blocks(block)) => {
                let installed_blocks =
                    persistent_block_manifest_roles(&self.arena, &descriptor, installed.authority)?;
                let green_index = role_index(CandidateRole::Green);
                let projection_index = role_index(CandidateRole::Projection);
                if block.claim.base_green != installed_blocks.green
                    || block.claim.base_projection != installed_blocks.projection
                    || block.base_green_metadata != descriptor.metadata[green_index]
                    || block.base_projection_metadata != descriptor.metadata[projection_index]
                    || block.target_green_metadata.record_count
                        != block.claim.target_green.record_count()
                    || block.target_green_metadata.canonical_bytes
                        != block.claim.target_green.canonical_bytes()
                    || block.target_projection_metadata.record_count
                        != block.claim.target_projection.record_count()
                    || block.target_projection_metadata.canonical_bytes
                        != block.claim.target_projection.canonical_bytes()
                {
                    return Err(CandidateHostError::BaseMismatch);
                }
                Some(ExactBaseStructuralHostSetup::Blocks {
                    base_root: installed_blocks.root,
                    claim: block.claim.clone(),
                })
            }
            Some(DecodedExactBaseStructuralSplice::RecursiveGreen(green)) => {
                let installed_green = persistent_recursive_green_manifest_role(
                    &self.arena,
                    &descriptor,
                    installed.authority,
                )?;
                let green_index = role_index(CandidateRole::Green);
                if green.claim.base_descriptor != installed_green.descriptor
                    || green.base_green_metadata != descriptor.metadata[green_index]
                    || green.target_green_metadata.record_count
                        != green.claim.target_descriptor.record_count()
                    || green.target_green_metadata.canonical_bytes
                        != green.claim.target_descriptor.canonical_bytes()
                {
                    return Err(CandidateHostError::BaseMismatch);
                }
                Some(ExactBaseStructuralHostSetup::RecursiveGreen {
                    base_root: installed_green.root,
                    claim: green.claim.clone(),
                })
            }
            None => None,
        };
        let structural_target_present = match begin.structural_splice.as_ref() {
            Some(DecodedExactBaseStructuralSplice::Blocks(_)) => true,
            Some(DecodedExactBaseStructuralSplice::RecursiveGreen(green)) => {
                green.claim.target_descriptor.storage_page_count() != 0
            }
            None => false,
        };
        let expected_virtual_root_count =
            1 + usize::from(target_source_facts_present) + usize::from(structural_target_present);
        let expected_structural_virtual_ordinal =
            u64::try_from(1 + usize::from(target_source_facts_present))
                .map_err(|_| CandidateHostError::InvalidFrame("virtual ordinal overflow"))?;
        let structural_virtual_ordinal_valid = match begin.structural_splice.as_ref() {
            Some(DecodedExactBaseStructuralSplice::Blocks(block)) => {
                block.virtual_ordinal == expected_structural_virtual_ordinal
            }
            Some(DecodedExactBaseStructuralSplice::RecursiveGreen(green)) => {
                if structural_target_present {
                    green.virtual_ordinal == expected_structural_virtual_ordinal
                } else {
                    green.virtual_ordinal == SNAPSHOT_ABSENT_VIRTUAL_ORDINAL
                }
            }
            None => true,
        };
        if begin.virtual_root_count != expected_virtual_root_count
            || (target_source_facts_present && begin.source_facts_virtual_ordinal != 1)
            || (!target_source_facts_present
                && begin.source_facts_virtual_ordinal != SNAPSHOT_ABSENT_VIRTUAL_ORDINAL)
            || !structural_virtual_ordinal_valid
        {
            return Err(CandidateHostError::InvalidFrame(
                "exact-base virtual-root program changed",
            ));
        }

        let wire_bytes = u64::try_from(frame.len())
            .map_err(|_| CandidateHostError::InvalidFrame("begin length overflow"))?;
        if wire_bytes > self.limits.maximum_snapshot_wire_bytes {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot exceeds its declared host envelope",
            ));
        }
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(expected_virtual_root_count)
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        let mut incoming_edges = Vec::new();
        incoming_edges
            .try_reserve_exact(expected_virtual_root_count)
            .map_err(|_| CandidateHostError::AllocationFailed)?;

        let mut session = self.arena.begin_build()?;
        let setup = (|| -> Result<_, CandidateHostError> {
            let references_owner = session.retain(references_root)?;
            let base_source_facts_owner = match base_source_facts_root {
                Some(root) => Some(session.retain(root)?),
                None => None,
            };
            let replay = PersistentSourceFactsHostReplay::new(
                &session,
                base_source_facts_owner,
                base_replay_descriptor,
                begin.base_page_range.clone(),
                target_replay_descriptor,
            )?;
            let structural_replay = match structural_setup.as_ref() {
                Some(ExactBaseStructuralHostSetup::Blocks { base_root, claim }) => {
                    let base_owner = base_root.map(|root| session.retain(root)).transpose()?;
                    Some(ExactBaseStructuralReplay::Blocks(
                        M11BlockSequenceHostReplay::new(&session, base_owner, claim.clone())?,
                    ))
                }
                Some(ExactBaseStructuralHostSetup::RecursiveGreen { base_root, claim }) => {
                    let base_owner = base_root.map(|root| session.retain(root)).transpose()?;
                    Some(ExactBaseStructuralReplay::RecursiveGreen(
                        M11RecursiveGreenHostReplay::new(&session, base_owner, claim.clone())?,
                    ))
                }
                None => None,
            };
            Ok((references_owner, replay, structural_replay))
        })();
        let (references_owner, replay, structural_replay) = match setup {
            Ok(setup) => setup,
            Err(error) => {
                let build = session.suspend()?;
                self.arena.abort_build(build)?;
                return Err(error);
            }
        };
        nodes.push(references_owner);
        incoming_edges.push(0);
        let mut digest = snapshot_hasher();
        digest.update(frame);
        let structural_splice = match (begin.structural_splice, structural_replay) {
            (
                Some(DecodedExactBaseStructuralSplice::Blocks(block)),
                Some(ExactBaseStructuralReplay::Blocks(replay)),
            ) => Some(ExactBaseStructuralSpliceHostState::Blocks(
                ExactBaseBlockSpliceHostState {
                    replay,
                    target_root: None,
                    target_green_metadata: block.target_green_metadata,
                    target_projection_metadata: block.target_projection_metadata,
                    target_green: block.claim.target_green,
                    target_projection: block.claim.target_projection,
                    next_replacement_page: block.claim.target_storage_range.start,
                    replacement_page_end: block.claim.target_storage_range.end,
                    work: None,
                },
            )),
            (
                Some(DecodedExactBaseStructuralSplice::RecursiveGreen(green)),
                Some(ExactBaseStructuralReplay::RecursiveGreen(replay)),
            ) => Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(
                ExactBaseRecursiveGreenSpliceHostState {
                    replay,
                    target_root: None,
                    target_green_metadata: green.target_green_metadata,
                    target_descriptor: green.claim.target_descriptor,
                    next_replacement_page: green.claim.target_storage_range.start,
                    replacement_page_end: green.claim.target_storage_range.end,
                    work: None,
                },
            )),
            (None, None) => None,
            _ => {
                let build = session.suspend()?;
                self.arena.abort_build(build)?;
                return Err(CandidateHostError::InvalidFrame(
                    "exact-base structural replay changed kind",
                ));
            }
        };
        let build = session.suspend()?;
        self.active = Some(ActiveOffer {
            authority,
            program: SnapshotProgram::ExactBaseDelta,
            reused_references: Some(reused_references),
            exact_base_delta: Some(ExactBaseDeltaHostState {
                replay,
                target_source_facts_root: None,
                target_source_facts_present,
                target_source_facts_metadata: begin.target_source_facts,
                target_source_facts_descriptor: begin.target_source_facts_descriptor,
                next_replacement_page: begin.target_page_range.start,
                replacement_page_end: begin.target_page_range.end,
                virtual_root_count: expected_virtual_root_count,
                replay_work: None,
                structural_splice,
            }),
            build: Some(build),
            nodes,
            incoming_edges,
            node_count: 1,
            payload_bytes: 0,
            wire_bytes,
            digest,
            phase: HostOfferPhase::ReceivingReplacementPages,
        });
        Ok(())
    }

    fn begin_snapshot_program(
        &mut self,
        frame: &[u8],
        expected_program: SnapshotProgram,
        reused: Option<(InstalledCandidateSnapshot, ReusedReferences)>,
    ) -> Result<(), CandidateHostError> {
        if self.closing || self.closed || self.active.is_some() {
            return Err(CandidateHostError::Busy);
        }
        let begin = decode_begin_frame(frame)?;
        if begin.program != expected_program {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot Begin mode changed",
            ));
        }
        let authority = begin.authority;
        if authority.document != self.document
            || authority.source_root != self.current_source.root()
            || authority.source_revision != self.current_source.revision()
            || authority.source_bytes != self.current_source.byte_len() as u64
            || authority.source_utf16 != self.current_source.utf16_len() as u64
            || authority.syntax_profile != self.syntax_profile
        {
            return Err(CandidateHostError::CrossAuthority);
        }
        if self.installed.as_ref().is_some_and(|installed| {
            authority.parse_generation <= installed.authority.parse_generation
        }) {
            return Err(CandidateHostError::StaleCandidate);
        }
        let wire_bytes = u64::try_from(frame.len())
            .map_err(|_| CandidateHostError::InvalidFrame("begin length overflow"))?;

        let (build, nodes, incoming_edges, node_count, reused_references) =
            if let Some((base, reused)) = reused {
                if expected_program != SnapshotProgram::ExactBaseReferences
                    || authority.publication == base.authority.publication
                    || authority.source_revision <= base.authority.source_revision
                {
                    return Err(CandidateHostError::BaseMismatch);
                }
                let mut nodes = Vec::new();
                nodes
                    .try_reserve_exact(1)
                    .map_err(|_| CandidateHostError::AllocationFailed)?;
                let mut incoming_edges = Vec::new();
                incoming_edges
                    .try_reserve_exact(1)
                    .map_err(|_| CandidateHostError::AllocationFailed)?;
                let (build, owner) = {
                    let mut session = self.arena.begin_build()?;
                    let owner = session.retain(reused.canonical_root)?;
                    let build = session.suspend()?;
                    (build, owner)
                };
                nodes.push(owner);
                incoming_edges.push(0);
                (Some(build), nodes, incoming_edges, 1, Some(reused))
            } else {
                if expected_program != SnapshotProgram::Full {
                    return Err(CandidateHostError::BaseMismatch);
                }
                (None, Vec::new(), Vec::new(), 0, None)
            };

        let mut digest = snapshot_hasher();
        digest.update(frame);
        self.active = Some(ActiveOffer {
            authority,
            program: expected_program,
            reused_references,
            exact_base_delta: None,
            build,
            nodes,
            incoming_edges,
            node_count,
            payload_bytes: 0,
            wire_bytes,
            digest,
            phase: HostOfferPhase::Receiving,
        });
        Ok(())
    }

    pub(crate) fn offer_source_facts_replacement_page(
        &mut self,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.offer_source_facts_replacement_page_inner(&mut active, frame);
        match result {
            Ok(()) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn offer_source_facts_replacement_page_inner(
        &mut self,
        active: &mut ActiveOffer,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if active.program != SnapshotProgram::ExactBaseDelta
            || !matches!(active.phase, HostOfferPhase::ReceivingReplacementPages)
        {
            return Err(CandidateHostError::Busy);
        }
        let exact = active
            .exact_base_delta
            .as_mut()
            .ok_or(CandidateHostError::InvalidFrame(
                "exact-base host lost its replay program",
            ))?;
        if exact.next_replacement_page >= exact.replacement_page_end {
            return Err(CandidateHostError::InvalidFrame(
                "too many SourceFacts replacement pages",
            ));
        }
        let replacement =
            decode_source_facts_replacement_frame(frame, exact.next_replacement_page)?;
        if replacement.payload.len() > ARENA_PAGE_BYTES
            || !is_persistent_source_facts_sequence_leaf_payload(replacement.payload)
        {
            return Err(CandidateHostError::InvalidFrame(
                "invalid SourceFacts replacement leaf",
            ));
        }
        let next_payload = active
            .payload_bytes
            .checked_add(replacement.payload.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
        let next_wire = active
            .wire_bytes
            .checked_add(frame.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
        if next_wire > self.limits.maximum_snapshot_wire_bytes {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot exceeds its declared host envelope",
            ));
        }

        let build = active.build.take().ok_or(CandidateHostError::InvalidFrame(
            "exact-base replay lost its build journal",
        ))?;
        self.arena.validate_suspended_build(&build)?;
        let mut session = self
            .arena
            .resume_build(build)
            .expect("prevalidated exact-base build must resume");
        let operation = (|| -> Result<(), CandidateHostError> {
            let leaf = session.allocate(replacement.payload, &[])?;
            validate_imported_persistent_source_facts_node(session.arena(), leaf.id()).map_err(
                |_| CandidateHostError::InvalidFrame("replacement SourceFacts leaf is invalid"),
            )?;
            exact.replay.offer_replacement_leaf(&mut session, leaf)?;
            Ok(())
        })();
        active.build = Some(session.suspend()?);
        operation?;

        exact.next_replacement_page =
            exact
                .next_replacement_page
                .checked_add(1)
                .ok_or(CandidateHostError::InvalidFrame(
                    "replacement page ordinal overflow",
                ))?;
        active.payload_bytes = next_payload;
        active.wire_bytes = next_wire;
        active.digest.update(frame);
        active.phase = HostOfferPhase::AdvancingReplacementPage;
        Ok(())
    }

    /// Offers one typed packed BlockSequence leaf after SourceFacts replay has
    /// advanced to the block operation.
    pub(crate) fn offer_block_sequence_replacement_page(
        &mut self,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.offer_block_sequence_replacement_page_inner(&mut active, frame);
        match result {
            Ok(()) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn offer_block_sequence_replacement_page_inner(
        &mut self,
        active: &mut ActiveOffer,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if active.program != SnapshotProgram::ExactBaseDelta
            || !matches!(active.phase, HostOfferPhase::ReceivingBlockReplacementPages)
        {
            return Err(CandidateHostError::Busy);
        }
        let block = match active
            .exact_base_delta
            .as_mut()
            .and_then(|exact| exact.structural_splice.as_mut())
        {
            Some(ExactBaseStructuralSpliceHostState::Blocks(block)) => block,
            _ => {
                return Err(CandidateHostError::InvalidFrame(
                    "exact-base host lost its block replay",
                ))
            }
        };
        if block.next_replacement_page >= block.replacement_page_end {
            return Err(CandidateHostError::InvalidFrame(
                "too many block replacement pages",
            ));
        }
        let replacement = decode_block_replacement_frame(frame, block.next_replacement_page)?;
        if replacement.payload.len() > ARENA_PAGE_BYTES
            || !is_m11_block_sequence_node_payload(replacement.payload)
        {
            return Err(CandidateHostError::InvalidFrame(
                "invalid block replacement leaf",
            ));
        }
        let next_payload = active
            .payload_bytes
            .checked_add(replacement.payload.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
        let next_wire = active
            .wire_bytes
            .checked_add(frame.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
        if next_wire > self.limits.maximum_snapshot_wire_bytes {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot exceeds its declared host envelope",
            ));
        }

        let build = active.build.take().ok_or(CandidateHostError::InvalidFrame(
            "exact-base block replay lost its build journal",
        ))?;
        self.arena.validate_suspended_build(&build)?;
        let mut session = self
            .arena
            .resume_build(build)
            .expect("prevalidated exact-base block build must resume");
        let operation = (|| -> Result<(), CandidateHostError> {
            let leaf = session.allocate(replacement.payload, &[])?;
            validate_imported_m11_block_sequence_node(session.arena(), leaf.id()).map_err(
                |_| CandidateHostError::InvalidFrame("replacement block leaf is invalid"),
            )?;
            block.replay.offer_replacement_leaf(&mut session, leaf)?;
            Ok(())
        })();
        active.build = Some(session.suspend()?);
        operation?;

        block.next_replacement_page =
            block
                .next_replacement_page
                .checked_add(1)
                .ok_or(CandidateHostError::InvalidFrame(
                    "block replacement page ordinal overflow",
                ))?;
        active.payload_bytes = next_payload;
        active.wire_bytes = next_wire;
        active.digest.update(frame);
        active.phase = HostOfferPhase::AdvancingBlockReplacementPage;
        Ok(())
    }

    /// Offers one canonical recursive Green RGL1 leaf after SourceFacts replay
    /// has advanced to the mutually exclusive structural operation.
    pub(crate) fn offer_recursive_green_replacement_page(
        &mut self,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.offer_recursive_green_replacement_page_inner(&mut active, frame);
        match result {
            Ok(()) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn offer_recursive_green_replacement_page_inner(
        &mut self,
        active: &mut ActiveOffer,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if active.program != SnapshotProgram::ExactBaseDelta
            || !matches!(
                active.phase,
                HostOfferPhase::ReceivingRecursiveGreenReplacementPages
            )
        {
            return Err(CandidateHostError::Busy);
        }
        let green = match active
            .exact_base_delta
            .as_mut()
            .and_then(|exact| exact.structural_splice.as_mut())
        {
            Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(green)) => green,
            _ => {
                return Err(CandidateHostError::InvalidFrame(
                    "exact-base host lost its recursive Green replay",
                ))
            }
        };
        if green.next_replacement_page >= green.replacement_page_end {
            return Err(CandidateHostError::InvalidFrame(
                "too many recursive Green replacement pages",
            ));
        }
        let replacement =
            decode_recursive_green_replacement_frame(frame, green.next_replacement_page)?;
        if replacement.payload.len() > ARENA_PAGE_BYTES
            || replacement.payload.get(..4) != Some(b"RGL1".as_slice())
        {
            return Err(CandidateHostError::InvalidFrame(
                "invalid recursive Green replacement leaf",
            ));
        }
        let next_payload = active
            .payload_bytes
            .checked_add(replacement.payload.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
        let next_wire = active
            .wire_bytes
            .checked_add(frame.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
        if next_wire > self.limits.maximum_snapshot_wire_bytes {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot exceeds its declared host envelope",
            ));
        }

        let build = active.build.take().ok_or(CandidateHostError::InvalidFrame(
            "exact-base recursive Green replay lost its build journal",
        ))?;
        self.arena.validate_suspended_build(&build)?;
        let mut session = self
            .arena
            .resume_build(build)
            .expect("prevalidated exact-base recursive Green build must resume");
        let operation = (|| -> Result<(), CandidateHostError> {
            let leaf = session.allocate(replacement.payload, &[])?;
            green.replay.offer_replacement_leaf(&mut session, leaf)?;
            Ok(())
        })();
        active.build = Some(session.suspend()?);
        operation?;

        green.next_replacement_page =
            green
                .next_replacement_page
                .checked_add(1)
                .ok_or(CandidateHostError::InvalidFrame(
                    "recursive Green replacement page ordinal overflow",
                ))?;
        active.payload_bytes = next_payload;
        active.wire_bytes = next_wire;
        active.digest.update(frame);
        active.phase = HostOfferPhase::AdvancingRecursiveGreenReplacementPage;
        Ok(())
    }

    /// Advances replacement construction and the one bounded SourceFacts
    /// splice. Ordinary Node frames remain inadmissible until `ready_for_nodes`
    /// is returned.
    pub(crate) fn poll_exact_base_delta_replay(
        &mut self,
        fuel: usize,
    ) -> Result<CandidateHostReplayPoll, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.poll_exact_base_delta_replay_inner(&mut active, fuel);
        match result {
            Ok(poll) => {
                self.active = Some(active);
                Ok(poll)
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn poll_exact_base_delta_replay_inner(
        &mut self,
        active: &mut ActiveOffer,
        fuel: usize,
    ) -> Result<CandidateHostReplayPoll, CandidateHostError> {
        if active.program != SnapshotProgram::ExactBaseDelta {
            return Err(CandidateHostError::Busy);
        }
        if matches!(active.phase, HostOfferPhase::Receiving) {
            return Ok(CandidateHostReplayPoll {
                transitions: 0,
                ready_for_replacement_page: false,
                ready_for_nodes: true,
            });
        }
        let build = active.build.take().ok_or(CandidateHostError::InvalidFrame(
            "exact-base replay lost its build journal",
        ))?;
        self.arena.validate_suspended_build(&build)?;
        let mut session = self
            .arena
            .resume_build(build)
            .expect("prevalidated exact-base build must resume");
        let operation = (|| -> Result<CandidateHostReplayPoll, CandidateHostError> {
            let exact =
                active
                    .exact_base_delta
                    .as_mut()
                    .ok_or(CandidateHostError::InvalidFrame(
                        "exact-base host lost its replay program",
                    ))?;
            let mut transitions = 0;
            while transitions < fuel {
                match active.phase {
                    HostOfferPhase::ReceivingReplacementPages => {
                        if exact.next_replacement_page < exact.replacement_page_end {
                            return Ok(CandidateHostReplayPoll {
                                transitions,
                                ready_for_replacement_page: true,
                                ready_for_nodes: false,
                            });
                        }
                        let poll = exact.replay.finish_replacement(&session)?;
                        transitions += 1;
                        active.phase = match poll {
                            PersistentSourceFactsHostReplayPoll::Pending => {
                                HostOfferPhase::FinishingReplacement
                            }
                            PersistentSourceFactsHostReplayPoll::Complete => {
                                HostOfferPhase::CompletingReplay
                            }
                        };
                    }
                    HostOfferPhase::AdvancingReplacementPage => {
                        let poll = exact.replay.poll_replacement(&mut session)?;
                        transitions += 1;
                        if poll == PersistentSourceFactsHostReplayPoll::Complete {
                            active.phase = HostOfferPhase::ReceivingReplacementPages;
                            if exact.next_replacement_page < exact.replacement_page_end {
                                return Ok(CandidateHostReplayPoll {
                                    transitions,
                                    ready_for_replacement_page: true,
                                    ready_for_nodes: false,
                                });
                            }
                        }
                    }
                    HostOfferPhase::FinishingReplacement => {
                        let poll = exact.replay.poll_replacement(&mut session)?;
                        transitions += 1;
                        if poll == PersistentSourceFactsHostReplayPoll::Complete {
                            active.phase = HostOfferPhase::CompletingReplay;
                        }
                    }
                    HostOfferPhase::CompletingReplay => {
                        let output = exact.replay.complete(&mut session)?;
                        let (target_root, work) = output.into_parts();
                        transitions += 1;
                        match (exact.target_source_facts_present, target_root) {
                            (true, Some(owner)) => {
                                active
                                    .nodes
                                    .try_reserve(1)
                                    .map_err(|_| CandidateHostError::AllocationFailed)?;
                                active
                                    .incoming_edges
                                    .try_reserve(1)
                                    .map_err(|_| CandidateHostError::AllocationFailed)?;
                                exact.target_source_facts_root = Some(owner.id());
                                active.nodes.push(owner);
                                active.incoming_edges.push(0);
                                active.node_count = active.node_count.checked_add(1).ok_or(
                                    CandidateHostError::InvalidFrame("node count overflow"),
                                )?;
                            }
                            (false, None) => {}
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "SourceFacts replay changed empty-root authority",
                                ));
                            }
                        }
                        exact.replay_work = Some(work);
                        if let Some(structural) = exact.structural_splice.as_ref() {
                            active.phase = match structural {
                                ExactBaseStructuralSpliceHostState::Blocks(_) => {
                                    HostOfferPhase::ReceivingBlockReplacementPages
                                }
                                ExactBaseStructuralSpliceHostState::RecursiveGreen(_) => {
                                    HostOfferPhase::ReceivingRecursiveGreenReplacementPages
                                }
                            };
                        } else {
                            if usize::try_from(active.node_count).ok()
                                != Some(exact.virtual_root_count)
                            {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base virtual ordinal count changed",
                                ));
                            }
                            active.phase = HostOfferPhase::Receiving;
                            return Ok(CandidateHostReplayPoll {
                                transitions,
                                ready_for_replacement_page: false,
                                ready_for_nodes: true,
                            });
                        }
                    }
                    HostOfferPhase::ReceivingBlockReplacementPages => {
                        let block = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::Blocks(block)) => block,
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base block replay disappeared",
                                ))
                            }
                        };
                        if block.next_replacement_page < block.replacement_page_end {
                            return Ok(CandidateHostReplayPoll {
                                transitions,
                                ready_for_replacement_page: true,
                                ready_for_nodes: false,
                            });
                        }
                        let poll = block.replay.finish_replacement(&session)?;
                        transitions += 1;
                        active.phase = match poll {
                            M11BlockSequenceHostReplayPoll::Pending => {
                                HostOfferPhase::FinishingBlockReplacement
                            }
                            M11BlockSequenceHostReplayPoll::Complete => {
                                HostOfferPhase::CompletingBlockReplay
                            }
                        };
                    }
                    HostOfferPhase::AdvancingBlockReplacementPage => {
                        let block = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::Blocks(block)) => block,
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base block replay disappeared",
                                ))
                            }
                        };
                        let poll = block.replay.poll_replacement(&mut session)?;
                        transitions += 1;
                        if poll == M11BlockSequenceHostReplayPoll::Complete {
                            active.phase = HostOfferPhase::ReceivingBlockReplacementPages;
                            if block.next_replacement_page < block.replacement_page_end {
                                return Ok(CandidateHostReplayPoll {
                                    transitions,
                                    ready_for_replacement_page: true,
                                    ready_for_nodes: false,
                                });
                            }
                        }
                    }
                    HostOfferPhase::FinishingBlockReplacement => {
                        let block = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::Blocks(block)) => block,
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base block replay disappeared",
                                ))
                            }
                        };
                        let poll = block.replay.poll_replacement(&mut session)?;
                        transitions += 1;
                        if poll == M11BlockSequenceHostReplayPoll::Complete {
                            active.phase = HostOfferPhase::CompletingBlockReplay;
                        }
                    }
                    HostOfferPhase::CompletingBlockReplay => {
                        let block = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::Blocks(block)) => block,
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base block replay disappeared",
                                ))
                            }
                        };
                        let output = block.replay.complete(&mut session)?;
                        let (target_root, work) = output.into_parts();
                        transitions += 1;
                        let owner = target_root.ok_or(CandidateHostError::InvalidFrame(
                            "nonempty block replay target lost its root",
                        ))?;
                        active
                            .nodes
                            .try_reserve(1)
                            .map_err(|_| CandidateHostError::AllocationFailed)?;
                        active
                            .incoming_edges
                            .try_reserve(1)
                            .map_err(|_| CandidateHostError::AllocationFailed)?;
                        block.target_root = Some(owner.id());
                        block.work = Some(work);
                        active.nodes.push(owner);
                        active.incoming_edges.push(0);
                        active.node_count = active
                            .node_count
                            .checked_add(1)
                            .ok_or(CandidateHostError::InvalidFrame("node count overflow"))?;
                        if usize::try_from(active.node_count).ok() != Some(exact.virtual_root_count)
                        {
                            return Err(CandidateHostError::InvalidFrame(
                                "exact-base virtual ordinal count changed",
                            ));
                        }
                        active.phase = HostOfferPhase::Receiving;
                        return Ok(CandidateHostReplayPoll {
                            transitions,
                            ready_for_replacement_page: false,
                            ready_for_nodes: true,
                        });
                    }
                    HostOfferPhase::ReceivingRecursiveGreenReplacementPages => {
                        let green = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(green)) => {
                                green
                            }
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base recursive Green replay disappeared",
                                ))
                            }
                        };
                        if green.next_replacement_page < green.replacement_page_end {
                            return Ok(CandidateHostReplayPoll {
                                transitions,
                                ready_for_replacement_page: true,
                                ready_for_nodes: false,
                            });
                        }
                        let poll = green.replay.finish_replacement(&session)?;
                        transitions += 1;
                        active.phase = match poll {
                            M11RecursiveGreenHostReplayPoll::Pending => {
                                HostOfferPhase::FinishingRecursiveGreenReplacement
                            }
                            M11RecursiveGreenHostReplayPoll::Complete => {
                                HostOfferPhase::CompletingRecursiveGreenReplay
                            }
                        };
                    }
                    HostOfferPhase::AdvancingRecursiveGreenReplacementPage => {
                        let green = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(green)) => {
                                green
                            }
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base recursive Green replay disappeared",
                                ))
                            }
                        };
                        let poll = green.replay.poll_replacement(&mut session)?;
                        transitions += 1;
                        if poll == M11RecursiveGreenHostReplayPoll::Complete {
                            active.phase = HostOfferPhase::ReceivingRecursiveGreenReplacementPages;
                            if green.next_replacement_page < green.replacement_page_end {
                                return Ok(CandidateHostReplayPoll {
                                    transitions,
                                    ready_for_replacement_page: true,
                                    ready_for_nodes: false,
                                });
                            }
                        }
                    }
                    HostOfferPhase::FinishingRecursiveGreenReplacement => {
                        let green = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(green)) => {
                                green
                            }
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base recursive Green replay disappeared",
                                ))
                            }
                        };
                        let poll = green.replay.poll_replacement(&mut session)?;
                        transitions += 1;
                        if poll == M11RecursiveGreenHostReplayPoll::Complete {
                            active.phase = HostOfferPhase::CompletingRecursiveGreenReplay;
                        }
                    }
                    HostOfferPhase::CompletingRecursiveGreenReplay => {
                        let green = match exact.structural_splice.as_mut() {
                            Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(green)) => {
                                green
                            }
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "exact-base recursive Green replay disappeared",
                                ))
                            }
                        };
                        let output = green.replay.complete(&mut session)?;
                        let (target_root, work) = output.into_parts();
                        transitions += 1;
                        let target_present = green.target_descriptor.storage_page_count() != 0;
                        match (target_present, target_root) {
                            (true, Some(owner)) => {
                                active
                                    .nodes
                                    .try_reserve(1)
                                    .map_err(|_| CandidateHostError::AllocationFailed)?;
                                active
                                    .incoming_edges
                                    .try_reserve(1)
                                    .map_err(|_| CandidateHostError::AllocationFailed)?;
                                green.target_root = Some(owner.id());
                                active.nodes.push(owner);
                                active.incoming_edges.push(0);
                                active.node_count = active.node_count.checked_add(1).ok_or(
                                    CandidateHostError::InvalidFrame("node count overflow"),
                                )?;
                            }
                            (false, None) => {}
                            _ => {
                                return Err(CandidateHostError::InvalidFrame(
                                    "recursive Green replay changed empty-root authority",
                                ))
                            }
                        }
                        green.work = Some(work);
                        if usize::try_from(active.node_count).ok() != Some(exact.virtual_root_count)
                        {
                            return Err(CandidateHostError::InvalidFrame(
                                "exact-base virtual ordinal count changed",
                            ));
                        }
                        active.phase = HostOfferPhase::Receiving;
                        return Ok(CandidateHostReplayPoll {
                            transitions,
                            ready_for_replacement_page: false,
                            ready_for_nodes: true,
                        });
                    }
                    HostOfferPhase::Receiving
                    | HostOfferPhase::CheckingClosure { .. }
                    | HostOfferPhase::ValidatingProjection { .. }
                    | HostOfferPhase::Validating(_)
                    | HostOfferPhase::Sealing(_) => return Err(CandidateHostError::Busy),
                }
            }
            Ok(CandidateHostReplayPoll {
                transitions,
                ready_for_replacement_page: match active.phase {
                    HostOfferPhase::ReceivingReplacementPages => {
                        exact.next_replacement_page < exact.replacement_page_end
                    }
                    HostOfferPhase::ReceivingBlockReplacementPages => {
                        matches!(
                            exact.structural_splice.as_ref(),
                            Some(ExactBaseStructuralSpliceHostState::Blocks(block))
                                if block.next_replacement_page < block.replacement_page_end
                        )
                    }
                    HostOfferPhase::ReceivingRecursiveGreenReplacementPages => {
                        matches!(
                            exact.structural_splice.as_ref(),
                            Some(ExactBaseStructuralSpliceHostState::RecursiveGreen(green))
                                if green.next_replacement_page < green.replacement_page_end
                        )
                    }
                    _ => false,
                },
                ready_for_nodes: matches!(active.phase, HostOfferPhase::Receiving),
            })
        })();
        active.build = Some(session.suspend()?);
        operation
    }

    pub(crate) fn offer_node(&mut self, frame: &[u8]) -> Result<(), CandidateHostError> {
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.offer_node_inner(&mut active, frame);
        match result {
            Ok(()) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn offer_node_inner(
        &mut self,
        active: &mut ActiveOffer,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if !matches!(active.phase, HostOfferPhase::Receiving) {
            return Err(CandidateHostError::Busy);
        }
        let node = decode_node_frame(frame, active.node_count)?;
        if node.payload.len() > ARENA_PAGE_BYTES
            || node.child_count > self.limits.arena.max_children_per_node
        {
            return Err(CandidateHostError::InvalidFrame(
                "node payload or child arity exceeds its envelope",
            ));
        }
        let persistent_source_facts_node =
            is_persistent_source_facts_sequence_node_payload(node.payload);
        let parser_page_node = is_m11_parser_page_node_payload(node.payload);
        let block_sequence_node = is_m11_block_sequence_node_payload(node.payload);
        let recursive_green_node = is_m11_recursive_green_node_payload(node.payload);
        if persistent_source_facts_node
            || parser_page_node
            || block_sequence_node
            || recursive_green_node
        {
            // The schema-owned decoder validates exact bytes and child
            // measures immediately after allocation below.
        } else if canonical_role_record_count(node.payload) == 1
            || crate::reference_root::is_canonical_reference_node_payload(node.payload)
        {
            decode_canonical_node_header(node.payload, node.payload[0])?;
        } else {
            decode_candidate_header(node.payload, node.payload[0], active.authority)?;
        }
        let next_nodes = active
            .node_count
            .checked_add(1)
            .ok_or(CandidateHostError::InvalidFrame("node count overflow"))?;
        let next_payload = active
            .payload_bytes
            .checked_add(node.payload.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("payload total overflow"))?;
        let next_wire = active
            .wire_bytes
            .checked_add(frame.len() as u64)
            .ok_or(CandidateHostError::InvalidFrame("wire total overflow"))?;
        if next_nodes > self.limits.maximum_snapshot_nodes
            || next_wire > self.limits.maximum_snapshot_wire_bytes
        {
            return Err(CandidateHostError::InvalidFrame(
                "snapshot exceeds its declared host envelope",
            ));
        }

        let mut child_ids = Vec::new();
        child_ids
            .try_reserve_exact(node.child_count)
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        for index in 0..node.child_count {
            let ordinal = node.child_ordinal(index)?;
            if ordinal >= active.node_count {
                return Err(CandidateHostError::InvalidFrame(
                    "node child is not a strict backward stream reference",
                ));
            }
            let child_index = usize::try_from(ordinal)
                .map_err(|_| CandidateHostError::InvalidFrame("child ordinal overflow"))?;
            child_ids.push(active.nodes[child_index].id());
            active.incoming_edges[child_index].checked_add(1).ok_or(
                CandidateHostError::InvalidFrame("incoming edge count overflow"),
            )?;
        }
        active
            .nodes
            .try_reserve(1)
            .map_err(|_| CandidateHostError::AllocationFailed)?;
        active
            .incoming_edges
            .try_reserve(1)
            .map_err(|_| CandidateHostError::AllocationFailed)?;

        let mut session = if let Some(build) = active.build.take() {
            self.arena.validate_suspended_build(&build)?;
            self.arena
                .resume_build(build)
                .expect("prevalidated host build must resume")
        } else {
            self.arena.begin_build()?
        };
        let parent = session.allocate(node.payload, &child_ids)?;
        if persistent_source_facts_node {
            validate_imported_persistent_source_facts_node(session.arena(), parent.id()).map_err(
                |_| CandidateHostError::InvalidFrame("persistent SourceFacts node is invalid"),
            )?;
        } else if parser_page_node {
            validate_imported_m11_parser_page_node(session.arena(), parent.id()).map_err(|_| {
                CandidateHostError::InvalidFrame("persistent parser page node is invalid")
            })?;
        } else if block_sequence_node {
            validate_imported_m11_block_sequence_node(session.arena(), parent.id()).map_err(
                |_| CandidateHostError::InvalidFrame("persistent block node is invalid"),
            )?;
        } else if recursive_green_node {
            validate_imported_m11_recursive_green_node(session.arena(), parent.id()).map_err(
                |_| CandidateHostError::InvalidFrame("persistent recursive Green node is invalid"),
            )?;
        }
        for index in 0..node.child_count {
            let ordinal = node.child_ordinal(index)?;
            let child_index = usize::try_from(ordinal)
                .map_err(|_| CandidateHostError::InvalidFrame("child ordinal overflow"))?;
            active.incoming_edges[child_index] += 1;
        }
        active.nodes.push(parent);
        active.incoming_edges.push(0);
        active.build = Some(session.suspend()?);
        active.node_count = next_nodes;
        active.payload_bytes = next_payload;
        active.wire_bytes = next_wire;
        active.digest.update(frame);
        Ok(())
    }

    pub(crate) fn finish_snapshot(&mut self, frame: &[u8]) -> Result<(), CandidateHostError> {
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.finish_snapshot_inner(&mut active, frame);
        match result {
            Ok(()) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn finish_snapshot_inner(
        &mut self,
        active: &mut ActiveOffer,
        frame: &[u8],
    ) -> Result<(), CandidateHostError> {
        if !matches!(active.phase, HostOfferPhase::Receiving) {
            return Err(CandidateHostError::Busy);
        }
        let claim = decode_end_frame(frame)?;
        let node_count = usize::try_from(active.node_count)
            .map_err(|_| CandidateHostError::InvalidFrame("node count overflow"))?;
        if claim.nodes != active.node_count
            || claim.payload_bytes != active.payload_bytes
            || claim.wire_bytes != active.wire_bytes
            || claim.digest != *active.digest.finalize().as_bytes()
            || node_count == 0
            || active.nodes.len() != node_count
            || active.incoming_edges.len() != node_count
            || active.build.is_none()
        {
            return Err(CandidateHostError::InvalidFrame(
                "truncated closure or snapshot digest mismatch",
            ));
        }
        active.phase = HostOfferPhase::CheckingClosure {
            claim,
            next_index: 0,
        };
        Ok(())
    }

    pub(crate) fn poll_install(
        &mut self,
        fuel: usize,
    ) -> Result<CandidateHostInstallPoll, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        let mut active = self.active.take().ok_or(CandidateHostError::NoOffer)?;
        let result = self.poll_install_inner(&mut active, fuel);
        match result {
            Ok((transitions, Some(root))) => {
                let snapshot = InstalledCandidateSnapshot {
                    authority: active.authority,
                };
                self.install_root(active.authority, root)?;
                Ok(CandidateHostInstallPoll {
                    transitions,
                    installed: Some(snapshot),
                })
            }
            Ok((transitions, None)) => {
                self.active = Some(active);
                Ok(CandidateHostInstallPoll {
                    transitions,
                    installed: None,
                })
            }
            Err(error) => {
                self.abort_active(active)?;
                Err(error)
            }
        }
    }

    fn poll_install_inner(
        &mut self,
        active: &mut ActiveOffer,
        fuel: usize,
    ) -> Result<(usize, Option<CommittedArenaRoot>), CandidateHostError> {
        match &mut active.phase {
            HostOfferPhase::ReceivingReplacementPages
            | HostOfferPhase::AdvancingReplacementPage
            | HostOfferPhase::FinishingReplacement
            | HostOfferPhase::CompletingReplay
            | HostOfferPhase::ReceivingBlockReplacementPages
            | HostOfferPhase::AdvancingBlockReplacementPage
            | HostOfferPhase::FinishingBlockReplacement
            | HostOfferPhase::CompletingBlockReplay
            | HostOfferPhase::ReceivingRecursiveGreenReplacementPages
            | HostOfferPhase::AdvancingRecursiveGreenReplacementPage
            | HostOfferPhase::FinishingRecursiveGreenReplacement
            | HostOfferPhase::CompletingRecursiveGreenReplay
            | HostOfferPhase::Receiving => Err(CandidateHostError::Busy),
            HostOfferPhase::CheckingClosure { claim, next_index } => {
                let node_count = usize::try_from(claim.nodes)
                    .map_err(|_| CandidateHostError::InvalidFrame("node count overflow"))?;
                let mut transitions = 0;
                while *next_index < node_count && transitions < fuel {
                    let incoming = active.incoming_edges[*next_index];
                    let is_root = *next_index == node_count - 1;
                    let virtual_incoming = match active.program {
                        SnapshotProgram::Full => None,
                        SnapshotProgram::ExactBaseReferences => (*next_index == 0).then_some(1),
                        SnapshotProgram::ExactBaseDelta => {
                            active.exact_base_delta.as_ref().and_then(|exact| {
                                if *next_index >= exact.virtual_root_count {
                                    None
                                } else if matches!(
                                    exact.structural_splice.as_ref(),
                                    Some(ExactBaseStructuralSpliceHostState::Blocks(_))
                                ) && *next_index + 1 == exact.virtual_root_count
                                {
                                    // Paired Green and Projection wrappers
                                    // intentionally share this canonical root.
                                    Some(2)
                                } else {
                                    Some(1)
                                }
                            })
                        }
                    };
                    if (is_root && incoming != 0)
                        || virtual_incoming.is_some_and(|expected| incoming != expected)
                        || (!is_root && virtual_incoming.is_none() && incoming == 0)
                    {
                        return Err(CandidateHostError::InvalidFrame(
                            "snapshot stream is not one connected root closure",
                        ));
                    }
                    *next_index += 1;
                    transitions += 1;
                }
                if *next_index == node_count && transitions < fuel {
                    let root = active.nodes[node_count - 1].id();
                    let descriptor = decode_manifest(&self.arena, root, active.authority)?;
                    let references_wrapper =
                        descriptor.children[role_index(CandidateRole::References)];
                    let references_root = self.arena.child_at(references_wrapper, 0)?;
                    let references_metadata =
                        descriptor.metadata[role_index(CandidateRole::References)];
                    let references_validator = if active.program == SnapshotProgram::ExactBaseDelta
                    {
                        let reused =
                            active
                                .reused_references
                                .ok_or(CandidateHostError::InvalidFrame(
                                    "exact-base program lost References authority",
                                ))?;
                        if references_root != reused.canonical_root
                            || references_metadata != reused.metadata
                        {
                            return Err(CandidateHostError::InvalidFrame(
                                "target References wrapper changed its exact base",
                            ));
                        }
                        let exact = active.exact_base_delta.as_ref().ok_or(
                            CandidateHostError::InvalidFrame(
                                "exact-base host lost its replay program",
                            ),
                        )?;
                        if exact.replay_work.is_none() {
                            return Err(CandidateHostError::InvalidFrame(
                                "target closure arrived before SourceFacts replay",
                            ));
                        }
                        let target_source_facts = persistent_source_facts_manifest_role(
                            &self.arena,
                            &descriptor,
                            active.authority,
                        )?;
                        if target_source_facts.root != exact.target_source_facts_root
                            || target_source_facts.metadata != exact.target_source_facts_metadata
                            || target_source_facts.descriptor_bytes
                                != exact.target_source_facts_descriptor
                        {
                            return Err(CandidateHostError::InvalidFrame(
                                "target SourceFacts wrapper changed its replay result",
                            ));
                        }
                        if let Some(structural) = exact.structural_splice.as_ref() {
                            match structural {
                                ExactBaseStructuralSpliceHostState::Blocks(block) => {
                                    if block.work.is_none() {
                                        return Err(CandidateHostError::InvalidFrame(
                                            "target closure arrived before block replay",
                                        ));
                                    }
                                    let target_blocks = persistent_block_manifest_roles(
                                        &self.arena,
                                        &descriptor,
                                        active.authority,
                                    )?;
                                    if target_blocks.root != block.target_root
                                        || target_blocks.green != block.target_green
                                        || target_blocks.projection != block.target_projection
                                        || descriptor.metadata[role_index(CandidateRole::Green)]
                                            != block.target_green_metadata
                                        || descriptor.metadata
                                            [role_index(CandidateRole::Projection)]
                                            != block.target_projection_metadata
                                    {
                                        return Err(CandidateHostError::InvalidFrame(
                                            "target block wrappers changed their replay result",
                                        ));
                                    }
                                }
                                ExactBaseStructuralSpliceHostState::RecursiveGreen(green) => {
                                    if green.work.is_none() {
                                        return Err(CandidateHostError::InvalidFrame(
                                            "target closure arrived before recursive Green replay",
                                        ));
                                    }
                                    let target_green = persistent_recursive_green_manifest_role(
                                        &self.arena,
                                        &descriptor,
                                        active.authority,
                                    )?;
                                    if target_green.root != green.target_root
                                        || target_green.descriptor != green.target_descriptor
                                        || descriptor.metadata[role_index(CandidateRole::Green)]
                                            != green.target_green_metadata
                                    {
                                        return Err(CandidateHostError::InvalidFrame(
                                            "target recursive Green wrapper changed its replay result",
                                        ));
                                    }
                                }
                            }
                        }
                        None
                    } else if let Some(reused) = active.reused_references {
                        if active.program != SnapshotProgram::ExactBaseReferences
                            || references_root != reused.canonical_root
                            || references_metadata != reused.metadata
                        {
                            return Err(CandidateHostError::InvalidFrame(
                                "target References wrapper changed its exact base",
                            ));
                        }
                        None
                    } else {
                        Some(Box::new(ReferenceRoleDigestValidator::new(
                            &self.arena,
                            active.authority,
                            references_root,
                            references_metadata,
                        )?))
                    };
                    let projection_validator = match persistent_inline_projection_manifest_role(
                        &self.arena,
                        &descriptor,
                        active.authority,
                    ) {
                        Ok(role) => Some(Box::new(
                            PersistentM11InlineProjectionHostValidator::new(
                                &self.arena,
                                role.fact_root,
                                role.link_value_root,
                                role.descriptor,
                            )
                            .map_err(ManifestError::from)?,
                        )),
                        Err(ManifestError::InvalidRole) => None,
                        Err(error) => return Err(error.into()),
                    };
                    match (projection_validator, references_validator) {
                        (Some(validator), references) => {
                            active.phase = HostOfferPhase::ValidatingProjection {
                                validator,
                                references,
                            };
                        }
                        (None, Some(references)) => {
                            active.phase = HostOfferPhase::Validating(references);
                        }
                        (None, None) => self.begin_active_seal(active)?,
                    }
                    transitions += 1;
                }
                Ok((transitions, None))
            }
            HostOfferPhase::ValidatingProjection {
                validator,
                references,
            } => {
                let PersistentM11InlineProjectionHostValidationPoll {
                    transitions,
                    complete,
                } = validator
                    .poll(&self.arena, fuel)
                    .map_err(ManifestError::from)?;
                if complete {
                    if let Some(references) = references.take() {
                        active.phase = HostOfferPhase::Validating(references);
                    } else {
                        self.begin_active_seal(active)?;
                    }
                }
                Ok((transitions, None))
            }
            HostOfferPhase::Validating(validator) => {
                let ReferenceRoleValidationPoll {
                    transitions,
                    complete,
                } = validator.poll(&self.arena, fuel)?;
                if complete {
                    self.begin_active_seal(active)?;
                }
                Ok((transitions, None))
            }
            HostOfferPhase::Sealing(seal) => {
                let receipt = self.arena.poll_seal(seal, fuel)?;
                Ok((receipt.transitions, receipt.root))
            }
        }
    }

    fn begin_active_seal(&mut self, active: &mut ActiveOffer) -> Result<(), CandidateHostError> {
        if active.build.is_none() || active.nodes.is_empty() {
            return Err(CandidateHostError::InvalidFrame(
                "validated snapshot lost its root or build capability",
            ));
        }
        let build = active
            .build
            .take()
            .expect("validated build presence was checked");
        let root = active
            .nodes
            .pop()
            .expect("validated root presence was checked");
        // Handles are non-owning views of the arena build journal; dropping
        // the non-root views does not release ownership.
        active.nodes.clear();
        active.incoming_edges.clear();
        let seal = match self.arena.begin_seal(build, root) {
            Ok(seal) => seal,
            Err(failure) => {
                self.arena.abort_build(failure.build)?;
                return Err(failure.error.into());
            }
        };
        active.phase = HostOfferPhase::Sealing(seal);
        Ok(())
    }

    fn install_root(
        &mut self,
        authority: CandidateAuthority,
        root: CommittedArenaRoot,
    ) -> Result<(), CandidateHostError> {
        let descriptor = decode_manifest_descriptor(&self.arena, root.id(), authority)?;
        let persistent_blocks =
            match persistent_block_manifest_roles(&self.arena, &descriptor, authority) {
                Ok(roles) => Some(InstalledPersistentBlockRoot {
                    root: roles.root,
                    claim: roles.claim,
                }),
                Err(ManifestError::InvalidRole) => None,
                Err(error) => {
                    self.arena
                        .release_committed_root(root)
                        .map_err(|failure| CandidateHostError::Arena(failure.error))?;
                    return Err(error.into());
                }
            };
        let persistent_recursive_green =
            match persistent_recursive_green_manifest_role(&self.arena, &descriptor, authority) {
                Ok(role) => Some(InstalledPersistentRecursiveGreenRoot {
                    root: role.root,
                    claim: role.claim,
                }),
                Err(ManifestError::InvalidRole) => None,
                Err(error) => {
                    self.arena
                        .release_committed_root(root)
                        .map_err(|failure| CandidateHostError::Arena(failure.error))?;
                    return Err(error.into());
                }
            };
        let next = InstalledRoot {
            authority,
            source: self.current_source,
            root,
            persistent_blocks,
            persistent_recursive_green,
        };
        if let Some(previous) = self.installed.take() {
            let InstalledRoot {
                authority: previous_authority,
                source: previous_source,
                root: previous_root,
                persistent_blocks: previous_blocks,
                persistent_recursive_green: previous_recursive_green,
            } = previous;
            if let Err(failure) = self.arena.release_committed_root(previous_root) {
                self.installed = Some(InstalledRoot {
                    authority: previous_authority,
                    source: previous_source,
                    root: failure.root,
                    persistent_blocks: previous_blocks,
                    persistent_recursive_green: previous_recursive_green,
                });
                self.arena
                    .release_committed_root(next.root)
                    .map_err(|failure| CandidateHostError::Arena(failure.error))?;
                return Err(CandidateHostError::Arena(ArenaError::StaleHandle));
            }
        }
        self.installed = Some(next);
        Ok(())
    }

    pub(crate) fn installed_snapshot(&self) -> Option<InstalledCandidateSnapshot> {
        self.installed
            .as_ref()
            .map(|installed| InstalledCandidateSnapshot {
                authority: installed.authority,
            })
    }

    /// Mints the only host-side base capability accepted by the sibling
    /// hot-inline store from one exact currently installed canonical root.
    pub(crate) fn inline_overlay_base(
        &self,
        snapshot: InstalledCandidateSnapshot,
        parser_profile: crate::ParserProfileId,
    ) -> Result<M11InlineOverlayBase, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::BaseMismatch);
        }
        M11InlineOverlayBase::new(installed.authority, installed.source, parser_profile).map_err(
            |error| match error {
                M11InlineOverlayError::CoordinateOverflow => CandidateHostError::InvalidFrame(
                    "installed hot-inline base coordinate overflow",
                ),
                _ => CandidateHostError::CrossAuthority,
            },
        )
    }

    /// Returns the exact full-width stream digest after End validation and
    /// before atomic installation. The digest covers Begin plus every ordered
    /// Node frame; the End claim is not recursively included in itself.
    pub(crate) fn active_snapshot_digest256(&self) -> Option<[u8; SNAPSHOT_DIGEST_BYTES]> {
        self.active.as_ref().and_then(|active| {
            matches!(
                active.phase,
                HostOfferPhase::CheckingClosure { .. }
                    | HostOfferPhase::ValidatingProjection { .. }
                    | HostOfferPhase::Validating(_)
                    | HostOfferPhase::Sealing(_)
            )
            .then(|| *active.digest.clone().finalize().as_bytes())
        })
    }

    pub(crate) fn active_authority(&self) -> Option<CandidateAuthority> {
        self.active.as_ref().map(|active| active.authority)
    }

    /// Returns the authenticated block-splice work once replay has reached
    /// the ordinary-node barrier.
    pub(crate) fn active_block_splice_work(&self) -> Option<M11BlockSequenceHostSpliceWork> {
        self.active
            .as_ref()
            .and_then(|active| active.exact_base_delta.as_ref())
            .and_then(|exact| exact.structural_splice.as_ref())
            .and_then(|structural| match structural {
                ExactBaseStructuralSpliceHostState::Blocks(block) => block.work,
                ExactBaseStructuralSpliceHostState::RecursiveGreen(_) => None,
            })
    }

    pub(crate) fn active_recursive_green_splice_work(
        &self,
    ) -> Option<M11RecursiveGreenHostSpliceWork> {
        self.active
            .as_ref()
            .and_then(|active| active.exact_base_delta.as_ref())
            .and_then(|exact| exact.structural_splice.as_ref())
            .and_then(|structural| match structural {
                ExactBaseStructuralSpliceHostState::Blocks(_) => None,
                ExactBaseStructuralSpliceHostState::RecursiveGreen(green) => green.work,
            })
    }

    /// Recomputes the installed manifest digest from the independent host
    /// arena after descriptor validation. No producer descriptor is trusted
    /// for the ACK proof.
    pub(crate) fn installed_manifest_digest256(
        &self,
        snapshot: InstalledCandidateSnapshot,
    ) -> Result<[u8; SNAPSHOT_DIGEST_BYTES], CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        Ok(manifest_digest256(installed.authority, &descriptor))
    }

    pub(crate) fn persistent_inline_projection_descriptor(
        &self,
        snapshot: InstalledCandidateSnapshot,
    ) -> Result<Option<InstalledPersistentInlineProjectionDescriptor>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        let role = match persistent_inline_projection_manifest_role(
            &self.arena,
            &descriptor,
            installed.authority,
        ) {
            Ok(role) => role,
            Err(ManifestError::InvalidRole) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let source_range = role.descriptor.source_range();
        let maximum_tree_nodes_visited = role.descriptor.maximum_query_tree_nodes_visited().ok_or(
            CandidateHostError::InvalidFrame("persistent Projection query work bound overflowed"),
        )?;
        Ok(Some(InstalledPersistentInlineProjectionDescriptor {
            source_start: source_range.start,
            source_end: source_range.end,
            structural_record_count: role.structural_record_count,
            logical_page_count: role.descriptor.logical_page_count(),
            fact_count: role.descriptor.fact_count(),
            storage_page_count: role.descriptor.storage_page_count(),
            link_value_entry_count: role.descriptor.link_value_entry_count(),
            link_value_storage_page_count: role.descriptor.link_value_storage_page_count(),
            link_value_encoded_bytes: role.descriptor.link_value_encoded_bytes(),
            maximum_open_depth: role.descriptor.maximum_query_open_depth(),
            maximum_tree_nodes_visited,
        }))
    }

    pub(crate) fn persistent_block_descriptor(
        &self,
        snapshot: InstalledCandidateSnapshot,
    ) -> Result<Option<InstalledPersistentBlockDescriptor>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(blocks) = installed.persistent_blocks else {
            return Ok(None);
        };
        let maximum_tree_nodes_visited = blocks.claim.maximum_point_query_node_headers();
        Ok(Some(InstalledPersistentBlockDescriptor {
            source_bytes: blocks.claim.source_bytes(),
            source_utf16: blocks.claim.source_utf16(),
            entry_count: blocks.claim.entry_count(),
            reference_definition_count: blocks.claim.reference_definition_count(),
            storage_page_count: blocks.claim.storage_page_count(),
            tree_height: blocks.claim.tree_height(),
            maximum_tree_nodes_visited,
        }))
    }

    pub(crate) fn persistent_recursive_green_descriptor(
        &self,
        snapshot: InstalledCandidateSnapshot,
    ) -> Result<Option<InstalledPersistentRecursiveGreenDescriptor>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(green) = installed.persistent_recursive_green else {
            return Ok(None);
        };
        Ok(Some(InstalledPersistentRecursiveGreenDescriptor {
            source_bytes: green.claim.source_bytes(),
            source_utf16: green.claim.source_utf16(),
            event_count: green.claim.event_count(),
            renderable_row_count: green.claim.renderable_row_count(),
            storage_page_count: green.claim.storage_page_count(),
            tree_height: green.claim.tree_height(),
        }))
    }

    pub(crate) fn persistent_recursive_green_point(
        &self,
        snapshot: InstalledCandidateSnapshot,
        point: M11RecursiveGreenPoint,
    ) -> Result<Option<M11RecursiveGreenLocation>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(green) = installed.persistent_recursive_green else {
            return Ok(None);
        };
        persistent_m11_recursive_green_locate_point(&self.arena, green.root, green.claim, point)
            .map_err(Into::into)
    }

    pub(crate) fn persistent_recursive_green_rows(
        &self,
        snapshot: InstalledCandidateSnapshot,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<Option<M11RecursiveGreenRowQueryOutcome>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(green) = installed.persistent_recursive_green else {
            return Ok(None);
        };
        persistent_m11_recursive_green_locate_rows(
            &self.arena,
            green.root,
            green.claim,
            point,
            requested_end_byte,
            limits,
        )
        .map(Some)
        .map_err(Into::into)
    }

    pub(crate) fn persistent_recursive_green_row_ordinal_window(
        &self,
        snapshot: InstalledCandidateSnapshot,
        start_ordinal: u64,
        maximum_rows: u32,
    ) -> Result<Option<M11RecursiveGreenRowOrdinalWindow>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(green) = installed.persistent_recursive_green else {
            return Ok(None);
        };
        persistent_m11_recursive_green_locate_row_ordinal_window(
            &self.arena,
            green.root,
            green.claim,
            start_ordinal,
            maximum_rows,
        )
        .map(Some)
        .map_err(Into::into)
    }

    pub(crate) fn persistent_block_point(
        &self,
        snapshot: InstalledCandidateSnapshot,
        point: M11BlockSequencePoint,
    ) -> Result<Option<M11BlockSequenceLocation>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(blocks) = installed.persistent_blocks else {
            return Ok(None);
        };
        persistent_m11_block_locate_point(&self.arena, blocks.root, blocks.claim, point)
            .map_err(Into::into)
    }

    pub(crate) fn persistent_block_ordinal_window(
        &self,
        snapshot: InstalledCandidateSnapshot,
        start_entry_ordinal: u64,
        maximum_entries: u32,
    ) -> Result<Option<M11BlockSequenceOrdinalWindow>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(blocks) = installed.persistent_blocks else {
            return Ok(None);
        };
        persistent_m11_block_locate_ordinal_window(
            &self.arena,
            blocks.root,
            blocks.claim,
            start_entry_ordinal,
            maximum_entries,
        )
        .map(Some)
        .map_err(Into::into)
    }

    pub(crate) fn visit_persistent_blocks(
        &self,
        snapshot: InstalledCandidateSnapshot,
        start: M11BlockSequenceVisitStart,
        maximum_entries: u32,
        maximum_storage_pages: u32,
        visitor: impl FnMut(M11BlockSequenceVisitEntry<'_>) -> M11BlockSequenceVisitControl,
    ) -> Result<Option<M11BlockSequenceVisitReceipt>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let Some(blocks) = installed.persistent_blocks else {
            return Ok(None);
        };
        persistent_m11_block_visit_entries(
            &self.arena,
            blocks.root,
            blocks.claim,
            start,
            maximum_entries,
            maximum_storage_pages,
            visitor,
        )
        .map(Some)
        .map_err(Into::into)
    }

    pub(crate) fn persistent_inline_projection_cursor(
        &self,
        snapshot: InstalledCandidateSnapshot,
    ) -> Result<Option<PersistentM11InlineProjectionHostCursor<'_>>, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        let role = match persistent_inline_projection_manifest_role(
            &self.arena,
            &descriptor,
            installed.authority,
        ) {
            Ok(role) => role,
            Err(ManifestError::InvalidRole) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        Ok(Some(PersistentM11InlineProjectionHostCursor::new(
            &self.arena,
            role.fact_root,
            role.descriptor,
        )))
    }

    pub(crate) fn copy_persistent_inline_link_values(
        &self,
        snapshot: InstalledCandidateSnapshot,
        output: &mut [u8],
    ) -> Result<Option<PersistentM11InlineLinkValueEncodeReceipt>, CandidateHostError> {
        if output.len() > self.limits.maximum_query_bytes {
            return Err(CandidateHostError::InvalidFrame(
                "inline link value query exceeds its bounded envelope",
            ));
        }
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        let role = match persistent_inline_projection_manifest_role(
            &self.arena,
            &descriptor,
            installed.authority,
        ) {
            Ok(role) => role,
            Err(ManifestError::InvalidRole) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        encode_persistent_inline_link_values(
            &self.arena,
            role.link_value_root,
            role.descriptor,
            output,
        )
        .map(Some)
        .map_err(ManifestError::from)
        .map_err(Into::into)
    }

    pub(crate) fn read_role_record(
        &self,
        snapshot: InstalledCandidateSnapshot,
        role: CandidateRole,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, CandidateHostError> {
        self.read_role_record_at(snapshot, role, 0, offset, output)
    }

    pub(crate) fn read_role_record_at(
        &self,
        snapshot: InstalledCandidateSnapshot,
        role: CandidateRole,
        ordinal: u64,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, CandidateHostError> {
        if output.len() > self.limits.maximum_query_bytes {
            return Err(CandidateHostError::InvalidFrame(
                "query output exceeds its bounded envelope",
            ));
        }
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        if role == CandidateRole::Projection {
            match persistent_inline_projection_manifest_role(
                &self.arena,
                &descriptor,
                installed.authority,
            ) {
                Ok(persistent) if ordinal >= persistent.structural_record_count => {
                    if ordinal >= persistent.metadata.record_count {
                        return Err(CandidateHostError::InvalidFrame(
                            "Projection record ordinal is out of range",
                        ));
                    }
                    let inline_ordinal = ordinal - persistent.structural_record_count;
                    let record = manifest_persistent_inline_projection_record_at(
                        &self.arena,
                        &descriptor,
                        installed.authority,
                        inline_ordinal,
                    )?;
                    let bytes = record.as_bytes();
                    if offset >= bytes.len() || output.is_empty() {
                        return Ok(0);
                    }
                    let count = output.len().min(bytes.len() - offset);
                    output[..count].copy_from_slice(&bytes[offset..offset + count]);
                    return Ok(count);
                }
                Ok(_) | Err(ManifestError::InvalidRole) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let bytes = manifest_role_record_bytes_at(
            &self.arena,
            installed.authority,
            &descriptor,
            role,
            ordinal,
        )?;
        if offset >= bytes.len() || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min(bytes.len() - offset);
        output[..count].copy_from_slice(&bytes[offset..offset + count]);
        Ok(count)
    }

    pub(crate) fn role_record_count(
        &self,
        snapshot: InstalledCandidateSnapshot,
        role: CandidateRole,
    ) -> Result<u64, CandidateHostError> {
        let installed = self.installed.as_ref().ok_or(CandidateHostError::NoOffer)?;
        if installed.authority != snapshot.authority {
            return Err(CandidateHostError::CrossAuthority);
        }
        let descriptor =
            decode_manifest_descriptor(&self.arena, installed.root.id(), installed.authority)?;
        Ok(descriptor.metadata[role_index(role)].record_count)
    }

    pub(crate) fn abort_snapshot(&mut self) -> Result<bool, CandidateHostError> {
        let Some(active) = self.active.take() else {
            return Ok(false);
        };
        self.abort_active(active)?;
        Ok(true)
    }

    fn abort_active(&mut self, mut active: ActiveOffer) -> Result<(), CandidateHostError> {
        match active.phase {
            HostOfferPhase::Sealing(seal) => self.arena.abort_seal(seal)?,
            HostOfferPhase::ReceivingReplacementPages
            | HostOfferPhase::AdvancingReplacementPage
            | HostOfferPhase::FinishingReplacement
            | HostOfferPhase::CompletingReplay
            | HostOfferPhase::ReceivingBlockReplacementPages
            | HostOfferPhase::AdvancingBlockReplacementPage
            | HostOfferPhase::FinishingBlockReplacement
            | HostOfferPhase::CompletingBlockReplay
            | HostOfferPhase::ReceivingRecursiveGreenReplacementPages
            | HostOfferPhase::AdvancingRecursiveGreenReplacementPage
            | HostOfferPhase::FinishingRecursiveGreenReplacement
            | HostOfferPhase::CompletingRecursiveGreenReplay
            | HostOfferPhase::Receiving
            | HostOfferPhase::CheckingClosure { .. }
            | HostOfferPhase::ValidatingProjection { .. }
            | HostOfferPhase::Validating(_) => {
                if let Some(build) = active.build.take() {
                    self.arena.abort_build(build)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn poll_reclaim(&mut self, fuel: usize) -> Result<bool, CandidateHostError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        Ok(self.arena.poll_reclaim(fuel).complete)
    }

    pub(crate) fn begin_close(&mut self) -> Result<(), CandidateHostError> {
        if self.closing || self.closed {
            return Ok(());
        }
        if let Some(active) = self.active.take() {
            self.abort_active(active)?;
        }
        if let Some(installed) = self.installed.take() {
            let InstalledRoot {
                authority,
                source,
                root,
                persistent_blocks,
                persistent_recursive_green,
            } = installed;
            if let Err(failure) = self.arena.release_committed_root(root) {
                self.installed = Some(InstalledRoot {
                    authority,
                    source,
                    root: failure.root,
                    persistent_blocks,
                    persistent_recursive_green,
                });
                return Err(CandidateHostError::Arena(failure.error));
            }
        }
        self.closing = true;
        Ok(())
    }

    pub(crate) fn poll_close(&mut self, fuel: usize) -> Result<bool, CandidateHostError> {
        if !self.closing {
            return Err(CandidateHostError::Busy);
        }
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        let receipt = self.arena.poll_reclaim(fuel);
        let metrics = self.arena.metrics();
        self.closed = receipt.complete && metrics.resident_nodes == 0 && metrics.live_builds == 0;
        Ok(self.closed)
    }
}

impl Drop for CandidateHostStore {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.closed
                    && self.installed.is_none()
                    && self.active.is_none()
                    && self.arena.metrics().resident_nodes == 0
                    && self.arena.metrics().live_builds == 0,
                "candidate host must be explicitly closed and fuel-drained"
            );
        }
    }
}

struct DecodedExactBaseDeltaBegin {
    authority: CandidateAuthority,
    base_authority: CandidateAuthority,
    virtual_root_count: usize,
    reused_references: RoleMetadata,
    source_facts_virtual_ordinal: u64,
    base_page_range: std::ops::Range<u64>,
    target_page_range: std::ops::Range<u64>,
    base_source_facts: RoleMetadata,
    target_source_facts: RoleMetadata,
    target_source_facts_descriptor: [u8; PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES],
    structural_splice: Option<DecodedExactBaseStructuralSplice>,
}

enum DecodedExactBaseStructuralSplice {
    Blocks(DecodedExactBaseBlockSplice),
    RecursiveGreen(DecodedExactBaseRecursiveGreenSplice),
}

#[derive(Clone, Copy)]
enum ExactBaseStructuralOperationKind {
    Blocks,
    RecursiveGreen,
}

struct DecodedExactBaseBlockSplice {
    virtual_ordinal: u64,
    claim: M11BlockSequenceHostSpliceClaim,
    base_green_metadata: RoleMetadata,
    base_projection_metadata: RoleMetadata,
    target_green_metadata: RoleMetadata,
    target_projection_metadata: RoleMetadata,
}

struct DecodedExactBaseRecursiveGreenSplice {
    virtual_ordinal: u64,
    claim: M11RecursiveGreenHostSpliceClaim,
    base_green_metadata: RoleMetadata,
    target_green_metadata: RoleMetadata,
}

fn encode_exact_base_delta_begin(
    authority: CandidateAuthority,
    delta: &ExactBaseDeltaEncodeState,
) -> Result<Vec<u8>, CandidateHostError> {
    let structural_virtual_root_present =
        delta
            .structural_splice
            .as_ref()
            .is_some_and(|structural| match structural {
                ExactBaseStructuralSpliceEncodeState::Blocks(_) => true,
                ExactBaseStructuralSpliceEncodeState::RecursiveGreen(green) => {
                    green.target_root.is_some()
                }
            });
    let virtual_root_count = 1_u16
        .checked_add(u16::from(delta.target_source_facts_root.is_some()))
        .and_then(|count| count.checked_add(u16::from(structural_virtual_root_present)))
        .ok_or(CandidateHostError::InvalidFrame(
            "exact-base virtual-root count overflow",
        ))?;
    let source_facts_virtual_ordinal = if delta.target_source_facts_root.is_some() {
        1
    } else {
        SNAPSHOT_ABSENT_VIRTUAL_ORDINAL
    };
    let (operation_count, operation_table_bytes, begin_bytes) =
        match delta.structural_splice.as_ref() {
            Some(ExactBaseStructuralSpliceEncodeState::Blocks(_)) => (
                SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_COUNT,
                SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_TABLE_BYTES,
                SNAPSHOT_EXACT_BASE_BLOCK_BEGIN_BYTES,
            ),
            Some(ExactBaseStructuralSpliceEncodeState::RecursiveGreen(_)) => (
                SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_COUNT,
                SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_OPERATION_TABLE_BYTES,
                SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_BEGIN_BYTES,
            ),
            None => (
                SNAPSHOT_EXACT_BASE_OPERATION_COUNT,
                SNAPSHOT_EXACT_BASE_OPERATION_TABLE_BYTES,
                SNAPSHOT_EXACT_BASE_BEGIN_BYTES,
            ),
        };
    let mut output = Vec::new();
    output
        .try_reserve_exact(begin_bytes)
        .map_err(|_| CandidateHostError::AllocationFailed)?;
    output.extend_from_slice(&encode_candidate_header(
        SNAPSHOT_EXACT_BASE_DELTA_BEGIN_TAG,
        authority,
    ));
    output.extend_from_slice(&SNAPSHOT_EXACT_BASE_PROGRAM_SCHEMA.to_le_bytes());
    output.extend_from_slice(&operation_count.to_le_bytes());
    output.extend_from_slice(&virtual_root_count.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(operation_table_bytes)
            .expect("exact-base op table fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(&delta.base_authority.publication.0);
    output.extend_from_slice(&delta.base_authority.source_root.get().to_le_bytes());
    output.extend_from_slice(&delta.base_authority.source_revision.get().to_le_bytes());
    output.extend_from_slice(&delta.base_authority.parse_generation.get().to_le_bytes());
    output.extend_from_slice(&delta.base_authority.source_bytes.to_le_bytes());
    output.extend_from_slice(&delta.base_authority.source_utf16.to_le_bytes());
    output.extend_from_slice(&[0; 8]);

    output.extend_from_slice(&[
        SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_OP,
        SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
        0,
        0,
    ]);
    output.extend_from_slice(
        &u32::try_from(SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_BYTES)
            .expect("exact-base References op fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(&0_u64.to_le_bytes());
    push_role_metadata(&mut output, delta.reused_references);

    output.extend_from_slice(&[
        SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_OP,
        SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
        0,
        0,
    ]);
    output.extend_from_slice(
        &u32::try_from(SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_BYTES)
            .expect("exact-base SourceFacts op fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(&source_facts_virtual_ordinal.to_le_bytes());
    output.extend_from_slice(&delta.base_page_range.start.to_le_bytes());
    output.extend_from_slice(&delta.base_page_range.end.to_le_bytes());
    output.extend_from_slice(&delta.target_page_range.start.to_le_bytes());
    output.extend_from_slice(&delta.target_page_range.end.to_le_bytes());
    push_role_metadata(&mut output, delta.base_source_facts);
    push_role_metadata(&mut output, delta.target_source_facts);
    output.extend_from_slice(&PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES)
            .expect("SourceFacts descriptor fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(&delta.target_source_facts_descriptor);
    output.extend_from_slice(&[0; 7]);
    if let Some(structural) = delta.structural_splice.as_ref() {
        match structural {
            ExactBaseStructuralSpliceEncodeState::Blocks(block) => {
                output.extend_from_slice(&[
                    SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_OP,
                    SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
                    0,
                    0,
                ]);
                output.extend_from_slice(
                    &u32::try_from(SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_BYTES)
                        .expect("exact-base block op fits u32")
                        .to_le_bytes(),
                );
                output.extend_from_slice(&block.virtual_ordinal.to_le_bytes());
                for value in [
                    block.claim.base_entry_range.start,
                    block.claim.base_entry_range.end,
                    block.claim.target_entry_range.start,
                    block.claim.target_entry_range.end,
                    block.claim.base_storage_range.start,
                    block.claim.base_storage_range.end,
                    block.claim.target_storage_range.start,
                    block.claim.target_storage_range.end,
                ] {
                    output.extend_from_slice(&value.to_le_bytes());
                }
                push_role_metadata(&mut output, block.base_green_metadata);
                push_role_metadata(&mut output, block.base_projection_metadata);
                push_role_metadata(&mut output, block.target_green_metadata);
                push_role_metadata(&mut output, block.target_projection_metadata);
                for descriptor in [
                    block.claim.base_green,
                    block.claim.base_projection,
                    block.claim.target_green,
                    block.claim.target_projection,
                ] {
                    output.extend_from_slice(&encode_persistent_m11_block_role_descriptor(
                        descriptor,
                    )?);
                }
            }
            ExactBaseStructuralSpliceEncodeState::RecursiveGreen(green) => {
                output.extend_from_slice(&[
                    SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_OP,
                    SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
                    0,
                    0,
                ]);
                output.extend_from_slice(
                    &u32::try_from(SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_BYTES)
                        .expect("exact-base recursive Green op fits u32")
                        .to_le_bytes(),
                );
                output.extend_from_slice(&green.virtual_ordinal.to_le_bytes());
                for value in [
                    green.claim.base_event_range.start,
                    green.claim.base_event_range.end,
                    green.claim.target_event_range.start,
                    green.claim.target_event_range.end,
                    green.claim.base_storage_range.start,
                    green.claim.base_storage_range.end,
                    green.claim.target_storage_range.start,
                    green.claim.target_storage_range.end,
                ] {
                    output.extend_from_slice(&value.to_le_bytes());
                }
                push_role_metadata(&mut output, green.base_green_metadata);
                push_role_metadata(&mut output, green.target_green_metadata);
                for descriptor in [green.claim.base_descriptor, green.claim.target_descriptor] {
                    output.extend_from_slice(
                        &encode_persistent_m11_recursive_green_role_descriptor(descriptor)?,
                    );
                }
            }
        }
    }
    debug_assert_eq!(output.len(), begin_bytes);
    Ok(output)
}

fn decode_exact_base_delta_begin(
    frame: &[u8],
) -> Result<DecodedExactBaseDeltaBegin, CandidateHostError> {
    if frame.len() < SNAPSHOT_EXACT_BASE_BEGIN_BYTES
        || read_u32(frame, 84)? != SNAPSHOT_EXACT_BASE_PROGRAM_SCHEMA
        || frame[152..160] != [0; 8]
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid exact-base Begin envelope",
        ));
    }
    let operation_count = u16::from_le_bytes(
        frame[88..90]
            .try_into()
            .map_err(|_| CandidateHostError::InvalidFrame("invalid operation count"))?,
    );
    let operation_table_bytes = read_u32(frame, 92)? as usize;
    let structural_operation = match operation_count {
        SNAPSHOT_EXACT_BASE_OPERATION_COUNT
            if operation_table_bytes == SNAPSHOT_EXACT_BASE_OPERATION_TABLE_BYTES
                && frame.len() == SNAPSHOT_EXACT_BASE_BEGIN_BYTES =>
        {
            None
        }
        SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_COUNT
            if operation_table_bytes == SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_TABLE_BYTES
                && frame.len() == SNAPSHOT_EXACT_BASE_BLOCK_BEGIN_BYTES =>
        {
            Some(ExactBaseStructuralOperationKind::Blocks)
        }
        SNAPSHOT_EXACT_BASE_BLOCK_OPERATION_COUNT
            if operation_table_bytes
                == SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_OPERATION_TABLE_BYTES
                && frame.len() == SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_BEGIN_BYTES =>
        {
            Some(ExactBaseStructuralOperationKind::RecursiveGreen)
        }
        _ => {
            return Err(CandidateHostError::InvalidFrame(
                "invalid exact-base operation table",
            ))
        }
    };
    let authority = decode_authority_header(
        &frame[..CANDIDATE_HEADER_BYTES],
        SNAPSHOT_EXACT_BASE_DELTA_BEGIN_TAG,
    )?;
    let virtual_root_count =
        usize::from(u16::from_le_bytes(frame[90..92].try_into().map_err(
            |_| CandidateHostError::InvalidFrame("invalid virtual-root count"),
        )?));
    let base_publication = StrongIdentity::new(
        frame[96..112]
            .try_into()
            .map_err(|_| CandidateHostError::InvalidFrame("invalid base publication"))?,
    )?;
    let base_source_root = SourceRootId::from_wire(read_u64(frame, 112)?)
        .ok_or(CandidateHostError::InvalidFrame("invalid base source root"))?;
    let base_parse_generation = CandidateGeneration::from_wire(read_u64(frame, 128)?).ok_or(
        CandidateHostError::InvalidFrame("invalid base parse generation"),
    )?;
    let base_authority = CandidateAuthority {
        document: authority.document,
        publication: base_publication,
        source_root: base_source_root,
        source_revision: SourceRevision::new(read_u64(frame, 120)?),
        parse_generation: base_parse_generation,
        syntax_profile: authority.syntax_profile,
        source_bytes: read_u64(frame, 136)?,
        source_utf16: read_u64(frame, 144)?,
    };
    if base_authority.document == base_authority.publication {
        return Err(CandidateHostError::InvalidFrame(
            "invalid base publication authority",
        ));
    }

    if frame[160..164]
        != [
            SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_OP,
            SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
            0,
            0,
        ]
        || read_u32(frame, 164)? as usize != SNAPSHOT_EXACT_BASE_REUSE_REFERENCES_BYTES
        || read_u64(frame, 168)? != 0
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid exact-base References operation",
        ));
    }
    let reused_references = read_role_metadata(frame, 176)?;
    if reused_references.role != CandidateRole::References {
        return Err(CandidateHostError::InvalidFrame(
            "exact-base References metadata changed role",
        ));
    }

    if frame[232..236]
        != [
            SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_OP,
            SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
            0,
            0,
        ]
        || read_u32(frame, 236)? as usize != SNAPSHOT_EXACT_BASE_SPLICE_SOURCE_FACTS_BYTES
        || read_u32(frame, 392)? != PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA
        || read_u32(frame, 396)? as usize != PERSISTENT_SOURCE_FACTS_ROLE_DESCRIPTOR_BYTES
        || frame[545..552] != [0; 7]
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid exact-base SourceFacts operation",
        ));
    }
    let source_facts_virtual_ordinal = read_u64(frame, 240)?;
    let base_page_range = read_u64(frame, 248)?..read_u64(frame, 256)?;
    let target_page_range = read_u64(frame, 264)?..read_u64(frame, 272)?;
    if base_page_range.start > base_page_range.end
        || target_page_range.start > target_page_range.end
        || base_page_range.start != target_page_range.start
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid exact-base SourceFacts ranges",
        ));
    }
    let base_source_facts = read_role_metadata(frame, 280)?;
    let target_source_facts = read_role_metadata(frame, 336)?;
    if base_source_facts.role != CandidateRole::SourceFacts
        || target_source_facts.role != CandidateRole::SourceFacts
        || base_source_facts.schema != PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA
        || target_source_facts.schema != PERSISTENT_SOURCE_FACTS_ROLE_SCHEMA
    {
        return Err(CandidateHostError::InvalidFrame(
            "exact-base SourceFacts metadata changed schema",
        ));
    }
    let target_source_facts_descriptor = frame[400..545]
        .try_into()
        .map_err(|_| CandidateHostError::InvalidFrame("invalid target SourceFacts descriptor"))?;
    let structural_splice = match structural_operation {
        Some(ExactBaseStructuralOperationKind::Blocks) => {
            let block_offset = SNAPSHOT_EXACT_BASE_BEGIN_BYTES;
            if frame[block_offset..block_offset + 4]
                != [
                    SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_OP,
                    SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
                    0,
                    0,
                ]
                || read_u32(frame, block_offset + 4)? as usize
                    != SNAPSHOT_EXACT_BASE_SPLICE_BLOCKS_BYTES
            {
                return Err(CandidateHostError::InvalidFrame(
                    "invalid exact-base block operation",
                ));
            }
            let virtual_ordinal = read_u64(frame, block_offset + 8)?;
            let base_entry_range =
                read_u64(frame, block_offset + 16)?..read_u64(frame, block_offset + 24)?;
            let target_entry_range =
                read_u64(frame, block_offset + 32)?..read_u64(frame, block_offset + 40)?;
            let base_storage_range =
                read_u64(frame, block_offset + 48)?..read_u64(frame, block_offset + 56)?;
            let target_storage_range =
                read_u64(frame, block_offset + 64)?..read_u64(frame, block_offset + 72)?;
            let base_green_metadata = read_role_metadata(frame, block_offset + 80)?;
            let base_projection_metadata = read_role_metadata(frame, block_offset + 136)?;
            let target_green_metadata = read_role_metadata(frame, block_offset + 192)?;
            let target_projection_metadata = read_role_metadata(frame, block_offset + 248)?;
            let descriptors_offset = block_offset + 304;
            let descriptor =
                |index: usize,
                 lane: M11BlockRoleLane,
                 source_bytes: u64,
                 source_utf16: u64|
                 -> Result<PersistentM11BlockRoleDescriptor, CandidateHostError> {
                    let start = descriptors_offset
                        + index
                            .checked_mul(PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES)
                            .ok_or(CandidateHostError::InvalidFrame(
                                "block descriptor offset overflow",
                            ))?;
                    let end = start + PERSISTENT_BLOCK_ROLE_DESCRIPTOR_BYTES;
                    decode_persistent_m11_block_role_descriptor(
                        &frame[start..end],
                        lane,
                        source_bytes,
                        source_utf16,
                    )
                    .map_err(Into::into)
                };
            let base_green = descriptor(
                0,
                M11BlockRoleLane::Green,
                base_authority.source_bytes,
                base_authority.source_utf16,
            )?;
            let base_projection = descriptor(
                1,
                M11BlockRoleLane::Projection,
                base_authority.source_bytes,
                base_authority.source_utf16,
            )?;
            let target_green = descriptor(
                2,
                M11BlockRoleLane::Green,
                authority.source_bytes,
                authority.source_utf16,
            )?;
            let target_projection = descriptor(
                3,
                M11BlockRoleLane::Projection,
                authority.source_bytes,
                authority.source_utf16,
            )?;
            if base_green_metadata.role != CandidateRole::Green
                || base_green_metadata.schema != PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA
                || base_projection_metadata.role != CandidateRole::Projection
                || base_projection_metadata.schema != PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
                || target_green_metadata.role != CandidateRole::Green
                || target_green_metadata.schema != PERSISTENT_BLOCK_GREEN_ROLE_SCHEMA
                || target_projection_metadata.role != CandidateRole::Projection
                || target_projection_metadata.schema != PERSISTENT_BLOCK_PROJECTION_ROLE_SCHEMA
            {
                return Err(CandidateHostError::InvalidFrame(
                    "exact-base block metadata changed role",
                ));
            }
            Some(DecodedExactBaseStructuralSplice::Blocks(
                DecodedExactBaseBlockSplice {
                    virtual_ordinal,
                    claim: M11BlockSequenceHostSpliceClaim {
                        base_entry_range,
                        target_entry_range,
                        base_storage_range,
                        target_storage_range,
                        base_green,
                        base_projection,
                        target_green,
                        target_projection,
                    },
                    base_green_metadata,
                    base_projection_metadata,
                    target_green_metadata,
                    target_projection_metadata,
                },
            ))
        }
        Some(ExactBaseStructuralOperationKind::RecursiveGreen) => {
            let green_offset = SNAPSHOT_EXACT_BASE_BEGIN_BYTES;
            if frame[green_offset..green_offset + 4]
                != [
                    SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_OP,
                    SNAPSHOT_EXACT_BASE_OPERATION_VERSION,
                    0,
                    0,
                ]
                || read_u32(frame, green_offset + 4)? as usize
                    != SNAPSHOT_EXACT_BASE_SPLICE_RECURSIVE_GREEN_BYTES
            {
                return Err(CandidateHostError::InvalidFrame(
                    "invalid exact-base recursive Green operation",
                ));
            }
            let virtual_ordinal = read_u64(frame, green_offset + 8)?;
            let base_event_range =
                read_u64(frame, green_offset + 16)?..read_u64(frame, green_offset + 24)?;
            let target_event_range =
                read_u64(frame, green_offset + 32)?..read_u64(frame, green_offset + 40)?;
            let base_storage_range =
                read_u64(frame, green_offset + 48)?..read_u64(frame, green_offset + 56)?;
            let target_storage_range =
                read_u64(frame, green_offset + 64)?..read_u64(frame, green_offset + 72)?;
            let base_green_metadata = read_role_metadata(frame, green_offset + 80)?;
            let target_green_metadata = read_role_metadata(frame, green_offset + 136)?;
            let descriptors_offset = green_offset + 192;
            let base_descriptor = decode_persistent_m11_recursive_green_role_descriptor(
                &frame[descriptors_offset
                    ..descriptors_offset + PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES],
                base_authority.source_bytes,
                base_authority.source_utf16,
            )?;
            let target_descriptor_offset =
                descriptors_offset + PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES;
            let target_descriptor = decode_persistent_m11_recursive_green_role_descriptor(
                &frame[target_descriptor_offset
                    ..target_descriptor_offset + PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES],
                authority.source_bytes,
                authority.source_utf16,
            )?;
            if base_green_metadata.role != CandidateRole::Green
                || base_green_metadata.schema != PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
                || target_green_metadata.role != CandidateRole::Green
                || target_green_metadata.schema != PERSISTENT_RECURSIVE_GREEN_ROLE_SCHEMA
            {
                return Err(CandidateHostError::InvalidFrame(
                    "exact-base recursive Green metadata changed role",
                ));
            }
            Some(DecodedExactBaseStructuralSplice::RecursiveGreen(
                DecodedExactBaseRecursiveGreenSplice {
                    virtual_ordinal,
                    claim: M11RecursiveGreenHostSpliceClaim {
                        base_event_range,
                        target_event_range,
                        base_storage_range,
                        target_storage_range,
                        base_descriptor,
                        target_descriptor,
                    },
                    base_green_metadata,
                    target_green_metadata,
                },
            ))
        }
        None => None,
    };
    Ok(DecodedExactBaseDeltaBegin {
        authority,
        base_authority,
        virtual_root_count,
        reused_references,
        source_facts_virtual_ordinal,
        base_page_range,
        target_page_range,
        base_source_facts,
        target_source_facts,
        target_source_facts_descriptor,
        structural_splice,
    })
}

fn encode_source_facts_replacement_frame(
    ordinal: u64,
    payload: &[u8],
) -> Result<Vec<u8>, CandidateHostError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(SNAPSHOT_SOURCE_FACTS_REPLACEMENT_HEADER_BYTES + payload.len())
        .map_err(|_| CandidateHostError::AllocationFailed)?;
    output.extend_from_slice(&[
        SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG,
        CANDIDATE_FORMAT_VERSION,
        SNAPSHOT_SOURCE_FACTS_OPERATION_INDEX,
        0,
    ]);
    output.extend_from_slice(&ordinal.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(|_| CandidateHostError::InvalidFrame("replacement payload exceeds u32"))?
            .to_le_bytes(),
    );
    output.extend_from_slice(payload);
    Ok(output)
}

struct DecodedSourceFactsReplacement<'a> {
    payload: &'a [u8],
}

fn decode_source_facts_replacement_frame(
    frame: &[u8],
    expected_ordinal: u64,
) -> Result<DecodedSourceFactsReplacement<'_>, CandidateHostError> {
    if frame.len() < SNAPSHOT_SOURCE_FACTS_REPLACEMENT_HEADER_BYTES
        || frame[..4]
            != [
                SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG,
                CANDIDATE_FORMAT_VERSION,
                SNAPSHOT_SOURCE_FACTS_OPERATION_INDEX,
                0,
            ]
        || read_u64(frame, 4)? != expected_ordinal
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid SourceFacts replacement frame",
        ));
    }
    let payload_len = usize::try_from(read_u32(frame, 12)?)
        .map_err(|_| CandidateHostError::InvalidFrame("replacement payload length overflow"))?;
    if payload_len == 0
        || SNAPSHOT_SOURCE_FACTS_REPLACEMENT_HEADER_BYTES
            .checked_add(payload_len)
            .is_none_or(|expected| expected != frame.len())
    {
        return Err(CandidateHostError::InvalidFrame(
            "SourceFacts replacement payload length changed",
        ));
    }
    Ok(DecodedSourceFactsReplacement {
        payload: &frame[SNAPSHOT_SOURCE_FACTS_REPLACEMENT_HEADER_BYTES..],
    })
}

fn encode_block_replacement_frame(
    ordinal: u64,
    payload: &[u8],
) -> Result<Vec<u8>, CandidateHostError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(SNAPSHOT_BLOCK_REPLACEMENT_HEADER_BYTES + payload.len())
        .map_err(|_| CandidateHostError::AllocationFailed)?;
    output.extend_from_slice(&[
        SNAPSHOT_BLOCK_REPLACEMENT_TAG,
        CANDIDATE_FORMAT_VERSION,
        SNAPSHOT_BLOCK_OPERATION_INDEX,
        0,
    ]);
    output.extend_from_slice(&ordinal.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(|_| CandidateHostError::InvalidFrame("replacement payload exceeds u32"))?
            .to_le_bytes(),
    );
    output.extend_from_slice(payload);
    Ok(output)
}

struct DecodedBlockReplacement<'a> {
    payload: &'a [u8],
}

fn decode_block_replacement_frame(
    frame: &[u8],
    expected_ordinal: u64,
) -> Result<DecodedBlockReplacement<'_>, CandidateHostError> {
    if frame.len() < SNAPSHOT_BLOCK_REPLACEMENT_HEADER_BYTES
        || frame[..4]
            != [
                SNAPSHOT_BLOCK_REPLACEMENT_TAG,
                CANDIDATE_FORMAT_VERSION,
                SNAPSHOT_BLOCK_OPERATION_INDEX,
                0,
            ]
        || read_u64(frame, 4)? != expected_ordinal
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid block replacement frame",
        ));
    }
    let payload_len = usize::try_from(read_u32(frame, 12)?)
        .map_err(|_| CandidateHostError::InvalidFrame("replacement payload length overflow"))?;
    if payload_len == 0
        || SNAPSHOT_BLOCK_REPLACEMENT_HEADER_BYTES
            .checked_add(payload_len)
            .is_none_or(|expected| expected != frame.len())
    {
        return Err(CandidateHostError::InvalidFrame(
            "block replacement payload length changed",
        ));
    }
    Ok(DecodedBlockReplacement {
        payload: &frame[SNAPSHOT_BLOCK_REPLACEMENT_HEADER_BYTES..],
    })
}

fn encode_recursive_green_replacement_frame(
    ordinal: u64,
    payload: &[u8],
) -> Result<Vec<u8>, CandidateHostError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES + payload.len())
        .map_err(|_| CandidateHostError::AllocationFailed)?;
    output.extend_from_slice(&[
        SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_TAG,
        CANDIDATE_FORMAT_VERSION,
        SNAPSHOT_RECURSIVE_GREEN_OPERATION_INDEX,
        0,
    ]);
    output.extend_from_slice(&ordinal.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(|_| CandidateHostError::InvalidFrame("replacement payload exceeds u32"))?
            .to_le_bytes(),
    );
    output.extend_from_slice(payload);
    Ok(output)
}

struct DecodedRecursiveGreenReplacement<'a> {
    payload: &'a [u8],
}

fn decode_recursive_green_replacement_frame(
    frame: &[u8],
    expected_ordinal: u64,
) -> Result<DecodedRecursiveGreenReplacement<'_>, CandidateHostError> {
    if frame.len() < SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES
        || frame[..4]
            != [
                SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_TAG,
                CANDIDATE_FORMAT_VERSION,
                SNAPSHOT_RECURSIVE_GREEN_OPERATION_INDEX,
                0,
            ]
        || read_u64(frame, 4)? != expected_ordinal
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid recursive Green replacement frame",
        ));
    }
    let payload_len = usize::try_from(read_u32(frame, 12)?)
        .map_err(|_| CandidateHostError::InvalidFrame("replacement payload length overflow"))?;
    if payload_len == 0
        || SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES
            .checked_add(payload_len)
            .is_none_or(|expected| expected != frame.len())
    {
        return Err(CandidateHostError::InvalidFrame(
            "recursive Green replacement payload length changed",
        ));
    }
    Ok(DecodedRecursiveGreenReplacement {
        payload: &frame[SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES..],
    })
}

struct DecodedNode<'a> {
    child_count: usize,
    child_ordinals: &'a [u8],
    payload: &'a [u8],
}

impl DecodedNode<'_> {
    fn child_ordinal(&self, index: usize) -> Result<u64, CandidateHostError> {
        let offset = index
            .checked_mul(SNAPSHOT_CHILD_ORDINAL_BYTES)
            .ok_or(CandidateHostError::InvalidFrame("child ordinal overflow"))?;
        read_u64(self.child_ordinals, offset).map_err(Into::into)
    }
}

struct SnapshotEndClaim {
    /// Program ordinal count. A References delta includes its one virtual
    /// canonical-root ordinal even though no Node frame carries that closure.
    nodes: u64,
    payload_bytes: u64,
    wire_bytes: u64,
    digest: [u8; SNAPSHOT_DIGEST_BYTES],
}

/// Decodes one complete snapshot frame through the engine-owned schema.
///
/// Node payload headers are authority-checked later by the active host offer;
/// this classification establishes only the closed frame shape, its own
/// ordinal, and whether it is one canonical role-record frame.
pub(crate) fn classify_snapshot_frame(
    frame: &[u8],
) -> Result<SnapshotFrameMetadata, CandidateHostError> {
    let Some(tag) = frame.first().copied() else {
        return Err(CandidateHostError::InvalidFrame("empty snapshot frame"));
    };
    match tag {
        SNAPSHOT_BEGIN_TAG | SNAPSHOT_REFERENCES_DELTA_BEGIN_TAG => {
            let _ = decode_begin_frame(frame)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::Begin,
                node_ordinal: None,
                canonical_record_count: 0,
                canonical_stream_digest256: None,
            })
        }
        SNAPSHOT_EXACT_BASE_DELTA_BEGIN_TAG => {
            let _ = decode_exact_base_delta_begin(frame)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::Begin,
                node_ordinal: None,
                canonical_record_count: 0,
                canonical_stream_digest256: None,
            })
        }
        SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG => {
            let page_ordinal = read_u64(frame, 4)?;
            let _ = decode_source_facts_replacement_frame(frame, page_ordinal)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::SourceFactsReplacementPage,
                node_ordinal: None,
                canonical_record_count: 1,
                canonical_stream_digest256: None,
            })
        }
        SNAPSHOT_BLOCK_REPLACEMENT_TAG => {
            let page_ordinal = read_u64(frame, 4)?;
            let _ = decode_block_replacement_frame(frame, page_ordinal)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::BlockSequenceReplacementPage,
                node_ordinal: None,
                canonical_record_count: 1,
                canonical_stream_digest256: None,
            })
        }
        SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_TAG => {
            let page_ordinal = read_u64(frame, 4)?;
            let _ = decode_recursive_green_replacement_frame(frame, page_ordinal)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::RecursiveGreenReplacementPage,
                node_ordinal: None,
                canonical_record_count: 1,
                canonical_stream_digest256: None,
            })
        }
        SNAPSHOT_NODE_TAG => {
            let ordinal = read_u64(frame, 4)?;
            let node = decode_node_frame(frame, ordinal)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::Node,
                node_ordinal: Some(ordinal),
                canonical_record_count: canonical_role_record_count(node.payload),
                canonical_stream_digest256: None,
            })
        }
        SNAPSHOT_END_TAG => {
            let claim = decode_end_frame(frame)?;
            Ok(SnapshotFrameMetadata {
                kind: SnapshotFrameKind::End,
                node_ordinal: None,
                canonical_record_count: 0,
                canonical_stream_digest256: Some(claim.digest),
            })
        }
        _ => Err(CandidateHostError::InvalidFrame(
            "unknown snapshot frame tag",
        )),
    }
}

fn snapshot_hasher() -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.candidate.snapshot.v1\0");
    hasher
}

fn encode_node_frame(
    ordinal: u64,
    child_ordinals: &[u64],
    payload: &[u8],
) -> Result<Vec<u8>, CandidateHostError> {
    let child_bytes = child_ordinals
        .len()
        .checked_mul(SNAPSHOT_CHILD_ORDINAL_BYTES)
        .ok_or(CandidateHostError::InvalidFrame(
            "child ordinal bytes overflow",
        ))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(SNAPSHOT_NODE_HEADER_BYTES + child_bytes + payload.len())
        .map_err(|_| CandidateHostError::AllocationFailed)?;
    output.extend_from_slice(&[SNAPSHOT_NODE_TAG, CANDIDATE_FORMAT_VERSION, 0, 0]);
    output.extend_from_slice(&ordinal.to_le_bytes());
    output.extend_from_slice(
        &u16::try_from(child_ordinals.len())
            .map_err(|_| CandidateHostError::InvalidFrame("child count exceeds u16"))?
            .to_le_bytes(),
    );
    output.extend_from_slice(&[0; 2]);
    output.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(|_| CandidateHostError::InvalidFrame("payload exceeds u32"))?
            .to_le_bytes(),
    );
    for child in child_ordinals {
        output.extend_from_slice(&child.to_le_bytes());
    }
    output.extend_from_slice(payload);
    Ok(output)
}

fn decode_node_frame(
    frame: &[u8],
    expected_ordinal: u64,
) -> Result<DecodedNode<'_>, CandidateHostError> {
    if frame.len() < SNAPSHOT_NODE_HEADER_BYTES
        || frame[..4] != [SNAPSHOT_NODE_TAG, CANDIDATE_FORMAT_VERSION, 0, 0]
        || read_u64(frame, 4)? != expected_ordinal
        || frame[14..16] != [0; 2]
    {
        return Err(CandidateHostError::InvalidFrame(
            "node header, ordinal, or reserved bytes changed",
        ));
    }
    let child_count =
        usize::from(u16::from_le_bytes(frame[12..14].try_into().map_err(
            |_| CandidateHostError::InvalidFrame("invalid child count"),
        )?));
    let payload_len = usize::try_from(read_u32(frame, 16)?)
        .map_err(|_| CandidateHostError::InvalidFrame("payload length overflow"))?;
    let child_bytes = child_count
        .checked_mul(SNAPSHOT_CHILD_ORDINAL_BYTES)
        .ok_or(CandidateHostError::InvalidFrame(
            "child ordinal bytes overflow",
        ))?;
    let payload_offset = SNAPSHOT_NODE_HEADER_BYTES.checked_add(child_bytes).ok_or(
        CandidateHostError::InvalidFrame("node frame length overflow"),
    )?;
    if payload_len == 0
        || payload_offset
            .checked_add(payload_len)
            .is_none_or(|expected| frame.len() != expected)
    {
        return Err(CandidateHostError::InvalidFrame(
            "node payload length changed",
        ));
    }
    Ok(DecodedNode {
        child_count,
        child_ordinals: &frame[SNAPSHOT_NODE_HEADER_BYTES..payload_offset],
        payload: &frame[payload_offset..],
    })
}

fn encode_end_frame(
    nodes: u64,
    payload_bytes: u64,
    wire_bytes: u64,
    digest: [u8; SNAPSHOT_DIGEST_BYTES],
) -> Vec<u8> {
    let mut output = Vec::with_capacity(SNAPSHOT_END_BYTES);
    output.extend_from_slice(&[SNAPSHOT_END_TAG, CANDIDATE_FORMAT_VERSION, 0, 0]);
    output.extend_from_slice(&nodes.to_le_bytes());
    output.extend_from_slice(&payload_bytes.to_le_bytes());
    output.extend_from_slice(&wire_bytes.to_le_bytes());
    output.extend_from_slice(&digest);
    debug_assert_eq!(output.len(), SNAPSHOT_END_BYTES);
    output
}

fn decode_end_frame(frame: &[u8]) -> Result<SnapshotEndClaim, CandidateHostError> {
    if frame.len() != SNAPSHOT_END_BYTES
        || frame[..4] != [SNAPSHOT_END_TAG, CANDIDATE_FORMAT_VERSION, 0, 0]
    {
        return Err(CandidateHostError::InvalidFrame("invalid end frame"));
    }
    let digest = frame[28..]
        .try_into()
        .map_err(|_| CandidateHostError::InvalidFrame("invalid snapshot digest"))?;
    Ok(SnapshotEndClaim {
        nodes: read_u64(frame, 4)?,
        payload_bytes: read_u64(frame, 12)?,
        wire_bytes: read_u64(frame, 20)?,
        digest,
    })
}

struct DecodedBegin {
    authority: CandidateAuthority,
    program: SnapshotProgram,
}

fn decode_begin_frame(frame: &[u8]) -> Result<DecodedBegin, CandidateHostError> {
    let program = match frame.first().copied() {
        Some(SNAPSHOT_BEGIN_TAG) => SnapshotProgram::Full,
        Some(SNAPSHOT_REFERENCES_DELTA_BEGIN_TAG) => SnapshotProgram::ExactBaseReferences,
        _ => return Err(CandidateHostError::InvalidFrame("invalid begin frame")),
    };
    if frame.len() != CANDIDATE_HEADER_BYTES {
        return Err(CandidateHostError::InvalidFrame("invalid begin frame"));
    }
    let authority = decode_authority_header(frame, frame[0])?;
    Ok(DecodedBegin { authority, program })
}

fn decode_authority_header(
    frame: &[u8],
    expected_tag: u8,
) -> Result<CandidateAuthority, CandidateHostError> {
    if frame.len() != CANDIDATE_HEADER_BYTES
        || frame[..4] != [expected_tag, CANDIDATE_FORMAT_VERSION, 0, 0]
        || frame[64..68] != [0; 4]
    {
        return Err(CandidateHostError::InvalidFrame(
            "invalid candidate authority header",
        ));
    }
    let document = StrongIdentity::new(
        frame[4..20]
            .try_into()
            .map_err(|_| CandidateHostError::InvalidFrame("invalid document identity"))?,
    )?;
    let publication = StrongIdentity::new(
        frame[20..36]
            .try_into()
            .map_err(|_| CandidateHostError::InvalidFrame("invalid publication identity"))?,
    )?;
    let source_root = SourceRootId::from_wire(read_u64(frame, 36)?)
        .ok_or(CandidateHostError::InvalidFrame("invalid source root"))?;
    let source_revision = SourceRevision::new(read_u64(frame, 44)?);
    let parse_generation = CandidateGeneration::from_wire(read_u64(frame, 52)?)
        .ok_or(CandidateHostError::InvalidFrame("invalid parse generation"))?;
    let syntax_profile = read_u32(frame, 60)?;
    let source_bytes = read_u64(frame, 68)?;
    let source_utf16 = read_u64(frame, 76)?;
    if document == publication || syntax_profile == 0 {
        return Err(CandidateHostError::InvalidFrame(
            "invalid publication authority",
        ));
    }
    let authority = CandidateAuthority {
        document,
        publication,
        source_root,
        source_revision,
        parse_generation,
        syntax_profile,
        source_bytes,
        source_utf16,
    };
    decode_candidate_header(frame, expected_tag, authority)?;
    Ok(authority)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_sequence::{
        splice_m11_block_sequence_atomic, M11BlockRoleRecord, M11BlockSequenceBuild,
        M11BlockSequenceBuildStatus, M11BlockSequenceEntry, M11BlockSequenceRoot,
        M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX,
    };
    use crate::candidate_manifest::{
        CandidateManifestAssembler, CanonicalRoleInputs, ManifestPoll,
    };
    use crate::document::{DocumentRuntime, DocumentRuntimeConfig};
    use crate::recursive_green::{
        M11RecursiveGreenBuild, M11RecursiveGreenBuildStatus, M11RecursiveGreenClosedChild,
        M11RecursiveGreenCoveragePart, M11RecursiveGreenEvent, M11RecursiveGreenFrameId,
        M11RecursiveGreenKind, M11RecursiveGreenLogicalAction, M11RecursiveGreenRoot,
        M11RecursiveGreenSourceMetric, M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS,
    };
    use crate::reference_root::{
        AuthoritativeReferenceFact, ReferenceRootLimits, ReferenceSourceRange, INLINE_FACT_TAG,
    };
    use crate::source::SourceStore;
    use crate::source_facts::{
        ParserProfileId, PersistentSourceFactsBuild, PersistentSourceFactsBuildOutput,
        PersistentSourceFactsBuildPoll, PersistentSourceFactsRoot, SourceFactsPoll,
        SourceFactsRootBuilder, SourceFactsRootLimits, SourceFactsScanProfile, SourceFactsScanner,
    };
    use std::collections::HashSet;
    use std::ops::Range;

    type SnapshotFrames = (Box<[u8]>, Vec<Box<[u8]>>, Box<[u8]>);

    fn range(start: u64, end: u64) -> ReferenceSourceRange {
        ReferenceSourceRange {
            bytes: Range { start, end },
            utf16: Range { start, end },
        }
    }

    fn build_publication(
        source: SourceVersion,
        document: StrongIdentity,
        publication_byte: u8,
        parse_generation: u64,
        green: &[u8],
    ) -> (PageArena, PublishedManifest, CandidateAuthority) {
        let authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([publication_byte; 16]).expect("publication identity"),
            source,
            CandidateGeneration::from_wire(parse_generation).expect("parse generation"),
            1,
        )
        .expect("authority");
        let mut arena = PageArena::new(ArenaLimits::default()).expect("candidate arena");
        let arena_limits = arena.limits();
        let mut assembler = CandidateManifestAssembler::new(
            &mut arena,
            authority,
            ReferenceRootLimits {
                arena: arena_limits,
                max_occurrences: 8,
                ..ReferenceRootLimits::default()
            },
            CanonicalRoleInputs::single(&b"source-facts"[..], green, &b"projection"[..]),
        )
        .expect("assembler");
        assembler
            .offer_reference(
                &arena,
                AuthoritativeReferenceFact {
                    authority,
                    source: range(0, 16),
                    label_source: range(1, 4),
                    destination_source: range(6, 12),
                    title_source: None,
                    normalized_label: Box::from(&b"x"[..]),
                    cooked_destination: Box::from(&b"/one"[..]),
                    cooked_title: None,
                    _not_sync: PhantomData,
                },
            )
            .expect("reference");
        while !assembler.references_idle() {
            assert!(matches!(
                assembler.poll(&mut arena, 1).expect("reference poll"),
                ManifestPoll::Pending { .. }
            ));
        }
        assembler
            .finish_references(&arena)
            .expect("finish references");
        loop {
            match assembler.poll(&mut arena, 1).expect("manifest poll") {
                ManifestPoll::Pending { .. } => {}
                ManifestPoll::Published { publication, .. } => {
                    drop(assembler);
                    return (arena, publication, authority);
                }
                ManifestPoll::Aborting => panic!("candidate unexpectedly aborted"),
            }
        }
    }

    fn finish_assembler(
        arena: &mut PageArena,
        mut assembler: CandidateManifestAssembler,
    ) -> PublishedManifest {
        loop {
            match assembler.poll(arena, 31).expect("manifest poll") {
                ManifestPoll::Pending { .. } => {}
                ManifestPoll::Published { publication, .. } => return publication,
                ManifestPoll::Aborting => panic!("candidate unexpectedly aborted"),
            }
        }
    }

    struct ReusedPair {
        source: SourceVersion,
        target_source: SourceVersion,
        arena: PageArena,
        base: PublishedManifest,
        target: PublishedManifest,
        document: StrongIdentity,
    }

    fn build_reused_pair(reference_count: usize) -> ReusedPair {
        let source_bytes = reference_count
            .checked_mul(16)
            .and_then(|bytes| bytes.checked_add(1))
            .expect("bounded source size");
        let mut source_store = SourceStore::new(&"a".repeat(source_bytes)).expect("base source");
        let source = source_store.version();
        let prepared = source_store
            .prepare_edit(source, source_bytes - 1..source_bytes, "b")
            .expect("tail edit");
        let _retired = source_store
            .commit_prepared_edit(prepared)
            .expect("commit tail edit");
        let target_source = source_store.version();
        let document = StrongIdentity::new([29; 16]).expect("document");
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([30; 16]).expect("base publication"),
            source,
            CandidateGeneration::from_wire(1).expect("base generation"),
            1,
        )
        .expect("base authority");
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([31; 16]).expect("target publication"),
            target_source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");
        let limits = ArenaLimits {
            max_slots: reference_count.saturating_mul(3).max(128),
            max_live_payload_bytes: reference_count.saturating_mul(512).max(256 * 1024),
            ..ArenaLimits::default()
        };
        let mut arena = PageArena::new(limits).expect("producer arena");
        let reference_limits = ReferenceRootLimits {
            arena: limits,
            max_occurrences: u64::try_from(reference_count.max(1))
                .expect("reference count fits u64"),
            ..ReferenceRootLimits::default()
        };
        let mut base_assembler = CandidateManifestAssembler::new(
            &mut arena,
            base_authority,
            reference_limits,
            CanonicalRoleInputs::single(
                &b"source-facts:base"[..],
                &b"green:base"[..],
                &b"projection:base"[..],
            ),
        )
        .expect("base assembler");
        for ordinal in 0..reference_count {
            let start = u64::try_from(ordinal * 16).expect("reference start");
            base_assembler
                .offer_reference(
                    &arena,
                    AuthoritativeReferenceFact {
                        authority: base_authority,
                        source: range(start, start + 15),
                        label_source: range(start + 1, start + 4),
                        destination_source: range(start + 6, start + 12),
                        title_source: None,
                        normalized_label: format!("label-{ordinal}")
                            .into_bytes()
                            .into_boxed_slice(),
                        cooked_destination: Box::from(&b"/same"[..]),
                        cooked_title: None,
                        _not_sync: PhantomData,
                    },
                )
                .expect("reference");
            while !base_assembler.references_idle() {
                assert!(matches!(
                    base_assembler.poll(&mut arena, 31).expect("reference poll"),
                    ManifestPoll::Pending { .. }
                ));
            }
        }
        base_assembler
            .finish_references(&arena)
            .expect("finish base references");
        let base = finish_assembler(&mut arena, base_assembler);
        let target_assembler = CandidateManifestAssembler::new_reusing_references(
            &mut arena,
            target_authority,
            reference_limits,
            CanonicalRoleInputs::single(
                &b"source-facts:target"[..],
                &b"green:target"[..],
                &b"projection:target"[..],
            ),
            &base,
        )
        .expect("target assembler");
        let target = finish_assembler(&mut arena, target_assembler);
        ReusedPair {
            source,
            target_source,
            arena,
            base,
            target,
            document,
        }
    }

    fn collect_stream(arena: &PageArena, publication: &PublishedManifest) -> SnapshotFrames {
        let mut encoder =
            CandidateSnapshotEncoder::new(arena, publication).expect("snapshot encoder");
        let begin = encoder.begin_frame().expect("begin frame");
        let mut nodes = Vec::new();
        loop {
            match encoder.poll(1).expect("encode poll") {
                CandidateSnapshotEncodePoll::Pending { transitions } => {
                    assert_eq!(transitions, 1);
                }
                CandidateSnapshotEncodePoll::Frame { transitions, bytes } => {
                    assert_eq!(transitions, 1);
                    assert!(bytes.len() <= M11_MAXIMUM_SNAPSHOT_FRAME_BYTES);
                    nodes.push(bytes);
                }
                CandidateSnapshotEncodePoll::ReplayRequired { .. } => {
                    panic!("full snapshot unexpectedly required replay")
                }
                CandidateSnapshotEncodePoll::Complete { transitions, bytes } => {
                    assert!(transitions <= 1);
                    return (begin, nodes, bytes);
                }
            }
        }
    }

    struct DeltaFrames {
        begin: Box<[u8]>,
        nodes: Vec<Box<[u8]>>,
        end: Box<[u8]>,
        transitions: usize,
        encoded_bytes: usize,
    }

    fn collect_delta_stream(arena: &PageArena, publication: &PublishedManifest) -> DeltaFrames {
        let mut encoder = CandidateSnapshotEncoder::new_references_delta(arena, publication)
            .expect("delta encoder");
        let begin = encoder.begin_frame().expect("delta begin");
        assert_eq!(
            decode_begin_frame(&begin)
                .expect("decode delta Begin")
                .program,
            SnapshotProgram::ExactBaseReferences
        );
        let mut nodes = Vec::new();
        let mut transitions = 0;
        let end = loop {
            match encoder.poll(1).expect("delta encode poll") {
                CandidateSnapshotEncodePoll::Pending {
                    transitions: consumed,
                } => transitions += consumed,
                CandidateSnapshotEncodePoll::Frame {
                    transitions: consumed,
                    bytes,
                } => {
                    transitions += consumed;
                    nodes.push(bytes);
                }
                CandidateSnapshotEncodePoll::ReplayRequired { .. } => {
                    panic!("legacy References delta unexpectedly required replay")
                }
                CandidateSnapshotEncodePoll::Complete {
                    transitions: consumed,
                    bytes,
                } => {
                    transitions += consumed;
                    break bytes;
                }
            }
        };
        let encoded_bytes =
            begin.len() + nodes.iter().map(|node| node.len()).sum::<usize>() + end.len();
        DeltaFrames {
            begin,
            nodes,
            end,
            transitions,
            encoded_bytes,
        }
    }

    #[test]
    fn m11_frame_ceiling_includes_maximum_payload_and_every_child_ordinal() {
        let payload = vec![0_u8; ARENA_PAGE_BYTES];
        let child_ordinals = [0_u64; M11_MAXIMUM_SNAPSHOT_CHILDREN];
        let frame = encode_node_frame(
            M11_MAXIMUM_SNAPSHOT_CHILDREN as u64,
            &child_ordinals,
            &payload,
        )
        .expect("maximum M1.1 Node frame");

        assert_eq!(M11_MAXIMUM_SNAPSHOT_FRAME_BYTES, 5_140);
        assert_eq!(frame.len(), M11_MAXIMUM_SNAPSHOT_FRAME_BYTES);
        assert_eq!(
            crate::m11_host::M11_HOST_MAXIMUM_FRAME_BYTES,
            M11_MAXIMUM_SNAPSHOT_FRAME_BYTES
        );
        assert_eq!(
            crate::m11_host::M11_HOST_MAXIMUM_PROGRAM_CHILDREN,
            M11_MAXIMUM_SNAPSHOT_CHILDREN
        );
        #[cfg(feature = "parser-internal")]
        {
            assert_eq!(
                crate::parser_internal::M11_MAX_SNAPSHOT_FRAME_BYTES,
                M11_MAXIMUM_SNAPSHOT_FRAME_BYTES
            );
            assert_eq!(
                crate::parser_internal::M11_MAX_ROLE_RECORDS,
                M11_MAXIMUM_SNAPSHOT_CHILDREN
            );
        }

        let decoded = decode_node_frame(&frame, M11_MAXIMUM_SNAPSHOT_CHILDREN as u64)
            .expect("maximum frame decodes");
        assert_eq!(decoded.child_count, M11_MAXIMUM_SNAPSHOT_CHILDREN);
        assert_eq!(decoded.payload.len(), ARENA_PAGE_BYTES);
        assert_eq!(
            classify_snapshot_frame(&frame)
                .expect("maximum frame classifies")
                .kind,
            SnapshotFrameKind::Node
        );

        let predecessor_frame =
            encode_node_frame(1, &[0], &payload).expect("predecessor-bearing blob shape");
        assert_eq!(predecessor_frame.len(), 4_124);
        assert!(predecessor_frame.len() <= M11_MAXIMUM_SNAPSHOT_FRAME_BYTES);
    }

    fn install_stream(
        host: &mut CandidateHostStore,
        begin: &[u8],
        nodes: &[Box<[u8]>],
        end: &[u8],
    ) -> InstalledCandidateSnapshot {
        host.begin_snapshot(begin).expect("begin snapshot");
        for node in nodes {
            host.offer_node(node).expect("offer node");
        }
        host.finish_snapshot(end).expect("finish snapshot");
        loop {
            let poll = host.poll_install(1).expect("install poll");
            assert!(poll.transitions <= 1);
            if let Some(snapshot) = poll.installed {
                return snapshot;
            }
        }
    }

    fn install_delta_stream(
        host: &mut CandidateHostStore,
        base: InstalledCandidateSnapshot,
        frames: &DeltaFrames,
    ) -> (InstalledCandidateSnapshot, usize) {
        host.begin_references_delta(base, &frames.begin)
            .expect("begin References delta");
        for node in &frames.nodes {
            host.offer_node(node).expect("offer delta node");
        }
        host.finish_snapshot(&frames.end).expect("finish delta");
        let mut transitions = 0;
        loop {
            let poll = host.poll_install(31).expect("delta install poll");
            transitions += poll.transitions;
            if let Some(snapshot) = poll.installed {
                return (snapshot, transitions);
            }
        }
    }

    fn installed_manifest_shape(
        host: &CandidateHostStore,
    ) -> (ArenaId, ArenaId, ArenaId, RoleMetadata) {
        let installed = host.installed.as_ref().expect("installed root");
        let manifest = installed.root.id();
        let descriptor = decode_manifest_descriptor(&host.arena, manifest, installed.authority)
            .expect("installed descriptor");
        let index = role_index(CandidateRole::References);
        let wrapper = descriptor.children[index];
        let canonical = host.arena.child_at(wrapper, 0).expect("References child");
        (manifest, wrapper, canonical, descriptor.metadata[index])
    }

    fn reachable(arena: &PageArena, root: ArenaId) -> HashSet<ArenaId> {
        let mut seen = HashSet::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            for index in 0..arena.child_count(node).expect("child count") {
                pending.push(arena.child_at(node, index).expect("child"));
            }
        }
        seen
    }

    fn release_reused_pair(mut pair: ReusedPair) {
        assert!(pair
            .arena
            .release_committed_root(pair.base.into_root())
            .is_ok());
        assert!(pair
            .arena
            .release_committed_root(pair.target.into_root())
            .is_ok());
        while !pair.arena.poll_reclaim(127).complete {}
        assert_eq!(pair.arena.metrics().resident_nodes, 0);
    }

    fn release_publication(arena: &mut PageArena, publication: PublishedManifest) {
        assert!(
            arena
                .release_committed_root(publication.into_root())
                .is_ok(),
            "release publication"
        );
        while !arena.poll_reclaim(31).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    fn persistent_source_facts(
        arena: &mut PageArena,
        store: &SourceStore,
        spacing: usize,
    ) -> PersistentSourceFactsRoot {
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
        .expect("SourceFacts builder");
        let certified = loop {
            match scanner.poll(64, 64).expect("SourceFacts scan") {
                SourceFactsPoll::Pending(_) => {}
                SourceFactsPoll::Page { page, .. } => {
                    builder.push_page(page).expect("SourceFacts page");
                }
                SourceFactsPoll::Complete { completion, .. } => {
                    break builder
                        .certify(completion)
                        .expect("SourceFacts certification");
                }
                SourceFactsPoll::Cancelled => panic!("SourceFacts scan cancelled"),
            }
        };
        let mut persistent = PersistentSourceFactsBuild::new(certified);
        loop {
            match persistent
                .poll(arena)
                .expect("persistent SourceFacts build")
            {
                PersistentSourceFactsBuildPoll::Pending => {}
                PersistentSourceFactsBuildPoll::Complete(output) => {
                    let PersistentSourceFactsBuildOutput { certified, root } = *output;
                    drop(certified);
                    return root;
                }
            }
        }
    }

    fn runtime_persistent_source_facts(
        runtime: &mut DocumentRuntime,
        spacing: usize,
    ) -> PersistentSourceFactsRoot {
        let profile = SourceFactsScanProfile::new(spacing).expect("scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let lease = runtime.snapshot_current_source().expect("source lease");
        let mut scanner =
            SourceFactsScanner::with_profile(lease.duplicate(), profile).expect("scanner");
        let mut builder = SourceFactsRootBuilder::new(
            lease,
            profile,
            parser_profile,
            SourceFactsRootLimits::default(),
        )
        .expect("SourceFacts builder");
        let certified = loop {
            match scanner.poll(64, 64).expect("SourceFacts scan") {
                SourceFactsPoll::Pending(_) => {}
                SourceFactsPoll::Page { page, .. } => {
                    builder.push_page(page).expect("SourceFacts page");
                }
                SourceFactsPoll::Complete { completion, .. } => {
                    break builder
                        .certify(completion)
                        .expect("SourceFacts certification");
                }
                SourceFactsPoll::Cancelled => panic!("SourceFacts scan cancelled"),
            }
        };
        let mut persistent = PersistentSourceFactsBuild::new(certified);
        loop {
            match persistent
                .poll(runtime.producer_arena_mut())
                .expect("persistent SourceFacts build")
            {
                PersistentSourceFactsBuildPoll::Pending => {}
                PersistentSourceFactsBuildPoll::Complete(output) => {
                    let PersistentSourceFactsBuildOutput { certified, root } = *output;
                    drop(certified);
                    return root;
                }
            }
        }
    }

    fn block_role(bytes: &[u8]) -> M11BlockRoleRecord {
        M11BlockRoleRecord::new(bytes).expect("block role")
    }

    fn build_runtime_blocks(
        runtime: &mut DocumentRuntime,
        entry_count: usize,
    ) -> M11BlockSequenceRoot {
        let lease = runtime.snapshot_current_source().expect("block source");
        let mut build = M11BlockSequenceBuild::new(runtime, lease).expect("block build");
        for ordinal in 0..entry_count {
            let entry = if ordinal % 2 == 0 {
                M11BlockSequenceEntry::paragraph(1, 1, 0, block_role(b"g"), block_role(b"p"))
                    .expect("paragraph")
            } else {
                M11BlockSequenceEntry::blank(1, 1).expect("blank")
            };
            build.offer_entry(entry).expect("offer block entry");
            loop {
                let poll = build.poll(runtime, 31).expect("block build poll");
                if poll.status() == M11BlockSequenceBuildStatus::NeedsInput {
                    break;
                }
            }
        }
        build.finish_input().expect("finish block input");
        loop {
            if build
                .poll(runtime, 31)
                .expect("finish block build")
                .status()
                == M11BlockSequenceBuildStatus::Complete
            {
                return build.take_root().expect("block root");
            }
        }
    }

    fn publish_runtime_block_candidate(
        runtime: &mut DocumentRuntime,
        persistent: &PersistentSourceFactsRoot,
        blocks: &M11BlockSequenceRoot,
        authority: CandidateAuthority,
        source: SourceVersion,
        spacing: usize,
        base: Option<&PublishedManifest>,
    ) -> PublishedManifest {
        let runtime_identity = runtime.producer_identity();
        let reference_limits = ReferenceRootLimits {
            arena: runtime.producer_arena().limits(),
            max_occurrences: 1,
            ..ReferenceRootLimits::default()
        };
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(spacing).expect("scan profile");
        let arena = runtime.producer_arena_mut();
        let mut assembler = match base {
            Some(base) => {
                CandidateManifestAssembler::new_with_persistent_source_facts_and_blocks_reusing_references(
                    arena,
                    persistent,
                    blocks,
                    runtime_identity,
                    authority,
                    source,
                    parser_profile,
                    scan_profile,
                    reference_limits,
                    base,
                )
                .expect("target persistent block candidate")
            }
            None => CandidateManifestAssembler::new_with_persistent_source_facts_and_blocks(
                arena,
                persistent,
                blocks,
                runtime_identity,
                authority,
                source,
                parser_profile,
                scan_profile,
                reference_limits,
            )
            .expect("base persistent block candidate"),
        };
        if base.is_none() {
            assembler
                .finish_references(arena)
                .expect("finish empty References");
        }
        finish_assembler(arena, assembler)
    }

    fn publish_persistent_candidate(
        arena: &mut PageArena,
        persistent: &PersistentSourceFactsRoot,
        authority: CandidateAuthority,
        source: SourceVersion,
        spacing: usize,
        reference_count: usize,
        base: Option<&PublishedManifest>,
    ) -> PublishedManifest {
        let reference_limits = ReferenceRootLimits {
            arena: arena.limits(),
            max_occurrences: u64::try_from(reference_count.max(1))
                .expect("reference count fits u64"),
            ..ReferenceRootLimits::default()
        };
        let green: &[u8] = if base.is_some() {
            b"green:target"
        } else {
            b"green:base"
        };
        let records = CanonicalRoleInputs::persistent(green, &b"projection"[..]);
        let mut assembler = match base {
            Some(base) => {
                CandidateManifestAssembler::new_with_persistent_source_facts_reusing_references(
                    arena,
                    persistent,
                    authority,
                    source,
                    ParserProfileId::new(1).expect("parser profile"),
                    SourceFactsScanProfile::new(spacing).expect("scan profile"),
                    reference_limits,
                    records,
                    base,
                )
                .expect("reused persistent candidate")
            }
            None => CandidateManifestAssembler::new_with_persistent_source_facts(
                arena,
                persistent,
                authority,
                source,
                ParserProfileId::new(1).expect("parser profile"),
                SourceFactsScanProfile::new(spacing).expect("scan profile"),
                reference_limits,
                records,
            )
            .expect("persistent candidate"),
        };
        if base.is_none() {
            for ordinal in 0..reference_count {
                let start = u64::try_from(ordinal * 16).expect("reference start");
                assembler
                    .offer_reference(
                        arena,
                        AuthoritativeReferenceFact {
                            authority,
                            source: range(start, start + 15),
                            label_source: range(start + 1, start + 4),
                            destination_source: range(start + 6, start + 12),
                            title_source: None,
                            normalized_label: format!("label-{ordinal}")
                                .into_bytes()
                                .into_boxed_slice(),
                            cooked_destination: Box::from(&b"/same"[..]),
                            cooked_title: None,
                            _not_sync: PhantomData,
                        },
                    )
                    .expect("reference");
                while !assembler.references_idle() {
                    assert!(matches!(
                        assembler.poll(arena, 31).expect("reference poll"),
                        ManifestPoll::Pending { .. }
                    ));
                }
            }
            assembler
                .finish_references(arena)
                .expect("finish references");
        }
        finish_assembler(arena, assembler)
    }

    struct PersistentExactPair {
        source: SourceVersion,
        target_source: SourceVersion,
        arena: PageArena,
        base: PublishedManifest,
        target: PublishedManifest,
        document: StrongIdentity,
        base_page_range: std::ops::Range<u64>,
        target_page_range: std::ops::Range<u64>,
    }

    fn build_persistent_exact_pair(
        source_text: &str,
        edit: std::ops::Range<usize>,
        replacement: &str,
        spacing: usize,
        reference_count: usize,
        force_full_replacement: bool,
    ) -> PersistentExactPair {
        let mut store = SourceStore::new(source_text).expect("base source");
        let source = store.version();
        let document = StrongIdentity::new([63; 16]).expect("document");
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([64; 16]).expect("base publication"),
            source,
            CandidateGeneration::FIRST,
            1,
        )
        .expect("base authority");
        let mut arena = PageArena::new(ArenaLimits::default()).expect("producer arena");
        let base_persistent = persistent_source_facts(&mut arena, &store, spacing);
        let base_root = base_persistent.tree_root_id_for_test();
        let base_count = base_persistent.page_count();
        let base = publish_persistent_candidate(
            &mut arena,
            &base_persistent,
            base_authority,
            source,
            spacing,
            reference_count,
            None,
        );

        let prepared = store
            .prepare_edit(source, edit, replacement)
            .expect("target edit");
        drop(
            store
                .commit_prepared_edit(prepared)
                .expect("commit target edit"),
        );
        let target_source = store.version();
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([65; 16]).expect("target publication"),
            target_source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");
        let target_persistent = persistent_source_facts(&mut arena, &store, spacing);
        let target_root = target_persistent.tree_root_id_for_test();
        let target_count = target_persistent.page_count();
        let target = publish_persistent_candidate(
            &mut arena,
            &target_persistent,
            target_authority,
            target_source,
            spacing,
            reference_count,
            Some(&base),
        );

        let mut common_prefix = 0;
        if !force_full_replacement {
            while common_prefix < base_count.min(target_count) {
                let base_page =
                    persistent_source_facts_leaf_record_at(&arena, base_root, common_prefix)
                        .expect("base page")
                        .expect("base page present");
                let target_page =
                    persistent_source_facts_leaf_record_at(&arena, target_root, common_prefix)
                        .expect("target page")
                        .expect("target page present");
                if base_page != target_page {
                    break;
                }
                common_prefix += 1;
            }
        }
        let base_page_range = common_prefix..base_count;
        let target_page_range = common_prefix..target_count;
        base_persistent
            .release(&mut arena)
            .unwrap_or_else(|_| panic!("release base persistent root"));
        target_persistent
            .release(&mut arena)
            .unwrap_or_else(|_| panic!("release target persistent root"));
        while !arena.poll_reclaim(127).complete {}

        PersistentExactPair {
            source,
            target_source,
            arena,
            base,
            target,
            document,
            base_page_range,
            target_page_range,
        }
    }

    struct PersistentBlockExactPair {
        runtime: DocumentRuntime,
        source: SourceVersion,
        target_source: SourceVersion,
        base: PublishedManifest,
        target: PublishedManifest,
        base_blocks: M11BlockSequenceRoot,
        target_blocks: M11BlockSequenceRoot,
        base_source_facts: PersistentSourceFactsRoot,
        target_source_facts: PersistentSourceFactsRoot,
        document: StrongIdentity,
        base_source_facts_page_range: Range<u64>,
        target_source_facts_page_range: Range<u64>,
        base_block_entry_range: Range<u64>,
        target_block_entry_range: Range<u64>,
    }

    fn build_persistent_block_exact_pair(pair_count: usize) -> PersistentBlockExactPair {
        let text = "x\n".repeat(pair_count);
        let entry_count = text.len();
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("base source");
        let document = StrongIdentity::new([91; 16]).expect("document");
        let spacing = 64;
        let base_source_facts = runtime_persistent_source_facts(&mut runtime, spacing);
        let base_blocks = build_runtime_blocks(&mut runtime, entry_count);
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([92; 16]).expect("base publication"),
            source,
            CandidateGeneration::FIRST,
            1,
        )
        .expect("base authority");
        let base = publish_runtime_block_candidate(
            &mut runtime,
            &base_source_facts,
            &base_blocks,
            base_authority,
            source,
            spacing,
            None,
        );

        let edited_entry = u64::try_from(entry_count / 2).expect("entry count") & !1_u64;
        runtime
            .apply_edit(
                source,
                usize::try_from(edited_entry).expect("edit start")
                    ..usize::try_from(edited_entry + 1).expect("edit end"),
                "y",
            )
            .expect("same-width local edit");
        let target_source = runtime.current_source_version().expect("target source");
        let target_lease = runtime.snapshot_current_source().expect("target lease");
        let replacement =
            [
                M11BlockSequenceEntry::paragraph(
                    1,
                    1,
                    0,
                    block_role(b"g-new"),
                    block_role(b"p-new"),
                )
                .expect("replacement paragraph"),
            ];
        let (target_blocks, _) = splice_m11_block_sequence_atomic(
            &mut runtime,
            &base_blocks,
            target_lease,
            edited_entry..edited_entry + 1,
            &replacement,
        )
        .expect("target block splice");
        let target_source_facts = runtime_persistent_source_facts(&mut runtime, spacing);
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([93; 16]).expect("target publication"),
            target_source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");
        let target = publish_runtime_block_candidate(
            &mut runtime,
            &target_source_facts,
            &target_blocks,
            target_authority,
            target_source,
            spacing,
            Some(&base),
        );

        let base_root = base_source_facts.tree_root_id_for_test();
        let target_root = target_source_facts.tree_root_id_for_test();
        let base_count = base_source_facts.page_count();
        let target_count = target_source_facts.page_count();
        let mut common_prefix = 0;
        while common_prefix < base_count.min(target_count) {
            let base_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                base_root,
                common_prefix,
            )
            .expect("base SourceFacts page")
            .expect("base SourceFacts page present");
            let target_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                target_root,
                common_prefix,
            )
            .expect("target SourceFacts page")
            .expect("target SourceFacts page present");
            if base_page != target_page {
                break;
            }
            common_prefix += 1;
        }
        let mut common_suffix = 0;
        while common_suffix < base_count.min(target_count) - common_prefix {
            let base_ordinal = base_count - common_suffix - 1;
            let target_ordinal = target_count - common_suffix - 1;
            let base_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                base_root,
                base_ordinal,
            )
            .expect("base suffix page")
            .expect("base suffix page present");
            let target_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                target_root,
                target_ordinal,
            )
            .expect("target suffix page")
            .expect("target suffix page present");
            if base_page != target_page {
                break;
            }
            common_suffix += 1;
        }

        PersistentBlockExactPair {
            runtime,
            source,
            target_source,
            base,
            target,
            base_blocks,
            target_blocks,
            base_source_facts,
            target_source_facts,
            document,
            base_source_facts_page_range: common_prefix..base_count - common_suffix,
            target_source_facts_page_range: common_prefix..target_count - common_suffix,
            base_block_entry_range: edited_entry..edited_entry + 1,
            target_block_entry_range: edited_entry..edited_entry + 1,
        }
    }

    fn green_frame(value: u64) -> M11RecursiveGreenFrameId {
        M11RecursiveGreenFrameId::new(value).expect("nonzero Green frame")
    }

    fn green_kind(value: u16) -> M11RecursiveGreenKind {
        M11RecursiveGreenKind::new(value).expect("nonzero Green kind")
    }

    fn offer_green_event(
        build: &mut M11RecursiveGreenBuild,
        runtime: &mut DocumentRuntime,
        event: M11RecursiveGreenEvent,
    ) {
        build.offer_event(event).expect("offer Green event");
        loop {
            let poll = build
                .poll(runtime, M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS)
                .expect("poll Green event");
            if poll.status() == M11RecursiveGreenBuildStatus::NeedsInput {
                break;
            }
            assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
        }
    }

    fn build_runtime_recursive_green(
        runtime: &mut DocumentRuntime,
        items: usize,
        changed_item: Option<usize>,
    ) -> M11RecursiveGreenRoot {
        let lease = runtime.snapshot_current_source().expect("Green source");
        let mut build = M11RecursiveGreenBuild::new(runtime, lease).expect("Green build");
        offer_green_event(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Enter {
                frame: green_frame(1),
                kind: green_kind(1),
            },
        );
        for index in 0..items {
            let frame = green_frame(u64::try_from(index + 2).expect("Green frame fits u64"));
            let kind = if changed_item == Some(index) {
                green_kind(9)
            } else {
                green_kind(2)
            };
            offer_green_event(
                &mut build,
                runtime,
                M11RecursiveGreenEvent::Enter { frame, kind },
            );
            offer_green_event(
                &mut build,
                runtime,
                M11RecursiveGreenEvent::Coverage {
                    physical: M11RecursiveGreenSourceMetric::new(1, 1).expect("one-byte coverage"),
                    owner_depth: 0,
                    part: M11RecursiveGreenCoveragePart::Content,
                    logical: M11RecursiveGreenLogicalAction::Identity,
                },
            );
            offer_green_event(
                &mut build,
                runtime,
                M11RecursiveGreenEvent::Exit {
                    frame,
                    final_kind: kind,
                    close: None,
                    last_line_blank: false,
                    child: M11RecursiveGreenClosedChild::default(),
                },
            );
        }
        offer_green_event(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Exit {
                frame: green_frame(1),
                final_kind: green_kind(1),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
        build.finish_input().expect("finish Green input");
        loop {
            let poll = build
                .poll(runtime, M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS)
                .expect("finish Green build");
            if poll.status() == M11RecursiveGreenBuildStatus::Complete {
                return build.take_root().expect("Green root");
            }
            assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
        }
    }

    fn publish_runtime_recursive_green_candidate(
        runtime: &mut DocumentRuntime,
        persistent: &PersistentSourceFactsRoot,
        green: &M11RecursiveGreenRoot,
        authority: CandidateAuthority,
        source: SourceVersion,
        spacing: usize,
        base: Option<&PublishedManifest>,
    ) -> PublishedManifest {
        let runtime_identity = runtime.producer_identity();
        let reference_limits = ReferenceRootLimits {
            arena: runtime.producer_arena().limits(),
            max_occurrences: 1,
            ..ReferenceRootLimits::default()
        };
        let records =
            CanonicalRoleInputs::persistent_recursive_green_projection_records([
                Box::<[u8]>::from(&b"projection"[..]),
            ]);
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(spacing).expect("scan profile");
        let arena = runtime.producer_arena_mut();
        let mut assembler = match base {
            Some(base) => CandidateManifestAssembler::new_with_persistent_source_facts_and_recursive_green_reusing_references(
                arena,
                persistent,
                green,
                runtime_identity,
                authority,
                source,
                parser_profile,
                scan_profile,
                reference_limits,
                records,
                base,
            )
            .expect("target persistent recursive Green candidate"),
            None => CandidateManifestAssembler::new_with_persistent_source_facts_and_recursive_green(
                arena,
                persistent,
                green,
                runtime_identity,
                authority,
                source,
                parser_profile,
                scan_profile,
                reference_limits,
                records,
            )
            .expect("base persistent recursive Green candidate"),
        };
        if base.is_none() {
            assembler
                .finish_references(arena)
                .expect("finish empty References");
        }
        finish_assembler(arena, assembler)
    }

    struct PersistentGreenExactPair {
        runtime: DocumentRuntime,
        source: SourceVersion,
        target_source: SourceVersion,
        base: PublishedManifest,
        target: PublishedManifest,
        base_green: M11RecursiveGreenRoot,
        target_green: M11RecursiveGreenRoot,
        base_source_facts: PersistentSourceFactsRoot,
        target_source_facts: PersistentSourceFactsRoot,
        document: StrongIdentity,
        base_source_facts_page_range: Range<u64>,
        target_source_facts_page_range: Range<u64>,
        base_green_event_range: Range<u64>,
        target_green_event_range: Range<u64>,
    }

    fn build_persistent_green_exact_pair(items: usize) -> PersistentGreenExactPair {
        let mut runtime =
            DocumentRuntime::new(&"x".repeat(items), DocumentRuntimeConfig::default())
                .expect("runtime");
        let source = runtime.current_source_version().expect("base source");
        let document = StrongIdentity::new([101; 16]).expect("document");
        let spacing = 64;
        let base_source_facts = runtime_persistent_source_facts(&mut runtime, spacing);
        let base_green = build_runtime_recursive_green(&mut runtime, items, None);
        let base_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([102; 16]).expect("base publication"),
            source,
            CandidateGeneration::FIRST,
            1,
        )
        .expect("base authority");
        let base = publish_runtime_recursive_green_candidate(
            &mut runtime,
            &base_source_facts,
            &base_green,
            base_authority,
            source,
            spacing,
            None,
        );

        let changed_item = items / 2;
        runtime
            .apply_edit(source, changed_item..changed_item + 1, "y")
            .expect("same-width local edit");
        let target_source = runtime.current_source_version().expect("target source");
        let target_source_facts = runtime_persistent_source_facts(&mut runtime, spacing);
        let target_green = build_runtime_recursive_green(&mut runtime, items, Some(changed_item));
        let target_authority = CandidateAuthority::new(
            document,
            StrongIdentity::new([103; 16]).expect("target publication"),
            target_source,
            CandidateGeneration::from_wire(2).expect("target generation"),
            1,
        )
        .expect("target authority");
        let target = publish_runtime_recursive_green_candidate(
            &mut runtime,
            &target_source_facts,
            &target_green,
            target_authority,
            target_source,
            spacing,
            Some(&base),
        );

        let base_root = base_source_facts.tree_root_id_for_test();
        let target_root = target_source_facts.tree_root_id_for_test();
        let base_count = base_source_facts.page_count();
        let target_count = target_source_facts.page_count();
        let mut common_prefix = 0;
        while common_prefix < base_count.min(target_count) {
            let base_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                base_root,
                common_prefix,
            )
            .expect("base SourceFacts page")
            .expect("base SourceFacts page present");
            let target_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                target_root,
                common_prefix,
            )
            .expect("target SourceFacts page")
            .expect("target SourceFacts page present");
            if base_page != target_page {
                break;
            }
            common_prefix += 1;
        }
        let mut common_suffix = 0;
        while common_suffix < base_count.min(target_count) - common_prefix {
            let base_ordinal = base_count - common_suffix - 1;
            let target_ordinal = target_count - common_suffix - 1;
            let base_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                base_root,
                base_ordinal,
            )
            .expect("base SourceFacts suffix")
            .expect("base SourceFacts suffix present");
            let target_page = persistent_source_facts_leaf_record_at(
                runtime.producer_arena(),
                target_root,
                target_ordinal,
            )
            .expect("target SourceFacts suffix")
            .expect("target SourceFacts suffix present");
            if base_page != target_page {
                break;
            }
            common_suffix += 1;
        }
        let event_start = 1 + u64::try_from(changed_item).expect("changed item fits u64") * 3;
        PersistentGreenExactPair {
            runtime,
            source,
            target_source,
            base,
            target,
            base_green,
            target_green,
            base_source_facts,
            target_source_facts,
            document,
            base_source_facts_page_range: common_prefix..base_count - common_suffix,
            target_source_facts_page_range: common_prefix..target_count - common_suffix,
            base_green_event_range: event_start..event_start + 3,
            target_green_event_range: event_start..event_start + 3,
        }
    }

    struct ExactDeltaFrames {
        begin: Box<[u8]>,
        replacement_pages: Vec<Box<[u8]>>,
        nodes: Vec<Box<[u8]>>,
        end: Box<[u8]>,
    }

    fn collect_exact_delta_stream(pair: &PersistentExactPair) -> ExactDeltaFrames {
        let mut encoder = CandidateSnapshotEncoder::new_exact_base_delta(
            &pair.arena,
            &pair.base,
            &pair.target,
            pair.base_page_range.clone(),
            pair.target_page_range.clone(),
        )
        .expect("exact-base encoder");
        let begin = encoder.begin_frame().expect("exact-base Begin");
        assert_eq!(begin.len(), SNAPSHOT_EXACT_BASE_BEGIN_BYTES);
        let mut replacement_pages = Vec::new();
        let mut nodes = Vec::new();
        let mut replay_barrier_seen = false;
        let end = loop {
            match encoder.poll(31).expect("exact-base encode poll") {
                CandidateSnapshotEncodePoll::Pending { .. } => {}
                CandidateSnapshotEncodePoll::Frame { bytes, .. }
                    if bytes[0] == SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG =>
                {
                    assert!(!replay_barrier_seen);
                    replacement_pages.push(bytes);
                }
                CandidateSnapshotEncodePoll::Frame { bytes, .. } => {
                    assert!(replay_barrier_seen);
                    nodes.push(bytes);
                }
                CandidateSnapshotEncodePoll::ReplayRequired { transitions } => {
                    assert_eq!(transitions, 0);
                    assert!(!replay_barrier_seen);
                    replay_barrier_seen = true;
                    encoder
                        .resume_exact_base_delta()
                        .expect("resume exact-base encoder");
                }
                CandidateSnapshotEncodePoll::Complete { bytes, .. } => break bytes,
            }
        };
        assert!(replay_barrier_seen);
        ExactDeltaFrames {
            begin,
            replacement_pages,
            nodes,
            end,
        }
    }

    fn install_exact_delta_stream(
        host: &mut CandidateHostStore,
        base: InstalledCandidateSnapshot,
        frames: &ExactDeltaFrames,
    ) -> InstalledCandidateSnapshot {
        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin exact-base delta");
        drive_exact_replay_to_nodes(host, frames);
        for node in &frames.nodes {
            host.offer_node(node).expect("offer exact-base node");
        }
        host.finish_snapshot(&frames.end)
            .expect("finish exact-base snapshot");
        loop {
            let poll = host.poll_install(31).expect("exact-base install poll");
            if let Some(snapshot) = poll.installed {
                return snapshot;
            }
        }
    }

    fn drive_exact_replay_to_nodes(host: &mut CandidateHostStore, frames: &ExactDeltaFrames) {
        for page in &frames.replacement_pages {
            host.offer_source_facts_replacement_page(page)
                .expect("offer SourceFacts replacement page");
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("replacement replay poll");
                assert!(poll.transitions <= 1);
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    break;
                }
            }
        }
        loop {
            let poll = host
                .poll_exact_base_delta_replay(1)
                .expect("complete replay poll");
            assert!(poll.transitions <= 1);
            if poll.ready_for_nodes {
                break;
            }
            assert!(
                !poll.ready_for_replacement_page,
                "all encoded replacement pages were already offered"
            );
        }
    }

    struct BlockExactDeltaFrames {
        begin: Box<[u8]>,
        source_facts_pages: Vec<Box<[u8]>>,
        block_pages: Vec<Box<[u8]>>,
        nodes: Vec<Box<[u8]>>,
        end: Box<[u8]>,
    }

    fn collect_block_exact_delta_stream(pair: &PersistentBlockExactPair) -> BlockExactDeltaFrames {
        let mut encoder = CandidateSnapshotEncoder::new_exact_base_delta_with_block_splice(
            pair.runtime.producer_arena(),
            &pair.base,
            &pair.target,
            pair.base_source_facts_page_range.clone(),
            pair.target_source_facts_page_range.clone(),
            pair.base_block_entry_range.clone(),
            pair.target_block_entry_range.clone(),
        )
        .expect("block exact-base encoder");
        let begin = encoder.begin_frame().expect("block exact-base Begin");
        assert_eq!(begin.len(), SNAPSHOT_EXACT_BASE_BLOCK_BEGIN_BYTES);
        let mut source_facts_pages = Vec::new();
        let mut block_pages = Vec::new();
        let mut nodes = Vec::new();
        let mut replay_barrier_seen = false;
        let end = loop {
            match encoder.poll(31).expect("block exact-base encode poll") {
                CandidateSnapshotEncodePoll::Pending { .. } => {}
                CandidateSnapshotEncodePoll::Frame { bytes, .. }
                    if bytes[0] == SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG =>
                {
                    assert!(!replay_barrier_seen);
                    source_facts_pages.push(bytes);
                }
                CandidateSnapshotEncodePoll::Frame { bytes, .. }
                    if bytes[0] == SNAPSHOT_BLOCK_REPLACEMENT_TAG =>
                {
                    assert!(!replay_barrier_seen);
                    let metadata =
                        classify_snapshot_frame(&bytes).expect("classify block replacement page");
                    assert_eq!(
                        metadata.kind,
                        SnapshotFrameKind::BlockSequenceReplacementPage
                    );
                    assert_eq!(metadata.node_ordinal, None);
                    assert_eq!(metadata.canonical_record_count, 1);
                    let public_metadata = crate::m11_host::M11CandidateHost::classify_frame(&bytes)
                        .expect("classify public block replacement page");
                    assert_eq!(
                        public_metadata.kind,
                        crate::m11_host::M11HostFrameKind::BlockSequenceReplacementPage
                    );
                    assert_eq!(public_metadata.node_ordinal, None);
                    assert_eq!(public_metadata.canonical_record_count, 1);
                    block_pages.push(bytes);
                }
                CandidateSnapshotEncodePoll::Frame { bytes, .. } => {
                    assert!(replay_barrier_seen);
                    let ordinal = read_u64(&bytes, 4).expect("target node ordinal");
                    let node = decode_node_frame(&bytes, ordinal).expect("ordinary target node");
                    assert!(
                        !is_m11_block_sequence_node_payload(node.payload),
                        "the target BlockSequence closure must be virtual"
                    );
                    nodes.push(bytes);
                }
                CandidateSnapshotEncodePoll::ReplayRequired { transitions } => {
                    assert_eq!(transitions, 0);
                    assert!(!replay_barrier_seen);
                    replay_barrier_seen = true;
                    encoder
                        .resume_exact_base_delta()
                        .expect("resume block exact-base encoder");
                }
                CandidateSnapshotEncodePoll::Complete { bytes, .. } => break bytes,
            }
        };
        assert!(replay_barrier_seen);
        BlockExactDeltaFrames {
            begin,
            source_facts_pages,
            block_pages,
            nodes,
            end,
        }
    }

    fn drive_source_facts_to_block_pages(
        host: &mut CandidateHostStore,
        frames: &BlockExactDeltaFrames,
    ) {
        for page in &frames.source_facts_pages {
            host.offer_source_facts_replacement_page(page)
                .expect("offer SourceFacts replacement page");
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("SourceFacts replay poll");
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    assert!(
                        !poll.ready_for_nodes,
                        "block replay must follow SourceFacts"
                    );
                    break;
                }
            }
        }
        if frames.source_facts_pages.is_empty() {
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("empty SourceFacts replay poll");
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    assert!(
                        !poll.ready_for_nodes,
                        "block replay must follow SourceFacts"
                    );
                    break;
                }
            }
        }
    }

    fn drive_block_exact_replay_to_nodes(
        host: &mut CandidateHostStore,
        frames: &BlockExactDeltaFrames,
    ) -> M11BlockSequenceHostSpliceWork {
        drive_source_facts_to_block_pages(host, frames);
        for page in &frames.block_pages {
            host.offer_block_sequence_replacement_page(page)
                .expect("offer block replacement page");
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("block replay poll");
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    break;
                }
            }
        }
        loop {
            let poll = host
                .poll_exact_base_delta_replay(1)
                .expect("complete block replay poll");
            if poll.ready_for_nodes {
                return host
                    .active_block_splice_work()
                    .expect("authenticated block replay receipt");
            }
            assert!(
                !poll.ready_for_replacement_page,
                "all encoded replacement pages were already offered"
            );
        }
    }

    struct GreenExactDeltaFrames {
        begin: Box<[u8]>,
        source_facts_pages: Vec<Box<[u8]>>,
        green_pages: Vec<Box<[u8]>>,
        nodes: Vec<Box<[u8]>>,
        end: Box<[u8]>,
    }

    fn collect_green_exact_delta_stream(pair: &PersistentGreenExactPair) -> GreenExactDeltaFrames {
        let mut encoder =
            CandidateSnapshotEncoder::new_exact_base_delta_with_recursive_green_splice(
                pair.runtime.producer_arena(),
                &pair.base,
                &pair.target,
                pair.base_source_facts_page_range.clone(),
                pair.target_source_facts_page_range.clone(),
                pair.base_green_event_range.clone(),
                pair.target_green_event_range.clone(),
            )
            .expect("recursive Green exact-base encoder");
        let begin = encoder.begin_frame().expect("recursive Green Begin");
        assert_eq!(begin.len(), SNAPSHOT_EXACT_BASE_RECURSIVE_GREEN_BEGIN_BYTES);
        let mut source_facts_pages = Vec::new();
        let mut green_pages = Vec::new();
        let mut nodes = Vec::new();
        let mut replay_barrier_seen = false;
        let end = loop {
            match encoder.poll(31).expect("recursive Green encode poll") {
                CandidateSnapshotEncodePoll::Pending { .. } => {}
                CandidateSnapshotEncodePoll::Frame { bytes, .. }
                    if bytes[0] == SNAPSHOT_SOURCE_FACTS_REPLACEMENT_TAG =>
                {
                    assert!(!replay_barrier_seen);
                    source_facts_pages.push(bytes);
                }
                CandidateSnapshotEncodePoll::Frame { bytes, .. }
                    if bytes[0] == SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_TAG =>
                {
                    assert!(!replay_barrier_seen);
                    assert_eq!(
                        classify_snapshot_frame(&bytes)
                            .expect("classify recursive Green replacement")
                            .kind,
                        SnapshotFrameKind::RecursiveGreenReplacementPage
                    );
                    assert_eq!(
                        bytes.get(
                            SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES
                                ..SNAPSHOT_RECURSIVE_GREEN_REPLACEMENT_HEADER_BYTES + 4
                        ),
                        Some(b"RGL1".as_slice()),
                        "exact transport must carry leaves, never RGB1 branches"
                    );
                    green_pages.push(bytes);
                }
                CandidateSnapshotEncodePoll::Frame { bytes, .. } => {
                    assert!(replay_barrier_seen);
                    let ordinal = read_u64(&bytes, 4).expect("target node ordinal");
                    let node = decode_node_frame(&bytes, ordinal).expect("ordinary target node");
                    assert!(
                        !is_m11_recursive_green_node_payload(node.payload),
                        "the target recursive Green closure must be virtual"
                    );
                    nodes.push(bytes);
                }
                CandidateSnapshotEncodePoll::ReplayRequired { transitions } => {
                    assert_eq!(transitions, 0);
                    assert!(!replay_barrier_seen);
                    replay_barrier_seen = true;
                    encoder
                        .resume_exact_base_delta()
                        .expect("resume recursive Green exact-base encoder");
                }
                CandidateSnapshotEncodePoll::Complete { bytes, .. } => break bytes,
            }
        };
        assert!(replay_barrier_seen);
        GreenExactDeltaFrames {
            begin,
            source_facts_pages,
            green_pages,
            nodes,
            end,
        }
    }

    fn drive_green_exact_replay_to_nodes(
        host: &mut CandidateHostStore,
        frames: &GreenExactDeltaFrames,
    ) -> M11RecursiveGreenHostSpliceWork {
        for page in &frames.source_facts_pages {
            host.offer_source_facts_replacement_page(page)
                .expect("offer SourceFacts replacement page");
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("SourceFacts replay poll");
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    assert!(
                        !poll.ready_for_nodes,
                        "Green replay must follow SourceFacts"
                    );
                    break;
                }
            }
        }
        if frames.source_facts_pages.is_empty() {
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("empty SourceFacts replay poll");
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    assert!(
                        !poll.ready_for_nodes,
                        "Green replay must follow SourceFacts"
                    );
                    break;
                }
            }
        }
        for page in &frames.green_pages {
            host.offer_recursive_green_replacement_page(page)
                .expect("offer recursive Green replacement page");
            loop {
                let poll = host
                    .poll_exact_base_delta_replay(1)
                    .expect("recursive Green replay poll");
                if poll.ready_for_replacement_page || poll.ready_for_nodes {
                    break;
                }
            }
        }
        loop {
            let poll = host
                .poll_exact_base_delta_replay(1)
                .expect("complete recursive Green replay poll");
            if poll.ready_for_nodes {
                return host
                    .active_recursive_green_splice_work()
                    .expect("authenticated recursive Green replay receipt");
            }
            assert!(
                !poll.ready_for_replacement_page,
                "all encoded recursive Green pages were already offered"
            );
        }
    }

    fn release_persistent_green_exact_pair(mut pair: PersistentGreenExactPair) {
        assert!(pair
            .runtime
            .producer_arena_mut()
            .release_committed_root(pair.base.into_root())
            .is_ok());
        assert!(pair
            .runtime
            .producer_arena_mut()
            .release_committed_root(pair.target.into_root())
            .is_ok());
        pair.base_source_facts
            .release(pair.runtime.producer_arena_mut())
            .unwrap_or_else(|_| panic!("release base SourceFacts"));
        pair.target_source_facts
            .release(pair.runtime.producer_arena_mut())
            .unwrap_or_else(|_| panic!("release target SourceFacts"));
        pair.target_green
            .begin_release(&mut pair.runtime)
            .expect("release target Green");
        pair.base_green
            .begin_release(&mut pair.runtime)
            .expect("release base Green");
        loop {
            let target = pair
                .target_green
                .poll_release(&mut pair.runtime, 127)
                .expect("target Green reclaim");
            let base = pair
                .base_green
                .poll_release(&mut pair.runtime, 127)
                .expect("base Green reclaim");
            if target.complete() && base.complete() {
                break;
            }
        }
        while !pair.runtime.producer_arena_mut().poll_reclaim(127).complete {}
        pair.runtime.begin_close().expect("begin runtime close");
        while !pair
            .runtime
            .poll_close(127)
            .expect("runtime close")
            .complete
        {}
    }

    fn release_persistent_block_exact_pair(mut pair: PersistentBlockExactPair) {
        assert!(pair
            .runtime
            .producer_arena_mut()
            .release_committed_root(pair.base.into_root())
            .is_ok());
        assert!(pair
            .runtime
            .producer_arena_mut()
            .release_committed_root(pair.target.into_root())
            .is_ok());
        pair.base_source_facts
            .release(pair.runtime.producer_arena_mut())
            .unwrap_or_else(|_| panic!("release base SourceFacts"));
        pair.target_source_facts
            .release(pair.runtime.producer_arena_mut())
            .unwrap_or_else(|_| panic!("release target SourceFacts"));
        pair.target_blocks
            .begin_release(&mut pair.runtime)
            .expect("release target blocks");
        pair.base_blocks
            .begin_release(&mut pair.runtime)
            .expect("release base blocks");
        loop {
            let target = pair
                .target_blocks
                .poll_release(&mut pair.runtime, 127)
                .expect("target block reclaim");
            let base = pair
                .base_blocks
                .poll_release(&mut pair.runtime, 127)
                .expect("base block reclaim");
            if target.complete() && base.complete() {
                break;
            }
        }
        while !pair.runtime.producer_arena_mut().poll_reclaim(127).complete {}
        pair.runtime.begin_close().expect("begin runtime close");
        while !pair
            .runtime
            .poll_close(127)
            .expect("runtime close")
            .complete
        {}
    }

    fn release_persistent_exact_pair(mut pair: PersistentExactPair) {
        assert!(pair
            .arena
            .release_committed_root(pair.base.into_root())
            .is_ok());
        assert!(pair
            .arena
            .release_committed_root(pair.target.into_root())
            .is_ok());
        while !pair.arena.poll_reclaim(127).complete {}
        assert_eq!(pair.arena.metrics().resident_nodes, 0);
    }

    fn close_host(host: &mut CandidateHostStore) {
        host.begin_close().expect("begin close");
        let mut polls = 0;
        while !host.poll_close(1).expect("close poll") {
            polls += 1;
        }
        assert!(polls > 1, "nontrivial closure must retire with fuel");
        assert_eq!(host.arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn closure_stream_uses_local_backreferences_and_emits_shared_dag_nodes_once() {
        let source_store = SourceStore::new("shared").expect("source");
        let authority = CandidateAuthority::new(
            StrongIdentity::new([4; 16]).expect("document"),
            StrongIdentity::new([44; 16]).expect("publication"),
            source_store.version(),
            CandidateGeneration::FIRST,
            1,
        )
        .expect("authority");
        let mut arena = PageArena::new(ArenaLimits::default()).expect("arena");
        let (build, root_owner) = {
            let mut session = arena.begin_build().expect("build");
            let shared = session
                .allocate(&encode_candidate_header(0xf0, authority), &[])
                .expect("shared");
            let left = session
                .allocate(&encode_candidate_header(0xf1, authority), &[shared.id()])
                .expect("left");
            let right = session
                .allocate(&encode_candidate_header(0xf2, authority), &[shared.id()])
                .expect("right");
            let root = session
                .allocate(
                    &encode_candidate_header(0xf3, authority),
                    &[left.id(), right.id()],
                )
                .expect("root");
            let build = session.suspend().expect("suspend");
            (build, root)
        };
        let mut seal = arena.begin_seal(build, root_owner).expect("seal");
        let committed = loop {
            if let Some(root) = arena.poll_seal(&mut seal, 1).expect("seal poll").root {
                break root;
            }
        };

        let mut encoder = CandidateSnapshotEncoder::from_root(&arena, authority, committed.id())
            .expect("DAG encoder");
        let _begin = encoder.begin_frame().expect("begin");
        let mut frames = Vec::new();
        let end = loop {
            match encoder.poll(1).expect("encode") {
                CandidateSnapshotEncodePoll::Pending { .. } => {}
                CandidateSnapshotEncodePoll::Frame { bytes, .. } => frames.push(bytes),
                CandidateSnapshotEncodePoll::ReplayRequired { .. } => {
                    panic!("full DAG snapshot unexpectedly required replay")
                }
                CandidateSnapshotEncodePoll::Complete { bytes, .. } => break bytes,
            }
        };
        assert_eq!(decode_end_frame(&end).expect("end").nodes, 4);
        assert_eq!(frames.len(), 4, "the shared leaf must not be duplicated");
        let shared = decode_node_frame(&frames[0], 0).expect("shared frame");
        let left = decode_node_frame(&frames[1], 1).expect("left frame");
        let right = decode_node_frame(&frames[2], 2).expect("right frame");
        let root = decode_node_frame(&frames[3], 3).expect("root frame");
        assert_eq!(shared.child_count, 0);
        assert_eq!(left.child_ordinal(0).expect("left child"), 0);
        assert_eq!(right.child_ordinal(0).expect("right child"), 0);
        assert_eq!(root.child_ordinal(0).expect("root left"), 1);
        assert_eq!(root.child_ordinal(1).expect("root right"), 2);

        assert!(arena.release_committed_root(committed).is_ok());
        while !arena.poll_reclaim(1).complete {}
        assert_eq!(arena.metrics().resident_nodes, 0);
    }

    #[test]
    fn full_closure_installs_in_independent_arena_and_answers_bounded_query() {
        let source_store = SourceStore::new(&"a".repeat(128)).expect("source");
        let source = source_store.version();
        let document = StrongIdentity::new([7; 16]).expect("document");
        let (mut producer_arena, publication, authority) =
            build_publication(source, document, 71, 1, b"green:block:paragraph");
        let (begin, nodes, end) = collect_stream(&producer_arena, &publication);
        assert!(
            nodes.len() > 5,
            "stream must contain the complete role closure"
        );

        let mut host = CandidateHostStore::new(document, source, 1, CandidateHostLimits::default())
            .expect("host");
        let snapshot = install_stream(&mut host, &begin, &nodes, &end);
        assert_eq!(snapshot.source_revision(), source.revision());
        assert_eq!(snapshot.parse_generation(), CandidateGeneration::FIRST);
        assert_eq!(snapshot.publication_identity(), authority.publication);

        let mut output = [0_u8; 8];
        let read = host
            .read_role_record(snapshot, CandidateRole::Green, 6, &mut output)
            .expect("bounded Green query");
        assert_eq!(&output[..read], b"block:pa");
        let mut oversized = vec![0_u8; host.limits.maximum_query_bytes + 1];
        assert!(matches!(
            host.read_role_record(snapshot, CandidateRole::Green, 0, &mut oversized),
            Err(CandidateHostError::InvalidFrame(
                "query output exceeds its bounded envelope"
            ))
        ));

        close_host(&mut host);
        release_publication(&mut producer_arena, publication);
    }

    #[test]
    fn truncation_digest_corruption_and_cross_authority_preserve_prior_root() {
        let source_store = SourceStore::new(&"b".repeat(128)).expect("source");
        let source = source_store.version();
        let document = StrongIdentity::new([8; 16]).expect("document");
        let (mut first_arena, first_publication, _) =
            build_publication(source, document, 81, 1, b"green:first");
        let (first_begin, first_nodes, first_end) =
            collect_stream(&first_arena, &first_publication);
        let mut host = CandidateHostStore::new(document, source, 1, CandidateHostLimits::default())
            .expect("host");
        let prior = install_stream(&mut host, &first_begin, &first_nodes, &first_end);

        assert!(matches!(
            host.begin_snapshot(&first_begin),
            Err(CandidateHostError::StaleCandidate)
        ));
        assert_eq!(host.installed_snapshot(), Some(prior));

        let mut cross = first_begin.to_vec();
        cross[4] ^= 0x40;
        assert!(matches!(
            host.begin_snapshot(&cross),
            Err(CandidateHostError::CrossAuthority)
        ));
        assert_eq!(host.installed_snapshot(), Some(prior));

        let (mut second_arena, second_publication, _) =
            build_publication(source, document, 82, 2, b"green:second");
        let (second_begin, second_nodes, second_end) =
            collect_stream(&second_arena, &second_publication);
        host.begin_snapshot(&second_begin).expect("begin truncated");
        host.offer_node(&second_nodes[0]).expect("first node");
        assert!(matches!(
            host.finish_snapshot(&second_end),
            Err(CandidateHostError::InvalidFrame(
                "truncated closure or snapshot digest mismatch"
            ))
        ));
        assert_eq!(host.installed_snapshot(), Some(prior));
        while !host.poll_reclaim(1).expect("abort reclaim") {}

        let (mut third_arena, third_publication, _) =
            build_publication(source, document, 83, 3, b"green:third");
        let (third_begin, mut third_nodes, third_end) =
            collect_stream(&third_arena, &third_publication);
        let value = third_nodes
            .iter_mut()
            .find(|frame| {
                let child_count = usize::from(u16::from_le_bytes(
                    frame[12..14].try_into().expect("child count"),
                ));
                let payload =
                    SNAPSHOT_NODE_HEADER_BYTES + child_count * SNAPSHOT_CHILD_ORDINAL_BYTES;
                matches!(frame.get(payload), Some(&0xd1) | Some(&INLINE_FACT_TAG))
                    && frame.len() > payload + CANDIDATE_HEADER_BYTES
            })
            .expect("reference value frame");
        *value.last_mut().expect("reference value byte") ^= 0x01;
        let claim = decode_end_frame(&third_end).expect("end claim");
        let mut digest = snapshot_hasher();
        digest.update(&third_begin);
        for node in &third_nodes {
            digest.update(node);
        }
        let rewritten_end = encode_end_frame(
            claim.nodes,
            claim.payload_bytes,
            claim.wire_bytes,
            *digest.finalize().as_bytes(),
        );
        host.begin_snapshot(&third_begin).expect("begin corrupt");
        for node in &third_nodes {
            host.offer_node(node).expect("offer corrupt closure");
        }
        host.finish_snapshot(&rewritten_end)
            .expect("transport digest is internally consistent");
        loop {
            match host.poll_install(1) {
                Ok(poll) => assert!(poll.installed.is_none()),
                Err(CandidateHostError::Reference(ReferenceRootError::Corrupt(
                    "reference role digest changed",
                ))) => break,
                Err(error) => panic!("unexpected validation failure: {error}"),
            }
        }
        assert_eq!(host.installed_snapshot(), Some(prior));
        while !host.poll_reclaim(1).expect("corrupt reclaim") {}

        let replacement = install_stream(&mut host, &second_begin, &second_nodes, &second_end);
        assert_ne!(replacement, prior);
        assert!(matches!(
            host.read_role_record(prior, CandidateRole::Green, 0, &mut [0_u8; 4]),
            Err(CandidateHostError::CrossAuthority)
        ));
        let mut replacement_bytes = [0_u8; 12];
        let read = host
            .read_role_record(replacement, CandidateRole::Green, 0, &mut replacement_bytes)
            .expect("replacement query");
        assert_eq!(&replacement_bytes[..read], b"green:second");
        let mut retirement_polls = 0;
        while !host.poll_reclaim(1).expect("prior-root retirement") {
            retirement_polls += 1;
        }
        assert!(retirement_polls > 1);

        close_host(&mut host);
        release_publication(&mut first_arena, first_publication);
        release_publication(&mut second_arena, second_publication);
        release_publication(&mut third_arena, third_publication);
    }

    struct DeltaScaleReceipt {
        frame_count: usize,
        encoded_bytes: usize,
        encode_transitions: usize,
        install_transitions: usize,
        retirement_polls: usize,
    }

    fn run_delta_scale(reference_count: usize) -> DeltaScaleReceipt {
        let pair = build_reused_pair(reference_count);
        let (base_begin, base_nodes, base_end) = collect_stream(&pair.arena, &pair.base);
        let delta = collect_delta_stream(&pair.arena, &pair.target);
        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_begin, &base_nodes, &base_end);
        let (base_manifest, base_wrapper, base_canonical, base_metadata) =
            installed_manifest_shape(&host);

        host.observe_source_version(pair.target_source)
            .expect("observe target");
        let (target, install_transitions) = install_delta_stream(&mut host, base, &delta);
        let (target_manifest, target_wrapper, target_canonical, target_metadata) =
            installed_manifest_shape(&host);
        assert_eq!(target_canonical, base_canonical);
        assert_eq!(target_metadata, base_metadata);
        assert_ne!(target_wrapper, base_wrapper);
        assert_ne!(target_manifest, base_manifest);

        let target_closure = reachable(&host.arena, target_manifest);
        assert!(target_closure.contains(&target_canonical));
        assert!(!target_closure.contains(&base_wrapper));
        assert!(!target_closure.contains(&base_manifest));

        let mut retirement_polls = 1;
        while !host.poll_reclaim(1).expect("retire base") {
            retirement_polls += 1;
        }
        assert!(host.arena.payload(base_manifest).is_err());
        assert!(host.arena.payload(base_wrapper).is_err());
        assert!(host.arena.payload(target_canonical).is_ok());
        let mut green = [0_u8; 32];
        let written = host
            .read_role_record(target, CandidateRole::Green, 0, &mut green)
            .expect("target Green");
        assert_eq!(&green[..written], b"green:target");

        assert!(matches!(
            host.begin_references_delta(base, &delta.begin),
            Err(CandidateHostError::BaseMismatch)
        ));

        let receipt = DeltaScaleReceipt {
            frame_count: delta.nodes.len() + 2,
            encoded_bytes: delta.encoded_bytes,
            encode_transitions: delta.transitions,
            install_transitions,
            retirement_polls,
        };
        close_host(&mut host);
        release_reused_pair(pair);
        receipt
    }

    #[test]
    fn references_delta_reuses_only_canonical_content_and_is_scale_invariant() {
        let one = run_delta_scale(1);
        let large = run_delta_scale(4_096);

        assert_eq!(large.frame_count, one.frame_count);
        assert_eq!(large.encoded_bytes, one.encoded_bytes);
        assert_eq!(large.encode_transitions, one.encode_transitions);
        assert_eq!(large.install_transitions, one.install_transitions);
        assert_eq!(large.retirement_polls, one.retirement_polls);
    }

    #[test]
    fn aborting_references_delta_preserves_the_exact_installed_base() {
        let pair = build_reused_pair(257);
        let (base_begin, base_nodes, base_end) = collect_stream(&pair.arena, &pair.base);
        let delta = collect_delta_stream(&pair.arena, &pair.target);
        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_begin, &base_nodes, &base_end);
        let (_, _, base_canonical, _) = installed_manifest_shape(&host);
        host.observe_source_version(pair.target_source)
            .expect("observe target");
        host.begin_references_delta(base, &delta.begin)
            .expect("begin delta");
        host.offer_node(delta.nodes.first().expect("fresh target node"))
            .expect("offer one node");

        assert!(host.abort_snapshot().expect("abort delta"));
        while !host.poll_reclaim(31).expect("drain aborted delta") {}
        assert_eq!(host.installed_snapshot(), Some(base));
        let (_, _, installed_canonical, _) = installed_manifest_shape(&host);
        assert_eq!(installed_canonical, base_canonical);
        let mut green = [0_u8; 32];
        let written = host
            .read_role_record(base, CandidateRole::Green, 0, &mut green)
            .expect("base query after abort");
        assert_eq!(&green[..written], b"green:base");

        close_host(&mut host);
        release_reused_pair(pair);
    }

    #[test]
    fn references_delta_rejects_target_metadata_that_does_not_match_the_base() {
        let pair = build_reused_pair(1);
        let (base_begin, base_nodes, base_end) = collect_stream(&pair.arena, &pair.base);
        let (mut bad_arena, bad_target, _) =
            build_publication(pair.target_source, pair.document, 32, 2, b"green:target");
        let bad_delta = collect_delta_stream(&bad_arena, &bad_target);
        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_begin, &base_nodes, &base_end);
        host.observe_source_version(pair.target_source)
            .expect("observe target");
        host.begin_references_delta(base, &bad_delta.begin)
            .expect("begin mismatched delta");
        for node in &bad_delta.nodes {
            host.offer_node(node).expect("offer mismatched node");
        }
        host.finish_snapshot(&bad_delta.end)
            .expect("finish mismatched delta");
        loop {
            match host.poll_install(31) {
                Ok(poll) => assert!(poll.installed.is_none()),
                Err(CandidateHostError::InvalidFrame(
                    "target References wrapper changed its exact base",
                )) => break,
                Err(error) => panic!("unexpected mismatch error: {error}"),
            }
        }
        while !host.poll_reclaim(31).expect("drain rejected target") {}
        assert_eq!(host.installed_snapshot(), Some(base));

        close_host(&mut host);
        release_reused_pair(pair);
        release_publication(&mut bad_arena, bad_target);
    }

    fn assert_exact_base_equivalence(
        pair: PersistentExactPair,
        expected_replacement_pages: usize,
        expected_virtual_roots: u64,
    ) {
        let base_stream = collect_stream(&pair.arena, &pair.base);
        let target_stream = collect_stream(&pair.arena, &pair.target);
        let exact_stream = collect_exact_delta_stream(&pair);
        assert_eq!(
            exact_stream.replacement_pages.len(),
            expected_replacement_pages
        );
        let _ = decode_node_frame(
            exact_stream.nodes.first().expect("first ordinary node"),
            expected_virtual_roots,
        )
        .expect("ordinary nodes start after virtual roots");

        let mut exact_host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("exact host");
        let base = install_stream(
            &mut exact_host,
            &base_stream.0,
            &base_stream.1,
            &base_stream.2,
        );
        exact_host
            .observe_source_version(pair.target_source)
            .expect("observe target source");
        let exact_target = install_exact_delta_stream(&mut exact_host, base, &exact_stream);

        let mut clean_host = CandidateHostStore::new(
            pair.document,
            pair.target_source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("clean host");
        let clean_target = install_stream(
            &mut clean_host,
            &target_stream.0,
            &target_stream.1,
            &target_stream.2,
        );
        assert_eq!(
            exact_host
                .installed_manifest_digest256(exact_target)
                .expect("exact manifest digest"),
            clean_host
                .installed_manifest_digest256(clean_target)
                .expect("clean manifest digest")
        );
        for role in CandidateRole::ORDERED {
            assert_eq!(
                exact_host
                    .role_record_count(exact_target, role)
                    .expect("exact role count"),
                clean_host
                    .role_record_count(clean_target, role)
                    .expect("clean role count")
            );
        }

        close_host(&mut exact_host);
        close_host(&mut clean_host);
        release_persistent_exact_pair(pair);
    }

    #[test]
    fn exact_base_delta_matches_clean_full_install_for_zero_one_many_and_empty_pages() {
        let unchanged = "a".repeat(64);
        let zero = build_persistent_exact_pair(&unchanged, 63..64, "a", 8, 0, false);
        assert_eq!(zero.base_page_range, zero.target_page_range);
        assert_eq!(zero.base_page_range.start, zero.base_page_range.end);
        assert_exact_base_equivalence(zero, 0, 2);

        let one = build_persistent_exact_pair("abcd", 3..4, "e", 64, 0, true);
        assert_eq!(one.target_page_range.end - one.target_page_range.start, 1);
        assert_exact_base_equivalence(one, 1, 2);

        let many_source = "a".repeat(4_096);
        let many = build_persistent_exact_pair(&many_source, 0..1, "b", 2, 0, false);
        let many_pages = usize::try_from(many.target_page_range.end - many.target_page_range.start)
            .expect("many replacement pages fit usize");
        assert!(many_pages > 1);
        assert_exact_base_equivalence(many, many_pages, 2);

        let empty = build_persistent_exact_pair("abc", 0..3, "", 64, 0, true);
        assert_eq!(empty.target_page_range, 0..0);
        assert_exact_base_equivalence(empty, 0, 1);
    }

    #[test]
    fn exact_base_delta_rejects_wrong_base_range_commitment_and_page_order() {
        let pair = build_persistent_exact_pair("abcdefgh", 7..8, "z", 64, 0, true);
        let base_stream = collect_stream(&pair.arena, &pair.base);
        let frames = collect_exact_delta_stream(&pair);
        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_stream.0, &base_stream.1, &base_stream.2);
        host.observe_source_version(pair.target_source)
            .expect("observe target");

        let wrong_capability = InstalledCandidateSnapshot {
            authority: pair.target.authority(),
        };
        assert!(matches!(
            host.begin_exact_base_delta(wrong_capability, &frames.begin),
            Err(CandidateHostError::BaseMismatch)
        ));

        let mut wrong_base = frames.begin.to_vec();
        wrong_base[96] ^= 0x40;
        assert!(matches!(
            host.begin_exact_base_delta(base, &wrong_base),
            Err(CandidateHostError::BaseMismatch)
        ));

        let decoded = decode_exact_base_delta_begin(&frames.begin).expect("decode Begin");
        let mut wrong_range = frames.begin.to_vec();
        wrong_range[256..264]
            .copy_from_slice(&(decoded.base_source_facts.record_count + 1).to_le_bytes());
        assert!(matches!(
            host.begin_exact_base_delta(base, &wrong_range),
            Err(CandidateHostError::InvalidFrame(
                "exact-base SourceFacts range exceeds its role"
            ))
        ));

        let mut wrong_commitment = frames.begin.to_vec();
        wrong_commitment[360] ^= 0x01;
        assert!(matches!(
            host.begin_exact_base_delta(base, &wrong_commitment),
            Err(CandidateHostError::SourceFacts(_))
        ));

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin page-order rejection");
        let mut wrong_page = frames
            .replacement_pages
            .first()
            .expect("one replacement page")
            .to_vec();
        wrong_page[4..12].copy_from_slice(&1_u64.to_le_bytes());
        assert!(matches!(
            host.offer_source_facts_replacement_page(&wrong_page),
            Err(CandidateHostError::InvalidFrame(
                "invalid SourceFacts replacement frame"
            ))
        ));
        while !host.poll_reclaim(31).expect("reclaim rejected page") {}
        assert_eq!(host.installed_snapshot(), Some(base));

        let target = install_exact_delta_stream(&mut host, base, &frames);
        assert_eq!(target, host.installed_snapshot().expect("installed target"));
        close_host(&mut host);
        release_persistent_exact_pair(pair);
    }

    #[test]
    fn aborting_exact_base_delta_in_every_host_phase_preserves_the_base() {
        let source = "a".repeat(2_048);
        let pair = build_persistent_exact_pair(&source, 0..1, "b", 2, 0, false);
        let base_stream = collect_stream(&pair.arena, &pair.base);
        let frames = collect_exact_delta_stream(&pair);
        assert!(!frames.replacement_pages.is_empty());
        assert!(!frames.nodes.is_empty());
        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_stream.0, &base_stream.1, &base_stream.2);
        host.observe_source_version(pair.target_source)
            .expect("observe target");

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin pages phase");
        assert!(host.abort_snapshot().expect("abort pages phase"));
        while !host.poll_reclaim(31).expect("reclaim pages phase") {}

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin replay phase");
        host.offer_source_facts_replacement_page(&frames.replacement_pages[0])
            .expect("offer replay page");
        assert!(matches!(
            host.active.as_ref().expect("active replay").phase,
            HostOfferPhase::AdvancingReplacementPage
        ));
        assert!(host.abort_snapshot().expect("abort replay phase"));
        while !host.poll_reclaim(31).expect("reclaim replay phase") {}

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin completed replay");
        drive_exact_replay_to_nodes(&mut host, &frames);
        assert!(host.abort_snapshot().expect("abort completed replay"));
        while !host.poll_reclaim(31).expect("reclaim completed replay") {}

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin node phase");
        drive_exact_replay_to_nodes(&mut host, &frames);
        host.offer_node(&frames.nodes[0]).expect("offer one node");
        assert!(host.abort_snapshot().expect("abort node phase"));
        while !host.poll_reclaim(31).expect("reclaim node phase") {}

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin seal phase");
        drive_exact_replay_to_nodes(&mut host, &frames);
        for node in &frames.nodes {
            host.offer_node(node).expect("offer node");
        }
        host.finish_snapshot(&frames.end).expect("finish snapshot");
        loop {
            if matches!(
                host.active.as_ref().expect("active install").phase,
                HostOfferPhase::Sealing(_)
            ) {
                break;
            }
            let poll = host.poll_install(1).expect("advance to seal");
            assert!(poll.installed.is_none());
        }
        assert!(host.abort_snapshot().expect("abort seal phase"));
        while !host.poll_reclaim(31).expect("reclaim seal phase") {}
        assert_eq!(host.installed_snapshot(), Some(base));

        close_host(&mut host);
        release_persistent_exact_pair(pair);
    }

    #[test]
    fn exact_base_delta_wire_work_is_independent_of_reference_scale() {
        let source = "a".repeat(4_113);
        let one = build_persistent_exact_pair(&source, 4_112..4_113, "b", 64, 1, false);
        let many = build_persistent_exact_pair(&source, 4_112..4_113, "b", 64, 256, false);
        let one_frames = collect_exact_delta_stream(&one);
        let many_frames = collect_exact_delta_stream(&many);

        assert_eq!(
            one_frames.replacement_pages.len(),
            many_frames.replacement_pages.len()
        );
        assert_eq!(one_frames.nodes.len(), many_frames.nodes.len());
        assert_eq!(
            one_frames
                .replacement_pages
                .iter()
                .map(|frame| frame.len())
                .sum::<usize>(),
            many_frames
                .replacement_pages
                .iter()
                .map(|frame| frame.len())
                .sum::<usize>()
        );
        assert_eq!(
            one_frames
                .nodes
                .iter()
                .map(|frame| frame.len())
                .sum::<usize>(),
            many_frames
                .nodes
                .iter()
                .map(|frame| frame.len())
                .sum::<usize>()
        );

        release_persistent_exact_pair(one);
        release_persistent_exact_pair(many);
    }

    #[derive(Clone, Copy, Debug)]
    struct BlockDeltaScaleReceipt {
        base_entries: u64,
        base_storage_pages: u64,
        block_frame_count: usize,
        block_payload_bytes: usize,
        work: M11BlockSequenceHostSpliceWork,
    }

    fn run_block_delta_scale(pair_count: usize, challenge_claims: bool) -> BlockDeltaScaleReceipt {
        let pair = build_persistent_block_exact_pair(pair_count);
        let (base_begin, base_nodes, base_end) =
            collect_stream(pair.runtime.producer_arena(), &pair.base);
        let frames = collect_block_exact_delta_stream(&pair);
        let target_authority = pair.target.authority();
        let target_descriptor = decode_manifest_descriptor(
            pair.runtime.producer_arena(),
            pair.target.root_id(),
            target_authority,
        )
        .expect("target manifest");
        let expected_manifest_digest = manifest_digest256(target_authority, &target_descriptor);
        let block_payload_bytes = frames
            .block_pages
            .iter()
            .map(|frame| frame.len() - SNAPSHOT_BLOCK_REPLACEMENT_HEADER_BYTES)
            .sum();

        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_begin, &base_nodes, &base_end);
        host.observe_source_version(pair.target_source)
            .expect("observe target source");

        if challenge_claims {
            let mut wrong_commitment = frames.begin.to_vec();
            let base_green_descriptor = SNAPSHOT_EXACT_BASE_BEGIN_BYTES + 304;
            wrong_commitment[base_green_descriptor + 104] ^= 0x01;
            assert!(matches!(
                host.begin_exact_base_delta(base, &wrong_commitment),
                Err(CandidateHostError::BaseMismatch)
            ));
            assert_eq!(host.installed_snapshot(), Some(base));

            let mut wrong_semantic_range = frames.begin.to_vec();
            let base_entry_end = SNAPSHOT_EXACT_BASE_BEGIN_BYTES + 24;
            let end = read_u64(&wrong_semantic_range, base_entry_end).expect("base entry end") + 1;
            wrong_semantic_range[base_entry_end..base_entry_end + 8]
                .copy_from_slice(&end.to_le_bytes());
            assert!(host
                .begin_exact_base_delta(base, &wrong_semantic_range)
                .is_err());
            assert_eq!(host.installed_snapshot(), Some(base));

            host.begin_exact_base_delta(base, &frames.begin)
                .expect("begin malformed block-page attempt");
            drive_source_facts_to_block_pages(&mut host, &frames);
            let mut malformed_page = frames.block_pages[0].to_vec();
            *malformed_page.last_mut().expect("block payload byte") ^= 0x01;
            assert!(matches!(
                host.offer_block_sequence_replacement_page(&malformed_page),
                Err(CandidateHostError::InvalidFrame(
                    "replacement block leaf is invalid"
                ))
            ));
            while !host.poll_reclaim(31).expect("malformed block reclaim") {}
            assert_eq!(host.installed_snapshot(), Some(base));

            host.begin_exact_base_delta(base, &frames.begin)
                .expect("begin abortable block replay");
            drive_source_facts_to_block_pages(&mut host, &frames);
            host.offer_block_sequence_replacement_page(&frames.block_pages[0])
                .expect("offer block page before abort");
            assert!(matches!(
                host.active.as_ref().expect("active block replay").phase,
                HostOfferPhase::AdvancingBlockReplacementPage
            ));
            assert!(host.abort_snapshot().expect("abort mid-block replay"));
            while !host.poll_reclaim(31).expect("mid-block reclaim") {}
            assert_eq!(host.installed_snapshot(), Some(base));
        }

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin block exact-base delta");
        let work = drive_block_exact_replay_to_nodes(&mut host, &frames);
        for node in &frames.nodes {
            host.offer_node(node).expect("offer fresh target node");
        }
        host.finish_snapshot(&frames.end)
            .expect("finish block exact-base snapshot");
        let target = loop {
            let poll = host.poll_install(31).expect("install block exact-base");
            if let Some(snapshot) = poll.installed {
                break snapshot;
            }
        };
        assert_eq!(
            host.installed_manifest_digest256(target)
                .expect("installed manifest digest"),
            expected_manifest_digest,
            "the replayed block commitment must produce the exact target manifest"
        );
        let edited = usize::try_from(pair.base_block_entry_range.start).expect("edited point");
        let location = host
            .persistent_block_point(
                target,
                M11BlockSequencePoint::new(
                    edited,
                    edited,
                    crate::source::SourceBoundaryAffinity::After,
                ),
            )
            .expect("replayed block point")
            .expect("edited block");
        assert_eq!(
            location.entry().green().expect("edited Green").as_bytes(),
            b"g-new"
        );
        assert!(matches!(
            host.begin_exact_base_delta(base, &frames.begin),
            Err(CandidateHostError::BaseMismatch)
        ));

        let receipt = BlockDeltaScaleReceipt {
            base_entries: work.base_entries(),
            base_storage_pages: work.base_storage_pages(),
            block_frame_count: frames.block_pages.len(),
            block_payload_bytes,
            work,
        };
        close_host(&mut host);
        release_persistent_block_exact_pair(pair);
        receipt
    }

    #[derive(Clone, Copy, Debug)]
    struct GreenDeltaScaleReceipt {
        base_events: u64,
        base_storage_pages: u64,
        green_frame_count: usize,
        work: M11RecursiveGreenHostSpliceWork,
    }

    fn run_green_delta_scale(items: usize, challenge_claim: bool) -> GreenDeltaScaleReceipt {
        let pair = build_persistent_green_exact_pair(items);
        let (base_begin, base_nodes, base_end) =
            collect_stream(pair.runtime.producer_arena(), &pair.base);
        let frames = collect_green_exact_delta_stream(&pair);
        let target_authority = pair.target.authority();
        let target_manifest = decode_manifest_descriptor(
            pair.runtime.producer_arena(),
            pair.target.root_id(),
            target_authority,
        )
        .expect("target manifest");
        let expected_green = persistent_recursive_green_manifest_role(
            pair.runtime.producer_arena(),
            &target_manifest,
            target_authority,
        )
        .expect("target recursive Green role");
        let expected_manifest_digest = manifest_digest256(target_authority, &target_manifest);
        if expected_green.descriptor.storage_page_count() > 1 {
            assert_eq!(
                pair.runtime
                    .producer_arena()
                    .payload(expected_green.root.expect("nonempty target Green"))
                    .expect("target Green root payload")
                    .get(..4),
                Some(b"RGB1".as_slice()),
                "the scale gate must prove an RGB1 tree is reconstructed, not transferred"
            );
        }

        let mut host = CandidateHostStore::new(
            pair.document,
            pair.source,
            1,
            CandidateHostLimits::default(),
        )
        .expect("host");
        let base = install_stream(&mut host, &base_begin, &base_nodes, &base_end);
        host.observe_source_version(pair.target_source)
            .expect("observe target source");

        if challenge_claim {
            let mut wrong_commitment = frames.begin.to_vec();
            let base_green_descriptor = SNAPSHOT_EXACT_BASE_BEGIN_BYTES + 192;
            wrong_commitment[base_green_descriptor + 88] ^= 0x01;
            assert!(matches!(
                host.begin_exact_base_delta(base, &wrong_commitment),
                Err(CandidateHostError::BaseMismatch)
            ));
            assert_eq!(
                host.installed_snapshot(),
                Some(base),
                "a false Green claim must not withdraw the exact installed base"
            );
        }

        host.begin_exact_base_delta(base, &frames.begin)
            .expect("begin recursive Green exact-base delta");
        let work = drive_green_exact_replay_to_nodes(&mut host, &frames);
        for node in &frames.nodes {
            host.offer_node(node).expect("offer fresh target node");
        }
        host.finish_snapshot(&frames.end)
            .expect("finish recursive Green exact-base snapshot");
        let target = loop {
            let poll = host
                .poll_install(31)
                .expect("install recursive Green exact-base");
            if let Some(snapshot) = poll.installed {
                break snapshot;
            }
        };
        assert_eq!(
            host.installed_manifest_digest256(target)
                .expect("installed manifest digest"),
            expected_manifest_digest,
            "the replayed Green commitment must produce the exact target manifest"
        );
        let installed = host.installed.as_ref().expect("installed target");
        let installed_manifest =
            decode_manifest_descriptor(&host.arena, installed.root.id(), installed.authority)
                .expect("installed target manifest");
        let installed_green = persistent_recursive_green_manifest_role(
            &host.arena,
            &installed_manifest,
            installed.authority,
        )
        .expect("installed recursive Green role");
        assert_eq!(installed_green.descriptor, expected_green.descriptor);
        assert_eq!(
            installed_green.descriptor.canonical_commitment256(),
            pair.target_green.canonical_event_commitment256(),
            "the independent host must install the exact target Green commitment"
        );

        let receipt = GreenDeltaScaleReceipt {
            base_events: work.base_events(),
            base_storage_pages: work.base_storage_pages(),
            green_frame_count: frames.green_pages.len(),
            work,
        };
        close_host(&mut host);
        release_persistent_green_exact_pair(pair);
        receipt
    }

    #[test]
    fn exact_base_recursive_green_splice_is_local_exact_and_fails_closed() {
        let small = run_green_delta_scale(128, true);
        let large = run_green_delta_scale(8_192, false);

        assert_eq!(small.base_events, 386);
        assert_eq!(large.base_events, 24_578);
        assert!(large.base_storage_pages > small.base_storage_pages * 16);
        assert!((1..=2).contains(&small.green_frame_count));
        assert!((1..=2).contains(&large.green_frame_count));
        assert_eq!(small.work.deleted_events(), 3);
        assert_eq!(large.work.deleted_events(), 3);
        assert_eq!(small.work.replacement_events(), 3);
        assert_eq!(large.work.replacement_events(), 3);
        assert!(small.work.transferred_storage_pages() <= 2);
        assert!(large.work.transferred_storage_pages() <= 2);
        assert_eq!(
            usize::try_from(large.work.transferred_storage_pages()).expect("page count"),
            large.green_frame_count
        );
        assert!(large.work.transferred_payload_bytes() <= 2 * ARENA_PAGE_BYTES as u64);
        assert!(large.work.reused_storage_pages() + 2 >= large.base_storage_pages);
        assert!(large.work.node_headers_decoded() > 0);
        assert!(
            large.work.tree_nodes_visited()
                < usize::from(large.work.maximum_atomic_height()) * 16 + 32
        );
        assert!(
            large.work.branches_allocated()
                < usize::from(large.work.maximum_atomic_height()) * 8 + 32
        );
    }

    #[test]
    fn exact_base_block_splice_transfers_only_local_pages_and_fails_closed() {
        let small = run_block_delta_scale(128, true);
        let large = run_block_delta_scale(4_096, false);

        assert_eq!(small.base_entries, 256);
        assert_eq!(large.base_entries, 8_192);
        assert_eq!(small.base_storage_pages, 4);
        assert_eq!(large.base_storage_pages, 128);
        assert_eq!(small.block_frame_count, 1);
        assert_eq!(large.block_frame_count, 1);
        assert_eq!(large.block_payload_bytes, small.block_payload_bytes);
        assert_eq!(small.work.deleted_entries(), 1);
        assert_eq!(large.work.deleted_entries(), 1);
        assert_eq!(small.work.replacement_entries(), 1);
        assert_eq!(large.work.replacement_entries(), 1);
        assert_eq!(small.work.transferred_storage_pages(), 1);
        assert_eq!(large.work.transferred_storage_pages(), 1);
        assert_eq!(
            large.work.transferred_payload_bytes(),
            small.work.transferred_payload_bytes()
        );
        assert_eq!(small.work.reused_storage_pages(), 3);
        assert_eq!(large.work.reused_storage_pages(), 127);
        assert!(
            large.work.boundary_entries_decoded()
                <= 2 * M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX as u64
        );
        assert!(large.work.node_headers_decoded() > 0);
        assert!(
            large.work.branch_payload_bytes() <= large.work.branches_allocated() * ARENA_PAGE_BYTES
        );
        assert!(
            large.work.tree_nodes_visited()
                < usize::from(large.work.maximum_atomic_height()) * 16 + 32
        );
        assert!(
            large.work.branches_allocated()
                < usize::from(large.work.maximum_atomic_height()) * 8 + 32
        );
    }
}
